//! Source-backed conflict classification without voting, replacement or scientific adjudication.
use crate::{
    extraction_review::normalized,
    fixture_model::EvidenceRelation,
    knowledge_coordinate_context::order,
    knowledge_coordinate_facts::{sorted, stable_id},
    knowledge_coordinate_input::{VerifiedReview, err},
    knowledge_coordinate_variants::build_context_occurrences,
    knowledge_coordination_model::IdentityMap,
    knowledge_coordination_v3_model::*,
    project::ProjectError,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
fn key(o: &Occurrence) -> Value {
    json!([
        "coordination_v3",
        o.object_id,
        o.property_id,
        normalized(&o.value),
        normalized(&o.unit_value),
        o.context.applicability_status,
        sorted(&o.conditions),
        sorted(&o.exceptions),
        [
            o.context.time.status,
            o.context.time.precision,
            o.context.time.value
        ],
        if o.context.applicability_status == "unknown" {
            Some(&o.occurrence_id)
        } else {
            None
        }
    ])
}
// Finite ASCII decimals are canonicalized only for comparison; original facts stay lexical.
fn decimal(value: &str) -> Option<(bool, String, String)> {
    let value = normalized(value);
    if value.len() > 1024 {
        return None;
    }
    let negative = value.starts_with('-');
    let unsigned = value.strip_prefix(['+', '-']).unwrap_or(&value);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || (unsigned.contains('.') && fraction.is_empty())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole }.to_owned();
    let fraction = fraction.trim_end_matches('0').to_owned();
    Some((
        negative && (whole != "0" || !fraction.is_empty()),
        whole,
        fraction,
    ))
}
pub(crate) fn coordinate(
    map: &IdentityMap,
    contexts: &[AtomContext],
    reviews: &[VerifiedReview],
) -> Result<CoordinatedFacts, ProjectError> {
    let (canonical_concepts, inputs, mut occurrences) =
        build_context_occurrences(map, contexts, reviews)?;
    // v1/v2 report only supports. Schema3 retains every linked source excerpt,
    // including refutes and qualifies, in the original atom's link order.
    for o in &mut occurrences {
        let r = reviews
            .iter()
            .find(|r| r.input.input_id == o.input_id)
            .ok_or_else(|| err("COORD_SOURCE_REVIEW", 3, "missing original input"))?;
        let mut seen = BTreeSet::new();
        o.original_evidence = o
            .original_atom
            .atom
            .evidence_links
            .iter()
            .filter(|l| seen.insert(l.evidence_id.clone()))
            .map(|l| {
                r.candidate
                    .payload
                    .evidence
                    .iter()
                    .find(|e| e.evidence_id == l.evidence_id)
                    .cloned()
                    .ok_or_else(|| {
                        err("COORD_SOURCE_REVIEW", 3, "missing original linked evidence")
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
    }
    let keys = occurrences.iter().map(key).collect::<Vec<_>>();
    let mut issues: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for o in &occurrences {
        if o.context.applicability_status == "unknown" {
            issues
                .entry(o.occurrence_id.clone())
                .or_default()
                .insert("COORD_UNKNOWN_CONDITION".into());
        }
    }
    let mut relations = vec![];
    for i in 0..occurrences.len() {
        for j in i + 1..occurrences.len() {
            let (a, b) = (&occurrences[i], &occurrences[j]);
            let temporal = order(&a.context.time, &b.context.time)?;
            let different_version = a.binding.source_id == b.binding.source_id
                && (a.binding.version_id != b.binding.version_id
                    || a.binding.parse_id != b.binding.parse_id);
            let applicability = |o: &Occurrence| {
                json!([
                    o.context.applicability_status,
                    sorted(&o.conditions),
                    sorted(&o.exceptions)
                ])
            };
            let (relation, issue) = if a.object_id != b.object_id {
                ("separate_object", None)
            } else if a.property_id != b.property_id {
                ("complement", None)
            } else if a.context.applicability_status == "unknown"
                || b.context.applicability_status == "unknown"
            {
                ("insufficient_evidence", Some("COORD_UNKNOWN_CONDITION"))
            } else if different_version && temporal.is_none() {
                ("insufficient_evidence", Some("COORD_UNHANDLED_VERSION"))
            } else if normalized(&a.unit_value) != normalized(&b.unit_value) {
                ("insufficient_evidence", Some("COORD_UNIT_NOT_COMPARABLE"))
            } else if keys[i] == keys[j] {
                ("duplicate", None)
            } else if applicability(a) != applicability(b) {
                ("condition_variant", None)
            } else if temporal.is_some() {
                ("historical_evolution", None)
            } else if a.context.time.status == "unknown"
                || b.context.time.status == "unknown"
                || json!([
                    a.context.time.status,
                    a.context.time.precision,
                    a.context.time.value
                ]) != json!([
                    b.context.time.status,
                    b.context.time.precision,
                    b.context.time.value
                ])
            {
                ("insufficient_evidence", Some("COORD_TIME_NOT_COMPARABLE"))
            } else {
                match (decimal(&a.value), decimal(&b.value)) {
                    (Some(a), Some(b)) if a == b => ("numeric_equivalent", None),
                    (Some(_), Some(_)) => ("true_conflict", Some("COORD_TRUE_CONFLICT")),
                    _ => ("insufficient_evidence", Some("COORD_VALUE_NOT_COMPARABLE")),
                }
            };
            if let Some(code) = issue {
                for o in [a, b] {
                    issues
                        .entry(o.occurrence_id.clone())
                        .or_default()
                        .insert(code.into());
                }
            }
            let (earlier, later) = if relation == "historical_evolution" {
                if temporal == Some(true) {
                    (Some(a.occurrence_id.clone()), Some(b.occurrence_id.clone()))
                } else {
                    (Some(b.occurrence_id.clone()), Some(a.occurrence_id.clone()))
                }
            } else {
                (None, None)
            };
            relations.push(Relation {
                left_occurrence_id: a.occurrence_id.clone(),
                right_occurrence_id: b.occurrence_id.clone(),
                relation: relation.into(),
                issue_code: issue.map(str::to_owned),
                earlier_occurrence_id: earlier,
                later_occurrence_id: later,
            });
        }
    }
    let mut groups: BTreeMap<String, Fact> = BTreeMap::new();
    for o in &mut occurrences {
        if let Some(codes) = issues.get(&o.occurrence_id) {
            o.status = "needs_review".into();
            o.issue_codes = codes.iter().cloned().collect();
        }
        let id = stable_id("fact-v3-", &key(o))?;
        let f = groups.entry(id.clone()).or_insert_with(|| Fact {
            fact_id: id,
            object_id: o.object_id.clone(),
            property_id: o.property_id.clone(),
            value_status: "known".into(),
            value: normalized(&o.value),
            unit: normalized(&o.unit_value),
            conditions: sorted(&o.conditions),
            exceptions: sorted(&o.exceptions),
            applicability_status: o.context.applicability_status.clone(),
            time: o.context.time.clone(),
            occurrence_ids: vec![],
            status: "passed".into(),
            issue_codes: vec![],
        });
        f.occurrence_ids.push(o.occurrence_id.clone());
        if o.status == "needs_review" {
            f.status = o.status.clone();
            f.issue_codes.extend(o.issue_codes.clone());
            f.issue_codes.sort();
            f.issue_codes.dedup();
        }
    }
    let n = occurrences.len();
    let retained = groups.len();
    let needs_review = issues.len();
    let conflicting_ids = relations
        .iter()
        .filter(|r| r.relation == "true_conflict")
        .flat_map(|r| [&r.left_occurrence_id, &r.right_occurrence_id])
        .collect::<BTreeSet<_>>();
    let mut insufficient_ids = relations
        .iter()
        .filter(|r| r.relation == "insufficient_evidence")
        .flat_map(|r| [&r.left_occurrence_id, &r.right_occurrence_id])
        .collect::<BTreeSet<_>>();
    insufficient_ids.extend(
        occurrences
            .iter()
            .filter(|o| o.context.applicability_status == "unknown")
            .map(|o| &o.occurrence_id),
    );
    let processing_records = occurrences
        .iter()
        .map(|o| ProcessingRecord {
            occurrence_id: o.occurrence_id.clone(),
            object_id: o.object_id.clone(),
            property_id: o.property_id.clone(),
            status: o.status.clone(),
            issue_codes: o.issue_codes.clone(),
            action: if o.status == "passed" {
                "retained"
            } else {
                "retain_for_review"
            }
            .into(),
            reason: if o.status == "passed" {
                "retained with complete source provenance".into()
            } else {
                o.issue_codes.join("; ")
            },
        })
        .collect();
    let totals = CoordinationTotals {
        true_conflicts: relations
            .iter()
            .filter(|r| r.relation == "true_conflict")
            .count(),
        insufficient_evidence_relations: relations
            .iter()
            .filter(|r| r.relation == "insufficient_evidence")
            .count(),
        conflicting_occurrences: conflicting_ids.len(),
        insufficient_evidence_occurrences: insufficient_ids.len(),
        counter_evidence_links: occurrences
            .iter()
            .map(|o| {
                o.original_atom
                    .atom
                    .evidence_links
                    .iter()
                    .filter(|l| l.relation == EvidenceRelation::Refutes)
                    .count()
            })
            .sum(),
        input_bundles: reviews.len(),
        validated_input_bundles: reviews.len(),
        input_atoms: reviews
            .iter()
            .map(|r| r.candidate.payload.atoms.len())
            .sum(),
        retained_fact_groups: retained,
        canonical_facts: if needs_review == 0 {
            Some(retained)
        } else {
            None
        },
        source_occurrences: n,
        evidence_links: occurrences
            .iter()
            .map(|o| o.original_atom.atom.evidence_links.len())
            .sum(),
        independent_sources: occurrences
            .iter()
            .map(|o| &o.binding.source_id)
            .collect::<BTreeSet<_>>()
            .len(),
        duplicate_occurrences: n - retained,
        passed: n - needs_review,
        failed: 0,
        needs_review,
        unprocessed: 0,
        condition_variants: relations
            .iter()
            .filter(|r| r.relation == "condition_variant")
            .count(),
        historical_relations: relations
            .iter()
            .filter(|r| r.relation == "historical_evolution")
            .count(),
        known_condition_occurrences: occurrences
            .iter()
            .filter(|o| o.context.applicability_status == "known")
            .count(),
        unknown_condition_occurrences: occurrences
            .iter()
            .filter(|o| o.context.applicability_status == "unknown")
            .count(),
        not_applicable_condition_occurrences: occurrences
            .iter()
            .filter(|o| o.context.applicability_status == "not_applicable")
            .count(),
        dated_occurrences: occurrences
            .iter()
            .filter(|o| o.context.time.status == "known")
            .count(),
        undated_occurrences: occurrences
            .iter()
            .filter(|o| o.context.time.status == "unknown")
            .count(),
        context_evidence_links: occurrences
            .iter()
            .map(|o| o.context.evidence_ids.len())
            .sum(),
    };
    Ok(CoordinatedFacts {
        canonical_concepts,
        inputs,
        occurrences,
        facts: groups.into_values().collect(),
        relations,
        processing_records,
        totals,
        status: if needs_review == 0 {
            "passed"
        } else {
            "needs_review"
        }
        .into(),
    })
}
