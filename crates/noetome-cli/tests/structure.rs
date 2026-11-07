//! Integration tests for `noetome structure`.
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
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/structure-family-v1");
    fs::canonicalize(&path).expect("fixture package")
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-structure-{label}-{}-{serial}",
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

fn adopt_file(family: &str, root: &Path) -> Output {
    cli(&[
        "structure".into(),
        "adopt".into(),
        arg(&fixture().join(family)),
        arg(root),
    ])
}

fn adopt(root: &Path) -> Output {
    adopt_file("family.json", root)
}

fn build(root: &Path, output: &Path) -> Output {
    cli(&["structure".into(), "build".into(), arg(root), arg(output)])
}

fn json(path: &Path) -> serde_json::Value {
    let bytes = fs::read(path).expect("read json");
    serde_json::from_slice(&bytes).expect("parse json")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("read text")
}

#[test]
fn adopt_freezes_three_notes_and_only_writes_control() {
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
    assert_eq!(manifest["policy"], "structure-navigation-v1");
    assert_eq!(manifest["producer"], "noetome-structure");
    assert_eq!(manifest["case_id"], "str-glycolysis-v1");
    assert_eq!(
        manifest["documents"].as_array().expect("documents").len(),
        3
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
            "concepts.json".to_string(),
            "family.json".to_string(),
            "inputs.json".to_string(),
            "manifest.json".to_string(),
            "text".to_string(),
        ]
    );

    let body = read(&root.join("control/text/note-ppp.md"));
    assert!(body.contains("磷酸戊糖途径"), "{body}");

    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(printed.contains("已建立"), "{printed}");
    assert!(printed.contains("3 份笔记、5 个骨架节"), "{printed}");
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
        text(&output).contains("STRUCT_ARGUMENT"),
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
fn an_unescaped_alias_in_a_table_is_refused() {
    let sandbox = sandbox("unescaped");
    let root = sandbox.join("root");
    let output = adopt_file("family-unescaped.json", &root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    let printed = text(&output);
    assert!(printed.contains("STRUCT_LINK_UNESCAPED_ALIAS"), "{printed}");
    assert!(printed.contains("未转义的别名"), "{printed}");
    assert_eq!(tree(&sandbox).len(), 0, "{:?}", tree(&sandbox));
}

#[test]
fn explaining_one_concept_twice_is_refused() {
    let sandbox = sandbox("duplicate");
    let root = sandbox.join("root");
    let output = adopt_file("family-duplicate.json", &root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    let printed = text(&output);
    assert!(
        printed.contains("STRUCT_DUPLICATE_EXPLANATION"),
        "{printed}"
    );
    assert!(printed.contains("concept-g6p"), "{printed}");
    assert!(printed.contains("只能有一个规范位置"), "{printed}");
    assert_eq!(tree(&sandbox).len(), 0, "{:?}", tree(&sandbox));
}

#[test]
fn an_unknown_concept_is_refused() {
    let sandbox = sandbox("unknown-concept");
    let root = sandbox.join("root");
    let output = adopt_file("family-unknown-concept.json", &root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    let printed = text(&output);
    assert!(printed.contains("STRUCT_UNKNOWN_CONCEPT"), "{printed}");
    assert!(printed.contains("concept-unknown"), "{printed}");
    assert_eq!(tree(&sandbox).len(), 0, "{:?}", tree(&sandbox));
}

#[test]
fn status_reports_counts_and_deviation_per_note() {
    let sandbox = sandbox("status");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());

    let before = tree(&root);
    let output = cli(&["structure".into(), "status".into(), arg(&root)]);
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(tree(&root), before);

    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(printed.contains("note-glycolysis 糖酵解"), "{printed}");
    assert!(
        printed.contains("5 个章节（就位 5、错位 0、多余 0、缺必填 0、省可选 0、空节 0）"),
        "{printed}"
    );
    assert!(printed.contains("（结构偏离）"), "{printed}");
    assert!(printed.contains("结构一致性：一致 1 / 3 份"), "{printed}");
    assert!(printed.contains("生成 9 条、断链 0"), "{printed}");
    assert!(printed.contains("外部引用 1 个"), "{printed}");
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
            "navigation.md".to_string(),
            "report.json".to_string(),
            "report.md".to_string(),
            "structure.json".to_string(),
        ]
    );

    // The expected file was written before the implementation existed; it is
    // the prior, not a transcript of the product.
    let expected = json(&fixture().join("expected.json"));
    let report = json(&first.join("report.json"));
    assert_eq!(report["case_id"], expected["case_id"]);
    assert_eq!(report["family"], expected["family"]);
    assert_eq!(report["counts"], expected["counts"]);
    assert_eq!(report["model_calls"], 0);
    assert_eq!(report["network_requests"], 0);

    let structure = json(&first.join("structure.json"));
    let notes = structure["notes"].as_array().expect("notes");
    let prior = expected["notes"].as_array().expect("prior notes");
    assert_eq!(notes.len(), prior.len());
    for (note, want) in notes.iter().zip(prior.iter()) {
        for key in [
            "note_id",
            "heading_count",
            "matched",
            "misordered",
            "extra",
            "missing_required",
            "omitted_optional",
            "empty",
            "consistent",
        ] {
            assert_eq!(note[key], want[key], "{}.{key}", want["note_id"]);
        }
        let sections: Vec<&str> = note["sections"]
            .as_array()
            .expect("sections")
            .iter()
            .map(|value| value.as_str().expect("title"))
            .collect();
        let want_sections: Vec<&str> = want["sections"]
            .as_array()
            .expect("want sections")
            .iter()
            .map(|value| value.as_str().expect("title"))
            .collect();
        assert_eq!(sections, want_sections, "{}", want["note_id"]);
    }
}

#[test]
fn the_links_keep_one_canonical_place_and_never_break() {
    let sandbox = sandbox("links");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());

    let expected = json(&fixture().join("expected.json"));
    let links = json(&output.join("links.json"));

    let source = links["source"].as_array().expect("source links");
    let prior = expected["source_links"].as_array().expect("prior");
    assert_eq!(source.len(), prior.len());
    for (link, want) in source.iter().zip(prior.iter()) {
        assert_eq!(link["note_id"], want["note_id"]);
        assert_eq!(link["line"], want["line"]);
        assert_eq!(link["target"], want["target"]);
        assert_eq!(link["heading"], want["heading"]);
        assert_eq!(link["alias"], want["alias"]);
        assert_eq!(link["in_table"], want["in_table"]);
        assert_eq!(link["status"], want["status"]);
    }

    let generated = links["generated"].as_array().expect("generated");
    assert_eq!(generated.len(), 9);
    for link in generated {
        assert_eq!(link["status"], "resolved", "{link}");
        // A canonical place is a note, never a guessed heading.
        assert!(link["target_heading"].is_null(), "{link}");
    }
    assert_eq!(links["counts"]["generated_broken"], 0);

    // concept-pfk is only referenced: no local canonical place, no fake link.
    let concepts = json(&output.join("report.json"));
    let concepts = concepts["concepts"].as_array().expect("concepts");
    let external: Vec<&serde_json::Value> = concepts
        .iter()
        .filter(|entry| entry["canonical"].is_null())
        .collect();
    assert_eq!(external.len(), 1, "{external:?}");
    assert_eq!(external[0]["concept_id"], "concept-pfk");

    let navigation = read(&output.join("navigation.md"));
    assert!(
        navigation.contains("# 导航与复习（糖代谢）"),
        "{navigation}"
    );
    assert!(navigation.contains("不虚构考试频次"), "{navigation}");
}

#[test]
fn the_report_lists_deviation_and_boundaries() {
    let sandbox = sandbox("report");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());

    let markdown = read(&output.join("report.md"));
    for needle in [
        "## 结构一致性",
        "## 概念的唯一解释位置",
        "## 链接",
        "## 边界",
        "note-ppp",
        "延伸阅读",
        "（无本地完整解释）",
        "不判断内容质量与科学正确性",
    ] {
        assert!(markdown.contains(needle), "missing {needle} in {markdown}");
    }
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
        text(&result).contains("STRUCT_ARGUMENT"),
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
    let body = root.join("control/text/note-gluconeogenesis.md");
    fs::write(&body, "# 糖异生\n被改写的正文。\n").expect("tamper");

    let output = sandbox.join("out");
    let result = build(&root, &output);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(
        text(&result).contains("STRUCT_INTEGRITY"),
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
    let result = cli(&["structure".into(), "status".into(), arg(&root)]);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(
        text(&result).contains("STRUCT_INTEGRITY"),
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
        "structure".into(),
        "status".into(),
        arg(&sandbox.join("none")),
    ]);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(
        text(&result).contains("STRUCT_UNKNOWN_ROOT"),
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
    let output = cli(&["structure".into(), "list".into(), arg(&root)]);
    assert!(output.status.success(), "{}", text(&output));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("list is json");
    assert_eq!(value["case_id"], "str-glycolysis-v1");
    assert_eq!(value["model_calls"], 0);
    assert_eq!(value["network_requests"], 0);
    assert_eq!(value["structure"]["counts"]["consistent_notes"], 1);
    assert_eq!(value["links"]["counts"]["generated_links"], 9);
    assert_eq!(value["links"]["counts"]["generated_broken"], 0);
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["structure".into(), "--help".into()]);
    assert!(help.status.success(), "{}", text(&help));
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("同类结构一致性与导航复习"),
        "{}",
        text(&help)
    );

    let bare = cli(&["structure".into()]);
    assert!(bare.status.success(), "{}", text(&bare));

    let bad = cli(&["structure".into(), "frobnicate".into(), "x".into()]);
    assert_eq!(bad.status.code(), Some(2), "{}", text(&bad));
    assert!(
        text(&bad).contains("CLI_INVALID_ARGUMENT"),
        "{}",
        text(&bad)
    );
}
