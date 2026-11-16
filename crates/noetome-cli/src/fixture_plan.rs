//! Validate fixture display scopes and compare independently declared expectations.

use std::collections::{BTreeMap, BTreeSet};

use crate::fixture_input::{LoadedCase, valid_relative_path};
use crate::fixture_model::*;

type CompletedSections = BTreeMap<(String, String), BTreeSet<String>>;

pub(crate) fn validate_plan(case: &LoadedCase) -> (Vec<Issue>, Vec<ResolvedTarget>) {
    let mut issues = Vec::new();
    if case.plan.schema_version != 1 || case.plan.revision == 0 {
        issue(
            &mut issues,
            "PLAN_SCHEMA",
            &[],
            "The plan requires schema 1 and a positive revision.",
        );
    }
    if case.plan.knowledge_revision != case.ir.revision
        || case.plan.knowledge_sha256 != case.ir_sha256
        || !is_sha(&case.plan.knowledge_sha256)
    {
        issue(
            &mut issues,
            "PLAN_IR_BINDING",
            &[],
            "The plan must bind the current IR revision and SHA256 of the complete raw ir.json bytes.",
        );
    }
    let completed = validate_notes(case, &mut issues);
    let targets = validate_placements(case, &completed, &mut issues);
    validate_links(case, &targets, &mut issues);
    let displayed: BTreeSet<_> = completed.values().flat_map(|ids| ids.iter()).collect();
    for atom in &case.ir.atoms {
        if atom.required && !displayed.contains(&atom.atom_id) {
            issue(
                &mut issues,
                "PLAN_REQUIRED",
                &[&atom.atom_id],
                "A required atom has no complete display of its content and every qualifier in any single scope.",
            );
        }
    }
    (issues, targets)
}

fn issue(issues: &mut Vec<Issue>, rule: &str, refs: &[&str], message: &str) {
    issues.push(Issue::new(
        rule,
        refs.iter().map(|id| (*id).to_owned()).collect(),
        message,
    ));
}

fn unique_id(id: &str, seen: &mut BTreeSet<String>, issues: &mut Vec<Issue>) {
    if !is_id(id) || !seen.insert(id.to_owned()) {
        issue(
            issues,
            "PLAN_ID_UNIQUE",
            &[id],
            "Each identity must have the fixture ID syntax and be unique within its declared entity scope.",
        );
    }
}

fn safe_heading(value: &str) -> bool {
    nonempty(value)
        && plain_text(value)
        && !value
            .chars()
            .any(|character| character.is_control() || "[]#|^".contains(character))
}

// Keep the historical internal path while sharing the plain-text-v1 rule.
pub(crate) use crate::domain::plain_text;

fn check_plain_text(value: &str, owner: &str, issues: &mut Vec<Issue>) -> bool {
    if plain_text(value) {
        true
    } else {
        issue(
            issues,
            "PLAN_TEXT_SYNTAX",
            &[owner],
            "plain-text-v1 display fields cannot contain raw HTML/comments, Obsidian hidden comments, unregistered wiki/Markdown links or images, email autolinks or unsupported control characters.",
        );
        false
    }
}

fn valid_output_path(value: &str) -> bool {
    valid_relative_path(value)
        && value.to_lowercase().ends_with(".md")
        && !value.chars().any(|character| "#[]^".contains(character))
}

fn validate_notes(case: &LoadedCase, issues: &mut Vec<Issue>) -> CompletedSections {
    let mut completed = BTreeMap::new();
    let mut note_ids = BTreeSet::new();
    let mut section_ids = BTreeSet::new();
    let mut node_ids = BTreeSet::new();
    let mut paths: BTreeMap<String, String> = BTreeMap::new();
    if case.plan.notes.is_empty() {
        issue(
            issues,
            "PLAN_STRUCTURE",
            &[],
            "The plan must contain at least one note.",
        );
    }
    for note in &case.plan.notes {
        unique_id(&note.note_id, &mut note_ids, issues);
        if note.revision == 0 || note.sections.is_empty() {
            issue(
                issues,
                "PLAN_STRUCTURE",
                &[&note.note_id],
                "A note requires a positive revision and at least one section.",
            );
        }
        check_plain_text(&note.title, &note.note_id, issues);
        if !safe_heading(&note.title) {
            issue(
                issues,
                "PLAN_HEADING",
                &[&note.note_id],
                "Note titles must be nonempty and cannot contain control characters or unsupported wiki title syntax.",
            );
        }
        let path = note.output_path.to_lowercase();
        if !valid_output_path(&note.output_path) {
            issue(
                issues,
                "PLAN_PATH",
                &[&note.note_id, &note.output_path],
                "A note output must be an ordinary Windows-safe relative Markdown path using forward slashes, without unsupported wiki address control characters #[]^.",
            );
        }
        if let Some(other_note) = paths.insert(path.clone(), note.note_id.clone()) {
            issue(
                issues,
                "PLAN_PATH",
                &[&other_note, &note.note_id],
                "Note output paths collide under Windows case-insensitive comparison.",
            );
        }
        let mut headings = BTreeSet::new();
        for section in &note.sections {
            unique_id(&section.section_id, &mut section_ids, issues);
            check_plain_text(&section.heading, &section.section_id, issues);
            if !safe_heading(&section.heading)
                || !headings.insert(section.heading.trim().to_lowercase())
            {
                issue(
                    issues,
                    "PLAN_HEADING",
                    &[&note.note_id, &section.section_id],
                    "A section heading must be safe and unique within its note after trimming and case folding.",
                );
            }
            if section.ordered_nodes.is_empty() {
                issue(
                    issues,
                    "PLAN_STRUCTURE",
                    &[&section.section_id],
                    "Each section requires at least one display node.",
                );
            }
            let mut section_completed = BTreeSet::new();
            for node in &section.ordered_nodes {
                unique_id(node.id(), &mut node_ids, issues);
                section_completed.extend(validate_node(case, node, issues));
            }
            completed
                .entry((note.note_id.clone(), section.section_id.clone()))
                .or_insert_with(BTreeSet::new)
                .extend(section_completed);
        }
    }
    // A file path cannot also be an ancestor directory of another output.
    for (path, note) in &paths {
        for (slash, _) in path.match_indices('/') {
            if let Some(ancestor_note) = paths.get(&path[..slash]) {
                issue(
                    issues,
                    "PLAN_PATH",
                    &[ancestor_note, note],
                    "An output file is also used as an ancestor directory of another note.",
                );
            }
        }
    }
    completed
}

fn refs_set(ids: &[String]) -> BTreeSet<String> {
    ids.iter().cloned().collect()
}

fn validate_scope(
    case: &LoadedCase,
    owner: &str,
    text: &str,
    ids: &[String],
    issues: &mut Vec<Issue>,
) -> BTreeSet<String> {
    let mut completed = BTreeSet::new();
    if !nonempty(text) || refs_set(ids).len() != ids.len() {
        issue(
            issues,
            "PLAN_MAPPING",
            &[owner],
            "A display scope requires nonempty text and cannot repeat an atom ID within the same scope.",
        );
    }
    if !check_plain_text(text, owner, issues) {
        return completed;
    }
    for atom_id in ids {
        let mut atoms = case.ir.atoms.iter().filter(|atom| atom.atom_id == *atom_id);
        let Some(atom) = atoms.next() else {
            issue(
                issues,
                "PLAN_MAPPING",
                &[owner, atom_id],
                "A display scope references an unknown atom.",
            );
            continue;
        };
        if atoms.next().is_some()
            || !nonempty(&atom.content)
            || !text.contains(&atom.content)
            || atom
                .qualifiers
                .iter()
                .any(|qualifier| !nonempty(qualifier) || !text.contains(qualifier))
        {
            issue(
                issues,
                "PLAN_MAPPING",
                &[owner, atom_id],
                "This scope must display this atom's complete content and every qualifier in its own text.",
            );
        } else {
            completed.insert(atom_id.clone());
        }
    }
    completed
}

fn validate_node(case: &LoadedCase, node: &Node, issues: &mut Vec<Issue>) -> BTreeSet<String> {
    let declared = refs_set(node.atom_ids());
    if declared.len() != node.atom_ids().len() {
        issue(
            issues,
            "PLAN_MAPPING",
            &[node.id()],
            "The node atom ID declaration cannot contain duplicate references.",
        );
    }
    let (mapped, completed) = match node {
        Node::Paragraph { text, atom_ids, .. } => (
            refs_set(atom_ids),
            validate_scope(case, node.id(), text, atom_ids, issues),
        ),
        Node::OrderedList { items, .. } => validate_list(case, node.id(), items, issues),
        Node::Table { columns, rows, .. } => validate_table(case, node.id(), columns, rows, issues),
    };
    if declared != mapped {
        issue(
            issues,
            "PLAN_MAPPING",
            &[node.id()],
            "The node atom ID set must equal the union of its actual child display mappings.",
        );
    }
    completed
}

fn validate_list(
    case: &LoadedCase,
    node_id: &str,
    items: &[ListItem],
    issues: &mut Vec<Issue>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut ids = BTreeSet::new();
    let mut mapped = BTreeSet::new();
    let mut completed = BTreeSet::new();
    if items.is_empty() {
        issue(
            issues,
            "PLAN_STRUCTURE",
            &[node_id],
            "An ordered list must contain at least one item.",
        );
    }
    for item in items {
        unique_id(&item.item_id, &mut ids, issues);
        mapped.extend(item.atom_ids.iter().cloned());
        completed.extend(validate_scope(
            case,
            &item.item_id,
            &item.text,
            &item.atom_ids,
            issues,
        ));
    }
    (mapped, completed)
}

fn validate_table(
    case: &LoadedCase,
    node_id: &str,
    columns: &[TableColumn],
    rows: &[TableRow],
    issues: &mut Vec<Issue>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut column_ids = BTreeSet::new();
    let mut row_ids = BTreeSet::new();
    let mut mapped = BTreeSet::new();
    let mut completed = BTreeSet::new();
    if columns.is_empty() || rows.is_empty() {
        issue(
            issues,
            "PLAN_TABLE",
            &[node_id],
            "A table must have at least one column and one row.",
        );
    }
    for column in columns {
        unique_id(&column.column_id, &mut column_ids, issues);
        check_plain_text(&column.label, &column.column_id, issues);
        if !nonempty(&column.label) {
            issue(
                issues,
                "PLAN_TABLE",
                &[node_id, &column.column_id],
                "A table column label cannot be empty.",
            );
        }
    }
    for row in rows {
        unique_id(&row.row_id, &mut row_ids, issues);
        if row.cells.len() != columns.len() {
            issue(
                issues,
                "PLAN_TABLE",
                &[node_id, &row.row_id],
                "Every row must contain exactly one cell per declared column.",
            );
        }
        for (column_index, cell) in row.cells.iter().enumerate() {
            let owner = format!("{node_id}/{}/{column_index}", row.row_id);
            if !nonempty(&cell.text) {
                issue(
                    issues,
                    "PLAN_TABLE",
                    &[&owner],
                    "A table cell cannot have empty display text.",
                );
            }
            match cell.kind {
                CellKind::Content => {
                    if cell.atom_ids.is_empty() {
                        issue(
                            issues,
                            "PLAN_TABLE",
                            &[&owner],
                            "A content cell requires a substantive atom mapping.",
                        );
                    }
                    mapped.extend(cell.atom_ids.iter().cloned());
                    completed.extend(validate_scope(
                        case,
                        &owner,
                        &cell.text,
                        &cell.atom_ids,
                        issues,
                    ));
                }
                CellKind::Label | CellKind::Missing => {
                    check_plain_text(&cell.text, &owner, issues);
                    if !cell.atom_ids.is_empty() || !cell.links.is_empty() {
                        issue(
                            issues,
                            "PLAN_TABLE",
                            &[&owner],
                            "Label and missing cells cannot carry atom mappings or semantic links.",
                        );
                    }
                    if cell.kind == CellKind::Missing
                        && !cell.text.contains("未给出")
                        && !cell.text.contains("不适用")
                    {
                        issue(
                            issues,
                            "PLAN_TABLE",
                            &[&owner],
                            "A missing cell must explicitly state 未给出 or 不适用.",
                        );
                    }
                }
            }
        }
    }
    (mapped, completed)
}

fn placement_target(
    case: &LoadedCase,
    placement: &Placement,
    completed: &CompletedSections,
    issues: &mut Vec<Issue>,
) -> Option<ResolvedTarget> {
    let refs = [
        &placement.concept_id[..],
        &placement.note_id,
        &placement.section_id,
    ];
    let concepts: Vec<_> = case
        .ir
        .concepts
        .iter()
        .filter(|concept| concept.concept_id == placement.concept_id)
        .collect();
    let notes: Vec<_> = case
        .plan
        .notes
        .iter()
        .filter(|note| note.note_id == placement.note_id)
        .collect();
    if concepts.len() != 1 || notes.len() != 1 {
        issue(
            issues,
            "PLAN_CANONICAL",
            &refs,
            "A placement must reference one existing concept and one existing note by ID.",
        );
        return None;
    }
    let note = notes[0];
    let sections: Vec<_> = note
        .sections
        .iter()
        .filter(|section| section.section_id == placement.section_id)
        .collect();
    if sections.len() != 1 {
        issue(
            issues,
            "PLAN_CANONICAL",
            &refs,
            "A placement must reference one existing section within its declared note.",
        );
        return None;
    }
    let related = completed
        .get(&(placement.note_id.clone(), placement.section_id.clone()))
        .is_some_and(|ids| {
            case.ir.atoms.iter().any(|atom| {
                ids.contains(&atom.atom_id) && atom.concept_ids.contains(&placement.concept_id)
            })
        });
    if !related {
        issue(
            issues,
            "PLAN_CANONICAL",
            &refs,
            "An existing placement requires a complete displayed atom related to its concept in that section.",
        );
        return None;
    }
    if placement.canonical {
        let reviewed: Vec<_> = case
            .expected
            .canonical_placements
            .iter()
            .filter(|expected| {
                expected.concept_id == placement.concept_id
                    && expected.note_id == placement.note_id
                    && expected.section_id == placement.section_id
            })
            .collect();
        if reviewed.len() != 1 || !reviewed_placement_complete(case, reviewed[0], completed) {
            issue(
                issues,
                "PLAN_CANONICAL",
                &refs,
                "A canonical placement must fully display every independently reviewed explanation atom in this note and section; another related atom cannot replace it.",
            );
            return None;
        }
    }
    let section = sections[0];
    if !target_address_unambiguous(case, note, section) {
        return None;
    }
    Some(ResolvedTarget {
        concept_id: placement.concept_id.clone(),
        note_id: placement.note_id.clone(),
        section_id: placement.section_id.clone(),
        output_path: note.output_path.clone(),
        heading: section.heading.clone(),
    })
}

fn reviewed_placement_complete(
    case: &LoadedCase,
    expected: &ExpectedPlacement,
    completed: &CompletedSections,
) -> bool {
    let atom_ids = refs_set(&expected.atom_ids);
    if !is_id(&expected.concept_id)
        || !is_id(&expected.note_id)
        || !is_id(&expected.section_id)
        || atom_ids.is_empty()
        || atom_ids.len() != expected.atom_ids.len()
    {
        return false;
    }
    let notes: Vec<_> = case
        .plan
        .notes
        .iter()
        .filter(|note| note.note_id == expected.note_id)
        .collect();
    if notes.len() != 1 {
        return false;
    }
    let sections: Vec<_> = notes[0]
        .sections
        .iter()
        .filter(|section| section.section_id == expected.section_id)
        .collect();
    if sections.len() != 1 || !target_address_unambiguous(case, notes[0], sections[0]) {
        return false;
    }
    let Some(displayed) = completed.get(&(expected.note_id.clone(), expected.section_id.clone()))
    else {
        return false;
    };
    expected.atom_ids.iter().all(|id| {
        let atoms: Vec<_> = case
            .ir
            .atoms
            .iter()
            .filter(|atom| atom.atom_id == *id)
            .collect();
        is_id(id)
            && displayed.contains(id)
            && atoms.len() == 1
            && atoms[0].concept_ids.contains(&expected.concept_id)
    })
}

fn target_address_unambiguous(case: &LoadedCase, note: &Note, section: &Section) -> bool {
    let path = note.output_path.to_lowercase();
    let heading = section.heading.trim().to_lowercase();
    valid_output_path(&note.output_path)
        && safe_heading(&note.title)
        && safe_heading(&section.heading)
        && note
            .sections
            .iter()
            .filter(|other| other.heading.trim().to_lowercase() == heading)
            .count()
            == 1
        && case
            .plan
            .notes
            .iter()
            .flat_map(|other| other.sections.iter())
            .filter(|other| other.section_id == section.section_id)
            .count()
            == 1
        && case
            .plan
            .notes
            .iter()
            .filter(|other| other.output_path.to_lowercase() == path)
            .count()
            == 1
        && !case.plan.notes.iter().any(|other| {
            let other_path = other.output_path.to_lowercase();
            path.starts_with(&format!("{other_path}/"))
                || other_path.starts_with(&format!("{path}/"))
        })
}

fn validate_placements(
    case: &LoadedCase,
    completed: &CompletedSections,
    issues: &mut Vec<Issue>,
) -> Vec<ResolvedTarget> {
    let mut seen = BTreeSet::new();
    let mut candidates: BTreeMap<String, Vec<ResolvedTarget>> = BTreeMap::new();
    for placement in &case.plan.placements {
        let identity = (
            placement.concept_id.clone(),
            placement.note_id.clone(),
            placement.section_id.clone(),
        );
        if !seen.insert(identity) {
            issue(
                issues,
                "PLAN_CANONICAL",
                &[
                    &placement.concept_id,
                    &placement.note_id,
                    &placement.section_id,
                ],
                "A placement cannot repeat the same concept, note and section identity.",
            );
        }
        if let Some(target) = placement_target(case, placement, completed, issues) {
            if placement.canonical {
                candidates
                    .entry(placement.concept_id.clone())
                    .or_default()
                    .push(target);
            }
        }
    }
    let mut targets = Vec::new();
    for concept in &case.ir.concepts {
        let declared_count = case
            .plan
            .placements
            .iter()
            .filter(|placement| placement.canonical && placement.concept_id == concept.concept_id)
            .count();
        if declared_count > 1 || (concept.needs_full_explanation && declared_count != 1) {
            issue(
                issues,
                "PLAN_CANONICAL",
                &[&concept.concept_id],
                "A concept requiring full explanation must have exactly one canonical placement; no concept may have multiple canonical placements.",
            );
        }
        if declared_count == 1 {
            if let Some(valid) = candidates.get(&concept.concept_id) {
                if valid.len() == 1 {
                    targets.push(valid[0].clone());
                }
            }
        }
    }
    targets.sort_by(|left, right| left.concept_id.cmp(&right.concept_id));
    targets
}

fn nodes(case: &LoadedCase) -> impl Iterator<Item = &Node> {
    case.plan
        .notes
        .iter()
        .flat_map(|note| note.sections.iter())
        .flat_map(|section| section.ordered_nodes.iter())
}

fn node_links(node: &Node) -> Vec<&SemanticLink> {
    match node {
        Node::Paragraph { links, .. } => links.iter().collect(),
        Node::OrderedList { items, .. } => {
            items.iter().flat_map(|item| item.links.iter()).collect()
        }
        Node::Table { rows, .. } => rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .flat_map(|cell| cell.links.iter())
            .collect(),
    }
}

fn validate_links(case: &LoadedCase, targets: &[ResolvedTarget], issues: &mut Vec<Issue>) {
    for node in nodes(case) {
        for link in node_links(node) {
            let plain = check_plain_text(&link.display_text, node.id(), issues);
            if !plain
                || !nonempty(&link.display_text)
                || !is_id(&link.target_concept_id)
                || targets
                    .iter()
                    .filter(|target| target.concept_id == link.target_concept_id)
                    .count()
                    != 1
            {
                issue(
                    issues,
                    "PLAN_LINK_TARGET",
                    &[node.id(), &link.target_concept_id],
                    "Every semantic link requires nonempty display text and resolves by concept ID to exactly one valid canonical placement.",
                );
            }
        }
    }
}

pub(crate) fn validate_expected(case: &LoadedCase) -> Vec<Issue> {
    let mut issues = Vec::new();
    if case.expected.schema_version != 1
        || case.expected.case_id != case.manifest.case_id
        || !is_id(&case.expected.case_id)
    {
        issue(
            &mut issues,
            "EXPECTED_CASE",
            &[&case.expected.case_id],
            "The independent expectations must identify this fixture case using schema 1.",
        );
    }
    validate_expected_atoms(case, &mut issues);
    validate_expected_evidence(case, &mut issues);
    validate_expected_coverage(case, &mut issues);
    validate_expected_placements(case, &mut issues);
    validate_expected_links(case, &mut issues);
    validate_expected_tables(case, &mut issues);
    issues
}

fn validate_expected_evidence(case: &LoadedCase, issues: &mut Vec<Issue>) {
    let expected_ids: BTreeSet<_> = case
        .expected
        .evidence
        .iter()
        .map(|evidence| evidence.evidence_id.as_str())
        .collect();
    let actual_ids: BTreeSet<_> = case
        .ir
        .evidence
        .iter()
        .map(|evidence| evidence.evidence_id.as_str())
        .collect();
    if expected_ids != actual_ids
        || expected_ids.is_empty()
        || expected_ids.len() != case.expected.evidence.len()
        || actual_ids.len() != case.ir.evidence.len()
    {
        issue(
            issues,
            "EXPECTED_EVIDENCE",
            &[],
            "The complete reviewed evidence identity set must exactly match the IR evidence inventory without duplicates.",
        );
    }
    for expected in &case.expected.evidence {
        let actual: Vec<_> = case
            .ir
            .evidence
            .iter()
            .filter(|evidence| evidence.evidence_id == expected.evidence_id)
            .collect();
        if actual.len() != 1 || actual[0] != expected {
            issue(
                issues,
                "EXPECTED_EVIDENCE",
                &[&expected.evidence_id],
                "Evidence ID, source Span, UTF8 byte range and excerpt SHA256 must exactly match the independently reviewed evidence.",
            );
        }
    }
}

fn validate_expected_atoms(case: &LoadedCase, issues: &mut Vec<Issue>) {
    let expected_ids: BTreeSet<_> = case
        .expected
        .required_atoms
        .iter()
        .map(|atom| atom.atom_id.as_str())
        .collect();
    let required: Vec<_> = case.ir.atoms.iter().filter(|atom| atom.required).collect();
    let required_ids: BTreeSet<_> = required.iter().map(|atom| atom.atom_id.as_str()).collect();
    if expected_ids.is_empty()
        || expected_ids != required_ids
        || expected_ids.len() != case.expected.required_atoms.len()
        || required_ids.len() != required.len()
    {
        issue(
            issues,
            "EXPECTED_REQUIRED",
            &[],
            "The nonempty independent required atom identity set must exactly equal IR.required, with no duplicates.",
        );
    }
    for expected in &case.expected.required_atoms {
        let atoms: Vec<_> = case
            .ir
            .atoms
            .iter()
            .filter(|atom| atom.atom_id == expected.atom_id)
            .collect();
        if atoms.len() != 1 {
            issue(
                issues,
                "EXPECTED_ATOM",
                &[&expected.atom_id],
                "Every expected atom must correspond to exactly one IR atom.",
            );
            continue;
        }
        let atom = atoms[0];
        let expected_spans = refs_set(&expected.evidence_span_ids);
        let support_spans = valid_support_spans(case, atom);
        if !is_id(&expected.atom_id)
            || !atom.required
            || atom.verification != Verification::FixtureReviewed
            || atom.value_status != ValueStatus::Known
            || atom.kind != expected.kind
            || atom.concept_ids != expected.concept_ids
            || expected.concept_ids.is_empty()
            || atom.content != expected.content
            || !nonempty(&expected.content)
            || atom.qualifiers != expected.qualifiers
            || expected
                .qualifiers
                .iter()
                .any(|qualifier| !nonempty(qualifier))
            || expected_spans.is_empty()
            || expected_spans.len() != expected.evidence_span_ids.len()
            || !expected_spans.is_subset(&support_spans)
        {
            issue(
                issues,
                "EXPECTED_ATOM",
                &[&expected.atom_id],
                "The required atom must retain the independent kind, concepts, exact content, ordered qualifiers, known/reviewed status and every expected valid supporting source position.",
            );
        }
    }
}

fn valid_support_spans(case: &LoadedCase, atom: &KnowledgeAtom) -> BTreeSet<String> {
    let mut spans = BTreeSet::new();
    for link in &atom.evidence_links {
        if link.relation != EvidenceRelation::Supports {
            continue;
        }
        let evidence: Vec<_> = case
            .ir
            .evidence
            .iter()
            .filter(|evidence| evidence.evidence_id == link.evidence_id)
            .collect();
        if evidence.len() != 1 {
            continue;
        }
        let evidence = evidence[0];
        let Some(span) = case.spans.get(&evidence.source_span_id) else {
            continue;
        };
        let Some(excerpt) = span
            .text
            .get(evidence.excerpt_start_byte..evidence.excerpt_end_byte)
        else {
            continue;
        };
        if !excerpt.is_empty() && crate::text::sha256(excerpt.as_bytes()) == evidence.excerpt_sha256
        {
            spans.insert(evidence.source_span_id.clone());
        }
    }
    spans
}

fn coverage_matches(expected: &ExpectedCoverage, actual: &CoverageRecord) -> bool {
    expected.disposition == actual.disposition
        && expected.atom_ids == actual.atom_ids
        && expected.required_atom_ids == actual.required_atom_ids
        && expected.duplicate_of_span_id == actual.duplicate_of_span_id
        && expected.exclusion_reason == actual.exclusion_reason
        && expected.decision_source == actual.decision_source
}

fn validate_expected_coverage(case: &LoadedCase, issues: &mut Vec<Issue>) {
    let expected_ids: BTreeSet<_> = case
        .expected
        .coverage
        .iter()
        .map(|coverage| coverage.span_id.as_str())
        .collect();
    let actual_ids: BTreeSet<_> = case
        .ir
        .coverage
        .iter()
        .map(|coverage| coverage.span_id.as_str())
        .collect();
    let span_ids: BTreeSet<_> = case.spans.keys().map(String::as_str).collect();
    if expected_ids != span_ids
        || actual_ids != span_ids
        || expected_ids.len() != case.expected.coverage.len()
        || actual_ids.len() != case.ir.coverage.len()
    {
        issue(
            issues,
            "EXPECTED_COVERAGE",
            &[],
            "Independent and IR coverage must each account for every actual Span exactly once, without extra records.",
        );
    }
    for expected in &case.expected.coverage {
        let actual: Vec<_> = case
            .ir
            .coverage
            .iter()
            .filter(|coverage| coverage.span_id == expected.span_id)
            .collect();
        if actual.len() != 1 || !coverage_matches(expected, actual[0]) {
            issue(
                issues,
                "EXPECTED_COVERAGE",
                &[&expected.span_id],
                "Coverage disposition, atom mappings, duplicate target, exact exclusion reason and decision source must match the independent expectation.",
            );
        }
    }
}

fn validate_expected_placements(case: &LoadedCase, issues: &mut Vec<Issue>) {
    let expected: BTreeSet<_> = case
        .expected
        .canonical_placements
        .iter()
        .map(|placement| {
            (
                &placement.concept_id,
                &placement.note_id,
                &placement.section_id,
            )
        })
        .collect();
    let canonical: Vec<_> = case
        .plan
        .placements
        .iter()
        .filter(|placement| placement.canonical)
        .collect();
    let actual: BTreeSet<_> = canonical
        .iter()
        .map(|placement| {
            (
                &placement.concept_id,
                &placement.note_id,
                &placement.section_id,
            )
        })
        .collect();
    if expected != actual
        || expected.len() != case.expected.canonical_placements.len()
        || actual.len() != canonical.len()
    {
        issue(
            issues,
            "EXPECTED_CANONICAL",
            &[],
            "The complete canonical placement identity set must exactly match the independent expectation with no duplicates.",
        );
    }
    let completed = validate_notes(case, &mut Vec::new());
    for expected in &case.expected.canonical_placements {
        if !reviewed_placement_complete(case, expected, &completed) {
            issue(
                issues,
                "EXPECTED_CANONICAL",
                &[
                    &expected.concept_id,
                    &expected.note_id,
                    &expected.section_id,
                ],
                "This independent canonical explanation requires its nonempty, distinct, concept-related atom set to be fully displayed at this exact note and section.",
            );
        }
    }
}

fn validate_expected_links(case: &LoadedCase, issues: &mut Vec<Issue>) {
    let mut seen = BTreeSet::new();
    for expected in &case.expected.required_links {
        let identity = (
            &expected.from_node_id,
            &expected.target_concept_id,
            &expected.display_text,
        );
        let from: Vec<_> = nodes(case)
            .filter(|node| node.id() == expected.from_node_id)
            .collect();
        let exists = from.len() == 1
            && node_links(from[0]).iter().any(|link| {
                link.target_concept_id == expected.target_concept_id
                    && link.display_text == expected.display_text
            });
        if !seen.insert(identity) || !exists || !nonempty(&expected.display_text) {
            issue(
                issues,
                "EXPECTED_LINK",
                &[&expected.from_node_id, &expected.target_concept_id],
                "Each distinct independent required semantic link must occur with its exact display text in that actual node.",
            );
        }
    }
}

fn validate_expected_tables(case: &LoadedCase, issues: &mut Vec<Issue>) {
    let tables: Vec<_> = nodes(case)
        .filter_map(|node| match node {
            Node::Table {
                node_id,
                columns,
                rows,
                ..
            } => Some((node_id, columns, rows)),
            _ => None,
        })
        .collect();
    let actual_ids: BTreeSet<_> = tables.iter().map(|(id, _, _)| id.as_str()).collect();
    let expected_ids: BTreeSet<_> = case
        .expected
        .tables
        .iter()
        .map(|table| table.node_id.as_str())
        .collect();
    if actual_ids != expected_ids
        || actual_ids.len() != tables.len()
        || expected_ids.len() != case.expected.tables.len()
    {
        issue(
            issues,
            "EXPECTED_TABLE",
            &[],
            "Every actual table node must have exactly one independent expected table, with no omitted or extra tables.",
        );
    }
    for expected in &case.expected.tables {
        let actual: Vec<_> = tables
            .iter()
            .filter(|(node_id, _, _)| **node_id == expected.node_id)
            .collect();
        if actual.len() != 1 || *actual[0].1 != expected.columns || *actual[0].2 != expected.rows {
            issue(
                issues,
                "EXPECTED_TABLE",
                &[&expected.node_id],
                "The table's ordered columns, rows and every cell kind, text, atom mapping and semantic link must exactly match the independent expectation.",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture_input::load;
    use serde_json::{Value, json};
    use std::path::Path;

    fn fixture() -> LoadedCase {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/text-compare-v1")
            .canonicalize()
            .unwrap();
        let case = load(&root).expect("the independently declared fixture must load");
        assert!(case.source_issues.is_empty(), "{:?}", case.source_issues);
        case
    }

    fn change_plan(case: &mut LoadedCase, change: impl FnOnce(&mut Value)) {
        let mut value = serde_json::to_value(&case.plan).unwrap();
        change(&mut value);
        case.plan = serde_json::from_value(value).unwrap();
    }

    fn change_ir(case: &mut LoadedCase, change: impl FnOnce(&mut Value)) {
        let mut value = serde_json::to_value(&case.ir).unwrap();
        change(&mut value);
        case.ir = serde_json::from_value(value).unwrap();
    }

    fn change_expected(case: &mut LoadedCase, change: impl FnOnce(&mut Value)) {
        let mut value = serde_json::to_value(&case.expected).unwrap();
        change(&mut value);
        case.expected = serde_json::from_value(value).unwrap();
    }

    fn has_rule(issues: &[Issue], rule: &str) {
        assert!(
            issues.iter().any(|issue| issue.rule_id == rule),
            "expected {rule}, got {issues:?}"
        );
    }

    #[test]
    fn valid_fixture_resolves_by_identity_to_two_literal_canonical_targets() {
        let case = fixture();
        let (issues, targets) = validate_plan(&case);
        assert!(issues.is_empty(), "{issues:?}");
        let values: Vec<_> = targets
            .iter()
            .map(|target| {
                (
                    target.concept_id.as_str(),
                    target.note_id.as_str(),
                    target.section_id.as_str(),
                    target.output_path.as_str(),
                    target.heading.as_str(),
                )
            })
            .collect();
        assert_eq!(
            values,
            vec![
                ("C2f", "N4u", "H2j", "笔记/甲容器.md", "定义与比较"),
                ("C6h", "N8v", "H6n", "笔记/乙容器.md", "定义与条件"),
            ]
        );
        assert!(validate_expected(&case).is_empty());
    }

    #[test]
    fn plan_binds_both_full_raw_ir_hash_and_revision() {
        for field in ["knowledge_sha256", "knowledge_revision"] {
            let mut case = fixture();
            change_plan(&mut case, |plan| {
                plan[field] = if field == "knowledge_sha256" {
                    json!("0".repeat(64))
                } else {
                    json!(2)
                }
            });
            has_rule(&validate_plan(&case).0, "PLAN_IR_BINDING");
        }
    }

    #[test]
    fn unsafe_and_windows_colliding_output_paths_are_rejected() {
        for path in [
            "../outside.md",
            "C:/outside.md",
            "notes/CON.md",
            "notes/x .md/../a.md",
            "notes/x.md.",
            "notes/x.md:stream",
            "notes\\x.md",
            "/x.md",
        ] {
            let mut case = fixture();
            change_plan(&mut case, |plan| {
                plan["notes"][0]["output_path"] = json!(path)
            });
            has_rule(&validate_plan(&case).0, "PLAN_PATH");
        }
        let mut case = fixture();
        change_plan(&mut case, |plan| {
            plan["notes"][0]["output_path"] = json!("notes/Alpha.md");
            plan["notes"][1]["output_path"] = json!("NOTES/alpha.MD");
        });
        has_rule(&validate_plan(&case).0, "PLAN_PATH");
    }

    #[test]
    fn headings_reject_wiki_syntax_and_case_trim_ambiguity() {
        for title in [
            "bad#title",
            "bad[title]",
            "bad|title",
            "bad^title",
            "bad\ntitle",
        ] {
            let mut case = fixture();
            change_plan(&mut case, |plan| plan["notes"][0]["title"] = json!(title));
            has_rule(&validate_plan(&case).0, "PLAN_HEADING");
        }
        let mut case = fixture();
        change_plan(&mut case, |plan| {
            let mut second = plan["notes"][0]["sections"][0].clone();
            second["section_id"] = json!("H9new");
            second["heading"] = json!(" 定义与比较 ");
            second["ordered_nodes"] = json!([]);
            plan["notes"][0]["sections"]
                .as_array_mut()
                .unwrap()
                .push(second);
        });
        has_rule(&validate_plan(&case).0, "PLAN_HEADING");
    }

    #[test]
    fn every_plan_identity_scope_rejects_duplicates() {
        for entity in ["note", "section", "node", "item", "column", "row"] {
            let mut case = fixture();
            change_plan(&mut case, |plan| match entity {
                "note" => plan["notes"][1]["note_id"] = json!("N4u"),
                "section" => plan["notes"][1]["sections"][0]["section_id"] = json!("H2j"),
                "node" => {
                    plan["notes"][1]["sections"][0]["ordered_nodes"][0]["node_id"] = json!("Q2z")
                }
                "item" => {
                    let items = &mut plan["notes"][1]["sections"][0]["ordered_nodes"][1]["items"];
                    let item = items[0].clone();
                    items.as_array_mut().unwrap().push(item);
                }
                "column" => {
                    plan["notes"][0]["sections"][0]["ordered_nodes"][1]["columns"][1]["column_id"] =
                        json!("D2c")
                }
                "row" => {
                    plan["notes"][0]["sections"][0]["ordered_nodes"][1]["rows"][1]["row_id"] =
                        json!("W3g")
                }
                _ => unreachable!(),
            });
            has_rule(&validate_plan(&case).0, "PLAN_ID_UNIQUE");
        }
    }

    #[test]
    fn canonical_placements_need_one_existing_related_section() {
        for mutation in ["absent", "duplicate", "wrong_section", "unknown_concept"] {
            let mut case = fixture();
            change_plan(&mut case, |plan| match mutation {
                "absent" => plan["placements"][0]["canonical"] = json!(false),
                "duplicate" => {
                    let placement = plan["placements"][0].clone();
                    plan["placements"].as_array_mut().unwrap().push(placement);
                }
                "wrong_section" => {
                    plan["placements"][0]["note_id"] = json!("N8v");
                    plan["placements"][0]["section_id"] = json!("H6n");
                }
                "unknown_concept" => plan["placements"][0]["concept_id"] = json!("Cunknown"),
                _ => unreachable!(),
            });
            has_rule(&validate_plan(&case).0, "PLAN_CANONICAL");
        }
    }

    #[test]
    fn semantic_links_resolve_concept_ids_even_when_names_are_equal() {
        let mut case = fixture();
        change_ir(&mut case, |ir| {
            ir["concepts"][1]["canonical_name"] = json!("甲容器")
        });
        assert!(validate_plan(&case).0.is_empty());
        change_plan(&mut case, |plan| {
            plan["notes"][0]["sections"][0]["ordered_nodes"][0]["links"][0]["target_concept_id"] =
                json!("Cmissing")
        });
        has_rule(&validate_plan(&case).0, "PLAN_LINK_TARGET");
    }

    #[test]
    fn mapped_paragraph_and_list_item_must_show_their_own_atom_text() {
        for location in ["paragraph", "list"] {
            let mut case = fixture();
            change_plan(&mut case, |plan| {
                if location == "paragraph" {
                    plan["notes"][0]["sections"][0]["ordered_nodes"][0]["text"] = json!("A3d");
                } else {
                    plan["notes"][1]["sections"][0]["ordered_nodes"][1]["items"][0]["text"] =
                        json!("乙容器的虚构刻度为8 cm。");
                }
            });
            has_rule(&validate_plan(&case).0, "PLAN_MAPPING");
        }
    }

    #[test]
    fn table_cell_cannot_borrow_units_or_qualifiers_from_another_cell() {
        let mut case = fixture();
        change_plan(&mut case, |plan| {
            let cells =
                &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1]["rows"][0]["cells"];
            cells[1]["text"] = json!("甲容器的虚构刻度为5。");
            cells[0]["text"] =
                json!("甲容器的虚构刻度为5 cm。仅用于室温演示；装满后停止读数；破损时不适用。");
        });
        has_rule(&validate_plan(&case).0, "PLAN_MAPPING");
        has_rule(&validate_plan(&case).0, "PLAN_REQUIRED");
    }

    #[test]
    fn node_atom_ids_equal_child_mapping_sets_without_duplicate_refs() {
        for kind in ["list_union", "table_union", "duplicate_ref"] {
            let mut case = fixture();
            change_plan(&mut case, |plan| {
                let node = if kind == "list_union" {
                    &mut plan["notes"][1]["sections"][0]["ordered_nodes"][1]
                } else {
                    &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1]
                };
                node["atom_ids"] = if kind == "duplicate_ref" {
                    json!(["A5p", "A9w", "A5p"])
                } else {
                    json!(["A5p"])
                };
            });
            has_rule(&validate_plan(&case).0, "PLAN_MAPPING");
        }
    }

    #[test]
    fn table_rectangles_content_label_and_missing_states_are_enforced() {
        for mutation in [
            "short_row",
            "empty_content",
            "label_mapping",
            "missing_mapping",
            "missing_zero",
        ] {
            let mut case = fixture();
            change_plan(&mut case, |plan| {
                let table = &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1];
                match mutation {
                    "short_row" => {
                        table["rows"][0]["cells"].as_array_mut().unwrap().pop();
                    }
                    "empty_content" => table["rows"][0]["cells"][1]["atom_ids"] = json!([]),
                    "label_mapping" => table["rows"][0]["cells"][0]["atom_ids"] = json!(["A5p"]),
                    "missing_mapping" => {
                        table["rows"][1]["cells"][1]["links"] =
                            json!([{"target_concept_id":"C2f", "display_text":"甲"}])
                    }
                    "missing_zero" => table["rows"][1]["cells"][1]["text"] = json!("0"),
                    _ => unreachable!(),
                }
            });
            has_rule(&validate_plan(&case).0, "PLAN_TABLE");
        }
    }

    #[test]
    fn expected_required_atom_set_catches_required_false_drift() {
        let mut case = fixture();
        change_ir(&mut case, |ir| ir["atoms"][0]["required"] = json!(false));
        has_rule(&validate_expected(&case), "EXPECTED_REQUIRED");
    }

    #[test]
    fn expected_atoms_fix_content_conditions_concepts_type_and_support_positions() {
        for field in [
            "content",
            "qualifiers",
            "concept_ids",
            "kind",
            "evidence_links",
            "verification",
            "value_status",
        ] {
            let mut case = fixture();
            change_ir(&mut case, |ir| {
                let atom = ir["atoms"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|atom| atom["atom_id"] == "A5p")
                    .unwrap();
                atom[field] = match field {
                    "content" => json!("甲容器的虚构刻度为5。"),
                    "qualifiers" | "evidence_links" => json!([]),
                    "concept_ids" => json!(["C6h"]),
                    "kind" => json!("statement"),
                    "verification" => json!("unverified"),
                    "value_status" => json!("unknown"),
                    _ => unreachable!(),
                };
            });
            has_rule(&validate_expected(&case), "EXPECTED_ATOM");
        }
    }

    #[test]
    fn expected_coverage_fix_all_spans_and_exact_exclusion_decisions() {
        for mutation in [
            "omit",
            "reason",
            "decision",
            "mapping",
            "disposition",
            "duplicate",
        ] {
            let mut case = fixture();
            change_ir(&mut case, |ir| match mutation {
                "omit" => {
                    ir["coverage"].as_array_mut().unwrap().pop();
                }
                "reason" => ir["coverage"][0]["exclusion_reason"] = json!("任意非空理由"),
                "decision" => ir["coverage"][0]["decision_source"] = json!("deterministic_format"),
                "mapping" => ir["coverage"][2]["required_atom_ids"] = json!([]),
                "disposition" => ir["coverage"][2]["disposition"] = json!("excluded"),
                "duplicate" => ir["coverage"][11]["duplicate_of_span_id"] = json!("P3b"),
                _ => unreachable!(),
            });
            has_rule(&validate_expected(&case), "EXPECTED_COVERAGE");
        }
    }

    #[test]
    fn expected_canonical_and_required_semantic_links_must_exist_exactly() {
        let mut case = fixture();
        change_plan(&mut case, |plan| {
            plan["placements"][0]["canonical"] = json!(false)
        });
        has_rule(&validate_expected(&case), "EXPECTED_CANONICAL");
        let mut case = fixture();
        change_plan(&mut case, |plan| {
            plan["notes"][0]["sections"][0]["ordered_nodes"][0]["links"][0]["display_text"] =
                json!("别名")
        });
        has_rule(&validate_expected(&case), "EXPECTED_LINK");
    }

    #[test]
    fn expected_tables_fix_all_columns_rows_cells_labels_and_missing_states() {
        for mutation in [
            "column",
            "row",
            "kind",
            "text",
            "mapping",
            "link",
            "omitted_table",
        ] {
            let mut case = fixture();
            change_plan(&mut case, |plan| {
                if mutation == "omitted_table" {
                    plan["notes"][0]["sections"][0]["ordered_nodes"]
                        .as_array_mut()
                        .unwrap()
                        .pop();
                    return;
                }
                let table = &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1];
                match mutation {
                    "column" => table["columns"][0]["label"] = json!("新标签"),
                    "row" => table["rows"][0]["row_id"] = json!("Wnew"),
                    "kind" => table["rows"][1]["cells"][1]["kind"] = json!("label"),
                    "text" => table["rows"][1]["cells"][1]["text"] = json!("不适用"),
                    "mapping" => table["rows"][0]["cells"][1]["atom_ids"] = json!([]),
                    "link" => {
                        table["rows"][0]["cells"][1]["links"] =
                            json!([{"target_concept_id":"C2f", "display_text":"甲"}])
                    }
                    _ => unreachable!(),
                }
            });
            has_rule(&validate_expected(&case), "EXPECTED_TABLE");
        }
    }

    #[test]
    fn expected_case_identity_and_duplicate_expected_entries_are_rejected() {
        let mut case = fixture();
        change_expected(&mut case, |expected| expected["case_id"] = json!("Cother"));
        has_rule(&validate_expected(&case), "EXPECTED_CASE");
        let mut case = fixture();
        change_expected(&mut case, |expected| {
            let atom = expected["required_atoms"][0].clone();
            expected["required_atoms"]
                .as_array_mut()
                .unwrap()
                .push(atom);
        });
        has_rule(&validate_expected(&case), "EXPECTED_REQUIRED");
    }

    #[test]
    fn empty_or_unsupported_plan_and_invalid_identity_are_rejected() {
        for mutation in ["schema", "revision", "notes", "sections", "id"] {
            let mut case = fixture();
            change_plan(&mut case, |plan| match mutation {
                "schema" => plan["schema_version"] = json!(2),
                "revision" => plan["revision"] = json!(0),
                "notes" => plan["notes"] = json!([]),
                "sections" => plan["notes"][0]["sections"] = json!([]),
                "id" => plan["notes"][0]["note_id"] = json!("invalid/identity"),
                _ => unreachable!(),
            });
            assert!(!validate_plan(&case).0.is_empty(), "{mutation}");
        }
    }

    #[test]
    fn missing_required_mapping_and_unknown_atom_are_rejected() {
        let mut case = fixture();
        change_plan(&mut case, |plan| {
            plan["notes"][0]["sections"][0]["ordered_nodes"][0]["atom_ids"] = json!([]);
        });
        has_rule(&validate_plan(&case).0, "PLAN_REQUIRED");
        let mut case = fixture();
        change_plan(&mut case, |plan| {
            plan["notes"][0]["sections"][0]["ordered_nodes"][0]["atom_ids"] = json!(["Amissing"]);
        });
        has_rule(&validate_plan(&case).0, "PLAN_MAPPING");
    }

    #[test]
    fn expected_support_positions_and_expected_collections_cannot_be_relabelled() {
        let mut case = fixture();
        change_expected(&mut case, |expected| {
            expected["required_atoms"][0]["evidence_span_ids"] = json!(["P5b"])
        });
        has_rule(&validate_expected(&case), "EXPECTED_ATOM");
        for collection in [
            "coverage",
            "canonical_placements",
            "required_links",
            "tables",
        ] {
            let mut case = fixture();
            change_expected(&mut case, |expected| {
                let first = expected[collection][0].clone();
                expected[collection].as_array_mut().unwrap().push(first);
            });
            let rule = match collection {
                "coverage" => "EXPECTED_COVERAGE",
                "canonical_placements" => "EXPECTED_CANONICAL",
                "required_links" => "EXPECTED_LINK",
                "tables" => "EXPECTED_TABLE",
                _ => unreachable!(),
            };
            has_rule(&validate_expected(&case), rule);
        }
    }

    #[test]
    fn output_file_directory_collision_is_detected_with_interleaving_sorted_paths() {
        let mut case = fixture();
        change_plan(&mut case, |plan| {
            plan["notes"][0]["output_path"] = json!("notes/a.md");
            plan["notes"][1]["output_path"] = json!("notes/a.md-more.md");
            let mut third = plan["notes"][1].clone();
            third["note_id"] = json!("Nnew");
            third["output_path"] = json!("notes/a.md/child.md");
            third["sections"][0]["section_id"] = json!("Hnew");
            third["sections"][0]["ordered_nodes"] = json!([]);
            plan["notes"].as_array_mut().unwrap().push(third);
        });
        has_rule(&validate_plan(&case).0, "PLAN_PATH");
    }

    #[test]
    fn ambiguous_headings_do_not_resolve_to_a_canonical_link_target() {
        let mut case = fixture();
        change_plan(&mut case, |plan| {
            let mut second = plan["notes"][0]["sections"][0].clone();
            second["section_id"] = json!("Hnew");
            second["ordered_nodes"] = json!([]);
            plan["notes"][0]["sections"]
                .as_array_mut()
                .unwrap()
                .push(second);
        });
        let (issues, targets) = validate_plan(&case);
        has_rule(&issues, "PLAN_HEADING");
        assert!(!targets.iter().any(|target| target.concept_id == "C2f"));
        has_rule(&issues, "PLAN_LINK_TARGET");
    }

    fn append_to_display(case: &mut LoadedCase, location: &str, suffix: &str) {
        change_plan(case, |plan| {
            let value = match location {
                "paragraph" => &mut plan["notes"][0]["sections"][0]["ordered_nodes"][0]["text"],
                "item" => {
                    &mut plan["notes"][1]["sections"][0]["ordered_nodes"][1]["items"][0]["text"]
                }
                "content" => {
                    &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1]["rows"][0]["cells"][1]
                        ["text"]
                }
                "label" => {
                    &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1]["rows"][0]["cells"][0]
                        ["text"]
                }
                "missing" => {
                    &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1]["rows"][1]["cells"][1]
                        ["text"]
                }
                "column" => {
                    &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1]["columns"][0]["label"]
                }
                "display_text" => {
                    &mut plan["notes"][0]["sections"][0]["ordered_nodes"][0]["links"][0]["display_text"]
                }
                _ => unreachable!(),
            };
            *value = json!(format!("{} {suffix}", value.as_str().unwrap()));
        });
    }

    #[test]
    fn reviewed_evidence_ranges_cannot_shrink_to_nonknowledge_prefixes() {
        let mut case = fixture();
        let prefix_hash = crate::text::sha256("仅".as_bytes());
        change_ir(&mut case, |ir| {
            let evidence = ir["evidence"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|evidence| evidence["evidence_id"] == "E5a")
                .unwrap();
            evidence["excerpt_end_byte"] = json!(3);
            evidence["excerpt_sha256"] = json!(prefix_hash);
        });
        assert!(crate::fixture_knowledge::validate_knowledge(&case).is_empty());
        has_rule(&validate_expected(&case), "EXPECTED_EVIDENCE");
    }

    #[test]
    fn reviewed_evidence_inventory_and_every_evidence_field_are_fixed() {
        for mutation in [
            "omitted", "extra", "identity", "span", "start", "end", "sha",
        ] {
            let mut case = fixture();
            change_ir(&mut case, |ir| match mutation {
                "omitted" => {
                    ir["evidence"].as_array_mut().unwrap().pop();
                }
                "extra" => {
                    let mut evidence = ir["evidence"][0].clone();
                    evidence["evidence_id"] = json!("Enew");
                    ir["evidence"].as_array_mut().unwrap().push(evidence);
                }
                "identity" => ir["evidence"][0]["evidence_id"] = json!("Enew"),
                "span" => ir["evidence"][0]["source_span_id"] = json!("P7b"),
                "start" => ir["evidence"][0]["excerpt_start_byte"] = json!(3),
                "end" => ir["evidence"][0]["excerpt_end_byte"] = json!(3),
                "sha" => ir["evidence"][0]["excerpt_sha256"] = json!("0".repeat(64)),
                _ => unreachable!(),
            });
            has_rule(&validate_expected(&case), "EXPECTED_EVIDENCE");
        }
    }

    #[test]
    fn canonical_section_must_display_its_reviewed_explanation_atoms() {
        let mut case = fixture();
        change_plan(&mut case, |plan| {
            let definition = plan["notes"][1]["sections"][0]["ordered_nodes"]
                .as_array_mut()
                .unwrap()
                .remove(0);
            plan["notes"][0]["sections"][0]["ordered_nodes"]
                .as_array_mut()
                .unwrap()
                .push(definition);
        });
        let (issues, targets) = validate_plan(&case);
        has_rule(&issues, "PLAN_CANONICAL");
        assert!(!targets.iter().any(|target| target.concept_id == "C6h"));
        has_rule(&validate_expected(&case), "EXPECTED_CANONICAL");
    }

    #[test]
    fn reviewed_canonical_atom_ids_cannot_be_empty_duplicate_unknown_or_unrelated() {
        for atom_ids in [
            json!([]),
            json!(["A7e", "A7e"]),
            json!(["Aunknown"]),
            json!(["A5p"]),
        ] {
            let mut case = fixture();
            change_expected(&mut case, |expected| {
                expected["canonical_placements"][1]["atom_ids"] = atom_ids
            });
            has_rule(&validate_plan(&case).0, "PLAN_CANONICAL");
            has_rule(&validate_expected(&case), "EXPECTED_CANONICAL");
        }
    }

    #[test]
    fn paragraph_atom_hidden_in_html_or_obsidian_comment_is_not_complete() {
        for text in [
            "<!--甲容器是虚构的蓝砂收纳盒。-->参见乙容器。",
            "<div hidden>甲容器是虚构的蓝砂收纳盒。</div>参见乙容器。",
            "%%甲容器是虚构的蓝砂收纳盒。%%参见乙容器。",
        ] {
            let mut case = fixture();
            change_plan(&mut case, |plan| {
                plan["notes"][0]["sections"][0]["ordered_nodes"][0]["text"] = json!(text)
            });
            let (issues, targets) = validate_plan(&case);
            has_rule(&issues, "PLAN_TEXT_SYNTAX");
            has_rule(&issues, "PLAN_REQUIRED");
            assert!(!targets.iter().any(|target| target.concept_id == "C2f"));
        }
    }

    #[test]
    fn all_display_fields_reject_raw_html_and_hidden_comments() {
        for location in [
            "paragraph",
            "item",
            "content",
            "label",
            "missing",
            "column",
            "display_text",
        ] {
            for suffix in ["<span hidden>隐藏</span>", "<!--隐藏-->", "%%隐藏%%"] {
                let mut case = fixture();
                append_to_display(&mut case, location, suffix);
                has_rule(&validate_plan(&case).0, "PLAN_TEXT_SYNTAX");
            }
        }
    }

    #[test]
    fn all_display_fields_reject_unregistered_wiki_links_and_embeds() {
        for location in [
            "paragraph",
            "item",
            "content",
            "label",
            "missing",
            "column",
            "display_text",
        ] {
            for suffix in ["[[不存在#幽灵]]", "![[不存在]]"] {
                let mut case = fixture();
                append_to_display(&mut case, location, suffix);
                has_rule(&validate_plan(&case).0, "PLAN_TEXT_SYNTAX");
            }
        }
    }

    #[test]
    fn all_display_fields_reject_raw_markdown_links_images_and_definitions() {
        for location in [
            "paragraph",
            "item",
            "content",
            "label",
            "missing",
            "column",
            "display_text",
        ] {
            for suffix in [
                "[幽灵](outside.md)",
                "![幽灵](outside.png)",
                "[幽灵][target]",
                "[target]: outside.md",
                "<https://example.invalid>",
            ] {
                let mut case = fixture();
                append_to_display(&mut case, location, suffix);
                has_rule(&validate_plan(&case).0, "PLAN_TEXT_SYNTAX");
            }
        }
    }

    #[test]
    fn literal_output_paths_reject_wiki_address_control_characters() {
        for path in ["笔记/甲#容器.md", "笔记/甲[容器].md", "笔记/甲^容器.md"] {
            let mut case = fixture();
            change_plan(&mut case, |plan| {
                plan["notes"][0]["output_path"] = json!(path)
            });
            let (issues, targets) = validate_plan(&case);
            has_rule(&issues, "PLAN_PATH");
            assert!(!targets.iter().any(|target| target.concept_id == "C2f"));
        }
    }

    #[test]
    fn normal_comparison_text_and_single_brackets_remain_visible_plain_text() {
        let mut case = fixture();
        for location in [
            "paragraph",
            "item",
            "content",
            "label",
            "missing",
            "column",
            "display_text",
        ] {
            append_to_display(&mut case, location, "演示 x < 2；区间[1, 2]。");
        }
        assert!(validate_plan(&case).0.is_empty());
    }

    #[test]
    fn display_fields_reject_controls_and_numeric_email_autolinks() {
        for location in [
            "paragraph",
            "item",
            "content",
            "label",
            "missing",
            "column",
            "display_text",
        ] {
            for suffix in ["隐藏\u{1}", "<1@example.invalid>"] {
                let mut case = fixture();
                append_to_display(&mut case, location, suffix);
                has_rule(&validate_plan(&case).0, "PLAN_TEXT_SYNTAX");
            }
        }
    }
}
