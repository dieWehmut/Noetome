//! B08-T02 集成测试：批准范围内的有界检索、持久预算与终止理由。
//!
//! 这些用例只通过 CLI 与磁盘观察行为，不 import 判定函数；断言看退出码、
//! `control/` 下的字节与 JSON 结构，不锁死人类可读措辞。

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

fn fixture() -> PathBuf {
    fs::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/retrieval-budget-v1"),
    )
    .expect("冻结作者包必须存在")
}

fn sandbox(label: &str) -> PathBuf {
    let sequence = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-retrieval-{label}-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("临时目录必须可创建");
    path
}

fn arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn cli(args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(args)
        .output()
        .expect("noetome 可执行文件必须可运行")
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries {
            let entry = entry.expect("目录项必须可读");
            let path = entry.path();
            let kind = entry.file_type().expect("目录项类型必须可读");
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .expect("路径必须位于根内")
                    .to_string_lossy()
                    .replace('\\', "/");
                files.insert(relative, fs::read(&path).expect("文件必须可读"));
            }
        }
    }
    files
}

fn adopt(package: &Path, plan: &str, root: &Path) -> Output {
    cli(&[
        "retrieve".to_owned(),
        "adopt".to_owned(),
        arg(&package.join(plan)),
        arg(root),
    ])
}

fn retrieve(root: &Path, verb: &str) -> Output {
    cli(&["retrieve".to_owned(), verb.to_owned(), arg(root)])
}

fn finish(root: &Path, package: &Path, reason: &str) -> Output {
    cli(&[
        "retrieve".to_owned(),
        "finish".to_owned(),
        arg(root),
        arg(&package.join("reasons").join(reason)),
    ])
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("标准输出必须是 JSON")
}

fn records(value: &serde_json::Value) -> Vec<serde_json::Value> {
    value["records"].as_array().cloned().unwrap_or_default()
}

fn outcomes(value: &serde_json::Value) -> Vec<String> {
    records(value)
        .iter()
        .map(|record| record["outcome"].as_str().unwrap_or("").to_owned())
        .collect()
}

fn budget(value: &serde_json::Value) -> serde_json::Value {
    value["budget"].clone()
}

fn unit_states(value: &serde_json::Value) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for unit in value["units"].as_array().cloned().unwrap_or_default() {
        let state = unit["state"].as_str().unwrap_or("").to_owned();
        *counts.entry(state).or_insert(0) += 1;
    }
    counts
}

#[test]
fn adopt_freezes_corpus_plan_and_budget_without_spending() {
    let package = fixture();
    let root = sandbox("adopt").join("root");
    let output = adopt(&package, "plan.json", &root);
    assert!(output.status.success(), "{}", text(&output));
    let listing = retrieve(&root, "list");
    assert!(listing.status.success(), "{}", text(&listing));
    let value = json(&listing);
    assert_eq!(value["external_retrieval"], serde_json::Value::Bool(false));
    assert_eq!(value["network_requests"], serde_json::Value::from(0));
    assert_eq!(value["model_calls"], serde_json::Value::from(0));
    assert_eq!(value["corpus"]["documents"], serde_json::Value::from(5));
    assert_eq!(value["corpus"]["bytes"], serde_json::Value::from(1758));
    assert_eq!(budget(&value)["queries_used"], serde_json::Value::from(0));
    assert_eq!(budget(&value)["bytes_used"], serde_json::Value::from(0));
    assert_eq!(value["attempts"].as_array().map(Vec::len), Some(0));
    // adopt 只创建 control/ 三个文件，不产生任何检索痕迹。
    let files = tree(&root);
    assert_eq!(
        files.keys().cloned().collect::<Vec<_>>(),
        vec![
            "control/owner.json".to_owned(),
            "control/retrieval.sqlite".to_owned(),
            "control/run.lock".to_owned()
        ]
    );
}

#[test]
fn adopt_refuses_missing_root_and_corpus_mismatch_keeps_site() {
    let package = fixture();
    let root = sandbox("adopt-refuse").join("root");
    let missing = root.join("absent");
    let output = cli(&[
        "retrieve".to_owned(),
        "adopt".to_owned(),
        arg(&package.join("plan.json")),
        arg(&missing),
    ]);
    assert!(!output.status.success());
    assert!(!missing.exists(), "失败的 adopt 不得留下根目录");

    // 语料被改动后 adopt 必须拒收，且不创建根。
    let broken_package = root.join("broken");
    fs::create_dir_all(broken_package.join("corpus")).expect("临时包必须可创建");
    for name in ["plan.json", "corpus-manifest.json"] {
        fs::copy(package.join(name), broken_package.join(name)).expect("文件必须可复制");
    }
    for entry in fs::read_dir(package.join("corpus")).expect("语料目录必须可读") {
        let entry = entry.expect("目录项必须可读");
        fs::copy(
            entry.path(),
            broken_package.join("corpus").join(entry.file_name()),
        )
        .expect("语料必须可复制");
    }
    let target = broken_package.join("corpus/chapter-glycolysis.md");
    let mut bytes = fs::read(&target).expect("语料必须可读");
    bytes.extend_from_slice(b"\n# tampered\n");
    fs::write(&target, &bytes).expect("语料必须可写");

    let refused = broken_package.join("root");
    let output = cli(&[
        "retrieve".to_owned(),
        "adopt".to_owned(),
        arg(&broken_package.join("plan.json")),
        arg(&refused),
    ]);
    assert!(!output.status.success(), "语料字节与清单不符必须拒收");
    assert!(
        text(&output).contains("RETRIEVAL_CORPUS_MISMATCH"),
        "{}",
        text(&output)
    );
    assert!(!refused.exists(), "拒收时不得创建检索根");
}

#[test]
fn unauthorized_scope_is_rejected_before_any_root_exists() {
    let package = fixture();
    let workspace = sandbox("scope");
    let plan = serde_json::from_slice::<serde_json::Value>(
        &fs::read(package.join("plan.json")).expect("计划必须可读"),
    )
    .expect("计划必须是 JSON");
    let mut plan = plan;
    plan["queries"][0]["scope_class"] = serde_json::Value::from("out_of_scope");
    let path = workspace.join("plan-escape.json");
    fs::write(&path, serde_json::to_vec(&plan).expect("计划必须可序列化")).expect("计划必须可写");
    let target = workspace.join("escaped");
    let output = cli(&[
        "retrieve".to_owned(),
        "adopt".to_owned(),
        arg(&path),
        arg(&target),
    ]);
    assert!(!output.status.success(), "越界主题不得进入检索计划");
    assert!(
        text(&output).contains("RETRIEVAL_ARGUMENT"),
        "{}",
        text(&output)
    );
}

#[test]
fn step_reserves_before_scanning_and_records_every_attempt() {
    let package = fixture();
    let root = sandbox("step").join("root");
    assert!(adopt(&package, "plan.json", &root).status.success());
    let first = retrieve(&root, "step");
    assert!(first.status.success(), "{}", text(&first));
    assert!(text(&first).contains("q-energy"), "{}", text(&first));
    let listing = json(&retrieve(&root, "list"));
    let attempts = listing["attempts"].as_array().cloned().unwrap_or_default();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0]["state"], serde_json::Value::from("settled"));
    assert_eq!(attempts[0]["reserved_queries"], serde_json::Value::from(1));
    assert_eq!(
        attempts[0]["reserved_bytes"],
        serde_json::Value::from(65536)
    );
    assert_eq!(
        attempts[0]["reserved_millis"],
        serde_json::Value::from(1000)
    );
    assert_eq!(attempts[0]["outcome"], serde_json::Value::from("evidence"));
    assert_eq!(attempts[0]["new_hits"], serde_json::Value::from(4));
    // 预留即计入消耗：即使实际只结算了少量字节，额度也不回收。
    assert_eq!(
        budget(&listing)["bytes_used"],
        serde_json::Value::from(65536)
    );
    assert_eq!(
        budget(&listing)["bytes_remaining"],
        serde_json::Value::from(524288 - 65536)
    );
}

#[test]
fn duplicate_and_conflict_and_external_outcomes_are_distinguished() {
    let package = fixture();
    let root = sandbox("outcomes").join("root");
    assert!(adopt(&package, "plan.json", &root).status.success());
    let run = retrieve(&root, "run");
    let value = json(&run);
    assert_eq!(
        outcomes(&value),
        vec![
            "evidence".to_owned(),
            "duplicates_only".to_owned(),
            "evidence".to_owned(),
            "evidence".to_owned(),
            "no_hits".to_owned(),
            "evidence".to_owned(),
            "conflict".to_owned(),
            "requires_external".to_owned()
        ]
    );
    let listing = json(&retrieve(&root, "list"));
    let states = unit_states(&listing);
    assert_eq!(states.get("settled"), Some(&7));
    assert_eq!(states.get("gapped"), Some(&1));
    // 需要外部来源的那条必须是待核验缺口，不能假装结算成证据。
    let external = listing["units"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .find(|unit| unit["query_id"] == serde_json::Value::from("q-external"))
        .expect("计划必须包含 q-external");
    assert_eq!(external["state"], serde_json::Value::from("gapped"));
    assert_eq!(
        listing["external_retrieval"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(listing["network_requests"], serde_json::Value::from(0));
    assert_eq!(listing["model_calls"], serde_json::Value::from(0));
}

#[test]
fn exhausted_budget_stops_scheduling_and_keeps_pending_work() {
    let package = fixture();
    let root = sandbox("budget").join("root");
    assert!(adopt(&package, "plan-tight.json", &root).status.success());
    let run = json(&retrieve(&root, "run"));
    assert_eq!(run["stopped"], serde_json::Value::from("budget_exhausted"));
    assert_eq!(run["steps"], serde_json::Value::from(4));
    let listing = json(&retrieve(&root, "list"));
    assert_eq!(budget(&listing)["queries_used"], serde_json::Value::from(4));
    assert_eq!(
        budget(&listing)["bytes_used"],
        serde_json::Value::from(4096)
    );
    assert_eq!(
        budget(&listing)["bytes_remaining"],
        serde_json::Value::from(0)
    );
    let states = unit_states(&listing);
    assert_eq!(states.get("pending"), Some(&4));
    assert_eq!(states.get("settled"), Some(&4));
    // 预算不足是「本轮停止」而不是「研究完整」：待执行查询必须保留。
    let finish = finish(&root, &package, "budget.json");
    assert!(finish.status.success(), "{}", text(&finish));
    assert!(text(&finish).contains("4"), "{}", text(&finish));
}

#[test]
fn all_seven_termination_reasons_are_supported_and_recorded_once() {
    let package = fixture();
    let cases: Vec<(&str, &str)> = vec![
        ("goal-met.json", "goal_met"),
        ("no-new-evidence.json", "no_reliable_new_evidence"),
        ("marginal-gain.json", "marginal_gain_insufficient"),
        ("network.json", "network_restricted"),
        ("manual.json", "manual_judgment_required"),
    ];
    for (reason_file, reason) in cases {
        let root = sandbox(&format!("reason-{reason}")).join("root");
        assert!(adopt(&package, "plan.json", &root).status.success());
        assert!(retrieve(&root, "run").status.success());
        let output = finish(&root, &package, reason_file);
        assert!(output.status.success(), "{reason}: {}", text(&output));
        let listing = json(&retrieve(&root, "list"));
        assert_eq!(
            listing["termination"]["reason"],
            serde_json::Value::from(reason)
        );
        // 终止理由只写一次。
        let again = finish(&root, &package, reason_file);
        assert!(!again.status.success(), "{reason} 不允许重复记录");
        assert!(
            text(&again).contains("RETRIEVAL_ARGUMENT"),
            "{}",
            text(&again)
        );
    }

    // 用户暂停：先 pause 再记录。
    let paused_root = sandbox("reason-paused").join("root");
    assert!(adopt(&package, "plan.json", &paused_root).status.success());
    assert!(retrieve(&paused_root, "pause").status.success());
    let paused = finish(&paused_root, &package, "paused.json");
    assert!(paused.status.success(), "{}", text(&paused));
    let listing = json(&retrieve(&paused_root, "list"));
    assert_eq!(
        listing["termination"]["reason"],
        serde_json::Value::from("user_paused")
    );
    assert_eq!(listing["paused"], serde_json::Value::Bool(true));

    // 预算不足：紧凑计划跑满后再记录。
    let tight_root = sandbox("reason-budget").join("root");
    assert!(
        adopt(&package, "plan-tight.json", &tight_root)
            .status
            .success()
    );
    assert!(retrieve(&tight_root, "run").status.success());
    let tight = finish(&tight_root, &package, "budget.json");
    assert!(tight.status.success(), "{}", text(&tight));
    let listing = json(&retrieve(&tight_root, "list"));
    assert_eq!(
        listing["termination"]["reason"],
        serde_json::Value::from("budget_exhausted")
    );
}

#[test]
fn termination_without_evidence_is_refused() {
    let package = fixture();
    let root = sandbox("reason-unsupported").join("root");
    assert!(adopt(&package, "plan.json", &root).status.success());
    // 一条都没跑就声称「目标满足」必须被拒。
    let output = finish(&root, &package, "goal-met.json");
    assert!(!output.status.success(), "无证据的终止理由必须被拒");
    assert!(
        text(&output).contains("RETRIEVAL_REASON_UNSUPPORTED"),
        "{}",
        text(&output)
    );
    let listing = json(&retrieve(&root, "list"));
    assert_eq!(listing["termination"], serde_json::Value::Null);
    assert_eq!(budget(&listing)["queries_used"], serde_json::Value::from(0));
}

#[test]
fn interrupted_reservation_is_settled_once_without_double_spending() {
    let package = fixture();
    let root = sandbox("resume").join("root");
    assert!(adopt(&package, "plan.json", &root).status.success());
    assert!(retrieve(&root, "run").status.success());
    let before = json(&retrieve(&root, "list"));
    let attempts_before = before["attempts"].as_array().cloned().unwrap_or_default();
    assert_eq!(attempts_before.len(), 8);
    // 再跑一次：没有待执行查询，不得新增尝试也不得多花预算。
    let again = retrieve(&root, "run");
    assert!(again.status.success(), "{}", text(&again));
    assert_eq!(json(&again)["steps"], serde_json::Value::from(0));
    assert_eq!(
        json(&again)["stopped"],
        serde_json::Value::from("already_stopped")
    );
    let after = json(&retrieve(&root, "list"));
    assert_eq!(
        after["attempts"].as_array().map(Vec::len),
        Some(attempts_before.len())
    );
    assert_eq!(budget(&after)["bytes_used"], budget(&before)["bytes_used"]);
    assert_eq!(
        budget(&after)["queries_used"],
        budget(&before)["queries_used"]
    );
}

#[test]
fn pause_blocks_scheduling_and_resume_restores_it() {
    let package = fixture();
    let root = sandbox("pause").join("root");
    assert!(adopt(&package, "plan.json", &root).status.success());
    assert!(retrieve(&root, "pause").status.success());
    let blocked = retrieve(&root, "step");
    assert!(!blocked.status.success(), "暂停后不得调度新检索");
    assert!(
        text(&blocked).contains("RETRIEVAL_PAUSED"),
        "{}",
        text(&blocked)
    );
    let listing = json(&retrieve(&root, "list"));
    assert_eq!(budget(&listing)["queries_used"], serde_json::Value::from(0));
    assert!(retrieve(&root, "resume").status.success());
    let resumed = retrieve(&root, "step");
    assert!(resumed.status.success(), "{}", text(&resumed));
}

#[test]
fn ledger_survives_process_restart_and_matches_frozen_expectations() {
    let package = fixture();
    let root = sandbox("restart").join("root");
    assert!(adopt(&package, "plan.json", &root).status.success());
    assert!(retrieve(&root, "run").status.success());
    let cases: serde_json::Value =
        serde_json::from_slice(&fs::read(package.join("cases.json")).expect("cases 必须可读"))
            .expect("cases 必须是 JSON");
    let expected = &cases["expected"]["plan.json"];
    let listing = json(&retrieve(&root, "list"));
    assert_eq!(
        budget(&listing)["queries_used"],
        expected["budget"]["queries_used"]
    );
    assert_eq!(
        budget(&listing)["bytes_used"],
        expected["budget"]["bytes_used"]
    );
    assert_eq!(
        budget(&listing)["millis_used"],
        expected["budget"]["millis_used"]
    );
    let states = unit_states(&listing);
    for (state, count) in expected["replay"]["unit_states"]
        .as_object()
        .cloned()
        .unwrap_or_default()
    {
        assert_eq!(
            states.get(&state).copied(),
            count.as_u64(),
            "单元状态 {state} 与先验不符"
        );
    }
    // 新进程读到的是同一份账本：预算与尝试数不因重读而漂移。
    let reread = json(&retrieve(&root, "list"));
    assert_eq!(
        budget(&reread)["bytes_used"],
        budget(&listing)["bytes_used"]
    );
    assert_eq!(
        reread["attempts"].as_array().map(Vec::len),
        listing["attempts"].as_array().map(Vec::len)
    );
}

#[test]
fn corrupted_ledger_and_foreign_layout_are_refused_without_touching_site() {
    let package = fixture();
    // 外来布局：根里多一个未知文件。
    let foreign = sandbox("foreign").join("root");
    assert!(adopt(&package, "plan.json", &foreign).status.success());
    fs::write(foreign.join("notes.txt"), b"foreign\n").expect("未知文件必须可写");
    let before = tree(&foreign);
    let refused = retrieve(&foreign, "list");
    assert!(!refused.status.success(), "未知顶层条目必须导致拒收");
    assert_eq!(tree(&foreign), before, "拒收时不得改动现场");

    // 账本头部损坏：明确报项目数据库无效，且不改动任何字节。
    let broken = sandbox("broken").join("root");
    assert!(adopt(&package, "plan.json", &broken).status.success());
    let database = broken.join("control/retrieval.sqlite");
    let mut bytes = fs::read(&database).expect("账本必须可读");
    bytes[..16].copy_from_slice(b"NOT-A-SQLITE-DB!");
    fs::write(&database, &bytes).expect("账本必须可写");
    let before = tree(&broken);
    let refused = retrieve(&broken, "list");
    assert!(!refused.status.success(), "损坏账本必须被拒");
    assert!(
        text(&refused).contains("PROJECT_INVALID_DATABASE"),
        "{}",
        text(&refused)
    );
    assert_eq!(tree(&broken), before, "拒收时不得改动现场");
}

#[test]
fn owner_tampering_is_refused_with_research_specific_code() {
    let package = fixture();
    let root = sandbox("owner").join("root");
    assert!(adopt(&package, "plan.json", &root).status.success());
    let owner = root.join("control/owner.json");
    let bytes = fs::read(&owner).expect("所有权记录必须可读");
    let mut tampered = Vec::with_capacity(bytes.len() + 1);
    tampered.push(bytes[0]);
    tampered.push(b' ');
    tampered.extend_from_slice(&bytes[1..]);
    fs::write(&owner, &tampered).expect("所有权记录必须可写");
    let refused = retrieve(&root, "list");
    assert!(!refused.status.success());
    assert!(
        text(&refused).contains("RETRIEVAL_INTEGRITY"),
        "{}",
        text(&refused)
    );
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let output = cli(&["retrieve".to_owned(), "--help".to_owned()]);
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(help.contains("retrieve"), "{help}");
    for verb in [
        "adopt", "step", "run", "pause", "resume", "finish", "list", "status",
    ] {
        assert!(help.contains(verb), "帮助必须列出 {verb}");
    }
    // 不带动词时显示帮助（与其它命令组一致），不算参数错误。
    let short = cli(&["retrieve".to_owned()]);
    assert!(short.status.success());
    assert!(text(&short).contains("retrieve"));
    for args in [
        vec!["retrieve".to_owned(), "adopt".to_owned()],
        vec!["retrieve".to_owned(), "unknown".to_owned(), "x".to_owned()],
        vec![
            "retrieve".to_owned(),
            "finish".to_owned(),
            "only-one".to_owned(),
        ],
    ] {
        let output = cli(&args);
        assert!(!output.status.success(), "{args:?} 必须失败");
        assert_eq!(output.status.code(), Some(2), "{args:?} 必须是参数错误");
    }
}
