use crate::domain::{Concept, KnowledgeAtom};
use crate::extraction_api_wire::ApiCandidates;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompileConfig {
    pub schema_version: u32,
    pub endpoint: String,
    pub model: String,
    pub credential_env: String,
    pub transport_mode: String,
    pub usage_ledger: PathBuf,
    pub max_chunk_bytes: usize,
    pub max_output_tokens: u64,
    pub request_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceScope {
    pub source_id: String,
    pub source_sha256: String,
    pub display_name: String,
    pub byte_length: u64,
    pub physical_pages: u32,
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
    pub prepared_pages: u64,
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
