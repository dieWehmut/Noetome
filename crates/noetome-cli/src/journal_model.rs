//! Deterministic audit event journal: four-layer failure localization.
//!
//! The journal answers one question: *given a failure, which task, which source
//! range, which knowledge objects and which affected files does it touch?* The
//! answer has to be a query result, not a human grep over unstructured lines.
//!
//! Two invariants shape everything here:
//!
//! 1. **Only append.** Events never get rewritten or reordered. The stream is a
//!    list ordered by (task, at, event) so that two runs over the same frozen
//!    declaration produce byte-identical output.
//! 2. **No credentials.** A declaration carrying anything that looks like a
//!    secret is refused as a whole. Filtering secrets out and then continuing
//!    would quietly produce a journal that looks complete while missing the very
//!    context needed to explain an incident.

use crate::project::ProjectError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const POLICY: &str = "audit-journal-v1";
pub(crate) const AUTHOR_PRODUCER: &str = "fixture_author";
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const JOURNAL_VERSION: u32 = 1;

pub(crate) const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_TASKS: usize = 16;
pub(crate) const MAX_EVENTS: usize = 256;
pub(crate) const MAX_SOURCES: usize = 16;
pub(crate) const MAX_OBJECTS: usize = 32;
pub(crate) const MAX_FILES: usize = 64;
pub(crate) const MAX_ID: usize = 48;
pub(crate) const MAX_TEXT: usize = 300;
pub(crate) const MAX_MESSAGE: usize = 300;
pub(crate) const MAX_CLASS: usize = 48;

pub(crate) const SEVERITY_INFO: &str = "info";
pub(crate) const SEVERITY_ERROR: &str = "error";

/// Event kinds kept narrow on purpose: a journal whose kinds multiply without
/// bound stops being locatable, because nobody remembers what `stage_5b` meant.
pub(crate) const EVENT_STARTED: &str = "started";
pub(crate) const EVENT_FAILED: &str = "failed";
pub(crate) const EVENT_RETRIED: &str = "retried";
pub(crate) const EVENT_RESOLVED: &str = "resolved";
pub(crate) const EVENT_KINDS: [&str; 4] =
    [EVENT_STARTED, EVENT_FAILED, EVENT_RETRIED, EVENT_RESOLVED];

/// Strings that must never reach a journal. Kept deliberately blunt: the cost of
/// a false positive is one refused declaration, the cost of a false negative is a
/// credential written into an artifact the user is told to share with support.
pub(crate) const SECRET_MARKERS: [&str; 8] = [
    "api_key",
    "apikey",
    "password",
    "secret",
    "bearer ",
    "authorization:",
    "token=",
    "-----begin",
];

pub(crate) fn argument<M: Into<String>>(message: M) -> ProjectError {
    ProjectError::new("JOURNAL_ARGUMENT", 2, message)
}

pub(crate) fn integrity<M: Into<String>>(message: M) -> ProjectError {
    ProjectError::new("JOURNAL_INTEGRITY", 3, message)
}

pub(crate) fn unknown_root<M: Into<String>>(message: M) -> ProjectError {
    ProjectError::new("JOURNAL_UNKNOWN_ROOT", 3, message)
}

pub(crate) fn secret_marker<M: Into<String>>(message: M) -> ProjectError {
    ProjectError::new("JOURNAL_SECRET_MARKER", 3, message)
}

fn single(value: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    if value.trim().is_empty() {
        return Err(argument(format!("{label}不能为空。")));
    }
    if value.chars().count() > limit {
        return Err(argument(format!(
            "{label}超过 {limit} 个字符：{}。",
            value.chars().count()
        )));
    }
    if value.contains('\n') || value.contains('\r') {
        return Err(argument(format!("{label}不能包含换行：{value}。")));
    }
    Ok(())
}

fn identifier(value: &str, label: &str) -> Result<(), ProjectError> {
    single(value, label, MAX_ID)?;
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(argument(format!(
            "{label}只能用 ASCII 字母、数字、连字符、下划线或点：{value}。"
        )));
    }
    Ok(())
}

fn relative(value: &str, label: &str) -> Result<(), ProjectError> {
    single(value, label, MAX_TEXT)?;
    if value.starts_with('/') || value.contains(':') || value.contains('\\') {
        return Err(argument(format!(
            "{label}必须是相对路径且不含盘符或反斜杠：{value}。"
        )));
    }
    for piece in value.split('/') {
        if piece.is_empty() || piece == "." || piece == ".." {
            return Err(argument(format!(
                "{label}不能包含空段、`.` 或 `..`：{value}。"
            )));
        }
    }
    Ok(())
}

/// Scan every declared string for credential-shaped substrings.
///
/// Returns the offending locations so the refusal can name them; a bare
/// "invalid" would leave the author guessing which field tripped the check.
pub(crate) fn secret_hits<'a>(values: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<String> {
    let mut hits = BTreeSet::new();
    for (label, value) in values {
        let lowered = value.to_ascii_lowercase();
        for marker in SECRET_MARKERS {
            if lowered.contains(marker) {
                hits.insert(format!("{label} 里出现疑似凭据字样「{marker}」"));
            }
        }
    }
    hits.into_iter().collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceSpec {
    pub source_id: String,
    pub locator: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ObjectSpec {
    pub object_id: String,
    pub source_id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileSpec {
    pub file_id: String,
    pub object_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskSpec {
    pub task_id: String,
    pub title: String,
    pub sources: Vec<String>,
    pub objects: Vec<String>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventSpec {
    pub event_id: String,
    pub task_id: String,
    /// Monotonic sequence inside the declaration; the journal never reorders.
    pub at: u32,
    pub kind: String,
    pub class: String,
    pub action: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_tasks: usize,
    pub max_events: usize,
    pub max_sources: usize,
    pub max_objects: usize,
    pub max_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    pub policy: String,
    pub schema_version: u32,
    pub producer: String,
    pub case_id: String,
    pub description: String,
    pub sources: Vec<SourceSpec>,
    pub objects: Vec<ObjectSpec>,
    pub files: Vec<FileSpec>,
    pub tasks: Vec<TaskSpec>,
    pub events: Vec<EventSpec>,
    pub limits: Limits,
}

#[derive(Debug, Clone, Copy)]
struct Bounds {
    tasks: usize,
    events: usize,
    sources: usize,
    objects: usize,
    files: usize,
}

impl Limits {
    fn bounds(&self) -> Result<Bounds, ProjectError> {
        let check =
            |value: usize, low: usize, high: usize, label: &str| -> Result<usize, ProjectError> {
                if value < low || value > high {
                    return Err(argument(format!(
                        "{label}必须在 {low} 到 {high} 之间：收到 {value}。"
                    )));
                }
                Ok(value)
            };
        Ok(Bounds {
            tasks: check(self.max_tasks, 1, MAX_TASKS, "任务上限")?,
            events: check(self.max_events, 1, MAX_EVENTS, "事件上限")?,
            sources: check(self.max_sources, 1, MAX_SOURCES, "来源上限")?,
            objects: check(self.max_objects, 1, MAX_OBJECTS, "知识对象上限")?,
            files: check(self.max_files, 1, MAX_FILES, "文件上限")?,
        })
    }
}

impl Declaration {
    pub(crate) fn parse(text: &str) -> Result<Self, ProjectError> {
        let declaration: Self = serde_json::from_str(text)
            .map_err(|error| argument(format!("审计声明不是合法 JSON：{error}。")))?;
        declaration.validate()?;
        Ok(declaration)
    }

    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!(
                "审计声明的 policy 必须是 {POLICY}：收到 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "审计声明的 schema_version 必须是 {SCHEMA_VERSION}：收到 {}。",
                self.schema_version
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "审计声明的 producer 必须是 {AUTHOR_PRODUCER}：收到 {}。",
                self.producer
            )));
        }
        identifier(&self.case_id, "case_id")?;
        single(&self.description, "description", MAX_TEXT)?;
        let bounds = self.limits.bounds()?;

        if self.sources.is_empty() || self.sources.len() > bounds.sources {
            return Err(argument(format!(
                "来源数量必须在 1 到 {} 之间：收到 {}。",
                bounds.sources,
                self.sources.len()
            )));
        }
        let mut source_ids = BTreeSet::new();
        for source in &self.sources {
            identifier(&source.source_id, "source_id")?;
            relative(&source.locator, "locator")?;
            single(&source.description, "来源描述", MAX_TEXT)?;
            if !source_ids.insert(source.source_id.as_str()) {
                return Err(argument(format!("来源编号重复：{}。", source.source_id)));
            }
        }

        if self.objects.len() > bounds.objects {
            return Err(argument(format!(
                "知识对象数量不能超过 {}：收到 {}。",
                bounds.objects,
                self.objects.len()
            )));
        }
        let mut object_ids = BTreeSet::new();
        for object in &self.objects {
            identifier(&object.object_id, "object_id")?;
            single(&object.label, "对象标签", MAX_TEXT)?;
            if !source_ids.contains(object.source_id.as_str()) {
                return Err(argument(format!(
                    "知识对象 {} 指向的来源 {} 不在本次声明里。",
                    object.object_id, object.source_id
                )));
            }
            if !object_ids.insert(object.object_id.as_str()) {
                return Err(argument(format!(
                    "知识对象编号重复：{}。",
                    object.object_id
                )));
            }
        }

        if self.files.len() > bounds.files {
            return Err(argument(format!(
                "受影响文件数量不能超过 {}：收到 {}。",
                bounds.files,
                self.files.len()
            )));
        }
        let mut file_ids = BTreeSet::new();
        for file in &self.files {
            identifier(&file.file_id, "file_id")?;
            relative(&file.path, "文件路径")?;
            if !object_ids.contains(file.object_id.as_str()) {
                return Err(argument(format!(
                    "受影响文件 {} 指向的对象 {} 不在本次声明里。",
                    file.file_id, file.object_id
                )));
            }
            if !file_ids.insert(file.file_id.as_str()) {
                return Err(argument(format!("文件编号重复：{}。", file.file_id)));
            }
        }

        if self.tasks.is_empty() || self.tasks.len() > bounds.tasks {
            return Err(argument(format!(
                "任务数量必须在 1 到 {} 之间：收到 {}。",
                bounds.tasks,
                self.tasks.len()
            )));
        }
        let mut task_ids = BTreeSet::new();
        for task in &self.tasks {
            identifier(&task.task_id, "task_id")?;
            single(&task.title, "任务标题", MAX_TEXT)?;
            if task.sources.is_empty() {
                return Err(argument(format!(
                    "任务 {} 没有声明任何来源范围：定位不到来源的失败无法复核。",
                    task.task_id
                )));
            }
            if task.objects.is_empty() {
                return Err(argument(format!(
                    "任务 {} 没有声明任何知识对象：定位不到对象的失败无法复核。",
                    task.task_id
                )));
            }
            for id in &task.sources {
                if !source_ids.contains(id.as_str()) {
                    return Err(argument(format!(
                        "任务 {} 引用的来源 {} 不在本次声明里。",
                        task.task_id, id
                    )));
                }
            }
            for id in &task.objects {
                if !object_ids.contains(id.as_str()) {
                    return Err(argument(format!(
                        "任务 {} 引用的知识对象 {} 不在本次声明里。",
                        task.task_id, id
                    )));
                }
            }
            let mut seen = BTreeSet::new();
            for id in &task.sources {
                if !seen.insert(id.as_str()) {
                    return Err(argument(format!(
                        "任务 {} 的来源列表里有重复项：{}。",
                        task.task_id, id
                    )));
                }
            }
            seen.clear();
            for id in &task.objects {
                if !seen.insert(id.as_str()) {
                    return Err(argument(format!(
                        "任务 {} 的对象列表里有重复项：{}。",
                        task.task_id, id
                    )));
                }
            }
            // Affected files are derived from objects, so a declared file that
            // belongs to no listed object would never be localized.
            for id in &task.files {
                if !file_ids.contains(id.as_str()) {
                    return Err(argument(format!(
                        "任务 {} 引用的文件 {} 不在本次声明里。",
                        task.task_id, id
                    )));
                }
                let file = self
                    .files
                    .iter()
                    .find(|file| file.file_id == *id)
                    .expect("file_ids was built from self.files");
                if !task.objects.contains(&file.object_id) {
                    return Err(argument(format!(
                        "任务 {} 声明了文件 {}，但该文件所属的对象 {} 不在这个任务的对象列表里。",
                        task.task_id, id, file.object_id
                    )));
                }
            }
            if !task_ids.insert(task.task_id.as_str()) {
                return Err(argument(format!("任务编号重复：{}。", task.task_id)));
            }
        }

        if self.events.len() > bounds.events {
            return Err(argument(format!(
                "事件数量不能超过 {}：收到 {}。",
                bounds.events,
                self.events.len()
            )));
        }
        let mut event_ids = BTreeSet::new();
        let mut sequences: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
        for event in &self.events {
            identifier(&event.event_id, "event_id")?;
            if !task_ids.contains(event.task_id.as_str()) {
                return Err(argument(format!(
                    "事件 {} 指向的任务 {} 不在本次声明里。",
                    event.event_id, event.task_id
                )));
            }
            if event.at == 0 {
                return Err(argument(format!(
                    "事件 {} 的 at 必须从 1 开始：0 无法表示先后。",
                    event.event_id
                )));
            }
            if !EVENT_KINDS.contains(&event.kind.as_str()) {
                return Err(argument(format!(
                    "事件 {} 的 kind 必须是 {} 之一：收到 {}。",
                    event.event_id,
                    EVENT_KINDS.join("、"),
                    event.kind
                )));
            }
            single(&event.class, "错误分类", MAX_CLASS)?;
            single(&event.action, "处理动作", MAX_TEXT)?;
            single(&event.message, "事件说明", MAX_MESSAGE)?;
            if !event_ids.insert(event.event_id.as_str()) {
                return Err(argument(format!("事件编号重复：{}。", event.event_id)));
            }
            sequences
                .entry(event.task_id.as_str())
                .or_default()
                .push(event.at);
        }
        for (task_id, mut points) in sequences {
            points.sort_unstable();
            for pair in points.windows(2) {
                if pair[0] == pair[1] {
                    return Err(argument(format!(
                        "任务 {task_id} 有两个事件都用 at = {}：先后无法判定。",
                        pair[0]
                    )));
                }
            }
        }

        let mut values: Vec<(&str, &str)> = Vec::new();
        values.push(("case_id", self.case_id.as_str()));
        values.push(("description", self.description.as_str()));
        for source in &self.sources {
            values.push(("来源描述", source.description.as_str()));
            values.push(("来源定位", source.locator.as_str()));
        }
        for object in &self.objects {
            values.push(("对象标签", object.label.as_str()));
        }
        for file in &self.files {
            values.push(("文件路径", file.path.as_str()));
        }
        for task in &self.tasks {
            values.push(("任务标题", task.title.as_str()));
        }
        for event in &self.events {
            values.push(("错误分类", event.class.as_str()));
            values.push(("处理动作", event.action.as_str()));
            values.push(("事件说明", event.message.as_str()));
        }
        let hits = secret_hits(values);
        if !hits.is_empty() {
            return Err(secret_marker(format!(
                "审计声明里出现疑似凭据：{}。日志与导出会跟着被分享出去，凭据不能进声明；请改成引用凭据名而不是凭据值。",
                hits.join("；")
            )));
        }

        Ok(())
    }

    fn task(&self, id: &str) -> Option<&TaskSpec> {
        self.tasks.iter().find(|task| task.task_id == id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EventRecord {
    pub event_id: String,
    pub task_id: String,
    pub at: u32,
    pub kind: String,
    pub severity: String,
    pub class: String,
    pub action: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskRecord {
    pub task_id: String,
    pub title: String,
    pub sources: Vec<String>,
    pub objects: Vec<String>,
    pub files: Vec<String>,
    pub events: usize,
    pub failures: usize,
    /// Last event kind seen for this task, so a reader can tell whether it ended
    /// resolved without replaying the stream.
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FailureRecord {
    pub event_id: String,
    pub task_id: String,
    pub task_title: String,
    pub at: u32,
    pub class: String,
    pub action: String,
    pub message: String,
    pub sources: Vec<String>,
    pub objects: Vec<String>,
    pub files: Vec<String>,
    /// True when a later event on the same task retried or resolved this class.
    pub followed_by_action: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub tasks: usize,
    pub events: usize,
    pub started: usize,
    pub failed: usize,
    pub retried: usize,
    pub resolved: usize,
    pub sources: usize,
    pub objects: usize,
    pub files: usize,
    pub located_failures: usize,
    pub unresolved_failures: usize,
    pub model_calls: usize,
    pub network_requests: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JournalFile {
    pub policy: String,
    pub journal_version: u32,
    pub case_id: String,
    pub tasks: Vec<TaskRecord>,
    pub events: Vec<EventRecord>,
    pub failures: Vec<FailureRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

pub(crate) struct Journal {
    pub journal: JournalFile,
    pub report: Report,
}

fn severity_of(kind: &str) -> &'static str {
    if kind == EVENT_FAILED {
        SEVERITY_ERROR
    } else {
        SEVERITY_INFO
    }
}

/// Build the journal. Pure: same declaration in, same bytes out.
pub(crate) fn journal(declaration: &Declaration) -> Result<Journal, ProjectError> {
    // Order is fixed by (task order in the declaration, at, event_id) so that a
    // reshuffled declaration cannot silently change the journal.
    let mut order: BTreeMap<(&str, u32, &str), &EventSpec> = BTreeMap::new();
    for event in &declaration.events {
        order.insert(
            (event.task_id.as_str(), event.at, event.event_id.as_str()),
            event,
        );
    }
    let events: Vec<EventRecord> = order
        .into_values()
        .map(|event| EventRecord {
            event_id: event.event_id.clone(),
            task_id: event.task_id.clone(),
            at: event.at,
            kind: event.kind.clone(),
            severity: severity_of(&event.kind).to_owned(),
            class: event.class.clone(),
            action: event.action.clone(),
            message: event.message.clone(),
        })
        .collect();

    let mut tasks = Vec::new();
    let mut failures = Vec::new();
    // Both lists are ordered by task id, exactly like the event list. Iterating the
    // declaration order would make the task table follow the author's layout while
    // the event list followed the ids — the same journal, described two ways.
    let mut ordered: Vec<&crate::journal_model::TaskSpec> = declaration.tasks.iter().collect();
    ordered.sort_by(|left, right| left.task_id.cmp(&right.task_id));
    for task in ordered {
        let mine: Vec<&EventRecord> = events
            .iter()
            .filter(|event| event.task_id == task.task_id)
            .collect();
        let failed = mine
            .iter()
            .filter(|event| event.kind == EVENT_FAILED)
            .count();
        let outcome = mine
            .last()
            .map(|event| event.kind.clone())
            .unwrap_or_else(|| "none".to_owned());
        // A failure counts as followed-up when the task has a retried or resolved
        // event after it. Without this flag the reader cannot tell "we retried"
        // from "we gave up".
        let mut pending: Vec<&EventRecord> = Vec::new();
        let mut followed: BTreeSet<&str> = BTreeSet::new();
        for event in &mine {
            match event.kind.as_str() {
                EVENT_FAILED => pending.push(event),
                EVENT_RETRIED | EVENT_RESOLVED => {
                    for failure in pending.drain(..) {
                        followed.insert(failure.event_id.as_str());
                    }
                }
                _ => {}
            }
        }
        for failure in &mine {
            if failure.kind != EVENT_FAILED {
                continue;
            }
            failures.push(FailureRecord {
                event_id: failure.event_id.clone(),
                task_id: failure.task_id.clone(),
                task_title: task.title.clone(),
                at: failure.at,
                class: failure.class.clone(),
                action: failure.action.clone(),
                message: failure.message.clone(),
                sources: task.sources.clone(),
                objects: task.objects.clone(),
                files: task.files.clone(),
                followed_by_action: followed.contains(failure.event_id.as_str()),
            });
        }
        tasks.push(TaskRecord {
            task_id: task.task_id.clone(),
            title: task.title.clone(),
            sources: task.sources.clone(),
            objects: task.objects.clone(),
            files: task.files.clone(),
            events: mine.len(),
            failures: failed,
            outcome,
        });
    }

    let mut values: Vec<(&str, &str)> = Vec::new();
    for task in &declaration.tasks {
        values.push(("任务标题", task.title.as_str()));
    }

    let counts = Counts {
        tasks: tasks.len(),
        events: events.len(),
        started: events
            .iter()
            .filter(|event| event.kind == EVENT_STARTED)
            .count(),
        failed: events
            .iter()
            .filter(|event| event.kind == EVENT_FAILED)
            .count(),
        retried: events
            .iter()
            .filter(|event| event.kind == EVENT_RETRIED)
            .count(),
        resolved: events
            .iter()
            .filter(|event| event.kind == EVENT_RESOLVED)
            .count(),
        sources: declaration.sources.len(),
        objects: declaration.objects.len(),
        files: declaration.files.len(),
        located_failures: failures.len(),
        unresolved_failures: failures
            .iter()
            .filter(|record| !record.followed_by_action)
            .count(),
        model_calls: 0,
        network_requests: 0,
    };

    let journal = JournalFile {
        policy: POLICY.to_owned(),
        journal_version: JOURNAL_VERSION,
        case_id: declaration.case_id.clone(),
        tasks,
        events,
        failures,
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
    Ok(Journal { journal, report })
}

pub(crate) fn render_markdown(value: &Journal) -> String {
    let mut out = String::new();
    out.push_str(&format!("# 审计事件日志（{}）\n\n", value.journal.case_id));
    out.push_str(&format!("{}\n\n", value.report.description));
    out.push_str("## 事件规模\n\n");
    out.push_str(&format!(
        "- 任务 {} 个、事件 {} 条：started {} / failed {} / retried {} / resolved {}。\n",
        value.report.counts.tasks,
        value.report.counts.events,
        value.report.counts.started,
        value.report.counts.failed,
        value.report.counts.retried,
        value.report.counts.resolved
    ));
    out.push_str(&format!(
        "- 来源 {} 个、知识对象 {} 个、受影响文件 {} 个。\n\n",
        value.report.counts.sources, value.report.counts.objects, value.report.counts.files
    ));

    out.push_str("## 失败定位\n\n");
    if value.journal.failures.is_empty() {
        out.push_str("- 本次声明里没有失败事件。\n\n");
    } else {
        out.push_str("| 事件 | 任务 | 分类 | 来源范围 | 知识对象 | 受影响文件 | 后续动作 |\n");
        out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
        for failure in &value.journal.failures {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                failure.event_id,
                failure.task_id,
                failure.class,
                failure.sources.join("、"),
                failure.objects.join("、"),
                failure.files.join("、"),
                if failure.followed_by_action {
                    "已重试或已解决"
                } else {
                    "未处理"
                }
            ));
        }
        out.push('\n');
    }

    out.push_str("## 任务状态\n\n");
    out.push_str("| 任务 | 标题 | 事件 | 失败 | 当前结果 |\n| --- | --- | --- | --- | --- |\n");
    for task in &value.journal.tasks {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            task.task_id, task.title, task.events, task.failures, task.outcome
        ));
    }
    out.push('\n');

    out.push_str("## 边界\n\n");
    out.push_str("- 事件只追加，不重写、不重排；同一冻结输入两次运行字节相同。\n");
    out.push_str("- 声明里出现疑似凭据即整包拒绝：先过滤再继续会得到一份看起来完整、实际缺关键上下文的日志。\n");
    out.push_str("- 模型调用与网络请求 0 次；本日志只做定位，不判断内容质量与科学正确性。\n");
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
            "case_id": "journal-metabolism-v1",
            "description": "糖代谢解析失败的定位样例。",
            "sources": [
                {"source_id": "src-a", "locator": "第三章#糖酵解", "description": "教材A 第三章"},
                {"source_id": "src-b", "locator": "第五章#三羧酸循环", "description": "教材B 第五章"}
            ],
            "objects": [
                {"object_id": "obj-net", "source_id": "src-a", "label": "糖酵解净能量收支"},
                {"object_id": "obj-tca", "source_id": "src-b", "label": "三羧酸循环"}
            ],
            "files": [
                {"file_id": "file-gly", "object_id": "obj-net", "path": "代谢/糖酵解.md"},
                {"file_id": "file-tca", "object_id": "obj-tca", "path": "代谢/三羧酸循环.md"}
            ],
            "tasks": [
                {
                    "task_id": "task-parse",
                    "title": "解析糖代谢来源",
                    "sources": ["src-a"],
                    "objects": ["obj-net"],
                    "files": ["file-gly"]
                },
                {
                    "task_id": "task-compile",
                    "title": "编译三羧酸循环",
                    "sources": ["src-b"],
                    "objects": ["obj-tca"],
                    "files": ["file-tca"]
                }
            ],
            "events": [
                {"event_id": "e1", "task_id": "task-parse", "at": 1, "kind": "started", "class": "none", "action": "读取来源", "message": "开始解析。"},
                {"event_id": "e2", "task_id": "task-parse", "at": 2, "kind": "failed", "class": "PDF_PARSE", "action": "停止解析", "message": "第 12 页没有文本层。"},
                {"event_id": "e3", "task_id": "task-parse", "at": 3, "kind": "retried", "class": "PDF_PARSE", "action": "换解析器", "message": "改用备用解析器重试。"},
                {"event_id": "e4", "task_id": "task-parse", "at": 4, "kind": "resolved", "class": "none", "action": "记录结果", "message": "解析完成。"},
                {"event_id": "e5", "task_id": "task-compile", "at": 1, "kind": "started", "class": "none", "action": "编译", "message": "开始编译。"},
                {"event_id": "e6", "task_id": "task-compile", "at": 2, "kind": "failed", "class": "COMPILE_LINK", "action": "保留旧版本", "message": "链接目标不存在。"}
            ],
            "limits": {"max_tasks": 8, "max_events": 64, "max_sources": 8, "max_objects": 16, "max_files": 16}
        })
        .to_string()
    }

    fn declaration() -> Declaration {
        Declaration::parse(&declaration_text()).expect("fixture declaration parses")
    }

    #[test]
    fn the_same_declaration_produces_the_same_journal() {
        let declaration = declaration();
        let first = journal(&declaration).unwrap();
        let second = journal(&declaration).unwrap();
        assert_eq!(
            serde_json::to_string(&first.journal).unwrap(),
            serde_json::to_string(&second.journal).unwrap()
        );
        assert_eq!(
            serde_json::to_string(&first.report).unwrap(),
            serde_json::to_string(&second.report).unwrap()
        );
    }

    #[test]
    fn a_failure_localizes_all_four_layers() {
        let journal = journal(&declaration()).unwrap();
        // Failures are listed by task id, so pick the one under test by event id
        // instead of by index: the point is the four layers, not the table order.
        let failure = journal
            .journal
            .failures
            .iter()
            .find(|failure| failure.event_id == "e2")
            .expect("e2 is a failure");
        assert_eq!(failure.task_id, "task-parse");
        assert_eq!(failure.task_title, "解析糖代谢来源");
        assert_eq!(failure.class, "PDF_PARSE");
        assert_eq!(failure.sources, vec!["src-a".to_owned()]);
        assert_eq!(failure.objects, vec!["obj-net".to_owned()]);
        assert_eq!(failure.files, vec!["file-gly".to_owned()]);
    }

    #[test]
    fn a_later_retry_marks_the_failure_as_followed_up() {
        let journal = journal(&declaration()).unwrap();
        // task-compile sorts before task-parse, so e6 (no follow-up) comes first.
        assert_eq!(journal.journal.failures[0].event_id, "e6");
        assert!(!journal.journal.failures[0].followed_by_action);
        assert_eq!(journal.journal.failures[1].event_id, "e2");
        assert!(journal.journal.failures[1].followed_by_action);
        assert_eq!(journal.report.counts.located_failures, 2);
        assert_eq!(journal.report.counts.unresolved_failures, 1);
    }

    #[test]
    fn events_are_ordered_by_task_then_sequence_not_declaration_order() {
        let mut declaration = declaration();
        declaration.events.reverse();
        let first = super::journal(&declaration).unwrap();
        let order: Vec<&str> = first
            .journal
            .events
            .iter()
            .map(|event| event.event_id.as_str())
            .collect();
        // Ordering is by task id first, then by the declared sequence: `task-compile`
        // sorts before `task-parse`, so its events lead the journal regardless of how
        // the declaration was laid out.
        assert_eq!(order, vec!["e5", "e6", "e1", "e2", "e3", "e4"]);
        declaration.events.reverse();
        let again = super::journal(&declaration).unwrap();
        let repeat: Vec<&str> = again
            .journal
            .events
            .iter()
            .map(|event| event.event_id.as_str())
            .collect();
        assert_eq!(repeat, order);
    }

    #[test]
    fn a_credential_shaped_string_refuses_the_whole_declaration() {
        let mut declaration = declaration();
        declaration.sources[0].description = "教材A 第三章 password=hunter2".to_owned();
        let error = declaration.validate().unwrap_err();
        assert_eq!(error.code(), "JOURNAL_SECRET_MARKER");
        assert_eq!(error.exit_code(), 3);
    }

    #[test]
    fn a_task_without_sources_is_refused() {
        let mut declaration = declaration();
        declaration.tasks[0].sources.clear();
        let error = declaration.validate().unwrap_err();
        assert_eq!(error.code(), "JOURNAL_ARGUMENT");
    }

    #[test]
    fn duplicate_event_sequences_are_refused() {
        let mut declaration = declaration();
        declaration.events[2].at = 2;
        let error = declaration.validate().unwrap_err();
        assert_eq!(error.code(), "JOURNAL_ARGUMENT");
    }

    #[test]
    fn a_file_must_belong_to_an_object_the_task_lists() {
        let mut declaration = declaration();
        declaration.tasks[0].files = vec!["file-tca".to_owned()];
        let error = declaration.validate().unwrap_err();
        assert_eq!(error.code(), "JOURNAL_ARGUMENT");
    }

    #[test]
    fn severity_follows_the_event_kind() {
        let journal = journal(&declaration()).unwrap();
        for event in &journal.journal.events {
            let expected = if event.kind == EVENT_FAILED {
                "error"
            } else {
                "info"
            };
            assert_eq!(event.severity, expected);
        }
    }

    #[test]
    fn the_markdown_names_the_four_layers_and_the_boundary() {
        let journal = journal(&declaration()).unwrap();
        let markdown = render_markdown(&journal);
        for needle in [
            "## 事件规模",
            "## 失败定位",
            "来源范围",
            "知识对象",
            "受影响文件",
            "## 边界",
            "疑似凭据",
        ] {
            assert!(markdown.contains(needle), "missing {needle}");
        }
    }
}
