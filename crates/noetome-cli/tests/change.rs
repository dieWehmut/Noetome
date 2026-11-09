//! CLI black-box tests for the locked-change planner.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/locked-change-v1")
        .canonicalize()
        .expect("fixture")
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-change-{label}-{}-{serial}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("sandbox");
    path
}

fn run(arguments: &[&str]) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(arguments)
        .output()
        .expect("run");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.code().unwrap_or(-1), text)
}

fn tree(path: &Path) -> Vec<(String, u64)> {
    let mut found = Vec::new();
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&current) {
            for entry in entries.flatten() {
                let item = entry.path();
                if item.is_dir() {
                    stack.push(item);
                } else {
                    let size = fs::metadata(&item)
                        .map(|value| value.len())
                        .unwrap_or_default();
                    let relative = item
                        .strip_prefix(path)
                        .unwrap_or(&item)
                        .display()
                        .to_string();
                    found.push((relative, size));
                }
            }
        }
    }
    found.sort();
    found
}

fn adopt(label: &str, project: &str) -> (PathBuf, PathBuf, i32, String) {
    let directory = sandbox(label);
    let root = directory.join("root");
    let package = fixture();
    let project_path = package.join(project);
    let (code, text) = run(&[
        "change",
        "adopt",
        project_path.to_str().expect("path"),
        root.to_str().expect("path"),
    ]);
    (directory, root, code, text)
}

#[test]
fn adopt_freezes_the_declaration_and_only_writes_control() {
    let (directory, root, code, text) = adopt("adopt-clean", "project.json");
    assert_eq!(code, 0, "{text}");
    let entries: Vec<String> = fs::read_dir(&root)
        .expect("root")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["control".to_owned()]);
    let control: Vec<String> = fs::read_dir(root.join("control"))
        .expect("control")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    let mut sorted = control.clone();
    sorted.sort();
    assert_eq!(sorted, vec!["inputs.json", "manifest.json", "project.json"]);
    assert!(text.contains("已应用 3 条"));
    assert!(text.contains("重构 1 条（引用变化 1 处）"));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn status_accepts_the_same_root_through_a_relative_path_without_writing() {
    let (directory, root, code, text) = adopt("relative-root", "project.json");
    assert_eq!(code, 0, "{text}");
    let before = tree(&root);
    let output = Command::new(env!("CARGO_BIN_EXE_noetome"))
        .current_dir(&directory)
        .args(["change", "status", "root"])
        .output()
        .expect("status relative root");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(tree(&root), before);
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn adopt_refuses_an_existing_root_without_touching_it() {
    let (directory, root, code, _text) = adopt("adopt-existing", "project.json");
    assert_eq!(code, 0);
    let before = tree(&root);
    let package = fixture();
    let (second, text) = run(&[
        "change",
        "adopt",
        package.join("project.json").to_str().expect("path"),
        root.to_str().expect("path"),
    ]);
    assert_eq!(second, 2, "{text}");
    assert!(text.contains("变更根必须尚不存在"));
    assert_eq!(before, tree(&root));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn adopt_refuses_a_missing_parent_and_leaves_nothing_behind() {
    let directory = sandbox("adopt-missing");
    let root = directory.join("no-parent").join("root");
    let package = fixture();
    let (code, text) = run(&[
        "change",
        "adopt",
        package.join("project.json").to_str().expect("path"),
        root.to_str().expect("path"),
    ]);
    assert_eq!(code, 3, "{text}");
    assert!(text.contains("无法访问变更根的父目录"));
    assert!(!root.exists());
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn a_file_lock_turns_a_rename_into_a_candidate() {
    let (directory, root, code, _text) = adopt("lock-file", "project-lock-file.json");
    assert_eq!(code, 0);
    let output = directory.join("out");
    let (build, text) = run(&[
        "change",
        "build",
        root.to_str().expect("path"),
        output.to_str().expect("path"),
    ]);
    assert_eq!(build, 3, "{text}");
    assert!(text.contains("CHANGE_FINDINGS"));
    let changes = fs::read_to_string(output.join("changes.json")).expect("changes");
    assert!(changes.contains("\"n-pfk\""));
    assert!(changes.contains("lock-file-pfk"));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn a_directory_lock_turns_a_move_into_a_candidate() {
    let (directory, root, code, _text) = adopt("lock-dir", "project-lock-dir.json");
    assert_eq!(code, 0);
    let output = directory.join("out");
    let (build, _text) = run(&[
        "change",
        "build",
        root.to_str().expect("path"),
        output.to_str().expect("path"),
    ]);
    assert_eq!(build, 3);
    let report = fs::read_to_string(output.join("report.json")).expect("report");
    assert!(report.contains("lock_conflict"));
    assert!(report.contains("n-gng"));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn a_canonical_lock_makes_a_move_unsafe() {
    let (_directory, root, code, text) = adopt("canonical", "project-canonical.json");
    assert_eq!(code, 3, "{text}");
    assert!(text.contains("CHANGE_UNSAFE_REFACTOR"));
    assert!(text.contains("已保留旧位置"));
    assert!(!root.exists());
}

#[test]
fn a_missed_dependency_is_reported_and_the_report_is_written() {
    let (directory, root, _code, _text) = adopt("missed", "project-missed.json");
    let output = directory.join("out");
    let (build, _text) = run(&[
        "change",
        "build",
        root.to_str().expect("path"),
        output.to_str().expect("path"),
    ]);
    assert_eq!(build, 3);
    let report = fs::read_to_string(output.join("report.json")).expect("report");
    assert!(report.contains("missed_dependency"));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn an_unexpected_touch_is_reported() {
    let (directory, root, _code, _text) = adopt("unexpected", "project-unexpected.json");
    let output = directory.join("out");
    let (build, _text) = run(&[
        "change",
        "build",
        root.to_str().expect("path"),
        output.to_str().expect("path"),
    ]);
    assert_eq!(build, 3);
    let report = fs::read_to_string(output.join("report.json")).expect("report");
    assert!(report.contains("unexpected_touch"));
    assert!(!report.contains("missed_dependency"));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn a_split_without_a_partition_is_refused() {
    let (_directory, root, code, text) = adopt("unsafe-split", "project-unsafe-split.json");
    assert_eq!(code, 3, "{text}");
    assert!(text.contains("CHANGE_UNSAFE_REFACTOR"));
    assert!(!root.exists());
}

#[test]
fn build_is_deterministic_and_matches_the_frozen_expectation() {
    let (directory, root, _code, _text) = adopt("determinism", "project.json");
    let first = directory.join("out-1");
    let second = directory.join("out-2");
    for output in [&first, &second] {
        let (code, text) = run(&[
            "change",
            "build",
            root.to_str().expect("path"),
            output.to_str().expect("path"),
        ]);
        assert_eq!(code, 0, "{text}");
    }
    assert_eq!(tree(&first), tree(&second));
    let report = fs::read_to_string(first.join("report.json")).expect("report");
    let expected = fs::read_to_string(fixture().join("expected.json")).expect("expected");
    let report_value: serde_json::Value = serde_json::from_str(&report).expect("json");
    let expected_value: serde_json::Value = serde_json::from_str(&expected).expect("json");
    assert_eq!(report_value["counts"], expected_value["counts"]);
    assert_eq!(report_value["verdict"], expected_value["verdict"]);
    let manifest = fs::read_to_string(first.join("manifest.json")).expect("manifest");
    assert!(!manifest.contains("created_at"));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn every_unchanged_note_keeps_its_declared_fingerprint() {
    let (directory, root, _code, _text) = adopt("unchanged", "project.json");
    let output = directory.join("out");
    let (code, _text) = run(&[
        "change",
        "build",
        root.to_str().expect("path"),
        output.to_str().expect("path"),
    ]);
    assert_eq!(code, 0);
    let changes = fs::read_to_string(output.join("changes.json")).expect("changes");
    let value: serde_json::Value = serde_json::from_str(&changes).expect("json");
    let unchanged = value["unchanged"].as_array().expect("array");
    assert_eq!(unchanged.len(), 1);
    assert_eq!(unchanged[0]["note_id"], "n-pyr");
    let fingerprint = unchanged[0]["fingerprint"].as_str().expect("string");
    assert_eq!(fingerprint.len(), 64);
    assert!(fingerprint.chars().all(|item| item == 'e'));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn the_refactor_lists_every_reference_change() {
    let (directory, root, _code, _text) = adopt("refactor", "project.json");
    let output = directory.join("out");
    let (code, _text) = run(&[
        "change",
        "build",
        root.to_str().expect("path"),
        output.to_str().expect("path"),
    ]);
    assert_eq!(code, 0);
    let changes = fs::read_to_string(output.join("changes.json")).expect("changes");
    let value: serde_json::Value = serde_json::from_str(&changes).expect("json");
    let refactors = value["refactors"].as_array().expect("array");
    assert_eq!(refactors.len(), 1);
    assert_eq!(refactors[0]["kind"], "move");
    assert_eq!(
        refactors[0]["references"].as_array().expect("refs").len(),
        1
    );
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn build_refuses_an_existing_output_root() {
    let (directory, root, _code, _text) = adopt("out-existing", "project.json");
    let output = directory.join("out");
    fs::create_dir(&output).expect("out");
    let (code, text) = run(&[
        "change",
        "build",
        root.to_str().expect("path"),
        output.to_str().expect("path"),
    ]);
    assert_eq!(code, 2, "{text}");
    assert!(text.contains("输出根必须尚不存在"));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn build_refuses_an_output_inside_the_frozen_root() {
    let (directory, root, _code, _text) = adopt("out-inside", "project.json");
    let output = root.join("out");
    let (code, text) = run(&[
        "change",
        "build",
        root.to_str().expect("path"),
        output.to_str().expect("path"),
    ]);
    assert_eq!(code, 2, "{text}");
    assert!(text.contains("不能与变更根互相包含"));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn a_changed_declaration_is_detected_before_any_report_is_written() {
    let (directory, root, _code, _text) = adopt("tampered", "project.json");
    fs::write(root.join("control").join("project.json"), "{}").expect("write");
    let output = directory.join("out");
    let (code, text) = run(&[
        "change",
        "build",
        root.to_str().expect("path"),
        output.to_str().expect("path"),
    ]);
    assert_eq!(code, 3, "{text}");
    assert!(text.contains("CHANGE_INTEGRITY"));
    assert!(!output.exists());
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn an_unknown_root_is_reported_as_such_and_left_alone() {
    let directory = sandbox("unknown");
    let root = directory.join("stranger");
    fs::create_dir(&root).expect("root");
    let before = tree(&root);
    let (code, text) = run(&["change", "status", root.to_str().expect("path")]);
    assert_eq!(code, 3, "{text}");
    assert!(text.contains("CHANGE_UNKNOWN_ROOT"));
    assert_eq!(before, tree(&root));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn status_and_list_report_the_same_plan_without_writing() {
    let (directory, root, _code, _text) = adopt("status", "project.json");
    let before = tree(&root);
    let (status_code, status) = run(&["change", "status", root.to_str().expect("path")]);
    assert_eq!(status_code, 0);
    assert!(status.contains("未受影响的笔记：1 份"));
    let (list_code, listing) = run(&["change", "list", root.to_str().expect("path")]);
    assert_eq!(list_code, 0);
    assert!(listing.contains("\"verdict\": \"pass\""));
    assert_eq!(before, tree(&root));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let (code, text) = run(&["change", "--help"]);
    assert_eq!(code, 0);
    assert!(text.contains("锁定"));
    let (bad, message) = run(&["change", "adopt", "only-one"]);
    assert_eq!(bad, 2);
    assert!(message.contains("adopt 需要"));
}
