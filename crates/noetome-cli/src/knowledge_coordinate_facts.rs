//! Explicit component identities and complete-key grouping, retaining every source occurrence.
use crate::{
    extraction_review::normalized,
    fixture_model::EvidenceRelation,
    knowledge_coordinate_input::{VerifiedReview, err, plain, uuid},
    knowledge_coordination_model::*,
    project::ProjectError,
    text::sha256,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) fn sorted(v: &[String]) -> Vec<String> {
    let mut v: Vec<_> = v.iter().map(|s| normalized(s)).collect();
    v.sort();
    v
}
fn fact_key(o: &Occurrence) -> Value {
    json!([
        o.object_id,
        o.property_id,
        normalized(&o.value),
        normalized(&o.unit_value),
        sorted(&o.conditions),
        sorted(&o.exceptions)
    ])
}
pub(crate) fn stable_id(prefix: &str, key: &Value) -> Result<String, ProjectError> {
    Ok(format!(
        "{prefix}{}",
        sha256(&serde_json::to_vec(key).map_err(|e| err("COORD_SCHEMA", 2, e))?)
    ))
}
pub(crate) fn build_occurrences(
    map: &IdentityMap,
    reviews: &[VerifiedReview],
) -> Result<
    (
        Vec<CanonicalConcept>,
        Vec<CoordinationInput>,
        Vec<Occurrence>,
    ),
    ProjectError,
> {
    let bad = || {
        err(
            "COORD_IDENTITY",
            2,
            "explicit identity mapping missing, duplicate, unknown or wrong component role",
        )
    };
    let mut canonical = BTreeSet::new();
    for c in &map.concepts {
        if !uuid(&c.canonical_id)
            || !plain(&c.display_name)
            || !canonical.insert(c.canonical_id.clone())
        {
            return Err(bad());
        }
    }
    let mut mappings = BTreeMap::new();
    for m in &map.concept_mappings {
        if !plain(&m.original_concept_id)
            || !canonical.contains(&m.canonical_id)
            || mappings
                .insert(
                    (m.input_id.clone(), m.original_concept_id.clone()),
                    m.canonical_id.clone(),
                )
                .is_some()
        {
            return Err(bad());
        }
    }
    let mut atoms = BTreeMap::new();
    for m in &map.atom_mappings {
        if !plain(&m.original_atom_id)
            || !canonical.contains(&m.object_id)
            || !canonical.contains(&m.property_id)
            || m.object_id == m.property_id
            || atoms
                .insert((m.input_id.clone(), m.original_atom_id.clone()), m)
                .is_some()
        {
            return Err(bad());
        }
    }
    let mut used_mappings = BTreeSet::new();
    let mut used_atoms = BTreeSet::new();
    let mut used_canonical = BTreeSet::new();
    let mut occurrences = vec![];
    let mut inputs = vec![];
    for r in reviews {
        let id = &r.input.input_id;
        let a = &r.candidate.payload.atoms[0];
        let atom_key = (id.clone(), a.atom.atom_id.clone());
        let m = atoms.get(&atom_key).ok_or_else(bad)?;
        used_atoms.insert(atom_key);
        for c in &r.candidate.payload.concepts {
            let key = (id.clone(), c.concept_id.clone());
            let canonical = mappings.get(&key).ok_or_else(bad)?;
            used_mappings.insert(key);
            used_canonical.insert(canonical.clone());
        }
        let mut refs = BTreeSet::new();
        let mut object_refs = BTreeSet::new();
        let mut property_refs = BTreeSet::new();
        for cid in &a.atom.concept_ids {
            let original = r
                .candidate
                .payload
                .concepts
                .iter()
                .find(|c| &c.concept_id == cid)
                .ok_or_else(bad)?;
            let mapped = mappings.get(&(id.clone(), cid.clone())).ok_or_else(bad)?;
            refs.insert(mapped.clone());
            let matches = |s: &str| {
                std::iter::once(&original.canonical_name)
                    .chain(original.aliases.iter())
                    .any(|n| normalized(n) == normalized(s))
            };
            if matches(&r.review.subject) {
                object_refs.insert(mapped.clone());
            }
            if matches(&r.review.property) {
                property_refs.insert(mapped.clone());
            }
        }
        if !object_refs.contains(&m.object_id)
            || !property_refs.contains(&m.property_id)
            || object_refs.len() != 1
            || property_refs.len() != 1
        {
            return Err(bad());
        }
        let unit = &r.source.units[0];
        let b = &r.source.binding;
        let occurrence_id = stable_id(
            "occ-",
            &json!([
                b.project_id,
                b.source_id,
                b.version_id,
                b.parse_id,
                unit.unit_id,
                unit.start_byte,
                unit.end_byte,
                a.atom.atom_id
            ]),
        )?;
        let original_evidence = a
            .atom
            .evidence_links
            .iter()
            .filter(|l| l.relation == EvidenceRelation::Supports)
            .map(|l| {
                r.candidate
                    .payload
                    .evidence
                    .iter()
                    .find(|e| e.evidence_id == l.evidence_id)
                    .cloned()
                    .ok_or_else(bad)
            })
            .collect::<Result<Vec<_>, _>>()?;
        occurrences.push(Occurrence {
            occurrence_id,
            input_id: id.clone(),
            binding: b.clone(),
            unit: unit.clone(),
            original_concepts: r.candidate.payload.concepts.clone(),
            original_atom: a.clone(),
            original_evidence,
            canonical_concept_ids: refs.into_iter().collect(),
            object_id: m.object_id.clone(),
            property_id: m.property_id.clone(),
            value: r.gold.value.clone().ok_or_else(bad)?,
            unit_value: r.gold.unit.clone().ok_or_else(bad)?,
            conditions: r.gold.conditions.clone(),
            exceptions: r.gold.exceptions.clone(),
            status: "passed".into(),
            issue_codes: vec![],
        });
        inputs.push(CoordinationInput {
            input_id: id.clone(),
            binding: b.clone(),
            request_id: r.candidate.request_id.clone(),
            source_review_status: "passed".into(),
            source_review_checks: r.checks.clone(),
            source_bundle_fingerprints: r.input.artifacts.clone(),
            copied_artifacts: r
                .files
                .keys()
                .map(|n| (n.clone(), format!("input-{id}--{n}")))
                .collect(),
        });
    }
    if used_mappings.len() != mappings.len()
        || used_atoms.len() != atoms.len()
        || used_canonical != canonical
    {
        return Err(bad());
    }
    occurrences.sort_by(|a, b| a.occurrence_id.cmp(&b.occurrence_id));
    inputs.sort_by(|a, b| a.input_id.cmp(&b.input_id));
    let mut canonical_concepts = map.concepts.clone();
    canonical_concepts.sort_by(|a, b| a.canonical_id.cmp(&b.canonical_id));
    Ok((canonical_concepts, inputs, occurrences))
}
pub(crate) fn coordinate(
    map: &IdentityMap,
    reviews: &[VerifiedReview],
) -> Result<CoordinatedFacts, ProjectError> {
    let (canonical_concepts, inputs, mut occurrences) = build_occurrences(map, reviews)?;
    let keys = occurrences.iter().map(fact_key).collect::<Vec<_>>();
    let mut relations = vec![];
    let mut review_ids = BTreeSet::new();
    for i in 0..occurrences.len() {
        for j in i + 1..occurrences.len() {
            let (a, b) = (&occurrences[i], &occurrences[j]);
            let different_version = a.binding.source_id == b.binding.source_id
                && (a.binding.version_id != b.binding.version_id
                    || a.binding.parse_id != b.binding.parse_id);
            let relation = if different_version {
                "needs_review"
            } else if a.object_id != b.object_id {
                "separate_object"
            } else if a.property_id != b.property_id {
                "complement"
            } else if keys[i] == keys[j] {
                "duplicate"
            } else {
                "needs_review"
            };
            let issue = if relation == "needs_review" {
                review_ids.insert(a.occurrence_id.clone());
                review_ids.insert(b.occurrence_id.clone());
                Some("COORD_UNHANDLED_DIFFERENCE".into())
            } else {
                None
            };
            relations.push(Relation {
                left_occurrence_id: a.occurrence_id.clone(),
                right_occurrence_id: b.occurrence_id.clone(),
                relation: relation.into(),
                issue_code: issue,
            });
        }
    }
    let mut groups: BTreeMap<String, Fact> = BTreeMap::new();
    for o in &mut occurrences {
        if review_ids.contains(&o.occurrence_id) {
            o.status = "needs_review".into();
            o.issue_codes = vec!["COORD_UNHANDLED_DIFFERENCE".into()];
        }
        let id = stable_id("fact-", &fact_key(o))?;
        let f = groups.entry(id.clone()).or_insert_with(|| Fact {
            fact_id: id,
            object_id: o.object_id.clone(),
            property_id: o.property_id.clone(),
            value_status: "known".into(),
            value: normalized(&o.value),
            unit: normalized(&o.unit_value),
            conditions: sorted(&o.conditions),
            exceptions: sorted(&o.exceptions),
            occurrence_ids: vec![],
            status: "passed".into(),
            issue_codes: vec![],
        });
        f.occurrence_ids.push(o.occurrence_id.clone());
        if o.status == "needs_review" {
            f.status = o.status.clone();
            f.issue_codes = o.issue_codes.clone();
        }
    }
    let needs_review = review_ids.len();
    let n = occurrences.len();
    let retained = groups.len();
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
