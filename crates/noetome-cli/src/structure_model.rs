//! 同类结构一致性与导航复习组织（B10-T03）。
//!
//! 纯判定层：只读作者包里的笔记文本与声明，不碰磁盘、不联网、不调用模型。
//! 三件事：章节骨架一致性、概念的唯一规范解释位置、导航与复习区块的确定性生成。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::project::ProjectError;

pub(crate) const POLICY: &str = "structure-navigation-v1";
pub(crate) const CONCEPTS_POLICY: &str = "structure-concepts-v1";
pub(crate) const FAMILY_POLICY: &str = "structure-family-v1";
pub(crate) const AUTHOR_PRODUCER: &str = "fixture_author";
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_NOTES: usize = 6;
pub(crate) const MAX_SECTIONS: usize = 12;
pub(crate) const MAX_CONCEPTS: usize = 32;
pub(crate) const MAX_LINKS: usize = 64;
pub(crate) const MAX_REVIEWS: usize = 8;
pub(crate) const MAX_ID: usize = 48;
pub(crate) const MAX_TITLE: usize = 200;
pub(crate) const MAX_TEXT: usize = 600;

/// 骨架里的一节是必须出现还是可以省略。
pub(crate) const REQ_REQUIRED: &str = "required";
pub(crate) const REQ_OPTIONAL: &str = "optional";

/// 一节相对骨架的判定结果。
pub(crate) const STATUS_MATCHED: &str = "matched";
pub(crate) const STATUS_MISORDERED: &str = "misordered";
pub(crate) const STATUS_EXTRA: &str = "extra";
pub(crate) const STATUS_MISSING: &str = "missing";
pub(crate) const STATUS_EMPTY: &str = "empty";

/// 链接目标解析结果。
pub(crate) const LINK_RESOLVED: &str = "resolved";
pub(crate) const LINK_TARGET_MISSING: &str = "target_missing";
pub(crate) const LINK_HEADING_MISSING: &str = "heading_missing";
pub(crate) const LINK_AMBIGUOUS: &str = "ambiguous";

/// 一条引用在生成物里的处置方式。
pub(crate) const REF_LINKED: &str = "linked";
pub(crate) const REF_EXTERNAL: &str = "external";

pub(crate) fn argument(message: String) -> ProjectError {
    ProjectError::new("STRUCT_ARGUMENT", 2, message)
}

pub(crate) fn integrity(message: String) -> ProjectError {
    ProjectError::new("STRUCT_INTEGRITY", 3, message)
}

pub(crate) fn unknown_root(message: String) -> ProjectError {
    ProjectError::new("STRUCT_UNKNOWN_ROOT", 3, message)
}

pub(crate) fn duplicate_explanation(message: String) -> ProjectError {
    ProjectError::new("STRUCT_DUPLICATE_EXPLANATION", 3, message)
}

pub(crate) fn unknown_concept(message: String) -> ProjectError {
    ProjectError::new("STRUCT_UNKNOWN_CONCEPT", 3, message)
}

pub(crate) fn unescaped_alias(message: String) -> ProjectError {
    ProjectError::new("STRUCT_LINK_UNESCAPED_ALIAS", 3, message)
}

fn single(text: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    if text.trim() != text || text.is_empty() {
        return Err(argument(format!("{label} 不能为空，也不能有首尾空白。")));
    }
    if text.chars().count() > limit {
        return Err(argument(format!("{label} 超过 {limit} 个字符。")));
    }
    if text.chars().any(char::is_control) {
        return Err(argument(format!("{label} 不能包含控制字符。")));
    }
    Ok(())
}

fn identifier(text: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    single(text, label, limit)?;
    if !text
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
    {
        return Err(argument(format!(
            "{label} 只能包含ASCII字母、数字、连字符、下划线和点。"
        )));
    }
    Ok(())
}

fn relative(text: &str, label: &str) -> Result<(), ProjectError> {
    single(text, label, MAX_TEXT)?;
    if text.starts_with('/') || text.contains('\\') {
        return Err(argument(format!(
            "{label} 必须是相对路径，并使用 / 分隔：{text}。"
        )));
    }
    if text
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(argument(format!("{label} 不能包含空段、. 或 ..：{text}。")));
    }
    Ok(())
}

fn identifier_list(values: &[String], label: &str, limit: usize) -> Result<(), ProjectError> {
    if values.len() > limit {
        return Err(argument(format!(
            "{label} 最多 {limit} 项，实际 {} 项。",
            values.len()
        )));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        identifier(value, label, MAX_ID)?;
        if !seen.insert(value.as_str()) {
            return Err(argument(format!("{label} 出现重复项 {value}。")));
        }
    }
    Ok(())
}

/// 骨架里的一个位置。`requirement` 决定"缺了算什么"。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SkeletonSection {
    pub(crate) key: String,
    pub(crate) title: String,
    pub(crate) requirement: String,
}

/// 概念表：只声明术语，不做语义相似度。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Concept {
    pub(crate) concept_id: String,
    pub(crate) term: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConceptFile {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) concepts: Vec<Concept>,
}

impl ConceptFile {
    pub(crate) fn parse(text: &str) -> Result<Self, ProjectError> {
        serde_json::from_str(text).map_err(|error| argument(format!("概念表无法解析：{error}。")))
    }

    pub(crate) fn validate(&self, limit: usize) -> Result<(), ProjectError> {
        if self.policy != CONCEPTS_POLICY {
            return Err(argument(format!(
                "概念表的 policy 必须是 {CONCEPTS_POLICY}，实际 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "概念表的 schema_version 必须是 {SCHEMA_VERSION}，实际 {}。",
                self.schema_version
            )));
        }
        if self.concepts.is_empty() || self.concepts.len() > limit {
            return Err(argument(format!(
                "概念表必须声明 1 到 {limit} 个概念，实际 {} 个。",
                self.concepts.len()
            )));
        }
        let mut ids = BTreeSet::new();
        let mut terms = BTreeSet::new();
        for concept in &self.concepts {
            identifier(&concept.concept_id, "概念编号", MAX_ID)?;
            single(&concept.term, "概念术语", MAX_TITLE)?;
            if !ids.insert(concept.concept_id.as_str()) {
                return Err(argument(format!("概念编号重复：{}。", concept.concept_id)));
            }
            if !terms.insert(concept.term.as_str()) {
                return Err(argument(format!("概念术语重复：{}。", concept.term)));
            }
        }
        Ok(())
    }

    fn term_of(&self, concept_id: &str) -> Option<&str> {
        self.concepts
            .iter()
            .find(|concept| concept.concept_id == concept_id)
            .map(|concept| concept.term.as_str())
    }

    fn ids(&self) -> BTreeSet<&str> {
        self.concepts
            .iter()
            .map(|concept| concept.concept_id.as_str())
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoteSpec {
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) title: String,
    pub(crate) explains: Vec<String>,
    pub(crate) references: Vec<String>,
    pub(crate) reviews: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Limits {
    pub(crate) max_notes: usize,
    pub(crate) max_sections: usize,
    pub(crate) max_concepts: usize,
    pub(crate) max_links: usize,
}

impl Limits {
    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if !(1..=MAX_NOTES).contains(&self.max_notes) {
            return Err(argument(format!(
                "max_notes 必须在 1 到 {MAX_NOTES} 之间，实际 {}。",
                self.max_notes
            )));
        }
        if !(1..=MAX_SECTIONS).contains(&self.max_sections) {
            return Err(argument(format!(
                "max_sections 必须在 1 到 {MAX_SECTIONS} 之间，实际 {}。",
                self.max_sections
            )));
        }
        if !(1..=MAX_CONCEPTS).contains(&self.max_concepts) {
            return Err(argument(format!(
                "max_concepts 必须在 1 到 {MAX_CONCEPTS} 之间，实际 {}。",
                self.max_concepts
            )));
        }
        if !(1..=MAX_LINKS).contains(&self.max_links) {
            return Err(argument(format!(
                "max_links 必须在 1 到 {MAX_LINKS} 之间，实际 {}。",
                self.max_links
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Family {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) family: String,
    pub(crate) skeleton: Vec<SkeletonSection>,
    pub(crate) concepts: String,
    pub(crate) notes: Vec<NoteSpec>,
    pub(crate) limits: Limits,
}

impl Family {
    pub(crate) fn parse(text: &str) -> Result<Self, ProjectError> {
        serde_json::from_str(text).map_err(|error| argument(format!("作者包无法解析：{error}。")))
    }

    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != FAMILY_POLICY {
            return Err(argument(format!(
                "作者包的 policy 必须是 {FAMILY_POLICY}，实际 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "作者包的 schema_version 必须是 {SCHEMA_VERSION}，实际 {}。",
                self.schema_version
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "作者包的 producer 必须是 {AUTHOR_PRODUCER}，实际 {}。",
                self.producer
            )));
        }
        identifier(&self.case_id, "case_id", MAX_ID)?;
        single(&self.description, "描述", MAX_TEXT)?;
        single(&self.family, "主题族名称", MAX_TITLE)?;
        relative(&self.concepts, "概念表路径")?;
        if !self.concepts.ends_with(".json") {
            return Err(argument("概念表路径必须是 .json 文件。".to_owned()));
        }
        self.limits.validate()?;
        if self.skeleton.is_empty() || self.skeleton.len() > self.limits.max_sections {
            return Err(argument(format!(
                "骨架必须声明 1 到 {} 节，实际 {} 节。",
                self.limits.max_sections,
                self.skeleton.len()
            )));
        }
        let mut keys = BTreeSet::new();
        let mut titles = BTreeSet::new();
        for section in &self.skeleton {
            identifier(&section.key, "骨架键", MAX_ID)?;
            single(&section.title, "骨架标题", MAX_TITLE)?;
            if section.requirement != REQ_REQUIRED && section.requirement != REQ_OPTIONAL {
                return Err(argument(format!(
                    "骨架键 {} 的 requirement 必须是 {REQ_REQUIRED} 或 {REQ_OPTIONAL}，实际 {}。",
                    section.key, section.requirement
                )));
            }
            if !keys.insert(section.key.as_str()) {
                return Err(argument(format!("骨架键重复：{}。", section.key)));
            }
            if !titles.insert(section.title.as_str()) {
                return Err(argument(format!("骨架标题重复：{}。", section.title)));
            }
        }
        if self.notes.is_empty() || self.notes.len() > self.limits.max_notes {
            return Err(argument(format!(
                "笔记必须声明 1 到 {} 份，实际 {} 份。",
                self.limits.max_notes,
                self.notes.len()
            )));
        }
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for note in &self.notes {
            identifier(&note.note_id, "笔记编号", MAX_ID)?;
            relative(&note.path, "笔记路径")?;
            if !note.path.ends_with(".md") {
                return Err(argument(format!(
                    "笔记路径必须是 .md 文件：{}。",
                    note.path
                )));
            }
            single(&note.title, "笔记标题", MAX_TITLE)?;
            if !ids.insert(note.note_id.as_str()) {
                return Err(argument(format!("笔记编号重复：{}。", note.note_id)));
            }
            if !paths.insert(note.path.as_str()) {
                return Err(argument(format!("笔记路径重复：{}。", note.path)));
            }
            identifier_list(&note.explains, "笔记解释", self.limits.max_concepts)?;
            identifier_list(&note.references, "笔记引用", self.limits.max_concepts)?;
            if note.reviews.len() > MAX_REVIEWS {
                return Err(argument(format!(
                    "笔记 {} 的复习提示最多 {MAX_REVIEWS} 条，实际 {} 条。",
                    note.note_id,
                    note.reviews.len()
                )));
            }
            for review in &note.reviews {
                single(review, "复习提示", MAX_TEXT)?;
            }
        }
        Ok(())
    }
}

/// 笔记里扫到的一个标题。
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Heading {
    pub(crate) line: usize,
    pub(crate) level: u8,
    pub(crate) title: String,
    pub(crate) body_lines: usize,
    pub(crate) empty: bool,
}

/// 一份笔记的章节分析。
#[derive(Clone, Debug, Serialize)]
pub(crate) struct NoteSections {
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) title: String,
    pub(crate) headings: Vec<Heading>,
    /// How many heading lines the note has, at every level. The list above
    /// alone forces a caller to count; the count alone would hide which lines
    /// were recognised as headings.
    pub(crate) heading_count: usize,
    pub(crate) sections: Vec<String>,
    pub(crate) skeleton: Vec<SkeletonVerdict>,
    /// Titles that exist in the note but are not declared by the skeleton.
    /// Only the count would make `extra` unfalsifiable: a reader must be able
    /// to see which section is surplus and where it sits.
    pub(crate) extra_titles: Vec<String>,
    pub(crate) matched: usize,
    pub(crate) misordered: usize,
    pub(crate) extra: usize,
    pub(crate) missing_required: usize,
    pub(crate) omitted_optional: usize,
    pub(crate) empty: usize,
    pub(crate) consistent: bool,
}

/// 骨架里某一节在这份笔记里的判定。
#[derive(Clone, Debug, Serialize)]
pub(crate) struct SkeletonVerdict {
    pub(crate) key: String,
    pub(crate) title: String,
    pub(crate) requirement: String,
    pub(crate) status: String,
    pub(crate) position: Option<usize>,
}

/// 笔记正文里的一条 Wikilink。
#[derive(Clone, Debug, Serialize)]
pub(crate) struct SourceLink {
    pub(crate) note_id: String,
    pub(crate) line: usize,
    pub(crate) raw: String,
    pub(crate) target: String,
    pub(crate) heading: Option<String>,
    pub(crate) alias: Option<String>,
    pub(crate) escaped_alias: bool,
    pub(crate) in_table: bool,
    pub(crate) status: String,
    pub(crate) detail: String,
}

/// 概念在输出里的位置：恰好一处完整解释，其余引用。
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ConceptPlacement {
    pub(crate) concept_id: String,
    pub(crate) term: String,
    pub(crate) canonical: Option<String>,
    pub(crate) explaining: Vec<String>,
    pub(crate) referenced_by: Vec<String>,
}

/// 生成物里的一条链接（导航或概念引用）。
#[derive(Clone, Debug, Serialize)]
pub(crate) struct GeneratedLink {
    pub(crate) owner: String,
    pub(crate) purpose: String,
    pub(crate) text: String,
    pub(crate) target_path: String,
    pub(crate) target_heading: Option<String>,
    pub(crate) status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SectionCounts {
    pub(crate) notes: usize,
    pub(crate) skeleton: usize,
    pub(crate) sections: usize,
    pub(crate) matched: usize,
    pub(crate) misordered: usize,
    pub(crate) extra: usize,
    pub(crate) missing_required: usize,
    pub(crate) omitted_optional: usize,
    pub(crate) empty: usize,
    pub(crate) consistent_notes: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct LinkCounts {
    pub(crate) source_links: usize,
    pub(crate) source_resolved: usize,
    pub(crate) source_target_missing: usize,
    pub(crate) source_heading_missing: usize,
    pub(crate) source_ambiguous: usize,
    pub(crate) generated_links: usize,
    pub(crate) generated_broken: usize,
    pub(crate) external_references: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Counts {
    pub(crate) sections: SectionCounts,
    pub(crate) links: LinkCounts,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Structure {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) family: String,
    pub(crate) skeleton: Vec<SkeletonSection>,
    pub(crate) notes: Vec<NoteSections>,
    pub(crate) counts: SectionCounts,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Links {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) source: Vec<SourceLink>,
    pub(crate) generated: Vec<GeneratedLink>,
    pub(crate) counts: LinkCounts,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Report {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) family: String,
    pub(crate) counts: Counts,
    pub(crate) concepts: Vec<ConceptPlacement>,
    pub(crate) model_calls: u32,
    pub(crate) network_requests: u32,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Decision {
    pub(crate) structure: Structure,
    pub(crate) links: Links,
    pub(crate) report: Report,
    pub(crate) navigation: String,
    pub(crate) markdown: String,
}

/// 笔记文本的只读投影。
pub(crate) struct NoteView<'a> {
    pub(crate) spec: &'a NoteSpec,
    pub(crate) text: &'a str,
}

fn heading_level(line: &str) -> u8 {
    let content = match line.len() - line.trim_start().len() {
        indentation if indentation > 3 => return 0,
        indentation => &line[indentation..],
    };
    let hashes = content.chars().take_while(|value| *value == '#').count();
    if !(1..=6).contains(&hashes) {
        return 0;
    }
    match content[hashes..].chars().next() {
        None => hashes as u8,
        Some(value) if value.is_whitespace() => hashes as u8,
        Some(_) => 0,
    }
}

fn heading_title(line: &str) -> String {
    line.trim().trim_start_matches('#').trim().to_owned()
}

fn is_table_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// 扫标题并统计每节正文行数（`body_lines` 为 0 即空节）。
pub(crate) fn headings(text: &str) -> Vec<Heading> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found: Vec<(usize, u8, String)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let level = heading_level(line);
        if level > 0 {
            found.push((index + 1, level, heading_title(line)));
        }
    }
    let mut headings: Vec<Heading> = Vec::new();
    for (position, (line, level, title)) in found.iter().enumerate() {
        let end = found
            .get(position + 1)
            .map(|next| next.0.saturating_sub(1))
            .unwrap_or(lines.len());
        let body_lines = lines[*line..end]
            .iter()
            .filter(|value| !value.trim().is_empty())
            .count();
        headings.push(Heading {
            line: *line,
            level: *level,
            title: title.clone(),
            body_lines,
            empty: body_lines == 0,
        });
    }
    headings
}

/// 把 H2 当作骨架位置；更深的小节属于该节内部，不参与骨架比对。
fn skeleton_titles(headings: &[Heading]) -> Vec<String> {
    let shallow = headings.iter().filter(|heading| heading.level == 2).count();
    if shallow > 0 {
        return headings
            .iter()
            .filter(|heading| heading.level == 2)
            .map(|heading| heading.title.clone())
            .collect();
    }
    // 没有任何 H2 时退化为"所有标题都是平级"，否则单层笔记永远判成缺全部。
    headings
        .iter()
        .map(|heading| heading.title.clone())
        .collect()
}

fn verdicts(family: &Family, headings: &[Heading]) -> Vec<SkeletonVerdict> {
    let present = skeleton_titles(headings);
    let empty_of = |title: &str| {
        headings
            .iter()
            .find(|heading| heading.title == title)
            .is_some_and(|heading| heading.empty)
    };
    let mut out = Vec::new();
    // 乱序要在"正文里的出现顺序"上判断：比较骨架序号会把顺序完全正确的笔记
    // 也判成错位（骨架子集被跳过时序号会跳），所以只追踪已见的最大正文位置。
    let mut highest: Option<usize> = None;
    for section in family.skeleton.iter() {
        let position = present.iter().position(|title| *title == section.title);
        let status = match position {
            None => STATUS_MISSING,
            Some(seen) => {
                // 位置乱序不是"缺节"：两者是不同缺陷，必须分别报出来。
                let misplaced = highest.is_some_and(|top| seen < top);
                highest = Some(highest.map_or(seen, |top| top.max(seen)));
                if empty_of(&section.title) {
                    STATUS_EMPTY
                } else if misplaced {
                    STATUS_MISORDERED
                } else {
                    STATUS_MATCHED
                }
            }
        };
        out.push(SkeletonVerdict {
            key: section.key.clone(),
            title: section.title.clone(),
            requirement: section.requirement.clone(),
            status: status.to_owned(),
            position,
        });
    }
    out
}

fn extra_titles(family: &Family, headings: &[Heading]) -> Vec<String> {
    let declared: BTreeSet<&str> = family
        .skeleton
        .iter()
        .map(|section| section.title.as_str())
        .collect();
    skeleton_titles(headings)
        .into_iter()
        .filter(|title| !declared.contains(title.as_str()))
        .collect()
}

/// 找到正文里所有 `[[...]]`。
struct RawLink {
    line: usize,
    raw: String,
    inner: String,
    in_table: bool,
}

fn raw_links(text: &str) -> Vec<RawLink> {
    let mut out = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let table = is_table_row(line);
        let bytes = line.as_bytes();
        let mut cursor = 0;
        while cursor + 1 < bytes.len() {
            if bytes[cursor] == b'[' && bytes[cursor + 1] == b'[' {
                if let Some(offset) = line[cursor + 2..].find("]]") {
                    let start = cursor + 2;
                    let end = start + offset;
                    out.push(RawLink {
                        line: index + 1,
                        raw: line[cursor..end + 2].to_owned(),
                        inner: line[start..end].to_owned(),
                        in_table: table,
                    });
                    cursor = end + 2;
                    continue;
                }
                break;
            }
            cursor += 1;
        }
    }
    out
}

/// 拆分 `目标#标题|别名`：区分裸 `|` 与表格里要求的 `\|`。
fn split_link(inner: &str) -> (String, Option<String>, Option<String>, bool) {
    let mut alias_at: Option<(usize, bool)> = None;
    let bytes = inner.as_bytes();
    for (index, value) in bytes.iter().enumerate() {
        if *value == b'|' {
            let escaped = index > 0 && bytes[index - 1] == b'\\';
            alias_at = Some((index, escaped));
            break;
        }
    }
    let (body, alias, escaped) = match alias_at {
        None => (inner.to_owned(), None, false),
        Some((index, escaped)) => {
            let body = if escaped {
                inner[..index - 1].to_owned()
            } else {
                inner[..index].to_owned()
            };
            (body, Some(inner[index + 1..].to_owned()), escaped)
        }
    };
    match body.split_once('#') {
        Some((target, heading)) => (target.to_owned(), Some(heading.to_owned()), alias, escaped),
        None => (body, None, alias, escaped),
    }
}

/// 解析链接目标。返回的错误直接就是链接状态与说明：解析失败是链接缺陷，
/// 由 `scan_links` 记进报告，而不是把整包判成非法。
fn resolve_target<'a>(
    family: &'a Family,
    target: &str,
) -> Result<&'a NoteSpec, (&'static str, String)> {
    if target.is_empty() {
        return Err((LINK_TARGET_MISSING, "链接目标不能为空。".to_owned()));
    }
    let exact: Vec<&NoteSpec> = family
        .notes
        .iter()
        .filter(|note| note.path == target)
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0]);
    }
    let stem = target.strip_suffix(".md").unwrap_or(target);
    let by_stem: Vec<&NoteSpec> = family
        .notes
        .iter()
        .filter(|note| {
            let path_stem = note.path.strip_suffix(".md").unwrap_or(&note.path);
            path_stem == stem || path_stem.ends_with(&format!("/{stem}")) || note.title == stem
        })
        .collect();
    match by_stem.len() {
        1 => Ok(by_stem[0]),
        0 => Err((
            LINK_TARGET_MISSING,
            format!("链接目标 {target} 不在本主题族声明的笔记里。"),
        )),
        count => Err((
            LINK_AMBIGUOUS,
            format!("链接目标 {target} 在这份作者包里对应 {count} 份笔记，有歧义。"),
        )),
    }
}

fn scan_links(
    family: &Family,
    views: &[NoteView<'_>],
    max_links: usize,
) -> Result<Vec<SourceLink>, ProjectError> {
    let mut out: Vec<SourceLink> = Vec::new();
    for view in views {
        for link in raw_links(view.text) {
            let (target, heading, alias, escaped) = split_link(&link.inner);
            let mut status = LINK_RESOLVED.to_owned();
            let mut detail = String::new();
            if link.in_table && alias.is_some() && !escaped {
                return Err(unescaped_alias(format!(
                    "笔记 {} 第 {} 行在表格单元里写了未转义的别名竖线：{}。表格内必须写成 \\|，否则单元格会被截断。",
                    view.spec.path, link.line, link.raw
                )));
            }
            match resolve_target(family, &target) {
                Ok(note) => {
                    if let Some(wanted) = &heading {
                        let found = views
                            .iter()
                            .find(|candidate| candidate.spec.note_id == note.note_id)
                            .map(|candidate| headings(candidate.text))
                            .unwrap_or_default();
                        if !found.iter().any(|value| value.title == *wanted) {
                            status = LINK_HEADING_MISSING.to_owned();
                            detail = format!("目标笔记里没有标题「{wanted}」。");
                        }
                    }
                }
                Err((kind, message)) => {
                    status = kind.to_owned();
                    detail = message;
                }
            }
            out.push(SourceLink {
                note_id: view.spec.note_id.clone(),
                line: link.line,
                raw: link.raw,
                target,
                heading,
                alias,
                escaped_alias: escaped,
                in_table: link.in_table,
                status,
                detail,
            });
        }
    }
    if out.len() > max_links {
        return Err(argument(format!(
            "正文里的链接最多 {max_links} 条，实际 {} 条。",
            out.len()
        )));
    }
    Ok(out)
}

fn concept_table(
    family: &Family,
    concepts: &ConceptFile,
) -> Result<Vec<ConceptPlacement>, ProjectError> {
    let known = concepts.ids();
    for note in &family.notes {
        for concept_id in note.explains.iter().chain(note.references.iter()) {
            if !known.contains(concept_id.as_str()) {
                return Err(unknown_concept(format!(
                    "笔记 {} 用的概念 {concept_id} 不在概念表里；概念必须先登记再解释或引用。",
                    note.note_id
                )));
            }
        }
        let overlap: Vec<&String> = note
            .references
            .iter()
            .filter(|concept_id| note.explains.contains(concept_id))
            .collect();
        if let Some(concept_id) = overlap.first() {
            return Err(argument(format!(
                "笔记 {} 同时解释并引用了概念 {concept_id}；同一处不能既当规范位置又当引用。",
                note.note_id
            )));
        }
    }
    let mut out = Vec::new();
    for concept in &concepts.concepts {
        let explaining: Vec<String> = family
            .notes
            .iter()
            .filter(|note| note.explains.iter().any(|id| *id == concept.concept_id))
            .map(|note| note.note_id.clone())
            .collect();
        if explaining.len() > 1 {
            return Err(duplicate_explanation(format!(
                "概念 {} 被多处完整解释：{}。同一概念在一条输出版本里只能有一个规范位置。",
                concept.concept_id,
                explaining.join("、")
            )));
        }
        let referenced_by: Vec<String> = family
            .notes
            .iter()
            .filter(|note| note.references.iter().any(|id| *id == concept.concept_id))
            .map(|note| note.note_id.clone())
            .collect();
        out.push(ConceptPlacement {
            concept_id: concept.concept_id.clone(),
            term: concept.term.clone(),
            canonical: explaining.first().cloned(),
            explaining,
            referenced_by,
        });
    }
    Ok(out)
}

fn generated(
    family: &Family,
    concepts: &[ConceptPlacement],
    views: &[NoteView<'_>],
) -> Vec<GeneratedLink> {
    let mut out = Vec::new();
    for note in &family.notes {
        for sibling in &family.notes {
            if sibling.note_id == note.note_id {
                continue;
            }
            out.push(GeneratedLink {
                owner: note.note_id.clone(),
                purpose: "navigation".to_owned(),
                text: sibling.title.clone(),
                target_path: sibling.path.clone(),
                target_heading: None,
                status: LINK_RESOLVED.to_owned(),
            });
        }
    }
    for note in &family.notes {
        for concept_id in &note.references {
            let Some(concept) = concepts
                .iter()
                .find(|candidate| candidate.concept_id == *concept_id)
            else {
                continue;
            };
            let Some(canonical_id) = &concept.canonical else {
                continue;
            };
            let Some(target) = family
                .notes
                .iter()
                .find(|candidate| candidate.note_id == *canonical_id)
            else {
                continue;
            };
            out.push(GeneratedLink {
                owner: note.note_id.clone(),
                purpose: "concept".to_owned(),
                text: concept.term.clone(),
                target_path: target.path.clone(),
                // 规范位置是"哪一份笔记"，不是某个标题：链接只指文件，
                // 猜标题一旦猜错就是假链接（FR-17 明确禁止）。
                target_heading: None,
                status: LINK_RESOLVED.to_owned(),
            });
        }
    }
    let mut verified = Vec::new();
    for link in out {
        let text = views
            .iter()
            .find(|view| view.spec.path == link.target_path)
            .map(|view| view.text);
        let status = match (text, &link.target_heading) {
            (None, _) => LINK_TARGET_MISSING,
            (Some(_), None) => LINK_RESOLVED,
            (Some(body), Some(wanted)) => {
                if headings(body).iter().any(|value| value.title == *wanted) {
                    LINK_RESOLVED
                } else {
                    LINK_HEADING_MISSING
                }
            }
        };
        verified.push(GeneratedLink {
            status: status.to_owned(),
            ..link
        });
    }
    verified
}

/// 判断一份笔记是否"结构一致"：只有缺必填、错位、多余或空节才算偏离。
fn consistent(verdicts: &[SkeletonVerdict], extra: usize) -> bool {
    verdicts.iter().all(|verdict| {
        matches!(verdict.status.as_str(), STATUS_MATCHED)
            || (verdict.status == STATUS_MISSING && verdict.requirement == REQ_OPTIONAL)
    }) && extra == 0
}

pub(crate) fn decide(
    family: &Family,
    concepts: &ConceptFile,
    views: &[NoteView<'_>],
) -> Result<Decision, ProjectError> {
    let placements = concept_table(family, concepts)?;
    let mut notes = Vec::new();
    for view in views {
        let scanned = headings(view.text);
        let sections = skeleton_titles(&scanned);
        let verdicts = verdicts(family, &scanned);
        let extra = extra_titles(family, &scanned);

        let matched = verdicts
            .iter()
            .filter(|verdict| verdict.status == STATUS_MATCHED)
            .count();
        let misordered = verdicts
            .iter()
            .filter(|verdict| verdict.status == STATUS_MISORDERED)
            .count();
        let empty = verdicts
            .iter()
            .filter(|verdict| verdict.status == STATUS_EMPTY)
            .count();
        let missing_required = verdicts
            .iter()
            .filter(|verdict| {
                verdict.status == STATUS_MISSING && verdict.requirement == REQ_REQUIRED
            })
            .count();
        let omitted_optional = verdicts
            .iter()
            .filter(|verdict| {
                verdict.status == STATUS_MISSING && verdict.requirement == REQ_OPTIONAL
            })
            .count();
        let ok = consistent(&verdicts, extra.len());
        notes.push(NoteSections {
            note_id: view.spec.note_id.clone(),
            path: view.spec.path.clone(),
            title: view.spec.title.clone(),
            heading_count: scanned.len(),
            headings: scanned,
            sections,
            skeleton: verdicts,
            extra: extra.len(),
            extra_titles: extra,
            matched,
            misordered,
            missing_required,
            omitted_optional,
            empty,
            consistent: ok,
        });
    }
    let section_counts = SectionCounts {
        notes: notes.len(),
        skeleton: family.skeleton.len(),
        sections: notes.iter().map(|note| note.sections.len()).sum(),
        matched: notes.iter().map(|note| note.matched).sum(),
        misordered: notes.iter().map(|note| note.misordered).sum(),
        extra: notes.iter().map(|note| note.extra).sum(),
        missing_required: notes.iter().map(|note| note.missing_required).sum(),
        omitted_optional: notes.iter().map(|note| note.omitted_optional).sum(),
        empty: notes.iter().map(|note| note.empty).sum(),
        consistent_notes: notes.iter().filter(|note| note.consistent).count(),
    };
    let source = scan_links(family, views, family.limits.max_links)?;
    let generated_links = generated(family, &placements, views);
    let external_references = placements
        .iter()
        .filter(|placement| placement.canonical.is_none() && !placement.referenced_by.is_empty())
        .count();
    let link_counts = LinkCounts {
        source_links: source.len(),
        source_resolved: source
            .iter()
            .filter(|link| link.status == LINK_RESOLVED)
            .count(),
        source_target_missing: source
            .iter()
            .filter(|link| link.status == LINK_TARGET_MISSING)
            .count(),
        source_heading_missing: source
            .iter()
            .filter(|link| link.status == LINK_HEADING_MISSING)
            .count(),
        source_ambiguous: source
            .iter()
            .filter(|link| link.status == LINK_AMBIGUOUS)
            .count(),
        generated_links: generated_links.len(),
        generated_broken: generated_links
            .iter()
            .filter(|link| link.status != LINK_RESOLVED)
            .count(),
        external_references,
    };
    let counts = Counts {
        sections: section_counts.clone(),
        links: link_counts.clone(),
    };
    let structure = Structure {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: family.case_id.clone(),
        family: family.family.clone(),
        skeleton: family.skeleton.clone(),
        notes,
        counts: section_counts,
    };
    let links = Links {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: family.case_id.clone(),
        source,
        generated: generated_links,
        counts: link_counts,
    };
    let report = Report {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: family.case_id.clone(),
        family: family.family.clone(),
        counts,
        concepts: placements,
        model_calls: 0,
        network_requests: 0,
    };
    let navigation = render_navigation(family, &structure);
    let markdown = render_markdown(&report, &structure, &links);
    Ok(Decision {
        structure,
        links,
        report,
        navigation,
        markdown,
    })
}

fn escape_alias(text: &str) -> String {
    text.replace('|', "\\|")
}

fn status_text(status: &str) -> &'static str {
    match status {
        STATUS_MATCHED => "就位",
        STATUS_MISORDERED => "位置偏离",
        STATUS_EXTRA => "多余",
        STATUS_MISSING => "缺失",
        STATUS_EMPTY => "空节",
        _ => "未判定",
    }
}

fn render_navigation(family: &Family, structure: &Structure) -> String {
    let mut out = String::new();
    out.push_str(&format!("# 导航与复习（{}）\n\n", family.family));
    out.push_str("由确定性规则生成：只用作者包声明的笔记路径、标题与复习提示，不生成外部链接，不猜测考试频次。\n\n");
    for note in &family.notes {
        let Some(sections) = structure
            .notes
            .iter()
            .find(|candidate| candidate.note_id == note.note_id)
        else {
            continue;
        };
        out.push_str(&format!("## {}\n\n", note.title));
        out.push_str(&format!("位置：`{}`\n\n", note.path));
        out.push_str("### 同族导航\n\n");
        let mut siblings = 0;
        for sibling in &family.notes {
            if sibling.note_id == note.note_id {
                continue;
            }
            siblings += 1;
            out.push_str(&format!(
                "- [[{}|{}]]\n",
                sibling.path,
                escape_alias(&sibling.title)
            ));
        }
        if siblings == 0 {
            out.push_str("- （本族只有这一份笔记）\n");
        }
        out.push('\n');
        out.push_str("### 章节骨架\n\n");
        for verdict in &sections.skeleton {
            out.push_str(&format!(
                "- {}：{}（{}）\n",
                verdict.title,
                status_text(&verdict.status),
                verdict.requirement
            ));
        }
        for title in &sections.extra_titles {
            out.push_str(&format!("- {title}：多余（骨架未声明）\n"));
        }
        out.push('\n');
        out.push_str("### 复习提示\n\n");
        if note.reviews.is_empty() {
            out.push_str("- （未声明复习提示）\n");
        }
        for review in &note.reviews {
            out.push_str(&format!("- {review}\n"));
        }
        out.push('\n');
    }
    out.push_str("## 边界\n\n");
    out.push_str("- 导航只指向本族已声明笔记，不生成库外假设目标。\n");
    out.push_str("- 复习提示来自作者声明，不虚构考试频次或出题概率。\n");
    out
}

fn render_markdown(report: &Report, structure: &Structure, links: &Links) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# 同类结构一致性与导航复习（{}）\n\n",
        report.case_id
    ));
    out.push_str(&format!(
        "主题族：{}；骨架 {} 节。\n\n",
        report.family, report.counts.sections.skeleton
    ));
    out.push_str("## 结构一致性\n\n");
    out.push_str("| 笔记 | 章节 | 就位 | 错位 | 多余 | 缺必填 | 省可选 | 空节 | 一致 |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for note in &structure.notes {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            note.title,
            note.sections.len(),
            note.matched,
            note.misordered,
            note.extra,
            note.missing_required,
            note.omitted_optional,
            note.empty,
            if note.consistent { "是" } else { "否" }
        ));
    }
    out.push_str(&format!(
        "\n合计：{} 份笔记、{} 个骨架节、{} 个章节；一致 {} 份。\n\n",
        report.counts.sections.notes,
        report.counts.sections.skeleton,
        report.counts.sections.sections,
        report.counts.sections.consistent_notes
    ));
    out.push_str("## 结构偏离明细\n\n");
    out.push_str("| 笔记 | 节 | 结果 | 位置 |\n| --- | --- | --- | --- |\n");
    let mut deviations = 0usize;
    for note in &structure.notes {
        for verdict in &note.skeleton {
            if verdict.status == STATUS_MATCHED {
                continue;
            }
            deviations += 1;
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                note.title,
                verdict.title,
                status_text(&verdict.status),
                match verdict.position {
                    Some(position) => format!("第 {} 节", position + 1),
                    None => "（缺失）".to_owned(),
                }
            ));
        }
        for title in &note.extra_titles {
            deviations += 1;
            out.push_str(&format!(
                "| {} | {} | 多余（骨架未声明） | （不在骨架内） |\n",
                note.title, title
            ));
        }
    }
    if deviations == 0 {
        out.push_str("| （无偏离） | | | |\n");
    }
    out.push_str(&format!(
        "\n合计 {} 处偏离，只是报告位置，不自动改写正文、不静默补齐。\n\n",
        deviations
    ));
    out.push_str("## 概念的唯一解释位置\n\n");
    out.push_str("| 概念 | 规范位置 | 引用方 |\n| --- | --- | --- |\n");
    for concept in &report.concepts {
        let canonical = concept
            .canonical
            .clone()
            .unwrap_or_else(|| "（无本地完整解释）".to_owned());
        let mut referenced = concept.referenced_by.clone();
        referenced.sort();
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            concept.term,
            canonical,
            if referenced.is_empty() {
                "（无）".to_owned()
            } else {
                referenced.join("、")
            }
        ));
    }
    out.push_str("\n## 链接\n\n");
    out.push_str(&format!(
        "- 来源正文链接 {} 条：可解析 {}、目标缺失 {}、标题缺失 {}、有歧义 {}。\n",
        report.counts.links.source_links,
        report.counts.links.source_resolved,
        report.counts.links.source_target_missing,
        report.counts.links.source_heading_missing,
        report.counts.links.source_ambiguous
    ));
    out.push_str(&format!(
        "- 生成链接 {} 条：断链 {} 条；未本地解释的外部引用 {} 个。\n",
        report.counts.links.generated_links,
        report.counts.links.generated_broken,
        report.counts.links.external_references
    ));
    if !links.source.is_empty() {
        out.push_str("\n| 笔记 | 行 | 链接 | 结果 |\n| --- | --- | --- | --- |\n");
        for link in links
            .source
            .iter()
            .filter(|link| link.status != LINK_RESOLVED)
        {
            out.push_str(&format!(
                "| {} | {} | `{}` | {} |\n",
                link.note_id, link.line, link.raw, link.status
            ));
        }
    }
    out.push_str("\n## 边界\n\n");
    out.push_str("- 只报告结构偏离与链接缺陷，不自动改写正文、不静默补齐缺失章节。\n");
    out.push_str("- 生成物里的链接全部指向本族已声明笔记与标题，断链必须为 0。\n");
    out.push_str("- 模型调用与网络请求 0 次；不判断内容质量与科学正确性。\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skeleton() -> Vec<SkeletonSection> {
        vec![
            SkeletonSection {
                key: "definition".to_owned(),
                title: "定义".to_owned(),
                requirement: REQ_REQUIRED.to_owned(),
            },
            SkeletonSection {
                key: "mechanism".to_owned(),
                title: "核心机制".to_owned(),
                requirement: REQ_REQUIRED.to_owned(),
            },
            SkeletonSection {
                key: "exception".to_owned(),
                title: "条件与例外".to_owned(),
                requirement: REQ_REQUIRED.to_owned(),
            },
        ]
    }

    fn family(notes: Vec<NoteSpec>) -> Family {
        Family {
            policy: FAMILY_POLICY.to_owned(),
            schema_version: SCHEMA_VERSION,
            producer: AUTHOR_PRODUCER.to_owned(),
            case_id: "case".to_owned(),
            description: "描述".to_owned(),
            family: "族".to_owned(),
            skeleton: skeleton(),
            concepts: "concepts.json".to_owned(),
            notes,
            limits: Limits {
                max_notes: 4,
                max_sections: 8,
                max_concepts: 8,
                max_links: 16,
            },
        }
    }

    fn note(id: &str, path: &str, explains: &[&str], references: &[&str]) -> NoteSpec {
        NoteSpec {
            note_id: id.to_owned(),
            path: path.to_owned(),
            title: id.to_owned(),
            explains: explains.iter().map(|value| (*value).to_owned()).collect(),
            references: references.iter().map(|value| (*value).to_owned()).collect(),
            reviews: vec!["复习一句".to_owned()],
        }
    }

    fn concepts() -> ConceptFile {
        ConceptFile {
            policy: CONCEPTS_POLICY.to_owned(),
            schema_version: SCHEMA_VERSION,
            concepts: vec![
                Concept {
                    concept_id: "a".to_owned(),
                    term: "概念甲".to_owned(),
                },
                Concept {
                    concept_id: "b".to_owned(),
                    term: "概念乙".to_owned(),
                },
            ],
        }
    }

    const GOOD: &str = "# 甲\n\n## 定义\n正文。\n\n## 核心机制\n正文。\n\n## 条件与例外\n正文。\n";

    #[test]
    fn a_matching_note_is_consistent_and_a_missing_required_section_is_not() {
        let family = family(vec![note("n1", "a.md", &["a"], &[])]);
        let decision = decide(
            &family,
            &concepts(),
            &[NoteView {
                spec: &family.notes[0],
                text: GOOD,
            }],
        )
        .expect("应当通过");
        assert_eq!(decision.structure.counts.matched, 3);
        assert_eq!(decision.structure.counts.consistent_notes, 1);

        let broken = "# 甲\n\n## 定义\n正文。\n\n## 核心机制\n正文。\n";
        let error = decide(
            &family,
            &concepts(),
            &[NoteView {
                spec: &family.notes[0],
                text: broken,
            }],
        );
        let decision = error.expect("缺必填不是错误，是偏离");
        assert_eq!(decision.structure.counts.missing_required, 1);
        assert_eq!(decision.structure.counts.consistent_notes, 0);
    }

    #[test]
    fn an_out_of_order_section_is_reported_rather_than_reordered() {
        let text = "# 甲\n\n## 定义\n正文。\n\n## 条件与例外\n正文。\n\n## 核心机制\n正文。\n";
        let family = family(vec![note("n1", "a.md", &["a"], &[])]);
        let decision = decide(
            &family,
            &concepts(),
            &[NoteView {
                spec: &family.notes[0],
                text,
            }],
        )
        .expect("应当通过");
        assert_eq!(decision.structure.counts.misordered, 1);
        assert_eq!(
            decision.structure.notes[0].sections,
            vec!["定义", "条件与例外", "核心机制"]
        );
    }

    #[test]
    fn an_empty_section_is_reported_even_when_the_title_is_in_place() {
        let text = "# 甲\n\n## 定义\n\n## 核心机制\n正文。\n\n## 条件与例外\n正文。\n";
        let family = family(vec![note("n1", "a.md", &["a"], &[])]);
        let decision = decide(
            &family,
            &concepts(),
            &[NoteView {
                spec: &family.notes[0],
                text,
            }],
        )
        .expect("应当通过");
        assert_eq!(decision.structure.counts.empty, 1);
        assert_eq!(decision.structure.counts.consistent_notes, 0);
    }

    #[test]
    fn a_concept_explained_twice_is_refused() {
        let family = family(vec![
            note("n1", "a.md", &["a"], &[]),
            note("n2", "b.md", &["a"], &[]),
        ]);
        let error = decide(
            &family,
            &concepts(),
            &[
                NoteView {
                    spec: &family.notes[0],
                    text: GOOD,
                },
                NoteView {
                    spec: &family.notes[1],
                    text: GOOD,
                },
            ],
        )
        .expect_err("不得两处完整解释");
        assert_eq!(error.code(), "STRUCT_DUPLICATE_EXPLANATION");
    }

    #[test]
    fn a_reference_to_an_unexplained_concept_stays_external_and_never_fakes_a_link() {
        let family = family(vec![
            note("n1", "a.md", &["a"], &[]),
            note("n2", "b.md", &[], &["b"]),
        ]);
        let decision = decide(
            &family,
            &concepts(),
            &[
                NoteView {
                    spec: &family.notes[0],
                    text: GOOD,
                },
                NoteView {
                    spec: &family.notes[1],
                    text: GOOD,
                },
            ],
        )
        .expect("应当通过");
        assert_eq!(decision.report.counts.links.external_references, 1);
        assert!(
            decision
                .links
                .generated
                .iter()
                .all(|link| link.purpose != "concept")
        );
    }

    #[test]
    fn an_unescaped_alias_inside_a_table_is_refused() {
        let text = "# 甲\n\n## 定义\n正文。\n\n| 列 | 值 |\n| --- | --- |\n| 对照 | [[a.md#定义|裸竖线]] |\n\n## 核心机制\n正文。\n\n## 条件与例外\n正文。\n";
        let family = family(vec![
            note("n1", "a.md", &["a"], &[]),
            note("n2", "b.md", &[], &[]),
        ]);
        let error = decide(
            &family,
            &concepts(),
            &[
                NoteView {
                    spec: &family.notes[0],
                    text,
                },
                NoteView {
                    spec: &family.notes[1],
                    text: GOOD,
                },
            ],
        )
        .expect_err("表格内裸竖线必须拒绝");
        assert_eq!(error.code(), "STRUCT_LINK_UNESCAPED_ALIAS");
    }

    #[test]
    fn an_escaped_alias_inside_a_table_is_accepted() {
        let text = "# 甲\n\n## 定义\n正文。\n\n| 列 | 值 |\n| --- | --- |\n| 对照 | [[b.md#定义\\|别名]] |\n\n## 核心机制\n正文。\n\n## 条件与例外\n正文。\n";
        let family = family(vec![
            note("n1", "a.md", &["a"], &[]),
            note("n2", "b.md", &[], &[]),
        ]);
        let decision = decide(
            &family,
            &concepts(),
            &[
                NoteView {
                    spec: &family.notes[0],
                    text,
                },
                NoteView {
                    spec: &family.notes[1],
                    text: GOOD,
                },
            ],
        )
        .expect("应当通过");
        assert_eq!(decision.report.counts.links.source_resolved, 1);
        assert!(decision.links.source[0].escaped_alias);
    }

    #[test]
    fn generated_links_stay_inside_the_family_and_never_break() {
        let family = family(vec![
            note("n1", "a.md", &["a"], &[]),
            note("n2", "b.md", &[], &["a"]),
        ]);
        let decision = decide(
            &family,
            &concepts(),
            &[
                NoteView {
                    spec: &family.notes[0],
                    text: GOOD,
                },
                NoteView {
                    spec: &family.notes[1],
                    text: GOOD,
                },
            ],
        )
        .expect("应当通过");
        assert_eq!(decision.report.counts.links.generated_broken, 0);
        assert!(decision.navigation.contains("[[a.md|n1]]"));
        assert!(decision.navigation.contains("[[b.md|n2]]"));
    }
}
