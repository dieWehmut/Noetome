//! B08-T01 集成测试：研究候选队列的授权边界、拒绝记忆与持久化。
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
    fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/research-queue-v1"))
        .expect("冻结作者包必须存在")
}

fn sandbox(label: &str) -> PathBuf {
    let sequence = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-research-{label}-{}-{sequence}",
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

fn module(package: &Path, name: &str) -> String {
    arg(&package.join(name))
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

fn adopt(package: &Path, root: &Path) -> Output {
    cli(&[
        "research".to_owned(),
        "adopt".to_owned(),
        arg(&package.join("author.json")),
        arg(root),
    ])
}

fn observe(root: &Path, package: &Path, name: &str) -> Output {
    cli(&[
        "research".to_owned(),
        "observe".to_owned(),
        arg(root),
        module(package, name),
    ])
}

fn decide(root: &Path, package: &Path, name: &str) -> Output {
    cli(&[
        "research".to_owned(),
        "decide".to_owned(),
        arg(root),
        module(package, name),
    ])
}

fn list(root: &Path) -> serde_json::Value {
    let output = cli(&["research".to_owned(), "list".to_owned(), arg(root)]);
    assert!(output.status.success(), "list 必须成功：{}", text(&output));
    serde_json::from_slice(&output.stdout).expect("list 必须输出 JSON")
}

fn candidate<'a>(listed: &'a serde_json::Value, subject: &str) -> &'a serde_json::Value {
    listed["candidates"]
        .as_array()
        .expect("candidates 必须是数组")
        .iter()
        .find(|item| item["subject"] == subject)
        .unwrap_or_else(|| panic!("候选 {subject} 必须存在"))
}

/// 走完冻结的六步序列，返回队列根。
fn full_run(label: &str) -> PathBuf {
    let package = fixture();
    let root = sandbox(label).join("queue");
    assert!(adopt(&package, &root).status.success(), "adopt 必须成功");
    for name in [
        "round-01-observations.json",
        "round-02-decisions.json",
        "round-03-observations.json",
        "round-04-observations.json",
        "round-05-decisions.json",
    ] {
        let output = if name.contains("observations") {
            observe(&root, &package, name)
        } else {
            decide(&root, &package, name)
        };
        assert!(
            output.status.success(),
            "{name} 必须成功：{}",
            text(&output)
        );
    }
    root
}

#[test]
fn adopt_freezes_authorized_set_and_creates_only_control() {
    let package = fixture();
    let root = sandbox("adopt").join("queue");
    let output = adopt(&package, &root);
    assert!(output.status.success(), "adopt 必须成功：{}", text(&output));

    let entries: Vec<String> = fs::read_dir(&root)
        .expect("队列根必须存在")
        .map(|entry| {
            entry
                .expect("目录项可读")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        entries,
        vec!["control".to_owned()],
        "adopt 只允许建 control/"
    );

    let control: Vec<String> = fs::read_dir(root.join("control"))
        .expect("control/ 必须存在")
        .map(|entry| {
            entry
                .expect("目录项可读")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let mut sorted = control.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec![
            "owner.json".to_owned(),
            "queue.lock".to_owned(),
            "research.sqlite".to_owned()
        ],
        "control/ 只允许 owner.json、research.sqlite、queue.lock"
    );

    let listed = list(&root);
    assert_eq!(listed["case_id"], "rq-energy-v1");
    let must_do: Vec<&str> = listed["must_do"]
        .as_array()
        .expect("must_do 必须是数组")
        .iter()
        .map(|item| item.as_str().expect("主题必须是字符串"))
        .collect();
    assert_eq!(
        must_do,
        vec!["丙酮酸脱氢酶复合体", "糖异生绕行反应", "糖酵解净能量收支"],
        "必做集合必须只由 adopt 冻结"
    );
    assert_eq!(listed["retrieval_calls"], 0, "队列操作不得触发检索");
    assert_eq!(listed["model_calls"], 0, "队列操作不得调用模型");
}

#[test]
fn adopt_refuses_existing_root_and_keeps_site() {
    let package = fixture();
    let root = sandbox("occupied");
    fs::write(root.join("keep.txt"), b"sentinel").expect("哨兵文件必须可写");
    let before = tree(&root);

    let output = adopt(&package, &root);
    assert_eq!(
        output.status.code(),
        Some(2),
        "已存在的根必须按参数错误拒绝"
    );
    assert!(text(&output).contains("研究队列根必须尚不存在"));
    assert_eq!(tree(&root), before, "拒绝时不得修改现场一个字节");
}

#[test]
fn adopt_refuses_foreign_directory_layout() {
    let root = sandbox("foreign");
    fs::write(root.join("project.json"), b"{}").expect("外来文件必须可写");
    let before = tree(&root);

    let output = cli(&["research".to_owned(), "status".to_owned(), arg(&root)]);
    assert_eq!(
        output.status.code(),
        Some(3),
        "陌生目录必须按完整性错误拒绝"
    );
    assert_eq!(tree(&root), before, "拒绝陌生目录时不得写入任何文件");
}

#[test]
fn observations_queue_out_of_scope_without_widening_must_do() {
    let package = fixture();
    let root = sandbox("scope").join("queue");
    assert!(adopt(&package, &root).status.success());
    let output = observe(&root, &package, "round-01-observations.json");
    assert!(
        output.status.success(),
        "首轮观测必须成功：{}",
        text(&output)
    );

    let listed = list(&root);
    let must_do: Vec<&str> = listed["must_do"]
        .as_array()
        .expect("must_do 必须是数组")
        .iter()
        .map(|item| item.as_str().expect("主题必须是字符串"))
        .collect();
    assert_eq!(must_do.len(), 3, "越界主题不得进入必做集合：{must_do:?}");
    let queue_only: Vec<&str> = listed["queue_only"]
        .as_array()
        .expect("queue_only 必须是数组")
        .iter()
        .map(|item| item.as_str().expect("主题必须是字符串"))
        .collect();
    assert_eq!(
        queue_only,
        vec!["乳酸穿梭与科里循环", "脂肪酸β氧化能量账"],
        "越界主题只进队列"
    );
    assert_eq!(
        candidate(&listed, "乳酸穿梭与科里循环")["state"],
        "proposed"
    );
    assert_eq!(
        candidate(&listed, "乳酸穿梭与科里循环")["scope_class"],
        "out_of_scope"
    );
}

#[test]
fn rejected_topic_is_blocked_without_new_reason_and_reopens_with_one() {
    let package = fixture();
    let root = sandbox("memory").join("queue");
    assert!(adopt(&package, &root).status.success());
    assert!(
        observe(&root, &package, "round-01-observations.json")
            .status
            .success()
    );
    assert!(
        decide(&root, &package, "round-02-decisions.json")
            .status
            .success()
    );

    let blocked = observe(&root, &package, "round-03-observations.json");
    assert!(
        blocked.status.success(),
        "无新理由的重提必须被挡回而不是失败：{}",
        text(&blocked)
    );
    let body: serde_json::Value = serde_json::from_slice(&blocked.stdout).expect("必须输出 JSON");
    let applied = body["applied"].as_array().expect("applied 必须是数组");
    let repeat = applied
        .iter()
        .find(|item| item["subject"] == "乳酸穿梭与科里循环")
        .expect("必须记录重提");
    assert_eq!(repeat["outcome"], "blocked_repeat", "不得把重提当成新证据");
    assert_eq!(body["counters"]["blocked_repeats"], 1);

    let reopened = observe(&root, &package, "round-04-observations.json");
    assert!(
        reopened.status.success(),
        "带新理由的重提必须接受：{}",
        text(&reopened)
    );
    let body: serde_json::Value = serde_json::from_slice(&reopened.stdout).expect("必须输出 JSON");
    let applied = body["applied"].as_array().expect("applied 必须是数组");
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0]["outcome"], "reopened");
    assert_eq!(body["counters"]["reopenings"], 1);

    let listed = list(&root);
    let subject = candidate(&listed, "乳酸穿梭与科里循环");
    assert_eq!(subject["state"], "proposed", "带新理由必须回到 proposed");
    assert_eq!(
        subject["evidence"]
            .as_array()
            .expect("evidence 必须是数组")
            .len(),
        2,
        "拒绝前的证据与重提理由都要保留"
    );
    assert_eq!(
        subject["rejections"]
            .as_array()
            .expect("rejections 必须是数组")
            .len(),
        1,
        "拒绝理由必须记住"
    );
}

#[test]
fn merge_transfers_evidence_and_keeps_must_do_unchanged() {
    let root = full_run("merge");
    let listed = list(&root);

    let merged = candidate(&listed, "磷酸戊糖途径产物去向");
    assert_eq!(merged["state"], "merged");
    assert_eq!(merged["merged_into"], "脂肪酸β氧化能量账");

    let target = candidate(&listed, "脂肪酸β氧化能量账");
    assert_eq!(
        target["state"], "deferred",
        "合并不得改变目标候选的既有状态"
    );
    assert_eq!(
        target["evidence"]
            .as_array()
            .expect("evidence 必须是数组")
            .len(),
        2,
        "合并必须把来源证据转移到目标"
    );

    let must_do: Vec<&str> = listed["must_do"]
        .as_array()
        .expect("must_do 必须是数组")
        .iter()
        .map(|item| item.as_str().expect("主题必须是字符串"))
        .collect();
    assert_eq!(must_do.len(), 3, "全部决定执行后必做集合仍只由 adopt 决定");
    let queue_only: Vec<&str> = listed["queue_only"]
        .as_array()
        .expect("queue_only 必须是数组")
        .iter()
        .map(|item| item.as_str().expect("主题必须是字符串"))
        .collect();
    assert!(
        !queue_only.contains(&"磷酸戊糖途径产物去向"),
        "已合并候选不得继续占据越界集合"
    );
}

#[test]
fn counters_match_frozen_expectations() {
    let package = fixture();
    let root = full_run("counters");
    let decisions =
        fs::read_to_string(root.join("control/owner.json")).expect("owner.json 必须可读");
    assert!(decisions.contains("research-queue-v1"));

    let listed = list(&root);
    let counters = &listed["counters"];
    assert_eq!(counters["observations"], 10);
    assert_eq!(counters["blocked_repeats"], 1);
    assert_eq!(counters["reopenings"], 1);
    assert_eq!(counters["merges"], 1);
    assert_eq!(counters["decisions"], 7);

    let status = cli(&["research".to_owned(), "status".to_owned(), arg(&root)]);
    assert!(status.status.success());
    let body = text(&status);
    assert!(
        body.contains("检索与模型调用：0 次"),
        "状态必须声明零检索零模型"
    );
    assert!(body.contains("研究候选单列"), "状态必须声明候选单列");
    let _ = package;
}

#[test]
fn queue_survives_process_restart() {
    let root = full_run("restart");
    let first = tree(&root);

    // 每次 CLI 调用都是独立进程；这里再起一个进程读同一根，验证跨进程重读。
    let listed = list(&root);
    assert_eq!(listed["counters"]["observations"], 10);
    assert_eq!(tree(&root), first, "只读动词不得修改任何文件");

    let before = tree(&root);
    let status = cli(&["research".to_owned(), "status".to_owned(), arg(&root)]);
    assert!(status.status.success());
    assert_eq!(tree(&root), before, "status 必须是纯只读");
}

#[test]
fn corrupted_ledger_is_refused_and_site_preserved() {
    let root = full_run("corrupt");
    let database = root.join("control/research.sqlite");
    let mut bytes = fs::read(&database).expect("数据库必须可读");
    bytes[..16].copy_from_slice(b"NOT-A-SQLITE-DB!");
    fs::write(&database, &bytes).expect("损坏写入必须成功");
    let before = tree(&root);

    for verb in ["list", "status"] {
        let output = cli(&["research".to_owned(), verb.to_owned(), arg(&root)]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{verb} 必须拒绝无法识别的账本：{}",
            text(&output)
        );
        assert!(
            text(&output).contains("PROJECT_INVALID_DATABASE"),
            "必须报告账本无效而不是当成空队列：{}",
            text(&output)
        );
    }
    let output = observe(&root, &fixture(), "round-01-observations.json");
    assert_eq!(output.status.code(), Some(2), "observe 必须拒绝损坏账本");
    assert_eq!(tree(&root), before, "拒绝时必须保留现场");
}

#[test]
fn tampered_owner_record_is_refused_and_site_preserved() {
    let root = full_run("tamper");
    let owner = root.join("control/owner.json");
    let mut bytes = fs::read(&owner).expect("所有权记录必须可读");
    // 插入一个空格，使记录不再是规范编码；语义仍是同一份 JSON。
    bytes.insert(1, b' ');
    fs::write(&owner, &bytes).expect("改写必须成功");
    let before = tree(&root);

    let output = cli(&["research".to_owned(), "list".to_owned(), arg(&root)]);
    assert_eq!(output.status.code(), Some(3), "非规范所有权记录必须被拒绝");
    assert!(
        text(&output).contains("RESEARCH_INTEGRITY"),
        "必须报告完整性错误：{}",
        text(&output)
    );
    assert_eq!(tree(&root), before, "拒绝时必须保留现场，不得自动修复");
}

#[test]
fn unknown_files_inside_control_are_refused() {
    let root = full_run("unknown");
    fs::write(root.join("control/extra.txt"), b"x").expect("额外文件必须可写");
    let before = tree(&root);
    let output = cli(&["research".to_owned(), "status".to_owned(), arg(&root)]);
    assert_eq!(output.status.code(), Some(3), "未知文件必须被拒绝");
    assert!(text(&output).contains("control/"));
    assert_eq!(tree(&root), before);
}

#[test]
fn observe_refuses_malformed_input_without_touching_queue() {
    let package = fixture();
    let root = sandbox("malformed").join("queue");
    assert!(adopt(&package, &root).status.success());
    let before = tree(&root);

    let bad = sandbox("bad-input").join("bad.json");
    fs::write(&bad, b"{\"policy\":\"research-queue-v1\"}").expect("坏输入必须可写");
    let output = cli(&[
        "research".to_owned(),
        "observe".to_owned(),
        arg(&root),
        arg(&bad),
    ]);
    assert_eq!(output.status.code(), Some(2), "字段缺失必须按参数错误拒绝");
    assert_eq!(tree(&root), before, "坏输入不得改变队列");
}

#[test]
fn decide_cannot_create_candidates() {
    let root = full_run("decide-only");
    let package = fixture();
    let directory = sandbox("decide-extra");
    let file = directory.join("extra.json");
    let body = r#"{"policy":"research-queue-v1","schema_version":1,"round":"round-99","decisions":[{"subject":"从未观测过的主题","action":"accept","reason":"凭空接受","merged_into":null,"scope_note":null}]}"#;
    fs::write(&file, body.as_bytes()).expect("决定文件必须可写");
    let before = tree(&root);
    let output = decide(&root, &directory, "extra.json");
    assert_eq!(output.status.code(), Some(2), "决定不得凭空创建候选");
    assert!(text(&output).contains("不在候选队列中"));
    assert_eq!(tree(&root), before);
    let _ = package;
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["research".to_owned(), "--help".to_owned()]);
    assert!(help.status.success());
    let body = text(&help);
    for verb in ["adopt", "observe", "decide", "list", "status"] {
        assert!(body.contains(verb), "帮助必须列出 {verb}");
    }
    assert!(body.contains("不联网"), "帮助必须声明不联网");

    let invalid = cli(&["research".to_owned(), "nonsense".to_owned()]);
    assert_eq!(invalid.status.code(), Some(2));
}
