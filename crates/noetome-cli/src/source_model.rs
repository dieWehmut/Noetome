//! Read-only source records and immutable snapshot verification.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Component, Path};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::project::{self, ProjectError};
use crate::text::{ParseStatus, SpanKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceFailure {
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceVersion {
    pub(crate) source_id: String,
    pub(crate) version_id: String,
    pub(crate) revision: u64,
    pub(crate) created_at: String,
    pub(crate) media_type: String,
    pub(crate) display_name: String,
    pub(crate) content_sha256: String,
    pub(crate) byte_length: u64,
    pub(crate) normalized_sha256: Option<String>,
    pub(crate) normalization_version: Option<String>,
    pub(crate) ingest_status: String,
    pub(crate) raw_snapshot: String,
    pub(crate) normalized_snapshot: Option<String>,
    pub(crate) error: Option<SourceFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceSpan {
    pub(crate) span_id: String,
    pub(crate) source_version_id: String,
    pub(crate) ordinal: usize,
    pub(crate) kind: SpanKind,
    pub(crate) start_byte: usize,
    pub(crate) end_byte: usize,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) text_sha256: String,
    pub(crate) parser_version: String,
    pub(crate) parse_status: ParseStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImportReport {
    pub(crate) schema_version: u32,
    pub(crate) project_id: String,
    pub(crate) import_id: String,
    pub(crate) imported_at: String,
    pub(crate) selected_path: String,
    pub(crate) source_id: Option<String>,
    pub(crate) version_id: Option<String>,
    pub(crate) status: String,
    pub(crate) reused: bool,
    pub(crate) span_count: u64,
    pub(crate) pending_extraction_count: u64,
    pub(crate) knowledge_status: String,
    pub(crate) version: Option<SourceVersion>,
    pub(crate) spans: Vec<SourceSpan>,
    pub(crate) error: Option<SourceFailure>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Store {
    pub(crate) sources: BTreeMap<String, String>,
    pub(crate) versions: BTreeMap<String, SourceVersion>,
    pub(crate) spans: BTreeMap<String, Vec<SourceSpan>>,
    pub(crate) reports: Vec<ImportReport>,
}

impl Store {
    pub(crate) fn load(connection: &Connection, project_id: &str) -> Result<Self, ProjectError> {
        let mut store = Self::default();
        let mut statement = connection
            .prepare("SELECT source_id, created_at FROM sources ORDER BY rowid")
            .map_err(invalid_record)?;
        let sources = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(invalid_record)?;
        for source in sources {
            let (id, created_at) = source.map_err(invalid_record)?;
            require(
                store.sources.insert(id, created_at).is_none(),
                "来源身份重复",
            )?;
        }
        let mut statement = connection
            .prepare("SELECT version_id, source_id, revision, created_at, media_type, display_name, content_sha256, byte_length, normalized_sha256, normalization_version, ingest_status, raw_snapshot, normalized_snapshot, error_code, error_message FROM source_versions ORDER BY rowid")
            .map_err(invalid_record)?;
        let versions = statement
            .query_map([], |row| {
                Ok(SourceVersion {
                    version_id: row.get(0)?,
                    source_id: row.get(1)?,
                    revision: sql_u64(row, 2)?,
                    created_at: row.get(3)?,
                    media_type: row.get(4)?,
                    display_name: row.get(5)?,
                    content_sha256: row.get(6)?,
                    byte_length: sql_u64(row, 7)?,
                    normalized_sha256: row.get(8)?,
                    normalization_version: row.get(9)?,
                    ingest_status: row.get(10)?,
                    raw_snapshot: row.get(11)?,
                    normalized_snapshot: row.get(12)?,
                    error: sql_failure(row, 13, 14)?,
                })
            })
            .map_err(invalid_record)?;
        for version in versions {
            let version = version.map_err(invalid_record)?;
            store.spans.insert(version.version_id.clone(), Vec::new());
            require(
                store
                    .versions
                    .insert(version.version_id.clone(), version)
                    .is_none(),
                "来源版本身份重复",
            )?;
        }
        let mut statement = connection.prepare("SELECT span_id, source_version_id, ordinal, kind, start_byte, end_byte, start_line, end_line, text_sha256, parser_version, parse_status FROM source_spans ORDER BY source_version_id, ordinal").map_err(invalid_record)?;
        let spans = statement
            .query_map([], |row| {
                let kind: String = row.get(3)?;
                let parse_status: String = row.get(10)?;
                Ok(SourceSpan {
                    span_id: row.get(0)?,
                    source_version_id: row.get(1)?,
                    ordinal: sql_usize(row, 2)?,
                    kind: SpanKind::from_str(&kind).ok_or(rusqlite::Error::InvalidQuery)?,
                    start_byte: sql_usize(row, 4)?,
                    end_byte: sql_usize(row, 5)?,
                    start_line: sql_usize(row, 6)?,
                    end_line: sql_usize(row, 7)?,
                    text_sha256: row.get(8)?,
                    parser_version: row.get(9)?,
                    parse_status: ParseStatus::from_str(&parse_status)
                        .ok_or(rusqlite::Error::InvalidQuery)?,
                })
            })
            .map_err(invalid_record)?;
        for span in spans {
            let span = span.map_err(invalid_record)?;
            store
                .spans
                .get_mut(&span.source_version_id)
                .ok_or_else(|| invalid_record("Span 引用不存在的来源版本"))?
                .push(span);
        }
        let mut statement = connection.prepare("SELECT import_id, imported_at, selected_path, source_id, version_id, status, reused, span_count, pending_extraction_count, knowledge_status, error_code, error_message FROM source_imports ORDER BY rowid").map_err(invalid_record)?;
        let reports = statement
            .query_map([], |row| {
                let reused = match row.get::<_, i64>(6)? {
                    0 => false,
                    1 => true,
                    _ => return Err(rusqlite::Error::InvalidQuery),
                };
                Ok((
                    ImportReport {
                        schema_version: 1,
                        project_id: project_id.into(),
                        import_id: row.get(0)?,
                        imported_at: row.get(1)?,
                        selected_path: row.get(2)?,
                        source_id: row.get(3)?,
                        version_id: row.get(4)?,
                        status: row.get(5)?,
                        reused,
                        span_count: sql_u64(row, 7)?,
                        pending_extraction_count: sql_u64(row, 8)?,
                        knowledge_status: row.get(9)?,
                        version: None,
                        spans: Vec::new(),
                        error: sql_failure(row, 10, 11)?,
                    },
                    sql_usize(row, 7)?,
                ))
            })
            .map_err(invalid_record)?;
        for report in reports {
            let (mut report, span_count) = report.map_err(invalid_record)?;
            if let Some(id) = &report.version_id {
                report.version = Some(
                    store
                        .versions
                        .get(id)
                        .ok_or_else(|| invalid_record("导入引用不存在的版本"))?
                        .clone(),
                );
                report.spans = store
                    .spans
                    .get(id)
                    .ok_or_else(|| invalid_record("导入缺少来源位置清单"))?
                    .clone();
            }
            require(
                span_count == report.spans.len()
                    && report.pending_extraction_count == report.spans.len() as u64,
                "导入 Span 或待抽取计数与实际记录不一致",
            )?;
            store.reports.push(report);
        }
        store.validate_records(project_id)?;
        Ok(store)
    }

    pub(crate) fn validate_snapshots(
        &self,
        root: &Path,
        project_id: &str,
        staged: Option<(&str, &Path)>,
    ) -> Result<(), ProjectError> {
        self.validate_records(project_id)?;
        let root = project::check_components(root)?;
        let root_identity = directory(&root)?;
        if let Some((version, stage)) = staged {
            require(
                self.versions.contains_key(version),
                "暂存版本不在本轮登记清单中",
            )?;
            if stage != root.join(".noetome-writing").join("version") {
                return Err(snapshot_conflict("暂存快照必须位于本项目的写入目录"));
            }
        }
        let expected_versions: BTreeSet<_> = self
            .versions
            .keys()
            .filter(|id| staged.is_none_or(|(staged_id, _)| id.as_str() != staged_id))
            .cloned()
            .collect();
        let cache = root.join(".noetome-sources");
        match fs::symlink_metadata(&cache) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !expected_versions.is_empty() {
                    return Err(snapshot_conflict("已登记版本的来源快照目录缺失"));
                }
            }
            Err(error) => return Err(ProjectError::io(&cache, error)),
            Ok(_) => {
                let identity = directory(&cache)?;
                let mut names: BTreeSet<OsString> =
                    expected_versions.iter().map(OsString::from).collect();
                names.insert(OsString::from(".owner.json"));
                exact_names(&cache, &names)?;
                let owner_bytes = read_snapshot(&cache.join(".owner.json"), 4096, None)?;
                let owner: CacheOwner = serde_json::from_slice(&owner_bytes)
                    .map_err(|_| snapshot_conflict("来源快照所有权文件无效或包含未知字段"))?;
                if owner.schema_version != 1 || owner.project_id != project_id {
                    return Err(snapshot_conflict("来源快照目录不属于此项目"));
                }
                same_directory(&cache, &identity)?;
            }
        }
        for version in self.versions.values() {
            let location = staged
                .filter(|(id, _)| *id == version.version_id)
                .map_or_else(
                    || cache.join(&version.version_id),
                    |(_, path)| path.to_path_buf(),
                );
            self.validate_version_snapshot(version, &location)?;
        }
        same_directory(&root, &root_identity)?;
        Ok(())
    }

    fn validate_records(&self, project_id: &str) -> Result<(), ProjectError> {
        require(valid_uuid(project_id), "项目 ID 无效")?;
        for (source, created_at) in &self.sources {
            require(
                valid_uuid(source) && valid_time(created_at),
                "来源身份或 UTC 时间无效",
            )?;
            let mut revisions: Vec<_> = self
                .versions
                .values()
                .filter(|version| version.source_id == *source)
                .map(|version| version.revision)
                .collect();
            revisions.sort_unstable();
            require(
                !revisions.is_empty()
                    && revisions
                        .iter()
                        .enumerate()
                        .all(|(index, revision)| *revision == index as u64 + 1),
                "来源修订必须从 1 连续递增且每个来源至少一个版本",
            )?;
        }
        require(
            self.spans.len() == self.versions.len(),
            "来源位置清单与版本清单不一致",
        )?;
        let mut global_span_ids = BTreeSet::new();
        for (id, version) in &self.versions {
            require(
                id == &version.version_id
                    && valid_uuid(id)
                    && self.sources.contains_key(&version.source_id),
                "来源版本身份或引用无效",
            )?;
            require(
                version.revision > 0 && valid_time(&version.created_at),
                "来源版本修订或 UTC 时间无效",
            )?;
            require(
                matches!(version.media_type.as_str(), "text/plain" | "text/markdown")
                    && !version.display_name.trim().is_empty(),
                "来源类型或显示名称无效",
            )?;
            require(
                valid_hash(&version.content_sha256) && version.byte_length <= MAX_RAW_BYTES,
                "原始来源指纹或长度无效",
            )?;
            require(
                version.raw_snapshot == format!(".noetome-sources/{id}/raw.bin"),
                "原始快照位置不符合固定版本目录",
            )?;
            match version.ingest_status.as_str() {
                "ready" => {
                    require(
                        version.error.is_none()
                            && version.normalization_version.as_deref()
                                == Some(crate::text::NORMALIZATION_VERSION)
                            && version.normalized_sha256.as_deref().is_some_and(valid_hash)
                            && version.normalized_snapshot.as_deref()
                                == Some(format!(".noetome-sources/{id}/normalized.txt").as_str()),
                        "成功来源的规范文本或错误字段不一致",
                    )?;
                }
                "failed" => {
                    require(
                        version.normalized_sha256.is_none()
                            && version.normalized_snapshot.is_none()
                            && version.normalization_version.is_none()
                            && version.error.as_ref().is_some_and(|error| {
                                valid_failure(error) && error.code == "SOURCE_INVALID_UTF8"
                            }),
                        "失败来源只能保存真实 UTF-8 失败和原始快照",
                    )?;
                }
                _ => return Err(invalid_record("来源处理状态无效")),
            }
            let spans = self
                .spans
                .get(id)
                .ok_or_else(|| invalid_record("版本缺少位置清单"))?;
            if version.ingest_status == "failed" {
                require(spans.is_empty(), "失败来源不允许规范文本 Span")?;
            }
            for (ordinal, span) in spans.iter().enumerate() {
                require(
                    valid_uuid(&span.span_id) && global_span_ids.insert(span.span_id.clone()),
                    "Span 身份无效或重复",
                )?;
                require(
                    span.source_version_id == *id && span.ordinal == ordinal,
                    "Span 版本引用或顺序无效",
                )?;
                require(
                    span.start_byte < span.end_byte
                        && span.end_byte as u64 <= MAX_RAW_BYTES
                        && span.start_line > 0
                        && span.end_line >= span.start_line
                        && valid_hash(&span.text_sha256)
                        && span.parser_version == crate::text::PARSER_VERSION,
                    "Span 区间、行号、指纹或解析规则无效",
                )?;
            }
        }
        let mut seen_import_ids = BTreeSet::new();
        let mut imported_versions = BTreeSet::new();
        for report in &self.reports {
            require(
                report.schema_version == 1
                    && report.project_id == project_id
                    && valid_uuid(&report.import_id)
                    && seen_import_ids.insert(report.import_id.clone())
                    && valid_time(&report.imported_at)
                    && report.knowledge_status == "pending",
                "导入身份、版本、UTC 时间或知识状态无效",
            )?;
            require(
                report.span_count == report.spans.len() as u64
                    && report.pending_extraction_count == report.spans.len() as u64,
                "导入 Span 或待抽取计数与实际记录不一致",
            )?;
            let selected = Path::new(&report.selected_path);
            require(
                !report.selected_path.is_empty()
                    && selected.is_absolute()
                    && !report.selected_path.contains('\0')
                    && !selected
                        .components()
                        .any(|component| matches!(component, Component::ParentDir)),
                "导入的明确选取路径必须为非空绝对路径",
            )?;
            require(
                report
                    .source_id
                    .as_ref()
                    .is_none_or(|id| self.sources.contains_key(id)),
                "导入引用不存在的来源",
            )?;
            match &report.version_id {
                Some(id) => {
                    let version = self
                        .versions
                        .get(id)
                        .ok_or_else(|| invalid_record("导入引用不存在的来源版本"))?;
                    require(
                        report.source_id.as_ref() == Some(&version.source_id)
                            && report.version.as_ref() == Some(version)
                            && self.spans.get(id) == Some(&report.spans)
                            && report.status == version.ingest_status
                            && report.error == version.error,
                        "导入、来源版本和位置清单状态不一致",
                    )?;
                    require(
                        report.reused == imported_versions.contains(id),
                        "复用必须对应此前已经登记的同一版本",
                    )?;
                    imported_versions.insert(id.clone());
                }
                None => {
                    require(
                        report.version.is_none()
                            && report.spans.is_empty()
                            && !report.reused
                            && report.status == "failed"
                            && report.error.as_ref().is_some_and(|error| {
                                valid_failure(error)
                                    && matches!(
                                        error.code.as_str(),
                                        "SOURCE_READ_ERROR" | "SOURCE_TOO_LARGE" | "SOURCE_CHANGED"
                                    )
                            }),
                        "未完整读取的导入只能保存真实失败尝试",
                    )?;
                }
            }
        }
        require(
            imported_versions.len() == self.versions.len(),
            "每个来源版本必须有初次导入记录",
        )?;
        Ok(())
    }

    fn validate_version_snapshot(
        &self,
        version: &SourceVersion,
        location: &Path,
    ) -> Result<(), ProjectError> {
        let identity = directory(location)?;
        let mut expected = BTreeSet::from([OsString::from("raw.bin")]);
        if version.ingest_status == "ready" {
            expected.insert(OsString::from("normalized.txt"));
        }
        exact_names(location, &expected)?;
        let raw = read_snapshot(
            &location.join("raw.bin"),
            MAX_RAW_BYTES,
            Some(version.byte_length),
        )?;
        if raw.len() as u64 != version.byte_length
            || crate::text::sha256(&raw) != version.content_sha256
        {
            return Err(invalid_snapshot("原始快照字节、长度与版本指纹不一致"));
        }
        match crate::text::analyze(&raw) {
            Ok(parsed) if version.ingest_status == "ready" => {
                let normalized = read_snapshot(
                    &location.join("normalized.txt"),
                    MAX_RAW_BYTES,
                    Some(parsed.text.len() as u64),
                )?;
                if normalized != parsed.text.as_bytes()
                    || version.normalized_sha256.as_deref()
                        != Some(parsed.normalized_sha256.as_str())
                {
                    return Err(invalid_snapshot("规范文本快照与原始字节及规范指纹不一致"));
                }
                let spans = self
                    .spans
                    .get(&version.version_id)
                    .ok_or_else(|| invalid_record("版本缺少位置清单"))?;
                if spans.len() != parsed.spans.len()
                    || !spans.iter().zip(&parsed.spans).all(|(span, expected)| {
                        span.kind == expected.kind
                            && span.start_byte == expected.start_byte
                            && span.end_byte == expected.end_byte
                            && span.start_line == expected.start_line
                            && span.end_line == expected.end_line
                            && span.text_sha256 == expected.text_sha256
                            && span.parse_status == expected.parse_status
                    })
                {
                    return Err(invalid_snapshot("Span 清单没有准确完整覆盖实际规范文本"));
                }
            }
            Err(_) if version.ingest_status == "failed" => {}
            _ => return Err(invalid_snapshot("UTF-8 解码结果与来源处理状态不一致")),
        }
        same_directory(location, &identity)?;
        Ok(())
    }
}

const MAX_RAW_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheOwner {
    schema_version: u32,
    project_id: String,
}
fn invalid_record(message: impl std::fmt::Display) -> ProjectError {
    ProjectError::new(
        "SOURCE_INVALID_RECORD",
        2,
        format!("来源记录无效：{message}。没有修改数据库或快照。"),
    )
}
fn invalid_snapshot(message: &str) -> ProjectError {
    ProjectError::new(
        "SOURCE_INVALID_SNAPSHOT",
        2,
        format!("来源快照校验失败：{message}。原内容保留。"),
    )
}
fn snapshot_conflict(message: &str) -> ProjectError {
    ProjectError::new(
        "SOURCE_SNAPSHOT_CONFLICT",
        3,
        format!("来源快照目录冲突：{message}。不会覆盖或接管。"),
    )
}
fn require(condition: bool, message: &str) -> Result<(), ProjectError> {
    if condition {
        Ok(())
    } else {
        Err(invalid_record(message))
    }
}
fn valid_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| {
        id.get_version_num() == 4
            && id.get_variant() == uuid::Variant::RFC4122
            && id.to_string() == value
    })
}
fn valid_time(value: &str) -> bool {
    OffsetDateTime::parse(value, &Rfc3339).is_ok_and(|time| time.offset() == UtcOffset::UTC)
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn valid_failure(error: &SourceFailure) -> bool {
    !error.code.trim().is_empty() && !error.message.trim().is_empty()
}
fn sql_u64(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    u64::try_from(row.get::<_, i64>(index)?).map_err(|_| rusqlite::Error::InvalidQuery)
}
fn sql_usize(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<usize> {
    usize::try_from(row.get::<_, i64>(index)?).map_err(|_| rusqlite::Error::InvalidQuery)
}
fn sql_failure(
    row: &rusqlite::Row<'_>,
    code_index: usize,
    message_index: usize,
) -> rusqlite::Result<Option<SourceFailure>> {
    match (
        row.get::<_, Option<String>>(code_index)?,
        row.get::<_, Option<String>>(message_index)?,
    ) {
        (None, None) => Ok(None),
        (Some(code), Some(message)) => Ok(Some(SourceFailure { code, message })),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}
fn directory(path: &Path) -> Result<same_file::Handle, ProjectError> {
    project::check_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            snapshot_conflict("已登记快照目录缺失")
        } else {
            ProjectError::io(path, error)
        }
    })?;
    if project::is_link(&metadata) {
        return Err(ProjectError::unsafe_path(path));
    }
    if !metadata.is_dir() {
        return Err(snapshot_conflict("快照路径不是普通目录"));
    }
    same_file::Handle::from_path(path).map_err(|error| ProjectError::io(path, error))
}
fn same_directory(path: &Path, identity: &same_file::Handle) -> Result<(), ProjectError> {
    if directory(path)? != *identity {
        return Err(snapshot_conflict("校验期间目录身份发生变化"));
    }
    Ok(())
}
fn exact_names(path: &Path, expected: &BTreeSet<OsString>) -> Result<(), ProjectError> {
    let names = fs::read_dir(path)
        .map_err(|error| ProjectError::io(path, error))?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<BTreeSet<_>>>()
        .map_err(|error| ProjectError::io(path, error))?;
    if &names != expected {
        return Err(snapshot_conflict("快照清单存在缺失或未知内容"));
    }
    Ok(())
}
fn read_snapshot(
    path: &Path,
    limit: u64,
    expected_length: Option<u64>,
) -> Result<Vec<u8>, ProjectError> {
    project::check_components(path)?;
    let before = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            snapshot_conflict("已登记快照文件缺失")
        } else {
            ProjectError::io(path, error)
        }
    })?;
    if project::is_link(&before) {
        return Err(ProjectError::unsafe_path(path));
    }
    if !before.is_file() {
        return Err(snapshot_conflict("快照必须是普通文件"));
    }
    if before.len() > limit || expected_length.is_some_and(|expected| before.len() != expected) {
        return Err(invalid_snapshot("快照文件超出受限读取长度"));
    }
    let guard = project::read_guard(path)?;
    let opened_metadata = guard
        .metadata()
        .map_err(|error| ProjectError::io(path, error))?;
    if opened_metadata.len() > limit
        || opened_metadata.len() != before.len()
        || expected_length.is_some_and(|expected| opened_metadata.len() != expected)
    {
        return Err(invalid_snapshot("已打开快照长度与声明或读取上限不一致"));
    }
    let opened = same_file::Handle::from_file(
        guard
            .try_clone()
            .map_err(|error| ProjectError::io(path, error))?,
    )
    .map_err(|error| ProjectError::io(path, error))?;
    let current =
        same_file::Handle::from_path(path).map_err(|error| ProjectError::io(path, error))?;
    if opened != current {
        return Err(snapshot_conflict("快照文件身份发生变化"));
    }
    let mut bytes = Vec::new();
    (&guard)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ProjectError::io(path, error))?;
    let after = guard
        .metadata()
        .map_err(|error| ProjectError::io(path, error))?;
    let current =
        same_file::Handle::from_path(path).map_err(|error| ProjectError::io(path, error))?;
    if opened != current
        || bytes.len() as u64 > limit
        || bytes.len() as u64 != before.len()
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err(invalid_snapshot("读取期间快照身份、字节或长度发生变化"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    use rusqlite::params;
    use sha2::{Digest, Sha256};
    use uuid::Uuid;

    const PROJECT: &str = "5232018c-42e9-4db6-8e2e-58d8e40d9e66";
    const SOURCE: &str = "5232018c-42e9-4db6-8e2e-58d8e40d9e67";
    const VERSION: &str = "37d03d00-ff4c-4bdf-b126-969a4b073e3f";
    const IMPORT: &str = "12facf7a-4fbd-42a7-8c2f-f96208bf740f";
    const TIME: &str = "2026-11-01T00:00:00Z";
    const RAW: &[u8] = b"# A\r\nbody\r\n";
    const NORMALIZED: &[u8] = b"# A\nbody\n";

    struct Sandbox(PathBuf);
    impl Sandbox {
        fn new() -> Self {
            let parent =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/source-model-tests");
            fs::create_dir_all(&parent).unwrap();
            let root = parent
                .canonicalize()
                .unwrap()
                .join(Uuid::new_v4().to_string());
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn digest(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }
    fn fixture_store() -> Store {
        let version = SourceVersion {
            source_id: SOURCE.into(),
            version_id: VERSION.into(),
            revision: 1,
            created_at: TIME.into(),
            media_type: "text/markdown".into(),
            display_name: "fixture.md".into(),
            content_sha256: digest(RAW),
            byte_length: RAW.len() as u64,
            normalized_sha256: Some(digest(NORMALIZED)),
            normalization_version: Some(crate::text::NORMALIZATION_VERSION.into()),
            ingest_status: "ready".into(),
            raw_snapshot: format!(".noetome-sources/{VERSION}/raw.bin"),
            normalized_snapshot: Some(format!(".noetome-sources/{VERSION}/normalized.txt")),
            error: None,
        };
        let spans = vec![
            SourceSpan {
                span_id: "d4cc1111-0812-4bac-9fc1-c71782669045".into(),
                source_version_id: VERSION.into(),
                ordinal: 0,
                kind: SpanKind::Heading,
                start_byte: 0,
                end_byte: 4,
                start_line: 1,
                end_line: 1,
                text_sha256: digest(b"# A\n"),
                parser_version: crate::text::PARSER_VERSION.into(),
                parse_status: ParseStatus::Parsed,
            },
            SourceSpan {
                span_id: "d4cc1111-0812-4bac-9fc1-c71782669046".into(),
                source_version_id: VERSION.into(),
                ordinal: 1,
                kind: SpanKind::Text,
                start_byte: 4,
                end_byte: 9,
                start_line: 2,
                end_line: 2,
                text_sha256: digest(b"body\n"),
                parser_version: crate::text::PARSER_VERSION.into(),
                parse_status: ParseStatus::Parsed,
            },
        ];
        let report = ImportReport {
            schema_version: 1,
            project_id: PROJECT.into(),
            import_id: IMPORT.into(),
            imported_at: TIME.into(),
            selected_path: "D:/selected/fixture.md".into(),
            source_id: Some(SOURCE.into()),
            version_id: Some(VERSION.into()),
            status: "ready".into(),
            reused: false,
            span_count: 2,
            pending_extraction_count: 2,
            knowledge_status: "pending".into(),
            version: Some(version.clone()),
            spans: spans.clone(),
            error: None,
        };
        Store {
            sources: BTreeMap::from([(SOURCE.into(), TIME.into())]),
            versions: BTreeMap::from([(VERSION.into(), version)]),
            spans: BTreeMap::from([(VERSION.into(), spans)]),
            reports: vec![report],
        }
    }
    fn database(store: &Store) -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(crate::source_schema::SCHEMA)
            .unwrap();
        connection
            .execute(
                "INSERT INTO project_metadata VALUES(1,?1,2,?2,'initialized')",
                params![PROJECT, TIME],
            )
            .unwrap();
        for (source, time) in &store.sources {
            connection
                .execute("INSERT INTO sources VALUES(?1,?2)", params![source, time])
                .unwrap();
        }
        for version in store.versions.values() {
            connection.execute("INSERT INTO source_versions VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                params![version.version_id,version.source_id,i64::try_from(version.revision).unwrap(),version.created_at,
                    version.media_type,version.display_name,version.content_sha256,i64::try_from(version.byte_length).unwrap(),
                    version.normalized_sha256,version.normalization_version,version.ingest_status,
                    version.raw_snapshot,version.normalized_snapshot,
                    version.error.as_ref().map(|error|&error.code),version.error.as_ref().map(|error|&error.message)]).unwrap();
        }
        for span in store.spans.values().flatten() {
            connection
                .execute(
                    "INSERT INTO source_spans VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                    params![
                        span.span_id,
                        span.source_version_id,
                        i64::try_from(span.ordinal).unwrap(),
                        span.kind.as_str(),
                        i64::try_from(span.start_byte).unwrap(),
                        i64::try_from(span.end_byte).unwrap(),
                        i64::try_from(span.start_line).unwrap(),
                        i64::try_from(span.end_line).unwrap(),
                        span.text_sha256,
                        span.parser_version,
                        span.parse_status.as_str()
                    ],
                )
                .unwrap();
        }
        for report in &store.reports {
            connection
                .execute(
                    "INSERT INTO source_imports VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                    params![
                        report.import_id,
                        report.imported_at,
                        report.selected_path,
                        report.source_id,
                        report.version_id,
                        report.status,
                        report.reused,
                        i64::try_from(report.span_count).unwrap(),
                        i64::try_from(report.pending_extraction_count).unwrap(),
                        report.knowledge_status,
                        report.error.as_ref().map(|error| &error.code),
                        report.error.as_ref().map(|error| &error.message)
                    ],
                )
                .unwrap();
        }
        connection
    }
    fn snapshots(root: &Path) {
        let cache = root.join(".noetome-sources");
        fs::create_dir(&cache).unwrap();
        fs::write(
            cache.join(".owner.json"),
            format!("{{\"schema_version\":1,\"project_id\":\"{PROJECT}\"}}"),
        )
        .unwrap();
        let version = cache.join(VERSION);
        fs::create_dir(&version).unwrap();
        fs::write(version.join("raw.bin"), RAW).unwrap();
        fs::write(version.join("normalized.txt"), NORMALIZED).unwrap();
    }

    #[test]
    fn records_reject_invalid_hashes_and_reuse_before_the_first_import() {
        let store = fixture_store();
        for mutation in [
            "UPDATE source_versions SET normalized_sha256='invalid-sha'",
            "UPDATE source_imports SET reused=1",
            "UPDATE source_imports SET span_count=3, pending_extraction_count=3",
            "UPDATE source_versions SET revision=2",
            "UPDATE source_imports SET imported_at='2026-11-01T08:00:00+08:00'",
            "UPDATE source_imports SET selected_path='relative/source.md'",
            "UPDATE source_versions SET display_name=''",
            "UPDATE source_versions SET raw_snapshot='../outside.bin'",
        ] {
            let connection = database(&store);
            connection.execute_batch(mutation).unwrap();
            assert!(
                Store::load(&connection, PROJECT).is_err(),
                "accepted {mutation}"
            );
        }
    }

    #[test]
    fn pending_count_constraint_rejects_a_different_count_without_changing_the_report() {
        let connection = database(&fixture_store());
        let before = Store::load(&connection, PROJECT).unwrap();
        let error = connection
            .execute("UPDATE source_imports SET pending_extraction_count=3", [])
            .unwrap_err();
        assert_eq!(
            error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::ConstraintViolation)
        );
        assert_eq!(Store::load(&connection, PROJECT).unwrap(), before);
        assert_eq!(before.reports[0].span_count, 2);
        assert_eq!(before.reports[0].pending_extraction_count, 2);
    }

    #[test]
    fn snapshots_reject_unknown_cache_content_and_invalid_owner() {
        let sandbox = Sandbox::new();
        snapshots(&sandbox.0);
        let store = fixture_store();
        let cache = sandbox.0.join(".noetome-sources");
        fs::write(cache.join("human.txt"), b"preserve unknown human content").unwrap();
        let error = store
            .validate_snapshots(&sandbox.0, PROJECT, None)
            .unwrap_err();
        assert_eq!(error.code(), "SOURCE_SNAPSHOT_CONFLICT");
        assert_eq!(
            fs::read(cache.join("human.txt")).unwrap(),
            b"preserve unknown human content"
        );
        fs::remove_file(cache.join("human.txt")).unwrap();
        fs::write(
            cache.join(".owner.json"),
            format!("{{\"schema_version\":1,\"project_id\":\"{PROJECT}\",\"unknown\":true}}"),
        )
        .unwrap();
        assert_eq!(
            store
                .validate_snapshots(&sandbox.0, PROJECT, None)
                .unwrap_err()
                .code(),
            "SOURCE_SNAPSHOT_CONFLICT"
        );
    }

    #[test]
    fn staged_snapshot_validation_is_restricted_to_the_current_writer_directory() {
        let sandbox = Sandbox::new();
        let store = fixture_store();
        let marker = sandbox.0.join(".noetome-writing");
        fs::create_dir(&marker).unwrap();
        let staging = marker.join("version");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("raw.bin"), RAW).unwrap();
        fs::write(staging.join("normalized.txt"), NORMALIZED).unwrap();
        store
            .validate_snapshots(&sandbox.0, PROJECT, Some((VERSION, &staging)))
            .unwrap();
        let outside = sandbox.0.join("unrelated-stage");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("raw.bin"), RAW).unwrap();
        fs::write(outside.join("normalized.txt"), NORMALIZED).unwrap();
        assert!(
            store
                .validate_snapshots(&sandbox.0, PROJECT, Some((VERSION, &outside)))
                .is_err()
        );
        assert!(
            store
                .validate_snapshots(&sandbox.0, PROJECT, Some((SOURCE, &staging)))
                .is_err()
        );
    }

    #[test]
    fn failed_utf8_versions_have_only_real_raw_bytes_and_no_spans() {
        let sandbox = Sandbox::new();
        snapshots(&sandbox.0);
        let mut expected = fixture_store();
        let error = SourceFailure {
            code: "SOURCE_INVALID_UTF8".into(),
            message: "invalid byte at 0".into(),
        };
        let version = expected.versions.get_mut(VERSION).unwrap();
        version.byte_length = 1;
        version.content_sha256 = digest(&[0xff]);
        version.normalized_sha256 = None;
        version.normalization_version = None;
        version.normalized_snapshot = None;
        version.ingest_status = "failed".into();
        version.error = Some(error.clone());
        expected.spans.get_mut(VERSION).unwrap().clear();
        expected.reports[0].version = Some(version.clone());
        expected.reports[0].spans.clear();
        expected.reports[0].span_count = 0;
        expected.reports[0].pending_extraction_count = 0;
        expected.reports[0].status = "failed".into();
        expected.reports[0].error = Some(error);
        let cache = sandbox.0.join(".noetome-sources").join(VERSION);
        fs::write(cache.join("raw.bin"), [0xff]).unwrap();
        fs::remove_file(cache.join("normalized.txt")).unwrap();
        let connection = database(&expected);
        let loaded = Store::load(&connection, PROJECT).unwrap();
        assert_eq!(loaded, expected);
        loaded
            .validate_snapshots(&sandbox.0, PROJECT, None)
            .unwrap();
        fs::write(cache.join("raw.bin"), b"valid").unwrap();
        assert!(
            loaded
                .validate_snapshots(&sandbox.0, PROJECT, None)
                .is_err()
        );
    }

    #[test]
    fn snapshots_reject_changed_raw_bytes_and_incomplete_span_inventory() {
        let sandbox = Sandbox::new();
        snapshots(&sandbox.0);
        let mut store = fixture_store();
        fs::write(
            sandbox
                .0
                .join(format!(".noetome-sources/{VERSION}/raw.bin")),
            b"changed",
        )
        .unwrap();
        assert!(store.validate_snapshots(&sandbox.0, PROJECT, None).is_err());
        fs::write(
            sandbox
                .0
                .join(format!(".noetome-sources/{VERSION}/raw.bin")),
            RAW,
        )
        .unwrap();
        store.spans.get_mut(VERSION).unwrap().pop();
        assert!(store.validate_snapshots(&sandbox.0, PROJECT, None).is_err());
    }

    #[test]
    fn loading_and_validating_real_snapshots_preserves_complete_records() {
        let sandbox = Sandbox::new();
        snapshots(&sandbox.0);
        let expected = fixture_store();
        let connection = database(&expected);
        let store = Store::load(&connection, PROJECT).unwrap();
        assert_eq!(store, expected);
        let raw_path = sandbox
            .0
            .join(format!(".noetome-sources/{VERSION}/raw.bin"));
        let before = fs::metadata(&raw_path).unwrap().modified().unwrap();
        store.validate_snapshots(&sandbox.0, PROJECT, None).unwrap();
        assert_eq!(fs::read(&raw_path).unwrap(), RAW);
        assert_eq!(fs::metadata(&raw_path).unwrap().modified().unwrap(), before);
    }
}
