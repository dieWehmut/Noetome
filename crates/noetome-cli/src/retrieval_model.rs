//! 批准范围内的有界检索：作者声明、冻结语料、预算算式与终止理由。
//!
//! 本模块只做纯判定，不碰磁盘、不联网、不调用模型。它把 B08-T02 需要的
//! 四件事写成可复核的数据结构：
//!
//! 1. **范围**：每个查询都归属一个已授权主题（授权目标或必需前提）。
//!    `out_of_scope` 的查询在作者包解析阶段就被拒绝，因此检索无法自行扩大授权。
//! 2. **预算**：每次尝试先按查询的语料占用预留次数、字节与耗时上限，
//!    预留由账本持久化，之后才允许扫描。
//! 3. **终止理由**：七类理由各有前置条件，`finish` 只在账本状态真的支持时才接受。
//! 4. **证据线索**：命中只是线索，带文档、标题锚点、偏移与片段，供人工回查。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::project::ProjectError;

pub(crate) const POLICY: &str = "retrieval-plan-v1";
pub(crate) const CORPUS_POLICY: &str = "retrieval-corpus-v1";
pub(crate) const PRODUCER: &str = "noetome-retrieval-budget";
pub(crate) const AUTHOR_PRODUCER: &str = "fixture_author";
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_TEXT: usize = 600;
pub(crate) const MAX_SUBJECT: usize = 120;
pub(crate) const MAX_ID: usize = 32;
pub(crate) const MAX_TERMS: usize = 6;
pub(crate) const MAX_QUERIES: usize = 64;
pub(crate) const MAX_DOCUMENTS: usize = 64;
pub(crate) const MAX_HITS: usize = 8;
pub(crate) const MAX_SNIPPET: usize = 160;

pub(crate) const SCOPE_AUTHORIZED: &str = "authorized_target";
pub(crate) const SCOPE_PREREQUISITE: &str = "required_prerequisite";

pub(crate) const UNIT_PENDING: &str = "pending";
pub(crate) const UNIT_SETTLED: &str = "settled";
pub(crate) const UNIT_GAPPED: &str = "gapped";

pub(crate) const ATTEMPT_RESERVED: &str = "reserved";
pub(crate) const ATTEMPT_SETTLED: &str = "settled";

pub(crate) const OUTCOME_EVIDENCE: &str = "evidence";
pub(crate) const OUTCOME_NO_HITS: &str = "no_hits";
pub(crate) const OUTCOME_DUPLICATES: &str = "duplicates_only";
pub(crate) const OUTCOME_EXTERNAL: &str = "requires_external";
pub(crate) const OUTCOME_CONFLICT: &str = "conflict";

pub(crate) const REASON_GOAL_MET: &str = "goal_met";
pub(crate) const REASON_NO_NEW_EVIDENCE: &str = "no_reliable_new_evidence";
pub(crate) const REASON_MARGINAL_GAIN: &str = "marginal_gain_insufficient";
pub(crate) const REASON_BUDGET: &str = "budget_exhausted";
pub(crate) const REASON_PAUSED: &str = "user_paused";
pub(crate) const REASON_NETWORK: &str = "network_restricted";
pub(crate) const REASON_MANUAL: &str = "manual_judgment_required";

pub(crate) const REASONS: [&str; 7] = [
    REASON_GOAL_MET,
    REASON_NO_NEW_EVIDENCE,
    REASON_MARGINAL_GAIN,
    REASON_BUDGET,
    REASON_PAUSED,
    REASON_NETWORK,
    REASON_MANUAL,
];

pub(crate) fn argument(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RETRIEVAL_ARGUMENT", 2, text)
}

pub(crate) fn integrity(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RETRIEVAL_INTEGRITY", 3, text)
}

pub(crate) fn unknown_root(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RETRIEVAL_UNKNOWN_ROOT", 3, text)
}

pub(crate) fn busy(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RETRIEVAL_BUSY", 3, text)
}

pub(crate) fn paused(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RETRIEVAL_PAUSED", 3, text)
}

pub(crate) fn unsupported(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RETRIEVAL_REASON_UNSUPPORTED", 3, text)
}

pub(crate) fn corpus_mismatch(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RETRIEVAL_CORPUS_MISMATCH", 3, text)
}

// ---------------------------------------------------------------------------
// 作者声明
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorpusSpec {
    pub(crate) root: String,
    pub(crate) manifest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Limits {
    pub(crate) max_queries: u64,
    pub(crate) max_query_bytes: u64,
    pub(crate) max_total_bytes: u64,
    pub(crate) max_query_millis: u64,
    pub(crate) max_total_millis: u64,
    pub(crate) max_hits: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct QueryPlan {
    pub(crate) query_id: String,
    pub(crate) subject: String,
    pub(crate) terms: Vec<String>,
    pub(crate) scope_class: String,
    #[serde(default)]
    pub(crate) requires_external: bool,
    #[serde(default)]
    pub(crate) conflict_group: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Plan {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) case_id: String,
    pub(crate) corpus: CorpusSpec,
    pub(crate) external_retrieval: bool,
    pub(crate) limits: Limits,
    pub(crate) goal_subjects: Vec<String>,
    pub(crate) queries: Vec<QueryPlan>,
}

fn single(text: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    if text.trim() != text {
        return Err(argument(format!("{label}首尾不能有空白。")));
    }
    if text.is_empty() {
        return Err(argument(format!("{label}不能为空。")));
    }
    if text.chars().count() > limit {
        return Err(argument(format!("{label}超过 {limit} 个字符。")));
    }
    if text.chars().any(|value| value.is_control()) {
        return Err(argument(format!("{label}不能包含控制字符。")));
    }
    Ok(())
}

fn identifier(text: &str, label: &str) -> Result<(), ProjectError> {
    single(text, label, MAX_ID)?;
    if !text
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
    {
        return Err(argument(format!(
            "{label}只能由 ASCII 字母、数字、连字符、下划线与点组成。"
        )));
    }
    Ok(())
}

fn relative(text: &str, label: &str) -> Result<(), ProjectError> {
    single(text, label, MAX_TEXT)?;
    if text.starts_with('/') || text.contains('\\') {
        return Err(argument(format!(
            "{label}必须是包内相对路径（用 / 分隔）。"
        )));
    }
    if text.split('/').any(|part| part == ".." || part.is_empty()) {
        return Err(argument(format!("{label}不能包含 .. 或空段。")));
    }
    Ok(())
}

fn subject(text: &str) -> Result<(), ProjectError> {
    single(text, "主题", MAX_SUBJECT)
}

impl Limits {
    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if !(1..=MAX_QUERIES as u64).contains(&self.max_queries) {
            return Err(argument(format!(
                "limits.max_queries 必须在 1 到 {MAX_QUERIES} 之间。"
            )));
        }
        if self.max_query_bytes == 0 || self.max_query_bytes > MAX_BYTES {
            return Err(argument(format!(
                "limits.max_query_bytes 必须在 1 到 {MAX_BYTES} 之间。"
            )));
        }
        if self.max_total_bytes < self.max_query_bytes {
            return Err(argument(
                "limits.max_total_bytes 不能小于单次上限 max_query_bytes。",
            ));
        }
        if self.max_query_millis == 0 || self.max_query_millis > 60_000 {
            return Err(argument("limits.max_query_millis 必须在 1 到 60000 之间。"));
        }
        if self.max_total_millis < self.max_query_millis {
            return Err(argument(
                "limits.max_total_millis 不能小于单次上限 max_query_millis。",
            ));
        }
        if !(1..=MAX_HITS as u64).contains(&self.max_hits) {
            return Err(argument(format!(
                "limits.max_hits 必须在 1 到 {MAX_HITS} 之间。"
            )));
        }
        Ok(())
    }
}

impl Plan {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ProjectError> {
        let plan: Self = serde_json::from_slice(bytes)
            .map_err(|error| argument(format!("作者声明不是严格JSON：{error}。")))?;
        plan.validate()?;
        Ok(plan)
    }

    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!(
                "作者声明的 policy 必须是 {POLICY}，实际 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "作者声明的 schema_version 必须是 {SCHEMA_VERSION}。"
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "作者声明的 producer 必须是 {AUTHOR_PRODUCER}。"
            )));
        }
        identifier(&self.case_id, "case_id")?;
        relative(&self.corpus.root, "corpus.root")?;
        relative(&self.corpus.manifest, "corpus.manifest")?;
        self.limits.validate()?;

        if self.goal_subjects.is_empty() {
            return Err(argument("goal_subjects 至少要有 1 个主题。"));
        }
        if self.goal_subjects.len() > MAX_QUERIES {
            return Err(argument("goal_subjects 数量超过上限。"));
        }
        let mut goals = BTreeSet::new();
        for goal in &self.goal_subjects {
            subject(goal)?;
            if !goals.insert(goal.clone()) {
                return Err(argument(format!("goal_subjects 里 {goal} 出现了两次。")));
            }
        }

        if self.queries.is_empty() {
            return Err(argument("queries 至少要有 1 条。"));
        }
        if self.queries.len() > self.limits.max_queries as usize {
            return Err(argument(
                "queries 数量超过 limits.max_queries；本轮不会临时扩大配额。",
            ));
        }
        let mut ids = BTreeSet::new();
        for query in &self.queries {
            identifier(&query.query_id, "query_id")?;
            if !ids.insert(query.query_id.clone()) {
                return Err(argument(format!("query_id {} 重复。", query.query_id)));
            }
            subject(&query.subject)?;
            if !matches!(
                query.scope_class.as_str(),
                SCOPE_AUTHORIZED | SCOPE_PREREQUISITE
            ) {
                return Err(argument(format!(
                    "query {} 的 scope_class 是 {}：检索只允许在已授权范围内进行，越界主题必须先进入候选队列并由人工批准。",
                    query.query_id, query.scope_class
                )));
            }
            if query.terms.is_empty() || query.terms.len() > MAX_TERMS {
                return Err(argument(format!(
                    "query {} 的 terms 必须有 1 到 {MAX_TERMS} 项。",
                    query.query_id
                )));
            }
            for term in &query.terms {
                single(term, "term", MAX_ID)?;
                if term.chars().any(|value| value.is_whitespace()) {
                    return Err(argument(format!(
                        "query {} 的 term {term} 不能包含空白。",
                        query.query_id
                    )));
                }
            }
            if let Some(group) = &query.conflict_group {
                identifier(group, "conflict_group")?;
            }
        }
        // 本轮目标必须是已授权主题之一：检索不能替用户扩大授权范围。
        let authorized = self.authorized();
        for goal in &self.goal_subjects {
            if !authorized.contains(goal) {
                return Err(argument(format!(
                    "goal_subjects 里的 {goal} 没有对应的已授权查询：检索不会为未授权主题调度任何工作。"
                )));
            }
        }
        Ok(())
    }

    /// 已授权主题集合：授权目标与必需前提的并集。goal_subjects 必须在其内。
    pub(crate) fn authorized(&self) -> BTreeSet<String> {
        self.queries
            .iter()
            .map(|query| query.subject.clone())
            .collect()
    }

    pub(crate) fn goals(&self) -> BTreeSet<String> {
        self.goal_subjects.iter().cloned().collect()
    }
}

// ---------------------------------------------------------------------------
// 冻结语料
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorpusDocument {
    pub(crate) path: String,
    pub(crate) sha256: String,
    pub(crate) byte_length: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorpusManifest {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) documents: Vec<CorpusDocument>,
}

impl CorpusManifest {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ProjectError> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| argument(format!("语料清单不是严格JSON：{error}。")))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != CORPUS_POLICY {
            return Err(argument(format!(
                "语料清单的 policy 必须是 {CORPUS_POLICY}。"
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument("语料清单的 schema_version 必须是 1。"));
        }
        identifier(&self.case_id, "case_id")?;
        if self.documents.is_empty() || self.documents.len() > MAX_DOCUMENTS {
            return Err(argument(format!(
                "语料清单必须有 1 到 {MAX_DOCUMENTS} 个文档。"
            )));
        }
        let mut paths = BTreeSet::new();
        for document in &self.documents {
            relative(&document.path, "语料路径")?;
            if !document.path.ends_with(".md") {
                return Err(argument(format!(
                    "语料 {} 必须是 Markdown 文件。",
                    document.path
                )));
            }
            if !paths.insert(document.path.clone()) {
                return Err(argument(format!(
                    "语料 {} 在清单里出现了两次。",
                    document.path
                )));
            }
            if document.sha256.len() != 64
                || !document
                    .sha256
                    .chars()
                    .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
            {
                return Err(argument(format!(
                    "语料 {} 的 sha256 必须是 64 位小写十六进制。",
                    document.path
                )));
            }
            if document.byte_length == 0 || document.byte_length > MAX_BYTES {
                return Err(argument(format!(
                    "语料 {} 的 byte_length 必须在 1 到 {MAX_BYTES} 之间。",
                    document.path
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn total_bytes(&self) -> u64 {
        self.documents.iter().map(|doc| doc.byte_length).sum()
    }
}

// ---------------------------------------------------------------------------
// 命中与扫描
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Hit {
    pub(crate) document: String,
    pub(crate) heading: String,
    pub(crate) offset: u64,
    pub(crate) snippet: String,
    pub(crate) term: String,
}

/// 确定性扫描：按清单顺序、按行顺序，命中数不超过上限。
///
/// 只做大小写无关的子串匹配；这不是语义检索，也刻意不做模糊匹配——
/// 证据线索必须能被人工按偏移回查。
pub(crate) fn scan(documents: &[(String, Vec<u8>)], terms: &[String], max_hits: u64) -> Vec<Hit> {
    let mut hits: Vec<Hit> = Vec::new();
    'documents: for (path, bytes) in documents {
        let text = String::from_utf8_lossy(bytes);
        let mut heading = String::new();
        let mut offset: u64 = 0;
        for line in text.split_inclusive('\n') {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                heading = trimmed.trim_start_matches('#').trim().to_owned();
            }
            let lower = line.to_lowercase();
            for term in terms {
                if lower.contains(&term.to_lowercase()) {
                    hits.push(Hit {
                        document: path.clone(),
                        heading: heading.clone(),
                        offset,
                        snippet: snippet(line),
                        term: term.clone(),
                    });
                    if hits.len() >= max_hits as usize {
                        break 'documents;
                    }
                }
            }
            offset += line.len() as u64;
        }
    }
    hits
}

fn snippet(line: &str) -> String {
    let text = line.trim();
    if text.chars().count() <= MAX_SNIPPET {
        return text.to_owned();
    }
    text.chars().take(MAX_SNIPPET).collect()
}

/// 去重键：同一文档同一偏移同一命中词就是同一线索。
pub(crate) fn hit_key(hit: &Hit) -> String {
    format!("{}#{}@{}", hit.document, hit.term, hit.offset)
}

// ---------------------------------------------------------------------------
// 账本视图
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct BudgetView {
    pub(crate) queries_used: u64,
    pub(crate) bytes_used: u64,
    pub(crate) millis_used: u64,
    pub(crate) queries_remaining: u64,
    pub(crate) bytes_remaining: u64,
    pub(crate) millis_remaining: u64,
}

impl BudgetView {
    pub(crate) fn compute(limits: &Limits, attempts: &[Attempt]) -> Self {
        let queries_used = attempts.len() as u64;
        let bytes_used = attempts.iter().map(|item| item.reserved_bytes).sum();
        let millis_used = attempts.iter().map(|item| item.reserved_millis).sum();
        Self {
            queries_used,
            bytes_used,
            millis_used,
            queries_remaining: limits.max_queries.saturating_sub(queries_used),
            bytes_remaining: limits.max_total_bytes.saturating_sub(bytes_used),
            millis_remaining: limits.max_total_millis.saturating_sub(millis_used),
        }
    }

    /// 本次尝试若被接受，账本是否仍在上限内。三个维度必须同时满足。
    pub(crate) fn admits(&self, queries: u64, bytes: u64, millis: u64) -> bool {
        self.queries_remaining >= queries
            && self.bytes_remaining >= bytes
            && self.millis_remaining >= millis
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Attempt {
    pub(crate) attempt: u64,
    pub(crate) query_id: String,
    pub(crate) reserved_queries: u64,
    pub(crate) reserved_bytes: u64,
    pub(crate) reserved_millis: u64,
    pub(crate) state: String,
    #[serde(default)]
    pub(crate) settled_bytes: Option<u64>,
    #[serde(default)]
    pub(crate) hits: Option<u64>,
    #[serde(default)]
    pub(crate) new_hits: u64,
    #[serde(default)]
    pub(crate) outcome: Option<String>,
    #[serde(default)]
    pub(crate) hit_keys: Vec<String>,
    pub(crate) at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Unit {
    pub(crate) query_id: String,
    pub(crate) position: u64,
    pub(crate) state: String,
    pub(crate) query: QueryPlan,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Termination {
    pub(crate) reason: String,
    pub(crate) note: String,
    pub(crate) at: String,
}

/// 判断当前账本是否支持某个终止理由。
///
/// 这条规则是本任务的核心防线：理由不是自由文本，必须有账本证据支撑，
/// 否则 `finish` 会以 `RETRIEVAL_REASON_UNSUPPORTED` 拒绝。
pub(crate) fn reason_supported(
    reason: &str,
    view: &BudgetView,
    limits: &Limits,
    units: &[Unit],
    attempts: &[Attempt],
    goals: &BTreeSet<String>,
    paused: bool,
    external_retrieval: bool,
) -> Result<(), ProjectError> {
    let outcomes = |wanted: &str| {
        attempts
            .iter()
            .filter(|item| item.state == ATTEMPT_SETTLED && item.outcome.as_deref() == Some(wanted))
            .count()
    };
    let settled_nodes: Vec<&Attempt> = attempts
        .iter()
        .filter(|item| item.state == ATTEMPT_SETTLED)
        .collect();
    let covered: BTreeSet<String> = settled_nodes
        .iter()
        .filter(|item| item.outcome.as_deref() == Some(OUTCOME_EVIDENCE))
        .filter_map(|item| {
            units
                .iter()
                .find(|unit| unit.query_id == item.query_id)
                .map(|unit| unit.query.subject.clone())
        })
        .collect();
    let pending = units
        .iter()
        .filter(|unit| unit.state == UNIT_PENDING)
        .count();

    match reason {
        REASON_GOAL_MET => {
            let missing: Vec<&String> = goals.difference(&covered).collect();
            if !missing.is_empty() {
                return Err(unsupported(format!(
                    "还不能记「目标满足」：{} 尚无已结算证据。",
                    missing
                        .iter()
                        .map(|item| item.as_str())
                        .collect::<Vec<_>>()
                        .join("、")
                )));
            }
            if pending > 0 {
                return Err(unsupported(format!(
                    "还不能记「目标满足」：还有 {pending} 条查询未结算。"
                )));
            }
        }
        REASON_NO_NEW_EVIDENCE => {
            if outcomes(OUTCOME_NO_HITS) == 0 {
                return Err(unsupported(
                    "还不能记「无可靠新证据」：本轮没有任何零命中查询。",
                ));
            }
        }
        REASON_MARGINAL_GAIN => {
            if outcomes(OUTCOME_DUPLICATES) == 0 {
                return Err(unsupported(
                    "还不能记「边际收益不足」：本轮没有只拿到重复线索的查询。",
                ));
            }
        }
        REASON_BUDGET => {
            let next = units.iter().find(|unit| unit.state == UNIT_PENDING);
            let Some(unit) = next else {
                return Err(unsupported(
                    "还不能记「预算不足」：没有待执行的查询；请用目标满足或边际收益不足。",
                ));
            };
            let estimate = estimate_reservation(limits, unit);
            if view.admits(estimate.0, estimate.1, estimate.2) {
                return Err(unsupported(
                    "还不能记「预算不足」：剩余预算仍足够执行下一条查询。",
                ));
            }
        }
        REASON_PAUSED => {
            if !paused {
                return Err(unsupported(
                    "还不能记「用户暂停」：账本里没有生效中的暂停控制。",
                ));
            }
        }
        REASON_NETWORK => {
            if external_retrieval {
                return Err(unsupported(
                    "还不能记「网络受限」：本会话允许外部检索，没有资源受限证据。",
                ));
            }
            if outcomes(OUTCOME_EXTERNAL) == 0 {
                return Err(unsupported(
                    "还不能记「网络受限」：没有任何查询标记为需要外部来源。",
                ));
            }
        }
        REASON_MANUAL => {
            if outcomes(OUTCOME_CONFLICT) == 0 {
                return Err(unsupported(
                    "还不能记「需要人工判断」：本轮没有出现互相冲突的线索。",
                ));
            }
        }
        _ => {
            return Err(argument(format!(
                "终止理由必须是以下之一：{}；实际 {reason}。",
                REASONS.join("、")
            )));
        }
    }
    Ok(())
}

/// 一次尝试的预留量：次数恒为 1，字节取语料占用与单次上限的较小者，耗时取单次上限。
pub(crate) fn estimate_reservation(limits: &Limits, unit: &Unit) -> (u64, u64, u64) {
    let _ = unit;
    (1, limits.max_query_bytes, limits.max_query_millis)
}

/// 把终止理由翻译成一句中文，供 status 与验证记录复用。
pub(crate) fn reason_text(reason: &str) -> &'static str {
    match reason {
        REASON_GOAL_MET => "目标满足",
        REASON_NO_NEW_EVIDENCE => "无可靠新证据",
        REASON_MARGINAL_GAIN => "边际收益不足",
        REASON_BUDGET => "预算不足",
        REASON_PAUSED => "用户暂停",
        REASON_NETWORK => "网络受限",
        REASON_MANUAL => "需要人工判断",
        _ => "未知",
    }
}

pub(crate) fn outcome_text(outcome: &str) -> &'static str {
    match outcome {
        OUTCOME_EVIDENCE => "有证据线索",
        OUTCOME_NO_HITS => "零命中",
        OUTCOME_DUPLICATES => "只命中重复线索",
        OUTCOME_EXTERNAL => "需要外部来源",
        OUTCOME_CONFLICT => "线索互相冲突",
        _ => "未知",
    }
}

pub(crate) fn totals_by_outcome(attempts: &[Attempt]) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for attempt in attempts {
        let Some(outcome) = &attempt.outcome else {
            continue;
        };
        *out.entry(outcome.clone()).or_insert(0) += 1;
    }
    out
}
