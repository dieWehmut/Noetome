//! Black-box tests for `noetome retention adopt|build|list|status`.

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
        .join("../../fixtures/retention-cleanup-v1")
        .canonicalize()
        .expect("fixture directory");
    assert!(path.is_dir(), "{} 必须是目录", path.display());
    path
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "noetome-retention-{label}-{}-{serial}",
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
    cli(&["retention", "adopt", &arg(&project), &arg(root)])
}

fn build(root: &Path, output: &Path) -> Output {
    cli(&["retention", "build", &arg(root), &arg(output)])
}

fn names(root: &Path) -> Vec<String> {
    let mut entries: Vec<String> = fs::read_dir(root)
        .expect("read_dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    entries.sort();
    entries
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
        vec![
            "inputs.json".to_owned(),
            "manifest.json".to_owned(),
            "project.json".to_owned()
        ]
    );
    let manifest = json(&root.join("control/manifest.json"));
    assert_eq!(manifest["policy"], "retention-policy-v1");
    assert_eq!(manifest["producer"], "noetome-retention");
    assert_eq!(manifest["case_id"], "retention-cleanup-v1");
    // the declaration is frozen verbatim, not re-encoded
    let frozen = fs::read(root.join("control/project.json")).expect("frozen declaration");
    let source = fs::read(fixture().join("project.json")).expect("source declaration");
    assert_eq!(frozen, source);
    let inputs = json(&root.join("control/inputs.json"));
    assert_eq!(inputs["categories"], 2);
    assert_eq!(inputs["entries"], 5);
    assert_eq!(inputs["published"], 1);
    assert_eq!(inputs["deleted"], 2);
    assert_eq!(inputs["kept"], 3);
    assert_eq!(inputs["protected"], 2);
    let summary = text(&output);
    assert!(summary.contains("保留与清理"), "{summary}");
    assert!(
        summary.contains("删除 2 条（9400 字节）、保留 3 条（2200 字节），其中受保护 2 条"),
        "{summary}"
    );
    assert!(summary.contains("0 次"), "{summary}");
}

#[test]
fn adopt_refuses_an_existing_root_without_touching_it() {
    let sandbox = sandbox("existing");
    let root = sandbox.join("root");
    fs::create_dir(&root).expect("root");
    fs::write(root.join("keep.txt"), b"keep").expect("keep");
    let before = tree(&root);
    let output = adopt(&root, "project.json");
    assert!(!output.status.success());
    assert!(
        text(&output).contains("保留根必须尚不存在"),
        "{}",
        text(&output)
    );
    assert_eq!(tree(&root), before);
}

#[test]
fn adopt_refuses_a_missing_parent_and_leaves_nothing_behind() {
    let sandbox = sandbox("parent");
    let root = sandbox.join("no-parent").join("root");
    let output = adopt(&root, "project.json");
    assert!(!output.status.success());
    assert!(
        text(&output).contains("无法访问保留根的父目录"),
        "{}",
        text(&output)
    );
    assert!(!sandbox.join("no-parent").exists());
}

#[test]
fn a_policy_that_would_expire_a_published_requirement_is_refused_before_the_root_exists() {
    let sandbox = sandbox("protected");
    let root = sandbox.join("root");
    let output = adopt(&root, "project-protected.json");
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(
        text(&output).contains("RETENTION_PROTECTED"),
        "{}",
        text(&output)
    );
    assert!(text(&output).contains("整条拒绝"), "{}", text(&output));
    assert!(!root.exists(), "被拒绝的声明不得留下根");
}

#[test]
fn an_unknown_category_a_missing_requirement_and_a_duplicate_entry_are_arguments() {
    for (variant, needle) in [
        ("project-unknown.json", "没有在可清理类别里声明"),
        ("project-missing.json", "不在存储条目里"),
        ("project-duplicate.json", "重复了"),
    ] {
        let sandbox = sandbox("argument");
        let root = sandbox.join("root");
        let output = adopt(&root, variant);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{variant}: {}",
            text(&output)
        );
        assert!(
            text(&output).contains("RETENTION_ARGUMENT"),
            "{variant}: {}",
            text(&output)
        );
        assert!(
            text(&output).contains(needle),
            "{variant}: {}",
            text(&output)
        );
        assert!(!root.exists(), "{variant} 被拒绝后不得留下根");
    }
}

#[test]
fn build_is_deterministic_and_matches_the_frozen_expectation() {
    let sandbox = sandbox("build");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let first = sandbox.join("out1");
    let second = sandbox.join("out2");
    let output = build(&root, &first);
    assert!(output.status.success(), "{}", text(&output));
    assert!(build(&root, &second).status.success());
    assert_eq!(tree(&first), tree(&second), "两次构建必须字节相同");
    assert_eq!(
        names(&first),
        vec![
            "manifest.json".to_owned(),
            "plan.json".to_owned(),
            "report.json".to_owned(),
            "report.md".to_owned(),
        ]
    );

    let expected = json(&fixture().join("expected.json"));
    let plan = json(&first.join("plan.json"));
    let report = json(&first.join("report.json"));
    assert_eq!(plan["policy"], expected["policy"]);
    assert_eq!(plan["plan_version"], expected["plan_version"]);
    assert_eq!(plan["case_id"], expected["case_id"]);
    assert_eq!(plan["actions"], expected["actions"]);
    assert_eq!(plan["protected"], expected["protected"]);
    assert_eq!(plan["recovery_loss"], expected["recovery_loss"]);
    assert_eq!(plan["trace_changes"], expected["trace_changes"]);
    for key in [
        "categories",
        "entries",
        "published",
        "deleted",
        "kept",
        "protected",
        "deleted_bytes",
        "kept_bytes",
        "lost_recovery",
        "shortened_traces",
        "model_calls",
        "network_requests",
    ] {
        assert_eq!(
            report["counts"][key], expected["counts"][key],
            "counts.{key}"
        );
    }
    let manifest = json(&first.join("manifest.json"));
    assert!(manifest.get("created_at").is_none(), "输出清单不得带时间戳");
    assert_eq!(manifest["producer"], "retention_plan_output");
    assert_eq!(manifest["counts"], report["counts"]);
}

#[test]
fn an_expired_entry_is_deleted_and_a_fresh_one_is_kept() {
    let sandbox = sandbox("actions");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());
    let plan = json(&output.join("plan.json"));
    let actions = plan["actions"].as_array().expect("actions");
    let find = |id: &str| {
        actions
            .iter()
            .find(|action| action["entry_id"] == id)
            .cloned()
            .expect("entry")
    };
    assert_eq!(find("cache-1")["action"], "delete");
    assert_eq!(find("cache-1")["reason"], "超过保留期");
    assert_eq!(find("log-2")["action"], "keep");
    assert_eq!(find("log-2")["reason"], "未超过保留期");
    assert_eq!(find("log-1")["action"], "delete");
}

#[test]
fn published_requirements_are_kept_and_listed_as_protected() {
    let sandbox = sandbox("protected-kept");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());
    let plan = json(&output.join("plan.json"));
    let actions = plan["actions"].as_array().expect("actions");
    let index = actions
        .iter()
        .find(|action| action["entry_id"] == "index-1")
        .expect("index entry");
    assert_eq!(index["action"], "keep");
    assert_eq!(index["reason"], "已发布版本仍需引用");
    assert_eq!(index["release_id"], "rel-1");
    let protected = plan["protected"].as_array().expect("protected");
    assert_eq!(protected.len(), 2);
    assert_eq!(protected[0]["entry_id"], "index-1");
    assert_eq!(protected[0]["reason"], "已发布版本必需的索引条目");
    assert_eq!(protected[1]["entry_id"], "trace-1");
    assert_eq!(protected[1]["reason"], "已发布版本必需的追溯条目");
}

#[test]
fn the_impact_of_each_deletion_is_spelled_out() {
    let sandbox = sandbox("impact");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());
    let plan = json(&output.join("plan.json"));
    let loss = plan["recovery_loss"].as_array().expect("recovery_loss");
    assert_eq!(loss.len(), 1);
    assert_eq!(loss[0]["entry_id"], "log-1");
    assert_eq!(loss[0]["recovery"], "按日志重放失败任务");
    let traces = plan["trace_changes"].as_array().expect("trace_changes");
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0]["trace"], "trace-parse");
    assert_eq!(traces[0]["before"], 3);
    assert_eq!(traces[0]["after"], 2);
    assert_eq!(traces[0]["removed"], serde_json::json!(["log-1"]));
}

#[test]
fn the_markdown_shows_the_actions_the_impact_and_the_boundaries() {
    let sandbox = sandbox("markdown");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());
    let markdown = fs::read_to_string(output.join("report.md")).expect("report.md");
    for heading in [
        "# 保留与清理计划（retention-cleanup-v1）",
        "## 清理规模",
        "## 逐条动作",
        "## 影响说明",
        "## 受保护条目",
        "## 边界",
    ] {
        assert!(markdown.contains(heading), "缺少 {heading}");
    }
    assert!(markdown.contains("| cache-1 | cache | 9000 | 30 天 | 45 天 | 删除 | 超过保留期 |"));
    assert!(markdown.contains("恢复能力「按日志重放失败任务」随之丧失"));
    assert!(markdown.contains("追溯链 trace-parse 会从 3 条变短到 2 条，删除的是 log-1"));
    assert!(markdown.contains("index-1（rel-1）：已发布版本必需的索引条目。"));
    assert!(markdown.contains("只生成计划：不删除任何文件"));
    assert!(markdown.contains("命中保护即整条拒绝"));
}

#[test]
fn build_refuses_an_existing_output_root() {
    let sandbox = sandbox("existing-out");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let output = sandbox.join("out");
    fs::create_dir(&output).expect("out");
    let result = build(&root, &output);
    assert_eq!(result.status.code(), Some(2), "{}", text(&result));
    assert!(
        text(&result).contains("输出根必须尚不存在"),
        "{}",
        text(&result)
    );
}

#[test]
fn build_refuses_an_output_inside_the_frozen_root() {
    let sandbox = sandbox("inside");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let output = root.join("out");
    let result = build(&root, &output);
    assert!(!result.status.success());
    assert!(
        text(&result).contains("输出根不能与保留根互相包含"),
        "{}",
        text(&result)
    );
    assert!(!output.exists());
}

#[test]
fn a_changed_declaration_is_detected_before_any_report_is_written() {
    let sandbox = sandbox("tampered");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let frozen = root.join("control/project.json");
    let mut payload: serde_json::Value = json(&frozen);
    payload["entries"][2]["age_days"] = serde_json::json!(1);
    fs::write(&frozen, serde_json::to_vec(&payload).expect("encode")).expect("write");
    let output = sandbox.join("out");
    let result = build(&root, &output);
    assert!(!result.status.success());
    assert!(
        text(&result).contains("RETENTION_INTEGRITY"),
        "{}",
        text(&result)
    );
    assert!(!output.exists(), "被拒绝的构建不得留下输出根");
}

#[test]
fn an_extra_entry_in_the_control_directory_is_refused() {
    let sandbox = sandbox("extra");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    fs::write(root.join("control/note.txt"), b"note").expect("note");
    let output = sandbox.join("out");
    let result = build(&root, &output);
    assert!(!result.status.success());
    assert!(
        text(&result).contains("控制目录必须恰好是"),
        "{}",
        text(&result)
    );
}

#[test]
fn an_unknown_root_is_reported_as_such_and_left_alone() {
    let sandbox = sandbox("unknown");
    let root = sandbox.join("empty");
    fs::create_dir(&root).expect("root");
    let before = tree(&root);
    let output = cli(&["retention", "status", &arg(&root)]);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(
        text(&output).contains("RETENTION_UNKNOWN_ROOT"),
        "{}",
        text(&output)
    );
    assert_eq!(tree(&root), before);

    let missing = cli(&["retention", "status", &arg(&sandbox.join("nope"))]);
    assert_eq!(missing.status.code(), Some(3), "{}", text(&missing));
    assert!(
        text(&missing).contains("RETENTION_UNKNOWN_ROOT"),
        "{}",
        text(&missing)
    );
}

#[test]
fn status_and_list_report_the_same_plan_without_writing() {
    let sandbox = sandbox("read-only");
    let root = sandbox.join("root");
    assert!(adopt(&root, "project.json").status.success());
    let before = tree(&root);
    let status = cli(&["retention", "status", &arg(&root)]);
    assert!(status.status.success(), "{}", text(&status));
    let listing = cli(&["retention", "list", &arg(&root)]);
    assert!(listing.status.success(), "{}", text(&listing));
    assert_eq!(tree(&root), before, "只读动词不得修改保留根");

    let summary = text(&status);
    assert!(
        summary.contains("删除 2 条（9400 字节）、保留 3 条（2200 字节），受保护 2 条"),
        "{summary}"
    );
    assert!(
        summary.contains("  - cache：删除 1 条、保留 2 条"),
        "{summary}"
    );
    assert!(
        summary.contains("  - run-logs：删除 1 条、保留 1 条"),
        "{summary}"
    );
    let payload: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&listing.stdout)).expect("json");
    assert_eq!(payload["policy"], "retention-policy-v1");
    assert_eq!(payload["producer"], "noetome-retention");
    assert_eq!(payload["counts"]["deleted"], 2);
    assert_eq!(payload["counts"]["protected"], 2);
    assert_eq!(payload["actions"].as_array().expect("actions").len(), 5);
    assert_eq!(payload["protected"].as_array().expect("protected").len(), 2);
    assert_eq!(payload["model_calls"], 0);
    assert_eq!(payload["network_requests"], 0);
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["retention", "--help"]);
    assert!(help.status.success());
    let body = text(&help);
    assert!(body.contains("Noetome 保留与清理"), "{body}");
    assert!(body.contains("整条拒绝而不是跳过它们继续清理"), "{body}");

    let invalid = cli(&["retention", "adopt", "only-one"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(
        text(&invalid).contains("CLI_INVALID_ARGUMENT"),
        "{}",
        text(&invalid)
    );
    let unknown = cli(&["retention", "frobnicate"]);
    assert_eq!(unknown.status.code(), Some(2));
}

#[test]
fn the_parsed_command_round_trips() {
    let project = fixture().join("project.json");
    match noetome_cli::parse(&[
        "retention".into(),
        "adopt".into(),
        project.clone().into_os_string(),
        "/tmp/root".into(),
    ])
    .expect("parse adopt")
    {
        CliCommand::RetentionAdopt {
            project: parsed,
            root,
        } => {
            assert_eq!(parsed, project);
            assert_eq!(root, PathBuf::from("/tmp/root"));
        }
        other => panic!("意外的命令：{other:?}"),
    }
    match noetome_cli::parse(&["retention".into(), "--help".into()]).expect("parse help") {
        CliCommand::RetentionHelp => {}
        other => panic!("意外的命令：{other:?}"),
    }
    assert!(noetome_cli::parse(&["retention".into(), "build".into()]).is_err());
}
