//! Synthetic credential only; these checks do not read existing credentials.
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use noetome_cli::credential_handoff::{self, SensitiveAttempt, TransientCredential};
use noetome_cli::model_candidate::{self, ExtractionRequest};
use noetome_cli::model_provider::AttemptStatus;
use noetome_sidecar_platform::ProcessObserver;
use serde_json::{Value, json};
use uuid::Uuid;

const SECRET: &str = "SYNTHETIC_ONLY_CREDENTIAL_7349";
const MAGIC: &[u8] = b"NOETOME-CREDENTIAL-STDIN\0";
const SOURCE: &str = "The lamp uses five watts.\n";
const ID: &str = "11111111-1111-4111-8111-111111111111";

fn sandbox() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.artifacts/B04-T02-credential-tests")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&root).unwrap();
    root.canonicalize().unwrap()
}
fn request() -> ExtractionRequest {
    serde_json::from_value(json!({
        "schema_version":1,"protocol":"noetome-model-single-v1","request_id":ID,
        "provider":{"producer":"fake","provider_id":"offline-credential-fake","adapter_version":"1","requested_model":"synthetic-v1"},
        "source":{"project_id":ID,"source_id":ID,"version_id":ID,"revision":1,
            "raw_sha256":noetome_cli::text::sha256(SOURCE.as_bytes()),"normalized_sha256":noetome_cli::text::sha256(SOURCE.as_bytes()),
            "normalization_version":noetome_cli::text::NORMALIZATION_VERSION,"parser_version":noetome_cli::text::PARSER_VERSION,
            "units":[{"kind":"text_span","span_id":ID,"ordinal":0,"start_byte":0,"end_byte":SOURCE.len(),"text_sha256":noetome_cli::text::sha256(SOURCE.as_bytes()),"text":SOURCE}]},
        "policy":{"max_input_bytes":8192,"max_response_bytes":524288,"max_output_tokens":1024,"timeout_ms":30000,"max_calls":1,"retries":0,"search_enabled":false,"tools_enabled":false}
    })).unwrap()
}
fn invoke(
    root: &Path,
    mode: &str,
    request: &ExtractionRequest,
    cancel: &AtomicBool,
) -> SensitiveAttempt {
    invoke_with_secret(root, mode, request, cancel, SECRET.as_bytes())
}

fn invoke_with_secret(
    root: &Path,
    mode: &str,
    request: &ExtractionRequest,
    cancel: &AtomicBool,
    secret: &[u8],
) -> SensitiveAttempt {
    let wire = serde_json::to_vec_pretty(request).unwrap();
    let args = [OsString::from("--credential-fake-child"), mode.into()];
    let credential = TransientCredential::new(secret).unwrap();
    let observed_root = root.to_owned();
    let returned = Arc::new(AtomicBool::new(false));
    let finished = returned.clone();
    let observer = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(bytes) = fs::read(observed_root.join("child-facts.json")) {
                if let Ok(facts) = serde_json::from_slice::<Value>(&bytes) {
                    let pid = facts["pid"].as_u64().unwrap() as u32;
                    let observer =
                        ProcessObserver::open(pid).expect("independent parent captures live child");
                    let creation_time = observer.creation_time();
                    assert_ne!(creation_time, 0);
                    fs::write(
                        observed_root.join("parent-observed.ready"),
                        b"native observer attached",
                    )
                    .unwrap();
                    let exit = observer
                        .wait_exit(Duration::from_secs(5))
                        .unwrap()
                        .expect("observed child actually exits");
                    fs::write(
                        observed_root.join("observed-exit.json"),
                        serde_json::to_vec(
                            &json!({"pid":pid,"creation_time":creation_time,"exit_code":exit}),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                    return Some((pid, exit));
                }
            }
            if finished.load(Ordering::Acquire) {
                return None;
            }
            assert!(Instant::now() < deadline, "private child handshake absent");
            std::thread::sleep(Duration::from_millis(2));
        }
    });
    let attempt = credential_handoff::run(
        &std::env::current_exe().unwrap(),
        &args,
        request,
        &wire,
        root,
        &credential,
        cancel,
    );
    returned.store(true, Ordering::Release);
    let observed = observer.join().unwrap();
    assert_eq!(observed.map(|(pid, _)| pid), attempt.pid);
    println!("credential evidence mode={mode}: {}", root.display());
    attempt
}
fn suppressed(attempt: &SensitiveAttempt) {
    assert!(attempt.stdout_suppressed);
    assert!(attempt.stderr_suppressed);
    assert!(attempt.response.is_none());
    assert!(attempt.response_sha256.is_none());
    assert!(attempt.outcome.is_none());
    assert!(!format!("{attempt:?}").contains(SECRET));
}
fn clean_root(root: &Path) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        assert!(!entry.file_name().to_string_lossy().contains(SECRET));
        let bytes = fs::read(entry.path()).unwrap();
        assert!(
            !bytes
                .windows(SECRET.len())
                .any(|part| part == SECRET.as_bytes())
        );
        assert!(
            !String::from_utf8_lossy(&bytes)
                .contains(&noetome_cli::text::sha256(SECRET.as_bytes()))
        );
    }
}

pub fn valid_frame_is_one_request_and_only_systemroot_is_inherited() {
    let root = sandbox();
    let request = request();
    let attempt = invoke(&root, "legal", &request, &AtomicBool::new(false));
    assert_eq!(attempt.status, AttemptStatus::Delivered);
    assert!(attempt.pid.is_some());
    assert!(!attempt.stdout_suppressed);
    assert!(attempt.stderr_suppressed);
    assert!(attempt.failure.is_none());
    let response = attempt.response.as_ref().unwrap();
    assert_eq!(
        attempt.response_sha256.as_deref(),
        Some(noetome_cli::text::sha256(response).as_str())
    );
    let outcome = attempt.outcome.unwrap();
    let wire = serde_json::to_vec_pretty(&request).unwrap();
    assert_eq!(outcome.request_sha256, noetome_cli::text::sha256(&wire));
    let facts: Value =
        serde_json::from_slice(&fs::read(root.join("child-facts.json")).unwrap()).unwrap();
    assert_eq!(facts["frames"], 1);
    assert_eq!(facts["pid"].as_u64(), attempt.pid.map(u64::from));
    assert_eq!(facts["environment_names"], json!(["SystemRoot"]));
    assert_eq!(facts["argv_has_credential"], false);
    assert_eq!(facts["request_sha256"], outcome.request_sha256);
    let exited: Value =
        serde_json::from_slice(&fs::read(root.join("observed-exit.json")).unwrap()).unwrap();
    assert_eq!(exited["exit_code"], 0);
    clean_root(&root);
    for (mode, secret) in [
        ("one-byte-secret", vec![b'~']),
        ("full-secret", vec![b'x'; 4096]),
    ] {
        let root = sandbox();
        let attempt = invoke_with_secret(&root, mode, &request, &AtomicBool::new(false), &secret);
        assert_eq!(attempt.status, AttemptStatus::Delivered);
        assert!(!attempt.stdout_suppressed);
        assert!(attempt.response.is_some());
        for entry in fs::read_dir(&root).unwrap() {
            let bytes = fs::read(entry.unwrap().path()).unwrap();
            assert!(!bytes.windows(secret.len()).any(|part| part == secret));
        }
    }
}

pub fn credential_shapes_and_missing_explicit_reference_are_fixed_errors() {
    for invalid in [
        Vec::new(),
        vec![b'x'; 4097],
        vec![0],
        vec![b'\r'],
        vec![b'\n'],
        vec![0xff],
    ] {
        assert_eq!(
            TransientCredential::new(&invalid).err().unwrap().code,
            "MODEL_CREDENTIAL_INVALID"
        );
    }
    assert!(TransientCredential::new(&vec![b'x'; 4096]).is_ok());
    assert!(TransientCredential::new("合成".as_bytes()).is_ok());
    for reference in [
        "",
        "file:private",
        "env:",
        "env:1WRONG",
        "env:A=B",
        "env:A\nB",
        "env:坏",
        "ENV:NAME",
    ] {
        assert_eq!(
            TransientCredential::from_env_ref(reference)
                .err()
                .unwrap()
                .code,
            "MODEL_CREDENTIAL_REF_INVALID"
        );
    }
    let missing = format!(
        "env:NOETOME_NONEXISTENT_SYNTHETIC_{}",
        Uuid::new_v4().simple()
    );
    assert_eq!(
        TransientCredential::from_env_ref(&missing)
            .err()
            .unwrap()
            .code,
        "MODEL_CREDENTIAL_MISSING"
    );
}

pub fn invalid_request_and_secret_in_request_or_argv_start_zero_children() {
    for mode in [
        "policy",
        "mismatch",
        "wire-limit",
        "request-secret",
        "argv-secret",
        "cancelled",
        "relative-program",
    ] {
        let root = sandbox();
        let mut request = request();
        if mode == "policy" {
            request.policy.retries = 1;
        }
        if mode == "request-secret" {
            request.provider.requested_model = SECRET.into();
        }
        let mut wire = serde_json::to_vec_pretty(&request).unwrap();
        if mode == "mismatch" {
            wire = b"{}".to_vec();
        }
        if mode == "wire-limit" {
            wire.resize(65537, b' ');
        }
        let args = [
            OsString::from("--credential-fake-child"),
            if mode == "argv-secret" {
                SECRET.into()
            } else {
                "legal".into()
            },
        ];
        let program = if mode == "relative-program" {
            PathBuf::from("fake.exe")
        } else {
            std::env::current_exe().unwrap()
        };
        let credential = TransientCredential::new(SECRET.as_bytes()).unwrap();
        let attempt = credential_handoff::run(
            &program,
            &args,
            &request,
            &wire,
            &root,
            &credential,
            &AtomicBool::new(mode == "cancelled"),
        );
        assert!(attempt.pid.is_none(), "{mode}");
        suppressed(&attempt);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0, "{mode}");
    }
}

pub fn all_sensitive_echo_and_malformed_streams_are_suppressed() {
    for mode in [
        "echo",
        "unicode",
        "key",
        "unicode-key",
        "unknown",
        "duplicate",
        "truncated",
        "nonzero",
        "oversize",
        "stderr-oversize",
    ] {
        let root = sandbox();
        let attempt = invoke(&root, mode, &request(), &AtomicBool::new(false));
        assert!(attempt.pid.is_some(), "{mode}");
        suppressed(&attempt);
        assert!(attempt.failure.is_some());
        clean_root(&root);
    }
    let root = sandbox();
    let attempt = invoke(&root, "stderr-echo", &request(), &AtomicBool::new(false));
    assert_eq!(attempt.status, AttemptStatus::Delivered);
    assert!(!attempt.stdout_suppressed);
    assert!(attempt.stderr_suppressed);
    assert!(attempt.response.is_some());
    clean_root(&root);
}

pub fn clean_bad_candidate_preserves_auditable_usage() {
    for mode in ["bad-candidate", "token-over"] {
        let root = sandbox();
        let request = request();
        let attempt = invoke(&root, mode, &request, &AtomicBool::new(false));
        assert_eq!(attempt.status, AttemptStatus::Delivered);
        let outcome = attempt.outcome.unwrap();
        assert_eq!(outcome.usage.input_tokens, Some(11));
        assert!(
            model_candidate::decode_outcome(
                attempt.response.as_ref().unwrap(),
                &request,
                &outcome.request_sha256
            )
            .is_err()
        );
        assert!(attempt.response_sha256.is_some());
        clean_root(&root);
    }
}

pub fn native_timeout_and_cancel_have_observed_exit_and_no_secret_facts() {
    for mode in ["sleep", "blocked-stdin", "cancel"] {
        let root = sandbox();
        let mut request = request();
        request.policy.timeout_ms = if mode == "cancel" { 30000 } else { 300 };
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let running_root = root.clone();
        let cancellation_case = mode == "cancel";
        let mode = mode.to_owned();
        let started = Instant::now();
        let worker = std::thread::spawn(move || invoke(&running_root, &mode, &request, &signal));
        let deadline = Instant::now() + Duration::from_secs(5);
        let pid = loop {
            if let Ok(bytes) = fs::read(root.join("child-facts.json")) {
                if let Ok(facts) = serde_json::from_slice::<Value>(&bytes) {
                    break facts["pid"].as_u64().unwrap() as u32;
                }
            }
            assert!(Instant::now() < deadline, "native child handshake absent");
            std::thread::sleep(Duration::from_millis(5));
        };
        let observer = ProcessObserver::open(pid).expect("capture actual contained child handle");
        assert_ne!(observer.creation_time(), 0);
        if cancellation_case {
            while !root.join("cancel.ready").exists() {
                assert!(Instant::now() < deadline, "cancellation handshake absent");
                std::thread::sleep(Duration::from_millis(5));
            }
            cancel.store(true, Ordering::Release);
        }
        let attempt = worker.join().unwrap();
        assert!(matches!(
            attempt.status,
            AttemptStatus::Timeout | AttemptStatus::Cancelled
        ));
        assert_eq!(attempt.pid, Some(pid));
        assert_eq!(observer.wait_exit(Duration::from_secs(1)).unwrap(), Some(1));
        assert!(started.elapsed() < Duration::from_secs(4));
        suppressed(&attempt);
        clean_root(&root);
    }
}

pub fn fake_child(args: &[OsString]) -> i32 {
    fn execute(args: &[OsString]) -> Result<(), ()> {
        if args.len() != 1 {
            return Err(());
        }
        let mode = args[0].to_str().ok_or(())?;
        if mode == "blocked-stdin" {
            fs::write(
                "child-facts.json",
                serde_json::to_vec(&json!({"pid":std::process::id()})).unwrap(),
            )
            .map_err(|_| ())?;
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        let mut input = Vec::new();
        std::io::stdin()
            .take(65536 + 4096 + 64 + 1)
            .read_to_end(&mut input)
            .map_err(|_| ())?;
        if !input.starts_with(MAGIC) || input.len() < MAGIC.len() + 12 {
            return Err(());
        }
        let mut offset = MAGIC.len();
        let mut read_u32 = || {
            let value = u32::from_le_bytes(input[offset..offset + 4].try_into().unwrap());
            offset += 4;
            value
        };
        let version = read_u32();
        let credential_len = read_u32() as usize;
        let request_len = read_u32() as usize;
        if version != 1
            || credential_len == 0
            || credential_len > 4096
            || request_len > 65536
            || input.len() != offset + credential_len + request_len
        {
            return Err(());
        }
        let supplied = &input[offset..offset + credential_len];
        let matches = match mode {
            "one-byte-secret" => supplied == b"~",
            "full-secret" => supplied.len() == 4096 && supplied.iter().all(|byte| *byte == b'x'),
            _ => supplied == SECRET.as_bytes(),
        };
        if !matches {
            return Err(());
        }
        let wire = &input[offset + credential_len..];
        let request: ExtractionRequest = serde_json::from_slice(wire).map_err(|_| ())?;
        let names: Vec<_> = std::env::vars_os()
            .map(|(key, _)| key.to_string_lossy().into_owned())
            .collect();
        let argv_has_credential =
            std::env::args_os().any(|arg| arg.to_string_lossy().contains(SECRET));
        let facts = json!({"pid":std::process::id(),"frames":1,"environment_names":names,"argv_has_credential":argv_has_credential,"request_sha256":noetome_cli::text::sha256(wire)});
        fs::write("child-facts.json", serde_json::to_vec(&facts).unwrap()).map_err(|_| ())?;
        let handshake_deadline = Instant::now() + Duration::from_secs(5);
        while !Path::new("parent-observed.ready").exists() {
            if Instant::now() >= handshake_deadline {
                return Err(());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        if mode == "cancel" {
            fs::write("cancel.ready", b"synthetic cancellation handshake").map_err(|_| ())?;
        }
        if mode == "sleep" || mode == "cancel" {
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        if mode == "nonzero" {
            std::io::stderr()
                .write_all(SECRET.as_bytes())
                .map_err(|_| ())?;
            std::process::exit(7);
        }
        if mode == "oversize" {
            std::io::stdout()
                .write_all(&vec![b'x'; request.policy.max_response_bytes as usize + 1])
                .map_err(|_| ())?;
            return Ok(());
        }
        if mode == "stderr-oversize" {
            std::io::stderr()
                .write_all(&vec![b'x'; 65537])
                .map_err(|_| ())?;
            return Ok(());
        }
        if mode == "truncated" {
            std::io::stdout()
                .write_all(b"{\"schema_version\":1")
                .map_err(|_| ())?;
            return Ok(());
        }
        if mode == "stderr-echo" {
            std::io::stderr()
                .write_all(SECRET.as_bytes())
                .map_err(|_| ())?;
        }
        let mut outcome = json!({"schema_version":1,"protocol":request.protocol,"request_id":request.request_id,
            "request_sha256":noetome_cli::text::sha256(wire),"provider":request.provider,"actual_model":"synthetic-v1","status":"complete",
            "candidates":[{"ordinal":0,"statement":"The lamp uses five watts.","qualifiers":[],"evidence":[{"span_id":ID,"excerpt_start_byte":0,"excerpt_end_byte":SOURCE.len(),"excerpt_sha256":noetome_cli::text::sha256(SOURCE.as_bytes())}]}],
            "usage":{"provenance":"simulated","input_tokens":11,"output_tokens":13,"cost":{"status":"unknown","amount_microunits":null,"currency":null,"basis":"not_available"}},"failure":null});
        match mode {
            "echo" | "unicode" => outcome["candidates"][0]["statement"] = json!(SECRET),
            "key" | "unicode-key" => outcome[SECRET] = json!(true),
            "unknown" => outcome["unknown"] = json!(SECRET),
            "bad-candidate" => {
                outcome["candidates"][0]["evidence"][0]["excerpt_sha256"] = json!("0".repeat(64))
            }
            "token-over" => outcome["usage"]["output_tokens"] = json!(1025),
            "legal" | "stderr-echo" | "duplicate" | "one-byte-secret" | "full-secret" => {}
            _ => return Err(()),
        }
        let mut encoded = serde_json::to_string(&outcome).unwrap();
        if mode == "unicode" || mode == "unicode-key" {
            let escaped: String = SECRET
                .chars()
                .map(|character| format!("\\u{:04x}", character as u32))
                .collect();
            encoded = encoded.replace(SECRET, &escaped);
        }
        if mode == "duplicate" {
            encoded = format!("{{\"actual_model\":\"{SECRET}\",{}", &encoded[1..]);
        }
        std::io::stdout()
            .write_all(encoded.as_bytes())
            .map_err(|_| ())?;
        input.fill(0);
        Ok(())
    }
    match execute(args) {
        Ok(()) => 0,
        Err(()) => 2,
    }
}
