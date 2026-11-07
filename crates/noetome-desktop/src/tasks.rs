//! Desktop task centre: a read-only projection of the four bounded roots.
//!
//! Every number, state and refusal code in this module comes from
//! `noetome_cli`. The desktop layer never derives a count, never converts a
//! count into a percentage, never repairs a damaged ledger and never opens a
//! root for writing: `fixture_run::status`, `research_store::list`,
//! `retrieval_store::list` and `revision_store::list` are all read-only entry
//! points, and this module adds no filesystem call of its own except asking
//! whether the directory exists so that a missing root can be reported as
//! "not established" instead of as a read failure.
//!
//! The frontend renders `facts`, `dimensions` and `counts` verbatim. It must
//! not invent a status, must not show "ok" when the read failed, and must not
//! hide a failure behind an empty list.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

/// Root kinds this projection understands. Anything else is refused by name.
pub const KIND_RUN: &str = "run";
pub const KIND_RESEARCH: &str = "research";
pub const KIND_RETRIEVE: &str = "retrieve";
pub const KIND_REVISION: &str = "revision";
pub const KINDS: [&str; 4] = [KIND_RUN, KIND_RESEARCH, KIND_RETRIEVE, KIND_REVISION];

/// A root that exists and was read successfully.
pub const STATE_PRESENT: &str = "present";
/// A root that does not exist yet: "not established", not an error.
pub const STATE_ABSENT: &str = "absent";
/// A root that was read but whose own ledger reports a failed run.
pub const STATE_FAILED: &str = "failed";
/// A root that exists but cannot be read as the requested kind.
pub const STATE_UNREADABLE: &str = "unreadable";

/// The four status dimensions every root reports, in the same order the
/// compile report uses. They are shown as states plus counts only.
const DIMENSIONS: [(&str, &str); 4] = [
    ("source_coverage", "来源覆盖"),
    ("knowledge_compilation", "知识编译"),
    ("quality_validation", "质量验证"),
    ("optional_research", "可选研究"),
];

/// One label/value pair rendered verbatim by the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fact {
    pub label: String,
    pub value: String,
}

/// One selectable root kind, with the label the page shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskKind {
    pub id: &'static str,
    pub label: &'static str,
}

/// The kinds the task centre offers, in display order.
pub fn kinds() -> Vec<TaskKind> {
    KINDS
        .iter()
        .map(|kind| TaskKind {
            id: kind,
            label: kind_label(kind),
        })
        .collect()
}

/// One of the four status dimensions, with counts instead of percentages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DimensionView {
    pub id: String,
    pub label: String,
    pub state: String,
    pub reason: String,
    pub counts: Vec<Fact>,
}

/// Everything the task centre page needs for one root, all values copied from
/// the CLI response (or from its refusal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskSummary {
    pub kind: String,
    pub kind_label: String,
    pub root: String,
    pub state: String,
    pub state_label: String,
    /// Empty on success; otherwise the CLI refusal code such as
    /// `RESEARCH_UNKNOWN_ROOT`. The frontend shows it as-is.
    pub code: String,
    /// Empty on success; otherwise the full `CODE：说明` line.
    pub message: String,
    pub headline: String,
    pub facts: Vec<Fact>,
    pub dimensions: Vec<DimensionView>,
    pub counts: Vec<Fact>,
    /// The verbatim CLI JSON, kept so that nothing is summarised away and so
    /// that the page can show the original document on demand.
    pub record: Option<Value>,
}

/// One dimension before its id/label are attached.
struct DimensionDraft {
    state: String,
    reason: String,
    counts: Vec<Fact>,
}

fn draft(state: &str, reason: impl Into<String>, counts: Vec<Fact>) -> DimensionDraft {
    DimensionDraft {
        state: state.to_owned(),
        reason: reason.into(),
        counts,
    }
}

fn fact(label: impl Into<String>, value: impl Into<String>) -> Fact {
    Fact {
        label: label.into(),
        value: value.into(),
    }
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        KIND_RUN => "六单元运行",
        KIND_RESEARCH => "研究队列",
        KIND_RETRIEVE => "有界检索",
        KIND_REVISION => "写作反馈修订",
        _ => "未知",
    }
}

fn state_label(state: &str) -> &'static str {
    match state {
        STATE_PRESENT => "已读取",
        STATE_ABSENT => "尚未建立",
        STATE_FAILED => "运行失败",
        STATE_UNREADABLE => "无法读取",
        _ => "未知",
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn text(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn nested_number(value: &Value, outer: &str, inner: &str) -> u64 {
    value
        .get(outer)
        .and_then(|inner_value| inner_value.get(inner))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn array_len(value: &Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

fn count(value: &Value, key: &str) -> Fact {
    fact(key.to_owned(), number(value, key).to_string())
}

fn dimensions(
    source: DimensionDraft,
    compilation: DimensionDraft,
    quality: DimensionDraft,
    research: DimensionDraft,
) -> Vec<DimensionView> {
    let rendered = [source, compilation, quality, research];
    DIMENSIONS
        .iter()
        .zip(rendered)
        .map(|((id, label), entry)| DimensionView {
            id: (*id).to_owned(),
            label: (*label).to_owned(),
            state: entry.state,
            reason: entry.reason,
            counts: entry.counts,
        })
        .collect()
}

fn count_state(completed: u64, total: u64) -> &'static str {
    if total == 0 {
        "not_applicable"
    } else if completed == total {
        "complete"
    } else {
        "incomplete"
    }
}

fn run_view(path: &Path, record: Value, failed: bool) -> TaskSummary {
    let confirmed = number(&record, "confirmed_units");
    let total = array_len(&record, "work_units") as u64;
    let state = if failed { STATE_FAILED } else { STATE_PRESENT };
    let mut facts = vec![
        fact("运行 ID", text(&record, "run_id")),
        fact("发布 ID", text(&record, "release_id")),
        fact("运行状态", text(&record, "run_status")),
        fact("检查点序号", text(&record, "checkpoint_sequence")),
        fact("确认单元", format!("{confirmed}/{total}")),
        fact("发布状态", text(&record, "release_status")),
        fact("科学质量", text(&record, "scientific_quality")),
        fact("编译指纹", text(&record, "compiler_fingerprint")),
    ];
    if let Some(controls) = record.get("controls") {
        let action = text(controls, "action");
        if !action.is_empty() {
            facts.push(fact("最近控制", action));
        }
    }
    let counts = vec![
        count(&record, "confirmed_units"),
        fact("工作单元总数", total.to_string()),
        count(&record, "real_model_calls"),
        count(&record, "paid_calls"),
    ];
    let compilation = match record
        .get("validation_report")
        .filter(|value| !value.is_null())
    {
        Some(report) => {
            let dimension = report
                .get("dimensions")
                .and_then(|all| all.get("knowledge_compilation"));
            match dimension {
                Some(dimension) => draft(
                    &text(dimension, "state"),
                    text(dimension, "reason"),
                    Vec::new(),
                ),
                None => draft("not_started", "编译报告里没有该维度。", Vec::new()),
            }
        }
        None => draft(
            "not_started",
            "运行尚未产生编译报告：不显示完成，也不显示失败。",
            Vec::new(),
        ),
    };
    let dimensions = dimensions(
        draft(
            count_state(confirmed, total),
            "按运行检查点统计已确认单元。",
            vec![
                fact("已确认单元", confirmed.to_string()),
                fact("工作单元", total.to_string()),
            ],
        ),
        compilation,
        draft(
            "not_run",
            "本轮没有执行科学质量验证（scientific_quality 不为 measured）。",
            Vec::new(),
        ),
        draft(
            "not_requested",
            "本次运行未启用研究检索或远程模型抽取。",
            vec![fact("真实模型调用", text(&record, "real_model_calls"))],
        ),
    );
    let headline = format!(
        "运行状态 {}，确认单元 {confirmed}/{total}。",
        text(&record, "run_status")
    );
    TaskSummary {
        kind: KIND_RUN.to_owned(),
        kind_label: kind_label(KIND_RUN).to_owned(),
        root: display_path(path),
        state: state.to_owned(),
        state_label: state_label(state).to_owned(),
        code: String::new(),
        message: if failed {
            "运行自身报告失败：下方字段是账本原文，没有隐藏。".to_owned()
        } else {
            String::new()
        },
        headline,
        facts,
        dimensions,
        counts,
        record: Some(record),
    }
}

/// Group `state` strings into ordered (name, count) pairs.
fn tally(items: &[Value], key: &str) -> Vec<(String, u64)> {
    let mut groups: Vec<(String, u64)> = Vec::new();
    for item in items {
        let state = text(item, key);
        match groups.iter_mut().find(|(name, _)| *name == state) {
            Some((_, value)) => *value += 1,
            None => groups.push((state, 1)),
        }
    }
    groups
}

fn research_view(path: &Path, record: Value) -> TaskSummary {
    let must_do = array_len(&record, "must_do");
    let queue_only = array_len(&record, "queue_only");
    let candidates = record
        .get("candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let by_state = tally(&candidates, "state");
    let counters = record.get("counters").cloned().unwrap_or(Value::Null);
    let mut counter_counts = Vec::new();
    for key in [
        "observations",
        "blocked_repeats",
        "reopenings",
        "merges",
        "decisions",
    ] {
        counter_counts.push(count(&counters, key));
    }
    let objective = record.get("objective").cloned().unwrap_or(Value::Null);
    let facts = vec![
        fact("案例 ID", text(&record, "case_id")),
        fact("本轮目标", text(&objective, "summary")),
        fact(
            "授权目标",
            array_len(&objective, "authorized_targets").to_string(),
        ),
        fact(
            "必需前提",
            array_len(&objective, "required_prerequisites").to_string(),
        ),
        fact("必做集合", must_do.to_string()),
        fact("越界候选项", queue_only.to_string()),
        fact("候选总数", candidates.len().to_string()),
        fact("检索调用", text(&record, "retrieval_calls")),
        fact("模型调用", text(&record, "model_calls")),
    ];
    let mut counts = vec![
        fact("必做集合", must_do.to_string()),
        fact("越界候选项", queue_only.to_string()),
        fact("候选总数", candidates.len().to_string()),
        count(&record, "retrieval_calls"),
        count(&record, "model_calls"),
    ];
    for (state, amount) in &by_state {
        counts.push(fact(format!("候选 {state}"), amount.to_string()));
    }
    let dimensions = dimensions(
        draft(
            "complete",
            "必做集合由 adopt 冻结，之后不接受增删。",
            counter_counts,
        ),
        draft(
            "complete",
            "研究候选单列，不计入主任务的完成或失败。",
            by_state
                .iter()
                .map(|(state, amount)| fact(format!("候选 {state}"), amount.to_string()))
                .collect(),
        ),
        draft("not_run", "本轮没有执行科学质量验证。", Vec::new()),
        draft(
            "not_requested",
            "本轮未执行检索：队列从不联网、不调用模型。",
            vec![fact("候选总数", candidates.len().to_string())],
        ),
    );
    let headline = format!(
        "必做集合 {must_do} 项，候选 {} 项，越界单列 {queue_only} 项。",
        candidates.len()
    );
    TaskSummary {
        kind: KIND_RESEARCH.to_owned(),
        kind_label: kind_label(KIND_RESEARCH).to_owned(),
        root: display_path(path),
        state: STATE_PRESENT.to_owned(),
        state_label: state_label(STATE_PRESENT).to_owned(),
        code: String::new(),
        message: String::new(),
        headline,
        facts,
        dimensions,
        counts,
        record: Some(record),
    }
}

fn retrieve_view(path: &Path, record: Value) -> TaskSummary {
    let units = record
        .get("units")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total = units.len() as u64;
    let settled = units
        .iter()
        .filter(|unit| text(unit, "state") == "settled")
        .count() as u64;
    let outstanding = total.saturating_sub(settled);
    let external = record
        .get("external_retrieval")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let termination = record.get("termination").cloned().unwrap_or(Value::Null);
    let reason = text(&termination, "reason");
    let note = text(&termination, "note");
    let facts = vec![
        fact("案例 ID", text(&record, "case_id")),
        fact("策略", text(&record, "policy")),
        fact(
            "外部检索",
            if external {
                "已允许".to_owned()
            } else {
                "已关闭（零网络请求）".to_owned()
            },
        ),
        fact(
            "冻结语料",
            format!(
                "{} 个文档、{} 字节",
                nested_number(&record, "corpus", "documents"),
                nested_number(&record, "corpus", "bytes")
            ),
        ),
        fact(
            "已用预算（查询/字节/毫秒）",
            format!(
                "{} / {} / {}",
                nested_number(&record, "budget", "queries_used"),
                nested_number(&record, "budget", "bytes_used"),
                nested_number(&record, "budget", "millis_used")
            ),
        ),
        fact(
            "剩余额度（查询/字节/毫秒）",
            format!(
                "{} / {} / {}",
                nested_number(&record, "budget", "queries_remaining"),
                nested_number(&record, "budget", "bytes_remaining"),
                nested_number(&record, "budget", "millis_remaining")
            ),
        ),
        fact(
            "暂停控制",
            if record
                .get("paused")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "已暂停".to_owned()
            } else {
                "未暂停".to_owned()
            },
        ),
        fact(
            "终止理由",
            if reason.is_empty() {
                "尚未记录".to_owned()
            } else {
                format!("{reason}（{note}）")
            },
        ),
        fact("网络请求", text(&record, "network_requests")),
        fact("模型调用", text(&record, "model_calls")),
    ];
    let mut counts = vec![
        fact("已结算查询", settled.to_string()),
        fact("待执行或待核验", outstanding.to_string()),
        fact("查询总数", total.to_string()),
        count(&record, "network_requests"),
        count(&record, "model_calls"),
    ];
    if let Some(totals) = record.get("totals").and_then(Value::as_object) {
        for (outcome, amount) in totals {
            counts.push(fact(
                format!("结果 {outcome}"),
                amount.as_u64().unwrap_or(0).to_string(),
            ));
        }
    }
    let dimensions = dimensions(
        draft(
            "complete",
            "语料在 adopt 时冻结，每次扫描前逐字节复核。",
            vec![
                fact(
                    "冻结文档",
                    nested_number(&record, "corpus", "documents").to_string(),
                ),
                fact(
                    "冻结字节",
                    nested_number(&record, "corpus", "bytes").to_string(),
                ),
            ],
        ),
        draft(
            count_state(settled, total),
            "按查询状态计数；预留即计入预算，未用额度不回收。",
            vec![
                fact("查询总数", total.to_string()),
                fact("已结算", settled.to_string()),
                fact("待执行或待核验", outstanding.to_string()),
            ],
        ),
        draft(
            "not_run",
            "本轮没有执行科学质量验证：命中只是证据线索。",
            Vec::new(),
        ),
        draft(
            if reason.is_empty() {
                "incomplete"
            } else {
                "complete"
            },
            if reason.is_empty() {
                "本轮尚未记录终止理由。"
            } else {
                "终止理由已落盘，未完成目标保持待执行。"
            },
            vec![fact(
                "终止理由",
                if reason.is_empty() {
                    "尚未记录".to_owned()
                } else {
                    reason.clone()
                },
            )],
        ),
    );
    let headline = format!(
        "查询 {settled}/{total} 已结算；终止理由 {}。",
        if reason.is_empty() {
            "尚未记录"
        } else {
            reason.as_str()
        }
    );
    TaskSummary {
        kind: KIND_RETRIEVE.to_owned(),
        kind_label: kind_label(KIND_RETRIEVE).to_owned(),
        root: display_path(path),
        state: STATE_PRESENT.to_owned(),
        state_label: state_label(STATE_PRESENT).to_owned(),
        code: String::new(),
        message: String::new(),
        headline,
        facts,
        dimensions,
        counts,
        record: Some(record),
    }
}

fn revision_view(path: &Path, record: Value) -> TaskSummary {
    let verified = nested_number(&record, "verification", "verified");
    let pending = nested_number(&record, "verification", "pending_verification");
    let atoms = array_len(&record, "atoms") as u64;
    let gaps = record
        .get("gaps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let revisions = array_len(&record, "revisions");
    let gap_states = tally(&gaps, "status");
    let representations = array_len(&record, "representations");
    let facts = vec![
        fact("案例 ID", text(&record, "case_id")),
        fact("策略", text(&record, "policy")),
        fact("IR 版本", text(&record, "ir_version")),
        fact("原子摘要", text(&record, "ir_atoms_sha256")),
        fact("原子总数", atoms.to_string()),
        fact("已核验原子", verified.to_string()),
        fact("待核验原子", pending.to_string()),
        fact("表示数量", representations.to_string()),
        fact("缺口", gaps.len().to_string()),
        fact("修订", revisions.to_string()),
        fact("网络调用", text(&record, "network_calls")),
        fact("模型调用", text(&record, "model_calls")),
    ];
    let mut counts = vec![
        fact("原子总数", atoms.to_string()),
        fact("已核验原子", verified.to_string()),
        fact("待核验原子", pending.to_string()),
        fact("缺口", gaps.len().to_string()),
        fact("修订", revisions.to_string()),
        count(&record, "network_calls"),
        count(&record, "model_calls"),
    ];
    for (state, amount) in &gap_states {
        counts.push(fact(format!("缺口 {state}"), amount.to_string()));
    }
    let dimensions = dimensions(
        draft(
            count_state(verified, atoms),
            "按原子核验状态计数；待核验原子不发布为结论。",
            vec![
                fact("原子总数", atoms.to_string()),
                fact("已核验", verified.to_string()),
                fact("待核验", pending.to_string()),
            ],
        ),
        draft(
            "complete",
            "每次修订都生成新的 IR 版本，并重编译声明依赖的表示。",
            vec![
                fact("IR 版本", text(&record, "ir_version")),
                fact("修订次数", revisions.to_string()),
                fact("表示数量", representations.to_string()),
            ],
        ),
        draft(
            "not_run",
            "本轮没有执行科学质量验证：重编译成功不等于知识正确。",
            Vec::new(),
        ),
        draft(
            "not_requested",
            "本轮未启用检索或模型抽取。",
            vec![
                count(&record, "network_calls"),
                count(&record, "model_calls"),
            ],
        ),
    );
    let headline = format!(
        "IR v{}，原子 {verified}/{atoms} 已核验，缺口 {} 条，修订 {revisions} 次。",
        text(&record, "ir_version"),
        gaps.len()
    );
    TaskSummary {
        kind: KIND_REVISION.to_owned(),
        kind_label: kind_label(KIND_REVISION).to_owned(),
        root: display_path(path),
        state: STATE_PRESENT.to_owned(),
        state_label: state_label(STATE_PRESENT).to_owned(),
        code: String::new(),
        message: String::new(),
        headline,
        facts,
        dimensions,
        counts,
        record: Some(record),
    }
}

fn absent(kind: &str, path: &Path) -> TaskSummary {
    TaskSummary {
        kind: kind.to_owned(),
        kind_label: kind_label(kind).to_owned(),
        root: display_path(path),
        state: STATE_ABSENT.to_owned(),
        state_label: state_label(STATE_ABSENT).to_owned(),
        code: String::new(),
        message: "这条根还没有建立；本页不会替你创建或接管它。".to_owned(),
        headline: "尚未建立".to_owned(),
        facts: Vec::new(),
        dimensions: Vec::new(),
        counts: Vec::new(),
        record: None,
    }
}

fn unreadable(kind: &str, path: &Path, code: &'static str, message: String) -> TaskSummary {
    TaskSummary {
        kind: kind.to_owned(),
        kind_label: kind_label(kind).to_owned(),
        root: display_path(path),
        state: STATE_UNREADABLE.to_owned(),
        state_label: state_label(STATE_UNREADABLE).to_owned(),
        code: code.to_owned(),
        message,
        headline: "无法读取：下面是 CLI 返回的原错误码，没有当作空任务。".to_owned(),
        facts: Vec::new(),
        dimensions: Vec::new(),
        counts: Vec::new(),
        record: None,
    }
}

fn parse(kind: &str, path: &Path, json: &str) -> Result<TaskSummary, String> {
    let record: Value = serde_json::from_str(json)
        .map_err(|error| format!("DESKTOP_TASK_RECORD：{kind} 返回的内容不是 JSON：{error}"))?;
    Ok(match kind {
        KIND_RUN => run_view(path, record, false),
        KIND_RESEARCH => research_view(path, record),
        KIND_RETRIEVE => retrieve_view(path, record),
        KIND_REVISION => revision_view(path, record),
        _ => unreachable!("kind is validated before parsing"),
    })
}

/// Read one root of `kind` without writing anything.
///
/// A missing directory is reported as `absent`, a readable root as `present`
/// (or `failed` when its own ledger says the run failed), and every refusal
/// keeps the CLI code intact inside `code`/`message`.
pub fn read(kind: &str, path: &Path) -> Result<TaskSummary, String> {
    if !KINDS.contains(&kind) {
        return Err(format!(
            "DESKTOP_TASK_KIND：未知任务类型 {kind}；只支持 {}。本次没有读取任何目录。",
            KINDS.join("、")
        ));
    }
    if path.as_os_str().is_empty() {
        return Err("DESKTOP_TASK_PATH：请填写任务根目录。本次没有读取任何目录。".to_owned());
    }
    if !path.exists() {
        return Ok(absent(kind, path));
    }
    match kind {
        KIND_RUN => match noetome_cli::fixture_run::status(path) {
            Ok(output) => {
                if output.failed {
                    let record: Value = serde_json::from_str(&output.json).map_err(|error| {
                        format!("DESKTOP_TASK_RECORD：run 返回的内容不是 JSON：{error}")
                    })?;
                    Ok(run_view(path, record, true))
                } else {
                    parse(kind, path, &output.json)
                }
            }
            Err(error) => Ok(unreadable(kind, path, error.code(), error.to_string())),
        },
        KIND_RESEARCH => match noetome_cli::research_store::list(path) {
            Ok(json) => parse(kind, path, &json),
            Err(error) => Ok(unreadable(kind, path, error.code(), error.to_string())),
        },
        KIND_RETRIEVE => match noetome_cli::retrieval_store::list(path) {
            Ok(json) => parse(kind, path, &json),
            Err(error) => Ok(unreadable(kind, path, error.code(), error.to_string())),
        },
        KIND_REVISION => match noetome_cli::revision_store::list(path) {
            Ok(json) => parse(kind, path, &json),
            Err(error) => Ok(unreadable(kind, path, error.code(), error.to_string())),
        },
        _ => unreachable!("kind is validated above"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

    fn repository() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repository root")
    }

    fn sandbox(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/desktop-task-tests")
            .join(format!(
                "{label}-{}-{nanos}-{}",
                std::process::id(),
                NEXT_CASE.fetch_add(1, Ordering::SeqCst)
            ));
        fs::create_dir_all(&root).expect("create sandbox");
        root.canonicalize().expect("canonical sandbox")
    }

    /// Every regular file below `root`, keyed by relative path.
    fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut files = BTreeMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(directory) = stack.pop() {
            let entries = fs::read_dir(&directory).expect("read directory");
            for entry in entries {
                let entry = entry.expect("entry");
                let path = entry.path();
                let metadata = fs::symlink_metadata(&path).expect("metadata");
                if metadata.is_dir() {
                    stack.push(path);
                } else if metadata.is_file() {
                    let relative = path
                        .strip_prefix(root)
                        .expect("relative")
                        .to_string_lossy()
                        .replace('\\', "/");
                    files.insert(relative, fs::read(&path).expect("read file"));
                }
            }
        }
        files
    }

    fn run_root(base: &Path) -> PathBuf {
        let root = base.join("run");
        let case = repository().join("fixtures/run-checkpoint-v1/fixture");
        noetome_cli::fixture_run::start(&case, &root, true).expect("run start");
        root
    }

    fn research_root(base: &Path) -> PathBuf {
        let root = base.join("research");
        let package = repository().join("fixtures/research-queue-v1");
        noetome_cli::research_store::adopt(&package.join("author.json"), &root)
            .expect("research adopt");
        noetome_cli::research_store::observe(&root, &package.join("round-01-observations.json"))
            .expect("research observe");
        root
    }

    fn retrieve_root(base: &Path) -> PathBuf {
        let root = base.join("retrieve");
        let package = repository().join("fixtures/retrieval-budget-v1");
        noetome_cli::retrieval_store::adopt(&package.join("plan.json"), &root)
            .expect("retrieve adopt");
        noetome_cli::retrieval_store::step_text(&root).expect("retrieve step");
        root
    }

    fn revision_root(base: &Path) -> PathBuf {
        let root = base.join("revision");
        let package = repository().join("fixtures/revision-feedback-v1");
        noetome_cli::revision_store::adopt(&package.join("author.json"), &root)
            .expect("revision adopt");
        noetome_cli::revision_store::gap(&root, &package.join("gaps/gap-net-account.json"))
            .expect("revision gap");
        root
    }

    fn dimension<'a>(summary: &'a TaskSummary, id: &str) -> &'a DimensionView {
        summary
            .dimensions
            .iter()
            .find(|dimension| dimension.id == id)
            .expect("dimension present")
    }

    fn fact_value<'a>(facts: &'a [Fact], label: &str) -> Option<&'a str> {
        facts
            .iter()
            .find(|fact| fact.label == label)
            .map(|fact| fact.value.as_str())
    }

    #[test]
    fn unknown_kind_and_empty_path_are_refused_by_name() {
        let base = sandbox("unknown-kind");
        let error = read("archive", &base).expect_err("unknown kind refused");
        assert!(error.starts_with("DESKTOP_TASK_KIND"), "{error}");
        let error = read(KIND_RUN, Path::new("")).expect_err("empty path refused");
        assert!(error.starts_with("DESKTOP_TASK_PATH"), "{error}");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn missing_root_is_reported_as_not_established_not_as_a_failure() {
        let base = sandbox("absent");
        for kind in KINDS {
            let summary = read(kind, &base.join("absent")).expect("absent is not an error");
            assert_eq!(summary.state, STATE_ABSENT, "{kind}");
            assert!(summary.code.is_empty(), "{kind}");
            assert!(summary.record.is_none(), "{kind}");
            assert!(summary.message.contains("还没有建立"), "{kind}");
        }
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn four_kinds_are_read_without_writing_anything() {
        let base = sandbox("present");
        let roots = [
            (KIND_RUN, run_root(&base)),
            (KIND_RESEARCH, research_root(&base)),
            (KIND_RETRIEVE, retrieve_root(&base)),
            (KIND_REVISION, revision_root(&base)),
        ];
        for (kind, root) in &roots {
            let before = snapshot(root);
            let summary = read(kind, root).expect("read");
            let after = snapshot(root);
            assert_eq!(before, after, "{kind} must not write");
            assert_eq!(summary.state, STATE_PRESENT, "{kind}");
            assert_eq!(summary.dimensions.len(), 4, "{kind}");
            assert!(summary.record.is_some(), "{kind}");
            assert!(!summary.headline.is_empty(), "{kind}");
        }
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn values_are_copied_from_the_ledger_instead_of_being_derived() {
        let base = sandbox("verbatim");
        let root = run_root(&base);
        let summary = read(KIND_RUN, &root).expect("read");
        let record = summary.record.as_ref().expect("record");
        assert_eq!(
            fact_value(&summary.facts, "运行 ID"),
            record["run_id"].as_str(),
            "run id is copied verbatim"
        );
        assert_eq!(
            fact_value(&summary.facts, "运行状态"),
            record["run_status"].as_str()
        );
        assert_eq!(
            fact_value(&summary.facts, "授权目标"),
            None,
            "no invented fields"
        );
        assert_eq!(dimension(&summary, "source_coverage").state, "incomplete");
        assert_eq!(dimension(&summary, "quality_validation").state, "not_run");
        assert_eq!(
            dimension(&summary, "knowledge_compilation").state,
            "not_started"
        );
        assert_eq!(
            dimension(&summary, "optional_research").state,
            "not_requested"
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn research_candidates_are_listed_with_the_frozen_must_do_set() {
        let base = sandbox("research");
        let root = research_root(&base);
        let summary = read(KIND_RESEARCH, &root).expect("read");
        assert_eq!(summary.state, STATE_PRESENT);
        assert!(
            summary.headline.contains("必做集合 3 项"),
            "{}",
            summary.headline
        );
        assert_eq!(
            fact_value(&summary.facts, "必做集合"),
            Some("3"),
            "must do comes from the ledger"
        );
        let source = dimension(&summary, "source_coverage");
        assert_eq!(source.state, "complete");
        assert_eq!(
            fact_value(&source.counts, "observations"),
            Some("5"),
            "counter is the ledger value"
        );
        let compilation = dimension(&summary, "knowledge_compilation");
        assert!(
            compilation
                .counts
                .iter()
                .any(|fact| fact.label == "候选 proposed")
        );
        assert_eq!(
            dimension(&summary, "optional_research").state,
            "not_requested"
        );
        assert_eq!(fact_value(&summary.counts, "越界候选项"), Some("2"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn retrieval_budget_and_termination_are_reported_as_counts() {
        let base = sandbox("retrieve");
        let root = retrieve_root(&base);
        let summary = read(KIND_RETRIEVE, &root).expect("read");
        assert_eq!(summary.state, STATE_PRESENT);
        assert!(
            fact_value(&summary.facts, "外部检索")
                .expect("external flag")
                .contains("已关闭"),
            "{:?}",
            summary.facts
        );
        assert_eq!(fact_value(&summary.facts, "终止理由"), Some("尚未记录"));
        assert_eq!(fact_value(&summary.counts, "已结算查询"), Some("1"));
        assert_eq!(fact_value(&summary.counts, "待执行或待核验"), Some("7"));
        let research = dimension(&summary, "optional_research");
        assert_eq!(research.state, "incomplete");
        assert_eq!(research.counts[0].label, "终止理由");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn revision_gaps_and_revisions_are_listed_separately() {
        let base = sandbox("revision");
        let root = revision_root(&base);
        let summary = read(KIND_REVISION, &root).expect("read");
        assert_eq!(summary.state, STATE_PRESENT);
        assert!(summary.headline.contains("IR v1"), "{}", summary.headline);
        assert_eq!(fact_value(&summary.counts, "缺口 open"), Some("1"));
        assert_eq!(fact_value(&summary.counts, "修订"), Some("0"));
        assert_eq!(dimension(&summary, "source_coverage").state, "incomplete");
        assert_eq!(dimension(&summary, "quality_validation").state, "not_run");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_foreign_or_corrupt_root_keeps_the_cli_code_and_is_never_written() {
        let base = sandbox("unreadable");
        let empty = base.join("empty");
        fs::create_dir_all(&empty).expect("empty dir");
        let before = snapshot(&empty);
        let summary = read(KIND_RESEARCH, &empty).expect("readable");
        assert_eq!(summary.state, STATE_UNREADABLE);
        assert_eq!(summary.code, "RESEARCH_UNKNOWN_ROOT");
        assert!(
            summary.message.contains("未修改任何文件"),
            "{}",
            summary.message
        );
        assert_eq!(before, snapshot(&empty));

        let corrupt = retrieve_root(&base);
        let database = corrupt.join("control/retrieval.sqlite");
        let mut bytes = fs::read(&database).expect("read database");
        bytes[..16].copy_from_slice(b"NOT-A-SQLITE-DB!");
        fs::write(&database, &bytes).expect("write database");
        let before = snapshot(&corrupt);
        let summary = read(KIND_RETRIEVE, &corrupt).expect("readable");
        assert_eq!(summary.state, STATE_UNREADABLE);
        assert_eq!(summary.code, "PROJECT_INVALID_DATABASE");
        assert_eq!(before, snapshot(&corrupt));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_root_of_another_kind_is_refused_with_its_own_code() {
        let base = sandbox("cross-kind");
        let root = run_root(&base);
        let before = snapshot(&root);
        let summary = read(KIND_REVISION, &root).expect("readable");
        assert_eq!(summary.state, STATE_UNREADABLE);
        assert!(summary.code.starts_with("REVISION_"), "{}", summary.code);
        assert_eq!(before, snapshot(&root));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn paused_run_is_present_and_keeps_every_field() {
        let base = sandbox("paused");
        let root = run_root(&base);
        let before = snapshot(&root);
        let summary = read(KIND_RUN, &root).expect("read");
        assert_eq!(summary.state, STATE_PRESENT);
        assert_eq!(fact_value(&summary.facts, "运行状态"), Some("paused"));
        assert_eq!(fact_value(&summary.facts, "确认单元"), Some("0/6"));
        assert_eq!(before, snapshot(&root));
        fs::remove_dir_all(&base).ok();
    }
}
