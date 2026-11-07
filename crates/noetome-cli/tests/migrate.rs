//! Integration tests for `noetome migrate`.
//!
//! Every test drives the real binary and reads the disk. Nothing here imports
//! the migration logic, so a test passing means the observable behaviour
//! changed, not that an internal helper still agrees with itself.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CASE: AtomicU64 = AtomicU64::new(1);

fn fixture() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/link-migration-v1");
    fs::canonicalize(&path).expect("fixture package")
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-migrate-{label}-{}-{serial}",
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

fn adopt_file(plan: &str, root: &Path) -> Output {
    cli(&[
        "migrate".into(),
        "adopt".into(),
        arg(&fixture().join(plan)),
        arg(root),
    ])
}

fn adopt(root: &Path) -> Output {
    adopt_file("plan.json", root)
}

fn build(root: &Path, output: &Path) -> Output {
    cli(&["migrate".into(), "build".into(), arg(root), arg(output)])
}

fn json(path: &Path) -> serde_json::Value {
    let bytes = fs::read(path).expect("read json");
    serde_json::from_slice(&bytes).expect("parse json")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("read text")
}

#[test]
fn adopt_freezes_before_and_after_notes_and_only_writes_control() {
    let sandbox = sandbox("adopt");
    let root = sandbox.join("root");
    let output = adopt(&root);
    assert!(output.status.success(), "{}", text(&output));

    let mut listing: Vec<String> = fs::read_dir(&root)
        .expect("root")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    listing.sort();
    assert_eq!(listing, vec!["control".to_string()]);

    let manifest = json(&root.join("control/manifest.json"));
    assert_eq!(manifest["policy"], "link-migration-v1");
    assert_eq!(manifest["producer"], "noetome-migrate");
    assert_eq!(manifest["case_id"], "mig-section-promotion-v1");
    assert_eq!(
        manifest["documents"].as_array().expect("documents").len(),
        8
    );

    let mut control: Vec<String> = fs::read_dir(root.join("control"))
        .expect("control")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    control.sort();
    assert_eq!(
        control,
        vec![
            "after".to_string(),
            "before".to_string(),
            "inputs.json".to_string(),
            "manifest.json".to_string(),
            "objects.json".to_string(),
            "plan.json".to_string(),
        ]
    );

    let before = read(&root.join("control/before/b-glycolysis.md"));
    assert!(before.contains("参见"), "{before}");
    let after = read(&root.join("control/after/a-glycolysis-mechanism.md"));
    assert!(after.contains("糖酵解"), "{after}");

    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(printed.contains("已建立"), "{printed}");
    assert!(
        printed.contains("3 份提升前笔记、5 份提升后笔记"),
        "{printed}"
    );
    assert!(printed.contains("模型调用与网络请求：0 次"), "{printed}");
}

#[test]
fn adopt_refuses_an_existing_root_without_touching_it() {
    let sandbox = sandbox("existing");
    let root = sandbox.join("root");
    fs::create_dir(&root).expect("pre-create root");
    fs::write(root.join("keep.txt"), b"keep me").expect("write sentinel");

    let before = tree(&sandbox);
    let output = adopt(&root);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(
        text(&output).contains("MIGRATE_ARGUMENT"),
        "{}",
        text(&output)
    );
    assert_eq!(tree(&sandbox), before);
}

#[test]
fn adopt_refuses_a_missing_parent_and_leaves_nothing_behind() {
    let sandbox = sandbox("noparent");
    let root = sandbox.join("missing").join("root");
    let output = adopt(&root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert_eq!(tree(&sandbox).len(), 0, "{:?}", tree(&sandbox));
}

#[test]
fn an_ambiguous_target_is_refused() {
    let sandbox = sandbox("ambiguous");
    let root = sandbox.join("root");
    let output = adopt_file("plan-ambiguous.json", &root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    let printed = text(&output);
    assert!(printed.contains("MIGRATE_AMBIGUOUS_TARGET"), "{printed}");
    assert!(printed.contains("有歧义"), "{printed}");
    assert_eq!(tree(&sandbox).len(), 0, "{:?}", tree(&sandbox));
}

#[test]
fn a_missing_target_is_refused() {
    let sandbox = sandbox("missing-target");
    let root = sandbox.join("root");
    let output = adopt_file("plan-missing-target.json", &root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    let printed = text(&output);
    assert!(printed.contains("MIGRATE_TARGET_MISSING"), "{printed}");
    assert!(printed.contains("不在本迁移计划声明的笔记里"), "{printed}");
    assert_eq!(tree(&sandbox).len(), 0, "{:?}", tree(&sandbox));
}

#[test]
fn an_unescaped_alias_in_a_table_is_refused() {
    let sandbox = sandbox("unescaped");
    let root = sandbox.join("root");
    let output = adopt_file("plan-unescaped-table.json", &root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    let printed = text(&output);
    assert!(printed.contains("MIGRATE_UNESCAPED_ALIAS"), "{printed}");
    assert!(printed.contains("未转义的别名竖线"), "{printed}");
    assert_eq!(tree(&sandbox).len(), 0, "{:?}", tree(&sandbox));
}

#[test]
fn an_object_whose_declared_position_is_missing_is_refused() {
    let sandbox = sandbox("object-missing");
    let root = sandbox.join("root");
    let output = adopt_file("plan-unsafe.json", &root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    let printed = text(&output);
    assert!(printed.contains("MIGRATE_OBJECT_MISSING"), "{printed}");
    assert!(printed.contains("不能靠猜"), "{printed}");
    assert_eq!(tree(&sandbox).len(), 0, "{:?}", tree(&sandbox));
}

#[test]
fn status_reports_each_after_note_and_the_identity_counts() {
    let sandbox = sandbox("status");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());

    let before = tree(&root);
    let output = cli(&["migrate".into(), "status".into(), arg(&root)]);
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(tree(&root), before);

    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    // created 没有对照物：必须是"不适用"，不能写成"否"。
    assert!(
        printed
            .contains("a-glycolysis-mechanism 笔记/糖酵解-核心机制.md（created）：字节相等 不适用"),
        "{printed}"
    );
    assert!(
        printed.contains("a-ppp 笔记/磷酸戊糖途径.md（unchanged）：字节相等 是"),
        "{printed}"
    );
    assert!(
        printed.contains(
            "a-gluconeogenesis 笔记/糖异生.md（links_only）：字节相等 否、剥链接后相等 是"
        ),
        "{printed}"
    );
    assert!(
        printed.contains("引用身份：5 个知识对象被引用 2 个、未被引用 3 个；身份违规 0 个"),
        "{printed}"
    );
    assert!(printed.contains("断链：0 条"), "{printed}");
}

#[test]
fn build_is_deterministic_and_matches_the_frozen_expectation() {
    let sandbox = sandbox("build");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let first = sandbox.join("out-a");
    let second = sandbox.join("out-b");
    let one = build(&root, &first);
    assert!(one.status.success(), "{}", text(&one));
    let two = build(&root, &second);
    assert!(two.status.success(), "{}", text(&two));
    assert_eq!(tree(&first), tree(&second));

    let mut names: Vec<String> = fs::read_dir(&first)
        .expect("output")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "links.json".to_string(),
            "manifest.json".to_string(),
            "objects.json".to_string(),
            "report.json".to_string(),
            "report.md".to_string(),
        ]
    );

    // The expected file was written before the implementation existed; it is
    // the prior, not a transcript of the product.
    let expected = json(&fixture().join("expected.json"));
    let report = json(&first.join("report.json"));
    assert_eq!(report["case_id"], expected["case_id"]);
    assert_eq!(report["counts"], expected["counts"]);
    assert_eq!(report["model_calls"], 0);
    assert_eq!(report["network_requests"], 0);

    let prior_notes = expected["notes"].as_array().expect("prior notes");
    let notes = report["notes"].as_array().expect("notes");
    assert_eq!(notes.len(), prior_notes.len());
    for (note, want) in notes.iter().zip(prior_notes.iter()) {
        for key in ["note_id", "path", "change", "counterpart"] {
            assert_eq!(note[key], want[key], "{}.{key}", want["note_id"]);
        }
        for key in ["bytes_equal", "stripped_equal", "promotion_satisfied"] {
            assert_eq!(note[key], want[key], "{}.{key}", want["note_id"]);
        }
    }

    let manifest = json(&first.join("manifest.json"));
    assert!(
        manifest.get("created_at").is_none(),
        "输出清单不能带时间戳：{manifest}"
    );
    assert_eq!(manifest["producer"], "link_migration_output");
}

#[test]
fn every_reference_resolves_to_the_same_object_after_the_move() {
    let sandbox = sandbox("links");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());

    let expected = json(&fixture().join("expected.json"));
    let links = json(&output.join("links.json"));

    let pairs = [("before", "before_links"), ("after", "after_links")];
    for (side, prior_key) in pairs {
        let got = links[side].as_array().expect("links");
        let want = expected[prior_key].as_array().expect("prior links");
        assert_eq!(got.len(), want.len(), "{side}");
        for (link, prior) in got.iter().zip(want.iter()) {
            for (key, prior_key) in [
                ("note_id", "note_id"),
                ("path", "path"),
                ("line", "line"),
                ("raw", "raw"),
                ("target", "target"),
                ("heading", "heading"),
                ("alias", "alias"),
                ("in_table", "in_table"),
                ("escaped_alias", "escaped"),
                ("object_id", "object_id"),
                ("matched", "match"),
                ("status", "status"),
            ] {
                assert_eq!(link[key], prior[prior_key], "{side}.{prior_key}");
            }
        }
    }

    let placements = json(&output.join("objects.json"));
    let objects = placements["objects"].as_array().expect("objects");
    let prior_objects = expected["objects"].as_array().expect("prior objects");
    assert_eq!(objects.len(), prior_objects.len());
    for (object, prior) in objects.iter().zip(prior_objects.iter()) {
        assert_eq!(object["object_id"], prior["object_id"]);
        assert_eq!(object["after_target"], prior["expected_target"]);
        for key in [
            "kind",
            "term",
            "links_before",
            "links_after",
            "links_after_canonical",
            "links_after_redirect",
            "referenced_before",
            "referenced_after",
            "identity_preserved",
        ] {
            assert_eq!(object[key], prior[key], "{}.{key}", prior["object_id"]);
        }
    }
    assert_eq!(placements["counts"]["identity_violations"], 0);
}

#[test]
fn the_report_and_status_keep_the_redirect_position_visible() {
    let sandbox = sandbox("report");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());

    let markdown = read(&output.join("report.md"));
    for needle in [
        "## 计数",
        "## 知识对象身份",
        "## 迁移后引用",
        "## 笔记核对",
        "## 边界",
        "| redirect |",
        "[[笔记/糖酵解-核心机制.md]]",
        "不判断内容质量与科学正确性",
    ] {
        assert!(markdown.contains(needle), "missing {needle} in {markdown}");
    }
    // 提升后的源笔记必须保留原标题，跳转位置才存在。
    assert!(
        markdown.contains("| 笔记/糖酵解.md | promoted |"),
        "{markdown}"
    );
}

#[test]
fn build_refuses_an_existing_output_root() {
    let sandbox = sandbox("output");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = sandbox.join("out");
    fs::create_dir(&output).expect("pre-create output");
    fs::write(output.join("keep.txt"), b"keep").expect("sentinel");

    let before = tree(&sandbox);
    let result = build(&root, &output);
    assert_eq!(result.status.code(), Some(2), "{}", text(&result));
    assert!(
        text(&result).contains("MIGRATE_ARGUMENT"),
        "{}",
        text(&result)
    );
    assert_eq!(tree(&sandbox), before);
}

#[test]
fn build_refuses_an_output_inside_the_frozen_root() {
    let sandbox = sandbox("inside");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let before = tree(&root);
    let result = build(&root, &root.join("out"));
    assert_eq!(result.status.code(), Some(2), "{}", text(&result));
    assert!(text(&result).contains("互相包含"), "{}", text(&result));
    assert_eq!(tree(&root), before);
}

#[test]
fn a_changed_frozen_body_is_detected_before_any_report_is_written() {
    let sandbox = sandbox("tamper");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let body = root.join("control/after/a-gluconeogenesis.md");
    fs::write(&body, "# 糖异生\n被改写的正文。\n").expect("tamper");

    let output = sandbox.join("out");
    let result = build(&root, &output);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(
        text(&result).contains("MIGRATE_INTEGRITY"),
        "{}",
        text(&result)
    );
    assert!(!output.exists(), "输出根不该被创建");
}

#[test]
fn an_extra_entry_in_the_control_directory_is_refused() {
    let sandbox = sandbox("extra");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    fs::write(root.join("control/notes.txt"), b"extra").expect("extra entry");

    let before = tree(&root);
    let result = cli(&["migrate".into(), "status".into(), arg(&root)]);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(
        text(&result).contains("MIGRATE_INTEGRITY"),
        "{}",
        text(&result)
    );
    assert_eq!(tree(&root), before);
}

#[test]
fn an_unknown_root_is_reported_as_such_and_left_alone() {
    let sandbox = sandbox("unknown");
    let before = tree(&sandbox);
    let result = cli(&[
        "migrate".into(),
        "status".into(),
        arg(&sandbox.join("none")),
    ]);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(
        text(&result).contains("MIGRATE_UNKNOWN_ROOT"),
        "{}",
        text(&result)
    );
    assert_eq!(tree(&sandbox), before);
}

#[test]
fn list_returns_machine_readable_counts() {
    let sandbox = sandbox("list");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = cli(&["migrate".into(), "list".into(), arg(&root)]);
    assert!(output.status.success(), "{}", text(&output));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("list is json");
    assert_eq!(value["case_id"], "mig-section-promotion-v1");
    assert_eq!(value["model_calls"], 0);
    assert_eq!(value["network_requests"], 0);
    assert_eq!(value["report"]["counts"]["broken_links"], 0);
    assert_eq!(value["report"]["counts"]["rewritten_targets"], 2);
    assert_eq!(value["placements"]["counts"]["identity_violations"], 0);
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["migrate".into(), "--help".into()]);
    assert!(help.status.success(), "{}", text(&help));
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("小节提升为独立笔记与链接批量更新"),
        "{}",
        text(&help)
    );

    let bare = cli(&["migrate".into()]);
    assert!(bare.status.success(), "{}", text(&bare));

    let bad = cli(&["migrate".into(), "frobnicate".into(), "x".into()]);
    assert_eq!(bad.status.code(), Some(2), "{}", text(&bad));
    assert!(
        text(&bad).contains("CLI_INVALID_ARGUMENT"),
        "{}",
        text(&bad)
    );
}
