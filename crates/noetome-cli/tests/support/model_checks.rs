//! Independent fixture checks and a narrow private process adapter.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use noetome_cli::knowledge_extract::{self, ExtractionOptions};
use noetome_cli::model_candidate::{ExtractionRequest, Policy, ProviderIdentity, Usage};
use noetome_cli::model_provider::{
    AttemptStatus, Budget, Invocation, Provider, SettlementLedger, SettlementResult,
};
use noetome_sidecar_platform::ProcessObserver;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const CREDENTIAL_SENTINEL: &str = "NOETOME_B04_CREDENTIAL_SENTINEL_7cf8";
const SOURCE_SHA: &str = "5aa51f5816ec39ac4eefd7a8299def081d33a2277a8fdddb66243b4a741c7591";
const EXPECTED_SHA: &str = "f86f6948a69e15dc8afdacbab0fc72435754067ec88c61b7a2e1b248be14cb04";
const TEMPLATE_SHA: &str = "c5b8295cdce3664d965933139dbb0509a7cd3f9edf964e4cd2ee6426fc1b9097";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Template {
    schema_version: u32,
    producer: String,
    statements: Vec<TemplateStatement>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateStatement {
    statement: String,
    qualifiers: Vec<String>,
    support_quote: String,
}

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
pub fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

pub struct FakeProvider {
    identity: ProviderIdentity,
    mode: String,
    template: PathBuf,
    pub calls: AtomicUsize,
}
impl FakeProvider {
    pub fn new(mode: &str, template: &Path) -> Self {
        Self {
            identity: serde_json::from_value(json!({"producer":"fake","provider_id":"offline-fake","adapter_version":"model-process-test-v1","requested_model":"reviewed-candidate-v1"})).unwrap(),
            mode: mode.into(), template: template.into(), calls: AtomicUsize::new(0),
        }
    }
}
impl Provider for FakeProvider {
    fn identity(&self) -> &ProviderIdentity {
        &self.identity
    }
    fn invocation(&self) -> Invocation {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Invocation {
            program: std::env::current_exe().unwrap(),
            args: vec![
                "--fake-child".into(),
                self.mode.clone().into(),
                self.template.as_os_str().to_owned(),
            ],
        }
    }
}

pub struct Sandbox {
    pub root: PathBuf,
    pub project: PathBuf,
    pub selected: PathBuf,
    pub template: PathBuf,
    pub imported: Value,
    project_before: BTreeMap<String, Vec<u8>>,
    sentinel_before: Vec<u8>,
}
impl Sandbox {
    pub fn new(name: &str) -> Self {
        let root = repo()
            .join(".artifacts/B04-T01-tests")
            .join(format!("{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let selected = root.join("选择的虚构 source.md");
        fs::copy(
            repo().join("tests/fixtures/model-candidate-v1/source.md"),
            &selected,
        )
        .unwrap();
        let template = root.join("response-template.json");
        fs::copy(
            repo().join("tests/fixtures/model-response-v1/response-template.json"),
            &template,
        )
        .unwrap();
        let sentinel_before = "外部人工哨兵，不得改写。\n".as_bytes().to_vec();
        fs::write(root.join("外部哨兵.md"), &sentinel_before).unwrap();
        let project = root.join("project with spaces");
        noetome_cli::project::initialize(&project).unwrap();
        let output = noetome_cli::source::import(&project, &selected, None).unwrap();
        assert_eq!(output.exit_code, 0);
        let imported: Value = serde_json::from_str(&output.json).unwrap();
        assert_eq!(imported["status"], "ready");
        write_json(&root.join("source-import.json"), &imported);
        let project_before = snapshot(&project);
        println!("evidence: {}", root.display());
        Self {
            root,
            project,
            selected,
            template,
            imported,
            project_before,
            sentinel_before,
        }
    }
    pub fn span_ids(&self) -> Vec<String> {
        self.imported["spans"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|span| span["parse_status"] == "parsed" && span["kind"] != "formatting")
            .map(|span| span["span_id"].as_str().unwrap().to_owned())
            .collect()
    }
    pub fn request(&self, provider: &FakeProvider) -> ExtractionRequest {
        let normalized_path = self.imported["version"]["normalized_snapshot"]
            .as_str()
            .unwrap();
        let normalized = fs::read(self.project.join(normalized_path)).unwrap();
        let units: Vec<_> = self.imported["spans"].as_array().unwrap().iter()
            .filter(|span| span["parse_status"] == "parsed" && span["kind"] != "formatting")
            .map(|span| {
                let start = span["start_byte"].as_u64().unwrap() as usize;
                let end = span["end_byte"].as_u64().unwrap() as usize;
                json!({"kind":"text_span","span_id":span["span_id"],"ordinal":span["ordinal"],"start_byte":start,"end_byte":end,"text_sha256":span["text_sha256"],"text":std::str::from_utf8(&normalized[start..end]).unwrap()})
            }).collect();
        serde_json::from_value(json!({"schema_version":1,"protocol":"noetome-model-single-v1","request_id":Uuid::new_v4().to_string(),"provider":provider.identity(),
            "source":{"project_id":self.imported["project_id"],"source_id":self.imported["source_id"],"version_id":self.imported["version_id"],"revision":self.imported["version"]["revision"],"raw_sha256":self.imported["version"]["content_sha256"],"normalized_sha256":self.imported["version"]["normalized_sha256"],"normalization_version":self.imported["version"]["normalization_version"],"parser_version":self.imported["spans"][0]["parser_version"],"units":units},"policy":Policy::default()})).unwrap()
    }
    pub fn options(&self) -> ExtractionOptions {
        ExtractionOptions {
            version_id: self.imported["version_id"].as_str().unwrap().into(),
            span_ids: self.span_ids(),
            policy: Policy::default(),
            budget: Budget {
                available_calls: 1,
                available_output_tokens: 1024,
            },
            credential_sentinels: vec![CREDENTIAL_SENTINEL.into()],
        }
    }
    pub fn extract(&self, name: &str, provider: &FakeProvider) -> (PathBuf, Value) {
        let output = self.root.join(name);
        let report = knowledge_extract::extract(
            &self.project,
            &output,
            provider,
            &self.options(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let report = serde_json::to_value(report).unwrap();
        assert_eq!(report["real_api_calls"], 0);
        assert_eq!(report["provider_invocations"], 1);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert_eq!(read_json(&output.join("report.json")), report);
        let reloaded = knowledge_extract::inspect(&self.project, &output).unwrap();
        assert_eq!(serde_json::to_value(reloaded).unwrap(), report);
        self.protect();
        (output, report)
    }
    fn cli_inspect(&self, output: &Path) -> Output {
        let arguments = [
            OsString::from("knowledge"),
            OsString::from("inspect"),
            self.project.as_os_str().to_owned(),
            output.as_os_str().to_owned(),
        ];
        let result = Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(&arguments)
            .current_dir(&self.root)
            .output()
            .unwrap();
        write_json(
            &self.root.join("actual-cli-inspect.json"),
            &json!({"argv":arguments.iter().map(|arg|arg.to_string_lossy().into_owned()).collect::<Vec<_>>(),"executable":env!("CARGO_BIN_EXE_noetome"),"exit_code":result.status.code(),"stdout":String::from_utf8_lossy(&result.stdout),"stderr":String::from_utf8_lossy(&result.stderr)}),
        );
        result
    }
    pub fn protect(&self) {
        assert_eq!(
            snapshot(&self.project),
            self.project_before,
            "extraction mutated registered source project"
        );
        assert_eq!(
            fs::read(self.root.join("外部哨兵.md")).unwrap(),
            self.sentinel_before
        );
        assert_eq!(
            sha(&fs::read(repo().join("tests/fixtures/model-candidate-v1/source.md")).unwrap()),
            SOURCE_SHA
        );
        assert_eq!(
            sha(&fs::read(repo().join("tests/fixtures/model-candidate-v1/expected.json")).unwrap()),
            EXPECTED_SHA
        );
        if self.selected.exists() {
            assert_eq!(
                sha(&fs::read(&self.selected).unwrap()),
                SOURCE_SHA,
                "selected original changed"
            );
        }
        assert_eq!(
            sha(&fs::read(&self.template).unwrap()),
            TEMPLATE_SHA,
            "reviewed response template changed"
        );
    }
}
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(base: &Path, path: &Path, result: &mut BTreeMap<String, Vec<u8>>) {
        let mut entries: Vec<_> = fs::read_dir(path).unwrap().map(Result::unwrap).collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            assert!(
                !fs::symlink_metadata(&path)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            if path.is_dir() {
                visit(base, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(base)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

pub fn frozen_author_and_response_template_are_independently_bound() {
    let author = repo().join("tests/fixtures/model-candidate-v1");
    let source = fs::read(author.join("source.md")).unwrap();
    let expected = fs::read(author.join("expected.json")).unwrap();
    assert_eq!(sha(&source), SOURCE_SHA);
    assert_eq!(sha(&expected), EXPECTED_SHA);
    assert_eq!(source.len(), 1208);
    let source = std::str::from_utf8(&source).unwrap();
    let expected: Value = serde_json::from_slice(&expected).unwrap();
    assert_eq!(expected["required_atom_count"], 12);
    let required = expected["required_atoms"].as_array().unwrap();
    assert_eq!(required.len(), 12);
    let keys: BTreeSet<_> = required
        .iter()
        .map(|atom| atom["atom_key"].as_str().unwrap())
        .collect();
    assert_eq!(keys.len(), 12);
    let template =
        fs::read(repo().join("tests/fixtures/model-response-v1/response-template.json")).unwrap();
    assert_eq!(sha(&template), TEMPLATE_SHA);
    let template: Template = serde_json::from_slice(&template).unwrap();
    assert_eq!(template.schema_version, 1);
    assert_eq!(template.producer, "offline_fake_template");
    assert_eq!(template.statements.len(), 12);
    for (atom, response) in required.iter().zip(&template.statements) {
        assert_eq!(
            source
                .matches(atom["support_quote"].as_str().unwrap())
                .count(),
            1
        );
        assert_eq!(source.matches(&response.support_quote).count(), 1);
        assert_eq!(response.statement, atom["content"].as_str().unwrap());
        assert_eq!(
            serde_json::to_value(&response.qualifiers).unwrap(),
            atom["qualifiers"]
        );
    }
}

pub fn real_process_delivers_twelve_source_bound_candidates() {
    let sandbox = Sandbox::new("01-actual-provider-red");
    let provider = FakeProvider::new("legal", &sandbox.template);
    let request = sandbox.request(&provider);
    let bytes = serde_json::to_vec(&request).unwrap();
    let workspace = sandbox.root.join("private invocation");
    fs::create_dir(&workspace).unwrap();
    let attempt = noetome_cli::model_provider::run(
        &provider,
        &request,
        &bytes,
        &workspace,
        &AtomicBool::new(false),
    );
    assert_eq!(
        attempt.status,
        AttemptStatus::Delivered,
        "real process provider did not deliver the reviewed response"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert!(attempt.pid.is_some());
    let response = attempt.response.as_ref().unwrap();
    assert_eq!(
        attempt.response_sha256.as_deref(),
        Some(sha(response).as_str())
    );
    let wire: Value = serde_json::from_slice(response).unwrap();
    assert_eq!(wire["request_sha256"], sha(&bytes));
    assert_eq!(wire["candidates"].as_array().unwrap().len(), 12);
    assert_eq!(wire["usage"]["provenance"], "simulated");
    assert!(wire["usage"]["cost"]["amount_microunits"].is_null());
    write_json(&sandbox.root.join("actual-provider.json"), &wire);
    sandbox.protect();
}

pub fn saved_candidate_survives_original_move_and_real_cli_inspect() {
    let sandbox = Sandbox::new("02-save-reload-cli");
    let provider = FakeProvider::new("legal", &sandbox.template);
    let (output, report) = sandbox.extract("new candidate output", &provider);
    assert_eq!(report["result"], "candidate");
    assert_eq!(report["verification"], "unverified");
    assert!(report["failure"].is_null());
    assert_eq!(report["usage"]["provenance"], "simulated");
    assert_eq!(report["usage"]["input_tokens"], 160);
    assert_eq!(report["usage"]["output_tokens"], 400);
    assert_eq!(report["usage"]["cost"]["status"], "unknown");
    assert!(report["usage"]["cost"]["amount_microunits"].is_null());
    assert!(report["usage"]["cost"]["currency"].is_null());
    assert_eq!(report["settlement"]["request_id"], report["request_id"]);
    let ir = read_json(&output.join("candidate.json"));
    assert_eq!(ir["producer"], "fake");
    assert_eq!(ir["verification"], "unverified");
    let atoms = ir["atoms"].as_array().unwrap();
    assert_eq!(atoms.len(), 12);
    let expected = read_json(&repo().join("tests/fixtures/model-candidate-v1/expected.json"));
    for (actual, author) in atoms
        .iter()
        .zip(expected["required_atoms"].as_array().unwrap())
    {
        assert_eq!(actual["verification"], "unverified");
        assert_eq!(actual["proposal"]["statement"], author["content"]);
        assert_eq!(actual["proposal"]["qualifiers"], author["qualifiers"]);
        let evidence = &actual["proposal"]["evidence"][0];
        let unit = report["source"]["units"]
            .as_array()
            .unwrap()
            .iter()
            .find(|unit| unit["span_id"] == evidence["span_id"])
            .unwrap();
        let text = unit["text"].as_str().unwrap();
        let excerpt = &text[evidence["excerpt_start_byte"].as_u64().unwrap() as usize
            ..evidence["excerpt_end_byte"].as_u64().unwrap() as usize];
        assert_eq!(excerpt, author["support_quote"].as_str().unwrap());
        assert_eq!(sha(excerpt.as_bytes()), evidence["excerpt_sha256"]);
    }
    let before_output = snapshot(&output);
    let moved = sandbox.root.join("原文件已移走.md");
    fs::rename(&sandbox.selected, &moved).unwrap();
    for _ in 0..2 {
        assert_eq!(
            serde_json::to_value(knowledge_extract::inspect(&sandbox.project, &output).unwrap())
                .unwrap(),
            report
        );
    }
    let cli = sandbox.cli_inspect(&output);
    assert!(
        cli.status.success(),
        "actual inspect CLI failed: {}",
        String::from_utf8_lossy(&cli.stderr)
    );
    assert!(cli.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&cli.stdout).unwrap(),
        report
    );
    assert_eq!(snapshot(&output), before_output);
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "reload invoked Provider again"
    );
    assert_eq!(sha(&fs::read(&moved).unwrap()), SOURCE_SHA);
    assert!(!sandbox.selected.exists());
    sandbox.protect();
}

pub fn budget_and_invalid_source_selection_start_zero_processes() {
    let mut sandbox = Sandbox::new("03-zero-calls");
    for mode in [
        "calls",
        "tokens",
        "version",
        "span",
        "duplicate-span",
        "no-span",
        "formatting-span",
        "retry",
        "search",
        "tools",
    ] {
        let provider = FakeProvider::new("legal", &sandbox.template);
        let mut options = sandbox.options();
        match mode {
            "calls" => options.budget.available_calls = 0,
            "tokens" => options.budget.available_output_tokens = 1023,
            "version" => options.version_id = Uuid::new_v4().to_string(),
            "span" => options.span_ids = vec![Uuid::new_v4().to_string()],
            "duplicate-span" => options.span_ids.push(options.span_ids[0].clone()),
            "no-span" => options.span_ids.clear(),
            "formatting-span" => {
                options.span_ids = vec![
                    sandbox.imported["spans"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|span| span["kind"] == "formatting")
                        .unwrap()["span_id"]
                        .as_str()
                        .unwrap()
                        .into(),
                ]
            }
            "retry" => options.policy.retries = 1,
            "search" => options.policy.search_enabled = true,
            "tools" => options.policy.tools_enabled = true,
            _ => unreachable!(),
        }
        let output = sandbox.root.join(format!("refused-{mode}"));
        let result = knowledge_extract::extract(
            &sandbox.project,
            &output,
            &provider,
            &options,
            &AtomicBool::new(false),
        );
        assert!(result.is_err(), "preflight accepted {mode}");
        assert_eq!(
            provider.calls.load(Ordering::SeqCst),
            0,
            "preflight {mode} invoked Provider"
        );
        assert!(!output.exists(), "preflight {mode} created output root");
        sandbox.protect();
    }
    let long_source = sandbox.root.join("large permitted source.md");
    for input_bytes in [8192, 8193] {
        fs::write(&long_source, "a".repeat(input_bytes)).unwrap();
        let imported = noetome_cli::source::import(&sandbox.project, &long_source, None).unwrap();
        assert_eq!(imported.exit_code, 0);
        sandbox.imported = serde_json::from_str(&imported.json).unwrap();
        sandbox.project_before = snapshot(&sandbox.project);
        let provider = FakeProvider::new("empty", &sandbox.template);
        let output = sandbox.root.join(format!("input-{input_bytes}"));
        let result = knowledge_extract::extract(
            &sandbox.project,
            &output,
            &provider,
            &sandbox.options(),
            &AtomicBool::new(false),
        );
        if input_bytes == 8192 {
            let report = result.unwrap();
            assert_eq!(
                report.result, "rejected",
                "empty response cannot create success IR"
            );
            assert_eq!(
                provider.calls.load(Ordering::SeqCst),
                1,
                "exact input boundary was refused"
            );
            assert!(output.is_dir());
        } else {
            assert!(result.is_err());
            assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
            assert!(!output.exists());
        }
        sandbox.protect();
    }
}

pub fn classified_provider_failures_save_facts_without_candidate() {
    let sandbox = Sandbox::new("04-classified-failures");
    for (mode, code) in [
        ("rate_limit", "MODEL_RATE_LIMIT"),
        ("auth", "MODEL_AUTHENTICATION"),
        ("capability", "MODEL_CAPABILITY_INSUFFICIENT"),
        ("remote_unknown", "MODEL_REMOTE_UNKNOWN"),
        ("failed_timeout", "MODEL_TIMEOUT"),
    ] {
        let provider = FakeProvider::new(mode, &sandbox.template);
        let (output, report) = sandbox.extract(&format!("classified-{mode}"), &provider);
        assert_eq!(report["result"], "provider_failed", "{mode}");
        assert_eq!(report["failure"]["code"], code);
        assert_eq!(report["response_saved"], true);
        assert!(output.join("response.json").is_file());
        assert!(!output.join("candidate.json").exists());
        assert!(report["candidate_id"].is_null());
        assert_eq!(report["usage"]["provenance"], "simulated");
        assert_eq!(report["usage"]["cost"]["status"], "unknown");
        assert!(report["usage"]["cost"]["amount_microunits"].is_null());
    }
}

pub fn malformed_real_responses_and_credentials_never_create_candidate() {
    let sandbox = Sandbox::new("05-malformed-wire");
    for mode in [
        "empty",
        "truncated",
        "oversize",
        "duplicate",
        "unknown",
        "invalidref",
        "token_over",
        "wrong_version",
        "wrong_identity",
        "wrong_sha",
        "wrong_model",
        "wrong_producer",
        "credential_echo",
        "credential_escaped",
    ] {
        let provider = FakeProvider::new(mode, &sandbox.template);
        let (output, report) = sandbox.extract(&format!("rejected-{mode}"), &provider);
        assert_eq!(
            report["result"], "rejected",
            "real process {mode} was accepted"
        );
        assert!(
            report["failure"]["code"]
                .as_str()
                .unwrap()
                .starts_with("MODEL_")
        );
        assert!(!output.join("candidate.json").exists());
        assert!(report["candidate_id"].is_null());
        assert_eq!(report["verification"], "not_applicable");
        if mode.starts_with("credential_") {
            assert_eq!(report["failure"]["code"], "MODEL_CREDENTIAL_ECHO");
            assert_eq!(report["response_saved"], false);
            assert!(!output.join("response.json").exists());
            assert!(report["settlement"].is_null());
        }
        for bytes in snapshot(&output).values() {
            assert!(
                !bytes
                    .windows(CREDENTIAL_SENTINEL.len())
                    .any(|part| part == CREDENTIAL_SENTINEL.as_bytes()),
                "credential sentinel saved in {mode}"
            );
        }
    }
}

pub fn native_timeout_and_cancellation_wait_for_observed_exit() {
    assert!(cfg!(windows), "native observer acceptance requires Windows");
    let sandbox = Sandbox::new("06-native-exit");
    for (mode, cancel_after_handshake) in [("sleep", false), ("sleep_cancel", true)] {
        let provider = Arc::new(FakeProvider::new(mode, &sandbox.template));
        let cancel = Arc::new(AtomicBool::new(false));
        let mut options = sandbox.options();
        options.policy.timeout_ms = if cancel_after_handshake { 30000 } else { 300 };
        let output = sandbox.root.join(format!("native-{mode}-output"));
        let project = sandbox.project.clone();
        let worker_provider = Arc::clone(&provider);
        let worker_cancel = Arc::clone(&cancel);
        let worker_output = output.clone();
        let started = Instant::now();
        let worker = std::thread::spawn(move || {
            knowledge_extract::extract(
                &project,
                &worker_output,
                worker_provider.as_ref(),
                &options,
                worker_cancel.as_ref(),
            )
        });
        let probe = sandbox
            .template
            .with_file_name(format!("native-{mode}.json"));
        let deadline = Instant::now() + Duration::from_secs(3);
        let pid = loop {
            if let Ok(bytes) = fs::read(&probe) {
                if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                    if let Some(pid) = value["pid"].as_u64() {
                        break pid as u32;
                    }
                }
            }
            assert!(
                Instant::now() < deadline,
                "real child did not provide native handshake"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        let observer = ProcessObserver::open(pid).expect("capture actual live child native handle");
        let creation_time = observer.creation_time();
        assert_ne!(creation_time, 0);
        assert_eq!(
            observer.wait_exit(Duration::ZERO).unwrap(),
            None,
            "child already exited before native observation"
        );
        if cancel_after_handshake {
            cancel.store(true, Ordering::SeqCst);
        }
        let report = worker.join().unwrap().unwrap();
        let report = serde_json::to_value(report).unwrap();
        assert_eq!(report["result"], "rejected");
        assert_eq!(
            report["failure"]["code"],
            if cancel_after_handshake {
                "MODEL_CANCELLED"
            } else {
                "MODEL_TIMEOUT"
            }
        );
        assert_eq!(report["attempt"]["pid"], pid);
        assert!(
            observer
                .wait_exit(Duration::from_secs(2))
                .unwrap()
                .is_some(),
            "observed native child still alive after return"
        );
        drop(observer);
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "timeout/cancel exceeded bounded cleanup"
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert!(!output.join("candidate.json").exists());
        write_json(
            &sandbox.root.join(format!("observed-{mode}.json")),
            &json!({"pid":pid,"native_creation_time":creation_time,"native_exit_observed":true,"report":report}),
        );
        assert_eq!(
            serde_json::to_value(knowledge_extract::inspect(&sandbox.project, &output).unwrap())
                .unwrap(),
            report
        );
        sandbox.protect();
    }
}

pub fn saved_facts_reject_unknown_inventory_and_synchronized_ir_tamper() {
    let sandbox = Sandbox::new("07-saved-tamper");
    for mode in [
        "unknown-file",
        "missing-report-field",
        "unknown-report-field",
        "duplicate-report-field",
        "verification",
        "atom-verification",
        "wrong-evidence",
        "response-fingerprint",
    ] {
        let provider = FakeProvider::new("legal", &sandbox.template);
        let (output, _) = sandbox.extract(&format!("saved-{mode}"), &provider);
        match mode {
            "unknown-file" => {
                fs::write(output.join("unknown human.md"), "必须保留的未知内容").unwrap()
            }
            "missing-report-field" | "unknown-report-field" => {
                let mut report = read_json(&output.join("report.json"));
                if mode == "missing-report-field" {
                    report.as_object_mut().unwrap().remove("usage");
                } else {
                    report["accepted"] = json!(true);
                }
                write_json(&output.join("report.json"), &report);
            }
            "duplicate-report-field" => {
                let bytes = fs::read_to_string(output.join("report.json")).unwrap();
                fs::write(
                    output.join("report.json"),
                    format!("{{\"schema_version\":1,{}", &bytes[1..]),
                )
                .unwrap();
            }
            "verification" | "atom-verification" | "wrong-evidence" => {
                let mut candidate = read_json(&output.join("candidate.json"));
                match mode {
                    "verification" => candidate["verification"] = json!("fixture_reviewed"),
                    "atom-verification" => {
                        candidate["atoms"][0]["verification"] = json!("fixture_reviewed")
                    }
                    _ => {
                        candidate["atoms"][0]["proposal"]["evidence"][0]["excerpt_end_byte"] =
                            json!(1)
                    }
                }
                write_json(&output.join("candidate.json"), &candidate);
                let mut report = read_json(&output.join("report.json"));
                report["candidate_sha256"] =
                    json!(sha(&fs::read(output.join("candidate.json")).unwrap()));
                write_json(&output.join("report.json"), &report);
            }
            "response-fingerprint" => {
                let mut response = read_json(&output.join("response.json"));
                response["request_sha256"] = json!("0".repeat(64));
                write_json(&output.join("response.json"), &response);
            }
            _ => unreachable!(),
        }
        let before = snapshot(&output);
        assert!(
            knowledge_extract::inspect(&sandbox.project, &output).is_err(),
            "read-only inspector accepted {mode}"
        );
        assert_eq!(snapshot(&output), before, "inspector repaired {mode}");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        sandbox.protect();
    }
}

pub fn existing_and_windows_protected_paths_never_start_process() {
    let sandbox = Sandbox::new("08-path-protection");
    let existing = sandbox.root.join("existing-human-output");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("human.md"), "人工正文必须保留").unwrap();
    let before = snapshot(&existing);
    let overlap = sandbox.project.join("nested-output");
    let mut outputs = vec![existing.clone(), overlap];
    if cfg!(windows) {
        for name in ["CON", "NUL.json", "trailing.", "trailing "] {
            outputs.push(sandbox.root.join(name));
        }
    }
    for output in outputs {
        let provider = FakeProvider::new("legal", &sandbox.template);
        assert!(
            knowledge_extract::extract(
                &sandbox.project,
                &output,
                &provider,
                &sandbox.options(),
                &AtomicBool::new(false)
            )
            .is_err(),
            "unsafe output accepted {}",
            output.display()
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        sandbox.protect();
    }
    assert_eq!(snapshot(&existing), before);
    if cfg!(windows) {
        let outside = sandbox.root.join("outside-junction-target");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("human.md"), "junction外部哨兵").unwrap();
        let outside_before = snapshot(&outside);
        let link = sandbox.root.join("junction-output-parent");
        let result = Command::new("cmd.exe")
            .args([
                OsString::from("/c"),
                OsString::from("mklink"),
                OsString::from("/J"),
                link.as_os_str().to_owned(),
                outside.as_os_str().to_owned(),
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "junction setup failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let provider = FakeProvider::new("legal", &sandbox.template);
        assert!(
            knowledge_extract::extract(
                &sandbox.project,
                &link.join("new-output"),
                &provider,
                &sandbox.options(),
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        assert_eq!(snapshot(&outside), outside_before);
        sandbox.protect();
    }
}

pub fn windows_saved_hardlink_and_sharelock_are_rejected() {
    assert!(
        cfg!(windows),
        "Windows file protection acceptance requires Windows"
    );
    let sandbox = Sandbox::new("09-native-file-protection");
    let provider = FakeProvider::new("legal", &sandbox.template);
    let (output, _) = sandbox.extract("saved-hardlink", &provider);
    let outside = sandbox.root.join("outside preserved response.json");
    fs::hard_link(output.join("response.json"), &outside).unwrap();
    let outside_before = fs::read(&outside).unwrap();
    assert!(
        knowledge_extract::inspect(&sandbox.project, &output).is_err(),
        "saved response accepted external hardlink"
    );
    assert_eq!(fs::read(&outside).unwrap(), outside_before);
    sandbox.protect();
    let provider = FakeProvider::new("legal", &sandbox.template);
    let (output, _) = sandbox.extract("saved-sharelock", &provider);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(output.join("report.json"))
            .unwrap();
        assert!(
            knowledge_extract::inspect(&sandbox.project, &output).is_err(),
            "inspector accepted exclusive Windows share-lock"
        );
        drop(lock);
        assert!(knowledge_extract::inspect(&sandbox.project, &output).is_ok());
    }
    sandbox.protect();
}

pub fn registered_real_pdf_is_refused_before_provider_invocation() {
    let mut sandbox = Sandbox::new("10-pdf-capability");
    let python = PathBuf::from(
        std::env::var_os("NOETOME_TEST_PDF_PYTHON")
            .expect("explicit selected real Python required for registered-PDF capability test"),
    );
    assert!(python.is_absolute() && python.is_file());
    let expected = read_json(&repo().join("fixtures/pdf-text-v1/expected.json"));
    let case = expected["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["case_id"] == "N01")
        .unwrap();
    let source = repo()
        .join("fixtures/pdf-text-v1")
        .join(case["raw"]["relative_path"].as_str().unwrap());
    let selected = sandbox.root.join("explicit supported text.pdf");
    fs::copy(&source, &selected).unwrap();
    assert_eq!(sha(&fs::read(&selected).unwrap()), case["raw"]["sha256"]);
    let result = Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args([
            OsString::from("source"),
            OsString::from("import"),
            sandbox.project.as_os_str().to_owned(),
            selected.as_os_str().to_owned(),
            OsString::from("--python"),
            python.as_os_str().to_owned(),
        ])
        .current_dir(&sandbox.root)
        .output()
        .unwrap();
    write_json(
        &sandbox.root.join("actual-pdf-registration.json"),
        &json!({"executable":env!("CARGO_BIN_EXE_noetome"),"selected_python":python,"exit_code":result.status.code(),"stdout":String::from_utf8_lossy(&result.stdout),"stderr":String::from_utf8_lossy(&result.stderr)}),
    );
    assert!(
        result.status.success(),
        "real PDF registration failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let imported: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        read_json(&sandbox.project.join("project.json"))["schema_version"],
        3
    );
    sandbox.project_before = snapshot(&sandbox.project);
    let provider = FakeProvider::new("legal", &sandbox.template);
    let mut options = sandbox.options();
    options.version_id = imported["version_id"].as_str().unwrap().into();
    let output = sandbox.root.join("refused-pdf-extraction");
    let result = knowledge_extract::extract(
        &sandbox.project,
        &output,
        &provider,
        &options,
        &AtomicBool::new(false),
    );
    assert_eq!(
        result.unwrap_err().code,
        "MODEL_SOURCE_CAPABILITY_INSUFFICIENT"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    assert!(!output.exists());
    assert_eq!(sha(&fs::read(&selected).unwrap()), case["raw"]["sha256"]);
    sandbox.protect();
}

fn observe_sleeping_child(probe: &Path) -> (u32, ProcessObserver) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(bytes) = fs::read(probe) {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                if let Some(pid) = value["pid"].as_u64() {
                    let observer =
                        ProcessObserver::open(pid as u32).expect("actual native child handle");
                    assert_ne!(observer.creation_time(), 0);
                    assert_eq!(observer.wait_exit(Duration::ZERO).unwrap(), None);
                    return (pid as u32, observer);
                }
            }
        }
        assert!(Instant::now() < deadline, "missing actual child handshake");
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub fn live_source_and_output_handles_reject_same_byte_replacement() {
    assert!(cfg!(windows), "active native file handles require Windows");
    let sandbox = Sandbox::new("11-active-source-output-handles");
    let provider = Arc::new(FakeProvider::new("sleep_cancel", &sandbox.template));
    let cancel = Arc::new(AtomicBool::new(false));
    let output = sandbox.root.join("live-output");
    let project = sandbox.project.clone();
    let worker_output = output.clone();
    let options = sandbox.options();
    let worker_provider = Arc::clone(&provider);
    let worker_cancel = Arc::clone(&cancel);
    let worker = std::thread::spawn(move || {
        knowledge_extract::extract(
            &project,
            &worker_output,
            worker_provider.as_ref(),
            &options,
            worker_cancel.as_ref(),
        )
    });
    let (pid, observer) =
        observe_sleeping_child(&sandbox.template.with_file_name("native-sleep_cancel.json"));
    let mut refused = Vec::new();
    for path in [
        sandbox.project.join("project.json"),
        sandbox.project.join("project.sqlite"),
        sandbox.project.join(
            sandbox.imported["version"]["raw_snapshot"]
                .as_str()
                .unwrap(),
        ),
        sandbox.project.join(
            sandbox.imported["version"]["normalized_snapshot"]
                .as_str()
                .unwrap(),
        ),
        output.join("owner.json"),
        output.join("request.json"),
    ] {
        let bytes = fs::read(&path).unwrap();
        let rewrite_denied = fs::write(&path, &bytes).is_err();
        let renamed = path.with_file_name(format!("same-bytes-replacement-{}", Uuid::new_v4()));
        let rename_denied = fs::rename(&path, &renamed).is_err();
        refused.push(
            json!({"path":path,"rewrite_denied":rewrite_denied,"rename_denied":rename_denied}),
        );
    }
    for path in [&sandbox.project, &output] {
        let renamed = sandbox
            .root
            .join(format!("directory-replacement-{}", Uuid::new_v4()));
        let rename_denied = fs::rename(path, renamed).is_err();
        refused.push(json!({"path":path,"rewrite_denied":true,"rename_denied":rename_denied}));
    }
    cancel.store(true, Ordering::SeqCst);
    let report = worker.join().unwrap();
    assert!(
        observer
            .wait_exit(Duration::from_secs(2))
            .unwrap()
            .is_some()
    );
    let creation_time = observer.creation_time();
    drop(observer);
    write_json(
        &sandbox.root.join("actual-active-handle-probes.json"),
        &json!({"pid":pid,"native_creation_time":creation_time,"native_exit_observed":true,"probes":refused}),
    );
    for refusal in &refused {
        assert_eq!(
            refusal["rewrite_denied"], true,
            "same-byte active write was allowed: {}",
            refusal["path"]
        );
        assert_eq!(
            refusal["rename_denied"], true,
            "same-byte active rename was allowed: {}",
            refusal["path"]
        );
    }
    let report = report.unwrap();
    assert_eq!(report.failure.unwrap().code, "MODEL_CANCELLED");
    assert_eq!(report.result, "rejected");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    sandbox.protect();
}

pub fn unknown_file_inserted_during_invocation_is_preserved() {
    let sandbox = Sandbox::new("12-active-unknown-inventory");
    let provider = Arc::new(FakeProvider::new("sleep_cancel", &sandbox.template));
    let cancel = Arc::new(AtomicBool::new(false));
    let output = sandbox.root.join("unknown-preserved-output");
    let project = sandbox.project.clone();
    let worker_output = output.clone();
    let options = sandbox.options();
    let worker_provider = Arc::clone(&provider);
    let worker_cancel = Arc::clone(&cancel);
    let worker = std::thread::spawn(move || {
        knowledge_extract::extract(
            &project,
            &worker_output,
            worker_provider.as_ref(),
            &options,
            worker_cancel.as_ref(),
        )
    });
    let (pid, observer) =
        observe_sleeping_child(&sandbox.template.with_file_name("native-sleep_cancel.json"));
    let unknown = output.join("unexpected human content.md");
    let unknown_bytes = "处理中出现的未知人工内容，不允许覆盖或删除。\n".as_bytes();
    fs::write(&unknown, unknown_bytes).unwrap();
    cancel.store(true, Ordering::SeqCst);
    let result = worker.join().unwrap();
    assert!(
        observer
            .wait_exit(Duration::from_secs(2))
            .unwrap()
            .is_some()
    );
    let creation_time = observer.creation_time();
    drop(observer);
    let error = result.expect_err("changed unknown inventory must block a committed report");
    assert_eq!(fs::read(&unknown).unwrap(), unknown_bytes);
    assert!(!output.join("report.json").exists());
    assert!(!output.join("candidate.json").exists());
    let before = snapshot(&output);
    assert!(knowledge_extract::inspect(&sandbox.project, &output).is_err());
    assert_eq!(snapshot(&output), before);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    write_json(
        &sandbox.root.join("actual-unknown-inventory-probe.json"),
        &json!({"pid":pid,"native_creation_time":creation_time,"native_exit_observed":true,"error_code":error.code,"unknown_sha256":sha(unknown_bytes),"unknown_preserved":true}),
    );
    sandbox.protect();
}

pub fn known_currency_policy_rejects_usdt_and_preserves_usd_settlement() {
    let sandbox = Sandbox::new("13-currency-regression");
    let invalid = FakeProvider::new("currency_invalid", &sandbox.template);
    let (invalid_output, invalid_report) =
        sandbox.extract("invalid-four-letter-currency", &invalid);
    assert_eq!(invalid_report["result"], "rejected");
    assert_eq!(invalid_report["failure"]["code"], "MODEL_USAGE_INVALID");
    assert_eq!(invalid_report["provider_invocations"], 1);
    assert!(invalid_output.join("report.json").is_file());
    assert!(!invalid_output.join("candidate.json").exists());
    assert!(invalid_report["candidate_id"].is_null());
    let before_invalid = snapshot(&invalid_output);
    assert_eq!(
        serde_json::to_value(
            knowledge_extract::inspect(&sandbox.project, &invalid_output).unwrap()
        )
        .unwrap(),
        invalid_report
    );
    assert_eq!(snapshot(&invalid_output), before_invalid);
    let valid = FakeProvider::new("currency_usd", &sandbox.template);
    let (valid_output, valid_report) = sandbox.extract("valid-three-letter-currency", &valid);
    assert_eq!(valid_report["result"], "candidate");
    assert_eq!(valid_report["usage"]["cost"]["status"], "known");
    assert_eq!(valid_report["usage"]["cost"]["currency"], "USD");
    assert_eq!(valid_report["usage"]["cost"]["amount_microunits"], 7);
    assert_eq!(
        valid_report["settlement"]["usage"]["cost"]["currency"],
        "USD"
    );
    assert_eq!(
        valid_report["settlement"]["usage"]["cost"]["amount_microunits"],
        7
    );
    let usage: Usage = serde_json::from_value(valid_report["usage"].clone()).unwrap();
    let request_id = valid_report["request_id"].as_str().unwrap();
    let response_sha = valid_report["attempt"]["response_sha256"].as_str().unwrap();
    let mut ledger = SettlementLedger::default();
    assert_eq!(
        ledger.confirm(request_id, response_sha, &usage),
        Ok(SettlementResult::Confirmed)
    );
    let original_records = ledger.records();
    let before_valid = snapshot(&valid_output);
    for _ in 0..2 {
        assert_eq!(
            serde_json::to_value(
                knowledge_extract::inspect(&sandbox.project, &valid_output).unwrap()
            )
            .unwrap(),
            valid_report
        );
        assert_eq!(
            ledger.confirm(request_id, response_sha, &usage),
            Ok(SettlementResult::AlreadyConfirmed)
        );
    }
    assert_eq!(ledger.records(), original_records);
    assert_eq!(ledger.records().len(), 1);
    assert_eq!(snapshot(&valid_output), before_valid);
    assert_eq!(valid.calls.load(Ordering::SeqCst), 1);
    let totals = ledger.real_usage_totals().unwrap();
    assert_eq!(totals.real_responses, 0);
    assert_eq!(totals.simulated_responses, 1);
    assert!(
        totals.known_costs_microunits.is_empty(),
        "fake known-cost metadata was counted as real billing"
    );
    sandbox.protect();
}

pub fn empty_response_report_cross_fields_reject_synchronized_tamper() {
    let sandbox = Sandbox::new("14-attempt-crossfield-regression");
    for mode in [
        "not-called-with-pid",
        "stderr-max",
        "response-max",
        "authentication-classification",
        "empty-response-sha",
    ] {
        let provider = FakeProvider::new("empty", &sandbox.template);
        let (output, original) = sandbox.extract(&format!("empty-response-{mode}"), &provider);
        assert_eq!(original["result"], "rejected");
        assert_eq!(original["attempt"]["status"], "delivered");
        assert!(original["attempt"]["pid"].as_u64().unwrap() > 0);
        assert_eq!(original["attempt"]["response_bytes"], 0);
        assert_eq!(original["attempt"]["response_sha256"], sha(&[]));
        assert_eq!(original["response_saved"], false);
        let mut altered = original;
        match mode {
            "not-called-with-pid" => altered["attempt"]["status"] = json!("not_called"),
            "stderr-max" => altered["attempt"]["stderr_bytes"] = json!(u64::MAX),
            "response-max" => altered["attempt"]["response_bytes"] = json!(u64::MAX),
            "authentication-classification" => {
                altered["failure"]["code"] = json!("MODEL_AUTHENTICATION")
            }
            "empty-response-sha" => altered["attempt"]["response_sha256"] = json!("0".repeat(64)),
            _ => unreachable!(),
        }
        write_json(&output.join("report.json"), &altered);
        let before = snapshot(&output);
        assert!(
            knowledge_extract::inspect(&sandbox.project, &output).is_err(),
            "saved empty-response report accepted {mode}"
        );
        assert_eq!(
            snapshot(&output),
            before,
            "read-only inspector changed {mode} evidence"
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        sandbox.protect();
    }
}

pub fn fake_child(arguments: &[OsString]) -> i32 {
    let [mode, template_path] = arguments else {
        return 2;
    };
    let Some(mode) = mode.to_str() else {
        return 2;
    };
    let template_path = PathBuf::from(template_path);
    if !template_path.is_absolute() {
        return 2;
    }
    let mut request_bytes = Vec::new();
    if std::io::stdin()
        .take(1_048_577)
        .read_to_end(&mut request_bytes)
        .is_err()
        || request_bytes.len() > 1_048_576
    {
        return 2;
    }
    let Ok(request) = serde_json::from_slice::<Value>(&request_bytes) else {
        return 2;
    };
    if matches!(mode, "sleep" | "sleep_cancel") {
        let probe = template_path.with_file_name(format!("native-{mode}.json"));
        let Ok(mut file) = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(probe)
        else {
            return 2;
        };
        if file
            .write_all(
                serde_json::to_string(&json!({"pid":std::process::id()}))
                    .unwrap()
                    .as_bytes(),
            )
            .is_err()
            || file.flush().is_err()
        {
            return 2;
        }
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    if mode == "empty" {
        return 0;
    }
    if mode == "truncated" {
        print!("{{\"schema_version\":1");
        return 0;
    }
    if mode == "oversize" {
        let length = request["policy"]["max_response_bytes"].as_u64().unwrap() as usize + 1;
        let _ = std::io::stdout().write_all(&vec![b'x'; length]);
        return 0;
    }
    let Ok(template_bytes) = fs::read(&template_path) else {
        return 2;
    };
    let Ok(template) = serde_json::from_slice::<Template>(&template_bytes) else {
        return 2;
    };
    if template.schema_version != 1 || template.producer != "offline_fake_template" {
        return 2;
    }
    let Some(units) = request["source"]["units"].as_array() else {
        return 2;
    };
    let mut candidates = Vec::new();
    for (ordinal, statement) in template.statements.iter().enumerate() {
        let mut matches = Vec::new();
        for unit in units {
            let Some(text) = unit["text"].as_str() else {
                return 2;
            };
            for (start, _) in text.match_indices(&statement.support_quote) {
                matches.push(json!({"span_id":unit["span_id"],"excerpt_start_byte":start,"excerpt_end_byte":start+statement.support_quote.len(),"excerpt_sha256":sha(statement.support_quote.as_bytes())}));
            }
        }
        if matches.len() != 1 {
            return 2;
        }
        candidates.push(json!({"ordinal":ordinal,"statement":statement.statement,"qualifiers":statement.qualifiers,"evidence":matches}));
    }
    let mut outcome = json!({"schema_version":1,"protocol":"noetome-model-single-v1","request_id":request["request_id"],"request_sha256":sha(&request_bytes),"provider":request["provider"],"actual_model":request["provider"]["requested_model"],"status":"complete","candidates":candidates,
        "usage":{"provenance":"simulated","input_tokens":160,"output_tokens":400,"cost":{"status":"unknown","amount_microunits":null,"currency":null,"basis":"not_available"}},"failure":null});
    match mode {
        "legal" => {}
        "currency_invalid" | "currency_usd" => {
            outcome["usage"]["cost"] = json!({
                "status":"known", "amount_microunits":7,
                "currency":if mode == "currency_invalid" {"USDT"} else {"USD"},
                "basis":"provider_reported"
            });
        }
        "rate_limit" | "auth" | "capability" | "remote_unknown" | "failed_timeout" => {
            outcome["status"] = json!("failed");
            outcome["candidates"] = json!([]);
            outcome["failure"] = json!(match mode {
                "rate_limit" => "rate_limit",
                "auth" => "authentication",
                "capability" => "capability_insufficient",
                "failed_timeout" => "timeout",
                _ => "remote_unknown",
            });
        }
        "credential_echo" => outcome["candidates"][0]["statement"] = json!(CREDENTIAL_SENTINEL),
        "credential_escaped" => {
            outcome["candidates"][0]["statement"] = json!(CREDENTIAL_SENTINEL);
            let encoded = serde_json::to_string(&outcome).unwrap();
            let escaped: String = CREDENTIAL_SENTINEL
                .chars()
                .map(|character| format!("\\u{:04x}", character as u32))
                .collect();
            print!("{}", encoded.replace(CREDENTIAL_SENTINEL, &escaped));
            return 0;
        }
        "unknown" => outcome["output_path"] = json!("../production-vault"),
        "invalidref" => {
            outcome["candidates"][0]["evidence"][0]["span_id"] = json!(Uuid::new_v4().to_string())
        }
        "token_over" => outcome["usage"]["output_tokens"] = json!(1025),
        "wrong_version" => outcome["schema_version"] = json!(2),
        "wrong_identity" => outcome["request_id"] = json!(Uuid::new_v4().to_string()),
        "wrong_sha" => outcome["request_sha256"] = json!("0".repeat(64)),
        "wrong_model" => outcome["actual_model"] = json!(""),
        "wrong_producer" => outcome["provider"]["producer"] = json!("fixture"),
        "duplicate" => {
            let encoded = serde_json::to_string(&outcome).unwrap();
            print!("{{\"schema_version\":1,{}", &encoded[1..]);
            return 0;
        }
        _ => return 2,
    }
    let _ = std::io::stdout().write_all(&serde_json::to_vec(&outcome).unwrap());
    0
}
