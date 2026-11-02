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
    /// Validate arbitrary JSON artifacts without disclosing the credential.
    pub fn validate_no_echo(&self, bytes: &[u8]) -> Result<(), CredentialError> {
        if bytes.len() > 524288 {
            return Err(CredentialError {
                code: "MODEL_CREDENTIAL_JSON_INVALID",
            });
        }
        if raw_contains_secret(bytes, self) {
            return Err(CredentialError {
                code: "MODEL_CREDENTIAL_ECHO",
            });
        }
        let value = strict_json_value(bytes)?;
        scan_artifact_value(
            &value,
            std::str::from_utf8(&self.bytes).map_err(|_| CredentialError {
                code: "MODEL_CREDENTIAL_INVALID",
            })?,
            0,
        )
    }
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

pub(crate) fn strict_json_value(bytes: &[u8]) -> Result<serde_json::Value, CredentialError> {
    use serde::de::{MapAccess, SeqAccess, Visitor};
    struct Strict(serde_json::Value);
    impl<'de> serde::Deserialize<'de> for Strict {
        fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
            struct V;
            impl<'de> Visitor<'de> for V {
                type Value = Strict;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("strict JSON")
                }
                fn visit_unit<E: serde::de::Error>(self) -> Result<Strict, E> {
                    Ok(Strict(serde_json::Value::Null))
                }
                fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Strict, E> {
                    Ok(Strict(v.into()))
                }
                fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Strict, E> {
                    Ok(Strict(v.into()))
                }
                fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Strict, E> {
                    Ok(Strict(v.into()))
                }
                fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Strict, E> {
                    serde_json::Number::from_f64(v)
                        .map(|n| Strict(n.into()))
                        .ok_or_else(|| E::custom("MODEL_JSON_NONFINITE"))
                }
                fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Strict, E> {
                    Ok(Strict(v.into()))
                }
                fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Strict, E> {
                    Ok(Strict(v.into()))
                }
                fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                    let mut values = Vec::new();
                    while let Some(v) = a.next_element::<Strict>()? {
                        values.push(v.0);
                    }
                    Ok(Strict(values.into()))
                }
                fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Strict, A::Error> {
                    let mut values = serde_json::Map::new();
                    while let Some(key) = a.next_key::<String>()? {
                        if values.contains_key(&key) {
                            return Err(serde::de::Error::custom("MODEL_JSON_DUPLICATE_FIELD"));
                        }
                        values.insert(key, a.next_value::<Strict>()?.0);
                    }
                    Ok(Strict(values.into()))
                }
            }
            de.deserialize_any(V)
        }
    }
    serde_json::from_slice::<Strict>(bytes)
        .map(|v| v.0)
        .map_err(|_| CredentialError {
            code: "MODEL_CREDENTIAL_JSON_INVALID",
        })
}

fn scan_artifact_value(
    value: &serde_json::Value,
    secret: &str,
    depth: u32,
) -> Result<(), CredentialError> {
    match value {
        serde_json::Value::String(text) => scan_artifact_string(text, secret, depth),
        serde_json::Value::Array(items) => {
            for item in items {
                scan_artifact_value(item, secret, depth)?;
            }
            Ok(())
        }
        serde_json::Value::Object(items) => {
            for (key, item) in items {
                scan_artifact_string(key, secret, depth)?;
                scan_artifact_value(item, secret, depth)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
fn scan_artifact_string(text: &str, secret: &str, depth: u32) -> Result<(), CredentialError> {
    if text.contains(secret) {
        return Err(CredentialError {
            code: "MODEL_CREDENTIAL_ECHO",
        });
    }
    // Decode valid escape fragments even without an enclosing quoted token:
    // model content can have prose prefixes or end inside an unfinished value.
    // Invalid literal escapes remain text, so ordinary quotations stay usable.
    let decoded = decode_escape_fragments(text);
    if decoded == text {
        return Ok(());
    }
    if depth >= 16 {
        return Err(CredentialError {
            code: "MODEL_CREDENTIAL_JSON_INVALID",
        });
    }
    scan_artifact_string(&decoded, secret, depth + 1)
}

fn decode_escape_fragments(text: &str) -> String {
    fn hex(text: &str, start: usize) -> Option<u32> {
        let digits = text.get(start..start.checked_add(4)?)?;
        if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(digits, 16).ok()
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\\' {
            let simple = match bytes.get(cursor + 1) {
                Some(b'"') => Some('"'),
                Some(b'\\') => Some('\\'),
                Some(b'/') => Some('/'),
                Some(b'b') => Some('\u{8}'),
                Some(b'f') => Some('\u{c}'),
                Some(b'n') => Some('\n'),
                Some(b'r') => Some('\r'),
                Some(b't') => Some('\t'),
                _ => None,
            };
            if let Some(ch) = simple {
                out.push(ch);
                cursor += 2;
                continue;
            }
            if bytes.get(cursor + 1) == Some(&b'u') {
                if let Some(first) = hex(text, cursor + 2) {
                    let (code, consumed) = if (0xd800..=0xdbff).contains(&first)
                        && bytes.get(cursor + 6..cursor + 8) == Some(b"\\u")
                    {
                        match hex(text, cursor + 8).filter(|n| (0xdc00..=0xdfff).contains(n)) {
                            Some(second) => {
                                (0x10000 + ((first - 0xd800) << 10) + second - 0xdc00, 12)
                            }
                            None => (first, 6),
                        }
                    } else {
                        (first, 6)
                    };
                    if let Some(ch) = char::from_u32(code) {
                        out.push(ch);
                        cursor += consumed;
                        continue;
                    }
                }
            }
        }
        let ch = text[cursor..]
            .chars()
            .next()
            .expect("cursor at UTF8 boundary");
        out.push(ch);
        cursor += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod artifact_scan_tests {
    use super::*;
    const SECRET: &str = "SYNTHETIC_ARTIFACT_CREDENTIAL_20261102";
    #[test]
    fn ordinary_complete_json_passes_without_exposing_key() {
        let key = TransientCredential::new(SECRET.as_bytes()).unwrap();
        assert!(
            key.validate_no_echo(br#"{"status":"completed","value":[1,true,null]}"#)
                .is_ok()
        );
    }
    #[test]
    fn nested_closed_string_in_truncated_json_does_not_hide_unicode_echo() {
        let key = TransientCredential::new(SECRET.as_bytes()).unwrap();
        let escaped: String = SECRET
            .chars()
            .map(|c| format!("\\u{:04x}", c as u32))
            .collect();
        let nested = format!("{{\"statement\":\"{escaped}\"");
        let raw = serde_json::to_vec(&serde_json::json!({"output_text":nested})).unwrap();
        assert_eq!(
            key.validate_no_echo(&raw).unwrap_err().code,
            "MODEL_CREDENTIAL_ECHO"
        );
    }
    #[test]
    fn duplicate_keys_and_nonfinite_numbers_are_not_folded() {
        for raw in [br#"{"a":1,"a":2}"#.as_slice(), br#"{"a":1e999}"#] {
            assert!(strict_json_value(raw).is_err());
        }
    }
    #[test]
    fn escaped_fragments_in_unclosed_or_prefixed_text_are_rejected() {
        let key = TransientCredential::new(SECRET.as_bytes()).unwrap();
        let escaped: String = SECRET
            .chars()
            .map(|c| format!("\\u{:04x}", c as u32))
            .collect();
        let cases = [
            format!("\"{escaped}\""),
            format!("\"{escaped}"),
            format!("{{\"value\":\"{escaped}"),
            format!("```json\n{{\"value\":\"{escaped}\"}}\n```"),
            serde_json::to_string(&format!("\"{escaped}\"")).unwrap(),
            format!("{{\"value\":\"{escaped}\\u12"),
        ];
        let results: Vec<_> = cases
            .iter()
            .map(|text| {
                let raw = serde_json::to_vec(&serde_json::json!({"output_text":text})).unwrap();
                key.validate_no_echo(&raw).err().map(|error| error.code)
            })
            .collect();
        assert_eq!(results, vec![Some("MODEL_CREDENTIAL_ECHO"); cases.len()]);
    }
    #[test]
    fn ordinary_prose_with_quotes_and_invalid_literal_escapes_remains_usable() {
        let key = TransientCredential::new(SECRET.as_bytes()).unwrap();
        for text in [
            "{literal \"a\\q\"}\n",
            "```note\n\"ordinary quotation\"\n```",
            "a literal \\u12 and \\q",
        ] {
            let raw = serde_json::to_vec(&serde_json::json!({"output_text":text})).unwrap();
            assert!(key.validate_no_echo(&raw).is_ok());
        }
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
