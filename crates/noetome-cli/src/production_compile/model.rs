use crate::domain::{Concept, KnowledgeAtom};
use crate::extraction_api_wire::ApiCandidates;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

pub(super) const LEGACY_REQUEST_STRATEGY: u32 = 1;
pub(super) const PRECISE_REQUEST_STRATEGY: u32 = 2;
pub(super) const ANCHOR_REQUEST_STRATEGY: u32 = 3;
pub(super) const COMPACT_REQUEST_STRATEGY: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompileConfig {
    pub schema_version: u32,
    pub endpoint: String,
    pub model: String,
    #[serde(default)]
    pub credential_env: String,
    #[serde(default)]
    pub credential_mode: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    pub transport_mode: String,
    pub usage_ledger: PathBuf,
    pub max_chunk_bytes: usize,
    pub max_output_tokens: u64,
    pub request_timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_strategy_version: Option<u32>,
}

impl CompileConfig {
    pub(super) fn strategy_version(&self) -> Result<u32, crate::project::ProjectError> {
        match self
            .request_strategy_version
            .unwrap_or(LEGACY_REQUEST_STRATEGY)
        {
            version @ (LEGACY_REQUEST_STRATEGY
            | PRECISE_REQUEST_STRATEGY
            | ANCHOR_REQUEST_STRATEGY
            | COMPACT_REQUEST_STRATEGY) => Ok(version),
            _ => Err(super::error(
                "COMPILE_REQUEST_STRATEGY",
                "请求策略版本不受支持；历史缺省配置仅按 v1 解释。",
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompileScope {
    pub schema_version: u32,
    pub sources: Vec<SourceSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceSelection {
    pub source_id: String,
    pub page_ranges: Vec<PageRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PageRange {
    pub start_page: u32,
    pub end_page: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceScope {
    pub source_id: String,
    pub source_sha256: String,
    pub display_name: String,
    pub byte_length: u64,
    pub physical_pages: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_schema_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_ranges: Option<Vec<PageRange>>,
}

impl SourceScope {
    pub(super) fn validate_ranges(&self) -> Result<(), crate::project::ProjectError> {
        let valid = match (self.scope_schema_version, self.page_ranges.as_deref()) {
            (None, None) => self.physical_pages > 0,
            (Some(1), Some(ranges)) if !ranges.is_empty() => {
                let mut previous_end = 0;
                ranges.iter().all(|range| {
                    let valid = range.start_page > previous_end
                        && range.start_page <= range.end_page
                        && range.end_page <= self.physical_pages;
                    previous_end = range.end_page;
                    valid
                })
            }
            _ => false,
        };
        if !valid {
            return Err(super::error(
                "COMPILE_SCOPE",
                "范围版本或物理页区间无效；需非空、一基、顺序不重叠且不超过原书页数。",
            ));
        }
        Ok(())
    }

    pub(super) fn selected_pages(&self) -> u64 {
        self.page_ranges
            .as_ref()
            .map_or(self.physical_pages as u64, |ranges| {
                ranges
                    .iter()
                    .map(|range| u64::from(range.end_page) - u64::from(range.start_page) + 1)
                    .sum()
            })
    }

    pub(super) fn contains_page(&self, number: u32) -> bool {
        number > 0
            && number <= self.physical_pages
            && self.page_ranges.as_ref().is_none_or(|ranges| {
                ranges
                    .iter()
                    .any(|range| (range.start_page..=range.end_page).contains(&number))
            })
    }

    // The saved preparation cursor is a count, never an absolute physical page.
    pub(super) fn page_at(&self, mut ordinal: u32) -> Option<u32> {
        if let Some(ranges) = &self.page_ranges {
            for range in ranges {
                let count = range.end_page - range.start_page + 1;
                if ordinal < count {
                    return Some(range.start_page + ordinal);
                }
                ordinal -= count;
            }
            None
        } else if ordinal < self.physical_pages {
            Some(ordinal + 1)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failure {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub schema_version: u32,
    pub run_id: String,
    pub status: String,
    pub stage: String,
    pub sources: Vec<SourceScope>,
    pub physical_pages: u64,
    pub selected_pages: u64,
    pub out_of_scope_pages: u64,
    pub request_strategy_version: u32,
    /// 已登记的范围内页，保留旧口径，包含准备失败页。
    pub prepared_pages: u64,
    pub unprepared_pages: u64,
    /// 准备失败或含失败单元的去重页数，可与 prepared_pages 重叠。
    pub failed_pages: u64,
    pub units_total: u64,
    pub units_confirmed: u64,
    pub units_failed: u64,
    pub pending_ocr_pages: u64,
    pub unresolved_pages: u64,
    pub concepts: u64,
    pub atoms: u64,
    pub evidence: u64,
    pub attempts: u64,
    pub ir_revision: u64,
    pub knowledge_status: String,
    pub scientific_quality: String,
    pub budget: Option<Value>,
    pub error: Option<Failure>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Unit {
    pub unit_id: String,
    pub source_id: String,
    pub source_sha256: String,
    pub page_number: u32,
    pub page_sha256: String,
    pub bbox: [f64; 4],
    pub source_status: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub text: String,
    pub issue_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Attempt {
    pub request_id: String,
    pub unit_id: String,
    pub attempt_no: u32,
    pub state: String,
    pub directory: PathBuf,
    pub request_sha256: String,
    pub response_sha256: Option<String>,
    pub receipt_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConceptContext {
    pub concept: Concept,
    pub sample_atoms: Vec<KnowledgeAtom>,
}

pub(crate) struct AcceptedBatch {
    pub candidates: ApiCandidates,
    pub allowed_existing_concepts: Vec<Concept>,
}
