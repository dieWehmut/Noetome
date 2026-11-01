//! Facts shared by the fixture compiler, SQLite store, and validation reports.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::fixture_model::Issue;
use crate::fixture_render::RenderedFile;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompileContext {
    pub(crate) case_id: String,
    pub(crate) run_id: String,
    pub(crate) release_id: String,
    pub(crate) started_at: String,
    pub(crate) fixture_scope_id: String,
    pub(crate) ir_revision: u64,
    pub(crate) plan_revision: u64,
    pub(crate) input_hashes: BTreeMap<String, String>,
    pub(crate) versions: BTreeMap<String, String>,
    pub(crate) counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckFact {
    pub(crate) status: String,
    pub(crate) issue_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Dimension {
    pub(crate) state: String,
    pub(crate) reason: String,
    pub(crate) counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublicationRef {
    pub(crate) database: String,
    pub(crate) commit_table: String,
    pub(crate) release_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompileReport {
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) case_id: String,
    pub(crate) run_id: String,
    pub(crate) release_id: String,
    pub(crate) started_at: String,
    pub(crate) finished_at: String,
    pub(crate) result: String,
    pub(crate) dimensions: BTreeMap<String, Dimension>,
    pub(crate) checks: BTreeMap<String, CheckFact>,
    pub(crate) counts: BTreeMap<String, u64>,
    pub(crate) issues: Vec<Issue>,
    pub(crate) files: Vec<RenderedFile>,
    pub(crate) input_hashes: BTreeMap<String, String>,
    pub(crate) versions: BTreeMap<String, String>,
    pub(crate) publication: PublicationRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReportPaths {
    pub(crate) json: String,
    pub(crate) markdown: String,
}
