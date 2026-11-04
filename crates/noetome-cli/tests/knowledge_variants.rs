use serde_json::{Value, json};
use std::{fs, path::Path};
#[path = "support/coordination_variants_case.rs"]
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
impl VariantsCase {
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
            let evidence = candidate["payload"]["atoms"][0]["atom"]["evidence_links"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|l| l["relation"] == "supports")
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
                "coordination_v2",
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
                format!("fact-v2-{}", hash(&serde_json::to_vec(&key).unwrap()))
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
fn conditional_variants_preserve_all_author_fields() {
    let f = VariantsCase::prepare("conditional");
    let r = f.coordinate();
    f.assert_protected();
    println!("root={}", f.root.display());
    assert_eq!(
        r.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let report: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(report["schema_version"], 2);
    assert_eq!(report["totals"]["retained_fact_groups"], 2);
    assert_eq!(report["totals"]["condition_variants"], 2);
    f.assert_author_expectations(&report);
}
fn author_case(name: &str) {
    let f = VariantsCase::prepare(name);
    let r = f.coordinate();
    println!("case={name} root={}", f.root.display());
    let expected = read(&f.root.join("expected.json"));
    assert_eq!(
        r.status.code(),
        Some(if expected["expected_status"] == "passed" {
            0
        } else {
            3
        }),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    f.assert_author_expectations(&serde_json::from_slice(&r.stdout).unwrap());
}
macro_rules! cases{($($name:ident),*)=>{$(#[test]fn $name(){author_case(stringify!($name));})*}}
cases!(
    history,
    history_day,
    unknown_condition,
    not_applicable,
    mixed_state,
    unit_difference,
    undated_difference,
    overlapping_time,
    exception_difference,
    combined_context,
    same_source_history,
    same_source_undated,
    same_source_condition
);
fn reject(mut f: VariantsCase, exit: i32, code: &str) {
    f.freeze();
    let r = f.coordinate();
    println!("rejection={code} root={}", f.root.display());
    assert_eq!(
        r.status.code(),
        Some(exit),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    assert!(String::from_utf8_lossy(&r.stderr).contains(code));
    assert!(!f.output.join("coordination-ir.json").exists());
    f.assert_protected();
}
fn mutate_map(change: impl Fn(&mut Value), exit: i32, code: &str) {
    let f = VariantsCase::prepare("history");
    let path = f.case.join("identity.json");
    let mut v = read(&path);
    change(&mut v);
    write(&path, &v);
    reject(f, exit, code);
}
#[test]
fn declared_context_must_match_actual_source() {
    for kind in [
        "date",
        "status",
        "support",
        "missing",
        "duplicate",
        "unused",
        "empty",
        "borrowed",
        "duplicate_support",
    ] {
        mutate_map(
            |v| match kind {
                "date" => v["atom_contexts"][0]["time"]["value"] = json!("1987"),
                "status" => v["atom_contexts"][0]["applicability_status"] = json!("unknown"),
                "support" => {
                    v["atom_contexts"][0]["context_evidence_ids"] = json!(["missing-support"])
                }
                "missing" => {
                    v["atom_contexts"].as_array_mut().unwrap().remove(0);
                }
                "duplicate" => {
                    let c = v["atom_contexts"][0].clone();
                    v["atom_contexts"].as_array_mut().unwrap().push(c);
                }
                "unused" => {
                    let mut c = v["atom_contexts"][0].clone();
                    c["input_id"] = json!("z");
                    v["atom_contexts"].as_array_mut().unwrap().push(c);
                }
                "empty" => v["atom_contexts"][0]["context_evidence_ids"] = json!([]),
                "borrowed" => v["atom_contexts"][0]["context_evidence_ids"] = json!(["b-support"]),
                "duplicate_support" => {
                    v["atom_contexts"][0]["context_evidence_ids"] =
                        json!(["a-support", "a-support"])
                }
                _ => unreachable!(),
            },
            3,
            "COORD_CONTEXT",
        );
    }
}
#[test]
fn unknown_and_not_applicable_cannot_be_exchanged() {
    for (case, status) in [
        ("unknown_condition", "not_applicable"),
        ("not_applicable", "unknown"),
    ] {
        let f = VariantsCase::prepare(case);
        let p = f.case.join("identity.json");
        let mut v = read(&p);
        v["atom_contexts"][0]["applicability_status"] = json!(status);
        write(&p, &v);
        reject(f, 3, "COORD_CONTEXT");
    }
}
#[test]
fn time_nullable_contract_and_gregorian_values_are_enforced() {
    for (precision, value) in [
        (json!(null), json!("2020")),
        (json!("year"), json!(null)),
        (json!("year"), json!("0000")),
        (json!("year"), json!("20")),
        (json!("year"), json!("２０２０")),
        (json!("day"), json!("1900-02-29")),
        (json!("day"), json!("2020-13-01")),
        (json!("day"), json!("2020-00-01")),
        (json!("day"), json!("2020-01-00")),
    ] {
        mutate_map(
            |v| {
                v["atom_contexts"][0]["time"]["precision"] = precision.clone();
                v["atom_contexts"][0]["time"]["value"] = value.clone();
            },
            3,
            "COORD_CONTEXT",
        );
    }
    mutate_map(
        |v| {
            v["atom_contexts"][0]["time"] =
                json!({"status":"unknown","precision":"year","value":"2020"})
        },
        3,
        "COORD_CONTEXT",
    );
}
#[test]
fn schema_two_strict_fields_enums_and_versions() {
    for kind in [
        "unknown",
        "missing_time",
        "missing_nullable",
        "bad_status",
        "bad_precision",
        "bad_type",
        "version",
    ] {
        mutate_map(
            |v| match kind {
                "unknown" => v["atom_contexts"][0]["extra"] = json!(1),
                "missing_time" => {
                    v["atom_contexts"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("time");
                }
                "missing_nullable" => {
                    v["atom_contexts"][0]["time"]
                        .as_object_mut()
                        .unwrap()
                        .remove("precision");
                }
                "bad_status" => v["atom_contexts"][0]["applicability_status"] = json!("inferred"),
                "bad_precision" => v["atom_contexts"][0]["time"]["precision"] = json!("month"),
                "bad_type" => v["atom_contexts"] = json!(1),
                "version" => v["schema_version"] = json!(1),
                _ => unreachable!(),
            },
            2,
            "COORD_SCHEMA",
        );
    }
    for kind in ["unknown", "version", "missing", "duplicate", "invalid"] {
        let f = VariantsCase::prepare("conditional");
        let p = f.case.join("coordination.json");
        let mut v = read(&p);
        match kind {
            "unknown" => v["atom_contexts"] = json!([]),
            "version" => v["schema_version"] = json!(3),
            "missing" => {
                v.as_object_mut().unwrap().remove("purpose");
            }
            "duplicate" | "invalid" => {}
            _ => unreachable!(),
        }
        if kind == "duplicate" {
            fs::write(&p, b"{\"schema_version\":2,\"schema_version\":2}").unwrap();
        } else if kind == "invalid" {
            fs::write(&p, b"{").unwrap();
        } else {
            write(&p, &v);
        }
        reject(f, 2, "COORD_SCHEMA");
    }
    let f = VariantsCase::prepare("history");
    let p = f.case.join("identity.json");
    let s = fs::read_to_string(&p).unwrap();
    let duplicate = s.replacen("\"time\": {", "\"time\": {\"status\":\"known\",", 1);
    assert_ne!(s, duplicate);
    fs::write(p, duplicate).unwrap();
    reject(f, 2, "COORD_SCHEMA");
    let f = VariantsCase::prepare("conditional");
    let p = f.case.join("coordination.json");
    let mut v = read(&p);
    v["schema_version"] = json!(1);
    v["purpose"] = json!("explicit_identity_duplicate_complement");
    v["atom_contexts"] = json!([]);
    write(&p, &v);
    reject(f, 2, "COORD_SCHEMA");
}
#[test]
fn background_must_be_complete_adjacent_and_fact_unique() {
    for kind in ["missing", "gap", "inline", "duplicate_fact", "invalid_date"] {
        let f = VariantsCase::custom("history", |id, source, gold| {
            if id == "a" {
                match kind {
                    "missing" => *source = format!("{}\n", gold["quote"].as_str().unwrap()),
                    "gap" => {
                        let fact = gold["quote"].as_str().unwrap();
                        *source = source
                            .replace(&format!("\n{fact}"), &format!("\n额外不相邻段落。\n{fact}"));
                    }
                    "inline" => *source = source.replace("适用条件：", "前缀适用条件："),
                    "duplicate_fact" => source.push_str(gold["quote"].as_str().unwrap()),
                    "invalid_date" => *source = source.replace("2020年", "1900-02-29"),
                    _ => unreachable!(),
                }
            }
        });
        reject(f, 3, "COORD_CONTEXT");
    }
}
#[test]
fn multiple_supports_keep_original_order_and_exact_context_set() {
    let mut f = VariantsCase::prepare_variant(
        "conditional",
        |a| {
            for i in a["inputs"].as_array_mut().unwrap() {
                let id = i["input_id"].as_str().unwrap();
                i["context"]["context_evidence_ids"] =
                    json!([format!("{id}-support-z"), format!("{id}-support")]);
            }
            a["identity"]["atom_contexts"] = json!(
                a["inputs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|i| i["context"].clone())
                    .collect::<Vec<_>>()
            );
            a["expected"]["totals"]["evidence_links"] = json!(9);
            a["expected"]["totals"]["context_evidence_links"] = json!(6);
        },
        |_, _, _| {},
        |id, c| {
            let mut second = c["payload"]["evidence"][0].clone();
            second["evidence_id"] = json!(format!("{id}-support-z"));
            let gold_quote = c["payload"]["atoms"][0]["atom"]["content"]
                .as_str()
                .unwrap()
                .to_owned();
            let mut fact_only = second.clone();
            fact_only["evidence_id"] = json!(format!("{id}-fact-only"));
            let start = fact_only["quote"]
                .as_str()
                .unwrap()
                .find(&gold_quote)
                .unwrap();
            fact_only["excerpt_start_byte"] = json!(start);
            fact_only["excerpt_end_byte"] = json!(start + gold_quote.len());
            fact_only["quote"] = json!(gold_quote);
            fact_only["excerpt_sha256"] = json!(hash(gold_quote.as_bytes()));
            c["payload"]["evidence"]
                .as_array_mut()
                .unwrap()
                .extend([second, fact_only]);
            c["payload"]["atoms"][0]["atom"]["evidence_links"] = json!([{"evidence_id":format!("{id}-support-z"),"relation":"supports"},{"evidence_id":format!("{id}-fact-only"),"relation":"supports"},{"evidence_id":format!("{id}-support"),"relation":"supports"}]);
        },
    );
    let r = f.coordinate();
    assert_eq!(
        r.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let a: Value = serde_json::from_slice(&r.stdout).unwrap();
    f.assert_author_expectations(&a);
    for o in a["occurrences"].as_array().unwrap() {
        let id = o["input_id"].as_str().unwrap();
        assert_eq!(
            o["original_evidence"][0]["evidence_id"],
            format!("{id}-support-z")
        );
        assert_eq!(
            o["original_evidence"][1]["evidence_id"],
            format!("{id}-fact-only")
        );
    }
    let p = f.case.join("identity.json");
    let mut v = read(&p);
    for c in v["atom_contexts"].as_array_mut().unwrap() {
        c["context_evidence_ids"].as_array_mut().unwrap().reverse();
    }
    write(&p, &v);
    f.output = f.root.join("reordered-context-output");
    f.freeze();
    let r = f.coordinate();
    let b: Value = serde_json::from_slice(&r.stdout).unwrap();
    for k in ["occurrences", "facts", "relations", "totals"] {
        assert_eq!(a[k], b[k]);
    }
    f.assert_author_expectations(&b);
}
#[test]
fn support_must_cover_both_background_and_fact() {
    let f = VariantsCase::candidate_custom(
        "conditional",
        |_, _, _| {},
        |_, c| {
            let quote = c["payload"]["atoms"][0]["atom"]["content"]
                .as_str()
                .unwrap()
                .to_owned();
            let ev = &mut c["payload"]["evidence"][0];
            let original = ev["quote"].as_str().unwrap();
            let start = original.find(&quote).unwrap();
            ev["excerpt_start_byte"] = json!(start);
            ev["excerpt_end_byte"] = json!(start + quote.len());
            ev["excerpt_sha256"] = json!(hash(quote.as_bytes()));
            ev["quote"] = json!(quote);
        },
    );
    reject(f, 3, "COORD_CONTEXT");
}
#[test]
fn conditions_and_exceptions_cannot_be_coherently_deleted() {
    for field in ["conditions", "exceptions"] {
        let f = VariantsCase::prepare("conditional");
        let p = f.case.join("bundle-b/gold-original.json");
        let mut v = read(&p);
        v[field] = json!([]);
        write(&p, &v);
        let p = f.case.join("coordination.json");
        let mut manifest = read(&p);
        let bytes = fs::read(f.case.join("bundle-b/gold-original.json")).unwrap();
        manifest["inputs"][1]["artifacts"]["gold-original.json"] =
            json!({"sha256":hash(&bytes),"byte_length":bytes.len()});
        write(&p, &manifest);
        reject(f, 3, "COORD_SOURCE_REVIEW");
    }
}
#[test]
fn display_names_and_all_manifest_order_do_not_change_identity() {
    let mut f = VariantsCase::prepare("conditional");
    let first = f.coordinate();
    let a: Value = serde_json::from_slice(&first.stdout).unwrap();
    f.assert_author_expectations(&a);
    let p = f.case.join("coordination.json");
    let mut v = read(&p);
    v["inputs"].as_array_mut().unwrap().reverse();
    write(&p, &v);
    let p = f.case.join("identity.json");
    let mut v = read(&p);
    for key in [
        "concepts",
        "concept_mappings",
        "atom_mappings",
        "atom_contexts",
    ] {
        v[key].as_array_mut().unwrap().reverse();
    }
    for c in v["concepts"].as_array_mut().unwrap() {
        c["display_name"] = json!("任意展示名称");
    }
    write(&p, &v);
    f.output = f.root.join("reordered-output");
    f.freeze();
    let second = f.coordinate();
    assert_eq!(second.status.code(), Some(0));
    let b: Value = serde_json::from_slice(&second.stdout).unwrap();
    for k in ["occurrences", "facts", "relations", "totals"] {
        assert_eq!(a[k], b[k], "{k}");
    }
    f.assert_author_expectations(&b);
}
#[test]
fn nonzero_utf8_context_range_keeps_actual_source_prefix() {
    let f = VariantsCase::custom("history", |_, s, _| *s = format!("独立资料前言 α。\n{s}"));
    let r = f.coordinate();
    assert_eq!(r.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    f.assert_author_expectations(&v);
    assert!(
        v["occurrences"]
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["context"]["start_byte"].as_u64().unwrap() > 0)
    );
}
#[test]
fn nonzero_source_unit_preserves_frozen_history_and_absolute_context() {
    let f = VariantsCase::nonzero_unit("history", |_, _| {});
    let r = f.coordinate();
    f.assert_protected();
    assert_eq!(
        r.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    f.assert_author_expectations(&v);
    for o in v["occurrences"].as_array().unwrap() {
        assert_eq!(o["unit"]["start_byte"], 25);
        assert_eq!(o["context"]["start_byte"], 57);
        let id = o["input_id"].as_str().unwrap();
        let review = read(&f.case.join(format!("bundle-{id}/review-original.json")));
        assert_eq!(review["selection"], json!([{"kind":"text","ordinal":2}]));
    }
}
#[test]
fn nonzero_source_unit_rejects_support_covering_only_fact() {
    let f = VariantsCase::nonzero_unit("history", |_, c| {
        let quote = c["payload"]["atoms"][0]["atom"]["content"]
            .as_str()
            .unwrap()
            .to_owned();
        let ev = &mut c["payload"]["evidence"][0];
        let start = ev["excerpt_start_byte"].as_u64().unwrap() as usize
            + ev["quote"].as_str().unwrap().find(&quote).unwrap();
        ev["excerpt_start_byte"] = json!(start);
        ev["excerpt_end_byte"] = json!(start + quote.len());
        ev["excerpt_sha256"] = json!(hash(quote.as_bytes()));
        ev["quote"] = json!(quote);
    });
    reject(f, 3, "COORD_CONTEXT");
}
#[test]
fn output_collisions_and_overlap_preserve_existing_content() {
    let mut f = VariantsCase::prepare("conditional");
    fs::create_dir(&f.output).unwrap();
    fs::write(f.output.join("human.txt"), b"preserve").unwrap();
    let r = f.coordinate();
    assert_eq!(r.status.code(), Some(2));
    assert_eq!(fs::read(f.output.join("human.txt")).unwrap(), b"preserve");
    f.output = f.case.join("nested");
    reject(f, 2, "EXTRACT_OUTPUT_OVERLAP");
}
