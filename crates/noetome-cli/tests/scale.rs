//! Black-box tests for the scale harness.
//!
//! Every test runs the real `noetome` binary. The cheap eight-page case is used
//! everywhere a rule is being checked; one test does the thousand-page run because
//! that is the only way to know the promise "千页规模真的跑过" is true.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_noetome"))
}

fn fixture() -> PathBuf {
    std::fs::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/scale-harness-v1"),
    )
    .expect("fixture directory")
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-scale-{label}-{}-{serial}",
        std::process::id()
    ));
    if path.exists() {
        std::fs::remove_dir_all(&path).expect("clean sandbox");
    }
    std::fs::create_dir_all(&path).expect("sandbox");
    path
}

fn cli(arguments: &[&str]) -> Output {
    Command::new(binary())
        .args(arguments)
        .output()
        .expect("run noetome")
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn measure(output: &Path, pages: &str, headings: &str, paragraphs: &str) -> Output {
    cli(&[
        "scale",
        "measure",
        "--pages",
        pages,
        "--headings",
        headings,
        "--paragraphs",
        paragraphs,
        "--output",
        &output.to_string_lossy(),
    ])
}

/// Every regular file below `root`, keyed by its relative path.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).expect("read directory") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("relative")
                    .to_string_lossy()
                    .into_owned();
                files.insert(relative, std::fs::read(&path).expect("read file"));
            }
        }
    }
    files
}

fn json(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).expect("read json"))
        .unwrap_or_else(|error| panic!("{} 不是合法 JSON：{error}", path.display()))
}

fn expected() -> serde_json::Value {
    json(&fixture().join("expected.json"))
}

#[test]
fn the_eight_page_case_matches_the_written_expectation() {
    let directory = sandbox("expected");
    let output = measure(&directory.join("out"), "8", "2", "2");
    assert!(output.status.success(), "measure failed: {}", text(&output));
    let structure = json(&directory.join("out/structure.json"));
    let prior = expected();
    assert_eq!(structure["counts"], prior["counts"]);
    assert_eq!(structure["pages"], prior["pages"]);
    assert_eq!(structure["warning_counts"], prior["warning_counts"]);
    assert_eq!(structure["nfr10_status"], prior["nfr10_status"]);
    for key in ["max_topics", "max_goals", "max_concepts", "max_atoms"] {
        assert_eq!(structure["limits"][key], prior["limits"][key]);
    }
    assert_eq!(structure["model_calls"], 0);
    assert_eq!(structure["network_requests"], 0);
    assert_eq!(structure["spec"]["pages"], 8);
}

#[test]
fn the_output_root_holds_exactly_four_files() {
    let directory = sandbox("layout");
    assert!(
        measure(&directory.join("out"), "8", "2", "2")
            .status
            .success()
    );
    let names: Vec<String> = tree(&directory.join("out")).into_keys().collect();
    assert_eq!(
        names,
        vec![
            "manifest.json".to_owned(),
            "report.md".to_owned(),
            "structure.json".to_owned(),
            "timings.json".to_owned()
        ]
    );
}

#[test]
fn two_runs_with_the_same_parameters_are_byte_identical_apart_from_the_timings() {
    let directory = sandbox("determinism");
    assert!(
        measure(&directory.join("first"), "8", "2", "2")
            .status
            .success()
    );
    assert!(
        measure(&directory.join("second"), "8", "2", "2")
            .status
            .success()
    );
    let first = tree(&directory.join("first"));
    let second = tree(&directory.join("second"));
    for name in ["structure.json", "manifest.json", "report.md"] {
        assert_eq!(
            first.get(name),
            second.get(name),
            "{name} 在同参数两次运行之间不一致"
        );
    }
    // The timings are allowed to differ, and the manifest must not depend on them.
    let manifest = json(&directory.join("first/manifest.json"));
    assert!(manifest.get("timings_sha256").is_none());
    assert_eq!(
        manifest["structure_sha256"],
        serde_json::Value::String(noetome_cli::text::sha256(
            first.get("structure.json").expect("structure")
        ))
    );
}

#[test]
fn a_different_parameter_changes_the_structure() {
    let directory = sandbox("parameter");
    assert!(
        measure(&directory.join("three"), "8", "2", "2")
            .status
            .success()
    );
    assert!(
        measure(&directory.join("four"), "8", "2", "3")
            .status
            .success()
    );
    let three = json(&directory.join("three/structure.json"));
    let four = json(&directory.join("four/structure.json"));
    assert_ne!(three["counts"]["bytes"], four["counts"]["bytes"]);
    assert_ne!(three["counts"]["spans"], four["counts"]["spans"]);
}

#[test]
fn the_report_states_the_synthetic_boundary_and_the_unmet_limit() {
    let directory = sandbox("report");
    assert!(
        measure(&directory.join("out"), "8", "2", "2")
            .status
            .success()
    );
    let markdown = std::fs::read_to_string(directory.join("out/report.md")).expect("read report");
    for needle in [
        "合成语料",
        "不证明真实教材",
        "未达标项",
        "topics=4、goals=8、concepts=64、atoms=256",
        "timings.json",
        "不联网、不调用模型",
    ] {
        assert!(markdown.contains(needle), "报告缺少「{needle}」");
    }
    assert!(!markdown.contains(" ms"), "报告不应包含耗时读数");
}

#[test]
fn the_summary_prints_the_measured_numbers_and_the_unmet_cap() {
    let directory = sandbox("summary");
    let output = measure(&directory.join("out"), "8", "2", "2");
    assert!(output.status.success());
    let printed = String::from_utf8_lossy(&output.stdout).into_owned();
    for needle in [
        "页面 8",
        "文本片段 96",
        "标题 16",
        "段落 32",
        "规划上限（未达标）：topics=4、goals=8、concepts=64、atoms=256",
    ] {
        assert!(printed.contains(needle), "摘要缺少「{needle}」：{printed}");
    }
}

#[test]
fn an_existing_output_root_is_refused_without_touching_it() {
    let directory = sandbox("existing");
    let output = directory.join("out");
    std::fs::create_dir(&output).expect("existing output");
    std::fs::write(output.join("keep.txt"), "保留").expect("write");
    let before = tree(&output);
    let result = measure(&output, "8", "2", "2");
    assert_eq!(result.status.code(), Some(2));
    assert!(text(&result).contains("SCALE_ARGUMENT"));
    assert!(text(&result).contains("输出根必须尚不存在"));
    assert_eq!(tree(&output), before);
}

#[test]
fn page_bounds_are_refused_before_anything_is_written() {
    let directory = sandbox("bounds");
    for (pages, headings, paragraphs) in [("0", "2", "2"), ("8", "13", "2"), ("8", "2", "13")] {
        let output = directory.join(format!("out-{pages}-{headings}-{paragraphs}"));
        let result = measure(&output, pages, headings, paragraphs);
        assert_eq!(result.status.code(), Some(2), "{}", text(&result));
        assert!(text(&result).contains("SCALE_ARGUMENT"));
        assert!(!output.exists(), "被拒绝时不应创建输出根");
    }
}

#[test]
fn an_unparsable_number_is_refused() {
    let directory = sandbox("number");
    let result = measure(&directory.join("out"), "八", "2", "2");
    assert_eq!(result.status.code(), Some(2));
    assert!(text(&result).contains("SCALE_ARGUMENT"));
    assert!(!directory.join("out").exists());
}

#[test]
fn a_missing_parent_directory_is_reported_as_io() {
    let directory = sandbox("parent");
    let result = measure(&directory.join("missing/out"), "8", "2", "2");
    assert_eq!(result.status.code(), Some(3));
    assert!(text(&result).contains("SCALE_IO"));
    assert!(text(&result).contains("无法访问输出根的父目录"));
    assert!(!directory.join("missing").exists());
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let output = cli(&["scale", "--help"]);
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(help.contains("scale measure"));
    assert!(help.contains("未达标项"));

    let bare = cli(&["scale", "measure"]);
    assert_eq!(bare.status.code(), Some(2));
    assert!(text(&bare).contains("CLI_INVALID_ARGUMENT"));

    let unknown = cli(&["scale", "run"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(text(&unknown).contains("CLI_INVALID_ARGUMENT"));

    let missing = cli(&[
        "scale",
        "measure",
        "--pages",
        "8",
        "--headings",
        "2",
        "--output",
        "/tmp/nowhere",
    ]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(text(&missing).contains("CLI_INVALID_ARGUMENT"));
}

#[test]
fn a_thousand_pages_really_run_through_the_text_pipeline() {
    let directory = sandbox("thousand");
    let result = measure(&directory.join("out"), "1000", "4", "3");
    assert!(result.status.success(), "{}", text(&result));
    let structure = json(&directory.join("out/structure.json"));
    assert_eq!(structure["counts"]["pages"], 1000);
    assert_eq!(structure["counts"]["headings"], 4000);
    assert_eq!(structure["counts"]["paragraphs"], 12000);
    assert_eq!(structure["counts"]["spans"], 32000);
    assert!(structure["counts"]["bytes"].as_u64().expect("bytes") > 1_000_000);
    let timings = json(&directory.join("out/timings.json"));
    assert!(timings["analyse_millis"].as_u64().is_some());
    assert!(timings["align_millis"].as_u64().is_some());
    // The thousand-page run must still say the planner has not been shown to scale.
    assert_eq!(structure["nfr10_status"], "not_met");
}
