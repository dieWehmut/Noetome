//! Ordinary bounded coordination CLI: independent review reconstruction, held inputs and new output.
use crate::{
    extraction_output::{self, OutputRoot},
    knowledge_coordinate_facts,
    knowledge_coordinate_input::{
        CaseInputs, CaseVersion, VerifiedReview, check, encoded, err, parse,
    },
    knowledge_coordinate_variants,
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
    let facts =
        match &inputs.version {
            CaseVersion::V1 => VersionFacts::V1(knowledge_coordinate_facts::coordinate(
                &inputs.map,
                &reviews,
            )?),
            CaseVersion::V2(contexts) => VersionFacts::V2(
                knowledge_coordinate_variants::coordinate(&inputs.map, contexts, &reviews)?,
            ),
        };
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
        let (json, failed) =
            save_version(facts, &inputs.config.project_id, &request_id, &mut output)?;
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
            failed,
        })
    })();
    if result.is_err() {
        output.discard_coordination_result()?;
    }
    result
}

enum VersionFacts {
    V1(CoordinatedFacts),
    V2(crate::knowledge_coordination_v2_model::CoordinatedFacts),
}
fn save_checked<T: serde::Serialize + serde::de::DeserializeOwned>(
    output: &mut OutputRoot,
    name: &str,
    value: &T,
) -> Result<Vec<u8>, ProjectError> {
    let bytes = encoded(value)?;
    output.add(name, &bytes)?;
    let saved: T = parse(&output.read(name)?)?;
    if encoded(&saved)? != bytes {
        return Err(err("COORD_INPUT_CHANGED", 3, "saved result differs"));
    }
    Ok(bytes)
}
fn save_version(
    facts: VersionFacts,
    project_id: &str,
    request_id: &str,
    output: &mut OutputRoot,
) -> Result<(String, bool), ProjectError> {
    use crate::knowledge_coordination_v2_model::{
        CoordinationIr as Ir2, CoordinationReport as Report2,
    };
    macro_rules! save {($facts:expr,$ir:ident,$report:ident,$version:expr,$scope:expr,$summary:expr)=>{{
        let facts=$facts;
        let ir = $ir {
            schema_version: $version,
            producer: "knowledge_coordination".into(),
            revision: 1,
            request_id: request_id.to_owned(),
            scope: $scope.into(),
            project_id: project_id.to_owned(),
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
        if facts.status == "passed" { save_checked(output,"coordination-ir.json",&ir)?; }
        let report = $report {
            schema_version: $version,
            report_kind: "knowledge_coordination".into(),
            producer: "knowledge_coordination".into(),
            request_id: request_id.to_owned(),
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

        let bytes=save_checked(output,"coordination.json",&report)?;
        let json=String::from_utf8(bytes).map_err(|e|err("COORD_SCHEMA",2,e))?;
        let markdown=format!("# 有界多来源知识协调\n\n状态：{}；知识状态：pending；发布：false。\n\n科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n{}\n\n```json\n{json}\n```\n",report.status,$summary);
        output.add("coordination.md",markdown.as_bytes())?;
        if output.read("coordination.md")?!=markdown.as_bytes(){return Err(err("COORD_INPUT_CHANGED",3,"saved dual report differs"));}
        Ok((json,report.status!="passed"))
    }}}
    match facts {
        VersionFacts::V1(f) => save!(
            f,
            CoordinationIr,
            CoordinationReport,
            1,
            "explicit_identity_duplicate_complement_v1",
            "显式身份重复／互补范围；完整来源、原对象、证据与分母见以下事实，与 coordination.json 一致。"
        ),
        VersionFacts::V2(f) => {
            let summary = format!(
                "条件已列明：{}；条件未知：{}；条件不适用：{}；时间已知：{}；时间未知：{}；条件并列关系：{}；历史关系：{}。完整来源、原对象、证据与分母与 coordination.json 一致。",
                f.totals.known_condition_occurrences,
                f.totals.unknown_condition_occurrences,
                f.totals.not_applicable_condition_occurrences,
                f.totals.dated_occurrences,
                f.totals.undated_occurrences,
                f.totals.condition_variants,
                f.totals.historical_relations
            );
            save!(
                f,
                Ir2,
                Report2,
                2,
                "explicit_identity_condition_time_v2",
                summary
            )
        }
    }
}
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/support/coordination_case.rs"]
mod test_case;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/support/coordination_variants_case.rs"]
mod variants_test_case;
#[cfg(test)]
mod variants_guard_tests {
    use super::*;
    use std::fs;
    #[test]
    fn v2_final_input_inventory_change_preserves_originals_and_human_additions() {
        for case_name in ["conditional", "unknown_condition"] {
            let f = variants_test_case::VariantsCase::prepare(case_name);
            let case = f.case.clone();
            let output = f.output.clone();
            TEST_HOOK.with(|h| {
                *h.borrow_mut() = Some(Box::new(move |stage| {
                    if stage == "results_saved" {
                        let case = case.clone();
                        let output = output.clone();
                        std::thread::spawn(move || {
                            fs::write(case.join("human.txt"), b"keep input").unwrap();
                            fs::write(output.join("human.txt"), b"keep output").unwrap();
                        })
                        .join()
                        .unwrap();
                    }
                }))
            });
            let result = run(&f.project, &f.case, &f.output);
            TEST_HOOK.with(|h| *h.borrow_mut() = None);
            assert_eq!(result.err().unwrap().code(), "COORD_INPUT_CHANGED");
            for name in [
                "coordination.json",
                "coordination.md",
                "coordination-ir.json",
            ] {
                assert!(!f.output.join(name).exists());
            }
            assert!(f.output.join("identity-original.json").exists());
            assert_eq!(
                fs::read(f.output.join("human.txt")).unwrap(),
                b"keep output"
            );
            f.assert_protected();
        }
    }
    #[test]
    fn v2_final_source_or_output_inventory_change_discards_captured_results() {
        for source in [true, false] {
            let f = variants_test_case::VariantsCase::prepare("history");
            let path = if source {
                f.project.join("human.txt")
            } else {
                f.output.join("human.txt")
            };
            TEST_HOOK.with(|h| {
                *h.borrow_mut() = Some(Box::new(move |stage| {
                    if stage == "results_saved" {
                        let path = path.clone();
                        std::thread::spawn(move || fs::write(path, b"preserve").unwrap())
                            .join()
                            .unwrap();
                    }
                }))
            });
            let result = run(&f.project, &f.case, &f.output);
            TEST_HOOK.with(|h| *h.borrow_mut() = None);
            assert_eq!(
                result.err().unwrap().code(),
                if source {
                    "EXTRACT_SOURCE_CHANGED"
                } else {
                    "EXTRACT_OUTPUT_CHANGED"
                }
            );
            for name in [
                "coordination.json",
                "coordination.md",
                "coordination-ir.json",
            ] {
                assert!(!f.output.join(name).exists());
            }
            f.assert_protected();
        }
    }
    #[cfg(windows)]
    #[test]
    fn v2_held_sources_inputs_and_results_deny_writes() {
        let f = variants_test_case::VariantsCase::prepare("history");
        let case = f.case.clone();
        let project = f.project.clone();
        let output = f.output.clone();
        TEST_HOOK.with(|h| {
            *h.borrow_mut() = Some(Box::new(move |stage| {
                let paths = if stage == "inputs_held" {
                    vec![
                        case.join("identity.json"),
                        case.join("bundle-b/knowledge-ir.json"),
                        project.join("project.sqlite"),
                    ]
                } else if stage == "results_saved" {
                    vec![
                        output.join("coordination-ir.json"),
                        output.join("coordination.json"),
                    ]
                } else {
                    vec![]
                };
                for path in paths {
                    std::thread::spawn(move || {
                        assert!(fs::write(&path, b"bad").is_err());
                        assert!(fs::remove_file(path).is_err());
                    })
                    .join()
                    .unwrap();
                }
            }))
        });
        let result = run(&f.project, &f.case, &f.output);
        TEST_HOOK.with(|h| *h.borrow_mut() = None);
        assert!(!result.unwrap().failed);
        f.assert_protected();
    }
}
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
