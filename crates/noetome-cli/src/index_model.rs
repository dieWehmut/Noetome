//! Deterministic, portable source index for one Noetome output area.
//!
//! The index answers a single question: for every note in the output area, which
//! original material did it come from, and where is that material now?  It does
//! so without ever putting a citation mark into the note body: the body stays
//! clean and every mapping lives here.
//!
//! Two rules make the index portable.  First, the output area, the runtime
//! library and the temporary area must be disjoint, so a copied output area can
//! never drag a session database or a cache file along with it.  Second, a
//! source is identified by the digest of its bytes, never by its file name: a
//! renamed file is the same source, and a changed file is a different one even
//! when the name is unchanged.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::migrate_model::{heading_level, heading_title};
use crate::project::ProjectError;

pub(crate) const POLICY: &str = "source-index-v1";
pub(crate) const AUTHOR_PRODUCER: &str = "fixture_author";
pub(crate) const INDEX_VERSION: u32 = 1;
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_NOTES: usize = 8;
pub(crate) const MAX_RESOURCES: usize = 8;
pub(crate) const MAX_SOURCES: usize = 8;
pub(crate) const MAX_ID: usize = 48;
pub(crate) const MAX_TEXT: usize = 600;
pub(crate) const MAX_TITLE: usize = 200;
pub(crate) const MAX_LOCATOR: usize = 120;

pub(crate) const ROLE_NOTE: &str = "note";
pub(crate) const ROLE_RESOURCE: &str = "resource";

pub(crate) const STATUS_RELOCATED: &str = "relocated";
pub(crate) const STATUS_MISSING: &str = "missing";
pub(crate) const STATUS_DIGEST_MISMATCH: &str = "digest_mismatch";

/// Substrings that must never appear in an exported index or in a note body:
/// the index is meant to be copied to another machine, and a credential that
/// travels with it is a credential leak.
pub(crate) const SECRET_MARKERS: [&str; 6] = [
    "api_key",
    "apikey",
    "password",
    "secret",
    "token=",
    "authorization:",
];

pub(crate) fn argument(message: String) -> ProjectError {
    ProjectError::new("INDEX_ARGUMENT", 2, message)
}

pub(crate) fn integrity(message: String) -> ProjectError {
    ProjectError::new("INDEX_INTEGRITY", 3, message)
}

pub(crate) fn unknown_root(message: String) -> ProjectError {
    ProjectError::new("INDEX_UNKNOWN_ROOT", 3, message)
}

pub(crate) fn citation_burden(message: String) -> ProjectError {
    ProjectError::new("INDEX_CITATION_BURDEN", 3, message)
}

pub(crate) fn area_leak(message: String) -> ProjectError {
    ProjectError::new("INDEX_AREA_LEAK", 3, message)
}

pub(crate) fn location_missing(message: String) -> ProjectError {
    ProjectError::new("INDEX_LOCATION_MISSING", 3, message)
}

pub(crate) fn secret_marker(message: String) -> ProjectError {
    ProjectError::new("INDEX_SECRET_MARKER", 3, message)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoteSpec {
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) title: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResourceSpec {
    pub(crate) resource_id: String,
    pub(crate) path: String,
    pub(crate) description: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceSpec {
    pub(crate) source_id: String,
    pub(crate) note_id: String,
    pub(crate) heading: String,
    pub(crate) description: String,
    pub(crate) locator: String,
    pub(crate) path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Limits {
    pub(crate) max_notes: usize,
    pub(crate) max_resources: usize,
    pub(crate) max_sources: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Project {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) output_area: String,
    pub(crate) runtime_area: String,
    pub(crate) temp_area: String,
    pub(crate) relocate_area: String,
    pub(crate) notes: Vec<NoteSpec>,
    pub(crate) resources: Vec<ResourceSpec>,
    pub(crate) sources: Vec<SourceSpec>,
    pub(crate) limits: Limits,
}

/// The four managed areas of a project.  They are kept together because the
/// whole point is that they never overlap: a note that lives in the runtime
/// library would be copied into the output area, and a source that lives in the
/// temporary area would vanish on the next run.
#[derive(Clone, Debug)]
pub(crate) struct Areas {
    pub(crate) output: String,
    pub(crate) runtime: String,
    pub(crate) temp: String,
    pub(crate) relocate: String,
}

impl Areas {
    pub(crate) fn of(project: &Project) -> Self {
        Self {
            output: project.output_area.clone(),
            runtime: project.runtime_area.clone(),
            temp: project.temp_area.clone(),
            relocate: project.relocate_area.clone(),
        }
    }

    pub(crate) fn all(&self) -> [&str; 4] {
        [&self.output, &self.runtime, &self.temp, &self.relocate]
    }

    pub(crate) fn output_only(&self) -> [&str; 3] {
        [&self.output, &self.runtime, &self.temp]
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NoteView<'a> {
    pub(crate) spec: &'a NoteSpec,
    pub(crate) text: &'a str,
    pub(crate) sha256: &'a str,
    pub(crate) byte_length: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct ResourceView<'a> {
    pub(crate) spec: &'a ResourceSpec,
    pub(crate) text: &'a str,
    pub(crate) sha256: &'a str,
    pub(crate) byte_length: u64,
}

/// One original material file, already read by the store layer.
#[derive(Clone, Debug)]
pub(crate) struct MaterialView<'a> {
    pub(crate) spec: &'a SourceSpec,
    pub(crate) sha256: &'a str,
    pub(crate) byte_length: u64,
}

/// A file found while scanning the relocation area.  Only the relative path and
/// the digest matter: the name is used to tell "renamed but same content" from
/// "same name but edited", and nothing else.
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub(crate) path: String,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FileEntry {
    pub(crate) path: String,
    pub(crate) role: String,
    pub(crate) sha256: String,
    pub(crate) byte_length: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct NoteEntry {
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) title: String,
    pub(crate) headings: Vec<String>,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SourceEntry {
    pub(crate) source_id: String,
    pub(crate) note_id: String,
    pub(crate) heading: String,
    pub(crate) description: String,
    pub(crate) locator: String,
    pub(crate) path: String,
    pub(crate) sha256: String,
    pub(crate) byte_length: u64,
    pub(crate) digest_short: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Counts {
    pub(crate) notes: usize,
    pub(crate) resources: usize,
    pub(crate) sources: usize,
    pub(crate) headings: usize,
    pub(crate) output_files: usize,
    pub(crate) excluded_runtime_entries: usize,
    pub(crate) excluded_temp_entries: usize,
    pub(crate) citation_marks: usize,
    pub(crate) absolute_paths: usize,
    pub(crate) secret_markers: usize,
    pub(crate) relocated: usize,
    pub(crate) missing: usize,
    pub(crate) digest_mismatch: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct RelocationEntry {
    pub(crate) source_id: String,
    pub(crate) status: String,
    pub(crate) found: Option<String>,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Relocation {
    pub(crate) policy: String,
    pub(crate) index_version: u32,
    pub(crate) case_id: String,
    pub(crate) directory: String,
    pub(crate) relocated: usize,
    pub(crate) missing: usize,
    pub(crate) digest_mismatch: usize,
    pub(crate) sources: Vec<RelocationEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Index {
    pub(crate) policy: String,
    pub(crate) index_version: u32,
    pub(crate) case_id: String,
    pub(crate) output_area: String,
    pub(crate) files: Vec<FileEntry>,
    pub(crate) notes: Vec<NoteEntry>,
    pub(crate) sources: Vec<SourceEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AreasRecord {
    pub(crate) output: String,
    pub(crate) runtime: String,
    pub(crate) temp: String,
    pub(crate) relocate: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Report {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) areas: AreasRecord,
    pub(crate) counts: Counts,
    pub(crate) model_calls: u32,
    pub(crate) network_requests: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct Decision {
    pub(crate) index: Index,
    pub(crate) relocation: Relocation,
    pub(crate) report: Report,
    pub(crate) markdown: String,
}

pub(crate) fn single(text: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    if text.is_empty() {
        return Err(argument(format!("{label}不能为空。")));
    }
    if text != text.trim() {
        return Err(argument(format!("{label}不能有首尾空白：{text}")));
    }
    if text.chars().count() > limit {
        return Err(argument(format!(
            "{label}最多 {limit} 个字符，实际 {} 个。",
            text.chars().count()
        )));
    }
    if text.chars().any(char::is_control) {
        return Err(argument(format!("{label}不能包含控制字符：{text}")));
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
            "{label}只能包含ASCII字母、数字、连字符、下划线和点：{text}"
        )));
    }
    Ok(())
}

pub(crate) fn relative(text: &str, label: &str) -> Result<(), ProjectError> {
    single(text, label, MAX_TEXT)?;
    if text.starts_with('/') || text.contains('\\') {
        return Err(argument(format!(
            "{label}必须是相对路径，并以 / 分隔：{text}"
        )));
    }
    if text
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(argument(format!("{label}不能包含空段、. 或 ..：{text}")));
    }
    Ok(())
}

/// True when `path` lives inside `area`.  Compared component by component so
/// that `vault2/x.md` is not treated as living inside `vault`.
pub(crate) fn under(path: &str, area: &str) -> bool {
    path == area
        || path
            .strip_prefix(area)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub(crate) fn digest_short(sha256: &str) -> String {
    sha256.chars().take(16).collect()
}

impl Limits {
    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if !(1..=MAX_NOTES).contains(&self.max_notes) {
            return Err(argument(format!(
                "max_notes 必须在 1 到 {MAX_NOTES} 之间，实际 {}。",
                self.max_notes
            )));
        }
        if !(0..=MAX_RESOURCES).contains(&self.max_resources) {
            return Err(argument(format!(
                "max_resources 必须在 0 到 {MAX_RESOURCES} 之间，实际 {}。",
                self.max_resources
            )));
        }
        if !(0..=MAX_SOURCES).contains(&self.max_sources) {
            return Err(argument(format!(
                "max_sources 必须在 0 到 {MAX_SOURCES} 之间，实际 {}。",
                self.max_sources
            )));
        }
        Ok(())
    }
}

impl Project {
    pub(crate) fn parse(text: &str) -> Result<Self, ProjectError> {
        serde_json::from_str(text)
            .map_err(|error| argument(format!("工程声明不是合法 JSON：{error}")))
    }

    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!(
                "工程声明的 policy 是 {}，期望 {POLICY}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "工程声明的 schema_version 是 {}，期望 {SCHEMA_VERSION}。",
                self.schema_version
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "工程声明的 producer 是 {}，期望 {AUTHOR_PRODUCER}。",
                self.producer
            )));
        }
        identifier(&self.case_id, "case_id", MAX_ID)?;
        single(&self.description, "description", MAX_TEXT)?;
        for (value, label) in [
            (&self.output_area, "output_area"),
            (&self.runtime_area, "runtime_area"),
            (&self.temp_area, "temp_area"),
            (&self.relocate_area, "relocate_area"),
        ] {
            relative(value, label)?;
        }
        // Disjointness is checked in both directions and on whole components:
        // `vault` and `vault-cache` are different areas, `vault` and `vault/x`
        // are not.
        let areas = Areas::of(self);
        let names = areas.all();
        for (index, left) in names.iter().enumerate() {
            for right in &names[index + 1..] {
                if under(right, left) || under(left, right) {
                    return Err(argument(format!(
                        "输出区、运行库、临时区与重定位目录必须互不包含：{left} 与 {right} 有重叠。"
                    )));
                }
            }
        }
        self.limits.validate()?;
        if self.notes.is_empty() {
            return Err(argument("工程至少要声明一份笔记。".to_owned()));
        }
        if self.notes.len() > self.limits.max_notes {
            return Err(argument(format!(
                "声明的笔记 {} 份，超过 max_notes {}。",
                self.notes.len(),
                self.limits.max_notes
            )));
        }
        if self.resources.len() > self.limits.max_resources {
            return Err(argument(format!(
                "声明的资源 {} 份，超过 max_resources {}。",
                self.resources.len(),
                self.limits.max_resources
            )));
        }
        if self.sources.len() > self.limits.max_sources {
            return Err(argument(format!(
                "声明的来源 {} 条，超过 max_sources {}。",
                self.sources.len(),
                self.limits.max_sources
            )));
        }
        let mut note_ids = BTreeSet::new();
        let mut note_paths = BTreeSet::new();
        for note in &self.notes {
            identifier(&note.note_id, "note_id", MAX_ID)?;
            relative(&note.path, "笔记路径")?;
            single(&note.title, "笔记标题", MAX_TITLE)?;
            // 区域分离是工程的结构不变量，不是参数笔误：笔记或资源跑到运行库、
            // 临时区里，等于把不该随输出走的东西混进了输出区。
            if !under(&note.path, &self.output_area) {
                return Err(area_leak(format!(
                    "笔记 {} 的路径 {} 不在输出区 {} 里；只有输出区里的文件才会随输出一起复制。",
                    note.note_id, note.path, self.output_area
                )));
            }
            if !note_ids.insert(note.note_id.as_str()) {
                return Err(argument(format!("note_id {} 重复声明。", note.note_id)));
            }
            if !note_paths.insert(note.path.as_str()) {
                return Err(argument(format!("笔记路径 {} 重复声明。", note.path)));
            }
        }
        let mut resource_ids = BTreeSet::new();
        for resource in &self.resources {
            identifier(&resource.resource_id, "resource_id", MAX_ID)?;
            relative(&resource.path, "资源路径")?;
            single(&resource.description, "资源说明", MAX_TEXT)?;
            if !under(&resource.path, &self.output_area) {
                return Err(area_leak(format!(
                    "资源 {} 的路径 {} 不在输出区 {} 里。",
                    resource.resource_id, resource.path, self.output_area
                )));
            }
            if !resource_ids.insert(resource.resource_id.as_str()) {
                return Err(argument(format!(
                    "resource_id {} 重复声明。",
                    resource.resource_id
                )));
            }
            if !note_paths.insert(resource.path.as_str()) {
                return Err(argument(format!(
                    "资源路径 {} 与已声明文件重名。",
                    resource.path
                )));
            }
        }
        let mut source_ids = BTreeSet::new();
        for source in &self.sources {
            identifier(&source.source_id, "source_id", MAX_ID)?;
            identifier(&source.note_id, "来源所属笔记", MAX_ID)?;
            single(&source.heading, "来源定位章节", MAX_TITLE)?;
            single(&source.description, "来源描述", MAX_TEXT)?;
            single(&source.locator, "来源定位", MAX_LOCATOR)?;
            relative(&source.path, "来源路径")?;
            if !note_ids.contains(source.note_id.as_str()) {
                return Err(argument(format!(
                    "来源 {} 指向的笔记 {} 没有声明。",
                    source.source_id, source.note_id
                )));
            }
            for area in areas.output_only() {
                if under(&source.path, area) {
                    return Err(area_leak(format!(
                        "来源 {} 的路径 {} 落在 {area} 里；原始资料必须放在输出区、运行库和临时区之外。",
                        source.source_id, source.path
                    )));
                }
            }
            if !source_ids.insert(source.source_id.as_str()) {
                return Err(argument(format!(
                    "source_id {} 重复声明。",
                    source.source_id
                )));
            }
        }
        Ok(())
    }
}

pub(crate) fn heading_titles(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| heading_level(line) > 0)
        .map(heading_title)
        .collect()
}

/// Citation marks are what FR-26 forbids inside a note body: bracketed numbers
/// such as `[1]`, and a reference list section.  Both are detected on the raw
/// text so that a note which only looks clean in rendering still fails.
pub(crate) fn citation_marks(text: &str) -> Vec<String> {
    let mut marks = Vec::new();
    for line in text.lines() {
        let title = heading_title(line);
        if heading_level(line) > 0 && (title.contains("参考文献") || title.contains("引用文献"))
        {
            marks.push(format!("「{title}」小节"));
        }
        let bytes = line.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'[' {
                let mut cursor = index + 1;
                let mut digits = 0;
                while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                    cursor += 1;
                    digits += 1;
                }
                if digits > 0 && cursor < bytes.len() && bytes[cursor] == b']' {
                    marks.push(line[index..=cursor].to_owned());
                    index = cursor + 1;
                    continue;
                }
            }
            index += 1;
        }
    }
    marks
}

pub(crate) fn absolute_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    paths
        .into_iter()
        .filter(|path| path.starts_with('/') || path.contains(':'))
        .map(str::to_owned)
        .collect()
}

pub(crate) fn secret_markers<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut found = BTreeSet::new();
    for value in values {
        let folded = value.to_ascii_lowercase();
        for marker in SECRET_MARKERS {
            if folded.contains(marker) {
                found.insert(marker.to_owned());
            }
        }
    }
    found.into_iter().collect()
}

pub(crate) fn relocation_of(
    case_id: &str,
    directory: &str,
    sources: &[SourceEntry],
    candidates: &[Candidate],
) -> Relocation {
    let by_digest: BTreeMap<&str, &str> = candidates
        .iter()
        .map(|candidate| (candidate.sha256.as_str(), candidate.path.as_str()))
        .collect();
    let mut entries = Vec::new();
    let mut relocated = 0;
    let mut missing = 0;
    let mut digest_mismatch = 0;
    for source in sources {
        let name = source
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&source.path)
            .to_owned();
        match by_digest.get(source.sha256.as_str()) {
            Some(path) => {
                relocated += 1;
                entries.push(RelocationEntry {
                    source_id: source.source_id.clone(),
                    status: STATUS_RELOCATED.to_owned(),
                    found: Some((*path).to_owned()),
                    detail: format!(
                        "在 {directory} 里找到内容摘要一致的文件：{path}（来源描述：{}）。",
                        source.description
                    ),
                });
            }
            None => {
                let same_name: Vec<&str> = candidates
                    .iter()
                    .filter(|candidate| {
                        candidate
                            .path
                            .rsplit('/')
                            .next()
                            .unwrap_or(candidate.path.as_str())
                            == name
                    })
                    .map(|candidate| candidate.path.as_str())
                    .collect();
                if same_name.is_empty() {
                    missing += 1;
                    entries.push(RelocationEntry {
                        source_id: source.source_id.clone(),
                        status: STATUS_MISSING.to_owned(),
                        found: None,
                        detail: format!(
                            "{directory} 里没有内容摘要一致的文件，需要重建映射；来源描述「{}」，定位 {}。",
                            source.description, source.locator
                        ),
                    });
                } else {
                    digest_mismatch += 1;
                    entries.push(RelocationEntry {
                        source_id: source.source_id.clone(),
                        status: STATUS_DIGEST_MISMATCH.to_owned(),
                        found: None,
                        detail: format!(
                            "{} 里有同名文件 {}，但内容摘要不同：同名不能证明是同一份来源，需要重建映射。",
                            directory,
                            same_name.join("、")
                        ),
                    });
                }
            }
        }
    }
    Relocation {
        policy: POLICY.to_owned(),
        index_version: INDEX_VERSION,
        case_id: case_id.to_owned(),
        directory: directory.to_owned(),
        relocated,
        missing,
        digest_mismatch,
        sources: entries,
    }
}

pub(crate) fn decide(
    project: &Project,
    notes: &[NoteView<'_>],
    resources: &[ResourceView<'_>],
    materials: &[MaterialView<'_>],
    runtime_entries: &[String],
    temp_entries: &[String],
    candidates: &[Candidate],
) -> Result<Decision, ProjectError> {
    let areas = Areas::of(project);

    // 1. note bodies stay clean: no bracketed numbers, no reference list.
    let mut marks = Vec::new();
    for note in notes {
        for mark in citation_marks(note.text) {
            marks.push(format!("{} 里出现 {mark}", note.spec.note_id));
        }
    }
    if !marks.is_empty() {
        return Err(citation_burden(format!(
            "笔记正文里出现了文献编号或参考文献小节：{}。默认不在正文插入编号、逐句脚注或长出处列表，来源由索引维护。",
            marks.join("；")
        )));
    }

    // 2. every declared file lives in the output area and nowhere else.
    let mut note_entries = Vec::new();
    let mut files = Vec::new();
    let mut headings_total = 0;
    for note in notes {
        if !under(&note.spec.path, &project.output_area) {
            return Err(area_leak(format!(
                "笔记 {} 的路径 {} 不在输出区 {} 里。",
                note.spec.note_id, note.spec.path, project.output_area
            )));
        }
        let titles = heading_titles(note.text);
        if titles.is_empty() {
            return Err(argument(format!(
                "笔记 {} 没有任何标题，无法建立章节级追溯。",
                note.spec.note_id
            )));
        }
        headings_total += titles.len();
        files.push(FileEntry {
            path: note.spec.path.clone(),
            role: ROLE_NOTE.to_owned(),
            sha256: note.sha256.to_owned(),
            byte_length: note.byte_length,
        });
        note_entries.push(NoteEntry {
            note_id: note.spec.note_id.clone(),
            path: note.spec.path.clone(),
            title: note.spec.title.clone(),
            headings: titles,
            sha256: note.sha256.to_owned(),
        });
    }
    for resource in resources {
        if !under(&resource.spec.path, &project.output_area) {
            return Err(area_leak(format!(
                "资源 {} 的路径 {} 不在输出区 {} 里。",
                resource.spec.resource_id, resource.spec.path, project.output_area
            )));
        }
        files.push(FileEntry {
            path: resource.spec.path.clone(),
            role: ROLE_RESOURCE.to_owned(),
            sha256: resource.sha256.to_owned(),
            byte_length: resource.byte_length,
        });
    }

    // 3. every source points at a heading that really exists in its note, and
    //    the material itself lives outside all managed areas.
    let headings: BTreeMap<&str, &Vec<String>> = note_entries
        .iter()
        .map(|entry| (entry.note_id.as_str(), &entry.headings))
        .collect();
    let mut source_entries = Vec::new();
    for material in materials {
        let Some(titles) = headings.get(material.spec.note_id.as_str()) else {
            return Err(argument(format!(
                "来源 {} 指向的笔记 {} 不在本次声明里。",
                material.spec.source_id, material.spec.note_id
            )));
        };
        if !titles.iter().any(|title| title == &material.spec.heading) {
            return Err(location_missing(format!(
                "来源 {} 声明的章节「{}」在笔记 {} 里不存在；可用的章节是 {}。",
                material.spec.source_id,
                material.spec.heading,
                material.spec.note_id,
                titles.join("、")
            )));
        }
        for area in areas.output_only() {
            if under(&material.spec.path, area) {
                return Err(area_leak(format!(
                    "来源 {} 的路径 {} 落在 {area} 里：原始资料必须放在输出区、运行库与临时区之外，否则复制输出区时来源会跟着泄漏或消失。",
                    material.spec.source_id, material.spec.path
                )));
            }
        }
        source_entries.push(SourceEntry {
            source_id: material.spec.source_id.clone(),
            note_id: material.spec.note_id.clone(),
            heading: material.spec.heading.clone(),
            description: material.spec.description.clone(),
            locator: material.spec.locator.clone(),
            path: material.spec.path.clone(),
            sha256: material.sha256.to_owned(),
            byte_length: material.byte_length,
            digest_short: digest_short(material.sha256),
        });
    }

    // 4. the index must be portable: no absolute path, no credential.
    let mut all_paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    all_paths.extend(source_entries.iter().map(|source| source.path.as_str()));
    let absolute = absolute_paths(all_paths.iter().copied());
    if !absolute.is_empty() {
        return Err(argument(format!(
            "索引里出现了绝对路径或盘符：{}。索引要能跨机器复制，只能写相对路径。",
            absolute.join("、")
        )));
    }
    let mut all_text: Vec<&str> = notes.iter().map(|note| note.text).collect();
    all_text.extend(resources.iter().map(|resource| resource.text));
    all_text.extend(
        source_entries
            .iter()
            .map(|source| source.description.as_str()),
    );
    all_text.extend(source_entries.iter().map(|source| source.locator.as_str()));
    let secrets = secret_markers(all_text);
    if !secrets.is_empty() {
        return Err(secret_marker(format!(
            "正文或来源描述里出现了疑似凭据字样：{}。索引导出会跟着输出区一起被复制，凭据不能进索引。",
            secrets.join("、")
        )));
    }

    let index = Index {
        policy: POLICY.to_owned(),
        index_version: INDEX_VERSION,
        case_id: project.case_id.clone(),
        output_area: project.output_area.clone(),
        files,
        notes: note_entries,
        sources: source_entries,
    };
    let relocation = relocation_of(
        &project.case_id,
        &project.relocate_area,
        &index.sources,
        candidates,
    );
    let counts = Counts {
        notes: index.notes.len(),
        resources: index
            .files
            .iter()
            .filter(|file| file.role == ROLE_RESOURCE)
            .count(),
        sources: index.sources.len(),
        headings: headings_total,
        output_files: index.files.len(),
        excluded_runtime_entries: runtime_entries.len(),
        excluded_temp_entries: temp_entries.len(),
        citation_marks: 0,
        absolute_paths: 0,
        secret_markers: 0,
        relocated: relocation.relocated,
        missing: relocation.missing,
        digest_mismatch: relocation.digest_mismatch,
    };
    let report = Report {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: project.case_id.clone(),
        description: project.description.clone(),
        areas: AreasRecord {
            output: areas.output.clone(),
            runtime: areas.runtime.clone(),
            temp: areas.temp.clone(),
            relocate: areas.relocate.clone(),
        },
        counts,
        model_calls: 0,
        network_requests: 0,
    };
    let markdown = render_markdown(&report, &index, &relocation);
    Ok(Decision {
        index,
        relocation,
        report,
        markdown,
    })
}

pub(crate) fn render_markdown(report: &Report, index: &Index, relocation: &Relocation) -> String {
    let mut out = String::new();
    out.push_str(&format!("# 来源索引（{}）\n\n", report.case_id));
    out.push_str(&format!("{}\n\n", report.description));
    out.push_str("## 区域\n\n");
    out.push_str(&format!(
        "- 输出区：`{}`（复制出去的东西只有这些）\n- 运行库：`{}`（数据库与缓存，不随输出复制）\n- 临时区：`{}`\n- 重定位目录：`{}`\n\n",
        report.areas.output, report.areas.runtime, report.areas.temp, report.areas.relocate
    ));
    out.push_str("## 计数\n\n");
    out.push_str(&format!(
        "- 笔记 {} 份、资源 {} 份、来源 {} 条、标题 {} 个\n",
        report.counts.notes, report.counts.resources, report.counts.sources, report.counts.headings
    ));
    out.push_str(&format!(
        "- 输出区文件 {} 个；未纳入的运行库条目 {} 个、临时区条目 {} 个\n",
        report.counts.output_files,
        report.counts.excluded_runtime_entries,
        report.counts.excluded_temp_entries
    ));
    out.push_str(&format!(
        "- 正文文献编号 {} 处、绝对路径 {} 处、疑似凭据 {} 处\n",
        report.counts.citation_marks, report.counts.absolute_paths, report.counts.secret_markers
    ));
    out.push_str(&format!(
        "- 重定位：已找到 {} 条、缺失 {} 条、摘要不符 {} 条\n\n",
        report.counts.relocated, report.counts.missing, report.counts.digest_mismatch
    ));
    out.push_str("## 文件清单\n\n");
    out.push_str("| 文件 | 角色 | 摘要 | 字节 |\n| --- | --- | --- | --- |\n");
    for file in &index.files {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            file.path,
            if file.role == ROLE_NOTE {
                "笔记"
            } else {
                "资源"
            },
            digest_short(&file.sha256),
            file.byte_length
        ));
    }
    out.push_str("\n## 来源映射\n\n");
    out.push_str(
        "| 来源 | 笔记 | 章节 | 描述 | 定位 | 摘要 |\n| --- | --- | --- | --- | --- | --- |\n",
    );
    for source in &index.sources {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            source.source_id,
            source.note_id,
            source.heading,
            source.description,
            source.locator,
            source.digest_short
        ));
    }
    out.push_str("\n## 重定位结果\n\n");
    out.push_str(&format!("在 `{}` 里核对：\n\n", relocation.directory));
    for entry in &relocation.sources {
        out.push_str(&format!(
            "- {}：{}。{}\n",
            entry.source_id, entry.status, entry.detail
        ));
    }
    out.push_str("\n## 边界\n\n");
    out.push_str(
        "- 正文保持干净：索引不插入文献编号、脚注或出处列表，人工新增文字也不会自动获得来源。\n",
    );
    out.push_str("- 来源按内容摘要识别，不按文件名：同名但内容不同不算同一份来源，缺失时只提示需要重建映射，不指向任何页面。\n");
    out.push_str("- 运行库、缓存、临时文件与凭据不进入输出区与索引；模型调用与网络请求 0 次，不判断内容质量与科学正确性。\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(note_id: &str, path: &str, title: &str) -> NoteSpec {
        NoteSpec {
            note_id: note_id.to_owned(),
            path: path.to_owned(),
            title: title.to_owned(),
        }
    }

    fn source(source_id: &str, note_id: &str, heading: &str, path: &str) -> SourceSpec {
        SourceSpec {
            source_id: source_id.to_owned(),
            note_id: note_id.to_owned(),
            heading: heading.to_owned(),
            description: "某教材 第三章".to_owned(),
            locator: "第 1-4 页".to_owned(),
            path: path.to_owned(),
        }
    }

    fn project(notes: Vec<NoteSpec>, sources: Vec<SourceSpec>) -> Project {
        Project {
            policy: POLICY.to_owned(),
            schema_version: SCHEMA_VERSION,
            producer: AUTHOR_PRODUCER.to_owned(),
            case_id: "idx-test".to_owned(),
            description: "测试工程".to_owned(),
            output_area: "vault".to_owned(),
            runtime_area: "runtime".to_owned(),
            temp_area: "temp".to_owned(),
            relocate_area: "moved".to_owned(),
            notes,
            resources: Vec::new(),
            sources,
            limits: Limits {
                max_notes: 4,
                max_resources: 2,
                max_sources: 4,
            },
        }
    }

    fn clean() -> &'static str {
        "# 糖酵解\n\n## 定义\n\n葡萄糖拆成丙酮酸。\n\n## 核心机制\n\n己糖激酶先磷酸化葡萄糖。\n"
    }

    fn sources() -> Vec<SourceSpec> {
        vec![source("src-a", "note-a", "核心机制", "资料/教材.md")]
    }

    fn materials<'a>(spec: &'a SourceSpec) -> Vec<MaterialView<'a>> {
        vec![MaterialView {
            spec,
            sha256: "aaaa1111",
            byte_length: 10,
        }]
    }

    #[test]
    fn areas_must_not_overlap() {
        let mut value = project(vec![note("note-a", "vault/a.md", "甲")], sources());
        value.runtime_area = "vault/runtime".to_owned();
        assert_eq!(
            value.validate().err().map(|error| error.code().to_owned()),
            Some("INDEX_ARGUMENT".to_owned())
        );
        let mut sibling = project(vec![note("note-a", "vault/a.md", "甲")], sources());
        sibling.runtime_area = "vault-cache".to_owned();
        assert!(sibling.validate().is_ok());
    }

    #[test]
    fn a_note_outside_the_output_area_is_refused() {
        let value = project(vec![note("note-a", "runtime/a.md", "甲")], sources());
        // 区域分离是结构不变量：笔记落进运行库不是参数笔误，而是区域混用。
        assert_eq!(
            value.validate().err().map(|error| error.code().to_owned()),
            Some("INDEX_AREA_LEAK".to_owned())
        );
    }

    #[test]
    fn a_bracketed_number_in_the_body_is_a_citation_burden() {
        let value = project(vec![note("note-a", "vault/a.md", "甲")], sources());
        let spec = &value.notes[0];
        let text = "# 甲\n\n## 定义\n\n葡萄糖拆成丙酮酸[1]。\n";
        let error = decide(
            &value,
            &[NoteView {
                spec,
                text,
                sha256: "ffff",
                byte_length: 20,
            }],
            &[],
            &materials(&value.sources[0]),
            &[],
            &[],
            &[],
        )
        .err()
        .expect("a bracketed number must be refused");
        assert_eq!(error.code(), "INDEX_CITATION_BURDEN");
    }

    #[test]
    fn a_reference_list_section_is_a_citation_burden() {
        let value = project(vec![note("note-a", "vault/a.md", "甲")], sources());
        let spec = &value.notes[0];
        let text = "# 甲\n\n## 定义\n\n葡萄糖拆成丙酮酸。\n\n## 参考文献\n\n某教材。\n";
        let error = decide(
            &value,
            &[NoteView {
                spec,
                text,
                sha256: "ffff",
                byte_length: 20,
            }],
            &[],
            &materials(&value.sources[0]),
            &[],
            &[],
            &[],
        )
        .err()
        .expect("a reference list must be refused");
        assert_eq!(error.code(), "INDEX_CITATION_BURDEN");
    }

    #[test]
    fn a_source_heading_that_does_not_exist_is_refused() {
        let value = project(
            vec![note("note-a", "vault/a.md", "甲")],
            vec![source("src-a", "note-a", "不存在的章节", "资料/教材.md")],
        );
        let spec = &value.notes[0];
        let error = decide(
            &value,
            &[NoteView {
                spec,
                text: clean(),
                sha256: "ffff",
                byte_length: 20,
            }],
            &[],
            &materials(&value.sources[0]),
            &[],
            &[],
            &[],
        )
        .err()
        .expect("an unknown heading must be refused");
        assert_eq!(error.code(), "INDEX_LOCATION_MISSING");
    }

    #[test]
    fn a_source_inside_the_runtime_area_is_refused() {
        let value = project(
            vec![note("note-a", "vault/a.md", "甲")],
            vec![source("src-a", "note-a", "定义", "runtime/教材.md")],
        );
        let spec = &value.notes[0];
        let error = decide(
            &value,
            &[NoteView {
                spec,
                text: clean(),
                sha256: "ffff",
                byte_length: 20,
            }],
            &[],
            &materials(&value.sources[0]),
            &[],
            &[],
            &[],
        )
        .err()
        .expect("a source in the runtime area must be refused");
        assert_eq!(error.code(), "INDEX_AREA_LEAK");
    }

    #[test]
    fn a_renamed_material_is_the_same_source_and_an_edited_one_is_not() {
        let value = project(vec![note("note-a", "vault/a.md", "甲")], sources());
        let spec = &value.notes[0];
        let renamed = [
            Candidate {
                path: "moved/别的名字.md".to_owned(),
                sha256: "aaaa1111".to_owned(),
            },
            Candidate {
                path: "moved/教材.md".to_owned(),
                sha256: "bbbb2222".to_owned(),
            },
        ];
        let decision = decide(
            &value,
            &[NoteView {
                spec,
                text: clean(),
                sha256: "ffff",
                byte_length: 20,
            }],
            &[],
            &materials(&value.sources[0]),
            &[],
            &[],
            &renamed,
        )
        .expect("matching digest must win over the file name");
        assert_eq!(decision.relocation.relocated, 1);
        assert_eq!(decision.relocation.digest_mismatch, 0);
        assert_eq!(
            decision.relocation.sources[0].found.as_deref(),
            Some("moved/别的名字.md")
        );

        let edited = [
            Candidate {
                path: "moved/教材.md".to_owned(),
                sha256: "bbbb2222".to_owned(),
            },
            Candidate {
                path: "moved/无关.md".to_owned(),
                sha256: "cccc3333".to_owned(),
            },
        ];
        let decision = decide(
            &value,
            &[NoteView {
                spec,
                text: clean(),
                sha256: "ffff",
                byte_length: 20,
            }],
            &[],
            &materials(&value.sources[0]),
            &[],
            &[],
            &edited,
        )
        .expect("a same-name different-digest file is a mismatch, not a hit");
        assert_eq!(decision.relocation.relocated, 0);
        assert_eq!(decision.relocation.digest_mismatch, 1);
        assert!(decision.relocation.sources[0].found.is_none());
    }

    #[test]
    fn a_missing_material_asks_for_a_rebuilt_mapping() {
        let value = project(vec![note("note-a", "vault/a.md", "甲")], sources());
        let spec = &value.notes[0];
        let decision = decide(
            &value,
            &[NoteView {
                spec,
                text: clean(),
                sha256: "ffff",
                byte_length: 20,
            }],
            &[],
            &materials(&value.sources[0]),
            &[],
            &[],
            &[],
        )
        .expect("a missing material is a report entry, not a failure");
        assert_eq!(decision.relocation.missing, 1);
        assert!(
            decision.relocation.sources[0]
                .detail
                .contains("需要重建映射")
        );
    }

    #[test]
    fn the_markdown_keeps_the_areas_and_the_boundaries_visible() {
        let value = project(vec![note("note-a", "vault/a.md", "甲")], sources());
        let spec = &value.notes[0];
        let decision = decide(
            &value,
            &[NoteView {
                spec,
                text: clean(),
                sha256: "ffff",
                byte_length: 20,
            }],
            &[],
            &materials(&value.sources[0]),
            &["runtime/a.sqlite".to_owned()],
            &["temp/upload.part".to_owned()],
            &[],
        )
        .expect("clean input must be accepted");
        assert!(decision.markdown.contains("## 区域"));
        assert!(decision.markdown.contains("需要重建映射"));
        assert!(decision.markdown.contains("不判断内容质量与科学正确性"));
        assert_eq!(decision.report.counts.excluded_runtime_entries, 1);
        assert_eq!(decision.report.counts.excluded_temp_entries, 1);
        assert_eq!(decision.report.model_calls, 0);
        assert_eq!(decision.report.network_requests, 0);
    }
}
