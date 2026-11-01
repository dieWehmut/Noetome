//! PDF raw acquisition and immutable parse attempts in explicit project Schema3.
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use rusqlite::{TransactionBehavior, params};
use serde_json::json;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::pdf_parse::{PdfLimits, PdfParseStatus, PdfParserSpec};
use crate::pdf_source_model::{self, PdfCacheConfig, PdfEvidence, PdfImportRecord, PdfStoredParse};
use crate::project::{self, ProjectError, ProjectInfo};
use crate::source::{self, SourceState};
use crate::source_model::{SourceFailure, SourceVersion};
use crate::source_write::Writer;

const MAX_INPUT: u64 = 64 * 1024 * 1024;
const MAX_EVIDENCE: usize = 16 * 1024 * 1024;

struct Selected {
    path: PathBuf,
    raw: Option<Vec<u8>>,
    error: Option<SourceFailure>,
    // The selected file stays immutable through the complete project commit.
    _guard: Option<File>,
}

fn read_selected(path: &Path) -> Result<Selected, ProjectError> {
    let path = project::check_components(path)?;
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    {
        return Err(ProjectError::new(
            "SOURCE_UNSUPPORTED_TYPE",
            2,
            "请明确选取单份.pdf文件。",
        ));
    }
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if project::is_link(&metadata) || !metadata.is_file() {
            return Err(ProjectError::unsafe_path(&path));
        }
    }
    let acquisition = (|| {
        let read_error = |error: String| SourceFailure {
            code: "SOURCE_READ_ERROR".into(),
            message: error,
        };
        let metadata = project::checked_file(&path).map_err(|e| read_error(e.to_string()))?;
        if metadata.len() > MAX_INPUT {
            return Err(SourceFailure {
                code: "SOURCE_TOO_LARGE".into(),
                message: "所选PDF超过64 MiB读取限制；没有登记完整原始指纹。".into(),
            });
        }
        let mut guard = project::read_guard(&path).map_err(|e| read_error(e.to_string()))?;
        if noetome_sidecar_platform::file_link_count(&guard)
            .map_err(|e| read_error(e.to_string()))?
            != 1
        {
            return Err(read_error("所选PDF含多个硬链接，未获取原始版本。".into()));
        }
        let identity =
            same_file::Handle::from_file(guard.try_clone().map_err(|e| read_error(e.to_string()))?)
                .map_err(|e| read_error(e.to_string()))?;
        let before = guard.metadata().map_err(|e| read_error(e.to_string()))?;
        let mut raw = Vec::new();
        (&mut guard)
            .take(MAX_INPUT + 1)
            .read_to_end(&mut raw)
            .map_err(|e| read_error(e.to_string()))?;
        let after = guard.metadata().map_err(|e| read_error(e.to_string()))?;
        if raw.len() as u64 > MAX_INPUT
            || raw.len() as u64 != before.len()
            || before.len() != after.len()
            || before.modified().ok() != after.modified().ok()
            || identity
                != same_file::Handle::from_path(&path).map_err(|e| read_error(e.to_string()))?
            || noetome_sidecar_platform::file_link_count(&guard)
                .map_err(|e| read_error(e.to_string()))?
                != 1
        {
            return Err(SourceFailure {
                code: "SOURCE_CHANGED".into(),
                message: "所选PDF没有完整稳定读取，未登记原始指纹。".into(),
            });
        }
        Ok((raw, guard))
    })();
    Ok(match acquisition {
        Ok((raw, guard)) => Selected {
            path,
            raw: Some(raw),
            error: None,
            _guard: Some(guard),
        },
        Err(error) => Selected {
            path,
            raw: None,
            error: Some(error),
            _guard: None,
        },
    })
}

fn state(root: &Path) -> Result<(ProjectInfo, SourceState, Vec<File>), ProjectError> {
    let info = project::inspect(root)?;
    let guards = vec![
        project::read_guard(&root.join(project::CONFIG))?,
        project::read_guard(&root.join(project::DATABASE))?,
    ];
    if project::inspect(root)? != info {
        return Err(ProjectError::new(
            "PROJECT_DATABASE_BUSY",
            3,
            "读取期间项目配置发生变化。",
        ));
    }
    let state = source::database_state(&root.join(project::DATABASE), root, &info, None, None)?;
    project::reject_sidecars(root)?;
    Ok((info, state, guards))
}

fn report_json(
    info: &ProjectInfo,
    state: &SourceState,
    record: &PdfImportRecord,
) -> Result<String, ProjectError> {
    let version = record
        .version_id
        .as_ref()
        .and_then(|id| state.0.versions.get(id));
    let parse = record
        .parse_id
        .as_ref()
        .and_then(|id| state.1.parses.get(id));
    let evidence = record.parse_id.as_ref().and_then(|id| state.2.get(id));
    let report = json!({
        "schema_version":2,"project_id":info.project_id,
        "import_id":record.import_id,"imported_at":record.imported_at,"selected_path":record.selected_path,
        "source_id":record.source_id,"version_id":record.version_id,
        "status":if record.raw_status=="failed" {"failed"} else {record.parse_status.as_str()},
        "exit_code":record.exit_code,
        "raw_acquisition":{"status":record.raw_status,"reused":record.reused_version,"version":version,
            "error":if record.raw_status=="failed" {record.error.as_ref()} else {None}},
        "pdf_parse":{"status":record.parse_status,"reused":record.reused_parse,"parse_id":record.parse_id,
            "config":parse.map(|p|&p.config),"report_path":parse.map(|p|&p.report_path),
            "report_sha256":parse.map(|p|&p.report_sha256),"evidence":evidence},
        "knowledge_status":"pending","error":record.error,
    });
    serde_json::to_string_pretty(&report)
        .map(|text| format!("{text}\n"))
        .map_err(|e| ProjectError::new("SOURCE_REPORT_ERROR", 1, e.to_string()))
}

pub(crate) fn inspect(root: &Path, import_id: &str) -> Result<String, ProjectError> {
    source::invalid_id(import_id)?;
    let (info, saved, _guards) = state(root)?;
    let record = saved
        .1
        .imports
        .iter()
        .find(|record| record.import_id == import_id)
        .ok_or_else(source::not_found)?;
    report_json(&info, &saved, record)
}

struct Plan {
    version: Option<SourceVersion>,
    new_source: bool,
    new_version: bool,
    parse: Option<PdfStoredParse>,
    new_parse: bool,
    evidence: Option<PdfEvidence>,
    files: BTreeMap<String, Vec<u8>>,
    record: PdfImportRecord,
}

fn select_version(
    saved: &SourceState,
    selected: &Selected,
    requested: Option<&str>,
    now: &str,
) -> Result<(Option<String>, Option<SourceVersion>, bool, bool), ProjectError> {
    let selected_path = selected
        .path
        .to_str()
        .ok_or_else(|| ProjectError::unsafe_path(&selected.path))?;
    let hash = selected.raw.as_ref().map(|raw| crate::text::sha256(raw));
    let path_source = saved
        .1
        .imports
        .iter()
        .rev()
        .filter(|record| record.selected_path == selected_path)
        .find_map(|record| record.source_id.clone());
    let content_source = hash.as_ref().and_then(|hash| {
        saved
            .1
            .imports
            .iter()
            .filter_map(|record| record.version_id.as_ref())
            .filter_map(|id| saved.0.versions.get(id))
            .find(|version| version.content_sha256 == *hash)
            .map(|version| version.source_id.clone())
    });
    let mut source_id = requested
        .map(str::to_owned)
        .or(path_source)
        .or(content_source);
    let Some(raw) = selected.raw.as_ref() else {
        return Ok((source_id, None, false, false));
    };
    let new_source = source_id.is_none();
    let source_id = source_id
        .get_or_insert_with(|| Uuid::new_v4().to_string())
        .clone();
    let hash = hash.ok_or_else(source::not_found)?;
    if let Some(existing) = saved
        .0
        .versions
        .values()
        .find(|version| version.source_id == source_id && version.content_sha256 == hash)
    {
        if existing.media_type != "application/pdf" {
            return Err(ProjectError::new(
                "SOURCE_MEDIA_CONFLICT",
                2,
                "同一来源的相同原始字节已有文本版本，不能作为PDF复用。",
            ));
        }
        return Ok((Some(source_id), Some(existing.clone()), false, false));
    }
    let version_id = Uuid::new_v4().to_string();
    let revision = saved
        .0
        .versions
        .values()
        .filter(|version| version.source_id == source_id)
        .map(|version| version.revision)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| ProjectError::new("SOURCE_INVALID_RECORD", 2, "版本修订数越界。"))?;
    let version = SourceVersion {
        source_id: source_id.clone(),
        version_id: version_id.clone(),
        revision,
        created_at: now.into(),
        media_type: "application/pdf".into(),
        display_name: selected
            .path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| ProjectError::unsafe_path(&selected.path))?
            .into(),
        content_sha256: hash,
        byte_length: raw.len() as u64,
        normalized_sha256: None,
        normalization_version: None,
        ingest_status: "acquired".into(),
        raw_snapshot: format!(".noetome-sources/{version_id}/raw.bin"),
        normalized_snapshot: None,
        error: None,
    };
    Ok((Some(source_id), Some(version), new_source, true))
}

fn prepare(
    writer: &mut Writer,
    saved: &SourceState,
    selected: &Selected,
    requested: Option<&str>,
    parser: &PdfParserSpec,
    limits: PdfLimits,
    cancel: &AtomicBool,
) -> Result<Plan, ProjectError> {
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| ProjectError::new("SOURCE_CLOCK_ERROR", 1, e.to_string()))?;
    let (source_id, version, new_source, new_version) =
        select_version(saved, selected, requested, &now)?;
    let mut plan = Plan {
        version,
        new_source,
        new_version,
        parse: None,
        new_parse: false,
        evidence: None,
        files: BTreeMap::new(),
        record: PdfImportRecord {
            import_id: Uuid::new_v4().to_string(),
            imported_at: now.clone(),
            selected_path: selected
                .path
                .to_str()
                .ok_or_else(|| ProjectError::unsafe_path(&selected.path))?
                .into(),
            source_id,
            version_id: None,
            parse_id: None,
            raw_status: "failed".into(),
            parse_status: "not_run".into(),
            reused_version: false,
            reused_parse: false,
            knowledge_status: "pending".into(),
            exit_code: 3,
            error: selected.error.clone(),
        },
    };
    let Some(version) = plan.version.as_ref() else {
        return Ok(plan);
    };
    plan.record.version_id = Some(version.version_id.clone());
    plan.record.raw_status = "acquired".into();
    plan.record.reused_version = !new_version;
    let config = PdfCacheConfig::current(limits);
    config.limits()?;
    let key = config.cache_key(&version.version_id, &version.content_sha256);
    if let Some(parse) = saved.1.parses.values().find(|parse| {
        parse.version_id == version.version_id && parse.cache_key == key && parse.status == "ready"
    }) {
        plan.parse = Some(parse.clone());
        plan.evidence = Some(
            saved
                .2
                .get(&parse.parse_id)
                .ok_or_else(source::not_found)?
                .clone(),
        );
        plan.record.parse_id = Some(parse.parse_id.clone());
        plan.record.parse_status = "ready".into();
        plan.record.reused_parse = true;
        plan.record.exit_code = 0;
        plan.record.error = None;
        return Ok(plan);
    }
    let raw = selected.raw.as_deref().ok_or_else(source::not_found)?;
    let (host_report, status, exit_code, failure) =
        match writer.run_pdf_parser(parser, raw, limits, cancel) {
            Ok(output) => {
                if output.report.input.byte_length != version.byte_length
                    || output.report.input.sha256 != version.content_sha256
                {
                    return Err(ProjectError::new(
                        "PDF_SOURCE_INVALID_EVIDENCE",
                        2,
                        "解析原始指纹与已选版本不同。",
                    ));
                }
                plan.files = writer.pdf_artifact_bytes(&output.report)?;
                let failure = output.report.failure.as_ref().map(|f| SourceFailure {
                    code: f.code.clone(),
                    message: f.message.clone(),
                });
                (
                    Some(output.report.clone()),
                    output.report.status,
                    output.exit_code,
                    failure,
                )
            }
            Err(error)
                if matches!(
                    error.code(),
                    "PDF_RUNTIME_INVALID"
                        | "PDF_ADAPTER_INVALID"
                        | "PDF_LIMITS_INVALID"
                        | "SIDECAR_UNSAFE_PATH"
                ) =>
            {
                let exit_code = error.exit_code();
                (
                    None,
                    PdfParseStatus::Failed,
                    exit_code,
                    Some(SourceFailure {
                        code: error.code().into(),
                        message: error.to_string(),
                    }),
                )
            }
            Err(error) => return Err(error),
        };
    let evidence = PdfEvidence {
        schema_version: 1,
        input: crate::sidecar::InputFingerprint {
            relative_path: "input/raw.bin".into(),
            byte_length: version.byte_length,
            sha256: version.content_sha256.clone(),
        },
        config: config.clone(),
        status,
        exit_code,
        host_report,
        failure: failure.clone(),
    };
    let mut bytes = serde_json::to_vec_pretty(&evidence)
        .map_err(|e| ProjectError::new("SOURCE_REPORT_ERROR", 1, e.to_string()))?;
    bytes.push(b'\n');
    if bytes.len() > MAX_EVIDENCE {
        return Err(ProjectError::new(
            "PDF_REPORT_LIMIT",
            3,
            "PDF来源证据超过16 MiB限制。",
        ));
    }
    let parse_id = Uuid::new_v4().to_string();
    let status = match status {
        PdfParseStatus::Ready => "ready",
        PdfParseStatus::NeedsReview => "needs_review",
        PdfParseStatus::Failed => "failed",
    };
    let ordinal = saved
        .1
        .parses
        .values()
        .filter(|parse| parse.version_id == version.version_id)
        .count() as u64;
    let page_count = evidence
        .host_report
        .as_ref()
        .and_then(|report| report.response.as_ref())
        .and_then(|response| response.page_count);
    let parse = PdfStoredParse {
        parse_id: parse_id.clone(),
        version_id: version.version_id.clone(),
        ordinal,
        created_at: now,
        cache_key: key,
        config,
        status: status.into(),
        page_count,
        report_path: format!(".noetome-pdf-parses/{parse_id}/report.json"),
        report_sha256: crate::text::sha256(&bytes),
        report_byte_length: bytes.len() as u64,
        error: failure.clone(),
    };
    plan.files.insert("report.json".into(), bytes);
    plan.parse = Some(parse);
    plan.new_parse = true;
    plan.evidence = Some(evidence);
    plan.record.parse_id = Some(parse_id);
    plan.record.parse_status = status.into();
    plan.record.exit_code = exit_code;
    plan.record.error = failure;
    Ok(plan)
}

fn register(
    connection: &mut rusqlite::Connection,
    info: &ProjectInfo,
    plan: &Plan,
) -> Result<ProjectInfo, ProjectError> {
    let updated = crate::pdf_source_schema::upgrade(connection, info)?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(ProjectError::database)?;
    if plan.new_source {
        tx.execute(
            "INSERT INTO sources(source_id,created_at) VALUES(?1,?2)",
            params![plan.record.source_id, plan.record.imported_at],
        )
        .map_err(ProjectError::database)?;
    }
    if plan.new_version {
        let version = plan.version.as_ref().ok_or_else(source::not_found)?;
        tx.execute(
            source::INSERT_VERSION,
            params![
                version.version_id,
                version.source_id,
                source::sql_integer(version.revision)?,
                version.created_at,
                version.media_type,
                version.display_name,
                version.content_sha256,
                source::sql_integer(version.byte_length)?,
                version.normalized_sha256,
                version.normalization_version,
                version.ingest_status,
                version.raw_snapshot,
                version.normalized_snapshot,
                Option::<String>::None,
                Option::<String>::None
            ],
        )
        .map_err(ProjectError::database)?;
    }
    if plan.new_parse {
        pdf_source_model::insert_parse(
            &tx,
            plan.parse.as_ref().ok_or_else(source::not_found)?,
            plan.evidence.as_ref().ok_or_else(source::not_found)?,
        )?;
    }
    pdf_source_model::insert_import(&tx, &plan.record)?;
    crate::pdf_source_schema::validate(&tx)?;
    let raw = crate::source_model::Store::load(&tx, &info.project_id)?;
    let pdf = pdf_source_model::PdfStore::load(&tx, &raw, &info.project_id)?;
    if pdf
        .imports
        .iter()
        .find(|record| record.import_id == plan.record.import_id)
        != Some(&plan.record)
    {
        return Err(ProjectError::new(
            "PDF_SOURCE_INVALID_RECORD",
            2,
            "候选PDF导入记录重读不匹配。",
        ));
    }
    tx.commit().map_err(ProjectError::database)?;
    Ok(updated)
}

pub fn import(
    root: &Path,
    file: &Path,
    requested: Option<&str>,
    parser: &PdfParserSpec,
    limits: PdfLimits,
) -> Result<source::CommandOutput, ProjectError> {
    if !parser.python.is_absolute() {
        return Err(ProjectError::new(
            "PDF_RUNTIME_INVALID",
            2,
            "Python解释器路径须为绝对路径。",
        ));
    }
    if let Some(id) = requested {
        source::invalid_id(id)?;
    }
    let root = project::check_components(root)?;
    let (initial, saved, acquisition_guards) = state(&root)?;
    if requested.is_some_and(|id| !saved.0.sources.contains_key(id)) {
        return Err(source::not_found());
    }
    let selected = read_selected(file)?;
    let mut writer = Writer::begin(&root)?;
    // Writer now retains the same original files through candidate publication.
    // Keep deny-write/delete capabilities continuous across the selected read.
    drop(acquisition_guards);
    let result = (|| {
        if writer.info() != &initial {
            return Err(ProjectError::new(
                "SOURCE_WRITE_INCOMPLETE",
                3,
                "获取原始文件后项目状态变化。",
            ));
        }
        let plan = prepare(
            &mut writer,
            &saved,
            &selected,
            requested,
            parser,
            limits,
            &AtomicBool::new(false),
        )?;
        let updated =
            writer.build_candidate(|connection, info| register(connection, info, &plan))?;
        let staged_raw = if plan.new_version {
            let version = plan.version.as_ref().ok_or_else(source::not_found)?;
            writer.stage_pdf_raw(
                &version.version_id,
                selected.raw.as_deref().ok_or_else(source::not_found)?,
            )?;
            Some((
                version.version_id.clone(),
                root.join(".noetome-writing/version"),
            ))
        } else {
            None
        };
        let staged_parse = if plan.new_parse {
            let parse = plan.parse.as_ref().ok_or_else(source::not_found)?;
            writer.stage_pdf_parse(&parse.parse_id, &plan.files)?;
            Some((parse.parse_id.clone(), root.join(".noetome-writing/parse")))
        } else {
            None
        };
        let candidate = source::database_state(
            writer.candidate_path(),
            &root,
            &updated,
            staged_raw
                .as_ref()
                .map(|(id, path)| (id.as_str(), path.as_path())),
            staged_parse
                .as_ref()
                .map(|(id, path)| (id.as_str(), path.as_path())),
        )?;
        let record = candidate
            .1
            .imports
            .iter()
            .find(|record| record.import_id == plan.record.import_id)
            .ok_or_else(source::not_found)?;
        if record != &plan.record {
            return Err(ProjectError::new(
                "PDF_SOURCE_INVALID_RECORD",
                2,
                "候选导入重读不匹配。",
            ));
        }
        let json = report_json(&updated, &candidate, record)?;
        writer.commit(&updated, |path, marker| {
            if project::inspect_for_writer(path, marker)? != updated {
                return Err(ProjectError::new(
                    "PROJECT_SCHEMA_MISMATCH",
                    2,
                    "发布后配置与数据库不匹配。",
                ));
            }
            let published =
                source::database_state(&path.join(project::DATABASE), path, &updated, None, None)?;
            let record = published
                .1
                .imports
                .iter()
                .find(|record| record.import_id == plan.record.import_id)
                .ok_or_else(source::not_found)?;
            if record != &plan.record || report_json(&updated, &published, record)? != json {
                return Err(ProjectError::new(
                    "PDF_SOURCE_INVALID_RECORD",
                    2,
                    "发布后证据和导入报告不匹配。",
                ));
            }
            Ok(())
        })?;
        Ok(source::CommandOutput {
            json,
            exit_code: plan.record.exit_code,
            failure: plan
                .record
                .error
                .map(|failure| (failure.code, failure.message)),
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn project_native_guards_remain_continuous_until_writer_takes_ownership() {
        let parent =
            std::env::temp_dir().join(format!("noetome-pdf-acquisition-{}", Uuid::new_v4()));
        fs::create_dir(&parent).unwrap();
        let root = parent.join("project");
        project::initialize(&root).unwrap();
        let database = root.join(project::DATABASE);
        let config = root.join(project::CONFIG);
        let before_db = fs::read(&database).unwrap();
        let before_config = fs::read(&config).unwrap();
        let (_, _, guards) = state(&root).unwrap();
        assert!(fs::rename(&database, parent.join("replaced.sqlite")).is_err());
        assert!(fs::write(&config, b"same metadata replacement").is_err());
        let mut writer = Writer::begin(&root).unwrap();
        drop(guards);
        assert!(fs::rename(&database, parent.join("replaced.sqlite")).is_err());
        assert!(fs::write(&config, b"same metadata replacement").is_err());
        writer.abort().unwrap();
        drop(writer);
        assert_eq!(fs::read(&database).unwrap(), before_db);
        assert_eq!(fs::read(&config).unwrap(), before_config);
        fs::rename(&database, parent.join("replaced.sqlite")).unwrap();
        fs::rename(parent.join("replaced.sqlite"), &database).unwrap();
        fs::remove_dir_all(parent).unwrap();
    }
}
