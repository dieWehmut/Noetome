use serde_json::{Value, json};
use std::{fs, path::Path};
#[path = "support/coordination_conflicts_case.rs"]
mod support;
use support::*;
fn read(p: &Path) -> Value {
    serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
}
fn write(p: &Path, v: &Value) {
    fs::write(p, encode(v)).unwrap()
}
fn norm(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn sorted(v: &Value) -> Value {
    let mut a = v
        .as_array()
        .unwrap()
        .iter()
        .map(|s| norm(s.as_str().unwrap()))
        .collect::<Vec<_>>();
    a.sort();
    json!(a)
}
fn canonical(v: &mut [Value]) {
    v.sort_by_key(|v| serde_json::to_string(v).unwrap());
}
impl ConflictsCase {
    fn assert_author_expectations(&self, report: &Value) {
        let author = read(&self.root.join("frozen-author.json"));
        let expected = read(&self.root.join("expected.json"));
        assert_eq!(
            report["totals"],
            expected["totals"],
            "{}",
            self.root.display()
        );
        assert_eq!(report["status"], expected["expected_status"]);
        for k in [
            "schema_version",
            "scope",
            "scientific_quality",
            "knowledge_status",
            "published",
        ] {
            assert_eq!(report[k], expected[k], "{k}");
        }
        assert_eq!(report["network_calls"], 0);
        assert_eq!(report["paid_calls"], 0);
        assert_eq!(report["research_status"], "not_requested");
        let occurrences = report["occurrences"].as_array().unwrap();
        let by_id = |id: &Value| {
            occurrences
                .iter()
                .find(|o| o["occurrence_id"] == *id)
                .unwrap()["input_id"]
                .clone()
        };
        for e in expected["occurrences"].as_array().unwrap() {
            let o = occurrences
                .iter()
                .find(|o| o["input_id"] == e["input_id"])
                .unwrap();
            for k in ["value", "conditions", "exceptions", "status", "issue_codes"] {
                assert_eq!(o[k], e[k], "occ {k}");
            }
            assert_eq!(o["unit_value"], e["unit"]);
            assert_eq!(o["original_atom"]["atom"]["atom_id"], e["original_atom_id"]);
            assert_eq!(
                o["context"]["applicability_status"],
                e["applicability_status"]
            );
            assert_eq!(o["context"]["time"], e["time"]);
            assert_eq!(o["context"]["quote"], e["context_quote"]);
            let input = author["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .find(|i| i["input_id"] == e["input_id"])
                .unwrap();
            let id = e["input_id"].as_str().unwrap();
            let source = fs::read_to_string(self.root.join(format!("selected-{id}.txt"))).unwrap();
            let cq = o["context"]["quote"].as_str().unwrap();
            let start = source.find(cq).unwrap();
            assert_eq!(o["context"]["start_byte"], start);
            assert_eq!(o["context"]["end_byte"], start + cq.len());
            assert_eq!(o["context"]["sha256"], hash(cq.as_bytes()));
            let mut expected_ids = input["context"]["context_evidence_ids"]
                .as_array()
                .unwrap()
                .clone();
            expected_ids.sort_by_key(|v| v.as_str().unwrap().to_owned());
            assert_eq!(o["context"]["evidence_ids"], json!(expected_ids));
            let candidate = read(
                &self
                    .case
                    .join(format!("bundle-{id}/candidates-original.json")),
            );
            assert_eq!(o["original_concepts"], candidate["payload"]["concepts"]);
            assert_eq!(o["original_atom"], candidate["payload"]["atoms"][0]);
            let mut seen_evidence = std::collections::BTreeSet::new();
            let evidence = candidate["payload"]["atoms"][0]["atom"]["evidence_links"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|l| seen_evidence.insert(l["evidence_id"].as_str().unwrap()))
                .map(|l| {
                    candidate["payload"]["evidence"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|ev| ev["evidence_id"] == l["evidence_id"])
                        .unwrap()
                        .clone()
                })
                .collect::<Vec<_>>();
            assert_eq!(o["original_evidence"], json!(evidence));
            let b = &o["binding"];
            let u = &o["unit"];
            assert_eq!(
                o["occurrence_id"],
                format!(
                    "occ-{}",
                    hash(
                        &serde_json::to_vec(&json!([
                            b["project_id"],
                            b["source_id"],
                            b["version_id"],
                            b["parse_id"],
                            u["unit_id"],
                            u["start_byte"],
                            u["end_byte"],
                            e["original_atom_id"]
                        ]))
                        .unwrap()
                    )
                )
            );
        }
        let records = report["processing_records"].as_array().unwrap();
        assert_eq!(records.len(), occurrences.len());
        for o in occurrences {
            let record = records
                .iter()
                .find(|r| r["occurrence_id"] == o["occurrence_id"])
                .unwrap();
            for field in ["object_id", "property_id", "status", "issue_codes"] {
                assert_eq!(record[field], o[field], "record {field}");
            }
            let review = o["status"] == "needs_review";
            assert_eq!(
                record["action"],
                if review {
                    "retain_for_review"
                } else {
                    "retained"
                }
            );
            let reason = if review {
                o["issue_codes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join("; ")
            } else {
                "retained with complete source provenance".to_string()
            };
            assert_eq!(record["reason"], reason);
            assert_eq!(record.as_object().unwrap().len(), 7);
        }
        let mut actual_facts = vec![];
        for f in report["facts"].as_array().unwrap() {
            let mut ids = f["occurrence_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(by_id)
                .collect::<Vec<_>>();
            ids.sort_by_key(|v| v.as_str().unwrap().to_owned());
            let mut atom_ids = f["occurrence_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| {
                    occurrences
                        .iter()
                        .find(|o| o["occurrence_id"] == *id)
                        .unwrap()["original_atom"]["atom"]["atom_id"]
                        .clone()
                })
                .collect::<Vec<_>>();
            atom_ids.sort_by_key(|v| v.as_str().unwrap().to_owned());
            let mut projection = f.clone();
            let obj = projection.as_object_mut().unwrap();
            obj.remove("fact_id");
            obj.remove("occurrence_ids");
            obj.insert("input_ids".into(), json!(ids));
            obj.insert("original_atom_ids".into(), json!(atom_ids));
            actual_facts.push(projection);
            let unknown = if f["applicability_status"] == "unknown" {
                f["occurrence_ids"][0].clone()
            } else {
                Value::Null
            };
            let t = &f["time"];
            let key = json!([
                "coordination_v3",
                f["object_id"],
                f["property_id"],
                f["value"],
                f["unit"],
                f["applicability_status"],
                f["conditions"],
                f["exceptions"],
                [t["status"], t["precision"], t["value"]],
                unknown
            ]);
            assert_eq!(
                f["fact_id"],
                format!("fact-v3-{}", hash(&serde_json::to_vec(&key).unwrap()))
            );
        }
        let mut expected_facts = expected["facts"].as_array().unwrap().clone();
        for f in &mut expected_facts {
            f["conditions"] = sorted(&f["conditions"]);
            f["exceptions"] = sorted(&f["exceptions"]);
        }
        canonical(&mut actual_facts);
        canonical(&mut expected_facts);
        assert_eq!(actual_facts, expected_facts);
        let mut actual_relations=report["relations"].as_array().unwrap().iter().map(|r|{
            let mut pair=[by_id(&r["left_occurrence_id"]),by_id(&r["right_occurrence_id"])];pair.sort_by_key(|v|v.as_str().unwrap().to_owned());
            json!({"left_input_id":pair[0],"right_input_id":pair[1],"relation":r["relation"],"issue_code":r["issue_code"],"earlier_input_id":if r["earlier_occurrence_id"].is_null(){Value::Null}else{by_id(&r["earlier_occurrence_id"])},"later_input_id":if r["later_occurrence_id"].is_null(){Value::Null}else{by_id(&r["later_occurrence_id"])}})
        }).collect::<Vec<_>>();
        let mut er = expected["relations"].as_array().unwrap().clone();
        canonical(&mut actual_relations);
        canonical(&mut er);
        assert_eq!(actual_relations, er);
        assert_eq!(read(&self.output.join("coordination.json")), *report);
        let md = fs::read_to_string(self.output.join("coordination.md")).unwrap();
        let embedded = md
            .split("```json\n")
            .nth(1)
            .unwrap()
            .split("\n```\n")
            .next()
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(embedded).unwrap(), *report);
        if report["status"] == "passed" {
            let ir = read(&self.output.join("coordination-ir.json"));
            for k in [
                "schema_version",
                "request_id",
                "scope",
                "project_id",
                "knowledge_status",
                "published",
                "scientific_quality",
                "research_status",
                "canonical_concepts",
                "inputs",
                "occurrences",
                "facts",
                "relations",
                "totals",
                "processing_records",
            ] {
                assert_eq!(ir[k], report[k], "IR {k}");
            }
        } else {
            assert!(!self.output.join("coordination-ir.json").exists());
        }
        self.assert_protected();
    }
}
#[test]
fn frozen_same_context_conflict_is_reported_after_successful_source_review() {
    let f = ConflictsCase::prepare("true_conflict");
    let r = f.coordinate();
    f.assert_protected();
    assert_eq!(
        r.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let report: Value = serde_json::from_slice(&r.stdout).unwrap();
    f.assert_author_expectations(&report);
}

fn check_case(name: &str) {
    let f = ConflictsCase::prepare(name);
    let expected = read(&f.root.join("expected.json"));
    let r = f.coordinate();
    f.assert_protected();
    assert_eq!(
        r.status.code(),
        Some(if expected["expected_status"] == "passed" {
            0
        } else {
            3
        }),
        "{name}: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    let report: Value = serde_json::from_slice(&r.stdout).unwrap();
    f.assert_author_expectations(&report);
}
#[test]
fn majority_support_retains_all_three_parties_and_two_conflict_pairs() {
    check_case("majority_conflict");
}
#[test]
fn unresolved_conflict_does_not_hide_the_passed_complement() {
    check_case("mixed_conflict");
}
#[test]
fn conditions_history_and_exact_decimal_equality_avoid_false_conflicts() {
    for name in [
        "condition_variant",
        "history",
        "same_source_history",
        "numeric_equal",
        "signed_zero",
    ] {
        check_case(name);
    }
}
#[test]
fn missing_or_incomparable_evidence_is_located_without_inventing_values() {
    for name in [
        "undated_difference",
        "unknown_condition",
        "mixed_state",
        "unit_difference",
        "overlapping_time",
        "same_source_undated",
        "non_decimal",
    ] {
        check_case(name);
    }
}
#[test]
fn explicit_not_applicable_scope_can_contain_a_true_conflict() {
    check_case("not_applicable");
}
#[test]
fn real_counter_evidence_is_preserved_with_its_original_link_and_position() {
    check_case("counter_support");
}
#[test]
fn input_order_and_display_names_never_select_a_winner() {
    let mut f = ConflictsCase::prepare("majority_conflict");
    let first = f.coordinate();
    assert_eq!(first.status.code(), Some(3));
    let a: Value = serde_json::from_slice(&first.stdout).unwrap();
    f.assert_author_expectations(&a);
    let p = f.case.join("coordination.json");
    let mut manifest = read(&p);
    manifest["inputs"].as_array_mut().unwrap().reverse();
    write(&p, &manifest);
    let p = f.case.join("identity.json");
    let mut identity = read(&p);
    for key in [
        "concepts",
        "concept_mappings",
        "atom_mappings",
        "atom_contexts",
    ] {
        identity[key].as_array_mut().unwrap().reverse();
    }
    for c in identity["concepts"].as_array_mut().unwrap() {
        c["display_name"] = json!("任意展示名称");
    }
    write(&p, &identity);
    f.output = f.root.join("permuted-output");
    f.freeze();
    let second = f.coordinate();
    assert_eq!(second.status.code(), Some(3));
    let b: Value = serde_json::from_slice(&second.stdout).unwrap();
    for key in [
        "occurrences",
        "facts",
        "relations",
        "processing_records",
        "totals",
    ] {
        assert_eq!(a[key], b[key], "{key}");
    }
    f.assert_author_expectations(&b);
}
#[test]
fn broken_counter_reference_is_input_failure_before_any_conflict_report() {
    let mut f = ConflictsCase::prepare("counter_support");
    let p = f.case.join("bundle-a/candidates-original.json");
    let mut candidate = read(&p);
    candidate["payload"]["atoms"][0]["atom"]["evidence_links"][1]["evidence_id"] =
        json!("missing-refutation");
    write(&p, &candidate);
    let bytes = fs::read(&p).unwrap();
    let p = f.case.join("coordination.json");
    let mut manifest = read(&p);
    manifest["inputs"][0]["artifacts"]["candidates-original.json"] =
        json!({"sha256":hash(&bytes),"byte_length":bytes.len()});
    write(&p, &manifest);
    f.freeze();
    let run = f.coordinate();
    f.assert_protected();
    assert!(matches!(run.status.code(), Some(2 | 3)));
    assert!(!f.output.join("coordination.json").exists());
    assert!(!f.output.join("coordination-ir.json").exists());
}
#[test]
fn missing_support_is_not_promoted_to_a_legitimate_viewpoint() {
    let mut f = ConflictsCase::prepare("true_conflict");
    let p = f.case.join("bundle-a/candidates-original.json");
    let mut candidate = read(&p);
    candidate["payload"]["atoms"][0]["atom"]["evidence_links"][0]["relation"] = json!("refutes");
    write(&p, &candidate);
    let bytes = fs::read(&p).unwrap();
    let p = f.case.join("coordination.json");
    let mut manifest = read(&p);
    manifest["inputs"][0]["artifacts"]["candidates-original.json"] =
        json!({"sha256":hash(&bytes),"byte_length":bytes.len()});
    write(&p, &manifest);
    f.freeze();
    let run = f.coordinate();
    f.assert_protected();
    assert_eq!(run.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&run.stderr).contains("COORD_SOURCE_REVIEW"));
    assert!(!f.output.join("coordination.json").exists());
    assert!(!f.output.join("coordination-ir.json").exists());
}
#[test]
fn output_collision_and_case_overlap_preserve_human_content() {
    let mut f = ConflictsCase::prepare("true_conflict");
    fs::create_dir(&f.output).unwrap();
    fs::write(f.output.join("human.txt"), b"preserve").unwrap();
    let run = f.coordinate();
    assert_eq!(run.status.code(), Some(2));
    f.assert_protected();
    assert_eq!(fs::read(f.output.join("human.txt")).unwrap(), b"preserve");
    f.output = f.case.join("nested-output");
    let run = f.coordinate();
    assert_eq!(run.status.code(), Some(2));
    f.assert_protected();
    assert!(!f.output.exists());
}

#[test]
fn large_decimal_inequality_is_compared_exactly_without_float_rounding() {
    check_case("precise_large_decimal");
}
#[test]
fn independent_unknown_condition_does_not_taint_complementary_property() {
    check_case("unknown_complement");
}
#[test]
fn counter_evidence_in_nonzero_utf8_unit_keeps_absolute_positions() {
    let f = ConflictsCase::nonzero_unit("counter_support", |_, _| {});
    let run = f.coordinate();
    f.assert_protected();
    assert_eq!(
        run.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report: Value = serde_json::from_slice(&run.stdout).unwrap();
    f.assert_author_expectations(&report);
    let occurrence = report["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["input_id"] == "a")
        .unwrap();
    assert!(occurrence["unit"]["start_byte"].as_u64().unwrap() > 0);
    let counter = occurrence["original_evidence"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["evidence_id"] == "a-refutes")
        .unwrap();
    let source = fs::read_to_string(f.root.join("selected-a.txt")).unwrap();
    let quote = counter["quote"].as_str().unwrap();
    assert_eq!(counter["excerpt_start_byte"], source.find(quote).unwrap());
    assert_eq!(counter["excerpt_sha256"], hash(quote.as_bytes()));
}

#[test]
fn qualifier_links_preserve_relations_without_duplicating_original_evidence() {
    let f = ConflictsCase::prepare_variant(
        "counter_support",
        |author| {
            // This explicit author override is frozen before any source/candidate CLI.
            author["expected"]["totals"]["evidence_links"] = json!(4);
        },
        |_, _, _| {},
        |id, candidate| {
            if id == "a" {
                candidate["payload"]["atoms"][0]["atom"]["evidence_links"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({
                        "evidence_id":"a-support", "relation":"qualifies"
                    }));
            }
        },
    );
    let run = f.coordinate();
    assert_eq!(run.status.code(), Some(3));
    let report: Value = serde_json::from_slice(&run.stdout).unwrap();
    f.assert_author_expectations(&report);
    let occurrence = report["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["input_id"] == "a")
        .unwrap();
    assert_eq!(occurrence["original_evidence"].as_array().unwrap().len(), 2);
    assert_eq!(
        occurrence["original_atom"]["atom"]["evidence_links"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn source_counter_to_the_claim_is_retained_without_a_fake_verified_verdict() {
    let f = ConflictsCase::prepare("counter_to_claim");
    let run = f.coordinate();
    assert_eq!(run.status.code(), Some(3));
    let report: Value = serde_json::from_slice(&run.stdout).unwrap();
    f.assert_author_expectations(&report);
    let occurrence = report["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["input_id"] == "a")
        .unwrap();
    let counter = occurrence["original_evidence"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["evidence_id"] == "a-refutes")
        .unwrap();
    assert_eq!(
        counter["quote"],
        "反证记录：同一范围的速度 10 m/s 不获本来源支持。"
    );
    assert_eq!(
        occurrence["original_atom"]["atom"]["evidence_links"][1],
        json!({"evidence_id":"a-refutes", "relation":"refutes"})
    );
    assert_eq!(report["scientific_quality"], "not_run");
    assert_eq!(report["knowledge_status"], "pending");
}
