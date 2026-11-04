//! Strict Schema2 exchange; source-review DTOs and Schema1 remain unchanged.
use crate::knowledge_coordination_model::{
    AtomMapping, CanonicalConcept, ConceptMapping, CoordinationInput, InputSpec,
};
use crate::{
    extraction_model::{CandidateAtom, CandidateEvidence, RequestUnit, SourceBinding},
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
fixed!(two, u32, 2);
fixed!(purpose, String, "explicit_identity_condition_time");
fixed!(producer, String, "knowledge_coordination");
fixed!(scope, String, "explicit_identity_condition_time_v2");
fixed!(pending, String, "pending");
fixed!(science, String, "not_run");
fixed!(research, String, "not_requested");
fixed!(no, bool, false);
fixed!(zero, usize, 0);
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
        "duplicate"
            | "complement"
            | "separate_object"
            | "needs_review"
            | "condition_variant"
            | "historical_evolution"
    ) {
        Ok(s)
    } else {
        Err(serde::de::Error::custom("invalid relation"))
    }
}
macro_rules! dto {($n:ident{$($(#[$a:meta])*$f:ident:$t:ty),*$(,)?})=>{#[derive(Debug,Clone,Serialize,Deserialize)]#[serde(deny_unknown_fields)]pub(crate) struct $n{$($(#[$a])*pub(crate) $f:$t),*}};}
dto!(CoordinationCase {
    #[serde(deserialize_with = "two")]
    schema_version: u32,
    #[serde(deserialize_with = "purpose")]
    purpose: String,
    project_id: String,
    identity_file: String,
    inputs: Vec<InputSpec>,
});
dto!(IdentityMap {
    #[serde(deserialize_with = "two")]
    schema_version: u32,
    author: String,
    concepts: Vec<CanonicalConcept>,
    concept_mappings: Vec<ConceptMapping>,
    atom_mappings: Vec<AtomMapping>,
    atom_contexts: Vec<AtomContext>,
});
dto!(Occurrence {
    context: Context,
    occurrence_id: String,
    input_id: String,
    binding: SourceBinding,
    unit: RequestUnit,
    original_concepts: Vec<Concept>,
    original_atom: CandidateAtom,
    original_evidence: Vec<CandidateEvidence>,
    canonical_concept_ids: Vec<String>,
    object_id: String,
    property_id: String,
    value: String,
    unit_value: String,
    conditions: Vec<String>,
    exceptions: Vec<String>,
    #[serde(deserialize_with = "status")]
    status: String,
    issue_codes: Vec<String>,
});
dto!(Fact {
    #[serde(deserialize_with = "applicability")]
    applicability_status: String,
    time: Time,
    fact_id: String,
    object_id: String,
    property_id: String,
    #[serde(deserialize_with = "known")]
    value_status: String,
    value: String,
    unit: String,
    conditions: Vec<String>,
    exceptions: Vec<String>,
    occurrence_ids: Vec<String>,
    #[serde(deserialize_with = "status")]
    status: String,
    issue_codes: Vec<String>,
});
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
dto!(CoordinationTotals {
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
    totals: CoordinationTotals,
    #[serde(deserialize_with = "status")]
    status: String,
});
dto!(CoordinationReport {
    #[serde(deserialize_with = "two")]
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
    totals: CoordinationTotals,
    artifacts: BTreeMap<String, Fingerprint>,
});
dto!(CoordinationIr {
    #[serde(deserialize_with = "two")]
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
    totals: CoordinationTotals,
});

fn applicability<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if matches!(s.as_str(), "known" | "unknown" | "not_applicable") {
        Ok(s)
    } else {
        Err(serde::de::Error::custom("invalid applicability"))
    }
}
fn time_status<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if matches!(s.as_str(), "known" | "unknown") {
        Ok(s)
    } else {
        Err(serde::de::Error::custom("invalid time status"))
    }
}
fn precision<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let s = Option::<String>::deserialize(d)?;
    if s.as_deref().is_none_or(|s| matches!(s, "year" | "day")) {
        Ok(s)
    } else {
        Err(serde::de::Error::custom("invalid time precision"))
    }
}
dto!(Time {
    #[serde(deserialize_with = "time_status")]
    status: String,
    #[serde(deserialize_with = "precision")]
    precision: Option<String>,
    #[serde(deserialize_with = "nullable")]
    value: Option<String>,
});
dto!(AtomContext {
    input_id: String,
    original_atom_id: String,
    #[serde(deserialize_with = "applicability")]
    applicability_status: String,
    time: Time,
    context_evidence_ids: Vec<String>,
});
dto!(Context {
    #[serde(deserialize_with = "applicability")]
    applicability_status: String,
    time: Time,
    evidence_ids: Vec<String>,
    unit_id: String,
    start_byte: usize,
    end_byte: usize,
    quote: String,
    sha256: String,
});
