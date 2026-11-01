//! One selected PDF, a pinned real Python adapter, and a new owned workspace.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

pub use crate::pdf_model::*;
use crate::sidecar::{
    ArtifactPolicy, InputSnapshot, OwnedFile, SidecarError, Workspace, WorkspacePolicy,
};
pub use crate::sidecar::{
    InputFingerprint, Producer, ProtocolViolation, ResponseStatus, UnitStatus,
};

const MAX_REPORT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct PdfParserSpec {
    pub python: PathBuf,
    pub adapter: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfParseStatus {
    Ready,
    NeedsReview,
    Failed,
}

impl<'de> Deserialize<'de> for PdfParseStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "ready" => Ok(Self::Ready),
            "needs_review" => Ok(Self::NeedsReview),
            "failed" => Ok(Self::Failed),
            _ => Err(serde::de::Error::custom("unknown PDF parse status")),
        }
    }
}

macro_rules! report_model {
    ($name:ident { $($(#[$attr:meta])* $field:ident : $kind:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Serialize)]
        pub struct $name { $($(#[$attr])* pub $field: $kind),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $($(#[$attr])* $field: $kind),* }
                let fields: Fields = crate::pdf_model::deserialize_object(deserializer)?;
                Ok(Self { $($field: fields.$field),* })
            }
        }
    };
}

report_model!(PdfFailure {
    code: String,
    message: String
});

report_model!(PdfPageDisplay {
    physical_page_idx: u32,
    displayed_physical_page: u32,
    #[serde(deserialize_with = "crate::pdf_model::required_option")]
    printed_page_label: Option<String>,
    status: UnitStatus,
});

report_model!(PdfParseReport {
    schema_version: u32,
    producer: Producer,
    operation: String,
    status: PdfParseStatus,
    input: InputFingerprint,
    request: PdfRequest,
    #[serde(deserialize_with = "crate::pdf_model::required_option")]
    response: Option<PdfResponse>,
    page_display: Vec<PdfPageDisplay>,
    validation_status: String,
    #[serde(deserialize_with = "crate::pdf_model::required_option")]
    failure: Option<PdfFailure>,
    violations: Vec<ProtocolViolation>,
    diagnostics: Vec<u8>,
    #[serde(deserialize_with = "crate::pdf_model::required_option")]
    pid: Option<u32>,
    #[serde(deserialize_with = "crate::pdf_model::required_option")]
    native_exit_code: Option<u32>,
    processing_elapsed_ms: u64,
    verified_artifact_count: u32,
    verified_artifact_bytes: u64,
    report_path: String,
});

#[derive(Debug)]
pub struct PdfParseOutput {
    pub json: String,
    pub failed: bool,
    pub exit_code: u8,
    pub report: PdfParseReport,
}

pub(crate) struct PdfParseHandoff {
    pub(crate) output: PdfParseOutput,
    pub(crate) receipt: PdfParseReceipt,
}

/// Retains the original native capabilities until the project writer has
/// recorded their identities. This capability is never serialized as evidence.
pub(crate) struct PdfParseReceipt {
    snapshot: InputSnapshot,
    workspace: Workspace,
    artifacts: Vec<OwnedFile>,
}

impl PdfParseReceipt {
    pub(crate) fn verify(&self, cancel: &AtomicBool) -> Result<(), SidecarError> {
        self.snapshot.verify(cancel)?;
        self.workspace.verify(cancel)?;
        for artifact in &self.artifacts {
            artifact.verify(cancel)?;
        }
        Ok(())
    }
}

pub fn default_adapter() -> Result<PathBuf, SidecarError> {
    // This development CLI is built with its selected repository adapter.
    // Relocatable application packaging is a later distribution task.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sidecars/pdf_parser/adapter.py")
        .canonicalize()
        .map_err(|error| failure("PDF_ADAPTER_INVALID", error))
}

pub fn error_exit_code(error: &SidecarError) -> u8 {
    match error.code {
        "SIDECAR_EXIT_NONZERO" => match error.native_exit_code {
            Some(1) => 1,
            Some(2) => 2,
            _ => 3,
        },
        "PDF_RUNTIME_INVALID"
        | "PDF_ADAPTER_INVALID"
        | "PDF_TYPE_UNSUPPORTED"
        | "PDF_LIMITS_INVALID"
        | "SIDECAR_UNSAFE_PATH"
        | "SIDECAR_LIMITS_INVALID"
        | "SIDECAR_REQUEST_LIMIT"
        | "SIDECAR_PROTOCOL_INVALID"
        | "SIDECAR_RESOURCE_INVALID" => 2,
        "PDF_REPORT_ERROR" => 1,
        _ => 3,
    }
}

pub fn parse_pdf(
    parser: &PdfParserSpec,
    selected_input: &Path,
    new_workspace: &Path,
    limits: PdfLimits,
    cancel: &AtomicBool,
) -> Result<PdfParseOutput, SidecarError> {
    parse_pdf_retained(parser, selected_input, new_workspace, limits, cancel)
        .map(|handoff| handoff.output)
}

pub(crate) fn parse_pdf_retained(
    parser: &PdfParserSpec,
    selected_input: &Path,
    new_workspace: &Path,
    limits: PdfLimits,
    cancel: &AtomicBool,
) -> Result<PdfParseHandoff, SidecarError> {
    let started = Instant::now();
    crate::sidecar::check_cancel(cancel)?;
    check_policy(&limits)?;
    if !selected_input
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        return Err(SidecarError::new(
            "PDF_TYPE_UNSUPPORTED",
            "Select one .pdf file explicitly.",
        ));
    }
    let python = trusted_program(&parser.python, "PDF_RUNTIME_INVALID")?;
    let adapter = trusted_program(&parser.adapter, "PDF_ADAPTER_INVALID")?;
    let _python_guard = crate::project::read_guard(&python)
        .map_err(|error| failure("PDF_RUNTIME_INVALID", error))?;
    let _adapter_guard = crate::project::read_guard(&adapter)
        .map_err(|error| failure("PDF_ADAPTER_INVALID", error))?;
    let snapshot =
        InputSnapshot::acquire(selected_input, new_workspace, &limits.transport, cancel)?;
    let request = PdfRequest {
        schema_version: 2,
        protocol: PROTOCOL.into(),
        request_id: Uuid::new_v4().to_string(),
        operation: PdfOperation::ParsePdf,
        input: snapshot.fingerprint(),
        workspace: WorkspacePolicy {
            artifact_directory: "artifacts".into(),
        },
        artifact_policy: ArtifactPolicy {
            max_file_bytes: limits.transport.max_artifact_bytes,
            max_total_bytes: limits.transport.max_total_artifact_bytes,
            max_files: limits.transport.max_artifacts,
        },
        pdf_policy: limits.policy,
        options_version: OPTIONS_VERSION.into(),
    };
    let request_bytes =
        serde_json::to_vec(&request).map_err(|error| failure("PDF_REPORT_ERROR", error))?;
    if request_bytes.len() > limits.transport.max_request_bytes {
        return Err(SidecarError::new(
            "SIDECAR_REQUEST_LIMIT",
            "PDF request exceeds its byte policy; no workspace was created.",
        ));
    }
    let mut workspace = Workspace::create(
        snapshot.workspace_path.clone(),
        &request.request_id,
        &request_bytes,
        &snapshot.raw,
        cancel,
    )?;
    workspace.verify(cancel)?;
    let args = [
        OsString::from("-I"),
        OsString::from("-B"),
        adapter.into_os_string(),
    ];
    let mut artifact_guards: Vec<OwnedFile> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut pid = None;
    let mut native_exit_code = None;
    let mut verification_started = false;
    let verified = (|| {
        let output = crate::sidecar_process::execute_command(
            &python,
            &args,
            &request_bytes,
            &workspace.root,
            &limits.transport,
            cancel,
        )?;
        pid = Some(output.pid);
        // The transport returns output only after observing native exit zero
        // and an empty process tree; nonzero exits take its classified error path.
        native_exit_code = Some(0);
        diagnostics = output.stderr;
        crate::sidecar::check_cancel(cancel)?;
        verification_started = true;
        let response: PdfResponse = serde_json::from_slice(&output.stdout).map_err(|error| {
            SidecarError::new(
                "SIDECAR_PROTOCOL_INVALID",
                format!(
                    "Expected one strict PDF response; category {:?} at line {}, column {}.",
                    error.classify(),
                    error.line(),
                    error.column()
                ),
            )
        })?;
        workspace.verify(cancel)?;
        snapshot.verify(cancel)?;
        let (bytes, guards) =
            workspace.read_artifacts(&response.artifacts, &limits.transport, cancel)?;
        artifact_guards = guards;
        let violations = validate_pdf(&request, &response, &bytes, &limits, cancel);
        if violations
            .iter()
            .any(|issue| issue.rule_id == "SIDECAR_CANCELLED")
        {
            return Err(SidecarError::new(
                "SIDECAR_CANCELLED",
                "Cancelled during PDF Core verification.",
            ));
        }
        if violations
            .iter()
            .any(|issue| issue.rule_id != "PDF_REPORTED_INCOMPLETE")
        {
            let mut error = SidecarError::new(
                "SIDECAR_PROTOCOL_INVALID",
                "PDF page, glyph, locator, text or identity validation failed.",
            );
            error.violations = violations;
            return Err(error);
        }
        for guard in &artifact_guards {
            guard.verify(cancel)?;
        }
        workspace.verify(cancel)?;
        workspace.verify_artifact_inventory(&response.artifacts)?;
        snapshot.verify(cancel)?;
        crate::sidecar::check_cancel(cancel)?;
        Ok((response, violations))
    })();
    let (response, violations, report_failure, validation_status, exit_code) = match verified {
        Ok((response, violations)) => {
            let report_failure = if response.status == ResponseStatus::Complete {
                None
            } else {
                Some(PdfFailure {
                    code: response
                        .issues
                        .first()
                        .map(|issue| issue.code.clone())
                        .unwrap_or_else(|| "PDF_PARSE_INCOMPLETE".into()),
                    message: "PDF解析包含失败或需复核范围；已确认页和原始证据已保留。".into(),
                })
            };
            let exit_code = if report_failure.is_none() { 0 } else { 3 };
            (
                Some(response),
                violations,
                report_failure,
                "passed",
                exit_code,
            )
        }
        Err(mut error) => {
            native_exit_code = error.native_exit_code;
            if pid.is_none() {
                pid = error.pid;
            }
            if diagnostics.is_empty() {
                diagnostics = std::mem::take(&mut error.diagnostics);
            }
            if matches!(error.code, "SIDECAR_CANCELLED" | "SIDECAR_CLEANUP_FAILED") {
                error.pid = pid;
                error.diagnostics = diagnostics;
                return Err(error);
            }
            let exit_code = error_exit_code(&error);
            let report_failure = Some(PdfFailure {
                code: error.code.into(),
                message: error.message,
            });
            (
                None,
                error.violations,
                report_failure,
                if verification_started {
                    "failed"
                } else {
                    "not_run"
                },
                exit_code,
            )
        }
    };
    let status = match response.as_ref() {
        Some(response) if response.status == ResponseStatus::Complete => PdfParseStatus::Ready,
        Some(response) if response.status == ResponseStatus::Partial => PdfParseStatus::NeedsReview,
        _ => PdfParseStatus::Failed,
    };
    let verified_artifact_count = response
        .as_ref()
        .map_or(0, |response| response.artifacts.len() as u32);
    let verified_artifact_bytes = response.as_ref().map_or(0, |response| {
        response
            .artifacts
            .iter()
            .map(|artifact| artifact.byte_length)
            .sum()
    });
    let page_display = response.as_ref().map_or_else(Vec::new, |response| {
        response
            .pages
            .iter()
            .map(|page| PdfPageDisplay {
                physical_page_idx: page.physical_page_index,
                displayed_physical_page: page.physical_page_index + 1,
                printed_page_label: page.printed_page_label.clone(),
                status: page.status,
            })
            .collect()
    });
    let report = PdfParseReport {
        schema_version: 1,
        producer: Producer::Parser,
        operation: "source_parse_pdf".into(),
        status,
        input: request.input.clone(),
        request,
        response,
        page_display,
        validation_status: validation_status.into(),
        failure: report_failure,
        violations,
        diagnostics,
        pid,
        native_exit_code,
        processing_elapsed_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        verified_artifact_count,
        verified_artifact_bytes,
        report_path: "parse.json".into(),
    };
    let mut bytes =
        serde_json::to_vec_pretty(&report).map_err(|error| failure("PDF_REPORT_ERROR", error))?;
    bytes.push(b'\n');
    if bytes.len() > MAX_REPORT_BYTES {
        return Err(SidecarError::new(
            "PDF_REPORT_LIMIT",
            "Host report exceeds its 8 MiB policy; original task files were preserved.",
        ));
    }
    workspace.prepare_report(&bytes, cancel)?;
    for guard in &artifact_guards {
        guard.verify(cancel)?;
    }
    if let Some(response) = &report.response {
        workspace.verify_artifact_inventory(&response.artifacts)?;
    }
    snapshot.verify(cancel)?;
    workspace.verify(cancel)?;
    crate::sidecar::check_cancel(cancel)?;
    let json = String::from_utf8(bytes).map_err(|error| failure("PDF_REPORT_ERROR", error))?;
    // Publishing is the delivery point. Cancellation before it leaves only an
    // explicitly uncommitted pending report; cancellation after it cannot turn
    // the already delivered result into an API failure.
    workspace.publish_report(cancel)?;
    Ok(PdfParseHandoff {
        output: PdfParseOutput {
            json,
            failed: exit_code != 0,
            exit_code,
            report,
        },
        receipt: PdfParseReceipt {
            snapshot,
            workspace,
            artifacts: artifact_guards,
        },
    })
}

fn trusted_program(path: &Path, code: &'static str) -> Result<PathBuf, SidecarError> {
    if !path.is_absolute() {
        return Err(SidecarError::new(
            code,
            "The trusted Python/adapter path must be absolute.",
        ));
    }
    let path = crate::sidecar::checked_path(path)?;
    crate::project::checked_file(&path).map_err(|error| failure(code, error))?;
    Ok(path)
}

fn check_policy(limits: &PdfLimits) -> Result<(), SidecarError> {
    crate::sidecar::check_limits(&limits.transport)?;
    let ceiling = PdfPolicy::default();
    if limits.policy.max_pages > ceiling.max_pages
        || limits.policy.max_glyphs > ceiling.max_glyphs
        || limits.policy.max_regions > ceiling.max_regions
        || limits.policy.max_objects > ceiling.max_objects
    {
        return Err(SidecarError::new(
            "PDF_LIMITS_INVALID",
            "PDF public policies may only tighten their bounded defaults.",
        ));
    }
    Ok(())
}

fn failure(code: &'static str, error: impl fmt::Display) -> SidecarError {
    SidecarError::new(code, error.to_string())
}

#[cfg(test)]
mod report_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn persisted_host_report_requires_named_fields_and_explicit_nulls() {
        let input =
            json!({"relative_path":"input/raw.bin","byte_length":1,"sha256":"a".repeat(64)});
        let value = json!({
            "schema_version":1,"producer":"parser","operation":"source_parse_pdf","status":"failed",
            "input":input,"request":{"schema_version":2,"protocol":PROTOCOL,
                "request_id":"febdeacd-a3cb-4d22-9e42-407cdb72a53b","operation":"parse_pdf","input":input,
                "workspace":{"artifact_directory":"artifacts"},"artifact_policy":{"max_file_bytes":1,"max_total_bytes":1,"max_files":1},
                "pdf_policy":{"max_pages":1,"max_glyphs":1,"max_regions":1,"max_objects":1},"options_version":OPTIONS_VERSION},
            "response":null,"page_display":[],"validation_status":"not_run",
            "failure":{"code":"SIDECAR_TIMEOUT","message":"actual deadline exceeded"},"violations":[],"diagnostics":[],
            "pid":null,"native_exit_code":null,"processing_elapsed_ms":1,
            "verified_artifact_count":0,"verified_artifact_bytes":0,"report_path":"parse.json"
        });
        assert!(serde_json::from_value::<PdfParseReport>(value.clone()).is_ok());
        for field in ["response", "failure", "pid", "native_exit_code"] {
            let mut changed = value.clone();
            changed.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<PdfParseReport>(changed).is_err(),
                "missing {field}"
            );
        }
        let mut changed = value.clone();
        changed["failure"] = json!(["SIDECAR_TIMEOUT", "actual deadline exceeded"]);
        assert!(serde_json::from_value::<PdfParseReport>(changed).is_err());
        let mut changed = value;
        changed["unknown"] = json!(1);
        assert!(serde_json::from_value::<PdfParseReport>(changed).is_err());
    }

    #[test]
    fn parse_status_rejects_object_enum_encoding() {
        assert!(serde_json::from_str::<PdfParseStatus>(r#"{"failed":null}"#).is_err());
        assert!(serde_json::from_str::<PdfParseStatus>(r#""failed""#).is_ok());
    }
}
