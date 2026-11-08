//! Locked positions and the local impact of a planned change.
//!
//! The planner answers three questions and nothing else:
//!
//! 1. Does the proposed structure respect every user lock? A conflict never
//!    overwrites the locked note: it becomes a candidate with the exact lock
//!    that refused it.
//! 2. Does a change stay inside the range it declares? A note in the same topic
//!    that references a touched concept must be listed, and a note that shares
//!    nothing with the change must not be listed.
//! 3. Is a refactor safe to state? Moves, splits and merges must enumerate every
//!    affected reference; when a reference cannot be rewritten the old position
//!    stays and the whole declaration is refused.
//!
//! Everything is a pure function of the declaration; the module never touches
//! the disk, the network or a model.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::project::ProjectError;

pub const POLICY: &str = "locked-change-v1";
pub const AUTHOR_PRODUCER: &str = "fixture_author";
pub const PLAN_VERSION: u32 = 1;
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_NOTES: usize = 16;
pub const MAX_CHANGES: usize = 16;
pub const MAX_LOCKS: usize = 16;
pub const MAX_REFACTORS: usize = 8;
pub const MAX_ID: usize = 48;
pub const MAX_TEXT: usize = 600;
pub const MAX_TITLE: usize = 200;

pub const LOCK_DIRECTORY: &str = "directory";
pub const LOCK_FILE: &str = "file";
pub const LOCK_SECTION: &str = "section";
pub const LOCK_ORDER: &str = "order";
pub const LOCK_CANONICAL: &str = "canonical";

pub const CHANGE_SOURCE_ADDED: &str = "source_added";
pub const CHANGE_SOURCE_UPDATED: &str = "source_updated";
pub const CHANGE_KNOWLEDGE_REVISION: &str = "knowledge_revision";
pub const CHANGE_STYLE: &str = "style_change";
pub const CHANGE_DIRECTORY: &str = "directory_change";
pub const CHANGE_EXTERNAL_EDIT: &str = "external_edit";

pub const REFACTOR_MOVE: &str = "move";
pub const REFACTOR_SPLIT: &str = "split";
pub const REFACTOR_MERGE: &str = "merge";

pub const FINDING_LOCK_CONFLICT: &str = "lock_conflict";
pub const FINDING_MISSED_DEPENDENCY: &str = "missed_dependency";
pub const FINDING_UNEXPECTED_TOUCH: &str = "unexpected_touch";

pub fn argument(message: impl Into<String>) -> ProjectError {
    ProjectError::new("CHANGE_ARGUMENT", 2, message)
}

pub fn integrity(message: impl Into<String>) -> ProjectError {
    ProjectError::new("CHANGE_INTEGRITY", 3, message)
}

pub fn unknown_root(message: impl Into<String>) -> ProjectError {
    ProjectError::new("CHANGE_UNKNOWN_ROOT", 3, message)
}

pub fn unsafe_refactor(message: impl Into<String>) -> ProjectError {
    ProjectError::new("CHANGE_UNSAFE_REFACTOR", 3, message)
}

fn single(value: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    if value.trim().is_empty() {
        return Err(argument(format!("{label}不能为空。")));
    }
    if value.chars().count() > limit {
        return Err(argument(format!("{label}最多 {limit} 个字符。")));
    }
    Ok(())
}

fn identifier(value: &str, label: &str) -> Result<(), ProjectError> {
    single(value, label, MAX_ID)?;
    if !value
        .chars()
        .all(|item| item.is_ascii_alphanumeric() || item == '-' || item == '_')
    {
        return Err(argument(format!(
            "{label}只能包含字母、数字、连字符和下划线：{value}。"
        )));
    }
    Ok(())
}

fn relative(value: &str, label: &str) -> Result<(), ProjectError> {
    single(value, label, MAX_TEXT)?;
    if value.starts_with('/') || value.contains(':') || value.split('/').any(|part| part == "..") {
        return Err(argument(format!(
            "{label}必须是相对路径，且不能包含 ..：{value}。"
        )));
    }
    Ok(())
}

fn under(path: &str, area: &str) -> bool {
    path == area
        || path
            .strip_prefix(area)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LockSpec {
    pub lock_id: String,
    pub kind: String,
    #[serde(default)]
    pub note_id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub concept_id: Option<String>,
    #[serde(default)]
    pub position: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NoteSpec {
    pub note_id: String,
    pub path: String,
    pub title: String,
    pub topic_id: String,
    pub position: u32,
    pub sections: Vec<String>,
    pub concepts: Vec<String>,
    /// Declared content fingerprint of the note as it exists today.
    pub fingerprint: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeSpec {
    pub change_id: String,
    pub kind: String,
    pub topic_id: String,
    pub concepts: Vec<String>,
    pub notes: Vec<String>,
    /// The path the new plan wants; absent means "keep the current path".
    #[serde(default)]
    pub proposed_path: Option<String>,
    /// The section order the new plan wants; absent means "keep the current order".
    #[serde(default)]
    pub proposed_sections: Option<Vec<String>>,
    #[serde(default)]
    pub refactor: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RefactorSpec {
    pub refactor_id: String,
    pub kind: String,
    pub note_id: String,
    /// For a move: the one new path. For a split: every new path. For a merge:
    /// the single surviving path.
    pub targets: Vec<String>,
    /// Every reference that must be rewritten, one entry per affected link.
    pub references: Vec<ReferenceSpec>,
    /// For a split: which atoms go where. Every atom of the note must appear
    /// exactly once, otherwise the split is not a partition.
    #[serde(default)]
    pub atoms: Vec<AtomSpec>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceSpec {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtomSpec {
    pub atom_id: String,
    pub note_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_notes: usize,
    pub max_changes: usize,
    pub max_locks: usize,
    pub max_refactors: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    pub policy: String,
    pub schema_version: u32,
    pub producer: String,
    pub case_id: String,
    pub description: String,
    pub notes: Vec<NoteSpec>,
    pub changes: Vec<ChangeSpec>,
    pub locks: Vec<LockSpec>,
    pub refactors: Vec<RefactorSpec>,
    pub limits: Limits,
}

impl Limits {
    fn validate(&self) -> Result<(), ProjectError> {
        if !(1..=MAX_NOTES).contains(&self.max_notes)
            || !(1..=MAX_CHANGES).contains(&self.max_changes)
            || !(1..=MAX_LOCKS).contains(&self.max_locks)
            || !(0..=MAX_REFACTORS).contains(&self.max_refactors)
        {
            return Err(argument("limits 超出允许范围。"));
        }
        Ok(())
    }
}

impl Declaration {
    pub fn parse(text: &str) -> Result<Self, ProjectError> {
        let declaration: Self = serde_json::from_str(text)
            .map_err(|error| argument(format!("变更声明不是合法 JSON：{error}")))?;
        declaration.validate()?;
        Ok(declaration)
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!("policy 必须是 {POLICY}。")));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "schema_version 必须是 {SCHEMA_VERSION}。"
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!("producer 必须是 {AUTHOR_PRODUCER}。")));
        }
        identifier(&self.case_id, "case_id")?;
        single(&self.description, "description", MAX_TEXT)?;
        self.limits.validate()?;
        if self.notes.is_empty() || self.notes.len() > self.limits.max_notes {
            return Err(argument("笔记数量必须在 limits.max_notes 之内且不为空。"));
        }
        if self.locks.len() > self.limits.max_locks {
            return Err(argument("锁定数量超过 limits.max_locks。"));
        }
        if self.changes.len() > self.limits.max_changes {
            return Err(argument("变更数量超过 limits.max_changes。"));
        }
        if self.refactors.len() > self.limits.max_refactors {
            return Err(argument("重构数量超过 limits.max_refactors。"));
        }
        let mut note_ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for note in &self.notes {
            identifier(&note.note_id, "note_id")?;
            relative(&note.path, "笔记路径")?;
            single(&note.title, "笔记标题", MAX_TITLE)?;
            identifier(&note.topic_id, "topic_id")?;
            if note.position == 0 {
                return Err(argument(format!(
                    "笔记 {} 的 position 必须从 1 开始。",
                    note.note_id
                )));
            }
            if note.sections.is_empty() {
                return Err(argument(format!("笔记 {} 没有任何章节。", note.note_id)));
            }
            let mut seen = BTreeSet::new();
            for section in &note.sections {
                single(section, "章节", MAX_TITLE)?;
                if !seen.insert(section) {
                    return Err(argument(format!(
                        "笔记 {} 的章节 {section} 重复。",
                        note.note_id
                    )));
                }
            }
            if note.fingerprint.len() != 64
                || !note.fingerprint.chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err(argument(format!(
                    "笔记 {} 的 fingerprint 必须是 64 位十六进制。",
                    note.note_id
                )));
            }
            if !note_ids.insert(note.note_id.clone()) {
                return Err(argument(format!("note_id 重复：{}。", note.note_id)));
            }
            if !paths.insert(note.path.clone()) {
                return Err(argument(format!("笔记路径重复：{}。", note.path)));
            }
        }
        let concepts: BTreeSet<&str> = self
            .notes
            .iter()
            .flat_map(|note| note.concepts.iter().map(String::as_str))
            .collect();
        let mut lock_ids = BTreeSet::new();
        for lock in &self.locks {
            identifier(&lock.lock_id, "lock_id")?;
            if !lock_ids.insert(lock.lock_id.clone()) {
                return Err(argument(format!("lock_id 重复：{}。", lock.lock_id)));
            }
            match lock.kind.as_str() {
                LOCK_DIRECTORY => {
                    let path = lock.path.as_deref().ok_or_else(|| {
                        argument(format!("目录锁 {} 必须给出 path。", lock.lock_id))
                    })?;
                    relative(path, "锁定目录")?;
                }
                LOCK_FILE => {
                    let path = lock.path.as_deref().ok_or_else(|| {
                        argument(format!("文件锁 {} 必须给出 path。", lock.lock_id))
                    })?;
                    relative(path, "锁定文件")?;
                }
                LOCK_SECTION => {
                    let note = lock.note_id.as_deref().ok_or_else(|| {
                        argument(format!("章节锁 {} 必须给出 note_id。", lock.lock_id))
                    })?;
                    let section = lock.section.as_deref().ok_or_else(|| {
                        argument(format!("章节锁 {} 必须给出 section。", lock.lock_id))
                    })?;
                    let note = self.note(note).ok_or_else(|| {
                        argument(format!(
                            "章节锁 {} 指向的笔记不存在：{note}。",
                            lock.lock_id
                        ))
                    })?;
                    if !note.sections.iter().any(|value| value == section) {
                        return Err(argument(format!(
                            "章节锁 {} 指向的章节 {section} 在当前笔记里不存在。",
                            lock.lock_id
                        )));
                    }
                }
                LOCK_ORDER => {
                    let note = lock.note_id.as_deref().ok_or_else(|| {
                        argument(format!("顺序锁 {} 必须给出 note_id。", lock.lock_id))
                    })?;
                    if self.note(note).is_none() {
                        return Err(argument(format!(
                            "顺序锁 {} 指向的笔记不存在：{note}。",
                            lock.lock_id
                        )));
                    }
                }
                LOCK_CANONICAL => {
                    let concept = lock.concept_id.as_deref().ok_or_else(|| {
                        argument(format!("规范位置锁 {} 必须给出 concept_id。", lock.lock_id))
                    })?;
                    if !concepts.contains(concept) {
                        return Err(argument(format!(
                            "规范位置锁 {} 指向的概念没有被任何笔记解释：{concept}。",
                            lock.lock_id
                        )));
                    }
                }
                other => {
                    return Err(argument(format!(
                        "未知的锁类型 {other}：只支持 directory、file、section、order、canonical。"
                    )));
                }
            }
        }
        let mut change_ids = BTreeSet::new();
        for change in &self.changes {
            identifier(&change.change_id, "change_id")?;
            if !change_ids.insert(change.change_id.clone()) {
                return Err(argument(format!("change_id 重复：{}。", change.change_id)));
            }
            if !matches!(
                change.kind.as_str(),
                CHANGE_SOURCE_ADDED
                    | CHANGE_SOURCE_UPDATED
                    | CHANGE_KNOWLEDGE_REVISION
                    | CHANGE_STYLE
                    | CHANGE_DIRECTORY
                    | CHANGE_EXTERNAL_EDIT
            ) {
                return Err(argument(format!("未知的变更类型：{}。", change.kind)));
            }
            identifier(&change.topic_id, "变更的 topic_id")?;
            if change.notes.is_empty() {
                return Err(argument(format!(
                    "变更 {} 没有声明任何受影响笔记。",
                    change.change_id
                )));
            }
            for note in &change.notes {
                if self.note(note).is_none() {
                    return Err(argument(format!(
                        "变更 {} 指向不存在的笔记：{note}。",
                        change.change_id
                    )));
                }
            }
            for concept in &change.concepts {
                if !concepts.contains(concept.as_str()) {
                    return Err(argument(format!(
                        "变更 {} 指向的概念没有被任何笔记解释：{concept}。",
                        change.change_id
                    )));
                }
            }
            if let Some(path) = &change.proposed_path {
                relative(path, "提案路径")?;
            }
            if let Some(sections) = &change.proposed_sections {
                if sections.is_empty() {
                    return Err(argument(format!(
                        "变更 {} 的提案章节不能为空。",
                        change.change_id
                    )));
                }
                let mut seen = BTreeSet::new();
                for section in sections {
                    single(section, "提案章节", MAX_TITLE)?;
                    if !seen.insert(section) {
                        return Err(argument(format!(
                            "变更 {} 的提案章节 {section} 重复。",
                            change.change_id
                        )));
                    }
                }
            }
            if let Some(refactor) = &change.refactor {
                if !self
                    .refactors
                    .iter()
                    .any(|item| &item.refactor_id == refactor)
                {
                    return Err(argument(format!(
                        "变更 {} 引用的重构 {refactor} 不存在。",
                        change.change_id
                    )));
                }
            }
        }
        let mut refactor_ids = BTreeSet::new();
        for refactor in &self.refactors {
            identifier(&refactor.refactor_id, "refactor_id")?;
            if !refactor_ids.insert(refactor.refactor_id.clone()) {
                return Err(argument(format!(
                    "refactor_id 重复：{}。",
                    refactor.refactor_id
                )));
            }
            if self.note(&refactor.note_id).is_none() {
                return Err(argument(format!(
                    "重构 {} 指向不存在的笔记：{}。",
                    refactor.refactor_id, refactor.note_id
                )));
            }
            for target in &refactor.targets {
                relative(target, "重构目标路径")?;
            }
            match refactor.kind.as_str() {
                REFACTOR_MOVE => {
                    if refactor.targets.len() != 1 {
                        return Err(argument(format!(
                            "移动 {} 必须恰好给一个目标路径。",
                            refactor.refactor_id
                        )));
                    }
                }
                REFACTOR_SPLIT => {
                    if refactor.targets.len() < 2 {
                        return Err(argument(format!(
                            "拆分 {} 至少要给两个目标路径。",
                            refactor.refactor_id
                        )));
                    }
                }
                REFACTOR_MERGE => {
                    if refactor.targets.len() != 1 {
                        return Err(argument(format!(
                            "合并 {} 必须给一个存活路径。",
                            refactor.refactor_id
                        )));
                    }
                }
                other => {
                    return Err(argument(format!(
                        "未知的重构类型 {other}：只支持 move、split、merge。"
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn note(&self, note_id: &str) -> Option<&NoteSpec> {
        self.notes.iter().find(|note| note.note_id == note_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conflict {
    pub lock_id: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub change_id: String,
    pub note_id: String,
    pub current_path: String,
    pub proposed_path: String,
    pub conflicts: Vec<Conflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Applied {
    pub change_id: String,
    pub note_id: String,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceChange {
    pub refactor_id: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefactorRecord {
    pub refactor_id: String,
    pub kind: String,
    pub note_id: String,
    pub targets: Vec<String>,
    pub references: Vec<ReferenceChange>,
    pub atoms: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub kind: String,
    pub subject: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unchanged {
    pub note_id: String,
    pub path: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub notes: usize,
    pub changes: usize,
    pub locks: usize,
    pub applied: usize,
    pub candidates: usize,
    pub refactors: usize,
    pub reference_changes: usize,
    pub unchanged: usize,
    pub findings: usize,
    pub model_calls: usize,
    pub network_requests: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeFile {
    pub policy: String,
    pub plan_version: u32,
    pub case_id: String,
    pub applied: Vec<Applied>,
    pub candidates: Vec<Candidate>,
    pub refactors: Vec<RefactorRecord>,
    pub unchanged: Vec<Unchanged>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub policy: String,
    pub schema_version: u32,
    pub case_id: String,
    pub description: String,
    pub verdict: String,
    pub counts: Counts,
    pub findings: Vec<Finding>,
    pub model_calls: usize,
    pub network_requests: usize,
}

#[derive(Debug, Clone)]
pub struct Planner {
    pub change_file: ChangeFile,
    pub report: Report,
}

fn lock_conflicts(
    declaration: &Declaration,
    note: &NoteSpec,
    proposed_path: &str,
    proposed_sections: &[String],
    concepts: &[String],
) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    for lock in &declaration.locks {
        match lock.kind.as_str() {
            LOCK_DIRECTORY => {
                let path = lock.path.as_deref().unwrap_or_default();
                if under(&note.path, path) && !under(proposed_path, path) {
                    conflicts.push(Conflict {
                        lock_id: lock.lock_id.clone(),
                        kind: LOCK_DIRECTORY.to_owned(),
                        detail: format!(
                            "目录 {path} 被锁定，笔记 {} 不能移到 {proposed_path}。",
                            note.note_id
                        ),
                    });
                }
            }
            LOCK_FILE => {
                let path = lock.path.as_deref().unwrap_or_default();
                if path == note.path.as_str() && path != proposed_path {
                    conflicts.push(Conflict {
                        lock_id: lock.lock_id.clone(),
                        kind: LOCK_FILE.to_owned(),
                        detail: format!("文件 {path} 被锁定，不能改名或移动。"),
                    });
                }
            }
            LOCK_SECTION => {
                if lock.note_id.as_deref() != Some(note.note_id.as_str()) {
                    continue;
                }
                let section = lock.section.as_deref().unwrap_or_default();
                let before = note.sections.iter().position(|value| value == section);
                let after = proposed_sections.iter().position(|value| value == section);
                if before != after {
                    conflicts.push(Conflict {
                        lock_id: lock.lock_id.clone(),
                        kind: LOCK_SECTION.to_owned(),
                        detail: format!("章节「{section}」被锁定，位置不能改变。"),
                    });
                }
            }
            LOCK_ORDER => {
                if lock.note_id.as_deref() != Some(note.note_id.as_str()) {
                    continue;
                }
                if let Some(position) = lock.position {
                    if position != note.position {
                        conflicts.push(Conflict {
                            lock_id: lock.lock_id.clone(),
                            kind: LOCK_ORDER.to_owned(),
                            detail: format!(
                                "顺序锁定为第 {position} 位，声明里是第 {} 位。",
                                note.position
                            ),
                        });
                    }
                }
            }
            LOCK_CANONICAL => {
                if let Some(concept) = lock.concept_id.as_deref() {
                    if concepts.iter().any(|value| value == concept) && proposed_path != note.path {
                        conflicts.push(Conflict {
                            lock_id: lock.lock_id.clone(),
                            kind: LOCK_CANONICAL.to_owned(),
                            detail: format!(
                                "概念 {concept} 的规范解释位置被锁在 {}，不能移到 {proposed_path}。",
                                note.path
                            ),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    conflicts
}

/// Every note that explains or references a touched concept must be declared,
/// and a declared note must share something with the change.
pub fn locality(declaration: &Declaration) -> Vec<Finding> {
    let mut findings = Vec::new();
    for change in &declaration.changes {
        let touched: BTreeSet<&str> = change.concepts.iter().map(String::as_str).collect();
        let declared: BTreeSet<&str> = change.notes.iter().map(String::as_str).collect();
        for note in &declaration.notes {
            let shares = note
                .concepts
                .iter()
                .any(|concept| touched.contains(concept.as_str()));
            let same_topic = note.topic_id == change.topic_id;
            let should = shares || (same_topic && shares);
            if shares && !declared.contains(note.note_id.as_str()) {
                findings.push(Finding {
                    kind: FINDING_MISSED_DEPENDENCY.to_owned(),
                    subject: format!("{}#{}", change.change_id, note.note_id),
                    detail: format!(
                        "变更 {} 触动了概念，但笔记 {} 也引用了它却没有被声明为受影响。",
                        change.change_id, note.note_id
                    ),
                });
            }
            let unrelated = !shares;
            if declared.contains(note.note_id.as_str()) && unrelated && !same_topic {
                findings.push(Finding {
                    kind: FINDING_UNEXPECTED_TOUCH.to_owned(),
                    subject: format!("{}#{}", change.change_id, note.note_id),
                    detail: format!(
                        "变更 {} 声明影响笔记 {}，但两者既不在同一主题也不共享概念。",
                        change.change_id, note.note_id
                    ),
                });
            }
            let _ = should;
        }
    }
    findings.sort_by(|left, right| {
        (left.kind.as_str(), left.subject.as_str())
            .cmp(&(right.kind.as_str(), right.subject.as_str()))
    });
    findings
}

fn check_refactor(
    declaration: &Declaration,
    refactor: &RefactorSpec,
) -> Result<RefactorRecord, ProjectError> {
    let note = declaration
        .note(&refactor.note_id)
        .ok_or_else(|| argument(format!("重构指向不存在的笔记：{}。", refactor.note_id)))?;
    let mut seen_atoms = BTreeSet::new();
    if refactor.kind == REFACTOR_SPLIT {
        if refactor.atoms.is_empty() {
            return Err(unsafe_refactor(format!(
                "拆分 {} 没有给出原子分配：拆分必须是完整划分，否则会丢知识。",
                refactor.refactor_id
            )));
        }
        for atom in &refactor.atoms {
            if !seen_atoms.insert(atom.atom_id.clone()) {
                return Err(unsafe_refactor(format!(
                    "拆分 {} 里原子 {} 出现了两次：拆分必须是完整划分。",
                    refactor.refactor_id, atom.atom_id
                )));
            }
            if !refactor.targets.contains(&atom.note_id) {
                return Err(unsafe_refactor(format!(
                    "拆分 {} 把原子 {} 分到 {}，但该路径不在目标列表里。",
                    refactor.refactor_id, atom.atom_id, atom.note_id
                )));
            }
        }
        for target in &refactor.targets {
            if !refactor.atoms.iter().any(|atom| &atom.note_id == target) {
                return Err(unsafe_refactor(format!(
                    "拆分 {} 的目标 {target} 没有分到任何原子：空壳笔记不是拆分。",
                    refactor.refactor_id
                )));
            }
        }
    }
    for target in &refactor.targets {
        if target == &note.path {
            continue;
        }
        for lock in &declaration.locks {
            let blocked = match lock.kind.as_str() {
                LOCK_FILE => lock.path.as_deref() == Some(note.path.as_str()),
                LOCK_DIRECTORY => lock
                    .path
                    .as_deref()
                    .is_some_and(|path| under(&note.path, path) && !under(target, path)),
                LOCK_CANONICAL => lock
                    .concept_id
                    .as_deref()
                    .is_some_and(|concept| note.concepts.iter().any(|value| value == concept)),
                _ => false,
            };
            if blocked {
                return Err(unsafe_refactor(format!(
                    "重构 {} 想把 {} 移到 {target}，但锁定 {} 不允许；已保留旧位置，未生成任何变更。",
                    refactor.refactor_id, note.path, lock.lock_id
                )));
            }
        }
    }
    if refactor.references.is_empty() {
        return Err(unsafe_refactor(format!(
            "重构 {} 没有列出引用变化：无法确认引用能同时更新，保留旧位置并停止。",
            refactor.refactor_id
        )));
    }
    let references: Vec<ReferenceChange> = refactor
        .references
        .iter()
        .map(|item| ReferenceChange {
            refactor_id: refactor.refactor_id.clone(),
            from: item.from.clone(),
            to: item.to.clone(),
        })
        .collect();
    Ok(RefactorRecord {
        refactor_id: refactor.refactor_id.clone(),
        kind: refactor.kind.clone(),
        note_id: note.note_id.clone(),
        targets: refactor.targets.clone(),
        references,
        atoms: refactor.atoms.len(),
    })
}

pub fn plan(declaration: &Declaration) -> Result<Planner, ProjectError> {
    let mut applied = Vec::new();
    let mut candidates = Vec::new();
    let mut changed_notes = BTreeSet::new();
    for change in &declaration.changes {
        for note_id in &change.notes {
            let note = declaration
                .note(note_id)
                .ok_or_else(|| argument(format!("变更指向不存在的笔记：{note_id}。")))?;
            if !changed_notes.insert(note.note_id.clone()) {
                continue;
            }
            let proposed_path = change
                .proposed_path
                .clone()
                .unwrap_or_else(|| note.path.clone());
            let proposed_sections = change
                .proposed_sections
                .clone()
                .unwrap_or_else(|| note.sections.clone());
            let conflicts = lock_conflicts(
                declaration,
                note,
                &proposed_path,
                &proposed_sections,
                &note.concepts,
            );
            if conflicts.is_empty() {
                applied.push(Applied {
                    change_id: change.change_id.clone(),
                    note_id: note.note_id.clone(),
                    path: proposed_path,
                    reason: change.kind.clone(),
                });
            } else {
                candidates.push(Candidate {
                    change_id: change.change_id.clone(),
                    note_id: note.note_id.clone(),
                    current_path: note.path.clone(),
                    proposed_path,
                    conflicts,
                });
            }
        }
    }
    for refactor_id in declaration
        .changes
        .iter()
        .filter_map(|change| change.refactor.as_deref())
    {
        changed_notes.insert(refactor_id.to_owned());
    }
    let mut refactors = Vec::new();
    for refactor in &declaration.refactors {
        refactors.push(check_refactor(declaration, refactor)?);
    }
    let refactor_notes: BTreeSet<&str> = declaration
        .refactors
        .iter()
        .map(|item| item.note_id.as_str())
        .collect();
    let unchanged: Vec<Unchanged> = declaration
        .notes
        .iter()
        .filter(|note| !refactor_notes.contains(note.note_id.as_str()))
        .filter(|note| !applied.iter().any(|item| item.note_id == note.note_id))
        .map(|note| Unchanged {
            note_id: note.note_id.clone(),
            path: note.path.clone(),
            fingerprint: note.fingerprint.clone(),
        })
        .collect();
    let mut findings = Vec::new();
    for candidate in &candidates {
        findings.push(Finding {
            kind: FINDING_LOCK_CONFLICT.to_owned(),
            subject: format!("{}#{}", candidate.change_id, candidate.note_id),
            detail: format!(
                "提案被 {} 条锁定拒绝，已转入候选，锁定结构保持原样。",
                candidate.conflicts.len()
            ),
        });
    }
    findings.extend(locality(declaration));
    findings.sort_by(|left, right| {
        (left.kind.as_str(), left.subject.as_str())
            .cmp(&(right.kind.as_str(), right.subject.as_str()))
    });
    let reference_changes: usize = refactors.iter().map(|item| item.references.len()).sum();
    let counts = Counts {
        notes: declaration.notes.len(),
        changes: declaration.changes.len(),
        locks: declaration.locks.len(),
        applied: applied.len(),
        candidates: candidates.len(),
        refactors: refactors.len(),
        reference_changes,
        unchanged: unchanged.len(),
        findings: findings.len(),
        model_calls: 0,
        network_requests: 0,
    };
    let verdict = if findings.is_empty() {
        "pass"
    } else {
        "findings"
    }
    .to_owned();
    let change_file = ChangeFile {
        policy: POLICY.to_owned(),
        plan_version: PLAN_VERSION,
        case_id: declaration.case_id.clone(),
        applied,
        candidates,
        refactors,
        unchanged,
    };
    let report = Report {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: declaration.case_id.clone(),
        description: declaration.description.clone(),
        verdict,
        counts,
        findings,
        model_calls: 0,
        network_requests: 0,
    };
    Ok(Planner {
        change_file,
        report,
    })
}

pub fn render_markdown(planner: &Planner) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# 锁定位置与变更影响（{}）\n\n",
        planner.report.case_id
    ));
    out.push_str("## 变更规模\n\n");
    out.push_str(&format!(
        "- 笔记 {} 份、变更 {} 条、锁定 {} 条\n",
        planner.report.counts.notes, planner.report.counts.changes, planner.report.counts.locks
    ));
    out.push_str(&format!(
        "- 已应用 {} 条、转候选 {} 条、重构 {} 条（引用变化 {} 处）\n",
        planner.report.counts.applied,
        planner.report.counts.candidates,
        planner.report.counts.refactors,
        planner.report.counts.reference_changes
    ));
    out.push_str("\n## 已应用的变更\n\n| 变更 | 笔记 | 位置 | 原因 |\n| --- | --- | --- | --- |\n");
    for item in &planner.change_file.applied {
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            item.change_id, item.note_id, item.path, item.reason
        ));
    }
    out.push_str("\n## 锁定冲突（转候选）\n\n");
    if planner.change_file.candidates.is_empty() {
        out.push_str("- 无\n");
    } else {
        for item in &planner.change_file.candidates {
            out.push_str(&format!(
                "- {}：{} 保持 {}\n",
                item.change_id, item.note_id, item.current_path
            ));
            for conflict in &item.conflicts {
                out.push_str(&format!(
                    "  - {}（{}）：{}\n",
                    conflict.lock_id, conflict.kind, conflict.detail
                ));
            }
        }
    }
    out.push_str("\n## 重构与引用变化\n\n");
    if planner.change_file.refactors.is_empty() {
        out.push_str("- 无\n");
    } else {
        for item in &planner.change_file.refactors {
            out.push_str(&format!(
                "- {}（{}）：{} → {}\n",
                item.refactor_id,
                item.kind,
                item.note_id,
                item.targets.join("、")
            ));
            for reference in &item.references {
                out.push_str(&format!("  - 引用 {} → {}\n", reference.from, reference.to));
            }
        }
    }
    out.push_str("\n## 未受影响的笔记\n\n");
    if planner.change_file.unchanged.is_empty() {
        out.push_str("- 无\n");
    } else {
        for item in &planner.change_file.unchanged {
            out.push_str(&format!(
                "- {}（{}）：指纹 {}\n",
                item.note_id,
                item.path,
                &item.fingerprint[..16]
            ));
        }
    }
    out.push_str("\n## 缺口\n\n");
    if planner.report.findings.is_empty() {
        out.push_str("- 无\n");
    } else {
        for finding in &planner.report.findings {
            out.push_str(&format!(
                "- {}（{}）：{}\n",
                finding.kind, finding.subject, finding.detail
            ));
        }
    }
    out.push_str("\n## 边界\n\n");
    out.push_str("- 只规划位置与影响范围，不写正文、不写 Vault。\n");
    out.push_str("- 锁定冲突只进候选，绝不覆盖已锁定结构。\n");
    out.push_str("- 模型调用与网络请求 0 次，不判断内容质量与科学正确性。\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> Declaration {
        serde_json::from_str(
            r#"{
              "policy": "locked-change-v1",
              "schema_version": 1,
              "producer": "fixture_author",
              "case_id": "chg-test",
              "description": "锁定与局部影响的自测声明",
              "notes": [
                {"note_id": "n-gly", "path": "代谢/糖酵解/01-糖酵解.md", "title": "糖酵解",
                 "topic_id": "t-gly", "position": 1, "sections": ["定位", "反应"],
                 "concepts": ["c-gly"], "fingerprint": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
                {"note_id": "n-reg", "path": "代谢/糖酵解/02-调控.md", "title": "调控",
                 "topic_id": "t-gly", "position": 2, "sections": ["定位"],
                 "concepts": ["c-pfk"], "fingerprint": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}
              ],
              "changes": [
                {"change_id": "chg-1", "kind": "source_added", "topic_id": "t-gly",
                 "concepts": ["c-gly"], "notes": ["n-gly"]}
              ],
              "locks": [
                {"lock_id": "lock-file", "kind": "file", "path": "代谢/糖酵解/01-糖酵解.md"}
              ],
              "refactors": [],
              "limits": {"max_notes": 8, "max_changes": 8, "max_locks": 8, "max_refactors": 4}
            }"#,
        )
        .expect("fixture")
    }

    fn report_error(declaration: &Declaration) -> ProjectError {
        match plan(declaration) {
            Ok(_) => panic!("expected a rejection"),
            Err(error) => error,
        }
    }

    #[test]
    fn a_clean_declaration_keeps_the_unchanged_notes_visible() {
        let plan = plan(&declaration()).expect("plan");
        assert_eq!(plan.report.verdict, "pass");
        assert_eq!(plan.report.counts.applied, 1);
        assert_eq!(plan.change_file.unchanged.len(), 1);
        assert_eq!(plan.change_file.unchanged[0].note_id, "n-reg");
    }

    #[test]
    fn a_declared_reason_marks_the_note_as_applied() {
        let plan = plan(&declaration()).expect("plan");
        assert_eq!(plan.change_file.applied[0].reason, "source_added");
    }

    #[test]
    fn a_lock_conflict_becomes_a_candidate_and_keeps_the_note() {
        let mut declaration = declaration();
        declaration.changes[0].notes = vec!["n-reg".to_owned()];
        declaration.changes[0].concepts = vec!["c-pfk".to_owned()];
        declaration.changes[0].topic_id = "t-gly".to_owned();
        declaration.locks.push(LockSpec {
            lock_id: "lock-order".to_owned(),
            kind: "order".to_owned(),
            note_id: Some("n-reg".to_owned()),
            path: None,
            section: None,
            concept_id: None,
            position: Some(3),
        });
        let plan = plan(&declaration).expect("plan");
        assert_eq!(plan.report.counts.candidates, 1);
        assert_eq!(plan.change_file.candidates[0].note_id, "n-reg");
        assert_eq!(plan.report.verdict, "findings");
    }

    #[test]
    fn a_section_that_moves_is_refused_by_its_lock() {
        let mut declaration = declaration();
        declaration.locks.push(LockSpec {
            lock_id: "lock-section".to_owned(),
            kind: "section".to_owned(),
            note_id: Some("n-gly".to_owned()),
            path: None,
            section: Some("定位".to_owned()),
            concept_id: None,
            position: None,
        });
        let plan = plan(&declaration).expect("plan");
        assert_eq!(plan.report.counts.candidates, 0);
        assert_eq!(plan.report.counts.applied, 1);
    }

    #[test]
    fn a_missed_dependency_is_reported() {
        let mut declaration = declaration();
        declaration.changes[0].concepts = vec!["c-pfk".to_owned()];
        let plan = plan(&declaration).expect("plan");
        let kinds: Vec<&str> = plan
            .report
            .findings
            .iter()
            .map(|item| item.kind.as_str())
            .collect();
        assert!(kinds.contains(&FINDING_MISSED_DEPENDENCY));
    }

    #[test]
    fn an_unrelated_touch_is_reported() {
        let mut declaration = declaration();
        declaration.notes.push(NoteSpec {
            note_id: "n-other".to_owned(),
            path: "代谢/糖异生/01-糖异生.md".to_owned(),
            title: "糖异生".to_owned(),
            topic_id: "t-gng".to_owned(),
            position: 1,
            sections: vec!["定位".to_owned()],
            concepts: vec!["c-gng".to_owned()],
            fingerprint: "c".repeat(64),
        });
        declaration.changes[0].notes = vec!["n-gly".to_owned(), "n-other".to_owned()];
        let plan = plan(&declaration).expect("plan");
        let kinds: Vec<&str> = plan
            .report
            .findings
            .iter()
            .map(|item| item.kind.as_str())
            .collect();
        assert!(kinds.contains(&FINDING_UNEXPECTED_TOUCH));
    }

    #[test]
    fn a_split_without_a_partition_is_refused() {
        let mut declaration = declaration();
        declaration.refactors.push(RefactorSpec {
            refactor_id: "r-split".to_owned(),
            kind: "split".to_owned(),
            note_id: "n-gly".to_owned(),
            targets: vec!["a.md".to_owned(), "b.md".to_owned()],
            references: vec![ReferenceSpec {
                from: "[[x]]".to_owned(),
                to: "[[a]]".to_owned(),
            }],
            atoms: vec![AtomSpec {
                atom_id: "a1".to_owned(),
                note_id: "a.md".to_owned(),
            }],
        });
        let error = report_error(&declaration);
        assert_eq!(error.code(), "CHANGE_UNSAFE_REFACTOR");
    }

    #[test]
    fn a_refactor_without_reference_changes_is_refused() {
        let mut declaration = declaration();
        declaration.refactors.push(RefactorSpec {
            refactor_id: "r-move".to_owned(),
            kind: "move".to_owned(),
            note_id: "n-reg".to_owned(),
            targets: vec!["代谢/糖酵解/03-调控.md".to_owned()],
            references: Vec::new(),
            atoms: Vec::new(),
        });
        let error = report_error(&declaration);
        assert_eq!(error.code(), "CHANGE_UNSAFE_REFACTOR");
    }

    #[test]
    fn a_safe_refactor_lists_every_reference() {
        let mut declaration = declaration();
        declaration.refactors.push(RefactorSpec {
            refactor_id: "r-move".to_owned(),
            kind: "move".to_owned(),
            note_id: "n-reg".to_owned(),
            targets: vec!["代谢/糖酵解/03-调控.md".to_owned()],
            references: vec![ReferenceSpec {
                from: "[[代谢/糖酵解/02-调控#定位]]".to_owned(),
                to: "[[代谢/糖酵解/03-调控#定位]]".to_owned(),
            }],
            atoms: Vec::new(),
        });
        let plan = plan(&declaration).expect("plan");
        assert_eq!(plan.report.counts.refactors, 1);
        assert_eq!(plan.report.counts.reference_changes, 1);
    }

    #[test]
    fn an_unknown_lock_kind_is_refused() {
        let mut declaration = declaration();
        declaration.locks.push(LockSpec {
            lock_id: "lock-x".to_owned(),
            kind: "everything".to_owned(),
            note_id: None,
            path: None,
            section: None,
            concept_id: None,
            position: None,
        });
        let error = match declaration.validate() {
            Ok(()) => panic!("expected a rejection"),
            Err(error) => error,
        };
        assert_eq!(error.code(), "CHANGE_ARGUMENT");
    }
}
