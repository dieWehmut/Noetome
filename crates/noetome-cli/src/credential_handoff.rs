//! Transient single-request credential transport, separate from saved model facts.
//!
//! This local boundary does not select a provider, authorize network access,
//! reserve a budget, or claim that a spawned process sent an API request.

use std::ffi::OsString;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::model_candidate::{ExtractionRequest, Outcome};
use crate::model_provider::AttemptStatus;

/// Opaque, bounded UTF8 credential. Never serialized, cloned or formatted.
/// Drop fills the owned bytes with zero; this is best effort and does not promise
/// optimizer-proof erasure, removal of OS copies, or protection from memory dumps.
pub struct TransientCredential {
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CredentialError {
    pub code: &'static str,
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code)
    }
}
impl std::error::Error for CredentialError {}

impl TransientCredential {
    pub fn new(bytes: &[u8]) -> Result<Self, CredentialError> {
        if !Self::valid(bytes) {
            return Err(CredentialError {
                code: "MODEL_CREDENTIAL_INVALID",
            });
        }
        Self::from_owned(bytes.to_vec())
    }

    fn valid(bytes: &[u8]) -> bool {
        !bytes.is_empty()
            && bytes.len() <= 4096
            && std::str::from_utf8(bytes).is_ok()
            && !bytes.iter().any(|byte| matches!(byte, 0 | b'\r' | b'\n'))
    }

    fn from_owned(mut bytes: Vec<u8>) -> Result<Self, CredentialError> {
        if !Self::valid(&bytes) {
            bytes.fill(0);
            return Err(CredentialError {
                code: "MODEL_CREDENTIAL_INVALID",
            });
        }
        Ok(Self { bytes })
    }

    /// Resolve only the explicitly named `env:NAME`; never enumerate variables.
    /// Caller must finish configuration, source and budget admission first.
    pub fn from_env_ref(reference: &str) -> Result<Self, CredentialError> {
        let Some(name) = reference.strip_prefix("env:") else {
            return Err(CredentialError {
                code: "MODEL_CREDENTIAL_REF_INVALID",
            });
        };
        if name.is_empty()
            || name.len() > 128
            || !name.bytes().enumerate().all(|(index, byte)| {
                byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
            })
        {
            return Err(CredentialError {
                code: "MODEL_CREDENTIAL_REF_INVALID",
            });
        }
        let value = std::env::var_os(name).ok_or(CredentialError {
            code: "MODEL_CREDENTIAL_MISSING",
        })?;
        let text = value.into_string().map_err(|_| CredentialError {
            code: "MODEL_CREDENTIAL_INVALID",
        })?;
        Self::from_owned(text.into_bytes())
    }
}

impl Drop for TransientCredential {
    fn drop(&mut self) {
        self.bytes.fill(0);
    }
}

/// Only clean, strictly identified response bytes may leave this boundary.
/// Suppressed streams have no body, byte count, SHA, prefix or diagnostic text.
pub struct SensitiveAttempt {
    pub status: AttemptStatus,
    pub pid: Option<u32>,
    pub elapsed_ms: u64,
    pub failure: Option<&'static str>,
    pub stdout_suppressed: bool,
    pub stderr_suppressed: bool,
    pub response: Option<Vec<u8>>,
    pub response_sha256: Option<String>,
    pub outcome: Option<Outcome>,
}

impl std::fmt::Debug for SensitiveAttempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SensitiveAttempt")
            .field("status", &self.status)
            .field("pid", &self.pid)
            .field("elapsed_ms", &self.elapsed_ms)
            .field("failure", &self.failure)
            .field("stdout_suppressed", &self.stdout_suppressed)
            .field("stderr_suppressed", &self.stderr_suppressed)
            .finish()
    }
}

/// Execute exactly one local process with a private credential prefix frame.
/// Budget admission and explicit network permission remain the caller's job.
/// An auditable but scientifically/structurally bad candidate can return clean
/// bytes and usage; the caller must still run the strict candidate validator.
pub fn run(
    program: &Path,
    args: &[OsString],
    original_request: &ExtractionRequest,
    wire: &[u8],
    workspace: &Path,
    credential: &TransientCredential,
    cancel: &AtomicBool,
) -> SensitiveAttempt {
    let started = Instant::now();
    let mut attempt = SensitiveAttempt {
        status: AttemptStatus::NotCalled,
        pid: None,
        elapsed_ms: 0,
        failure: Some("MODEL_NOT_CALLED"),
        stdout_suppressed: true,
        stderr_suppressed: true,
        response: None,
        response_sha256: None,
        outcome: None,
    };
    let result = execute(
        program,
        args,
        original_request,
        wire,
        workspace,
        credential,
        cancel,
    );
    match result {
        Ok((mut output, outcome)) => {
            attempt.status = AttemptStatus::Delivered;
            attempt.pid = Some(output.pid);
            output.stderr.fill(0);
            attempt.stdout_suppressed = false;
            attempt.failure = None;
            attempt.response_sha256 = Some(crate::text::sha256(&output.stdout));
            attempt.response = Some(output.stdout);
            attempt.outcome = Some(outcome);
        }
        Err(failure) => {
            attempt.status = failure.status;
            attempt.pid = failure.pid;
            attempt.failure = Some(failure.code);
        }
    }
    attempt.elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    attempt
}

struct Failure {
    status: AttemptStatus,
    pid: Option<u32>,
    code: &'static str,
}

fn not_called(code: &'static str) -> Failure {
    Failure {
        status: AttemptStatus::NotCalled,
        pid: None,
        code,
    }
}

fn decoded_contains_secret<T: serde::Serialize>(
    value: &T,
    credential: &TransientCredential,
) -> bool {
    fn scan(value: &serde_json::Value, secret: &str) -> bool {
        match value {
            serde_json::Value::String(text) => text.contains(secret),
            serde_json::Value::Array(values) => values.iter().any(|value| scan(value, secret)),
            serde_json::Value::Object(values) => values
                .iter()
                .any(|(key, value)| key.contains(secret) || scan(value, secret)),
            _ => false,
        }
    }
    // The DTO was decoded strictly from the original wire. Value is used only
    // for secret inspection, so duplicate/unknown fields cannot be folded away.
    let Ok(secret) = std::str::from_utf8(&credential.bytes) else {
        return true;
    };
    serde_json::to_value(value).map_or(true, |value| scan(&value, secret))
}

fn raw_contains_secret(bytes: &[u8], credential: &TransientCredential) -> bool {
    bytes
        .windows(credential.bytes.len())
        .any(|part| part == credential.bytes)
}

fn execute(
    program: &Path,
    args: &[OsString],
    request: &ExtractionRequest,
    wire: &[u8],
    workspace: &Path,
    credential: &TransientCredential,
    cancel: &AtomicBool,
) -> Result<(crate::sidecar_process::ProcessOutput, Outcome), Failure> {
    if crate::model_candidate::validate_request(request).is_err()
        || wire.len() > 65536
        || !serde_json::from_slice::<ExtractionRequest>(wire)
            .is_ok_and(|decoded| &decoded == request)
    {
        return Err(not_called("MODEL_REQUEST_INVALID"));
    }
    let secret = std::str::from_utf8(&credential.bytes)
        .map_err(|_| not_called("MODEL_CREDENTIAL_INVALID"))?;
    if raw_contains_secret(wire, credential)
        || decoded_contains_secret(request, credential)
        || program.as_os_str().to_string_lossy().contains(secret)
        || workspace.as_os_str().to_string_lossy().contains(secret)
        || args
            .iter()
            .any(|arg| arg.to_string_lossy().contains(secret))
    {
        return Err(not_called("MODEL_CREDENTIAL_ECHO"));
    }
    if cancel.load(Ordering::Acquire) {
        return Err(Failure {
            status: AttemptStatus::Cancelled,
            pid: None,
            code: "MODEL_CANCELLED",
        });
    }
    if !program.is_absolute()
        || crate::project::checked_file(program).is_err()
        || !workspace.is_absolute()
        || crate::project::check_components(workspace).is_err()
    {
        return Err(not_called("MODEL_NOT_CALLED"));
    }
    let _guard = crate::project::read_guard(program).map_err(|_| not_called("MODEL_NOT_CALLED"))?;
    let _workspace_guards =
        crate::sidecar::guard_chain(workspace).map_err(|_| not_called("MODEL_NOT_CALLED"))?;
    if std::env::var_os("SystemRoot").is_some_and(|value| value.to_string_lossy().contains(secret))
    {
        return Err(not_called("MODEL_CREDENTIAL_ECHO"));
    }
    let limits = crate::sidecar::SidecarLimits {
        max_input_bytes: request.policy.max_input_bytes,
        max_request_bytes: 65536,
        max_stdout_bytes: request.policy.max_response_bytes as usize,
        max_stderr_bytes: 65536,
        timeout: Duration::from_millis(request.policy.timeout_ms),
        cleanup_timeout: Duration::from_secs(2),
        ..crate::sidecar::SidecarLimits::default()
    };
    // Private runtime frame: magic, u32 LE version, credential length, original
    // wire length, then credential bytes and unchanged original request bytes.
    // Neither this frame nor its length/digest becomes a saved model fact.
    const MAGIC: &[u8] = b"NOETOME-CREDENTIAL-STDIN\0";
    let mut framed = Vec::with_capacity(MAGIC.len() + 12 + credential.bytes.len() + wire.len());
    framed.extend_from_slice(MAGIC);
    framed.extend_from_slice(&1u32.to_le_bytes());
    framed.extend_from_slice(&(credential.bytes.len() as u32).to_le_bytes());
    framed.extend_from_slice(&(wire.len() as u32).to_le_bytes());
    framed.extend_from_slice(&credential.bytes);
    framed.extend_from_slice(wire);
    let mut output = crate::sidecar_process::execute_command_sensitive(
        program, args, framed, workspace, &limits, cancel,
    )
    .map_err(|mut error| {
        error.diagnostics.fill(0);
        let status = match error.code {
            "SIDECAR_SPAWN_FAILED" if error.pid.is_none() => AttemptStatus::NotCalled,
            "SIDECAR_TIMEOUT" => AttemptStatus::Timeout,
            "SIDECAR_STDOUT_LIMIT" => AttemptStatus::ResponseLimit,
            "SIDECAR_CANCELLED" => AttemptStatus::Cancelled,
            "SIDECAR_CLEANUP_FAILED" => AttemptStatus::RemoteUnknown,
            _ => AttemptStatus::TransportFailed,
        };
        let code = match status {
            AttemptStatus::NotCalled => "MODEL_NOT_CALLED",
            AttemptStatus::Timeout => "MODEL_TIMEOUT",
            AttemptStatus::ResponseLimit => "MODEL_RESPONSE_LIMIT",
            AttemptStatus::Cancelled => "MODEL_CANCELLED",
            AttemptStatus::RemoteUnknown => "MODEL_REMOTE_UNKNOWN",
            _ => "MODEL_TRANSPORT_FAILED",
        };
        Failure {
            status,
            pid: error.pid,
            code,
        }
    })?;
    output.stderr.fill(0);
    let request_sha = crate::text::sha256(wire);
    let audited = if raw_contains_secret(&output.stdout, credential) {
        Err("MODEL_CREDENTIAL_ECHO")
    } else {
        crate::model_candidate::decode_auditable_outcome(&output.stdout, request, &request_sha)
            .and_then(|outcome| {
                if decoded_contains_secret(&outcome, credential) {
                    Err("MODEL_CREDENTIAL_ECHO")
                } else {
                    Ok(outcome)
                }
            })
    };
    match audited {
        Ok(outcome) => Ok((output, outcome)),
        Err(code) => {
            output.stdout.fill(0);
            Err(Failure {
                status: AttemptStatus::Delivered,
                pid: Some(output.pid),
                code,
            })
        }
    }
}
