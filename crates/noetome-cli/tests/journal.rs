//! Black-box tests for `noetome journal adopt|build|list|status`.

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
        .join("../../fixtures/journal-audit-v1")
        .canonicalize()
        .expect("fixture directory");
    assert!(path.is_dir(), "{} 必须是目录", path.display());
    path
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "noetome-journal-{label}-{}-{serial}",
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
    cli(&["journal", "adopt", &arg(&project), &arg(root)])
}

fn build(root: &Path, output: &Path) -> Output {
    cli(&["journal", "build", &arg(root), &arg(output)])
}

#[test]
fn adopt_freezes_the_declaration_and_only_writes_control() {
    let sandbox = sandbox("adopt");
    let root = sandbox.join("root");
    let output = adopt(&root, "project.json");
    assert!(output.status.success(), "{}", text(&output));

    let top: Vec<String> = fs::read_dir(&root)
        .expect("root")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(top, vec!["control".to_owned()]);

    let mut control: Vec<String> = fs::read_dir(root.join("control"))
        .expect("control")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    control.sort();
    assert_eq!(
        control,
        vec!["inputs.json", "manifest.json", "project.json"]
    );

    let manifest = json(&root.join("control").join("manifest.json"));
    assert_eq!(manifest["policy"], "audit-journal-v1");
    assert_eq!(manifest["producer"], "noetome-journal");
    let inputs = json(&root.join("control").join("inputs.json"));
    assert_eq!(inputs["tasks"], 2);
    assert_eq!(inputs["events"], 6);
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
    assert!(text(&output).contains("JOURNAL_ARGUMENT"));
    assert!(text(&output).contains("审计根必须尚不存在"));
    assert_eq!(tree(&root), before);
}

#[test]
fn adopt_refuses_a_missing_parent_and_leaves_nothing_behind() {
    let sandbox = sandbox("noparent");
    let root = sandbox.join("missing").join("root");
    let output = adopt(&root, "project.json");
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(text(&output).contains("JOURNAL_INTEGRITY"));
    assert!(!sandbox.join("missing").exists());
}

#[test]
fn a_task_without_sources_is_refused_before_the_root_exists() {
    let sandbox = sandbox("nosource");
    let root = sandbox.join("root");
    let output = adopt(&root, "project-no-source.json");
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("没有声明任何来源"));
    assert!(!root.exists());
}

#[test]
fn a_duplicate_sequence_is_refused() {
    let sandbox = sandbox("duplicate");
    let root = sandbox.join("root");
    let output = adopt(&root, "project-duplicate-at.json");
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("先后无法判定"));
    assert!(!root.exists());
}

#[test]
fn a_credential_shaped_string_refuses_the_whole_declaration() {
    let sandbox = sandbox("secret");
    let root = sandbox.join("root");
    let output = adopt(&root, "project-secret.json");
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(text(&output).contains("JOURNAL_SECRET_MARKER"));
    assert!(text(&output).contains("password"));
    assert!(!root.exists());
}

#[test]
fn a_file_outside_the_task_objects_is_refused() {
    let sandbox = sandbox("foreign");
    let root = sandbox.join("root");
    let output = adopt(&root, "project-foreign-file.json");
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("不在这个任务的对象列表里"));
    assert!(!root.exists());
}

#[test]
fn build_is_deterministic_and_matches_the_frozen_expectation() {
    let sandbox = sandbox("build");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());

    let first = sandbox.join("out1");
    let second = sandbox.join("out2");
    assert!(build(&root, &first).status.success());
    assert!(build(&root, &second).status.success());
    assert_eq!(tree(&first), tree(&second));

    let expected: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture().join("expected.json")).expect("prior"))
            .expect("json");
    let report = json(&first.join("report.json"));
    assert_eq!(report["counts"], expected["counts"]);
    assert_eq!(report["model_calls"], 0);
    assert_eq!(report["network_requests"], 0);

    let manifest = json(&first.join("manifest.json"));
    assert_eq!(manifest["producer"], "audit_journal_output");
    assert!(manifest.get("created_at").is_none(), "输出清单不得带时间戳");
    assert!(manifest.get("timings_sha256").is_none());
}

#[test]
fn the_journal_orders_all_three_tables_by_task_then_sequence() {
    let sandbox = sandbox("order");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());

    let journal = json(&output.join("journal.json"));
    let events: Vec<&str> = journal["events"]
        .as_array()
        .expect("events")
        .iter()
        .map(|event| event["event_id"].as_str().expect("id"))
        .collect();
    assert_eq!(events, vec!["e5", "e6", "e1", "e2", "e3", "e4"]);

    let tasks: Vec<&str> = journal["tasks"]
        .as_array()
        .expect("tasks")
        .iter()
        .map(|task| task["task_id"].as_str().expect("id"))
        .collect();
    assert_eq!(tasks, vec!["task-compile", "task-parse"]);

    let failures: Vec<&str> = journal["failures"]
        .as_array()
        .expect("failures")
        .iter()
        .map(|failure| failure["event_id"].as_str().expect("id"))
        .collect();
    assert_eq!(failures, vec!["e6", "e2"]);
}

#[test]
fn every_failure_localizes_all_four_layers() {
    let sandbox = sandbox("layers");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());

    let journal = json(&output.join("journal.json"));
    let failures = journal["failures"].as_array().expect("failures");
    assert_eq!(failures.len(), 2);
    for failure in failures {
        assert!(!failure["task_id"].as_str().expect("task").is_empty());
        assert!(!failure["sources"].as_array().expect("sources").is_empty());
        assert!(!failure["objects"].as_array().expect("objects").is_empty());
        assert!(!failure["files"].as_array().expect("files").is_empty());
    }
    let followed: Vec<bool> = failures
        .iter()
        .map(|failure| failure["followed_by_action"].as_bool().expect("flag"))
        .collect();
    assert_eq!(followed, vec![false, true]);
}

#[test]
fn the_markdown_names_the_layers_and_the_boundary() {
    let sandbox = sandbox("markdown");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());

    let markdown = fs::read_to_string(output.join("report.md")).expect("markdown");
    for needle in [
        "## 失败定位",
        "## 任务状态",
        "## 边界",
        "来源范围",
        "知识对象",
        "受影响文件",
        "已重试或已解决",
        "未处理",
    ] {
        assert!(markdown.contains(needle), "缺少 {needle}");
    }
}

#[test]
fn build_refuses_an_existing_output_root() {
    let sandbox = sandbox("out-exists");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let output = sandbox.join("out");
    fs::create_dir(&output).expect("out");
    fs::write(output.join("keep.txt"), b"keep").expect("keep");
    let before = tree(&output);
    let result = build(&root, &output);
    assert_eq!(result.status.code(), Some(2), "{}", text(&result));
    assert!(text(&result).contains("输出根必须尚不存在"));
    assert_eq!(tree(&output), before);
}

#[test]
fn build_refuses_an_output_inside_the_frozen_root() {
    let sandbox = sandbox("out-inside");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let output = root.join("out");
    let result = build(&root, &output);
    assert_eq!(result.status.code(), Some(2), "{}", text(&result));
    assert!(text(&result).contains("不能与审计根互相包含"));
    assert!(!output.exists());
}

#[test]
fn a_changed_declaration_is_detected_before_any_report_is_written() {
    let sandbox = sandbox("tampered");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let path = root.join("control").join("project.json");
    let mut bytes = fs::read(&path).expect("read");
    bytes.push(b' ');
    fs::write(&path, &bytes).expect("write");

    let output = sandbox.join("out");
    let result = build(&root, &output);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(text(&result).contains("JOURNAL_INTEGRITY"));
    assert!(!output.exists(), "被改动的根不得产出输出");
}

#[test]
fn an_extra_entry_in_the_control_directory_is_refused() {
    let sandbox = sandbox("extra");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    fs::write(root.join("control").join("extra.json"), b"{}").expect("extra");
    let result = build(&root, &sandbox.join("out"));
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(text(&result).contains("控制目录必须恰好是"));
}

#[test]
fn an_unknown_root_is_reported_as_such_and_left_alone() {
    let sandbox = sandbox("unknown");
    let root = sandbox.join("root");
    fs::create_dir(&root).expect("root");
    fs::write(root.join("note.txt"), b"x").expect("note");
    let before = tree(&root);
    let result = cli(&["journal", "status", &arg(&root)]);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(text(&result).contains("JOURNAL_UNKNOWN_ROOT"));
    assert!(text(&result).contains("陌生目录保持只读"));
    assert_eq!(tree(&root), before);
}

#[test]
fn status_and_list_report_the_same_journal_without_writing() {
    let sandbox = sandbox("status");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let before = tree(&root);

    let status = cli(&["journal", "status", &arg(&root)]);
    assert!(status.status.success(), "{}", text(&status));
    let status_text = text(&status);
    for needle in [
        "失败定位",
        "2 条失败事件",
        "未处理 1 条",
        "当前结果 resolved",
    ] {
        assert!(status_text.contains(needle), "缺少 {needle}");
    }

    let listing = cli(&["journal", "list", &arg(&root)]);
    assert!(listing.status.success(), "{}", text(&listing));
    let payload: serde_json::Value = serde_json::from_slice(&listing.stdout).expect("list json");
    assert_eq!(payload["counts"]["events"], 6);
    assert_eq!(payload["counts"]["unresolved_failures"], 1);
    assert_eq!(payload["model_calls"], 0);
    assert_eq!(payload["network_requests"], 0);

    assert_eq!(tree(&root), before, "只读动词不得改动审计根");
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["journal", "--help"]);
    assert!(help.status.success());
    assert!(text(&help).contains("Noetome 审计事件日志"));

    for arguments in [
        vec!["journal"],
        vec!["journal", "adopt"],
        vec!["journal", "adopt", "a"],
        vec!["journal", "adopt", "a", "b", "c"],
        vec!["journal", "build", "a"],
        vec!["journal", "list"],
        vec!["journal", "status"],
        vec!["journal", "unknown"],
    ] {
        let output = cli(&arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(
            text(&output).contains("CLI_INVALID_ARGUMENT"),
            "{arguments:?}"
        );
    }
}

#[test]
fn the_parsed_command_round_trips() {
    use std::ffi::OsString;
    let parsed = noetome_cli::parse(&[
        OsString::from("journal"),
        OsString::from("adopt"),
        OsString::from("a"),
        OsString::from("b"),
    ])
    .expect("parsed");
    match parsed {
        CliCommand::JournalAdopt { project, root } => {
            assert_eq!(project, PathBuf::from("a"));
            assert_eq!(root, PathBuf::from("b"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}
