//! Library-wide audit of link availability and canonical explanation position.
//!
//! This layer answers two questions about one frozen release set:
//!
//! * does every internal link resolve to exactly one expected target, and
//! * does every concept that needs a full explanation have exactly one?
//!
//! Both answers are structural. The audit reads declared note bodies, headings,
//! wikilinks and explicit `> 完整解释：<concept>` markers; it never guesses at
//! meaning, never rewrites a note and never calls a model. A concept counted as
//! explained only means "exactly one note section declares itself the canonical
//! explanation" — it says nothing about whether that section is scientifically
//! right, and the report says so in its own boundary block.
//!
//! Two rules carry most of the weight:
//!
//! 1. A link is only `resolved` when the note *and* the heading are unique. A
//!    duplicated heading makes the target ambiguous rather than picking the
//!    first match, because a silent first-match is how a link starts pointing
//!    at the wrong paragraph without anyone noticing.
//! 2. Findings are a result, not a reason to refuse. `build` still writes the
//!    whole report when links are broken or explanations are duplicated, and
//!    reports the distinct exit code so the caller can tell "the input was
//!    illegal" from "the input is legal and the library has gaps".

use crate::project::ProjectError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const POLICY: &str = "library-audit-v1";
pub(crate) const GOLD_POLICY: &str = "library-gold-v1";
pub(crate) const AUTHOR_PRODUCER: &str = "fixture_author";
pub(crate) const SCHEMA_VERSION: u32 = 1;

pub(crate) const MAX_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_NOTES: usize = 16;
pub(crate) const MAX_ATOMS: usize = 512;
pub(crate) const MAX_LINKS: usize = 4096;
pub(crate) const MAX_ID: usize = 48;
pub(crate) const MAX_TEXT: usize = 600;
pub(crate) const MAX_TITLE: usize = 200;

/// A note section claims to hold the full explanation of a concept with this
/// exact marker prefix. Anything else — prose, a callout with a different
/// wording — is not a declaration, so a concept can never be "explained" by
/// accident.
pub(crate) const MARKER_PREFIX: &str = "> 完整解释：";

pub(crate) const LINK_RESOLVED: &str = "resolved";
pub(crate) const LINK_BROKEN: &str = "broken";
pub(crate) const LINK_AMBIGUOUS: &str = "ambiguous";

pub(crate) const BASIS_EXACT_PATH: &str = "exact_path";
pub(crate) const BASIS_BARE_NAME: &str = "bare_name";
pub(crate) const BASIS_DUPLICATE_NOTE_NAME: &str = "duplicate_note_name";
pub(crate) const BASIS_PATH_MISSING: &str = "path_missing";
pub(crate) const BASIS_DUPLICATE_HEADING: &str = "duplicate_heading";
pub(crate) const BASIS_HEADING_MISSING: &str = "heading_missing";

pub(crate) const FINDING_BROKEN_LINK: &str = "broken_link";
pub(crate) const FINDING_AMBIGUOUS_LINK: &str = "ambiguous_link";
pub(crate) const FINDING_DUPLICATE_EXPLANATION: &str = "duplicate_explanation";
pub(crate) const FINDING_CONCEPT_UNEXPLAINED: &str = "concept_unexplained";
pub(crate) const FINDING_CANONICAL_MISMATCH: &str = "canonical_position_mismatch";
pub(crate) const FINDING_ATOM_MISSING: &str = "atom_missing";
pub(crate) const FINDING_RECALL_BELOW: &str = "recall_below_threshold";

pub(crate) const SEVERITY_CRITICAL: &str = "critical";
pub(crate) const SEVERITY_GENERAL: &str = "general";

/// NFR-02 allows a small gap for general atoms but never for a key atom.
pub(crate) const GENERAL_RECALL_PERCENT: usize = 98;

pub(crate) fn argument(message: impl Into<String>) -> ProjectError {
    ProjectError::new("AUDIT_ARGUMENT", 2, message.into())
}

pub(crate) fn integrity(message: impl Into<String>) -> ProjectError {
    ProjectError::new("AUDIT_INTEGRITY", 3, message.into())
}

pub(crate) fn unknown_root(message: impl Into<String>) -> ProjectError {
    ProjectError::new("AUDIT_UNKNOWN_ROOT", 3, message.into())
}

pub(crate) fn findings(message: impl Into<String>) -> ProjectError {
    ProjectError::new("AUDIT_FINDINGS", 3, message.into())
}

// ---------------------------------------------------------------------------
// declarations
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoteSpec {
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) title: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Limits {
    pub(crate) max_notes: usize,
    pub(crate) max_atoms: usize,
    pub(crate) max_links: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Project {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) gold: String,
    pub(crate) notes: Vec<NoteSpec>,
    pub(crate) limits: Limits,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConceptSpec {
    pub(crate) concept_id: String,
    pub(crate) canonical: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AtomSpec {
    pub(crate) atom_id: String,
    pub(crate) concept_id: String,
    pub(crate) key: bool,
    pub(crate) note_id: String,
    pub(crate) heading: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Gold {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) concepts: Vec<ConceptSpec>,
    pub(crate) atoms: Vec<AtomSpec>,
}

// ---------------------------------------------------------------------------
// views produced by the caller
// ---------------------------------------------------------------------------

pub(crate) struct NoteView<'a> {
    pub(crate) spec: &'a NoteSpec,
    pub(crate) text: &'a str,
}

// ---------------------------------------------------------------------------
// scanned structures
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Section {
    pub(crate) index: usize,
    pub(crate) level: u8,
    pub(crate) title: String,
    pub(crate) line: usize,
    pub(crate) end_line: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Marker {
    pub(crate) concept_id: String,
    pub(crate) line: usize,
    pub(crate) heading: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct RawLink {
    pub(crate) line: usize,
    pub(crate) raw: String,
    pub(crate) target: String,
    pub(crate) heading: Option<String>,
    pub(crate) alias: Option<String>,
    pub(crate) embed: bool,
    pub(crate) in_table: bool,
    pub(crate) escaped_alias: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct LinkRecord {
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) line: usize,
    pub(crate) raw: String,
    pub(crate) target: String,
    pub(crate) heading: Option<String>,
    pub(crate) alias: Option<String>,
    pub(crate) embed: bool,
    pub(crate) in_table: bool,
    pub(crate) escaped_alias: bool,
    pub(crate) status: String,
    pub(crate) basis: String,
    pub(crate) resolved_note: Option<String>,
    pub(crate) resolved_heading: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct MarkerRecord {
    pub(crate) note_id: String,
    pub(crate) heading: Option<String>,
    pub(crate) line: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ConceptRecord {
    pub(crate) concept_id: String,
    pub(crate) canonical: String,
    pub(crate) markers: Vec<MarkerRecord>,
    pub(crate) canonical_note: Option<String>,
    pub(crate) canonical_heading: Option<String>,
    pub(crate) unique: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AtomRecord {
    pub(crate) atom_id: String,
    pub(crate) concept_id: String,
    pub(crate) key: bool,
    pub(crate) note_id: String,
    pub(crate) heading: String,
    pub(crate) recalled: bool,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Finding {
    pub(crate) kind: String,
    pub(crate) severity: String,
    pub(crate) subject: String,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ratio {
    pub(crate) numerator: usize,
    pub(crate) denominator: usize,
    pub(crate) applicable: bool,
    pub(crate) display: String,
    pub(crate) note: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Counts {
    pub(crate) notes: usize,
    pub(crate) headings: usize,
    pub(crate) links: usize,
    pub(crate) links_resolved: usize,
    pub(crate) links_broken: usize,
    pub(crate) links_ambiguous: usize,
    pub(crate) links_embedded: usize,
    pub(crate) links_in_table: usize,
    pub(crate) links_escaped_alias: usize,
    pub(crate) concepts: usize,
    pub(crate) markers: usize,
    pub(crate) atoms: usize,
    pub(crate) atoms_key: usize,
    pub(crate) atoms_general: usize,
    pub(crate) atoms_recalled: usize,
    pub(crate) atoms_missing: usize,
    pub(crate) duplicate_concepts: usize,
    pub(crate) findings: usize,
    pub(crate) model_calls: usize,
    pub(crate) network_requests: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Audit {
    pub(crate) links: Vec<LinkRecord>,
    pub(crate) concepts: Vec<ConceptRecord>,
    pub(crate) atoms: Vec<AtomRecord>,
    pub(crate) findings: Vec<Finding>,
    pub(crate) counts: Counts,
    pub(crate) availability: Ratio,
    pub(crate) key_recall: Ratio,
    pub(crate) general_recall: Ratio,
    pub(crate) verdict: String,
}

// ---------------------------------------------------------------------------
// validation helpers
// ---------------------------------------------------------------------------

pub(crate) fn single(text: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    if text.trim().is_empty() {
        return Err(argument(format!("{label} 不能为空。")));
    }
    if text.chars().count() > limit {
        return Err(argument(format!("{label} 超过 {limit} 个字符。")));
    }
    if text.contains('\n') || text.contains('\r') || text.contains('\t') {
        return Err(argument(format!("{label} 不能包含换行或制表符。")));
    }
    Ok(())
}

pub(crate) fn identifier(text: &str, label: &str) -> Result<(), ProjectError> {
    single(text, label, MAX_ID)?;
    let ok = text
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'));
    if !ok || text.starts_with('.') {
        return Err(argument(format!(
            "{label} 只能使用 ASCII 字母、数字与 - _ .，且不能以点开头：{text}"
        )));
    }
    Ok(())
}

pub(crate) fn relative(text: &str, label: &str) -> Result<(), ProjectError> {
    single(text, label, MAX_TEXT)?;
    if text.starts_with('/') || text.contains(':') || text.contains('\\') {
        return Err(argument(format!(
            "{label} 必须是相对路径且不含盘符与反斜杠：{text}"
        )));
    }
    if text.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
        return Err(argument(format!(
            "{label} 不能包含空路径段、. 或 ..：{text}"
        )));
    }
    Ok(())
}

impl Project {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Project, ProjectError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|error| argument(format!("工程声明不是有效的 UTF-8：{error}")))?;
        let project: Project = serde_json::from_str(text)
            .map_err(|error| argument(format!("工程声明不是有效的声明 JSON：{error}")))?;
        project.validate()?;
        Ok(project)
    }

    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!(
                "工程声明的 policy 必须是 {POLICY}，实际是 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "工程声明的 schema_version 必须是 {SCHEMA_VERSION}，实际是 {}。",
                self.schema_version
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "工程声明的 producer 必须是 {AUTHOR_PRODUCER}，实际是 {}。",
                self.producer
            )));
        }
        identifier(&self.case_id, "case_id")?;
        single(&self.description, "description", MAX_TEXT)?;
        relative(&self.gold, "gold")?;
        if self.limits.max_notes < 1
            || self.limits.max_notes > MAX_NOTES
            || self.limits.max_atoms < 1
            || self.limits.max_atoms > MAX_ATOMS
            || self.limits.max_links < 1
            || self.limits.max_links > MAX_LINKS
        {
            return Err(argument(format!(
                "limits 必须满足 1..={MAX_NOTES} 份笔记、1..={MAX_ATOMS} 个原子、1..={MAX_LINKS} 条链接。"
            )));
        }
        if self.notes.is_empty() || self.notes.len() > self.limits.max_notes {
            return Err(argument(format!(
                "笔记数量必须在 1..={} 之间，实际 {}。",
                self.limits.max_notes,
                self.notes.len()
            )));
        }
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for note in &self.notes {
            identifier(&note.note_id, "note_id")?;
            relative(&note.path, "笔记路径")?;
            simple_title(&note.title, "笔记标题")?;
            if !note.path.ends_with(".md") {
                return Err(argument(format!("笔记路径必须以 .md 结尾：{}", note.path)));
            }
            if !ids.insert(note.note_id.as_str()) {
                return Err(argument(format!("note_id 重复：{}", note.note_id)));
            }
            if !paths.insert(note.path.as_str()) {
                return Err(argument(format!("笔记路径重复：{}", note.path)));
            }
        }
        Ok(())
    }

    pub(crate) fn note(&self, note_id: &str) -> Option<&NoteSpec> {
        self.notes.iter().find(|note| note.note_id == note_id)
    }
}

fn simple_title(text: &str, label: &str) -> Result<(), ProjectError> {
    single(text, label, MAX_TITLE)?;
    if text.contains('|') || text.contains('[') || text.contains(']') {
        return Err(argument(format!(
            "{label} 不能包含竖线或方括号：{text}"
        )));
    }
    Ok(())
}

impl Gold {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Gold, ProjectError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|error| argument(format!("金标准不是有效的 UTF-8：{error}")))?;
        let gold: Gold = serde_json::from_str(text)
            .map_err(|error| argument(format!("金标准不是有效的声明 JSON：{error}")))?;
        gold.validate()?;
        Ok(gold)
    }

    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != GOLD_POLICY {
            return Err(argument(format!(
                "金标准的 policy 必须是 {GOLD_POLICY}，实际是 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "金标准的 schema_version 必须是 {SCHEMA_VERSION}，实际是 {}。",
                self.schema_version
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "金标准的 producer 必须是 {AUTHOR_PRODUCER}，实际是 {}。",
                self.producer
            )));
        }
        identifier(&self.case_id, "case_id")?;
        single(&self.description, "description", MAX_TEXT)?;
        if self.concepts.is_empty() || self.concepts.len() > MAX_ATOMS {
            return Err(argument("金标准必须列出至少一个概念，且不超过上限。"));
        }
        let mut concept_ids = BTreeSet::new();
        for concept in &self.concepts {
            identifier(&concept.concept_id, "concept_id")?;
            simple_title(&concept.canonical, "概念正名")?;
            if !concept_ids.insert(concept.concept_id.as_str()) {
                return Err(argument(format!(
                    "concept_id 重复：{}",
                    concept.concept_id
                )));
            }
        }
        if self.atoms.is_empty() || self.atoms.len() > MAX_ATOMS {
            return Err(argument("金标准必须列出至少一个原子，且不超过上限。"));
        }
        let mut atom_ids = BTreeSet::new();
        for atom in &self.atoms {
            identifier(&atom.atom_id, "atom_id")?;
            if !atom_ids.insert(atom.atom_id.as_str()) {
                return Err(argument(format!("atom_id 重复：{}", atom.atom_id)));
            }
            if !concept_ids.contains(atom.concept_id.as_str()) {
                return Err(argument(format!(
                    "原子 {} 指向未登记的概念 {}。",
                    atom.atom_id, atom.concept_id
                )));
            }
            if atom.heading.trim().is_empty() {
                return Err(argument(format!(
                    "原子 {} 的期望章节不能为空。",
                    atom.atom_id
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn concept(&self, concept_id: &str) -> Option<&ConceptSpec> {
        self.concepts
            .iter()
            .find(|concept| concept.concept_id == concept_id)
    }
}

// ---------------------------------------------------------------------------
// body scanning
// ---------------------------------------------------------------------------

/// `#` followed by end of line or whitespace, mirroring `crate::text::heading`.
/// A looser rule would let the audit claim headings the span layer never saw.
pub(crate) fn heading_level(line: &str) -> u8 {
    let trimmed = line.trim_start_matches(' ');
    let indentation = line.len() - trimmed.len();
    if indentation > 3 {
        return 0;
    }
    let hashes = trimmed.chars().take_while(|value| *value == '#').count();
    if !(1..=6).contains(&hashes) {
        return 0;
    }
    let rest = &trimmed[hashes..];
    match rest.chars().next() {
        None => hashes as u8,
        Some(value) if value.is_whitespace() => hashes as u8,
        Some(_) => 0,
    }
}

pub(crate) fn heading_title(line: &str) -> String {
    line.trim()
        .trim_start_matches('#')
        .trim()
        .trim_end_matches('#')
        .trim()
        .to_owned()
}

/// Sections tile the body: each one runs from its heading line to the line
/// before the next heading. A section that stopped at its own heading line
/// could never contain a marker, making "explained here" unfalsifiable.
pub(crate) fn sections(text: &str) -> Vec<Section> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found: Vec<Section> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let level = heading_level(line);
        if level == 0 {
            continue;
        }
        found.push(Section {
            index: found.len(),
            level,
            title: heading_title(line),
            line: index + 1,
            end_line: index + 1,
        });
    }
    let total = lines.len().max(1);
    for position in 0..found.len() {
        let start = found[position].line;
        let end = found
            .get(position + 1)
            .map(|next| next.line.saturating_sub(1))
            .unwrap_or(total);
        found[position].end_line = end.max(start);
    }
    found
}

pub(crate) fn markers(text: &str) -> Vec<Marker> {
    let line_sections = sections(text);
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let Some(rest) = line.strip_prefix(MARKER_PREFIX) else {
            continue;
        };
        let concept_id = rest.trim().to_owned();
        if concept_id.is_empty() {
            continue;
        }
        let heading = line_sections
            .iter()
            .rev()
            .find(|section| section.line <= number && number <= section.end_line)
            .map(|section| section.title.clone());
        found.push(Marker {
            concept_id,
            line: number,
            heading,
        });
    }
    found
}

/// Scan one body for wikilinks. `![[...]]` is an embed, and a table cell needs
/// an escaped pipe before the alias — an unescaped one silently ends the cell
/// and is reported by the caller as a declaration error.
pub(crate) fn scan_links(text: &str) -> Vec<RawLink> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let in_table = line.trim_start().starts_with('|');
        let bytes = line.as_bytes();
        let mut cursor = 0usize;
        while cursor + 1 < bytes.len() {
            // 双链必须以两个左方括号开头：单个 [ 不是链接，否则每个 [[..]] 的目标都会
            // 多带一个左括号，所有按路径解析的链接都会被判成断链。
            let embed = bytes[cursor] == b'!' && bytes.get(cursor + 1) == Some(&b'[');
            let opened = if embed {
                bytes.get(cursor + 2) == Some(&b'[')
            } else {
                bytes[cursor] == b'[' && bytes[cursor + 1] == b'['
            };
            if !opened {
                cursor += 1;
                continue;
            }
            let start = if embed { cursor + 3 } else { cursor + 2 };
            let mut end = start;
            let mut closed = false;
            while end + 1 < bytes.len() {
                if bytes[end] == b']' && bytes[end + 1] == b']' {
                    closed = true;
                    break;
                }
                end += 1;
            }
            if !closed {
                break;
            }
            let inner = &line[start..end];
            found.push(parse_link(inner, number, embed, in_table));
            cursor = end + 2;
        }
    }
    found
}

pub(crate) fn parse_link(inner: &str, line: usize, embed: bool, in_table: bool) -> RawLink {
    let (body, alias_split) = match inner.find("\\|") {
        Some(position) => (
            &inner[..position],
            Some((position + 2, true)),
        ),
        None => match inner.find('|') {
            Some(position) => (&inner[..position], Some((position + 1, false))),
            None => (inner, None),
        },
    };
    let alias = alias_split.map(|(start, _)| inner[start..].trim().to_owned());
    let escaped_alias = alias_split.is_some_and(|(_, escaped)| escaped);
    let (target, heading) = match body.split_once('#') {
        Some((target, heading)) => (
            target.trim().to_owned(),
            Some(heading.trim().to_owned()).filter(|value| !value.is_empty()),
        ),
        None => (body.trim().to_owned(), None),
    };
    RawLink {
        line,
        raw: format!(
            "{}[[{inner}]]",
            if embed { "!" } else { "" }
        ),
        target: target.trim_end_matches(".md").to_owned(),
        heading,
        alias,
        embed,
        in_table,
        escaped_alias,
    }
}

// ---------------------------------------------------------------------------
// evaluation
// ---------------------------------------------------------------------------

fn note_stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".md").unwrap_or(name)
}

fn resolve_note<'a>(
    project: &'a Project,
    target: &str,
) -> Result<(&'a NoteSpec, String), (&'static str, String)> {
    if let Some(note) = project.notes.iter().find(|note| {
        note.path == target
            || note.path.strip_suffix(".md") == Some(target)
            || note.note_id == target
    }) {
        return Ok((note, BASIS_EXACT_PATH.to_owned()));
    }
    if !target.contains('/') {
        let hits: Vec<&NoteSpec> = project
            .notes
            .iter()
            .filter(|note| note_stem(&note.path) == target)
            .collect();
        if hits.len() == 1 {
            return Ok((hits[0], BASIS_BARE_NAME.to_owned()));
        }
        if hits.len() > 1 {
            let names = hits
                .iter()
                .map(|note| note.path.clone())
                .collect::<Vec<_>>()
                .join("、");
            return Err((
                BASIS_DUPLICATE_NOTE_NAME,
                format!("目标 {target} 匹配到多份同名笔记：{names}"),
            ));
        }
    }
    Err((BASIS_PATH_MISSING, format!("发布集合里没有 {target} 这份笔记")))
}

fn resolve_heading<'a>(
    sections: &'a [Section],
    heading: &str,
) -> Result<&'a Section, (&'static str, String)> {
    let hits: Vec<&Section> = sections
        .iter()
        .filter(|section| section.title == heading)
        .collect();
    match hits.len() {
        1 => Ok(hits[0]),
        0 => Err((
            BASIS_HEADING_MISSING,
            format!("目标笔记里没有「{heading}」这一节"),
        )),
        _ => Err((
            BASIS_DUPLICATE_HEADING,
            format!(
                "目标笔记里有 {} 个同名「{heading}」标题（第 {} 行）",
                hits.len(),
                hits.iter()
                    .map(|section| section.line.to_string())
                    .collect::<Vec<_>>()
                    .join("、")
            ),
        )),
    }
}

pub(crate) fn evaluate(
    project: &Project,
    notes: &[NoteView<'_>],
    gold: &Gold,
) -> Result<Audit, ProjectError> {
    if gold.case_id != project.case_id {
        return Err(argument(format!(
            "金标准的 case_id ({}) 与工程声明 ({}) 不一致。",
            gold.case_id, project.case_id
        )));
    }
    for atom in &gold.atoms {
        if project.note(&atom.note_id).is_none() {
            return Err(argument(format!(
                "金标准原子 {} 指向未声明的笔记 {}。",
                atom.atom_id, atom.note_id
            )));
        }
    }

    let mut section_map: BTreeMap<&str, Vec<Section>> = BTreeMap::new();
    let mut bodies: BTreeMap<&str, &str> = BTreeMap::new();
    let mut all_markers: Vec<(&str, Marker)> = Vec::new();
    for view in notes {
        let found = sections(view.text);
        for marker in markers(view.text) {
            all_markers.push((view.spec.note_id.as_str(), marker));
        }
        section_map.insert(view.spec.note_id.as_str(), found);
        bodies.insert(view.spec.note_id.as_str(), view.text);
    }

    for (note_id, marker) in &all_markers {
        if gold.concept(&marker.concept_id).is_none() {
            return Err(argument(format!(
                "笔记 {note_id} 第 {} 行标记了未登记的概念 {}。",
                marker.line, marker.concept_id
            )));
        }
    }

    let mut links: Vec<LinkRecord> = Vec::new();
    let mut findings: Vec<Finding> = Vec::new();
    let mut link_total = 0usize;
    for view in notes {
        for raw in scan_links(view.text) {
            if raw.in_table && raw.alias.is_some() && !raw.escaped_alias {
                return Err(argument(format!(
                    "笔记 {} 第 {} 行的表格单元里，别名前的竖线没有转义：{}。表格里必须写成 \\|，否则单元会被提前切断。",
                    view.spec.note_id, raw.line, raw.raw
                )));
            }
            link_total += 1;
            if link_total > project.limits.max_links {
                return Err(argument(format!(
                    "链接数量超过上限 {}。",
                    project.limits.max_links
                )));
            }
            let (status, basis, note, heading) = match resolve_note(project, &raw.target) {
                Err((basis, detail)) => {
                    let status = if basis == BASIS_DUPLICATE_NOTE_NAME {
                        LINK_AMBIGUOUS
                    } else {
                        LINK_BROKEN
                    };
                    findings.push(Finding {
                        kind: if status == LINK_BROKEN {
                            FINDING_BROKEN_LINK.to_owned()
                        } else {
                            FINDING_AMBIGUOUS_LINK.to_owned()
                        },
                        severity: if status == LINK_BROKEN {
                            SEVERITY_CRITICAL.to_owned()
                        } else {
                            SEVERITY_GENERAL.to_owned()
                        },
                        subject: format!("{}:{}", view.spec.note_id, raw.line),
                        detail: format!("{}：{}", raw.raw, detail),
                    });
                    (status.to_owned(), basis.to_owned(), None, None)
                }
                Ok((target_note, basis)) => {
                    let target_id = target_note.note_id.clone();
                    match raw.heading.as_ref() {
                        None => (LINK_RESOLVED.to_owned(), basis, Some(target_id), None),
                        Some(wanted) => {
                            let found = section_map
                                .get(target_id.as_str())
                                .map(|value| value.as_slice())
                                .unwrap_or(&[]);
                            match resolve_heading(found, wanted) {
                                Ok(section) => (
                                    LINK_RESOLVED.to_owned(),
                                    basis,
                                    Some(target_id),
                                    Some(section.title.clone()),
                                ),
                                Err((basis, detail)) => {
                                    let status = if basis == BASIS_DUPLICATE_HEADING {
                                        LINK_AMBIGUOUS
                                    } else {
                                        LINK_BROKEN
                                    };
                                    findings.push(Finding {
                                        kind: if status == LINK_BROKEN {
                                            FINDING_BROKEN_LINK.to_owned()
                                        } else {
                                            FINDING_AMBIGUOUS_LINK.to_owned()
                                        },
                                        severity: if status == LINK_BROKEN {
                                            SEVERITY_CRITICAL.to_owned()
                                        } else {
                                            SEVERITY_GENERAL.to_owned()
                                        },
                                        subject: format!("{}:{}", view.spec.note_id, raw.line),
                                        detail: format!("{}：{}", raw.raw, detail),
                                    });
                                    (status.to_owned(), basis.to_owned(), Some(target_id), None)
                                }
                            }
                        }
                    }
                }
            };
            links.push(LinkRecord {
                note_id: view.spec.note_id.clone(),
                path: view.spec.path.clone(),
                line: raw.line,
                raw: raw.raw,
                target: raw.target,
                heading: raw.heading,
                alias: raw.alias,
                embed: raw.embed,
                in_table: raw.in_table,
                escaped_alias: raw.escaped_alias,
                status,
                basis,
                resolved_note: note,
                resolved_heading: heading,
            });
        }
    }

    let mut concepts: Vec<ConceptRecord> = Vec::new();
    for concept in &gold.concepts {
        let own: Vec<&(&str, Marker)> = all_markers
            .iter()
            .filter(|(_, marker)| marker.concept_id == concept.concept_id)
            .collect();
        let records: Vec<MarkerRecord> = own
            .iter()
            .map(|(note_id, marker)| MarkerRecord {
                note_id: (*note_id).to_owned(),
                heading: marker.heading.clone(),
                line: marker.line,
            })
            .collect();
        let unique = records.len() == 1;
        if records.is_empty() {
            findings.push(Finding {
                kind: FINDING_CONCEPT_UNEXPLAINED.to_owned(),
                severity: SEVERITY_CRITICAL.to_owned(),
                subject: concept.concept_id.clone(),
                detail: format!(
                    "概念「{}」在整个发布集合里没有任何 `{MARKER_PREFIX}{}` 标记。",
                    concept.canonical, concept.concept_id
                ),
            });
        } else if records.len() > 1 {
            let places = records
                .iter()
                .map(|record| format!("{}#{}（第 {} 行）", record.note_id, record.heading.clone().unwrap_or_default(), record.line))
                .collect::<Vec<_>>()
                .join("、");
            findings.push(Finding {
                kind: FINDING_DUPLICATE_EXPLANATION.to_owned(),
                severity: SEVERITY_CRITICAL.to_owned(),
                subject: concept.concept_id.clone(),
                detail: format!(
                    "概念「{}」在 {} 处都声明为完整解释：{}。同一概念只能有一处完整解释，其余应改为引用。",
                    concept.canonical,
                    records.len(),
                    places
                ),
            });
        }
        let canonical_note = records.first().map(|record| record.note_id.clone());
        let canonical_heading = records.first().and_then(|record| record.heading.clone());
        concepts.push(ConceptRecord {
            concept_id: concept.concept_id.clone(),
            canonical: concept.canonical.clone(),
            markers: records,
            canonical_note,
            canonical_heading,
            unique,
        });
    }

    // 原子的位置是"这条知识应当落在哪里"，不必等于概念的完整解释位置；但概念的
    // 完整解释必须落在金标准声明过的位置上，否则这条金标准并没有覆盖它自称的结论。
    for record in &concepts {
        let (Some(note), Some(heading)) = (
            record.canonical_note.as_deref(),
            record.canonical_heading.as_deref(),
        ) else {
            continue;
        };
        let covered = gold.atoms.iter().any(|atom| {
            atom.concept_id == record.concept_id && atom.note_id == note && atom.heading == heading
        });
        if !covered {
            findings.push(Finding {
                kind: FINDING_CANONICAL_MISMATCH.to_owned(),
                severity: SEVERITY_CRITICAL.to_owned(),
                subject: record.concept_id.clone(),
                detail: format!(
                    "概念「{}」的完整解释标记落在 {}#{}，但金标准没有在该位置声明任何原子：这个位置不在金标准覆盖范围内。",
                    record.canonical, note, heading
                ),
            });
        }
    }

    let mut atoms: Vec<AtomRecord> = Vec::new();
    for atom in &gold.atoms {
        let concept = concepts
            .iter()
            .find(|record| record.concept_id == atom.concept_id)
            .expect("gold validation guarantees the concept exists");
        let heading_exists = section_map
            .get(atom.note_id.as_str())
            .map(|value| value.iter().any(|section| section.title == atom.heading))
            .unwrap_or(false);
        let (recalled, reason) = if concept.markers.is_empty() {
            (false, "concept_unexplained".to_owned())
        } else if concept.markers.len() > 1 {
            (false, "duplicate_explanation".to_owned())
        } else if !heading_exists {
            (false, "heading_missing".to_owned())
        } else {
            (true, "recalled".to_owned())
        };
        if !recalled {
            findings.push(Finding {
                kind: FINDING_ATOM_MISSING.to_owned(),
                severity: if atom.key {
                    SEVERITY_CRITICAL.to_owned()
                } else {
                    SEVERITY_GENERAL.to_owned()
                },
                subject: atom.atom_id.clone(),
                detail: format!(
                    "原子 {}（概念 {}，{}）期望位置 {}#{}，实际结论：{}。",
                    atom.atom_id,
                    atom.concept_id,
                    if atom.key { "关键原子" } else { "一般原子" },
                    atom.note_id,
                    atom.heading,
                    reason
                ),
            });
        }
        atoms.push(AtomRecord {
            atom_id: atom.atom_id.clone(),
            concept_id: atom.concept_id.clone(),
            key: atom.key,
            note_id: atom.note_id.clone(),
            heading: atom.heading.clone(),
            recalled,
            reason,
        });
    }

    let key_total = atoms.iter().filter(|atom| atom.key).count();
    let key_hit = atoms.iter().filter(|atom| atom.key && atom.recalled).count();
    let general_total = atoms.len() - key_total;
    let general_hit = atoms.iter().filter(|atom| !atom.key && atom.recalled).count();
    let key_recall = ratio(key_hit, key_total, "NFR-02 要求关键原子 100%");
    let general_recall = ratio(
        general_hit,
        general_total,
        &format!("NFR-02 允许一般原子最多缺 {GENERAL_RECALL_PERCENT}% 以下的缺口"),
    );
    let general_ok = general_total == 0
        || general_hit * 100 >= general_total * GENERAL_RECALL_PERCENT;
    if !general_ok && general_total > 0 {
        findings.push(Finding {
            kind: FINDING_RECALL_BELOW.to_owned(),
            severity: SEVERITY_GENERAL.to_owned(),
            subject: "general".to_owned(),
            detail: format!(
                "一般原子召回 {general_hit}/{general_total}（{}），低于 {GENERAL_RECALL_PERCENT}% 的下限。",
                general_recall.display
            ),
        });
    }
    if key_total > 0 && key_hit < key_total {
        findings.push(Finding {
            kind: FINDING_RECALL_BELOW.to_owned(),
            severity: SEVERITY_CRITICAL.to_owned(),
            subject: "key".to_owned(),
            detail: format!(
                "关键原子召回 {key_hit}/{key_total}（{}），NFR-02 要求 100%。",
                key_recall.display
            ),
        });
    }

    let resolved = links
        .iter()
        .filter(|link| link.status == LINK_RESOLVED)
        .count();
    let broken = links
        .iter()
        .filter(|link| link.status == LINK_BROKEN)
        .count();
    let ambiguous = links
        .iter()
        .filter(|link| link.status == LINK_AMBIGUOUS)
        .count();
    let availability = ratio(
        resolved,
        links.len(),
        "NFR-05 要求本轮发布范围内链接有效率 100%",
    );
    let verdict = if broken == 0
        && ambiguous == 0
        && findings.iter().all(|finding| {
            finding.kind != FINDING_DUPLICATE_EXPLANATION
                && finding.kind != FINDING_CONCEPT_UNEXPLAINED
                && finding.kind != FINDING_CANONICAL_MISMATCH
        })
        && key_hit == key_total
        && general_ok
    {
        "pass".to_owned()
    } else {
        "findings".to_owned()
    };
    let counts = Counts {
        notes: notes.len(),
        headings: section_map.values().map(|value| value.len()).sum(),
        links: links.len(),
        links_resolved: resolved,
        links_broken: broken,
        links_ambiguous: ambiguous,
        links_embedded: links.iter().filter(|link| link.embed).count(),
        links_in_table: links.iter().filter(|link| link.in_table).count(),
        links_escaped_alias: links.iter().filter(|link| link.escaped_alias).count(),
        concepts: concepts.len(),
        markers: all_markers.len(),
        atoms: atoms.len(),
        atoms_key: key_total,
        atoms_general: general_total,
        atoms_recalled: key_hit + general_hit,
        atoms_missing: atoms.len() - key_hit - general_hit,
        duplicate_concepts: concepts.iter().filter(|record| record.markers.len() > 1).count(),
        findings: findings.len(),
        model_calls: 0,
        network_requests: 0,
    };
    let _ = bodies;
    Ok(Audit {
        links,
        concepts,
        atoms,
        findings,
        counts,
        availability,
        key_recall,
        general_recall,
        verdict,
    })
}

fn ratio(numerator: usize, denominator: usize, note: &str) -> Ratio {
    let applicable = denominator > 0;
    let display = if applicable {
        format!("{:.1}%", numerator as f64 * 100.0 / denominator as f64)
    } else {
        "不适用".to_owned()
    };
    Ratio {
        numerator,
        denominator,
        applicable,
        display,
        note: note.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// projection
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Summary {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) verdict: String,
    pub(crate) counts: Counts,
    pub(crate) availability: Ratio,
    pub(crate) key_recall: Ratio,
    pub(crate) general_recall: Ratio,
    pub(crate) findings: Vec<Finding>,
    pub(crate) model_calls: usize,
    pub(crate) network_requests: usize,
}

pub(crate) fn summarize(project: &Project, audit: &Audit) -> Summary {
    Summary {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: project.case_id.clone(),
        description: project.description.clone(),
        verdict: audit.verdict.clone(),
        counts: audit.counts.clone(),
        availability: audit.availability.clone(),
        key_recall: audit.key_recall.clone(),
        general_recall: audit.general_recall.clone(),
        findings: audit.findings.clone(),
        model_calls: 0,
        network_requests: 0,
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct LinksFile {
    pub(crate) policy: String,
    pub(crate) case_id: String,
    pub(crate) links: Vec<LinkRecord>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ExplanationsFile {
    pub(crate) policy: String,
    pub(crate) case_id: String,
    pub(crate) concepts: Vec<ConceptRecord>,
    pub(crate) atoms: Vec<AtomRecord>,
}

pub(crate) fn links_file(project: &Project, audit: &Audit) -> LinksFile {
    LinksFile {
        policy: POLICY.to_owned(),
        case_id: project.case_id.clone(),
        links: audit.links.clone(),
    }
}

pub(crate) fn explanations_file(project: &Project, audit: &Audit) -> ExplanationsFile {
    ExplanationsFile {
        policy: POLICY.to_owned(),
        case_id: project.case_id.clone(),
        concepts: audit.concepts.clone(),
        atoms: audit.atoms.clone(),
    }
}

pub(crate) fn status_text(audit: &Audit, case_id: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "审计结论：{}（case={case_id}）\n",
        verdict_text(&audit.verdict)
    ));
    out.push_str(&format!(
        "链接可用：{}/{}（{}），断链 {}、歧义 {}\n",
        audit.availability.numerator,
        audit.availability.denominator,
        audit.availability.display,
        audit.counts.links_broken,
        audit.counts.links_ambiguous
    ));
    out.push_str(&format!(
        "关键原子召回：{}/{}（{}）\n",
        audit.key_recall.numerator, audit.key_recall.denominator, audit.key_recall.display
    ));
    out.push_str(&format!(
        "一般原子召回：{}/{}（{}）\n",
        audit.general_recall.numerator,
        audit.general_recall.denominator,
        audit.general_recall.display
    ));
    out.push_str(&format!(
        "唯一解释位置：{} 个概念，{} 个标记，{} 个概念多处声明\n",
        audit.counts.concepts, audit.counts.markers, audit.counts.duplicate_concepts
    ));
    if audit.findings.is_empty() {
        out.push_str("缺口：0 条\n");
    } else {
        out.push_str(&format!("缺口：{} 条（逐条见报告）\n", audit.findings.len()));
        for finding in &audit.findings {
            out.push_str(&format!(
                "  - [{}] {}：{}\n",
                finding.severity, finding.kind, finding.subject
            ));
        }
    }
    out.push_str("模型调用与网络请求：0 次；只核对结构与位置，不判断内容质量。\n");
    out
}

pub(crate) fn verdict_text(verdict: &str) -> &'static str {
    match verdict {
        "pass" => "通过（链接全部可用、每个概念恰有一处完整解释、召回达标）",
        _ => "存在缺口（报告已写出，缺口逐条列名）",
    }
}

pub(crate) fn render_markdown(project: &Project, audit: &Audit) -> String {
    let mut out = String::new();
    out.push_str(&format!("# 全库链接与唯一解释位置审计（{}）\n\n", project.case_id));
    out.push_str(&format!("{}\n\n", project.description));
    out.push_str(&format!("## 结论\n\n- 审计结论：{}\n", verdict_text(&audit.verdict)));
    out.push_str(&format!(
        "- 模型调用与网络请求：0 次\n\n"
    ));
    out.push_str("## 口径\n\n| 指标 | 分子 | 分母 | 结果 | 不适用情形 |\n| --- | --- | --- | --- | --- |\n");
    out.push_str(&format!(
        "| 链接可用率 | {} | {} | {} | 发布集合内没有任何内部链接时显示「不适用」 |\n",
        audit.availability.numerator,
        audit.availability.denominator,
        audit.availability.display
    ));
    out.push_str(&format!(
        "| 关键原子召回 | {} | {} | {} | 金标准没有关键原子时显示「不适用」 |\n",
        audit.key_recall.numerator, audit.key_recall.denominator, audit.key_recall.display
    ));
    out.push_str(&format!(
        "| 一般原子召回 | {} | {} | {} | 金标准没有一般原子时显示「不适用」 |\n\n",
        audit.general_recall.numerator,
        audit.general_recall.denominator,
        audit.general_recall.display
    ));
    out.push_str("## 链接明细\n\n| 笔记 | 行 | 链接 | 结果 | 依据 | 解析到 |\n| --- | --- | --- | --- | --- | --- |\n");
    for link in &audit.links {
        let target = match (&link.resolved_note, &link.resolved_heading) {
            (Some(note), Some(heading)) => format!("{note}#{heading}"),
            (Some(note), None) => note.clone(),
            _ => "—".to_owned(),
        };
        out.push_str(&format!(
            "| {} | {} | `{}` | {} | {} | {} |\n",
            link.note_id,
            link.line,
            link.raw.replace('|', "\\|"),
            link.status,
            link.basis,
            target
        ));
    }
    out.push('\n');
    out.push_str("## 唯一解释位置\n\n| 概念 | 正名 | 完整解释位置 | 是否唯一 |\n| --- | --- | --- | --- |\n");
    for concept in &audit.concepts {
        let where_text = if concept.markers.is_empty() {
            "无".to_owned()
        } else {
            concept
                .markers
                .iter()
                .map(|record| {
                    format!(
                        "{}#{}（第 {} 行）",
                        record.note_id,
                        record.heading.clone().unwrap_or_default(),
                        record.line
                    )
                })
                .collect::<Vec<_>>()
                .join("；")
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            concept.concept_id,
            concept.canonical,
            where_text,
            if concept.unique { "是" } else { "否" }
        ));
    }
    out.push('\n');
    out.push_str("## 缺口\n\n");
    if audit.findings.is_empty() {
        out.push_str("- 无\n\n");
    } else {
        for finding in &audit.findings {
            out.push_str(&format!(
                "- [{}] {}（{}）：{}\n",
                finding.severity, finding.kind, finding.subject, finding.detail
            ));
        }
        out.push('\n');
    }
    out.push_str("## 边界\n\n");
    out.push_str("- 只核对结构与位置：链接是否唯一解析、每个概念是否恰有一处完整解释，不判断内容质量与科学正确性。\n");
    out.push_str("- 「完整解释」只认正文里显式写出的 `> 完整解释：<概念>` 标记；没有标记的段落不算声明，标记了也不会自动变成长度合格的讲解。\n");
    out.push_str("- 链接只在笔记与标题都唯一时才算可用；同名标题与同名笔记一律记为歧义，不取第一个匹配。\n");
    out.push_str("- 表格单元里的别名必须写成 `\\|`，未转义即整包拒绝：那是声明错误，不是审计结论。\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        Project {
            policy: POLICY.to_owned(),
            schema_version: SCHEMA_VERSION,
            producer: AUTHOR_PRODUCER.to_owned(),
            case_id: "aud-test".to_owned(),
            description: "test".to_owned(),
            gold: "gold.json".to_owned(),
            notes: vec![
                NoteSpec {
                    note_id: "note-a".to_owned(),
                    path: "笔记/甲.md".to_owned(),
                    title: "甲".to_owned(),
                },
                NoteSpec {
                    note_id: "note-b".to_owned(),
                    path: "笔记/乙.md".to_owned(),
                    title: "乙".to_owned(),
                },
            ],
            limits: Limits {
                max_notes: 8,
                max_atoms: 32,
                max_links: 32,
            },
        }
    }

    fn gold() -> Gold {
        Gold {
            policy: GOLD_POLICY.to_owned(),
            schema_version: SCHEMA_VERSION,
            producer: AUTHOR_PRODUCER.to_owned(),
            case_id: "aud-test".to_owned(),
            description: "test".to_owned(),
            concepts: vec![ConceptSpec {
                concept_id: "concept-a".to_owned(),
                canonical: "甲概念".to_owned(),
            }],
            atoms: vec![AtomSpec {
                atom_id: "atom-a".to_owned(),
                concept_id: "concept-a".to_owned(),
                key: true,
                note_id: "note-a".to_owned(),
                heading: "定义".to_owned(),
            }],
        }
    }

    #[test]
    fn a_section_owns_every_line_until_the_next_heading() {
        let text = "# 甲\n\n> 完整解释：concept-a\n## 定义\n正文\n## 其他\n正文\n";
        let found = sections(text);
        assert_eq!(found.len(), 3);
        assert_eq!((found[0].line, found[0].end_line), (1, 3));
        // 定义 节在第 4 行，其他 节在第 6 行：本节只拥有到下一标题前一行（第 5 行）。
        assert_eq!((found[1].line, found[1].end_line), (4, 5));
        assert_eq!((found[2].line, found[2].end_line), (6, 7));
    }

    #[test]
    fn a_marker_is_attributed_to_the_heading_it_sits_under() {
        let text = "# 甲\n> 完整解释：concept-a\n## 定义\n";
        let found = markers(text);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].heading.as_deref(), Some("甲"));
    }

    #[test]
    fn a_table_alias_must_escape_its_pipe() {
        let escaped = parse_link("笔记/乙#定义\\|乙的定义", 3, false, true);
        assert_eq!(escaped.target, "笔记/乙");
        assert_eq!(escaped.heading.as_deref(), Some("定义"));
        assert_eq!(escaped.alias.as_deref(), Some("乙的定义"));
        assert!(escaped.escaped_alias);
        let unescaped = parse_link("笔记/乙#定义|乙的定义", 3, false, true);
        assert!(!unescaped.escaped_alias);
    }

    #[test]
    fn a_heading_needs_a_space_after_the_hashes() {
        assert_eq!(heading_level("#没有空格"), 0);
        assert_eq!(heading_level("## 正常"), 2);
        assert_eq!(heading_level("    ### 缩进过深"), 0);
    }

    #[test]
    fn a_duplicated_heading_is_ambiguous_rather_than_first_match() {
        let found = sections("# 甲\n## 重名\n正文\n## 重名\n正文\n");
        let error = resolve_heading(&found, "重名").expect_err("must be ambiguous");
        assert_eq!(error.0, BASIS_DUPLICATE_HEADING);
    }

    #[test]
    fn evaluation_passes_when_everything_resolves() {
        let project = project();
        let gold = gold();
        let notes = vec![
            NoteView {
                spec: &project.notes[0],
                text: "# 甲\n## 定义\n> 完整解释：concept-a\n甲的定义\n\n见 [[笔记/乙#定义|乙]]\n",
            },
            NoteView {
                spec: &project.notes[1],
                text: "# 乙\n## 定义\n乙的定义\n",
            },
        ];
        let audit = evaluate(&project, &notes, &gold).expect("must evaluate");
        assert_eq!(audit.verdict, "pass");
        assert_eq!(audit.counts.links, 1);
        assert_eq!(audit.availability.display, "100.0%");
        assert_eq!(audit.key_recall.display, "100.0%");
    }

    #[test]
    fn a_missing_heading_is_a_broken_link_finding() {
        let project = project();
        let gold = gold();
        let notes = vec![
            NoteView {
                spec: &project.notes[0],
                text: "# 甲\n> 完整解释：concept-a\n## 定义\n见 [[笔记/乙#不存在]]\n",
            },
            NoteView {
                spec: &project.notes[1],
                text: "# 乙\n## 定义\n乙\n",
            },
        ];
        let audit = evaluate(&project, &notes, &gold).expect("must evaluate");
        assert_eq!(audit.verdict, "findings");
        assert_eq!(audit.counts.links_broken, 1);
        assert!(audit
            .findings
            .iter()
            .any(|finding| finding.kind == FINDING_BROKEN_LINK));
    }

    #[test]
    fn two_markers_for_one_concept_are_duplicates() {
        let project = project();
        let gold = gold();
        let notes = vec![
            NoteView {
                spec: &project.notes[0],
                text: "# 甲\n> 完整解释：concept-a\n## 定义\n甲\n",
            },
            NoteView {
                spec: &project.notes[1],
                text: "# 乙\n> 完整解释：concept-a\n## 定义\n乙\n",
            },
        ];
        let audit = evaluate(&project, &notes, &gold).expect("must evaluate");
        assert_eq!(audit.verdict, "findings");
        assert_eq!(audit.counts.duplicate_concepts, 1);
        assert_eq!(audit.counts.atoms_missing, 1);
        assert!(audit
            .findings
            .iter()
            .any(|finding| finding.kind == FINDING_DUPLICATE_EXPLANATION));
    }

    #[test]
    fn an_unknown_marker_concept_is_refused() {
        let project = project();
        let gold = gold();
        let notes = vec![NoteView {
            spec: &project.notes[0],
            text: "# 甲\n> 完整解释：concept-unknown\n## 定义\n",
        }];
        let error = evaluate(&project, &notes, &gold).expect_err("must refuse");
        assert_eq!(error.code(), "AUDIT_ARGUMENT");
    }

    #[test]
    fn a_ratio_with_a_zero_denominator_is_not_applicable() {
        let value = ratio(0, 0, "note");
        assert!(!value.applicable);
        assert_eq!(value.display, "不适用");
    }
}
