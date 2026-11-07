//! Deterministic link migration for section promotions.
//!
//! The migration never rewrites the author package: the author supplies the
//! "after" state and this layer proves that every internal reference still
//! resolves to the same knowledge object. Links are matched against declared
//! objects by location, never by file name, because a promoted section changes
//! the file name while the object identity must survive.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::project::ProjectError;

pub(crate) const POLICY: &str = "link-migration-v1";
pub(crate) const OBJECTS_POLICY: &str = "link-objects-v1";
pub(crate) const AUTHOR_PRODUCER: &str = "fixture_author";
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_NOTES: usize = 8;
pub(crate) const MAX_OBJECTS: usize = 8;
pub(crate) const MAX_LINKS: usize = 32;
pub(crate) const MAX_PROMOTIONS: usize = 4;
pub(crate) const MAX_ID: usize = 48;
pub(crate) const MAX_TEXT: usize = 600;
pub(crate) const MAX_TITLE: usize = 200;

pub(crate) const SIDE_BEFORE: &str = "before";
pub(crate) const SIDE_AFTER: &str = "after";
pub(crate) const CHANGE_UNCHANGED: &str = "unchanged";
pub(crate) const CHANGE_LINKS_ONLY: &str = "links_only";
pub(crate) const CHANGE_PROMOTED: &str = "promoted";
pub(crate) const CHANGE_CREATED: &str = "created";
pub(crate) const CHANGES: [&str; 4] = [
    CHANGE_UNCHANGED,
    CHANGE_LINKS_ONLY,
    CHANGE_PROMOTED,
    CHANGE_CREATED,
];

pub(crate) const MATCH_CANONICAL: &str = "canonical";
pub(crate) const MATCH_REDIRECT: &str = "redirect";
pub(crate) const LINK_RESOLVED: &str = "resolved";

pub(crate) fn argument(message: String) -> ProjectError {
    ProjectError::new("MIGRATE_ARGUMENT", 2, message)
}

pub(crate) fn integrity(message: String) -> ProjectError {
    ProjectError::new("MIGRATE_INTEGRITY", 3, message)
}

pub(crate) fn unknown_root(message: String) -> ProjectError {
    ProjectError::new("MIGRATE_UNKNOWN_ROOT", 3, message)
}

pub(crate) fn ambiguous_target(message: String) -> ProjectError {
    ProjectError::new("MIGRATE_AMBIGUOUS_TARGET", 3, message)
}

pub(crate) fn target_missing(message: String) -> ProjectError {
    ProjectError::new("MIGRATE_TARGET_MISSING", 3, message)
}

pub(crate) fn unescaped_alias(message: String) -> ProjectError {
    ProjectError::new("MIGRATE_UNESCAPED_ALIAS", 3, message)
}

pub(crate) fn object_missing(message: String) -> ProjectError {
    ProjectError::new("MIGRATE_OBJECT_MISSING", 3, message)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoteSpec {
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) counterpart: Option<String>,
    #[serde(default)]
    pub(crate) change: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Promotion {
    pub(crate) promotion_id: String,
    pub(crate) from_note: String,
    pub(crate) from_heading: String,
    pub(crate) to_note: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Limits {
    pub(crate) max_notes: usize,
    pub(crate) max_objects: usize,
    pub(crate) max_links: usize,
    pub(crate) max_promotions: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Plan {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) before_dir: String,
    pub(crate) after_dir: String,
    pub(crate) objects: String,
    pub(crate) before: Vec<NoteSpec>,
    pub(crate) after: Vec<NoteSpec>,
    pub(crate) promotions: Vec<Promotion>,
    pub(crate) limits: Limits,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Location {
    pub(crate) note_id: String,
    #[serde(default)]
    pub(crate) heading: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObjectSpec {
    pub(crate) object_id: String,
    pub(crate) kind: String,
    pub(crate) term: String,
    pub(crate) before: Location,
    pub(crate) after: Location,
    #[serde(default)]
    pub(crate) redirects: Vec<Location>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObjectFile {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) objects: Vec<ObjectSpec>,
}

#[derive(Clone, Debug)]
pub(crate) struct NoteView<'a> {
    pub(crate) spec: &'a NoteSpec,
    pub(crate) text: &'a str,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct LinkRecord {
    pub(crate) side: String,
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) line: usize,
    pub(crate) raw: String,
    pub(crate) target: String,
    pub(crate) heading: Option<String>,
    pub(crate) alias: Option<String>,
    pub(crate) in_table: bool,
    pub(crate) escaped_alias: bool,
    pub(crate) object_id: String,
    pub(crate) matched: String,
    pub(crate) status: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Placement {
    pub(crate) object_id: String,
    pub(crate) kind: String,
    pub(crate) term: String,
    pub(crate) before_target: String,
    pub(crate) after_target: String,
    pub(crate) redirect_targets: Vec<String>,
    pub(crate) links_before: usize,
    pub(crate) links_after: usize,
    pub(crate) links_after_canonical: usize,
    pub(crate) links_after_redirect: usize,
    pub(crate) referenced_before: bool,
    pub(crate) referenced_after: bool,
    pub(crate) identity_preserved: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LinkCounts {
    pub(crate) links_before: usize,
    pub(crate) links_after: usize,
    pub(crate) resolved_before: usize,
    pub(crate) resolved_after: usize,
    pub(crate) canonical_links_after: usize,
    pub(crate) redirect_links_after: usize,
    pub(crate) broken_links: usize,
    pub(crate) table_links_before: usize,
    pub(crate) table_links_after: usize,
    pub(crate) escaped_alias_before: usize,
    pub(crate) escaped_alias_after: usize,
    pub(crate) rewritten_targets: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObjectCounts {
    pub(crate) objects: usize,
    pub(crate) objects_referenced: usize,
    pub(crate) objects_unreferenced: usize,
    pub(crate) identity_violations: usize,
    pub(crate) redirects: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Counts {
    pub(crate) notes_before: usize,
    pub(crate) notes_after: usize,
    pub(crate) objects: usize,
    pub(crate) links_before: usize,
    pub(crate) links_after: usize,
    pub(crate) objects_referenced: usize,
    pub(crate) objects_unreferenced: usize,
    pub(crate) identity_violations: usize,
    pub(crate) broken_links: usize,
    pub(crate) canonical_links_after: usize,
    pub(crate) redirect_links_after: usize,
    pub(crate) table_links_before: usize,
    pub(crate) table_links_after: usize,
    pub(crate) escaped_alias_before: usize,
    pub(crate) escaped_alias_after: usize,
    pub(crate) unchanged_notes: usize,
    pub(crate) stripped_equal_notes: usize,
    pub(crate) promoted_notes: usize,
    pub(crate) created_notes: usize,
    pub(crate) rewritten_targets: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct NoteVerdict {
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) side: String,
    pub(crate) counterpart: Option<String>,
    pub(crate) change: String,
    pub(crate) bytes_equal: Option<bool>,
    pub(crate) stripped_equal: Option<bool>,
    pub(crate) promotion_satisfied: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Links {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) before: Vec<LinkRecord>,
    pub(crate) after: Vec<LinkRecord>,
    pub(crate) counts: LinkCounts,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Placements {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) objects: Vec<Placement>,
    pub(crate) counts: ObjectCounts,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Report {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) counts: Counts,
    pub(crate) notes: Vec<NoteVerdict>,
    pub(crate) model_calls: u64,
    pub(crate) network_requests: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct Decision {
    pub(crate) links: Links,
    pub(crate) objects: Placements,
    pub(crate) report: Report,
    pub(crate) markdown: String,
}

pub(crate) fn single(text: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(argument(format!("{label}不能为空。")));
    }
    if trimmed != text {
        return Err(argument(format!("{label}不能有首尾空白。")));
    }
    if text.chars().count() > limit {
        return Err(argument(format!("{label}超过 {limit} 个字符。")));
    }
    if text.chars().any(char::is_control) {
        return Err(argument(format!("{label}不能包含控制字符。")));
    }
    Ok(())
}

pub(crate) fn identifier(text: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    single(text, label, limit)?;
    if !text
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
    {
        return Err(argument(format!(
            "{label}只能包含ASCII字母、数字、连字符、下划线和点。"
        )));
    }
    Ok(())
}

pub(crate) fn relative(text: &str, label: &str) -> Result<(), ProjectError> {
    single(text, label, MAX_TITLE)?;
    if text.starts_with('/') || text.contains('\\') {
        return Err(argument(format!("{label}必须是作者包内的相对路径。")));
    }
    for segment in text.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(argument(format!("{label}不能包含空段、`.` 或 `..`。")));
        }
    }
    Ok(())
}

impl Limits {
    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if !(1..=MAX_NOTES).contains(&self.max_notes) {
            return Err(argument(format!(
                "limits.max_notes 必须在 1..={MAX_NOTES}。"
            )));
        }
        if !(1..=MAX_OBJECTS).contains(&self.max_objects) {
            return Err(argument(format!(
                "limits.max_objects 必须在 1..={MAX_OBJECTS}。"
            )));
        }
        if !(1..=MAX_LINKS).contains(&self.max_links) {
            return Err(argument(format!(
                "limits.max_links 必须在 1..={MAX_LINKS}。"
            )));
        }
        if self.max_promotions > MAX_PROMOTIONS {
            return Err(argument(format!(
                "limits.max_promotions 必须在 0..={MAX_PROMOTIONS}。"
            )));
        }
        Ok(())
    }
}

impl Plan {
    pub(crate) fn parse(text: &str) -> Result<Self, ProjectError> {
        serde_json::from_str(text)
            .map_err(|error| argument(format!("迁移计划不是合法 JSON 或含未知字段：{error}。")))
    }

    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!(
                "policy 必须是 {POLICY}，实际是 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "schema_version 必须是 {SCHEMA_VERSION}。"
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "producer 必须是 {AUTHOR_PRODUCER}，实际是 {}。",
                self.producer
            )));
        }
        identifier(&self.case_id, "case_id", MAX_ID)?;
        single(&self.description, "description", MAX_TEXT)?;
        relative(&self.before_dir, "before_dir")?;
        relative(&self.after_dir, "after_dir")?;
        relative(&self.objects, "objects")?;
        self.limits.validate()?;
        if self.before.is_empty() || self.before.len() > self.limits.max_notes {
            return Err(argument(format!(
                "before 笔记数必须在 1..={}。",
                self.limits.max_notes
            )));
        }
        if self.after.is_empty() || self.after.len() > self.limits.max_notes {
            return Err(argument(format!(
                "after 笔记数必须在 1..={}。",
                self.limits.max_notes
            )));
        }
        let mut before_ids = BTreeSet::new();
        let mut before_paths = BTreeSet::new();
        for note in &self.before {
            identifier(&note.note_id, "before.note_id", MAX_ID)?;
            relative(&note.path, "before.path")?;
            single(&note.title, "before.title", MAX_TITLE)?;
            if note.counterpart.is_some() || note.change.is_some() {
                return Err(argument(format!(
                    "before 笔记 {} 不能声明 counterpart 或 change；迁移方向只有 before → after。",
                    note.note_id
                )));
            }
            if !before_ids.insert(note.note_id.clone()) {
                return Err(argument(format!("before 笔记 {} 重复。", note.note_id)));
            }
            if !before_paths.insert(note.path.clone()) {
                return Err(argument(format!("before 路径 {} 重复。", note.path)));
            }
        }
        let mut after_ids = BTreeSet::new();
        let mut after_paths = BTreeSet::new();
        for note in &self.after {
            identifier(&note.note_id, "after.note_id", MAX_ID)?;
            relative(&note.path, "after.path")?;
            single(&note.title, "after.title", MAX_TITLE)?;
            let change = note.change.as_deref().ok_or_else(|| {
                argument(format!("after 笔记 {} 必须声明 change。", note.note_id))
            })?;
            if !CHANGES.contains(&change) {
                return Err(argument(format!(
                    "after 笔记 {} 的 change {change} 不在允许集合（unchanged、links_only、promoted、created）内。",
                    note.note_id
                )));
            }
            if change != CHANGE_CREATED {
                let counterpart = note.counterpart.as_deref().ok_or_else(|| {
                    argument(format!(
                        "after 笔记 {} 的 change 是 {change}，必须声明 counterpart。",
                        note.note_id
                    ))
                })?;
                if !before_ids.contains(counterpart) {
                    return Err(argument(format!(
                        "after 笔记 {} 的 counterpart {counterpart} 不在 before 声明里。",
                        note.note_id
                    )));
                }
            } else if note.counterpart.is_some() {
                return Err(argument(format!(
                    "after 笔记 {} 的 change 是 created，不能声明 counterpart。",
                    note.note_id
                )));
            }
            if !after_ids.insert(note.note_id.clone()) {
                return Err(argument(format!("after 笔记 {} 重复。", note.note_id)));
            }
            if !after_paths.insert(note.path.clone()) {
                return Err(argument(format!("after 路径 {} 重复。", note.path)));
            }
        }
        if self.promotions.len() > self.limits.max_promotions {
            return Err(argument(format!(
                "promotions 数超过 limits.max_promotions={}。",
                self.limits.max_promotions
            )));
        }
        let mut promotion_ids = BTreeSet::new();
        for promotion in &self.promotions {
            identifier(&promotion.promotion_id, "promotion_id", MAX_ID)?;
            single(&promotion.from_heading, "from_heading", MAX_TITLE)?;
            if !before_ids.contains(&promotion.from_note) {
                return Err(argument(format!(
                    "提升 {} 的 from_note {} 不在 before 声明里。",
                    promotion.promotion_id, promotion.from_note
                )));
            }
            if !after_ids.contains(&promotion.to_note) {
                return Err(argument(format!(
                    "提升 {} 的 to_note {} 不在 after 声明里。",
                    promotion.promotion_id, promotion.to_note
                )));
            }
            if !promotion_ids.insert(promotion.promotion_id.clone()) {
                return Err(argument(format!("提升 {} 重复。", promotion.promotion_id)));
            }
        }
        Ok(())
    }
}

impl ObjectFile {
    pub(crate) fn parse(text: &str) -> Result<Self, ProjectError> {
        serde_json::from_str(text)
            .map_err(|error| argument(format!("知识对象表不是合法 JSON 或含未知字段：{error}。")))
    }

    pub(crate) fn validate(&self, limits: &Limits) -> Result<(), ProjectError> {
        if self.policy != OBJECTS_POLICY {
            return Err(argument(format!(
                "知识对象表 policy 必须是 {OBJECTS_POLICY}，实际是 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "schema_version 必须是 {SCHEMA_VERSION}。"
            )));
        }
        if self.objects.is_empty() || self.objects.len() > limits.max_objects {
            return Err(argument(format!(
                "知识对象数必须在 1..={}。",
                limits.max_objects
            )));
        }
        let mut ids = BTreeSet::new();
        for object in &self.objects {
            identifier(&object.object_id, "object_id", MAX_ID)?;
            identifier(&object.kind, "object.kind", MAX_ID)?;
            single(&object.term, "object.term", MAX_TITLE)?;
            for (label, location) in [
                ("object.before", &object.before),
                ("object.after", &object.after),
            ] {
                identifier(&location.note_id, &format!("{label}.note_id"), MAX_ID)?;
                if let Some(heading) = &location.heading {
                    single(heading, &format!("{label}.heading"), MAX_TITLE)?;
                }
            }
            for redirect in &object.redirects {
                identifier(&redirect.note_id, "redirect.note_id", MAX_ID)?;
                if let Some(heading) = &redirect.heading {
                    single(heading, "redirect.heading", MAX_TITLE)?;
                }
            }
            if !ids.insert(object.object_id.clone()) {
                return Err(argument(format!("知识对象 {} 重复。", object.object_id)));
            }
        }
        Ok(())
    }
}

pub(crate) fn heading_level(line: &str) -> u8 {
    let indentation = line.len() - line.trim_start_matches(' ').len();
    if indentation > 3 {
        return 0;
    }
    let content = &line[indentation..];
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

pub(crate) fn heading_title(line: &str) -> String {
    let indentation = line.len() - line.trim_start_matches(' ').len();
    let content = &line[indentation..];
    let hashes = content.chars().take_while(|value| *value == '#').count();
    content[hashes..].trim().to_owned()
}

pub(crate) fn heading_titles(text: &str) -> Vec<String> {
    text.split('\n')
        .filter(|line| heading_level(line) > 0)
        .map(heading_title)
        .collect()
}

fn is_table_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

#[derive(Clone, Debug)]
struct RawLink {
    line: usize,
    raw: String,
    target: String,
    heading: Option<String>,
    alias: Option<String>,
    escaped_alias: bool,
    in_table: bool,
}

fn raw_links(text: &str) -> Vec<RawLink> {
    let mut found = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let in_table = is_table_row(line);
        let bytes = line.as_bytes();
        let mut cursor = 0;
        while cursor + 1 < bytes.len() {
            if bytes[cursor] != b'[' || bytes[cursor + 1] != b'[' {
                cursor += 1;
                continue;
            }
            let start = cursor;
            let mut end = None;
            let mut probe = cursor + 2;
            while probe + 1 < bytes.len() {
                if bytes[probe] == b']' && bytes[probe + 1] == b']' {
                    end = Some(probe);
                    break;
                }
                probe += 1;
            }
            let Some(end) = end else {
                break;
            };
            let inner = &line[start + 2..end];
            let (body, alias, escaped_alias) = split_link(inner);
            let (target, heading) = match body.split_once('#') {
                Some((target, heading)) => (
                    target.trim().to_owned(),
                    Some(heading.trim().to_owned()).filter(|value| !value.is_empty()),
                ),
                None => (body.trim().to_owned(), None),
            };
            found.push(RawLink {
                line: index + 1,
                raw: line[start..end + 2].to_owned(),
                target,
                heading,
                alias: alias.filter(|value| !value.is_empty()),
                escaped_alias,
                in_table,
            });
            cursor = end + 2;
        }
    }
    found
}

fn split_link(inner: &str) -> (String, Option<String>, bool) {
    let mut body = String::new();
    let mut alias = String::new();
    let mut escaped_alias = false;
    let mut in_alias = false;
    let mut chars = inner.chars().peekable();
    while let Some(value) = chars.next() {
        if value == '\\' {
            if let Some(next) = chars.peek().copied() {
                if next == '|' {
                    chars.next();
                    if in_alias {
                        alias.push('|');
                        escaped_alias = true;
                    } else {
                        in_alias = true;
                        escaped_alias = true;
                    }
                    continue;
                }
            }
            if in_alias {
                alias.push(value);
            } else {
                body.push(value);
            }
            continue;
        }
        if value == '|' && !in_alias {
            in_alias = true;
            continue;
        }
        if in_alias {
            alias.push(value);
        } else {
            body.push(value);
        }
    }
    let alias = if in_alias { Some(alias) } else { None };
    (body, alias, escaped_alias)
}

fn strip_links(text: &str) -> String {
    let mut result = String::new();
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if cursor + 1 < bytes.len() && bytes[cursor] == b'[' && bytes[cursor + 1] == b'[' {
            let mut probe = cursor + 2;
            let mut end = None;
            while probe + 1 < bytes.len() {
                if bytes[probe] == b']' && bytes[probe + 1] == b']' {
                    end = Some(probe);
                    break;
                }
                probe += 1;
            }
            if let Some(end) = end {
                cursor = end + 2;
                continue;
            }
        }
        let value = text[cursor..]
            .chars()
            .next()
            .expect("cursor stays on a boundary");
        result.push(value);
        cursor += value.len_utf8();
    }
    result
}

fn resolve_note<'a>(
    notes: &'a [NoteView<'a>],
    target: &str,
) -> Result<&'a NoteView<'a>, (&'static str, String)> {
    let mut exact: Vec<&NoteView> = notes
        .iter()
        .filter(|note| note.spec.path == target)
        .collect();
    if exact.is_empty() {
        let with_suffix = format!("{target}.md");
        exact = notes
            .iter()
            .filter(|note| note.spec.path == with_suffix)
            .collect();
    }
    if exact.len() == 1 {
        return Ok(exact[0]);
    }
    if exact.len() > 1 {
        return Err((
            "ambiguous",
            format!("目标 {target} 精确匹配到 {} 份笔记，有歧义。", exact.len()),
        ));
    }
    let by_stem: Vec<&NoteView> = notes
        .iter()
        .filter(|note| {
            let stem = note
                .spec
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&note.spec.path)
                .trim_end_matches(".md");
            stem == target || note.spec.title == target
        })
        .collect();
    match by_stem.len() {
        1 => Ok(by_stem[0]),
        0 => Err((
            "missing",
            format!("目标 {target} 不在本迁移计划声明的笔记里。"),
        )),
        count => Err((
            "ambiguous",
            format!("目标 {target} 对应 {count} 份笔记，有歧义；请写完整路径。"),
        )),
    }
}

fn location_key(note_id: &str, heading: Option<&str>) -> String {
    match heading {
        Some(value) => format!("{note_id}#{value}"),
        None => format!("{note_id}#"),
    }
}

/// 位置在报告里必须写成读者能直接点的完整 wikilink：
/// 只写标题会让同名标题的不同笔记看起来一样，读者无从核对目标。
fn location_text(plan: &Plan, side: &str, location: &Location) -> String {
    let specs = if side == SIDE_BEFORE {
        &plan.before
    } else {
        &plan.after
    };
    let path = specs
        .iter()
        .find(|spec| spec.note_id == location.note_id)
        .map(|spec| spec.path.clone())
        .unwrap_or_else(|| location.note_id.clone());
    match &location.heading {
        Some(heading) => format!("[[{path}#{heading}]]"),
        None => format!("[[{path}]]"),
    }
}

struct LinkIndex {
    before: BTreeMap<String, usize>,
    after: BTreeMap<String, usize>,
    redirect: BTreeMap<String, usize>,
}

fn index_objects(objects: &[ObjectSpec]) -> LinkIndex {
    let mut before = BTreeMap::new();
    let mut after = BTreeMap::new();
    let mut redirect = BTreeMap::new();
    for (index, object) in objects.iter().enumerate() {
        before.insert(
            location_key(&object.before.note_id, object.before.heading.as_deref()),
            index,
        );
        after.insert(
            location_key(&object.after.note_id, object.after.heading.as_deref()),
            index,
        );
        for item in &object.redirects {
            redirect.insert(location_key(&item.note_id, item.heading.as_deref()), index);
        }
    }
    LinkIndex {
        before,
        after,
        redirect,
    }
}

pub(crate) fn decide(
    plan: &Plan,
    objects: &ObjectFile,
    before: &[NoteView<'_>],
    after: &[NoteView<'_>],
) -> Result<Decision, ProjectError> {
    for view in before.iter().chain(after.iter()) {
        for heading in &objects.objects {
            for location in std::iter::once(&heading.before)
                .chain(std::iter::once(&heading.after))
                .chain(heading.redirects.iter())
            {
                if location.note_id != view.spec.note_id {
                    continue;
                }
                if let Some(title) = &location.heading {
                    if !heading_titles(view.text).iter().any(|value| value == title) {
                        return Err(object_missing(format!(
                            "知识对象 {} 声明的位置 {} 在笔记 {} 里不存在这个标题；位置必须真实存在，不能靠猜。",
                            heading.object_id, title, view.spec.path
                        )));
                    }
                }
            }
        }
    }
    let index = index_objects(&objects.objects);
    let before_links = scan(before, SIDE_BEFORE, &objects.objects, &index)?;
    let after_links = scan(after, SIDE_AFTER, &objects.objects, &index)?;

    let mut placements = Vec::new();
    let mut identity_violations = 0usize;
    for object in &objects.objects {
        // 逐条按对象ID计数：链接解析出的对象才是引用归属，
        // 用链接自己的 note_id 去查对象表只会查到来源位置。
        let before_count = before_links
            .iter()
            .filter(|link| link.object_id == object.object_id)
            .count();
        let after_canonical = after_links
            .iter()
            .filter(|link| link.object_id == object.object_id && link.matched == MATCH_CANONICAL)
            .count();
        let after_redirect = after_links
            .iter()
            .filter(|link| link.object_id == object.object_id && link.matched == MATCH_REDIRECT)
            .count();
        let after_count = after_canonical + after_redirect;
        let referenced_before = before_count > 0;
        let referenced_after = after_count > 0;
        let identity_preserved = !referenced_before || referenced_after;
        if !identity_preserved {
            identity_violations += 1;
        }
        placements.push(Placement {
            object_id: object.object_id.clone(),
            kind: object.kind.clone(),
            term: object.term.clone(),
            before_target: location_text(plan, SIDE_BEFORE, &object.before),
            after_target: location_text(plan, SIDE_AFTER, &object.after),
            redirect_targets: object
                .redirects
                .iter()
                .map(|location| location_text(plan, SIDE_AFTER, location))
                .collect(),
            links_before: before_count,
            links_after: after_count,
            links_after_canonical: after_canonical,
            links_after_redirect: after_redirect,
            referenced_before,
            referenced_after,
            identity_preserved,
        });
    }
    if identity_violations > 0 {
        let names: Vec<String> = placements
            .iter()
            .filter(|item| !item.identity_preserved)
            .map(|item| item.object_id.clone())
            .collect();
        return Err(integrity(format!(
            "迁移后这些知识对象不再被任何引用指向：{}。迁移不能丢引用。",
            names.join("、")
        )));
    }

    // 重写目标是"迁移后落在规范位置、而且对象确实换了位置"的引用条数；
    // 仍指向旧位置的跳转链接算保留，不算重写。
    // 两侧的 note_id 前缀不同（before 是 b-*、after 是 a-*），
    // 所以比较的是解析后的路径与标题，不是 id。
    let path_of = |side: &str, note_id: &str| -> Option<String> {
        let specs = if side == SIDE_BEFORE {
            &plan.before
        } else {
            &plan.after
        };
        specs
            .iter()
            .find(|spec| spec.note_id == note_id)
            .map(|spec| spec.path.clone())
    };
    let moved = |object: &ObjectSpec| -> bool {
        let before_path = path_of(SIDE_BEFORE, &object.before.note_id);
        let after_path = path_of(SIDE_AFTER, &object.after.note_id);
        before_path != after_path || object.before.heading != object.after.heading
    };
    let rewritten_targets = after_links
        .iter()
        .filter(|link| {
            if link.matched != MATCH_CANONICAL {
                return false;
            }
            objects
                .objects
                .iter()
                .find(|object| object.object_id == link.object_id)
                .is_some_and(moved)
        })
        .count();

    let mut notes = Vec::new();
    let mut unchanged_notes = 0usize;
    let mut stripped_equal_notes = 0usize;
    let mut promoted_notes = 0usize;
    let mut created_notes = 0usize;
    for spec in &plan.after {
        let change = spec.change.clone().unwrap_or_default();
        let view = after
            .iter()
            .find(|view| view.spec.note_id == spec.note_id)
            .ok_or_else(|| argument(format!("after 笔记 {} 缺少正文。", spec.note_id)))?;
        let counterpart = match &spec.counterpart {
            Some(id) => Some(
                before
                    .iter()
                    .find(|view| view.spec.note_id == *id)
                    .ok_or_else(|| argument(format!("before 笔记 {id} 缺少正文。")))?,
            ),
            None => None,
        };
        let mut bytes_equal = None;
        let mut stripped_equal = None;
        let mut promotion_satisfied = None;
        match change.as_str() {
            CHANGE_UNCHANGED => {
                let other = counterpart.expect("validated counterpart");
                let equal = other.text == view.text;
                if !equal {
                    return Err(argument(format!(
                        "笔记 {} 声明 unchanged，但和 {} 的正文不同；未受影响的表示必须逐字节不变。",
                        spec.path, other.spec.path
                    )));
                }
                unchanged_notes += 1;
                bytes_equal = Some(true);
                stripped_equal = Some(true);
            }
            CHANGE_LINKS_ONLY => {
                let other = counterpart.expect("validated counterpart");
                let left = strip_links(other.text);
                let right = strip_links(view.text);
                let equal = left == right;
                if !equal {
                    return Err(argument(format!(
                        "笔记 {} 声明 links_only，但剥掉链接后的正文和 {} 不同；links_only 只允许改链接。",
                        spec.path, other.spec.path
                    )));
                }
                stripped_equal_notes += 1;
                bytes_equal = Some(other.text == view.text);
                stripped_equal = Some(true);
            }
            CHANGE_PROMOTED => {
                let other = counterpart.expect("validated counterpart");
                let promotion = plan
                    .promotions
                    .iter()
                    .find(|item| item.from_note == other.spec.note_id)
                    .ok_or_else(|| {
                        argument(format!(
                            "笔记 {} 声明 promoted，但没有声明从 {} 提升出什么。",
                            spec.path, other.spec.path
                        ))
                    })?;
                // 提升不是删标题：源笔记要保留那个标题作为重定向位置，
                // 否则旧的 [[源笔记#标题]] 会在迁移后立刻变成断链。
                if !heading_titles(other.text)
                    .iter()
                    .any(|value| value == &promotion.from_heading)
                {
                    return Err(argument(format!(
                        "提升来源 {} 里本来就没有标题「{}」；迁移计划声明的提升内容不存在。",
                        other.spec.path, promotion.from_heading
                    )));
                }
                if !heading_titles(view.text)
                    .iter()
                    .any(|value| value == &promotion.from_heading)
                {
                    return Err(argument(format!(
                        "笔记 {} 把「{}」整个删掉了：提升必须保留原标题作为重定向位置，否则旧链接会立刻断掉。",
                        spec.path, promotion.from_heading
                    )));
                }
                if !after
                    .iter()
                    .any(|item| item.spec.note_id == promotion.to_note)
                {
                    return Err(argument(format!(
                        "提升目标 {} 不在 after 声明里。",
                        promotion.to_note
                    )));
                }
                promoted_notes += 1;
                bytes_equal = Some(other.text == view.text);
                stripped_equal = Some(false);
                promotion_satisfied = Some(true);
            }
            CHANGE_CREATED => {
                created_notes += 1;
            }
            _ => {}
        }
        notes.push(NoteVerdict {
            note_id: spec.note_id.clone(),
            path: spec.path.clone(),
            side: SIDE_AFTER.to_owned(),
            counterpart: spec.counterpart.clone(),
            change,
            bytes_equal,
            stripped_equal,
            promotion_satisfied,
        });
    }
    // 核对结论只对 after 侧成立：before 侧没有"是否保持"可判，
    // 它们的内容摘要已经在 inputs.json 里，混进报告只会让读者以为
    // 每一行都有一个可核对的结果。

    let count_links = |links: &[LinkRecord]| links.len();
    let table_links = |links: &[LinkRecord]| links.iter().filter(|link| link.in_table).count();
    let escaped = |links: &[LinkRecord]| links.iter().filter(|link| link.escaped_alias).count();
    let canonical_after = after_links
        .iter()
        .filter(|link| link.matched == MATCH_CANONICAL)
        .count();
    let redirect_after = after_links
        .iter()
        .filter(|link| link.matched == MATCH_REDIRECT)
        .count();
    let link_counts = LinkCounts {
        links_before: count_links(&before_links),
        links_after: count_links(&after_links),
        resolved_before: before_links
            .iter()
            .filter(|link| link.status == LINK_RESOLVED)
            .count(),
        resolved_after: after_links
            .iter()
            .filter(|link| link.status == LINK_RESOLVED)
            .count(),
        canonical_links_after: canonical_after,
        redirect_links_after: redirect_after,
        broken_links: before_links
            .iter()
            .chain(after_links.iter())
            .filter(|link| link.status != LINK_RESOLVED)
            .count(),
        table_links_before: table_links(&before_links),
        table_links_after: table_links(&after_links),
        escaped_alias_before: escaped(&before_links),
        escaped_alias_after: escaped(&after_links),
        rewritten_targets,
    };
    // 未被引用的口径是"两侧都没人指向"：只在迁移前被引用也算被引用过。
    let unreferenced = placements
        .iter()
        .filter(|item| !item.referenced_after && !item.referenced_before)
        .count();
    let object_counts = ObjectCounts {
        objects: placements.len(),
        objects_referenced: placements.len() - unreferenced,
        objects_unreferenced: unreferenced,
        identity_violations,
        redirects: placements
            .iter()
            .filter(|item| !item.redirect_targets.is_empty())
            .count(),
    };
    let counts = Counts {
        notes_before: plan.before.len(),
        notes_after: plan.after.len(),
        objects: objects.objects.len(),
        links_before: link_counts.links_before,
        links_after: link_counts.links_after,
        objects_referenced: object_counts.objects_referenced,
        objects_unreferenced: object_counts.objects_unreferenced,
        identity_violations,
        broken_links: link_counts.broken_links,
        canonical_links_after: link_counts.canonical_links_after,
        redirect_links_after: link_counts.redirect_links_after,
        table_links_before: link_counts.table_links_before,
        table_links_after: link_counts.table_links_after,
        escaped_alias_before: link_counts.escaped_alias_before,
        escaped_alias_after: link_counts.escaped_alias_after,
        unchanged_notes,
        stripped_equal_notes,
        promoted_notes,
        created_notes,
        rewritten_targets,
    };
    if counts.broken_links > 0 {
        return Err(integrity(format!(
            "迁移后仍有 {} 条断链；断链必须为 0 才能写出迁移报告。",
            counts.broken_links
        )));
    }
    let links = Links {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: plan.case_id.clone(),
        before: before_links,
        after: after_links,
        counts: link_counts,
    };
    let placements = Placements {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: plan.case_id.clone(),
        objects: placements,
        counts: object_counts,
    };
    let report = Report {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: plan.case_id.clone(),
        description: plan.description.clone(),
        counts,
        notes,
        model_calls: 0,
        network_requests: 0,
    };
    let markdown = render_markdown(&report, &links, &placements);
    Ok(Decision {
        links,
        objects: placements,
        report,
        markdown,
    })
}

fn scan(
    notes: &[NoteView<'_>],
    side: &str,
    objects: &[ObjectSpec],
    index: &LinkIndex,
) -> Result<Vec<LinkRecord>, ProjectError> {
    let map = if side == SIDE_BEFORE {
        &index.before
    } else {
        &index.after
    };
    let mut records = Vec::new();
    for view in notes {
        for link in raw_links(view.text) {
            if link.in_table && link.alias.is_some() && !link.escaped_alias {
                return Err(unescaped_alias(format!(
                    "笔记 {} 第 {} 行在表格单元里写了未转义的别名竖线：{}。表格内必须写成 \\|，否则单元格会被截断。",
                    view.spec.path, link.line, link.raw
                )));
            }
            let resolved = match resolve_note(notes, &link.target) {
                Ok(resolved) => resolved,
                Err((kind, detail)) if kind == "ambiguous" => return Err(ambiguous_target(detail)),
                Err((_, detail)) => return Err(target_missing(detail)),
            };
            if let Some(heading) = &link.heading {
                if !heading_titles(resolved.text)
                    .iter()
                    .any(|value| value == heading)
                {
                    return Err(target_missing(format!(
                        "笔记 {} 第 {} 行的链接 {} 指向 {}，但那份笔记里没有标题「{}」。",
                        view.spec.path, link.line, link.raw, resolved.spec.path, heading
                    )));
                }
            }
            let key = location_key(&resolved.spec.note_id, link.heading.as_deref());
            let (position, matched) = match map.get(&key) {
                Some(position) => (*position, MATCH_CANONICAL),
                None => match index.redirect.get(&key) {
                    Some(position) => (*position, MATCH_REDIRECT),
                    None => {
                        return Err(object_missing(format!(
                            "笔记 {} 第 {} 行的链接 {} 解析到 {}，但那个位置没有登记为知识对象；迁移不能指向未登记的位置。",
                            view.spec.path, link.line, link.raw, resolved.spec.path
                        )));
                    }
                },
            };
            records.push(LinkRecord {
                side: side.to_owned(),
                note_id: view.spec.note_id.clone(),
                path: view.spec.path.clone(),
                line: link.line,
                raw: link.raw,
                target: link.target,
                heading: link.heading,
                alias: link.alias,
                in_table: link.in_table,
                escaped_alias: link.escaped_alias,
                object_id: objects[position].object_id.clone(),
                matched: matched.to_owned(),
                status: LINK_RESOLVED.to_owned(),
            });
        }
    }
    Ok(records)
}

fn render_markdown(report: &Report, links: &Links, objects: &Placements) -> String {
    let mut text = String::new();
    text.push_str(&format!("# 链接迁移报告（{}）\n\n", report.case_id));
    text.push_str(&format!("{}\n\n", report.description));
    text.push_str("## 计数\n\n");
    text.push_str(&format!(
        "- 笔记：迁移前 {} 篇、迁移后 {} 篇\n",
        report.counts.notes_before, report.counts.notes_after
    ));
    text.push_str(&format!(
        "- 引用：迁移前 {} 条、迁移后 {} 条；断链 {} 条\n",
        report.counts.links_before, report.counts.links_after, report.counts.broken_links
    ));
    text.push_str(&format!(
        "- 迁移后规范位置 {} 条、跳转位置 {} 条\n",
        report.counts.canonical_links_after, report.counts.redirect_links_after
    ));
    text.push_str(&format!(
        "- 知识对象 {} 个：被引用 {} 个、未被引用 {} 个、身份违规 {} 个\n",
        report.counts.objects,
        report.counts.objects_referenced,
        report.counts.objects_unreferenced,
        report.counts.identity_violations
    ));
    text.push_str(&format!(
        "- 重写目标 {} 处；unchanged {} 篇、links_only {} 篇、promoted {} 篇、created {} 篇\n\n",
        report.counts.rewritten_targets,
        report.counts.unchanged_notes,
        report.counts.stripped_equal_notes,
        report.counts.promoted_notes,
        report.counts.created_notes
    ));
    text.push_str("## 知识对象身份\n\n");
    text.push_str("| 对象 | 迁移前位置 | 迁移后位置 | 迁移前引用 | 迁移后引用 |\n");
    text.push_str("| --- | --- | --- | --- | --- |\n");
    for object in &objects.objects {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            object.object_id,
            object.before_target,
            object.after_target,
            object.links_before,
            object.links_after
        ));
    }
    text.push_str("\n## 迁移后引用\n\n");
    text.push_str("| 笔记 | 行 | 链接 | 对象 | 匹配 |\n");
    text.push_str("| --- | --- | --- | --- | --- |\n");
    for link in &links.after {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            link.path,
            link.line,
            link.raw.replace('|', "\\|"),
            link.object_id,
            link.matched
        ));
    }
    text.push_str("\n## 笔记核对\n\n");
    text.push_str("| 笔记 | 变更 | 逐字节相等 | 剥链接后相等 |\n");
    text.push_str("| --- | --- | --- | --- |\n");
    for note in &report.notes {
        if note.side != SIDE_AFTER {
            continue;
        }
        let bytes = match note.bytes_equal {
            Some(true) => "是",
            Some(false) => "否",
            None => "—",
        };
        let stripped = match note.stripped_equal {
            Some(true) => "是",
            Some(false) => "否",
            None => "—",
        };
        text.push_str(&format!(
            "| {} | {} | {bytes} | {stripped} |\n",
            note.path, note.change
        ));
    }
    text.push_str("\n## 边界\n\n");
    text.push_str(
        "- 迁移只核对与报告，不改写作者包：after 目录由作者提供本层只验证身份是否保持。\n",
    );
    text.push_str("- 链接先解析到知识对象再比较身份，文件名只是位置；断链、歧义、目标缺失、表格未转义别名或对象位置缺失一律整包拒绝。\n");
    text.push_str("- 模型调用与网络请求 0 次；不判断内容质量与科学正确性。\n");
    text
}
