//! Persistent accounting for the bounded, synthetic single-request scheduler.
//! Events are authoritative; the views below are reconstructed projections.

use crate::{
    compile_identity::Identity,
    run_model::{FileStamp, required_option},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const RESERVATION_POLICY: &str = "request_utf8_bytes_plus_1024_v1";
pub(crate) const MAX_EVENTS: u64 = 4096;

macro_rules! object {
    ($name:ident { $($(#[$attr:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct $name { $($(#[$attr])* pub(crate) $field: $ty),* }
    };
}

object!(BudgetLimits {
    max_requests: u64,
    max_total_tokens: u64,
    #[serde(deserialize_with = "required_option")]
    max_cost_usd: Option<String>
});
object!(OpeningBalance {
    accounted_tokens: u64,
    held_tokens: u64,
    dispatched_requests: u64,
    held_requests: u64
});
object!(AuthoritySpec {
    schema_version: u32,
    authority_id: String,
    global_limits: BudgetLimits,
    opening: OpeningBalance,
    policy_sha256: String
});
object!(UnitSpec {
    unit_id: String,
    max_attempts: u32,
    max_retry_tokens: u64,
    deadline_utc: String
});
object!(RunSpec {
    run_id: String,
    run_root: String,
    case_sha256: String,
    input_pins: BTreeMap<String, FileStamp>,
    run_limits: BudgetLimits,
    deadline_utc: String,
    retry_base_delay_ms: u64,
    retry_max_delay_ms: u64,
    units: Vec<UnitSpec>
});
object!(FilePin {
    sha256: String,
    byte_length: u64
});
object!(AttemptKey {
    run_id: String,
    unit_id: String,
    attempt_no: u32,
    request_id: String
});
object!(AttemptPlan {
    key: AttemptKey,
    attempt_root: String,
    request_body: FilePin,
    request_metadata: FilePin,
    input_reserved_tokens: u64,
    max_output_tokens: u64,
    reserved_tokens: u64,
    reservation_policy: String
});
object!(TokenUsage {
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64
});
object!(TransportReceipt {
    schema_version: u32,
    producer: String,
    request_id: String,
    #[serde(deserialize_with = "required_option")]
    http_status: Option<u16>,
    #[serde(deserialize_with = "required_option")]
    http_request_id: Option<String>,
    response_complete: bool,
    network_attempts: u32,
    response: FilePin,
    #[serde(deserialize_with = "required_option")]
    response_file: Option<String>,
    #[serde(deserialize_with = "required_option")]
    error_code: Option<String>,
    #[serde(deserialize_with = "required_option")]
    omission_reason: Option<String>,
    elapsed_ms: u64
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CandidateStatus {
    Validated,
    Failed,
}

object!(AttemptReceipt {
    report: FilePin,
    #[serde(deserialize_with = "required_option")]
    response: Option<FilePin>,
    network_attempts: u32,
    #[serde(deserialize_with = "required_option")]
    http_status: Option<u16>,
    response_complete: bool,
    #[serde(deserialize_with = "required_option")]
    response_id: Option<String>,
    #[serde(deserialize_with = "required_option")]
    http_request_id: Option<String>,
    #[serde(deserialize_with = "required_option")]
    usage: Option<TokenUsage>,
    candidate_status: CandidateStatus,
    #[serde(deserialize_with = "required_option")]
    error_code: Option<String>,
    elapsed_ms: u64,
    #[serde(deserialize_with = "required_option")]
    price_usd: Option<String>
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AttemptState {
    Prepared,
    Reserved,
    MaySend,
    Unknown,
    Known,
    NotSent,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UnitOutcome {
    Succeeded,
    Failed,
    BudgetStopped,
    Invalidated,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunOutcome {
    Succeeded,
    Failed,
    BudgetStopped,
    Invalidated,
}
object!(RetrySchedule {
    retry_at_utc: String,
    error_code: String
});
object!(RunIdentity {
    root: Identity,
    parent: Identity,
    control: Identity,
    attempts: Identity,
    owner: FileStamp,
    worker: FileStamp,
    units: BTreeMap<String, Identity>
});
object!(AttemptIdentity { root: Identity, parent: Identity, directories: BTreeMap<String, Identity> });
object!(AttemptView {
    plan: AttemptPlan,
    identity: AttemptIdentity,
    sealed_files: BTreeMap<String, FileStamp>,
    transport_sealed: bool,
    state: AttemptState,
    #[serde(deserialize_with = "required_option")]
    receipt: Option<AttemptReceipt>,
    #[serde(deserialize_with = "required_option")]
    unknown_reason: Option<String>,
    #[serde(deserialize_with = "required_option")]
    retry: Option<RetrySchedule>
});
object!(UnitView {
    spec: UnitSpec,
    attempts: Vec<AttemptView>,
    #[serde(deserialize_with = "required_option")]
    outcome: Option<UnitOutcome>,
    #[serde(deserialize_with = "required_option")]
    diagnostic: Option<String>
});
object!(RunView {
    spec: RunSpec,
    identity: RunIdentity,
    units: Vec<UnitView>,
    #[serde(deserialize_with = "required_option")]
    outcome: Option<RunOutcome>,
    #[serde(deserialize_with = "required_option")]
    diagnostic: Option<String>,
    budget_stopped: bool
});
object!(BudgetTotals {
    accounted_tokens: u64,
    held_tokens: u64,
    dispatched_requests: u64,
    held_requests: u64,
    remaining_tokens: u64,
    remaining_requests: u64
});
object!(BudgetView {
    schema_version: u32,
    authority: AuthoritySpec,
    sequence: u64,
    last_sha256: String,
    global_stopped: bool,
    totals: BudgetTotals,
    runs: Vec<RunView>,
    #[serde(deserialize_with = "required_option")]
    price_usd: Option<String>
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum BudgetEvent {
    RunRegistered {
        spec: RunSpec,
        identity: RunIdentity,
    },
    Prepared {
        plan: AttemptPlan,
        identity: AttemptIdentity,
        files: BTreeMap<String, FileStamp>,
    },
    Reserved {
        key: AttemptKey,
    },
    MaySend {
        key: AttemptKey,
    },
    TransportSealed {
        key: AttemptKey,
        files: BTreeMap<String, FileStamp>,
    },
    ReservedReleased {
        key: AttemptKey,
        reason: String,
    },
    UnknownRetained {
        key: AttemptKey,
        reason: String,
    },
    ReceiptSettled {
        key: AttemptKey,
        receipt: AttemptReceipt,
        files: BTreeMap<String, FileStamp>,
    },
    RetryScheduled {
        key: AttemptKey,
        schedule: RetrySchedule,
    },
    UnitFinished {
        run_id: String,
        unit_id: String,
        outcome: UnitOutcome,
        diagnostic: String,
    },
    RunFinished {
        run_id: String,
        outcome: RunOutcome,
        diagnostic: String,
    },
    RunInvalidated {
        run_id: String,
        diagnostic: String,
    },
}
object!(EventRecord {
    schema_version: u32,
    authority_id: String,
    sequence: u64,
    previous_sha256: String,
    at_utc: String,
    event: BudgetEvent
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DenialScope {
    Global,
    Run,
    UnitAttempts,
    UnitRetryTokens,
    Deadline,
    Terminal,
    RetryNotDue,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReservationDecision {
    Reserved,
    AlreadyReserved,
    Denied { scope: DenialScope, reason: String },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DispatchDecision {
    Permitted,
    AlreadyMaySend,
    Denied,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettlementDecision {
    Applied,
    AlreadySettled,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn required_nullable_price_is_not_an_omitted_default() {
        assert!(
            serde_json::from_str::<BudgetLimits>(r#"{"max_requests":2,"max_total_tokens":8}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<BudgetLimits>(
                r#"{"max_requests":2,"max_total_tokens":8,"max_cost_usd":null,"extra":0}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<BudgetLimits>(
                r#"{"max_requests":2,"max_total_tokens":8,"max_cost_usd":null}"#
            )
            .is_ok()
        );
    }
    #[test]
    fn tagged_events_refuse_unknown_fields_and_unknown_state() {
        assert!(serde_json::from_str::<BudgetEvent>(r#"{"kind":"unknown_retained","key":{"run_id":"r","unit_id":"u","attempt_no":1,"request_id":"q"},"reason":"timeout","released":true}"#).is_err());
        assert!(serde_json::from_str::<AttemptState>(r#""zero_billed""#).is_err());
    }
}
