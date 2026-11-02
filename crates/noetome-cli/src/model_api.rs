//! One explicitly configured Responses call and strictly owned audit reload.

use crate::credential_handoff::{self, SensitiveAttempt, TransientCredential};
use crate::knowledge_extract::ModelError;
use crate::model_candidate::{
    self, CandidateIr, Cost, CostBasis, CostStatus, ExtractionRequest, Outcome, OutcomeStatus,
    Policy, Producer, ProviderIdentity, Usage, UsageProvenance,
};
use crate::model_preflight::{self, FullTextSummary, PreparedText};
use crate::model_provider::AttemptStatus;
use crate::{api_store, project, sidecar, sidecar_process, source, source_model};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

#[path = "model_api_decode.rs"]
mod decode;
#[path = "model_api_guard.rs"]
mod guard;

pub const ENDPOINT: &str = "https://ai.1343263.xyz/v1/responses";
pub const PROVIDER_ID: &str = "ai-1343263-responses";
pub const MODEL: &str = "deepseek-flash";
pub const CREDENTIAL_REF: &str = "env:NOETOME_AI134_KEY";

macro_rules! named {
    ($name:ident { $($(#[$attribute:meta])* $field:ident : $kind:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $($(#[$attribute])* pub $field: $kind),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $($(#[$attribute])* $field: $kind),* }
                let fields: Fields = crate::pdf_model::deserialize_object(deserializer)?;
                Ok(Self { $($field: fields.$field),* })
            }
        }
    };
}
named!(ApiConfiguration {
    schema_version: u32,
    protocol: String,
    provider_id: String,
    endpoint: String,
    model: String,
    credential_ref: String,
    policy: Policy,
    total_token_budget: u64
});
named!(SourceAuthorization {
    schema_version: u32,
    protocol: String,
    authorization_id: String,
    configuration_sha256: String,
    endpoint: String,
    model: String,
    credential_ref: String,
    budget_total_tokens: u64,
    max_calls: u64,
    allow_externalization: bool,
    source: FullTextSummary,
    authorized_at: String
});
named!(ArtifactFingerprint {
    sha256: String,
    byte_length: u64
});
named!(RuntimeFingerprint {
    python_path: String,
    python: ArtifactFingerprint,
    adapter_path: String,
    adapter: ArtifactFingerprint,
    schema: ArtifactFingerprint,
    instructions: ArtifactFingerprint
});
named!(TotalTokenReservation {
    calls: u32,
    input_token_upper_bound: u64,
    output_tokens: u64,
    total_tokens: u64,
    basis: String
});
named!(ApiAttempt {
    status:AttemptStatus,
    #[serde(deserialize_with="crate::pdf_model::required_option")] pid:Option<u32>,
    elapsed_ms:u64,
    #[serde(deserialize_with="crate::pdf_model::required_option")] failure:Option<String>,
    stdout_suppressed:bool, stderr_suppressed:bool
});
named!(HttpObservation {
    schema_version:u32, protocol:String, request_sha256:String, http_request_sha256:String,
    endpoint:String, http_state:String,
    #[serde(deserialize_with="crate::pdf_model::required_option")] http_status:Option<u32>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] response_id:Option<String>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] actual_model:Option<String>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] failure:Option<String>,
    raw_response_available:bool, canonical_response_available:bool,
    #[serde(deserialize_with="crate::pdf_model::required_option")] usage_input_tokens:Option<u64>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] usage_output_tokens:Option<u64>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] usage_total_tokens:Option<u64>
});
named!(PreflightReport {
    schema_version: u32,
    protocol: String,
    provider_id: String,
    endpoint: String,
    model: String,
    credential_ref: String,
    source: FullTextSummary,
    configuration_path: String,
    configuration: ArtifactFingerprint,
    authorization_path: String,
    authorization: ArtifactFingerprint,
    runtime: RuntimeFingerprint,
    reservation: TotalTokenReservation,
    budget_total_tokens: u64,
    no_api_calls: bool,
    key_loaded: bool
});
named!(ApiReport {
    schema_version:u32, protocol:String, execution_kind:String, request_id:String, request_sha256:String,
    configuration_path:String, configuration:ArtifactFingerprint, authorization_path:String,
    authorization:ArtifactFingerprint, source:FullTextSummary, runtime:RuntimeFingerprint,
    reservation:TotalTokenReservation, budget_total_tokens:u64, attempt:ApiAttempt,
    #[serde(deserialize_with="crate::pdf_model::required_option")] observation:Option<HttpObservation>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] http_request:Option<ArtifactFingerprint>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] raw_response:Option<ArtifactFingerprint>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] canonical_response:Option<ArtifactFingerprint>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] candidate:Option<ArtifactFingerprint>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] candidate_id:Option<String>,
    http_state:String,
    #[serde(deserialize_with="crate::pdf_model::required_option")] confirmed_http_posts:Option<u32>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] http_status:Option<u32>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] response_id:Option<String>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] actual_model:Option<String>,
    #[serde(deserialize_with="crate::pdf_model::required_option")] usage_total_tokens:Option<u64>,
    usage:Usage, cost_unknown:bool, candidate_success:bool, verification:String,
    #[serde(deserialize_with="crate::pdf_model::required_option")] failure:Option<String>
});

include!("model_api_core.rs");

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn python() -> PathBuf {
        let path = std::env::var_os("NOETOME_TEST_API_PYTHON")
            .map(PathBuf::from)
            .expect("Set NOETOME_TEST_API_PYTHON to the explicitly selected absolute Python executable before running API offline tests.");
        assert!(
            path.is_absolute(),
            "NOETOME_TEST_API_PYTHON must be an absolute executable path."
        );
        path
    }
    const SOURCE: &[u8] = "灯箱的输入电压为 12 V，仅用于直流供电。\n".as_bytes();
    struct Sandbox {
        root: PathBuf,
        project: PathBuf,
        selected: PathBuf,
        version: String,
        config: PathBuf,
        authorization: PathBuf,
        output: PathBuf,
        before: BTreeMap<String, Vec<u8>>,
    }
    fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
        fn visit(root: &Path, current: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            for entry in fs::read_dir(current).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(root, &path, out)
                } else {
                    out.insert(
                        path.strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                        fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut out = BTreeMap::new();
        visit(root, root, &mut out);
        out
    }
    impl Sandbox {
        fn new() -> Self {
            let parent =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.artifacts/B04-T02-core-tests");
            fs::create_dir_all(&parent).unwrap();
            let root = parent
                .canonicalize()
                .unwrap()
                .join(Uuid::new_v4().to_string());
            fs::create_dir(&root).unwrap();
            let project = root.join("source-project");
            crate::project::initialize(&project).unwrap();
            let selected = root.join("selected.md");
            fs::write(&selected, SOURCE).unwrap();
            let imported: Value = serde_json::from_str(
                &crate::source::import(&project, &selected, None)
                    .unwrap()
                    .json,
            )
            .unwrap();
            let version = imported["version_id"].as_str().unwrap().to_owned();
            let source = crate::model_preflight::prepare_whole_text(&project, &version)
                .unwrap()
                .summary()
                .clone();
            let config = root.join("api-config.json");
            let authorization = root.join("api-authorization.json");
            let output = root.join("api-output");
            let configuration = ApiConfiguration {
                schema_version: 1,
                protocol: "noetome-api-config-v1".into(),
                provider_id: PROVIDER_ID.into(),
                endpoint: ENDPOINT.into(),
                model: MODEL.into(),
                credential_ref: CREDENTIAL_REF.into(),
                policy: Policy::default(),
                total_token_budget: 10_000_000,
            };
            let bytes = serde_json::to_vec_pretty(&configuration).unwrap();
            fs::write(&config, &bytes).unwrap();
            let auth = SourceAuthorization {
                schema_version: 1,
                protocol: "noetome-api-authorized-source-v1".into(),
                authorization_id: Uuid::new_v4().to_string(),
                configuration_sha256: crate::text::sha256(&bytes),
                endpoint: ENDPOINT.into(),
                model: MODEL.into(),
                credential_ref: CREDENTIAL_REF.into(),
                budget_total_tokens: 10_000_000,
                max_calls: 1,
                allow_externalization: true,
                source,
                authorized_at: "2026-11-02T00:00:00Z".into(),
            };
            fs::write(&authorization, serde_json::to_vec_pretty(&auth).unwrap()).unwrap();
            fs::write(root.join("external-sentinel"), b"preserve external bytes").unwrap();
            let before = files(&project);
            Self {
                root,
                project,
                selected,
                version,
                config,
                authorization,
                output,
                before,
            }
        }
        fn preflight(&self) -> Result<PreflightReport, ModelError> {
            preflight(
                &self.project,
                &self.version,
                &self.config,
                &self.authorization,
                &self.output,
                python().as_path(),
            )
        }
        fn protect(&self) {
            assert_eq!(files(&self.project), self.before);
            assert_eq!(
                fs::read(self.root.join("external-sentinel")).unwrap(),
                b"preserve external bytes"
            );
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            if std::thread::panicking() {
                return;
            }
            self.protect();
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
    fn err<T>(result: Result<T, ModelError>) -> ModelError {
        match result {
            Err(e) => e,
            Ok(_) => panic!("expected admission rejection"),
        }
    }
    fn rewrite(path: &Path, change: impl FnOnce(&mut Value)) {
        let mut value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        change(&mut value);
        fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }

    fn replay(sandbox: &Sandbox) -> Result<ApiReport, ModelError> {
        replay_raw(sandbox, None)
    }
    fn replay_raw(sandbox: &Sandbox, replacement: Option<&[u8]>) -> Result<ApiReport, ModelError> {
        use crate::credential_handoff::{self, TransientCredential};
        use std::ffi::OsString;
        use std::sync::atomic::AtomicBool;
        let adapter = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sidecars/responses_api/adapter.py")
            .canonicalize()
            .unwrap();
        let mut raw = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/responses-api-synthetic-v1/http-response.json")
            .canonicalize()
            .unwrap();
        if let Some(bytes) = replacement {
            raw = sandbox.root.join("private-vendor-response.json");
            fs::write(&raw, bytes).unwrap();
        }
        let script = sandbox.root.join("private-replay.py");
        fs::write(&script,br#"import importlib.util,pathlib,sys
s=importlib.util.spec_from_file_location('responses_private_replay',sys.argv[1]);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
credential,wire=m.decode_frame(sys.stdin.buffer.read())
raw=pathlib.Path(sys.argv[2]).read_bytes()
outcome,observation=m.decode_http_response(wire,raw,200,credential)
m.save_fixed('http-request.json',m.build_http_body(wire))
m.save_fixed('http-observation.json',m.encode(observation))
if observation['raw_response_available']:m.save_fixed('http-response.bin',raw)
if outcome is not None:sys.stdout.buffer.write(m.encode(outcome))
"#).unwrap();
        let credential = TransientCredential::new(b"B04_T02_PRIVATE_SYNTHETIC_KEY_28176").unwrap();
        extract_for_test(
            &sandbox.project,
            &sandbox.version,
            &sandbox.config,
            &sandbox.authorization,
            &sandbox.output,
            python().as_path(),
            &credential,
            |request, wire, runtime, key| {
                credential_handoff::run(
                    python().as_path(),
                    &[
                        OsString::from("-B"),
                        script.into_os_string(),
                        adapter.into_os_string(),
                        raw.into_os_string(),
                    ],
                    request,
                    wire,
                    runtime,
                    key,
                    &AtomicBool::new(false),
                )
            },
        )
    }
    #[test]
    fn admission_retains_native_guards_for_source_config_runtime_and_shared_assets_until_drop() {
        use std::fs::OpenOptions;
        let sandbox = Sandbox::new();
        let admission = Admission::open(
            &sandbox.project,
            &sandbox.version,
            &sandbox.config,
            &sandbox.authorization,
            python().as_path(),
        )
        .unwrap();
        for path in [
            &admission.config.path,
            &admission.auth.path,
            &admission.python.path,
            &admission.adapter.path,
            &admission.schema.path,
            &admission.instructions.path,
        ] {
            assert!(
                OpenOptions::new().write(true).open(path).is_err(),
                "selected input lost its read guard"
            );
        }
        for input in &admission.source_files {
            assert!(OpenOptions::new().write(true).open(&input.path).is_err());
        }
        assert!(fs::rename(&sandbox.project, sandbox.root.join("replaced-project")).is_err());
        assert!(fs::rename(&sandbox.root, sandbox.root.with_extension("moved")).is_err());
        admission.verify().unwrap();
        drop(admission);
        let file = OpenOptions::new()
            .write(true)
            .open(&sandbox.config)
            .unwrap();
        drop(file);
        sandbox.protect();
    }
    #[test]
    fn selected_input_hardlinks_and_registered_snapshot_tamper_are_rejected_without_output() {
        let sandbox = Sandbox::new();
        let link = sandbox.root.join("config-link");
        fs::hard_link(&sandbox.config, &link).unwrap();
        assert_eq!(err(sandbox.preflight()).code, "MODEL_API_CONFIG_INVALID");
        fs::remove_file(link).unwrap();
        let prepared =
            model_preflight::prepare_whole_text(&sandbox.project, &sandbox.version).unwrap();
        drop(prepared);
        let guards = source_guards(&sandbox.project, &sandbox.version).unwrap();
        let raw = guards[2].path.clone();
        drop(guards);
        let bytes = fs::read(&raw).unwrap();
        fs::write(&raw, b"changed bytes").unwrap();
        assert_eq!(err(sandbox.preflight()).code, "MODEL_API_SOURCE_INVALID");
        fs::write(&raw, bytes).unwrap();
        assert!(!sandbox.output.exists());
        sandbox.protect();
    }
    #[test]
    fn archived_saved_documents_reload_without_original_config_or_executing_provenance_paths() {
        let sandbox = Sandbox::new();
        replay(&sandbox).unwrap();
        fs::remove_file(&sandbox.config).unwrap();
        fs::remove_file(&sandbox.authorization).unwrap();
        rewrite(&sandbox.output.join("report.json"), |report| {
            report["configuration_path"] = json!("C:/unselected/private-data");
            report["authorization_path"] = json!("C:/unselected/private-authorization");
            report["runtime"]["python_path"] = json!("C:/Windows/System32/cmd.exe");
            report["runtime"]["adapter_path"] = json!("C:/unselected/execute-me.py")
        });
        let copied = sandbox.root.join("archived-api-copy");
        fs::create_dir(&copied).unwrap();
        for entry in fs::read_dir(&sandbox.output).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), copied.join(entry.file_name())).unwrap();
        }
        let before = files(&copied);
        let report = inspect(&sandbox.project, &copied).unwrap();
        assert!(report.candidate_success);
        assert_eq!(files(&copied), before);
        sandbox.protect();
    }
    #[test]
    fn incomplete_response_preserves_reported_usage_and_never_creates_candidate() {
        let sandbox = Sandbox::new();
        let mut raw: Value = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/responses-api-synthetic-v1/http-response.json"
        ))
        .unwrap();
        raw["status"] = json!("incomplete");
        raw["incomplete_details"] = json!({"reason":"max_output_tokens"});
        let report = replay_raw(&sandbox, Some(&serde_json::to_vec(&raw).unwrap())).unwrap();
        assert!(!report.candidate_success);
        assert!(report.raw_response.is_some());
        assert!(report.canonical_response.is_none());
        assert_eq!(report.usage.input_tokens, Some(100));
        assert_eq!(report.usage_total_tokens, Some(130));
        assert_eq!(
            report.failure.as_deref(),
            Some("MODEL_API_CAPABILITY_INSUFFICIENT")
        );
        assert_eq!(inspect(&sandbox.project, &sandbox.output).unwrap(), report);
        sandbox.protect();
    }
    #[test]
    fn vendor_credential_echo_is_suppressed_without_response_length_sha_or_candidate() {
        let sandbox = Sandbox::new();
        let mut raw: Value = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/responses-api-synthetic-v1/http-response.json"
        ))
        .unwrap();
        raw["metadata"] = json!({"echo":"B04_T02_PRIVATE_SYNTHETIC_KEY_28176"});
        let report = replay_raw(&sandbox, Some(&serde_json::to_vec(&raw).unwrap())).unwrap();
        assert!(!report.candidate_success);
        assert!(report.raw_response.is_none());
        assert!(report.canonical_response.is_none());
        assert!(report.actual_model.is_none());
        assert!(report.response_id.is_none());
        for bytes in files(&sandbox.output).values() {
            assert!(
                !bytes
                    .windows(b"B04_T02_PRIVATE_SYNTHETIC_KEY_28176".len())
                    .any(|part| part == b"B04_T02_PRIVATE_SYNTHETIC_KEY_28176")
            );
        }
        assert_eq!(inspect(&sandbox.project, &sandbox.output).unwrap(), report);
        sandbox.protect();
    }
    #[test]
    fn saved_transport_failure_cannot_be_arbitrary_text_or_contradict_delivered_candidate() {
        let sandbox = Sandbox::new();
        replay(&sandbox).unwrap();
        rewrite(&sandbox.output.join("report.json"), |report| {
            report["attempt"]["failure"] = json!("arbitrary remote error body")
        });
        assert!(inspect(&sandbox.project, &sandbox.output).is_err());
    }
    #[test]
    fn saved_failure_classification_must_match_the_observed_raw_response() {
        let sandbox = Sandbox::new();
        let mut raw: Value = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/responses-api-synthetic-v1/http-response.json"
        ))
        .unwrap();
        raw["status"] = json!("incomplete");
        raw["incomplete_details"] = json!({"reason":"max_output_tokens"});
        replay_raw(&sandbox, Some(&serde_json::to_vec(&raw).unwrap())).unwrap();
        rewrite(&sandbox.output.join("report.json"), |report| {
            report["failure"] = json!("MODEL_API_RATE_LIMIT")
        });
        assert!(inspect(&sandbox.project, &sandbox.output).is_err());
    }
    #[test]
    fn review_raw_absent_rejects_synchronized_claimed_model_response_id_or_usage() {
        let sandbox = Sandbox::new();
        let mut raw: Value = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/responses-api-synthetic-v1/http-response.json"
        ))
        .unwrap();
        raw["metadata"] = json!({"echo":"B04_T02_PRIVATE_SYNTHETIC_KEY_28176"});
        let report = replay_raw(&sandbox, Some(&serde_json::to_vec(&raw).unwrap())).unwrap();
        assert!(report.raw_response.is_none());
        let base_report = serde_json::to_value(&report).unwrap();
        let base_observation = serde_json::to_value(report.observation.as_ref().unwrap()).unwrap();
        for (field, value) in [
            ("response_id", json!("forged-response-id")),
            ("actual_model", json!("forged-model")),
            ("usage_input_tokens", json!(100)),
            ("usage_output_tokens", json!(30)),
            ("usage_total_tokens", json!(130)),
        ] {
            let mut observation = base_observation.clone();
            observation[field] = value.clone();
            let mut saved = base_report.clone();
            saved["observation"] = observation.clone();
            match field {
                "usage_input_tokens" => saved["usage"]["input_tokens"] = value,
                "usage_output_tokens" => saved["usage"]["output_tokens"] = value,
                field => saved[field] = value,
            };
            fs::write(
                sandbox.output.join("http-observation.json"),
                serde_json::to_vec_pretty(&observation).unwrap(),
            )
            .unwrap();
            fs::write(
                sandbox.output.join("report.json"),
                serde_json::to_vec_pretty(&saved).unwrap(),
            )
            .unwrap();
            assert!(
                inspect(&sandbox.project, &sandbox.output).is_err(),
                "raw absent accepted forged {field}"
            );
        }
    }
    #[test]
    fn review_completed_envelope_shape_failures_preserve_raw_model_usage_and_python_classification()
    {
        let original: Value = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/responses-api-synthetic-v1/http-response.json"
        ))
        .unwrap();
        for case in [
            "zero-messages",
            "two-messages",
            "incomplete-message",
            "in-progress-message",
            "zero-content",
            "two-content",
        ] {
            let sandbox = Sandbox::new();
            let mut raw = original.clone();
            match case {
                "zero-messages" => raw["output"] = json!([]),
                "two-messages" => {
                    raw["output"] =
                        json!([original["output"][0].clone(), original["output"][0].clone()])
                }
                "incomplete-message" => raw["output"][0]["status"] = json!("incomplete"),
                "in-progress-message" => raw["output"][0]["status"] = json!("in_progress"),
                "zero-content" => raw["output"][0]["content"] = json!([]),
                _ => {
                    raw["output"][0]["content"] = json!([
                        original["output"][0]["content"][0].clone(),
                        original["output"][0]["content"][0].clone()
                    ])
                }
            }
            let report = replay_raw(&sandbox, Some(&serde_json::to_vec(&raw).unwrap())).unwrap();
            assert!(
                report.raw_response.is_some(),
                "{case} erased complete raw response"
            );
            assert_eq!(
                report.actual_model.as_deref(),
                Some("deepseek-flash-returned-synthetic"),
                "{case}"
            );
            assert_eq!(report.usage.input_tokens, Some(100), "{case}");
            assert_eq!(report.usage_total_tokens, Some(130), "{case}");
            assert_eq!(
                report.failure.as_deref(),
                Some("MODEL_API_CAPABILITY_INSUFFICIENT"),
                "{case}"
            );
            assert_eq!(inspect(&sandbox.project, &sandbox.output).unwrap(), report);
        }
    }
    #[test]
    fn review_budget_rejection_failure_cannot_be_relabelled_as_authentication() {
        let sandbox = Sandbox::new();
        let mut raw: Value = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/responses-api-synthetic-v1/http-response.json"
        ))
        .unwrap();
        raw["usage"]["input_tokens"] = json!(12_000_000);
        raw["usage"]["total_tokens"] = json!(12_000_030);
        let report = replay_raw(&sandbox, Some(&serde_json::to_vec(&raw).unwrap())).unwrap();
        assert_eq!(
            report.failure.as_deref(),
            Some("MODEL_API_USAGE_BUDGET_EXCEEDED")
        );
        assert!(report.canonical_response.is_some());
        assert!(!report.candidate_success);
        assert_eq!(inspect(&sandbox.project, &sandbox.output).unwrap(), report);
        rewrite(&sandbox.output.join("report.json"), |report| {
            report["failure"] = json!("MODEL_API_AUTHENTICATION")
        });
        assert!(inspect(&sandbox.project, &sandbox.output).is_err());
    }
    #[test]
    fn final_review_not_called_cannot_coexist_with_completed_http_receipt() {
        let sandbox = Sandbox::new();
        let mut raw: Value = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/responses-api-synthetic-v1/http-response.json"
        ))
        .unwrap();
        raw["status"] = json!("incomplete");
        raw["incomplete_details"] = json!({"reason":"max_output_tokens"});
        let report = replay_raw(&sandbox, Some(&serde_json::to_vec(&raw).unwrap())).unwrap();
        assert!(report.raw_response.is_some());
        rewrite(&sandbox.output.join("report.json"), |report| {
            report["attempt"]["status"] = json!("not_called");
            report["attempt"]["pid"] = Value::Null;
            report["attempt"]["failure"] = json!("MODEL_API_TRANSPORT_FAILED");
            report["attempt"]["stdout_suppressed"] = json!(true)
        });
        assert!(inspect(&sandbox.project, &sandbox.output).is_err());
    }
    #[test]
    fn final_review_known_input_above_total_budget_cannot_hide_behind_unknown_total() {
        let sandbox = Sandbox::new();
        let mut raw: Value = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/responses-api-synthetic-v1/http-response.json"
        ))
        .unwrap();
        raw["usage"]["input_tokens"] = json!(12_000_000);
        raw["usage"]["total_tokens"] = Value::Null;
        let report = replay_raw(&sandbox, Some(&serde_json::to_vec(&raw).unwrap())).unwrap();
        assert!(!report.candidate_success);
        assert_eq!(
            report.failure.as_deref(),
            Some("MODEL_API_USAGE_BUDGET_EXCEEDED")
        );
        assert_eq!(report.usage.input_tokens, Some(12_000_000));
        assert_eq!(report.usage_total_tokens, None);
        assert!(report.canonical_response.is_some());
        assert_eq!(inspect(&sandbox.project, &sandbox.output).unwrap(), report);
    }
    #[test]
    fn private_actual_child_produces_unverified_candidate_and_readonly_reload_with_zero_real_api() {
        let sandbox = Sandbox::new();
        let report = replay(&sandbox).unwrap();
        assert_eq!(report.execution_kind, "offline_test");
        assert_eq!(report.confirmed_http_posts, Some(0));
        assert!(report.candidate_success);
        assert_eq!(
            report.actual_model.as_deref(),
            Some("deepseek-flash-returned-synthetic")
        );
        assert_eq!(report.usage.input_tokens, Some(100));
        assert_eq!(report.usage.output_tokens, Some(30));
        assert_eq!(report.verification, "unverified");
        fs::rename(&sandbox.selected, sandbox.root.join("moved-source.md")).unwrap();
        let before = files(&sandbox.output);
        let reloaded = inspect(&sandbox.project, &sandbox.output).unwrap();
        assert_eq!(reloaded, report);
        assert_eq!(files(&sandbox.output), before);
        sandbox.protect();
    }
    #[test]
    fn saved_http_body_response_observation_and_usage_are_cross_checked_independently() {
        for artifact in [
            "http-request.json",
            "http-response.bin",
            "http-observation.json",
            "response.json",
            "candidate.json",
        ] {
            let sandbox = Sandbox::new();
            replay(&sandbox).unwrap();
            let path = sandbox.output.join(artifact);
            rewrite(&path, |value| match artifact {
                "http-request.json" => value["model"] = json!("synchronized-tamper"),
                "http-response.bin" => value["usage"]["input_tokens"] = json!(101),
                "http-observation.json" => value["usage_input_tokens"] = json!(101),
                "response.json" => value["usage"]["input_tokens"] = json!(101),
                _ => {
                    value["atoms"][0]["proposal"]["statement"] =
                        json!("unsupported synthetic claim")
                }
            });
            let digest = crate::text::sha256(&fs::read(&path).unwrap());
            let len = fs::metadata(&path).unwrap().len();
            rewrite(&sandbox.output.join("report.json"), |report| {
                let field = match artifact {
                    "http-request.json" => "http_request",
                    "http-response.bin" => "raw_response",
                    "http-observation.json" => "observation",
                    "response.json" => "canonical_response",
                    _ => "candidate",
                };
                if field == "observation" {
                    report[field]["usage_input_tokens"] = json!(101)
                } else {
                    report[field]["sha256"] = json!(digest);
                    report[field]["byte_length"] = json!(len)
                }
            });
            assert!(
                inspect(&sandbox.project, &sandbox.output).is_err(),
                "accepted synchronized {artifact} mutation"
            );
            sandbox.protect();
        }
    }
    #[test]
    fn saved_config_and_authorization_cannot_change_fixed_policy_or_source() {
        for artifact in ["config.json", "authorization.json"] {
            let sandbox = Sandbox::new();
            replay(&sandbox).unwrap();
            let path = sandbox.output.join(artifact);
            rewrite(&path, |value| {
                if artifact == "config.json" {
                    value["total_token_budget"] = json!(9_000_000)
                } else {
                    value["endpoint"] = json!("https://changed.invalid/v1/responses")
                }
            });
            let sha = crate::text::sha256(&fs::read(&path).unwrap());
            let length = fs::metadata(&path).unwrap().len();
            rewrite(&sandbox.output.join("report.json"), |report| {
                let field = if artifact == "config.json" {
                    "configuration"
                } else {
                    "authorization"
                };
                report[field]["sha256"] = json!(sha);
                report[field]["byte_length"] = json!(length)
            });
            assert!(inspect(&sandbox.project, &sandbox.output).is_err());
        }
    }
    #[test]
    fn incomplete_missing_observation_does_not_become_unsent_or_bill_zero() {
        use crate::credential_handoff::{SensitiveAttempt, TransientCredential};
        let sandbox = Sandbox::new();
        let key = TransientCredential::new(b"B04_SYNTHETIC_NO_OBSERVATION_KEY").unwrap();
        let report = extract_for_test(
            &sandbox.project,
            &sandbox.version,
            &sandbox.config,
            &sandbox.authorization,
            &sandbox.output,
            python().as_path(),
            &key,
            |_, _, _, _| SensitiveAttempt {
                status: AttemptStatus::RemoteUnknown,
                pid: Some(1234),
                elapsed_ms: 2,
                failure: Some("MODEL_REMOTE_UNKNOWN"),
                stdout_suppressed: true,
                stderr_suppressed: true,
                response: None,
                response_sha256: None,
                outcome: None,
            },
        )
        .unwrap();
        assert_eq!(report.http_state, "remote_unknown");
        assert_eq!(report.confirmed_http_posts, Some(0));
        assert!(!report.candidate_success);
        assert_eq!(report.usage.input_tokens, None);
        assert!(report.cost_unknown);
        assert_eq!(inspect(&sandbox.project, &sandbox.output).unwrap(), report);
    }
    #[test]
    fn unsafe_metadata_is_refused_before_store_creation_or_provider_entry() {
        use crate::credential_handoff::TransientCredential;
        let sandbox = Sandbox::new();
        let key = TransientCredential::new(b"deepseek-flash").unwrap();
        let error = err(extract_for_test(
            &sandbox.project,
            &sandbox.version,
            &sandbox.config,
            &sandbox.authorization,
            &sandbox.output,
            python().as_path(),
            &key,
            |_, _, _, _| panic!("unsafe admission invoked provider"),
        ));
        assert_eq!(error.code, "MODEL_API_CREDENTIAL_ECHO");
        assert!(!sandbox.output.exists());
        sandbox.protect();
    }

    #[test]
    fn full_text_preflight_binds_actual_config_authorization_source_and_budget_without_output() {
        let sandbox = Sandbox::new();
        let result = sandbox.preflight().unwrap();
        assert_eq!(result.source.normalized_byte_length, 57);
        assert_eq!(result.endpoint, ENDPOINT);
        assert_eq!(result.reservation.total_tokens, 66560);
        assert!(result.no_api_calls);
        assert!(!result.key_loaded);
        assert!(!sandbox.output.exists());
        sandbox.protect();
    }
    #[test]
    fn configuration_rejects_unknown_endpoint_model_and_budget_before_output() {
        for (field, value, code) in [
            (
                "endpoint",
                json!("https://different.invalid/v1/responses"),
                "MODEL_API_CONFIG_INVALID",
            ),
            ("model", json!("different"), "MODEL_API_CONFIG_INVALID"),
            (
                "total_token_budget",
                json!(1),
                "MODEL_API_BUDGET_INSUFFICIENT",
            ),
        ] {
            let sandbox = Sandbox::new();
            rewrite(&sandbox.config, |cfg| cfg[field] = value);
            assert_eq!(err(sandbox.preflight()).code, code);
            assert!(!sandbox.output.exists());
            sandbox.protect();
        }
    }
    #[test]
    fn authorization_binds_config_sha_actual_source_and_single_external_call() {
        for (field, value) in [
            ("configuration_sha256", json!("b".repeat(64))),
            ("allow_externalization", json!(false)),
            ("max_calls", json!(2)),
            ("budget_total_tokens", json!(10)),
            ("authorized_at", json!("2026-11-02T08:00:00+08:00")),
        ] {
            let sandbox = Sandbox::new();
            rewrite(&sandbox.authorization, |auth| auth[field] = value);
            assert_eq!(
                err(sandbox.preflight()).code,
                "MODEL_API_AUTHORIZATION_INVALID"
            );
            assert!(!sandbox.output.exists());
        }
        let sandbox = Sandbox::new();
        rewrite(&sandbox.authorization, |auth| {
            auth["source"]["normalized_sha256"] = json!("b".repeat(64))
        });
        assert_eq!(
            err(sandbox.preflight()).code,
            "MODEL_API_AUTHORIZATION_INVALID"
        );
    }
    #[test]
    fn named_dtos_reject_duplicates_unknown_arrays_and_missing_required_fields() {
        let sandbox = Sandbox::new();
        let bytes = fs::read(&sandbox.config).unwrap();
        for raw in [
            b"[]".to_vec(),
            b"{}".to_vec(),
            format!(
                "{{\"schema_version\":1,{}",
                std::str::from_utf8(&bytes)
                    .unwrap()
                    .trim_start()
                    .trim_start_matches('{')
            )
            .into_bytes(),
        ] {
            fs::write(&sandbox.config, raw).unwrap();
            assert_eq!(err(sandbox.preflight()).code, "MODEL_API_CONFIG_INVALID");
            assert!(!sandbox.output.exists());
        }
        fs::write(&sandbox.config, &bytes).unwrap();
        rewrite(&sandbox.config, |cfg| cfg["extra"] = json!(0));
        assert_eq!(err(sandbox.preflight()).code, "MODEL_API_CONFIG_INVALID");
    }
    #[test]
    fn missing_key_and_low_budget_have_no_child_or_output() {
        let sandbox = Sandbox::new();
        assert!(
            std::env::var_os("NOETOME_AI134_KEY").is_none(),
            "ordinary offline test must run without live key"
        );
        assert_eq!(
            err(extract(
                &sandbox.project,
                &sandbox.version,
                &sandbox.config,
                &sandbox.authorization,
                &sandbox.output,
                python().as_path()
            ))
            .code,
            "MODEL_API_CREDENTIAL_MISSING"
        );
        assert!(!sandbox.output.exists());
        rewrite(&sandbox.config, |cfg| cfg["total_token_budget"] = json!(1));
        assert_eq!(
            err(extract(
                &sandbox.project,
                &sandbox.version,
                &sandbox.config,
                &sandbox.authorization,
                &sandbox.output,
                python().as_path()
            ))
            .code,
            "MODEL_API_BUDGET_INSUFFICIENT"
        );
        assert!(!sandbox.output.exists());
        sandbox.protect();
    }
    #[test]
    fn original_source_may_move_but_registered_snapshot_and_selected_runtime_are_rechecked() {
        let sandbox = Sandbox::new();
        fs::rename(&sandbox.selected, sandbox.root.join("moved.md")).unwrap();
        assert!(sandbox.preflight().is_ok());
        assert_eq!(
            err(preflight(
                &sandbox.project,
                &sandbox.version,
                &sandbox.config,
                &sandbox.authorization,
                &sandbox.output,
                &sandbox.root.join("missing-python.exe")
            ))
            .code,
            "MODEL_API_RUNTIME_INVALID"
        );
        assert!(!sandbox.output.exists());
    }
}
