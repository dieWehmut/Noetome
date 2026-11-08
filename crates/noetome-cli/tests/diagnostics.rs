//! Black-box tests for `noetome diagnostics adopt|build|list|status`.

use noetome_cli::CliCommand;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_noetome")
}

fn fixture() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/diagnostics-export-v1")
        .canonicalize()
        .expect("fixture directory");
    assert!(path.is_dir(), "{} 必须是目录", path.display());
    path
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "noetome-diagnostics-{label}-{}-{serial}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("sandbox");
    path
}

fn arg(value: &Path) -> String {
    value.to_string_lossy().into_owned()
}

fn cli(arguments: &[&str]) -> Output {
    Command::new(binary())
        .args(arguments)
        .output()
        .expect("run noetome")
}

fn text(output: &Output) -> String {
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    fn walk(base: &Path, path: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let mut entries: Vec<PathBuf> = fs::read_dir(path)
            .expect("read_dir")
            .map(|entry| entry.expect("entry").path())
            .collect();
        entries.sort();
        for entry in entries {
            if entry.is_dir() {
                walk(base, &entry, out);
            } else {
                let key = entry
                    .strip_prefix(base)
                    .expect("strip")
                    .to_string_lossy()
                    .into_owned();
                out.insert(key, fs::read(&entry).expect("read"));
            }
        }
    }
    walk(root, root, &mut out);
    out
}

fn json(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path).expect("read json")).expect("valid json")
}

fn adopt(root: &Path, variant: &str) -> Output {
    let project = fixture().join(variant);
    cli(&["diagnostics", "adopt", &arg(&project), &arg(root)])
}

fn build(root: &Path, output: &Path) -> Output {
    cli(&["diagnostics", "build", &arg(root), &arg(output)])
}

fn names(path: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(path)
        .expect("read_dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    out.sort();
    out
}

#[test]
fn adopt_freezes_the_declaration_and_only_writes_control() {
    let sandbox = sandbox("adopt");
    let root = sandbox.join("root");
    let output = adopt(&root, "project.json");
    assert!(output.status.success(), "{}", text(&output));

    assert_eq!(names(&root), vec!["control".to_owned()]);
    assert_eq!(
        names(&root.join("control")),
        vec!["inputs.json", "manifest.json", "project.json"]
    );

    let manifest = json(&root.join("control").join("manifest.json"));
    assert_eq!(manifest["policy"], "diagnostics-export-v1");
    assert_eq!(manifest["producer"], "noetome-diagnostics");
    let inputs = json(&root.join("control").join("inputs.json"));
    assert_eq!(inputs["categories"], 3);
    assert_eq!(inputs["entries"], 4);
    assert_eq!(inputs["selected_categories"], 2);

    // The frozen copy is the declaration's own bytes, not a re-encoding.
    let source = fs::read(fixture().join("project.json")).expect("source");
    let frozen = fs::read(root.join("control").join("project.json")).expect("frozen");
    assert_eq!(source, frozen);
}

#[test]
fn adopt_refuses_an_existing_root_without_touching_it() {
    let sandbox = sandbox("existing");
    let root = sandbox.join("root");
    fs::create_dir(&root).expect("root");
    fs::write(root.join("keep.txt"), b"keep").expect("keep");
    let before = tree(&root);

    let output = adopt(&root, "project.json");
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("DIAGNOSTICS_ARGUMENT"));
    assert!(text(&output).contains("诊断根必须尚不存在"));
    assert_eq!(tree(&root), before);
}

#[test]
fn adopt_refuses_a_missing_parent_and_leaves_nothing_behind() {
    let sandbox = sandbox("noparent");
    let root = sandbox.join("no-such-parent").join("root");
    let output = adopt(&root, "project.json");
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(text(&output).contains("无法访问诊断根的父目录"));
    assert!(!sandbox.join("no-such-parent").exists());
}

#[test]
fn selecting_a_sensitive_category_without_consent_is_refused() {
    let sandbox = sandbox("consent");
    let root = sandbox.join("root");
    let output = adopt(&root, "project-consent.json");
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(text(&output).contains("DIAGNOSTICS_CONSENT"));
    assert!(text(&output).contains("source-body"));
    assert!(!root.exists());
}

#[test]
fn an_absolute_path_pasted_into_a_preview_is_refused() {
    let sandbox = sandbox("absolute");
    let root = sandbox.join("root");
    let output = adopt(&root, "project-absolute.json");
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(text(&output).contains("DIAGNOSTICS_ABSOLUTE_PATH"));
    assert!(!root.exists());
}

#[test]
fn a_credential_shaped_preview_is_refused() {
    let sandbox = sandbox("secret");
    let root = sandbox.join("root");
    let output = adopt(&root, "project-secret.json");
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(text(&output).contains("DIAGNOSTICS_SECRET_MARKER"));
    assert!(text(&output).contains("password"));
    assert!(!root.exists());
}

#[test]
fn an_unknown_selection_and_an_empty_category_are_arguments() {
    let sandbox = sandbox("arguments");
    let unknown = sandbox.join("unknown");
    let output = adopt(&unknown, "project-unknown.json");
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("勾选了不存在的范围类别：network-capture"));

    let empty = sandbox.join("empty");
    let output = adopt(&empty, "project-empty.json");
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("没有任何条目"));
    assert!(!unknown.exists() && !empty.exists());
}

#[test]
fn build_is_deterministic_and_matches_the_frozen_expectation() {
    let sandbox = sandbox("build");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());

    let first = sandbox.join("out-a");
    let second = sandbox.join("out-b");
    assert!(build(&root, &first).status.success());
    assert!(build(&root, &second).status.success());
    assert_eq!(tree(&first), tree(&second));

    assert_eq!(
        names(&first),
        vec![
            "export.json".to_owned(),
            "manifest.json".to_owned(),
            "report.json".to_owned(),
            "report.md".to_owned()
        ]
    );

    let expected = json(&fixture().join("expected.json"));
    let report = json(&first.join("report.json"));
    for key in ["categories", "selected_categories", "excluded_categories"] {
        assert_eq!(
            report["counts"][key], expected["counts"][key],
            "counts.{key}"
        );
    }
    for key in ["sensitive_included", "entries", "selected_entries"] {
        assert_eq!(
            report["counts"][key], expected["counts"][key],
            "counts.{key}"
        );
    }
    for key in ["exported_bytes", "excluded_bytes", "model_calls"] {
        assert_eq!(
            report["counts"][key], expected["counts"][key],
            "counts.{key}"
        );
    }

    // The output manifest must never carry a timestamp.
    let manifest = json(&first.join("manifest.json"));
    assert!(manifest.get("created_at").is_none());
    assert_eq!(manifest["producer"], "diagnostics_export_output");
}

#[test]
fn unselected_categories_are_reported_not_silently_filled() {
    let sandbox = sandbox("excluded");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let out = sandbox.join("out");
    assert!(build(&root, &out).status.success());

    let export = json(&out.join("export.json"));
    let entries = export["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 3);
    assert!(
        entries
            .iter()
            .all(|entry| entry["category_id"] != "source-body")
    );

    let report = json(&out.join("report.json"));
    let categories = report["categories"].as_array().expect("categories");
    let body = categories
        .iter()
        .find(|category| category["category_id"] == "source-body")
        .expect("source-body");
    assert_eq!(body["included"], false);
    assert_eq!(body["exported_bytes"], 0);
    assert_eq!(body["bytes"], 9000);
}

#[test]
fn the_markdown_lists_the_excluded_category_and_the_boundaries() {
    let sandbox = sandbox("markdown");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let out = sandbox.join("out");
    assert!(build(&root, &out).status.success());

    let markdown = fs::read_to_string(out.join("report.md")).expect("markdown");
    assert!(markdown.contains("## 导出范围"));
    assert!(markdown.contains("## 类别与预览"));
    assert!(markdown.contains("## 未导出的类别"));
    assert!(markdown.contains("## 边界"));
    assert!(markdown.contains("source-body（来源正文）：未勾选，如实标为未导出"));
    assert!(markdown.contains("没有勾选的内容不会被静默补全"));
    assert!(markdown.contains("导出清单不含时间戳"));
    assert!(markdown.contains("不判断内容质量与科学正确性"));
}

#[test]
fn build_refuses_an_existing_output_root() {
    let sandbox = sandbox("existing-out");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let out = sandbox.join("out");
    fs::create_dir(&out).expect("out");
    fs::write(out.join("keep.txt"), b"keep").expect("keep");
    let before = tree(&out);

    let output = build(&root, &out);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("输出根必须尚不存在"));
    assert_eq!(tree(&out), before);
}

#[test]
fn build_refuses_an_output_inside_the_frozen_root() {
    let sandbox = sandbox("inner-out");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let before = tree(&root);

    let output = build(&root, &root.join("inner"));
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("输出根不能与诊断根互相包含"));
    assert_eq!(tree(&root), before);
}

#[test]
fn a_changed_declaration_is_detected_before_any_report_is_written() {
    let sandbox = sandbox("tampered");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());

    let project = root.join("control").join("project.json");
    let mut bytes = fs::read(&project).expect("project");
    bytes.extend_from_slice(b" ");
    fs::write(&project, bytes).expect("tamper");

    let out = sandbox.join("out");
    let output = build(&root, &out);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(text(&output).contains("DIAGNOSTICS_INTEGRITY"));
    assert!(!out.exists());
}

#[test]
fn an_extra_entry_in_the_control_directory_is_refused() {
    let sandbox = sandbox("extra");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    fs::write(root.join("control").join("note.txt"), b"note").expect("note");

    let output = cli(&["diagnostics", "status", &arg(&root)]);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(text(&output).contains("控制目录必须恰好是"));
}

#[test]
fn an_unknown_root_is_reported_as_such_and_left_alone() {
    let sandbox = sandbox("unknown");
    let empty = sandbox.join("empty");
    fs::create_dir(&empty).expect("empty");
    let before = tree(&empty);

    let output = cli(&["diagnostics", "status", &arg(&empty)]);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(text(&output).contains("DIAGNOSTICS_UNKNOWN_ROOT"));
    assert_eq!(tree(&empty), before);
}

#[test]
fn status_and_list_report_the_same_export_without_writing() {
    let sandbox = sandbox("readonly");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let before = tree(&root);

    let status = cli(&["diagnostics", "status", &arg(&root)]);
    assert!(status.status.success(), "{}", text(&status));
    let summary = text(&status);
    assert!(summary.contains("诊断导出"));
    assert!(summary.contains("已导出 2 个类别、3 条、700 字节"));
    assert!(summary.contains("未导出 1 个类别、9000 字节"));
    assert!(summary.contains("source-body（来源正文）：1 条、9000 字节，未导出"));

    let listed = cli(&["diagnostics", "list", &arg(&root)]);
    assert!(listed.status.success(), "{}", text(&listed));
    let value: serde_json::Value = serde_json::from_slice(&listed.stdout).expect("list json");
    assert_eq!(value["counts"]["exported_bytes"], 700);
    assert_eq!(value["counts"]["excluded_bytes"], 9000);
    assert_eq!(value["categories"][2]["included"], false);

    assert_eq!(tree(&root), before);
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["diagnostics", "--help"]);
    assert!(help.status.success());
    let body = text(&help);
    assert!(body.contains("Noetome 诊断导出"));
    assert!(body.contains("不静默补全"));
    assert!(body.contains("整包拒绝"));

    let missing = cli(&["diagnostics"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(text(&missing).contains("CLI_INVALID_ARGUMENT"));

    let extra = cli(&["diagnostics", "status", "a", "b"]);
    assert_eq!(extra.status.code(), Some(2));
}

#[test]
fn the_parsed_command_round_trips() {
    let sandbox = sandbox("parse");
    let root = sandbox.join("root");
    let parsed = noetome_cli::parse(&[
        std::ffi::OsString::from("diagnostics"),
        std::ffi::OsString::from("status"),
        std::ffi::OsString::from(arg(&root)),
    ])
    .expect("parse");
    match parsed {
        CliCommand::DiagnosticsStatus { root: parsed_root } => {
            assert_eq!(arg(&parsed_root), arg(&root));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}
