//! Integration tests for `noetome audit`.
//!
//! Every test drives the real binary and reads the disk. Nothing here imports
//! the auditing logic, so a passing test means observable behaviour changed,
//! not that an internal helper still agrees with itself.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CASE: AtomicU64 = AtomicU64::new(1);

fn fixture() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/library-audit-v1");
    fs::canonicalize(&path).expect("fixture package")
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-audit-{label}-{}-{serial}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("sandbox root");
    fs::canonicalize(&path).expect("canonical sandbox")
}

fn arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn cli(args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(args)
        .output()
        .expect("run noetome")
}

fn text(output: &Output) -> String {
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut queue = vec![root.to_path_buf()];
    while let Some(current) = queue.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.is_dir() {
                queue.push(path);
            } else if metadata.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .expect("inside root")
                    .to_string_lossy()
                    .replace('\\', "/");
                found.insert(relative, fs::read(&path).expect("read file"));
            }
        }
    }
    found
}

fn adopt_file(project: &str, root: &Path) -> Output {
    cli(&[
        "audit".into(),
        "adopt".into(),
        arg(&fixture().join(project)),
        arg(root),
    ])
}

fn adopt(root: &Path) -> Output {
    adopt_file("project.json", root)
}

fn build(root: &Path, output: &Path) -> Output {
    cli(&["audit".into(), "build".into(), arg(root), arg(output)])
}

fn json(path: &Path) -> serde_json::Value {
    let bytes = fs::read(path).expect("read json");
    serde_json::from_slice(&bytes).expect("parse json")
}

fn expected() -> serde_json::Value {
    json(&fixture().join("expected.json"))
}

#[test]
fn adopt_freezes_the_notes_and_only_writes_control() {
    let sandbox = sandbox("adopt");
    let root = sandbox.join("root");
    let output = adopt(&root);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let message = text(&output);
    assert!(message.contains("4 份笔记、12 个标题、5 条链接"), "{message}");
    assert!(message.contains("模型调用与网络请求：0 次"), "{message}");

    let mut top: Vec<String> = fs::read_dir(&root)
        .expect("root")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    top.sort();
    assert_eq!(top, vec!["control".to_owned()]);

    let mut control: Vec<String> = fs::read_dir(root.join("control"))
        .expect("control")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    control.sort();
    assert_eq!(
        control,
        vec![
            "gold.json".to_owned(),
            "inputs.json".to_owned(),
            "manifest.json".to_owned(),
            "notes".to_owned(),
            "project.json".to_owned(),
        ]
    );

    let copied = tree(&root.join("control").join("notes"));
    let mut names: Vec<String> = copied.keys().cloned().collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "笔记/三羧酸循环.md".to_owned(),
            "笔记/术语表.md".to_owned(),
            "笔记/糖异生.md".to_owned(),
            "笔记/糖酵解.md".to_owned(),
        ]
    );
    assert_eq!(
        copied["笔记/糖酵解.md"],
        fs::read(fixture().join("笔记/糖酵解.md")).expect("source note")
    );
}

#[test]
fn adopt_refuses_an_existing_root_without_touching_it() {
    let sandbox = sandbox("existing");
    let root = sandbox.join("root");
    fs::create_dir_all(&root).expect("prepared root");
    fs::write(root.join("keep.txt"), b"keep").expect("keep");
    let before = tree(&sandbox);
    let output = adopt(&root);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("AUDIT_ARGUMENT"), "{}", text(&output));
    assert_eq!(tree(&sandbox), before);
}

#[test]
fn adopt_refuses_a_missing_parent_and_leaves_nothing_behind() {
    let sandbox = sandbox("missing-parent");
    let root = sandbox.join("absent").join("root");
    let before = tree(&sandbox);
    let output = adopt(&root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(
        text(&output).contains("无法访问审计根的父目录"),
        "{}",
        text(&output)
    );
    assert_eq!(tree(&sandbox), before);
}

#[test]
fn a_link_to_a_missing_heading_is_reported_and_the_report_is_still_written() {
    let sandbox = sandbox("broken");
    let root = sandbox.join("root");
    assert_eq!(
        adopt_file("variants/broken/project.json", &root).status.code(),
        Some(0)
    );
    let output = sandbox.join("out");
    let built = build(&root, &output);
    assert_eq!(built.status.code(), Some(3), "{}", text(&built));
    assert!(text(&built).contains("AUDIT_FINDINGS"), "{}", text(&built));

    let report = json(&output.join("report.json"));
    assert_eq!(report["verdict"], "findings");
    assert_eq!(report["counts"]["links"], 5);
    assert_eq!(report["counts"]["links_broken"], 1);
    assert_eq!(report["counts"]["links_resolved"], 4);
    assert_eq!(report["availability"]["display"], "80.0%");
    let findings = report["findings"].as_array().expect("findings");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["kind"], "broken_link");
    assert_eq!(findings[0]["severity"], "critical");

    let entries: Vec<String> = fs::read_dir(&output)
        .expect("output")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries.len(), 5, "缺口不阻止报告写出：{entries:?}");
}

#[test]
fn two_markers_for_one_concept_are_reported_as_duplicates() {
    let sandbox = sandbox("duplicate");
    let root = sandbox.join("root");
    assert_eq!(
        adopt_file("variants/duplicate/project.json", &root)
            .status
            .code(),
        Some(0)
    );
    let output = sandbox.join("out");
    assert_eq!(build(&root, &output).status.code(), Some(3));
    let report = json(&output.join("report.json"));
    assert_eq!(report["counts"]["markers"], 4);
    assert_eq!(report["counts"]["duplicate_concepts"], 1);
    assert_eq!(report["key_recall"]["display"], "50.0%");
    let kinds: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| finding["kind"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(kinds.contains(&"duplicate_explanation".to_owned()), "{kinds:?}");
}

#[test]
fn a_concept_without_a_marker_is_reported_as_unexplained() {
    let sandbox = sandbox("recall");
    let root = sandbox.join("root");
    assert_eq!(
        adopt_file("variants/recall/project.json", &root)
            .status
            .code(),
        Some(0)
    );
    let output = sandbox.join("out");
    assert_eq!(build(&root, &output).status.code(), Some(3));
    let report = json(&output.join("report.json"));
    assert_eq!(report["counts"]["markers"], 2);
    assert_eq!(report["counts"]["atoms_missing"], 1);
    assert_eq!(report["key_recall"]["display"], "75.0%");
    let kinds: Vec<String> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| finding["kind"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(kinds.contains(&"concept_unexplained".to_owned()), "{kinds:?}");
}

#[test]
fn status_reports_the_verdict_and_the_three_ratios() {
    let sandbox = sandbox("status");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let before = tree(&root);
    let output = cli(&["audit".into(), "status".into(), arg(&root)]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let message = text(&output);
    for needle in [
        "审计结论：通过",
        "链接可用：5/5（100.0%）",
        "关键原子召回：4/4（100.0%）",
        "一般原子召回：2/2（100.0%）",
        "唯一解释位置：3 个概念，3 个标记，0 个概念多处声明",
        "冻结输入 4 份笔记",
    ] {
        assert!(message.contains(needle), "缺少 {needle}：{message}");
    }
    assert_eq!(tree(&root), before, "status 不得改写审计根");
}

#[test]
fn build_is_deterministic_and_matches_the_frozen_expectation() {
    let sandbox = sandbox("build");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let first = sandbox.join("first");
    let second = sandbox.join("second");
    assert_eq!(build(&root, &first).status.code(), Some(0));
    assert_eq!(build(&root, &second).status.code(), Some(0));
    assert_eq!(tree(&first), tree(&second), "两次构建必须字节相同");

    let report = json(&first.join("report.json"));
    let prior = expected();
    assert_eq!(report["verdict"], prior["verdict"]);
    for (key, value) in prior["counts"].as_object().expect("counts") {
        assert_eq!(&report["counts"][key], value, "计数 {key} 与先验不符");
    }
    assert_eq!(report["availability"]["display"], prior["availability"]["display"]);
    assert_eq!(report["key_recall"]["display"], prior["key_recall"]["display"]);
    assert_eq!(report["general_recall"]["display"], prior["general_recall"]["display"]);
    assert_eq!(report["model_calls"], 0);
    assert_eq!(report["network_requests"], 0);

    let manifest = json(&first.join("manifest.json"));
    assert_eq!(manifest["producer"], "library_audit_output");
    assert!(
        manifest.get("created_at").is_none(),
        "输出清单不含时间戳，否则两次构建不可能字节相同"
    );
    assert_eq!(manifest["verdict"], "pass");
}

#[test]
fn the_links_file_matches_the_frozen_links() {
    let sandbox = sandbox("links");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let output = sandbox.join("out");
    assert_eq!(build(&root, &output).status.code(), Some(0));
    let links = json(&output.join("links.json"));
    assert_eq!(links["case_id"], "aud-traceability-v1");
    let prior = expected();
    let want = prior["links"].as_array().expect("prior links");
    let got = links["links"].as_array().expect("links");
    assert_eq!(got.len(), want.len());
    for (index, want) in want.iter().enumerate() {
        for field in [
            "note_id",
            "line",
            "target",
            "heading",
            "alias",
            "embed",
            "in_table",
            "escaped_alias",
            "status",
            "basis",
            "resolved_note",
            "resolved_heading",
        ] {
            assert_eq!(&got[index][field], &want[field], "第 {index} 条链接的 {field}");
        }
    }
}

#[test]
fn the_explanations_file_lists_one_marker_per_concept() {
    let sandbox = sandbox("explanations");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let output = sandbox.join("out");
    assert_eq!(build(&root, &output).status.code(), Some(0));
    let file = json(&output.join("explanations.json"));
    let prior = expected();
    let want = prior["explanations"].as_array().expect("prior concepts");
    let got = file["concepts"].as_array().expect("concepts");
    assert_eq!(got.len(), want.len());
    for (index, want) in want.iter().enumerate() {
        assert_eq!(got[index]["concept_id"], want["concept_id"]);
        assert_eq!(got[index]["unique"], true);
        assert_eq!(got[index]["canonical_note"], want["canonical_note"]);
        assert_eq!(got[index]["canonical_heading"], want["canonical_heading"]);
        assert_eq!(got[index]["markers"].as_array().map(Vec::len), Some(1));
    }
    let atoms = file["atoms"].as_array().expect("atoms");
    assert_eq!(atoms.len(), 6);
    assert!(atoms.iter().all(|atom| atom["recalled"] == true));
}

#[test]
fn the_markdown_keeps_the_ratios_and_the_boundaries_visible() {
    let sandbox = sandbox("markdown");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let output = sandbox.join("out");
    assert_eq!(build(&root, &output).status.code(), Some(0));
    let markdown = fs::read_to_string(output.join("report.md")).expect("markdown");
    for needle in [
        "# 全库链接与唯一解释位置审计（aud-traceability-v1）",
        "## 口径",
        "## 链接明细",
        "## 唯一解释位置",
        "## 缺口",
        "## 边界",
        "不判断内容质量",
        "模型调用与网络请求：0 次",
        "| 关键原子召回 | 4 | 4 | 100.0% |",
        "不适用",
    ] {
        assert!(markdown.contains(needle), "缺少 {needle}");
    }
}

#[test]
fn build_refuses_an_existing_output_root() {
    let sandbox = sandbox("output-exists");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let output = sandbox.join("out");
    fs::create_dir_all(&output).expect("prepared output");
    fs::write(output.join("keep.txt"), b"keep").expect("keep");
    let before = tree(&sandbox);
    let built = build(&root, &output);
    assert_eq!(built.status.code(), Some(2), "{}", text(&built));
    assert_eq!(tree(&sandbox), before);
}

#[test]
fn build_refuses_an_output_inside_the_audit_root() {
    let sandbox = sandbox("output-inside");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let before = tree(&root);
    let built = build(&root, &root.join("out"));
    assert_eq!(built.status.code(), Some(2), "{}", text(&built));
    assert_eq!(tree(&root), before);
}

#[test]
fn a_changed_frozen_body_is_detected_before_any_report_is_written() {
    let sandbox = sandbox("tamper");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let note = root.join("control").join("notes").join("笔记").join("糖酵解.md");
    let mut bytes = fs::read(&note).expect("note");
    bytes.extend_from_slice(b"\n");
    fs::write(&note, bytes).expect("tamper");
    let output = sandbox.join("out");
    let built = build(&root, &output);
    assert_eq!(built.status.code(), Some(3), "{}", text(&built));
    assert!(text(&built).contains("AUDIT_INTEGRITY"), "{}", text(&built));
    assert!(!output.exists(), "校验失败不得留下输出根");
}

#[test]
fn an_extra_entry_in_the_control_directory_is_refused() {
    let sandbox = sandbox("control-extra");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    fs::write(root.join("control").join("note.txt"), b"extra").expect("extra");
    let status = cli(&["audit".into(), "status".into(), arg(&root)]);
    assert_eq!(status.status.code(), Some(3), "{}", text(&status));
    assert!(text(&status).contains("AUDIT_INTEGRITY"), "{}", text(&status));
}

#[test]
fn an_unknown_root_is_reported_as_such_and_left_alone() {
    let sandbox = sandbox("unknown");
    let before = tree(&sandbox);
    let status = cli(&["audit".into(), "status".into(), arg(&sandbox)]);
    assert_eq!(status.status.code(), Some(3), "{}", text(&status));
    assert!(
        text(&status).contains("AUDIT_UNKNOWN_ROOT"),
        "{}",
        text(&status)
    );
    assert_eq!(tree(&sandbox), before);
}

#[test]
fn list_returns_machine_readable_counts() {
    let sandbox = sandbox("list");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let output = cli(&["audit".into(), "list".into(), arg(&root)]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let listing: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list is json");
    assert_eq!(listing["policy"], "library-audit-v1");
    assert_eq!(listing["producer"], "noetome-audit");
    assert_eq!(listing["verdict"], "pass");
    assert_eq!(listing["counts"]["links"], 5);
    assert_eq!(listing["counts"]["atoms"], 6);
    assert_eq!(listing["model_calls"], 0);
    assert_eq!(listing["network_requests"], 0);
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["audit".into(), "--help".into()]);
    assert_eq!(help.status.code(), Some(0), "{}", text(&help));
    assert!(text(&help).contains("全库链接可用性与唯一解释位置审计"));

    let bad = cli(&["audit".into(), "adopt".into(), "only-one".into()]);
    assert_eq!(bad.status.code(), Some(2), "{}", text(&bad));
    assert!(
        text(&bad).contains("CLI_INVALID_ARGUMENT"),
        "{}",
        text(&bad)
    );
}
