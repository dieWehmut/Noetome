//! Ordinary bounded coordination CLI: independent review reconstruction, held inputs and new output.
use crate::{
    extraction_output::{self, OutputRoot},
    knowledge_coordinate_facts,
    knowledge_coordinate_input::{CaseInputs, VerifiedReview, check, encoded, err, parse},
    knowledge_coordination_model::*,
    project::{self, ProjectError},
    sidecar_process::Control,
};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};
pub struct CoordinationOutput {
    pub json: String,
    pub failed: bool,
}
#[cfg(test)]
thread_local! {static TEST_HOOK:std::cell::RefCell<Option<Box<dyn FnMut(&str)>>> = std::cell::RefCell::new(None);}
#[cfg(test)]
fn stage(s: &str) {
    TEST_HOOK.with(|h| {
        if let Some(h) = h.borrow_mut().as_mut() {
            h(s)
        }
    })
}
pub fn run(project: &Path, case: &Path, output: &Path) -> Result<CoordinationOutput, ProjectError> {
    let cancel = Arc::new(AtomicBool::new(false));
    let control = Control::new(30000, cancel.clone());
    let inputs = CaseInputs::open(case, &control)?;
    let path = project::check_components(output)?;
    // Membership uses captured real ancestor identities; a nonexistent target cannot contain existing inputs.
    for root in [project, inputs.root()] {
        if extraction_output::inside(&path, root)? || path == root {
            return Err(err(
                "EXTRACT_OUTPUT_OVERLAP",
                2,
                "output overlaps source or coordination input",
            ));
        }
    }
    let mut reviews = vec![];
    let mut units = BTreeSet::new();
    let mut bytes = inputs.original_bytes().0.len() + inputs.original_bytes().1.len();
    for i in &inputs.config.inputs {
        check(&control)?;
        let r = VerifiedReview::open(project, i, inputs.root(), &control, &cancel)?;
        bytes += r.files.values().map(Vec::len).sum::<usize>();
        if bytes > 96 * 1024 * 1024 {
            return Err(err(
                "COORD_INPUT_LIMIT",
                2,
                "aggregate inputs exceed 96 MiB",
            ));
        }
        let b = &r.source.binding;
        let u = &r.source.units[0];
        let key = serde_json::to_string(&serde_json::json!([
            b.project_id,
            b.source_id,
            b.version_id,
            b.parse_id,
            u.unit_id,
            u.start_byte,
            u.end_byte
        ]))
        .map_err(|e| err("COORD_SCHEMA", 2, e))?;
        if !units.insert(key) {
            return Err(err(
                "COORD_DUPLICATE_INPUT",
                2,
                format!("{}: repeated actual source unit", i.input_id),
            ));
        }
        reviews.push(r);
        check(&control)?;
    }
    let facts = knowledge_coordinate_facts::coordinate(&inputs.map, &reviews)?;
    #[cfg(test)]
    stage("inputs_held");
    inputs.verify(&control)?;
    for r in &reviews {
        r.verify(&control)?;
    }
    check(&control)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut output = OutputRoot::create_coordination(&path, &request_id)?;
    let result = (|| {
        output.add("coordination-original.json", inputs.original_bytes().0)?;
        output.add("identity-original.json", inputs.original_bytes().1)?;
        for r in &reviews {
            for (n, b) in &r.files {
                check(&control)?;
                output.add(&format!("input-{}--{n}", r.input.input_id), b)?;
            }
        }
        let ir = CoordinationIr {
            schema_version: 1,
            producer: "knowledge_coordination".into(),
            revision: 1,
            request_id: request_id.clone(),
            scope: "explicit_identity_duplicate_complement_v1".into(),
            project_id: inputs.config.project_id.clone(),
            knowledge_status: "pending".into(),
            published: false,
            scientific_quality: "not_run".into(),
            research_status: "not_requested".into(),
            canonical_concepts: facts.canonical_concepts.clone(),
            inputs: facts.inputs.clone(),
            occurrences: facts.occurrences.clone(),
            facts: facts.facts.clone(),
            relations: facts.relations.clone(),
            totals: facts.totals.clone(),
        };
        if facts.status == "passed" {
            let bytes = encoded(&ir)?;
            output.add("coordination-ir.json", &bytes)?;
            let saved: CoordinationIr = parse(&output.read("coordination-ir.json")?)?;
            if encoded(&saved)? != bytes {
                return Err(err("COORD_INPUT_CHANGED", 3, "saved IR differs"));
            }
        }
        let report = CoordinationReport {
            schema_version: 1,
            report_kind: "knowledge_coordination".into(),
            producer: "knowledge_coordination".into(),
            request_id,
            scope: ir.scope,
            project_id: ir.project_id,
            status: facts.status.clone(),
            knowledge_status: ir.knowledge_status,
            published: false,
            scientific_quality: ir.scientific_quality,
            research_status: ir.research_status,
            network_calls: 0,
            paid_calls: 0,
            canonical_concepts: facts.canonical_concepts,
            inputs: facts.inputs,
            occurrences: facts.occurrences,
            facts: facts.facts,
            relations: facts.relations,
            totals: facts.totals,
            artifacts: output.fingerprints(),
        };
        let bytes = encoded(&report)?;
        let json = String::from_utf8(bytes.clone()).map_err(|e| err("COORD_SCHEMA", 2, e))?;
        let markdown = format!(
            "# 有界多来源知识协调\n\n状态：{}；知识状态：pending；发布：false。\n\n科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n显式身份重复／互补范围；完整来源、原对象、证据与分母见以下事实，与 coordination.json 一致。\n\n```json\n{json}\n```\n",
            report.status
        );
        output.add("coordination.json", &bytes)?;
        output.add("coordination.md", markdown.as_bytes())?;
        let saved: CoordinationReport = parse(&output.read("coordination.json")?)?;
        if encoded(&saved)? != bytes || output.read("coordination.md")? != markdown.as_bytes() {
            return Err(err("COORD_INPUT_CHANGED", 3, "saved dual report differs"));
        }
        #[cfg(test)]
        stage("results_saved");
        check(&control)?;
        inputs.verify(&control)?;
        for r in &reviews {
            r.verify(&control)?;
        }
        output.verify()?;
        check(&control)?;
        Ok(CoordinationOutput {
            json: format!("{json}\n"),
            failed: report.status != "passed",
        })
    })();
    if result.is_err() {
        output.discard_coordination_result()?;
    }
    result
}
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/support/coordination_case.rs"]
mod test_case;
#[cfg(test)]
mod guard_tests {
    use super::*;
    use std::fs;
    fn execute(
        f: &test_case::PreparedCase,
        hook: impl FnMut(&str) + 'static,
    ) -> Result<CoordinationOutput, ProjectError> {
        TEST_HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
        let r = run(&f.project, &f.case, &f.output);
        TEST_HOOK.with(|h| *h.borrow_mut() = None);
        r
    }
    fn no_results(f: &test_case::PreparedCase) {
        for n in [
            "coordination-ir.json",
            "coordination.json",
            "coordination.md",
        ] {
            assert!(!f.output.join(n).exists());
        }
        assert!(f.output.join("identity-original.json").exists());
    }
    #[test]
    fn final_input_inventory_change_discards_only_captured_results() {
        let f = test_case::PreparedCase::baseline();
        let case = f.case.clone();
        let output = f.output.clone();
        let r = execute(&f, move |s| {
            if s == "results_saved" {
                let case = case.clone();
                let output = output.clone();
                std::thread::spawn(move || {
                    fs::write(case.join("human-addition.txt"), b"keep input").unwrap();
                    fs::write(output.join("human-addition.txt"), b"keep output").unwrap();
                })
                .join()
                .unwrap();
            }
        });
        assert_eq!(r.err().unwrap().code(), "COORD_INPUT_CHANGED");
        no_results(&f);
        f.assert_protected();
        assert_eq!(
            fs::read(f.output.join("human-addition.txt")).unwrap(),
            b"keep output"
        );
    }
    #[test]
    fn final_source_and_output_changes_preserve_unknown_content() {
        for source in [true, false] {
            let f = test_case::PreparedCase::baseline();
            let project = f.project.clone();
            let output = f.output.clone();
            let r = execute(&f, move |s| {
                if s == "results_saved" {
                    let path = if source {
                        project.join("human-new-source.txt")
                    } else {
                        output.join("human-new-output.txt")
                    };
                    std::thread::spawn(move || fs::write(path, b"human addition").unwrap())
                        .join()
                        .unwrap();
                }
            });
            assert_eq!(
                r.err().unwrap().code(),
                if source {
                    "EXTRACT_SOURCE_CHANGED"
                } else {
                    "EXTRACT_OUTPUT_CHANGED"
                }
            );
            no_results(&f);
            f.assert_protected();
        }
    }
    #[cfg(windows)]
    #[test]
    fn held_inputs_sources_and_results_deny_concurrent_write_delete() {
        let f = test_case::PreparedCase::baseline();
        let case = f.case.clone();
        let project = f.project.clone();
        let output = f.output.clone();
        let imported: serde_json::Value =
            serde_json::from_slice(&fs::read(f.root.join("import-a-stdout.txt")).unwrap()).unwrap();
        let resource = project.join(imported["version"]["normalized_snapshot"].as_str().unwrap());
        let r = execute(&f, move |s| {
            let paths = if s == "inputs_held" {
                vec![
                    case.join("identity.json"),
                    case.join("bundle-b/knowledge-ir.json"),
                    project.join("project.sqlite"),
                    resource.clone(),
                ]
            } else if s == "results_saved" {
                vec![
                    output.join("coordination-ir.json"),
                    output.join("coordination.json"),
                ]
            } else {
                vec![]
            };
            for p in paths {
                std::thread::spawn(move || {
                    assert!(fs::write(&p, b"bad").is_err());
                    assert!(fs::remove_file(&p).is_err());
                })
                .join()
                .unwrap();
            }
        })
        .unwrap();
        assert!(!r.failed);
        f.assert_protected();
    }
    #[test]
    fn final_result_hardlink_denied_or_invalidated_without_alias_cleanup() {
        let f = test_case::PreparedCase::baseline();
        let output = f.output.clone();
        let alias = f.root.join("human-ir-alias.json");
        let a = alias.clone();
        let r = execute(&f, move |s| {
            if s == "results_saved" {
                let path = output.join("coordination-ir.json");
                let a = a.clone();
                std::thread::spawn(move || fs::hard_link(path, a).is_ok())
                    .join()
                    .unwrap();
            }
        });
        if alias.exists() {
            assert!(r.is_err());
            no_results(&f);
            assert!(alias.is_file());
        } else {
            assert!(!r.unwrap().failed);
        }
        f.assert_protected();
    }
}
