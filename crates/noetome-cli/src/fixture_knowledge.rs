//! Fixture knowledge and source-position accounting; no scientific truth claim.

use std::collections::{BTreeMap, BTreeSet};

use crate::fixture_input::LoadedCase;
use crate::fixture_model::{
    CoverageDisposition, CoverageRecord, CoverageStage, DecisionSource, Evidence, EvidenceRelation,
    Issue, ValueStatus, Verification, is_id, is_sha, nonempty,
};
use crate::text::{ParseStatus, SpanKind, sha256};

fn add(issues: &mut Vec<Issue>, rule: &str, refs: &[&str], message: &str) {
    issues.push(Issue::new(
        rule,
        refs.iter().map(|reference| (*reference).into()).collect(),
        message,
    ));
}

fn id_counts<'a>(ids: impl Iterator<Item = &'a str>) -> BTreeMap<&'a str, usize> {
    let mut counts = BTreeMap::new();
    for id in ids {
        *counts.entry(id).or_insert(0) += 1;
    }
    counts
}

fn excerpt<'a>(case: &'a LoadedCase, evidence: &Evidence) -> Option<&'a str> {
    if evidence.excerpt_start_byte >= evidence.excerpt_end_byte {
        return None;
    }
    case.spans
        .get(&evidence.source_span_id)?
        .text
        .get(evidence.excerpt_start_byte..evidence.excerpt_end_byte)
}

fn bound_span(case: &LoadedCase, evidence: &Evidence) -> bool {
    let Some(span) = case.spans.get(&evidence.source_span_id) else {
        return false;
    };
    case.ir.sources.iter().any(|source| {
        source.source_id == span.source_id
            && source.version_id == span.version_id
            && case.manifest.sources.iter().any(|declared| {
                source.source_id == declared.source_id
                    && source.version_id == declared.version_id
                    && source.content_sha256 == declared.content_sha256
                    && source.normalized_sha256 == declared.normalized_sha256
            })
    })
}

fn valid_evidence<'a>(case: &'a LoadedCase) -> BTreeMap<&'a str, &'a Evidence> {
    let counts = id_counts(
        case.ir
            .evidence
            .iter()
            .map(|entry| entry.evidence_id.as_str()),
    );
    case.ir
        .evidence
        .iter()
        .filter(|entry| {
            is_id(&entry.evidence_id)
                && counts.get(entry.evidence_id.as_str()) == Some(&1)
                && bound_span(case, entry)
                && is_sha(&entry.excerpt_sha256)
                && excerpt(case, entry)
                    .is_some_and(|text| sha256(text.as_bytes()) == entry.excerpt_sha256)
        })
        .map(|entry| (entry.evidence_id.as_str(), entry))
        .collect()
}

/// Preserve historical diagnostics while delegating structural rules to domain.
pub(crate) fn validate_basics<'a>(
    concept_entries: &[crate::fixture_model::Concept],
    evidence_ids: impl Iterator<Item = &'a str>,
    atom_entries: &[&crate::fixture_model::KnowledgeAtom],
) -> Vec<Issue> {
    let mut issues = crate::domain::validate_basics(concept_entries, evidence_ids, atom_entries);
    for issue in &mut issues {
        if issue.rule_id == "KNOWLEDGE_ID" {
            issue.message = "实体ID必须是有效fixture标识且在同类清单中唯一。".into();
        }
    }
    issues
}

pub(crate) fn validate_knowledge(case: &LoadedCase) -> Vec<Issue> {
    let mut issues = Vec::new();
    let ir = &case.ir;
    if ir.revision == 0 {
        add(
            &mut issues,
            "KNOWLEDGE_REVISION",
            &[&ir.coverage_run_id],
            "IR修订号必须为正整数。",
        );
    }
    if !is_id(&ir.coverage_run_id) {
        add(
            &mut issues,
            "KNOWLEDGE_ID",
            &[&ir.coverage_run_id],
            "覆盖审计作用域必须为有效fixture标识。",
        );
    }

    let source_counts = id_counts(ir.sources.iter().map(|source| source.source_id.as_str()));
    let source_binding_matches = ir.sources.len() == case.manifest.sources.len()
        && source_counts.values().all(|count| *count == 1)
        && case.manifest.sources.iter().all(|declared| {
            ir.sources.iter().any(|source| {
                source.source_id == declared.source_id
                    && source.version_id == declared.version_id
                    && source.content_sha256 == declared.content_sha256
                    && source.normalized_sha256 == declared.normalized_sha256
            })
        });
    if !source_binding_matches {
        let refs: Vec<_> = ir
            .sources
            .iter()
            .map(|source| source.source_id.as_str())
            .collect();
        add(
            &mut issues,
            "KNOWLEDGE_SOURCE_BINDING",
            &refs,
            "IR来源集合及每份来源的版本、原始和规范SHA必须与manifest双向一致，不得重复。",
        );
    }

    issues.extend(validate_basics(
        &ir.concepts,
        ir.evidence.iter().map(|e| e.evidence_id.as_str()),
        &ir.atoms.iter().collect::<Vec<_>>(),
    ));
    for entry in &ir.evidence {
        let refs = [&entry.evidence_id[..], &entry.source_span_id[..]];
        if !is_id(&entry.source_span_id) || !bound_span(case, entry) {
            add(
                &mut issues,
                "KNOWLEDGE_EVIDENCE_SPAN",
                &refs,
                "证据必须引用实际Span及IR绑定的来源版本。",
            );
        }
        match excerpt(case, entry) {
            None => add(
                &mut issues,
                "KNOWLEDGE_EVIDENCE_RANGE",
                &refs,
                "证据范围必须是Span内非空且落在UTF8字符边界的字节半开区间。",
            ),
            Some(text) => {
                if !is_sha(&entry.excerpt_sha256) || sha256(text.as_bytes()) != entry.excerpt_sha256
                {
                    add(
                        &mut issues,
                        "KNOWLEDGE_EVIDENCE_SHA",
                        &refs,
                        "证据SHA与实际原文切片的UTF8字节不符。",
                    );
                }
            }
        }
    }
    let valid_evidence = valid_evidence(case);
    for atom in &ir.atoms {
        if atom.required {
            if atom.verification != Verification::FixtureReviewed
                || atom.value_status != ValueStatus::Known
            {
                add(
                    &mut issues,
                    "KNOWLEDGE_ATOM_REQUIRED_STATUS",
                    &[&atom.atom_id],
                    "必要原子必须标记为known和fixture_reviewed；该标记不证明科学正确性。",
                );
            }
            if !atom.evidence_links.iter().any(|link| {
                link.relation == EvidenceRelation::Supports
                    && valid_evidence.contains_key(link.evidence_id.as_str())
            }) {
                add(
                    &mut issues,
                    "KNOWLEDGE_ATOM_SUPPORT",
                    &[&atom.atom_id],
                    "必要原子至少需要一条范围、SHA与来源绑定均有效的supports证据，仅反证或限定证据不足。",
                );
            }
        }
    }
    issues
}

fn unique_record<'a>(
    index: &BTreeMap<&'a str, Vec<&'a CoverageRecord>>,
    id: &str,
) -> Option<&'a CoverageRecord> {
    let records = index.get(id)?;
    if records.len() == 1 {
        records.first().copied()
    } else {
        None
    }
}

fn validate_duplicate_chains<'a>(
    index: &BTreeMap<&'a str, Vec<&'a CoverageRecord>>,
    issues: &mut Vec<Issue>,
) {
    // Cache resolved tails and use an iterative walk; external input cannot grow
    // the call stack or cause every prefix of a long valid chain to be rescanned.
    let mut tails = BTreeMap::<&str, bool>::new();
    for records in index.values() {
        let Some(record) = records.first().copied().filter(|record| {
            records.len() == 1 && record.disposition == CoverageDisposition::Duplicate
        }) else {
            continue;
        };
        let mut path = Vec::new();
        let mut visited = BTreeSet::new();
        let mut cursor = record.span_id.as_str();
        let mut cycle = false;
        let resolved = loop {
            if let Some(resolved) = tails.get(cursor) {
                break *resolved;
            }
            if !visited.insert(cursor) {
                cycle = true;
                break false;
            }
            path.push(cursor);
            let Some(current) = unique_record(index, cursor) else {
                break false;
            };
            match current.disposition {
                CoverageDisposition::Included => break true,
                CoverageDisposition::Duplicate => match current.duplicate_of_span_id.as_deref() {
                    Some(target) => cursor = target,
                    None => break false,
                },
                _ => break false,
            }
        };
        if !resolved {
            if cycle {
                path.push(cursor);
                add(
                    issues,
                    "COVERAGE_DUPLICATE_CYCLE",
                    &path,
                    "重复归并引用不得自引用或形成循环。",
                );
            } else {
                add(
                    issues,
                    "COVERAGE_DUPLICATE_TERMINAL",
                    &[&record.span_id],
                    "每条重复链必须最终解析到唯一included记录。",
                );
            }
        }
        for id in path {
            tails.insert(id, resolved);
        }
    }
}

pub(crate) fn validate_coverage(case: &LoadedCase) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut index = BTreeMap::<&str, Vec<&CoverageRecord>>::new();
    for record in &case.ir.coverage {
        index
            .entry(record.span_id.as_str())
            .or_default()
            .push(record);
    }
    for span_id in case.spans.keys() {
        if unique_record(&index, span_id).is_none() {
            add(
                &mut issues,
                "COVERAGE_RECORD_COUNT",
                &[span_id],
                "每个实际Span必须恰有一条CoverageRecord，不能缺漏或重复。",
            );
        }
    }
    for span_id in index.keys() {
        if !case.spans.contains_key(*span_id) {
            add(
                &mut issues,
                "COVERAGE_RECORD_COUNT",
                &[span_id],
                "CoverageRecord不得引用实际Span清单之外的位置。",
            );
        }
    }
    let atoms = id_counts(case.ir.atoms.iter().map(|atom| atom.atom_id.as_str()));
    let valid_evidence = valid_evidence(case);
    let mut required = BTreeSet::new();
    let mut supported_positions = BTreeMap::<&str, BTreeSet<&str>>::new();
    for atom in &case.ir.atoms {
        if atoms.get(atom.atom_id.as_str()) != Some(&1) || !is_id(&atom.atom_id) {
            continue;
        }
        if atom.required {
            required.insert(atom.atom_id.as_str());
        }
        let supported: BTreeSet<_> = atom
            .evidence_links
            .iter()
            .filter(|link| link.relation == EvidenceRelation::Supports)
            .filter_map(|link| valid_evidence.get(link.evidence_id.as_str()))
            .map(|evidence| evidence.source_span_id.as_str())
            .collect();
        supported_positions.insert(atom.atom_id.as_str(), supported);
    }
    for record in &case.ir.coverage {
        let refs = [&record.span_id[..]];
        if !is_id(&record.run_id)
            || record.run_id != case.ir.coverage_run_id
            || record.policy_version != "fixture-policy-v1"
            || !matches!(record.stage, CoverageStage::Mapped | CoverageStage::Planned)
            || (record.disposition != CoverageDisposition::Excluded
                && record.decision_source != DecisionSource::FixtureReview)
        {
            add(
                &mut issues,
                "COVERAGE_SCOPE",
                &refs,
                "覆盖记录必须绑定当前审计作用域与fixture-policy-v1，阶段只能mapped/planned；知识映射须由fixture_review决定。",
            );
        }
        let Some(span) = case.spans.get(&record.span_id) else {
            continue;
        };
        if span.declared.parse_status != ParseStatus::Parsed {
            add(
                &mut issues,
                "COVERAGE_PARSE_STATUS",
                &refs,
                "失败或尚待复核的解析位置不能被覆盖处置当作已完成。",
            );
        }
        let mapped: BTreeSet<_> = record.atom_ids.iter().map(String::as_str).collect();
        if mapped.len() != record.atom_ids.len()
            || mapped
                .iter()
                .any(|id| !is_id(id) || atoms.get(id) != Some(&1))
        {
            add(
                &mut issues,
                "COVERAGE_ATOM_REFERENCE",
                &refs,
                "覆盖原子映射不得重复，且必须唯一解析到已有原子。",
            );
        }
        let required_mapping: BTreeSet<_> = record
            .required_atom_ids
            .iter()
            .map(String::as_str)
            .collect();
        let expected_required: BTreeSet<_> = mapped.intersection(&required).copied().collect();
        if required_mapping.len() != record.required_atom_ids.len()
            || required_mapping != expected_required
        {
            add(
                &mut issues,
                "COVERAGE_REQUIRED_SUBSET",
                &refs,
                "required_atom_ids必须恰为当前atom_ids中required原子的子集，不得重复、缺漏或增加。",
            );
        }
        match record.disposition {
            CoverageDisposition::Included | CoverageDisposition::Duplicate => {
                if record.disposition == CoverageDisposition::Included
                    && (mapped.is_empty()
                        || record.duplicate_of_span_id.is_some()
                        || record.exclusion_reason.is_some())
                {
                    add(
                        &mut issues,
                        "COVERAGE_INCLUDED_MAPPING",
                        &refs,
                        "included要求非空原子映射，且不能同时声明排除理由或重复引用。",
                    );
                }
                for atom_id in &mapped {
                    if !supported_positions
                        .get(atom_id)
                        .is_some_and(|positions| positions.contains(record.span_id.as_str()))
                    {
                        add(
                            &mut issues,
                            "COVERAGE_LOCATION_SUPPORT",
                            &[&record.span_id, atom_id],
                            "每个已映射原子在当前来源位置仍需有范围和SHA有效的supports证据。",
                        );
                    }
                }
                if record.disposition == CoverageDisposition::Duplicate {
                    if record.exclusion_reason.is_some()
                        || record
                            .duplicate_of_span_id
                            .as_deref()
                            .is_none_or(|id| !is_id(id))
                    {
                        add(
                            &mut issues,
                            "COVERAGE_DUPLICATE_REFERENCE",
                            &refs,
                            "duplicate必须声明有效重复目标，且不能附带排除理由。",
                        );
                    }
                    let Some(target_id) = record.duplicate_of_span_id.as_deref() else {
                        continue;
                    };
                    let Some(target) = unique_record(&index, target_id) else {
                        add(
                            &mut issues,
                            "COVERAGE_DUPLICATE_REFERENCE",
                            &[&record.span_id, target_id],
                            "重复目标必须唯一解析到已有覆盖记录。",
                        );
                        continue;
                    };
                    let Some(target_span) = case.spans.get(target_id) else {
                        add(
                            &mut issues,
                            "COVERAGE_DUPLICATE_REFERENCE",
                            &[&record.span_id, target_id],
                            "重复目标必须引用实际Span。",
                        );
                        continue;
                    };
                    if span.declared.text_sha256 != target_span.declared.text_sha256 {
                        add(
                            &mut issues,
                            "COVERAGE_DUPLICATE_SHA",
                            &[&record.span_id, target_id],
                            "重复归并只允许完整Span文本SHA相同的位置。",
                        );
                    }
                    let target_mapping: BTreeSet<_> =
                        target.atom_ids.iter().map(String::as_str).collect();
                    let target_required: BTreeSet<_> = target
                        .required_atom_ids
                        .iter()
                        .map(String::as_str)
                        .collect();
                    if mapped.is_empty()
                        || mapped != target_mapping
                        || required_mapping != target_required
                    {
                        add(
                            &mut issues,
                            "COVERAGE_DUPLICATE_MAPPING",
                            &[&record.span_id, target_id],
                            "重复位置必须保留与目标一致的非空原子映射和必要原子集合。",
                        );
                    }
                }
            }
            CoverageDisposition::Excluded => {
                if !mapped.is_empty()
                    || !required_mapping.is_empty()
                    || record.duplicate_of_span_id.is_some()
                    || !record.exclusion_reason.as_deref().is_some_and(nonempty)
                    || (record.decision_source == DecisionSource::DeterministicFormat
                        && span.declared.kind != SpanKind::Formatting)
                {
                    add(
                        &mut issues,
                        "COVERAGE_EXCLUSION",
                        &refs,
                        "排除须有非空理由、空映射及空重复引用；确定性格式规则只允许排除实际formatting位置。",
                    );
                }
            }
            CoverageDisposition::Pending
            | CoverageDisposition::Failed
            | CoverageDisposition::NeedsReview => {
                add(
                    &mut issues,
                    "COVERAGE_DISPOSITION",
                    &refs,
                    "pending、failed和needs_review不能计为来源覆盖完成。",
                );
            }
        }
    }
    validate_duplicate_chains(&index, &mut issues);
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture_input::load;
    use crate::text::{ParseStatus, SpanKind};
    use serde_json::{Value, json};
    use std::path::Path;

    fn fixture() -> LoadedCase {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/text-compare-v1")
            .canonicalize()
            .expect("the checked-in fixture has a concrete root");
        let case = load(&root).expect("the checked-in fixture must load");
        assert!(case.source_issues.is_empty(), "{:?}", case.source_issues);
        case
    }

    fn mutated(change: impl FnOnce(&mut Value)) -> LoadedCase {
        let mut case = fixture();
        let mut ir = serde_json::to_value(&case.ir).unwrap();
        change(&mut ir);
        case.ir = serde_json::from_value(ir).expect("mutation retains the JSON contract shape");
        case
    }

    fn record<'a>(ir: &'a mut Value, disposition: &str) -> &'a mut Value {
        ir["coverage"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entry| entry["disposition"] == disposition)
            .unwrap()
    }

    fn required_atom(ir: &mut Value) -> &mut Value {
        ir["atoms"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entry| entry["required"] == true)
            .unwrap()
    }

    fn has_rule(issues: Vec<Issue>, rule: &str) {
        assert!(
            issues.iter().any(|issue| issue.rule_id == rule),
            "expected {rule}, received {issues:?}"
        );
        assert!(issues.iter().all(|issue| !issue.message.trim().is_empty()));
    }

    #[test]
    fn reviewed_fixture_has_valid_knowledge_and_complete_coverage() {
        let case = fixture();
        assert!(
            validate_knowledge(&case).is_empty(),
            "{:?}",
            validate_knowledge(&case)
        );
        assert!(
            validate_coverage(&case).is_empty(),
            "{:?}",
            validate_coverage(&case)
        );
    }

    #[test]
    fn knowledge_sources_must_match_the_entire_manifest_binding() {
        for field in [
            "source_id",
            "version_id",
            "content_sha256",
            "normalized_sha256",
        ] {
            let case = mutated(|ir| ir["sources"][0][field] = json!("changed-binding"));
            has_rule(validate_knowledge(&case), "KNOWLEDGE_SOURCE_BINDING");
        }
        let missing = mutated(|ir| {
            ir["sources"].as_array_mut().unwrap().pop();
        });
        has_rule(validate_knowledge(&missing), "KNOWLEDGE_SOURCE_BINDING");
        let extra = mutated(|ir| {
            let mut source = ir["sources"][0].clone();
            source["source_id"] = json!("extra_source");
            ir["sources"].as_array_mut().unwrap().push(source);
        });
        has_rule(validate_knowledge(&extra), "KNOWLEDGE_SOURCE_BINDING");
    }

    #[test]
    fn repeated_source_bindings_are_rejected_even_if_the_set_is_unchanged() {
        let case = mutated(|ir| {
            let source = ir["sources"][0].clone();
            ir["sources"].as_array_mut().unwrap().push(source);
        });
        has_rule(validate_knowledge(&case), "KNOWLEDGE_SOURCE_BINDING");
    }

    #[test]
    fn ids_and_entity_revisions_cannot_be_invalid_or_ambiguous() {
        for (collection, id_field) in [
            ("concepts", "concept_id"),
            ("atoms", "atom_id"),
            ("evidence", "evidence_id"),
        ] {
            let invalid = mutated(|ir| ir[collection][0][id_field] = json!("invalid id"));
            has_rule(validate_knowledge(&invalid), "KNOWLEDGE_ID");
            let duplicate = mutated(|ir| {
                let entry = ir[collection][0].clone();
                ir[collection].as_array_mut().unwrap().push(entry);
            });
            has_rule(validate_knowledge(&duplicate), "KNOWLEDGE_ID");
        }
        for collection in ["concepts", "atoms"] {
            let case = mutated(|ir| ir[collection][0]["revision"] = json!(0));
            has_rule(validate_knowledge(&case), "KNOWLEDGE_REVISION");
        }
        let case = mutated(|ir| ir["revision"] = json!(0));
        has_rule(validate_knowledge(&case), "KNOWLEDGE_REVISION");
    }

    #[test]
    fn concepts_require_text_but_names_and_aliases_may_overlap() {
        let blank = mutated(|ir| ir["concepts"][0]["canonical_name"] = json!("  "));
        has_rule(validate_knowledge(&blank), "KNOWLEDGE_CONCEPT");
        let alias = mutated(|ir| ir["concepts"][0]["aliases"] = json!([""]));
        has_rule(validate_knowledge(&alias), "KNOWLEDGE_CONCEPT");
        let shared = mutated(|ir| {
            let name = ir["concepts"][0]["canonical_name"].clone();
            for concept in ir["concepts"].as_array_mut().unwrap() {
                concept["canonical_name"] = name.clone();
                concept["aliases"] = json!(["shared alias"]);
            }
        });
        assert!(validate_knowledge(&shared).is_empty());
    }

    #[test]
    fn evidence_references_must_resolve_to_actual_spans() {
        let case = mutated(|ir| ir["evidence"][0]["source_span_id"] = json!("missing_span"));
        has_rule(validate_knowledge(&case), "KNOWLEDGE_EVIDENCE_SPAN");
    }

    #[test]
    fn evidence_rejects_empty_reversed_or_out_of_bounds_ranges() {
        for (start, end) in [(0usize, 0usize), (2, 1), (0, usize::MAX)] {
            let case = mutated(|ir| {
                ir["evidence"][0]["excerpt_start_byte"] = json!(start);
                ir["evidence"][0]["excerpt_end_byte"] = json!(end);
            });
            has_rule(validate_knowledge(&case), "KNOWLEDGE_EVIDENCE_RANGE");
        }
    }

    #[test]
    fn evidence_offsets_must_be_utf8_character_boundaries() {
        let mut case = fixture();
        let evidence = case
            .ir
            .evidence
            .iter_mut()
            .find(|entry| {
                case.spans[&entry.source_span_id]
                    .text
                    .chars()
                    .any(|character| character.len_utf8() > 1)
            })
            .expect("fixture contains Unicode evidence");
        let text = &case.spans[&evidence.source_span_id].text;
        let split = text
            .char_indices()
            .find(|(_, character)| character.len_utf8() > 1)
            .unwrap()
            .0
            + 1;
        evidence.excerpt_start_byte = split;
        evidence.excerpt_end_byte = text.len();
        has_rule(validate_knowledge(&case), "KNOWLEDGE_EVIDENCE_RANGE");
    }

    #[test]
    fn evidence_hash_must_match_the_actual_excerpt_bytes() {
        let case = mutated(|ir| ir["evidence"][0]["excerpt_sha256"] = json!("0".repeat(64)));
        has_rule(validate_knowledge(&case), "KNOWLEDGE_EVIDENCE_SHA");
    }

    #[test]
    fn evidence_can_bind_a_nonempty_partial_unicode_excerpt() {
        let case = mutated(|ir| {
            let entry = ir["evidence"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|entry| entry["evidence_id"] == "E3a")
                .unwrap();
            entry["excerpt_start_byte"] = json!(0);
            entry["excerpt_end_byte"] = json!(3);
            entry["excerpt_sha256"] =
                json!("33e3580c093111e1c4cac922872c40371f7896bb3fa4e23be5840bd1834fc622");
        });
        assert!(
            validate_knowledge(&case).is_empty(),
            "{:?}",
            validate_knowledge(&case)
        );
    }

    #[test]
    fn atom_content_and_qualifiers_cannot_be_empty() {
        let content = mutated(|ir| required_atom(ir)["content"] = json!("  "));
        has_rule(validate_knowledge(&content), "KNOWLEDGE_ATOM_CONTENT");
        let qualifier = mutated(|ir| required_atom(ir)["qualifiers"] = json!([" "]));
        has_rule(validate_knowledge(&qualifier), "KNOWLEDGE_ATOM_CONTENT");
    }

    #[test]
    fn atoms_require_nonempty_unambiguous_concept_and_evidence_references() {
        for field in ["concept_ids", "evidence_links"] {
            let case = mutated(|ir| required_atom(ir)[field] = json!([]));
            let rule = if field == "concept_ids" {
                "KNOWLEDGE_ATOM_REFERENCE"
            } else {
                "KNOWLEDGE_ATOM_SUPPORT"
            };
            has_rule(validate_knowledge(&case), rule);
        }
        let concept = mutated(|ir| required_atom(ir)["concept_ids"] = json!(["missing_concept"]));
        has_rule(validate_knowledge(&concept), "KNOWLEDGE_ATOM_REFERENCE");
        let evidence = mutated(|ir| {
            required_atom(ir)["evidence_links"][0]["evidence_id"] = json!("missing_evidence")
        });
        has_rule(validate_knowledge(&evidence), "KNOWLEDGE_ATOM_REFERENCE");
        let repeated = mutated(|ir| {
            let concept = required_atom(ir)["concept_ids"][0].clone();
            required_atom(ir)["concept_ids"] = json!([concept.clone(), concept]);
        });
        has_rule(validate_knowledge(&repeated), "KNOWLEDGE_ATOM_REFERENCE");
    }

    #[test]
    fn required_atoms_need_valid_support_not_only_refutes_or_qualifies() {
        for relation in ["refutes", "qualifies"] {
            let case = mutated(|ir| {
                for link in required_atom(ir)["evidence_links"].as_array_mut().unwrap() {
                    link["relation"] = json!(relation);
                }
            });
            has_rule(validate_knowledge(&case), "KNOWLEDGE_ATOM_SUPPORT");
        }
        let case = mutated(|ir| {
            let evidence_ids: Vec<_> = required_atom(ir)["evidence_links"]
                .as_array()
                .unwrap()
                .iter()
                .map(|link| link["evidence_id"].clone())
                .collect();
            for evidence in ir["evidence"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .filter(|entry| evidence_ids.contains(&entry["evidence_id"]))
            {
                evidence["excerpt_end_byte"] = evidence["excerpt_start_byte"].clone();
            }
        });
        has_rule(validate_knowledge(&case), "KNOWLEDGE_ATOM_SUPPORT");
    }

    #[test]
    fn required_atoms_must_be_known_and_fixture_reviewed() {
        let unverified = mutated(|ir| required_atom(ir)["verification"] = json!("unverified"));
        has_rule(
            validate_knowledge(&unverified),
            "KNOWLEDGE_ATOM_REQUIRED_STATUS",
        );
        for status in ["unknown", "not_applicable"] {
            let case = mutated(|ir| required_atom(ir)["value_status"] = json!(status));
            has_rule(validate_knowledge(&case), "KNOWLEDGE_ATOM_REQUIRED_STATUS");
        }
    }

    #[test]
    fn coverage_is_exactly_one_record_per_actual_span() {
        let missing = mutated(|ir| {
            ir["coverage"].as_array_mut().unwrap().pop();
        });
        has_rule(validate_coverage(&missing), "COVERAGE_RECORD_COUNT");
        let duplicate = mutated(|ir| {
            let entry = ir["coverage"][0].clone();
            ir["coverage"].as_array_mut().unwrap().push(entry);
        });
        has_rule(validate_coverage(&duplicate), "COVERAGE_RECORD_COUNT");
        let unknown = mutated(|ir| ir["coverage"][0]["span_id"] = json!("unlisted_span"));
        has_rule(validate_coverage(&unknown), "COVERAGE_RECORD_COUNT");
    }

    #[test]
    fn coverage_requires_current_scope_policy_and_mapped_or_planned_stage() {
        for (field, value) in [
            ("run_id", "other_scope"),
            ("policy_version", "future-policy"),
            ("stage", "registered"),
            ("stage", "rendered"),
            ("stage", "validated"),
        ] {
            let case = mutated(|ir| ir["coverage"][0][field] = json!(value));
            has_rule(validate_coverage(&case), "COVERAGE_SCOPE");
        }
        let planned = mutated(|ir| {
            for entry in ir["coverage"].as_array_mut().unwrap() {
                entry["stage"] = json!("planned");
            }
        });
        assert!(validate_coverage(&planned).is_empty());
    }

    #[test]
    fn unresolved_dispositions_and_unreviewed_parse_status_cannot_pass() {
        for disposition in ["pending", "failed", "needs_review"] {
            let case = mutated(|ir| record(ir, "included")["disposition"] = json!(disposition));
            has_rule(validate_coverage(&case), "COVERAGE_DISPOSITION");
        }
        for status in [ParseStatus::Failed, ParseStatus::NeedsReview] {
            let mut case = fixture();
            case.spans
                .values_mut()
                .next()
                .unwrap()
                .declared
                .parse_status = status;
            has_rule(validate_coverage(&case), "COVERAGE_PARSE_STATUS");
        }
    }

    #[test]
    fn included_coverage_requires_nonempty_mapping_and_resolved_atoms() {
        let empty = mutated(|ir| record(ir, "included")["atom_ids"] = json!([]));
        has_rule(validate_coverage(&empty), "COVERAGE_INCLUDED_MAPPING");
        let unknown = mutated(|ir| record(ir, "included")["atom_ids"] = json!(["missing_atom"]));
        has_rule(validate_coverage(&unknown), "COVERAGE_ATOM_REFERENCE");
        let repeated = mutated(|ir| {
            let atom = record(ir, "included")["atom_ids"][0].clone();
            record(ir, "included")["atom_ids"] = json!([atom.clone(), atom]);
        });
        has_rule(validate_coverage(&repeated), "COVERAGE_ATOM_REFERENCE");
    }

    #[test]
    fn included_and_duplicate_decisions_cannot_claim_deterministic_format_review() {
        for disposition in ["included", "duplicate"] {
            let case = mutated(|ir| {
                record(ir, disposition)["decision_source"] = json!("deterministic_format")
            });
            has_rule(validate_coverage(&case), "COVERAGE_SCOPE");
        }
    }

    #[test]
    fn included_mapping_cannot_also_claim_exclusion_or_duplicate() {
        for (field, value) in [
            ("exclusion_reason", "pretend exclusion"),
            ("duplicate_of_span_id", "other_span"),
        ] {
            let case = mutated(|ir| record(ir, "included")[field] = json!(value));
            has_rule(validate_coverage(&case), "COVERAGE_INCLUDED_MAPPING");
        }
    }

    #[test]
    fn required_mapping_is_exactly_the_required_subset_of_mapped_atoms() {
        let missing = mutated(|ir| {
            let entry = ir["coverage"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|entry| {
                    entry["required_atom_ids"]
                        .as_array()
                        .is_some_and(|ids| !ids.is_empty())
                })
                .unwrap();
            entry["required_atom_ids"] = json!([]);
        });
        has_rule(validate_coverage(&missing), "COVERAGE_REQUIRED_SUBSET");
        let extra =
            mutated(|ir| record(ir, "included")["required_atom_ids"] = json!(["unmapped_atom"]));
        has_rule(validate_coverage(&extra), "COVERAGE_REQUIRED_SUBSET");
    }

    #[test]
    fn each_included_position_needs_its_own_valid_supporting_excerpt() {
        let case = mutated(|ir| {
            let span = record(ir, "included")["span_id"].clone();
            ir["evidence"]
                .as_array_mut()
                .unwrap()
                .retain(|entry| entry["source_span_id"] != span);
        });
        has_rule(validate_coverage(&case), "COVERAGE_LOCATION_SUPPORT");
    }

    #[test]
    fn exclusions_require_reason_no_mapping_and_no_duplicate_reference() {
        for (field, value) in [
            ("exclusion_reason", json!(" ")),
            ("atom_ids", json!(["unexpected_atom"])),
            ("required_atom_ids", json!(["unexpected_atom"])),
            ("duplicate_of_span_id", json!("unexpected_span")),
        ] {
            let case = mutated(|ir| record(ir, "excluded")[field] = value);
            has_rule(validate_coverage(&case), "COVERAGE_EXCLUSION");
        }
        let mut case = fixture();
        let excluded = case
            .ir
            .coverage
            .iter()
            .find(|entry| {
                serde_json::to_value(&entry.decision_source).unwrap() == "deterministic_format"
            })
            .unwrap();
        case.spans.get_mut(&excluded.span_id).unwrap().declared.kind = SpanKind::Text;
        has_rule(validate_coverage(&case), "COVERAGE_EXCLUSION");
    }

    #[test]
    fn duplicates_require_existing_reference_identical_span_hash_and_no_cycle() {
        let missing =
            mutated(|ir| record(ir, "duplicate")["duplicate_of_span_id"] = json!("missing_span"));
        has_rule(validate_coverage(&missing), "COVERAGE_DUPLICATE_REFERENCE");
        let self_cycle = mutated(|ir| {
            let entry = record(ir, "duplicate");
            entry["duplicate_of_span_id"] = entry["span_id"].clone();
        });
        has_rule(validate_coverage(&self_cycle), "COVERAGE_DUPLICATE_CYCLE");
        let mut unequal = fixture();
        let span_id = unequal
            .ir
            .coverage
            .iter()
            .find(|entry| serde_json::to_value(&entry.disposition).unwrap() == "duplicate")
            .unwrap()
            .span_id
            .clone();
        unequal
            .spans
            .get_mut(&span_id)
            .unwrap()
            .declared
            .text_sha256 = "0".repeat(64);
        has_rule(validate_coverage(&unequal), "COVERAGE_DUPLICATE_SHA");
    }

    #[test]
    fn duplicate_chains_must_be_acyclic_and_end_at_included_mapping() {
        let cycle = mutated(|ir| {
            let duplicate = record(ir, "duplicate").clone();
            let target_id = duplicate["duplicate_of_span_id"].clone();
            let target = ir["coverage"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|entry| entry["span_id"] == target_id)
                .unwrap();
            target["disposition"] = json!("duplicate");
            target["duplicate_of_span_id"] = duplicate["span_id"].clone();
        });
        has_rule(validate_coverage(&cycle), "COVERAGE_DUPLICATE_CYCLE");
        let terminal = mutated(|ir| {
            let target_id = record(ir, "duplicate")["duplicate_of_span_id"].clone();
            let target = ir["coverage"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|entry| entry["span_id"] == target_id)
                .unwrap();
            target["disposition"] = json!("excluded");
        });
        has_rule(validate_coverage(&terminal), "COVERAGE_DUPLICATE_TERMINAL");
    }

    #[test]
    fn duplicate_mapping_must_agree_and_have_support_at_the_repeat_position() {
        let mismatch = mutated(|ir| record(ir, "duplicate")["atom_ids"] = json!([]));
        has_rule(validate_coverage(&mismatch), "COVERAGE_DUPLICATE_MAPPING");
        let missing = mutated(|ir| {
            let span = record(ir, "duplicate")["span_id"].clone();
            ir["evidence"]
                .as_array_mut()
                .unwrap()
                .retain(|entry| entry["source_span_id"] != span);
        });
        has_rule(validate_coverage(&missing), "COVERAGE_LOCATION_SUPPORT");
    }
}
