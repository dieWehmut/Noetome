//! Complete context keys, bounded pair classifications and actual denominators.
use crate::{
    extraction_review::normalized,
    knowledge_coordinate_context::{order, rebuild},
    knowledge_coordinate_facts::{build_occurrences, sorted, stable_id},
    knowledge_coordinate_input::{VerifiedReview, err},
    knowledge_coordination_model::IdentityMap,
    knowledge_coordination_v2_model::*,
    project::ProjectError,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
fn key(o: &Occurrence) -> Value {
    json!([
        "coordination_v2",
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
pub(crate) fn build_context_occurrences(
    map: &IdentityMap,
    contexts: &[AtomContext],
    reviews: &[VerifiedReview],
) -> Result<
    (
        Vec<crate::knowledge_coordination_model::CanonicalConcept>,
        Vec<crate::knowledge_coordination_model::CoordinationInput>,
        Vec<Occurrence>,
    ),
    ProjectError,
> {
    let (canonical_concepts, inputs, originals) = build_occurrences(map, reviews)?;
    let mut declarations = BTreeMap::new();
    if contexts.len() > 1024 {
        return Err(err("COORD_INPUT_LIMIT", 2, "too many contexts"));
    }
    for c in contexts {
        if declarations
            .insert((c.input_id.clone(), c.original_atom_id.clone()), c)
            .is_some()
        {
            return Err(err("COORD_CONTEXT", 3, "duplicate atom context"));
        }
    }
    let mut occurrences = vec![];
    for o in originals {
        let c = declarations
            .remove(&(o.input_id.clone(), o.original_atom.atom.atom_id.clone()))
            .ok_or_else(|| err("COORD_CONTEXT", 3, "missing atom context"))?;
        let r = reviews
            .iter()
            .find(|r| r.input.input_id == o.input_id)
            .ok_or_else(|| err("COORD_CONTEXT", 3, "unknown input"))?;
        let context = rebuild(r, c)?;
        occurrences.push(Occurrence {
            context,
            occurrence_id: o.occurrence_id,
            input_id: o.input_id,
            binding: o.binding,
            unit: o.unit,
            original_concepts: o.original_concepts,
            original_atom: o.original_atom,
            original_evidence: o.original_evidence,
            canonical_concept_ids: o.canonical_concept_ids,
            object_id: o.object_id,
            property_id: o.property_id,
            value: o.value,
            unit_value: o.unit_value,
            conditions: o.conditions,
            exceptions: o.exceptions,
            status: o.status,
            issue_codes: o.issue_codes,
        });
    }
    if !declarations.is_empty() {
        return Err(err("COORD_CONTEXT", 3, "unused atom context"));
    }
    Ok((canonical_concepts, inputs, occurrences))
}
pub(crate) fn coordinate(
    map: &IdentityMap,
    contexts: &[AtomContext],
    reviews: &[VerifiedReview],
) -> Result<CoordinatedFacts, ProjectError> {
    let (canonical_concepts, inputs, mut occurrences) =
        build_context_occurrences(map, contexts, reviews)?;
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
            let (relation, issue) = if a.context.applicability_status == "unknown"
                || b.context.applicability_status == "unknown"
            {
                ("needs_review", Some("COORD_UNKNOWN_CONDITION"))
            } else if different_version && temporal.is_none() {
                ("needs_review", Some("COORD_UNHANDLED_VERSION"))
            } else if a.object_id != b.object_id {
                ("separate_object", None)
            } else if a.property_id != b.property_id {
                ("complement", None)
            } else if normalized(&a.unit_value) != normalized(&b.unit_value) {
                ("needs_review", Some("COORD_UNIT_NOT_COMPARABLE"))
            } else if keys[i] == keys[j] {
                ("duplicate", None)
            } else if applicability(a) != applicability(b) {
                ("condition_variant", None)
            } else if temporal.is_some() {
                ("historical_evolution", None)
            } else if json!([
                a.context.time.status,
                a.context.time.precision,
                a.context.time.value
            ]) != json!([
                b.context.time.status,
                b.context.time.precision,
                b.context.time.value
            ]) {
                ("needs_review", Some("COORD_TIME_NOT_COMPARABLE"))
            } else {
                ("needs_review", Some("COORD_UNHANDLED_DIFFERENCE"))
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
        let id = stable_id("fact-v2-", &key(o))?;
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
    let totals = CoordinationTotals {
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
        evidence_links: occurrences.iter().map(|o| o.original_evidence.len()).sum(),
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
        totals,
        status: if needs_review == 0 {
            "passed"
        } else {
            "needs_review"
        }
        .into(),
    })
}
