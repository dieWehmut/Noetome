//! Explicit one-file registration. Ingestion never runs knowledge extraction.
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, params};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::project::{self, ProjectError, ProjectInfo};
use crate::source_model::{ImportReport, SourceFailure, SourceSpan, SourceVersion, Store};
use crate::{source_schema, source_write::Writer, text};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const INSERT_VERSION: &str = "INSERT INTO source_versions
    (version_id,source_id,revision,created_at,media_type,display_name,content_sha256,
    byte_length,normalized_sha256,normalization_version,ingest_status,raw_snapshot,
    normalized_snapshot,error_code,error_message)
    VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)";
const INSERT_SPAN: &str = "INSERT INTO source_spans
    (span_id,source_version_id,ordinal,kind,start_byte,end_byte,start_line,end_line,
    text_sha256,parser_version,parse_status)
    VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)";
const INSERT_IMPORT: &str = "INSERT INTO source_imports
    (import_id,imported_at,selected_path,source_id,version_id,status,reused,span_count,
    pending_extraction_count,knowledge_status,error_code,error_message)
    VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)";

pub(crate) fn sql_integer(value: u64) -> Result<i64, ProjectError> {
    i64::try_from(value).map_err(|_| {
        ProjectError::new(
            "SOURCE_INVALID_RECORD",
            2,
            "来源数字超出SQLite有符号整数范围。",
        )
    })
}

pub struct CommandOutput {
    pub json: String,
    pub failure: Option<(String, String)>,
}

fn empty_store() -> Store {
    Store::default()
}

pub(crate) fn invalid_id(id: &str) -> Result<(), ProjectError> {
    if Uuid::parse_str(id).is_ok_and(|parsed| {
        parsed.get_version_num() == 4
            && parsed.get_variant() == uuid::Variant::RFC4122
            && parsed.to_string() == id
    }) {
        Ok(())
    } else {
        Err(ProjectError::new(
            "SOURCE_INVALID_ID",
            2,
            "来源及导入身份须为已登记的标准小写UUID v4；不能用名称或路径作为ID。",
        ))
    }
}

pub(crate) fn not_found() -> ProjectError {
    ProjectError::new(
        "SOURCE_NOT_FOUND",
        2,
        "该来源或导入身份不在此项目中；请使用同一项目已经登记的ID。",
    )
}

fn database_store(
    path: &Path,
    root: &Path,
    info: &ProjectInfo,
    staged: Option<(&str, &Path)>,
) -> Result<Store, ProjectError> {
    database_store_staged(path, root, info, staged, None)
}
pub(crate) fn database_store_staged(
    path: &Path,
    root: &Path,
    info: &ProjectInfo,
    staged: Option<(&str, &Path)>,
    staged_parse: Option<(&str, &Path)>,
) -> Result<Store, ProjectError> {
    project::checked_file(path)?;
    let guard = project::read_guard(path)?;
    let before = guard
        .metadata()
        .map_err(|error| ProjectError::io(path, error))?;
    let opened = same_file::Handle::from_file(
        guard
            .try_clone()
            .map_err(|error| ProjectError::io(path, error))?,
    )
    .map_err(|error| ProjectError::io(path, error))?;
    let canonical = path
        .canonicalize()
        .map_err(|error| ProjectError::io(path, error))?;
    let mut uri =
        url::Url::from_file_path(&canonical).map_err(|_| ProjectError::unsafe_path(path))?;
    uri.query_pairs_mut()
        .append_pair("mode", "ro")
        .append_pair("immutable", "1");
    let connection = Connection::open_with_flags(
        uri.as_str(),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(ProjectError::database)?;
    let result = (|| {
        connection
            .execute_batch("PRAGMA temp_store=MEMORY;")
            .map_err(ProjectError::database)?;
        let stored = connection.query_row("SELECT project_id,schema_version,created_at,state FROM project_metadata WHERE singleton=1", [], |row| Ok(ProjectInfo {project_id:row.get(0)?,schema_version:row.get(1)?,created_at:row.get(2)?,state:row.get(3)?,database:project::DATABASE.into()})).map_err(ProjectError::database)?;
        if &stored != info {
            return Err(ProjectError::new(
                "PROJECT_SCHEMA_MISMATCH",
                2,
                "读取来源时项目元数据与已校验配置不一致。",
            ));
        }
        let mut store = if matches!(info.schema_version, 2 | 3) {
            source_schema::validate(&connection)?;
            Store::load(&connection, &info.project_id)?
        } else {
            empty_store()
        };
        store.validate_snapshots_staged(root, &info.project_id, staged, staged_parse)?;
        store.hydrate_pdf_reports(root, staged_parse)?;
        Ok(store)
    })();
    connection
        .close()
        .map_err(|(_, error)| ProjectError::database(error))?;
    let after = guard
        .metadata()
        .map_err(|error| ProjectError::io(path, error))?;
    let current =
        same_file::Handle::from_path(path).map_err(|error| ProjectError::io(path, error))?;
    if opened != current
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err(ProjectError::new(
            "PROJECT_DATABASE_BUSY",
            3,
            "读取来源期间项目数据库发生变化；请关闭写入程序后重试。",
        ));
    }
    result
}

pub(crate) fn current_store(root: &Path) -> Result<(ProjectInfo, Store), ProjectError> {
    let info = project::inspect(root)?;
    let _config_guard = project::read_guard(&root.join(project::CONFIG))?;
    let _database_guard = project::read_guard(&root.join(project::DATABASE))?;
    if project::inspect(root)? != info {
        return Err(ProjectError::new(
            "PROJECT_DATABASE_BUSY",
            3,
            "项目配置或数据库在来源读取前发生变化。",
        ));
    }
    let store = database_store(&root.join(project::DATABASE), root, &info, None)?;
    project::reject_sidecars(root)?;
    Ok((info, store))
}

fn report_json(report: &ImportReport) -> Result<String, ProjectError> {
    serde_json::to_string_pretty(report)
        .map(|json| format!("{json}\n"))
        .map_err(|error| {
            ProjectError::new(
                "SOURCE_REPORT_ERROR",
                1,
                format!("无法序列化导入结果：{error}"),
            )
        })
}

pub fn inspect(path: &Path, import_id: &str) -> Result<String, ProjectError> {
    inspect_output(path, import_id).map(|output| output.json)
}
pub fn inspect_output(path: &Path, import_id: &str) -> Result<CommandOutput, ProjectError> {
    invalid_id(import_id)?;
    let root = project::check_components(path)?;
    let (_, store) = current_store(&root)?;
    if let Some(report) = store
        .pdf
        .reports
        .iter()
        .find(|report| report.import_id == import_id)
    {
        return crate::pdf_source::output(report);
    }
    let report = store
        .reports
        .iter()
        .find(|report| report.import_id == import_id)
        .ok_or_else(not_found)?;
    Ok(CommandOutput {
        json: report_json(report)?,
        failure: None,
    })
}

pub(crate) struct Selected {
    pub(crate) path: PathBuf,
    pub(crate) media_type: &'static str,
    pub(crate) raw: Option<Vec<u8>>,
    pub(crate) failure: Option<SourceFailure>,
}

fn read_selected(path: &Path) -> Result<Selected, ProjectError> {
    let absolute = project::check_components(path)?;
    let extension = absolute
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let media_type = match extension.as_str() {
        "md" => "text/markdown",
        "txt" => "text/plain",
        "pdf" => "application/pdf",
        _ => {
            return Err(ProjectError::new(
                "SOURCE_UNSUPPORTED_TYPE",
                2,
                "本阶段只接受明确选取的.md、.txt或.pdf单文件。",
            ));
        }
    };
    if let Ok(metadata) = fs::symlink_metadata(&absolute) {
        if project::is_link(&metadata) {
            return Err(ProjectError::unsafe_path(&absolute));
        }
        if !metadata.is_file() {
            return Err(ProjectError::new(
                "SOURCE_UNSUPPORTED_TYPE",
                2,
                "请选取普通MD/TXT/PDF文件，不能导入目录。",
            ));
        }
    }
    let read = (|| -> Result<Vec<u8>, SourceFailure> {
        let fail = |message: String| SourceFailure {
            code: "SOURCE_READ_ERROR".into(),
            message,
        };
        let guard = project::read_guard(&absolute).map_err(|error| {
            fail(format!(
                "未能完整读取选取文件 {}：{error}",
                absolute.display()
            ))
        })?;
        let before = guard.metadata().map_err(|error| fail(error.to_string()))?;
        if before.len() > MAX_BYTES {
            return Err(SourceFailure {
                code: "SOURCE_TOO_LARGE".into(),
                message: format!(
                    "选取文件超过本阶段64 MiB上限，未截断为成功内容；实际长度为{}字节。",
                    before.len()
                ),
            });
        }
        let identity = same_file::Handle::from_file(
            guard.try_clone().map_err(|error| fail(error.to_string()))?,
        )
        .map_err(|error| fail(error.to_string()))?;
        let mut bytes = Vec::new();
        (&guard)
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| fail(error.to_string()))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(SourceFailure {
                code: "SOURCE_TOO_LARGE".into(),
                message: "读取期间文件超过64 MiB上限，没有登记截断版本。".into(),
            });
        }
        let after = guard.metadata().map_err(|error| fail(error.to_string()))?;
        let current =
            same_file::Handle::from_path(&absolute).map_err(|error| fail(error.to_string()))?;
        if identity != current
            || before.len() != bytes.len() as u64
            || before.len() != after.len()
            || before.modified().ok() != after.modified().ok()
        {
            return Err(SourceFailure {
                code: "SOURCE_CHANGED".into(),
                message: "选取文件在读取期间发生变化，未确认完整字节；请停止外部写入后重试。"
                    .into(),
            });
        }
        Ok(bytes)
    })();
    Ok(match read {
        Ok(raw) => Selected {
            path: absolute,
            media_type,
            raw: Some(raw),
            failure: None,
        },
        Err(failure) => Selected {
            path: absolute,
            media_type,
            raw: None,
            failure: Some(failure),
        },
    })
}

struct Plan {
    info: ProjectInfo,
    report: ImportReport,
    new_version: bool,
}

fn register(
    connection: &mut Connection,
    info: &ProjectInfo,
    root: &Path,
    selected: &Selected,
    requested: Option<&str>,
    normalized: Option<&text::ParsedText>,
    decode_failure: Option<&SourceFailure>,
) -> Result<Plan, ProjectError> {
    let updated = source_schema::upgrade(connection, info)?;
    let store = Store::load(connection, &info.project_id)?;
    store.validate_snapshots(root, &info.project_id, None)?;
    if requested.is_some_and(|id| !store.sources.contains_key(id)) {
        return Err(not_found());
    }
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| ProjectError::new("SOURCE_CLOCK_ERROR", 1, error.to_string()))?;
    let selected_path = selected
        .path
        .to_str()
        .ok_or_else(|| ProjectError::unsafe_path(&selected.path))?
        .to_owned();
    let path_source = store.source_for_path(&selected_path);
    let hash = selected.raw.as_ref().map(|raw| text::sha256(raw));
    let content_source = hash.as_ref().and_then(|hash| store.source_for_hash(hash));
    let mut source_id = requested
        .map(str::to_owned)
        .or(path_source)
        .or(content_source);
    let mut version = None;
    let mut spans = Vec::new();
    let mut reused = false;
    let mut new_version = false;
    let mut new_source = false;
    if let Some(raw) = selected.raw.as_ref() {
        if source_id.is_none() {
            source_id = Some(Uuid::new_v4().to_string());
            new_source = true;
        }
        let source = source_id.as_ref().ok_or_else(not_found)?;
        if let Some(existing) = store.versions.values().find(|version| {
            version.source_id == *source && Some(&version.content_sha256) == hash.as_ref()
        }) {
            if existing.media_type == "application/pdf" {
                return Err(ProjectError::new(
                    "SOURCE_MEDIA_CONFLICT",
                    2,
                    "同一来源同字节版本已有不同媒体解释；不能改变旧版本类型。",
                ));
            }
            version = Some(existing.clone());
            spans = store
                .spans
                .get(&existing.version_id)
                .cloned()
                .unwrap_or_default();
            reused = true;
        } else {
            let version_id = Uuid::new_v4().to_string();
            let revision = store
                .versions
                .values()
                .filter(|version| version.source_id == *source)
                .map(|version| version.revision)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| {
                    ProjectError::new("SOURCE_INVALID_RECORD", 2, "来源版本修订数越界。")
                })?;
            if let Some(parsed) = normalized {
                spans = parsed
                    .spans
                    .iter()
                    .enumerate()
                    .map(|(ordinal, draft)| SourceSpan {
                        span_id: Uuid::new_v4().to_string(),
                        source_version_id: version_id.clone(),
                        ordinal,
                        kind: draft.kind.clone(),
                        start_byte: draft.start_byte,
                        end_byte: draft.end_byte,
                        start_line: draft.start_line,
                        end_line: draft.end_line,
                        text_sha256: draft.text_sha256.clone(),
                        parser_version: text::PARSER_VERSION.into(),
                        parse_status: draft.parse_status.clone(),
                    })
                    .collect();
            }
            version = Some(SourceVersion {
                source_id: source.clone(),
                version_id: version_id.clone(),
                revision,
                created_at: now.clone(),
                media_type: selected.media_type.into(),
                display_name: selected
                    .path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| ProjectError::unsafe_path(&selected.path))?
                    .into(),
                content_sha256: hash.clone().ok_or_else(not_found)?,
                byte_length: raw.len() as u64,
                normalized_sha256: normalized.map(|parsed| parsed.normalized_sha256.clone()),
                normalization_version: normalized.map(|_| text::NORMALIZATION_VERSION.into()),
                ingest_status: if normalized.is_some() {
                    "ready"
                } else {
                    "failed"
                }
                .into(),
                raw_snapshot: format!(".noetome-sources/{version_id}/raw.bin"),
                normalized_snapshot: normalized
                    .map(|_| format!(".noetome-sources/{version_id}/normalized.txt")),
                error: decode_failure.cloned(),
            });
            new_version = true;
        }
    }
    let error = selected
        .failure
        .clone()
        .or_else(|| version.as_ref().and_then(|version| version.error.clone()));
    let report = ImportReport {
        schema_version: 1,
        project_id: info.project_id.clone(),
        import_id: Uuid::new_v4().to_string(),
        imported_at: now.clone(),
        selected_path,
        source_id: source_id.clone(),
        version_id: version.as_ref().map(|version| version.version_id.clone()),
        status: if error.is_some() { "failed" } else { "ready" }.into(),
        reused,
        knowledge_status: "pending".into(),
        span_count: spans.len() as u64,
        pending_extraction_count: spans.len() as u64,
        version: version.clone(),
        spans: spans.clone(),
        error: error.clone(),
    };
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(ProjectError::database)?;
    if new_source {
        transaction
            .execute(
                "INSERT INTO sources(source_id,created_at) VALUES(?1,?2)",
                params![source_id, now],
            )
            .map_err(ProjectError::database)?;
    }
    if new_version {
        let version = version.as_ref().ok_or_else(not_found)?;
        transaction
            .execute(
                INSERT_VERSION,
                params![
                    version.version_id,
                    version.source_id,
                    sql_integer(version.revision)?,
                    version.created_at,
                    version.media_type,
                    version.display_name,
                    version.content_sha256,
                    sql_integer(version.byte_length)?,
                    version.normalized_sha256,
                    version.normalization_version,
                    version.ingest_status,
                    version.raw_snapshot,
                    version.normalized_snapshot,
                    version.error.as_ref().map(|error| &error.code),
                    version.error.as_ref().map(|error| &error.message)
                ],
            )
            .map_err(ProjectError::database)?;
        for span in &spans {
            transaction
                .execute(
                    INSERT_SPAN,
                    params![
                        span.span_id,
                        span.source_version_id,
                        sql_integer(span.ordinal as u64)?,
                        span.kind.as_str(),
                        sql_integer(span.start_byte as u64)?,
                        sql_integer(span.end_byte as u64)?,
                        sql_integer(span.start_line as u64)?,
                        sql_integer(span.end_line as u64)?,
                        span.text_sha256,
                        span.parser_version,
                        span.parse_status.as_str()
                    ],
                )
                .map_err(ProjectError::database)?;
        }
    }
    let insert_import = if updated.schema_version == 3 {
        "INSERT INTO source_imports
        (rowid,import_id,imported_at,selected_path,source_id,version_id,status,reused,span_count,
         pending_extraction_count,knowledge_status,error_code,error_message)
         VALUES((SELECT COALESCE(MAX(n),0)+1 FROM (SELECT rowid AS n FROM source_imports UNION ALL SELECT rowid AS n FROM pdf_imports)),?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"
    } else {
        INSERT_IMPORT
    };
    // Check the signed sequence before SQL arithmetic could promote overflow
    // to REAL. Both formats share this order in Schema3.
    store.next_import_order()?;
    transaction
        .execute(
            insert_import,
            params![
                report.import_id,
                report.imported_at,
                report.selected_path,
                report.source_id,
                report.version_id,
                report.status,
                report.reused,
                sql_integer(report.spans.len() as u64)?,
                sql_integer(report.pending_extraction_count)?,
                report.knowledge_status,
                error.as_ref().map(|error| &error.code),
                error.as_ref().map(|error| &error.message)
            ],
        )
        .map_err(ProjectError::database)?;
    source_schema::validate(&transaction)?;
    let saved = Store::load(&transaction, &info.project_id)?;
    if saved
        .reports
        .iter()
        .find(|saved| saved.import_id == report.import_id)
        != Some(&report)
    {
        return Err(ProjectError::new(
            "SOURCE_INVALID_RECORD",
            2,
            "导入记录与实际写入的来源版本或位置不一致。",
        ));
    }
    transaction.commit().map_err(ProjectError::database)?;
    Ok(Plan {
        info: updated,
        report,
        new_version,
    })
}

pub fn import(
    path: &Path,
    file: &Path,
    requested: Option<&str>,
) -> Result<CommandOutput, ProjectError> {
    import_with_options(path, file, requested, false)
}
pub fn import_with_options(
    path: &Path,
    file: &Path,
    requested: Option<&str>,
    reparse: bool,
) -> Result<CommandOutput, ProjectError> {
    if let Some(id) = requested {
        invalid_id(id)?;
    }
    let root = project::check_components(path)?;
    let (initial_info, store) = current_store(&root)?;
    if requested.is_some_and(|id| !store.sources.contains_key(id)) {
        return Err(not_found());
    }
    let selected = read_selected(file)?;
    if selected.media_type == "application/pdf" {
        return crate::pdf_source::import_selected(
            &root,
            &initial_info,
            &store,
            &selected,
            requested,
            reparse,
        );
    }
    if reparse {
        return Err(ProjectError::new(
            "SOURCE_INVALID_OPTIONS",
            2,
            "--reparse只用于PDF解析尝试。",
        ));
    }
    let analyzed = selected.raw.as_ref().map(|raw| text::analyze(raw));
    let normalized = analyzed.as_ref().and_then(|result| result.as_ref().ok());
    let decode_failure=analyzed.as_ref().and_then(|result|result.as_ref().err()).map(|error|SourceFailure {code:"SOURCE_INVALID_UTF8".into(),message:format!("选取文件不是有效UTF8（原始字节位置{}）；原始快照保留，没有生成替换文本或规范指纹。",error.valid_up_to())});
    let mut writer = Writer::begin(&root)?;
    let result = (|| {
        if writer.info().project_id != initial_info.project_id
            || writer.info().created_at != initial_info.created_at
        {
            return Err(ProjectError::new(
                "SOURCE_WRITE_INCOMPLETE",
                3,
                "来源读取后项目身份或创建记录发生变化，已停止此次写入。",
            ));
        }
        let plan = writer.build_candidate(|connection, info| {
            register(
                connection,
                info,
                &root,
                &selected,
                requested,
                normalized,
                decode_failure.as_ref(),
            )
        })?;
        let staged = if plan.new_version {
            let version_id = plan.report.version_id.as_deref().ok_or_else(not_found)?;
            writer.stage_version(
                version_id,
                selected.raw.as_deref().ok_or_else(not_found)?,
                normalized.map(|parsed| parsed.text.as_bytes()),
            )?;
            Some((version_id.to_owned(), root.join(".noetome-writing/version")))
        } else {
            None
        };
        let candidate = database_store(
            writer.candidate_path(),
            &root,
            &plan.info,
            staged
                .as_ref()
                .map(|(id, path)| (id.as_str(), path.as_path())),
        )?;
        if candidate
            .reports
            .iter()
            .find(|report| report.import_id == plan.report.import_id)
            != Some(&plan.report)
        {
            return Err(ProjectError::new(
                "SOURCE_INVALID_RECORD",
                2,
                "候选导入记录重读不匹配。",
            ));
        }
        let json = report_json(&plan.report)?;
        writer.commit(&plan.info, |path, marker| {
            if project::inspect_for_writer(path, marker)? != plan.info {
                return Err(ProjectError::new(
                    "PROJECT_SCHEMA_MISMATCH",
                    2,
                    "发布后的项目配置与数据库不匹配。",
                ));
            }
            let stored = database_store(&path.join(project::DATABASE), path, &plan.info, None)?;
            if stored
                .reports
                .iter()
                .find(|report| report.import_id == plan.report.import_id)
                != Some(&plan.report)
            {
                return Err(ProjectError::new(
                    "SOURCE_INVALID_RECORD",
                    2,
                    "发布后的导入记录重读不匹配。",
                ));
            }
            Ok(())
        })?;
        Ok(CommandOutput {
            json,
            failure: plan.report.error.map(|error| (error.code, error.message)),
        })
    })();
    match result {
        Ok(output) => Ok(output),
        Err(error) => match writer.abort() {
            Ok(()) => Err(error),
            Err(rollback) => Err(ProjectError::new(
                "SOURCE_WRITE_INCOMPLETE",
                3,
                format!("{error}；未确认完整回滚：{rollback}"),
            )),
        },
    }
}
