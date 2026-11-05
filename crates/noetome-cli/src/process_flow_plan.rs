//! Deterministic planning from an already source-verified process document.
use std::collections::{BTreeMap, BTreeSet};

use crate::process_flow_model::*;
use crate::process_model::{ProcessDocument, Relation, RelationKind, Step, StepKind};

fn issue(code: &str, ids: impl IntoIterator<Item = String>, message: &str) -> Vec<FlowIssue> {
    vec![FlowIssue {
        code: code.into(),
        object_ids: ids
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        message: message.into(),
    }]
}

pub(crate) fn build(
    document: &ProcessDocument,
    profile: &FlowProfile,
    request_id: &str,
) -> Result<FlowPlan, Vec<FlowIssue>> {
    let steps: BTreeMap<_, _> = document.steps.iter().map(|s| (s.id.as_str(), s)).collect();
    // Source review is responsible for strict input validation. Guard references
    // here as well so damaged internal plans cannot panic during read-back.
    if steps.len() != document.steps.len()
        || steps.is_empty()
        || !steps.contains_key(document.process.entry.as_str())
        || document
            .process
            .goals
            .iter()
            .any(|g| !steps.contains_key(g.as_str()))
        || document
            .relations
            .iter()
            .any(|r| !steps.contains_key(r.left.as_str()) || !steps.contains_key(r.right.as_str()))
    {
        return Err(issue(
            "FLOW_DISCONNECTED",
            ["process".into()],
            "过程入口、目标或关系端点未形成完整的节点集合。",
        ));
    }
    let unknown_criteria: Vec<_> = document
        .steps
        .iter()
        .filter(|s| {
            s.kind == StepKind::Decision && s.criterion.as_ref().is_none_or(|c| c.trim().is_empty())
        })
        .map(|s| s.id.clone())
        .collect();
    if !unknown_criteria.is_empty() {
        return Err(issue(
            "FLOW_UNKNOWN_CRITERION",
            unknown_criteria,
            "判定缺少明确判据，不能生成真假分支表达。",
        ));
    }
    for (kind, code, message) in [
        (
            RelationKind::Correlates,
            "FLOW_CORRELATION_UNDIRECTED",
            "来源只说明相关性，不能推定执行或因果方向。",
        ),
        (
            RelationKind::Unknown,
            "FLOW_DIRECTION_UNKNOWN",
            "来源未说明关系方向，不能生成有向流程。",
        ),
    ] {
        let ids: Vec<_> = document
            .relations
            .iter()
            .filter(|r| r.kind == kind)
            .map(|r| r.id.clone())
            .collect();
        if !ids.is_empty() {
            return Err(issue(code, ids, message));
        }
    }
    let mut outgoing: BTreeMap<&str, Vec<&Relation>> =
        steps.keys().map(|id| (*id, Vec::new())).collect();
    let mut indegree: BTreeMap<&str, usize> = steps.keys().map(|id| (*id, 0)).collect();
    for relation in &document.relations {
        outgoing
            .get_mut(relation.left.as_str())
            .expect("checked endpoint")
            .push(relation);
        *indegree
            .get_mut(relation.right.as_str())
            .expect("checked endpoint") += 1;
    }
    let mut incomplete = BTreeSet::new();
    for step in &document.steps {
        let edges = &outgoing[step.id.as_str()];
        if step.kind == StepKind::Decision
            && (edges.len() != 2
                || edges.iter().any(|r| r.kind != RelationKind::Branch)
                || edges
                    .iter()
                    .filter(|r| r.branch_outcome == Some(true))
                    .count()
                    != 1
                || edges
                    .iter()
                    .filter(|r| r.branch_outcome == Some(false))
                    .count()
                    != 1)
        {
            incomplete.insert(step.id.clone());
        }
    }
    for relation in &document.relations {
        if (relation.kind == RelationKind::Branch
            && (steps[relation.left.as_str()].kind != StepKind::Decision
                || relation.branch_outcome.is_none()))
            || (relation.kind != RelationKind::Branch && relation.branch_outcome.is_some())
        {
            incomplete.insert(relation.id.clone());
        }
    }
    if !incomplete.is_empty() {
        return Err(issue(
            "FLOW_DECISION_INCOMPLETE",
            incomplete,
            "每个判定须恰有一条真分支和一条假分支，分支必须来自具有判据的判定。",
        ));
    }
    let mut ready: BTreeSet<&str> = indegree
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut topological_order = Vec::new();
    while let Some(id) = ready.pop_first() {
        topological_order.push(id.to_owned());
        for edge in &outgoing[id] {
            let remaining = indegree
                .get_mut(edge.right.as_str())
                .expect("checked endpoint");
            *remaining -= 1;
            if *remaining == 0 {
                ready.insert(edge.right.as_str());
            }
        }
    }
    if topological_order.len() != steps.len() {
        return Err(issue(
            "FLOW_UNSUPPORTED_CYCLE",
            indegree
                .iter()
                .filter(|(_, n)| **n > 0)
                .map(|(id, _)| (*id).to_owned()),
            "来源关系含有循环，本版流程模型未支持循环表达。",
        ));
    }
    let mut reachable = BTreeSet::new();
    let mut pending = vec![document.process.entry.as_str()];
    while let Some(id) = pending.pop() {
        if reachable.insert(id) {
            pending.extend(outgoing[id].iter().map(|r| r.right.as_str()));
        }
    }
    if reachable.len() != steps.len() {
        return Err(issue(
            "FLOW_DISCONNECTED",
            steps
                .keys()
                .filter(|id| !reachable.contains(**id))
                .map(|id| (*id).to_owned()),
            "存在不能从所声明入口到达的步骤，不能补造缺失关系。",
        ));
    }
    let terminals: BTreeSet<_> = outgoing
        .iter()
        .filter(|(_, edges)| edges.is_empty())
        .map(|(id, _)| *id)
        .collect();
    let goals: BTreeSet<_> = document.process.goals.iter().map(String::as_str).collect();
    if goals != terminals || goals.len() != document.process.goals.len() {
        return Err(issue(
            "FLOW_DISCONNECTED",
            ["process".to_owned()].into_iter().chain(
                goals
                    .symmetric_difference(&terminals)
                    .map(|id| (*id).to_owned()),
            ),
            "所声明目标须与流程终点逐项一致；当前目标与终点集合不一致。",
        ));
    }

    let mut fallback_reasons = Vec::new();
    if !(profile.schema_version == 1
        && profile.producer == "flow_render_profile"
        && profile.profile_id == "obsidian_1_6_7_flowchart"
        && profile.app == "Obsidian"
        && profile.app_version == "1.6.7"
        && profile.mermaid_version == "10.6.1"
        && profile.flowchart_support == FlowchartSupport::Supported
        && profile.label_encoding == "decimal_ascii_punctuation_v1"
        && profile.html_labels)
    {
        fallback_reasons.push(FallbackReason {
            code: "FLOW_TARGET_UNVERIFIED".into(),
            object_ids: vec![],
            message: "所选目标版本或标签配置尚无本版实测依据，使用保留完整关系的文字表达。".into(),
        });
    }
    let whitespace_ids: Vec<_> = document
        .steps
        .iter()
        .filter(|s| {
            unverified_label_whitespace(&s.label)
                || s.criterion
                    .as_ref()
                    .is_some_and(|c| unverified_label_whitespace(c))
        })
        .map(|s| s.id.clone())
        .collect();
    if !whitespace_ids.is_empty() {
        fallback_reasons.push(FallbackReason {
            code: "FLOW_LABEL_WHITESPACE_FALLBACK".into(),
            object_ids: whitespace_ids,
            message: "图标签含当前显示契约未确认可保真的空白，改用预格式文字保留原字符。".into(),
        });
    }
    let representation = if fallback_reasons.is_empty() {
        Representation::Flowchart
    } else {
        Representation::OrderedList
    };
    let diagram = if representation == Representation::Flowchart {
        let node_ids: BTreeMap<_, _> = document
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.as_str(), format!("n{i}")))
            .collect();
        Some(FlowDiagram {
            direction: "TD".into(),
            nodes: document
                .steps
                .iter()
                .map(|s| FlowNode {
                    node_id: node_ids[s.id.as_str()].clone(),
                    step_id: s.id.clone(),
                    kind: s.kind,
                    text: node_text(s),
                })
                .collect(),
            edges: document
                .relations
                .iter()
                .enumerate()
                .map(|(i, r)| FlowEdge {
                    edge_id: format!("e{i}"),
                    relation_id: r.id.clone(),
                    left_node_id: node_ids[r.left.as_str()].clone(),
                    right_node_id: node_ids[r.right.as_str()].clone(),
                    kind: r.kind,
                    branch_outcome: r.branch_outcome,
                    text: edge_text(r, &steps),
                })
                .collect(),
        })
    } else {
        None
    };
    let notes = notes(
        document,
        representation,
        &topological_order,
        &steps,
        &fallback_reasons,
    );
    Ok(FlowPlan {
        schema_version: 1,
        producer: "knowledge_flow".into(),
        scope: "active_flow_v1".into(),
        representation_origin: "deterministic_planner".into(),
        request_id: request_id.into(),
        revision: 1,
        knowledge_status: "pending".into(),
        scientific_quality: "not_run".into(),
        research_status: "not_requested".into(),
        published: false,
        document: document.clone(),
        profile: profile.clone(),
        representation,
        decision: FlowDecision {
            rule_id: if representation == Representation::Flowchart {
                "verified_directed_acyclic_flow_v1"
            } else {
                "verified_relations_text_fallback_v1"
            }
            .into(),
            reason: if representation == Representation::Flowchart {
                "来源明确给出完整有向无环关系，且所选目标的当前图形及标签配置已有实测依据。"
            } else {
                "来源关系已通过流程语义检查；当前显示条件采用有据文字回退。"
            }
            .into(),
            fallback_reasons,
        },
        topological_order,
        diagram,
        notes,
    })
}

fn unverified_label_whitespace(text: &str) -> bool {
    // HTML labels collapse some whitespace. Only a single internal ASCII
    // space belongs to the measured display contract; other whitespace is
    // preserved in text without claiming that every target would collapse it.
    text.starts_with(' ')
        || text.ends_with(' ')
        || text.contains("  ")
        || text.chars().any(|c| c.is_whitespace() && c != ' ')
}

fn node_text(step: &Step) -> String {
    match &step.criterion {
        Some(criterion) if step.kind == StepKind::Decision => {
            format!("{}：{criterion}", step.label)
        }
        _ => step.label.clone(),
    }
}
fn edge_text(relation: &Relation, steps: &BTreeMap<&str, &Step>) -> String {
    match relation.kind {
        RelationKind::Sequence => "随后".into(),
        RelationKind::Causes => "导致".into(),
        RelationKind::Promotes => "+：促进".into(),
        RelationKind::Inhibits => "−：抑制".into(),
        RelationKind::Branch => format!(
            "判据为{}：{}",
            if relation.branch_outcome == Some(true) {
                "真"
            } else {
                "假"
            },
            steps[relation.left.as_str()]
                .criterion
                .as_deref()
                .unwrap_or("")
        ),
        RelationKind::Correlates => "相关（无推定方向）".into(),
        RelationKind::Unknown => "方向未说明".into(),
    }
}
fn array_text(label: &str, values: &[String]) -> String {
    if values.is_empty() {
        format!("{label}：选定来源未列出")
    } else {
        format!(
            "{label}：\n{}",
            values
                .iter()
                .enumerate()
                .map(|(i, v)| format!("{}. {v}", i + 1))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}
fn step_text(step: &Step) -> String {
    format!(
        "步骤：{}\n类型：{}\n名称：{}\n说明：{}\n判据：{}\n{}\n{}\n{}\n{}",
        step.id,
        if step.kind == StepKind::Decision {
            "判定"
        } else {
            "步骤"
        },
        step.label,
        step.description,
        step.criterion.as_deref().unwrap_or("不适用（普通步骤）"),
        array_text("输入", &step.inputs),
        array_text("输出", &step.outputs),
        array_text("条件", &step.conditions),
        array_text("例外", &step.exceptions)
    )
}
fn relation_text(relation: &Relation, steps: &BTreeMap<&str, &Step>) -> String {
    format!(
        "关系：{}\n起点：{}\n起点名称：{}\n终点：{}\n终点名称：{}\n含义：{}\n{}\n{}",
        relation.id,
        relation.left,
        steps[relation.left.as_str()].label,
        relation.right,
        steps[relation.right.as_str()].label,
        edge_text(relation, steps),
        array_text("条件", &relation.conditions),
        array_text("例外", &relation.exceptions)
    )
}
fn text(block_id: &str, body: String, ids: Vec<String>) -> FlowBlock {
    FlowBlock::Text {
        content: FlowText {
            block_id: block_id.into(),
            text: body,
            source_object_ids: ids,
        },
    }
}
fn detail_link(heading: String, alias: &str, ids: Vec<String>) -> FlowLink {
    FlowLink {
        target_path: "过程明细.md".into(),
        target_heading: heading,
        display_text: alias.into(),
        source_object_ids: ids,
    }
}
fn relation_ids(relation: &Relation) -> Vec<String> {
    // Endpoint labels are displayed alongside relation semantics in the details.
    let mut ids = vec![
        relation.id.clone(),
        relation.left.clone(),
        relation.right.clone(),
    ];
    ids.sort();
    ids.dedup();
    ids
}
fn notes(
    document: &ProcessDocument,
    representation: Representation,
    order: &[String],
    steps: &BTreeMap<&str, &Step>,
    fallback: &[FallbackReason],
) -> Vec<FlowNote> {
    let mut overview_blocks = vec![text("process_title", format!("过程名称：{}", document.process.title), vec!["process".into()]),
        text("reading_scope", "图和列表仅表达来源明确列出的关系。步骤的输入、输出、条件和例外，以及每条关系的条件和例外，见过程明细；这些限定仍然适用。文字编号用于定位和阅读，不另行主张步骤之间的执行顺序。".into(), vec![])];
    if representation == Representation::Flowchart {
        overview_blocks.push(FlowBlock::Diagram {
            block_id: "process_diagram".into(),
        });
    } else {
        overview_blocks.push(text(
            "fallback_reason",
            fallback
                .iter()
                .map(|r| r.message.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            vec![],
        ));
    }
    let mut links = vec![detail_link(
        "过程说明".into(),
        "查看完整过程说明",
        vec!["process".into()],
    )];
    links.extend(
        document
            .steps
            .iter()
            .map(|s| detail_link(format!("步骤-{}", s.id), "查看步骤明细", vec![s.id.clone()])),
    );
    links.extend(
        document
            .relations
            .iter()
            .map(|r| detail_link(format!("关系-{}", r.id), "查看关系明细", vec![r.id.clone()])),
    );
    overview_blocks.push(FlowBlock::Links {
        block_id: "detail_links".into(),
        links,
    });
    let mut overview_sections = vec![FlowSection {
        section_id: "flow".into(),
        heading: "流程表达".into(),
        blocks: overview_blocks,
    }];
    if representation == Representation::OrderedList {
        overview_sections.push(FlowSection {
            section_id: "step_list".into(),
            heading: "步骤列表".into(),
            blocks: vec![FlowBlock::OrderedList {
                block_id: "steps".into(),
                items: order
                    .iter()
                    .map(|id| FlowListItem {
                        item_id: id.clone(),
                        text: step_text(steps[id.as_str()]),
                        source_object_ids: vec![id.clone()],
                        links: vec![detail_link(
                            format!("步骤-{id}"),
                            "查看步骤明细",
                            vec![id.clone()],
                        )],
                    })
                    .collect(),
            }],
        });
        overview_sections.push(FlowSection {
            section_id: "relation_list".into(),
            heading: "明确关系列表".into(),
            blocks: vec![FlowBlock::OrderedList {
                block_id: "relations".into(),
                items: document
                    .relations
                    .iter()
                    .map(|r| FlowListItem {
                        item_id: r.id.clone(),
                        text: relation_text(r, steps),
                        source_object_ids: relation_ids(r),
                        links: vec![detail_link(
                            format!("关系-{}", r.id),
                            "查看关系明细",
                            vec![r.id.clone()],
                        )],
                    })
                    .collect(),
            }],
        });
    }
    let process = &document.process;
    let mut detail_sections = vec![FlowSection {
        section_id: "process_detail".into(),
        heading: "过程说明".into(),
        blocks: vec![text(
            "process",
            format!(
                "过程名称：{}\n入口：{}\n{}\n{}\n{}",
                process.title,
                process.entry,
                array_text("目标", &process.goals),
                array_text("条件", &process.conditions),
                array_text("例外", &process.exceptions)
            ),
            vec!["process".into()],
        )],
    }];
    detail_sections.extend(document.steps.iter().map(|s| FlowSection {
        section_id: format!("step_{}", s.id),
        heading: format!("步骤-{}", s.id),
        blocks: vec![text(&s.id, step_text(s), vec![s.id.clone()])],
    }));
    detail_sections.extend(document.relations.iter().map(|r| FlowSection {
        section_id: format!("relation_{}", r.id),
        heading: format!("关系-{}", r.id),
        blocks: vec![text(&r.id, relation_text(r, steps), relation_ids(r))],
    }));
    detail_sections.push(FlowSection {
        section_id: "return".into(),
        heading: "返回总览".into(),
        blocks: vec![FlowBlock::Links {
            block_id: "return_link".into(),
            links: vec![FlowLink {
                target_path: "流程总览.md".into(),
                target_heading: "流程表达".into(),
                display_text: "返回流程总览".into(),
                source_object_ids: vec!["process".into()],
            }],
        }],
    });
    vec![
        FlowNote {
            note_id: "flow_overview".into(),
            output_path: "流程总览.md".into(),
            title: "流程总览".into(),
            sections: overview_sections,
        },
        FlowNote {
            note_id: "process_details".into(),
            output_path: "过程明细.md".into(),
            title: "过程明细".into(),
            sections: detail_sections,
        },
    ]
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::process_model::ProcessGold;

    pub(crate) fn profile() -> FlowProfile {
        serde_json::from_str(include_str!(
            "../../../fixtures/process-flow-v1/profile-obsidian_1_6_7_flowchart.json"
        ))
        .unwrap()
    }
    pub(crate) fn document() -> ProcessDocument {
        serde_json::from_str::<ProcessGold>(include_str!(
            "../../../fixtures/process-flow-v1/gold-linear.json"
        ))
        .unwrap()
        .document()
    }
    #[test]
    fn frozen_semantic_cases_have_their_primary_issue() {
        let cases = [
            (
                include_str!("../../../fixtures/process-flow-v1/gold-correlation.json"),
                "FLOW_CORRELATION_UNDIRECTED",
            ),
            (
                include_str!("../../../fixtures/process-flow-v1/gold-direction_unknown.json"),
                "FLOW_DIRECTION_UNKNOWN",
            ),
            (
                include_str!("../../../fixtures/process-flow-v1/gold-cycle.json"),
                "FLOW_UNSUPPORTED_CYCLE",
            ),
            (
                include_str!("../../../fixtures/process-flow-v1/gold-missing_branch.json"),
                "FLOW_DECISION_INCOMPLETE",
            ),
            (
                include_str!("../../../fixtures/process-flow-v1/gold-missing_connection.json"),
                "FLOW_DISCONNECTED",
            ),
            (
                include_str!("../../../fixtures/process-flow-v1/gold-unknown_criterion.json"),
                "FLOW_UNKNOWN_CRITERION",
            ),
        ];
        for (gold, code) in cases {
            let document = serde_json::from_str::<ProcessGold>(gold)
                .unwrap()
                .document();
            let issues = build(&document, &profile(), "test-request").unwrap_err();
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].code, code);
        }
    }
    #[test]
    fn profile_claim_cannot_promote_an_unmeasured_target() {
        let mut p = profile();
        p.app_version = "9.9.9".into();
        let plan = build(&document(), &p, "test-request").unwrap();
        assert_eq!(plan.representation, Representation::OrderedList);
        assert!(plan.diagram.is_none());
        assert_eq!(
            plan.decision.fallback_reasons[0].code,
            "FLOW_TARGET_UNVERIFIED"
        );
    }
    #[test]
    fn multiline_labels_fall_back_without_losing_characters_or_semantics() {
        let doc = serde_json::from_str::<ProcessGold>(include_str!(
            "../../../fixtures/process-flow-v1/gold-multiline_labels.json"
        ))
        .unwrap()
        .document();
        let mut p = profile();
        p.html_labels = false;
        let plan = build(&doc, &p, "test-request").unwrap();
        assert_eq!(plan.document, doc);
        assert_eq!(plan.representation, Representation::OrderedList);
        assert_eq!(
            plan.decision
                .fallback_reasons
                .iter()
                .map(|r| r.code.as_str())
                .collect::<Vec<_>>(),
            vec!["FLOW_TARGET_UNVERIFIED", "FLOW_LABEL_WHITESPACE_FALLBACK"]
        );
        for step in &doc.steps {
            let section = &plan.notes[1]
                .sections
                .iter()
                .find(|s| s.heading == format!("步骤-{}", step.id))
                .unwrap();
            let FlowBlock::Text { content } = &section.blocks[0] else {
                panic!("complete step text expected")
            };
            assert!(content.text.contains(&step.label));
        }
    }
    #[test]
    fn label_and_criterion_whitespace_have_a_conservative_display_boundary() {
        for (text, fallback) in [
            ("校验卡片", false),
            ("校验 卡片", false),
            (" 校验卡片", true),
            ("校验卡片 ", true),
            ("校验  卡片", true),
            ("校验\t卡片", true),
            ("校验\n卡片", true),
            ("校验\u{00a0}卡片", true),
            ("校验\u{2028}卡片", true),
            ("校验\u{2029}卡片", true),
            ("校验\u{3000}卡片", true),
        ] {
            for criterion in [false, true] {
                let mut doc = if criterion {
                    serde_json::from_str::<ProcessGold>(include_str!(
                        "../../../fixtures/process-flow-v1/gold-branch.json"
                    ))
                    .unwrap()
                    .document()
                } else {
                    document()
                };
                let step = if criterion {
                    doc.steps
                        .iter_mut()
                        .find(|s| s.kind == StepKind::Decision)
                        .unwrap()
                } else {
                    &mut doc.steps[0]
                };
                let step_id = step.id.clone();
                if criterion {
                    step.criterion = Some(text.into());
                } else {
                    step.label = text.into();
                }
                let plan = build(&doc, &profile(), "test-request").unwrap();
                assert_eq!(plan.document, doc, "source fields must stay byte-exact");
                assert_eq!(
                    plan.representation,
                    if fallback {
                        Representation::OrderedList
                    } else {
                        Representation::Flowchart
                    },
                    "{text:?}/criterion={criterion}"
                );
                if fallback {
                    assert_eq!(plan.decision.fallback_reasons.len(), 1);
                    assert_eq!(
                        plan.decision.fallback_reasons[0].code,
                        "FLOW_LABEL_WHITESPACE_FALLBACK"
                    );
                    assert_eq!(
                        plan.decision.fallback_reasons[0].object_ids,
                        vec![step_id.clone()]
                    );
                } else {
                    assert!(plan.decision.fallback_reasons.is_empty());
                }
                let details = plan.notes[1]
                    .sections
                    .iter()
                    .find(|s| s.heading == format!("步骤-{step_id}"))
                    .unwrap();
                let FlowBlock::Text { content } = &details.blocks[0] else {
                    panic!("complete step text expected")
                };
                assert!(content.text.contains(text), "detail text retains {text:?}");
            }
        }
    }
    #[test]
    fn body_only_whitespace_does_not_disable_an_otherwise_verified_diagram() {
        let mut doc = document();
        let body = " 正文  原字符\t保留\n保留\u{00a0}\u{2028}\u{2029}\u{3000} ";
        doc.steps[0].description = body.into();
        doc.steps[0].inputs = vec![body.into()];
        doc.steps[0].outputs = vec![body.into()];
        doc.steps[0].conditions = vec![body.into()];
        doc.steps[0].exceptions = vec![body.into()];
        let plan = build(&doc, &profile(), "test-request").unwrap();
        assert_eq!(plan.representation, Representation::Flowchart);
        assert_eq!(plan.document, doc);
        let details = plan.notes[1]
            .sections
            .iter()
            .find(|s| s.heading == format!("步骤-{}", doc.steps[0].id))
            .unwrap();
        let FlowBlock::Text { content } = &details.blocks[0] else {
            panic!("complete step text expected")
        };
        assert_eq!(content.text.matches(body).count(), 5);
    }
    #[test]
    fn detached_cycle_is_diagnosed_before_secondary_disconnection() {
        let mut doc = document();
        let mut step = doc.steps[0].clone();
        step.id = "detached".into();
        doc.steps.push(step);
        let mut relation = doc.relations[0].clone();
        relation.id = "loop".into();
        relation.left = "detached".into();
        relation.right = "detached".into();
        doc.relations.push(relation);
        assert_eq!(
            build(&doc, &profile(), "test-request").unwrap_err()[0].code,
            "FLOW_UNSUPPORTED_CYCLE"
        );
    }
    #[test]
    fn goals_must_equal_actual_terminal_set() {
        let mut doc = document();
        doc.process.goals = vec![doc.process.entry.clone()];
        assert_eq!(
            build(&doc, &profile(), "test-request").unwrap_err()[0].code,
            "FLOW_DISCONNECTED"
        );
    }
    #[test]
    fn strict_profile_rejects_missing_extra_and_codec_fields() {
        let value = serde_json::to_value(profile()).unwrap();
        for field in [
            "schema_version",
            "producer",
            "profile_id",
            "app",
            "app_version",
            "mermaid_version",
            "flowchart_support",
            "label_encoding",
            "html_labels",
        ] {
            let mut bad = value.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<FlowProfile>(bad).is_err(),
                "{field}"
            );
        }
        let mut bad = value.clone();
        bad["extra"] = true.into();
        assert!(serde_json::from_value::<FlowProfile>(bad).is_err());
        let mut bad = value;
        bad["label_encoding"] = "raw".into();
        assert!(serde_json::from_value::<FlowProfile>(bad).is_err());
    }
}
