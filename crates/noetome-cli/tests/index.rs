//! Integration tests for `noetome index`.
//!
//! Every test drives the real binary and reads the disk. Nothing here imports
//! the indexing logic, so a test passing means the observable behaviour
//! changed, not that an internal helper still agrees with itself.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CASE: AtomicU64 = AtomicU64::new(1);

fn fixture() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/source-index-v1");
    fs::canonicalize(&path).expect("fixture package")
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-index-{label}-{}-{serial}",
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
        "index".into(),
        "adopt".into(),
        arg(&fixture().join(project)),
        arg(root),
    ])
}

fn adopt(root: &Path) -> Output {
    adopt_file("project.json", root)
}

fn build(root: &Path, output: &Path) -> Output {
    cli(&["index".into(), "build".into(), arg(root), arg(output)])
}

fn json(path: &Path) -> serde_json::Value {
    let bytes = fs::read(path).expect("read json");
    serde_json::from_slice(&bytes).expect("parse json")
}

#[test]
fn adopt_freezes_the_output_area_and_leaves_the_runtime_area_alone() {
    let sandbox = sandbox("adopt");
    let root = sandbox.join("root");
    let output = adopt(&root);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert!(text(&output).contains("2 份笔记、1 份资源、3 条来源"));

    let top: Vec<String> = fs::read_dir(&root)
        .expect("root")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(top, vec!["control".to_owned()]);

    let control: Vec<String> = fs::read_dir(root.join("control"))
        .expect("control")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    let mut control = control;
    control.sort();
    assert_eq!(
        control,
        vec![
            "files".to_owned(),
            "inputs.json".to_owned(),
            "manifest.json".to_owned(),
            "project.json".to_owned(),
        ]
    );

    // Runtime, temp and the raw materials never get copied into the index root:
    // their digests live in the inputs record, the bytes stay where they are.
    let frozen = tree(&root.join("control/files"));
    let keys: Vec<&String> = frozen.keys().collect();
    assert_eq!(
        keys,
        vec![
            &"vault/笔记/糖异生.md".to_owned(),
            &"vault/笔记/糖酵解.md".to_owned(),
            &"vault/资源/术语表.md".to_owned()
        ]
    );
    assert!(!frozen.contains_key("runtime/session.sqlite"));

    let manifest = json(&root.join("control/manifest.json"));
    assert_eq!(manifest["producer"], "noetome-index");
    assert_eq!(manifest["policy"], "source-index-v1");
}

#[test]
fn adopt_refuses_an_existing_root_without_touching_it() {
    let sandbox = sandbox("adopt-exists");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let before = tree(&root);
    let output = adopt(&root);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("INDEX_ARGUMENT"));
    assert_eq!(tree(&root), before);
}

#[test]
fn adopt_refuses_a_missing_parent_and_leaves_nothing_behind() {
    let sandbox = sandbox("adopt-missing-parent");
    let root = sandbox.join("no-such-parent").join("root");
    let output = adopt(&root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    // 目录不可访问统一按 integrity 报（与 structure/migrate 一致），关键是
    // 失败后不留半个根，而不是错误码措辞。
    assert!(
        text(&output).contains("无法访问索引根的父目录"),
        "{}",
        text(&output)
    );
    assert!(!root.exists());
}

#[test]
fn a_note_with_citation_marks_in_the_body_is_refused() {
    let sandbox = sandbox("citation");
    let root = sandbox.join("root");
    let output = adopt_file("project-citation.json", &root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(text(&output).contains("INDEX_CITATION_BURDEN"));
    assert!(!root.exists(), "语义拒绝不能留下半个根");
}

#[test]
fn a_note_inside_the_runtime_area_is_refused() {
    let sandbox = sandbox("leak");
    let root = sandbox.join("root");
    let output = adopt_file("project-leak.json", &root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(text(&output).contains("INDEX_AREA_LEAK"));
    assert!(!root.exists());
}

#[test]
fn a_source_heading_that_does_not_exist_is_refused() {
    let sandbox = sandbox("heading");
    let root = sandbox.join("root");
    let output = adopt_file("project-bad-heading.json", &root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(text(&output).contains("INDEX_LOCATION_MISSING"));
    assert!(!root.exists());
}

#[test]
fn status_reports_the_areas_the_citation_burden_and_the_relocation() {
    let sandbox = sandbox("status");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let before = tree(&root);
    let output = cli(&["index".into(), "status".into(), arg(&root)]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let body = text(&output);
    for needle in [
        "区域：输出区 vault；运行库 runtime；临时区 temp；重定位目录 moved",
        "冻结来源：2 份笔记、1 份资源、3 条来源、7 个标题",
        "  - note-glycolysis 糖酵解：4 个标题",
        "排除项：运行库 3 个条目、临时区 1 个条目未进入索引",
        "正文负担：文献编号 0 处、绝对路径 0 处、疑似凭据 0 处",
        "重定位：在 moved 里已找到 3、缺失 0、摘要不符 0",
    ] {
        assert!(body.contains(needle), "缺少：{needle}\n{body}");
    }
    assert_eq!(tree(&root), before, "status 必须只读");
}

#[test]
fn build_is_deterministic_and_matches_the_frozen_expectation() {
    let sandbox = sandbox("build");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let first = sandbox.join("out-a");
    let second = sandbox.join("out-b");
    let output = build(&root, &first);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert_eq!(build(&root, &second).status.code(), Some(0));
    assert_eq!(tree(&first), tree(&second), "两次构建必须字节相同");

    let expected = json(&fixture().join("expected.json"));
    let report = json(&first.join("report.json"));
    for (key, value) in expected["counts"].as_object().expect("counts").iter() {
        assert_eq!(&report["counts"][key], value, "counts.{key}");
    }
    let manifest = json(&first.join("manifest.json"));
    assert!(manifest.get("created_at").is_none(), "输出清单不能带时间戳");
    assert_eq!(manifest["producer"], "source_index_output");
    assert_eq!(
        manifest["counts"]["output_files"],
        expected["counts"]["output_files"]
    );

    let entries: Vec<String> = fs::read_dir(&first)
        .expect("output")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    let mut entries = entries;
    entries.sort();
    assert_eq!(
        entries,
        vec![
            "index.json".to_owned(),
            "manifest.json".to_owned(),
            "relocation.json".to_owned(),
            "report.json".to_owned(),
            "report.md".to_owned(),
        ]
    );
}

#[test]
fn the_index_matches_the_frozen_files_and_source_map() {
    let sandbox = sandbox("index");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let output = sandbox.join("out");
    assert_eq!(build(&root, &output).status.code(), Some(0));
    let expected = json(&fixture().join("expected.json"));
    let index = json(&output.join("index.json"));

    assert_eq!(index["files"], expected["files"]);
    for (actual, prior) in index["notes"]
        .as_array()
        .expect("notes")
        .iter()
        .zip(expected["notes"].as_array().expect("notes"))
    {
        assert_eq!(actual["note_id"], prior["note_id"]);
        assert_eq!(actual["path"], prior["path"]);
        assert_eq!(actual["headings"], prior["headings"]);
        assert_eq!(actual["sha256"], prior["sha256"]);
    }
    for (actual, prior) in index["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .zip(expected["sources"].as_array().expect("sources"))
    {
        for key in [
            "source_id",
            "note_id",
            "heading",
            "description",
            "locator",
            "path",
            "sha256",
            "byte_length",
            "digest_short",
        ] {
            assert_eq!(actual[key], prior[key], "sources.{key}");
        }
    }
    // 输出区里的文件必须正好是被声明的那几个：多带哪个都算泄漏。
    for file in index["files"].as_array().expect("files") {
        let path = file["path"].as_str().expect("path");
        assert!(
            !path.starts_with("runtime/") && !path.starts_with("temp/"),
            "运行库或临时区文件混进了输出区：{path}"
        );
    }
}

#[test]
fn the_relocation_finds_renamed_files_and_asks_for_a_rebuild_on_an_edit() {
    let sandbox = sandbox("relocate");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let package = fixture();

    let moved = cli(&[
        "index".into(),
        "relocate".into(),
        arg(&root),
        arg(&package.join("moved")),
    ]);
    assert_eq!(moved.status.code(), Some(0), "{}", text(&moved));
    assert!(text(&moved).contains("合计：已找到 3、缺失 0、摘要不符 0"));

    let partial = cli(&[
        "index".into(),
        "relocate".into(),
        arg(&root),
        arg(&package.join("moved-partial")),
    ]);
    let body = text(&partial);
    assert_eq!(partial.status.code(), Some(0), "{body}");
    assert!(
        body.contains("合计：已找到 2、缺失 0、摘要不符 1"),
        "{body}"
    );
    assert!(
        body.contains("同名不能证明是同一份来源，需要重建映射"),
        "{body}"
    );

    let foreign = cli(&[
        "index".into(),
        "relocate".into(),
        arg(&root),
        arg(&package.join("moved-foreign")),
    ]);
    let body = text(&foreign);
    assert_eq!(foreign.status.code(), Some(0), "{body}");
    assert!(
        body.contains("合计：已找到 0、缺失 3、摘要不符 0"),
        "{body}"
    );
    assert!(body.contains("需要重建映射"), "{body}");
}

#[test]
fn the_markdown_keeps_the_areas_and_the_boundaries_visible() {
    let sandbox = sandbox("markdown");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let output = sandbox.join("out");
    assert_eq!(build(&root, &output).status.code(), Some(0));
    let body = fs::read_to_string(output.join("report.md")).expect("read report");
    for heading in [
        "# 来源索引",
        "## 区域",
        "## 计数",
        "## 文件清单",
        "## 来源映射",
        "## 重定位结果",
        "## 边界",
    ] {
        assert!(body.contains(heading), "缺少小节：{heading}\n{body}");
    }
    assert!(body.contains("不判断内容质量与科学正确性"), "{body}");
    assert!(body.contains("运行库") && body.contains("临时区"), "{body}");
}

#[test]
fn build_refuses_an_existing_output_root() {
    let sandbox = sandbox("build-exists");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let output = sandbox.join("out");
    assert_eq!(build(&root, &output).status.code(), Some(0));
    let before = tree(&output);
    let again = build(&root, &output);
    assert_eq!(again.status.code(), Some(2), "{}", text(&again));
    assert!(text(&again).contains("输出根必须尚不存在"));
    assert_eq!(tree(&output), before);
}

#[test]
fn build_refuses_an_output_inside_the_frozen_root() {
    let sandbox = sandbox("build-inside");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let output = root.join("out");
    let result = build(&root, &output);
    assert_eq!(result.status.code(), Some(2), "{}", text(&result));
    assert!(text(&result).contains("不能与索引根互相包含"));
    assert!(!output.exists());
}

#[test]
fn a_changed_frozen_body_is_detected_before_any_report_is_written() {
    let sandbox = sandbox("changed");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let target = root.join("control/files/vault/笔记/糖酵解.md");
    let mut bytes = fs::read(&target).expect("read");
    bytes.extend_from_slice("\n被人改过\n".as_bytes());
    fs::write(&target, &bytes).expect("write");
    let output = sandbox.join("out");
    let result = build(&root, &output);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(text(&result).contains("INDEX_INTEGRITY"));
    assert!(!output.exists());
}

#[test]
fn an_extra_entry_in_the_control_directory_is_refused() {
    let sandbox = sandbox("extra");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    fs::write(root.join("control/note.txt"), b"extra").expect("write");
    let output = sandbox.join("out");
    let result = build(&root, &output);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(text(&result).contains("控制目录只允许"));
}

#[test]
fn an_unknown_root_is_reported_as_such_and_left_alone() {
    let sandbox = sandbox("unknown");
    let root = sandbox.join("empty");
    fs::create_dir(&root).expect("create");
    let before = tree(&root);
    let result = cli(&["index".into(), "status".into(), arg(&root)]);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(text(&result).contains("INDEX_UNKNOWN_ROOT"));
    assert_eq!(tree(&root), before);
}

#[test]
fn list_returns_machine_readable_counts() {
    let sandbox = sandbox("list");
    let root = sandbox.join("root");
    assert_eq!(adopt(&root).status.code(), Some(0));
    let output = cli(&["index".into(), "list".into(), arg(&root)]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("list is json");
    let expected = json(&fixture().join("expected.json"));
    assert_eq!(value["counts"], expected["counts"]);
    assert_eq!(value["model_calls"], 0);
    assert_eq!(value["network_requests"], 0);
    assert_eq!(value["relocation"]["relocated"], 3);
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["index".into(), "--help".into()]);
    assert_eq!(help.status.code(), Some(0));
    assert!(text(&help).contains("可移植来源索引"));

    let bare = cli(&["index".into()]);
    assert_eq!(bare.status.code(), Some(0));

    let invalid = cli(&["index".into(), "frobnicate".into(), "x".into()]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(text(&invalid).contains("CLI_INVALID_ARGUMENT"));
}
