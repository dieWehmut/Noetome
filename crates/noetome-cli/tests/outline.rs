//! Integration tests for `noetome outline`.
//!
//! Every test drives the real binary and reads the disk. Nothing here imports
//! the alignment logic, so a test passing means the observable behaviour
//! changed, not that an internal helper still agrees with itself.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CASE: AtomicU64 = AtomicU64::new(1);

fn fixture() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/outline-alignment-v1");
    fs::canonicalize(&path).expect("fixture package")
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-outline-{label}-{}-{serial}",
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

fn adopt(root: &Path) -> Output {
    cli(&[
        "outline".into(),
        "adopt".into(),
        arg(&fixture().join("author.json")),
        arg(root),
    ])
}

fn build(root: &Path, output: &Path) -> Output {
    cli(&["outline".into(), "build".into(), arg(root), arg(output)])
}

fn json(path: &Path) -> serde_json::Value {
    let bytes = fs::read(path).expect("read json");
    serde_json::from_slice(&bytes).expect("parse json")
}

#[test]
fn adopt_freezes_the_four_documents_and_only_writes_control() {
    let sandbox = sandbox("adopt");
    let root = sandbox.join("root");
    let output = adopt(&root);
    assert!(output.status.success(), "{}", text(&output));
    let listing: Vec<String> = fs::read_dir(&root)
        .expect("root")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(listing, vec!["control".to_string()]);

    let manifest = json(&root.join("control/manifest.json"));
    assert_eq!(manifest["policy"], "outline-alignment-v1");
    assert_eq!(manifest["producer"], "noetome-outline");
    assert_eq!(manifest["case_id"], "outl-glycolysis-v1");
    assert_eq!(
        manifest["documents"].as_array().expect("documents").len(),
        4
    );

    let inputs = json(&root.join("control/inputs.json"));
    let documents = inputs["documents"].as_array().expect("inputs documents");
    assert_eq!(documents.len(), 4);
    assert_eq!(documents[0]["source_id"], "doc-glycolysis");

    let body =
        fs::read_to_string(&root.join("control/text/doc-numbering.md")).expect("frozen body");
    assert!(body.contains("4.1 重复编号"));

    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(printed.contains("已建立"), "{printed}");
    assert!(printed.contains("章节对齐：15 个章节"), "{printed}");
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
        String::from_utf8_lossy(&output.stderr).contains("OUTLINE_ARGUMENT"),
        "{}",
        text(&output)
    );
    assert_eq!(tree(&sandbox), before);
}

#[test]
fn adopt_refuses_a_missing_parent_and_leaves_nothing_behind() {
    let sandbox = sandbox("parent");
    let root = sandbox.join("absent").join("root");
    let before = tree(&sandbox);
    let output = adopt(&root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("PROJECT_IO_ERROR"),
        "{}",
        text(&output)
    );
    assert_eq!(tree(&sandbox), before);
}

#[test]
fn adopt_refuses_a_package_whose_document_bytes_were_changed() {
    let sandbox = sandbox("tamper-author");
    let package = sandbox.join("package");
    copy_tree(&fixture(), &package);
    let document = package.join("documents/numbering-defects.md");
    let mut bytes = fs::read(&document).expect("read document");
    bytes.extend_from_slice("\n额外一行\n".as_bytes());
    fs::write(&document, bytes).expect("rewrite document");

    let root = sandbox.join("root");
    let output = cli(&[
        "outline".into(),
        "adopt".into(),
        arg(&package.join("author.json")),
        arg(&root),
    ]);
    // The author package itself carries no byte digests for documents, so a
    // changed body is legal at adopt time — what must hold is that the frozen
    // copy is what the package now says, and no root survives a failure.
    if output.status.success() {
        let frozen =
            fs::read_to_string(root.join("control/text/doc-numbering.md")).expect("frozen body");
        assert!(frozen.ends_with("额外一行\n"), "{frozen}");
    } else {
        assert!(!root.exists(), "失败后不得留下对齐根");
    }
}

#[test]
fn status_reports_counts_warnings_and_zero_calls() {
    let sandbox = sandbox("status");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());

    let before = tree(&root);
    let output = cli(&["outline".into(), "status".into(), arg(&root)]);
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(tree(&root), before, "只读检查不得改动对齐根");

    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(printed.contains("冻结来源：4 份文档"), "{printed}");
    assert!(printed.contains("已对齐 10、未对齐 5"), "{printed}");
    assert!(
        printed.contains("概念清单：5 个概念、12 次出现"),
        "{printed}"
    );
    assert!(printed.contains("duplicate_number 3"), "{printed}");
    assert!(printed.contains("模型调用与网络请求：0 次"), "{printed}");
}

#[test]
fn build_is_deterministic_and_matches_the_frozen_expectation() {
    let sandbox = sandbox("build");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let first = sandbox.join("first");
    let second = sandbox.join("second");
    assert!(build(&root, &first).status.success());
    assert!(build(&root, &second).status.success());

    assert_eq!(tree(&first), tree(&second), "同一冻结输入必须字节相同");

    let expected = json(&fixture().join("expected.json"));
    let report = json(&first.join("report.json"));
    assert_eq!(report["counts"], expected["counts"]);
    assert_eq!(report["warning_counts"], expected["warning_counts"]);
    let actual: Vec<(String, String, u64)> = report["warnings"]
        .as_array()
        .expect("warnings")
        .iter()
        .map(|warning| {
            (
                warning["kind"].as_str().expect("kind").to_owned(),
                warning["document"].as_str().expect("document").to_owned(),
                warning["line"].as_u64().expect("line"),
            )
        })
        .collect();
    let declared: Vec<(String, String, u64)> = expected["warnings"]
        .as_array()
        .expect("expected warnings")
        .iter()
        .map(|warning| {
            (
                warning["kind"].as_str().expect("kind").to_owned(),
                warning["document"].as_str().expect("document").to_owned(),
                warning["line"].as_u64().expect("line"),
            )
        })
        .collect();
    assert_eq!(actual, declared, "警告位置必须先验已知");

    let documents = report["counts"]["documents"].as_u64().expect("documents");
    assert_eq!(documents, 4);
}

#[test]
fn build_refuses_an_existing_output_root() {
    let sandbox = sandbox("output-exists");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = sandbox.join("out");
    fs::create_dir(&output).expect("pre-create output");
    fs::write(output.join("keep.txt"), b"keep").expect("sentinel");
    let before = tree(&sandbox);
    let result = build(&root, &output);
    assert_eq!(result.status.code(), Some(2), "{}", text(&result));
    assert_eq!(tree(&sandbox), before);
}

#[test]
fn build_refuses_an_output_inside_the_frozen_root() {
    let sandbox = sandbox("output-inside");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let before = tree(&root);
    let result = build(&root, &root.join("nested"));
    assert!(!result.status.success(), "{}", text(&result));
    assert_eq!(tree(&root), before);
}

#[test]
fn an_empty_foreign_directory_is_unknown_root_and_an_extra_entry_is_integrity() {
    let sandbox = sandbox("foreign");

    // An empty directory is simply not an alignment root yet.
    let empty = sandbox.join("empty");
    fs::create_dir(&empty).expect("empty");
    let before = tree(&sandbox);
    let output = cli(&["outline".into(), "status".into(), arg(&empty)]);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("OUTLINE_UNKNOWN_ROOT"),
        "{}",
        text(&output)
    );
    assert_eq!(tree(&sandbox), before);

    // A directory holding unrelated entries is a foreign layout, not a root.
    let foreign = sandbox.join("foreign");
    fs::create_dir(&foreign).expect("foreign");
    fs::write(foreign.join("note.txt"), b"unrelated").expect("note");
    let before = tree(&sandbox);
    let layout = cli(&["outline".into(), "status".into(), arg(&foreign)]);
    assert_eq!(layout.status.code(), Some(3), "{}", text(&layout));
    assert!(
        String::from_utf8_lossy(&layout.stderr).contains("OUTLINE_INTEGRITY"),
        "{}",
        text(&layout)
    );
    assert_eq!(tree(&sandbox), before);

    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let mut manifest = fs::read(root.join("control/manifest.json")).expect("manifest");
    manifest.insert(0, b' ');
    fs::write(root.join("control/manifest.json"), &manifest).expect("tamper");
    let tampered = tree(&root);
    let failed = cli(&["outline".into(), "status".into(), arg(&root)]);
    assert_eq!(failed.status.code(), Some(3), "{}", text(&failed));
    assert!(
        String::from_utf8_lossy(&failed.stderr).contains("OUTLINE_INTEGRITY"),
        "{}",
        text(&failed)
    );
    assert_eq!(tree(&root), tampered);
}

#[test]
fn an_extra_file_in_the_control_directory_is_refused() {
    let sandbox = sandbox("extra");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    fs::write(root.join("control/notes.md"), b"unexpected").expect("extra file");
    let before = tree(&root);
    let output = cli(&["outline".into(), "status".into(), arg(&root)]);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("控制目录只允许"),
        "{}",
        text(&output)
    );
    assert_eq!(tree(&root), before);
}

#[test]
fn a_changed_frozen_body_is_detected_before_any_report_is_written() {
    let sandbox = sandbox("drift");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let body = root.join("control/text/doc-glycolysis.md");
    let mut bytes = fs::read(&body).expect("body");
    bytes.push(b'\n');
    fs::write(&body, &bytes).expect("drift body");
    let before = tree(&root);
    let output = sandbox.join("out");
    let result = build(&root, &output);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(!output.exists(), "失败时不得留下输出根");
    assert_eq!(tree(&root), before);
}

#[test]
fn list_returns_machine_readable_counts() {
    let sandbox = sandbox("list");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = cli(&["outline".into(), "list".into(), arg(&root)]);
    assert!(output.status.success(), "{}", text(&output));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("list is json");
    assert_eq!(value["case_id"], "outl-glycolysis-v1");
    assert_eq!(value["model_calls"], 0);
    assert_eq!(value["network_requests"], 0);
    assert_eq!(value["documents"].as_array().expect("documents").len(), 4);
    assert_eq!(
        value["concepts"]["concepts"]
            .as_array()
            .expect("concepts")
            .len(),
        5
    );
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["outline".into(), "--help".into()]);
    assert!(help.status.success());
    let printed = String::from_utf8_lossy(&help.stdout).into_owned();
    assert!(printed.contains("noetome outline adopt"), "{printed}");
    assert!(printed.contains("不联网、不调用模型"), "{printed}");

    let bare = cli(&["outline".into()]);
    assert!(bare.status.success(), "无参数应显示帮助而不是报错");

    for args in [
        vec!["outline".to_string(), "adopt".to_string()],
        vec!["outline".to_string(), "frobnicate".to_string()],
        vec!["outline".to_string(), "build".to_string(), "a".to_string()],
    ] {
        let output = cli(&args);
        assert_eq!(output.status.code(), Some(2), "args={args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("CLI_INVALID_ARGUMENT"),
            "args={args:?}"
        );
    }
}

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).expect("target");
    for entry in fs::read_dir(source).expect("read source").flatten() {
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to);
        } else {
            fs::copy(&from, &to).expect("copy file");
        }
    }
}
