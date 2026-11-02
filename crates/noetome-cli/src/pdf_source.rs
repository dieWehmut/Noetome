//! One explicitly selected PDF: original content identity and independently
//! versioned parse attempts are committed together, then reread from disk.
use std::path::{Path, PathBuf};

use rusqlite::{Connection, TransactionBehavior, params};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::pdf_model::{Page, ParserIdentity};
use crate::pdf_parse::{self, CapturedPdf};
use crate::pdf_source_model::{self, PdfImportReport, PdfParse, PdfResource, PdfStatus};
use crate::project::{self, ProjectError, ProjectInfo};
use crate::sidecar_model::{Fingerprint, Issue, ParseStatus};
use crate::source::{self, CommandOutput, Selected, sql_integer};
use crate::source_model::{SourceFailure, SourceVersion, Store};
use crate::{source_schema, source_write::Writer, text};

const INSERT_PARSE: &str = "INSERT INTO pdf_parses
 (parse_id,source_version_id,created_at,cache_key,protocol_version,requested_parser_json,
 parser_json,import_options_version,adapter_sha256,status,outcome_kind,page_count,
 confirmed_page_count,parsed_page_count,needs_review_page_count,failed_page_count,
 region_count,snapshot_path,outcome_sha256,manifest_sha256,error_code,error_message)
 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)";
const INSERT_PDF_IMPORT: &str = "INSERT INTO pdf_imports
 (rowid,import_id,imported_at,selected_path,source_id,version_id,parse_id,status,reused,
 parse_reused,acquisition_status,error_code,error_message)
 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)";
const ADAPTER_SOURCE: &[u8] = include_bytes!("../../../sidecars/pdf_parser/adapter.py");

struct NewParse {
    attempt: PdfParse,
    pages: Vec<Page>,
    page_json: Vec<String>,
    resources: Vec<PdfResource>,
}
#[derive(serde::Deserialize)]
struct RawPages {
    pages: Vec<Box<serde_json::value::RawValue>>,
}
struct Plan {
    report: PdfImportReport,
    new_source: bool,
    new_version: bool,
    parse: Option<NewParse>,
}

fn invalid(message: impl ToString) -> ProjectError {
    ProjectError::new("SOURCE_INVALID_RECORD", 2, message.to_string())
}
fn json<T: serde::Serialize>(record: &T) -> Result<String, ProjectError> {
    serde_json::to_string(record).map_err(invalid)
}
fn parse_status(status: &ParseStatus) -> &'static str {
    match status {
        ParseStatus::Parsed => "parsed",
        ParseStatus::NeedsReview => "needs_review",
        ParseStatus::Failed => "failed",
    }
}
fn overall_status(status: &str) -> &'static str {
    match status {
        "parsed" => "ready",
        "needs_review" => "needs_review",
        _ => "failed",
    }
}

pub(crate) fn output(report: &PdfImportReport) -> Result<CommandOutput, ProjectError> {
    let json = serde_json::to_string_pretty(report)
        .map(|json| json + "\n")
        .map_err(|error| ProjectError::new("SOURCE_REPORT_ERROR", 1, error.to_string()))?;
    let failure = if report.status == "ready" {
        None
    } else if let Some(error) = &report.error {
        Some((error.code.clone(), error.message.clone()))
    } else {
        Some((
            "PDF_INCOMPLETE".into(),
            format!(
                "PDF status {}: {} parsed, {} needs_review, {} failed pages; confirmed total {}. See manifest/report for reasons.",
                report.pdf.status,
                report.pdf.parsed_page_count,
                report.pdf.needs_review_page_count,
                report.pdf.failed_page_count,
                report
                    .pdf
                    .page_count
                    .map_or_else(|| "unknown".into(), |count| count.to_string())
            ),
        ))
    };
    Ok(CommandOutput { json, failure })
}

fn new_version(
    store: &Store,
    selected: &Selected,
    requested: Option<&str>,
    now: &str,
) -> Result<(Option<String>, Option<SourceVersion>, bool, bool), ProjectError> {
    let selected_path = selected
        .path
        .to_str()
        .ok_or_else(|| ProjectError::unsafe_path(&selected.path))?;
    let hash = selected.raw.as_ref().map(|raw| text::sha256(raw));
    let mut source = requested
        .map(str::to_owned)
        .or_else(|| store.source_for_path(selected_path))
        .or_else(|| hash.as_ref().and_then(|hash| store.source_for_hash(hash)));
    let Some(raw) = &selected.raw else {
        return Ok((source, None, false, false));
    };
    let create_source = source.is_none();
    if create_source {
        source = Some(Uuid::new_v4().to_string());
    }
    let source_id = source.as_ref().ok_or_else(source::not_found)?;
    let hash = hash.ok_or_else(source::not_found)?;
    if let Some(version) = store
        .versions
        .values()
        .find(|version| version.source_id == *source_id && version.content_sha256 == hash)
    {
        if version.media_type != "application/pdf" {
            return Err(ProjectError::new(
                "SOURCE_MEDIA_CONFLICT",
                2,
                "同一来源同字节版本已作为UTF8文本登记；不能改写为PDF版本。",
            ));
        }
        return Ok((source, Some(version.clone()), false, false));
    }
    let revision = store
        .versions
        .values()
        .filter(|version| version.source_id == *source_id)
        .map(|version| version.revision)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| invalid("来源版本修订数越界"))?;
    sql_integer(revision)?;
    let version_id = Uuid::new_v4().to_string();
    let version = SourceVersion {
        source_id: source_id.clone(),
        version_id: version_id.clone(),
        revision,
        created_at: now.into(),
        media_type: "application/pdf".into(),
        display_name: selected
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ProjectError::unsafe_path(&selected.path))?
            .into(),
        content_sha256: hash,
        byte_length: raw.len() as u64,
        normalized_sha256: None,
        normalization_version: None,
        ingest_status: "acquired".into(),
        raw_snapshot: format!(".noetome-sources/{version_id}/raw.pdf"),
        normalized_snapshot: None,
        error: None,
    };
    Ok((source, Some(version), create_source, true))
}

fn capture_attempt(
    version: &SourceVersion,
    now: &str,
    key: String,
    options: &str,
    adapter_sha: &str,
    captured: &CapturedPdf,
) -> Result<(NewParse, Vec<Issue>), ProjectError> {
    let id = Uuid::new_v4().to_string();
    let mut attempt = PdfParse {
        parse_id: id.clone(),
        source_version_id: version.version_id.clone(),
        created_at: now.into(),
        cache_key: key,
        protocol_version: 1,
        requested_parser: ParserIdentity::pinned(),
        parser: None,
        import_options_version: options.into(),
        adapter_sha256: adapter_sha.into(),
        status: "failed".into(),
        outcome_kind: "host_failed".into(),
        page_count: None,
        confirmed_page_count: 0,
        parsed_page_count: 0,
        needs_review_page_count: 0,
        failed_page_count: 0,
        region_count: 0,
        snapshot_path: format!(".noetome-pdf-parses/{id}"),
        outcome_sha256: String::new(),
        manifest_sha256: None,
        error: captured.error.as_ref().map(|error| SourceFailure {
            code: error.code.into(),
            message: error.message.clone(),
        }),
    };
    let mut pages = Vec::new();
    let mut page_json = Vec::new();
    let mut issues = Vec::new();
    let complete = captured.report.is_some();
    if let (Some(report), Some(manifest)) = (&captured.report, &captured.manifest) {
        if report.input_fingerprint.sha256 != version.content_sha256
            || report.input_fingerprint.byte_length != version.byte_length
        {
            return Err(invalid("真实PDF解析原始指纹与选取版本不一致"));
        }
        attempt.parser = Some(report.parser.clone());
        attempt.status = parse_status(&report.status).into();
        attempt.outcome_kind = "validated".into();
        attempt.page_count = report.page_count.map(u64::from);
        attempt.confirmed_page_count = manifest.pages.len() as u64;
        attempt.parsed_page_count = u64::from(report.parsed_page_count);
        attempt.needs_review_page_count = u64::from(report.needs_review_page_count);
        attempt.failed_page_count = u64::from(report.failed_page_count);
        attempt.region_count = manifest
            .pages
            .iter()
            .map(|page| page.regions.len() as u64)
            .sum();
        attempt.manifest_sha256 = Some(report.manifest_sha256.clone());
        attempt.error = if report.status == ParseStatus::Failed {
            captured
                .report_failure
                .as_ref()
                .map(|(code, message)| SourceFailure {
                    code: code.clone(),
                    message: message.clone(),
                })
        } else {
            None
        };
        pages = manifest.pages.clone();
        // Preserve the exact original JSON numbers, including -0.0 and long
        // decimal glyph coordinates. Parsing and serializing typed f64 pages
        // would change the manifest's lexical values before a later reread.
        // The complete manifest was already strictly checked by the Core.
        let raw: RawPages = serde_json::from_slice(
            captured
                .tree
                .files
                .get("parser/pdf-manifest.json")
                .ok_or_else(|| invalid("缺少实际捕获的PDF manifest"))?,
        )
        .map_err(invalid)?;
        if raw.pages.len() != pages.len() {
            return Err(invalid("原始页JSON与已复核页数量不一致"));
        }
        page_json = raw
            .pages
            .into_iter()
            .map(|page| page.get().to_owned())
            .collect();
        issues = report.sidecar.issues.clone();
    } else if captured.report.is_some() || captured.manifest.is_some() || captured.error.is_none() {
        return Err(invalid("PDF捕获结果缺少其严格报告或真实失败"));
    }
    let outcome = if complete {
        "parse-result.json"
    } else {
        "failure.json"
    };
    attempt.outcome_sha256 = text::sha256(
        captured
            .tree
            .files
            .get(outcome)
            .ok_or_else(|| invalid("缺少捕获结果文件"))?,
    );
    let mut resources = Vec::new();
    for (path, bytes) in &captured.tree.files {
        let (resource_id, kind, media, confidence) = match path.as_str() {
            "parse-result.json" => (
                pdf_source_model::OUTCOME_RESOURCE_ID.into(),
                "utf8",
                "application/json",
                "diagnostic",
            ),
            "parse-result.md" => (
                pdf_source_model::MARKDOWN_RESOURCE_ID.into(),
                "utf8",
                "text/markdown",
                "diagnostic",
            ),
            "failure.json" => (
                pdf_source_model::FAILURE_RESOURCE_ID.into(),
                "utf8",
                "application/json",
                "diagnostic",
            ),
            _ => {
                let (id, kind, media) = pdf_source_model::parser_resource_identity(path)
                    .ok_or_else(|| invalid("未知parser捕获文件"))?;
                (
                    id,
                    kind,
                    media,
                    if complete { "confirmed" } else { "unconfirmed" },
                )
            }
        };
        resources.push(PdfResource {
            resource_id,
            relative_path: path.clone(),
            sha256: text::sha256(bytes),
            byte_length: bytes.len() as u64,
            kind: kind.into(),
            media_type: media.into(),
            confidence: confidence.into(),
        });
    }
    Ok((
        NewParse {
            attempt,
            pages,
            page_json,
            resources,
        },
        issues,
    ))
}

fn insert_parse(connection: &Connection, new: &NewParse) -> Result<(), ProjectError> {
    let p = &new.attempt;
    connection
        .execute(
            INSERT_PARSE,
            params![
                p.parse_id,
                p.source_version_id,
                p.created_at,
                p.cache_key,
                p.protocol_version,
                json(&p.requested_parser)?,
                p.parser.as_ref().map(json).transpose()?,
                p.import_options_version,
                p.adapter_sha256,
                p.status,
                p.outcome_kind,
                p.page_count.map(sql_integer).transpose()?,
                sql_integer(p.confirmed_page_count)?,
                sql_integer(p.parsed_page_count)?,
                sql_integer(p.needs_review_page_count)?,
                sql_integer(p.failed_page_count)?,
                sql_integer(p.region_count)?,
                p.snapshot_path,
                p.outcome_sha256,
                p.manifest_sha256,
                p.error.as_ref().map(|error| &error.code),
                p.error.as_ref().map(|error| &error.message)
            ],
        )
        .map_err(ProjectError::database)?;
    for resource in &new.resources {
        connection.execute("INSERT INTO pdf_resources(parse_id,resource_id,relative_path,sha256,byte_length,kind,media_type,confidence) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![p.parse_id,resource.resource_id,resource.relative_path,resource.sha256,
                sql_integer(resource.byte_length)?,resource.kind,resource.media_type,resource.confidence])
            .map_err(ProjectError::database)?;
    }
    for (page, raw_json) in new.pages.iter().zip(&new.page_json) {
        connection.execute("INSERT INTO pdf_pages(parse_id,physical_page_index,display_page_number,printed_page_label,status,record_json) VALUES(?1,?2,?3,?4,?5,?6)",
            params![p.parse_id,page.physical_page_index,page.display_page_number,page.printed_page_label,
                parse_status(&page.status),raw_json]).map_err(ProjectError::database)?;
        for region in &page.regions {
            connection.execute("INSERT INTO pdf_regions(parse_id,region_id,physical_page_index,locator_kind,bbox_left,bbox_top,bbox_right,bbox_bottom,text_resource_id,text_resource_sha256,start_byte,end_byte,glyph_indices_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                params![p.parse_id,region.region_id,region.physical_page_index,region.locator_kind,
                    region.bbox[0],region.bbox[1],region.bbox[2],region.bbox[3],region.text.resource_id,
                    region.text.resource_sha256,sql_integer(region.text.start_byte)?,sql_integer(region.text.end_byte)?,
                    json(&region.glyph_indices)?]).map_err(ProjectError::database)?;
        }
    }
    Ok(())
}

fn register(
    connection: &mut Connection,
    info: &ProjectInfo,
    root: &Path,
    before: &Store,
    plan: &Plan,
) -> Result<ProjectInfo, ProjectError> {
    let updated = source_schema::upgrade_pdf(connection, info)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(ProjectError::database)?;
    source_schema::validate(&transaction)?;
    let metadata: (String,u32,String,String) = transaction.query_row(
        "SELECT project_id,schema_version,created_at,state FROM project_metadata WHERE singleton=1", [],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).map_err(ProjectError::database)?;
    if metadata
        != (
            updated.project_id.clone(),
            updated.schema_version,
            updated.created_at.clone(),
            updated.state.clone(),
        )
    {
        return Err(invalid("登记事务取得锁后项目元数据已变化"));
    }
    let mut previous = Store::load(&transaction, &info.project_id)?;
    previous.validate_snapshots(root, &info.project_id, None)?;
    previous.hydrate_pdf_reports(root, None)?;
    if &previous != before {
        return Err(invalid(
            "读取来源后原项目记录发生变化，不能提交基于旧记录的PDF登记",
        ));
    }
    let report = &plan.report;
    if plan.new_source {
        transaction
            .execute(
                "INSERT INTO sources(source_id,created_at) VALUES(?1,?2)",
                params![report.source_id, report.imported_at],
            )
            .map_err(ProjectError::database)?;
    }
    if plan.new_version {
        let v = report.version.as_ref().ok_or_else(source::not_found)?;
        transaction
            .execute(
                source::INSERT_VERSION,
                params![
                    v.version_id,
                    v.source_id,
                    sql_integer(v.revision)?,
                    v.created_at,
                    v.media_type,
                    v.display_name,
                    v.content_sha256,
                    sql_integer(v.byte_length)?,
                    v.normalized_sha256,
                    v.normalization_version,
                    v.ingest_status,
                    v.raw_snapshot,
                    v.normalized_snapshot,
                    v.error.as_ref().map(|error| &error.code),
                    v.error.as_ref().map(|error| &error.message)
                ],
            )
            .map_err(ProjectError::database)?;
    }
    if let Some(parse) = &plan.parse {
        insert_parse(&transaction, parse)?;
    }
    let order = previous.next_import_order()?;
    transaction
        .execute(
            INSERT_PDF_IMPORT,
            params![
                order,
                report.import_id,
                report.imported_at,
                report.selected_path,
                report.source_id,
                report.version_id,
                report.pdf.parse_id,
                report.status,
                report.reused,
                report.pdf.reused,
                report.acquisition_status,
                report.error.as_ref().map(|error| &error.code),
                report.error.as_ref().map(|error| &error.message)
            ],
        )
        .map_err(ProjectError::database)?;
    source_schema::validate(&transaction)?;
    let saved = Store::load(&transaction, &info.project_id)?;
    let mut sql_report = report.clone();
    sql_report.pdf.issues.clear(); // SQL reconstructs issues only from verified snapshot bytes.
    if saved
        .pdf
        .reports
        .iter()
        .find(|saved| saved.import_id == report.import_id)
        != Some(&sql_report)
    {
        return Err(invalid(
            "写入的PDF来源、解析状态、计数或真实错误与报告不一致",
        ));
    }
    transaction.commit().map_err(ProjectError::database)?;
    Ok(updated)
}

pub(crate) fn import_selected(
    root: &Path,
    info: &ProjectInfo,
    before: &Store,
    selected: &Selected,
    requested: Option<&str>,
    reparse: bool,
) -> Result<CommandOutput, ProjectError> {
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| ProjectError::new("SOURCE_CLOCK_ERROR", 1, error.to_string()))?;
    let (source_id, version, new_source, create_version) =
        new_version(before, selected, requested, &now)?;
    let mut writer = Writer::begin(root)?;
    let result = (|| {
        if writer.info() != info {
            return Err(invalid("读取来源后项目身份、版本或创建记录发生变化"));
        }
        let mut staged_version: Option<(String, PathBuf)> = None;
        if create_version {
            let version = version.as_ref().ok_or_else(source::not_found)?;
            writer.stage_pdf_version(
                &version.version_id,
                selected.raw.as_deref().ok_or_else(source::not_found)?,
            )?;
            staged_version = Some((
                version.version_id.clone(),
                root.join(".noetome-writing/version"),
            ));
        }
        let options = if reparse {
            pdf_source_model::FRESH_IMPORT_OPTIONS
        } else {
            pdf_source_model::DEFAULT_IMPORT_OPTIONS
        };
        let adapter_sha = text::sha256(ADAPTER_SOURCE);
        let mut new_parse = None;
        let mut staged_parse = None;
        let mut pdf = PdfStatus::not_attempted();
        let mut error = selected.failure.clone();
        if let Some(version) = &version {
            let key = pdf_source_model::cache_key(
                &Fingerprint {
                    sha256: version.content_sha256.clone(),
                    byte_length: version.byte_length,
                },
                &ParserIdentity::pinned(),
                &adapter_sha,
                options,
            )?;
            let cached = if reparse {
                None
            } else {
                before.pdf.reports.iter().rev().find_map(|report| {
                    let attempt = before.pdf.attempts.get(report.pdf.parse_id.as_ref()?)?;
                    (attempt.source_version_id == version.version_id
                        && attempt.cache_key == key
                        && attempt.outcome_kind == "validated"
                        && matches!(attempt.status.as_str(), "parsed" | "needs_review"))
                    .then(|| (attempt.clone(), report.pdf.issues.clone()))
                })
            };
            if let Some((attempt, issues)) = cached {
                pdf = PdfStatus::from_attempt(&attempt, true, issues);
                error = attempt.error;
            } else {
                let input = staged_version.as_ref().map_or_else(
                    || root.join(&version.raw_snapshot),
                    |(_, path)| path.join("raw.pdf"),
                );
                let captured =
                    pdf_parse::parse_owned(&input, &writer.pdf_parse_path()?).map_err(|error| {
                        ProjectError::new(
                            "SOURCE_WRITE_INCOMPLETE",
                            3,
                            format!("PDF作业未能安全移交：{error}"),
                        )
                    })?;
                let (parse, issues) =
                    capture_attempt(version, &now, key, options, &adapter_sha, &captured)?;
                pdf = PdfStatus::from_attempt(&parse.attempt, false, issues);
                error = parse.attempt.error.clone();
                writer.stage_pdf_parse(&parse.attempt.parse_id, captured.tree)?;
                staged_parse = Some((
                    parse.attempt.parse_id.clone(),
                    root.join(".noetome-writing/parse"),
                ));
                new_parse = Some(parse);
            }
        }
        let report = PdfImportReport {
            schema_version: 2,
            report_kind: "source_pdf_import".into(),
            project_id: info.project_id.clone(),
            import_id: Uuid::new_v4().to_string(),
            imported_at: now.clone(),
            selected_path: selected
                .path
                .to_str()
                .ok_or_else(|| ProjectError::unsafe_path(&selected.path))?
                .into(),
            source_id,
            version_id: version.as_ref().map(|version| version.version_id.clone()),
            version,
            status: if pdf.parse_id.is_some() {
                overall_status(&pdf.status)
            } else {
                "failed"
            }
            .into(),
            reused: !create_version && selected.raw.is_some(),
            acquisition_status: if selected.raw.is_some() {
                "acquired"
            } else {
                "failed"
            }
            .into(),
            pending_extraction_count: pdf.region_count,
            pdf,
            knowledge_status: "pending".into(),
            error,
        };
        let plan = Plan {
            report,
            new_source,
            new_version: create_version,
            parse: new_parse,
        };
        let updated = writer
            .build_candidate(|connection, info| register(connection, info, root, before, &plan))?;
        let candidate = source::database_store_staged(
            writer.candidate_path(),
            root,
            &updated,
            staged_version
                .as_ref()
                .map(|(id, path)| (id.as_str(), path.as_path())),
            staged_parse
                .as_ref()
                .map(|(id, path)| (id.as_str(), path.as_path())),
        )?;
        if candidate
            .pdf
            .reports
            .iter()
            .find(|saved| saved.import_id == plan.report.import_id)
            != Some(&plan.report)
        {
            return Err(invalid("候选PDF报告与实际快照重读不匹配"));
        }
        let output = output(&plan.report)?;
        writer.commit(&updated, |path, marker| {
            if project::inspect_for_writer(path, marker)? != updated {
                return Err(invalid("发布的配置与PDF项目元数据不匹配"));
            }
            let stored = source::database_store_staged(
                &path.join(project::DATABASE),
                path,
                &updated,
                None,
                None,
            )?;
            if stored
                .pdf
                .reports
                .iter()
                .find(|saved| saved.import_id == plan.report.import_id)
                != Some(&plan.report)
            {
                return Err(invalid("发布的PDF报告与实际持久快照重读不匹配"));
            }
            Ok(())
        })?;
        Ok(output)
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
