//! Source-verified active presentation in a new, unpublished test Vault.
use crate::{
    extraction_output::{self, OutputRoot},
    knowledge_coordinate_input::{check, encoded, fingerprint, parse},
    knowledge_presentation_input::{PresentationInputs, error},
    knowledge_presentation_model::*,
    knowledge_presentation_plan, knowledge_presentation_render,
    project::{self, ProjectError},
    sidecar_process::Control,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

pub struct PresentationOutput {
    pub json: String,
    pub failed: bool,
}

#[cfg(test)]
#[path = "../tests/support/presentation_guard_tests.rs"]
mod guard_tests;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/support/presentation_case.rs"]
mod test_case;

#[cfg(test)]
thread_local! {static TEST_HOOK:std::cell::RefCell<Option<Box<dyn FnMut(&str)>>> = std::cell::RefCell::new(None);}
#[cfg(test)]
fn stage(s: &str) {
    TEST_HOOK.with(|h| {
        if let Some(h) = h.borrow_mut().as_mut() {
            h(s)
        }
    });
}

fn save_checked<T: serde::Serialize + serde::de::DeserializeOwned>(
    output: &mut OutputRoot,
    name: &str,
    value: &T,
) -> Result<Vec<u8>, ProjectError> {
    let bytes = encoded(value)?;
    output.add(name, &bytes)?;
    let actual = output.read(name)?;
    let saved: T = parse(&actual)?;
    if actual != bytes || encoded(&saved)? != bytes {
        return Err(error(
            "PRESENT_OUTPUT_CHANGED",
            3,
            "saved result differs from validated result",
        ));
    }
    Ok(bytes)
}

pub fn run(
    project: &Path,
    case: &Path,
    coordinated: &Path,
    output: &Path,
) -> Result<PresentationOutput, ProjectError> {
    let cancel = Arc::new(AtomicBool::new(false));
    let control = Control::new(30000, cancel.clone());
    let path = project::check_components(output)?;
    let inputs = PresentationInputs::open(project, case, coordinated, &control, &cancel)?;
    for root in [project, inputs.case.root(), inputs.root()] {
        if extraction_output::inside(&path, root)? || path == root {
            return Err(error(
                "EXTRACT_OUTPUT_OVERLAP",
                2,
                "output overlaps project, selection, or coordination",
            ));
        }
    }
    #[cfg(test)]
    stage("inputs_held");
    inputs.verify(&control)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut output = OutputRoot::create_presentation(&path, &request_id)?;
    let result = (|| {
        let copies = inputs.copies()?;
        let input_fingerprints = copies
            .iter()
            .map(|(name, bytes)| (name.clone(), fingerprint(bytes)))
            .collect();
        for (name, bytes) in &copies {
            check(&control)?;
            output.add(name, bytes)?;
        }
        let ready = inputs.ready();
        let mut files = vec![];
        let mut decision = None;
        let mut plan_fingerprint = None;
        let mut table_cells = 0;
        let mut missing_cells = 0;
        if ready {
            let plan = knowledge_presentation_plan::build(&inputs, &request_id, &control)?;
            let bytes = save_checked(&mut output, "plan.json", &plan)?;
            plan_fingerprint = Some(fingerprint(&bytes));
            let rendered = knowledge_presentation_render::render(&plan)?;
            let mut actual = BTreeMap::new();
            for note in rendered {
                check(&control)?;
                output.add(&note.file.relative_path, &note.bytes)?;
                actual.insert(
                    note.file.relative_path.clone(),
                    output.read(&note.file.relative_path)?,
                );
                files.push(note.file);
            }
            knowledge_presentation_render::validate_rendered(&plan, &files, &actual)?;
            // Counts come from the actual selected structure; a text choice has no matrix cells.
            for note in &plan.notes {
                for node in &note.nodes {
                    if let Node::Table { rows, .. } = node {
                        for row in rows {
                            table_cells += row.cells.len();
                            missing_cells += row
                                .cells
                                .iter()
                                .filter(|c| c.kind == CellKind::Missing)
                                .count();
                        }
                    }
                }
            }
            decision = Some(plan.decision);
        }
        let facts = inputs.rebuilt["facts"]
            .as_array()
            .ok_or_else(|| error("PRESENT_SCHEMA", 2, "missing facts"))?;
        let role_count = |role: &str| {
            facts
                .iter()
                .filter_map(|f| f[role].as_str())
                .collect::<BTreeSet<_>>()
                .len()
        };
        let report = PresentationReport {
            schema_version: 1,
            producer: "knowledge_presentation".into(),
            report_kind: "knowledge_presentation".into(),
            scope: "active_presentation_v1".into(),
            request_id: request_id.clone(),
            project_id: inputs.case.config.project_id.clone(),
            coordination_schema_version: inputs.schema,
            status: if ready { "passed" } else { "needs_review" }.into(),
            preview_status: if ready { "rendered" } else { "not_run" }.into(),
            knowledge_status: "pending".into(),
            published: false,
            scientific_quality: "not_run".into(),
            research_status: "not_requested".into(),
            network_calls: 0,
            paid_calls: 0,
            checks: Checks {
                source_review: "passed".into(),
                coordination: if ready { "passed" } else { "needs_review" }.into(),
                note_plan: if ready { "passed" } else { "not_run" }.into(),
                rendering: if ready { "passed" } else { "not_run" }.into(),
                saved_output: "passed".into(),
            },
            totals: Totals {
                input_bundles: inputs.reviews.len(),
                facts: facts.len(),
                source_occurrences: inputs.rebuilt["occurrences"]
                    .as_array()
                    .ok_or_else(|| error("PRESENT_SCHEMA", 2, "missing occurrences"))?
                    .len(),
                objects: role_count("object_id"),
                properties: role_count("property_id"),
                table_cells,
                missing_cells,
                rendered_notes: files.len(),
                content_mappings: files.iter().map(|f| f.content_mappings.len()).sum(),
                links: files.iter().map(|f| f.links.len()).sum(),
            },
            issue_codes: if ready {
                vec![]
            } else {
                vec!["PRESENT_COORDINATION_UNRESOLVED".into()]
            },
            decision,
            coordination_ir_fingerprint: inputs.ir_fingerprint.clone(),
            plan_fingerprint,
            inputs: input_fingerprints,
            files,
            artifacts: output.fingerprints(),
        };
        let bytes = save_checked(&mut output, "presentation.json", &report)?;
        let json = String::from_utf8(bytes).map_err(|e| error("PRESENT_SCHEMA", 2, e))?;
        let markdown = format!(
            "# 有据主动表达预览\n\n状态：{}；预览：{}；知识状态：pending；发布：false。\n\n科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n```json\n{json}\n```\n",
            report.status, report.preview_status
        );
        output.add("presentation.md", markdown.as_bytes())?;
        if output.read("presentation.md")? != markdown.as_bytes() {
            return Err(error("PRESENT_OUTPUT_CHANGED", 3, "saved Markdown differs"));
        }
        #[cfg(test)]
        stage("results_saved");
        check(&control)?;
        inputs.verify(&control)?;
        output.verify()?;
        check(&control)?;
        Ok(PresentationOutput {
            json: format!("{json}\n"),
            failed: !ready,
        })
    })();
    if result.is_err() {
        output.discard_presentation_result()?;
    }
    result
}
