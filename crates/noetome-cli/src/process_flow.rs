//! Source-rebuilt, unpublished process expression in a fresh test Vault.
use crate::{
    extraction_model::SourceBinding,
    extraction_output::{self, OutputRoot},
    knowledge_coordinate_input::{fingerprint, plain},
    process_flow_model::{
        FlowDecision, FlowIssue, FlowPlan, FlowProfile, FlowRenderedFile, Representation,
    },
    process_flow_plan, process_flow_render,
    process_input::{ProcessInputs, SavedReview, SingleInput, check, encoded, error, parse},
    process_model::ProcessDocument,
    process_review::ProcessOutput,
    project::{self, ProjectError},
    sidecar_model::Fingerprint,
    sidecar_process::Control,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

#[cfg(test)]
#[path = "../tests/support/process_guard_tests.rs"]
mod guard_tests;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/support/process_case.rs"]
mod test_case;

fn nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
macro_rules! fixed {
    ($name:ident,$ty:ty,$value:expr) => {
        fn $name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<$ty, D::Error> {
            let v = <$ty>::deserialize(d)?;
            if v == $value {
                Ok(v)
            } else {
                Err(serde::de::Error::custom("unsupported flow fixed field"))
            }
        }
    };
}
fixed!(one, u32, 1);
fixed!(producer, String, "knowledge_flow");
fixed!(scope, String, "active_flow_v1");
fixed!(pending, String, "pending");
fixed!(science, String, "not_run");
fixed!(research, String, "not_requested");
fixed!(false_value, bool, false);
fixed!(zero, usize, 0);
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct FlowChecks {
    pub(crate) source_review: String,
    pub(crate) model_support: String,
    pub(crate) note_plan: String,
    pub(crate) rendering: String,
    pub(crate) saved_output: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct FlowReport {
    #[serde(deserialize_with = "one")]
    pub(crate) schema_version: u32,
    #[serde(deserialize_with = "producer")]
    pub(crate) producer: String,
    #[serde(deserialize_with = "scope")]
    pub(crate) scope: String,
    pub(crate) request_id: String,
    pub(crate) binding: SourceBinding,
    pub(crate) status: String,
    pub(crate) preview_status: String,
    #[serde(deserialize_with = "pending")]
    pub(crate) knowledge_status: String,
    #[serde(deserialize_with = "science")]
    pub(crate) scientific_quality: String,
    #[serde(deserialize_with = "research")]
    pub(crate) research_status: String,
    #[serde(deserialize_with = "false_value")]
    pub(crate) published: bool,
    #[serde(deserialize_with = "zero")]
    pub(crate) network_calls: usize,
    #[serde(deserialize_with = "zero")]
    pub(crate) paid_calls: usize,
    pub(crate) checks: FlowChecks,
    pub(crate) issue_codes: Vec<String>,
    pub(crate) issues: Vec<FlowIssue>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) process_document: Option<ProcessDocument>,
    pub(crate) profile: FlowProfile,
    #[serde(deserialize_with = "nullable")]
    pub(crate) representation: Option<Representation>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) decision: Option<FlowDecision>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) review_ir_fingerprint: Option<Fingerprint>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) plan_fingerprint: Option<Fingerprint>,
    pub(crate) totals: BTreeMap<String, usize>,
    pub(crate) inputs: BTreeMap<String, Fingerprint>,
    pub(crate) files: Vec<FlowRenderedFile>,
    pub(crate) artifacts: BTreeMap<String, Fingerprint>,
}

#[cfg(test)]
thread_local! { pub(crate) static TEST_HOOK:std::cell::RefCell<Option<Box<dyn FnMut(&str)>>> = std::cell::RefCell::new(None); }
#[cfg(test)]
pub(crate) fn stage(s: &str) {
    TEST_HOOK.with(|h| {
        if let Some(h) = h.borrow_mut().as_mut() {
            h(s)
        }
    });
}

fn save_checked<T: Serialize + serde::de::DeserializeOwned>(
    out: &mut OutputRoot,
    name: &str,
    value: &T,
) -> Result<Vec<u8>, ProjectError> {
    let bytes = encoded(value)?;
    out.add(name, &bytes)?;
    let actual = out.read(name)?;
    let decoded: T = parse(&actual)?;
    if actual != bytes || encoded(&decoded)? != bytes {
        return Err(error(
            "FLOW_OUTPUT_CHANGED",
            3,
            "saved value differs from validated bytes",
        ));
    }
    Ok(bytes)
}

pub fn run(
    project: &Path,
    case: &Path,
    saved: &Path,
    profile: &Path,
    output: &Path,
) -> Result<ProcessOutput, ProjectError> {
    let control = Control::new(30000, Arc::new(AtomicBool::new(false)));
    let path = project::check_components(output)?;
    let inputs = ProcessInputs::open(project, case, &control)?;
    let reviewed = SavedReview::open(saved, &inputs, &control)?;
    let profile_input = SingleInput::open(profile, &control)?;
    let profile: FlowProfile = parse(&profile_input.bytes)?;
    if [
        &profile.profile_id,
        &profile.app,
        &profile.app_version,
        &profile.mermaid_version,
    ]
    .iter()
    .any(|v| !plain(v))
    {
        return Err(error(
            "FLOW_PROFILE_SCHEMA",
            2,
            "profile labels must be bounded plain text",
        ));
    }
    for root in [
        inputs.source.root(),
        inputs.case.root(),
        reviewed.held.root(),
    ] {
        if extraction_output::inside(&path, root)? || path == root {
            return Err(error(
                "EXTRACT_OUTPUT_OVERLAP",
                2,
                "output overlaps project, process case, or saved review",
            ));
        }
    }
    // Both paths have already passed component checks. The destination does
    // not exist yet, so it cannot be canonicalized as an existing input root.
    if profile_input.path.starts_with(&path) {
        return Err(error(
            "EXTRACT_OUTPUT_OVERLAP",
            2,
            "profile overlaps output",
        ));
    }
    #[cfg(test)]
    stage("inputs_held");
    inputs.verify(&control)?;
    reviewed.verify(&control)?;
    profile_input.verify(&control)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut output = OutputRoot::create_flow(&path, &request_id)?;
    let result = (|| {
        let mut copies = inputs.originals()?;
        copies.insert(".profile.json".into(), profile_input.bytes.clone());
        for (name, bytes) in &reviewed.files {
            copies.insert(format!(".review-{name}"), bytes.clone());
        }
        let input_fingerprints = copies
            .iter()
            .map(|(n, b)| (n.clone(), fingerprint(b)))
            .collect();
        for (name, bytes) in copies {
            check(&control)?;
            output.add(&name, &bytes)?;
        }
        let source_ready = reviewed.report.status == "passed";
        let mut issues = vec![];
        let plan = if source_ready {
            let document = reviewed.report.parsed_document.as_ref().ok_or_else(|| {
                error(
                    "FLOW_SOURCE_UNVERIFIED",
                    3,
                    "missing source-verified document",
                )
            })?;
            match process_flow_plan::build(document, &profile, &request_id) {
                Ok(plan) => Some(plan),
                Err(found) => {
                    issues = found;
                    None
                }
            }
        } else {
            issues.push(FlowIssue {
                code: "FLOW_SOURCE_UNVERIFIED".into(),
                object_ids: vec![],
                message: "过程来源核对尚未通过，未运行流程规划。".into(),
            });
            None
        };
        let ready = plan.is_some();
        let mut files = vec![];
        let mut plan_fingerprint = None;
        let mut representation = None;
        let mut decision = None;
        if let Some(plan) = plan {
            let bytes = save_checked(&mut output, "plan.json", &plan)?;
            let saved_plan: FlowPlan = parse(&bytes)?;
            plan_fingerprint = Some(fingerprint(&bytes));
            let rendered = process_flow_render::render(&saved_plan)?;
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
            process_flow_render::validate_rendered(&saved_plan, &files, &actual)?;
            representation = Some(saved_plan.representation);
            decision = Some(saved_plan.decision);
        }
        let mut issue_codes: Vec<String> = issues.iter().map(|i| i.code.clone()).collect();
        if let Some(decision) = &decision {
            issue_codes.extend(decision.fallback_reasons.iter().map(|r| r.code.clone()));
        }
        let document = reviewed.report.parsed_document.clone();
        let mut totals = reviewed.report.totals.clone();
        totals.insert(
            "source_statements".into(),
            reviewed.report.totals["source_statements"],
        );
        totals.insert(
            "flow_nodes".into(),
            if ready {
                document.as_ref().map_or(0, |d| d.steps.len())
            } else {
                0
            },
        );
        totals.insert(
            "flow_edges".into(),
            if ready {
                document.as_ref().map_or(0, |d| d.relations.len())
            } else {
                0
            },
        );
        totals.insert("rendered_notes".into(), files.len());
        totals.insert(
            "content_mappings".into(),
            files.iter().map(|f| f.content_mappings.len()).sum(),
        );
        totals.insert("links".into(), files.iter().map(|f| f.links.len()).sum());
        let report = FlowReport {
            schema_version: 1,
            producer: "knowledge_flow".into(),
            scope: "active_flow_v1".into(),
            request_id: request_id.clone(),
            binding: inputs.source.binding.clone(),
            status: if ready { "passed" } else { "needs_review" }.into(),
            preview_status: if ready { "rendered" } else { "not_run" }.into(),
            knowledge_status: "pending".into(),
            scientific_quality: "not_run".into(),
            research_status: "not_requested".into(),
            published: false,
            network_calls: 0,
            paid_calls: 0,
            checks: FlowChecks {
                source_review: reviewed.report.status.clone(),
                model_support: if !source_ready {
                    "not_run"
                } else if ready {
                    "passed"
                } else {
                    "needs_review"
                }
                .into(),
                note_plan: if ready { "passed" } else { "not_run" }.into(),
                rendering: if ready { "passed" } else { "not_run" }.into(),
                saved_output: "passed".into(),
            },
            issue_codes,
            issues,
            process_document: document,
            profile,
            representation,
            decision,
            review_ir_fingerprint: reviewed.report.ir_fingerprint.clone(),
            plan_fingerprint,
            totals,
            inputs: input_fingerprints,
            files,
            artifacts: output.fingerprints(),
        };
        let bytes = save_checked(&mut output, "flow.json", &report)?;
        let json = String::from_utf8(bytes).map_err(|e| error("FLOW_SCHEMA", 3, e))?;
        let markdown = format!(
            "# 有据流程表达预览\n\n状态：{}；预览：{}；知识状态：pending；发布：false。\n\n科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n```json\n{json}\n```\n",
            report.status, report.preview_status
        );
        output.add("flow.md", markdown.as_bytes())?;
        if output.read("flow.md")? != markdown.as_bytes() {
            return Err(error("FLOW_OUTPUT_CHANGED", 3, "saved Markdown differs"));
        }
        #[cfg(test)]
        stage("results_saved");
        check(&control)?;
        inputs.verify(&control)?;
        reviewed.verify(&control)?;
        profile_input.verify(&control)?;
        output.verify()?;
        check(&control)?;
        Ok(ProcessOutput {
            json: format!("{json}\n"),
            failed: !ready,
        })
    })();
    if result.is_err() {
        output.discard_process_result()?;
    }
    result
}
