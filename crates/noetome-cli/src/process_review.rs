//! Deterministic process source review with auditable, reconstructed output bytes.
use crate::{
    extraction_output::{self, OutputRoot},
    knowledge_coordinate_input::fingerprint,
    process_input::{ProcessInputs, check, encoded, error, parse},
    process_model::*,
    project::{self, ProjectError},
    sidecar_process::Control,
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

pub struct ProcessOutput {
    pub json: String,
    pub failed: bool,
}
pub(crate) struct ReviewBuild {
    pub(crate) files: BTreeMap<String, Vec<u8>>,
    pub(crate) report: ProcessReviewReport,
}

pub(crate) fn build(inputs: &ProcessInputs, request_id: &str) -> Result<ReviewBuild, ProjectError> {
    let evaluation = inputs.evaluate();
    let mut files = inputs.originals()?;
    let originals = files
        .iter()
        .map(|(name, bytes)| (name.clone(), fingerprint(bytes)))
        .collect();
    files.insert(".owner.json".into(), encoded(&serde_json::json!({"schema_version":1,"producer":"process_source_review","request_id":request_id,"purpose":"unpublished_process_source_review"}))?);
    let mut ir_fingerprint = None;
    if evaluation.ready() {
        let document = evaluation
            .document
            .as_ref()
            .ok_or_else(|| error("PROCESS_SCHEMA", 3, "missing parsed document"))?;
        let ir = ProcessIr {
            schema_version: 1,
            producer: "process_source_review".into(),
            scope: "bounded_process_text_v1".into(),
            request_id: request_id.into(),
            binding: inputs.source.binding.clone(),
            unit: inputs.source.units[0].clone(),
            source_match: "passed".into(),
            gold_match: "passed".into(),
            knowledge_status: "pending".into(),
            scientific_quality: "not_run".into(),
            published: false,
            document: document.clone(),
            evidence_fingerprints: document
                .evidence
                .iter()
                .map(|e| (e.object_id.clone(), fingerprint(e.quote.as_bytes())))
                .collect(),
        };
        let bytes = encoded(&ir)?;
        let _: ProcessIr = parse(&bytes)?;
        ir_fingerprint = Some(fingerprint(&bytes));
        files.insert("process-ir.json".into(), bytes);
    }
    let ready = evaluation.ready();
    let mut actual_totals = evaluation.observed_totals;
    actual_totals.insert(
        "source_statements".into(),
        inputs.source.units[0]
            .text
            .strip_suffix('\n')
            .unwrap_or(&inputs.source.units[0].text)
            .split('\n')
            .count(),
    );
    let report = ProcessReviewReport {
        schema_version: 1,
        producer: "process_source_review".into(),
        scope: "bounded_process_text_v1".into(),
        request_id: request_id.into(),
        binding: inputs.source.binding.clone(),
        selection: inputs.config.selection.clone(),
        status: if ready { "passed" } else { "needs_review" }.into(),
        knowledge_status: "pending".into(),
        scientific_quality: "not_run".into(),
        research_status: "not_requested".into(),
        published: false,
        network_calls: 0,
        paid_calls: 0,
        checks: ReviewChecks {
            source_parsing: if evaluation.source_ok {
                "passed"
            } else {
                "needs_review"
            }
            .into(),
            gold_match: if !evaluation.source_ok {
                "not_run"
            } else if evaluation.gold_ok {
                "passed"
            } else {
                "needs_review"
            }
            .into(),
            saved_output: "passed".into(),
        },
        totals: actual_totals,
        expected_totals: document_totals(Some(&inputs.gold), evaluation.unparsed_statements),
        issue_codes: evaluation.issue_codes,
        unparsed_statements: evaluation.unparsed_statements,
        parsed_document: evaluation.document,
        gold_sha256: inputs.config.gold_sha256.clone(),
        ir_fingerprint,
        inputs: originals,
        artifacts: files
            .iter()
            .map(|(n, b)| (n.clone(), fingerprint(b)))
            .collect(),
    };
    let bytes = encoded(&report)?;
    let _: ProcessReviewReport = parse(&bytes)?;
    let json = String::from_utf8(bytes.clone()).map_err(|e| error("PROCESS_SCHEMA", 2, e))?;
    files.insert("review.json".into(), bytes);
    files.insert("review.md".into(), format!("# 有界过程来源核对\n\n状态：{}；知识状态：pending；发布：false。\n\n科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n```json\n{json}\n```\n", report.status).into_bytes());
    Ok(ReviewBuild { files, report })
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

pub fn run(project: &Path, case: &Path, output: &Path) -> Result<ProcessOutput, ProjectError> {
    let control = Control::new(30000, Arc::new(AtomicBool::new(false)));
    let path = project::check_components(output)?;
    let inputs = ProcessInputs::open(project, case, &control)?;
    for root in [inputs.source.root(), inputs.case.root()] {
        if extraction_output::inside(&path, root)? || path == root {
            return Err(error(
                "EXTRACT_OUTPUT_OVERLAP",
                2,
                "output overlaps project or process review case",
            ));
        }
    }
    #[cfg(test)]
    stage("inputs_held");
    inputs.verify(&control)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut output = OutputRoot::create_process_review(&path, &request_id)?;
    let result = (|| {
        let rebuilt = build(&inputs, &request_id)?;
        for (name, bytes) in &rebuilt.files {
            check(&control)?;
            if name != ".owner.json" {
                output.add(name, bytes)?;
            }
            if output.read(name)? != *bytes {
                return Err(error(
                    "PROCESS_OUTPUT_CHANGED",
                    3,
                    "saved output differs from reconstructed bytes",
                ));
            }
        }
        #[cfg(test)]
        stage("results_saved");
        inputs.verify(&control)?;
        output.verify()?;
        check(&control)?;
        let json = String::from_utf8(rebuilt.files["review.json"].clone())
            .map_err(|e| error("PROCESS_SCHEMA", 3, e))?;
        Ok(ProcessOutput {
            json: format!("{json}\n"),
            failed: rebuilt.report.status != "passed",
        })
    })();
    if result.is_err() {
        output.discard_process_result()?;
    }
    result
}
