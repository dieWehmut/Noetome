//! Black-box tests for the library-scale measurement verb.
//!
//! Every test runs the real binary against the frozen fixture declaration and
//! reads only files on disk. The clock readings are asserted to exist and to be
//! internally consistent; they are never compared across runs, because they are
//! the one part of the output that is allowed to differ.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_noetome")
}

fn fixture() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/library-bench-v1");
    std::fs::canonicalize(path).expect("fixture")
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-bench-{label}-{}-{serial}",
        std::process::id()
    ));
    if path.exists() {
        std::fs::remove_dir_all(&path).expect("clear sandbox");
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
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

fn measure(output: &Path, notes: &str, edits: &str, samples: &str, warmup: &str) -> Output {
    let output = output.to_str().expect("utf-8 path").to_owned();
    cli(&[
        "bench",
        "measure",
        "--notes",
        notes,
        "--edits",
        edits,
        "--samples",
        samples,
        "--warmup",
        warmup,
        "--output",
        &output,
    ])
}

fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current).expect("read dir") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if entry.file_type().expect("type").is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("relative")
                    .to_string_lossy()
                    .into_owned();
                out.insert(relative, std::fs::read(&path).expect("bytes"));
            }
        }
    }
    out
}

fn json(path: &Path) -> Value {
    let bytes = std::fs::read(path).expect("read json");
    serde_json::from_slice(&bytes).expect("parse json")
}

fn expected() -> Value {
    json(&fixture().join("expected.json"))
}

#[test]
fn the_three_hundred_note_case_matches_the_written_expectation() {
    let sandbox = sandbox("prior");
    let output = sandbox.join("a");
    let result = measure(&output, "300", "20", "3", "0");
    assert!(result.status.success(), "{}", text(&result));

    let structure = json(&output.join("structure.json"));
    let prior = expected();
    assert_eq!(structure["policy"], prior["policy"]);
    assert_eq!(structure["case_id"], prior["case_id"]);
    assert_eq!(structure["spec"], prior["spec"]);
    assert_eq!(structure["counts"], prior["counts"]);
    assert_eq!(structure["corpus_sha256"], prior["corpus_sha256"]);
    assert_eq!(structure["baseline_sha256"], prior["baseline_sha256"]);
    assert_eq!(structure["first_page_sha256"], prior["first_page_sha256"]);
    assert_eq!(structure["threshold_millis"], prior["threshold_millis"]);
    assert_eq!(structure["model_calls"], 0);
    assert_eq!(structure["network_requests"], 0);

    let changed: Vec<&str> = structure["changed_notes"]
        .as_array()
        .expect("array")
        .iter()
        .map(|entry| entry["note_id"].as_str().expect("id"))
        .collect();
    let prior_changed: Vec<&str> = prior["changed_notes"]
        .as_array()
        .expect("array")
        .iter()
        .map(|entry| entry.as_str().expect("id"))
        .collect();
    assert_eq!(changed, prior_changed);

    for key in [
        "samples",
        "warmup",
        "process_start_included",
        "percentile",
        "quantile_method",
    ] {
        assert_eq!(
            structure["protocol"][key], prior["protocol"][key],
            "protocol {key}"
        );
    }
    for (index, coverage) in prior["coverage"]
        .as_array()
        .expect("array")
        .iter()
        .enumerate()
    {
        assert_eq!(
            structure["coverage"][index]["requirement"],
            coverage["requirement"]
        );
        assert_eq!(structure["coverage"][index]["status"], coverage["status"]);
    }
}

#[test]
fn the_output_root_holds_exactly_four_files() {
    let sandbox = sandbox("layout");
    let output = sandbox.join("a");
    manifest_measure(&output);
    let files = tree(&output);
    let mut names: Vec<&str> = files.keys().map(|name| name.as_str()).collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "manifest.json",
            "report.md",
            "structure.json",
            "timings.json"
        ]
    );
}

fn manifest_measure(output: &Path) {
    let result = measure(output, "300", "20", "3", "0");
    assert!(result.status.success(), "{}", text(&result));
}

#[test]
fn two_runs_with_the_same_parameters_are_byte_identical_apart_from_the_timings() {
    let sandbox = sandbox("determinism");
    let first = sandbox.join("a");
    let second = sandbox.join("b");
    manifest_measure(&first);
    manifest_measure(&second);

    for name in ["structure.json", "manifest.json", "report.md"] {
        let left = std::fs::read(first.join(name)).expect("left");
        let right = std::fs::read(second.join(name)).expect("right");
        assert_eq!(left, right, "{name} must not depend on the clock");
    }

    let manifest = json(&first.join("manifest.json"));
    assert!(
        manifest.get("timings_sha256").is_none(),
        "the manifest must not pin the clock readings"
    );
    assert_eq!(
        manifest["structure_sha256"].as_str().expect("digest"),
        noetome_cli::text::sha256(&std::fs::read(first.join("structure.json")).expect("bytes"))
    );
    assert_eq!(
        manifest["structure_byte_length"].as_u64().expect("length"),
        std::fs::metadata(first.join("structure.json"))
            .expect("meta")
            .len()
    );
}

#[test]
fn a_different_parameter_changes_the_structure() {
    let sandbox = sandbox("different");
    let first = sandbox.join("a");
    let second = sandbox.join("b");
    manifest_measure(&first);
    let result = measure(&second, "300", "20", "4", "1");
    assert!(result.status.success(), "{}", text(&result));

    let left = json(&first.join("structure.json"));
    let right = json(&second.join("structure.json"));
    assert_eq!(left["corpus_sha256"], right["corpus_sha256"]);
    assert_ne!(left["protocol"], right["protocol"]);
    assert_ne!(left["spec"], right["spec"]);
}

#[test]
fn the_report_states_the_synthetic_boundary_without_any_clock_reading() {
    let sandbox = sandbox("report");
    let output = sandbox.join("a");
    manifest_measure(&output);
    let markdown = std::fs::read_to_string(output.join("report.md")).expect("markdown");
    for needle in [
        "语料是**合成的**",
        "## 测量协议",
        "nearest_rank",
        "不含进程启动",
        "本文件不含本次运行的任何耗时读数",
        "## 覆盖范围与未达标项",
        "NFR-11（partially_measured）",
        "NFR-12（partially_measured）",
        "没有界面，因此「不因全量渲染冻结界面」没有被验证。",
    ] {
        assert!(markdown.contains(needle), "missing {needle}");
    }
    // The only millisecond figure the report may carry is the constant threshold:
    // any real reading would make two runs of the same parameters differ.
    let mut cursor = 0;
    let mut numbers = Vec::new();
    while let Some(found) = markdown[cursor..].find(" ms") {
        let at = cursor + found;
        let digits: String = markdown[..at]
            .chars()
            .rev()
            .take_while(|character| character.is_ascii_digit())
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect();
        assert!(!digits.is_empty(), "a unit without a number at byte {at}");
        numbers.push(digits.parse::<u64>().expect("millisecond figure"));
        cursor = at + 3;
    }
    assert!(!numbers.is_empty(), "the report states the threshold");
    assert!(
        numbers.iter().all(|value| *value == 2000),
        "the report may only carry the threshold, found {numbers:?}"
    );
}

#[test]
fn the_summary_prints_the_stage_percentiles_and_the_coverage() {
    let sandbox = sandbox("summary");
    let output = sandbox.join("a");
    let result = measure(&output, "300", "20", "3", "0");
    assert!(result.status.success(), "{}", text(&result));
    let printed = String::from_utf8_lossy(&result.stdout).into_owned();
    for needle in [
        "大库实测",
        "笔记 300、人工改动 20、样本 3、预热 0",
        "主题 30、概念 600、文本片段 3080",
        "影响范围 190 篇",
        "scan P95",
        "门槛：2000 ms",
        "覆盖：NFR-11 partially_measured、NFR-12 partially_measured",
    ] {
        assert!(printed.contains(needle), "missing {needle}");
    }
}

#[test]
fn the_timings_are_internally_consistent_and_cover_every_stage() {
    let sandbox = sandbox("timings");
    let output = sandbox.join("a");
    manifest_measure(&output);
    let timings = json(&output.join("timings.json"));
    let stages = timings["stages"].as_array().expect("stages");
    let names: Vec<&str> = stages
        .iter()
        .map(|stage| stage["stage"].as_str().expect("name"))
        .collect();
    assert_eq!(names, vec!["scan", "detect", "plan", "page"]);
    for stage in stages {
        let samples = stage["samples_millis"].as_array().expect("samples");
        assert_eq!(samples.len(), 3, "every stage keeps one reading per sample");
        let numbers: Vec<u64> = samples
            .iter()
            .map(|value| value.as_u64().expect("millis"))
            .collect();
        let mut sorted = numbers.clone();
        sorted.sort_unstable();
        assert_eq!(
            stage["min_millis"].as_u64().expect("min"),
            *sorted.first().expect("first")
        );
        assert_eq!(
            stage["max_millis"].as_u64().expect("max"),
            *sorted.last().expect("last")
        );
        let rank = (95 * sorted.len()).div_ceil(100).max(1);
        assert_eq!(
            stage["p95_millis"].as_u64().expect("p95"),
            sorted[rank.min(sorted.len()) - 1],
            "nearest-rank P95 for {}",
            stage["stage"]
        );
    }
    for verdict in timings["verdicts"].as_array().expect("verdicts") {
        let p95 = verdict["p95_millis"].as_u64().expect("p95");
        let threshold = verdict["threshold_millis"].as_u64().expect("threshold");
        let expected_status = if p95 <= threshold { "met" } else { "not_met" };
        assert_eq!(verdict["status"].as_str().expect("status"), expected_status);
    }
    assert_eq!(timings["threshold_millis"], 2000);
    assert!(
        timings["memory"]["note"]
            .as_str()
            .expect("note")
            .contains("VmRSS")
    );
}

#[test]
fn an_existing_output_root_is_refused_without_touching_it() {
    let sandbox = sandbox("existing");
    let output = sandbox.join("a");
    manifest_measure(&output);
    let before = tree(&output);
    let result = measure(&output, "300", "20", "3", "0");
    assert_eq!(result.status.code(), Some(2));
    assert!(text(&result).contains("BENCH_ARGUMENT"));
    assert!(text(&result).contains("输出根必须尚不存在"));
    assert_eq!(before, tree(&output));
}

#[test]
fn bounds_are_refused_before_anything_is_written() {
    let sandbox = sandbox("bounds");
    for (label, notes, edits, samples, warmup) in [
        ("zero-notes", "0", "0", "3", "0"),
        ("too-many-edits", "300", "301", "3", "0"),
        ("zero-samples", "300", "20", "0", "0"),
        ("too-many-warmup", "300", "20", "3", "51"),
    ] {
        let output = sandbox.join(label);
        let result = measure(&output, notes, edits, samples, warmup);
        assert_eq!(result.status.code(), Some(2), "{label}");
        assert!(text(&result).contains("BENCH_ARGUMENT"), "{label}");
        assert!(!output.exists(), "{label} must not leave a root");
    }
}

#[test]
fn an_unparsable_number_is_refused() {
    let sandbox = sandbox("number");
    let output = sandbox.join("a");
    let result = measure(&output, "abc", "20", "3", "0");
    assert_eq!(result.status.code(), Some(2));
    assert!(text(&result).contains("笔记数必须是非负整数"));
    assert!(!output.exists());
}

#[test]
fn a_missing_parent_directory_is_reported_as_io() {
    let sandbox = sandbox("parent");
    let output = sandbox.join("missing").join("a");
    let result = measure(&output, "300", "20", "3", "0");
    assert_eq!(result.status.code(), Some(3));
    assert!(text(&result).contains("BENCH_IO"));
    assert!(text(&result).contains("无法访问输出根的父目录"));
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["bench", "--help"]);
    assert!(help.status.success());
    let printed = String::from_utf8_lossy(&help.stdout).into_owned();
    assert!(printed.contains("Noetome 大库规模实测"));
    assert!(printed.contains("nearest_rank") || printed.contains("P95 口径"));

    for arguments in [
        vec!["bench"],
        vec!["bench", "unknown"],
        vec!["bench", "measure", "--notes", "300"],
        vec![
            "bench",
            "measure",
            "--notes",
            "300",
            "--edits",
            "20",
            "--samples",
            "3",
            "--warmup",
            "0",
        ],
        vec![
            "bench",
            "measure",
            "--notes",
            "300",
            "--edits",
            "20",
            "--samples",
            "3",
            "--warmup",
            "0",
            "--output",
            "/tmp/x",
            "--extra",
            "1",
        ],
    ] {
        let result = cli(&arguments);
        assert_eq!(result.status.code(), Some(2), "{arguments:?}");
        assert!(
            text(&result).contains("CLI_INVALID_ARGUMENT"),
            "{arguments:?} → {}",
            text(&result)
        );
    }
}

#[test]
fn the_three_thousand_note_target_really_runs() {
    let sandbox = sandbox("target");
    let output = sandbox.join("big");
    let result = measure(&output, "3000", "200", "3", "1");
    assert!(result.status.success(), "{}", text(&result));

    let structure = json(&output.join("structure.json"));
    assert_eq!(structure["counts"]["notes"], 3000);
    assert_eq!(structure["counts"]["changed"], 200);
    assert_eq!(structure["counts"]["pages"], 2);
    assert_eq!(structure["counts"]["page_size"], 100);
    assert_eq!(structure["spec"]["notes"], 3000);
    assert_eq!(structure["spec"]["edits"], 200);

    let timings = json(&output.join("timings.json"));
    let scan = timings["stages"]
        .as_array()
        .expect("stages")
        .iter()
        .find(|stage| stage["stage"] == "scan")
        .expect("scan");
    assert_eq!(scan["samples_millis"].as_array().expect("samples").len(), 3);
    assert_eq!(timings["protocol"]["warmup"], 1);
    assert_eq!(timings["protocol"]["samples"], 3);
}
