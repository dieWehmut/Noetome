use std::{fs, io::{Read, Write}, path::Path, sync::atomic::{AtomicBool, AtomicUsize, Ordering}};
use noetome_cli::{knowledge_extract::{self, ExtractionOptions}, model_candidate::{self, ExtractionRequest, Policy, ProviderIdentity}, model_provider::{self, Budget, Invocation, Provider}};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn sha(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
struct ProbeProvider { identity: ProviderIdentity, mode: String, calls: AtomicUsize }
impl ProbeProvider {
    fn new(mode: &str) -> Self {
        Self { identity: serde_json::from_value(json!({"producer":"fake", "provider_id":"read-only-review-fake", "adapter_version":"wire-review-v1", "requested_model":"synthetic-review"})).unwrap(), mode: mode.into(), calls: AtomicUsize::new(0) }
    }
}
impl Provider for ProbeProvider {
    fn identity(&self) -> &ProviderIdentity { &self.identity }
    fn invocation(&self) -> Invocation {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Invocation { program: std::env::current_exe().unwrap(), args: vec!["--fake-child".into(), self.mode.clone().into()] }
    }
}
fn outcome(request_bytes: &[u8], currency: &str) -> Value {
    let request: Value = serde_json::from_slice(request_bytes).unwrap();
    let unit = &request["source"]["units"][0];
    let text = unit["text"].as_str().unwrap();
    json!({"schema_version":1, "protocol":"noetome-model-single-v1", "request_id":request["request_id"], "request_sha256":sha(request_bytes), "provider":request["provider"], "actual_model":"synthetic-review", "status":"complete", "candidates":[{"ordinal":0, "statement":"Synthetic response used only for wire review.", "qualifiers":[], "evidence":[{"span_id":unit["span_id"], "excerpt_start_byte":0, "excerpt_end_byte":text.len(), "excerpt_sha256":sha(text.as_bytes())}]}], "usage":{"provenance":"simulated", "input_tokens":20, "output_tokens":10, "cost":{"status":"known", "amount_microunits":7, "currency":currency, "basis":"provider_reported"}}, "failure":null})
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg|arg=="--fake-child") {
        let mode=&args[1];
        let mut bytes=Vec::new();
        std::io::stdin().read_to_end(&mut bytes).unwrap();
        if mode=="empty" { return; }
        std::io::stdout().write_all(&serde_json::to_vec(&outcome(&bytes,mode)).unwrap()).unwrap();
        return;
    }
    let root=Path::new(env!("CARGO_MANIFEST_DIR"));
    let evidence=root.join("evidence");
    fs::create_dir(&evidence).unwrap();
    let project=evidence.join("project");
    noetome_cli::project::initialize(&project).unwrap();
    let source=evidence.join("selected.md");
    fs::write(&source,"Review source text.\n").unwrap();
    let imported=noetome_cli::source::import(&project,&source,None).unwrap();
    assert_eq!(imported.exit_code,0);
    let imported:Value=serde_json::from_str(&imported.json).unwrap();
    let span_ids=imported["spans"].as_array().unwrap().iter().filter(|span|span["parse_status"]=="parsed" && span["kind"]!="formatting").map(|span|span["span_id"].as_str().unwrap().to_string()).collect();
    let options=ExtractionOptions {version_id:imported["version_id"].as_str().unwrap().into(),span_ids,policy:Policy::default(),budget:Budget{available_calls:1,available_output_tokens:1024},credential_sentinels:vec![]};
    let before=fs::read(project.join("project.sqlite")).unwrap();
    let mut results=Vec::new();
    for currency in ["USD","USDT","usd"] {
        let provider=ProbeProvider::new(currency);
        let output=evidence.join(format!("currency-{currency}"));
        let result=knowledge_extract::extract(&project,&output,&provider,&options,&AtomicBool::new(false));
        let request_bytes=fs::read(output.join("request.json")).unwrap();
        let request:ExtractionRequest=serde_json::from_slice(&request_bytes).unwrap();
        let response=serde_json::to_vec(&outcome(&request_bytes,currency)).unwrap();
        let decoded=model_candidate::decode_outcome(&response,&request,&sha(&request_bytes));
        let ledger=decoded.as_ref().map(|value|model_provider::SettlementLedger::default().confirm(&request.request_id,&sha(&response),&value.usage));
        let inventory:Vec<String>=fs::read_dir(&output).unwrap().map(|entry|entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
        results.push(json!({"currency":currency,"decode_outcome":format!("{decoded:?}"),"settlement":format!("{ledger:?}"),"extract":format!("{result:?}"),"provider_invocations":provider.calls.load(Ordering::SeqCst),"inventory":inventory,"response_sha":sha(&response)}));
    }
    let provider=ProbeProvider::new("empty");
    let output=evidence.join("failure-report");
    let baseline=knowledge_extract::extract(&project,&output,&provider,&options,&AtomicBool::new(false)).unwrap();
    fs::write(evidence.join("baseline-failure-report.json"),serde_json::to_vec_pretty(&baseline).unwrap()).unwrap();
    let mut tampered=serde_json::to_value(&baseline).unwrap();
    tampered["attempt"]["status"]=json!("not_called");
    tampered["attempt"]["response_bytes"]=json!(u64::MAX);
    tampered["attempt"]["response_sha256"]=json!("a".repeat(64));
    tampered["failure"]["code"]=json!("MODEL_AUTHENTICATION");
    fs::write(output.join("report.json"),serde_json::to_vec_pretty(&tampered).unwrap()).unwrap();
    let inspected=knowledge_extract::inspect(&project,&output);
    results.push(json!({"failure_report_tamper_accepted":inspected.is_ok(),"tamper_inspect":format!("{inspected:?}"),"baseline_status":baseline.attempt.status,"original_pid":baseline.attempt.pid,"provider_invocations":provider.calls.load(Ordering::SeqCst)}));
    assert_eq!(before,fs::read(project.join("project.sqlite")).unwrap());
    fs::write(evidence.join("results.json"),serde_json::to_vec_pretty(&results).unwrap()).unwrap();
    println!("{}",serde_json::to_string_pretty(&results).unwrap());
}
