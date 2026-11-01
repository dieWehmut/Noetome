//! Strict persisted PDF evidence and the relational source index.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::pdf_model::{deserialize_object, required_option};
use rusqlite::Connection;
use serde::{Deserialize, Deserializer, Serialize};

use crate::pdf_parse::{PdfLimits, PdfParseReport, PdfParseStatus, PdfPolicy, PdfRuntime};
use crate::project::ProjectError;
use crate::sidecar::{
    Artifact, ArtifactKind, ArtifactPolicy, InputFingerprint, Producer, ResponseStatus, UnitStatus,
};
use crate::source_model::{
    SourceFailure, Store, directory, exact_names, invalid_record, invalid_snapshot,
    read_single_snapshot, require, same_directory, snapshot_conflict, sql_failure, sql_u64,
    valid_failure, valid_hash, valid_selected_path, valid_time, valid_uuid,
};

macro_rules! named_deserialize {
    ($name:ident {$($(#[$attribute:meta])* $field:ident:$kind:ty),* $(,)?})=>{
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D:Deserializer<'de>>(deserializer:D)->Result<Self,D::Error>{
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $($(#[$attribute])* $field:$kind),* }
                let fields:Fields=deserialize_object(deserializer)?;
                Ok(Self{$($field:fields.$field),*})
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PdfCacheConfig {
    pub(crate) protocol_schema: u32,
    pub(crate) protocol: String,
    pub(crate) adapter_id: String,
    pub(crate) adapter_version: String,
    pub(crate) options_version: String,
    pub(crate) runtime: PdfRuntime,
    pub(crate) pdf_policy: PdfPolicy,
    pub(crate) artifact_policy: ArtifactPolicy,
}
named_deserialize!(PdfCacheConfig {
    protocol_schema: u32,
    protocol: String,
    adapter_id: String,
    adapter_version: String,
    options_version: String,
    runtime: PdfRuntime,
    pdf_policy: PdfPolicy,
    artifact_policy: ArtifactPolicy
});
impl PdfCacheConfig {
    pub(crate) fn current(limits: PdfLimits) -> Self {
        Self {
            protocol_schema: 2,
            protocol: crate::pdf_parse::PROTOCOL.into(),
            adapter_id: "pdfplumber-text".into(),
            adapter_version: "1.0.0".into(),
            options_version: crate::pdf_parse::OPTIONS_VERSION.into(),
            runtime: crate::pdf_parse::expected_runtime(),
            pdf_policy: limits.policy,
            artifact_policy: ArtifactPolicy {
                max_file_bytes: limits.transport.max_artifact_bytes,
                max_total_bytes: limits.transport.max_total_artifact_bytes,
                max_files: limits.transport.max_artifacts,
            },
        }
    }
    pub(crate) fn cache_key(&self, version_id: &str, raw_sha256: &str) -> String {
        #[derive(Serialize)]
        struct Key<'a> {
            version_id: &'a str,
            raw_sha256: &'a str,
            config: &'a PdfCacheConfig,
        }
        crate::text::sha256(
            &serde_json::to_vec(&Key {
                version_id,
                raw_sha256,
                config: self,
            })
            .expect("fixed cache key DTO serializes"),
        )
    }
    pub(crate) fn limits(&self) -> Result<PdfLimits, ProjectError> {
        let maximum = PdfLimits::default();
        require(
            self.protocol_schema == 2
                && self.protocol == crate::pdf_parse::PROTOCOL
                && self.adapter_id == "pdfplumber-text"
                && self.adapter_version == "1.0.0"
                && self.options_version == crate::pdf_parse::OPTIONS_VERSION
                && self.runtime == crate::pdf_parse::expected_runtime(),
            "PDF 解析配置的协议、解析器或运行时身份不受支持",
        )?;
        require(
            self.pdf_policy.max_pages <= maximum.policy.max_pages
                && self.pdf_policy.max_glyphs <= maximum.policy.max_glyphs
                && self.pdf_policy.max_regions <= maximum.policy.max_regions
                && self.pdf_policy.max_objects <= maximum.policy.max_objects
                && self.artifact_policy.max_file_bytes <= maximum.transport.max_artifact_bytes
                && self.artifact_policy.max_total_bytes
                    <= maximum.transport.max_total_artifact_bytes
                && self.artifact_policy.max_files <= maximum.transport.max_artifacts,
            "PDF 保存政策超出实现的读取上限",
        )?;
        let mut limits = maximum;
        limits.policy = self.pdf_policy;
        limits.transport.max_artifact_bytes = self.artifact_policy.max_file_bytes;
        limits.transport.max_total_artifact_bytes = self.artifact_policy.max_total_bytes;
        limits.transport.max_artifacts = self.artifact_policy.max_files;
        Ok(limits)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PdfEvidence {
    pub(crate) schema_version: u32,
    pub(crate) input: InputFingerprint,
    pub(crate) config: PdfCacheConfig,
    pub(crate) status: PdfParseStatus,
    pub(crate) exit_code: u8,
    pub(crate) host_report: Option<PdfParseReport>,
    pub(crate) failure: Option<SourceFailure>,
}
named_deserialize!(PdfEvidence {schema_version:u32,input:InputFingerprint,config:PdfCacheConfig,status:PdfParseStatus,exit_code:u8,#[serde(deserialize_with="required_option")] host_report:Option<PdfParseReport>,#[serde(deserialize_with="required_option")] failure:Option<SourceFailure>});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PdfStoredParse {
    pub(crate) parse_id: String,
    pub(crate) version_id: String,
    pub(crate) ordinal: u64,
    pub(crate) created_at: String,
    pub(crate) cache_key: String,
    pub(crate) config: PdfCacheConfig,
    pub(crate) status: String,
    pub(crate) page_count: Option<u32>,
    pub(crate) report_path: String,
    pub(crate) report_sha256: String,
    pub(crate) report_byte_length: u64,
    pub(crate) error: Option<SourceFailure>,
}
named_deserialize!(PdfStoredParse {parse_id:String,version_id:String,ordinal:u64,created_at:String,cache_key:String,config:PdfCacheConfig,status:String,#[serde(deserialize_with="required_option")] page_count:Option<u32>,report_path:String,report_sha256:String,report_byte_length:u64,#[serde(deserialize_with="required_option")] error:Option<SourceFailure>});

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PdfImportRecord {
    pub(crate) import_id: String,
    pub(crate) imported_at: String,
    pub(crate) selected_path: String,
    pub(crate) source_id: Option<String>,
    pub(crate) version_id: Option<String>,
    pub(crate) parse_id: Option<String>,
    pub(crate) raw_status: String,
    pub(crate) parse_status: String,
    pub(crate) reused_version: bool,
    pub(crate) reused_parse: bool,
    pub(crate) knowledge_status: String,
    pub(crate) exit_code: u8,
    pub(crate) error: Option<SourceFailure>,
}
named_deserialize!(PdfImportRecord {import_id:String,imported_at:String,selected_path:String,#[serde(deserialize_with="required_option")] source_id:Option<String>,#[serde(deserialize_with="required_option")] version_id:Option<String>,#[serde(deserialize_with="required_option")] parse_id:Option<String>,raw_status:String,parse_status:String,reused_version:bool,reused_parse:bool,knowledge_status:String,exit_code:u8,#[serde(deserialize_with="required_option")] error:Option<SourceFailure>});

#[derive(Debug, Clone, Default)]
pub(crate) struct PdfStore {
    pub(crate) parses: BTreeMap<String, PdfStoredParse>,
    pub(crate) imports: Vec<PdfImportRecord>,
    pub(crate) artifacts: BTreeMap<String, Vec<Artifact>>,
    pub(crate) pages: BTreeMap<String, Vec<crate::pdf_parse::PdfPage>>,
}
impl PdfStore {
    pub(crate) fn load(
        connection: &Connection,
        store: &Store,
        project_id: &str,
    ) -> Result<Self, ProjectError> {
        store.validate_records(project_id)?;
        if store.schema_version != 3 {
            return Ok(Self::default());
        }
        crate::pdf_source_schema::validate(connection)?;
        let mut result = Self::default();
        let mut query=connection.prepare("SELECT parse_id,version_id,ordinal,created_at,cache_key,config_json,status,page_count,report_path,report_sha256,report_byte_length,error_code,error_message FROM pdf_parses ORDER BY version_id,ordinal").map_err(invalid_record)?;
        let rows = query
            .query_map([], |row| {
                Ok(PdfStoredParse {
                    parse_id: row.get(0)?,
                    version_id: row.get(1)?,
                    ordinal: sql_u64(row, 2)?,
                    created_at: row.get(3)?,
                    cache_key: row.get(4)?,
                    config: sql_json(row, 5, 65536)?,
                    status: row.get(6)?,
                    page_count: sql_option_u32(row, 7)?,
                    report_path: row.get(8)?,
                    report_sha256: row.get(9)?,
                    report_byte_length: sql_u64(row, 10)?,
                    error: sql_failure(row, 11, 12)?,
                })
            })
            .map_err(invalid_record)?;
        for row in rows {
            let parse = row.map_err(invalid_record)?;
            result.artifacts.insert(parse.parse_id.clone(), Vec::new());
            result.pages.insert(parse.parse_id.clone(), Vec::new());
            require(
                result
                    .parses
                    .insert(parse.parse_id.clone(), parse)
                    .is_none(),
                "PDF 解析身份重复",
            )?;
        }
        let mut query=connection.prepare("SELECT parse_id,artifact_id,kind,relative_path,byte_length,sha256 FROM pdf_artifacts ORDER BY parse_id,artifact_id").map_err(invalid_record)?;
        let rows = query
            .query_map([], |row| {
                let kind: String = row.get(2)?;
                Ok((
                    row.get::<_, String>(0)?,
                    Artifact {
                        artifact_id: row.get(1)?,
                        kind: match kind.as_str() {
                            "binary" => ArtifactKind::Binary,
                            "utf8_text" => ArtifactKind::Utf8Text,
                            _ => return Err(rusqlite::Error::InvalidQuery),
                        },
                        relative_path: row.get(3)?,
                        byte_length: sql_u64(row, 4)?,
                        sha256: row.get(5)?,
                    },
                ))
            })
            .map_err(invalid_record)?;
        for row in rows {
            let (id, artifact) = row.map_err(invalid_record)?;
            result
                .artifacts
                .get_mut(&id)
                .ok_or_else(|| invalid_record("PDF 资源引用不存在的解析"))?
                .push(artifact);
        }
        let mut query=connection.prepare("SELECT parse_id,physical_page_index,printed_page_label,status,geometry_json,glyph_count,excluded_glyph_ordinals_json,text_artifact_id,text_artifact_sha256 FROM pdf_pages ORDER BY parse_id,physical_page_index").map_err(invalid_record)?;
        let rows = query
            .query_map([], |row| {
                let status: String = row.get(3)?;
                let reference = match (
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ) {
                    (None, None) => None,
                    (Some(artifact_id), Some(sha256)) => Some(crate::pdf_parse::ArtifactRef {
                        artifact_id,
                        sha256,
                    }),
                    _ => return Err(rusqlite::Error::InvalidQuery),
                };
                let geometry: Option<String> = row.get(4)?;
                Ok((
                    row.get::<_, String>(0)?,
                    crate::pdf_parse::PdfPage {
                        physical_page_index: sql_u32(row, 1)?,
                        printed_page_label: row.get(2)?,
                        status: unit_status(&status)?,
                        geometry: geometry
                            .map(|value| bounded_json(&value, 4096))
                            .transpose()?,
                        glyph_count: sql_option_u32(row, 5)?,
                        excluded_glyph_ordinals: sql_json(row, 6, JSON_LIST_LIMIT)?,
                        text_artifact: reference,
                        regions: Vec::new(),
                    },
                ))
            })
            .map_err(invalid_record)?;
        for row in rows {
            let (id, page) = row.map_err(invalid_record)?;
            result
                .pages
                .get_mut(&id)
                .ok_or_else(|| invalid_record("PDF 页面引用不存在的解析"))?
                .push(page);
        }
        let mut query=connection.prepare("SELECT parse_id,physical_page_index,region_id,ordinal,bbox_json,artifact_id,sha256,start_byte,end_byte,glyph_ordinals_json FROM pdf_regions ORDER BY parse_id,physical_page_index,ordinal").map_err(invalid_record)?;
        let rows = query
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    crate::pdf_parse::PdfRegion {
                        region_id: row.get(2)?,
                        ordinal: sql_u32(row, 3)?,
                        locator: crate::pdf_parse::PdfLocator {
                            kind: crate::pdf_parse::PdfLocatorKind::PdfPageRegion,
                            physical_page_index: sql_u32(row, 1)?,
                            bbox_pt: sql_json(row, 4, 4096)?,
                        },
                        content: crate::pdf_parse::PdfContent {
                            artifact_id: row.get(5)?,
                            sha256: row.get(6)?,
                            start_byte: sql_u64(row, 7)?,
                            end_byte: sql_u64(row, 8)?,
                        },
                        glyph_ordinals: sql_json(row, 9, JSON_LIST_LIMIT)?,
                    },
                ))
            })
            .map_err(invalid_record)?;
        for row in rows {
            let (id, region) = row.map_err(invalid_record)?;
            let pages = result
                .pages
                .get_mut(&id)
                .ok_or_else(|| invalid_record("PDF 区域引用不存在的解析"))?;
            pages
                .iter_mut()
                .find(|page| page.physical_page_index == region.locator.physical_page_index)
                .ok_or_else(|| invalid_record("PDF 区域引用不存在的物理页"))?
                .regions
                .push(region);
        }
        result.imports = load_import_records(connection)?;
        require(
            result.imports == store.pdf_imports,
            "PDF 导入索引与统一来源清单不一致",
        )?;
        result.validate_records(store, project_id)?;
        Ok(result)
    }

    fn validate_records(&self, store: &Store, project_id: &str) -> Result<(), ProjectError> {
        store.validate_records(project_id)?;
        require(
            self.imports == store.pdf_imports,
            "PDF 导入索引与统一来源清单不一致",
        )?;
        require(
            self.artifacts.len() == self.parses.len() && self.pages.len() == self.parses.len(),
            "PDF 关系清单与解析集合不一致",
        )?;
        let mut ordinals: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
        let mut ready_keys = BTreeSet::new();
        for (id, parse) in &self.parses {
            let version = store
                .versions
                .get(&parse.version_id)
                .ok_or_else(|| invalid_record("PDF 解析缺少原始版本"))?;
            let limits = parse.config.limits()?;
            require(
                store.schema_version == 3
                    && id == &parse.parse_id
                    && valid_uuid(id)
                    && version.media_type == "application/pdf"
                    && version.ingest_status == "acquired"
                    && valid_time(&parse.created_at)
                    && parse.cache_key
                        == parse
                            .config
                            .cache_key(&parse.version_id, &version.content_sha256)
                    && valid_hash(&parse.cache_key),
                "PDF 解析身份、原始版本、政策或缓存键无效",
            )?;
            require(
                parse.report_path == format!(".noetome-pdf-parses/{id}/report.json")
                    && valid_hash(&parse.report_sha256)
                    && parse.report_byte_length > 0
                    && parse.report_byte_length <= MAX_EVIDENCE_BYTES
                    && parse
                        .page_count
                        .is_none_or(|count| count <= limits.policy.max_pages),
                "PDF 报告路径、指纹、长度或页数无效",
            )?;
            match parse.status.as_str() {
                "ready" => {
                    require(
                        parse.error.is_none() && parse.page_count.is_some(),
                        "PDF ready 解析错误或页数不一致",
                    )?;
                    require(
                        ready_keys.insert((&parse.version_id, &parse.cache_key)),
                        "同一 PDF 缓存身份存在多个 ready 解析",
                    )?;
                }
                "needs_review" => require(
                    parse.page_count.is_some() && parse.error.as_ref().is_some_and(valid_failure),
                    "PDF 需复核解析缺少页数或真实错误",
                )?,
                "failed" => require(
                    parse.error.as_ref().is_some_and(valid_failure),
                    "PDF 失败解析缺少真实错误",
                )?,
                _ => return Err(invalid_record("PDF 解析状态无效")),
            }
            ordinals
                .entry(&parse.version_id)
                .or_default()
                .push(parse.ordinal);
            let artifacts = self
                .artifacts
                .get(id)
                .ok_or_else(|| invalid_record("PDF 解析缺少资源清单"))?;
            let pages = self
                .pages
                .get(id)
                .ok_or_else(|| invalid_record("PDF 解析缺少页面清单"))?;
            require(
                artifacts.len() as u64 <= u64::from(limits.transport.max_artifacts)
                    && pages.len() as u64 <= u64::from(limits.policy.max_pages),
                "PDF 关系数量超出保存政策",
            )?;
            let mut ids = BTreeSet::new();
            let mut bytes = 0u64;
            for artifact in artifacts {
                require(
                    valid_artifact_id(&artifact.artifact_id)
                        && ids.insert(&artifact.artifact_id)
                        && artifact.relative_path == artifact_path(artifact)
                        && valid_hash(&artifact.sha256)
                        && artifact.byte_length <= limits.transport.max_artifact_bytes,
                    "PDF 资源身份、固定路径、指纹或长度无效",
                )?;
                bytes = bytes
                    .checked_add(artifact.byte_length)
                    .ok_or_else(|| invalid_record("PDF 资源长度溢出"))?;
            }
            require(
                bytes <= limits.transport.max_total_artifact_bytes,
                "PDF 资源总长度超出保存政策",
            )?;
            if parse.page_count.is_none() {
                require(
                    pages.is_empty() && artifacts.is_empty(),
                    "未建立完整库存的 PDF 不允许虚构页面或资源",
                )?;
            } else {
                require(
                    pages.len() as u64 == u64::from(parse.page_count.unwrap()),
                    "PDF 完整物理页清单与页数不一致",
                )?;
            }
            let mut region_ids = BTreeSet::new();
            let mut total_regions = 0u64;
            let mut total_glyphs = 0u64;
            for (index, page) in pages.iter().enumerate() {
                require(
                    page.physical_page_index as usize == index
                        && page.excluded_glyph_ordinals.len() as u64
                            <= u64::from(limits.policy.max_glyphs)
                        && page
                            .glyph_count
                            .is_none_or(|count| count <= limits.policy.max_glyphs),
                    "PDF 页索引或字形计数超出保存政策",
                )?;
                if let Some(reference) = &page.text_artifact {
                    require(
                        artifacts.iter().any(|a| {
                            a.artifact_id == reference.artifact_id
                                && a.sha256 == reference.sha256
                                && a.kind == ArtifactKind::Utf8Text
                        }),
                        "PDF 页面正文引用不匹配本解析资源",
                    )?;
                }
                total_regions += page.regions.len() as u64;
                total_glyphs += u64::from(page.glyph_count.unwrap_or(0));
                for (ordinal, region) in page.regions.iter().enumerate() {
                    require(
                        valid_artifact_id(&region.region_id)
                            && region_ids.insert(&region.region_id)
                            && region.ordinal as usize == ordinal
                            && region.locator.physical_page_index == page.physical_page_index
                            && region.locator.bbox_pt.iter().all(|value| value.is_finite())
                            && region.glyph_ordinals.len() as u64
                                <= u64::from(limits.policy.max_glyphs),
                        "PDF 区域身份、页引用、坐标或字形清单无效",
                    )?;
                    require(
                        artifacts.iter().any(|a| {
                            a.artifact_id == region.content.artifact_id
                                && a.sha256 == region.content.sha256
                                && a.kind == ArtifactKind::Utf8Text
                                && region.content.start_byte < region.content.end_byte
                                && region.content.end_byte <= a.byte_length
                        }),
                        "PDF 区域正文引用或字节区间无效",
                    )?;
                }
            }
            require(
                total_regions <= u64::from(limits.policy.max_regions)
                    && total_glyphs <= u64::from(limits.policy.max_glyphs),
                "PDF 全文区域或字形数量超出政策",
            )?;
        }
        for values in ordinals.values_mut() {
            values.sort_unstable();
            require(
                values
                    .iter()
                    .enumerate()
                    .all(|(index, value)| *value == index as u64),
                "PDF 解析 ordinal 必须按原始版本从 0 连续递增",
            )?;
        }
        let mut seen = BTreeSet::new();
        let mut next: BTreeMap<&str, u64> = BTreeMap::new();
        for import in &self.imports {
            require(
                valid_uuid(&import.import_id)
                    && valid_time(&import.imported_at)
                    && valid_selected_path(&import.selected_path)
                    && import.knowledge_status == "pending"
                    && import.exit_code <= 3,
                "PDF 导入身份、路径、时间或退出码无效",
            )?;
            if let Some(id) = &import.parse_id {
                let parse = self
                    .parses
                    .get(id)
                    .ok_or_else(|| invalid_record("PDF 导入引用不存在的解析"))?;
                require(
                    import.version_id.as_ref() == Some(&parse.version_id)
                        && import.raw_status == "acquired"
                        && import.parse_status == parse.status
                        && import.error == parse.error
                        && ((parse.status == "ready" && import.exit_code == 0)
                            || (parse.status != "ready" && import.exit_code > 0)),
                    "PDF 导入与解析版本、状态、错误或退出码不一致",
                )?;
                require(
                    import.reused_parse == seen.contains(id),
                    "PDF 解析复用标记没有对应此前的解析事件",
                )?;
                if import.reused_parse {
                    require(
                        parse.status == "ready" && import.reused_version,
                        "只能复用更早 ready 解析及已获取版本",
                    )?;
                } else {
                    let ordinal = next.entry(&parse.version_id).or_default();
                    require(
                        parse.ordinal == *ordinal,
                        "PDF 首次解析导入必须与 ordinal 追加顺序一致",
                    )?;
                    *ordinal += 1;
                    seen.insert(id.clone());
                }
            } else {
                require(
                    import.version_id.is_none()
                        && import.parse_status == "not_run"
                        && !import.reused_parse,
                    "PDF 未执行解析记录不能附加版本或复用",
                )?;
            }
        }
        require(
            seen.len() == self.parses.len(),
            "每个 PDF 解析必须至少被一次真实导入引用",
        )?;
        Ok(())
    }

    pub(crate) fn validate_snapshots(
        &self,
        root: &Path,
        store: &Store,
        project_id: &str,
        staged_parse: Option<(&str, &Path)>,
        staged_raw: Option<(&str, &Path)>,
    ) -> Result<BTreeMap<String, PdfEvidence>, ProjectError> {
        self.validate_records(store, project_id)?;
        let root = crate::project::check_components(root)?;
        let root_identity = directory(&root)?;
        if let Some((id, path)) = staged_parse {
            require(self.parses.contains_key(id), "PDF 暂存解析不在本轮关系清单")?;
            if path != root.join(".noetome-writing").join("parse") {
                return Err(snapshot_conflict("PDF 暂存解析必须位于本项目固定写入目录"));
            }
        }
        store.validate_snapshots(&root, project_id, staged_raw)?;
        let cache = root.join(".noetome-pdf-parses");
        let expected: BTreeSet<OsString> = self
            .parses
            .keys()
            .filter(|id| staged_parse.is_none_or(|(staged, _)| id.as_str() != staged))
            .map(OsString::from)
            .chain(std::iter::once(OsString::from(".owner.json")))
            .collect();
        match fs::symlink_metadata(&cache) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if expected.len() != 1 {
                    return Err(snapshot_conflict("已登记 PDF 解析根目录缺失"));
                }
            }
            Err(error) => return Err(ProjectError::io(&cache, error)),
            Ok(_) => {
                if store.schema_version != 3 {
                    return Err(snapshot_conflict("旧项目包含未知 PDF 快照根"));
                }
                let identity = directory(&cache)?;
                exact_names(&cache, &expected)?;
                let bytes = read_single_snapshot(&cache.join(".owner.json"), 4096, None)?;
                let owner: PdfCacheOwner = serde_json::from_slice(&bytes)
                    .map_err(|_| snapshot_conflict("PDF 所有权 JSON 不是严格命名对象"))?;
                require(
                    owner.schema_version == 1 && owner.project_id == project_id,
                    "PDF 快照根不属于当前项目",
                )?;
                same_directory(&cache, &identity)?;
            }
        }
        let mut evidence = BTreeMap::new();
        for (id, parse) in &self.parses {
            let location = staged_parse
                .filter(|(staged, _)| *staged == id)
                .map_or_else(|| cache.join(id), |(_, path)| path.to_path_buf());
            let identity = directory(&location)?;
            exact_names(
                &location,
                &BTreeSet::from([OsString::from("report.json"), OsString::from("artifacts")]),
            )?;
            let bytes = read_single_snapshot(
                &location.join("report.json"),
                MAX_EVIDENCE_BYTES,
                Some(parse.report_byte_length),
            )?;
            if crate::text::sha256(&bytes) != parse.report_sha256 {
                return Err(invalid_snapshot("PDF 报告字节与保存指纹不一致"));
            }
            let saved: PdfEvidence = serde_json::from_slice(&bytes)
                .map_err(|_| invalid_snapshot("PDF 证据不是严格完整命名对象 JSON"))?;
            let resources = location.join("artifacts");
            let resources_identity = directory(&resources)?;
            let artifacts = &self.artifacts[id];
            exact_names(
                &resources,
                &artifacts
                    .iter()
                    .map(|a| {
                        OsString::from(format!("{}.{}", a.artifact_id, artifact_extension(a.kind)))
                    })
                    .collect(),
            )?;
            let mut actual = BTreeMap::new();
            for artifact in artifacts {
                let bytes = read_single_snapshot(
                    &location.join(artifact_path(artifact)),
                    parse.config.artifact_policy.max_file_bytes,
                    Some(artifact.byte_length),
                )?;
                if crate::text::sha256(&bytes) != artifact.sha256 {
                    return Err(invalid_snapshot("PDF 资源实际字节不匹配关系指纹"));
                }
                actual.insert(artifact.relative_path.clone(), bytes);
            }
            let version = &store.versions[&parse.version_id];
            let raw_location = staged_raw
                .filter(|(version_id, _)| *version_id == version.version_id)
                .map_or_else(
                    || root.join(&version.raw_snapshot),
                    |(_, path)| path.join("raw.bin"),
                );
            let raw = read_single_snapshot(
                &raw_location,
                PdfLimits::default().transport.max_input_bytes,
                Some(version.byte_length),
            )?;
            validate_evidence(
                parse,
                &saved,
                version,
                artifacts,
                &self.pages[id],
                &actual,
                &raw,
            )?;
            require(
                self.imports
                    .iter()
                    .filter(|import| import.parse_id.as_deref() == Some(id))
                    .all(|import| {
                        import.parse_status == status_name(saved.status)
                            && import.exit_code == saved.exit_code
                            && import.error == saved.failure
                    }),
                "PDF 导入状态、退出码或错误必须精确匹配实际保存证据",
            )?;
            same_directory(&resources, &resources_identity)?;
            same_directory(&location, &identity)?;
            evidence.insert(id.clone(), saved);
        }
        same_directory(&root, &root_identity)?;
        Ok(evidence)
    }
}
pub(crate) fn load_import_records(
    connection: &Connection,
) -> Result<Vec<PdfImportRecord>, ProjectError> {
    let mut query=connection.prepare("SELECT import_id,imported_at,selected_path,source_id,version_id,parse_id,raw_status,parse_status,reused_version,reused_parse,knowledge_status,exit_code,error_code,error_message FROM pdf_imports ORDER BY rowid").map_err(invalid_record)?;
    query
        .query_map([], |row| {
            Ok(PdfImportRecord {
                import_id: row.get(0)?,
                imported_at: row.get(1)?,
                selected_path: row.get(2)?,
                source_id: row.get(3)?,
                version_id: row.get(4)?,
                parse_id: row.get(5)?,
                raw_status: row.get(6)?,
                parse_status: row.get(7)?,
                reused_version: sql_bool(row, 8)?,
                reused_parse: sql_bool(row, 9)?,
                knowledge_status: row.get(10)?,
                exit_code: u8::try_from(row.get::<_, i64>(11)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                error: sql_failure(row, 12, 13)?,
            })
        })
        .map_err(invalid_record)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(invalid_record)
}

pub(crate) fn insert_parse(
    connection: &Connection,
    parse: &PdfStoredParse,
    evidence: &PdfEvidence,
) -> Result<(), ProjectError> {
    require(
        parse.status == status_name(evidence.status)
            && parse.config == evidence.config
            && parse.error == evidence.failure
            && parse.page_count
                == evidence.host_report.as_ref().and_then(|host| {
                    host.response
                        .as_ref()
                        .and_then(|response| response.page_count)
                }),
        "PDF 新解析行与证据状态不一致",
    )?;
    connection.execute("INSERT INTO pdf_parses(parse_id,version_id,ordinal,created_at,cache_key,config_json,status,page_count,report_path,report_sha256,report_byte_length,error_code,error_message) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",rusqlite::params![parse.parse_id,parse.version_id,sql_integer(parse.ordinal)?,parse.created_at,parse.cache_key,to_json(&parse.config)?,parse.status,parse.page_count,parse.report_path,parse.report_sha256,sql_integer(parse.report_byte_length)?,parse.error.as_ref().map(|error|error.code.as_str()),parse.error.as_ref().map(|error|error.message.as_str())]).map_err(invalid_record)?;
    if let Some(response) = evidence
        .host_report
        .as_ref()
        .and_then(|host| host.response.as_ref())
    {
        for artifact in &response.artifacts {
            connection
                .execute(
                    "INSERT INTO pdf_artifacts VALUES(?1,?2,?3,?4,?5,?6)",
                    rusqlite::params![
                        parse.parse_id,
                        artifact.artifact_id,
                        match artifact.kind {
                            ArtifactKind::Binary => "binary",
                            ArtifactKind::Utf8Text => "utf8_text",
                        },
                        artifact.relative_path,
                        sql_integer(artifact.byte_length)?,
                        artifact.sha256
                    ],
                )
                .map_err(invalid_record)?;
        }
        for page in &response.pages {
            connection
                .execute(
                    "INSERT INTO pdf_pages VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    rusqlite::params![
                        parse.parse_id,
                        page.physical_page_index,
                        page.printed_page_label,
                        unit_name(page.status),
                        page.geometry.as_ref().map(to_json).transpose()?,
                        page.glyph_count,
                        to_json(&page.excluded_glyph_ordinals)?,
                        page.text_artifact
                            .as_ref()
                            .map(|artifact| artifact.artifact_id.as_str()),
                        page.text_artifact
                            .as_ref()
                            .map(|artifact| artifact.sha256.as_str())
                    ],
                )
                .map_err(invalid_record)?;
            for region in &page.regions {
                connection
                    .execute(
                        "INSERT INTO pdf_regions VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                        rusqlite::params![
                            parse.parse_id,
                            page.physical_page_index,
                            region.region_id,
                            region.ordinal,
                            to_json(&region.locator.bbox_pt)?,
                            region.content.artifact_id,
                            region.content.sha256,
                            sql_integer(region.content.start_byte)?,
                            sql_integer(region.content.end_byte)?,
                            to_json(&region.glyph_ordinals)?
                        ],
                    )
                    .map_err(invalid_record)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn insert_import(
    connection: &Connection,
    import: &PdfImportRecord,
) -> Result<(), ProjectError> {
    connection
        .execute(
            "INSERT INTO pdf_imports VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            rusqlite::params![
                import.import_id,
                import.imported_at,
                import.selected_path,
                import.source_id,
                import.version_id,
                import.parse_id,
                import.raw_status,
                import.parse_status,
                i64::from(import.reused_version),
                i64::from(import.reused_parse),
                import.knowledge_status,
                import.exit_code,
                import.error.as_ref().map(|error| error.code.as_str()),
                import.error.as_ref().map(|error| error.message.as_str())
            ],
        )
        .map_err(invalid_record)?;
    Ok(())
}

fn validate_evidence(
    parse: &PdfStoredParse,
    evidence: &PdfEvidence,
    version: &crate::source_model::SourceVersion,
    artifacts: &[Artifact],
    pages: &[crate::pdf_parse::PdfPage],
    actual: &BTreeMap<String, Vec<u8>>,
    raw: &[u8],
) -> Result<(), ProjectError> {
    let input = InputFingerprint {
        relative_path: "input/raw.bin".into(),
        byte_length: version.byte_length,
        sha256: version.content_sha256.clone(),
    };
    require(
        evidence.schema_version == 1
            && evidence.input == input
            && evidence.config == parse.config
            && status_name(evidence.status) == parse.status
            && evidence.failure == parse.error
            && evidence.exit_code <= 3
            && raw.len() as u64 == version.byte_length
            && crate::text::sha256(raw) == version.content_sha256,
        "PDF 证据与原始实际字节、解析身份、状态或错误不一致",
    )?;
    require(
        (evidence.status == PdfParseStatus::Ready
            && evidence.exit_code == 0
            && evidence.failure.is_none())
            || (evidence.status != PdfParseStatus::Ready
                && evidence.exit_code > 0
                && evidence.failure.as_ref().is_some_and(valid_failure)),
        "PDF 证据状态、退出码或失败事实不一致",
    )?;
    let limits = parse.config.limits()?;
    let Some(host) = &evidence.host_report else {
        require(
            evidence.status == PdfParseStatus::Failed
                && parse.page_count.is_none()
                && pages.is_empty()
                && artifacts.is_empty()
                && actual.is_empty(),
            "未启动或未产生 Host 报告的失败证据不能虚构页面或资源",
        )?;
        require(
            evidence.failure.as_ref().is_some_and(|failure| {
                evidence.exit_code == failure_exit_code(&failure.code, None)
            }),
            "PDF 前置失败退出码必须精确匹配保存的错误分类",
        )?;
        return Ok(());
    };
    require(
        to_json(host)?.len() <= 8 * 1024 * 1024
            && host.schema_version == 1
            && host.producer == Producer::Parser
            && host.operation == "source_parse_pdf"
            && host.status == evidence.status
            && host.input == input
            && host.report_path == "parse.json"
            && host.diagnostics.len() <= limits.transport.max_stderr_bytes
            && host.violations.len() as u64 <= u64::from(limits.transport.max_issues),
        "PDF Host 报告身份、状态、长度或诊断上限无效",
    )?;
    let request = &host.request;
    require(
        request.schema_version == parse.config.protocol_schema
            && request.protocol == parse.config.protocol
            && valid_uuid(&request.request_id)
            && request.operation == crate::pdf_parse::PdfOperation::ParsePdf
            && request.input == input
            && request.workspace.artifact_directory == "artifacts"
            && request.artifact_policy == parse.config.artifact_policy
            && request.pdf_policy == parse.config.pdf_policy
            && request.options_version == parse.config.options_version,
        "PDF Host 请求必须精确绑定原始版本及保存政策",
    )?;
    let host_failure = host.failure.as_ref().map(|failure| SourceFailure {
        code: failure.code.clone(),
        message: failure.message.clone(),
    });
    require(
        host_failure == evidence.failure,
        "PDF Host 与持久证据的真实失败信息不一致",
    )?;
    if let Some(response) = &host.response {
        require(
            host.validation_status == "passed"
                && host.native_exit_code == Some(0)
                && host.pid.is_some_and(|pid| pid > 0)
                && parse.page_count == response.page_count
                && pages == response.pages
                && artifacts.len() == response.artifacts.len()
                && artifacts
                    .iter()
                    .all(|artifact| response.artifacts.iter().any(|saved| saved == artifact)),
            "PDF 已确认 Host 响应的页面、资源、native 退出或页数与关系表不一致",
        )?;
        let recomputed = crate::pdf_parse::validate_pdf(
            request,
            response,
            actual,
            &limits,
            &AtomicBool::new(false),
        );
        require(
            recomputed
                .iter()
                .all(|violation| violation.rule_id == "PDF_REPORTED_INCOMPLETE")
                && recomputed == host.violations,
            "PDF 资源、字形、正文、坐标、页状态或重算违规清单不一致",
        )?;
        let status = match response.status {
            ResponseStatus::Complete => PdfParseStatus::Ready,
            ResponseStatus::Partial => PdfParseStatus::NeedsReview,
            ResponseStatus::Failed => PdfParseStatus::Failed,
        };
        require(
            status == host.status
                && ((status == PdfParseStatus::Ready
                    && host.failure.is_none()
                    && evidence.exit_code == 0)
                    || (status != PdfParseStatus::Ready
                        && evidence.exit_code == 3
                        && host.failure.as_ref().is_some_and(|failure| {
                            failure.code
                                == response
                                    .issues
                                    .first()
                                    .map_or("PDF_PARSE_INCOMPLETE", |issue| issue.code.as_str())
                        }))),
            "PDF 整体处理状态、退出码或失败范围不匹配已确认响应",
        )?;
        let total = response
            .artifacts
            .iter()
            .try_fold(0u64, |total, artifact| {
                total.checked_add(artifact.byte_length)
            })
            .ok_or_else(|| invalid_record("PDF Host 资源总长度溢出"))?;
        require(
            host.verified_artifact_count as usize == response.artifacts.len()
                && host.verified_artifact_bytes == total
                && host.page_display.len() == response.pages.len()
                && host
                    .page_display
                    .iter()
                    .zip(&response.pages)
                    .all(|(display, page)| {
                        display.physical_page_idx == page.physical_page_index
                            && display.displayed_physical_page == page.physical_page_index + 1
                            && display.printed_page_label == page.printed_page_label
                            && display.status == page.status
                    }),
            "PDF Host 资源计数或显示物理页不匹配实际响应",
        )?;
    } else {
        require(
            host.status == PdfParseStatus::Failed
                && matches!(host.validation_status.as_str(), "failed" | "not_run")
                && host.failure.is_some()
                && parse.page_count.is_none()
                && pages.is_empty()
                && artifacts.is_empty()
                && actual.is_empty()
                && host.page_display.is_empty()
                && host.verified_artifact_count == 0
                && host.verified_artifact_bytes == 0,
            "无已确认响应的 Host 失败报告不能附加虚构页面或资源",
        )?;
        let failure = host.failure.as_ref().unwrap();
        let expected = failure_exit_code(&failure.code, host.native_exit_code);
        require(
            evidence.exit_code == expected,
            "PDF 实际失败退出码与 native/runtime 分类不一致",
        )?;
    }
    Ok(())
}
fn failure_exit_code(code: &str, native: Option<u32>) -> u8 {
    match code {
        "SIDECAR_EXIT_NONZERO" => match native {
            Some(1) => 1,
            Some(2) => 2,
            _ => 3,
        },
        "PDF_RUNTIME_INVALID"
        | "PDF_ADAPTER_INVALID"
        | "PDF_TYPE_UNSUPPORTED"
        | "PDF_LIMITS_INVALID"
        | "SIDECAR_UNSAFE_PATH"
        | "SIDECAR_LIMITS_INVALID"
        | "SIDECAR_REQUEST_LIMIT"
        | "SIDECAR_PROTOCOL_INVALID"
        | "SIDECAR_RESOURCE_INVALID" => 2,
        "PDF_REPORT_ERROR" => 1,
        _ => 3,
    }
}

const MAX_EVIDENCE_BYTES: u64 = 16 * 1024 * 1024;
const JSON_LIST_LIMIT: usize = 512 * 1024;
struct PdfCacheOwner {
    schema_version: u32,
    project_id: String,
}
named_deserialize!(PdfCacheOwner {
    schema_version: u32,
    project_id: String
});
fn status_name(status: PdfParseStatus) -> &'static str {
    match status {
        PdfParseStatus::Ready => "ready",
        PdfParseStatus::NeedsReview => "needs_review",
        PdfParseStatus::Failed => "failed",
    }
}
fn unit_name(status: UnitStatus) -> &'static str {
    match status {
        UnitStatus::Parsed => "parsed",
        UnitStatus::NeedsReview => "needs_review",
        UnitStatus::Failed => "failed",
    }
}
fn unit_status(status: &str) -> rusqlite::Result<UnitStatus> {
    match status {
        "parsed" => Ok(UnitStatus::Parsed),
        "needs_review" => Ok(UnitStatus::NeedsReview),
        "failed" => Ok(UnitStatus::Failed),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}
fn artifact_extension(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Binary => "bin",
        ArtifactKind::Utf8Text => "txt",
    }
}
fn artifact_path(artifact: &Artifact) -> String {
    format!(
        "artifacts/{}.{}",
        artifact.artifact_id,
        artifact_extension(artifact.kind)
    )
}
fn valid_artifact_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
fn sql_bool(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<bool> {
    match row.get::<_, i64>(index)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}
fn sql_u32(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u32> {
    u32::try_from(row.get::<_, i64>(index)?).map_err(|_| rusqlite::Error::InvalidQuery)
}
fn sql_option_u32(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<u32>> {
    row.get::<_, Option<i64>>(index)?
        .map(|value| u32::try_from(value).map_err(|_| rusqlite::Error::InvalidQuery))
        .transpose()
}
fn bounded_json<T: serde::de::DeserializeOwned>(value: &str, limit: usize) -> rusqlite::Result<T> {
    if value.len() > limit {
        return Err(rusqlite::Error::InvalidQuery);
    }
    serde_json::from_str(value).map_err(|_| rusqlite::Error::InvalidQuery)
}
fn sql_json<T: serde::de::DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    index: usize,
    limit: usize,
) -> rusqlite::Result<T> {
    bounded_json(&row.get::<_, String>(index)?, limit)
}
fn to_json<T: Serialize>(value: &T) -> Result<String, ProjectError> {
    serde_json::to_string(value).map_err(invalid_record)
}
fn sql_integer(value: u64) -> Result<i64, ProjectError> {
    i64::try_from(value).map_err(invalid_record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_parse::*;
    use crate::sidecar::WorkspacePolicy;
    use serde_json::json;
    const PROJECT: &str = "92175542-40d5-47e4-a62b-31209ee9fe52";
    const SOURCE: &str = "32175542-40d5-47e4-a62b-31209ee9fe52";
    const VERSION: &str = "42175542-40d5-47e4-a62b-31209ee9fe52";
    const PARSE: &str = "52175542-40d5-47e4-a62b-31209ee9fe52";
    const IMPORT: &str = "62175542-40d5-47e4-a62b-31209ee9fe52";
    const TIME: &str = "2026-11-01T00:00:00Z";
    const RAW: &[u8] = b"unit primitive input, not an author PDF";
    struct Sandbox(std::path::PathBuf);
    impl Sandbox {
        fn new() -> Self {
            let parent =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pdf-source-model-tests");
            fs::create_dir_all(&parent).unwrap();
            let root = parent
                .canonicalize()
                .unwrap()
                .join(uuid::Uuid::new_v4().to_string());
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    // A typed primitive fixture checks persistence/Core boundaries. It does not
    // prove extraction from a real PDF; independent CLI tests own that evidence.
    fn primitive_evidence() -> (PdfEvidence, BTreeMap<String, Vec<u8>>) {
        let config = PdfCacheConfig::current(PdfLimits::default());
        let input = InputFingerprint {
            relative_path: "input/raw.bin".into(),
            byte_length: RAW.len() as u64,
            sha256: crate::text::sha256(RAW),
        };
        let request = PdfRequest {
            schema_version: 2,
            protocol: PROTOCOL.into(),
            request_id: PARSE.into(),
            operation: PdfOperation::ParsePdf,
            input: input.clone(),
            workspace: WorkspacePolicy {
                artifact_directory: "artifacts".into(),
            },
            artifact_policy: config.artifact_policy.clone(),
            pdf_policy: config.pdf_policy,
            options_version: OPTIONS_VERSION.into(),
        };
        let inventory = PdfInventory {
            schema_version: 1,
            input: input.clone(),
            page_count: 1,
            pages: vec![PdfSourcePage {
                physical_page_index: 0,
                geometry: Some(PdfSourceGeometry {
                    media_box: [0., 0., 100., 100.],
                    crop_box: [0., 0., 100., 100.],
                    rotation: 0,
                    user_unit: 1.,
                }),
                glyphs: Some(vec![PdfGlyph {
                    ordinal: 0,
                    text: "A".into(),
                    media_bbox: [10., 10., 20., 20.],
                    upright: true,
                    font_size: 10.,
                    font_name: "Courier".into(),
                }]),
                visual_objects: Some(PdfVisualObjects {
                    images: 0,
                    lines: 0,
                    rectangles: 0,
                    curves: 0,
                    annotations: 0,
                }),
                decode_error: None,
            }],
        };
        let inventory_bytes = serde_json::to_vec(&inventory).unwrap();
        let text = b"A\n".to_vec();
        let inventory_sha = crate::text::sha256(&inventory_bytes);
        let text_sha = crate::text::sha256(&text);
        let page = PdfPage {
            physical_page_index: 0,
            printed_page_label: None,
            status: UnitStatus::Parsed,
            geometry: Some(PdfPageGeometry {
                media_box: [0., 0., 100., 100.],
                crop_box: [0., 0., 100., 100.],
                rotation: 0,
                user_unit: 1.,
                display_width_pt: 100.,
                display_height_pt: 100.,
            }),
            glyph_count: Some(1),
            excluded_glyph_ordinals: vec![],
            text_artifact: Some(ArtifactRef {
                artifact_id: "P0_text".into(),
                sha256: text_sha.clone(),
            }),
            regions: vec![PdfRegion {
                region_id: "P0_R0".into(),
                ordinal: 0,
                locator: PdfLocator {
                    kind: PdfLocatorKind::PdfPageRegion,
                    physical_page_index: 0,
                    bbox_pt: [10., 10., 20., 20.],
                },
                content: PdfContent {
                    artifact_id: "P0_text".into(),
                    sha256: text_sha.clone(),
                    start_byte: 0,
                    end_byte: 1,
                },
                glyph_ordinals: vec![0],
            }],
        };
        let response = PdfResponse {
            schema_version: 2,
            protocol: PROTOCOL.into(),
            request_id: PARSE.into(),
            adapter: PdfAdapter {
                producer: Producer::Parser,
                adapter_id: config.adapter_id.clone(),
                adapter_version: config.adapter_version.clone(),
                options_version: config.options_version.clone(),
                runtime: config.runtime.clone(),
            },
            input: input.clone(),
            status: ResponseStatus::Complete,
            page_count: Some(1),
            inventory: Some(ArtifactRef {
                artifact_id: "pdf_inventory".into(),
                sha256: inventory_sha.clone(),
            }),
            pages: vec![page],
            artifacts: vec![
                Artifact {
                    artifact_id: "pdf_inventory".into(),
                    kind: ArtifactKind::Binary,
                    relative_path: "artifacts/pdf_inventory.bin".into(),
                    byte_length: inventory_bytes.len() as u64,
                    sha256: inventory_sha,
                },
                Artifact {
                    artifact_id: "P0_text".into(),
                    kind: ArtifactKind::Utf8Text,
                    relative_path: "artifacts/P0_text.txt".into(),
                    byte_length: text.len() as u64,
                    sha256: text_sha,
                },
            ],
            issues: vec![],
        };
        let bytes = BTreeMap::from([
            ("artifacts/pdf_inventory.bin".into(), inventory_bytes),
            ("artifacts/P0_text.txt".into(), text),
        ]);
        assert!(
            validate_pdf(
                &request,
                &response,
                &bytes,
                &PdfLimits::default(),
                &AtomicBool::new(false)
            )
            .is_empty()
        );
        let host = PdfParseReport {
            schema_version: 1,
            producer: Producer::Parser,
            operation: "source_parse_pdf".into(),
            status: PdfParseStatus::Ready,
            input: input.clone(),
            request,
            response: Some(response.clone()),
            page_display: vec![PdfPageDisplay {
                physical_page_idx: 0,
                displayed_physical_page: 1,
                printed_page_label: None,
                status: UnitStatus::Parsed,
            }],
            validation_status: "passed".into(),
            failure: None,
            violations: vec![],
            diagnostics: vec![],
            pid: Some(1234),
            native_exit_code: Some(0),
            processing_elapsed_ms: 1,
            verified_artifact_count: 2,
            verified_artifact_bytes: response
                .artifacts
                .iter()
                .map(|artifact| artifact.byte_length)
                .sum(),
            report_path: "parse.json".into(),
        };
        (
            PdfEvidence {
                schema_version: 1,
                input,
                config,
                status: PdfParseStatus::Ready,
                exit_code: 0,
                host_report: Some(host),
                failure: None,
            },
            bytes,
        )
    }
    fn fixture(root: &Path) -> (Connection, PdfEvidence) {
        let (evidence, resources) = primitive_evidence();
        let report = serde_json::to_vec_pretty(&evidence).unwrap();
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(crate::pdf_source_schema::SCHEMA)
            .unwrap();
        connection
            .execute(
                "INSERT INTO project_metadata VALUES(1,?1,3,?2,'initialized')",
                rusqlite::params![PROJECT, TIME],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO sources VALUES(?1,?2)",
                rusqlite::params![SOURCE, TIME],
            )
            .unwrap();
        connection.execute("INSERT INTO source_versions VALUES(?1,?2,1,?3,'application/pdf','primitive.pdf',?4,?5,NULL,NULL,'acquired',?6,NULL,NULL,NULL)",rusqlite::params![VERSION,SOURCE,TIME,evidence.input.sha256,RAW.len() as i64,format!(".noetome-sources/{VERSION}/raw.bin")]).unwrap();
        let parse = PdfStoredParse {
            parse_id: PARSE.into(),
            version_id: VERSION.into(),
            ordinal: 0,
            created_at: TIME.into(),
            cache_key: evidence.config.cache_key(VERSION, &evidence.input.sha256),
            config: evidence.config.clone(),
            status: "ready".into(),
            page_count: Some(1),
            report_path: format!(".noetome-pdf-parses/{PARSE}/report.json"),
            report_sha256: crate::text::sha256(&report),
            report_byte_length: report.len() as u64,
            error: None,
        };
        insert_parse(&connection, &parse, &evidence).unwrap();
        insert_import(
            &connection,
            &PdfImportRecord {
                import_id: IMPORT.into(),
                imported_at: TIME.into(),
                selected_path: "D:/selected/primitive.pdf".into(),
                source_id: Some(SOURCE.into()),
                version_id: Some(VERSION.into()),
                parse_id: Some(PARSE.into()),
                raw_status: "acquired".into(),
                parse_status: "ready".into(),
                reused_version: false,
                reused_parse: false,
                knowledge_status: "pending".into(),
                exit_code: 0,
                error: None,
            },
        )
        .unwrap();
        for cache in [".noetome-sources", ".noetome-pdf-parses"] {
            fs::create_dir(root.join(cache)).unwrap();
            fs::write(
                root.join(cache).join(".owner.json"),
                format!("{{\"schema_version\":1,\"project_id\":\"{PROJECT}\"}}"),
            )
            .unwrap();
        }
        fs::create_dir(root.join(".noetome-sources").join(VERSION)).unwrap();
        fs::write(
            root.join(".noetome-sources").join(VERSION).join("raw.bin"),
            RAW,
        )
        .unwrap();
        let location = root.join(".noetome-pdf-parses").join(PARSE);
        fs::create_dir(&location).unwrap();
        fs::create_dir(location.join("artifacts")).unwrap();
        fs::write(location.join("report.json"), report).unwrap();
        for (path, bytes) in resources {
            fs::write(location.join(path), bytes).unwrap();
        }
        (connection, evidence)
    }
    #[test]
    fn persisted_primitive_relations_and_artifact_paths_replay() {
        let sandbox = Sandbox::new();
        let (connection, _) = fixture(&sandbox.0);
        let store = Store::load(&connection, PROJECT).unwrap();
        let pdf = PdfStore::load(&connection, &store, PROJECT).unwrap();
        pdf.validate_snapshots(&sandbox.0, &store, PROJECT, None, None)
            .unwrap();
    }
    #[test]
    fn failed_import_exit_must_equal_the_saved_actual_evidence_exit() {
        let sandbox = Sandbox::new();
        let (connection, mut evidence) = fixture(&sandbox.0);
        evidence.status = PdfParseStatus::Failed;
        evidence.exit_code = 2;
        evidence.host_report = None;
        evidence.failure = Some(SourceFailure {
            code: "PDF_RUNTIME_INVALID".into(),
            message: "actual unsupported runtime".into(),
        });
        let bytes = serde_json::to_vec_pretty(&evidence).unwrap();
        let path = sandbox.0.join(".noetome-pdf-parses").join(PARSE);
        fs::write(path.join("report.json"), &bytes).unwrap();
        for name in ["P0_text.txt", "pdf_inventory.bin"] {
            fs::remove_file(path.join("artifacts").join(name)).unwrap();
        }
        connection
            .execute_batch(
                "DELETE FROM pdf_regions;DELETE FROM pdf_pages;DELETE FROM pdf_artifacts;",
            )
            .unwrap();
        connection.execute("UPDATE pdf_parses SET status='failed',page_count=NULL,report_sha256=?1,report_byte_length=?2,error_code='PDF_RUNTIME_INVALID',error_message='actual unsupported runtime'",rusqlite::params![crate::text::sha256(&bytes),bytes.len() as i64]).unwrap();
        connection.execute_batch("UPDATE pdf_imports SET parse_status='failed',exit_code=2,error_code='PDF_RUNTIME_INVALID',error_message='actual unsupported runtime'").unwrap();
        let store = Store::load(&connection, PROJECT).unwrap();
        let pdf = PdfStore::load(&connection, &store, PROJECT).unwrap();
        pdf.validate_snapshots(&sandbox.0, &store, PROJECT, None, None)
            .unwrap();
        connection
            .execute_batch("UPDATE pdf_imports SET exit_code=3")
            .unwrap();
        let store = Store::load(&connection, PROJECT).unwrap();
        let pdf = PdfStore::load(&connection, &store, PROJECT).unwrap();
        assert!(
            pdf.validate_snapshots(&sandbox.0, &store, PROJECT, None, None)
                .is_err(),
            "changed import exit was accepted despite actual saved evidence exit2"
        );
        evidence.exit_code = 3;
        let bytes = serde_json::to_vec_pretty(&evidence).unwrap();
        fs::write(path.join("report.json"), &bytes).unwrap();
        connection
            .execute(
                "UPDATE pdf_parses SET report_sha256=?1,report_byte_length=?2",
                rusqlite::params![crate::text::sha256(&bytes), bytes.len() as i64],
            )
            .unwrap();
        let store = Store::load(&connection, PROJECT).unwrap();
        let pdf = PdfStore::load(&connection, &store, PROJECT).unwrap();
        assert!(
            pdf.validate_snapshots(&sandbox.0, &store, PROJECT, None, None)
                .is_err(),
            "synchronized no-Host runtime failure exit3 was accepted despite code requiring2"
        );
    }
    fn host_observation_missing(which: &str) {
        let sandbox = Sandbox::new();
        let (connection, mut evidence) = fixture(&sandbox.0);
        let host = evidence.host_report.as_mut().unwrap();
        if which == "pid" {
            host.pid = None;
        } else {
            host.native_exit_code = None;
        }
        let bytes = serde_json::to_vec_pretty(&evidence).unwrap();
        let path = sandbox.0.join(".noetome-pdf-parses").join(PARSE);
        fs::write(path.join("report.json"), &bytes).unwrap();
        connection
            .execute(
                "UPDATE pdf_parses SET report_sha256=?1,report_byte_length=?2",
                rusqlite::params![crate::text::sha256(&bytes), bytes.len() as i64],
            )
            .unwrap();
        let store = Store::load(&connection, PROJECT).unwrap();
        let pdf = PdfStore::load(&connection, &store, PROJECT).unwrap();
        assert!(
            pdf.validate_snapshots(&sandbox.0, &store, PROJECT, None, None)
                .is_err(),
            "missing observed {which} was accepted"
        );
    }
    #[test]
    fn response_requires_observed_success_native_exit() {
        host_observation_missing("native_exit_code");
    }
    #[test]
    fn response_requires_observed_positive_pid() {
        host_observation_missing("pid");
    }
    #[test]
    fn unread_pdf_import_requires_the_real_acquisition_failure_exit() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(crate::pdf_source_schema::SCHEMA)
            .unwrap();
        connection
            .execute(
                "INSERT INTO project_metadata VALUES(1,?1,3,?2,'initialized')",
                rusqlite::params![PROJECT, TIME],
            )
            .unwrap();
        insert_import(
            &connection,
            &PdfImportRecord {
                import_id: IMPORT.into(),
                imported_at: TIME.into(),
                selected_path: "D:/selected/missing.pdf".into(),
                source_id: None,
                version_id: None,
                parse_id: None,
                raw_status: "failed".into(),
                parse_status: "not_run".into(),
                reused_version: false,
                reused_parse: false,
                knowledge_status: "pending".into(),
                exit_code: 3,
                error: Some(SourceFailure {
                    code: "SOURCE_READ_ERROR".into(),
                    message: "actual missing input".into(),
                }),
            },
        )
        .unwrap();
        Store::load(&connection, PROJECT).unwrap();
        connection
            .execute("UPDATE pdf_imports SET exit_code=2", [])
            .unwrap();
        assert!(
            Store::load(&connection, PROJECT).is_err(),
            "changed unread acquisition exit2 was accepted"
        );
    }
    #[test]
    fn synchronized_report_and_sql_bbox_changes_still_fail_primitive_recomputation() {
        let sandbox = Sandbox::new();
        let (connection, mut evidence) = fixture(&sandbox.0);
        evidence
            .host_report
            .as_mut()
            .unwrap()
            .response
            .as_mut()
            .unwrap()
            .pages[0]
            .regions[0]
            .locator
            .bbox_pt = [11., 10., 20., 20.];
        let bytes = serde_json::to_vec_pretty(&evidence).unwrap();
        fs::write(
            sandbox
                .0
                .join(".noetome-pdf-parses")
                .join(PARSE)
                .join("report.json"),
            &bytes,
        )
        .unwrap();
        connection
            .execute(
                "UPDATE pdf_regions SET bbox_json='[11.0,10.0,20.0,20.0]'",
                [],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE pdf_parses SET report_sha256=?1,report_byte_length=?2",
                rusqlite::params![crate::text::sha256(&bytes), bytes.len() as i64],
            )
            .unwrap();
        let store = Store::load(&connection, PROJECT).unwrap();
        let pdf = PdfStore::load(&connection, &store, PROJECT).unwrap();
        let error = pdf
            .validate_snapshots(&sandbox.0, &store, PROJECT, None, None)
            .unwrap_err();
        assert!(error.to_string().contains("重算违规清单"));
        assert_eq!(
            fs::read(
                sandbox
                    .0
                    .join(".noetome-sources")
                    .join(VERSION)
                    .join("raw.bin")
            )
            .unwrap(),
            RAW
        );
    }
    #[test]
    fn rows_reject_reordered_reuse_orphans_and_unsupported_configuration() {
        for mutation in [
            "UPDATE pdf_imports SET reused_version=1,reused_parse=1",
            "UPDATE pdf_parses SET ordinal=1",
            "UPDATE pdf_parses SET cache_key='bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'",
            "DELETE FROM pdf_imports",
            "UPDATE pdf_regions SET ordinal=1",
            "UPDATE pdf_pages SET physical_page_index=1",
            "UPDATE pdf_artifacts SET relative_path='../human.txt'",
            "UPDATE pdf_parses SET config_json=replace(config_json,'3.12.14','3.12.15')",
            "UPDATE pdf_pages SET geometry_json='[]'",
            "UPDATE pdf_pages SET glyph_count=32769",
        ] {
            let sandbox = Sandbox::new();
            let (connection, _) = fixture(&sandbox.0);
            if let Err(error) = connection.execute_batch(mutation) {
                assert_eq!(
                    error.sqlite_error_code(),
                    Some(rusqlite::ErrorCode::ConstraintViolation),
                    "unexpected error for {mutation}"
                );
                let store = Store::load(&connection, PROJECT).unwrap();
                PdfStore::load(&connection, &store, PROJECT).unwrap();
                continue;
            }
            let read = Store::load(&connection, PROJECT)
                .and_then(|store| PdfStore::load(&connection, &store, PROJECT));
            assert!(read.is_err(), "accepted {mutation}");
        }
    }
    #[test]
    fn snapshots_reject_extra_files_missing_nullable_and_pdf_hardlinks() {
        for mutation in 0..4 {
            let sandbox = Sandbox::new();
            let (connection, mut evidence) = fixture(&sandbox.0);
            let parse = sandbox.0.join(".noetome-pdf-parses").join(PARSE);
            match mutation {
                0 => fs::write(parse.join("human.txt"), b"keep this human file").unwrap(),
                1 => {
                    let mut value = serde_json::to_value(&evidence).unwrap();
                    value.as_object_mut().unwrap().remove("host_report");
                    let bytes = serde_json::to_vec(&value).unwrap();
                    fs::write(parse.join("report.json"), &bytes).unwrap();
                    connection
                        .execute(
                            "UPDATE pdf_parses SET report_sha256=?1,report_byte_length=?2",
                            rusqlite::params![crate::text::sha256(&bytes), bytes.len() as i64],
                        )
                        .unwrap();
                }
                2 => fs::hard_link(
                    parse.join("artifacts/P0_text.txt"),
                    sandbox.0.join("external-link.txt"),
                )
                .unwrap(),
                _ => {
                    evidence.host_report.as_mut().unwrap().page_display[0]
                        .displayed_physical_page = 2;
                    let bytes = serde_json::to_vec(&evidence).unwrap();
                    fs::write(parse.join("report.json"), &bytes).unwrap();
                    connection
                        .execute(
                            "UPDATE pdf_parses SET report_sha256=?1,report_byte_length=?2",
                            rusqlite::params![crate::text::sha256(&bytes), bytes.len() as i64],
                        )
                        .unwrap();
                }
            }
            let store = Store::load(&connection, PROJECT).unwrap();
            let pdf = PdfStore::load(&connection, &store, PROJECT).unwrap();
            assert!(
                pdf.validate_snapshots(&sandbox.0, &store, PROJECT, None, None)
                    .is_err(),
                "accepted mutation {mutation}"
            );
            if mutation == 0 {
                assert_eq!(
                    fs::read(parse.join("human.txt")).unwrap(),
                    b"keep this human file"
                );
            }
        }
    }
    #[test]
    fn every_persisted_dto_is_named_strict_and_nullable_fields_are_required() {
        let (evidence, _) = primitive_evidence();
        let config = serde_json::to_value(&evidence.config).unwrap();
        for field in [
            "protocol_schema",
            "protocol",
            "adapter_id",
            "adapter_version",
            "options_version",
            "runtime",
            "pdf_policy",
            "artifact_policy",
        ] {
            let mut changed = config.clone();
            changed.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<PdfCacheConfig>(changed).is_err(),
                "missing config {field}"
            );
        }
        let mut changed = config.clone();
        changed["unknown"] = json!(1);
        assert!(serde_json::from_value::<PdfCacheConfig>(changed).is_err());
        assert!(
            serde_json::from_value::<PdfCacheConfig>(json!(
                config.as_object().unwrap().values().collect::<Vec<_>>()
            ))
            .is_err()
        );
        let text = serde_json::to_string(&evidence).unwrap();
        assert!(
            serde_json::from_str::<PdfEvidence>(&text.replacen(
                "\"schema_version\":1",
                "\"schema_version\":1,\"schema_version\":1",
                1
            ))
            .is_err()
        );
        for field in ["host_report", "failure"] {
            let mut changed = serde_json::to_value(&evidence).unwrap();
            changed.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<PdfEvidence>(changed).is_err(),
                "missing evidence {field}"
            );
        }
        let sandbox = Sandbox::new();
        let (connection, _) = fixture(&sandbox.0);
        let store = Store::load(&connection, PROJECT).unwrap();
        let pdf = PdfStore::load(&connection, &store, PROJECT).unwrap();
        let parse = serde_json::to_value(&pdf.parses[PARSE]).unwrap();
        for field in ["page_count", "error"] {
            let mut changed = parse.clone();
            changed.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<PdfStoredParse>(changed).is_err());
        }
        assert!(
            serde_json::from_value::<PdfStoredParse>(json!(
                parse.as_object().unwrap().values().collect::<Vec<_>>()
            ))
            .is_err()
        );
        let import = serde_json::to_value(&pdf.imports[0]).unwrap();
        for field in ["source_id", "version_id", "parse_id", "error"] {
            let mut changed = import.clone();
            changed.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<PdfImportRecord>(changed).is_err());
        }
        assert!(
            serde_json::from_value::<PdfImportRecord>(json!(
                import.as_object().unwrap().values().collect::<Vec<_>>()
            ))
            .is_err()
        );
        assert!(
            serde_json::from_str::<PdfCacheOwner>("[1,\"92175542-40d5-47e4-a62b-31209ee9fe52\"]")
                .is_err()
        );
    }
    #[test]
    fn configuration_key_separates_real_policy_and_source_identity() {
        let normal = PdfCacheConfig::current(PdfLimits::default());
        let mut limits = PdfLimits::default();
        limits.policy.max_pages = 1;
        let narrow = PdfCacheConfig::current(limits);
        let raw = "a".repeat(64);
        assert_ne!(
            normal.cache_key("3df1b5c9-2afc-44c7-9dba-dfda5b75d920", &raw),
            narrow.cache_key("3df1b5c9-2afc-44c7-9dba-dfda5b75d920", &raw)
        );
        assert_ne!(
            normal.cache_key(VERSION, &raw),
            normal.cache_key(PARSE, &raw)
        );
        assert_ne!(
            normal.cache_key(VERSION, &raw),
            normal.cache_key(VERSION, &"b".repeat(64))
        );
        assert!(normal.limits().is_ok());
    }
    #[test]
    fn evidence_requires_named_object_and_explicit_nulls() {
        let value = json!({"schema_version":1,"input":{"relative_path":"input/raw.bin","byte_length":1,"sha256":"a".repeat(64)},"config":PdfCacheConfig::current(PdfLimits::default()),"status":"failed","exit_code":3,"host_report":null,"failure":{"code":"SIDECAR_TIMEOUT","message":"Actual timeout"}});
        assert!(serde_json::from_value::<PdfEvidence>(value.clone()).is_ok());
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove("host_report");
        assert!(
            serde_json::from_value::<PdfEvidence>(missing).is_err(),
            "missing required nullable field was accepted"
        );
    }
    #[test]
    fn evidence_failure_rejects_sequence_form() {
        let value = json!({"schema_version":1,"input":{"relative_path":"input/raw.bin","byte_length":1,"sha256":"a".repeat(64)},"config":PdfCacheConfig::current(PdfLimits::default()),"status":"failed","exit_code":3,"host_report":null,"failure":["SIDECAR_TIMEOUT","Actual timeout"]});
        assert!(serde_json::from_value::<PdfEvidence>(value).is_err());
    }
}
