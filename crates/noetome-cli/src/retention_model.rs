//! Retention policy, cleanup planning and published-index protection.
//!
//! The retention declaration says how long each category of stored material may
//! live, which entries are referenced by already published versions, and how old
//! each stored entry is. Planning a cleanup is deliberately a pure function: the
//! same declaration always yields the same plan, and every entry in the plan
//! carries an explicit reason for being deleted or kept.
//!
//! Two rules cannot be relaxed:
//!
//! * a cleanup may only touch categories the declaration lists as cleanable;
//! * an entry that a published version still needs is never reached, not even as
//!   a skip. If the configured retention would expire such an entry the whole
//!   declaration is refused, because a policy that quietly keeps it would report
//!   a cleanup that never fully happens.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::project::ProjectError;

pub const POLICY: &str = "retention-policy-v1";
pub const AUTHOR_PRODUCER: &str = "fixture_author";
pub const SCHEMA_VERSION: u32 = 1;
pub const PLAN_VERSION: u32 = 1;

pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CATEGORIES: usize = 8;
pub const MAX_ENTRIES: usize = 64;
pub const MAX_PUBLISHED: usize = 8;
pub const MAX_REQUIRED: usize = 32;
pub const MAX_ID: usize = 48;
pub const MAX_TEXT: usize = 300;
pub const MAX_LABEL: usize = 120;
pub const MAX_DAYS: u32 = 3650;

pub const REASON_EXPIRED: &str = "超过保留期";
pub const REASON_WITHIN_POLICY: &str = "未超过保留期";
pub const REASON_PROTECTED: &str = "已发布版本仍需引用";

fn argument(message: impl Into<String>) -> ProjectError {
    ProjectError::new("RETENTION_ARGUMENT", 2, message)
}

pub(crate) fn unknown_root(message: impl Into<String>) -> ProjectError {
    ProjectError::new("RETENTION_UNKNOWN_ROOT", 3, message)
}

fn protected(message: impl Into<String>) -> ProjectError {
    ProjectError::new("RETENTION_PROTECTED", 3, message)
}

fn single(value: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    if value.is_empty() || value.chars().count() > limit {
        return Err(argument(format!(
            "{label} 必须是 1..={limit} 个字符，实际是 {} 个。",
            value.chars().count()
        )));
    }
    Ok(())
}

fn identifier(value: &str, label: &str) -> Result<(), ProjectError> {
    single(value, label, MAX_ID)?;
    if !value
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
    {
        return Err(argument(format!(
            "{label}「{value}」只能包含 ASCII 字母数字与 - _ .。"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CategorySpec {
    pub category_id: String,
    pub label: String,
    pub keep_days: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EntrySpec {
    pub entry_id: String,
    pub category_id: String,
    pub bytes: u64,
    pub age_days: u32,
    #[serde(default)]
    pub recovery: Option<String>,
    #[serde(default)]
    pub trace: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedSpec {
    pub release_id: String,
    pub required_index: Vec<String>,
    pub required_traces: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_categories: usize,
    pub max_entries: usize,
    pub max_published: usize,
    pub max_required: usize,
}

impl Limits {
    fn validate(&self) -> Result<(), ProjectError> {
        if self.max_categories == 0 || self.max_categories > MAX_CATEGORIES {
            return Err(argument(format!(
                "limits.max_categories 必须是 1..={MAX_CATEGORIES}，实际是 {}。",
                self.max_categories
            )));
        }
        if self.max_entries == 0 || self.max_entries > MAX_ENTRIES {
            return Err(argument(format!(
                "limits.max_entries 必须是 1..={MAX_ENTRIES}，实际是 {}。",
                self.max_entries
            )));
        }
        if self.max_published == 0 || self.max_published > MAX_PUBLISHED {
            return Err(argument(format!(
                "limits.max_published 必须是 1..={MAX_PUBLISHED}，实际是 {}。",
                self.max_published
            )));
        }
        if self.max_required == 0 || self.max_required > MAX_REQUIRED {
            return Err(argument(format!(
                "limits.max_required 必须是 1..={MAX_REQUIRED}，实际是 {}。",
                self.max_required
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    pub policy: String,
    pub schema_version: u32,
    pub producer: String,
    pub case_id: String,
    pub description: String,
    pub categories: Vec<CategorySpec>,
    pub entries: Vec<EntrySpec>,
    pub published: Vec<PublishedSpec>,
    pub limits: Limits,
}

impl Declaration {
    pub fn parse(text: &str) -> Result<Self, ProjectError> {
        let value: Declaration = serde_json::from_str(text)
            .map_err(|error| argument(format!("保留声明不是合法的 JSON 结构：{error}")))?;
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!(
                "保留声明的 policy 必须是 {POLICY}，实际是 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "保留声明的 schema_version 必须是 {SCHEMA_VERSION}，实际是 {}。",
                self.schema_version
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "保留声明的 producer 必须是 {AUTHOR_PRODUCER}，实际是 {}。",
                self.producer
            )));
        }
        identifier(&self.case_id, "case_id")?;
        single(&self.description, "description", MAX_TEXT)?;
        self.limits.validate()?;

        if self.categories.is_empty() || self.categories.len() > self.limits.max_categories {
            return Err(argument(format!(
                "保留声明必须有 1..={} 个可清理类别，实际是 {} 个。",
                self.limits.max_categories,
                self.categories.len()
            )));
        }
        let mut category_ids: BTreeSet<&str> = BTreeSet::new();
        let mut labels: BTreeSet<&str> = BTreeSet::new();
        for category in &self.categories {
            identifier(&category.category_id, "category_id")?;
            single(&category.label, "category.label", MAX_LABEL)?;
            if !category_ids.insert(category.category_id.as_str()) {
                return Err(argument(format!(
                    "可清理类别编号 {} 重复了。",
                    category.category_id
                )));
            }
            if !labels.insert(category.label.as_str()) {
                return Err(argument(format!(
                    "可清理类别名称「{}」重复了。",
                    category.label
                )));
            }
            if category.keep_days > MAX_DAYS {
                return Err(argument(format!(
                    "类别 {} 的保留期必须 ≤{MAX_DAYS} 天，实际是 {} 天。",
                    category.category_id, category.keep_days
                )));
            }
        }

        if self.entries.is_empty() || self.entries.len() > self.limits.max_entries {
            return Err(argument(format!(
                "保留声明必须有 1..={} 条存储条目，实际是 {} 条。",
                self.limits.max_entries,
                self.entries.len()
            )));
        }
        let mut entry_ids: BTreeSet<&str> = BTreeSet::new();
        for entry in &self.entries {
            identifier(&entry.entry_id, "entry_id")?;
            if !entry_ids.insert(entry.entry_id.as_str()) {
                return Err(argument(format!(
                    "存储条目编号 {} 重复了。",
                    entry.entry_id
                )));
            }
            if !category_ids.contains(entry.category_id.as_str()) {
                return Err(argument(format!(
                    "存储条目 {} 的类别 {} 没有在可清理类别里声明。",
                    entry.entry_id, entry.category_id
                )));
            }
            if entry.age_days > MAX_DAYS {
                return Err(argument(format!(
                    "存储条目 {} 的年龄必须 ≤{MAX_DAYS} 天，实际是 {} 天。",
                    entry.entry_id, entry.age_days
                )));
            }
            if let Some(recovery) = &entry.recovery {
                single(recovery, "entry.recovery", MAX_TEXT)?;
            }
            if let Some(trace) = &entry.trace {
                single(trace, "entry.trace", MAX_TEXT)?;
            }
        }

        if self.published.is_empty() || self.published.len() > self.limits.max_published {
            return Err(argument(format!(
                "保留声明必须有 1..={} 个已发布版本，实际是 {} 个。",
                self.limits.max_published,
                self.published.len()
            )));
        }
        let mut release_ids: BTreeSet<&str> = BTreeSet::new();
        for published in &self.published {
            identifier(&published.release_id, "release_id")?;
            if !release_ids.insert(published.release_id.as_str()) {
                return Err(argument(format!(
                    "已发布版本编号 {} 重复了。",
                    published.release_id
                )));
            }
            if published.required_index.len() > self.limits.max_required
                || published.required_traces.len() > self.limits.max_required
            {
                return Err(argument(format!(
                    "已发布版本 {} 的必需条目最多 {} 个。",
                    published.release_id, self.limits.max_required
                )));
            }
            for required in published
                .required_index
                .iter()
                .chain(published.required_traces.iter())
            {
                identifier(required, "required entry")?;
                if !entry_ids.contains(required.as_str()) {
                    return Err(argument(format!(
                        "已发布版本 {} 声明了必需条目 {required}，但它不在存储条目里。",
                        published.release_id
                    )));
                }
            }
        }
        Ok(())
    }

    fn category(&self, category_id: &str) -> Option<&CategorySpec> {
        self.categories
            .iter()
            .find(|value| value.category_id == category_id)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementRecord {
    pub entry_id: String,
    pub release_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRecord {
    pub entry_id: String,
    pub category_id: String,
    pub bytes: u64,
    pub keep_days: u32,
    pub age_days: u32,
    pub action: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
}

pub const ACTION_DELETE: &str = "delete";
pub const ACTION_KEEP: &str = "keep";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryLoss {
    pub entry_id: String,
    pub recovery: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TraceChange {
    pub trace: String,
    pub before: usize,
    pub after: usize,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub categories: usize,
    pub entries: usize,
    pub published: usize,
    pub deleted: usize,
    pub kept: usize,
    pub protected: usize,
    pub deleted_bytes: usize,
    pub kept_bytes: usize,
    pub lost_recovery: usize,
    pub shortened_traces: usize,
    pub model_calls: usize,
    pub network_requests: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanFile {
    pub policy: String,
    pub plan_version: u32,
    pub case_id: String,
    pub actions: Vec<ActionRecord>,
    pub protected: Vec<RequirementRecord>,
    pub recovery_loss: Vec<RecoveryLoss>,
    pub trace_changes: Vec<TraceChange>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub policy: String,
    pub schema_version: u32,
    pub case_id: String,
    pub description: String,
    pub counts: Counts,
    pub model_calls: usize,
    pub network_requests: usize,
}

#[derive(Debug, Clone)]
pub struct Planned {
    pub plan: PlanFile,
    pub report: Report,
}

fn protected_map(declaration: &Declaration) -> BTreeMap<&str, Vec<RequirementRecord>> {
    let mut map: BTreeMap<&str, Vec<RequirementRecord>> = BTreeMap::new();
    for published in &declaration.published {
        for entry in &published.required_index {
            map.entry(entry.as_str())
                .or_default()
                .push(RequirementRecord {
                    entry_id: entry.clone(),
                    release_id: published.release_id.clone(),
                    reason: "已发布版本必需的索引条目".to_owned(),
                });
        }
        for entry in &published.required_traces {
            map.entry(entry.as_str())
                .or_default()
                .push(RequirementRecord {
                    entry_id: entry.clone(),
                    release_id: published.release_id.clone(),
                    reason: "已发布版本必需的追溯条目".to_owned(),
                });
        }
    }
    map
}

pub(crate) fn plan(declaration: &Declaration) -> Result<Planned, ProjectError> {
    declaration.validate()?;
    let required = protected_map(declaration);

    let mut actions: Vec<ActionRecord> = Vec::with_capacity(declaration.entries.len());
    let mut expired_protected: Vec<String> = Vec::new();

    for entry in &declaration.entries {
        let category = declaration
            .category(&entry.category_id)
            .ok_or_else(|| argument(format!("类别 {} 没有声明。", entry.category_id)))?;
        let expired = entry.age_days > category.keep_days;
        let release = required
            .get(entry.entry_id.as_str())
            .and_then(|items| items.first())
            .map(|item| item.release_id.clone());
        if expired && release.is_some() {
            expired_protected.push(entry.entry_id.clone());
            continue;
        }
        let (action, reason) = if expired {
            (ACTION_DELETE, REASON_EXPIRED)
        } else if release.is_some() {
            (ACTION_KEEP, REASON_PROTECTED)
        } else {
            (ACTION_KEEP, REASON_WITHIN_POLICY)
        };
        let (action, reason) = (action.to_owned(), reason.to_owned());
        actions.push(ActionRecord {
            entry_id: entry.entry_id.clone(),
            category_id: entry.category_id.clone(),
            bytes: entry.bytes,
            keep_days: category.keep_days,
            age_days: entry.age_days,
            action,
            reason,
            release_id: release,
        });
    }

    if !expired_protected.is_empty() {
        expired_protected.sort();
        let detail: Vec<String> = expired_protected
            .iter()
            .map(|entry_id| {
                let releases: Vec<String> = required
                    .get(entry_id.as_str())
                    .map(|items| items.iter().map(|item| item.release_id.clone()).collect())
                    .unwrap_or_default();
                format!("{entry_id}（{}）", releases.join("、"))
            })
            .collect();
        return Err(protected(format!(
            "按当前保留策略，已发布版本必需的条目会过期：{}。清理必须保护已发布版本必需的索引与追溯条目，因此整条拒绝，而不是跳过它们继续清理。请延长这些类别的保留期，或先把已发布版本退役。",
            detail.join("；")
        )));
    }

    actions.sort_by(|left, right| left.entry_id.cmp(&right.entry_id));

    let mut recovery_loss: Vec<RecoveryLoss> = Vec::new();
    for entry in &declaration.entries {
        let deleted = actions
            .iter()
            .any(|action| action.entry_id == entry.entry_id && action.action == ACTION_DELETE);
        if deleted {
            if let Some(recovery) = &entry.recovery {
                recovery_loss.push(RecoveryLoss {
                    entry_id: entry.entry_id.clone(),
                    recovery: recovery.clone(),
                });
            }
        }
    }
    recovery_loss.sort_by(|left, right| left.entry_id.cmp(&right.entry_id));

    let mut trace_changes: Vec<TraceChange> = Vec::new();
    let mut traces: BTreeMap<&str, (usize, Vec<String>)> = BTreeMap::new();
    for entry in &declaration.entries {
        let Some(trace) = entry.trace.as_deref() else {
            continue;
        };
        let slot = traces.entry(trace).or_insert_with(|| (0, Vec::new()));
        slot.0 += 1;
        let deleted = actions
            .iter()
            .any(|action| action.entry_id == entry.entry_id && action.action == ACTION_DELETE);
        if deleted {
            slot.1.push(entry.entry_id.clone());
        }
    }
    for (trace, (before, mut removed)) in traces {
        if removed.is_empty() {
            continue;
        }
        removed.sort();
        trace_changes.push(TraceChange {
            trace: trace.to_owned(),
            before,
            after: before - removed.len(),
            removed,
        });
    }
    trace_changes.sort_by(|left, right| left.trace.cmp(&right.trace));

    let mut protected_records: Vec<RequirementRecord> = Vec::new();
    for entry in &declaration.entries {
        if let Some(items) = required.get(entry.entry_id.as_str()) {
            protected_records.extend(items.iter().cloned());
        }
    }
    protected_records.sort_by(|left, right| {
        left.entry_id
            .cmp(&right.entry_id)
            .then(left.release_id.cmp(&right.release_id))
            .then(left.reason.cmp(&right.reason))
    });

    let kept = actions.iter().filter(|a| a.action == ACTION_KEEP).count();
    let deleted = actions.iter().filter(|a| a.action == ACTION_DELETE).count();
    let deleted_bytes = actions
        .iter()
        .filter(|a| a.action == ACTION_DELETE)
        .map(|a| a.bytes as usize)
        .sum();
    let kept_bytes = actions
        .iter()
        .filter(|a| a.action == ACTION_KEEP)
        .map(|a| a.bytes as usize)
        .sum();
    let protected_count = actions
        .iter()
        .filter(|a| a.reason == REASON_PROTECTED)
        .count();

    let counts = Counts {
        categories: declaration.categories.len(),
        entries: declaration.entries.len(),
        published: declaration.published.len(),
        deleted,
        kept,
        protected: protected_count,
        deleted_bytes,
        kept_bytes,
        lost_recovery: recovery_loss.len(),
        shortened_traces: trace_changes.len(),
        model_calls: 0,
        network_requests: 0,
    };

    let plan = PlanFile {
        policy: POLICY.to_owned(),
        plan_version: PLAN_VERSION,
        case_id: declaration.case_id.clone(),
        actions,
        protected: protected_records,
        recovery_loss,
        trace_changes,
    };
    let report = Report {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: declaration.case_id.clone(),
        description: declaration.description.clone(),
        counts,
        model_calls: 0,
        network_requests: 0,
    };
    Ok(Planned { plan, report })
}

pub(crate) fn render_markdown(planned: &Planned) -> String {
    let counts = &planned.report.counts;
    let mut out = String::new();
    let _ = writeln!(out, "# 保留与清理计划（{}）\n", planned.report.case_id);
    let _ = writeln!(out, "{}\n", planned.report.description);
    let _ = writeln!(out, "## 清理规模\n");
    let _ = writeln!(
        out,
        "- 可清理类别 {} 个、存储条目 {} 条、已发布版本 {} 个",
        counts.categories, counts.entries, counts.published
    );
    let _ = writeln!(
        out,
        "- 删除 {} 条（{} 字节）、保留 {} 条（{} 字节），其中受保护 {} 条",
        counts.deleted, counts.deleted_bytes, counts.kept, counts.kept_bytes, counts.protected
    );
    let _ = writeln!(
        out,
        "- 模型调用与网络请求 {} 次\n",
        counts.model_calls + counts.network_requests
    );
    let _ = writeln!(out, "## 逐条动作\n");
    let _ = writeln!(
        out,
        "| 条目 | 类别 | 字节 | 保留期 | 年龄 | 动作 | 理由 |\n| --- | --- | --- | --- | --- | --- | --- |"
    );
    for action in &planned.plan.actions {
        let verb = if action.action == ACTION_DELETE {
            "删除"
        } else {
            "保留"
        };
        let mut reason = action.reason.clone();
        if let Some(release) = &action.release_id {
            let _ = write!(reason, "（{release}）");
        }
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} 天 | {} 天 | {verb} | {reason} |",
            action.entry_id, action.category_id, action.bytes, action.keep_days, action.age_days
        );
    }
    let _ = writeln!(out, "\n## 影响说明\n");
    if planned.plan.recovery_loss.is_empty() {
        let _ = writeln!(out, "- 没有恢复能力会因此丧失。");
    } else {
        for loss in &planned.plan.recovery_loss {
            let _ = writeln!(
                out,
                "- {} 被删除后，恢复能力「{}」随之丧失。",
                loss.entry_id, loss.recovery
            );
        }
    }
    if planned.plan.trace_changes.is_empty() {
        let _ = writeln!(out, "- 没有追溯链会因此变短。");
    } else {
        for change in &planned.plan.trace_changes {
            let _ = writeln!(
                out,
                "- 追溯链 {} 会从 {} 条变短到 {} 条，删除的是 {}。",
                change.trace,
                change.before,
                change.after,
                change.removed.join("、")
            );
        }
    }
    let _ = writeln!(out, "\n## 受保护条目\n");
    if planned.plan.protected.is_empty() {
        let _ = writeln!(out, "- 本次没有已发布版本必需的条目。");
    } else {
        for item in &planned.plan.protected {
            let _ = writeln!(
                out,
                "- {}（{}）：{}。",
                item.entry_id, item.release_id, item.reason
            );
        }
    }
    let _ = writeln!(out, "\n## 边界\n");
    let _ = writeln!(
        out,
        "- 只生成计划：不删除任何文件、不改写任何已发布版本，清理动作由调用方按计划执行。"
    );
    let _ = writeln!(
        out,
        "- 只作用于声明的可清理类别；已发布版本必需的索引与追溯条目一律受保护，命中保护即整条拒绝。"
    );
    let _ = writeln!(
        out,
        "- 同一冻结输入的两次计划必须字节相同（输出清单不含时间戳）。"
    );
    let _ = writeln!(
        out,
        "- 模型调用与网络请求 0 次，不判断内容质量与科学正确性。"
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration_text() -> String {
        serde_json::json!({
            "policy": POLICY,
            "schema_version": SCHEMA_VERSION,
            "producer": AUTHOR_PRODUCER,
            "case_id": "retention-cleanup-v1",
            "description": "已发布版本之后的保留策略：运行日志与中间缓存可清理，来源索引受保护。",
            "categories": [
                {"category_id": "run-logs", "label": "运行日志", "keep_days": 14},
                {"category_id": "cache", "label": "中间缓存", "keep_days": 30}
            ],
            "entries": [
                {"entry_id": "log-1", "category_id": "run-logs", "bytes": 400, "age_days": 21,
                 "recovery": "按日志重放失败任务", "trace": "trace-parse"},
                {"entry_id": "log-2", "category_id": "run-logs", "bytes": 300, "age_days": 3,
                 "trace": "trace-parse"},
                {"entry_id": "cache-1", "category_id": "cache", "bytes": 9000, "age_days": 45},
                {"entry_id": "index-1", "category_id": "cache", "bytes": 1200, "age_days": 12},
                {"entry_id": "trace-1", "category_id": "cache", "bytes": 700, "age_days": 20,
                 "trace": "trace-parse"}
            ],
            "published": [
                {"release_id": "rel-1", "required_index": ["index-1"], "required_traces": ["trace-1"]}
            ],
            "limits": {
                "max_categories": MAX_CATEGORIES,
                "max_entries": MAX_ENTRIES,
                "max_published": MAX_PUBLISHED,
                "max_required": MAX_REQUIRED
            }
        })
        .to_string()
    }

    fn declaration() -> Declaration {
        Declaration::parse(&declaration_text()).expect("declaration should be valid")
    }

    fn planned() -> Planned {
        plan(&declaration()).expect("plan should be produced")
    }

    fn report_error(declaration: Declaration) -> ProjectError {
        if let Err(error) = declaration.validate() {
            return error;
        }
        match plan(&declaration) {
            Ok(_) => panic!("expected the plan to be refused"),
            Err(error) => error,
        }
    }

    #[test]
    fn the_same_declaration_produces_the_same_plan() {
        let first = planned();
        let second = planned();
        assert_eq!(
            serde_json::to_string(&first.plan).unwrap(),
            serde_json::to_string(&second.plan).unwrap()
        );
        assert_eq!(
            serde_json::to_string(&first.report).unwrap(),
            serde_json::to_string(&second.report).unwrap()
        );
    }

    #[test]
    fn an_expired_entry_is_deleted_and_a_fresh_one_is_kept() {
        let planned = planned();
        let log1 = planned
            .plan
            .actions
            .iter()
            .find(|action| action.entry_id == "log-1")
            .unwrap();
        assert_eq!(log1.action, ACTION_DELETE);
        assert_eq!(log1.reason, REASON_EXPIRED);
        let log2 = planned
            .plan
            .actions
            .iter()
            .find(|action| action.entry_id == "log-2")
            .unwrap();
        assert_eq!(log2.action, ACTION_KEEP);
        assert_eq!(log2.reason, REASON_WITHIN_POLICY);
    }

    #[test]
    fn a_published_index_entry_is_kept_even_when_old() {
        let planned = planned();
        let index = planned
            .plan
            .actions
            .iter()
            .find(|action| action.entry_id == "index-1")
            .unwrap();
        assert_eq!(index.action, ACTION_KEEP);
        assert_eq!(index.reason, REASON_PROTECTED);
        assert_eq!(index.release_id.as_deref(), Some("rel-1"));
        assert_eq!(planned.report.counts.protected, 2);
    }

    #[test]
    fn a_policy_that_would_expire_a_required_entry_is_refused() {
        let mut value: serde_json::Value = serde_json::from_str(&declaration_text()).unwrap();
        value["categories"][1]["keep_days"] = serde_json::json!(10);
        let declaration = Declaration::parse(&value.to_string()).unwrap();
        let error = report_error(declaration);
        assert_eq!(error.code(), "RETENTION_PROTECTED");
        assert_eq!(error.exit_code(), 3);
    }

    #[test]
    fn recovery_loss_is_reported_for_every_deleted_entry_that_had_one() {
        let planned = planned();
        assert_eq!(planned.plan.recovery_loss.len(), 1);
        assert_eq!(planned.plan.recovery_loss[0].entry_id, "log-1");
        assert_eq!(planned.plan.recovery_loss[0].recovery, "按日志重放失败任务");
    }

    #[test]
    fn a_shortened_trace_counts_what_is_left() {
        let planned = planned();
        let change = planned
            .plan
            .trace_changes
            .iter()
            .find(|change| change.trace == "trace-parse")
            .unwrap();
        assert_eq!(change.before, 3);
        assert_eq!(change.after, 2);
        assert_eq!(change.removed, vec!["log-1".to_owned()]);
    }

    #[test]
    fn counts_add_up() {
        let counts = planned().report.counts;
        assert_eq!(counts.deleted + counts.kept, counts.entries);
        assert_eq!(counts.deleted, 2);
        assert_eq!(counts.kept, 3);
        assert_eq!(counts.deleted_bytes, 400 + 9000);
        assert_eq!(counts.kept_bytes, 300 + 1200 + 700);
        assert_eq!(counts.model_calls, 0);
        assert_eq!(counts.network_requests, 0);
    }

    #[test]
    fn an_unknown_category_or_duplicate_entry_is_refused() {
        let mut value: serde_json::Value = serde_json::from_str(&declaration_text()).unwrap();
        value["entries"][0]["category_id"] = serde_json::json!("unknown");
        let error = Declaration::parse(&value.to_string()).unwrap_err();
        assert_eq!(error.code(), "RETENTION_ARGUMENT");

        let mut value: serde_json::Value = serde_json::from_str(&declaration_text()).unwrap();
        value["entries"][1]["entry_id"] = serde_json::json!("log-1");
        let error = Declaration::parse(&value.to_string()).unwrap_err();
        assert_eq!(error.code(), "RETENTION_ARGUMENT");
    }

    #[test]
    fn a_required_entry_that_does_not_exist_is_refused() {
        let mut value: serde_json::Value = serde_json::from_str(&declaration_text()).unwrap();
        value["published"][0]["required_index"] = serde_json::json!(["index-2"]);
        let error = Declaration::parse(&value.to_string()).unwrap_err();
        assert_eq!(error.code(), "RETENTION_ARGUMENT");
    }

    #[test]
    fn the_markdown_names_the_impact_and_the_boundary() {
        let planned = planned();
        let text = render_markdown(&planned);
        assert!(text.contains("## 影响说明"));
        assert!(text.contains("恢复能力「按日志重放失败任务」随之丧失"));
        assert!(text.contains("追溯链 trace-parse 会从 3 条变短到 2 条"));
        assert!(text.contains("## 受保护条目"));
        assert!(text.contains("命中保护即整条拒绝"));
        assert!(text.contains("不删除任何文件"));
    }
}
