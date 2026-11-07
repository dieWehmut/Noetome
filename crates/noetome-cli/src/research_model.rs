//! B08-T01：研究候选队列、授权边界与拒绝记忆的数据合同。
//!
//! FR-13 的八字段候选（主题、发现原因、与当前目标关系、可能收益、来源线索、预估代价、
//! 依赖、状态）只由显式动词写入；本轮授权目标与必需前提在 `adopt` 时冻结成 `Objective`，
//! 之后任何观测都不能让它增删——远离批准范围的新主题只进候选队列，绝不改写必做集合。
//! 拒绝记录带理由保存；同一主题没有真正新理由的重提被挡回并计数，有新理由才允许
//! 重新进入 `proposed`。这里不联网、不调用模型、不读任何外部检索结果。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::project::ProjectError;

pub(crate) const POLICY: &str = "research-queue-v1";
pub(crate) const PRODUCER: &str = "noetome-research-queue";
pub(crate) const AUTHOR_PRODUCER: &str = "fixture_author";
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_SUBJECT: usize = 120;
pub(crate) const MAX_TEXT: usize = 600;
pub(crate) const MAX_LEADS: usize = 16;
pub(crate) const MAX_DEPENDS: usize = 16;
pub(crate) const MAX_ROUND: usize = 64;

pub(crate) const SCOPE_AUTHORIZED: &str = "authorized_target";
pub(crate) const SCOPE_PREREQUISITE: &str = "required_prerequisite";
pub(crate) const SCOPE_OUT_OF_SCOPE: &str = "out_of_scope";

pub(crate) const STATE_PROPOSED: &str = "proposed";
pub(crate) const STATE_ACCEPTED: &str = "accepted";
pub(crate) const STATE_REJECTED: &str = "rejected";
pub(crate) const STATE_DEFERRED: &str = "deferred";
pub(crate) const STATE_MERGED: &str = "merged";

pub(crate) const ACTION_ACCEPT: &str = "accept";
pub(crate) const ACTION_REJECT: &str = "reject";
pub(crate) const ACTION_DEFER: &str = "defer";
pub(crate) const ACTION_MERGE: &str = "merge";
pub(crate) const ACTION_REVISE_SCOPE: &str = "revise_scope";

pub(crate) const OUTCOME_CREATED: &str = "created";
pub(crate) const OUTCOME_MERGED_EVIDENCE: &str = "merged_evidence";
pub(crate) const OUTCOME_DUPLICATE_EVIDENCE: &str = "duplicate_evidence";
pub(crate) const OUTCOME_BLOCKED_REPEAT: &str = "blocked_repeat";
pub(crate) const OUTCOME_REOPENED: &str = "reopened";
pub(crate) const OUTCOME_ACCEPTED: &str = "accepted";
pub(crate) const OUTCOME_REJECTED: &str = "rejected";
pub(crate) const OUTCOME_DEFERRED: &str = "deferred";
pub(crate) const OUTCOME_MERGED: &str = "merged";
pub(crate) const OUTCOME_SCOPE_REVISED: &str = "scope_revised";

pub(crate) fn argument(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RESEARCH_ARGUMENT", 2, text)
}

pub(crate) fn integrity(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RESEARCH_INTEGRITY", 3, text)
}

pub(crate) fn unknown_root(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RESEARCH_UNKNOWN_ROOT", 3, text)
}

pub(crate) fn busy(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RESEARCH_BUSY", 3, text)
}

// ---------------------------------------------------------------------------
// 输入：作者包
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Goal {
    pub(crate) subject: String,
    pub(crate) goal: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Objective {
    pub(crate) summary: String,
    pub(crate) authorized_targets: Vec<Goal>,
    pub(crate) required_prerequisites: Vec<Goal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Limits {
    pub(crate) max_candidates: usize,
    pub(crate) max_evidence: usize,
    pub(crate) max_leads: usize,
    pub(crate) max_revisions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation {
    pub(crate) subject: String,
    pub(crate) scope_class: String,
    pub(crate) reason: String,
    pub(crate) relation: String,
    pub(crate) benefit: String,
    pub(crate) source_leads: Vec<String>,
    pub(crate) estimated_cost: String,
    pub(crate) depends_on: Vec<String>,
    #[serde(default)]
    pub(crate) revisit_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DecisionInput {
    pub(crate) subject: String,
    pub(crate) action: String,
    #[serde(default)]
    pub(crate) reason: Option<String>,
    #[serde(default)]
    pub(crate) merged_into: Option<String>,
    #[serde(default)]
    pub(crate) scope_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Author {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) case_id: String,
    pub(crate) objective: Objective,
    pub(crate) limits: Limits,
    pub(crate) observations: Vec<Observation>,
    #[serde(default)]
    pub(crate) decisions: Vec<DecisionInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObservationFile {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) round: String,
    pub(crate) observations: Vec<Observation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DecisionFile {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) round: String,
    pub(crate) decisions: Vec<DecisionInput>,
}

// ---------------------------------------------------------------------------
// 持久状态
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Evidence {
    pub(crate) round: String,
    pub(crate) at: String,
    pub(crate) note: String,
    pub(crate) leads: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Rejection {
    pub(crate) at: String,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScopeRevision {
    pub(crate) at: String,
    pub(crate) reason: String,
    pub(crate) before: String,
    pub(crate) after: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Candidate {
    pub(crate) subject: String,
    pub(crate) scope_class: String,
    pub(crate) reason: String,
    pub(crate) relation: String,
    pub(crate) benefit: String,
    pub(crate) source_leads: Vec<String>,
    pub(crate) estimated_cost: String,
    pub(crate) depends_on: Vec<String>,
    pub(crate) state: String,
    pub(crate) scope_note: String,
    pub(crate) evidence: Vec<Evidence>,
    pub(crate) rejections: Vec<Rejection>,
    pub(crate) scope_revisions: Vec<ScopeRevision>,
    pub(crate) merged_into: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Counters {
    pub(crate) observations: u64,
    pub(crate) blocked_repeats: u64,
    pub(crate) reopenings: u64,
    pub(crate) merges: u64,
    pub(crate) decisions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StateRecord {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) objective: Objective,
    pub(crate) limits: Limits,
    pub(crate) candidates: Vec<Candidate>,
    pub(crate) counters: Counters,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Applied {
    pub(crate) subject: String,
    pub(crate) outcome: String,
    pub(crate) detail: String,
}

// ---------------------------------------------------------------------------
// 校验与解析
// ---------------------------------------------------------------------------

/// 主题归一：去首尾空白并把内部空白折叠成单个空格。归并判定只看这个键。
pub(crate) fn subject_key(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn single(field: &str, raw: &str, limit: usize) -> Result<String, ProjectError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(argument(format!("{field} 不能为空。")));
    }
    if trimmed.chars().count() > limit {
        return Err(argument(format!("{field} 超过 {limit} 字符上限。")));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(argument(format!("{field} 含控制字符。")));
    }
    Ok(trimmed.to_owned())
}

fn subject(field: &str, raw: &str) -> Result<String, ProjectError> {
    Ok(single(field, raw, MAX_SUBJECT)?)
}

fn leads(field: &str, raw: &[String], limit: usize) -> Result<Vec<String>, ProjectError> {
    if raw.len() > limit {
        return Err(argument(format!("{field} 最多 {limit} 条。")));
    }
    let mut out = Vec::new();
    for item in raw {
        let text = single(field, item, MAX_TEXT)?;
        if text.contains('\\') || text.starts_with('/') || text.starts_with('-') {
            return Err(argument(format!(
                "{field} 只接受不以 / 或 - 开头、不含反斜杠的相对线索：{text}"
            )));
        }
        if text.split('/').any(|part| part == "..") {
            return Err(argument(format!("{field} 不得包含 .. 段：{text}")));
        }
        out.push(text);
    }
    Ok(out)
}

pub(crate) fn scope_class(raw: &str) -> Result<String, ProjectError> {
    match raw.trim() {
        SCOPE_AUTHORIZED | SCOPE_PREREQUISITE | SCOPE_OUT_OF_SCOPE => Ok(raw.trim().to_owned()),
        other => Err(argument(format!(
            "未知范围类别 {other}；只接受 authorized_target、required_prerequisite、out_of_scope。"
        ))),
    }
}

pub(crate) fn action(raw: &str) -> Result<String, ProjectError> {
    match raw.trim() {
        ACTION_ACCEPT | ACTION_REJECT | ACTION_DEFER | ACTION_MERGE | ACTION_REVISE_SCOPE => {
            Ok(raw.trim().to_owned())
        }
        other => Err(argument(format!(
            "未知决定动作 {other}；只接受 accept、reject、defer、merge、revise_scope。"
        ))),
    }
}

fn round_text(raw: &str) -> Result<String, ProjectError> {
    let text = single("轮次标识", raw, MAX_ROUND)?;
    if !text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(argument(format!(
            "轮次标识只接受 ASCII 字母数字与 - _ .：{text}"
        )));
    }
    Ok(text)
}

impl Objective {
    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        single("目标摘要", &self.summary, MAX_TEXT)?;
        if self.authorized_targets.is_empty() {
            return Err(argument("本轮授权目标不能为空。"));
        }
        if self.authorized_targets.len() + self.required_prerequisites.len() > 64 {
            return Err(argument("授权目标与必需前提合计最多 64 项。"));
        }
        let mut seen = BTreeSet::new();
        for (label, goals) in [
            ("授权目标", &self.authorized_targets),
            ("必需前提", &self.required_prerequisites),
        ] {
            for goal in goals {
                let key = subject_key(&goal.subject);
                if key.is_empty() {
                    return Err(argument(format!("{label}的主题不能为空。")));
                }
                single(&format!("{label}主题"), &goal.subject, MAX_SUBJECT)?;
                single(&format!("{label}目标"), &goal.goal, MAX_TEXT)?;
                if !seen.insert(key) {
                    return Err(argument(format!("{label}主题重复：{}", goal.subject)));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn subjects(&self) -> BTreeSet<String> {
        self.authorized_targets
            .iter()
            .chain(self.required_prerequisites.iter())
            .map(|goal| subject_key(&goal.subject))
            .collect()
    }
}

impl Limits {
    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if self.max_candidates == 0 || self.max_candidates > 512 {
            return Err(argument("limits.max_candidates 必须在 1..=512。"));
        }
        if self.max_evidence == 0 || self.max_evidence > 32 {
            return Err(argument("limits.max_evidence 必须在 1..=32。"));
        }
        if self.max_leads > MAX_LEADS {
            return Err(argument(format!("limits.max_leads 最多 {MAX_LEADS}。")));
        }
        if self.max_revisions == 0 || self.max_revisions > 8 {
            return Err(argument("limits.max_revisions 必须在 1..=8。"));
        }
        Ok(())
    }
}

fn parse<T>(bytes: &[u8], label: &str) -> Result<T, ProjectError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_slice(bytes).map_err(|error| argument(format!("{label}不是严格JSON：{error}")))
}

impl Author {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ProjectError> {
        let author: Self = parse(bytes, "作者包")?;
        author.validate()?;
        Ok(author)
    }

    fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!(
                "作者包 policy 必须是 {POLICY}，实际 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "作者包 schema_version 必须是 {SCHEMA_VERSION}，实际 {}。",
                self.schema_version
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "作者包 producer 必须是 {AUTHOR_PRODUCER}，实际 {}。",
                self.producer
            )));
        }
        round_text(&self.case_id)?;
        self.objective.validate()?;
        self.limits.validate()?;
        // 作者包可以只声明目标、不带任何观测；一旦带观测则按同一套规则校验。
        if !self.observations.is_empty() {
            validate_observations(&self.objective, &self.limits, &self.observations)?;
        }
        for decision in &self.decisions {
            validate_decision(decision)?;
        }
        Ok(())
    }
}

impl ObservationFile {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ProjectError> {
        let file: Self = parse(bytes, "观测文件")?;
        if file.policy != POLICY {
            return Err(argument(format!("观测文件 policy 必须是 {POLICY}。")));
        }
        if file.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "观测文件 schema_version 必须是 {SCHEMA_VERSION}。"
            )));
        }
        round_text(&file.round)?;
        Ok(file)
    }
}

impl DecisionFile {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ProjectError> {
        let file: Self = parse(bytes, "决定文件")?;
        if file.policy != POLICY {
            return Err(argument(format!("决定文件 policy 必须是 {POLICY}。")));
        }
        if file.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "决定文件 schema_version 必须是 {SCHEMA_VERSION}。"
            )));
        }
        round_text(&file.round)?;
        if file.decisions.is_empty() {
            return Err(argument("决定文件不能为空。"));
        }
        for decision in &file.decisions {
            validate_decision(decision)?;
        }
        Ok(file)
    }
}

fn validate_decision(decision: &DecisionInput) -> Result<(), ProjectError> {
    subject("决定主题", &decision.subject)?;
    let action = action(&decision.action)?;
    match action.as_str() {
        ACTION_ACCEPT | ACTION_DEFER => {
            if decision.reason.is_none() {
                return Err(argument(format!(
                    "{action} 必须给出理由（reason），以便之后追溯。"
                )));
            }
        }
        ACTION_REJECT => {
            let reason = decision
                .reason
                .as_deref()
                .ok_or_else(|| argument("reject 必须给出理由（reason）。"))?;
            single("拒绝理由", reason, MAX_TEXT)?;
        }
        ACTION_MERGE => {
            let target = decision
                .merged_into
                .as_deref()
                .ok_or_else(|| argument("merge 必须给出 merged_into 目标主题。"))?;
            subject("合并目标", target)?;
            if subject_key(target) == subject_key(&decision.subject) {
                return Err(argument("merge 不能把候选合并到自身。"));
            }
        }
        ACTION_REVISE_SCOPE => {
            let note = decision
                .scope_note
                .as_deref()
                .ok_or_else(|| argument("revise_scope 必须给出 scope_note。"))?;
            single("范围说明", note, MAX_TEXT)?;
        }
        _ => return Err(argument("未知决定动作。")),
    }
    if let Some(reason) = decision.reason.as_deref() {
        single("决定理由", reason, MAX_TEXT)?;
    }
    Ok(())
}

pub(crate) fn validate_observations(
    objective: &Objective,
    limits: &Limits,
    observations: &[Observation],
) -> Result<(), ProjectError> {
    if observations.is_empty() {
        return Err(argument("观测文件至少要有 1 条观测。"));
    }
    if observations.len() > limits.max_candidates {
        return Err(argument(format!(
            "单文件观测 {} 条，超过 limits.max_candidates {}。",
            observations.len(),
            limits.max_candidates
        )));
    }
    let mut keys = BTreeSet::new();
    let mut subjects = objective.subjects();
    for observation in observations {
        let key = subject_key(&observation.subject);
        if key.is_empty() {
            return Err(argument("观测主题不能为空。"));
        }
        subjects.insert(key.clone());
        if !keys.insert(key) {
            return Err(argument(format!(
                "同一文件内主题重复：{}；请合并成一条观测。",
                observation.subject
            )));
        }
    }
    for observation in observations {
        let key = subject("观测主题", &observation.subject)?;
        scope_class(&observation.scope_class)?;
        single("发现原因", &observation.reason, MAX_TEXT)?;
        single("与当前目标关系", &observation.relation, MAX_TEXT)?;
        single("可能收益", &observation.benefit, MAX_TEXT)?;
        single("预估代价", &observation.estimated_cost, MAX_TEXT)?;
        leads("来源线索", &observation.source_leads, limits.max_leads)?;
        if observation.source_leads.is_empty() {
            return Err(argument(format!(
                "候选 {key} 必须至少给出一条来源线索；无来源的候选不入队。"
            )));
        }
        if observation.depends_on.len() > MAX_DEPENDS {
            return Err(argument(format!(
                "候选 {key} 的依赖最多 {MAX_DEPENDS} 项。"
            )));
        }
        for dependency in &observation.depends_on {
            let dependency_key = subject_key(dependency);
            if dependency_key.is_empty() {
                return Err(argument(format!("候选 {key} 的依赖主题不能为空。")));
            }
            if dependency_key == subject_key(&key) {
                return Err(argument(format!("候选 {key} 不能依赖自身。")));
            }
            if !subjects.contains(&dependency_key) {
                return Err(argument(format!(
                    "候选 {key} 依赖的主题 {dependency} 既不在授权集合中也未在本批观测出现。"
                )));
            }
        }
        if let Some(reason) = observation.revisit_reason.as_deref() {
            single("重提理由", reason, MAX_TEXT)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 状态与变更
// ---------------------------------------------------------------------------

impl Candidate {
    fn new(observation: &Observation, round: &str, at: &str) -> Result<Self, ProjectError> {
        let key = subject("观测主题", &observation.subject)?;
        Ok(Self {
            subject: key,
            scope_class: scope_class(&observation.scope_class)?,
            reason: single("发现原因", &observation.reason, MAX_TEXT)?,
            relation: single("与当前目标关系", &observation.relation, MAX_TEXT)?,
            benefit: single("可能收益", &observation.benefit, MAX_TEXT)?,
            source_leads: leads("来源线索", &observation.source_leads, MAX_LEADS)?,
            estimated_cost: single("预估代价", &observation.estimated_cost, MAX_TEXT)?,
            depends_on: observation
                .depends_on
                .iter()
                .map(|item| subject("依赖主题", item))
                .collect::<Result<Vec<_>, _>>()?,
            state: STATE_PROPOSED.to_owned(),
            scope_note: String::new(),
            evidence: vec![Evidence {
                round: round.to_owned(),
                at: at.to_owned(),
                note: single("发现原因", &observation.reason, MAX_TEXT)?,
                leads: observation.source_leads.clone(),
            }],
            rejections: Vec::new(),
            scope_revisions: Vec::new(),
            merged_into: None,
        })
    }

    fn has_evidence(&self, round: &str, note: &str) -> bool {
        self.evidence
            .iter()
            .any(|item| item.round == round && item.note == note)
    }
}

impl StateRecord {
    pub(crate) fn new(author: &Author, at: &str) -> Result<Self, ProjectError> {
        let mut state = Self {
            policy: POLICY.to_owned(),
            schema_version: SCHEMA_VERSION,
            case_id: author.case_id.clone(),
            objective: author.objective.clone(),
            limits: author.limits.clone(),
            candidates: Vec::new(),
            counters: Counters::default(),
        };
        let round = format!("adopt-{at}");
        for observation in &author.observations {
            let candidate = Candidate::new(observation, &round, at)?;
            state.insert(candidate)?;
        }
        state.counters.observations = author.observations.len() as u64;
        if !author.decisions.is_empty() {
            state.apply_decisions(&author.decisions, at)?;
        }
        state.sort();
        Ok(state)
    }

    fn sort(&mut self) {
        self.candidates
            .sort_by(|a, b| subject_key(&a.subject).cmp(&subject_key(&b.subject)));
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.candidates
            .iter()
            .position(|candidate| subject_key(&candidate.subject) == key)
    }

    fn insert(&mut self, candidate: Candidate) -> Result<(), ProjectError> {
        if self.candidates.len() >= self.limits.max_candidates {
            return Err(argument(format!(
                "候选数已达上限 {}；本轮停止扩大队列，保留现场。",
                self.limits.max_candidates
            )));
        }
        self.candidates.push(candidate);
        self.sort();
        Ok(())
    }

    pub(crate) fn totals(&self) -> BTreeMap<String, u64> {
        let mut counts = BTreeMap::new();
        counts.insert("total".to_owned(), self.candidates.len() as u64);
        for state in [
            STATE_PROPOSED,
            STATE_ACCEPTED,
            STATE_REJECTED,
            STATE_DEFERRED,
            STATE_MERGED,
        ] {
            let value = self
                .candidates
                .iter()
                .filter(|candidate| candidate.state == state)
                .count() as u64;
            counts.insert(state.to_owned(), value);
        }
        for scope in [SCOPE_AUTHORIZED, SCOPE_PREREQUISITE, SCOPE_OUT_OF_SCOPE] {
            let value = self
                .candidates
                .iter()
                .filter(|candidate| candidate.scope_class == scope)
                .count() as u64;
            counts.insert(scope.to_owned(), value);
        }
        counts
    }

    /// 应用一批观测。返回逐条结果；任何校验失败都不写账本。
    pub(crate) fn apply_observations(
        &mut self,
        file: &ObservationFile,
        at: &str,
    ) -> Result<Vec<Applied>, ProjectError> {
        validate_observations(&self.objective, &self.limits, &file.observations)?;
        if file.observations.len() as u64 + self.counters.observations > u64::MAX / 2 {
            return Err(integrity("观测计数溢出，保留现场并拒绝继续。"));
        }
        let mut applied = Vec::new();
        for observation in &file.observations {
            let key = subject_key(&observation.subject);
            match self.position(&key) {
                None => {
                    let candidate = Candidate::new(observation, &file.round, at)?;
                    self.insert(candidate)?;
                    applied.push(Applied {
                        subject: key,
                        outcome: OUTCOME_CREATED.to_owned(),
                        detail: format!("首轮发现，入队；范围类别 {}", observation.scope_class),
                    });
                }
                Some(index) => {
                    let candidate = &self.candidates[index];
                    if candidate.state == STATE_MERGED {
                        return Err(argument(format!(
                            "候选 {key} 已合并到 {}，不再接受新观测；请观测合并目标。",
                            candidate.merged_into.as_deref().unwrap_or("<未知>")
                        )));
                    }
                    if candidate.state == STATE_REJECTED {
                        let note = observation
                            .revisit_reason
                            .as_deref()
                            .map(|value| value.trim().to_owned())
                            .filter(|value| !value.is_empty());
                        let repeats = note.as_deref().is_some_and(|value| {
                            candidate
                                .rejections
                                .iter()
                                .any(|rejection| rejection.reason == value)
                                || candidate.evidence.iter().any(|item| item.note == value)
                        });
                        match note {
                            None => {
                                self.counters.blocked_repeats += 1;
                                applied.push(Applied {
                                    subject: key,
                                    outcome: OUTCOME_BLOCKED_REPEAT.to_owned(),
                                    detail: "该主题已被拒绝且本次未给出新理由；不入队、不改状态，仅计数。"
                                        .to_owned(),
                                });
                            }
                            Some(value) if repeats => {
                                self.counters.blocked_repeats += 1;
                                applied.push(Applied {
                                    subject: key,
                                    outcome: OUTCOME_BLOCKED_REPEAT.to_owned(),
                                    detail: format!("重提理由「{value}」与既有拒绝或重提记录相同；视为无新理由。"),
                                });
                            }
                            Some(value) => {
                                if candidate.evidence.len() >= self.limits.max_evidence {
                                    return Err(argument(format!(
                                        "候选 {key} 的证据已达上限 {}。",
                                        self.limits.max_evidence
                                    )));
                                }
                                let candidate = &mut self.candidates[index];
                                candidate.state = STATE_PROPOSED.to_owned();
                                candidate.evidence.push(Evidence {
                                    round: file.round.clone(),
                                    at: at.to_owned(),
                                    note: value.clone(),
                                    leads: observation.source_leads.clone(),
                                });
                                self.counters.reopenings += 1;
                                applied.push(Applied {
                                    subject: key,
                                    outcome: OUTCOME_REOPENED.to_owned(),
                                    detail: format!(
                                        "带新理由重提：「{value}」；回到 proposed，等待决定。"
                                    ),
                                });
                            }
                        }
                    } else if candidate.has_evidence(&file.round, observation.reason.trim()) {
                        applied.push(Applied {
                            subject: key,
                            outcome: OUTCOME_DUPLICATE_EVIDENCE.to_owned(),
                            detail: "同一轮同一理由已记录，未重复追加。".to_owned(),
                        });
                    } else {
                        if candidate.evidence.len() >= self.limits.max_evidence {
                            return Err(argument(format!(
                                "候选 {key} 的证据已达上限 {}。",
                                self.limits.max_evidence
                            )));
                        }
                        let candidate = &mut self.candidates[index];
                        candidate.evidence.push(Evidence {
                            round: file.round.clone(),
                            at: at.to_owned(),
                            note: single("发现原因", &observation.reason, MAX_TEXT)?,
                            leads: observation.source_leads.clone(),
                        });
                        applied.push(Applied {
                            subject: key,
                            outcome: OUTCOME_MERGED_EVIDENCE.to_owned(),
                            detail: format!("重复发现已归并；证据数 {}", candidate.evidence.len()),
                        });
                    }
                }
            }
        }
        self.counters.observations += file.observations.len() as u64;
        self.sort();
        Ok(applied)
    }

    /// 应用一批用户决定。状态只在这里改变，且每条都留痕。
    pub(crate) fn apply_decisions(
        &mut self,
        decisions: &[DecisionInput],
        at: &str,
    ) -> Result<Vec<Applied>, ProjectError> {
        let mut applied = Vec::new();
        for decision in decisions {
            validate_decision(decision)?;
            let key = subject_key(&decision.subject);
            let index = self.position(&key).ok_or_else(|| {
                argument(format!(
                    "决定主题 {key} 不在候选队列中；决定不能凭空创建候选。"
                ))
            })?;
            let action = action(&decision.action)?;
            let reason = decision
                .reason
                .as_deref()
                .map(|value| single("决定理由", value, MAX_TEXT))
                .transpose()?;
            match action.as_str() {
                ACTION_ACCEPT => {
                    let candidate = &self.candidates[index];
                    if candidate.state == STATE_REJECTED {
                        return Err(argument(format!(
                            "候选 {key} 已被拒绝；接受前必须先给出新理由重提（观察阶段）。"
                        )));
                    }
                    if candidate.state == STATE_MERGED {
                        return Err(argument(format!("候选 {key} 已合并；请接受合并目标。")));
                    }
                    let candidate = &mut self.candidates[index];
                    candidate.state = STATE_ACCEPTED.to_owned();
                    applied.push(Applied {
                        subject: key,
                        outcome: OUTCOME_ACCEPTED.to_owned(),
                        detail: format!(
                            "接受；范围类别 {} 不变，必做集合仍只由 adopt 的授权目标决定。",
                            candidate.scope_class
                        ),
                    });
                }
                ACTION_REJECT => {
                    let candidate = &self.candidates[index];
                    if candidate.state == STATE_MERGED {
                        return Err(argument(format!("候选 {key} 已合并，无需拒绝。")));
                    }
                    let reason = reason.clone().unwrap_or_default();
                    let candidate = &mut self.candidates[index];
                    candidate.state = STATE_REJECTED.to_owned();
                    candidate.rejections.push(Rejection {
                        at: at.to_owned(),
                        reason,
                    });
                    applied.push(Applied {
                        subject: key,
                        outcome: OUTCOME_REJECTED.to_owned(),
                        detail: "拒绝并记住理由；之后无新理由的重提会被挡回。".to_owned(),
                    });
                }
                ACTION_DEFER => {
                    let candidate = &self.candidates[index];
                    if candidate.state == STATE_MERGED {
                        return Err(argument(format!("候选 {key} 已合并，无需延后。")));
                    }
                    let candidate = &mut self.candidates[index];
                    candidate.state = STATE_DEFERRED.to_owned();
                    applied.push(Applied {
                        subject: key,
                        outcome: OUTCOME_DEFERRED.to_owned(),
                        detail: format!("延后；理由 {}", reason.unwrap_or_default()),
                    });
                }
                ACTION_MERGE => {
                    let target_key =
                        subject_key(decision.merged_into.as_deref().unwrap_or_default());
                    let target_index = self.position(&target_key).ok_or_else(|| {
                        argument(format!("合并目标 {target_key} 不在候选队列中。"))
                    })?;
                    if target_index == index {
                        return Err(argument("merge 不能把候选合并到自身。"));
                    }
                    let source_state = self.candidates[index].state.clone();
                    let target_state = self.candidates[target_index].state.clone();
                    if source_state == STATE_MERGED {
                        return Err(argument(format!("候选 {key} 已合并过。")));
                    }
                    if target_state == STATE_MERGED || target_state == STATE_REJECTED {
                        return Err(argument(format!(
                            "合并目标 {target_key} 处于 {target_state}，不能接收合并。"
                        )));
                    }
                    let source_evidence = self.candidates[index].evidence.clone();
                    let target = &mut self.candidates[target_index];
                    if target.evidence.len() + source_evidence.len() > self.limits.max_evidence {
                        return Err(argument(format!(
                            "合并后 {target_key} 的证据会超过上限 {}。",
                            self.limits.max_evidence
                        )));
                    }
                    let mut added = 0u64;
                    for evidence in source_evidence {
                        if !target
                            .evidence
                            .iter()
                            .any(|item| item.round == evidence.round && item.note == evidence.note)
                        {
                            target.evidence.push(evidence);
                            added += 1;
                        }
                    }
                    let candidate = &mut self.candidates[index];
                    candidate.state = STATE_MERGED.to_owned();
                    candidate.merged_into = Some(target_key.clone());
                    self.counters.merges += 1;
                    applied.push(Applied {
                        subject: key,
                        outcome: OUTCOME_MERGED.to_owned(),
                        detail: format!(
                            "并入 {target_key}；转移证据 {added} 条，来源候选标记 merged。"
                        ),
                    });
                }
                ACTION_REVISE_SCOPE => {
                    let note = single(
                        "范围说明",
                        decision.scope_note.as_deref().unwrap_or_default(),
                        MAX_TEXT,
                    )?;
                    let candidate = &self.candidates[index];
                    if candidate.state == STATE_MERGED {
                        return Err(argument(format!("候选 {key} 已合并，不能再改范围。")));
                    }
                    if candidate.scope_note == note {
                        return Err(argument(format!(
                            "候选 {key} 的范围说明没有变化；不做空修订。"
                        )));
                    }
                    if candidate.scope_revisions.len() >= self.limits.max_revisions {
                        return Err(argument(format!(
                            "候选 {key} 的范围修订已达上限 {}；请升级为人工判断。",
                            self.limits.max_revisions
                        )));
                    }
                    let before = candidate.scope_note.clone();
                    let candidate = &mut self.candidates[index];
                    candidate.scope_note = note.clone();
                    candidate.scope_revisions.push(ScopeRevision {
                        at: at.to_owned(),
                        reason: reason.unwrap_or_default(),
                        before,
                        after: note,
                    });
                    applied.push(Applied {
                        subject: key,
                        outcome: OUTCOME_SCOPE_REVISED.to_owned(),
                        detail: "范围说明已修订；范围类别与必做集合均未改变。".to_owned(),
                    });
                }
                _ => return Err(argument("未知决定动作。")),
            }
            self.counters.decisions += 1;
        }
        self.sort();
        Ok(applied)
    }

    /// 必做集合只由 adopt 冻结的目标决定；这里重新计算供审计比对。
    pub(crate) fn must_do(&self) -> BTreeSet<String> {
        self.objective.subjects()
    }

    pub(crate) fn queue_only(&self) -> BTreeSet<String> {
        self.candidates
            .iter()
            .filter(|candidate| candidate.scope_class == SCOPE_OUT_OF_SCOPE)
            .filter(|candidate| candidate.state != STATE_MERGED)
            .map(|candidate| subject_key(&candidate.subject))
            .collect()
    }
}
