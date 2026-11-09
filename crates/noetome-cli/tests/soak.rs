//! Black-box tests for the soak harness verb.
//!
//! Every test runs the real binary and reads only files on disk. The clock and
//! memory readings are checked for platform support and internal consistency; they are
//! never compared across runs, because `timings.json` is the one output allowed to
//! differ. The determinism test compares the other three files byte for byte.

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
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/soak-harness-v1");
    std::fs::canonicalize(path).expect("fixture")
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-soak-{label}-{}-{serial}",
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

fn soak(output: &Path, rounds: &str, interval_ms: &str, pause: Option<&str>) -> Output {
    let target = output.to_str().expect("utf-8 path").to_owned();
    match pause {
        Some(after) => cli(&[
            "soak",
            "run",
            "--rounds",
            rounds,
            "--interval-ms",
            interval_ms,
            "--pause-after-ms",
            after,
            "--max-rss-growth-bytes",
            "1073741824",
            "--output",
            &target,
        ]),
        None => cli(&[
            "soak",
            "run",
            "--rounds",
            rounds,
            "--interval-ms",
            interval_ms,
            "--max-rss-growth-bytes",
            "1073741824",
            "--output",
            &target,
        ]),
    }
}

fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut entries = BTreeMap::new();
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
                entries.insert(relative, std::fs::read(&path).expect("read file"));
            }
        }
    }
    entries
}

fn json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read json")).expect("parse json")
}

fn expected() -> Value {
    json(&fixture().join("expected.json"))
}

#[test]
fn the_eight_round_case_matches_the_written_expectation() {
    let sandbox = sandbox("prior");
    let output = soak(&sandbox.join("run"), "8", "0", None);
    assert!(output.status.success(), "{}", text(&output));
    let structure = json(&sandbox.join("run/structure.json"));
    let prior = expected();
    assert_eq!(structure["policy"], prior["policy"]);
    assert_eq!(structure["schema_version"], prior["schema_version"]);
    assert_eq!(structure["case_id"], prior["case_id"]);
    assert_eq!(structure["spec"], prior["no_pause"]["spec"]);
    assert_eq!(
        structure["enforcement"]["pause_bound_millis"],
        prior["enforcement"]["pause_bound_millis"]
    );
    assert_eq!(
        structure["enforcement"]["required_millis"],
        prior["enforcement"]["required_millis"]
    );
    assert_eq!(
        structure["enforcement"]["planned_floor_millis"],
        prior["enforcement"]["planned_floor_millis"]
    );
    assert_eq!(
        structure["enforcement"]["unit"],
        prior["enforcement"]["unit"]
    );
    assert_eq!(structure["model_calls"], 0);
    assert_eq!(structure["network_requests"], 0);
    let coverage: Vec<(&str, &str)> = structure["coverage"]
        .as_array()
        .expect("coverage")
        .iter()
        .map(|entry| {
            (
                entry["requirement"].as_str().expect("requirement"),
                entry["status"].as_str().expect("status"),
            )
        })
        .collect();
    let prior_coverage: Vec<(&str, &str)> = prior["coverage"]
        .as_array()
        .expect("prior coverage")
        .iter()
        .map(|entry| {
            (
                entry["requirement"].as_str().expect("requirement"),
                entry["status"].as_str().expect("status"),
            )
        })
        .collect();
    assert_eq!(coverage, prior_coverage);

    let timings = json(&sandbox.join("run/timings.json"));
    for key in ["rounds", "scheduled", "refused", "units", "errors"] {
        assert_eq!(
            timings["summary"][key], prior["no_pause"]["summary"][key],
            "summary.{key}"
        );
    }
    assert_eq!(
        timings["summary"]["scheduled_after_pause"],
        prior["no_pause"]["summary"]["scheduled_after_pause"]
    );
    assert_eq!(
        timings["pause"]["after_ms"],
        Value::Null,
        "no pause was requested"
    );
    assert_eq!(timings["pause"]["effective_round"], Value::Null);
    assert_eq!(timings["pause"]["within_bound"], Value::Null);
    assert_eq!(timings["pause"]["scheduled_after_pause"], 0);
    let rounds = timings["rounds"].as_array().expect("rounds");
    assert_eq!(rounds.len(), 8);
    assert!(rounds.iter().all(|round| round["state"] == "scheduled"));
}

#[test]
fn the_paused_case_refuses_every_round_and_never_schedules_after_the_gate() {
    let sandbox = sandbox("paused");
    let output = soak(&sandbox.join("run"), "8", "0", Some("0"));
    assert!(output.status.success(), "{}", text(&output));
    let prior = expected();
    let timings = json(&sandbox.join("run/timings.json"));
    for key in ["rounds", "scheduled", "refused", "units", "errors"] {
        assert_eq!(
            timings["summary"][key], prior["paused"]["summary"][key],
            "summary.{key}"
        );
    }
    assert_eq!(
        timings["summary"]["scheduled_after_pause"],
        prior["paused"]["summary"]["scheduled_after_pause"]
    );
    assert_eq!(timings["pause"]["after_ms"], 0);
    assert_eq!(
        timings["pause"]["effective_round"],
        prior["paused"]["pause"]["effective_round"]
    );
    assert_eq!(timings["pause"]["within_bound"], true);
    assert_eq!(timings["pause"]["scheduled_after_pause"], 0);
    let rounds = timings["rounds"].as_array().expect("rounds");
    assert!(rounds.iter().all(|round| round["state"] == "refused"));
}

#[test]
fn a_pause_in_the_middle_of_the_run_stops_scheduling_without_exception() {
    let sandbox = sandbox("mid");
    let output = soak(&sandbox.join("run"), "400", "10", Some("120"));
    assert!(output.status.success(), "{}", text(&output));
    let timings = json(&sandbox.join("run/timings.json"));
    let summary = &timings["summary"];
    let scheduled = summary["scheduled"].as_u64().expect("scheduled");
    let refused = summary["refused"].as_u64().expect("refused");
    assert_eq!(scheduled + refused, 400);
    assert!(scheduled > 0, "闸门关闭之前应当已经调度过若干轮");
    assert!(refused > 0, "闸门关闭之后应当有被拒绝的轮次");
    assert_eq!(
        summary["scheduled_after_pause"], 0,
        "闸门关闭后不得再调度任何一轮"
    );
    let after = timings["pause"]["after_ms"].as_u64().expect("after_ms");
    // 闸门关闭之后的所有轮次都必须被拒绝，且它们的起点时间不早于暂停时点。
    let effective = timings["pause"]["effective_round"]
        .as_u64()
        .expect("effective");
    let rounds = timings["rounds"].as_array().expect("rounds");
    for round in rounds {
        let index = round["round"].as_u64().expect("round");
        let elapsed = round["elapsed_millis"].as_u64().expect("elapsed");
        if index >= effective {
            assert_eq!(round["state"], "refused", "第 {index} 轮不能被调度");
            assert!(
                elapsed >= after,
                "第 {index} 轮的起点 {elapsed} 早于暂停时点 {after}"
            );
        } else {
            assert_eq!(round["state"], "scheduled", "第 {index} 轮应当已被调度");
        }
    }
    assert_eq!(timings["pause"]["within_bound"], true);
}

#[test]
fn the_summary_prints_the_counts_the_pause_and_the_unmet_requirement() {
    let sandbox = sandbox("summary");
    let output = soak(&sandbox.join("paused"), "8", "0", Some("0"));
    let text = text(&output);
    for needle in [
        "长时运行 soak",
        "参数：轮次 8、轮间隔 0 毫秒",
        "暂停：0 毫秒处关闭闸门（第 1 轮生效）",
        "关闭后调度 0 轮",
        "覆盖：NFR-09；not_met",
        "边界：合成负载",
    ] {
        assert!(text.contains(needle), "缺少「{needle}」：{text}");
    }
}

#[test]
fn the_report_states_the_unmet_requirement_and_carries_no_readings() {
    let sandbox = sandbox("report");
    soak(&sandbox.join("run"), "8", "0", None);
    let markdown = std::fs::read_to_string(sandbox.join("run/report.md")).expect("report");
    for needle in [
        "NFR-09",
        "not_met",
        "NFR-13",
        "partially_measured",
        "2000",
        "## 覆盖范围与未达标项",
        "## 边界",
        "noetome soak run --rounds 8",
    ] {
        assert!(markdown.contains(needle), "缺少「{needle}」：{markdown}");
    }
    // 报告里允许出现的毫秒数只有两类：规则常量（2000、86400000）与参数
    // （轮间隔 0）。任何本次运行的读数写进来，同参数两次运行就不再相同。
    let allowed: [u128; 3] = [0, 2000, 86_400_000];
    for (offset, _) in markdown.match_indices("毫秒") {
        let digits: String = markdown[..offset]
            .chars()
            .rev()
            .take_while(|value| value.is_ascii_digit())
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect();
        if digits.is_empty() {
            continue;
        }
        let value: u128 = digits.parse().expect("数字");
        assert!(
            allowed.contains(&value),
            "报告里出现了疑似读数：{digits} 毫秒"
        );
    }
}

#[test]
fn two_runs_with_the_same_parameters_are_byte_identical_apart_from_the_timings() {
    let sandbox = sandbox("determinism");
    let first = soak(&sandbox.join("a"), "8", "0", None);
    assert!(first.status.success(), "{}", text(&first));
    let second = soak(&sandbox.join("b"), "8", "0", None);
    assert!(second.status.success(), "{}", text(&second));
    let left = tree(&sandbox.join("a"));
    let right = tree(&sandbox.join("b"));
    assert_eq!(
        left.keys().collect::<Vec<_>>(),
        right.keys().collect::<Vec<_>>(),
        "两次运行应当写出同一组文件"
    );
    for name in ["structure.json", "manifest.json", "report.md"] {
        assert_eq!(left[name], right[name], "{name} 应当字节相同");
    }
    // Millisecond quantization and unavailable RSS can give two real runs the
    // same readings. Validate each measurement without requiring randomness.
    for bytes in [&left["timings.json"], &right["timings.json"]] {
        let readings: Value = serde_json::from_slice(bytes).expect("合法计时JSON");
        let rounds = readings["rounds"].as_array().expect("逐轮真实读数");
        assert_eq!(rounds.len(), 8);
        for (index, round) in rounds.iter().enumerate() {
            assert_eq!(round["round"].as_u64(), Some(index as u64 + 1));
            for field in ["elapsed_millis", "work_millis", "total_millis"] {
                assert!(round[field].as_u64().is_some(), "{field} 必须是非负读数");
            }
            assert!(round["rss_bytes"].is_null() || round["rss_bytes"].as_u64().is_some());
        }
    }

    // 清单只描述参数与结论：不能带任何一次运行的读数摘要。
    let manifest = json(&sandbox.join("a/manifest.json"));
    assert!(manifest.get("timings_sha256").is_none());
    assert!(manifest.get("summary").is_none());
    assert!(manifest.get("pause").is_none());
    let structure = std::fs::read(sandbox.join("a/structure.json")).expect("structure");
    assert_eq!(
        manifest["structure_sha256"],
        Value::String(noetome_cli::text::sha256(&structure)),
        "清单应当摘的是结构产物"
    );
    assert_eq!(
        manifest["structure_byte_length"].as_u64().expect("length"),
        structure.len() as u64
    );
}

#[test]
fn the_output_root_holds_exactly_four_files() {
    let sandbox = sandbox("layout");
    soak(&sandbox.join("run"), "4", "0", None);
    let entries: Vec<String> = tree(&sandbox.join("run")).into_keys().collect();
    assert_eq!(
        entries,
        vec![
            "manifest.json".to_owned(),
            "report.md".to_owned(),
            "structure.json".to_owned(),
            "timings.json".to_owned(),
        ]
    );
}

#[test]
fn the_timings_are_internally_consistent() {
    let sandbox = sandbox("timings");
    soak(&sandbox.join("run"), "10", "0", None);
    let timings = json(&sandbox.join("run/timings.json"));
    let rounds = timings["rounds"].as_array().expect("rounds");
    assert_eq!(rounds.len(), 10);
    let summary = &timings["summary"];
    let scheduled = summary["scheduled"].as_u64().expect("scheduled") as usize;
    let units = summary["units"].as_u64().expect("units") as usize;
    assert_eq!(units, scheduled, "每个被调度的轮次都应当跑成一个单元");
    assert_eq!(summary["rounds"].as_u64().expect("rounds"), 10);
    #[cfg(target_os = "linux")]
    {
        let peak = summary["peak_rss_bytes"].as_u64().expect("peak");
        let first = summary["first_rss_bytes"].as_u64().expect("first");
        let last = summary["last_rss_bytes"].as_u64().expect("last");
        assert!(peak >= first && peak >= last, "峰值不得低于首末读数");
        assert_eq!(
            summary["rss_growth_bytes"].as_i64().expect("growth"),
            last as i64 - first as i64
        );
    }
    #[cfg(not(target_os = "linux"))]
    assert_memory_unavailable(&timings);
    // 每一轮的 work 读数都不得为负，且 total 不小于 work。
    for round in rounds {
        let work = round["work_millis"].as_u64().expect("work");
        let total = round["total_millis"].as_u64().expect("total");
        assert!(total >= work, "total 不得小于 work");
        assert_eq!(round["error_code"], Value::Null);
    }
    assert!(
        timings["note"]
            .as_str()
            .expect("note")
            .contains("不参与确定性比较")
    );
}

#[test]
fn an_existing_output_root_is_refused_without_touching_it() {
    let sandbox = sandbox("existing");
    let root = sandbox.join("run");
    std::fs::create_dir_all(&root).expect("pre-create");
    std::fs::write(root.join("keep.txt"), b"untouched").expect("seed");
    let before = tree(&root);
    let output = soak(&root, "4", "0", None);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("SOAK_ARGUMENT"));
    assert!(text(&output).contains("输出根必须尚不存在"));
    assert_eq!(tree(&root), before, "拒绝时不得改动已存在的目录");
}

#[test]
fn bounds_are_refused_before_anything_is_written() {
    let sandbox = sandbox("bounds");
    let cases: [(&str, &str, Option<&str>, &str); 4] = [
        ("0", "0", None, "轮次必须在 1 到 100000 之间"),
        ("4", "1001", None, "轮间隔必须在 0 到 1000 毫秒之间"),
        (
            "4",
            "0",
            Some("600001"),
            "暂停时点必须在 0 到 600000 毫秒之间",
        ),
        (
            "4",
            "0",
            None,
            "内存增长上限必须在 0 到 1073741824 字节之间",
        ),
    ];
    for (index, (rounds, interval, pause, needle)) in cases.iter().enumerate() {
        let target = sandbox.join(format!("case-{index}"));
        let output = match pause {
            Some(after) => {
                let path = target.to_str().expect("utf-8").to_owned();
                let mut arguments = vec![
                    "soak",
                    "run",
                    "--rounds",
                    rounds,
                    "--interval-ms",
                    interval,
                    "--pause-after-ms",
                    after,
                ];
                if *needle != "内存增长上限必须在 0 到 1073741824 字节之间" {
                    arguments.extend(["--max-rss-growth-bytes", "1073741824"]);
                } else {
                    arguments.extend(["--max-rss-growth-bytes", "1073741825"]);
                }
                arguments.extend(["--output", &path]);
                cli(&arguments)
            }
            None => {
                if *needle == "内存增长上限必须在 0 到 1073741824 字节之间" {
                    let path = target.to_str().expect("utf-8").to_owned();
                    cli(&[
                        "soak",
                        "run",
                        "--rounds",
                        rounds,
                        "--interval-ms",
                        interval,
                        "--max-rss-growth-bytes",
                        "1073741825",
                        "--output",
                        &path,
                    ])
                } else {
                    let path = target.to_str().expect("utf-8").to_owned();
                    cli(&[
                        "soak",
                        "run",
                        "--rounds",
                        rounds,
                        "--interval-ms",
                        interval,
                        "--max-rss-growth-bytes",
                        "1073741824",
                        "--output",
                        &path,
                    ])
                }
            }
        };
        assert_eq!(output.status.code(), Some(2), "{needle}: {}", text(&output));
        assert!(text(&output).contains("SOAK_ARGUMENT"), "{needle}");
        assert!(text(&output).contains(needle), "{}", text(&output));
        assert!(!target.exists(), "{needle}：越界参数不得留下输出根");
    }
}

#[test]
fn an_unparsable_number_is_refused() {
    let sandbox = sandbox("number");
    let output = soak(&sandbox.join("run"), "abc", "0", None);
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output).contains("轮次必须是非负整数：收到「abc」"));
    assert!(!sandbox.join("run").exists());
}

#[test]
fn a_missing_parent_directory_is_reported_as_io() {
    let sandbox = sandbox("missing-parent");
    let output = soak(&sandbox.join("absent/run"), "4", "0", None);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(text(&output).contains("SOAK_IO"));
    assert!(text(&output).contains("无法访问输出根的父目录"));
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let output = cli(&["soak", "--help"]);
    assert!(output.status.success());
    let text = text(&output);
    assert!(text.contains("Noetome 长时运行 soak"));
    assert!(text.contains("未跑满 24 小时即未达到 NFR-09"));

    let unknown = cli(&["soak", "walk"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(text_of(&unknown).contains("CLI_INVALID_ARGUMENT"));

    let missing_output = cli(&[
        "soak",
        "run",
        "--rounds",
        "4",
        "--interval-ms",
        "0",
        "--max-rss-growth-bytes",
        "1073741824",
    ]);
    assert_eq!(missing_output.status.code(), Some(2));
    assert!(text_of(&missing_output).contains("CLI_INVALID_ARGUMENT"));
}

fn text_of(output: &Output) -> String {
    text(output)
}

#[cfg(not(target_os = "linux"))]
fn assert_memory_unavailable(timings: &Value) {
    // The current sampler reads /proc/self/status only. Unsupported hosts
    // must preserve unknown, including per-round readings, rather than zero.
    for field in [
        "peak_rss_bytes",
        "first_rss_bytes",
        "last_rss_bytes",
        "rss_growth_bytes",
    ] {
        assert_eq!(
            timings["summary"].get(field).expect("memory summary field"),
            &Value::Null,
            "{field}"
        );
    }
    for round in timings["rounds"].as_array().expect("rounds") {
        assert_eq!(
            round.get("rss_bytes").expect("round memory field"),
            &Value::Null
        );
    }
}

#[test]
fn a_long_run_keeps_recording_rounds_without_unbounded_growth() {
    let sandbox = sandbox("long");
    let output = soak(&sandbox.join("run"), "300", "2", None);
    assert!(output.status.success(), "{}", text(&output));
    let timings = json(&sandbox.join("run/timings.json"));
    let rounds = timings["rounds"].as_array().expect("rounds");
    assert_eq!(rounds.len(), 300);
    let summary = &timings["summary"];
    assert_eq!(summary["units"].as_u64().expect("units"), 300);
    assert_eq!(summary["errors"].as_u64().expect("errors"), 0);
    #[cfg(target_os = "linux")]
    {
        let growth = summary["rss_growth_bytes"].as_i64().expect("growth");
        // 只是一次短时观察，不表示 NFR-09 达标。
        assert!(
            growth < 256 * 1024 * 1024,
            "内存增长 {growth} 字节，看起来不是有界行为"
        );
    }
    #[cfg(not(target_os = "linux"))]
    assert_memory_unavailable(&timings);
    // 结构产物必须仍然说 NFR-09 未达标，短跑不能变成通过。
    let structure = json(&sandbox.join("run/structure.json"));
    assert_eq!(structure["coverage"][0]["status"], "not_met");
    assert_eq!(structure["coverage"][0]["requirement"], "NFR-09");
}
