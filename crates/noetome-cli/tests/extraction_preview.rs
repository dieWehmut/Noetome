//! Ordinary CLI and real local HTTP transport; model data is explicitly fake.
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
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
    read_body_path(stream, "/v1/responses")
}
fn read_body_path(stream: &mut TcpStream, expected_path: &str) -> Vec<u8> {
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
    assert!(headers.starts_with(&format!("POST {expected_path} HTTP/1.1\r\n")));
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

fn successful_api() -> Example {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            if let Ok((stream, _)) = listener.accept() {
                break stream;
            }
            assert!(
                Instant::now() < deadline,
                "CLI never sent the one authorized request"
            );
            thread::sleep(Duration::from_millis(5));
        };
        let body = read_body(&mut stream);
        let response = fake_response(&body);
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",response.len()).unwrap();
        stream.write_all(&response).unwrap();
    });
    let example = Example::new(&endpoint, 1);
    let mut config: Value = serde_json::from_slice(&fs::read(&example.config).unwrap()).unwrap();
    config["limits"]["timeout_ms"] = json!(10000);
    fs::write(&example.config, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let run = example.run();
    server.join().unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    example
}
fn preview(example: &Example, destination: &Path) -> Output {
    cli(&[
        "preview",
        "export",
        example.project.to_str().unwrap(),
        example.imported["import_id"].as_str().unwrap(),
        example.output.to_str().unwrap(),
        destination.to_str().unwrap(),
    ])
}
fn assert_refused(output: Output, destination: &Path, code: &str) {
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !destination.exists(),
        "refusal wrote output before preflight"
    );
}
fn snapshot_tree(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    let mut todo = vec![root.to_owned()];
    while let Some(path) = todo.pop() {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                todo.push(path)
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    out
}
#[test]
fn cli_choices_units_and_one_saved_api_export_preserve_truth_and_original_source() {
    let example = successful_api();
    let before = snapshot_tree(&example.project);
    let id = example.imported["import_id"].as_str().unwrap();
    let run = cli(&["preview", "choices", example.project.to_str().unwrap(), id]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let choices: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(choices["filename"], "source.txt");
    assert_eq!(
        choices["choices"][0]["selection"],
        json!({"kind":"text","ordinal":0})
    );
    assert_eq!(choices["choices"][0]["selectable"], true);
    let selection = example.root.join("selection.json");
    fs::write(&selection, b"[{\"kind\":\"text\",\"ordinal\":0}]").unwrap();
    let run = cli(&[
        "preview",
        "units",
        example.project.to_str().unwrap(),
        id,
        selection.to_str().unwrap(),
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let units: Value = serde_json::from_slice(&run.stdout).unwrap();
    let request: Value =
        serde_json::from_slice(&fs::read(example.output.join("request.json")).unwrap()).unwrap();
    assert_eq!(units["binding"], choices["binding"]);
    assert_eq!(units["units"], request["units"]);
    let destination = example.root.join("new pending notes");
    let run = preview(&example, &destination);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let exported: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(exported["producer"], "extraction_preview");
    assert_eq!(exported["status"], "pending");
    assert_eq!(
        exported["counts"],
        json!({"concepts":1,"atoms":1,"evidence":1})
    );
    eprintln!("PREVIEW_TEST_OUTPUT {}", destination.display());
    let vault = destination.join("vault");
    assert_eq!(
        fs::read(vault.join("attachments/source-0001.txt")).unwrap(),
        fs::read(repo().join("fixtures/extraction-gateway-v1/source.txt")).unwrap()
    );
    let overview = fs::read_to_string(vault.join("待核验笔记.md")).unwrap();
    assert_eq!(overview, exported["markdown"].as_str().unwrap());
    assert!(overview.contains("本地 HTTP 假服务"));
    assert!(overview.contains("scientific_quality：not_run"));
    assert!(overview.contains(units["units"][0]["text"].as_str().unwrap()));
    let note = fs::read_to_string(vault.join("concept-0001.md")).unwrap();
    for text in [
        "m/s",
        "温度 20 ℃",
        "仅适用样例 A",
        "数值：10",
        "relation：supports",
        "value_status：known",
        "required：false",
        "unverified",
    ] {
        assert!(note.contains(text), "missing {text}: {note}");
    }
    let evidence = fs::read_to_string(vault.join("evidence-0001.md")).unwrap();
    assert!(evidence.contains("[0, 94)"));
    assert!(evidence.contains(units["units"][0]["text"].as_str().unwrap()));
    for text in [&overview, &note, &evidence] {
        for link in text.split("[[").skip(1) {
            let name = link
                .split("]]")
                .next()
                .unwrap()
                .split('|')
                .next()
                .unwrap()
                .split('#')
                .next()
                .unwrap();
            let path = vault.join(name);
            assert!(
                path.is_file() || path.with_extension("md").is_file(),
                "broken actual-file link {name}"
            );
        }
    }
    let files = snapshot_tree(&vault);
    assert_eq!(files.len(), 4);
    assert!(
        files
            .keys()
            .all(|p| p.extension().is_some_and(|ext| ext == "md" || ext == "txt"))
    );
    let audit: Value =
        serde_json::from_slice(&fs::read(destination.join("reports/preview.json")).unwrap())
            .unwrap();
    assert_eq!(audit["units"], units["units"]);
    assert_eq!(audit["published"], false);
    assert_eq!(audit["scientific_quality"], "not_run");
    assert_eq!(snapshot_tree(&example.project), before);
    example.unchanged();
    let unchanged = snapshot_tree(&destination);
    assert!(!preview(&example, &destination).status.success());
    assert_eq!(snapshot_tree(&destination), unchanged);
}
#[test]
fn existing_vault_source_and_api_overlap_refuse_without_any_writes() {
    let example = successful_api();
    let vault = example.root.join("existing vault");
    fs::create_dir(&vault).unwrap();
    fs::create_dir(vault.join(".obsidian")).unwrap();
    fs::write(vault.join("manual.md"), "我的人工资料").unwrap();
    let before = snapshot_tree(&vault);
    let destination = vault.join("new test");
    assert_refused(
        preview(&example, &destination),
        &destination,
        "PREVIEW_EXISTING_VAULT",
    );
    assert_eq!(snapshot_tree(&vault), before);
    for root in [&example.project, &example.output] {
        let before = snapshot_tree(root);
        let destination = root.join("new test");
        assert_refused(
            preview(&example, &destination),
            &destination,
            "PREVIEW_OUTPUT_OVERLAP",
        );
        assert_eq!(snapshot_tree(root), before);
    }
    example.unchanged();
}
fn replace_report(example: &Example, report: &Value) {
    fs::write(
        example.output.join("validation.json"),
        serde_json::to_vec_pretty(report).unwrap(),
    )
    .unwrap();
    let md = fs::read_to_string(example.output.join("validation.md")).unwrap();
    let head = md.rsplit_once("```json\n").unwrap().0;
    fs::write(
        example.output.join("validation.md"),
        format!(
            "{head}```json\n{}\n```\n",
            serde_json::to_string_pretty(report).unwrap()
        ),
    )
    .unwrap();
}
fn pin(bytes: &[u8]) -> Value {
    use sha2::{Digest, Sha256};
    json!({"sha256":format!("{:x}",Sha256::digest(bytes)),"byte_length":bytes.len()})
}
#[test]
fn failures_tampered_receipts_and_rebound_candidates_do_not_become_exportable() {
    let example = successful_api();
    let original = snapshot_tree(&example.output);
    let mut report: Value =
        serde_json::from_slice(&original[Path::new("validation.json")]).unwrap();
    report["status"] = json!("failed");
    replace_report(&example, &report);
    let destination = example.root.join("failed-export");
    assert_refused(
        preview(&example, &destination),
        &destination,
        "PREVIEW_INVALID_API",
    );
    for (path, bytes) in &original {
        fs::write(example.output.join(path), bytes).unwrap();
    }
    let mut candidates: Value =
        serde_json::from_slice(&original[Path::new("candidates.json")]).unwrap();
    candidates["payload"]["atoms"][0]["measurement"]["value"] = json!("999");
    let bytes = serde_json::to_vec_pretty(&candidates).unwrap();
    fs::write(example.output.join("candidates.json"), &bytes).unwrap();
    let mut report: Value =
        serde_json::from_slice(&original[Path::new("validation.json")]).unwrap();
    report["artifacts"]["candidates.json"] = pin(&bytes);
    replace_report(&example, &report);
    let destination = example.root.join("candidate-tamper");
    assert_refused(
        preview(&example, &destination),
        &destination,
        "PREVIEW_INVALID_API",
    );
    for (path, bytes) in &original {
        fs::write(example.output.join(path), bytes).unwrap();
    }
    let mut request: Value = serde_json::from_slice(&original[Path::new("request.json")]).unwrap();
    request["units"][0]["text"] = json!("伪造来源正文");
    let bytes = serde_json::to_vec_pretty(&request).unwrap();
    fs::write(example.output.join("request.json"), &bytes).unwrap();
    let mut report: Value =
        serde_json::from_slice(&original[Path::new("validation.json")]).unwrap();
    report["artifacts"]["request.json"] = pin(&bytes);
    replace_report(&example, &report);
    let destination = example.root.join("request-tamper");
    assert_refused(
        preview(&example, &destination),
        &destination,
        "PREVIEW_INVALID_API",
    );
    for (path, bytes) in &original {
        fs::write(example.output.join(path), bytes).unwrap();
    }
    fs::write(example.output.join("response.bin"), b"{}").unwrap();
    let destination = example.root.join("response-tamper");
    assert_refused(
        preview(&example, &destination),
        &destination,
        "PREVIEW_INVALID_API",
    );
    example.unchanged();
}
#[test]
fn strict_selection_rejects_duplicates_unknown_locator_and_sixty_five_units() {
    let example = Example::new("http://127.0.0.1:1234/v1/responses", 1);
    let id = example.imported["import_id"].as_str().unwrap();
    let selection = example.root.join("selection.json");
    for bytes in [
        "[{\"kind\":\"text\",\"ordinal\":0,\"ordinal\":1}]",
        "[{\"kind\":\"text\",\"ordinal\":0,\"unknown\":true}]",
        "[{\"kind\":\"pdf\",\"page_index\":0,\"region_id\":\"invented-whole-page\"}]",
        "[]",
    ] {
        fs::write(&selection, bytes).unwrap();
        let run = cli(&[
            "preview",
            "units",
            example.project.to_str().unwrap(),
            id,
            selection.to_str().unwrap(),
        ]);
        assert!(!run.status.success(), "accepted {bytes}");
    }
    let value = json!(
        (0..65)
            .map(|ordinal| json!({"kind":"text","ordinal" :ordinal}))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        noetome_cli::extraction_preview::selected_units(&example.project, id, &value)
            .unwrap_err()
            .code(),
        "EXTRACT_INPUT_LIMIT"
    );
    example.unchanged();
}
fn import_text(example: &Example, filename: &str, text: &str) -> Value {
    let file = example.root.join(filename);
    fs::write(&file, text).unwrap();
    let run = cli(&[
        "source",
        "import",
        example.project.to_str().unwrap(),
        file.to_str().unwrap(),
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    serde_json::from_slice(&run.stdout).unwrap()
}
#[test]
fn sixty_four_real_text_spans_fit_and_input_over_sixty_four_kib_is_refused() {
    let example = Example::new("http://127.0.0.1:1234/v1/responses", 1);
    let text = (0..64)
        .map(|i| format!("自造上手材料第 {} 段，明确这是测试资料。", i + 1))
        .collect::<Vec<_>>()
        .join("\n\n");
    let imported = import_text(&example, "many.txt", &text);
    let id = imported["import_id"].as_str().unwrap();
    let choices = noetome_cli::extraction_preview::choices(&example.project, id).unwrap();
    let selectable: Vec<_> = choices["choices"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["selectable"] == true)
        .collect();
    assert_eq!(selectable.len(), 64);
    assert!(
        choices["choices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["selectable"] == false)
    );
    let selection = json!(
        selectable
            .iter()
            .map(|item| item["selection"].clone())
            .collect::<Vec<_>>()
    );
    let units =
        noetome_cli::extraction_preview::selected_units(&example.project, id, &selection).unwrap();
    assert_eq!(units["units"].as_array().unwrap().len(), 64);
    assert!(
        units["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u["text"].as_str().unwrap().len())
            .sum::<usize>()
            < 65536
    );
    let imported = import_text(&example, "too-large.txt", &"a".repeat(65537));
    let result = noetome_cli::extraction_preview::selected_units(
        &example.project,
        imported["import_id"].as_str().unwrap(),
        &json!([{"kind":"text","ordinal":0}]),
    );
    assert_eq!(result.unwrap_err().code(), "EXTRACT_INPUT_LIMIT");
}
#[test]
fn a_marker_file_and_selected_ancestor_link_cannot_bypass_existing_vault_check() {
    let example = Example::new("http://127.0.0.1:1234/v1/responses", 1);
    let vault = example.root.join("vault-marker-file");
    fs::create_dir(&vault).unwrap();
    fs::write(vault.join(".obsidian"), b"marker").unwrap();
    assert_eq!(
        noetome_cli::extraction_preview::ensure_outside_vault(&vault.join("child"))
            .unwrap_err()
            .code(),
        "PREVIEW_EXISTING_VAULT"
    );
    let outside = example.root.join("outside");
    fs::create_dir(&outside).unwrap();
    noetome_cli::extraction_preview::ensure_outside_vault(&outside.join("child")).unwrap();
    #[cfg(unix)]
    {
        let alias = example.root.join("vault-alias");
        std::os::unix::fs::symlink(&vault, &alias).unwrap();
        assert!(
            noetome_cli::extraction_preview::ensure_outside_vault(&alias.join("child")).is_err()
        );
    }
    example.unchanged();
}
#[test]
fn selected_source_capability_survives_an_independent_cli_read() {
    let example = Example::new("http://127.0.0.1:1234/v1/responses", 1);
    let id = example.imported["import_id"].as_str().unwrap();
    let selection = json!([{"kind":"text","ordinal":0}]);
    let held =
        noetome_cli::extraction_preview::hold_selected_units(&example.project, id, &selection)
            .unwrap();
    let file = example.root.join("held-selection.json");
    fs::write(&file, serde_json::to_vec(&selection).unwrap()).unwrap();
    let run = cli(&[
        "preview",
        "units",
        example.project.to_str().unwrap(),
        id,
        file.to_str().unwrap(),
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&run.stdout).unwrap(),
        held.value()
    );
    held.verify().unwrap();
    example.unchanged();
}
#[test]
fn selected_source_capability_blocks_or_detects_changed_source_bytes() {
    let example = Example::new("http://127.0.0.1:1234/v1/responses", 1);
    let id = example.imported["import_id"].as_str().unwrap();
    let held = noetome_cli::extraction_preview::hold_selected_units(
        &example.project,
        id,
        &json!([{"kind":"text","ordinal":0}]),
    )
    .unwrap();
    let path = example.project.join(
        example.imported["version"]["normalized_snapshot"]
            .as_str()
            .unwrap(),
    );
    let original = fs::read(&path).unwrap();
    #[cfg(windows)]
    {
        assert!(
            fs::OpenOptions::new().write(true).open(&path).is_err(),
            "real held source handle permitted write"
        );
        assert!(
            fs::remove_file(&path).is_err(),
            "real held source handle permitted delete"
        );
        held.verify().unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    #[cfg(not(windows))]
    {
        fs::write(&path, b"changed while preview source was held").unwrap();
        assert_eq!(held.verify().unwrap_err().code(), "EXTRACT_SOURCE_CHANGED");
        drop(held);
        fs::write(&path, original).unwrap();
    }
}
#[test]
fn unknown_measurement_keeps_null_value_and_unit_and_explicit_conditions() {
    // Author-written data fixed in this test, served by a public local fake HTTP endpoint.
    const SOURCE: &str =
        "本自造 HTTP 测试材料：目标 B 的速度未知，未报告单位；条件为温度 20 ℃、仅用于测试。";
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/responses", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            if let Ok((stream, _)) = listener.accept() {
                break stream;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        };
        let body: Value = serde_json::from_slice(&read_body(&mut stream)).unwrap();
        let input: Value = serde_json::from_str(body["input"].as_str().unwrap()).unwrap();
        let unit = &input["units"][0];
        assert_eq!(unit["text"], SOURCE);
        let candidates = json!({"schema_version":1,"request_id":input["request_id"],"binding":input["binding"],"concepts":[{"concept_id":"concept-unknown","revision":1,"canonical_name":"目标 B 速度","aliases":[],"needs_full_explanation":true}],
            "evidence":[{"evidence_id":"evidence-unknown","unit_id":unit["unit_id"],"excerpt_start_byte":unit["start_byte"],"excerpt_end_byte":unit["end_byte"],"excerpt_sha256":pin(SOURCE.as_bytes())["sha256"],"quote":SOURCE}],
            "atoms":[{"atom":{"atom_id":"atom-unknown","revision":1,"kind":"comparison_value","concept_ids":["concept-unknown"],"content":"目标 B 的速度未知，未报告单位。","qualifiers":["温度 20 ℃","仅用于测试"],"evidence_links":[{"evidence_id":"evidence-unknown","relation":"supports"}],"required":false,"verification":"unverified","value_status":"unknown"},"measurement":{"value_status":"unknown","value":null,"unit":null,"conditions":["温度 20 ℃","仅用于测试"]}}]});
        let response=serde_json::to_vec(&json!({"id":"resp_noetome_unknown","object":"response","model":"noetome-test-model","status":"completed","error":null,"incomplete_details":null,"service_tier":"default","output":[{"type":"message","role":"assistant","status":"completed","id":"msg_noetome_unknown","content":[{"type":"output_text","text":serde_json::to_string(&candidates).unwrap(),"annotations":[],"logprobs":[]}]}],"usage":{"input_tokens":100,"input_tokens_details":{"cached_tokens":0},"output_tokens":600,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":700}})).unwrap();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",response.len()).unwrap();
        stream.write_all(&response).unwrap();
    });
    let mut example = Example::new(&endpoint, 1);
    example.imported = import_text(&example, "unknown.txt", SOURCE);
    example.database = fs::read(example.project.join("project.sqlite")).unwrap();
    let mut config: Value = serde_json::from_slice(&fs::read(&example.config).unwrap()).unwrap();
    config["limits"]["timeout_ms"] = json!(10000);
    fs::write(&example.config, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let run = example.run();
    server.join().unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let destination = example.root.join("unknown preview");
    let run = preview(&example, &destination);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    eprintln!("PREVIEW_TEST_OUTPUT {}", destination.display());
    let note = fs::read_to_string(destination.join("vault/concept-0001.md")).unwrap();
    for text in [
        "value_status：unknown",
        "数值状态：unknown",
        "数值：未知 / 不适用（null）",
        "单位：未知 / 不适用（null）",
        "温度 20 ℃",
        "仅用于测试",
    ] {
        assert!(note.contains(text), "{text}: {note}");
    }
    let audit: Value =
        serde_json::from_slice(&fs::read(destination.join("reports/candidates.json")).unwrap())
            .unwrap();
    assert!(audit["payload"]["atoms"][0]["measurement"]["value"].is_null());
    assert!(audit["payload"]["atoms"][0]["measurement"]["unit"].is_null());
    example.unchanged();
}
#[test]
fn saved_chat_candidates_export_after_one_http_request_without_repeating_model_call() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            if let Ok((stream, _)) = listener.accept() {
                break stream;
            }
            assert!(Instant::now() < deadline, "no Chat request observed");
            thread::sleep(Duration::from_millis(5));
        };
        let body: Value =
            serde_json::from_slice(&read_body_path(&mut stream, "/v1/chat/completions")).unwrap();
        let input: Value = serde_json::from_str(
            body["messages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["role"] == "user")
                .unwrap()["content"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let request = &input["request"];
        assert!(request["units"][0]["text"].is_string());
        assert!(input["response_schema"].is_object());
        let mut candidates: Value = serde_json::from_slice(
            &fs::read(repo().join("fixtures/extraction-gateway-v1/response.json")).unwrap(),
        )
        .unwrap();
        candidates.as_object_mut().unwrap().remove("producer");
        candidates["request_id"] = request["request_id"].clone();
        candidates["binding"] = request["binding"].clone();
        let response=serde_json::to_vec(&json!({"id":"chatcmpl-preview-test","object":"chat.completion","model":"noetome-test-model","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":serde_json::to_string(&candidates).unwrap(),"refusal":null}}],"usage":{"prompt_tokens":100,"prompt_tokens_details":{"cached_tokens":0},"completion_tokens":600,"completion_tokens_details":{"reasoning_tokens":0},"total_tokens":700}})).unwrap();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",response.len()).unwrap();
        stream.write_all(&response).unwrap();
        drop(stream);
        // Finish the only server before export. Any extra HTTP has no responder.
        assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    });
    let example = Example::new(&endpoint, 1);
    let mut config: Value = serde_json::from_slice(&fs::read(&example.config).unwrap()).unwrap();
    config["provider"] = json!("openai_chat_completions");
    config["limits"]["timeout_ms"] = json!(10000);
    fs::write(&example.config, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let run = example.run();
    server.join().unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(report["status"], "candidates_validated");
    assert_eq!(
        report["request_strategy"],
        "tangmao-deepseek-chat-positive-revision-v6"
    );
    assert_eq!(report["usage"]["network_attempts"], 1);
    let before = snapshot_tree(&example.output);
    let destination = example.root.join("chat saved preview");
    let run = preview(&example, &destination);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let exported: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(
        exported["counts"],
        json!({"concepts":1,"atoms":1,"evidence":1})
    );
    assert_eq!(exported["status"], "pending");
    assert_eq!(snapshot_tree(&example.output), before);
    example.unchanged();
    let repeated = example.root.join("chat recovered again");
    let run = preview(&example, &repeated);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(snapshot_tree(&example.output), before);
    example.unchanged();
}
