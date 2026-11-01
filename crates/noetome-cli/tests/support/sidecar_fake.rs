//! Private fake entry for the harness=false sidecar integration target.
//! Scenario selection comes from trusted test argv, never request content.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

pub const ENTRY: &str = "__noetome_sidecar_fake";
pub const UUID: &str = "11111111-1111-4111-8111-111111111111";
pub const SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
pub const DIAGNOSTIC: &[u8] = b"identity-fake: one synthetic byte request\n";
pub const LARGE_LEN: usize = 16 * 1024 * 1024;
pub const LARGE_SHA: &str = "a06c26cbac8b80704f420222dae5658b88ff2da96702d12ef7a4223e9361f7c1";
pub const UTF8_SHA: &str = "4a99557e4033c3539de2eb65472017cad5f9557f7a0625a09f1c3f6e2ba69c4c";
pub const REQUEST: &str = r#"{
  "schema_version":1,"protocol":"noetome-parser-single-v1",
  "request_id":"11111111-1111-4111-8111-111111111111","operation":"inspect_bytes",
  "input":{"relative_path":"input/raw.bin","byte_length":3,"sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"},
  "workspace":{"artifact_directory":"artifacts"},
  "artifact_policy":{"max_file_bytes":16777216,"max_total_bytes":67108864,"max_files":64}
}"#;
pub const RESPONSE: &str = r#"{
  "schema_version":1,"protocol":"noetome-parser-single-v1",
  "request_id":"11111111-1111-4111-8111-111111111111",
  "adapter":{"producer":"fake","adapter_id":"identity-fake","adapter_version":"1",
    "capabilities":{"locator_kinds":["input_bytes"],"artifact_kinds":["binary","utf8_text"]}},
  "input":{"relative_path":"input/raw.bin","byte_length":3,"sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"},
  "status":"complete","unit_count":2,
  "units":[
    {"unit_id":"U1","ordinal":0,"locator":{"kind":"input_bytes","start_byte":0,"end_byte":1},"status":"parsed","content":{"artifact_id":"A1","start_byte":0,"end_byte":1}},
    {"unit_id":"U2","ordinal":1,"locator":{"kind":"input_bytes","start_byte":1,"end_byte":3},"status":"parsed","content":{"artifact_id":"A1","start_byte":1,"end_byte":3}}
  ],
  "artifacts":[{"artifact_id":"A1","kind":"binary","relative_path":"artifacts/A1.bin","byte_length":3,"sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}],
  "issues":[]
}"#;

pub fn response(request_id: &str) -> Value {
    serde_json::from_str(&RESPONSE.replace(UUID, request_id)).unwrap()
}

pub fn main(arguments: &[OsString]) -> i32 {
    match execute(arguments) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("SIDECAR_TEST_HELPER_ERROR: {error}");
            2
        }
    }
}

fn execute(arguments: &[OsString]) -> Result<(), String> {
    if arguments.len() != 3 {
        return Err("expected trusted scenario, control directory and nonce arguments".into());
    }
    let scenario = arguments[0].to_str().ok_or("scenario is not UTF8")?;
    let control = PathBuf::from(&arguments[1]);
    let nonce = arguments[2].to_str().ok_or("nonce is not UTF8")?;
    uuid::Uuid::parse_str(nonce).map_err(|error| error.to_string())?;
    let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/sidecar-process-tests")
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let control = control.canonicalize().map_err(|error| error.to_string())?;
    if control == parent || !control.starts_with(&parent) {
        return Err("control directory is outside the owned test area".into());
    }
    let cwd = std::env::current_dir()
        .map_err(|error| error.to_string())?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if cwd == parent || !cwd.starts_with(&parent) {
        return Err("fake cwd is outside the owned test area".into());
    }
    let worker = scenario.starts_with("grandchild-worker");
    mark(
        &control,
        if worker {
            "grandchild.json"
        } else {
            "root.json"
        },
        nonce,
    )?;
    if worker {
        loop {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    if scenario == "stderr-first" {
        std::io::stderr()
            .write_all(&vec![b'd'; 200 * 1024])
            .map_err(|error| error.to_string())?;
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 64 * 1024 {
        return Err("test request exceeded its reviewed wire limit".into());
    }
    let request: Value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let request_id = request["request_id"].as_str().ok_or("request_id missing")?;
    uuid::Uuid::parse_str(request_id).map_err(|error| error.to_string())?;
    let expected_input = if scenario == "large_inventory_probe" {
        vec![b'x'; LARGE_LEN]
    } else if scenario.starts_with("utf8-") {
        "é".as_bytes().to_vec()
    } else {
        b"abc".to_vec()
    };
    if fs::read("input/raw.bin").map_err(|error| error.to_string())? != expected_input {
        return Err("input differs from the author-fixed scenario bytes".into());
    }
    let artifact_path = if scenario.starts_with("utf8-") {
        "artifacts/A1.txt"
    } else {
        "artifacts/A1.bin"
    };
    let mut artifact = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(artifact_path)
        .map_err(|error| error.to_string())?;
    artifact
        .write_all(&expected_input)
        .map_err(|error| error.to_string())?;
    artifact.sync_all().map_err(|error| error.to_string())?;
    mark(&control, "ready.json", nonce)?;
    if scenario.starts_with("grandchild-") {
        let role = if scenario == "grandchild-no-pipe" {
            "grandchild-worker-no-pipe"
        } else {
            "grandchild-worker"
        };
        let mut child = Command::new(std::env::current_exe().map_err(|error| error.to_string())?);
        child
            .args([ENTRY, role])
            .arg(&control)
            .arg(nonce)
            .stdin(Stdio::null());
        if scenario == "grandchild-no-pipe" {
            child.stdout(Stdio::null()).stderr(Stdio::null());
        } else {
            child.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        }
        let grandchild = child.spawn().map_err(|error| error.to_string())?;
        drop(grandchild);
        while !control.join("release").is_file() {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    emit(scenario, request_id, &control, nonce)
}

fn mark(control: &Path, name: &str, nonce: &str) -> Result<(), String> {
    let pid = std::process::id();
    let observer =
        noetome_sidecar_platform::ProcessObserver::open(pid).map_err(|error| error.to_string())?;
    let marker = json!({"nonce":nonce,"pid":pid,"creation_time":observer.creation_time()});
    fs::write(control.join(name), serde_json::to_vec(&marker).unwrap())
        .map_err(|error| error.to_string())?;
    drop(observer);
    Ok(())
}

fn emit(scenario: &str, request_id: &str, control: &Path, nonce: &str) -> Result<(), String> {
    match scenario {
        "silent" | "stderr-first" => {}
        "stderr-4096" => std::io::stderr()
            .write_all(&vec![b'd'; 4096])
            .map_err(|error| error.to_string())?,
        "stderr-4097" | "grandchild-stderr-limit" => std::io::stderr()
            .write_all(&vec![b'd'; 4097])
            .map_err(|error| error.to_string())?,
        "stderr-binary" => std::io::stderr()
            .write_all(&[0xff, 0x00, 0x80])
            .map_err(|error| error.to_string())?,
        _ => std::io::stderr()
            .write_all(DIAGNOSTIC)
            .map_err(|error| error.to_string())?,
    }
    let mut value = response(request_id);
    match scenario {
        "valid"
        | "silent"
        | "stderr-first"
        | "stderr-4096"
        | "stderr-4097"
        | "stderr-binary"
        | "stdout-4096"
        | "stdout-4097"
        | "empty"
        | "invalid-utf8"
        | "invalid-json"
        | "truncated"
        | "bom"
        | "extra-json"
        | "banner"
        | "extra-text"
        | "duplicate-key"
        | "duplicate-nested-key"
        | "exit-7"
        | "crash"
        | "hang"
        | "grandchild-hang"
        | "grandchild-root-exit"
        | "grandchild-no-pipe"
        | "grandchild-stdout-limit"
        | "grandchild-stderr-limit" => {}
        "large_inventory_probe" => {
            value["input"]["byte_length"] = json!(LARGE_LEN);
            value["input"]["sha256"] = json!(LARGE_SHA);
            value["unit_count"] = json!(1);
            value["units"].as_array_mut().unwrap().truncate(1);
            value["units"][0]["locator"]["end_byte"] = json!(LARGE_LEN);
            value["units"][0]["content"]["end_byte"] = json!(LARGE_LEN);
            value["artifacts"][0]["byte_length"] = json!(LARGE_LEN);
            value["artifacts"][0]["sha256"] = json!(LARGE_SHA);
        }
        "utf8-valid" | "utf8-boundary" => {
            value["input"]["byte_length"] = json!(2);
            value["input"]["sha256"] = json!(UTF8_SHA);
            value["artifacts"][0]["kind"] = json!("utf8_text");
            value["artifacts"][0]["relative_path"] = json!("artifacts/A1.txt");
            value["artifacts"][0]["byte_length"] = json!(2);
            value["artifacts"][0]["sha256"] = json!(UTF8_SHA);
            if scenario == "utf8-valid" {
                value["unit_count"] = json!(1);
                value["units"].as_array_mut().unwrap().truncate(1);
                value["units"][0]["locator"]["end_byte"] = json!(2);
                value["units"][0]["content"]["end_byte"] = json!(2);
            } else {
                value["units"][1]["locator"]["end_byte"] = json!(2);
                value["units"][1]["content"]["end_byte"] = json!(2);
            }
        }
        "two-resources" | "resource-hardlink" | "resource-case-collision" => {
            let mut other = value["artifacts"][0].clone();
            let (id, path) = if scenario == "resource-case-collision" {
                ("a1", "artifacts/a1.bin")
            } else {
                ("A2", "artifacts/A2.bin")
            };
            other["artifact_id"] = json!(id);
            other["relative_path"] = json!(path);
            value["artifacts"].as_array_mut().unwrap().push(other);
            value["units"][1]["content"]["artifact_id"] = json!(id);
            if scenario == "resource-hardlink" {
                fs::hard_link("artifacts/A1.bin", path).map_err(|error| error.to_string())?;
            } else if scenario == "two-resources" {
                fs::write(path, b"abc").map_err(|error| error.to_string())?;
            }
        }
        "resource-junction" => {
            fs::remove_file("artifacts/A1.bin").map_err(|error| error.to_string())?;
            let link = std::env::current_dir()
                .map_err(|error| error.to_string())?
                .join("artifacts/A1.bin");
            let dos = |path: &Path| {
                path.to_string_lossy()
                    .trim_start_matches(r"\\?\")
                    .to_owned()
            };
            let result = Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(dos(&link))
                .arg(dos(&control.join("external")))
                .output()
                .map_err(|error| error.to_string())?;
            if !result.status.success() {
                return Err(format!(
                    "owned junction creation failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                ));
            }
        }
        "unknown-field" => {
            value["unreviewed"] = json!(true);
        }
        "unknown-nested-field" => {
            value["adapter"]["unreviewed"] = json!(true);
        }
        // These lists are the author's declared schema order. JSON object key
        // ordering is unrelated and must not manufacture an invalid probe.
        "root-array" => {
            value = positional(
                &value,
                &[
                    "schema_version",
                    "protocol",
                    "request_id",
                    "adapter",
                    "input",
                    "status",
                    "unit_count",
                    "units",
                    "artifacts",
                    "issues",
                ],
            );
        }
        "adapter-array" => {
            value["adapter"] = positional(
                &value["adapter"],
                &["producer", "adapter_id", "adapter_version", "capabilities"],
            );
        }
        "capabilities-array" => {
            value["adapter"]["capabilities"] = positional(
                &value["adapter"]["capabilities"],
                &["locator_kinds", "artifact_kinds"],
            );
        }
        "input-array" => {
            value["input"] =
                positional(&value["input"], &["relative_path", "byte_length", "sha256"]);
        }
        "unit-array" => {
            value["units"][0] = positional(
                &value["units"][0],
                &["unit_id", "ordinal", "locator", "status", "content"],
            );
        }
        "locator-array" => {
            value["units"][0]["locator"] = positional(
                &value["units"][0]["locator"],
                &["kind", "start_byte", "end_byte"],
            );
        }
        "content-array" => {
            value["units"][0]["content"] = positional(
                &value["units"][0]["content"],
                &["artifact_id", "start_byte", "end_byte"],
            );
        }
        "artifact-array" => {
            value["artifacts"][0] = positional(
                &value["artifacts"][0],
                &[
                    "artifact_id",
                    "kind",
                    "relative_path",
                    "byte_length",
                    "sha256",
                ],
            );
        }
        "issue-array" => {
            value["status"] = json!("partial");
            value["units"][1]["status"] = json!("failed");
            value["units"][1]["content"] = Value::Null;
            let issue =
                json!({"code":"test_partial","message":"作者合成未完成。","unit_ids":["U2"]});
            value["issues"] = json!([positional(&issue, &["code", "message", "unit_ids"])]);
        }
        "status-tagged-map" => value["status"] = json!({"complete":null}),
        "producer-tagged-map" => value["adapter"]["producer"] = json!({"fake":null}),
        "unit-status-tagged-map" => value["units"][0]["status"] = json!({"parsed":null}),
        "locator-kind-tagged-map" => {
            value["units"][0]["locator"]["kind"] = json!({"input_bytes":null})
        }
        "artifact-kind-tagged-map" => value["artifacts"][0]["kind"] = json!({"binary":null}),
        "capability-locator-tagged-map" => {
            value["adapter"]["capabilities"]["locator_kinds"][0] = json!({"input_bytes":null})
        }
        "capability-artifact-tagged-map" => {
            value["adapter"]["capabilities"]["artifact_kinds"][0] = json!({"binary":null})
        }
        "missing-nullable" => {
            value.as_object_mut().unwrap().remove("unit_count");
        }
        "future-version" => {
            value["schema_version"] = json!(2);
        }
        "wrong-protocol" => {
            value["protocol"] = json!("unreviewed-protocol");
        }
        "unknown-status" => {
            value["status"] = json!("published");
        }
        "wrong-request-id" => {
            value["request_id"] = json!("22222222-2222-4222-8222-222222222222");
        }
        "wrong-input-sha" => {
            value["input"]["sha256"] = json!("0".repeat(64));
        }
        "wrong-input-length" => {
            value["input"]["byte_length"] = json!(2);
        }
        "wrong-producer" => {
            value["adapter"]["producer"] = json!("parser");
        }
        "wrong-adapter-version" => {
            value["adapter"]["adapter_version"] = json!("future");
        }
        "wrong-capabilities" => {
            value["adapter"]["capabilities"]["artifact_kinds"] = json!([]);
        }
        "duplicate-ordinal" => {
            value["units"][1]["ordinal"] = json!(0);
        }
        "missing-ordinal" => {
            value["units"][1]["ordinal"] = json!(2);
        }
        "missing-unit-refreshed-count" => {
            value["units"].as_array_mut().unwrap().pop();
            value["unit_count"] = json!(1);
        }
        "empty-complete" => {
            value["units"] = json!([]);
            value["unit_count"] = json!(0);
            value["artifacts"] = json!([]);
            fs::remove_file("artifacts/A1.bin").map_err(|error| error.to_string())?;
        }
        "empty-range" => {
            value["units"][0]["locator"]["end_byte"] = json!(0);
        }
        "overlap-range" => {
            value["units"][1]["locator"]["start_byte"] = json!(0);
        }
        "outside-range" => {
            value["units"][1]["locator"]["end_byte"] = json!(4);
        }
        "missing-content" => {
            value["units"][0]["content"] = Value::Null;
        }
        "partial" | "malformed-partial" | "partial-two-issues" => {
            value["status"] = json!("partial");
            value["units"][1]["status"] = json!("failed");
            value["units"][1]["content"] = Value::Null;
            value["issues"] =
                json!([{"code":"test_partial","message":"作者合成单元未完成。","unit_ids":["U2"]}]);
            if scenario == "malformed-partial" {
                value["issues"][0]["unit_ids"] = json!(["ghost"]);
            }
            if scenario == "partial-two-issues" {
                value["issues"].as_array_mut().unwrap().push(json!({
                    "code":"test_second", "message":"作者合成第二个问题。", "unit_ids":["U2"]
                }));
            }
        }
        "reported-failure" => {
            value["status"] = json!("failed");
            value["unit_count"] = Value::Null;
            value["units"] = json!([]);
            value["artifacts"] = json!([]);
            value["issues"] =
                json!([{"code":"test_failed","message":"作者合成解析失败。","unit_ids":[]}]);
            fs::remove_file("artifacts/A1.bin").map_err(|error| error.to_string())?;
        }
        "resource-sha" => {
            value["artifacts"][0]["sha256"] = json!("0".repeat(64));
        }
        "resource-length" => {
            value["artifacts"][0]["byte_length"] = json!(2);
        }
        "resource-bytes" => {
            fs::write("artifacts/A1.bin", b"abd").map_err(|error| error.to_string())?;
        }
        "resource-missing" => {
            fs::remove_file("artifacts/A1.bin").map_err(|error| error.to_string())?;
        }
        "resource-extra-file" => {
            fs::write("artifacts/unknown.bin", b"author extra")
                .map_err(|error| error.to_string())?;
        }
        "resource-extra-directory" => {
            fs::create_dir("artifacts/unknown").map_err(|error| error.to_string())?;
        }
        "root-extra-file" => {
            fs::write("unknown.md", b"author extra").map_err(|error| error.to_string())?;
        }
        "resource-parent" => {
            value["artifacts"][0]["relative_path"] = json!("../external-sentinel.md");
        }
        "resource-absolute" => {
            value["artifacts"][0]["relative_path"] =
                json!(control.join("root.json").to_string_lossy());
        }
        "resource-backslash" => {
            value["artifacts"][0]["relative_path"] = json!("artifacts\\A1.bin");
        }
        "resource-subdirectory" => {
            value["artifacts"][0]["relative_path"] = json!("artifacts/sub/A1.bin");
        }
        "resource-reserved-name" => {
            value["artifacts"][0]["artifact_id"] = json!("CON");
            value["artifacts"][0]["relative_path"] = json!("artifacts/CON.bin");
            value["units"][0]["content"]["artifact_id"] = json!("CON");
            value["units"][1]["content"]["artifact_id"] = json!("CON");
        }
        "resource-tail-dot" => {
            value["artifacts"][0]["relative_path"] = json!("artifacts/A1.bin.");
        }
        "resource-stream" => {
            value["artifacts"][0]["relative_path"] = json!("artifacts/A1.bin:stream");
        }
        _ => return Err(format!("unreviewed scenario {scenario}")),
    }
    let mut wire = serde_json::to_vec(&value).unwrap();
    wire.push(b'\n');
    match scenario {
        "stdout-4096" | "stdout-4097" | "grandchild-stdout-limit" => {
            wire.resize(
                if scenario == "stdout-4096" {
                    4096
                } else {
                    4097
                },
                b' ',
            );
        }
        "empty" => wire.clear(),
        "invalid-utf8" => wire = vec![0xff],
        "invalid-json" => wire = b"{ invalid".to_vec(),
        "truncated" => wire.truncate(wire.len() / 2),
        "bom" => {
            wire.splice(0..0, [0xef, 0xbb, 0xbf]);
        }
        "extra-json" => {
            wire.extend_from_slice(&serde_json::to_vec(&value).unwrap());
        }
        "banner" => {
            wire.splice(0..0, b"test helper banner\n".iter().copied());
        }
        "extra-text" => wire.extend_from_slice(b"unreviewed trailing text"),
        "duplicate-key" => {
            wire = String::from_utf8(wire)
                .unwrap()
                .replacen("\"unit_count\":2", "\"unit_count\":2,\"unit_count\":1", 1)
                .into_bytes();
        }
        "duplicate-nested-key" => {
            wire = String::from_utf8(wire)
                .unwrap()
                .replacen(
                    "\"adapter_version\":\"1\"",
                    "\"adapter_version\":\"1\",\"adapter_version\":\"1\"",
                    1,
                )
                .into_bytes();
        }
        _ => {}
    }
    if scenario == "crash" {
        // Only this intentional crash helper changes its process policy.
        // A configuration failure prevents the before-abort handshake and is
        // never counted as a successful native crash classification.
        noetome_sidecar_platform::configure_crash_test_process()
            .map_err(|error| error.to_string())?;
        mark(control, "before-abort.json", nonce)?;
        while !control.join("abort-release").is_file() {
            std::thread::sleep(Duration::from_millis(1));
        }
        std::process::abort();
    }
    if scenario == "hang" || scenario == "grandchild-hang" {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    let mut output = std::io::stdout().lock();
    output.write_all(&wire).map_err(|error| error.to_string())?;
    output.flush().map_err(|error| error.to_string())?;
    if scenario == "exit-7" {
        std::process::exit(7);
    }
    Ok(())
}

fn positional(value: &Value, fields: &[&str]) -> Value {
    let object = value.as_object().expect("author-declared DTO object");
    assert_eq!(object.len(), fields.len(), "author DTO fields changed");
    Value::Array(fields.iter().map(|name| object[*name].clone()).collect())
}
