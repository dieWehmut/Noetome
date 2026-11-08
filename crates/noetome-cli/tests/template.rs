//! Integration tests for `noetome template`.
//!
//! Every test drives the real binary and reads the disk. Nothing here imports
//! the planning logic, so a passing test means the observable structure changed,
//! not that an internal helper still agrees with itself.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CASE: AtomicU64 = AtomicU64::new(1);

fn fixture() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/structure-template-v1");
    fs::canonicalize(&path).expect("fixture package")
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-template-{label}-{}-{serial}",
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
        "template".into(),
        "adopt".into(),
        arg(&fixture().join(project)),
        arg(root),
    ])
}

fn adopt(root: &Path) -> Output {
    adopt_file("project.json", root)
}

fn build(root: &Path, output: &Path) -> Output {
    cli(&["template".into(), "build".into(), arg(root), arg(output)])
}

fn json(path: &Path) -> serde_json::Value {
    let bytes = fs::read(path).expect("read json");
    serde_json::from_slice(&bytes).expect("parse json")
}

#[test]
fn adopt_freezes_the_declaration_and_only_writes_control() {
    let sandbox = sandbox("adopt");
    let root = sandbox.join("root");
    let output = adopt(&root);
    assert!(output.status.success(), "{}", text(&output));
    let entries = tree(&root);
    let names: Vec<&str> = entries.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        vec![
            "control/inputs.json",
            "control/manifest.json",
            "control/project.json"
        ]
    );
    let manifest = json(&root.join("control/manifest.json"));
    assert_eq!(manifest["producer"], "noetome-template");
    assert_eq!(manifest["case_id"], "tpl-metabolism-v1");
    let inputs = json(&root.join("control/inputs.json"));
    assert_eq!(inputs["templates"], 1);
    assert_eq!(inputs["topics"], 2);
    assert_eq!(inputs["goals"], 6);
    assert_eq!(inputs["atoms"], 10);
}

#[test]
fn adopt_refuses_an_existing_root_without_touching_it() {
    let sandbox = sandbox("existing");
    let root = sandbox.join("root");
    fs::create_dir(&root).expect("root");
    fs::write(root.join("note.md"), b"keep").expect("seed");
    let before = tree(&root);
    let output = adopt(&root);
    assert!(!output.status.success());
    assert!(
        text(&output).contains("结构根必须尚不存在"),
        "{}",
        text(&output)
    );
    assert_eq!(tree(&root), before);
}

#[test]
fn adopt_refuses_a_missing_parent_and_leaves_nothing_behind() {
    let sandbox = sandbox("missing");
    let root = sandbox.join("no-parent/root");
    let output = adopt(&root);
    assert!(!output.status.success());
    assert!(
        text(&output).contains("无法访问结构根的父目录"),
        "{}",
        text(&output)
    );
    assert!(!sandbox.join("no-parent").exists());
}

#[test]
fn an_empty_declared_section_is_refused_before_the_root_exists() {
    let sandbox = sandbox("empty");
    let root = sandbox.join("root");
    let output = adopt_file("project-empty-section.json", &root);
    assert_eq!(output.status.code(), Some(3));
    assert!(
        text(&output).contains("TEMPLATE_EMPTY_SECTION"),
        "{}",
        text(&output)
    );
    assert!(!root.exists());
}

#[test]
fn a_section_out_of_order_is_refused() {
    let sandbox = sandbox("order");
    let root = sandbox.join("root");
    let output = adopt_file("project-section-order.json", &root);
    assert_eq!(output.status.code(), Some(3));
    assert!(
        text(&output).contains("TEMPLATE_SECTION_ORDER"),
        "{}",
        text(&output)
    );
    assert!(!root.exists());
}

#[test]
fn a_section_outside_the_template_needs_a_reason() {
    let sandbox = sandbox("special");
    let root = sandbox.join("root");
    let output = adopt_file("project-special.json", &root);
    assert_eq!(output.status.code(), Some(3));
    assert!(
        text(&output).contains("特殊结构必须记录特殊理由"),
        "{}",
        text(&output)
    );
    assert!(!root.exists());
}

#[test]
fn a_title_that_does_not_follow_the_pattern_needs_a_reason() {
    let sandbox = sandbox("title");
    let root = sandbox.join("root");
    let output = adopt_file("project-title.json", &root);
    assert_eq!(output.status.code(), Some(3));
    assert!(text(&output).contains("title_reason"), "{}", text(&output));
    assert!(!root.exists());
}

#[test]
fn an_inconsistent_shape_is_a_finding_and_the_report_is_still_written() {
    let sandbox = sandbox("inconsistent");
    let root = sandbox.join("root");
    let adopted = adopt_file("project-inconsistent.json", &root);
    assert!(adopted.status.success(), "{}", text(&adopted));
    let output = sandbox.join("out");
    let built = build(&root, &output);
    assert_eq!(built.status.code(), Some(3));
    assert!(
        text(&built).contains("TEMPLATE_INCONSISTENT"),
        "{}",
        text(&built)
    );
    let report = json(&output.join("report.json"));
    assert_eq!(report["verdict"], "findings");
    let kinds: Vec<&str> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| finding["kind"].as_str().expect("kind"))
        .collect();
    assert_eq!(kinds, vec!["inconsistent_structure"]);
}

#[test]
fn a_second_explanation_for_one_concept_is_a_finding() {
    let sandbox = sandbox("duplicate");
    let root = sandbox.join("root");
    assert!(adopt_file("project-duplicate.json", &root).status.success());
    let output = sandbox.join("out");
    let built = build(&root, &output);
    assert_eq!(built.status.code(), Some(3));
    assert!(
        text(&built).contains("TEMPLATE_DUPLICATE_EXPLANATION"),
        "{}",
        text(&built)
    );
    let report = json(&output.join("report.json"));
    assert_eq!(report["counts"]["findings"], 1);
}

#[test]
fn build_is_deterministic_and_matches_the_frozen_expectation() {
    let sandbox = sandbox("deterministic");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let first = sandbox.join("out-one");
    let second = sandbox.join("out-two");
    assert!(build(&root, &first).status.success());
    assert!(build(&root, &second).status.success());
    assert_eq!(tree(&first), tree(&second));
    let expected = json(&fixture().join("expected.json"));
    let plan = json(&first.join("plan.json"));
    let report = json(&first.join("report.json"));
    assert_eq!(report["counts"], expected["counts"]);
    assert_eq!(report["verdict"], expected["verdict"]);
    assert_eq!(
        plan["goals"].as_array().expect("goals").len(),
        expected["goals"].as_array().expect("goals").len()
    );
    let manifest = json(&first.join("manifest.json"));
    assert_eq!(manifest["producer"], "structure_template_output");
    assert!(manifest.get("created_at").is_none());
}

#[test]
fn every_concept_has_exactly_one_explanation_note() {
    let sandbox = sandbox("explanations");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());
    let plan = json(&output.join("plan.json"));
    let explanations = plan["explanations"].as_array().expect("explanations");
    assert_eq!(explanations.len(), 4);
    let mut notes: Vec<&str> = explanations
        .iter()
        .map(|entry| entry["note"].as_str().expect("note"))
        .collect();
    notes.sort_unstable();
    notes.dedup();
    assert!(!notes.is_empty());
    let mut concepts: Vec<&str> = explanations
        .iter()
        .map(|entry| entry["concept_id"].as_str().expect("concept"))
        .collect();
    concepts.sort_unstable();
    concepts.dedup();
    assert_eq!(concepts.len(), explanations.len());
}

#[test]
fn references_point_at_the_explanation_note_instead_of_copying_it() {
    let sandbox = sandbox("references");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());
    let plan = json(&output.join("plan.json"));
    let mut explanation_notes = BTreeMap::new();
    for entry in plan["explanations"].as_array().expect("explanations") {
        explanation_notes.insert(
            entry["concept_id"].as_str().expect("concept").to_owned(),
            entry["note"].as_str().expect("note").to_owned(),
        );
    }
    let mut references = 0;
    let mut embeds = 0;
    for goal in plan["goals"].as_array().expect("goals") {
        let path = goal["path"].as_str().expect("path");
        for reference in goal["references"].as_array().expect("references") {
            match reference["role"].as_str().expect("role") {
                "reference" => references += 1,
                "embed" => embeds += 1,
                other => panic!("unexpected role {other}"),
            }
            let concept = reference["concept_id"].as_str().expect("concept");
            assert_eq!(explanation_notes[concept], reference["target_note"]);
            assert_ne!(path, reference["target_note"]);
        }
    }
    assert_eq!(references, 5);
    assert_eq!(embeds, 1);
}

#[test]
fn the_markdown_shows_sections_deviations_and_boundaries() {
    let sandbox = sandbox("markdown");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());
    let markdown = fs::read_to_string(output.join("report.md")).expect("markdown");
    for heading in [
        "## 规划规模",
        "## 阅读目标与结构",
        "## 唯一解释位置",
        "## 偏差与理由",
        "## 缺口",
        "## 边界",
    ] {
        assert!(markdown.contains(heading), "missing {heading}");
    }
    assert!(markdown.contains("代谢/糖酵解/01-goal-glycolysis-main.md"));
    assert!(markdown.contains("section_omitted"));
    assert!(markdown.contains("不判断内容质量与科学正确性"));
}

#[test]
fn build_refuses_an_existing_output_root() {
    let sandbox = sandbox("output-exists");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = sandbox.join("out");
    fs::create_dir(&output).expect("output");
    fs::write(output.join("keep.md"), b"keep").expect("seed");
    let before = tree(&output);
    let built = build(&root, &output);
    assert_eq!(built.status.code(), Some(2));
    assert!(text(&built).contains("输出根必须尚不存在"));
    assert_eq!(tree(&output), before);
}

#[test]
fn build_refuses_an_output_inside_the_frozen_root() {
    let sandbox = sandbox("output-inside");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let built = build(&root, &root.join("out"));
    assert_eq!(built.status.code(), Some(2));
    assert!(text(&built).contains("输出根不能与结构根互相包含"));
}

#[test]
fn a_changed_declaration_is_detected_before_any_report_is_written() {
    let sandbox = sandbox("tampered");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    fs::write(root.join("control/project.json"), b"{\"policy\":\"x\"}").expect("tamper");
    let output = sandbox.join("out");
    let built = build(&root, &output);
    assert_eq!(built.status.code(), Some(3));
    assert!(
        text(&built).contains("TEMPLATE_INTEGRITY"),
        "{}",
        text(&built)
    );
    assert!(!output.exists());
}

#[test]
fn an_extra_entry_in_the_control_directory_is_refused() {
    let sandbox = sandbox("extra");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    fs::write(root.join("control/extra.json"), b"{}").expect("extra");
    let output = build(&root, &sandbox.join("out"));
    assert_eq!(output.status.code(), Some(3));
    assert!(text(&output).contains("控制目录只允许"));
}

#[test]
fn an_unknown_root_is_reported_as_such_and_left_alone() {
    let sandbox = sandbox("unknown");
    let root = sandbox.join("stranger");
    fs::create_dir(&root).expect("stranger");
    fs::write(root.join("mine.md"), b"mine").expect("seed");
    let before = tree(&root);
    let output = cli(&["template".into(), "status".into(), arg(&root)]);
    assert_eq!(output.status.code(), Some(3));
    assert!(
        text(&output).contains("TEMPLATE_UNKNOWN_ROOT"),
        "{}",
        text(&output)
    );
    assert_eq!(tree(&root), before);
}

#[test]
fn status_and_list_report_the_same_structure_without_writing() {
    let sandbox = sandbox("status");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let before = tree(&root);
    let status = cli(&["template".into(), "status".into(), arg(&root)]);
    assert!(status.status.success(), "{}", text(&status));
    let listing = cli(&["template".into(), "list".into(), arg(&root)]);
    assert!(listing.status.success(), "{}", text(&listing));
    let value: serde_json::Value = serde_json::from_slice(&listing.stdout).expect("list json");
    assert_eq!(value["counts"]["notes"], 6);
    assert_eq!(value["verdict"], "pass");
    assert_eq!(tree(&root), before);
    assert!(text(&status).contains("结论：通过"));
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["template".into(), "--help".into()]);
    assert!(help.status.success());
    assert!(text(&help).contains("template adopt"));
    let missing = cli(&["template".into(), "adopt".into(), "only-one".into()]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(text(&missing).contains("CLI_INVALID_ARGUMENT"));
}
