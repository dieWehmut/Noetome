//! Human-readable view of the same deterministic validation facts as JSON.

use crate::compile_model::CompileReport;

fn cell(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "\\|")
        .replace('`', "\\`")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('\r', "&#13;")
        .replace('\n', "<br>")
        .replace('\t', "&#9;")
}

fn row(output: &mut String, values: &[String]) {
    output.push_str("| ");
    output.push_str(
        &values
            .iter()
            .map(|value| cell(value))
            .collect::<Vec<_>>()
            .join(" | "),
    );
    output.push_str(" |\n");
}

pub(crate) fn markdown(report: &CompileReport) -> String {
    let mut out = String::from("# Noetome fixture 验证报告\n\n");
    // Keys are stable and deliberately remain machine-readable.
    for (key, value) in [
        ("schema_version", report.schema_version.to_string()),
        ("producer", report.producer.clone()),
        ("case_id", report.case_id.clone()),
        ("run_id", report.run_id.clone()),
        ("release_id", report.release_id.clone()),
        ("started_at", report.started_at.clone()),
        ("finished_at", report.finished_at.clone()),
        ("result", report.result.clone()),
    ] {
        out.push_str(&format!("- {key}: {}\n", cell(&value)));
    }
    out.push_str("\n## 四维状态\n\n| dimension | state | reason |\n| --- | --- | --- |\n");
    for (key, dimension) in &report.dimensions {
        row(
            &mut out,
            &[
                key.clone(),
                dimension.state.clone(),
                dimension.reason.clone(),
            ],
        );
    }
    out.push_str("\n## 计数\n\n| scope | key | value |\n| --- | --- | --- |\n");
    for (key, count) in &report.counts {
        row(
            &mut out,
            &["overall".into(), key.clone(), count.to_string()],
        );
    }
    for (scope, dimension) in &report.dimensions {
        for (key, count) in &dimension.counts {
            row(&mut out, &[scope.clone(), key.clone(), count.to_string()]);
        }
    }
    out.push_str("\n## 检查\n\n| check | status | issue_count |\n| --- | --- | --- |\n");
    for (key, check) in &report.checks {
        row(
            &mut out,
            &[
                key.clone(),
                check.status.clone(),
                check.issue_count.to_string(),
            ],
        );
    }
    out.push_str("\n## 文件\n\n| relative_path | note_id | plan_revision | byte_length | content_sha256 |\n| --- | --- | --- | --- | --- |\n");
    for file in &report.files {
        row(
            &mut out,
            &[
                file.relative_path.clone(),
                file.note_id.clone(),
                file.plan_revision.to_string(),
                file.byte_length.to_string(),
                file.content_sha256.clone(),
            ],
        );
    }
    out.push_str("\n## 内容映射\n\n| relative_path | section_id | node_id | scope | atom_ids | start_byte | end_byte |\n| --- | --- | --- | --- | --- | --- | --- |\n");
    for file in &report.files {
        for mapping in &file.content_mappings {
            let scope = &mapping.scope;
            let locator = format!(
                "{}:{}:{}:{}",
                scope.kind,
                scope.item_id.as_deref().unwrap_or(""),
                scope.row_id.as_deref().unwrap_or(""),
                scope.column_id.as_deref().unwrap_or("")
            );
            row(
                &mut out,
                &[
                    file.relative_path.clone(),
                    scope.section_id.clone(),
                    scope.node_id.clone(),
                    locator,
                    mapping.atom_ids.join(","),
                    mapping.start_byte.to_string(),
                    mapping.end_byte.to_string(),
                ],
            );
        }
    }
    out.push_str("\n## 语义引用\n\n| relative_path | section_id | node_id | scope | target_concept_id | target_path | target_heading | display_text | start_byte | end_byte |\n| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for file in &report.files {
        for link in &file.links {
            let scope = &link.scope;
            let locator = format!(
                "{}:{}:{}:{}",
                scope.kind,
                scope.item_id.as_deref().unwrap_or(""),
                scope.row_id.as_deref().unwrap_or(""),
                scope.column_id.as_deref().unwrap_or("")
            );
            row(
                &mut out,
                &[
                    file.relative_path.clone(),
                    link.scope.section_id.clone(),
                    link.scope.node_id.clone(),
                    locator,
                    link.target_concept_id.clone(),
                    link.target_path.clone(),
                    link.target_heading.clone(),
                    link.display_text.clone(),
                    link.start_byte.to_string(),
                    link.end_byte.to_string(),
                ],
            );
        }
    }
    out.push_str("\n## 问题\n\n| rule_id | object_refs | message |\n| --- | --- | --- |\n");
    for issue in &report.issues {
        row(
            &mut out,
            &[
                issue.rule_id.clone(),
                issue.object_refs.join(","),
                issue.message.clone(),
            ],
        );
    }
    out.push_str("\n## 输入与规则\n\n| scope | key | value |\n| --- | --- | --- |\n");
    for (scope, values) in [
        ("input_hashes", &report.input_hashes),
        ("versions", &report.versions),
    ] {
        for (key, value) in values {
            row(&mut out, &[scope.into(), key.clone(), value.clone()]);
        }
    }
    out.push_str("\n## 发布依据\n\n验证事实通过不等于发布已经提交。只有以下数据库中与本轮文件和双报告指纹一致的已提交记录才是发布依据。\n\n");
    for (key, value) in [
        ("database", &report.publication.database),
        ("commit_table", &report.publication.commit_table),
        ("release_id", &report.publication.release_id),
    ] {
        out.push_str(&format!("- {key}: {}\n", cell(value)));
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::compile_model::{CheckFact, Dimension, PublicationRef};
    use crate::fixture_model::Issue;

    fn facts() -> CompileReport {
        CompileReport {
            schema_version: 1,
            producer: "fixture".into(),
            case_id: "C7k".into(),
            run_id: "run-example".into(),
            release_id: "release-example".into(),
            started_at: "2026-11-01T00:00:00Z".into(),
            finished_at: "2026-11-01T00:00:01Z".into(),
            result: "failed".into(),
            dimensions: BTreeMap::from([
                (
                    "source_coverage".into(),
                    Dimension {
                        state: "complete".into(),
                        reason: "12个位置已对账".into(),
                        counts: BTreeMap::from([("spans".into(), 12)]),
                    },
                ),
                (
                    "quality_validation".into(),
                    Dimension {
                        state: "incomplete".into(),
                        reason: "输出缺少单位".into(),
                        counts: BTreeMap::from([("issues".into(), 1)]),
                    },
                ),
                (
                    "optional_research".into(),
                    Dimension {
                        state: "not_applicable".into(),
                        reason: "未启用研究".into(),
                        counts: BTreeMap::new(),
                    },
                ),
            ]),
            checks: BTreeMap::from([
                (
                    "output_content".into(),
                    CheckFact {
                        status: "failed".into(),
                        issue_count: 1,
                    },
                ),
                (
                    "scientific_quality".into(),
                    CheckFact {
                        status: "not_run".into(),
                        issue_count: 0,
                    },
                ),
            ]),
            counts: BTreeMap::from([("sources".into(), 2), ("spans".into(), 12)]),
            issues: vec![Issue::new(
                "OUTPUT_CONTENT",
                vec!["A5p".into()],
                "实际正文缺少单位",
            )],
            files: Vec::new(),
            input_hashes: BTreeMap::from([("expected".into(), "0".repeat(64))]),
            versions: BTreeMap::from([("renderer".into(), "fixture-markdown-v1".into())]),
            publication: PublicationRef {
                database: "state/compile.sqlite".into(),
                commit_table: "publication_commits".into(),
                release_id: "release-example".into(),
            },
        }
    }

    #[test]
    fn markdown_preserves_failed_facts_counts_and_unrun_science() {
        let rendered = markdown(&facts()).replace("\\_", "_");
        for expected in [
            "- result: failed",
            "- run_id: run-example",
            "- release_id: release-example",
            "| source_coverage | complete | 12个位置已对账 |",
            "| quality_validation | incomplete | 输出缺少单位 |",
            "| optional_research | not_applicable | 未启用研究 |",
            "| overall | spans | 12 |",
            "| source_coverage | spans | 12 |",
            "| output_content | failed | 1 |",
            "| scientific_quality | not_run | 0 |",
            "| OUTPUT_CONTENT | A5p | 实际正文缺少单位 |",
            "- database: state/compile.sqlite",
            "- commit_table: publication_commits",
        ] {
            assert!(
                rendered.contains(expected),
                "missing {expected}: {rendered}"
            );
        }
        assert!(!rendered.contains("published"));
    }

    #[test]
    fn diagnostic_text_does_not_inject_table_columns_or_html() {
        let mut report = facts();
        report.issues[0].message = "a|b\n<script>".into();
        let rendered = markdown(&report);
        assert!(rendered.contains("a\\|b<br>&lt;script&gt;"), "{rendered}");
        assert!(!rendered.contains("\n<script>"));
    }

    #[test]
    fn underscore_delimited_diagnostic_is_visible_as_literal_text() {
        let mut report = facts();
        report.issues[0].message = "_literal_".into();
        assert!(markdown(&report).contains("\\_literal\\_"));
    }

    #[test]
    fn file_revision_and_each_link_scope_are_visible_in_markdown() {
        use crate::fixture_render::{LinkMapping, RenderedFile, ScopeLocator};
        let mut report = facts();
        report.files.push(RenderedFile {
            relative_path: "甲.md".into(),
            note_id: "N1".into(),
            plan_revision: 17,
            byte_length: 2,
            content_sha256: "1".repeat(64),
            content_mappings: Vec::new(),
            links: vec![LinkMapping {
                scope: ScopeLocator {
                    section_id: "S1".into(),
                    node_id: "T1".into(),
                    kind: "table_cell".into(),
                    item_id: None,
                    row_id: Some("R1".into()),
                    column_id: Some("C1".into()),
                },
                target_concept_id: "Concept1".into(),
                target_path: "乙.md".into(),
                target_heading: "定义".into(),
                display_text: "乙".into(),
                start_byte: 0,
                end_byte: 2,
            }],
        });
        let first = markdown(&report).replace("\\_", "_");
        assert!(first.contains("| 甲.md | N1 | 17 | 2 |"));
        assert!(first.contains("table_cell::R1:C1"));
        report.files[0].plan_revision = 18;
        report.files[0].links[0].scope.column_id = Some("C2".into());
        assert_ne!(first, markdown(&report).replace("\\_", "_"));
    }
}
