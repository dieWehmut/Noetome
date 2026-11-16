//! Shared knowledge values and structural rules, independent of input producers.
//!
//! Callers validate source bindings, evidence bytes and review policy separately.
//! Passing these rules does not grant scientific verification or publication.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

macro_rules! model {
    ($name:ident { $($field:ident : $kind:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $(pub $field: $kind),* }
    };
}

macro_rules! enumeration {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),* }
    };
}

enumeration!(AtomKind {
    Definition,
    Statement,
    ComparisonValue
});
// The historical reviewed variant remains part of the existing wire contract.
// Its interpretation belongs to the caller's review policy, not this module.
enumeration!(Verification {
    FixtureReviewed,
    Unverified
});
enumeration!(ValueStatus {
    Known,
    Unknown,
    NotApplicable
});
enumeration!(EvidenceRelation {
    Supports,
    Refutes,
    Qualifies
});

model!(Concept {
    concept_id: String,
    revision: u64,
    canonical_name: String,
    aliases: Vec<String>,
    needs_full_explanation: bool,
});

model!(KnowledgeAtom {
    atom_id: String,
    revision: u64,
    kind: AtomKind,
    concept_ids: Vec<String>,
    content: String,
    qualifiers: Vec<String>,
    evidence_links: Vec<EvidenceLink>,
    required: bool,
    verification: Verification,
    value_status: ValueStatus,
});

model!(EvidenceLink {
    evidence_id: String,
    relation: EvidenceRelation
});
model!(Issue { rule_id: String, object_refs: Vec<String>, message: String });

impl Issue {
    pub fn new(rule: &str, refs: Vec<String>, message: &str) -> Self {
        Self {
            rule_id: rule.into(),
            object_refs: refs,
            message: message.into(),
        }
    }
}

/// Structural identifier syntax only; persistent identity policies are separate.
pub fn is_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

pub fn is_sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn nonempty(value: &str) -> bool {
    !value.trim().is_empty() && !value.contains('\0')
}

/// plain-text-v1 forbids hidden content and undeclared links or images.
/// Single brackets and mathematical comparisons remain ordinary text.
pub fn plain_text(value: &str) -> bool {
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
        || value.contains("%%")
        || value.contains("[[")
        || value.contains("![")
        || value.as_bytes().windows(2).any(|pair| {
            pair[0] == b'<' && (pair[1].is_ascii_alphabetic() || b"/!?".contains(&pair[1]))
        })
    {
        return false;
    }
    if value.match_indices(']').any(|(offset, _)| {
        value[offset + 1..]
            .trim_start()
            .starts_with(['(', '[', ':'])
    }) {
        return false;
    }
    !value.match_indices('<').any(|(offset, _)| {
        value[offset + 1..]
            .split_once('>')
            .is_some_and(|(inside, _)| {
                inside.contains('@') && !inside.chars().any(char::is_whitespace)
            })
    })
}

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

/// Identity, revision, plain-text and unique-reference rules only.
/// Evidence validity and required/support/review policies belong to the caller.
pub fn validate_basics<'a>(
    concept_entries: &[Concept],
    evidence_ids: impl Iterator<Item = &'a str>,
    atom_entries: &[&KnowledgeAtom],
) -> Vec<Issue> {
    let mut issues = Vec::new();
    let concepts = id_counts(
        concept_entries
            .iter()
            .map(|concept| concept.concept_id.as_str()),
    );
    let evidence = id_counts(evidence_ids);
    let atoms = id_counts(atom_entries.iter().map(|atom| atom.atom_id.as_str()));
    for counts in [&concepts, &evidence, &atoms] {
        for (id, count) in counts {
            if !is_id(id) || *count != 1 {
                add(
                    &mut issues,
                    "KNOWLEDGE_ID",
                    &[id],
                    "实体ID必须格式有效且在同类清单中唯一。",
                );
            }
        }
    }
    for concept in concept_entries {
        if concept.revision == 0 {
            add(
                &mut issues,
                "KNOWLEDGE_REVISION",
                &[&concept.concept_id],
                "概念修订号必须为正整数。",
            );
        }
        if !nonempty(&concept.canonical_name)
            || !plain_text(&concept.canonical_name)
            || concept
                .aliases
                .iter()
                .any(|alias| !nonempty(alias) || !plain_text(alias))
        {
            add(
                &mut issues,
                "KNOWLEDGE_CONCEPT",
                &[&concept.concept_id],
                "概念名称和每个已声明别名必须为非空明文，链接须由结构化引用声明。",
            );
        }
    }
    for atom in atom_entries {
        if atom.revision == 0 {
            add(
                &mut issues,
                "KNOWLEDGE_REVISION",
                &[&atom.atom_id],
                "原子修订号必须为正整数。",
            );
        }
        if !nonempty(&atom.content)
            || !plain_text(&atom.content)
            || atom
                .qualifiers
                .iter()
                .any(|qualifier| !nonempty(qualifier) || !plain_text(qualifier))
        {
            add(
                &mut issues,
                "KNOWLEDGE_ATOM_CONTENT",
                &[&atom.atom_id],
                "原子正文和每个已声明条件、单位或例外必须为非空明文。",
            );
        }
        let concept_refs: BTreeSet<_> = atom.concept_ids.iter().map(String::as_str).collect();
        if concept_refs.is_empty()
            || concept_refs.len() != atom.concept_ids.len()
            || concept_refs
                .iter()
                .any(|id| !is_id(id) || concepts.get(id) != Some(&1))
        {
            add(
                &mut issues,
                "KNOWLEDGE_ATOM_REFERENCE",
                &[&atom.atom_id],
                "原子概念引用必须非空、不重复且唯一解析到已有概念ID。",
            );
        }
        let mut link_refs = BTreeSet::new();
        for link in &atom.evidence_links {
            let relation = match link.relation {
                EvidenceRelation::Supports => 0u8,
                EvidenceRelation::Refutes => 1,
                EvidenceRelation::Qualifies => 2,
            };
            if !is_id(&link.evidence_id)
                || evidence.get(link.evidence_id.as_str()) != Some(&1)
                || !link_refs.insert((link.evidence_id.as_str(), relation))
            {
                add(
                    &mut issues,
                    "KNOWLEDGE_ATOM_REFERENCE",
                    &[&atom.atom_id, &link.evidence_id],
                    "原子证据关系必须唯一引用已有证据，不得重复同一关系。",
                );
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn existing_candidate_fields_and_enum_values_round_trip_without_extensions() {
        let payload: Value = serde_json::from_str(include_str!(
            "../../../fixtures/extraction-gateway-v1/response.json"
        ))
        .unwrap();
        for value in payload["concepts"].as_array().unwrap() {
            let concept: Concept = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(concept).unwrap(), *value);
        }
        for candidate in payload["atoms"].as_array().unwrap() {
            let value = &candidate["atom"];
            let atom: KnowledgeAtom = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(&atom).unwrap(), *value);
            assert_eq!(atom.verification, Verification::Unverified);
            let mut extra = value.clone();
            extra["published"] = Value::Bool(true);
            assert!(serde_json::from_value::<KnowledgeAtom>(extra).is_err());
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove("verification");
            assert!(serde_json::from_value::<KnowledgeAtom>(missing).is_err());
        }
    }

    #[test]
    fn structural_validation_leaves_required_review_and_source_support_to_callers() {
        let concept = Concept {
            concept_id: "concept-1".into(),
            revision: 1,
            canonical_name: "速度".into(),
            aliases: vec![],
            needs_full_explanation: true,
        };
        let mut atom = KnowledgeAtom {
            atom_id: "atom-1".into(),
            revision: 1,
            kind: AtomKind::Statement,
            concept_ids: vec![concept.concept_id.clone()],
            content: "速度有方向。".into(),
            qualifiers: vec![],
            evidence_links: vec![],
            required: true,
            verification: Verification::Unverified,
            value_status: ValueStatus::Known,
        };
        assert!(validate_basics(&[concept.clone()], std::iter::empty(), &[&atom]).is_empty());
        assert_eq!(atom.verification, Verification::Unverified);
        atom.concept_ids.push(concept.concept_id.clone());
        let issues = validate_basics(&[concept], std::iter::empty(), &[&atom]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].rule_id, "KNOWLEDGE_ATOM_REFERENCE");
    }
}
