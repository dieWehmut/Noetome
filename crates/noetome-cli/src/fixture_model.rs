//! Versioned fixture input, distinct from production persisted identities.

use serde::{Deserialize, Serialize};

use crate::text::{ParseStatus, SpanKind};

macro_rules! model {
    ($name:ident { $($(#[$attribute:meta])* $field:ident : $kind:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct $name {
            $($(#[$attribute])* pub(crate) $field: $kind),*
        }
    };
}

macro_rules! enumeration {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub(crate) enum $name { $($variant),* }
    };
}

enumeration!(Producer { Fixture });
enumeration!(AtomKind {
    Definition,
    Statement,
    ComparisonValue
});
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
enumeration!(CoverageDisposition {
    Pending,
    Included,
    Excluded,
    Duplicate,
    Failed,
    NeedsReview
});
enumeration!(CoverageStage {
    Registered,
    Mapped,
    Planned,
    Rendered,
    Validated
});
enumeration!(DecisionSource {
    FixtureReview,
    DeterministicFormat
});
enumeration!(CellKind {
    Label,
    Content,
    Missing
});

fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

model!(Manifest {
    schema_version: u32,
    producer: Producer,
    case_id: String,
    sources: Vec<FixtureSource>,
    ir_file: String,
    note_plan_file: String,
    expected_file: String,
});

model!(FixtureSource {
    source_id: String,
    version_id: String,
    relative_path: String,
    media_type: String,
    content_sha256: String,
    normalized_sha256: String,
    normalization_version: String,
    parser_version: String,
    spans: Vec<DeclaredSpan>,
});

model!(DeclaredSpan {
    span_id: String,
    ordinal: usize,
    kind: SpanKind,
    start_byte: usize,
    end_byte: usize,
    start_line: usize,
    end_line: usize,
    text_sha256: String,
    parse_status: ParseStatus,
});

model!(KnowledgeIr {
    schema_version: u32,
    producer: Producer,
    revision: u64,
    coverage_run_id: String,
    sources: Vec<IrSourceRef>,
    concepts: Vec<Concept>,
    evidence: Vec<Evidence>,
    atoms: Vec<KnowledgeAtom>,
    coverage: Vec<CoverageRecord>,
});

model!(IrSourceRef {
    source_id: String,
    version_id: String,
    content_sha256: String,
    normalized_sha256: String,
});

model!(Concept {
    concept_id: String,
    revision: u64,
    canonical_name: String,
    aliases: Vec<String>,
    needs_full_explanation: bool,
});

model!(Evidence {
    evidence_id: String,
    source_span_id: String,
    excerpt_start_byte: usize,
    excerpt_end_byte: usize,
    excerpt_sha256: String,
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
    relation: EvidenceRelation,
});

model!(CoverageRecord {
    run_id: String,
    span_id: String,
    disposition: CoverageDisposition,
    atom_ids: Vec<String>,
    required_atom_ids: Vec<String>,
    #[serde(deserialize_with = "required_option")]
    duplicate_of_span_id: Option<String>,
    #[serde(deserialize_with = "required_option")]
    exclusion_reason: Option<String>,
    decision_source: DecisionSource,
    policy_version: String,
    stage: CoverageStage,
});

model!(PlanPackage {
    schema_version: u32,
    producer: Producer,
    revision: u64,
    knowledge_revision: u64,
    knowledge_sha256: String,
    notes: Vec<Note>,
    placements: Vec<Placement>,
});

model!(Note {
    note_id: String,
    revision: u64,
    title: String,
    output_path: String,
    sections: Vec<Section>,
});

model!(Section {
    section_id: String,
    heading: String,
    ordered_nodes: Vec<Node>,
});

model!(Placement {
    concept_id: String,
    note_id: String,
    section_id: String,
    canonical: bool,
});

model!(SemanticLink {
    target_concept_id: String,
    display_text: String,
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Node {
    Paragraph {
        node_id: String,
        representation_origin: Producer,
        atom_ids: Vec<String>,
        text: String,
        links: Vec<SemanticLink>,
    },
    OrderedList {
        node_id: String,
        representation_origin: Producer,
        atom_ids: Vec<String>,
        items: Vec<ListItem>,
    },
    Table {
        node_id: String,
        representation_origin: Producer,
        atom_ids: Vec<String>,
        columns: Vec<TableColumn>,
        rows: Vec<TableRow>,
    },
}

impl Node {
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::Paragraph { node_id, .. }
            | Self::OrderedList { node_id, .. }
            | Self::Table { node_id, .. } => node_id,
        }
    }

    pub(crate) fn atom_ids(&self) -> &[String] {
        match self {
            Self::Paragraph { atom_ids, .. }
            | Self::OrderedList { atom_ids, .. }
            | Self::Table { atom_ids, .. } => atom_ids,
        }
    }
}

model!(ListItem {
    item_id: String,
    text: String,
    atom_ids: Vec<String>,
    links: Vec<SemanticLink>,
});

model!(TableColumn {
    column_id: String,
    label: String
});
model!(TableRow { row_id: String, cells: Vec<TableCell> });
model!(TableCell {
    kind: CellKind,
    text: String,
    atom_ids: Vec<String>,
    links: Vec<SemanticLink>,
});

model!(ExpectedPackage {
    schema_version: u32,
    producer: Producer,
    case_id: String,
    required_atoms: Vec<ExpectedAtom>,
    evidence: Vec<Evidence>,
    coverage: Vec<ExpectedCoverage>,
    canonical_placements: Vec<ExpectedPlacement>,
    required_links: Vec<ExpectedLink>,
    tables: Vec<ExpectedTable>,
});

model!(ExpectedAtom {
    atom_id: String,
    kind: AtomKind,
    concept_ids: Vec<String>,
    content: String,
    qualifiers: Vec<String>,
    evidence_span_ids: Vec<String>,
});

model!(ExpectedCoverage {
    span_id: String,
    disposition: CoverageDisposition,
    atom_ids: Vec<String>,
    required_atom_ids: Vec<String>,
    #[serde(deserialize_with = "required_option")]
    duplicate_of_span_id: Option<String>,
    #[serde(deserialize_with = "required_option")]
    exclusion_reason: Option<String>,
    decision_source: DecisionSource,
});

model!(ExpectedPlacement {
    concept_id: String,
    note_id: String,
    section_id: String,
    atom_ids: Vec<String>
});
model!(ExpectedLink {
    from_node_id: String,
    target_concept_id: String,
    display_text: String
});
model!(ExpectedTable { node_id: String, columns: Vec<TableColumn>, rows: Vec<TableRow> });

model!(Issue { rule_id: String, object_refs: Vec<String>, message: String });

impl Issue {
    pub(crate) fn new(rule: &str, refs: Vec<String>, message: &str) -> Self {
        Self {
            rule_id: rule.into(),
            object_refs: refs,
            message: message.into(),
        }
    }
}

model!(ResolvedTarget {
    concept_id: String,
    note_id: String,
    section_id: String,
    output_path: String,
    heading: String,
});

pub(crate) fn is_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

pub(crate) fn is_sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn nonempty(value: &str) -> bool {
    !value.trim().is_empty() && !value.contains('\0')
}
