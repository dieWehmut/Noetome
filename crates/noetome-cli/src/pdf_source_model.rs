//! Strict Schema3 PDF attempts and their immutable, independently checked
//! snapshots. Native PDF pages never become legacy UTF-8 SourceSpans.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::AtomicBool};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::pdf_model::{self, Manifest, Page, ParserIdentity, Region, RegionText, Report};
use crate::project::{self, ProjectError};
use crate::sidecar_model::{Fingerprint, Issue, Limits, ParseStatus, ResourceKind, SidecarError};
use crate::sidecar_process::Control;
use crate::sidecar_workspace::{Directory, GuardedFile, valid_id};
use crate::source_model::{
    SourceFailure, SourceVersion, invalid_record, require, snapshot_conflict, sql_failure, sql_u64,
    valid_failure, valid_hash, valid_selected_path, valid_time, valid_uuid,
};

pub(crate) const OUTCOME_RESOURCE_ID: &str = "parse-result-json";
pub(crate) const MARKDOWN_RESOURCE_ID: &str = "parse-result-md";
pub(crate) const FAILURE_RESOURCE_ID: &str = "failure-json";
pub(crate) const DEFAULT_IMPORT_OPTIONS: &str = "pdf-import-default-v1";
pub(crate) const FRESH_IMPORT_OPTIONS: &str = "pdf-import-fresh-v1";
const REPORT_BYTES: u64 = 16 * 1024 * 1024;
// 64 KiB diagnostics can expand sixfold as JSON control escapes; allow the
// bounded 16 KiB error message and envelope within the same explicit cap.
const FAILURE_BYTES: u64 = 512 * 1024;
const SNAPSHOT_BYTES: u64 = 96 * 1024 * 1024;

macro_rules! record {
    ($name:ident { $($(#[$attr:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct $name { $($(#[$attr])* pub(crate) $field: $ty),* }
        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool { same_record(self, other) }
        }
        impl Eq for $name {}
    };
}

record!(PdfParse {
    parse_id: String, source_version_id: String, created_at: String,
    cache_key: String, protocol_version: u32, requested_parser: ParserIdentity,
    #[serde(deserialize_with = "pdf_model::nullable")]
    parser: Option<ParserIdentity>, import_options_version: String,
    adapter_sha256: String, status: String, outcome_kind: String,
    #[serde(deserialize_with = "pdf_model::nullable")]
    page_count: Option<u64>, confirmed_page_count: u64, parsed_page_count: u64,
    needs_review_page_count: u64, failed_page_count: u64, region_count: u64,
    snapshot_path: String, outcome_sha256: String,
    #[serde(deserialize_with = "pdf_model::nullable")]
    manifest_sha256: Option<String>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    error: Option<SourceFailure>
});
record!(PdfStatus {
    #[serde(deserialize_with = "pdf_model::nullable")]
    parse_id: Option<String>, reused: bool, status: String,
    #[serde(deserialize_with = "pdf_model::nullable")]
    requested_parser: Option<ParserIdentity>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    parser: Option<ParserIdentity>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    protocol_version: Option<u32>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    import_options_version: Option<String>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    adapter_sha256: Option<String>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    cache_key: Option<String>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    page_count: Option<u64>,
    confirmed_page_count: u64, parsed_page_count: u64, needs_review_page_count: u64,
    failed_page_count: u64, region_count: u64,
    #[serde(deserialize_with = "pdf_model::nullable")]
    snapshot_path: Option<String>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    outcome_kind: Option<String>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    outcome_sha256: Option<String>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    manifest_sha256: Option<String>, issues: Vec<Issue>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    error: Option<SourceFailure>
});
record!(PdfImportReport {
    schema_version: u32, report_kind: String, project_id: String, import_id: String,
    imported_at: String, selected_path: String,
    #[serde(deserialize_with = "pdf_model::nullable")]
    source_id: Option<String>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    version_id: Option<String>,
    #[serde(deserialize_with = "required_pdf_version")]
    version: Option<SourceVersion>, status: String,
    reused: bool, acquisition_status: String, pdf: PdfStatus,
    pending_extraction_count: u64, knowledge_status: String,
    #[serde(deserialize_with = "pdf_model::nullable")]
    error: Option<SourceFailure>
});
record!(PdfResource {
    resource_id: String,
    relative_path: String,
    sha256: String,
    byte_length: u64,
    kind: String,
    media_type: String,
    confidence: String
});

// The old SourceVersion DTO and its decoder retain their existing behavior.
// At the PDF report boundary, nullable entries are mandatory. A typed proxy
// also preserves serde's duplicate-field and unknown-field rejection.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequiredPdfVersion {
    source_id: String,
    version_id: String,
    revision: u64,
    created_at: String,
    media_type: String,
    display_name: String,
    content_sha256: String,
    byte_length: u64,
    #[serde(deserialize_with = "pdf_model::nullable")]
    normalized_sha256: Option<String>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    normalization_version: Option<String>,
    ingest_status: String,
    raw_snapshot: String,
    #[serde(deserialize_with = "pdf_model::nullable")]
    normalized_snapshot: Option<String>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    error: Option<SourceFailure>,
}
fn required_pdf_version<'de, D: serde::Deserializer<'de>>(
    de: D,
) -> Result<Option<SourceVersion>, D::Error> {
    Option::<RequiredPdfVersion>::deserialize(de).map(|version| {
        version.map(|version| SourceVersion {
            source_id: version.source_id,
            version_id: version.version_id,
            revision: version.revision,
            created_at: version.created_at,
            media_type: version.media_type,
            display_name: version.display_name,
            content_sha256: version.content_sha256,
            byte_length: version.byte_length,
            normalized_sha256: version.normalized_sha256,
            normalization_version: version.normalization_version,
            ingest_status: version.ingest_status,
            raw_snapshot: version.raw_snapshot,
            normalized_snapshot: version.normalized_snapshot,
            error: version.error,
        })
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PdfStore {
    pub(crate) attempts: BTreeMap<String, PdfParse>,
    pub(crate) reports: Vec<PdfImportReport>,
    pub(crate) import_orders: BTreeMap<String, i64>,
    pub(crate) pages: BTreeMap<String, BTreeMap<u32, Page>>,
    pub(crate) regions: BTreeMap<String, BTreeMap<String, Region>>,
    pub(crate) resources: BTreeMap<String, BTreeMap<String, PdfResource>>,
}
impl PartialEq for PdfStore {
    fn eq(&self, other: &Self) -> bool {
        same_record(self, other)
    }
}
impl Eq for PdfStore {}

pub(crate) fn cache_key(
    input: &Fingerprint,
    parser: &ParserIdentity,
    adapter_sha: &str,
    import_options: &str,
) -> Result<String, ProjectError> {
    require(
        valid_hash(&input.sha256)
            && input.byte_length <= Limits::default().input_bytes
            && valid_hash(adapter_sha)
            && matches!(
                import_options,
                DEFAULT_IMPORT_OPTIONS | FRESH_IMPORT_OPTIONS
            )
            && [
                &parser.engine,
                &parser.engine_version,
                &parser.engine_dependency,
                &parser.engine_dependency_version,
                &parser.adapter_version,
                &parser.options_version,
                &parser.python_version,
            ]
            .iter()
            .all(|field| {
                !field.trim().is_empty()
                    && field.len() <= 256
                    && !field.chars().any(char::is_control)
            }),
        "PDF 缓存键的指纹、解析器身份或选项无效",
    )?;
    // serde_json's default Map is key-sorted, including both nested objects.
    // Compact JSON is the versioned preimage; no platform path or timestamp
    // participates, so byte-identical renamed sources retain the same key.
    let canonical = serde_json::to_vec(&serde_json::json!({
        "adapter_sha256": adapter_sha,
        "cache_version": "pdf-source-cache-v1",
        "import_options_version": import_options,
        "input_fingerprint": input,
        "protocol_version": 1,
        "requested_parser": parser,
    }))
    .map_err(invalid_record)?;
    Ok(crate::text::sha256(&canonical))
}

fn same_record<T: Serialize>(left: &T, right: &T) -> bool {
    match (serde_json::to_value(left), serde_json::to_value(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}
fn json_record<T: serde::de::DeserializeOwned>(value: &str) -> rusqlite::Result<T> {
    if value.len() as u64 > REPORT_BYTES {
        return Err(rusqlite::Error::InvalidQuery);
    }
    serde_json::from_str(value).map_err(|_| rusqlite::Error::InvalidQuery)
}
fn sql_u32(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u32> {
    u32::try_from(row.get::<_, i64>(index)?).map_err(|_| rusqlite::Error::InvalidQuery)
}
fn sql_optional_u64(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<u64>> {
    row.get::<_, Option<i64>>(index)?
        .map(|value| u64::try_from(value).map_err(|_| rusqlite::Error::InvalidQuery))
        .transpose()
}
fn sql_bool(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<bool> {
    match row.get::<_, i64>(index)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}
pub(crate) fn status_name(status: &ParseStatus) -> &'static str {
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
impl PdfStatus {
    pub(crate) fn not_attempted() -> Self {
        Self {
            parse_id: None,
            reused: false,
            status: "not_attempted".into(),
            requested_parser: None,
            parser: None,
            protocol_version: None,
            import_options_version: None,
            adapter_sha256: None,
            cache_key: None,
            page_count: None,
            confirmed_page_count: 0,
            parsed_page_count: 0,
            needs_review_page_count: 0,
            failed_page_count: 0,
            region_count: 0,
            snapshot_path: None,
            outcome_kind: None,
            outcome_sha256: None,
            manifest_sha256: None,
            issues: Vec::new(),
            error: None,
        }
    }
    pub(crate) fn from_attempt(attempt: &PdfParse, reused: bool, issues: Vec<Issue>) -> Self {
        Self {
            parse_id: Some(attempt.parse_id.clone()),
            reused,
            status: attempt.status.clone(),
            requested_parser: Some(attempt.requested_parser.clone()),
            parser: attempt.parser.clone(),
            protocol_version: Some(attempt.protocol_version),
            import_options_version: Some(attempt.import_options_version.clone()),
            adapter_sha256: Some(attempt.adapter_sha256.clone()),
            cache_key: Some(attempt.cache_key.clone()),
            page_count: attempt.page_count,
            confirmed_page_count: attempt.confirmed_page_count,
            parsed_page_count: attempt.parsed_page_count,
            needs_review_page_count: attempt.needs_review_page_count,
            failed_page_count: attempt.failed_page_count,
            region_count: attempt.region_count,
            snapshot_path: Some(attempt.snapshot_path.clone()),
            outcome_kind: Some(attempt.outcome_kind.clone()),
            outcome_sha256: Some(attempt.outcome_sha256.clone()),
            manifest_sha256: attempt.manifest_sha256.clone(),
            issues,
            error: attempt.error.clone(),
        }
    }
}
impl PdfStore {
    /// Load every edge before the unified Store validates version/import
    /// history. Snapshot-derived issues are hydrated only after the bytes audit.
    pub(crate) fn load(connection: &Connection, project_id: &str) -> Result<Self, ProjectError> {
        let mut store = Self::default();
        let mut query = connection.prepare(
            "SELECT parse_id,source_version_id,created_at,cache_key,protocol_version,requested_parser_json,parser_json,import_options_version,adapter_sha256,status,outcome_kind,page_count,confirmed_page_count,parsed_page_count,needs_review_page_count,failed_page_count,region_count,snapshot_path,outcome_sha256,manifest_sha256,error_code,error_message FROM pdf_parses ORDER BY rowid"
        ).map_err(invalid_record)?;
        let rows = query
            .query_map([], |row| {
                let requested: String = row.get(5)?;
                let parser: Option<String> = row.get(6)?;
                Ok(PdfParse {
                    parse_id: row.get(0)?,
                    source_version_id: row.get(1)?,
                    created_at: row.get(2)?,
                    cache_key: row.get(3)?,
                    protocol_version: sql_u32(row, 4)?,
                    requested_parser: json_record(&requested)?,
                    parser: parser.as_deref().map(json_record).transpose()?,
                    import_options_version: row.get(7)?,
                    adapter_sha256: row.get(8)?,
                    status: row.get(9)?,
                    outcome_kind: row.get(10)?,
                    page_count: sql_optional_u64(row, 11)?,
                    confirmed_page_count: sql_u64(row, 12)?,
                    parsed_page_count: sql_u64(row, 13)?,
                    needs_review_page_count: sql_u64(row, 14)?,
                    failed_page_count: sql_u64(row, 15)?,
                    region_count: sql_u64(row, 16)?,
                    snapshot_path: row.get(17)?,
                    outcome_sha256: row.get(18)?,
                    manifest_sha256: row.get(19)?,
                    error: sql_failure(row, 20, 21)?,
                })
            })
            .map_err(invalid_record)?;
        for row in rows {
            let attempt = row.map_err(invalid_record)?;
            let id = attempt.parse_id.clone();
            require(
                store.attempts.insert(id.clone(), attempt).is_none(),
                "PDF 解析身份重复",
            )?;
            store.pages.insert(id.clone(), BTreeMap::new());
            store.regions.insert(id.clone(), BTreeMap::new());
            store.resources.insert(id, BTreeMap::new());
        }
        let mut query = connection.prepare(
            "SELECT parse_id,physical_page_index,display_page_number,printed_page_label,status,record_json FROM pdf_pages ORDER BY parse_id,physical_page_index"
        ).map_err(invalid_record)?;
        let rows = query
            .query_map([], |row| {
                let record: String = row.get(5)?;
                Ok((
                    row.get::<_, String>(0)?,
                    sql_u32(row, 1)?,
                    sql_u32(row, 2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    json_record::<Page>(&record)?,
                ))
            })
            .map_err(invalid_record)?;
        for row in rows {
            let (parse, index, display, label, status, page) = row.map_err(invalid_record)?;
            require(
                index == page.physical_page_index
                    && display == page.display_page_number
                    && label == page.printed_page_label
                    && status == status_name(&page.status),
                "PDF 页 SQL 索引和完整 JSON 不一致",
            )?;
            let pages = store
                .pages
                .get_mut(&parse)
                .ok_or_else(|| invalid_record("PDF 页引用未知解析"))?;
            require(
                pages.len() < pdf_model::MAX_PAGES && pages.insert(index, page).is_none(),
                "PDF 页身份重复或超过上限",
            )?;
        }
        let mut query = connection.prepare(
            "SELECT parse_id,resource_id,relative_path,sha256,byte_length,kind,media_type,confidence FROM pdf_resources ORDER BY parse_id,resource_id"
        ).map_err(invalid_record)?;
        let rows = query
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    PdfResource {
                        resource_id: row.get(1)?,
                        relative_path: row.get(2)?,
                        sha256: row.get(3)?,
                        byte_length: sql_u64(row, 4)?,
                        kind: row.get(5)?,
                        media_type: row.get(6)?,
                        confidence: row.get(7)?,
                    },
                ))
            })
            .map_err(invalid_record)?;
        for row in rows {
            let (parse, resource) = row.map_err(invalid_record)?;
            let resources = store
                .resources
                .get_mut(&parse)
                .ok_or_else(|| invalid_record("PDF 资源引用未知解析"))?;
            require(
                resources.len() < Limits::default().resources as usize + 2
                    && resources
                        .insert(resource.resource_id.clone(), resource)
                        .is_none(),
                "PDF 资源身份重复或超过上限",
            )?;
        }
        let mut query = connection.prepare(
            "SELECT parse_id,region_id,physical_page_index,locator_kind,bbox_left,bbox_top,bbox_right,bbox_bottom,text_resource_id,text_resource_sha256,start_byte,end_byte,glyph_indices_json FROM pdf_regions ORDER BY parse_id,region_id"
        ).map_err(invalid_record)?;
        let rows = query
            .query_map([], |row| {
                let glyphs: String = row.get(12)?;
                Ok((
                    row.get::<_, String>(0)?,
                    Region {
                        region_id: row.get(1)?,
                        physical_page_index: sql_u32(row, 2)?,
                        locator_kind: row.get(3)?,
                        bbox: [row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?],
                        text: RegionText {
                            resource_id: row.get(8)?,
                            resource_sha256: row.get(9)?,
                            start_byte: sql_u64(row, 10)?,
                            end_byte: sql_u64(row, 11)?,
                        },
                        glyph_indices: json_record(&glyphs)?,
                    },
                ))
            })
            .map_err(invalid_record)?;
        for row in rows {
            let (parse, region) = row.map_err(invalid_record)?;
            let regions = store
                .regions
                .get_mut(&parse)
                .ok_or_else(|| invalid_record("PDF 区域引用未知解析"))?;
            require(
                regions.len() < pdf_model::MAX_GLYPHS
                    && regions.insert(region.region_id.clone(), region).is_none(),
                "PDF 区域身份重复或超过上限",
            )?;
        }
        let mut query = connection.prepare(
            "SELECT import_id,imported_at,selected_path,source_id,version_id,parse_id,status,reused,parse_reused,acquisition_status,error_code,error_message,rowid FROM pdf_imports ORDER BY rowid"
        ).map_err(invalid_record)?;
        let rows = query
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    sql_bool(row, 7)?,
                    sql_bool(row, 8)?,
                    row.get::<_, String>(9)?,
                    sql_failure(row, 10, 11)?,
                    row.get::<_, i64>(12)?,
                ))
            })
            .map_err(invalid_record)?;
        for row in rows {
            let (
                id,
                time,
                path,
                source,
                version,
                parse,
                status,
                reused,
                parse_reused,
                acquisition,
                error,
                order,
            ) = row.map_err(invalid_record)?;
            let pdf = match parse {
                Some(parse) => PdfStatus::from_attempt(
                    store
                        .attempts
                        .get(&parse)
                        .ok_or_else(|| invalid_record("PDF 导入引用未知解析"))?,
                    parse_reused,
                    Vec::new(),
                ),
                None => {
                    require(!parse_reused, "未尝试解析不能复用解析缓存")?;
                    PdfStatus::not_attempted()
                }
            };
            require(
                store.import_orders.insert(id.clone(), order).is_none(),
                "PDF 导入身份重复",
            )?;
            store.reports.push(PdfImportReport {
                schema_version: 2,
                report_kind: "source_pdf_import".into(),
                project_id: project_id.into(),
                import_id: id,
                imported_at: time,
                selected_path: path,
                source_id: source,
                version_id: version,
                version: None,
                status,
                reused,
                acquisition_status: acquisition,
                pending_extraction_count: pdf.region_count,
                knowledge_status: "pending".into(),
                pdf,
                error,
            });
        }
        Ok(store)
    }

    pub(crate) fn validate_records(
        &self,
        project_id: &str,
        sources: &BTreeMap<String, String>,
        versions: &BTreeMap<String, SourceVersion>,
    ) -> Result<(), ProjectError> {
        require(
            self.pages.keys().eq(self.attempts.keys())
                && self.regions.keys().eq(self.attempts.keys())
                && self.resources.keys().eq(self.attempts.keys()),
            "PDF 页、区域或资源解析索引不完整",
        )?;
        for (id, attempt) in &self.attempts {
            let version = versions
                .get(&attempt.source_version_id)
                .ok_or_else(|| invalid_record("PDF 解析引用不存在的版本"))?;
            require(
                id == &attempt.parse_id
                    && valid_uuid(id)
                    && valid_time(&attempt.created_at)
                    && version.media_type == "application/pdf"
                    && version.ingest_status == "acquired",
                "PDF 解析身份、UTC 时间或版本引用无效",
            )?;
            let expected_key = cache_key(
                &Fingerprint {
                    sha256: version.content_sha256.clone(),
                    byte_length: version.byte_length,
                },
                &attempt.requested_parser,
                &attempt.adapter_sha256,
                &attempt.import_options_version,
            )?;
            require(
                attempt.protocol_version == 1
                    && valid_hash(&attempt.cache_key)
                    && attempt.cache_key == expected_key
                    && attempt.requested_parser == ParserIdentity::pinned()
                    && valid_hash(&attempt.outcome_sha256)
                    && attempt.manifest_sha256.as_deref().is_none_or(valid_hash)
                    && attempt.snapshot_path == format!(".noetome-pdf-parses/{id}"),
                "PDF 协议、缓存键、请求身份、指纹或固定快照路径无效",
            )?;
            let sum = attempt
                .parsed_page_count
                .checked_add(attempt.needs_review_page_count)
                .and_then(|sum| sum.checked_add(attempt.failed_page_count));
            require(
                sum == Some(attempt.confirmed_page_count)
                    && attempt.confirmed_page_count <= pdf_model::MAX_PAGES as u64
                    && attempt
                        .page_count
                        .is_none_or(|count| count == attempt.confirmed_page_count)
                    && attempt.region_count <= pdf_model::MAX_GLYPHS as u64,
                "PDF 页或区域计数不完整、越界或溢出",
            )?;
            require(
                match attempt.status.as_str() {
                    "parsed" => {
                        attempt.error.is_none()
                            && attempt.page_count.is_some_and(|count| count > 0)
                            && attempt.needs_review_page_count == 0
                            && attempt.failed_page_count == 0
                    }
                    "needs_review" => {
                        attempt.error.is_none()
                            && attempt.page_count.is_some_and(|count| count > 0)
                            && attempt.failed_page_count == 0
                    }
                    "failed" => {
                        attempt.error.as_ref().is_some_and(valid_pdf_failure)
                            && (attempt.failed_page_count > 0
                                || attempt.page_count.is_none()
                                || attempt.page_count == Some(0))
                    }
                    _ => false,
                },
                "PDF 总状态、已完成页或真实错误不一致",
            )?;
            match attempt.outcome_kind.as_str() {
                "validated" => require(
                    attempt.parser.as_ref() == Some(&ParserIdentity::pinned())
                        && attempt.parser.as_ref() == Some(&attempt.requested_parser)
                        && attempt.manifest_sha256.is_some(),
                    "已验证 PDF 必须保留受支持的实际解析器和 manifest 指纹",
                )?,
                "host_failed" => require(
                    attempt.status == "failed"
                        && attempt.parser.is_none()
                        && attempt.manifest_sha256.is_none()
                        && attempt.page_count.is_none()
                        && attempt.confirmed_page_count == 0
                        && attempt.region_count == 0,
                    "宿主失败不能声称实际解析器、页或区域已确认",
                )?,
                _ => return Err(invalid_record("PDF 结果类型未知")),
            }
            let pages = &self.pages[id];
            let regions = &self.regions[id];
            let resources = &self.resources[id];
            require(
                pages.len() as u64 == attempt.confirmed_page_count
                    && regions.len() as u64 == attempt.region_count,
                "PDF 页或区域计数与 SQL 记录不一致",
            )?;
            let mut counts = [0u64; 3];
            let mut expected_regions = BTreeMap::new();
            let mut expected_resources = BTreeSet::new();
            for (ordinal, (index, page)) in pages.iter().enumerate() {
                require(
                    *index as usize == ordinal
                        && page.physical_page_index == *index
                        && page.display_page_number == index.checked_add(1).unwrap_or(0),
                    "PDF 物理页清单不是完整连续顺序",
                )?;
                let count = match page.status {
                    ParseStatus::Parsed => 0,
                    ParseStatus::NeedsReview => 1,
                    ParseStatus::Failed => 2,
                };
                counts[count] += 1;
                require(
                    page.glyphs.len() <= pdf_model::MAX_PAGE_GLYPHS,
                    "PDF 页字形超过上限",
                )?;
                if let Some(text) = &page.text {
                    let resource = resources
                        .get(&text.resource_id)
                        .ok_or_else(|| invalid_record("PDF 页正文资源不存在"))?;
                    require(
                        text.resource_id == pdf_model::text_id(*index as usize)
                            && resource.sha256 == text.sha256
                            && resource.byte_length == text.byte_length,
                        "PDF 页正文指纹或长度与资源不一致",
                    )?;
                    validate_resource(
                        resource,
                        &text.resource_id,
                        &format!("parser/pages/{index:06}.txt"),
                        "utf8",
                        "text/plain",
                        "confirmed",
                    )?;
                    expected_resources.insert(text.resource_id.clone());
                }
                for region in &page.regions {
                    require(
                        valid_id(&region.region_id)
                            && region.locator_kind == "pdf_page_region"
                            && region.physical_page_index == *index
                            && valid_bbox(&region.bbox)
                            && !region.glyph_indices.is_empty()
                            && region
                                .glyph_indices
                                .iter()
                                .all(|glyph| (*glyph as usize) < page.glyphs.len())
                            && expected_regions
                                .insert(region.region_id.clone(), region)
                                .is_none(),
                        "PDF 原生区域身份、页、bbox 或字形索引无效",
                    )?;
                    let resource = resources
                        .get(&region.text.resource_id)
                        .ok_or_else(|| invalid_record("PDF 区域正文资源不存在"))?;
                    require(
                        page.text
                            .as_ref()
                            .is_some_and(|text| text.resource_id == region.text.resource_id)
                            && resource.sha256 == region.text.resource_sha256
                            && resource.kind == "utf8"
                            && resource.confidence == "confirmed"
                            && region.text.start_byte < region.text.end_byte
                            && region.text.end_byte <= resource.byte_length,
                        "PDF 区域正文引用、SHA 或半开范围无效",
                    )?;
                }
            }
            require(
                counts
                    == [
                        attempt.parsed_page_count,
                        attempt.needs_review_page_count,
                        attempt.failed_page_count,
                    ]
                    && expected_regions.len() == regions.len()
                    && expected_regions.iter().all(|(id, expected)| {
                        regions
                            .get(id)
                            .is_some_and(|actual| same_record(*expected, actual))
                    }),
                "PDF 完整页 JSON 和独立区域索引不一致",
            )?;
            let mut paths = BTreeSet::new();
            let mut bytes = 0u64;
            for (resource_id, resource) in resources {
                require(
                    resource_id == &resource.resource_id
                        && valid_id(resource_id)
                        && valid_hash(&resource.sha256)
                        && crate::fixture_input::valid_relative_path(&resource.relative_path)
                        && paths.insert(resource.relative_path.to_lowercase())
                        && resource.byte_length <= REPORT_BYTES
                        && matches!(resource.kind.as_str(), "binary" | "utf8")
                        && matches!(
                            resource.confidence.as_str(),
                            "confirmed" | "unconfirmed" | "diagnostic"
                        ),
                    "PDF 资源路径、身份、SHA、长度、类型或确认状态无效",
                )?;
                bytes = bytes
                    .checked_add(resource.byte_length)
                    .ok_or_else(|| invalid_record("PDF 资源字节数溢出"))?;
            }
            require(bytes <= SNAPSHOT_BYTES, "PDF 资源总字节超过上限")?;
            if attempt.outcome_kind == "validated" {
                let raw = resources
                    .get(pdf_model::RAW_ID)
                    .ok_or_else(|| invalid_record("PDF 原始解析资源缺失"))?;
                validate_resource(
                    raw,
                    pdf_model::RAW_ID,
                    "parser/raw.pdf",
                    "binary",
                    "application/pdf",
                    "confirmed",
                )?;
                require(
                    raw.sha256 == version.content_sha256 && raw.byte_length == version.byte_length,
                    "PDF 解析原始资源和 SourceVersion 指纹不一致",
                )?;
                let manifest = resources
                    .get(pdf_model::MANIFEST_ID)
                    .ok_or_else(|| invalid_record("PDF manifest 资源缺失"))?;
                validate_resource(
                    manifest,
                    pdf_model::MANIFEST_ID,
                    "parser/pdf-manifest.json",
                    "utf8",
                    "application/json",
                    "confirmed",
                )?;
                require(
                    Some(&manifest.sha256) == attempt.manifest_sha256.as_ref(),
                    "PDF manifest SHA 不一致",
                )?;
                let outcome = resources
                    .get(OUTCOME_RESOURCE_ID)
                    .ok_or_else(|| invalid_record("PDF Core JSON 报告资源缺失"))?;
                validate_resource(
                    outcome,
                    OUTCOME_RESOURCE_ID,
                    "parse-result.json",
                    "utf8",
                    "application/json",
                    "diagnostic",
                )?;
                require(
                    outcome.sha256 == attempt.outcome_sha256,
                    "PDF Core JSON 报告 SHA 不一致",
                )?;
                validate_resource(
                    resources
                        .get(MARKDOWN_RESOURCE_ID)
                        .ok_or_else(|| invalid_record("PDF Core Markdown 报告资源缺失"))?,
                    MARKDOWN_RESOURCE_ID,
                    "parse-result.md",
                    "utf8",
                    "text/markdown",
                    "diagnostic",
                )?;
                expected_resources.extend([
                    pdf_model::RAW_ID.to_string(),
                    pdf_model::MANIFEST_ID.to_string(),
                    OUTCOME_RESOURCE_ID.to_string(),
                    MARKDOWN_RESOURCE_ID.to_string(),
                ]);
                require(
                    expected_resources.iter().eq(resources.keys()),
                    "PDF 资源清单存在缺失或额外索引",
                )?;
            } else {
                let failure = resources
                    .get(FAILURE_RESOURCE_ID)
                    .ok_or_else(|| invalid_record("PDF 宿主失败报告资源缺失"))?;
                validate_resource(
                    failure,
                    FAILURE_RESOURCE_ID,
                    "failure.json",
                    "utf8",
                    "application/json",
                    "diagnostic",
                )?;
                require(
                    failure.sha256 == attempt.outcome_sha256
                        && failure.byte_length <= FAILURE_BYTES,
                    "PDF 宿主失败报告 SHA 或长度无效",
                )?;
                for resource in resources
                    .values()
                    .filter(|resource| resource.resource_id != FAILURE_RESOURCE_ID)
                {
                    let (id, kind, media) = parser_resource_identity(&resource.relative_path)
                        .ok_or_else(|| invalid_record("PDF 宿主失败包含未知残留路径"))?;
                    validate_resource(
                        resource,
                        &id,
                        &resource.relative_path,
                        kind,
                        media,
                        "unconfirmed",
                    )?;
                }
            }
        }
        let mut imports = BTreeSet::new();
        for report in &self.reports {
            require(
                report.schema_version == 2
                    && report.report_kind == "source_pdf_import"
                    && report.project_id == project_id
                    && valid_uuid(&report.import_id)
                    && imports.insert(report.import_id.clone())
                    && valid_time(&report.imported_at)
                    && valid_selected_path(&report.selected_path)
                    && report.knowledge_status == "pending"
                    && report
                        .source_id
                        .as_ref()
                        .is_none_or(|id| sources.contains_key(id)),
                "PDF 导入报告身份、字段、UTC 时间、选取路径或知识状态无效",
            )?;
            match report.acquisition_status.as_str() {
                "acquired" => {
                    let version = report
                        .version_id
                        .as_ref()
                        .and_then(|id| versions.get(id))
                        .ok_or_else(|| invalid_record("PDF 完整获取的版本引用缺失"))?;
                    let parse = report
                        .pdf
                        .parse_id
                        .as_ref()
                        .and_then(|id| self.attempts.get(id))
                        .ok_or_else(|| invalid_record("PDF 完整获取的解析尝试缺失"))?;
                    require(
                        version.media_type == "application/pdf"
                            && report.version.as_ref() == Some(version)
                            && report.source_id.as_ref() == Some(&version.source_id)
                            && parse.source_version_id == version.version_id
                            && report.status == overall_status(&parse.status)
                            && report.error == parse.error
                            && report.pending_extraction_count == parse.region_count
                            && report.pdf
                                == PdfStatus::from_attempt(
                                    parse,
                                    report.pdf.reused,
                                    report.pdf.issues.clone(),
                                ),
                        "PDF 导入报告、原始版本、解析和计数或错误不一致",
                    )?;
                    for issue in &report.pdf.issues {
                        require(
                            issue.code.starts_with("PDF_")
                                && valid_id(&issue.code)
                                && !issue.message.trim().is_empty()
                                && issue.message.len() <= 4096
                                && issue
                                    .resource_ids
                                    .iter()
                                    .all(|id| self.resources[&parse.parse_id].contains_key(id))
                                && issue.unit_ids.iter().all(|id| {
                                    id == "pdf-document"
                                        || self.pages[&parse.parse_id]
                                            .keys()
                                            .any(|index| pdf_model::page_id(*index as usize) == *id)
                                }),
                            "PDF 问题与持久页或资源引用不一致",
                        )?;
                    }
                }
                "failed" => require(
                    report.status == "failed"
                        && report.version_id.is_none()
                        && report.version.is_none()
                        && !report.reused
                        && report.pending_extraction_count == 0
                        && report.pdf == PdfStatus::not_attempted()
                        && report.error.as_ref().is_some_and(|error| {
                            valid_failure(error)
                                && matches!(
                                    error.code.as_str(),
                                    "SOURCE_READ_ERROR" | "SOURCE_TOO_LARGE" | "SOURCE_CHANGED"
                                )
                        }),
                    "PDF 未完整读取只能保留真实获取失败，不得捏造版本或页",
                )?,
                _ => return Err(invalid_record("PDF 原始获取状态未知")),
            }
        }
        Ok(())
    }

    pub(crate) fn validate_snapshots(
        &self,
        root: &Path,
        project_id: &str,
        versions: &BTreeMap<String, SourceVersion>,
        staged: Option<(&str, &Path)>,
    ) -> Result<(), ProjectError> {
        let control = Control::new(30000, Arc::new(AtomicBool::new(false)));
        let cache = CacheGuards::capture(root, project_id, &self.attempts, staged, &control)?;
        let mut checked = Vec::new();
        for attempt in self.attempts.values() {
            let version = versions
                .get(&attempt.source_version_id)
                .ok_or_else(|| invalid_record("PDF 快照缺少其原始版本"))?;
            let location = parse_location(root, &attempt.parse_id, staged);
            let (mut tree, _) =
                self.validate_attempt_snapshot(attempt, version, &location, &control)?;
            tree.bytes.clear();
            checked.push(tree);
        }
        for tree in &checked {
            tree.verify(&control)?;
        }
        cache.verify(&control)?;
        Ok(())
    }

    /// Reports contain issues only from verified persisted outcome bytes.
    /// The SQL-only loader intentionally cannot manufacture this information.
    pub(crate) fn report_from_snapshot(
        &self,
        root: &Path,
        report: &PdfImportReport,
        staged: Option<(&str, &Path)>,
    ) -> Result<PdfImportReport, ProjectError> {
        let registered = self
            .reports
            .iter()
            .find(|registered| registered.import_id == report.import_id)
            .ok_or_else(|| invalid_record("PDF 报告身份未在本项目导入表中登记"))?;
        let mut expected = registered.clone();
        let mut supplied = report.clone();
        expected.pdf.issues.clear();
        supplied.pdf.issues.clear();
        require(
            expected == supplied,
            "PDF 待补齐报告与已读取 SQL 事实不一致",
        )?;
        if report.acquisition_status == "failed" {
            require(
                report.pdf == PdfStatus::not_attempted(),
                "获取失败不能包含 PDF 尝试",
            )?;
            return Ok(report.clone());
        }
        let parse_id = report
            .pdf
            .parse_id
            .as_ref()
            .ok_or_else(|| invalid_record("PDF 报告缺少解析身份"))?;
        let attempt = self
            .attempts
            .get(parse_id)
            .ok_or_else(|| invalid_record("PDF 报告引用未知解析"))?;
        let version = report
            .version
            .as_ref()
            .ok_or_else(|| invalid_record("PDF 报告尚未联合读取原始版本"))?;
        let control = Control::new(30000, Arc::new(AtomicBool::new(false)));
        let cache =
            CacheGuards::capture(root, &report.project_id, &self.attempts, staged, &control)?;
        let (tree, issues) = self.validate_attempt_snapshot(
            attempt,
            version,
            &parse_location(root, parse_id, staged),
            &control,
        )?;
        tree.verify(&control)?;
        cache.verify(&control)?;
        let mut hydrated = report.clone();
        hydrated.pdf = PdfStatus::from_attempt(attempt, report.pdf.reused, issues);
        Ok(hydrated)
    }

    fn validate_attempt_snapshot(
        &self,
        attempt: &PdfParse,
        version: &SourceVersion,
        location: &Path,
        control: &Control,
    ) -> Result<(CheckedTree, Vec<Issue>), ProjectError> {
        let resources = self
            .resources
            .get(&attempt.parse_id)
            .ok_or_else(|| invalid_record("PDF 快照缺少资源索引"))?;
        let tree = CheckedTree::open(location, resources, control)?;
        let issues = if attempt.outcome_kind == "validated" {
            // This is the actual B03-T02 native semantic inspector. It starts
            // no Python process and reopens its own exact resource guards.
            let inspected = crate::pdf_parse::inspect(location).map_err(snapshot_error)?;
            let report: Report = serde_json::from_slice(tree.resource(OUTCOME_RESOURCE_ID)?)
                .map_err(|_| invalid_snapshot("PDF Core JSON 报告格式无效或包含未知字段"))?;
            let audited: Report = serde_json::from_str(&inspected.json)
                .map_err(|_| invalid_snapshot("PDF Core 重查报告格式无效"))?;
            let manifest: Manifest = serde_json::from_slice(tree.resource(pdf_model::MANIFEST_ID)?)
                .map_err(|_| invalid_snapshot("PDF manifest 格式无效或包含未知字段"))?;
            let expected_error = if attempt.status == "failed" {
                inspected
                    .failure
                    .map(|(code, message)| SourceFailure { code, message })
            } else {
                None
            };
            snapshot_require(
                same_record(&report, &audited)
                    && report.input_fingerprint.sha256 == version.content_sha256
                    && report.input_fingerprint.byte_length == version.byte_length
                    && manifest.input_fingerprint.sha256 == version.content_sha256
                    && manifest.input_fingerprint.byte_length == version.byte_length
                    && attempt.parser.as_ref() == Some(&report.parser)
                    && attempt.requested_parser == report.parser
                    && attempt.protocol_version == report.sidecar.protocol_version
                    && attempt.status == status_name(&report.status)
                    && attempt.page_count == report.page_count.map(u64::from)
                    && attempt.confirmed_page_count == manifest.pages.len() as u64
                    && attempt.parsed_page_count == u64::from(report.parsed_page_count)
                    && attempt.needs_review_page_count == u64::from(report.needs_review_page_count)
                    && attempt.failed_page_count == u64::from(report.failed_page_count)
                    && attempt.error == expected_error
                    && attempt.manifest_sha256.as_ref() == Some(&report.manifest_sha256)
                    && report.manifest_path == "parser/pdf-manifest.json",
                "PDF SQL 尝试状态、原始指纹、解析器、页计数或错误与实际报告不一致",
            )?;
            let pages = self
                .pages
                .get(&attempt.parse_id)
                .ok_or_else(|| invalid_record("PDF 快照缺少页索引"))?;
            let regions = self
                .regions
                .get(&attempt.parse_id)
                .ok_or_else(|| invalid_record("PDF 快照缺少区域索引"))?;
            snapshot_require(
                pages.len() == manifest.pages.len()
                    && manifest.pages.iter().all(|page| {
                        pages
                            .get(&page.physical_page_index)
                            .is_some_and(|stored| same_record(page, stored))
                    }),
                "PDF SQL 完整页 JSON 与真实 manifest 不一致",
            )?;
            let actual_regions: Vec<_> = manifest
                .pages
                .iter()
                .flat_map(|page| &page.regions)
                .collect();
            snapshot_require(
                actual_regions.len() == regions.len()
                    && actual_regions.len() as u64 == attempt.region_count
                    && actual_regions.iter().all(|region| {
                        regions
                            .get(&region.region_id)
                            .is_some_and(|stored| same_record(*region, stored))
                    }),
                "PDF SQL 原生区域和真实 manifest 不一致",
            )?;
            snapshot_require(
                resources.len() == report.sidecar.resources.len() + 2,
                "PDF SQL 资源数量与真实 parser 和 Core 报告不一致",
            )?;
            for actual in &report.sidecar.resources {
                let stored = resources
                    .get(&actual.resource_id)
                    .ok_or_else(|| invalid_snapshot("真实 parser 资源未登记"))?;
                let path = format!("parser/{}", actual.relative_path);
                let (_, kind, media) = parser_resource_identity(&path)
                    .ok_or_else(|| invalid_snapshot("真实 parser 资源路径超出当前支持范围"))?;
                snapshot_require(
                    stored.relative_path == path
                        && stored.sha256 == actual.fingerprint.sha256
                        && stored.byte_length == actual.fingerprint.byte_length
                        && stored.kind == kind
                        && stored.media_type == media
                        && stored.confidence == "confirmed"
                        && ((kind == "binary" && actual.kind == ResourceKind::Binary)
                            || (kind == "utf8" && actual.kind == ResourceKind::Utf8Text)),
                    "PDF SQL parser 资源索引与实际严格清单不一致",
                )?;
            }
            report.sidecar.issues
        } else {
            let failure: FailureJson = serde_json::from_slice(tree.resource(FAILURE_RESOURCE_ID)?)
                .map_err(|_| invalid_snapshot("PDF 宿主失败 JSON 格式无效或包含未知字段"))?;
            snapshot_require(
                failure.schema_version == 1
                    && failure.report_kind == "pdf_parse_failure"
                    && failure.status == "failed"
                    && failure.raw_acquisition == "not_confirmed"
                    && failure.input_fingerprint.is_none()
                    && failure.page_count.is_none()
                    && failure.confirmed_pages.is_empty()
                    && failure.error.diagnostics.len()
                        <= 3 * Limits::default().diagnostic_bytes as usize
                    && attempt.error.as_ref().is_some_and(|error| {
                        error.code == failure.error.code && error.message == failure.error.message
                    }),
                "PDF 宿主失败报告捏造完整页、原始确认或与真实错误不一致",
            )?;
            Vec::new()
        };
        tree.verify(control)?;
        Ok((tree, issues))
    }
}

fn valid_pdf_failure(error: &SourceFailure) -> bool {
    valid_failure(error)
        && valid_id(&error.code)
        && (error.code.starts_with("PDF_") || error.code.starts_with("SIDECAR_"))
        && error.message.len() <= 16 * 1024
}
fn valid_bbox(bbox: &[f64; 4]) -> bool {
    bbox.iter()
        .all(|number| number.is_finite() && *number >= 0.0)
        && bbox[2] > bbox[0]
        && bbox[3] > bbox[1]
}
fn validate_resource(
    resource: &PdfResource,
    id: &str,
    path: &str,
    kind: &str,
    media: &str,
    confidence: &str,
) -> Result<(), ProjectError> {
    require(
        resource.resource_id == id
            && resource.relative_path == path
            && resource.kind == kind
            && resource.media_type == media
            && resource.confidence == confidence,
        "PDF 资源固定身份、路径、媒体类型或确认状态不一致",
    )
}
pub(crate) fn parser_resource_identity(path: &str) -> Option<(String, &'static str, &'static str)> {
    match path {
        "parser/raw.pdf" => Some((pdf_model::RAW_ID.into(), "binary", "application/pdf")),
        "parser/pdf-manifest.json" => {
            Some((pdf_model::MANIFEST_ID.into(), "utf8", "application/json"))
        }
        _ => {
            let number = path.strip_prefix("parser/pages/")?.strip_suffix(".txt")?;
            if number.len() != 6 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            let index: usize = number.parse().ok()?;
            (index < pdf_model::MAX_PAGES)
                .then(|| (pdf_model::text_id(index), "utf8", "text/plain"))
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheOwner {
    schema_version: u32,
    project_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FailureDiagnostic {
    code: String,
    message: String,
    diagnostics: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FailureJson {
    schema_version: u32,
    report_kind: String,
    status: String,
    raw_acquisition: String,
    #[serde(deserialize_with = "pdf_model::nullable")]
    input_fingerprint: Option<Fingerprint>,
    #[serde(deserialize_with = "pdf_model::nullable")]
    page_count: Option<u64>,
    confirmed_pages: Vec<serde_json::Value>,
    error: FailureDiagnostic,
}

pub(crate) fn snapshot_error(error: SidecarError) -> ProjectError {
    if matches!(
        error.code,
        "SOURCE_SNAPSHOT_CONFLICT" | "SIDECAR_WORKSPACE" | "SIDECAR_RESOURCE"
    ) {
        snapshot_conflict(&error.to_string())
    } else {
        invalid_snapshot(&error.to_string())
    }
}
fn invalid_snapshot(message: &str) -> ProjectError {
    ProjectError::new(
        "SOURCE_INVALID_SNAPSHOT",
        2,
        format!("PDF 来源快照校验失败：{message}。原内容保留。"),
    )
}
fn snapshot_require(condition: bool, message: &str) -> Result<(), ProjectError> {
    if condition {
        Ok(())
    } else {
        Err(invalid_snapshot(message))
    }
}
fn parse_location(root: &Path, id: &str, staged: Option<(&str, &Path)>) -> PathBuf {
    staged
        .filter(|(staged_id, _)| *staged_id == id)
        .map_or_else(
            || root.join(".noetome-pdf-parses").join(id),
            |(_, path)| path.to_path_buf(),
        )
}
fn exact_names(path: &Path, expected: &BTreeSet<String>) -> Result<(), ProjectError> {
    let names = fs::read_dir(path)
        .map_err(|error| ProjectError::io(path, error))?
        .map(|entry| {
            entry
                .map_err(|error| ProjectError::io(path, error))
                .and_then(|entry| {
                    entry
                        .file_name()
                        .into_string()
                        .map_err(|_| snapshot_conflict("PDF 快照文件名不是 UTF-8"))
                })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if &names == expected {
        Ok(())
    } else {
        Err(snapshot_conflict("PDF 快照存在缺失或未知内容"))
    }
}

struct CacheGuards {
    root: Directory,
    cache: Option<Directory>,
    owner: Option<GuardedFile>,
    cache_path: PathBuf,
    expected: BTreeSet<String>,
    staging: Option<Directory>,
}
impl CacheGuards {
    fn capture(
        root: &Path,
        project_id: &str,
        attempts: &BTreeMap<String, PdfParse>,
        staged: Option<(&str, &Path)>,
        control: &Control,
    ) -> Result<Self, ProjectError> {
        control.check().map_err(snapshot_error)?;
        let root_guard =
            Directory::capture(root, "SOURCE_SNAPSHOT_CONFLICT").map_err(snapshot_error)?;
        let staging = if let Some((id, path)) = staged {
            require(attempts.contains_key(id), "暂存 PDF 解析不在本轮登记清单中")?;
            if path != root.join(".noetome-writing/parse") {
                return Err(snapshot_conflict(
                    "暂存 PDF 解析必须位于本项目写入目录的 parse",
                ));
            }
            Some(
                Directory::capture(&root.join(".noetome-writing"), "SOURCE_SNAPSHOT_CONFLICT")
                    .map_err(snapshot_error)?,
            )
        } else {
            None
        };
        let expected: BTreeSet<_> = attempts
            .keys()
            .filter(|id| staged.is_none_or(|(staged_id, _)| id.as_str() != staged_id))
            .cloned()
            .chain(std::iter::once(".owner.json".to_string()))
            .collect();
        let path = root.join(".noetome-pdf-parses");
        let (cache, owner) = match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if expected.len() != 1 {
                    return Err(snapshot_conflict("已登记 PDF 解析快照根缺失"));
                }
                (None, None)
            }
            Err(error) => return Err(ProjectError::io(&path, error)),
            Ok(_) => {
                if attempts.is_empty() {
                    return Err(snapshot_conflict("项目含有未登记的 PDF 快照根"));
                }
                let cache = Directory::capture(&path, "SOURCE_SNAPSHOT_CONFLICT")
                    .map_err(snapshot_error)?;
                exact_names(&path, &expected)?;
                let (owner, bytes) = GuardedFile::open(
                    &path.join(".owner.json"),
                    4096,
                    control,
                    "SOURCE_SNAPSHOT_CONFLICT",
                )
                .map_err(snapshot_error)?;
                let recorded: CacheOwner = serde_json::from_slice(&bytes)
                    .map_err(|_| snapshot_conflict("PDF 快照所有权文件无效或包含未知字段"))?;
                if recorded.schema_version != 1 || recorded.project_id != project_id {
                    return Err(snapshot_conflict("PDF 快照根不属于当前项目"));
                }
                (Some(cache), Some(owner))
            }
        };
        let guards = Self {
            root: root_guard,
            cache,
            owner,
            cache_path: path,
            expected,
            staging,
        };
        guards.verify(control)?;
        Ok(guards)
    }
    fn verify(&self, control: &Control) -> Result<(), ProjectError> {
        control.check().map_err(snapshot_error)?;
        self.root
            .verify("SOURCE_SNAPSHOT_CONFLICT")
            .map_err(snapshot_error)?;
        if let Some(staging) = &self.staging {
            staging
                .verify("SOURCE_SNAPSHOT_CONFLICT")
                .map_err(snapshot_error)?;
        }
        if let Some(cache) = &self.cache {
            cache
                .verify("SOURCE_SNAPSHOT_CONFLICT")
                .map_err(snapshot_error)?;
            exact_names(&self.cache_path, &self.expected)?;
            self.owner
                .as_ref()
                .ok_or_else(|| snapshot_conflict("PDF 快照所有权守卫缺失"))?
                .verify(control, "SOURCE_SNAPSHOT_CONFLICT")
                .map_err(snapshot_error)?;
        } else {
            match fs::symlink_metadata(&self.cache_path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(ProjectError::io(&self.cache_path, error)),
                Ok(_) => return Err(snapshot_conflict("PDF 校验期间出现未经确认的新快照根")),
            }
        }
        Ok(())
    }
}

pub(crate) struct RawSnapshotGuards {
    directory: Directory,
    file: GuardedFile,
    path: PathBuf,
}
impl RawSnapshotGuards {
    pub(crate) fn verify(&self, control: &Control) -> Result<(), ProjectError> {
        self.directory
            .verify("SOURCE_SNAPSHOT_CONFLICT")
            .map_err(snapshot_error)?;
        exact_names(&self.path, &BTreeSet::from(["raw.pdf".to_string()]))?;
        self.file
            .verify(control, "SOURCE_SNAPSHOT_CONFLICT")
            .map_err(snapshot_error)
    }
}
pub(crate) fn validate_raw_snapshot(
    version: &SourceVersion,
    location: &Path,
    control: &Control,
) -> Result<RawSnapshotGuards, ProjectError> {
    let directory =
        Directory::capture(location, "SOURCE_SNAPSHOT_CONFLICT").map_err(snapshot_error)?;
    exact_names(location, &BTreeSet::from(["raw.pdf".to_string()]))?;
    let (file, raw) = GuardedFile::open(
        &location.join("raw.pdf"),
        Limits::default().input_bytes,
        control,
        "SOURCE_SNAPSHOT_CONFLICT",
    )
    .map_err(snapshot_error)?;
    snapshot_require(
        raw.len() as u64 == version.byte_length
            && crate::text::sha256(&raw) == version.content_sha256,
        "PDF 原始快照字节、长度与 SourceVersion 不一致",
    )?;
    let guarded = RawSnapshotGuards {
        directory,
        file,
        path: location.to_path_buf(),
    };
    guarded.verify(control)?;
    Ok(guarded)
}

struct CheckedTree {
    location: PathBuf,
    directories: Vec<Directory>,
    files: Vec<GuardedFile>,
    expected_files: BTreeSet<String>,
    expected_dirs: BTreeSet<String>,
    bytes: BTreeMap<String, Vec<u8>>,
}
impl CheckedTree {
    fn open(
        location: &Path,
        resources: &BTreeMap<String, PdfResource>,
        control: &Control,
    ) -> Result<Self, ProjectError> {
        let expected_files: BTreeSet<_> = resources
            .values()
            .map(|resource| resource.relative_path.clone())
            .collect();
        let mut expected_dirs = BTreeSet::new();
        for path in &expected_files {
            let pieces: Vec<_> = path.split('/').collect();
            for length in 1..pieces.len() {
                expected_dirs.insert(pieces[..length].join("/"));
            }
        }
        if expected_files.len() != resources.len() || !expected_files.is_disjoint(&expected_dirs) {
            return Err(snapshot_conflict("PDF 资源存在重复路径或目录冲突"));
        }
        let mut directories =
            vec![Directory::capture(location, "SOURCE_SNAPSHOT_CONFLICT").map_err(snapshot_error)?];
        walk_tree(
            location,
            &expected_files,
            &expected_dirs,
            control,
            &mut directories,
        )?;
        let mut files = Vec::new();
        let mut bytes = BTreeMap::new();
        let mut total = 0u64;
        for resource in resources.values() {
            control.check().map_err(snapshot_error)?;
            let limit = if resource.resource_id == FAILURE_RESOURCE_ID {
                FAILURE_BYTES
            } else {
                REPORT_BYTES
            };
            let (guard, data) = GuardedFile::open(
                &location.join(&resource.relative_path),
                limit,
                control,
                "SOURCE_SNAPSHOT_CONFLICT",
            )
            .map_err(snapshot_error)?;
            total = total
                .checked_add(data.len() as u64)
                .ok_or_else(|| invalid_snapshot("PDF 资源实际字节溢出"))?;
            snapshot_require(
                total <= SNAPSHOT_BYTES
                    && resource.byte_length == data.len() as u64
                    && resource.sha256 == crate::text::sha256(&data),
                "PDF 资源实际 SHA、长度或总字节与 SQL 索引不一致",
            )?;
            if resource.kind == "utf8" && resource.confidence != "unconfirmed" {
                snapshot_require(
                    std::str::from_utf8(&data).is_ok(),
                    "已确认 PDF UTF-8 资源解码失败",
                )?;
            }
            // An interrupted .txt residue is merely intended UTF-8 output;
            // unconfirmed bytes are deliberately never called verified text.
            files.push(guard);
            bytes.insert(resource.resource_id.clone(), data);
        }
        let tree = Self {
            location: location.to_path_buf(),
            directories,
            files,
            expected_files,
            expected_dirs,
            bytes,
        };
        tree.verify(control)?;
        Ok(tree)
    }
    fn resource(&self, id: &str) -> Result<&[u8], ProjectError> {
        self.bytes
            .get(id)
            .map(Vec::as_slice)
            .ok_or_else(|| invalid_snapshot("PDF 结果引用未守卫的资源"))
    }
    fn verify(&self, control: &Control) -> Result<(), ProjectError> {
        for directory in &self.directories {
            directory
                .verify("SOURCE_SNAPSHOT_CONFLICT")
                .map_err(snapshot_error)?;
        }
        for file in &self.files {
            file.verify(control, "SOURCE_SNAPSHOT_CONFLICT")
                .map_err(snapshot_error)?;
        }
        let mut directories = Vec::new();
        walk_tree(
            &self.location,
            &self.expected_files,
            &self.expected_dirs,
            control,
            &mut directories,
        )
    }
}
fn walk_tree(
    root: &Path,
    expected_files: &BTreeSet<String>,
    expected_dirs: &BTreeSet<String>,
    control: &Control,
    guards: &mut Vec<Directory>,
) -> Result<(), ProjectError> {
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    let mut pending = vec![(root.to_path_buf(), String::new())];
    while let Some((path, prefix)) = pending.pop() {
        control.check().map_err(snapshot_error)?;
        for entry in fs::read_dir(&path).map_err(|error| ProjectError::io(&path, error))? {
            control.check().map_err(snapshot_error)?;
            let entry = entry.map_err(|error| ProjectError::io(&path, error))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| snapshot_conflict("PDF 资源名不是 UTF-8"))?;
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let meta = fs::symlink_metadata(entry.path())
                .map_err(|error| ProjectError::io(&entry.path(), error))?;
            if project::is_link(&meta) {
                return Err(snapshot_conflict("PDF 资源或目录是链接"));
            }
            if meta.is_dir() {
                if !expected_dirs.contains(&relative) || !directories.insert(relative.clone()) {
                    return Err(snapshot_conflict("PDF 存在未知、空或重复资源目录"));
                }
                guards.push(
                    Directory::capture(&entry.path(), "SOURCE_SNAPSHOT_CONFLICT")
                        .map_err(snapshot_error)?,
                );
                pending.push((entry.path(), relative));
            } else if meta.is_file() {
                if !expected_files.contains(&relative) || !files.insert(relative) {
                    return Err(snapshot_conflict("PDF 存在未知或重复资源文件"));
                }
            } else {
                return Err(snapshot_conflict("PDF 资源不是普通文件或目录"));
            }
        }
    }
    if &files != expected_files || &directories != expected_dirs {
        return Err(snapshot_conflict("PDF 文件或目录清单不完整"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/support/pdf_source_model_tests.rs"]
mod tests;
