//! One explicit fixture graph, separate from production project persistence.

use crate::compile_identity::Identity;
use crate::compile_model::{CheckFact, CompileContext, CompileReport};
use crate::compile_store::{StoreIntent, StoreSnapshot};
use crate::fixture_render::RenderedFile;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) const UNITS: [&str; 6] = [
    "source_contract",
    "render",
    "stage_verification",
    "vault_readback",
    "reports",
    "publication",
];
pub(crate) const POLICY: &str = "fixture-six-unit-v1";

pub(crate) fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileStamp {
    pub(crate) identity: Identity,
    pub(crate) sha256: String,
    pub(crate) byte_length: u64,
    pub(crate) modified: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Owner {
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) run_id: String,
    pub(crate) release_id: String,
    pub(crate) root: String,
    pub(crate) case_root: String,
    pub(crate) root_identity: Identity,
    pub(crate) parent_identity: Identity,
    pub(crate) control_identity: Identity,
    pub(crate) database_identity: Identity,
    pub(crate) lock_identity: Identity,
    pub(crate) case_identity: Identity,
    pub(crate) compiler_fingerprint: String,
    pub(crate) policy_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RunStatus {
    Queued,
    Running,
    Paused,
    Cancelled,
    Succeeded,
    Invalidated,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UnitStatus {
    Pending,
    Running,
    Confirmed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkUnit {
    pub(crate) unit_id: String,
    pub(crate) dependencies: Vec<String>,
    pub(crate) state: UnitStatus,
    pub(crate) attempts: u64,
    pub(crate) output_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunState {
    pub(crate) schema_version: u32,
    pub(crate) status: RunStatus,
    pub(crate) units: Vec<WorkUnit>,
    pub(crate) acknowledged_control: u64,
    pub(crate) checks: BTreeMap<String, CheckFact>,
    pub(crate) manifest: Vec<RenderedFile>,
    #[serde(deserialize_with = "required_option")]
    pub(crate) output_snapshot: Option<StoreSnapshot>,
    #[serde(deserialize_with = "required_option")]
    pub(crate) pending_intent: Option<StoreIntent>,
    #[serde(deserialize_with = "required_option")]
    pub(crate) validation_report: Option<CompileReport>,
    #[serde(deserialize_with = "required_option")]
    pub(crate) diagnostic: Option<String>,
}

impl RunState {
    pub(crate) fn initial(paused: bool) -> Self {
        Self {
            schema_version: 1,
            status: if paused {
                RunStatus::Paused
            } else {
                RunStatus::Queued
            },
            units: UNITS
                .iter()
                .enumerate()
                .map(|(i, id)| WorkUnit {
                    unit_id: (*id).into(),
                    dependencies: if i == 0 {
                        vec![]
                    } else {
                        vec![UNITS[i - 1].into()]
                    },
                    state: UnitStatus::Pending,
                    attempts: 0,
                    output_paths: vec![],
                })
                .collect(),
            acknowledged_control: 0,
            checks: BTreeMap::new(),
            manifest: vec![],
            output_snapshot: None,
            pending_intent: None,
            validation_report: None,
            diagnostic: None,
        }
    }
    pub(crate) fn confirmed(&self) -> usize {
        self.units
            .iter()
            .take_while(|u| u.state == UnitStatus::Confirmed)
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Checkpoint {
    pub(crate) schema_version: u32,
    pub(crate) sequence: u64,
    pub(crate) event: String,
    pub(crate) at: String,
    pub(crate) run_id: String,
    pub(crate) state: RunState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunMetadata {
    pub(crate) owner: Owner,
    pub(crate) owner_file_identity: Identity,
    pub(crate) context: CompileContext,
    pub(crate) inputs: BTreeMap<String, FileStamp>,
}
