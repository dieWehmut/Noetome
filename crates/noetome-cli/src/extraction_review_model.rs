//! Strict input DTOs for an offline, single-measurement review.
use crate::extraction_api_wire::ApiCandidates;
use crate::extraction_model::{RequestUnit, Selection, SourceBinding};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
macro_rules! fixed {
    ($name:ident,$ty:ty,$value:expr) => {
        fn $name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<$ty, D::Error> {
            let v = <$ty>::deserialize(d)?;
            if v != $value {
                Err(serde::de::Error::custom("unsupported fixed field"))
            } else {
                Ok(v)
            }
        }
    };
}
fixed!(one, u32, 1);
fixed!(two, u32, 2);
fixed!(purpose, String, "single_measurement_source_review");
fixed!(api, String, "api");
fixed!(fake, String, "fake");
fixed!(science, String, "not_run");
fixed!(verification, String, "unverified");
fixed!(pending, String, "pending");
fixed!(false_field, bool, false);
fixed!(reviewer, String, "automated_source_review");
fixed!(review_producer, String, "source_review");
fixed!(matched, String, "passed");
macro_rules! dto {($name:ident {$($(#[$a:meta])* $f:ident:$t:ty),* $(,)?})=>{
 #[derive(Debug,Clone,Serialize,Deserialize)] #[serde(deny_unknown_fields)] pub(crate) struct $name {$($(#[$a])* pub(crate) $f:$t),*}
};}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Timing {
    AfterModelResponse,
    BeforeSyntheticResponse,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Transport {
    Live,
    LocalHttpTest,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Disposition {
    Included,
    Excluded,
    NeedsReview,
    Pending,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GoldValueStatus {
    Known,
    Unknown,
}
dto!(Review {#[serde(deserialize_with="one")] schema_version:u32,#[serde(deserialize_with="purpose")] purpose:String,selection:Vec<Selection>,candidate_file:String,gold_file:String,gold_sha256:String,decision_file:String,subject:String,property:String,allowed_contexts:Vec<String>,#[serde(deserialize_with="reviewer")] reviewer:String,interpretation_timing:Timing});
dto!(SavedCandidate {
    #[serde(deserialize_with = "one")]
    schema_version: u32,
    #[serde(deserialize_with = "api")]
    producer: String,
    transport_mode: Transport,
    request_id: String,
    binding: SourceBinding,
    payload: ApiCandidates
});
dto!(Decision {#[serde(deserialize_with="one")] schema_version:u32,unit_id:String,disposition:Disposition,atom_ids:Vec<String>,#[serde(deserialize_with="nullable")] exclusion_reason:Option<String>});
dto!(Gold1 {#[serde(deserialize_with="one")] schema_version:u32,author:String,#[serde(deserialize_with="fake")] producer:String,#[serde(deserialize_with="science")] scientific_quality:String,concept_name:String,atom_content:String,value:String,unit:String,conditions:Vec<String>,quote:String,#[serde(deserialize_with="verification")] verification:String,#[serde(deserialize_with="false_field")] required:bool,#[serde(deserialize_with="pending")] knowledge_status:String,#[serde(deserialize_with="false_field")] published:bool});
dto!(Gold2 {#[serde(deserialize_with="two")] schema_version:u32,author:String,#[serde(deserialize_with="fake")] producer:String,#[serde(deserialize_with="science")] scientific_quality:String,concept_name:String,atom_content:String,#[serde(deserialize_with="nullable")] value:Option<String>,#[serde(deserialize_with="nullable")] unit:Option<String>,value_status:GoldValueStatus,conditions:Vec<String>,exceptions:Vec<String>,quote:String,#[serde(deserialize_with="verification")] verification:String,#[serde(deserialize_with="false_field")] required:bool,#[serde(deserialize_with="pending")] knowledge_status:String,#[serde(deserialize_with="false_field")] published:bool});
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum GoldInput {
    V1(Gold1),
    V2(Gold2),
}
pub(crate) struct Gold {
    pub(crate) concept_name: String,
    pub(crate) content: String,
    pub(crate) value: Option<String>,
    pub(crate) unit: Option<String>,
    pub(crate) conditions: Vec<String>,
    pub(crate) exceptions: Vec<String>,
    pub(crate) quote: String,
    pub(crate) unknown: bool,
}
impl From<GoldInput> for Gold {
    fn from(g: GoldInput) -> Self {
        match g {
            GoldInput::V1(g) => Self {
                concept_name: g.concept_name,
                content: g.atom_content,
                value: Some(g.value),
                unit: Some(g.unit),
                conditions: g.conditions,
                exceptions: vec![],
                quote: g.quote,
                unknown: false,
            },
            GoldInput::V2(g) => Self {
                concept_name: g.concept_name,
                content: g.atom_content,
                value: g.value,
                unit: g.unit,
                conditions: g.conditions,
                exceptions: g.exceptions,
                quote: g.quote,
                unknown: g.value_status == GoldValueStatus::Unknown,
            },
        }
    }
}
dto!(Check {category:String,object_id:String,unit_id:String,start_byte:usize,end_byte:usize,status:String,#[serde(deserialize_with="nullable")] code:Option<String>,expected:Value,actual:Value});
dto!(ObjectResult {object_id:String,unit_id:String,start_byte:usize,end_byte:usize,status:String,issue_codes:Vec<String>});
dto!(ReviewIr {#[serde(deserialize_with="one")] schema_version:u32,#[serde(deserialize_with="review_producer")] producer:String,#[serde(deserialize_with="one")] revision:u32,request_id:String,binding:SourceBinding,#[serde(deserialize_with="matched")] source_match:String,concepts:Vec<crate::fixture_model::Concept>,atoms:Vec<crate::extraction_model::CandidateAtom>,evidence:Vec<crate::extraction_model::CandidateEvidence>,checks:Vec<Check>});
dto!(Snapshot {#[serde(deserialize_with="one")] schema_version:u32,binding:SourceBinding,units:Vec<RequestUnit>});
dto!(Scope {purpose:String,selection:Vec<Selection>,binding:SourceBinding,units:Vec<RequestUnit>,expected_atoms:usize});
dto!(Origin {#[serde(deserialize_with="nullable")] producer:Option<String>,#[serde(deserialize_with="nullable")] transport_mode:Option<Transport>,reviewer:String,interpretation_timing:Timing,input_fingerprints:std::collections::BTreeMap<String,crate::sidecar_model::Fingerprint>,gold_sha256:String});
dto!(Rules {grammar:String,subject:String,property:String,allowed_contexts:Vec<String>,reviewer:String,interpretation_timing:Timing});
dto!(Totals {expected_atoms:usize,passed:usize,failed:usize,needs_review:usize,unprocessed:usize,#[serde(deserialize_with="nullable")] candidate_atoms:Option<usize>,#[serde(deserialize_with="nullable")] unexpected_atoms:Option<usize>,#[serde(deserialize_with="nullable")] duplicate_atoms:Option<usize>,required_concept_groups:usize,additional_source_concepts:usize});
dto!(Quality {
    expected: usize,
    passed: usize,
    failed: usize,
    needs_review: usize,
    unprocessed: usize,
    status: String
});
dto!(ReviewReport {#[serde(deserialize_with="one")] schema_version:u32,report_kind:String,#[serde(deserialize_with="review_producer")] producer:String,#[serde(deserialize_with="nullable")] request_id:Option<String>,status:String,knowledge_status:String,published:bool,scientific_quality:String,research_status:String,network_calls:usize,paid_calls:usize,scope:Scope,binding:SourceBinding,origin:Origin,rules:Rules,expected_results:Vec<ObjectResult>,candidate_results:Vec<ObjectResult>,checks:Vec<Check>,totals:Totals,quality:std::collections::BTreeMap<String,Quality>,artifacts:std::collections::BTreeMap<String,crate::sidecar_model::Fingerprint>});
