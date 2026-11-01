//! Core-owned admission, one bounded process invocation, and response settlement.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::model_candidate::{
    CostStatus, ExtractionRequest, Policy, ProviderIdentity, Usage, UsageProvenance,
};

/// The trusted adapter supplies only a process capability. It does not run I/O.
pub trait Provider {
    fn identity(&self) -> &ProviderIdentity;
    fn invocation(&self) -> Invocation;
}

pub struct Invocation {
    pub program: PathBuf,
    pub args: Vec<OsString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub available_calls: u32,
    pub available_output_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetReservation {
    pub calls: u32,
    pub output_tokens: u64,
}

impl<'de> Deserialize<'de> for BudgetReservation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            calls: u32,
            output_tokens: u64,
        }
        let fields: Fields = crate::pdf_model::deserialize_object(deserializer)?;
        if fields.calls != 1 || fields.output_tokens == 0 || fields.output_tokens > 1024 {
            return Err(serde::de::Error::custom("MODEL_RESERVATION_INVALID"));
        }
        Ok(Self {
            calls: fields.calls,
            output_tokens: fields.output_tokens,
        })
    }
}

impl Budget {
    pub fn admit(&self, policy: &Policy) -> Result<BudgetReservation, &'static str> {
        admit(self, policy)
    }
}

/// Token/call admission is an application policy, with no monetary promise.
pub fn admit(budget: &Budget, policy: &Policy) -> Result<BudgetReservation, &'static str> {
    let ceiling = Policy::default();
    if policy.max_input_bytes == 0
        || policy.max_input_bytes > ceiling.max_input_bytes
        || policy.max_response_bytes == 0
        || policy.max_response_bytes > ceiling.max_response_bytes
        || policy.max_output_tokens == 0
        || policy.max_output_tokens > ceiling.max_output_tokens
        || policy.timeout_ms == 0
        || policy.timeout_ms > ceiling.timeout_ms
        || policy.max_calls != 1
        || policy.retries != 0
        || policy.search_enabled
        || policy.tools_enabled
    {
        return Err("MODEL_POLICY_INVALID");
    }
    if budget.available_calls == 0 || budget.available_output_tokens < policy.max_output_tokens {
        return Err("MODEL_BUDGET_INSUFFICIENT");
    }
    Ok(BudgetReservation {
        calls: 1,
        output_tokens: policy.max_output_tokens,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Delivered,
    NotCalled,
    Timeout,
    RemoteUnknown,
    ResponseLimit,
    Cancelled,
    TransportFailed,
}

impl<'de> Deserialize<'de> for AttemptStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Wire {
            Delivered,
            NotCalled,
            Timeout,
            RemoteUnknown,
            ResponseLimit,
            Cancelled,
            TransportFailed,
        }
        let name = String::deserialize(deserializer)?;
        let wire = Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(name))?;
        Ok(match wire {
            Wire::Delivered => Self::Delivered,
            Wire::NotCalled => Self::NotCalled,
            Wire::Timeout => Self::Timeout,
            Wire::RemoteUnknown => Self::RemoteUnknown,
            Wire::ResponseLimit => Self::ResponseLimit,
            Wire::Cancelled => Self::Cancelled,
            Wire::TransportFailed => Self::TransportFailed,
        })
    }
}

/// Response bytes are transient and must pass Core credential checks before save.
/// No diagnostic body, executable, argv, or external error text is exposed.
pub struct Attempt {
    pub response: Option<Vec<u8>>,
    pub response_sha256: Option<String>,
    pub response_bytes: u64,
    pub stderr_sha256: Option<String>,
    pub stderr_bytes: u64,
    pub pid: Option<u32>,
    pub elapsed_ms: u64,
    pub status: AttemptStatus,
}

impl std::fmt::Debug for Attempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Attempt")
            .field("response_sha256", &self.response_sha256)
            .field("response_bytes", &self.response_bytes)
            .field("stderr_sha256", &self.stderr_sha256)
            .field("stderr_bytes", &self.stderr_bytes)
            .field("pid", &self.pid)
            .field("elapsed_ms", &self.elapsed_ms)
            .field("status", &self.status)
            .finish()
    }
}

pub fn run(
    provider: &dyn Provider,
    request: &ExtractionRequest,
    request_bytes: &[u8],
    workspace: &Path,
    cancel: &AtomicBool,
) -> Attempt {
    let started = Instant::now();
    let mut attempt = Attempt {
        response: None,
        response_sha256: None,
        response_bytes: 0,
        stderr_sha256: None,
        stderr_bytes: 0,
        pid: None,
        elapsed_ms: 0,
        status: AttemptStatus::NotCalled,
    };
    // The caller admits its budget before invoking this function. The host also
    // refuses mismatched/invalid wire bytes, so a caller cannot send a broader
    // policy or a different source under an already admitted typed request.
    if crate::model_candidate::validate_request(request).is_err()
        || request_bytes.len() > 64 * 1024
        || !serde_json::from_slice::<ExtractionRequest>(request_bytes)
            .is_ok_and(|wire| &wire == request)
        || provider.identity() != &request.provider
    {
        attempt.elapsed_ms = milliseconds(started.elapsed());
        return attempt;
    }
    if cancel.load(Ordering::Acquire) {
        attempt.status = AttemptStatus::Cancelled;
        attempt.elapsed_ms = milliseconds(started.elapsed());
        return attempt;
    }
    let invocation = provider.invocation();
    if !invocation.program.is_absolute()
        || crate::project::checked_file(&invocation.program).is_err()
    {
        attempt.elapsed_ms = milliseconds(started.elapsed());
        return attempt;
    }
    // Retain the executable's ordinary-file guard across execution. Neither
    // trusted process arguments nor this guard is part of saved model facts.
    let Ok(_program_guard) = crate::project::read_guard(&invocation.program) else {
        attempt.elapsed_ms = milliseconds(started.elapsed());
        return attempt;
    };
    let limits = crate::sidecar::SidecarLimits {
        max_input_bytes: request.policy.max_input_bytes,
        max_request_bytes: 64 * 1024,
        max_stdout_bytes: request.policy.max_response_bytes as usize,
        max_stderr_bytes: 64 * 1024,
        timeout: Duration::from_millis(request.policy.timeout_ms),
        cleanup_timeout: Duration::from_secs(2),
        ..crate::sidecar::SidecarLimits::default()
    };
    match crate::sidecar_process::execute_command(
        &invocation.program,
        &invocation.args,
        request_bytes,
        workspace,
        &limits,
        cancel,
    ) {
        Ok(output) => {
            attempt.pid = Some(output.pid);
            attempt.stderr_bytes = output.stderr.len() as u64;
            attempt.stderr_sha256 = Some(crate::text::sha256(&output.stderr));
            // The host has confirmed the complete response and process exit.
            // A later cancellation cannot erase confirmed usage or trigger a retry.
            attempt.response_bytes = output.stdout.len() as u64;
            attempt.response_sha256 = Some(crate::text::sha256(&output.stdout));
            attempt.response = Some(output.stdout);
            attempt.status = AttemptStatus::Delivered;
        }
        Err(error) => {
            attempt.pid = error.pid;
            // Error diagnostics may be only the bounded captured prefix. Its
            // byte count/SHA describe those captured bytes, never a full stream.
            attempt.stderr_bytes = error.diagnostics.len() as u64;
            attempt.stderr_sha256 = Some(crate::text::sha256(&error.diagnostics));
            attempt.status = match error.code {
                "SIDECAR_SPAWN_FAILED" if error.pid.is_none() => AttemptStatus::NotCalled,
                "SIDECAR_TIMEOUT" => AttemptStatus::Timeout,
                "SIDECAR_STDOUT_LIMIT" => AttemptStatus::ResponseLimit,
                "SIDECAR_CANCELLED" => AttemptStatus::Cancelled,
                "SIDECAR_CLEANUP_FAILED" => AttemptStatus::RemoteUnknown,
                _ => AttemptStatus::TransportFailed,
            };
            if attempt.status == AttemptStatus::NotCalled {
                attempt.stderr_sha256 = None;
                attempt.stderr_bytes = 0;
            }
            // Partial stdout never receives a response SHA or a response body.
        }
    }
    attempt.elapsed_ms = milliseconds(started.elapsed());
    attempt
}

fn milliseconds(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettlementRecord {
    pub request_id: String,
    pub response_sha256: String,
    pub usage: Usage,
}

impl<'de> Deserialize<'de> for SettlementRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            request_id: String,
            response_sha256: String,
            usage: Usage,
        }
        let fields: Fields = crate::pdf_model::deserialize_object(deserializer)?;
        if !crate::source_model::valid_uuid(&fields.request_id)
            || !crate::source_model::valid_hash(&fields.response_sha256)
            || !valid_usage(&fields.usage)
        {
            return Err(serde::de::Error::custom("MODEL_SETTLEMENT_INVALID"));
        }
        Ok(Self {
            request_id: fields.request_id,
            response_sha256: fields.response_sha256,
            usage: fields.usage,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettlementResult {
    Confirmed,
    AlreadyConfirmed,
}

#[derive(Debug, Clone, Default)]
pub struct SettlementLedger {
    records: BTreeMap<String, SettlementRecord>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UsageTotals {
    pub real_responses: u64,
    pub simulated_responses: u64,
    pub known_input_tokens: u64,
    pub known_output_tokens: u64,
    pub unknown_input_usages: u64,
    pub unknown_output_usages: u64,
    pub known_costs_microunits: BTreeMap<String, u64>,
    pub unknown_cost_usages: u64,
}

impl SettlementLedger {
    pub fn confirm(
        &mut self,
        request_id: &str,
        response_sha256: &str,
        usage: &Usage,
    ) -> Result<SettlementResult, &'static str> {
        if !crate::source_model::valid_uuid(request_id)
            || !crate::source_model::valid_hash(response_sha256)
        {
            return Err("MODEL_SETTLEMENT_INVALID");
        }
        if !valid_usage(usage) {
            return Err("MODEL_USAGE_INVALID");
        }
        if let Some(existing) = self.records.get(request_id) {
            return if existing.response_sha256 == response_sha256 && &existing.usage == usage {
                Ok(SettlementResult::AlreadyConfirmed)
            } else {
                Err("MODEL_SETTLEMENT_CONFLICT")
            };
        }
        self.records.insert(
            request_id.into(),
            SettlementRecord {
                request_id: request_id.into(),
                response_sha256: response_sha256.into(),
                usage: usage.clone(),
            },
        );
        Ok(SettlementResult::Confirmed)
    }

    pub fn records(&self) -> Vec<SettlementRecord> {
        self.records.values().cloned().collect()
    }

    pub fn real_usage_totals(&self) -> Result<UsageTotals, &'static str> {
        let mut totals = UsageTotals::default();
        for record in self.records.values() {
            let usage = &record.usage;
            if usage.provenance == UsageProvenance::Simulated {
                add(&mut totals.simulated_responses, 1)?;
                continue;
            }
            add(&mut totals.real_responses, 1)?;
            match usage.input_tokens {
                Some(value) => add(&mut totals.known_input_tokens, value)?,
                None => add(&mut totals.unknown_input_usages, 1)?,
            }
            match usage.output_tokens {
                Some(value) => add(&mut totals.known_output_tokens, value)?,
                None => add(&mut totals.unknown_output_usages, 1)?,
            }
            match usage.cost.status {
                CostStatus::Unknown => add(&mut totals.unknown_cost_usages, 1)?,
                CostStatus::Known => {
                    let currency = usage.cost.currency.as_ref().ok_or("MODEL_USAGE_INVALID")?;
                    let amount = usage.cost.amount_microunits.ok_or("MODEL_USAGE_INVALID")?;
                    add(
                        totals
                            .known_costs_microunits
                            .entry(currency.clone())
                            .or_default(),
                        amount,
                    )?;
                }
            }
        }
        Ok(totals)
    }
}

fn valid_usage(usage: &Usage) -> bool {
    crate::model_candidate::valid_cost(&usage.cost)
}

fn add(total: &mut u64, value: u64) -> Result<(), &'static str> {
    *total = total.checked_add(value).ok_or("MODEL_USAGE_OVERFLOW")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_candidate::{Cost, CostBasis};

    const REQUEST_ID: &str = "11111111-1111-4111-8111-111111111111";
    const RESPONSE_SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const OTHER_SHA: &str = "4a99557e4033c3539de2eb65472017cad5f9557f7a0625a09f1c3f6e2ba69c4c";

    fn usage(provenance: UsageProvenance) -> Usage {
        Usage {
            provenance,
            input_tokens: Some(80),
            output_tokens: Some(23),
            cost: Cost {
                status: CostStatus::Unknown,
                amount_microunits: None,
                currency: None,
                basis: CostBasis::NotAvailable,
            },
        }
    }

    #[test]
    fn zero_or_insufficient_budget_cannot_reserve_a_provider_call() {
        for budget in [
            Budget {
                available_calls: 0,
                available_output_tokens: 1024,
            },
            Budget {
                available_calls: 1,
                available_output_tokens: 0,
            },
            Budget {
                available_calls: 1,
                available_output_tokens: 1023,
            },
        ] {
            assert_eq!(
                budget.admit(&Policy::default()),
                Err("MODEL_BUDGET_INSUFFICIENT")
            );
        }
    }

    #[test]
    fn exact_budget_reserves_one_full_output_cap() {
        let reservation = Budget {
            available_calls: 1,
            available_output_tokens: 1024,
        }
        .admit(&Policy::default())
        .unwrap();
        assert_eq!(
            reservation,
            BudgetReservation {
                calls: 1,
                output_tokens: 1024,
            }
        );
    }

    #[test]
    fn invalid_or_expanded_policy_cannot_be_reserved() {
        for policy in [
            Policy {
                timeout_ms: 0,
                ..Policy::default()
            },
            Policy {
                timeout_ms: 30001,
                ..Policy::default()
            },
            Policy {
                max_output_tokens: 1025,
                ..Policy::default()
            },
            Policy {
                max_response_bytes: 524289,
                ..Policy::default()
            },
            Policy {
                max_calls: 2,
                ..Policy::default()
            },
            Policy {
                retries: 1,
                ..Policy::default()
            },
        ] {
            assert_eq!(
                Budget {
                    available_calls: 20,
                    available_output_tokens: 20000,
                }
                .admit(&policy),
                Err("MODEL_POLICY_INVALID")
            );
        }
    }

    #[test]
    fn repeated_confirmed_response_settles_exactly_once() {
        let mut ledger = SettlementLedger::default();
        let usage = usage(UsageProvenance::ProviderReported);
        assert_eq!(
            ledger.confirm(REQUEST_ID, RESPONSE_SHA, &usage),
            Ok(SettlementResult::Confirmed)
        );
        assert_eq!(
            ledger.confirm(REQUEST_ID, RESPONSE_SHA, &usage),
            Ok(SettlementResult::AlreadyConfirmed)
        );
        assert_eq!(ledger.records().len(), 1);
        let totals = ledger.real_usage_totals().unwrap();
        assert_eq!(totals.real_responses, 1);
        assert_eq!(totals.known_input_tokens, 80);
        assert_eq!(totals.known_output_tokens, 23);
        assert_eq!(totals.unknown_cost_usages, 1);
        assert!(totals.known_costs_microunits.is_empty());
    }

    #[test]
    fn conflicting_response_or_usage_preserves_original_settlement() {
        let mut ledger = SettlementLedger::default();
        let original = usage(UsageProvenance::ProviderReported);
        ledger.confirm(REQUEST_ID, RESPONSE_SHA, &original).unwrap();
        assert_eq!(
            ledger.confirm(REQUEST_ID, OTHER_SHA, &original),
            Err("MODEL_SETTLEMENT_CONFLICT")
        );
        let mut changed = original.clone();
        changed.output_tokens = Some(24);
        assert_eq!(
            ledger.confirm(REQUEST_ID, RESPONSE_SHA, &changed),
            Err("MODEL_SETTLEMENT_CONFLICT")
        );
        assert_eq!(ledger.records()[0].usage, original);
        assert_eq!(ledger.records()[0].response_sha256, RESPONSE_SHA);
    }

    #[test]
    fn simulated_counts_and_unknown_cost_never_become_real_zero_cost() {
        let mut ledger = SettlementLedger::default();
        let fake = usage(UsageProvenance::Simulated);
        ledger.confirm(REQUEST_ID, RESPONSE_SHA, &fake).unwrap();
        let totals = ledger.real_usage_totals().unwrap();
        assert_eq!(totals.simulated_responses, 1);
        assert_eq!(totals.real_responses, 0);
        assert_eq!(totals.known_input_tokens, 0);
        assert_eq!(totals.known_output_tokens, 0);
        assert!(totals.known_costs_microunits.is_empty());
        assert_eq!(ledger.records()[0].usage.cost.amount_microunits, None);
    }

    #[test]
    fn unknown_usage_is_counted_as_unknown_and_invalid_cost_is_rejected() {
        let mut ledger = SettlementLedger::default();
        let mut unknown = usage(UsageProvenance::ProviderReported);
        unknown.input_tokens = None;
        unknown.output_tokens = None;
        ledger.confirm(REQUEST_ID, RESPONSE_SHA, &unknown).unwrap();
        let totals = ledger.real_usage_totals().unwrap();
        assert_eq!(totals.unknown_input_usages, 1);
        assert_eq!(totals.unknown_output_usages, 1);
        assert_eq!(totals.unknown_cost_usages, 1);
        assert!(totals.known_costs_microunits.is_empty());
        unknown.cost.amount_microunits = Some(0);
        assert_eq!(
            SettlementLedger::default().confirm(REQUEST_ID, RESPONSE_SHA, &unknown),
            Err("MODEL_USAGE_INVALID")
        );
    }

    #[test]
    fn persisted_reservation_settlement_and_status_require_named_shapes() {
        assert!(serde_json::from_str::<BudgetReservation>("[1,1024]").is_err());
        assert!(
            serde_json::from_str::<BudgetReservation>(
                r#"{"calls":1,"output_tokens":1024,"ignored":true}"#
            )
            .is_err()
        );
        assert!(serde_json::from_str::<AttemptStatus>(r#"{"timeout":null}"#).is_err());
        let array =
            serde_json::json!([REQUEST_ID, RESPONSE_SHA, usage(UsageProvenance::Simulated)]);
        assert!(
            serde_json::from_slice::<SettlementRecord>(&serde_json::to_vec(&array).unwrap())
                .is_err()
        );
    }

    #[test]
    fn known_costs_preserve_currency_and_totals_reject_integer_overflow() {
        let mut ledger = SettlementLedger::default();
        let mut usd = usage(UsageProvenance::ProviderReported);
        usd.cost = Cost {
            status: CostStatus::Known,
            amount_microunits: Some(90),
            currency: Some("USD".into()),
            basis: CostBasis::ProviderReported,
        };
        ledger.confirm(REQUEST_ID, RESPONSE_SHA, &usd).unwrap();
        let mut cny = usd.clone();
        cny.cost.amount_microunits = Some(40);
        cny.cost.currency = Some("CNY".into());
        ledger
            .confirm("11111111-1111-4111-8111-111111111112", OTHER_SHA, &cny)
            .unwrap();
        let totals = ledger.real_usage_totals().unwrap();
        assert_eq!(totals.known_costs_microunits.get("USD"), Some(&90));
        assert_eq!(totals.known_costs_microunits.get("CNY"), Some(&40));
        assert_eq!(totals.unknown_cost_usages, 0);

        let mut overflow = SettlementLedger::default();
        usd.input_tokens = Some(u64::MAX);
        overflow.confirm(REQUEST_ID, RESPONSE_SHA, &usd).unwrap();
        overflow
            .confirm("11111111-1111-4111-8111-111111111112", OTHER_SHA, &cny)
            .unwrap();
        assert_eq!(overflow.real_usage_totals(), Err("MODEL_USAGE_OVERFLOW"));
    }
}
