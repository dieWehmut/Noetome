//! Deterministic audience-level relevance filtering with an auditable record.
//!
//! This module is a pure planner: it reads frozen normalized documents, their
//! registered span lists and the author package's declared audience, atom set
//! and exclusion register, and derives a keep/exclude decision, a two-density
//! compilation of the surviving atom set and a report. It never calls a model,
//! never touches the network, never rewrites a source and never deletes a span
//! from the frozen record: exclusions live beside the source, not instead of it.
//!
//! Three invariants are load bearing:
//!
//! * A necessary prerequisite may never be dropped silently. When a kept atom
//!   needs knowledge that neither the package nor the declared reader supplies,
//!   the whole adopt is refused with `RELEVANCE_PREREQUISITE_MISSING`.
//! * An atom the declared level requires may never be excluded. Exclusion of
//!   such a span is refused with `RELEVANCE_REQUIRED_EXCLUDED` instead of being
//!   reported as a mild note.
//! * The minimal and the detailed compilation always carry the same atoms and
//!   the same conditions; only `expansion` differs. That is what makes AC-09
//!   checkable instead of a promise.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::outline_model::{DocumentRecord, DocumentView};
use crate::project::ProjectError;
use crate::text::SpanKind;

pub(crate) const POLICY: &str = "relevance-filter-v1";
pub(crate) const AUDIENCE_POLICY: &str = "relevance-audience-v1";
pub(crate) const REVIEW_POLICY: &str = "relevance-review-v1";
pub(crate) const ATOMS_POLICY: &str = "relevance-atoms-v1";
pub(crate) const AUTHOR_PRODUCER: &str = "fixture_author";
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_SOURCES: usize = 8;
pub(crate) const MAX_ATOMS: usize = 64;
pub(crate) const MAX_CONDITIONS: usize = 8;
pub(crate) const MAX_REQUIREMENTS: usize = 8;
pub(crate) const MAX_REASONS: usize = 16;
pub(crate) const MAX_EXCLUSIONS: usize = 64;
pub(crate) const MAX_ID: usize = 48;
pub(crate) const MAX_TEXT: usize = 600;
pub(crate) const MAX_STATEMENT: usize = 400;

/// Declared reader levels, cheapest first. A level higher in this list is a
/// superset: an atom open to `introductory` is also required at `advanced`.
pub(crate) const LEVELS: [&str; 3] = ["introductory", "university", "advanced"];

pub(crate) const DENSITY_MINIMAL: &str = "minimal";
pub(crate) const DENSITY_DETAILED: &str = "detailed";

pub(crate) const STATUS_KEPT: &str = "kept";
pub(crate) const STATUS_EXCLUDED: &str = "excluded";
pub(crate) const STATUS_DEFERRED: &str = "deferred";

pub(crate) fn argument(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RELEVANCE_ARGUMENT", 2, text)
}

pub(crate) fn integrity(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RELEVANCE_INTEGRITY", 3, text)
}

pub(crate) fn unknown_root(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RELEVANCE_UNKNOWN_ROOT", 3, text)
}

pub(crate) fn prerequisite_missing(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RELEVANCE_PREREQUISITE_MISSING", 3, text)
}

pub(crate) fn required_excluded(text: impl Into<String>) -> ProjectError {
    ProjectError::new("RELEVANCE_REQUIRED_EXCLUDED", 3, text)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceSpec {
    pub(crate) source_id: String,
    pub(crate) path: String,
    pub(crate) title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Limits {
    pub(crate) max_sources: usize,
    pub(crate) max_atoms: usize,
    pub(crate) max_conditions: usize,
    pub(crate) max_requirements: usize,
}

/// What the reader already has. This is the only thing that can satisfy a
/// prerequisite the package itself does not establish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Audience {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) level: String,
    pub(crate) declared_knowledge: Vec<String>,
    pub(crate) purpose: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReasonSpec {
    pub(crate) reason: String,
    pub(crate) note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExclusionSpec {
    pub(crate) source_id: String,
    pub(crate) line: usize,
    pub(crate) reason: String,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Review {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) reasons: Vec<ReasonSpec>,
    pub(crate) exclusions: Vec<ExclusionSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AtomSpec {
    pub(crate) atom_id: String,
    pub(crate) source_id: String,
    pub(crate) line: usize,
    pub(crate) kind: String,
    pub(crate) statement: String,
    pub(crate) conditions: Vec<String>,
    pub(crate) requires: Vec<String>,
    pub(crate) covers: Vec<String>,
    pub(crate) levels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Atoms {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) atoms: Vec<AtomSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Author {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) audience: String,
    pub(crate) atoms: String,
    pub(crate) review: String,
    pub(crate) sources: Vec<SourceSpec>,
    pub(crate) limits: Limits,
}

fn single(text: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    if text.trim() != text {
        return Err(argument(format!("{label}不能有首尾空白。")));
    }
    if text.is_empty() {
        return Err(argument(format!("{label}不能为空。")));
    }
    if text.chars().count() > limit {
        return Err(argument(format!(
            "{label}超过 {limit} 个字符；请缩短后重试。"
        )));
    }
    if text.chars().any(char::is_control) {
        return Err(argument(format!("{label}不能包含控制字符。")));
    }
    Ok(())
}

fn identifier(text: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    single(text, label, limit)?;
    if !text
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.')
    {
        return Err(argument(format!(
            "{label}只能包含ASCII字母、数字、连字符、下划线和点。"
        )));
    }
    Ok(())
}

fn relative(text: &str, label: &str) -> Result<(), ProjectError> {
    single(text, label, MAX_TEXT)?;
    if text.starts_with('/') {
        return Err(argument(format!("{label}必须是相对路径。")));
    }
    if text.contains('\\') {
        return Err(argument(format!("{label}不能包含反斜杠。")));
    }
    if text
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(argument(format!(
            "{label}不能包含空的、`.` 或 `..` 路径段。"
        )));
    }
    Ok(())
}

fn identifiers(values: &[String], label: &str, limit: usize) -> Result<(), ProjectError> {
    if values.len() > limit {
        return Err(argument(format!("{label}最多 {limit} 项；请合并重复项。")));
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for value in values {
        identifier(value, label, MAX_ID)?;
        if !seen.insert(value.as_str()) {
            return Err(argument(format!("{label}里出现重复项 {value}。")));
        }
    }
    Ok(())
}

/// Conditions are prose shown to the reader (for example "镁离子存在"), not
/// identifiers. They still have to be short, single-line and free of control
/// characters so the compiled density text stays a single readable clause.
fn condition_list(values: &[String], label: &str, limit: usize) -> Result<(), ProjectError> {
    if values.len() > limit {
        return Err(argument(format!("{label}最多 {limit} 项；请合并重复项。")));
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for value in values {
        single(value, label, MAX_TEXT)?;
        if !seen.insert(value.as_str()) {
            return Err(argument(format!("{label}里出现重复项 {value}。")));
        }
    }
    Ok(())
}

fn level_index(level: &str) -> Result<usize, ProjectError> {
    LEVELS
        .iter()
        .position(|candidate| *candidate == level)
        .ok_or_else(|| {
            argument(format!(
                "用户水平 {level} 不在允许集合内（{}）。",
                LEVELS.join("、")
            ))
        })
}

impl Audience {
    pub(crate) fn parse(text: &str) -> Result<Self, ProjectError> {
        let value: Self = serde_json::from_str(text)
            .map_err(|error| argument(format!("用户水平声明无法解析：{error}")))?;
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != AUDIENCE_POLICY {
            return Err(argument(format!(
                "用户水平声明的 policy 必须是 {AUDIENCE_POLICY}，实际是 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "用户水平声明的 schema_version 必须是 {SCHEMA_VERSION}。"
            )));
        }
        identifier(&self.case_id, "案例编号", MAX_ID)?;
        level_index(&self.level)?;
        identifiers(
            &self.declared_knowledge,
            "已声明掌握的知识",
            MAX_REQUIREMENTS,
        )?;
        single(&self.purpose, "学习目的", MAX_TEXT)
    }
}

impl Review {
    pub(crate) fn parse(text: &str) -> Result<Self, ProjectError> {
        let value: Self = serde_json::from_str(text)
            .map_err(|error| argument(format!("排除登记无法解析：{error}")))?;
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != REVIEW_POLICY {
            return Err(argument(format!(
                "排除登记的 policy 必须是 {REVIEW_POLICY}，实际是 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "排除登记的 schema_version 必须是 {SCHEMA_VERSION}。"
            )));
        }
        identifier(&self.case_id, "案例编号", MAX_ID)?;
        if self.reasons.is_empty() {
            return Err(argument(
                "排除登记必须至少列出一条允许的排除理由；没有理由集合就无法审计排除。",
            ));
        }
        if self.reasons.len() > MAX_REASONS {
            return Err(argument(format!(
                "排除理由最多 {MAX_REASONS} 条；请合并同类理由。"
            )));
        }
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for item in &self.reasons {
            identifier(&item.reason, "排除理由", MAX_ID)?;
            single(&item.note, "排除理由说明", MAX_TEXT)?;
            if !seen.insert(item.reason.as_str()) {
                return Err(argument(format!(
                    "排除理由 {} 在理由集合里重复登记。",
                    item.reason
                )));
            }
        }
        if self.exclusions.len() > MAX_EXCLUSIONS {
            return Err(argument(format!(
                "排除条目最多 {MAX_EXCLUSIONS} 条；请合并同类条目。"
            )));
        }
        for item in &self.exclusions {
            identifier(&item.source_id, "排除条目的来源编号", MAX_ID)?;
            if item.line == 0 {
                return Err(argument("排除条目的行号从 1 开始，不能是 0。"));
            }
            single(&item.detail, "排除条目说明", MAX_TEXT)?;
        }
        Ok(())
    }

    fn reason_set(&self) -> BTreeSet<&str> {
        self.reasons
            .iter()
            .map(|item| item.reason.as_str())
            .collect()
    }
}

impl Atoms {
    pub(crate) fn parse(text: &str, limits: &Limits) -> Result<Self, ProjectError> {
        let value: Self = serde_json::from_str(text)
            .map_err(|error| argument(format!("原子清单无法解析：{error}")))?;
        value.validate(limits)?;
        Ok(value)
    }

    fn validate(&self, limits: &Limits) -> Result<(), ProjectError> {
        if self.policy != ATOMS_POLICY {
            return Err(argument(format!(
                "原子清单的 policy 必须是 {ATOMS_POLICY}，实际是 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "原子清单的 schema_version 必须是 {SCHEMA_VERSION}。"
            )));
        }
        identifier(&self.case_id, "案例编号", MAX_ID)?;
        if self.atoms.is_empty() {
            return Err(argument("原子清单不能为空；没有原子就无法核对完整性。"));
        }
        if self.atoms.len() > limits.max_atoms {
            return Err(argument(format!(
                "原子清单有 {} 个原子，超过声明的上限 {}。",
                self.atoms.len(),
                limits.max_atoms
            )));
        }
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for atom in &self.atoms {
            identifier(&atom.atom_id, "原子编号", MAX_ID)?;
            identifier(&atom.source_id, "原子来源编号", MAX_ID)?;
            if atom.line == 0 {
                return Err(argument(format!(
                    "原子 {} 的行号从 1 开始，不能是 0。",
                    atom.atom_id
                )));
            }
            identifier(&atom.kind, "原子类型", MAX_ID)?;
            single(&atom.statement, "原子陈述", MAX_STATEMENT)?;
            condition_list(&atom.conditions, "适用条件", limits.max_conditions)?;
            identifiers(&atom.requires, "前置知识", limits.max_requirements)?;
            identifiers(&atom.covers, "原子提供的知识", limits.max_requirements)?;
            identifiers(&atom.levels, "适用水平", LEVELS.len())?;
            for level in &atom.levels {
                level_index(level)?;
            }
            if !seen.insert(atom.atom_id.as_str()) {
                return Err(argument(format!("原子编号 {} 重复。", atom.atom_id)));
            }
        }
        Ok(())
    }
}

impl Limits {
    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if !(1..=MAX_SOURCES).contains(&self.max_sources) {
            return Err(argument(format!("来源上限必须在 1..={MAX_SOURCES} 之间。")));
        }
        if !(1..=MAX_ATOMS).contains(&self.max_atoms) {
            return Err(argument(format!("原子上限必须在 1..={MAX_ATOMS} 之间。")));
        }
        if !(0..=MAX_CONDITIONS).contains(&self.max_conditions) {
            return Err(argument(format!(
                "条件下限必须在 0..={MAX_CONDITIONS} 之间。"
            )));
        }
        if !(0..=MAX_REQUIREMENTS).contains(&self.max_requirements) {
            return Err(argument(format!(
                "前置知识上限必须在 0..={MAX_REQUIREMENTS} 之间。"
            )));
        }
        Ok(())
    }
}

impl Author {
    pub(crate) fn parse(text: &str) -> Result<Self, ProjectError> {
        let value: Self = serde_json::from_str(text)
            .map_err(|error| argument(format!("作者包无法解析：{error}")))?;
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!(
                "作者包的 policy 必须是 {POLICY}，实际是 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "作者包的 schema_version 必须是 {SCHEMA_VERSION}。"
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "本阶段的作者包只能由 {AUTHOR_PRODUCER} 生成，实际是 {}。",
                self.producer
            )));
        }
        identifier(&self.case_id, "案例编号", MAX_ID)?;
        single(&self.description, "作者包说明", MAX_TEXT)?;
        relative(&self.audience, "用户水平声明路径")?;
        relative(&self.atoms, "原子清单路径")?;
        relative(&self.review, "排除登记路径")?;
        self.limits.validate()?;
        if self.sources.is_empty() {
            return Err(argument("作者包必须至少声明一份来源。"));
        }
        if self.sources.len() > self.limits.max_sources {
            return Err(argument(format!(
                "作者包声明了 {} 份来源，超过上限 {}。",
                self.sources.len(),
                self.limits.max_sources
            )));
        }
        let mut ids: BTreeSet<&str> = BTreeSet::new();
        let mut paths: BTreeSet<&str> = BTreeSet::new();
        for source in &self.sources {
            identifier(&source.source_id, "来源编号", MAX_ID)?;
            relative(&source.path, "来源路径")?;
            if !source.path.ends_with(".md") {
                return Err(argument(format!(
                    "来源 {} 必须是 .md 文件，实际是 {}。",
                    source.source_id, source.path
                )));
            }
            single(&source.title, "来源标题", MAX_TEXT)?;
            if !ids.insert(source.source_id.as_str()) {
                return Err(argument(format!("来源编号 {} 重复。", source.source_id)));
            }
            if !paths.insert(source.path.as_str()) {
                return Err(argument(format!(
                    "来源路径 {} 被两份来源重复声明。",
                    source.path
                )));
            }
        }
        Ok(())
    }
}

/// One frozen span and what happened to it. Non-text spans are always `kept`
/// because a heading is structure, not optional prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SpanDecision {
    pub(crate) span_id: String,
    pub(crate) source_id: String,
    pub(crate) kind: String,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) status: String,
    pub(crate) reason: Option<String>,
    pub(crate) detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AtomDecision {
    pub(crate) atom_id: String,
    pub(crate) source_id: String,
    pub(crate) line: usize,
    pub(crate) kind: String,
    pub(crate) statement: String,
    pub(crate) conditions: Vec<String>,
    pub(crate) requires: Vec<String>,
    pub(crate) covers: Vec<String>,
    pub(crate) levels: Vec<String>,
    pub(crate) required: bool,
    pub(crate) status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Counts {
    pub(crate) sources: usize,
    pub(crate) spans: usize,
    pub(crate) text_spans: usize,
    pub(crate) kept_spans: usize,
    pub(crate) excluded_spans: usize,
    pub(crate) atoms: usize,
    pub(crate) kept_atoms: usize,
    pub(crate) deferred_atoms: usize,
    pub(crate) conditions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Filter {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) level: String,
    pub(crate) purpose: String,
    pub(crate) counts: Counts,
    pub(crate) reason_counts: BTreeMap<String, usize>,
    pub(crate) spans: Vec<SpanDecision>,
    pub(crate) atoms: Vec<AtomDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Locator {
    pub(crate) source_id: String,
    pub(crate) line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompiledAtom {
    pub(crate) atom_id: String,
    pub(crate) kind: String,
    pub(crate) statement: String,
    pub(crate) conditions: Vec<String>,
    pub(crate) requires: Vec<String>,
    pub(crate) source: Locator,
    pub(crate) expansion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DensityCounts {
    pub(crate) atoms: usize,
    pub(crate) conditions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Density {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) density: String,
    pub(crate) level: String,
    pub(crate) counts: DensityCounts,
    pub(crate) atoms: Vec<CompiledAtom>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Report {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) level: String,
    pub(crate) counts: Counts,
    pub(crate) reason_counts: BTreeMap<String, usize>,
    pub(crate) densities: BTreeMap<String, DensityCounts>,
    pub(crate) unchanged_across_densities: bool,
    pub(crate) model_calls: u64,
    pub(crate) network_requests: u64,
}

pub(crate) struct Decision {
    pub(crate) filter: Filter,
    pub(crate) minimal: Density,
    pub(crate) detailed: Density,
    pub(crate) report: Report,
}

fn line_count(text: &str) -> usize {
    // Mirrors `str::lines()`: a trailing newline does not open a new line.
    if text.is_empty() {
        0
    } else {
        text.lines().count()
    }
}

fn span_of_line(record: &DocumentRecord, line: usize) -> Option<&crate::outline_model::SpanRecord> {
    record
        .spans
        .iter()
        .find(|span| span.start_line <= line && line <= span.end_line)
}

fn text_span_covering_line(
    record: &DocumentRecord,
    line: usize,
    label: &str,
) -> Result<crate::outline_model::SpanRecord, ProjectError> {
    let span = span_of_line(record, line).ok_or_else(|| {
        argument(format!(
            "{label}指向第 {line} 行，但来源 {} 只有 {} 行；请核实行号。",
            record.source_id,
            line_count(&record.title).max(0)
        ))
    })?;
    if span.kind != SpanKind::Text.as_str() {
        return Err(argument(format!(
            "{label}指向第 {line} 行，落在 {} 片段上；只有正文片段可以被排除或承载原子，标题与空行属于结构。",
            span.kind
        )));
    }
    Ok(span.clone())
}

/// Decide what is kept, what is excluded and why, then compile the two
/// densities. The order of the refusals is fixed so a caller can tell which
/// invariant fired: reason register, then exclusion placement, then excluded
/// required atoms, then missing prerequisites.
pub(crate) fn decide(
    documents: &[DocumentView<'_>],
    audience: &Audience,
    atoms: &Atoms,
    review: &Review,
    limits: &Limits,
) -> Result<Decision, ProjectError> {
    let level = level_index(&audience.level)?;
    let registered = review.reason_set();

    let mut by_source: BTreeMap<&str, &DocumentView<'_>> = BTreeMap::new();
    for view in documents {
        by_source.insert(view.record.source_id.as_str(), view);
    }
    for atom in &atoms.atoms {
        if !by_source.contains_key(atom.source_id.as_str()) {
            return Err(argument(format!(
                "原子 {} 声明的来源 {} 不在作者包里。",
                atom.atom_id, atom.source_id
            )));
        }
    }

    // 1. The exclusion register must be about registered reasons and real text.
    let mut exclusions: BTreeMap<(String, usize), (&ExclusionSpec, String)> = BTreeMap::new();
    for item in &review.exclusions {
        if !registered.contains(item.reason.as_str()) {
            return Err(argument(format!(
                "排除条目 {} 第 {} 行用了未登记的排除理由 {}；允许的理由是 {}。",
                item.source_id,
                item.line,
                item.reason,
                review
                    .reasons
                    .iter()
                    .map(|reason| reason.reason.as_str())
                    .collect::<Vec<_>>()
                    .join("、")
            )));
        }
        let view = by_source
            .get(item.source_id.as_str())
            .ok_or_else(|| argument(format!("排除条目指向未声明的来源 {}。", item.source_id)))?;
        let span = text_span_covering_line(view.record, item.line, "排除条目")?;
        let key = (item.source_id.clone(), item.line);
        if exclusions.contains_key(&key) {
            return Err(argument(format!(
                "来源 {} 第 {} 行被重复排除。",
                item.source_id, item.line
            )));
        }
        exclusions.insert(key, (item, span.span_id));
    }

    // 2. Project every span, then every atom.
    let mut spans: Vec<SpanDecision> = Vec::new();
    let mut reason_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut excluded_spans: BTreeSet<String> = BTreeSet::new();
    let mut kept_spans = 0usize;
    let mut total_text = 0usize;
    let mut total_spans = 0usize;
    for view in documents {
        for span in &view.record.spans {
            total_spans += 1;
            let is_text = span.kind == SpanKind::Text.as_str();
            if is_text {
                total_text += 1;
            }
            let hit = if is_text {
                exclusions
                    .iter()
                    .find(|((source, line), (_, span_id))| {
                        source == &view.record.source_id
                            && *span_id == span.span_id
                            && (span.start_line..=span.end_line).contains(line)
                    })
                    .map(|(_, (item, _))| *item)
            } else {
                None
            };
            match hit {
                Some(item) => {
                    excluded_spans.insert(span.span_id.clone());
                    *reason_counts.entry(item.reason.clone()).or_default() += 1;
                    spans.push(SpanDecision {
                        span_id: span.span_id.clone(),
                        source_id: view.record.source_id.clone(),
                        kind: span.kind.clone(),
                        start_line: span.start_line,
                        end_line: span.end_line,
                        status: STATUS_EXCLUDED.into(),
                        reason: Some(item.reason.clone()),
                        detail: Some(item.detail.clone()),
                    });
                }
                None => {
                    if is_text {
                        kept_spans += 1;
                    }
                    spans.push(SpanDecision {
                        span_id: span.span_id.clone(),
                        source_id: view.record.source_id.clone(),
                        kind: span.kind.clone(),
                        start_line: span.start_line,
                        end_line: span.end_line,
                        status: STATUS_KEPT.into(),
                        reason: None,
                        detail: None,
                    });
                }
            }
        }
    }

    let mut decisions: Vec<AtomDecision> = Vec::new();
    for atom in &atoms.atoms {
        let view = by_source[atom.source_id.as_str()];
        let span =
            text_span_covering_line(view.record, atom.line, &format!("原子 {}", atom.atom_id))?;
        let required = atom
            .levels
            .iter()
            .any(|candidate| level_index(candidate).is_ok_and(|index| index <= level));
        let status = if excluded_spans.contains(&span.span_id) {
            STATUS_DEFERRED
        } else {
            STATUS_KEPT
        };
        decisions.push(AtomDecision {
            atom_id: atom.atom_id.clone(),
            source_id: atom.source_id.clone(),
            line: atom.line,
            kind: atom.kind.clone(),
            statement: atom.statement.clone(),
            conditions: atom.conditions.clone(),
            requires: atom.requires.clone(),
            covers: atom.covers.clone(),
            levels: atom.levels.clone(),
            required,
            status: status.into(),
        });
    }

    // 3. A level-required atom may not be excluded; that is a refusal, not a note.
    let mut blocked: Vec<String> = Vec::new();
    for atom in decisions
        .iter()
        .filter(|atom| atom.required && atom.status != STATUS_KEPT)
    {
        let span = span_of_line(by_source[atom.source_id.as_str()].record, atom.line)
            .map(|span| span.span_id.clone())
            .unwrap_or_default();
        let reason = exclusions
            .values()
            .find(|(_, span_id)| *span_id == span)
            .map(|(item, _)| item.reason.clone())
            .unwrap_or_default();
        blocked.push(format!(
            "{}（来源 {} 第 {} 行，排除理由 {}）",
            atom.atom_id, atom.source_id, atom.line, reason
        ));
    }
    if !blocked.is_empty() {
        return Err(required_excluded(format!(
            "用户水平 {} 需要的原子被排除：{}。必要原子不能因为「极简」或「超纲」被删除；请调整水平声明或撤销排除。",
            audience.level,
            blocked.join("；")
        )));
    }

    // 4. Everything kept must be reachable from what the package establishes or
    //    what the reader declared. Silent omission is not allowed here either.
    let provided: BTreeSet<&str> = decisions
        .iter()
        .filter(|atom| atom.status == STATUS_KEPT)
        .flat_map(|atom| atom.covers.iter().map(String::as_str))
        .collect();
    let declared: BTreeSet<&str> = audience
        .declared_knowledge
        .iter()
        .map(String::as_str)
        .collect();
    let mut missing: Vec<String> = Vec::new();
    for atom in decisions.iter().filter(|atom| atom.status == STATUS_KEPT) {
        for need in &atom.requires {
            if !provided.contains(need.as_str()) && !declared.contains(need.as_str()) {
                missing.push(format!("{} 需要 {}", atom.atom_id, need));
            }
        }
    }
    if !missing.is_empty() {
        return Err(prerequisite_missing(format!(
            "必要前提缺失：{}。来源没有提供、用户也没有声明掌握；补足前提或降低目标后再筛选，不能静默省略。",
            missing.join("；")
        )));
    }

    let kept_atoms: Vec<&AtomDecision> = decisions
        .iter()
        .filter(|atom| atom.status == STATUS_KEPT)
        .collect();
    let conditions = kept_atoms
        .iter()
        .map(|atom| atom.conditions.len())
        .sum::<usize>();
    if conditions > limits.max_conditions * limits.max_atoms.max(1) {
        return Err(argument(format!(
            "保留的条件总数 {conditions} 超过声明的上限；请合并条件或调整 limits。"
        )));
    }

    let counts = Counts {
        sources: documents.len(),
        spans: total_spans,
        text_spans: total_text,
        kept_spans,
        excluded_spans: total_text - kept_spans,
        atoms: decisions.len(),
        kept_atoms: kept_atoms.len(),
        deferred_atoms: decisions.len() - kept_atoms.len(),
        conditions,
    };

    let mut minimal_atoms: Vec<CompiledAtom> = Vec::new();
    let mut detailed_atoms: Vec<CompiledAtom> = Vec::new();
    for atom in &kept_atoms {
        let base = CompiledAtom {
            atom_id: atom.atom_id.clone(),
            kind: atom.kind.clone(),
            statement: atom.statement.clone(),
            conditions: atom.conditions.clone(),
            requires: atom.requires.clone(),
            source: Locator {
                source_id: atom.source_id.clone(),
                line: atom.line,
            },
            expansion: None,
        };
        minimal_atoms.push(base.clone());
        detailed_atoms.push(CompiledAtom {
            expansion: Some(expansion(atom)),
            ..base
        });
    }

    let filter = Filter {
        policy: POLICY.into(),
        schema_version: SCHEMA_VERSION,
        case_id: audience.case_id.clone(),
        level: audience.level.clone(),
        purpose: audience.purpose.clone(),
        counts: counts.clone(),
        reason_counts: reason_counts.clone(),
        spans,
        atoms: decisions,
    };

    let density_counts = DensityCounts {
        atoms: minimal_atoms.len(),
        conditions,
    };
    let minimal = Density {
        policy: POLICY.into(),
        schema_version: SCHEMA_VERSION,
        case_id: audience.case_id.clone(),
        density: DENSITY_MINIMAL.into(),
        level: audience.level.clone(),
        counts: density_counts.clone(),
        atoms: minimal_atoms,
    };
    let detailed = Density {
        policy: POLICY.into(),
        schema_version: SCHEMA_VERSION,
        case_id: audience.case_id.clone(),
        density: DENSITY_DETAILED.into(),
        level: audience.level.clone(),
        counts: density_counts.clone(),
        atoms: detailed_atoms,
    };

    let mut densities: BTreeMap<String, DensityCounts> = BTreeMap::new();
    densities.insert(DENSITY_MINIMAL.into(), minimal.counts.clone());
    densities.insert(DENSITY_DETAILED.into(), detailed.counts.clone());
    let unchanged = comparable(&minimal) == comparable(&detailed);

    let report = Report {
        policy: POLICY.into(),
        schema_version: SCHEMA_VERSION,
        case_id: audience.case_id.clone(),
        level: audience.level.clone(),
        counts: filter.counts.clone(),
        reason_counts,
        densities,
        unchanged_across_densities: unchanged,
        model_calls: 0,
        network_requests: 0,
    };

    Ok(Decision {
        filter,
        minimal,
        detailed,
        report,
    })
}

fn expansion(atom: &AtomDecision) -> String {
    let conditions = if atom.conditions.is_empty() {
        "无附加条件".to_owned()
    } else {
        atom.conditions.join("、")
    };
    let requires = if atom.requires.is_empty() {
        "无".to_owned()
    } else {
        atom.requires.join("、")
    };
    format!(
        "{}。适用条件：{}。前置知识：{}。来源：{} 第 {} 行。",
        atom.statement.trim_end_matches('。'),
        conditions,
        requires,
        atom.source_id,
        atom.line
    )
}

/// The part of a density that must not change between minimal and detailed.
fn comparable(density: &Density) -> Vec<(String, Vec<String>, Vec<String>)> {
    density
        .atoms
        .iter()
        .map(|atom| {
            (
                atom.atom_id.clone(),
                atom.conditions.clone(),
                atom.requires.clone(),
            )
        })
        .collect()
}

pub(crate) fn render_markdown(decision: &Decision) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# 相关性筛选记录（{}）\n\n",
        decision.filter.case_id
    ));
    out.push_str(&format!(
        "用户水平：{}；学习目的：{}\n\n",
        decision.filter.level, decision.filter.purpose
    ));
    out.push_str("## 计数\n\n");
    let counts = &decision.filter.counts;
    out.push_str(&format!(
        "- 来源 {} 份、片段 {} 个（正文片段 {} 个）\n",
        counts.sources, counts.spans, counts.text_spans
    ));
    out.push_str(&format!(
        "- 保留正文片段 {} 个、排除 {} 个\n",
        counts.kept_spans, counts.excluded_spans
    ));
    out.push_str(&format!(
        "- 原子 {} 个：保留 {}、暂缓 {}\n",
        counts.atoms, counts.kept_atoms, counts.deferred_atoms
    ));
    out.push_str(&format!("- 保留条件 {} 条\n\n", counts.conditions));

    out.push_str("## 排除理由分布\n\n| 理由 | 条数 |\n| --- | --- |\n");
    if decision.filter.reason_counts.is_empty() {
        out.push_str("| （无排除） | 0 |\n");
    }
    for (reason, count) in &decision.filter.reason_counts {
        out.push_str(&format!("| {reason} | {count} |\n"));
    }
    out.push('\n');

    out.push_str("## 排除明细\n\n| 来源 | 行 | 理由 | 说明 |\n| --- | --- | --- | --- |\n");
    let mut rows = 0usize;
    for span in decision
        .filter
        .spans
        .iter()
        .filter(|span| span.status == STATUS_EXCLUDED)
    {
        rows += 1;
        out.push_str(&format!(
            "| {} | {}-{} | {} | {} |\n",
            span.source_id,
            span.start_line,
            span.end_line,
            span.reason.clone().unwrap_or_default(),
            span.detail.clone().unwrap_or_default()
        ));
    }
    if rows == 0 {
        out.push_str("| （无） | - | - | - |\n");
    }
    out.push('\n');

    out.push_str("## 保留原子与两种密度\n\n");
    out.push_str(
        "| 原子 | 状态 | 条件数 | 极简原子数 | 详细原子数 |\n| --- | --- | --- | --- | --- |\n",
    );
    out.push_str(&format!(
        "| （合计） | - | {} | {} | {} |\n",
        decision.minimal.counts.conditions,
        decision.minimal.counts.atoms,
        decision.detailed.counts.atoms
    ));
    for atom in &decision.filter.atoms {
        out.push_str(&format!(
            "| {} | {} | {} | - | - |\n",
            atom.atom_id,
            atom.status,
            atom.conditions.len()
        ));
    }
    out.push('\n');

    out.push_str("## 边界\n\n");
    out.push_str(&format!(
        "- 两种密度的原子与条件完全一致：{}。\n",
        if decision.report.unchanged_across_densities {
            "是"
        } else {
            "否"
        }
    ));
    out.push_str(
        "- 排除只写进筛选记录，冻结来源的每个片段都保留在 `control/` 里，没有被删除或改写。\n",
    );
    out.push_str(
        "- 模型调用与网络请求：0 次；本阶段不判断科学正确性，只做可审计的保留/排除决策。\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline_model::DocumentRecord;

    fn record(source_id: &str) -> DocumentRecord {
        DocumentRecord {
            source_id: source_id.into(),
            title: "示例".into(),
            path: format!("documents/{source_id}.md"),
            raw_sha256: "a".repeat(64),
            byte_length: 1,
            normalized_sha256: "b".repeat(64),
            span_count: 0,
            spans: Vec::new(),
        }
    }

    fn audience(level: &str, knowledge: &[&str]) -> Audience {
        Audience {
            policy: AUDIENCE_POLICY.into(),
            schema_version: SCHEMA_VERSION,
            case_id: "case".into(),
            level: level.into(),
            declared_knowledge: knowledge.iter().map(|item| item.to_string()).collect(),
            purpose: "考试".into(),
        }
    }

    fn atom(
        id: &str,
        line: usize,
        levels: &[&str],
        requires: &[&str],
        conditions: &[&str],
    ) -> AtomSpec {
        AtomSpec {
            atom_id: id.into(),
            source_id: "doc".into(),
            line,
            kind: "definition".into(),
            statement: format!("陈述 {id}"),
            conditions: conditions.iter().map(|item| item.to_string()).collect(),
            requires: requires.iter().map(|item| item.to_string()).collect(),
            covers: Vec::new(),
            levels: levels.iter().map(|item| item.to_string()).collect(),
        }
    }

    fn view<'a>(record: &'a DocumentRecord, text: &'a str) -> DocumentView<'a> {
        DocumentView { record, text }
    }

    fn limits() -> Limits {
        Limits {
            max_sources: 4,
            max_atoms: 8,
            max_conditions: 4,
            max_requirements: 4,
        }
    }

    fn review(reasons: &[&str], exclusions: Vec<ExclusionSpec>) -> Review {
        Review {
            policy: REVIEW_POLICY.into(),
            schema_version: SCHEMA_VERSION,
            case_id: "case".into(),
            reasons: reasons
                .iter()
                .map(|reason| ReasonSpec {
                    reason: (*reason).into(),
                    note: format!("{reason} 说明"),
                })
                .collect(),
            exclusions,
        }
    }

    fn two_span_record() -> DocumentRecord {
        let mut record = record("doc");
        record.spans = vec![
            crate::outline_model::SpanRecord {
                span_id: "doc-00001".into(),
                kind: SpanKind::Text.as_str().into(),
                start_byte: 0,
                end_byte: 6,
                start_line: 1,
                end_line: 2,
                text_sha256: "c".repeat(64),
            },
            crate::outline_model::SpanRecord {
                span_id: "doc-00002".into(),
                kind: SpanKind::Text.as_str().into(),
                start_byte: 7,
                end_byte: 13,
                start_line: 3,
                end_line: 4,
                text_sha256: "d".repeat(64),
            },
        ];
        record.span_count = 2;
        record
    }

    #[test]
    fn levels_are_ordered_from_introductory_to_advanced() {
        assert!(matches!(level_index("introductory"), Ok(0)));
        assert!(matches!(level_index("university"), Ok(1)));
        assert!(matches!(level_index("advanced"), Ok(2)));
        assert!(level_index("phd").is_err());
    }

    #[test]
    fn an_exclusion_needs_a_registered_reason() {
        let record = two_span_record();
        let text = "第一行\n第二行\n第三行\n第四行\n";
        let atoms = Atoms {
            policy: ATOMS_POLICY.into(),
            schema_version: SCHEMA_VERSION,
            case_id: "case".into(),
            atoms: vec![atom("a", 1, &["university"], &[], &[])],
        };
        let review = review(
            &["decoration"],
            vec![ExclusionSpec {
                source_id: "doc".into(),
                line: 3,
                reason: "boring".into(),
                detail: "自造词".into(),
            }],
        );
        let error = decide(
            &[view(&record, text)],
            &audience("university", &[]),
            &atoms,
            &review,
            &limits(),
        )
        .err()
        .expect("未登记的理由必须拒绝");
        assert_eq!(error.code(), "RELEVANCE_ARGUMENT");
        assert!(error.to_string().contains("未登记的排除理由"));
    }

    #[test]
    fn headings_cannot_be_excluded_because_they_are_structure() {
        let mut record = record("doc");
        record.spans = vec![crate::outline_model::SpanRecord {
            span_id: "doc-00001".into(),
            kind: SpanKind::Heading.as_str().into(),
            start_byte: 0,
            end_byte: 4,
            start_line: 1,
            end_line: 1,
            text_sha256: "e".repeat(64),
        }];
        let atoms = Atoms {
            policy: ATOMS_POLICY.into(),
            schema_version: SCHEMA_VERSION,
            case_id: "case".into(),
            atoms: vec![atom("a", 1, &["university"], &[], &[])],
        };
        let review = review(
            &["decoration"],
            vec![ExclusionSpec {
                source_id: "doc".into(),
                line: 1,
                reason: "decoration".into(),
                detail: "想删标题".into(),
            }],
        );
        let error = decide(
            &[view(&record, "# 标题\n")],
            &audience("university", &[]),
            &atoms,
            &review,
            &limits(),
        )
        .err()
        .expect("标题不能被排除");
        assert_eq!(error.code(), "RELEVANCE_ARGUMENT");
        assert!(error.to_string().contains("只有正文片段可以被排除"));
    }

    #[test]
    fn excluding_an_atom_the_level_requires_is_refused() {
        let record = two_span_record();
        let text = "第一行\n第二行\n第三行\n第四行\n";
        let atoms = Atoms {
            policy: ATOMS_POLICY.into(),
            schema_version: SCHEMA_VERSION,
            case_id: "case".into(),
            atoms: vec![atom("a", 3, &["university"], &[], &[])],
        };
        let review = review(
            &["beyond_target_level"],
            vec![ExclusionSpec {
                source_id: "doc".into(),
                line: 3,
                reason: "beyond_target_level".into(),
                detail: "以为超纲".into(),
            }],
        );
        let error = decide(
            &[view(&record, text)],
            &audience("university", &[]),
            &atoms,
            &review,
            &limits(),
        )
        .err()
        .expect("必要原子不能排除");
        assert_eq!(error.code(), "RELEVANCE_REQUIRED_EXCLUDED");
    }

    #[test]
    fn an_option_of_a_higher_level_can_be_deferred_but_not_needed() {
        let record = two_span_record();
        let text = "第一行\n第二行\n第三行\n第四行\n";
        let atoms = Atoms {
            policy: ATOMS_POLICY.into(),
            schema_version: SCHEMA_VERSION,
            case_id: "case".into(),
            atoms: vec![
                atom("keep", 1, &["university"], &[], &[]),
                atom("frontier", 3, &["advanced"], &[], &[]),
            ],
        };
        let review = review(
            &["beyond_target_level"],
            vec![ExclusionSpec {
                source_id: "doc".into(),
                line: 3,
                reason: "beyond_target_level".into(),
                detail: "前沿方法学".into(),
            }],
        );
        let decision = decide(
            &[view(&record, text)],
            &audience("university", &[]),
            &atoms,
            &review,
            &limits(),
        )
        .expect("本科水平可以暂缓进阶原子");
        assert_eq!(decision.filter.counts.kept_atoms, 1);
        assert_eq!(decision.filter.counts.deferred_atoms, 1);
        assert!(decision.report.unchanged_across_densities);
    }

    #[test]
    fn a_missing_prerequisite_is_a_refusal_and_a_declared_one_is_not() {
        let record = two_span_record();
        let text = "第一行\n第二行\n第三行\n第四行\n";
        let atoms = Atoms {
            policy: ATOMS_POLICY.into(),
            schema_version: SCHEMA_VERSION,
            case_id: "case".into(),
            atoms: vec![atom("a", 1, &["university"], &["ring"], &[])],
        };
        let no_register = review(&["decoration"], Vec::new());
        let error = decide(
            &[view(&record, text)],
            &audience("university", &[]),
            &atoms,
            &no_register,
            &limits(),
        )
        .err()
        .expect("前提缺失必须拒绝");
        assert_eq!(error.code(), "RELEVANCE_PREREQUISITE_MISSING");

        let decision = decide(
            &[view(&record, text)],
            &audience("university", &["ring"]),
            &atoms,
            &no_register,
            &limits(),
        )
        .expect("用户已声明掌握该前提");
        assert_eq!(decision.filter.counts.kept_atoms, 1);
    }

    #[test]
    fn both_densities_carry_the_same_atoms_and_conditions() {
        let record = two_span_record();
        let text = "第一行\n第二行\n第三行\n第四行\n";
        let atoms = Atoms {
            policy: ATOMS_POLICY.into(),
            schema_version: SCHEMA_VERSION,
            case_id: "case".into(),
            atoms: vec![
                atom("a", 1, &["university"], &[], &["条件一"]),
                atom("b", 3, &["university"], &[], &["条件二", "条件三"]),
            ],
        };
        let review = review(&["decoration"], Vec::new());
        let decision = decide(
            &[view(&record, text)],
            &audience("university", &[]),
            &atoms,
            &review,
            &limits(),
        )
        .expect("两种密度都成立");
        assert_eq!(decision.minimal.counts, decision.detailed.counts);
        assert_eq!(decision.minimal.counts.atoms, 2);
        assert_eq!(decision.minimal.counts.conditions, 3);
        assert!(decision.report.unchanged_across_densities);
        assert!(
            decision
                .minimal
                .atoms
                .iter()
                .all(|atom| atom.expansion.is_none())
        );
        assert!(
            decision
                .detailed
                .atoms
                .iter()
                .all(|atom| atom.expansion.is_some())
        );
    }

    #[test]
    fn the_markdown_record_lists_counts_reasons_and_boundaries() {
        let record = two_span_record();
        let text = "第一行\n第二行\n第三行\n第四行\n";
        let atoms = Atoms {
            policy: ATOMS_POLICY.into(),
            schema_version: SCHEMA_VERSION,
            case_id: "case".into(),
            atoms: vec![atom("a", 1, &["university"], &[], &[])],
        };
        let review = review(
            &["decoration"],
            vec![ExclusionSpec {
                source_id: "doc".into(),
                line: 3,
                reason: "decoration".into(),
                detail: "修辞".into(),
            }],
        );
        let decision = decide(
            &[view(&record, text)],
            &audience("university", &[]),
            &atoms,
            &review,
            &limits(),
        )
        .expect("可以生成记录");
        let markdown = render_markdown(&decision);
        assert!(markdown.contains("## 排除理由分布"));
        assert!(markdown.contains("| decoration | 1 |"));
        assert!(markdown.contains("## 边界"));
        assert!(markdown.contains("模型调用与网络请求：0 次"));
    }
}
