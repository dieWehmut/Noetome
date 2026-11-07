//! Deterministic chapter coarse alignment and a global concept inventory.
//!
//! This module is a pure planner: it reads frozen normalized documents and
//! their registered span lists, and derives a chapter tree, a concept list and
//! an alignment report. It never calls a model, never touches the network and
//! never rewrites a source. Concept identity comes from the author package's
//! declared lexicon, not from embeddings or similarity.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::project::ProjectError;
use crate::text::SpanKind;

pub(crate) const POLICY: &str = "outline-alignment-v1";
pub(crate) const CONCEPTS_POLICY: &str = "outline-concepts-v1";
pub(crate) const AUTHOR_PRODUCER: &str = "fixture_author";
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_DOCUMENTS: usize = 8;
pub(crate) const MAX_CONCEPTS: usize = 128;
pub(crate) const MAX_ALIASES: usize = 8;
pub(crate) const MAX_TITLE: usize = 200;
pub(crate) const MAX_ID: usize = 48;
pub(crate) const MAX_OCCURRENCES: usize = 4096;

pub(crate) const CONTEXT_HEADING: &str = "heading";
pub(crate) const CONTEXT_BODY: &str = "body";
pub(crate) const CONTEXT_CAPTION: &str = "caption";
pub(crate) const CONTEXT_APPENDIX: &str = "appendix";

pub(crate) const WARN_HEADING_WITHOUT_NUMBER: &str = "heading_without_number";
pub(crate) const WARN_LEVEL_JUMP: &str = "level_jump";
pub(crate) const WARN_DUPLICATE_NUMBER: &str = "duplicate_number";
pub(crate) const WARN_NUMBER_DEPTH: &str = "number_depth_mismatch";
pub(crate) const WARN_CAPTION_ONLY: &str = "concept_only_in_caption_or_appendix";
pub(crate) const WARN_HEADING_TERM_MISSING: &str = "heading_concept_missing_from_body";
pub(crate) const WARN_NO_HEADINGS: &str = "no_headings";

pub(crate) const WARNINGS: [&str; 7] = [
    WARN_HEADING_WITHOUT_NUMBER,
    WARN_LEVEL_JUMP,
    WARN_DUPLICATE_NUMBER,
    WARN_NUMBER_DEPTH,
    WARN_CAPTION_ONLY,
    WARN_HEADING_TERM_MISSING,
    WARN_NO_HEADINGS,
];

pub(crate) fn argument(text: impl Into<String>) -> ProjectError {
    ProjectError::new("OUTLINE_ARGUMENT", 2, text)
}

pub(crate) fn integrity(text: impl Into<String>) -> ProjectError {
    ProjectError::new("OUTLINE_INTEGRITY", 3, text)
}

pub(crate) fn unknown_root(text: impl Into<String>) -> ProjectError {
    ProjectError::new("OUTLINE_UNKNOWN_ROOT", 3, text)
}

/// A single registered span. The identifiers are derived at adopt time so the
/// manifest can be replayed without asking the source parser for consent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SpanRecord {
    pub(crate) span_id: String,
    pub(crate) kind: String,
    pub(crate) start_byte: usize,
    pub(crate) end_byte: usize,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) text_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentRecord {
    pub(crate) source_id: String,
    pub(crate) title: String,
    pub(crate) path: String,
    pub(crate) raw_sha256: String,
    pub(crate) byte_length: u64,
    pub(crate) normalized_sha256: String,
    pub(crate) span_count: usize,
    pub(crate) spans: Vec<SpanRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Inputs {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) documents: Vec<DocumentRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentSpec {
    pub(crate) source_id: String,
    pub(crate) path: String,
    pub(crate) title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Limits {
    pub(crate) max_documents: usize,
    pub(crate) max_concepts: usize,
    pub(crate) max_occurrences_per_concept: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Author {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) concepts: String,
    pub(crate) documents: Vec<DocumentSpec>,
    pub(crate) limits: Limits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Concept {
    pub(crate) concept_id: String,
    pub(crate) canonical: String,
    pub(crate) aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConceptFile {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) concepts: Vec<Concept>,
}

/// One row of the tree. Sections are flat; `parent` points at the enclosing
/// section of one level up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Section {
    pub(crate) section_id: String,
    pub(crate) document: String,
    pub(crate) number: Option<String>,
    pub(crate) level: u8,
    pub(crate) title: String,
    pub(crate) path: String,
    pub(crate) parent: Option<String>,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentTree {
    pub(crate) source_id: String,
    pub(crate) title: String,
    pub(crate) roots: Vec<String>,
    pub(crate) sections: Vec<Section>,
    pub(crate) aligned: usize,
    pub(crate) unaligned: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Tree {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) documents: Vec<DocumentTree>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Occurrence {
    pub(crate) concept_id: String,
    pub(crate) term: String,
    pub(crate) document: String,
    pub(crate) section: Option<String>,
    pub(crate) context: String,
    pub(crate) line: usize,
    pub(crate) byte: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConceptEntry {
    pub(crate) concept_id: String,
    pub(crate) canonical: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) total: usize,
    pub(crate) by_document: BTreeMap<String, usize>,
    pub(crate) contexts: BTreeMap<String, usize>,
    pub(crate) first: Option<Occurrence>,
    pub(crate) occurrences: Vec<Occurrence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConceptInventory {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) concepts: Vec<ConceptEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Warning {
    pub(crate) kind: String,
    pub(crate) document: String,
    pub(crate) section: Option<String>,
    pub(crate) line: usize,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Counts {
    pub(crate) documents: usize,
    pub(crate) sections: usize,
    pub(crate) sections_aligned: usize,
    pub(crate) sections_unaligned: usize,
    pub(crate) concepts: usize,
    pub(crate) occurrences: usize,
    pub(crate) warnings: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Report {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) counts: Counts,
    pub(crate) warning_counts: BTreeMap<String, usize>,
    pub(crate) warnings: Vec<Warning>,
    pub(crate) model_calls: u64,
    pub(crate) network_requests: u64,
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
    single(text, label, MAX_TITLE)?;
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

fn concepts_text(file: &ConceptFile) -> Result<(), ProjectError> {
    if file.concepts.len() > MAX_CONCEPTS {
        return Err(argument(format!(
            "概念词表最多 {MAX_CONCEPTS} 条；请拆分作者包。"
        )));
    }
    let mut seen = BTreeSet::new();
    for concept in &file.concepts {
        identifier(&concept.concept_id, "concept_id", MAX_ID)?;
        single(&concept.canonical, "canonical", MAX_TITLE)?;
        if concept.aliases.len() > MAX_ALIASES {
            return Err(argument(format!(
                "概念 {} 的别名最多 {MAX_ALIASES} 条。",
                concept.concept_id
            )));
        }
        for alias in &concept.aliases {
            single(alias, "alias", MAX_TITLE)?;
            if alias == &concept.canonical {
                return Err(argument(format!(
                    "概念 {} 的别名与正名重复。",
                    concept.concept_id
                )));
            }
        }
        if !seen.insert(concept.concept_id.as_str()) {
            return Err(argument(format!("概念 {} 重复出现。", concept.concept_id)));
        }
    }
    Ok(())
}

impl ConceptFile {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ProjectError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|error| integrity(format!("概念词表必须是UTF-8文本：{error}。")))?;
        let file: Self = serde_json::from_str(text)
            .map_err(|error| integrity(format!("概念词表不是严格JSON：{error}。")))?;
        file.validate()?;
        Ok(file)
    }

    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != CONCEPTS_POLICY {
            return Err(argument(format!(
                "概念词表 policy 必须是 {CONCEPTS_POLICY}；收到 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "概念词表 schema_version 必须是 {SCHEMA_VERSION}。"
            )));
        }
        if self.concepts.is_empty() {
            return Err(argument(
                "概念词表不能为空：本任务的全局概念清单由作者包声明的词表决定。",
            ));
        }
        concepts_text(self)
    }

    pub(crate) fn terms(&self) -> Vec<(String, String, String)> {
        let mut terms = Vec::new();
        for concept in &self.concepts {
            terms.push((
                concept.concept_id.clone(),
                concept.canonical.clone(),
                concept.canonical.clone(),
            ));
            for alias in &concept.aliases {
                terms.push((concept.concept_id.clone(), alias.clone(), alias.clone()));
            }
        }
        terms
    }
}

impl Limits {
    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if !(1..=MAX_DOCUMENTS).contains(&self.max_documents) {
            return Err(argument(format!(
                "limits.max_documents 必须在 1..={MAX_DOCUMENTS} 之间。"
            )));
        }
        if !(1..=MAX_CONCEPTS).contains(&self.max_concepts) {
            return Err(argument(format!(
                "limits.max_concepts 必须在 1..={MAX_CONCEPTS} 之间。"
            )));
        }
        if !(1..=MAX_OCCURRENCES).contains(&self.max_occurrences_per_concept) {
            return Err(argument(format!(
                "limits.max_occurrences_per_concept 必须在 1..={MAX_OCCURRENCES} 之间。"
            )));
        }
        Ok(())
    }
}

impl Author {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ProjectError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|error| integrity(format!("作者包必须是UTF-8文本：{error}。")))?;
        let author: Self = serde_json::from_str(text)
            .map_err(|error| integrity(format!("作者包不是严格JSON：{error}。")))?;
        author.validate()?;
        Ok(author)
    }

    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!(
                "作者包 policy 必须是 {POLICY}；收到 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "作者包 schema_version 必须是 {SCHEMA_VERSION}。"
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "作者包 producer 必须是 {AUTHOR_PRODUCER}；收到 {}。",
                self.producer
            )));
        }
        identifier(&self.case_id, "case_id", MAX_ID)?;
        single(&self.description, "description", 600)?;
        relative(&self.concepts, "concepts")?;
        if !self.concepts.ends_with(".json") {
            return Err(argument("concepts 必须指向一个 .json 文件。"));
        }
        self.limits.validate()?;
        if self.documents.is_empty() {
            return Err(argument("作者包至少要声明一份文档。"));
        }
        if self.documents.len() > self.limits.max_documents {
            return Err(argument(format!(
                "作者包声明了 {} 份文档，超过 limits.max_documents={}。",
                self.documents.len(),
                self.limits.max_documents
            )));
        }
        let mut sources = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for document in &self.documents {
            identifier(&document.source_id, "documents.source_id", MAX_ID)?;
            relative(&document.path, "documents.path")?;
            if !document.path.ends_with(".md") {
                return Err(argument(format!(
                    "文档 {} 的 path 必须以 .md 结尾：本任务只对齐已登记的 Markdown 来源。",
                    document.source_id
                )));
            }
            single(&document.title, "documents.title", MAX_TITLE)?;
            if !sources.insert(document.source_id.as_str()) {
                return Err(argument(format!("source_id {} 重复。", document.source_id)));
            }
            if !paths.insert(document.path.as_str()) {
                return Err(argument(format!("文档路径 {} 重复。", document.path)));
            }
        }
        Ok(())
    }
}

/// Split a heading title into an optional leading section number and the rest.
pub(crate) fn split_number(title: &str) -> (Option<String>, String) {
    let bytes = title.as_bytes();
    let mut index = 0;
    while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'.') {
        index += 1;
    }
    if index == 0 || !bytes[..index].iter().any(u8::is_ascii_digit) {
        return (None, title.to_owned());
    }
    let candidate: String = title[..index]
        .trim_end_matches('.')
        .trim_end_matches('、')
        .to_owned();
    if candidate.is_empty() {
        return (None, title.to_owned());
    }
    let rest = title[index..].trim_start_matches(['.', '、', ')', '）', ' ', '\u{3000}']);
    if rest.is_empty() {
        return (Some(candidate), String::new());
    }
    (Some(candidate), rest.to_owned())
}

pub(crate) fn number_depth(number: &str) -> usize {
    number
        .split('.')
        .filter(|segment| !segment.is_empty())
        .count()
}

pub(crate) fn heading_title(text: &str) -> String {
    let line = text.lines().next().unwrap_or(text).trim_end();
    let content = line.trim_start_matches(' ').trim_start_matches('#');
    content
        .trim_start_matches([' ', '\u{3000}'])
        .trim_end()
        .to_owned()
}

/// A heading is 1..=6 leading `#` followed by whitespace or the end of the line.
///
/// This must stay identical to `crate::text::heading`: the frozen `inputs.json`
/// decides which spans are headings, so a looser rule here would let the section
/// tree claim headings the span layer never marked (for example `#没有空格`).
fn heading_level(text: &str) -> u8 {
    let line = text.lines().next().unwrap_or(text);
    let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
    if indentation > 3 {
        return 0;
    }
    let content = &line[indentation..];
    let hashes = content.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&hashes) {
        return 0;
    }
    match content[hashes..].chars().next() {
        None => hashes as u8,
        Some(value) if value.is_whitespace() => hashes as u8,
        Some(_) => 0,
    }
}

fn caption_line(line: &str) -> bool {
    let trimmed = line.trim_start_matches([' ', '\t', '*', '_']);
    for prefix in ["图", "表", "附录", "Figure", "Fig.", "Table", "Appendix"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let next = rest.chars().next();
            if next.is_none()
                || next.is_some_and(|value| {
                    value.is_whitespace()
                        || value.is_ascii_digit()
                        || value == ':'
                        || value == '：'
                        || value == '.'
                        || value == '、'
                })
            {
                return true;
            }
        }
    }
    false
}

/// Find every occurrence of `needle` in `haystack`, comparing ASCII letters
/// case-insensitively while preserving byte offsets.
fn find_all(haystack: &str, needle: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    if needle.is_empty() || needle.len() > haystack.len() {
        return hits;
    }
    let hay = haystack.as_bytes();
    let needle = needle.as_bytes();
    let mut index = 0;
    while index + needle.len() <= hay.len() {
        let matched = hay[index..index + needle.len()]
            .iter()
            .zip(needle.iter())
            .all(|(left, right)| left.eq_ignore_ascii_case(right));
        if matched {
            if haystack.is_char_boundary(index) {
                hits.push(index);
            }
            index += 1;
        } else {
            index += 1;
        }
    }
    hits
}

fn line_of(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())]
        .bytes()
        .filter(|value| *value == b'\n')
        .count()
        + 1
}

fn line_text<'a>(text: &'a str, byte: usize) -> &'a str {
    let start = text[..byte.min(text.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let end = text[byte.min(text.len())..]
        .find('\n')
        .map_or(text.len(), |index| byte + index);
    &text[start..end]
}

#[derive(Clone)]
struct HeadingPoint {
    section_id: String,
    start_byte: usize,
    end_byte: usize,
}

pub(crate) struct DocumentView<'a> {
    pub(crate) record: &'a DocumentRecord,
    pub(crate) text: &'a str,
}

pub(crate) struct Alignment {
    pub(crate) tree: Vec<DocumentTree>,
    pub(crate) inventory: Vec<ConceptEntry>,
    pub(crate) warnings: Vec<Warning>,
}

/// Align every document and build the global concept inventory.
pub(crate) fn align(
    documents: &[DocumentView<'_>],
    concepts: &ConceptFile,
    max_occurrences: usize,
) -> Result<Alignment, ProjectError> {
    let mut trees = Vec::new();
    let mut warnings = Vec::new();
    let mut occurrences: BTreeMap<String, Vec<Occurrence>> = BTreeMap::new();
    for concept in &concepts.concepts {
        occurrences.insert(concept.concept_id.clone(), Vec::new());
    }

    for view in documents {
        let text = view.text;
        let mut sections: Vec<Section> = Vec::new();
        let mut points: Vec<HeadingPoint> = Vec::new();
        let mut stack: Vec<(u8, String)> = Vec::new();
        let mut numbers: BTreeMap<String, usize> = BTreeMap::new();
        let mut headings = 0usize;

        for span in &view.record.spans {
            if span.kind != SpanKind::Heading.as_str()
                || !text.is_char_boundary(span.start_byte)
                || !text.is_char_boundary(span.end_byte)
            {
                continue;
            }
            headings += 1;
            let slice = &text[span.start_byte..span.end_byte];
            let level = heading_level(slice);
            if level == 0 {
                continue;
            }
            let raw_title = heading_title(slice);
            let (number, title) = split_number(&raw_title);
            let section_id = format!("{}-{:04}", view.record.source_id, headings);
            while stack.last().is_some_and(|(depth, _)| *depth >= level) {
                stack.pop();
            }
            let parent = stack.last().map(|(_, id)| id.clone());
            if let Some(parent_id) = &parent {
                let parent_level = sections
                    .iter()
                    .find(|section| &section.section_id == parent_id)
                    .map(|section| section.level)
                    .unwrap_or(level);
                if level as i16 - parent_level as i16 > 1 {
                    warnings.push(Warning {
                        kind: WARN_LEVEL_JUMP.into(),
                        document: view.record.source_id.clone(),
                        section: Some(section_id.clone()),
                        line: span.start_line,
                        detail: format!(
                            "标题层级从 H{parent_level} 跳到 H{level}：父节 {} 下缺少中间层。",
                            sections
                                .iter()
                                .find(|section| &section.section_id == parent_id)
                                .map(|section| section.path.clone())
                                .unwrap_or_default()
                        ),
                    });
                }
            }
            if let Some(value) = &number {
                *numbers.entry(value.clone()).or_default() += 1;
            }
            let path = match &number {
                Some(value) if title.is_empty() => format!("{value}"),
                Some(value) => format!("{value} {title}"),
                None => raw_title.clone(),
            };
            sections.push(Section {
                section_id: section_id.clone(),
                document: view.record.source_id.clone(),
                number: number.clone(),
                level,
                title: title.clone(),
                path: path.clone(),
                parent: parent.clone(),
                start_line: span.start_line,
                end_line: span.end_line,
            });
            points.push(HeadingPoint {
                section_id: section_id.clone(),
                start_byte: span.start_byte,
                end_byte: span.end_byte,
            });
            stack.push((level, section_id));
        }

        if headings == 0 || sections.is_empty() {
            warnings.push(Warning {
                kind: WARN_NO_HEADINGS.into(),
                document: view.record.source_id.clone(),
                section: None,
                line: 1,
                detail: "文档没有任何标题：无法做章节粗对齐，内容全部留在根节点外。".into(),
            });
        }

        for section in &sections {
            if section.level == 1 {
                continue;
            }
            let Some(number) = &section.number else {
                warnings.push(Warning {
                    kind: WARN_HEADING_WITHOUT_NUMBER.into(),
                    document: section.document.clone(),
                    section: Some(section.section_id.clone()),
                    line: section.start_line,
                    detail: format!("章节「{}」没有编号。", section.path),
                });
                continue;
            };
            if numbers.get(number).copied().unwrap_or(0) > 1 {
                warnings.push(Warning {
                    kind: WARN_DUPLICATE_NUMBER.into(),
                    document: section.document.clone(),
                    section: Some(section.section_id.clone()),
                    line: section.start_line,
                    detail: format!("编号 {number} 在同一文档内出现多次。"),
                });
            }
            // Numbering conventions differ between books, so the rule is
            // relative: a subsection number must extend its parent's number by
            // exactly one segment. Absolute depth against the heading level
            // would flag every conventional `1.1` at H2.
            let parent_depth = section
                .parent
                .as_ref()
                .and_then(|id| sections.iter().find(|parent| &parent.section_id == id))
                .and_then(|parent| parent.number.as_ref())
                .map(|parent_number| number_depth(parent_number));
            if let Some(parent_depth) = parent_depth {
                let depth = number_depth(number);
                if depth != parent_depth + 1 {
                    warnings.push(Warning {
                        kind: WARN_NUMBER_DEPTH.into(),
                        document: section.document.clone(),
                        section: Some(section.section_id.clone()),
                        line: section.start_line,
                        detail: format!(
                            "编号 {number} 有 {depth} 段，但父节编号只有 {parent_depth} 段（期望 {} 段）。",
                            parent_depth + 1
                        ),
                    });
                }
            }
        }

        // A section owns every line up to the next heading; the heading span
        // alone would make "标题提到但正文没讲" unfalsifiable.
        let total_lines = text.lines().count().max(1);
        for index in 0..sections.len() {
            let end = sections
                .get(index + 1)
                .map(|next| next.start_line.saturating_sub(1))
                .unwrap_or(total_lines);
            let start = sections[index].start_line;
            sections[index].end_line = end.max(start);
        }

        let unaligned: BTreeSet<&str> = warnings
            .iter()
            .filter(|warning| warning.document == view.record.source_id)
            .filter_map(|warning| warning.section.as_deref())
            .collect();
        let aligned = sections
            .iter()
            .filter(|section| !unaligned.contains(section.section_id.as_str()))
            .count();
        let unaligned = sections.len() - aligned;

        // Concept occurrences for this document.
        let lower_terms: Vec<(String, String, String)> = concepts.terms();
        let mut found: Vec<(usize, usize, String, String)> = Vec::new(); // (byte, term order, concept, term)
        for (order, (concept_id, term, _)) in lower_terms.iter().enumerate() {
            for byte in find_all(text, term) {
                found.push((byte, order, concept_id.clone(), term.clone()));
            }
        }
        found.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| right.3.len().cmp(&left.3.len()))
                .then_with(|| left.1.cmp(&right.1))
        });
        // Keep the longest term at a given start offset; drop overlaps of the
        // same concept that start inside an already claimed term.
        let mut claimed: Vec<(usize, usize, String)> = Vec::new();
        for (byte, _, concept_id, term) in found {
            let end = byte + term.len();
            if claimed
                .iter()
                .any(|(start, stop, id)| *id == concept_id && byte < *stop && end > *start)
            {
                continue;
            }
            claimed.push((byte, end, concept_id.clone()));
            let line = line_of(text, byte);
            let in_heading = points
                .iter()
                .any(|point| point.start_byte <= byte && byte < point.end_byte);
            let section_id = points
                .iter()
                .filter(|point| point.start_byte <= byte)
                .max_by_key(|point| point.start_byte)
                .map(|point| point.section_id.clone());
            let appendix_section = section_id.as_ref().and_then(|id| {
                sections
                    .iter()
                    .find(|section| &section.section_id == id)
                    .filter(|section| {
                        section.path.contains("附录")
                            || section.title.to_ascii_lowercase().contains("appendix")
                    })
                    .map(|section| section.section_id.clone())
            });
            let context = if in_heading {
                CONTEXT_HEADING
            } else if caption_line(line_text(text, byte)) {
                CONTEXT_CAPTION
            } else if appendix_section.is_some() {
                CONTEXT_APPENDIX
            } else {
                CONTEXT_BODY
            };
            if let Some(list) = occurrences.get_mut(&concept_id) {
                list.push(Occurrence {
                    concept_id: concept_id.clone(),
                    term,
                    document: view.record.source_id.clone(),
                    section: section_id,
                    context: context.into(),
                    line,
                    byte,
                });
            }
        }

        trees.push(DocumentTree {
            source_id: view.record.source_id.clone(),
            title: view.record.title.clone(),
            roots: sections
                .iter()
                .filter(|section| section.parent.is_none())
                .map(|section| section.section_id.clone())
                .collect(),
            sections,
            aligned,
            unaligned,
        });
    }

    // Concepts named in a heading must be discussed in that section's body.
    for tree in &trees {
        for section in &tree.sections {
            let named: BTreeSet<String> = occurrences
                .iter()
                .flat_map(|(_, list)| list.iter())
                .filter(|occurrence| {
                    occurrence.document == tree.source_id
                        && occurrence.context == CONTEXT_HEADING
                        && occurrence.line == section.start_line
                })
                .map(|occurrence| occurrence.concept_id.clone())
                .collect();
            for concept_id in named {
                let discussed = occurrences.get(&concept_id).is_some_and(|list| {
                    list.iter().any(|occurrence| {
                        occurrence.document == tree.source_id
                            && occurrence.line >= section.start_line
                            && occurrence.line <= section.end_line
                            && occurrence.context != CONTEXT_HEADING
                    })
                });
                if !discussed {
                    warnings.push(Warning {
                        kind: WARN_HEADING_TERM_MISSING.into(),
                        document: tree.source_id.clone(),
                        section: Some(section.section_id.clone()),
                        line: section.start_line,
                        detail: format!(
                            "标题提到概念 {concept_id}，但本节正文没有再次出现：标题与正文不一致。"
                        ),
                    });
                }
            }
        }
    }

    let mut inventory = Vec::new();
    for concept in &concepts.concepts {
        let list = occurrences.remove(&concept.concept_id).unwrap_or_default();
        if list.len() > max_occurrences {
            return Err(argument(format!(
                "概念 {} 在冻结文档中出现 {} 次，超过 limits.max_occurrences_per_concept={max_occurrences}；请缩小文档范围。",
                concept.concept_id,
                list.len()
            )));
        }
        let mut by_document: BTreeMap<String, usize> = BTreeMap::new();
        let mut contexts: BTreeMap<String, usize> = BTreeMap::new();
        for occurrence in &list {
            *by_document.entry(occurrence.document.clone()).or_default() += 1;
            *contexts.entry(occurrence.context.clone()).or_default() += 1;
        }
        let only_outside_body = !list.is_empty()
            && list.iter().all(|occurrence| {
                occurrence.context == CONTEXT_CAPTION || occurrence.context == CONTEXT_APPENDIX
            });
        if only_outside_body {
            let first = list.first().cloned();
            warnings.push(Warning {
                kind: WARN_CAPTION_ONLY.into(),
                document: first
                    .as_ref()
                    .map(|occurrence| occurrence.document.clone())
                    .unwrap_or_default(),
                section: first
                    .as_ref()
                    .and_then(|occurrence| occurrence.section.clone()),
                line: first
                    .as_ref()
                    .map(|occurrence| occurrence.line)
                    .unwrap_or(1),
                detail: format!(
                    "概念 {} 只出现在图注或附录里，正文没有展开。",
                    concept.concept_id
                ),
            });
        }
        inventory.push(ConceptEntry {
            concept_id: concept.concept_id.clone(),
            canonical: concept.canonical.clone(),
            aliases: concept.aliases.clone(),
            total: list.len(),
            by_document,
            contexts,
            first: list.first().cloned(),
            occurrences: list,
        });
    }

    warnings.sort_by(|left, right| {
        left.document
            .cmp(&right.document)
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.detail.cmp(&right.detail))
    });

    if warnings.len() > MAX_OCCURRENCES {
        return Err(argument(format!(
            "对齐警告数量 {} 超过 {MAX_OCCURRENCES} 条；请缩小作者包范围。",
            warnings.len()
        )));
    }

    Ok(Alignment {
        tree: trees,
        inventory,
        warnings,
    })
}

pub(crate) fn build_tree(case_id: &str, alignment: &Alignment) -> Tree {
    Tree {
        policy: POLICY.into(),
        schema_version: SCHEMA_VERSION,
        case_id: case_id.into(),
        documents: alignment.tree.clone(),
    }
}

pub(crate) fn build_inventory(case_id: &str, alignment: &Alignment) -> ConceptInventory {
    ConceptInventory {
        policy: POLICY.into(),
        schema_version: SCHEMA_VERSION,
        case_id: case_id.into(),
        concepts: alignment.inventory.clone(),
    }
}

pub(crate) fn build_report(
    case_id: &str,
    tree: &Tree,
    inventory: &ConceptInventory,
    warnings: Vec<Warning>,
) -> Report {
    let sections: usize = tree
        .documents
        .iter()
        .map(|document| document.sections.len())
        .sum();
    let sections_aligned: usize = tree.documents.iter().map(|document| document.aligned).sum();
    let sections_unaligned: usize = tree
        .documents
        .iter()
        .map(|document| document.unaligned)
        .sum();
    let occurrences: usize = inventory.concepts.iter().map(|concept| concept.total).sum();
    let mut warning_counts = BTreeMap::new();
    for warning in &warnings {
        // Only the seven frozen kinds may reach a report.
        if !WARNINGS.contains(&warning.kind.as_str()) {
            continue;
        }
        *warning_counts.entry(warning.kind.clone()).or_default() += 1;
    }
    Report {
        policy: POLICY.into(),
        schema_version: SCHEMA_VERSION,
        case_id: case_id.into(),
        counts: Counts {
            documents: tree.documents.len(),
            sections,
            sections_aligned,
            sections_unaligned,
            concepts: inventory.concepts.len(),
            occurrences,
            warnings: warnings.len(),
        },
        warning_counts,
        warnings,
        model_calls: 0,
        network_requests: 0,
    }
}

pub(crate) fn render_markdown(
    tree: &Tree,
    inventory: &ConceptInventory,
    report: &Report,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# 章节对齐与全局概念清单（{}）\n\n",
        report.case_id
    ));
    out.push_str("## 章节对齐\n\n");
    out.push_str(&format!(
        "- 文档 {} 份；章节 {} 个；已对齐 {}、未对齐 {}。\n",
        report.counts.documents,
        report.counts.sections,
        report.counts.sections_aligned,
        report.counts.sections_unaligned
    ));
    out.push_str("\n| 文档 | 标题 | 章节 | 已对齐 | 未对齐 |\n| --- | --- | --- | --- | --- |\n");
    for document in &tree.documents {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            document.source_id,
            document.title,
            document.sections.len(),
            document.aligned,
            document.unaligned
        ));
    }
    out.push_str("\n## 全局概念清单\n\n");
    out.push_str(&format!(
        "- 概念 {} 个；出现 {} 次。\n\n",
        report.counts.concepts, report.counts.occurrences
    ));
    out.push_str("| 概念 | 正名 | 出现次数 | 文档数 | 首次出现 | 图注或附录 |\n| --- | --- | --- | --- | --- | --- |\n");
    for concept in &inventory.concepts {
        let first = concept
            .first
            .as_ref()
            .map(|occurrence| format!("{}:{}", occurrence.document, occurrence.line))
            .unwrap_or_else(|| "未出现".into());
        let outside = concept.contexts.get(CONTEXT_CAPTION).copied().unwrap_or(0)
            + concept.contexts.get(CONTEXT_APPENDIX).copied().unwrap_or(0);
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            concept.concept_id,
            concept.canonical,
            concept.total,
            concept.by_document.len(),
            first,
            outside
        ));
    }
    out.push_str("\n## 对齐警告\n\n");
    if report.warnings.is_empty() {
        out.push_str("- 没有对齐警告。\n");
    } else {
        let mut kinds: Vec<_> = report.warning_counts.iter().collect();
        kinds.sort();
        for (kind, count) in kinds {
            out.push_str(&format!("- {kind}：{count} 条\n"));
        }
        out.push_str("\n| 类型 | 文档 | 行 | 说明 |\n| --- | --- | --- | --- |\n");
        for warning in &report.warnings {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                warning.kind, warning.document, warning.line, warning.detail
            ));
        }
    }
    out.push_str("\n## 边界\n\n");
    out.push_str(
        "- 本报告由冻结文档与已登记 Span 确定性推导，零模型调用、零网络请求。\n\
         - 章节身份来自标题编号；概念身份来自作者包声明的词表与别名，不做语义相似度或实体识别。\n\
         - 未在词表中的词不会进入清单；警告只描述位置问题，不修改来源、不生成知识。\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_split_from_titles_with_chinese_punctuation() {
        assert_eq!(
            split_number("1.2 糖酵解净能量收支"),
            (Some("1.2".into()), "糖酵解净能量收支".into())
        );
        assert_eq!(
            split_number("2、糖异生"),
            (Some("2".into()), "糖异生".into())
        );
        assert_eq!(
            split_number("3.1.2） 三羧酸循环"),
            (Some("3.1.2".into()), "三羧酸循环".into())
        );
        assert_eq!(split_number("附录A 术语"), (None, "附录A 术语".into()));
        assert_eq!(split_number("糖酵解"), (None, "糖酵解".into()));
        assert_eq!(split_number("1. 概述"), (Some("1".into()), "概述".into()));
    }

    #[test]
    fn heading_levels_and_titles_ignore_deep_indentation() {
        assert_eq!(heading_level("### 2.1 糖酵解"), 3);
        assert_eq!(heading_level("    #### 太深"), 0);
        assert_eq!(heading_level("#没有空格"), 0);
        assert_eq!(heading_title("## 2.1 糖酵解\n"), "2.1 糖酵解");
        assert_eq!(heading_title("## 2.1 糖酵解"), "2.1 糖酵解");
    }

    #[test]
    fn number_depth_counts_non_empty_segments() {
        assert_eq!(number_depth("1"), 1);
        assert_eq!(number_depth("1.2"), 2);
        assert_eq!(number_depth("1..2"), 2);
    }

    #[test]
    fn ascii_case_insensitive_search_preserves_byte_offsets() {
        let text = "ATP 与 atp 与 aTp，中文不受影响";
        let hits = find_all(text, "atp");
        assert_eq!(hits.len(), 3);
        assert_eq!(&text[hits[0]..hits[0] + 3], "ATP");
        assert_eq!(&text[hits[1]..hits[1] + 3], "atp");
    }

    #[test]
    fn captions_are_recognised_by_leading_marker() {
        assert!(caption_line("图 3-1 糖酵解途径"));
        assert!(caption_line("**表 2** 能量对比"));
        assert!(caption_line("Table 4: yields"));
        assert!(!caption_line("糖酵解途径见图 3-1"));
        assert!(!caption_line("表格数据说明"));
    }
}
