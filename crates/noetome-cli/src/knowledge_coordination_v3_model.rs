//! Strict Schema3 conflict exchange; old coordination and source-review DTOs stay unchanged.
use crate::knowledge_coordination_model::{
    AtomMapping, CanonicalConcept, ConceptMapping, CoordinationInput, InputSpec,
};
use crate::sidecar_model::Fingerprint;
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
fixed!(three, u32, 3);
fixed!(purpose, String, "explicit_identity_conflict_review");
fixed!(producer, String, "knowledge_coordination");
fixed!(scope, String, "explicit_identity_conflict_review_v3");
fixed!(pending, String, "pending");
fixed!(science, String, "not_run");
fixed!(research, String, "not_requested");
fixed!(no, bool, false);
fixed!(zero, usize, 0);
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
        "duplicate"
            | "complement"
            | "separate_object"
            | "condition_variant"
            | "historical_evolution"
            | "true_conflict"
            | "insufficient_evidence"
            | "numeric_equivalent"
    ) {
        Ok(s)
    } else {
        Err(serde::de::Error::custom("invalid relation"))
    }
}
macro_rules! dto {($n:ident{$($(#[$a:meta])*$f:ident:$t:ty),*$(,)?})=>{#[derive(Debug,Clone,Serialize,Deserialize)]#[serde(deny_unknown_fields)]pub(crate) struct $n{$($(#[$a])*pub(crate) $f:$t),*}};}
dto!(CoordinationCase {
    #[serde(deserialize_with = "three")]
    schema_version: u32,
    #[serde(deserialize_with = "purpose")]
    purpose: String,
    project_id: String,
    identity_file: String,
    inputs: Vec<InputSpec>,
});
dto!(IdentityMap {
    #[serde(deserialize_with = "three")]
    schema_version: u32,
    author: String,
    concepts: Vec<CanonicalConcept>,
    concept_mappings: Vec<ConceptMapping>,
    atom_mappings: Vec<AtomMapping>,
    atom_contexts: Vec<AtomContext>,
});
// Context and occurrence fields retain precisely the validated v2 structure.
pub(crate) use crate::knowledge_coordination_v2_model::{AtomContext, Fact, Occurrence};
dto!(Relation {
    #[serde(deserialize_with = "nullable")]
    earlier_occurrence_id: Option<String>,
    #[serde(deserialize_with = "nullable")]
    later_occurrence_id: Option<String>,
    left_occurrence_id: String,
    right_occurrence_id: String,
    #[serde(deserialize_with = "relation")]
    relation: String,
    #[serde(deserialize_with = "nullable")]
    issue_code: Option<String>,
});
dto!(ProcessingRecord {
    occurrence_id: String,
    object_id: String,
    property_id: String,
    #[serde(deserialize_with = "status")]
    status: String,
    issue_codes: Vec<String>,
    #[serde(deserialize_with = "action")]
    action: String,
    reason: String,
});
fn action<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if matches!(s.as_str(), "retained" | "retain_for_review") {
        Ok(s)
    } else {
        Err(serde::de::Error::custom("invalid processing action"))
    }
}
dto!(CoordinationTotals {
    true_conflicts: usize,
    insufficient_evidence_relations: usize,
    conflicting_occurrences: usize,
    insufficient_evidence_occurrences: usize,
    counter_evidence_links: usize,
    condition_variants: usize,
    historical_relations: usize,
    known_condition_occurrences: usize,
    unknown_condition_occurrences: usize,
    not_applicable_condition_occurrences: usize,
    dated_occurrences: usize,
    undated_occurrences: usize,
    context_evidence_links: usize,
    input_bundles: usize,
    validated_input_bundles: usize,
    input_atoms: usize,
    retained_fact_groups: usize,
    #[serde(deserialize_with = "nullable")]
    canonical_facts: Option<usize>,
    source_occurrences: usize,
    evidence_links: usize,
    independent_sources: usize,
    duplicate_occurrences: usize,
    passed: usize,
    failed: usize,
    needs_review: usize,
    unprocessed: usize,
});
dto!(CoordinatedFacts {
    canonical_concepts: Vec<CanonicalConcept>,
    inputs: Vec<CoordinationInput>,
    occurrences: Vec<Occurrence>,
    facts: Vec<Fact>,
    relations: Vec<Relation>,
    processing_records: Vec<ProcessingRecord>,
    totals: CoordinationTotals,
    #[serde(deserialize_with = "status")]
    status: String,
});
dto!(CoordinationReport {
    #[serde(deserialize_with = "three")]
    schema_version: u32,
    #[serde(deserialize_with = "producer")]
    report_kind: String,
    #[serde(deserialize_with = "producer")]
    producer: String,
    request_id: String,
    #[serde(deserialize_with = "scope")]
    scope: String,
    project_id: String,
    #[serde(deserialize_with = "status")]
    status: String,
    #[serde(deserialize_with = "pending")]
    knowledge_status: String,
    #[serde(deserialize_with = "no")]
    published: bool,
    #[serde(deserialize_with = "science")]
    scientific_quality: String,
    #[serde(deserialize_with = "research")]
    research_status: String,
    #[serde(deserialize_with = "zero")]
    network_calls: usize,
    #[serde(deserialize_with = "zero")]
    paid_calls: usize,
    canonical_concepts: Vec<CanonicalConcept>,
    inputs: Vec<CoordinationInput>,
    occurrences: Vec<Occurrence>,
    facts: Vec<Fact>,
    relations: Vec<Relation>,
    processing_records: Vec<ProcessingRecord>,
    totals: CoordinationTotals,
    artifacts: BTreeMap<String, Fingerprint>,
});
dto!(CoordinationIr {
    #[serde(deserialize_with = "three")]
    schema_version: u32,
    #[serde(deserialize_with = "producer")]
    producer: String,
    #[serde(deserialize_with = "one")]
    revision: u32,
    request_id: String,
    #[serde(deserialize_with = "scope")]
    scope: String,
    project_id: String,
    #[serde(deserialize_with = "pending")]
    knowledge_status: String,
    #[serde(deserialize_with = "no")]
    published: bool,
    #[serde(deserialize_with = "science")]
    scientific_quality: String,
    #[serde(deserialize_with = "research")]
    research_status: String,
    canonical_concepts: Vec<CanonicalConcept>,
    inputs: Vec<CoordinationInput>,
    occurrences: Vec<Occurrence>,
    facts: Vec<Fact>,
    relations: Vec<Relation>,
    processing_records: Vec<ProcessingRecord>,
    totals: CoordinationTotals,
});
