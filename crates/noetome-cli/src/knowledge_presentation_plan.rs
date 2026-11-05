//! Deterministic expression choices over completely reconstructed, source-held facts.
use crate::{
    knowledge_coordinate_input::check,
    knowledge_presentation_input::{PresentationInputs, error},
    knowledge_presentation_model::*,
    project::ProjectError,
    sidecar_process::Control,
};
use std::collections::{BTreeMap, BTreeSet};

fn unique(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|v| seen.insert(v.clone()))
        .collect()
}

pub(crate) fn fact_text(fact: &PresentationFact) -> Result<String, ProjectError> {
    let list = |values: &[String]| {
        if values.is_empty() {
            "未列出".into()
        } else {
            values.join("；")
        }
    };
    let applicability = match fact.applicability_status.as_deref() {
        None => "适用背景：本协调版本未声明",
        Some("known") => "适用条件：已列明",
        Some("unknown") => "适用条件：未知",
        Some("not_applicable") => "适用条件：不适用",
        _ => return Err(error("PRESENT_SCHEMA", 2, "unknown applicability status")),
    };
    let time = match &fact.time {
        None => "本协调版本未声明".into(),
        Some(time)
            if time.status == "unknown" && time.value.is_none() && time.precision.is_none() =>
        {
            "未知".into()
        }
        Some(time) if time.status == "known" => {
            let precision = match time.precision.as_deref() {
                Some("year") => "年",
                Some("day") => "日",
                _ => return Err(error("PRESENT_SCHEMA", 2, "missing known time precision")),
            };
            format!(
                "{}（{precision}）",
                time.value.as_ref().ok_or_else(|| error(
                    "PRESENT_SCHEMA",
                    2,
                    "missing known time"
                ))?
            )
        }
        _ => return Err(error("PRESENT_SCHEMA", 2, "inconsistent time")),
    };
    Ok(format!(
        "{} {}；条件：{}；例外：{}；{applicability}；观点时间：{time}",
        fact.value,
        fact.unit,
        list(&fact.conditions),
        list(&fact.exceptions)
    ))
}

fn detail_path(id: &str) -> String {
    format!("对象-{id}.md")
}
fn link(id: &str) -> SemanticLink {
    SemanticLink {
        target_concept_id: id.into(),
        target_path: detail_path(id),
        target_heading: "知识明细".into(),
        display_text: "查看详情".into(),
    }
}

pub(crate) fn build(
    inputs: &PresentationInputs,
    request_id: &str,
    control: &Control,
) -> Result<PresentationPlan, ProjectError> {
    if !inputs.ready() {
        return Err(error(
            "PRESENT_COORDINATION_UNRESOLVED",
            3,
            "coordination requires review",
        ));
    }
    let facts = inputs.facts()?;
    let provenance = inputs.provenance()?;
    let labels: BTreeMap<_, _> = inputs
        .case
        .map
        .concepts
        .iter()
        .map(|c| (c.canonical_id.clone(), c.display_name.clone()))
        .collect();
    let entity = |id: &String, object: bool| -> Result<EntityLabel, ProjectError> {
        let role = if object { "object_id" } else { "property_id" };
        let occurrence_ids = inputs.rebuilt["occurrences"]
            .as_array()
            .ok_or_else(|| error("PRESENT_SCHEMA", 2, "missing occurrences"))?
            .iter()
            .filter(|o| o[role] == id.as_str())
            .map(|o| {
                o["occurrence_id"]
                    .as_str()
                    .map(String::from)
                    .ok_or_else(|| error("PRESENT_SCHEMA", 2, "missing occurrence identity"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(EntityLabel {
            concept_id: id.clone(),
            label: labels
                .get(id)
                .ok_or_else(|| error("PRESENT_SCHEMA", 2, "unknown canonical identity"))?
                .clone(),
            occurrence_ids,
        })
    };
    let object_ids: BTreeSet<_> = facts.iter().map(|f| f.object_id.clone()).collect();
    let property_ids: BTreeSet<_> = facts.iter().map(|f| f.property_id.clone()).collect();
    let objects = object_ids
        .iter()
        .map(|id| entity(id, true))
        .collect::<Result<Vec<_>, _>>()?;
    let properties = property_ids
        .iter()
        .map(|id| entity(id, false))
        .collect::<Result<Vec<_>, _>>()?;
    let shared_property_ids = property_ids
        .iter()
        .filter(|id| {
            facts
                .iter()
                .filter(|f| &f.property_id == *id)
                .map(|f| &f.object_id)
                .collect::<BTreeSet<_>>()
                .len()
                >= 2
        })
        .cloned()
        .collect::<Vec<_>>();
    let (representation, rule, reason) = if !shared_property_ids.is_empty() {
        (
            Representation::Table,
            "PRESENT_SHARED_PROPERTY_TABLE",
            "多个明确对象共享已核对属性，以比较表并列完整事实；单位与条件保留，不作数值裁决。",
        )
    } else if objects.len() == 1 {
        (
            Representation::Paragraph,
            "PRESENT_SINGLE_OBJECT_TEXT",
            "选定事实仅涉及一个对象，以正文保留各属性、条件和原始表述。",
        )
    } else {
        (
            Representation::Paragraph,
            "PRESENT_NO_SHARED_PROPERTY_TEXT",
            "选定对象没有共同的显式属性，以正文分别保留各自事实。",
        )
    };
    let paragraph = |fact: &PresentationFact, details: bool| -> Result<Node, ProjectError> {
        let mut text = format!(
            "{}的{}：{}",
            labels[&fact.object_id],
            labels[&fact.property_id],
            fact_text(fact)?
        );
        if details {
            let originals = unique(
                fact.occurrence_ids
                    .iter()
                    .map(|id| {
                        provenance
                            .iter()
                            .find(|o| &o.occurrence_id == id)
                            .map(|o| o.original_atom.atom.content.clone())
                            .ok_or_else(|| error("PRESENT_SCHEMA", 2, "unknown fact occurrence"))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
            text.push_str("\n原始表述：");
            text.push_str(&originals.join("\n"));
        }
        Ok(Node::Paragraph {
            node_id: format!(
                "{}-{}",
                if details { "detail" } else { "overview" },
                fact.fact_id
            ),
            text,
            fact_ids: vec![fact.fact_id.clone()],
            occurrence_ids: fact.occurrence_ids.clone(),
            concept_ids: vec![fact.object_id.clone(), fact.property_id.clone()],
            links: if details {
                vec![]
            } else {
                vec![link(&fact.object_id)]
            },
        })
    };
    let mut overview = Note {
        note_id: "overview".into(),
        title: "知识比较".into(),
        output_path: "知识比较.md".into(),
        section_id: "overview".into(),
        heading: "比较概览".into(),
        nodes: vec![],
    };
    if representation == Representation::Table {
        let mut columns = vec![Column {
            column_id: "object".into(),
            property_id: None,
            label: "对象".into(),
            concept_ids: vec![],
            occurrence_ids: vec![],
        }];
        columns.extend(properties.iter().map(|p| Column {
            column_id: p.concept_id.clone(),
            property_id: Some(p.concept_id.clone()),
            label: p.label.clone(),
            concept_ids: vec![p.concept_id.clone()],
            occurrence_ids: p.occurrence_ids.clone(),
        }));
        let mut rows = vec![];
        for object in &objects {
            check(control)?;
            let mut cells = vec![Cell {
                column_id: "object".into(),
                kind: CellKind::Label,
                text: object.label.clone(),
                fact_ids: vec![],
                occurrence_ids: object.occurrence_ids.clone(),
                concept_ids: vec![object.concept_id.clone()],
                links: vec![link(&object.concept_id)],
            }];
            for property in &properties {
                let matching = facts
                    .iter()
                    .filter(|f| {
                        f.object_id == object.concept_id && f.property_id == property.concept_id
                    })
                    .collect::<Vec<_>>();
                let cell = if matching.is_empty() {
                    Cell {
                        column_id: property.concept_id.clone(),
                        kind: CellKind::Missing,
                        text: "选定事实未给出".into(),
                        fact_ids: vec![],
                        occurrence_ids: vec![],
                        concept_ids: vec![],
                        links: vec![],
                    }
                } else {
                    Cell {
                        column_id: property.concept_id.clone(),
                        kind: CellKind::Facts,
                        text: matching
                            .iter()
                            .map(|f| fact_text(f))
                            .collect::<Result<Vec<_>, _>>()?
                            .join("\n"),
                        fact_ids: matching.iter().map(|f| f.fact_id.clone()).collect(),
                        occurrence_ids: unique(
                            matching.iter().flat_map(|f| f.occurrence_ids.clone()),
                        ),
                        concept_ids: vec![object.concept_id.clone(), property.concept_id.clone()],
                        links: vec![],
                    }
                };
                cells.push(cell);
            }
            rows.push(Row {
                row_id: object.concept_id.clone(),
                object_id: object.concept_id.clone(),
                cells,
            });
        }
        overview.nodes.push(Node::Table {
            node_id: "comparison".into(),
            columns,
            rows,
        });
    } else {
        overview.nodes = facts
            .iter()
            .map(|f| paragraph(f, false))
            .collect::<Result<Vec<_>, _>>()?;
    }
    let mut notes = vec![overview];
    for object in &objects {
        check(control)?;
        notes.push(Note {
            note_id: format!("object-{}", object.concept_id),
            title: object.label.clone(),
            output_path: detail_path(&object.concept_id),
            section_id: "details".into(),
            heading: "知识明细".into(),
            nodes: facts
                .iter()
                .filter(|f| f.object_id == object.concept_id)
                .map(|f| paragraph(f, true))
                .collect::<Result<Vec<_>, _>>()?,
        });
    }
    Ok(PresentationPlan {
        schema_version: 1,
        producer: "knowledge_presentation".into(),
        scope: "active_presentation_v1".into(),
        representation_origin: "deterministic_planner".into(),
        revision: 1,
        request_id: request_id.into(),
        project_id: inputs.case.config.project_id.clone(),
        coordination_schema_version: inputs.schema,
        coordination_ir_fingerprint: inputs
            .ir_fingerprint
            .clone()
            .ok_or_else(|| error("PRESENT_SCHEMA", 2, "ready coordination lacks IR"))?,
        knowledge_status: "pending".into(),
        published: false,
        scientific_quality: "not_run".into(),
        research_status: "not_requested".into(),
        representation,
        decision: Decision {
            rule_id: rule.into(),
            reason: reason.into(),
            shared_property_ids,
        },
        objects,
        properties,
        facts,
        provenance,
        notes,
    })
}
