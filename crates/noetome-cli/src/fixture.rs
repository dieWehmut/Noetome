//! Read-only fixture verification, never a claim of rendered or scientific quality.

use std::path::Path;

use serde_json::json;

use crate::fixture_input::load;
use crate::fixture_knowledge::{validate_coverage, validate_knowledge};
use crate::fixture_model::Issue;
use crate::fixture_plan::{validate_expected, validate_plan};
use crate::project::ProjectError;

pub struct CommandOutput {
    pub json: String,
    pub failed: bool,
}

fn check(issues: &[Issue], ran: bool) -> serde_json::Value {
    json!({"status": if !ran { "not_run" } else if issues.is_empty() { "passed" } else { "failed" },
        "issue_count": issues.len()})
}

pub fn verify(path: &Path) -> Result<CommandOutput, ProjectError> {
    let case = load(path)?;
    let sources = case.source_issues.clone();
    let ran = sources.is_empty();
    let coverage = if ran {
        validate_coverage(&case)
    } else {
        Vec::new()
    };
    let knowledge = if ran {
        validate_knowledge(&case)
    } else {
        Vec::new()
    };
    let (note_plan, mut targets) = if ran {
        validate_plan(&case)
    } else {
        (Vec::new(), Vec::new())
    };
    let expected = if ran {
        validate_expected(&case)
    } else {
        Vec::new()
    };
    if !note_plan.is_empty() {
        targets.clear();
    }
    let issues: Vec<_> = [&sources, &coverage, &knowledge, &note_plan, &expected]
        .into_iter()
        .flat_map(|items| items.iter().cloned())
        .collect();
    let passed = ran && issues.is_empty();
    let nodes = case
        .plan
        .notes
        .iter()
        .flat_map(|note| &note.sections)
        .map(|section| section.ordered_nodes.len())
        .sum::<usize>();
    case.check_unchanged()?;
    let report = json!({
        "schema_version": 1,
        "producer": "fixture",
        "case_id": case.manifest.case_id,
        "validator_version": "fixture-contracts-v1",
        "passed": passed,
        "checks": {
            "sources": check(&sources, true), "coverage": check(&coverage, ran),
            "knowledge": check(&knowledge, ran), "note_plan": check(&note_plan, ran),
            "expected": check(&expected, ran), "rendering": check(&[], false),
            "scientific_quality": check(&[], false)
        },
        "counts": {"sources": case.manifest.sources.len(), "spans": case.spans.len(),
            "atoms": case.ir.atoms.len(), "required_atoms": case.ir.atoms.iter().filter(|atom| atom.required).count(),
            "notes": case.plan.notes.len(), "nodes": nodes},
        "issues": issues, "resolved_targets": targets,
        "ir_sha256": case.ir_sha256, "plan_sha256": case.plan_sha256,
        "expected_sha256": case.expected_sha256, "case_sha256": case.case_sha256
    });
    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| ProjectError::new("CASE_REPORT_ERROR", 1, error.to_string()))?;
    Ok(CommandOutput {
        json: format!("{json}\n"),
        failed: !passed,
    })
}
