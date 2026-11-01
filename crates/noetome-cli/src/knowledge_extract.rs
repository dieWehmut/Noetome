//! Core orchestration for the single-request unverified candidate contract.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use same_file::Handle;
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::model_candidate::*;
use crate::model_provider::{
    self, AttemptStatus, Budget, BudgetReservation, Provider, SettlementLedger, SettlementRecord,
};
use crate::{project, sidecar, source, source_model, text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelError {
    pub code: &'static str,
}

#[cfg(test)]
mod credential_debug_tests {
    use super::*;
    #[test]
    fn options_debug_never_prints_credential_sentinels() {
        let secret = "B04_T02_SYNTHETIC_DEBUG_SECRET_6bca";
        let options = ExtractionOptions {
            version_id: "test-version".into(),
            span_ids: vec!["test-span".into()],
            policy: Policy::default(),
            budget: Budget {
                available_calls: 1,
                available_output_tokens: 1024,
            },
            credential_sentinels: vec![secret.into()],
        };
        assert!(!format!("{options:?}").contains(secret));
    }
}
impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}：候选操作未完成；已有资料和未知现场保留。", self.code)
    }
}
impl std::error::Error for ModelError {}
impl From<&'static str> for ModelError {
    fn from(code: &'static str) -> Self {
        Self { code }
    }
}
impl From<project::ProjectError> for ModelError {
    fn from(error: project::ProjectError) -> Self {
        Self { code: error.code() }
    }
}
impl From<sidecar::SidecarError> for ModelError {
    fn from(error: sidecar::SidecarError) -> Self {
        Self { code: error.code }
    }
}
fn require(condition: bool, code: &'static str) -> Result<(), ModelError> {
    if condition { Ok(()) } else { Err(code.into()) }
}
fn json<T: Serialize>(value: &T) -> Result<Vec<u8>, ModelError> {
    serde_json::to_vec_pretty(value).map_err(|_| "MODEL_SERIALIZATION_FAILED".into())
}
fn parse<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, ModelError> {
    serde_json::from_slice(bytes).map_err(|_| "MODEL_SAVED_CONTRACT_INVALID".into())
}
fn now() -> Result<String, ModelError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|_| "MODEL_TIME_FAILED".into())
}

macro_rules! record {
    ($name:ident { $($(#[$attribute:meta])* $field:ident : $kind:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        pub struct $name { $($(#[$attribute])* pub $field: $kind),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self,D::Error> {
                #[derive(Deserialize)] #[serde(deny_unknown_fields)]
                struct Fields { $($(#[$attribute])* $field:$kind),* }
                let fields: Fields = crate::pdf_model::deserialize_object(deserializer)?;
                Ok(Self { $($field:fields.$field),* })
            }
        }
    };
}

#[derive(Clone)]
pub struct ExtractionOptions {
    pub version_id: String,
    pub span_ids: Vec<String>,
    pub policy: Policy,
    pub budget: Budget,
    /// Transient values, never serialized. Real credentials belong to the adapter.
    pub credential_sentinels: Vec<String>,
}
impl std::fmt::Debug for ExtractionOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtractionOptions")
            .field("version_id", &self.version_id)
            .field("span_ids", &self.span_ids)
            .field("policy", &self.policy)
            .field("budget", &self.budget)
            .field("credential_sentinels", &"[redacted]")
            .finish()
    }
}
record!(ReportFailure { code: String });
record!(AttemptFacts {
    status: AttemptStatus,
    #[serde(deserialize_with="crate::pdf_model::required_option")]
    pid: Option<u32>, elapsed_ms: u64, response_bytes: u64,
    #[serde(deserialize_with="crate::pdf_model::required_option")]
    response_sha256: Option<String>, stderr_bytes: u64,
    #[serde(deserialize_with="crate::pdf_model::required_option")]
    stderr_sha256: Option<String>
});
record!(CandidateReport {
    schema_version: u32, protocol: String, request_id: String, request_sha256: String,
    started_at: String, finished_at: String, result: String,
    #[serde(deserialize_with="crate::pdf_model::required_option")]
    failure: Option<ReportFailure>, attempt: AttemptFacts, reservation: BudgetReservation,
    provider_invocations: u32, real_api_calls: u32, usage: Usage,
    #[serde(deserialize_with="crate::pdf_model::required_option")]
    settlement: Option<SettlementRecord>,
    #[serde(deserialize_with="crate::pdf_model::required_option")]
    actual_model: Option<String>, response_saved: bool,
    #[serde(deserialize_with="crate::pdf_model::required_option")]
    candidate_sha256: Option<String>,
    #[serde(deserialize_with="crate::pdf_model::required_option")]
    candidate_id: Option<String>, verification: String, source: SourceSlice
});
record!(Owner {
    schema_version: u32,
    protocol: String,
    request_id: String,
    request_sha256: String
});

struct SourceReceipt {
    root: PathBuf,
    slice: SourceSlice,
    dirs: Vec<(PathBuf, Handle)>,
    identities: Vec<(PathBuf, Handle)>,
    _files: Vec<File>,
}
impl SourceReceipt {
    fn acquire(project_root: &Path, version_id: &str, ids: &[String]) -> Result<Self, ModelError> {
        require(
            !ids.is_empty() && ids.len() <= 256,
            "MODEL_SOURCE_SELECTION_INVALID",
        )?;
        let root = project::check_components(project_root)?;
        let mut dirs = sidecar::guard_chain(&root)?;
        let mut paths = vec![root.join(project::CONFIG), root.join(project::DATABASE)];
        let mut files = vec![
            project::read_guard(&root.join(project::CONFIG))?,
            project::read_guard(&root.join(project::DATABASE))?,
        ];
        let info = project::inspect(&root)?;
        let (store, _, _) =
            source::database_state(&root.join(project::DATABASE), &root, &info, None, None)?;
        project::reject_sidecars(&root)?;
        let version = store
            .versions
            .get(version_id)
            .ok_or(ModelError::from("MODEL_SOURCE_VERSION_UNKNOWN"))?;
        require(
            version.media_type != "application/pdf",
            "MODEL_SOURCE_CAPABILITY_INSUFFICIENT",
        )?;
        require(version.ingest_status == "ready", "MODEL_SOURCE_NOT_READY")?;
        let wanted: BTreeSet<_> = ids.iter().collect();
        require(wanted.len() == ids.len(), "MODEL_SOURCE_SELECTION_INVALID")?;
        let all = store
            .spans
            .get(version_id)
            .ok_or(ModelError::from("MODEL_SOURCE_SELECTION_INVALID"))?;
        let selected: Vec<_> = all
            .iter()
            .filter(|span| wanted.contains(&span.span_id))
            .collect();
        require(
            selected.len() == ids.len(),
            "MODEL_SOURCE_SELECTION_INVALID",
        )?;
        let normalized_path = root.join(
            version
                .normalized_snapshot
                .as_ref()
                .ok_or(ModelError::from("MODEL_SOURCE_NOT_READY"))?,
        );
        dirs.extend(sidecar::guard_chain(
            normalized_path
                .parent()
                .ok_or(ModelError::from("MODEL_SOURCE_CHANGED"))?,
        )?);
        paths.push(root.join(&version.raw_snapshot));
        paths.push(normalized_path.clone());
        files.push(project::read_guard(&root.join(&version.raw_snapshot))?);
        files.push(project::read_guard(&normalized_path)?);
        // Revalidate after acquiring selected snapshot handles; they stay alive through save.
        let (again, _, _) =
            source::database_state(&root.join(project::DATABASE), &root, &info, None, None)?;
        require(again == store, "MODEL_SOURCE_CHANGED")?;
        let bytes = source_model::read_snapshot(&normalized_path, 64 * 1024 * 1024, None)?;
        require(
            sidecar::digest(&bytes)
                == *version
                    .normalized_sha256
                    .as_ref()
                    .ok_or(ModelError::from("MODEL_SOURCE_NOT_READY"))?,
            "MODEL_SOURCE_CHANGED",
        )?;
        let normalized =
            std::str::from_utf8(&bytes).map_err(|_| ModelError::from("MODEL_SOURCE_NOT_READY"))?;
        let mut units = Vec::new();
        for span in selected {
            require(
                span.parse_status == text::ParseStatus::Parsed
                    && span.kind != text::SpanKind::Formatting,
                "MODEL_SOURCE_NOT_READY",
            )?;
            let text = normalized
                .get(span.start_byte..span.end_byte)
                .ok_or(ModelError::from("MODEL_SOURCE_CHANGED"))?;
            require(
                !text.trim().is_empty() && sidecar::digest(text.as_bytes()) == span.text_sha256,
                "MODEL_SOURCE_CHANGED",
            )?;
            units.push(SourceUnit {
                kind: SourceUnitKind::TextSpan,
                span_id: span.span_id.clone(),
                ordinal: span.ordinal as u64,
                start_byte: span.start_byte as u64,
                end_byte: span.end_byte as u64,
                text_sha256: span.text_sha256.clone(),
                text: text.into(),
            });
        }
        let slice = SourceSlice {
            project_id: info.project_id,
            source_id: version.source_id.clone(),
            version_id: version.version_id.clone(),
            revision: version.revision,
            raw_sha256: version.content_sha256.clone(),
            normalized_sha256: version
                .normalized_sha256
                .clone()
                .ok_or(ModelError::from("MODEL_SOURCE_NOT_READY"))?,
            normalization_version: version
                .normalization_version
                .clone()
                .ok_or(ModelError::from("MODEL_SOURCE_NOT_READY"))?,
            parser_version: text::PARSER_VERSION.into(),
            units,
        };
        let identities = paths
            .into_iter()
            .zip(files.iter())
            .map(|(path, file)| {
                let cloned = file
                    .try_clone()
                    .map_err(|_| ModelError::from("MODEL_SOURCE_CHANGED"))?;
                let handle = Handle::from_file(cloned)
                    .map_err(|_| ModelError::from("MODEL_SOURCE_CHANGED"))?;
                sidecar::ensure_identity(&path, &handle, "MODEL_SOURCE_CHANGED")?;
                Ok((path, handle))
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        Ok(Self {
            root,
            slice,
            dirs,
            identities,
            _files: files,
        })
    }
    fn verify(&self) -> Result<(), ModelError> {
        for (path, handle) in &self.dirs {
            sidecar::ensure_identity(path, handle, "MODEL_SOURCE_CHANGED")?;
        }
        for (path, handle) in &self.identities {
            sidecar::ensure_identity(path, handle, "MODEL_SOURCE_CHANGED")?;
        }
        let ids: Vec<_> = self
            .slice
            .units
            .iter()
            .map(|unit| unit.span_id.clone())
            .collect();
        let again = Self::acquire(&self.root, &self.slice.version_id, &ids)?;
        require(again.slice == self.slice, "MODEL_SOURCE_CHANGED")
    }
}

struct CandidateStore {
    root: PathBuf,
    dirs: Vec<(PathBuf, Handle)>,
    files: Vec<sidecar::OwnedFile>,
    names: Vec<String>,
}
impl CandidateStore {
    fn read(&self, name: &str, limit: u64, length: Option<u64>) -> Result<Vec<u8>, ModelError> {
        let index = self
            .names
            .iter()
            .position(|value| value == name)
            .ok_or(ModelError::from("MODEL_OUTPUT_CHANGED"))?;
        let bytes = self
            .files
            .get(index)
            .ok_or(ModelError::from("MODEL_OUTPUT_CHANGED"))?
            .read_owned(limit)?;
        require(
            length.is_none_or(|length| length == bytes.len() as u64),
            "MODEL_OUTPUT_CHANGED",
        )?;
        Ok(bytes)
    }
    fn create(
        project_root: &Path,
        output: &Path,
        request: &ExtractionRequest,
        bytes: &[u8],
        cancel: &AtomicBool,
    ) -> Result<Self, ModelError> {
        require(!cancel.load(Ordering::Acquire), "MODEL_CANCELLED")?;
        let root = project::check_components(output)?;
        require(!project::exists(&root)?, "MODEL_OUTPUT_EXISTS")?;
        let parent = root
            .parent()
            .ok_or(ModelError::from("MODEL_OUTPUT_INVALID"))?;
        // Actual directory identities handle Windows spelling aliases and links.
        for ancestor in parent.ancestors() {
            require(
                !same_file::is_same_file(ancestor, project_root)
                    .map_err(|_| ModelError::from("MODEL_OUTPUT_INVALID"))?,
                "MODEL_OUTPUT_OVERLAP",
            )?;
        }
        let mut dirs = sidecar::guard_chain(parent)?;
        fs::create_dir(&root).map_err(|_| ModelError::from("MODEL_OUTPUT_EXISTS"))?;
        dirs.push((root.clone(), sidecar::guard_directory(&root)?));
        let owner = Owner {
            schema_version: 1,
            protocol: PROTOCOL.into(),
            request_id: request.request_id.clone(),
            request_sha256: sidecar::digest(bytes),
        };
        let mut store = Self {
            root,
            dirs,
            files: Vec::new(),
            names: Vec::new(),
        };
        store.add("owner.json", &json(&owner)?, cancel)?;
        store.add("request.json", bytes, cancel)?;
        Ok(store)
    }
    fn verify(&self, cancel: &AtomicBool) -> Result<(), ModelError> {
        require(!cancel.load(Ordering::Acquire), "MODEL_CANCELLED")?;
        for (path, handle) in &self.dirs {
            sidecar::ensure_identity(path, handle, "MODEL_OUTPUT_CHANGED")?;
        }
        let wanted: BTreeSet<_> = self.names.iter().cloned().collect();
        source_model::exact_names(
            &self.root,
            &wanted.iter().map(std::ffi::OsString::from).collect(),
        )?;
        for file in &self.files {
            file.verify(cancel)?;
        }
        Ok(())
    }
    fn add(&mut self, name: &str, bytes: &[u8], cancel: &AtomicBool) -> Result<(), ModelError> {
        self.verify(cancel)?;
        self.files.push(sidecar::OwnedFile::create(
            self.root.join(name),
            bytes,
            cancel,
        )?);
        self.names.push(name.into());
        self.verify(cancel)
    }
    fn commit(&mut self, bytes: &[u8], cancel: &AtomicBool) -> Result<(), ModelError> {
        self.verify(cancel)?;
        let pending =
            sidecar::OwnedFile::pending(self.root.join(".report.pending.json"), bytes, cancel)?;
        self.names.push(".report.pending.json".into());
        self.verify(cancel)?;
        pending.verify(cancel)?;
        let report = pending.publish(self.root.join("report.json"))?;
        self.names.pop();
        self.names.push("report.json".into());
        self.files.push(report);
        self.verify(cancel)
    }
}

fn unknown_usage(producer: Producer) -> Usage {
    Usage {
        provenance: if producer == Producer::Fake {
            UsageProvenance::Simulated
        } else {
            UsageProvenance::ProviderReported
        },
        input_tokens: None,
        output_tokens: None,
        cost: Cost {
            status: CostStatus::Unknown,
            amount_microunits: None,
            currency: None,
            basis: CostBasis::NotAvailable,
        },
    }
}
fn contains_secret(bytes: &[u8], sentinels: &[String]) -> bool {
    sentinels.iter().any(|secret| {
        if secret.is_empty() {
            return false;
        }
        let raw = secret.as_bytes();
        let escaped = serde_json::to_string(secret).unwrap_or_default();
        let escaped = escaped
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or("");
        bytes.windows(raw.len()).any(|part| part == raw)
            || (!escaped.is_empty()
                && bytes
                    .windows(escaped.len())
                    .any(|part| part == escaped.as_bytes()))
    })
}
fn decoded_secret(outcome: &Outcome, sentinels: &[String]) -> bool {
    fn scan(value: &serde_json::Value, sentinels: &[String]) -> bool {
        match value {
            serde_json::Value::String(text) => sentinels
                .iter()
                .any(|secret| !secret.is_empty() && text.contains(secret)),
            serde_json::Value::Array(values) => values.iter().any(|value| scan(value, sentinels)),
            serde_json::Value::Object(values) => {
                values.values().any(|value| scan(value, sentinels))
            }
            _ => false,
        }
    }
    // The strict DTO was decoded directly from the original bytes; this Value
    // is only for redaction, never a duplicate-field or contract validator.
    serde_json::to_value(outcome).map_or(true, |value| scan(&value, sentinels))
}
fn provider_failure(failure: Failure) -> &'static str {
    match failure {
        Failure::Authentication => "MODEL_AUTHENTICATION",
        Failure::RateLimit => "MODEL_RATE_LIMIT",
        Failure::Timeout => "MODEL_TIMEOUT",
        Failure::CapabilityInsufficient => "MODEL_CAPABILITY_INSUFFICIENT",
        Failure::RemoteUnknown => "MODEL_REMOTE_UNKNOWN",
    }
}
fn attempt_failure(status: AttemptStatus) -> &'static str {
    match status {
        AttemptStatus::Timeout => "MODEL_TIMEOUT",
        AttemptStatus::NotCalled => "MODEL_NOT_CALLED",
        AttemptStatus::ResponseLimit => "MODEL_RESPONSE_LIMIT",
        AttemptStatus::Cancelled => "MODEL_CANCELLED",
        AttemptStatus::TransportFailed => "MODEL_TRANSPORT_FAILED",
        _ => "MODEL_REMOTE_UNKNOWN",
    }
}
fn validate_attempt(facts: &AttemptFacts, policy: &Policy) -> Result<(), ModelError> {
    let empty = sidecar::digest(&[]);
    require(
        facts.pid.is_none_or(|pid| pid > 0)
            && facts.stderr_bytes <= 65536
            && facts
                .stderr_sha256
                .as_deref()
                .is_none_or(source_model::valid_hash),
        "MODEL_SAVED_ATTEMPT_INVALID",
    )?;
    require(
        facts.stderr_bytes == 0 || facts.stderr_sha256.is_some(),
        "MODEL_SAVED_ATTEMPT_INVALID",
    )?;
    require(
        facts.stderr_bytes != 0
            || facts
                .stderr_sha256
                .as_deref()
                .is_none_or(|sha| sha == empty),
        "MODEL_SAVED_ATTEMPT_INVALID",
    )?;
    if facts.status == AttemptStatus::Delivered {
        require(
            facts.pid.is_some()
                && facts.response_bytes <= policy.max_response_bytes
                && facts
                    .response_sha256
                    .as_deref()
                    .is_some_and(source_model::valid_hash)
                && facts.stderr_sha256.is_some(),
            "MODEL_SAVED_ATTEMPT_INVALID",
        )?;
        require(
            facts.response_bytes != 0 || facts.response_sha256.as_deref() == Some(empty.as_str()),
            "MODEL_SAVED_ATTEMPT_INVALID",
        )?;
    } else {
        require(
            facts.response_bytes == 0 && facts.response_sha256.is_none(),
            "MODEL_SAVED_ATTEMPT_INVALID",
        )?;
        if facts.status == AttemptStatus::NotCalled {
            require(
                facts.pid.is_none() && facts.stderr_bytes == 0 && facts.stderr_sha256.is_none(),
                "MODEL_SAVED_ATTEMPT_INVALID",
            )?;
        }
        if matches!(
            facts.status,
            AttemptStatus::Timeout | AttemptStatus::ResponseLimit
        ) {
            require(facts.pid.is_some(), "MODEL_SAVED_ATTEMPT_INVALID")?;
        }
    }
    Ok(())
}

pub fn extract(
    project_root: &Path,
    output: &Path,
    provider: &dyn Provider,
    options: &ExtractionOptions,
    cancel: &AtomicBool,
) -> Result<CandidateReport, ModelError> {
    let started_at = now()?;
    let receipt = SourceReceipt::acquire(project_root, &options.version_id, &options.span_ids)?;
    let request = ExtractionRequest {
        schema_version: 1,
        protocol: PROTOCOL.into(),
        request_id: Uuid::new_v4().to_string(),
        provider: provider.identity().clone(),
        source: receipt.slice.clone(),
        policy: options.policy.clone(),
    };
    validate_request(&request)?;
    let request_bytes = json(&request)?;
    require(request_bytes.len() <= 65536, "MODEL_REQUEST_LIMIT")?;
    require(
        !contains_secret(&request_bytes, &options.credential_sentinels)
            && !request.source.units.iter().any(|unit| {
                options
                    .credential_sentinels
                    .iter()
                    .any(|secret| !secret.is_empty() && unit.text.contains(secret))
            }),
        "MODEL_CREDENTIAL_ECHO",
    )?;
    let reservation = options.budget.admit(&request.policy)?;
    receipt.verify()?;
    let mut store =
        CandidateStore::create(&receipt.root, output, &request, &request_bytes, cancel)?;
    let attempt = model_provider::run(provider, &request, &request_bytes, &store.root, cancel);
    let invocations = u32::from(attempt.pid.is_some());
    let facts = AttemptFacts {
        status: attempt.status,
        pid: attempt.pid,
        elapsed_ms: attempt.elapsed_ms,
        response_bytes: attempt.response_bytes,
        response_sha256: attempt.response_sha256.clone(),
        stderr_bytes: attempt.stderr_bytes,
        stderr_sha256: attempt.stderr_sha256.clone(),
    };
    let mut report = CandidateReport {
        schema_version: 1,
        protocol: PROTOCOL.into(),
        request_id: request.request_id.clone(),
        request_sha256: sidecar::digest(&request_bytes),
        started_at,
        finished_at: now()?,
        result: "rejected".into(),
        failure: Some(ReportFailure {
            code: attempt_failure(attempt.status).into(),
        }),
        attempt: facts,
        reservation,
        provider_invocations: invocations,
        real_api_calls: if request.provider.producer == Producer::Model {
            invocations
        } else {
            0
        },
        usage: unknown_usage(request.provider.producer),
        settlement: None,
        actual_model: None,
        response_saved: false,
        candidate_sha256: None,
        candidate_id: None,
        verification: "not_applicable".into(),
        source: request.source.clone(),
    };
    let mut ir = None;
    let mut accepted = None;
    if let Some(bytes) = attempt.response {
        if contains_secret(&bytes, &options.credential_sentinels) {
            report.failure = Some(ReportFailure {
                code: "MODEL_CREDENTIAL_ECHO".into(),
            });
        } else {
            match decode_auditable_outcome(&bytes, &request, &report.request_sha256) {
                Ok(outcome) if decoded_secret(&outcome, &options.credential_sentinels) => {
                    report.failure = Some(ReportFailure {
                        code: "MODEL_CREDENTIAL_ECHO".into(),
                    });
                }
                Ok(outcome) => {
                    report.usage = outcome.usage.clone();
                    report.actual_model = Some(outcome.actual_model.clone());
                    let mut ledger = SettlementLedger::default();
                    ledger.confirm(
                        &request.request_id,
                        &sidecar::digest(&bytes),
                        &outcome.usage,
                    )?;
                    report.settlement = ledger.records().into_iter().next();
                    report.response_saved = true;
                    match decode_outcome(&bytes, &request, &report.request_sha256) {
                        Ok(valid) if valid.status == OutcomeStatus::Complete => {
                            let candidate = make_candidate_ir(
                                &valid,
                                &request,
                                &report.request_sha256,
                                &sidecar::digest(&bytes),
                            )?;
                            let candidate_bytes = json(&candidate)?;
                            report.candidate_sha256 = Some(sidecar::digest(&candidate_bytes));
                            report.candidate_id = Some(candidate.candidate_id.clone());
                            report.verification = "unverified".into();
                            report.result = "candidate".into();
                            report.failure = None;
                            ir = Some(candidate_bytes);
                        }
                        Ok(valid) => {
                            report.result = "provider_failed".into();
                            report.failure = Some(ReportFailure {
                                code: provider_failure(
                                    valid
                                        .failure
                                        .ok_or(ModelError::from("MODEL_OUTCOME_INVALID"))?,
                                )
                                .into(),
                            });
                        }
                        Err(code) => {
                            report.failure = Some(ReportFailure { code: code.into() });
                        }
                    }
                    accepted = Some(bytes);
                }
                Err(code) => {
                    report.failure = Some(ReportFailure { code: code.into() });
                }
            }
        }
    }
    // Finish persisting the confirmed response/failure even if the caller's
    // cancellation signal changed, so a local save is never called unbilled.
    let persist = AtomicBool::new(false);
    receipt.verify()?;
    if let Some(bytes) = accepted {
        store.add("response.json", &bytes, &persist)?;
    }
    if let Some(bytes) = ir {
        store.add("candidate.json", &bytes, &persist)?;
    }
    report.finished_at = now()?;
    store.commit(&json(&report)?, &persist)?;
    let reloaded = inspect_internal(project_root, &store.root, Some(&store))?;
    require(reloaded == report, "MODEL_SAVED_CONTRACT_INVALID")?;
    Ok(report)
}

pub fn inspect(project_root: &Path, root: &Path) -> Result<CandidateReport, ModelError> {
    inspect_internal(project_root, root, None)
}
fn inspect_internal(
    project_root: &Path,
    root: &Path,
    owned: Option<&CandidateStore>,
) -> Result<CandidateReport, ModelError> {
    let root = project::check_components(root)?;
    let dirs = sidecar::guard_chain(&root)?;
    let read = |name: &str, limit: u64, length: Option<u64>| match owned {
        Some(store) => store.read(name, limit, length),
        None => Ok(source_model::read_single_snapshot(
            &root.join(name),
            limit,
            length,
        )?),
    };
    let report_bytes = read("report.json", 2 * 1024 * 1024, None)?;
    let report: CandidateReport = parse(&report_bytes)?;
    let mut names: BTreeSet<std::ffi::OsString> = ["owner.json", "request.json", "report.json"]
        .into_iter()
        .map(Into::into)
        .collect();
    if report.response_saved {
        names.insert("response.json".into());
    }
    if report.result == "candidate" {
        names.insert("candidate.json".into());
    }
    source_model::exact_names(&root, &names)?;
    let mut guards = Vec::new();
    if owned.is_none() {
        for name in &names {
            guards.push(project::read_guard(&root.join(name))?);
        }
    }
    let report_again = read("report.json", 2 * 1024 * 1024, None)?;
    require(report_bytes == report_again, "MODEL_OUTPUT_CHANGED")?;
    let owner: Owner = parse(&read("owner.json", 4096, None)?)?;
    let request_bytes = read("request.json", 65536, None)?;
    let request: ExtractionRequest = parse(&request_bytes)?;
    validate_request(&request)?;
    let request_sha = sidecar::digest(&request_bytes);
    require(
        owner
            == Owner {
                schema_version: 1,
                protocol: PROTOCOL.into(),
                request_id: request.request_id.clone(),
                request_sha256: request_sha.clone(),
            },
        "MODEL_SAVED_IDENTITY_INVALID",
    )?;
    require(
        report.schema_version == 1
            && report.protocol == PROTOCOL
            && report.request_id == request.request_id
            && report.request_sha256 == request_sha
            && report.source == request.source,
        "MODEL_SAVED_IDENTITY_INVALID",
    )?;
    let start = OffsetDateTime::parse(&report.started_at, &Rfc3339)
        .map_err(|_| ModelError::from("MODEL_SAVED_CONTRACT_INVALID"))?;
    let finish = OffsetDateTime::parse(&report.finished_at, &Rfc3339)
        .map_err(|_| ModelError::from("MODEL_SAVED_CONTRACT_INVALID"))?;
    require(
        start.offset().is_utc() && finish.offset().is_utc() && finish >= start,
        "MODEL_SAVED_CONTRACT_INVALID",
    )?;
    require(
        report.reservation.calls == 1
            && report.reservation.output_tokens == request.policy.max_output_tokens,
        "MODEL_SAVED_CONTRACT_INVALID",
    )?;
    require(
        report.provider_invocations == u32::from(report.attempt.pid.is_some())
            && report.attempt.pid.is_none_or(|pid| pid > 0)
            && report.real_api_calls
                == if request.provider.producer == Producer::Fake {
                    0
                } else {
                    report.provider_invocations
                },
        "MODEL_SAVED_CONTRACT_INVALID",
    )?;
    validate_attempt(&report.attempt, &request.policy)?;
    require(
        report
            .attempt
            .response_sha256
            .as_deref()
            .is_none_or(source_model::valid_hash)
            && report
                .attempt
                .stderr_sha256
                .as_deref()
                .is_none_or(source_model::valid_hash),
        "MODEL_SAVED_CONTRACT_INVALID",
    )?;
    let ids: Vec<_> = request
        .source
        .units
        .iter()
        .map(|unit| unit.span_id.clone())
        .collect();
    let receipt = SourceReceipt::acquire(project_root, &request.source.version_id, &ids)?;
    require(receipt.slice == request.source, "MODEL_SOURCE_CHANGED")?;
    if report.response_saved {
        require(
            report.attempt.status == AttemptStatus::Delivered && report.provider_invocations == 1,
            "MODEL_SAVED_CONTRACT_INVALID",
        )?;
        let bytes = read(
            "response.json",
            request.policy.max_response_bytes,
            Some(report.attempt.response_bytes),
        )?;
        let sha = sidecar::digest(&bytes);
        require(
            report.attempt.response_sha256.as_deref() == Some(sha.as_str()),
            "MODEL_SAVED_CONTRACT_INVALID",
        )?;
        let outcome = decode_auditable_outcome(&bytes, &request, &request_sha)?;
        require(
            report.usage == outcome.usage
                && report.actual_model.as_deref() == Some(outcome.actual_model.as_str()),
            "MODEL_SAVED_CONTRACT_INVALID",
        )?;
        let mut ledger = SettlementLedger::default();
        ledger.confirm(&request.request_id, &sha, &outcome.usage)?;
        require(
            report.settlement == ledger.records().into_iter().next(),
            "MODEL_SAVED_CONTRACT_INVALID",
        )?;
        require(
            ledger.confirm(&request.request_id, &sha, &outcome.usage)?
                == model_provider::SettlementResult::AlreadyConfirmed,
            "MODEL_SETTLEMENT_INVALID",
        )?;
        let strict = decode_outcome(&bytes, &request, &request_sha);
        if let Ok(valid) = strict {
            if valid.status == OutcomeStatus::Complete {
                require(
                    report.result == "candidate"
                        && report.failure.is_none()
                        && report.verification == "unverified",
                    "MODEL_SAVED_CONTRACT_INVALID",
                )?;
                let bytes = read("candidate.json", 2 * 1024 * 1024, None)?;
                require(
                    report.candidate_sha256.as_deref() == Some(sidecar::digest(&bytes).as_str()),
                    "MODEL_SAVED_CONTRACT_INVALID",
                )?;
                let ir: CandidateIr = parse(&bytes)?;
                require(
                    report.candidate_id.as_deref() == Some(ir.candidate_id.as_str()),
                    "MODEL_SAVED_CONTRACT_INVALID",
                )?;
                validate_saved_ir(&ir, &outcome, &request, &request_sha, &sha)?;
            } else {
                require(
                    report.result == "provider_failed"
                        && report.failure
                            == Some(ReportFailure {
                                code: provider_failure(
                                    valid
                                        .failure
                                        .ok_or(ModelError::from("MODEL_SAVED_CONTRACT_INVALID"))?,
                                )
                                .into(),
                            })
                        && report.candidate_sha256.is_none()
                        && report.candidate_id.is_none()
                        && report.verification == "not_applicable",
                    "MODEL_SAVED_CONTRACT_INVALID",
                )?;
            }
        } else {
            require(
                report.result == "rejected"
                    && report.failure
                        == Some(ReportFailure {
                            code: strict
                                .err()
                                .ok_or(ModelError::from("MODEL_SAVED_CONTRACT_INVALID"))?
                                .into(),
                        })
                    && report.candidate_sha256.is_none()
                    && report.candidate_id.is_none()
                    && report.verification == "not_applicable",
                "MODEL_SAVED_CONTRACT_INVALID",
            )?;
        }
    } else {
        let code = report.failure.as_ref().map(|failure| failure.code.as_str());
        if report.attempt.status == AttemptStatus::Delivered {
            require(
                matches!(
                    code,
                    Some(
                        "MODEL_RESPONSE_INVALID" | "MODEL_USAGE_INVALID" | "MODEL_CREDENTIAL_ECHO"
                    )
                ),
                "MODEL_SAVED_ATTEMPT_INVALID",
            )?;
            if report.attempt.response_bytes == 0 {
                require(
                    code == Some("MODEL_RESPONSE_INVALID"),
                    "MODEL_SAVED_ATTEMPT_INVALID",
                )?;
            }
        } else {
            require(
                code == Some(attempt_failure(report.attempt.status)),
                "MODEL_SAVED_ATTEMPT_INVALID",
            )?;
        }
        require(
            report.result == "rejected"
                && report.failure.as_ref().is_some_and(|failure| {
                    failure.code.starts_with("MODEL_")
                        && failure.code.len() <= 96
                        && failure
                            .code
                            .bytes()
                            .all(|byte| byte.is_ascii_uppercase() || byte == b'_')
                })
                && report.settlement.is_none()
                && report.actual_model.is_none()
                && report.usage == unknown_usage(request.provider.producer)
                && report.candidate_sha256.is_none()
                && report.candidate_id.is_none()
                && report.verification == "not_applicable",
            "MODEL_SAVED_CONTRACT_INVALID",
        )?;
    }
    for (path, handle) in &dirs {
        sidecar::ensure_identity(path, handle, "MODEL_OUTPUT_CHANGED")?;
    }
    receipt.verify()?;
    source_model::exact_names(&root, &names)?;
    Ok(report)
}
