//! B08-T03 集成测试：写作反馈缺口回到修订与依赖重编译。
//!
//! 这些用例只通过 CLI 与磁盘观察行为，不 import 判定函数；断言看退出码、
//! `control/` 与 `render/` 下的字节和 JSON 结构，不锁死人类可读措辞。

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
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/revision-feedback-v1"),
    )
    .expect("冻结作者包必须存在")
}

fn sandbox(label: &str) -> PathBuf {
    let sequence = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-revision-{label}-{}-{sequence}",
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

fn adopt(package: &Path, root: &Path) -> Output {
    cli(&[
        "revision".to_owned(),
        "adopt".to_owned(),
        arg(&package.join("author.json")),
        arg(root),
    ])
}

fn gap(root: &Path, package: &Path, name: &str) -> Output {
    cli(&[
        "revision".to_owned(),
        "gap".to_owned(),
        arg(root),
        arg(&package.join("gaps").join(format!("{name}.json"))),
    ])
}

fn revise(root: &Path, package: &Path, name: &str) -> Output {
    cli(&[
        "revision".to_owned(),
        "revise".to_owned(),
        arg(root),
        arg(&package.join("revisions").join(format!("{name}.json"))),
    ])
}

fn listing(root: &Path) -> serde_json::Value {
    let output = cli(&["revision".to_owned(), "list".to_owned(), arg(root)]);
    assert!(output.status.success(), "{}", text(&output));
    serde_json::from_slice(&output.stdout).expect("标准输出必须是 JSON")
}

fn ir_version(value: &serde_json::Value) -> u64 {
    value["ir_version"].as_u64().expect("ir_version 必须是整数")
}

fn verification(value: &serde_json::Value) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for (key, number) in value["verification"]
        .as_object()
        .cloned()
        .unwrap_or_default()
    {
        counts.insert(key, number.as_u64().unwrap_or_default());
    }
    counts
}

fn gap_status(value: &serde_json::Value, gap_id: &str) -> String {
    value["gaps"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .find(|item| item["gap_id"].as_str() == Some(gap_id))
        .map(|item| item["status"].as_str().unwrap_or("").to_owned())
        .unwrap_or_default()
}

#[test]
fn adopt_freezes_corpus_atoms_and_renders_every_representation() {
    let package = fixture();
    let root = sandbox("adopt").join("root");
    let output = adopt(&package, &root);
    assert!(output.status.success(), "{}", text(&output));
    let value = listing(&root);
    assert_eq!(value["case_id"], serde_json::Value::from("rfv-energy-v1"));
    assert_eq!(
        value["policy"],
        serde_json::Value::from("revision-feedback-v1")
    );
    assert_eq!(ir_version(&value), 1);
    assert_eq!(value["atoms"].as_array().map(Vec::len), Some(5));
    assert_eq!(
        value["representations"].as_array().map(Vec::len),
        Some(3),
        "冻结作者包声明三种表示"
    );
    assert_eq!(
        verification(&value).get("verified").copied(),
        Some(4),
        "四个原子有来源"
    );
    assert_eq!(
        verification(&value).get("pending_verification").copied(),
        Some(1),
        "无来源原子必须待核验"
    );
    assert_eq!(value["network_calls"], serde_json::Value::from(0));
    assert_eq!(value["model_calls"], serde_json::Value::from(0));
    // adopt 只写 control/ 三个文件与 render/ 下的表示，不写别的东西。
    let files = tree(&root);
    assert_eq!(
        files.keys().cloned().collect::<Vec<_>>(),
        vec![
            "control/owner.json".to_owned(),
            "control/revision.sqlite".to_owned(),
            "control/run.lock".to_owned(),
            "render/rep-energy.md".to_owned(),
            "render/rep-narrative.md".to_owned(),
            "render/rep-trace.md".to_owned()
        ]
    );
    let energy = String::from_utf8_lossy(&files["render/rep-energy.md"]).into_owned();
    assert!(
        energy.contains("【待核验】") && energy.contains("pending_verification"),
        "无来源原子在表格里必须标为待核验：{energy}"
    );
}

#[test]
fn adopt_refuses_existing_root_and_corpus_mismatch_keeps_site() {
    let package = fixture();
    let root = sandbox("adopt-refuse").join("root");
    assert!(adopt(&package, &root).status.success());
    let before = tree(&root);
    let repeat = adopt(&package, &root);
    assert!(!repeat.status.success());
    assert_eq!(before, tree(&root), "拒绝重复 adopt 时不得改动已有字节");

    // 语料被改动后必须拒收，且不创建根。
    let broken = sandbox("adopt-broken");
    let copy = broken.join("package");
    fs::create_dir_all(copy.join("corpus")).expect("临时包必须可创建");
    for name in ["author.json", "corpus-manifest.json"] {
        fs::copy(package.join(name), copy.join(name)).expect("文件必须可复制");
    }
    for entry in fs::read_dir(package.join("corpus")).expect("语料目录必须可读") {
        let entry = entry.expect("目录项必须可读");
        fs::copy(entry.path(), copy.join("corpus").join(entry.file_name()))
            .expect("语料必须可复制");
    }
    let target = copy.join("corpus").join("chapter-pdc.md");
    let mut bytes = fs::read(&target).expect("语料必须可读");
    bytes.push(b'x');
    fs::write(&target, &bytes).expect("语料必须可写");
    let fresh = broken.join("root");
    let output = adopt(&copy, &fresh);
    assert!(!output.status.success());
    assert!(!fresh.exists(), "语料不匹配时不得创建修订根");
}

#[test]
fn gap_requires_sources_and_reports_the_exact_impact_set() {
    let package = fixture();
    let root = sandbox("gap").join("root");
    assert!(adopt(&package, &root).status.success());

    let work = sandbox("gap-inputs");
    let mut record: serde_json::Value =
        serde_json::from_slice(&fs::read(package.join("gaps/gap-net-account.json")).unwrap())
            .unwrap();
    record["gap_id"] = serde_json::Value::from("gap-no-source");
    record["source"] = serde_json::Value::Array(Vec::new());
    let path = work.join("no-source.json");
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    let output = cli(&[
        "revision".to_owned(),
        "gap".to_owned(),
        arg(&root),
        arg(&path),
    ]);
    assert!(!output.status.success());
    assert!(
        text(&output).contains("REVISION_ARGUMENT"),
        "{}",
        text(&output)
    );

    // 影响范围必须恰好等于引用该原子的表示。
    let mut narrow = record.clone();
    narrow["gap_id"] = serde_json::Value::from("gap-narrow");
    narrow["source"] = serde_json::Value::from(vec![
        serde_json::json!({"source": "chapter-glycolysis.md", "locator": "#净能量收支"}),
    ]);
    narrow["impact"] = serde_json::Value::from(vec!["rep-energy"]);
    let path = work.join("narrow.json");
    fs::write(&path, serde_json::to_vec(&narrow).unwrap()).unwrap();
    let output = cli(&[
        "revision".to_owned(),
        "gap".to_owned(),
        arg(&root),
        arg(&path),
    ]);
    assert!(!output.status.success());
    assert!(text(&output).contains("影响范围"), "{}", text(&output));

    let accepted = gap(&root, &package, "gap-net-account");
    assert!(accepted.status.success(), "{}", text(&accepted));
    let value = listing(&root);
    assert_eq!(gap_status(&value, "gap-net-account"), "open");
    assert_eq!(ir_version(&value), 1, "登记缺口本身不生成新的 IR 版本");
    assert_eq!(
        value["revisions"].as_array().map(Vec::len),
        Some(0),
        "缺口不能自动变成修订"
    );
}

#[test]
fn revision_creates_a_new_ir_version_and_recompiles_only_dependents() {
    let package = fixture();
    let root = sandbox("revision").join("root");
    assert!(adopt(&package, &root).status.success());
    assert!(gap(&root, &package, "gap-net-account").status.success());
    let before = tree(&root);

    let output = revise(&root, &package, "rev-net-1");
    assert!(output.status.success(), "{}", text(&output));
    let value = listing(&root);
    assert_eq!(ir_version(&value), 2);
    let revisions = value["revisions"].as_array().cloned().unwrap_or_default();
    assert_eq!(revisions.len(), 1);
    assert_eq!(
        revisions[0]["revision_id"],
        serde_json::Value::from("rev-net-1")
    );
    assert_eq!(revisions[0]["ir_version"], serde_json::Value::from(2));
    assert_eq!(
        revisions[0]["recompiled"],
        serde_json::Value::from(vec!["rep-energy", "rep-trace"]),
        "只有声明依赖的表示被重编译"
    );
    assert_eq!(
        revisions[0]["unchanged"],
        serde_json::Value::from(vec!["rep-narrative"]),
        "未受影响的表示必须原样保留"
    );
    assert_eq!(gap_status(&value, "gap-net-account"), "resolved");

    let after = tree(&root);
    assert_eq!(
        before["render/rep-narrative.md"], after["render/rep-narrative.md"],
        "未受影响表示的字节必须完全不变"
    );
    assert_ne!(
        before["render/rep-energy.md"], after["render/rep-energy.md"],
        "依赖表示必须真的被重编译"
    );
}

#[test]
fn overturn_without_new_evidence_is_refused_without_touching_bytes() {
    let package = fixture();
    let root = sandbox("overturn").join("root");
    assert!(adopt(&package, &root).status.success());
    assert!(gap(&root, &package, "gap-net-account").status.success());
    assert!(revise(&root, &package, "rev-net-1").status.success());
    assert!(revise(&root, &package, "rev-net-2").status.success());
    let before = tree(&root);

    let output = revise(&root, &package, "rev-net-3");
    assert!(!output.status.success());
    let message = text(&output);
    assert!(
        message.contains("REVISION_NO_NEW_EVIDENCE"),
        "改回旧值且没有新来源必须被拒：{message}"
    );
    let value = listing(&root);
    assert_eq!(ir_version(&value), 3, "被拒的修订不生成新版本");
    assert_eq!(before, tree(&root), "被拒的修订不得修改任何字节");
}

#[test]
fn exceeding_the_revision_limit_escalates_and_blocks_automation() {
    let package = fixture();
    let root = sandbox("escalate").join("root");
    assert!(adopt(&package, &root).status.success());
    assert!(gap(&root, &package, "gap-net-account").status.success());
    for name in ["rev-net-1", "rev-net-2", "rev-net-4"] {
        let output = revise(&root, &package, name);
        assert!(output.status.success(), "{name}: {}", text(&output));
    }
    let value = listing(&root);
    let record = value["gaps"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .find(|item| item["gap_id"] == serde_json::Value::from("gap-net-account"))
        .expect("缺口必须存在");
    assert_eq!(record["status"], serde_json::Value::from("escalated"));
    assert_eq!(
        record["severity"],
        serde_json::Value::from("manual_judgment_required"),
        "超出上限后必须升级为人工判断"
    );
    assert_eq!(record["revisions"], serde_json::Value::from(3));

    let before = tree(&root);
    let blocked = revise(&root, &package, "rev-net-5");
    assert!(!blocked.status.success());
    assert!(
        text(&blocked).contains("REVISION_LIMIT_ESCALATED"),
        "{}",
        text(&blocked)
    );
    assert_eq!(before, tree(&root), "已升级的缺口不再接受自动修订");
}

#[test]
fn sourceless_revision_is_marked_pending_and_not_published() {
    let package = fixture();
    let root = sandbox("pending").join("root");
    assert!(adopt(&package, &root).status.success());
    assert!(gap(&root, &package, "gap-pdc-nadh").status.success());
    let output = revise(&root, &package, "rev-pdc-1");
    assert!(output.status.success(), "{}", text(&output));
    let value = listing(&root);
    assert_eq!(gap_status(&value, "gap-pdc-nadh"), "pending_verification");
    assert_eq!(
        verification(&value).get("pending_verification").copied(),
        Some(1)
    );
    let files = tree(&root);
    let energy = String::from_utf8_lossy(&files["render/rep-energy.md"]).into_owned();
    assert!(
        energy.contains("【待核验】"),
        "无来源补充不得显示为已核验结论：{energy}"
    );
}

#[test]
fn noop_revision_is_refused_without_burning_a_version() {
    let package = fixture();
    let root = sandbox("noop").join("root");
    assert!(adopt(&package, &root).status.success());
    assert!(gap(&root, &package, "gap-pdc-nadh").status.success());
    assert!(revise(&root, &package, "rev-pdc-1").status.success());
    let before = tree(&root);
    let output = revise(&root, &package, "rev-pdc-noop");
    assert!(!output.status.success());
    assert!(
        text(&output).contains("REVISION_ARGUMENT"),
        "{}",
        text(&output)
    );
    assert_eq!(ir_version(&listing(&root)), 2, "空修订不得刷出版本");
    assert_eq!(before, tree(&root));
}

#[test]
fn whole_frozen_sequence_survives_a_restart_and_matches_expectations() {
    let package = fixture();
    let root = sandbox("sequence").join("root");
    let cases: serde_json::Value =
        serde_json::from_slice(&fs::read(package.join("cases.json")).expect("cases 必须可读"))
            .expect("cases 必须是 JSON");
    let expected = cases["expected"].clone();
    for step in cases["sequence"].as_array().cloned().unwrap_or_default() {
        let action = step["action"].as_str().unwrap_or("");
        let input = step["input"].as_str().unwrap_or("");
        let output = match action {
            "adopt" => adopt(&package, &root),
            "gap" => cli(&[
                "revision".to_owned(),
                "gap".to_owned(),
                arg(&root),
                arg(&package.join(input)),
            ]),
            "revise" => cli(&[
                "revision".to_owned(),
                "revise".to_owned(),
                arg(&root),
                arg(&package.join(input)),
            ]),
            other => panic!("未知步骤动作 {other}"),
        };
        let round = step["round"].as_str().unwrap_or("");
        let refused = expected["refusals"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .any(|item| item["round"].as_str() == Some(round));
        if refused {
            assert!(
                !output.status.success(),
                "{round} 必须被拒：{}",
                text(&output)
            );
            let record = expected["refusals"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .find(|item| item["round"].as_str() == Some(round))
                .expect("拒绝记录必须存在");
            let code = record["code"].as_str().unwrap_or("");
            assert!(
                text(&output).contains(code),
                "{round} 期望 {code}，实际 {}",
                text(&output)
            );
            assert_eq!(
                output.status.code().map(i64::from),
                record["exit"].as_i64(),
                "{round} 的退出码必须与冻结预期一致"
            );
        } else {
            assert!(output.status.success(), "{round}: {}", text(&output));
        }
    }

    // 重新打开一个新进程读账本，结果必须与冻结预期一致。
    let value = listing(&root);
    assert_eq!(
        ir_version(&value),
        expected["ir_versions"]
            .as_array()
            .and_then(|items| items.last())
            .and_then(|item| item["version"].as_u64())
            .unwrap_or_default(),
        "最终 IR 版本号必须与冻结预期一致"
    );
    let revisions: Vec<serde_json::Value> = value["revisions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            serde_json::json!({
                "revision_id": item["revision_id"],
                "gap_id": item["gap_id"],
                "ir_version": item["ir_version"],
                "recompiled": item["recompiled"],
                "unchanged": item["unchanged"],
            })
        })
        .collect();
    let expected_revisions: Vec<serde_json::Value> = expected["revisions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            serde_json::json!({
                "revision_id": item["revision_id"],
                "gap_id": item["gap_id"],
                "ir_version": item["ir_version"],
                "recompiled": item["recompiled"],
                "unchanged": item["unchanged"],
            })
        })
        .collect();
    assert_eq!(revisions, expected_revisions, "修订记录必须与冻结预期一致");

    let gaps: Vec<serde_json::Value> = value["gaps"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            serde_json::json!({
                "gap_id": item["gap_id"],
                "status": item["status"],
                "severity": item["severity"],
                "revisions": item["revisions"],
            })
        })
        .collect();
    let expected_gaps: Vec<serde_json::Value> = expected["gaps"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            serde_json::json!({
                "gap_id": item["gap_id"],
                "status": item["status"],
                "severity": item["severity"],
                "revisions": item["revisions"],
            })
        })
        .collect();
    assert_eq!(gaps, expected_gaps, "缺口状态必须与冻结预期一致");
    assert_eq!(
        verification(&value),
        expected["verification"]
            .as_object()
            .map(|map| {
                map.iter()
                    .map(|(key, number)| (key.clone(), number.as_u64().unwrap_or_default()))
                    .collect::<BTreeMap<String, u64>>()
            })
            .unwrap_or_default()
    );
    assert_eq!(value["network_calls"], serde_json::Value::from(0));
    assert_eq!(value["model_calls"], serde_json::Value::from(0));
}

#[test]
fn corrupted_ledger_and_foreign_layout_are_refused_without_touching_site() {
    let package = fixture();
    let root = sandbox("corrupt").join("root");
    assert!(adopt(&package, &root).status.success());
    let database = root.join("control").join("revision.sqlite");
    let mut bytes = fs::read(&database).expect("账本必须可读");
    for value in bytes.iter_mut().take(16) {
        *value = b'X';
    }
    fs::write(&database, &bytes).expect("账本必须可写");
    let before = tree(&root);
    let output = cli(&["revision".to_owned(), "list".to_owned(), arg(&root)]);
    assert!(!output.status.success());
    assert!(
        text(&output).contains("PROJECT_INVALID_DATABASE"),
        "{}",
        text(&output)
    );
    assert_eq!(before, tree(&root), "损坏的账本不得被自动修复或改写");

    let foreign = sandbox("foreign");
    fs::create_dir_all(foreign.join("control")).expect("外来目录必须可创建");
    let output = cli(&["revision".to_owned(), "status".to_owned(), arg(&foreign)]);
    assert!(!output.status.success());
    assert!(
        text(&output).contains("不是修订根"),
        "外来目录必须被拒绝：{}",
        text(&output)
    );

    let missing = sandbox("missing").join("absent");
    let output = cli(&["revision".to_owned(), "status".to_owned(), arg(&missing)]);
    assert!(!output.status.success());
    assert!(
        text(&output).contains("REVISION_UNKNOWN_ROOT"),
        "不存在的根必须报未知根：{}",
        text(&output)
    );
}

#[test]
fn owner_tampering_is_refused_with_revision_specific_code() {
    let package = fixture();
    let root = sandbox("owner").join("root");
    assert!(adopt(&package, &root).status.success());
    let path = root.join("control").join("owner.json");
    let bytes = fs::read(&path).expect("owner 必须可读");
    let mut tampered = Vec::with_capacity(bytes.len() + 1);
    tampered.push(bytes[0]);
    tampered.push(b' ');
    tampered.extend_from_slice(&bytes[1..]);
    fs::write(&path, &tampered).expect("owner 必须可写");
    let output = cli(&["revision".to_owned(), "status".to_owned(), arg(&root)]);
    assert!(!output.status.success());
    let message = text(&output);
    assert!(
        message.contains("REVISION_INTEGRITY"),
        "修订账本必须报自己的完整性错误码：{message}"
    );
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["revision".to_owned(), "--help".to_owned()]);
    assert!(help.status.success(), "{}", text(&help));
    assert!(String::from_utf8_lossy(&help.stdout).contains("写作反馈缺口"));

    let bare = cli(&["revision".to_owned()]);
    assert!(bare.status.success(), "不带动词时显示帮助：{}", text(&bare));

    let unknown = cli(&["revision".to_owned(), "unknown".to_owned()]);
    assert_eq!(unknown.status.code(), Some(2));

    let missing = cli(&["revision".to_owned(), "adopt".to_owned()]);
    assert_eq!(missing.status.code(), Some(2));

    let single = cli(&[
        "revision".to_owned(),
        "revise".to_owned(),
        arg(&PathBuf::from("/tmp/only-one")),
    ]);
    assert_eq!(single.status.code(), Some(2));

    let dash = cli(&["revision".to_owned(), "list".to_owned(), "-".to_owned()]);
    assert_eq!(dash.status.code(), Some(2));
}
