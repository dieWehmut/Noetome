//! A real local HTTP fake server, ordinary CLI, and author data fixed before implementation.
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use uuid::Uuid;

const TEST_KEY: &str = "noetome-test-public-credential-never-real";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(args)
        .output()
        .unwrap()
}

struct Example {
    root: PathBuf,
    project: PathBuf,
    config: PathBuf,
    output: PathBuf,
    imported: Value,
    database: Vec<u8>,
    project_config: Vec<u8>,
}
impl Example {
    fn new(endpoint: &str, max_requests: u32) -> Self {
        let parent = repo().join("target/compiler-temp");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!("api-cli-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let project = root.join("source 中文");
        let init = cli(&["project", "init", project.to_str().unwrap()]);
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );
        let source = repo().join("fixtures/extraction-gateway-v1/source.txt");
        let run = cli(&[
            "source",
            "import",
            project.to_str().unwrap(),
            source.to_str().unwrap(),
        ]);
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let imported = serde_json::from_slice(&run.stdout).unwrap();
        let config = root.join("api-config.json");
        let value = json!({
            "schema_version":1, "provider":"openai_responses", "transport_mode":"local_http_test",
            "endpoint":endpoint, "model":"noetome-test-model", "credential_env":"NOETOME_TEST_CREDENTIAL",
            "required_input":"text", "capabilities":{"text":true,"structured_output":true,"vision":false},
            "selection":[{"kind":"text","ordinal":0}],
            "limits":{"max_units":1,"max_input_bytes":4096,"max_response_bytes":65536,
                "max_output_tokens":2048,"timeout_ms":3000},
            "budget":{"max_requests":max_requests,"max_total_tokens":65536,"max_cost_usd":null}
        });
        fs::write(&config, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        let database = fs::read(project.join("project.sqlite")).unwrap();
        let project_config = fs::read(project.join("project.json")).unwrap();
        Self {
            output: root.join("new audit"),
            root,
            project,
            config,
            imported,
            database,
            project_config,
        }
    }
    fn run(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args([
                "extract",
                "api",
                self.project.to_str().unwrap(),
                self.imported["import_id"].as_str().unwrap(),
                self.config.to_str().unwrap(),
                self.output.to_str().unwrap(),
            ])
            .env("NOETOME_TEST_CREDENTIAL", TEST_KEY)
            .output()
            .unwrap()
    }
    fn unchanged(&self) {
        assert_eq!(
            fs::read(self.project.join("project.sqlite")).unwrap(),
            self.database
        );
        assert_eq!(
            fs::read(self.project.join("project.json")).unwrap(),
            self.project_config
        );
        for name in [
            "project.sqlite-wal",
            "project.sqlite-shm",
            ".noetome-writing",
        ] {
            assert!(!self.project.join(name).exists());
        }
    }
}

fn read_body(stream: &mut TcpStream) -> Vec<u8> {
    // Windows accepted sockets may inherit a nonblocking listener's mode.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    let mut buffer = Vec::new();
    let header_end = loop {
        let mut block = [0; 1024];
        let count = stream.read(&mut block).unwrap();
        assert!(count > 0);
        buffer.extend_from_slice(&block[..count]);
        assert!(buffer.len() < 300 * 1024);
        if let Some(i) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    assert!(headers.starts_with("POST /v1/responses HTTP/1.1\r\n"));
    assert!(
        headers
            .to_ascii_lowercase()
            .contains(&format!("authorization: bearer {TEST_KEY}"))
    );
    let length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    while buffer.len() < header_end + length {
        let mut block = [0; 2048];
        let count = stream.read(&mut block).unwrap();
        assert!(count > 0);
        buffer.extend_from_slice(&block[..count]);
    }
    buffer[header_end..header_end + length].to_vec()
}
fn fake_response(body: &[u8]) -> Vec<u8> {
    let body: Value = serde_json::from_slice(body).unwrap();
    let input: Value = serde_json::from_str(body["input"].as_str().unwrap()).unwrap();
    let mut candidates: Value = serde_json::from_slice(
        &fs::read(repo().join("fixtures/extraction-gateway-v1/response.json")).unwrap(),
    )
    .unwrap();
    candidates.as_object_mut().unwrap().remove("producer");
    candidates["request_id"] = input["request_id"].clone();
    candidates["binding"] = input["binding"].clone();
    serde_json::to_vec(&json!({
        "id":"resp_noetome_test","object":"response","model":"noetome-test-model","status":"completed",
        "error":null,"incomplete_details":null,"service_tier":"default",
        "output":[{"type":"message","role":"assistant","status":"completed","id":"msg_noetome_test",
            "content":[{"type":"output_text","text":serde_json::to_string(&candidates).unwrap(),
                "annotations":[],"logprobs":[]}]}],
        "usage":{"input_tokens":100,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},
            "output_tokens":600,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":700}
    })).unwrap()
}

#[test]
fn ordinary_api_sends_one_structured_request_and_validates_author_candidate() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let observed = Arc::new(Mutex::new(None));
    let receipt = observed.clone();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            if let Ok((stream, _)) = listener.accept() {
                break stream;
            }
            if Instant::now() > deadline {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        };
        let body = read_body(&mut stream);
        let response = fake_response(&body);
        *receipt.lock().unwrap() = Some(body);
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",response.len()).unwrap();
        stream.write_all(&response).unwrap();
    });
    let example = Example::new(&endpoint, 1);
    let run = example.run();
    server.join().unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(report["status"], "candidates_validated");
    assert_eq!(report["producer"], "api");
    assert_eq!(report["transport_mode"], "local_http_test");
    assert_eq!(report["usage"]["network_attempts"], 1);
    assert_eq!(report["usage"]["real_model_calls"], 0);
    assert_eq!(report["usage"]["price_usd"], Value::Null);
    assert_eq!(report["knowledge_status"], "pending");
    assert_eq!(report["published"], false);
    assert_eq!(report["validated_atoms"], 1);
    let body = observed.lock().unwrap().take().unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["store"], false);
    assert_eq!(body["stream"], false);
    assert_eq!(body["background"], false);
    assert_eq!(body["tools"], json!([]));
    assert_eq!(body["text"]["format"]["type"], "json_schema");
    assert_eq!(body["text"]["format"]["strict"], true);
    assert_eq!(body["max_output_tokens"], 2048);
    let saved = fs::read(example.output.join("request-body.json")).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&saved).unwrap(), body);
    let candidates: Value =
        serde_json::from_slice(&fs::read(example.output.join("candidates.json")).unwrap()).unwrap();
    assert_eq!(
        candidates["payload"]["atoms"][0]["measurement"]["value"],
        "10"
    );
    assert_eq!(
        candidates["payload"]["atoms"][0]["measurement"]["unit"],
        "m/s"
    );
    assert_eq!(
        candidates["payload"]["atoms"][0]["measurement"]["conditions"],
        json!(["温度 20 ℃", "仅适用样例 A"])
    );
    for item in fs::read_dir(&example.output).unwrap() {
        let bytes = fs::read(item.unwrap().path()).unwrap();
        assert!(
            !bytes
                .windows(TEST_KEY.len())
                .any(|w| w == TEST_KEY.as_bytes())
        );
    }
    example.unchanged();
}

#[test]
fn ordinary_api_zero_budget_stops_before_any_http_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let example = Example::new(
        &format!("http://{}/v1/responses", listener.local_addr().unwrap()),
        0,
    );
    let run = example.run();
    assert_eq!(
        run.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(report["usage"]["network_attempts"], 0);
    assert_eq!(report["usage"]["model_usage"], Value::Null);
    assert_eq!(report["errors"][0]["code"], "EXTRACT_BUDGET_STOP");
    assert!(listener.accept().is_err());
    assert!(!example.output.join("candidates.json").exists());
    example.unchanged();
}

#[test]
fn ordinary_api_rate_limit_is_one_attempt_without_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let example = Example::new(
        &format!("http://{}/v1/responses", listener.local_addr().unwrap()),
        1,
    );
    let receipt = Arc::new(Mutex::new(0));
    let count = receipt.clone();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = read_body(&mut stream);
                *count.lock().unwrap() += 1;
                let bytes=b"{\"error\":{\"message\":\"local fake rate limit\",\"type\":\"rate_limit_error\"}}";
                write!(stream,"HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {}\r\nRetry-After: 1\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();
                stream.write_all(bytes).unwrap();
                thread::sleep(Duration::from_millis(100));
                assert!(listener.accept().is_err(), "unexpected automatic retry");
                return;
            }
            if Instant::now() > deadline {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    });
    let run = example.run();
    server.join().unwrap();
    assert_eq!(
        run.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(report["errors"][0]["code"], "EXTRACT_API_HTTP_RATE_LIMIT");
    assert_eq!(report["usage"]["network_attempts"], 1);
    assert_eq!(report["usage"]["model_usage"], Value::Null);
    assert_eq!(*receipt.lock().unwrap(), 1);
    assert!(!example.output.join("candidates.json").exists());
    assert!(example.output.join("response.bin").exists());
    assert_eq!(report["http_status"], 429);
    assert!(example.root.exists());
    example.unchanged();
}

#[test]
fn ordinary_selected_gateway_zero_budget_keeps_model_strategy_and_never_calls() {
    // This is a local preflight with a public synthetic key and an explicit zero
    // call budget. It must not send an HTTP request to the configured live URL.
    let example = Example::new("http://127.0.0.1:1234/v1/responses", 0);
    let mut config: Value = serde_json::from_slice(&fs::read(&example.config).unwrap()).unwrap();
    config["transport_mode"] = json!("live");
    config["endpoint"] = json!("https://ai.1343263.xyz/v1/responses");
    config["model"] = json!("deepseek-flash");
    fs::write(&example.config, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let run = example.run();
    assert_eq!(
        run.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(report["errors"][0]["code"], "EXTRACT_BUDGET_STOP");
    assert_eq!(report["configured_model"], "deepseek-flash");
    assert_eq!(report["actual_model"], Value::Null);
    assert_eq!(
        report["request_strategy"],
        "tangmao-deepseek-responses-inline-json-schema-v2"
    );
    assert_eq!(report["usage"]["network_attempts"], 0);
    assert_eq!(report["usage"]["real_model_calls"], 0);
    assert_eq!(report["usage"]["model_usage"], Value::Null);
    assert_eq!(report["knowledge_status"], "pending");
    assert_eq!(report["published"], false);
    let request: Value =
        serde_json::from_slice(&fs::read(example.output.join("request.json")).unwrap()).unwrap();
    assert_eq!(request["request_strategy"], report["request_strategy"]);
    let body: Value =
        serde_json::from_slice(&fs::read(example.output.join("request-body.json")).unwrap())
            .unwrap();
    assert_eq!(body["model"], "deepseek-flash");
    let inline: Value = serde_json::from_str(body["input"].as_str().unwrap()).unwrap();
    assert!(
        inline["task"]
            .as_str()
            .unwrap()
            .contains("Extract only evidence-supported")
    );
    assert_eq!(inline["response_schema"], body["text"]["format"]["schema"]);
    assert_eq!(inline["request"]["request_id"], request["request_id"]);
    assert_eq!(inline["request"]["binding"], request["binding"]);
    assert_eq!(inline["request"]["units"], request["units"]);
    assert_eq!(
        inline["request"]["evidence_anchors"][0]["quote"],
        request["units"][0]["text"]
    );
    assert_eq!(body["reasoning"]["effort"], "none");
    assert_eq!(body["text"]["format"]["type"], "json_schema");
    assert_eq!(body["text"]["format"]["name"], "noetome_candidates_v1");
    assert!(body["text"]["format"].get("strict").is_none());
    assert_eq!(
        body["text"]["format"]["schema"]["additionalProperties"],
        false
    );
    assert_eq!(body["stream"], false);
    assert_eq!(body["tools"], json!([]));
    assert_eq!(body["tool_choice"], "none");
    assert_eq!(body["store"], false);
    for unsupported in ["background", "service_tier", "truncation"] {
        assert!(body.get(unsupported).is_none(), "{unsupported}");
    }
    assert!(!example.output.join("candidates.json").exists());
    example.unchanged();
}

#[test]
fn ordinary_selected_gateway_rejects_other_authorities_models_and_paths() {
    let example = Example::new("http://127.0.0.1:1234/v1/responses", 0);
    let base: Value = serde_json::from_slice(&fs::read(&example.config).unwrap()).unwrap();
    for (endpoint, model) in [
        ("https://ai.1343263.xyz:443/v1/responses", "deepseek-flash"),
        ("http://ai.1343263.xyz/v1/responses", "deepseek-flash"),
        (
            "https://ai.1343263.xyz/v1/chat/completions",
            "deepseek-flash",
        ),
        ("https://ai.1343263.xyz/responses", "deepseek-flash"),
        ("https://ai.1343263.xyz/v1/responses?x=1", "deepseek-flash"),
        ("https://ai.1343263.xyz/v1/responses#x", "deepseek-flash"),
        ("https://user@ai.1343263.xyz/v1/responses", "deepseek-flash"),
        (
            "https://ai.1343263.xyz.other.example/v1/responses",
            "deepseek-flash",
        ),
        (
            "https://api.openai.com@ai.1343263.xyz/v1/responses",
            "deepseek-flash",
        ),
        ("https://127.0.0.1/v1/responses", "deepseek-flash"),
        ("https://other.example/v1/responses", "deepseek-flash"),
        ("https://ai.1343263.xyz/v1/responses", "deepseek-chat"),
        ("https://ai.1343263.xyz/v1/responses", "deepseek-flash "),
    ] {
        let mut config = base.clone();
        config["transport_mode"] = json!("live");
        config["endpoint"] = json!(endpoint);
        config["model"] = json!(model);
        fs::write(&example.config, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
        let run = example.run();
        assert_eq!(run.status.code(), Some(2), "{endpoint} / {model}");
        let diagnostic = String::from_utf8_lossy(&run.stderr);
        assert!(
            diagnostic.contains("EXTRACT_API_ENDPOINT")
                || diagnostic.contains("EXTRACT_API_CONFIG"),
            "{diagnostic}"
        );
        assert!(!example.output.exists());
        example.unchanged();
    }
}
