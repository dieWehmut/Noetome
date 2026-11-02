//! Real CLI and authoring-oracle checks, never synthetic PDF extraction.
//! Set NOETOME_PDF_PYTHON to the absolute locked interpreter before running.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("noetome-pdf-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        fs::write(
            path.join("external-sentinel.txt"),
            b"outside PDF output - preserve bytes",
        )
        .unwrap();
        Self(path)
    }
    fn sentinel(&self) {
        assert_eq!(
            fs::read(self.0.join("external-sentinel.txt")).unwrap(),
            b"outside PDF output - preserve bytes"
        );
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        if let Ok(path) = fs::canonicalize(&self.0) {
            assert!(path.starts_with(base));
            assert!(
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("noetome-pdf-test-")
            );
            let _ = fs::remove_dir_all(path);
        }
    }
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/pdf-text-v1")
        .join(name)
}
fn cli(args: &[&str], paths: &[&Path]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_noetome"));
    command.args(args);
    for path in paths {
        command.arg(path);
    }
    command.output().unwrap()
}
fn json_output(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "invalid JSON {error}; stderr={}; stdout={}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        )
    })
}
fn sha(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn check_authoring(root: &Path, expected: &Value) {
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("parser/pdf-manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["input_fingerprint"]["sha256"],
        expected["raw_sha256"]
    );
    assert_eq!(manifest["page_count"], expected["parse_page_count"]);
    assert_eq!(manifest["status"], expected["status"]);
    let actual_pages = manifest["pages"].as_array().unwrap();
    let expected_pages = expected["pages"].as_array().unwrap();
    assert_eq!(
        actual_pages.len(),
        expected_pages.len(),
        "page registry must be bidirectional"
    );
    for (actual, want) in actual_pages.iter().zip(expected_pages) {
        for field in [
            "physical_page_index",
            "display_page_number",
            "printed_page_label",
            "status",
        ] {
            assert_eq!(actual[field], want[field], "{field}");
        }
        if actual["status"] == "failed" {
            continue;
        }
        for field in ["media_box", "crop_box"] {
            for axis in 0..4 {
                assert!(
                    (actual["geometry"][field][axis].as_f64().unwrap()
                        - want[field][axis].as_f64().unwrap())
                    .abs()
                        < 0.000001
                );
            }
        }
        assert_eq!(
            actual["geometry"]["rotation_degrees"],
            want["rotation_degrees"]
        );
        let expected_lines = want["lines"].as_array().unwrap();
        if expected_lines.is_empty() {
            continue;
        }
        let text_path = root.join("parser").join(format!(
            "pages/{:06}.txt",
            want["physical_page_index"].as_u64().unwrap()
        ));
        let text_bytes = fs::read(&text_path).unwrap();
        let want_text: String = expected_lines
            .iter()
            .map(|line| format!("{}\n", line["text"].as_str().unwrap()))
            .collect();
        assert_eq!(
            text_bytes,
            want_text.as_bytes(),
            "full body and reading order at {}",
            text_path.display()
        );
        assert_eq!(actual["text"]["sha256"], sha(&text_bytes));
        let regions = actual["regions"].as_array().unwrap();
        assert_eq!(regions.len(), expected_lines.len());
        for (region, line) in regions.iter().zip(expected_lines) {
            assert_eq!(region["locator_kind"], "pdf_page_region");
            assert_eq!(region["physical_page_index"], want["physical_page_index"]);
            let start = region["text"]["start_byte"].as_u64().unwrap() as usize;
            let end = region["text"]["end_byte"].as_u64().unwrap() as usize;
            assert_eq!(
                &text_bytes[start..end],
                line["text"].as_str().unwrap().as_bytes()
            );
            let tolerance = line["tolerance_pt"].as_f64().unwrap();
            for index in 0..4 {
                assert!(
                    (region["bbox"][index].as_f64().unwrap()
                        - line["bbox"][index].as_f64().unwrap())
                    .abs()
                        <= tolerance,
                    "actual position differs from independent authoring: {region} vs {line}"
                );
            }
        }
    }
    assert!(!root.join("project.sqlite").exists());
    assert!(!root.join("vault").exists());
}

#[test]
fn real_cli_parses_fixed_single_and_multiple_pages_with_actual_positions() {
    let scratch = Scratch::new();
    let expected: Value =
        serde_json::from_slice(&fs::read(fixture("expected.json")).unwrap()).unwrap();
    for name in [
        "single-page.pdf",
        "multiple-pages.pdf",
        "crop-offset.pdf",
        "layout-order.pdf",
    ] {
        let root = scratch.0.join(name.replace('.', "-"));
        let input = fixture(name);
        let original = fs::read(&input).unwrap();
        let output = cli(&["source", "parse-pdf"], &[&input, &root]);
        assert!(
            output.status.success(),
            "missing or failed real PDF CLI: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report = json_output(&output);
        assert_eq!(report["status"], "parsed");
        assert_eq!(report["parser"]["engine"], "pdfplumber");
        let case = expected["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["file"] == name)
            .unwrap();
        check_authoring(&root, case);
        assert_eq!(fs::read(&input).unwrap(), original);
        scratch.sentinel();
    }
}

#[test]
fn real_boundary_matrix_preserves_physical_pages_and_honest_nonzero_status() {
    let scratch = Scratch::new();
    let expected: Value =
        serde_json::from_slice(&fs::read(fixture("expected.json")).unwrap()).unwrap();
    for case in expected["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["status"] != "parsed")
    {
        let name = case["file"].as_str().unwrap();
        let input = fixture(name);
        let original = fs::read(&input).unwrap();
        let root = scratch.0.join(name.replace('.', "-"));
        let output = cli(&["source", "parse-pdf"], &[&input, &root]);
        assert_eq!(
            output.status.code(),
            Some(3),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report = json_output(&output);
        assert_eq!(report["status"], case["status"], "{name}");
        check_authoring(&root, case);
        assert_eq!(fs::read(&input).unwrap(), original);
        assert_eq!(fs::read(root.join("parser/raw.pdf")).unwrap(), original);
        scratch.sentinel();
    }
}

#[test]
fn inspect_uses_saved_raw_after_input_moves_and_rejects_changed_text() {
    let scratch = Scratch::new();
    let input = scratch.0.join("第88章 page999 文本.pdf");
    fs::copy(fixture("single-page.pdf"), &input).unwrap();
    let root = scratch.0.join("新 PDF 测试根");
    let parsed = cli(&["source", "parse-pdf"], &[&input, &root]);
    assert!(
        parsed.status.success(),
        "{}",
        String::from_utf8_lossy(&parsed.stderr)
    );
    let original_report = fs::read(root.join("parse-result.json")).unwrap();
    fs::rename(&input, scratch.0.join("moved-original.pdf")).unwrap();
    let inspected = cli(&["source", "inspect-pdf"], &[&root]);
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    assert_eq!(
        json_output(&inspected)["input_fingerprint"],
        json_output(&parsed)["input_fingerprint"]
    );
    assert_eq!(
        fs::read(root.join("parse-result.json")).unwrap(),
        original_report
    );
    fs::write(root.join("parser/pages/000000.txt"), b"missing body\n").unwrap();
    let corrupt = cli(&["source", "inspect-pdf"], &[&root]);
    assert!(!corrupt.status.success());
    assert!(String::from_utf8_lossy(&corrupt.stderr).contains("RESOURCE"));
    scratch.sentinel();
}

#[test]
fn existing_root_unreadable_input_and_unknown_workspace_entries_never_succeed() {
    let scratch = Scratch::new();
    let root = scratch.0.join("occupied");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("human.txt"), b"keep human content").unwrap();
    let rejected = cli(
        &["source", "parse-pdf"],
        &[&fixture("single-page.pdf"), &root],
    );
    assert!(!rejected.status.success());
    assert_eq!(
        fs::read(root.join("human.txt")).unwrap(),
        b"keep human content"
    );
    let missing_root = scratch.0.join("read-failure");
    let missing = cli(
        &["source", "parse-pdf"],
        &[&scratch.0.join("missing.pdf"), &missing_root],
    );
    assert!(!missing.status.success());
    let failure = json_output(&missing);
    assert_eq!(failure["input_fingerprint"], Value::Null);
    assert_eq!(failure["page_count"], Value::Null);
    let valid = scratch.0.join("valid");
    assert!(
        cli(
            &["source", "parse-pdf"],
            &[&fixture("single-page.pdf"), &valid]
        )
        .status
        .success()
    );
    fs::write(valid.join("parser/unknown.txt"), b"unknown file").unwrap();
    let unknown = cli(&["source", "inspect-pdf"], &[&valid]);
    assert!(!unknown.status.success());
    scratch.sentinel();
}

#[test]
fn location_and_body_mutations_fail_even_after_artifact_hashes_are_updated() {
    let scratch = Scratch::new();
    let root = scratch.0.join("mutations");
    let output = cli(
        &["source", "parse-pdf"],
        &[&fixture("single-page.pdf"), &root],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest_path = root.join("parser/pdf-manifest.json");
    let report_path = root.join("parse-result.json");
    let markdown_path = root.join("parse-result.md");
    let manifest_bytes = fs::read(&manifest_path).unwrap();
    let original_report: Value = serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    let original_markdown = fs::read_to_string(&markdown_path).unwrap();
    for mutation in [
        "missing_page",
        "wrong_page",
        "out_of_bounds_bbox",
        "wrong_in_bounds_bbox",
        "wrong_order",
        "missing_body",
        "cut_utf8",
        "unsafe_resource_path",
        "unknown_field",
    ] {
        let mut manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
        let mut report = original_report.clone();
        let page = &mut manifest["pages"][0];
        match mutation {
            "missing_page" => {
                manifest["pages"] = json!([]);
            }
            "wrong_page" => {
                page["regions"][0]["physical_page_index"] = json!(1);
            }
            "out_of_bounds_bbox" => {
                page["regions"][0]["bbox"][2] = json!(10000);
            }
            "wrong_in_bounds_bbox" => {
                for axis in [0, 2] {
                    let value = page["regions"][0]["bbox"][axis].as_f64().unwrap();
                    page["regions"][0]["bbox"][axis] = json!(value + 10.0);
                }
            }
            "wrong_order" => {
                page["regions"].as_array_mut().unwrap().swap(0, 1);
            }
            "missing_body" => {
                page["regions"].as_array_mut().unwrap().remove(1);
            }
            "cut_utf8" => {
                let start = page["regions"][2]["text"]["start_byte"].as_u64().unwrap();
                page["regions"][2]["text"]["start_byte"] = json!(start + 4);
            }
            "unsafe_resource_path" => {
                manifest["resources"][0]["relative_path"] = json!("../external-sentinel.txt");
                report["sidecar"]["resources"][0]["relative_path"] =
                    json!("../external-sentinel.txt");
            }
            "unknown_field" => {
                page["unknown"] = json!(true);
            }
            _ => unreachable!(),
        }
        let new_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        let new_sha = sha(&new_bytes);
        let old_sha = report["manifest_sha256"].as_str().unwrap().to_owned();
        report["manifest_sha256"] = json!(new_sha);
        let manifest_resource = report["sidecar"]["resources"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["resource_id"] == "pdf-manifest")
            .unwrap();
        manifest_resource["fingerprint"]["sha256"] = json!(new_sha);
        manifest_resource["fingerprint"]["byte_length"] = json!(new_bytes.len());
        report["resource_bytes"] = json!(
            report["sidecar"]["resources"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["fingerprint"]["byte_length"].as_u64().unwrap())
                .sum::<u64>()
        );
        fs::write(&manifest_path, new_bytes).unwrap();
        fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        fs::write(
            &markdown_path,
            original_markdown.replace(&old_sha, &new_sha),
        )
        .unwrap();
        let inspected = cli(&["source", "inspect-pdf"], &[&root]);
        assert!(
            !inspected.status.success(),
            "accepted semantic mutation {mutation}"
        );
        assert_eq!(
            fs::read(root.join("parser/raw.pdf")).unwrap(),
            fs::read(fixture("single-page.pdf")).unwrap()
        );
        scratch.sentinel();
    }
}
