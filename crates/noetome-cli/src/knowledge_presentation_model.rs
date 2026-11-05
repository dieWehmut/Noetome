//! Strict presentation exchange, separate from author-provided fixture plans.
use crate::{
    extraction_model::{CandidateAtom, CandidateEvidence, RequestUnit, SourceBinding},
    fixture_model::Concept,
    knowledge_coordination_v2_model::{AtomContext, Time},
    sidecar_model::Fingerprint,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

macro_rules! fixed {
    ($name:ident,$kind:ty,$value:expr) => {
        fn $name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<$kind, D::Error> {
            let value = <$kind>::deserialize(d)?;
            if value == $value {
                Ok(value)
            } else {
                Err(serde::de::Error::custom(
                    "unsupported fixed presentation field",
                ))
            }
        }
    };
}
fixed!(one, u32, 1);
fixed!(producer, String, "knowledge_presentation");
fixed!(scope, String, "active_presentation_v1");
fixed!(origin, String, "deterministic_planner");
fixed!(pending, String, "pending");
fixed!(science, String, "not_run");
fixed!(research, String, "not_requested");
fixed!(no, bool, false);
fixed!(zero, usize, 0);
fixed!(known, String, "known");
fn nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
macro_rules! dto {
    ($name:ident {$($(#[$attr:meta])* $field:ident:$kind:ty),*$(,)?}) => {
        #[derive(Debug,Clone,Serialize,Deserialize)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct $name { $($(#[$attr])* pub(crate) $field:$kind),* }
    };
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Representation {
    Table,
    Paragraph,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CellKind {
    Label,
    Facts,
    Missing,
}
dto!(EntityLabel {concept_id:String,label:String,occurrence_ids:Vec<String>});
dto!(Decision {rule_id:String,reason:String,shared_property_ids:Vec<String>});
dto!(PresentationFact {
    fact_id:String,object_id:String,property_id:String,
    #[serde(deserialize_with="known")]value_status:String,
    value:String,unit:String,conditions:Vec<String>,exceptions:Vec<String>,
    #[serde(deserialize_with="nullable")]applicability_status:Option<String>,
    #[serde(deserialize_with="nullable")]time:Option<Time>,
    occurrence_ids:Vec<String>
});
dto!(Provenance {
    occurrence_id:String,input_id:String,original_atom_id:String,binding:SourceBinding,
    unit:RequestUnit,original_concepts:Vec<Concept>,original_atom:CandidateAtom,
    evidence:Vec<CandidateEvidence>,
    #[serde(deserialize_with="nullable")]context:Option<AtomContext>
});
dto!(SemanticLink {
    target_concept_id: String,
    target_path: String,
    target_heading: String,
    display_text: String
});
dto!(Column {
    column_id:String,
    #[serde(deserialize_with="nullable")]property_id:Option<String>,
    label:String,concept_ids:Vec<String>,occurrence_ids:Vec<String>
});
dto!(Cell {column_id:String,kind:CellKind,text:String,fact_ids:Vec<String>,occurrence_ids:Vec<String>,concept_ids:Vec<String>,links:Vec<SemanticLink>});
dto!(Row {row_id:String,object_id:String,cells:Vec<Cell>});
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Node {
    Table {
        node_id: String,
        columns: Vec<Column>,
        rows: Vec<Row>,
    },
    Paragraph {
        node_id: String,
        text: String,
        fact_ids: Vec<String>,
        occurrence_ids: Vec<String>,
        concept_ids: Vec<String>,
        links: Vec<SemanticLink>,
    },
}
dto!(Note {note_id:String,title:String,output_path:String,section_id:String,heading:String,nodes:Vec<Node>});
dto!(PresentationPlan {
    #[serde(deserialize_with="one")]schema_version:u32,
    #[serde(deserialize_with="producer")]producer:String,
    #[serde(deserialize_with="scope")]scope:String,
    #[serde(deserialize_with="origin")]representation_origin:String,
    #[serde(deserialize_with="one")]revision:u32,
    request_id:String,project_id:String,coordination_schema_version:u32,coordination_ir_fingerprint:Fingerprint,
    #[serde(deserialize_with="pending")]knowledge_status:String,
    #[serde(deserialize_with="no")]published:bool,
    #[serde(deserialize_with="science")]scientific_quality:String,
    #[serde(deserialize_with="research")]research_status:String,
    representation:Representation,decision:Decision,objects:Vec<EntityLabel>,properties:Vec<EntityLabel>,
    facts:Vec<PresentationFact>,provenance:Vec<Provenance>,notes:Vec<Note>
});
dto!(ScopeLocator {
    section_id:String,node_id:String,kind:String,
    #[serde(deserialize_with="nullable")]row_id:Option<String>,
    #[serde(deserialize_with="nullable")]column_id:Option<String>
});
dto!(ContentMapping {scope:ScopeLocator,fact_ids:Vec<String>,occurrence_ids:Vec<String>,concept_ids:Vec<String>,start_byte:u64,end_byte:u64});
dto!(LinkMapping {
    scope: ScopeLocator,
    target_concept_id: String,
    target_path: String,
    target_heading: String,
    display_text: String,
    start_byte: u64,
    end_byte: u64
});
dto!(RenderedFile {relative_path:String,note_id:String,plan_revision:u32,content_sha256:String,byte_length:u64,content_mappings:Vec<ContentMapping>,links:Vec<LinkMapping>});
dto!(Checks {
    source_review: String,
    coordination: String,
    note_plan: String,
    rendering: String,
    saved_output: String
});
dto!(Totals {
    input_bundles: usize,
    facts: usize,
    source_occurrences: usize,
    objects: usize,
    properties: usize,
    table_cells: usize,
    missing_cells: usize,
    rendered_notes: usize,
    content_mappings: usize,
    links: usize
});
dto!(PresentationReport {
    #[serde(deserialize_with="one")]schema_version:u32,
    #[serde(deserialize_with="producer")]producer:String,
    #[serde(deserialize_with="producer")]report_kind:String,
    #[serde(deserialize_with="scope")]scope:String,
    request_id:String,project_id:String,coordination_schema_version:u32,status:String,preview_status:String,
    #[serde(deserialize_with="pending")]knowledge_status:String,
    #[serde(deserialize_with="no")]published:bool,
    #[serde(deserialize_with="science")]scientific_quality:String,
    #[serde(deserialize_with="research")]research_status:String,
    #[serde(deserialize_with="zero")]network_calls:usize,
    #[serde(deserialize_with="zero")]paid_calls:usize,
    checks:Checks,totals:Totals,issue_codes:Vec<String>,
    #[serde(deserialize_with="nullable")]decision:Option<Decision>,
    #[serde(deserialize_with="nullable")]coordination_ir_fingerprint:Option<Fingerprint>,
    #[serde(deserialize_with="nullable")]plan_fingerprint:Option<Fingerprint>,
    inputs:BTreeMap<String,Fingerprint>,files:Vec<RenderedFile>,artifacts:BTreeMap<String,Fingerprint>
});
