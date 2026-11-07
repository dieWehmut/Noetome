//! Integration tests for `noetome relevance`.
//!
//! Every test drives the real binary and reads the disk. Nothing here imports
//! the filter logic, so a test passing means the observable behaviour changed,
//! not that an internal helper still agrees with itself.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CASE: AtomicU64 = AtomicU64::new(1);

fn fixture() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/relevance-filter-v1");
    fs::canonicalize(&path).expect("fixture package")
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-relevance-{label}-{}-{serial}",
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

fn adopt_file(author: &str, root: &Path) -> Output {
    cli(&[
        "relevance".into(),
        "adopt".into(),
        arg(&fixture().join(author)),
        arg(root),
    ])
}

fn adopt(root: &Path) -> Output {
    adopt_file("author.json", root)
}

fn build(root: &Path, output: &Path) -> Output {
    cli(&["relevance".into(), "build".into(), arg(root), arg(output)])
}

fn json(path: &Path) -> serde_json::Value {
    let bytes = fs::read(path).expect("read json");
    serde_json::from_slice(&bytes).expect("parse json")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("read text")
}

#[test]
fn adopt_freezes_three_sources_and_only_writes_control() {
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
    assert_eq!(manifest["policy"], "relevance-filter-v1");
    assert_eq!(manifest["producer"], "noetome-relevance");
    assert_eq!(manifest["case_id"], "rlv-glycolysis-v1");
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
            "atoms.json".to_string(),
            "audience.json".to_string(),
            "author.json".to_string(),
            "inputs.json".to_string(),
            "manifest.json".to_string(),
            "review.json".to_string(),
            "text".to_string(),
        ]
    );

    let body = read(&root.join("control/text/doc-overshoot.md"));
    assert!(body.contains("单分子荧光共振能量转移"), "{body}");

    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(printed.contains("已建立"), "{printed}");
    assert!(
        printed.contains("保留正文片段 4 个、排除 6 个"),
        "{printed}"
    );
    assert!(printed.contains("原子 6 个：保留 5、暂缓 1"), "{printed}");
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
        text(&output).contains("RELEVANCE_ARGUMENT"),
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
fn a_missing_prerequisite_is_refused_rather_than_quietly_omitted() {
    let sandbox = sandbox("prerequisite");
    let root = sandbox.join("root");
    let output = adopt_file("author-insufficient.json", &root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    let printed = text(&output);
    assert!(
        printed.contains("RELEVANCE_PREREQUISITE_MISSING"),
        "{printed}"
    );
    assert!(printed.contains("不能静默省略"), "{printed}");
    assert_eq!(tree(&sandbox).len(), 0, "{:?}", tree(&sandbox));
}

#[test]
fn excluding_an_atom_the_level_needs_is_refused() {
    let sandbox = sandbox("required");
    let root = sandbox.join("root");
    let output = adopt_file("author-advanced.json", &root);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    let printed = text(&output);
    assert!(printed.contains("RELEVANCE_REQUIRED_EXCLUDED"), "{printed}");
    assert!(printed.contains("atom-pfk-fret"), "{printed}");
    assert_eq!(tree(&sandbox).len(), 0, "{:?}", tree(&sandbox));
}

#[test]
fn an_unregistered_or_unknown_reason_is_refused() {
    for (author, needle) in [
        ("author-unknown-reason.json", "boring"),
        ("author-unregistered-reason.json", "pure_exercise"),
    ] {
        let sandbox = sandbox("reason");
        let root = sandbox.join("root");
        let output = adopt_file(author, &root);
        assert_eq!(output.status.code(), Some(2), "{}", text(&output));
        let printed = text(&output);
        assert!(printed.contains("RELEVANCE_ARGUMENT"), "{printed}");
        assert!(printed.contains(needle), "{printed}");
        assert_eq!(tree(&sandbox).len(), 0, "{:?}", tree(&sandbox));
    }
}

#[test]
fn status_reports_counts_reasons_and_both_densities() {
    let sandbox = sandbox("status");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());

    let before = tree(&root);
    let output = cli(&["relevance".into(), "status".into(), arg(&root)]);
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(tree(&root), before);

    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(printed.contains("用户水平：university"), "{printed}");
    assert!(
        printed.contains("保留正文片段 4 个、排除 6 个"),
        "{printed}"
    );
    assert!(printed.contains("beyond_target_level 2"), "{printed}");
    assert!(printed.contains("原子 6 个：保留 5、暂缓 1"), "{printed}");
    assert!(printed.contains("条件集合完全一致"), "{printed}");
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
            "densities.json".to_string(),
            "filter.json".to_string(),
            "manifest.json".to_string(),
            "report.json".to_string(),
            "report.md".to_string(),
        ]
    );

    // The expected counts were written before the implementation existed.
    let expected = json(&fixture().join("expected.json"));
    let report = json(&first.join("report.json"));
    // The expected file is the prior: every count it names must match. The
    // report may carry extra derived counts (for example the declared atom
    // total), which the prior does not have to enumerate.
    for (key, value) in expected["counts"].as_object().expect("counts") {
        assert_eq!(&report["counts"][key], value, "counts.{key}");
    }
    assert_eq!(report["counts"]["atoms"], 6);
    assert_eq!(report["reason_counts"], expected["reason_counts"]);
    assert_eq!(report["level"], expected["level"]);
    assert_eq!(report["model_calls"], 0);
    assert_eq!(report["network_requests"], 0);
    assert_eq!(report["unchanged_across_densities"], true);

    let densities = json(&first.join("densities.json"));
    assert_eq!(
        densities["minimal"]["counts"],
        expected["densities"]["minimal"]
    );
    assert_eq!(
        densities["detailed"]["counts"],
        expected["densities"]["detailed"]
    );
    assert_eq!(densities["unchanged_across_densities"], true);

    let exclusions = expected["exclusions"].as_array().expect("exclusions");
    let spans = json(&first.join("filter.json"));
    let spans = spans["spans"].as_array().expect("spans");
    let excluded: Vec<(String, usize, String)> = spans
        .iter()
        .filter(|span| span["status"] == "excluded")
        .map(|span| {
            (
                span["source_id"].as_str().expect("source").to_owned(),
                span["start_line"].as_u64().expect("line") as usize,
                span["reason"].as_str().expect("reason").to_owned(),
            )
        })
        .collect();
    assert_eq!(excluded.len(), exclusions.len(), "{excluded:?}");
    for item in exclusions {
        let source = item["source_id"].as_str().expect("source");
        let line = item["line"].as_u64().expect("line") as usize;
        let reason = item["reason"].as_str().expect("reason");
        assert!(
            excluded
                .iter()
                .any(|(kept_source, kept_line, kept_reason)| {
                    kept_source == source
                        && *kept_line <= line
                        && line <= *kept_line
                        && kept_reason == reason
                }),
            "missing exclusion {source}:{line} {reason} in {excluded:?}"
        );
    }
}

#[test]
fn the_minimal_and_detailed_densities_keep_the_same_atoms_and_conditions() {
    let sandbox = sandbox("density");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());

    let densities = json(&output.join("densities.json"));
    let minimal = densities["minimal"]["atoms"].as_array().expect("minimal");
    let detailed = densities["detailed"]["atoms"].as_array().expect("detailed");
    assert_eq!(minimal.len(), 5);
    assert_eq!(detailed.len(), 5);
    for (plain, rich) in minimal.iter().zip(detailed.iter()) {
        assert_eq!(plain["atom_id"], rich["atom_id"]);
        assert_eq!(plain["conditions"], rich["conditions"]);
        assert_eq!(plain["requires"], rich["requires"]);
        assert!(plain["expansion"].is_null(), "{plain}");
        assert!(rich["expansion"].as_str().is_some(), "{rich}");
    }
    assert_eq!(
        densities["minimal"]["counts"],
        densities["detailed"]["counts"]
    );
}

#[test]
fn the_markdown_record_lists_counts_reasons_and_boundaries() {
    let sandbox = sandbox("markdown");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let output = sandbox.join("out");
    assert!(build(&root, &output).status.success());

    let markdown = read(&output.join("report.md"));
    for needle in [
        "# 相关性筛选记录（rlv-glycolysis-v1）",
        "## 计数",
        "## 排除理由分布",
        "## 排除明细",
        "## 保留原子与两种密度",
        "## 边界",
        "| beyond_target_level | 2 |",
        "| pure_exercise | 1 |",
        "atom-pfk-fret",
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
        text(&result).contains("RELEVANCE_ARGUMENT"),
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
    let body = root.join("control/text/doc-basics.md");
    fs::write(&body, "# 糖酵解速览\n被改写的正文。\n").expect("tamper");

    let output = sandbox.join("out");
    let result = build(&root, &output);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(
        text(&result).contains("RELEVANCE_INTEGRITY"),
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
    let result = cli(&["relevance".into(), "status".into(), arg(&root)]);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(
        text(&result).contains("RELEVANCE_INTEGRITY"),
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
        "relevance".into(),
        "status".into(),
        arg(&sandbox.join("none")),
    ]);
    assert_eq!(result.status.code(), Some(3), "{}", text(&result));
    assert!(
        text(&result).contains("RELEVANCE_UNKNOWN_ROOT"),
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
    let output = cli(&["relevance".into(), "list".into(), arg(&root)]);
    assert!(output.status.success(), "{}", text(&output));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("list is json");
    assert_eq!(value["case_id"], "rlv-glycolysis-v1");
    assert_eq!(value["model_calls"], 0);
    assert_eq!(value["network_requests"], 0);
    assert_eq!(value["audience"]["level"], "university");
    assert_eq!(value["filter"]["counts"]["excluded_spans"], 6);
    assert_eq!(value["minimal"]["counts"]["atoms"], 5);
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["relevance".into(), "--help".into()]);
    assert!(help.status.success(), "{}", text(&help));
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("用户水平与相关性筛选"),
        "{}",
        text(&help)
    );

    let bare = cli(&["relevance".into()]);
    assert!(bare.status.success(), "{}", text(&bare));

    let bad = cli(&["relevance".into(), "frobnicate".into(), "x".into()]);
    assert_eq!(bad.status.code(), Some(2), "{}", text(&bad));
    assert!(
        text(&bad).contains("CLI_INVALID_ARGUMENT"),
        "{}",
        text(&bad)
    );
}
