//! Strict process-source namespace; independent of measurement/coordination DTOs.
use crate::extraction_model::{RequestUnit, Selection, SourceBinding};
use crate::sidecar_model::Fingerprint;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
macro_rules! fixed {
    ($name:ident,$ty:ty,$value:expr) => {
        fn $name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<$ty, D::Error> {
            let value = <$ty>::deserialize(d)?;
            if value == $value {
                Ok(value)
            } else {
                Err(serde::de::Error::custom("unsupported process fixed field"))
            }
        }
    };
}
fixed!(one, u32, 1);
fixed!(purpose, String, "process_source_review");
fixed!(scope, String, "bounded_process_text_v1");
fixed!(gold_producer, String, "process_source_gold");
fixed!(science, String, "not_run");
fixed!(pending, String, "pending");
fixed!(research, String, "not_requested");
fixed!(passed, String, "passed");
fixed!(false_value, bool, false);
fixed!(zero, usize, 0);
macro_rules! dto {
    ($name:ident {$($(#[$attr:meta])* $field:ident:$ty:ty),* $(,)?}) => {
        #[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct $name { $($(#[$attr])* pub(crate) $field:$ty),* }
    };
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StepKind {
    Step,
    Decision,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RelationKind {
    Sequence,
    Branch,
    Causes,
    Promotes,
    Inhibits,
    Correlates,
    Unknown,
}
dto!(Process {title:String,entry:String,goals:Vec<String>,conditions:Vec<String>,exceptions:Vec<String>});
dto!(Step {id:String,kind:StepKind,label:String,description:String,#[serde(deserialize_with="nullable")] criterion:Option<String>,inputs:Vec<String>,outputs:Vec<String>,conditions:Vec<String>,exceptions:Vec<String>});
dto!(Relation {id:String,left:String,right:String,kind:RelationKind,#[serde(deserialize_with="nullable")] branch_outcome:Option<bool>,conditions:Vec<String>,exceptions:Vec<String>});
dto!(Evidence {
    object_id: String,
    start_byte: usize,
    end_byte: usize,
    quote: String
});
dto!(ProcessDocument {process:Process,steps:Vec<Step>,relations:Vec<Relation>,evidence:Vec<Evidence>});
dto!(ProcessGold {
    #[serde(deserialize_with="one")] schema_version:u32,
    #[serde(deserialize_with="gold_producer")] producer:String,
    author:String,
    #[serde(deserialize_with="science")] scientific_quality:String,
    process:Process,steps:Vec<Step>,relations:Vec<Relation>,evidence:Vec<Evidence>
});
impl ProcessGold {
    pub(crate) fn document(&self) -> ProcessDocument {
        ProcessDocument {
            process: self.process.clone(),
            steps: self.steps.clone(),
            relations: self.relations.clone(),
            evidence: self.evidence.clone(),
        }
    }
}
dto!(ProcessCase {
    #[serde(deserialize_with="one")] schema_version:u32,
    #[serde(deserialize_with="purpose")] purpose:String,
    project_id:String,import_id:String,selection:Vec<Selection>,gold_file:String,gold_sha256:String
});
dto!(ProcessSnapshot {#[serde(deserialize_with="one")] schema_version:u32,binding:SourceBinding,units:Vec<RequestUnit>});
dto!(ProcessIr {
    #[serde(deserialize_with="one")] schema_version:u32,
    #[serde(deserialize_with="purpose")] producer:String,
    #[serde(deserialize_with="scope")] scope:String,
    request_id:String,binding:SourceBinding,unit:RequestUnit,
    #[serde(deserialize_with="passed")] source_match:String,
    #[serde(deserialize_with="passed")] gold_match:String,
    #[serde(deserialize_with="pending")] knowledge_status:String,
    #[serde(deserialize_with="science")] scientific_quality:String,
    #[serde(deserialize_with="false_value")] published:bool,
    document:ProcessDocument,evidence_fingerprints:BTreeMap<String,Fingerprint>
});
dto!(ReviewChecks {
    source_parsing: String,
    gold_match: String,
    saved_output: String
});
dto!(ProcessReviewReport {
    #[serde(deserialize_with="one")] schema_version:u32,
    #[serde(deserialize_with="purpose")] producer:String,
    #[serde(deserialize_with="scope")] scope:String,
    request_id:String,binding:SourceBinding,selection:Vec<Selection>,status:String,
    #[serde(deserialize_with="pending")] knowledge_status:String,
    #[serde(deserialize_with="science")] scientific_quality:String,
    #[serde(deserialize_with="research")] research_status:String,
    #[serde(deserialize_with="false_value")] published:bool,
    #[serde(deserialize_with="zero")] network_calls:usize,
    #[serde(deserialize_with="zero")] paid_calls:usize,
    checks:ReviewChecks,issue_codes:Vec<String>,unparsed_statements:usize,
    #[serde(deserialize_with="nullable")] parsed_document:Option<ProcessDocument>,
    totals:BTreeMap<String,usize>,expected_totals:BTreeMap<String,usize>,gold_sha256:String,
    #[serde(deserialize_with="nullable")] ir_fingerprint:Option<Fingerprint>,
    inputs:BTreeMap<String,Fingerprint>,artifacts:BTreeMap<String,Fingerprint>
});
dto!(ProcessOwner {
    #[serde(deserialize_with = "one")]
    schema_version: u32,
    producer: String,
    request_id: String,
    purpose: String
});

pub(crate) fn document_totals(
    document: Option<&ProcessDocument>,
    unparsed: usize,
) -> BTreeMap<String, usize> {
    let mut totals = BTreeMap::new();
    let steps = document.map(|d| d.steps.as_slice()).unwrap_or(&[]);
    let relations = document.map(|d| d.relations.as_slice()).unwrap_or(&[]);
    totals.insert(
        "source_statements".into(),
        document.map_or(0, |d| d.evidence.len()) + unparsed,
    );
    totals.insert(
        "parsed_statements".into(),
        document.map_or(0, |d| d.evidence.len()),
    );
    totals.insert("unparsed_statements".into(), unparsed);
    totals.insert("steps".into(), steps.len());
    totals.insert(
        "decision_steps".into(),
        steps
            .iter()
            .filter(|s| s.kind == StepKind::Decision)
            .count(),
    );
    totals.insert("relations".into(), relations.len());
    for (name, kind) in [
        ("sequence", RelationKind::Sequence),
        ("branch", RelationKind::Branch),
        ("causes", RelationKind::Causes),
        ("promotes", RelationKind::Promotes),
        ("inhibits", RelationKind::Inhibits),
        ("correlates", RelationKind::Correlates),
        ("unknown", RelationKind::Unknown),
    ] {
        totals.insert(
            format!("{name}_relations"),
            relations.iter().filter(|r| r.kind == kind).count(),
        );
    }
    totals.insert(
        "evidence_records".into(),
        document.map_or(0, |d| d.evidence.len()),
    );
    totals.insert(
        "node_inputs".into(),
        steps.iter().map(|s| s.inputs.len()).sum(),
    );
    totals.insert(
        "node_outputs".into(),
        steps.iter().map(|s| s.outputs.len()).sum(),
    );
    totals.insert(
        "process_conditions".into(),
        document.map_or(0, |d| d.process.conditions.len()),
    );
    totals.insert(
        "process_exceptions".into(),
        document.map_or(0, |d| d.process.exceptions.len()),
    );
    totals.insert(
        "node_conditions".into(),
        steps.iter().map(|s| s.conditions.len()).sum(),
    );
    totals.insert(
        "node_exceptions".into(),
        steps.iter().map(|s| s.exceptions.len()).sum(),
    );
    totals.insert(
        "relation_conditions".into(),
        relations.iter().map(|r| r.conditions.len()).sum(),
    );
    totals.insert(
        "relation_exceptions".into(),
        relations.iter().map(|r| r.exceptions.len()).sum(),
    );
    totals
}
