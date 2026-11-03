//! Strict, independent coordination exchange version; original review DTOs stay unchanged.
use crate::{
    extraction_model::{CandidateAtom, CandidateEvidence, RequestUnit, SourceBinding},
    extraction_review_model::Check,
    fixture_model::Concept,
    sidecar_model::Fingerprint,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
fn nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
macro_rules! fixed {
    ($n:ident,$t:ty,$v:expr) => {
        fn $n<'de, D: serde::Deserializer<'de>>(d: D) -> Result<$t, D::Error> {
            let v = <$t>::deserialize(d)?;
            if v != $v {
                Err(serde::de::Error::custom("unsupported fixed field"))
            } else {
                Ok(v)
            }
        }
    };
}
fixed!(one, u32, 1);
fixed!(purpose, String, "explicit_identity_duplicate_complement");
fixed!(producer, String, "knowledge_coordination");
fixed!(scope, String, "explicit_identity_duplicate_complement_v1");
fixed!(pending, String, "pending");
fixed!(science, String, "not_run");
fixed!(research, String, "not_requested");
fixed!(no, bool, false);
fixed!(zero, usize, 0);
fixed!(passed, String, "passed");
fixed!(known, String, "known");
fn status<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if matches!(s.as_str(), "passed" | "needs_review") {
        Ok(s)
    } else {
        Err(serde::de::Error::custom("invalid status"))
    }
}
fn relation<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if matches!(
        s.as_str(),
        "duplicate" | "complement" | "separate_object" | "needs_review"
    ) {
        Ok(s)
    } else {
        Err(serde::de::Error::custom("invalid relation"))
    }
}
macro_rules! dto {($n:ident{$($(#[$a:meta])*$f:ident:$t:ty),*$(,)?})=>{#[derive(Debug,Clone,Serialize,Deserialize)]#[serde(deny_unknown_fields)]pub(crate) struct $n{$($(#[$a])*pub(crate) $f:$t),*}};}
dto!(CoordinationCase{#[serde(deserialize_with="one")]schema_version:u32,#[serde(deserialize_with="purpose")]purpose:String,project_id:String,identity_file:String,inputs:Vec<InputSpec>});
dto!(InputSpec{input_id:String,review_bundle:String,binding:SourceBinding,artifacts:BTreeMap<String,Fingerprint>});
dto!(IdentityMap{#[serde(deserialize_with="one")]schema_version:u32,author:String,concepts:Vec<CanonicalConcept>,concept_mappings:Vec<ConceptMapping>,atom_mappings:Vec<AtomMapping>});
dto!(CanonicalConcept {
    canonical_id: String,
    display_name: String
});
dto!(ConceptMapping {
    input_id: String,
    original_concept_id: String,
    canonical_id: String
});
dto!(AtomMapping {
    input_id: String,
    original_atom_id: String,
    object_id: String,
    property_id: String
});
dto!(CoordinationInput{input_id:String,binding:SourceBinding,request_id:String,#[serde(deserialize_with="passed")]source_review_status:String,source_review_checks:Vec<Check>,source_bundle_fingerprints:BTreeMap<String,Fingerprint>,copied_artifacts:BTreeMap<String,String>});
dto!(Occurrence{occurrence_id:String,input_id:String,binding:SourceBinding,unit:RequestUnit,original_concepts:Vec<Concept>,original_atom:CandidateAtom,original_evidence:Vec<CandidateEvidence>,canonical_concept_ids:Vec<String>,object_id:String,property_id:String,value:String,unit_value:String,conditions:Vec<String>,exceptions:Vec<String>,#[serde(deserialize_with="status")]status:String,issue_codes:Vec<String>});
dto!(Fact{fact_id:String,object_id:String,property_id:String,#[serde(deserialize_with="known")]value_status:String,value:String,unit:String,conditions:Vec<String>,exceptions:Vec<String>,occurrence_ids:Vec<String>,#[serde(deserialize_with="status")]status:String,issue_codes:Vec<String>});
dto!(Relation{left_occurrence_id:String,right_occurrence_id:String,#[serde(deserialize_with="relation")]relation:String,#[serde(deserialize_with="nullable")]issue_code:Option<String>});
dto!(CoordinationTotals{input_bundles:usize,validated_input_bundles:usize,input_atoms:usize,retained_fact_groups:usize,#[serde(deserialize_with="nullable")]canonical_facts:Option<usize>,source_occurrences:usize,evidence_links:usize,independent_sources:usize,duplicate_occurrences:usize,passed:usize,failed:usize,needs_review:usize,unprocessed:usize});
dto!(CoordinatedFacts{canonical_concepts:Vec<CanonicalConcept>,inputs:Vec<CoordinationInput>,occurrences:Vec<Occurrence>,facts:Vec<Fact>,relations:Vec<Relation>,totals:CoordinationTotals,#[serde(deserialize_with="status")]status:String});
dto!(CoordinationReport{#[serde(deserialize_with="one")]schema_version:u32,#[serde(deserialize_with="producer")]report_kind:String,#[serde(deserialize_with="producer")]producer:String,request_id:String,#[serde(deserialize_with="scope")]scope:String,project_id:String,#[serde(deserialize_with="status")]status:String,#[serde(deserialize_with="pending")]knowledge_status:String,#[serde(deserialize_with="no")]published:bool,#[serde(deserialize_with="science")]scientific_quality:String,#[serde(deserialize_with="research")]research_status:String,#[serde(deserialize_with="zero")]network_calls:usize,#[serde(deserialize_with="zero")]paid_calls:usize,canonical_concepts:Vec<CanonicalConcept>,inputs:Vec<CoordinationInput>,occurrences:Vec<Occurrence>,facts:Vec<Fact>,relations:Vec<Relation>,totals:CoordinationTotals,artifacts:BTreeMap<String,Fingerprint>});
dto!(CoordinationIr{#[serde(deserialize_with="one")]schema_version:u32,#[serde(deserialize_with="producer")]producer:String,#[serde(deserialize_with="one")]revision:u32,request_id:String,#[serde(deserialize_with="scope")]scope:String,project_id:String,#[serde(deserialize_with="pending")]knowledge_status:String,#[serde(deserialize_with="no")]published:bool,#[serde(deserialize_with="science")]scientific_quality:String,#[serde(deserialize_with="research")]research_status:String,canonical_concepts:Vec<CanonicalConcept>,inputs:Vec<CoordinationInput>,occurrences:Vec<Occurrence>,facts:Vec<Fact>,relations:Vec<Relation>,totals:CoordinationTotals});
