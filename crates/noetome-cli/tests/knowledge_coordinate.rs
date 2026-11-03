use serde_json::{Value, json};
use std::{fs, path::Path};
#[path = "support/coordination_case.rs"]
mod support;
use support::*;
#[test]
fn ordinary_cli_coalesces_duplicate_preserving_three_sources() {
    let f = PreparedCase::baseline();
    let result = f.coordinate();
    f.save_cli("first-coordinate", &result);
    assert_eq!(
        result.status.code(),
        Some(0),
        "{} root={}",
        String::from_utf8_lossy(&result.stderr),
        f.root.display()
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["totals"]["canonical_facts"], 2);
    assert_eq!(report["totals"]["source_occurrences"], 3);
    assert_eq!(report["totals"]["independent_sources"], 3);
    assert_eq!(report["totals"]["evidence_links"], 3);
    let velocity = report["facts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["value"] == "10" && x["unit"] == "m/s")
        .unwrap();
    assert_eq!(velocity["occurrence_ids"].as_array().unwrap().len(), 2);
    f.assert_protected();
    println!("positive root={}", f.root.display());
}
fn read(p: &Path) -> Value {
    serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
}
fn write(p: &Path, v: &Value) {
    fs::write(p, encode(v)).unwrap()
}
fn repin(f: &PreparedCase) {
    let p = f.case.join("coordination.json");
    let mut v = read(&p);
    for i in v["inputs"].as_array_mut().unwrap() {
        let dir = f.case.join(i["review_bundle"].as_str().unwrap());
        for n in FILES {
            let b = fs::read(dir.join(n)).unwrap();
            i["artifacts"][n] = json!({"sha256":hash(&b),"byte_length":b.len()});
        }
    }
    write(&p, &v);
}
fn refresh_old_fingerprints(f: &PreparedCase) {
    let dir = f.case.join("bundle-b");
    let p = dir.join("verification.json");
    let mut report = read(&p);
    for n in FILES
        .into_iter()
        .filter(|n| *n != "verification.json" && *n != "verification.md")
    {
        let b = fs::read(dir.join(n)).unwrap();
        report["artifacts"][n] = json!({"sha256":hash(&b),"byte_length":b.len()});
    }
    for (role, n) in [
        ("review.json", "review-original.json"),
        ("candidates.json", "candidates-original.json"),
        ("gold.json", "gold-original.json"),
        ("decision.json", "decision-original.json"),
    ] {
        let b = fs::read(dir.join(n)).unwrap();
        report["origin"]["input_fingerprints"][role] =
            json!({"sha256":hash(&b),"byte_length":b.len()});
    }
    write(&p, &report);
    let md = fs::read_to_string(dir.join("verification.md")).unwrap();
    let prefix = md.split("```json\n").next().unwrap();
    fs::write(
        dir.join("verification.md"),
        format!(
            "{prefix}```json\n{}\n```\n",
            String::from_utf8(encode(&report)).unwrap()
        ),
    )
    .unwrap();
    repin(f);
}
#[test]
fn forged_candidate_and_ir_with_coherent_all_fingerprints_fail_real_assessment() {
    let f = PreparedCase::baseline();
    let dir = f.case.join("bundle-b");
    for n in ["candidates-original.json", "knowledge-ir.json"] {
        let p = dir.join(n);
        let mut v = read(&p);
        let atoms = if n == "knowledge-ir.json" {
            &mut v["atoms"]
        } else {
            &mut v["payload"]["atoms"]
        };
        atoms[0]["measurement"]["value"] = json!("99");
        atoms[0]["atom"]["content"] =
            json!("样例 A 的速度为 99 m/s，条件为温度 20 ℃且仅适用样例 A；例外为结冰时不适用。");
        write(&p, &v);
    }
    refresh_old_fingerprints(&f);
    reject(f, 3, "COORD_SOURCE_REVIEW");
}
fn reject(mut f: PreparedCase, code: i32, error: &str) {
    f.freeze();
    let r = f.coordinate();
    assert_eq!(
        r.status.code(),
        Some(code),
        "{} root={}",
        String::from_utf8_lossy(&r.stderr),
        f.root.display()
    );
    assert!(
        String::from_utf8_lossy(&r.stderr).contains(error),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    assert!(!f.output.join("coordination-ir.json").exists());
    f.assert_protected();
}
#[test]
fn reordered_inputs_mapping_and_display_names_preserve_ids() {
    let mut f = PreparedCase::baseline();
    let r = f.coordinate();
    assert_eq!(r.status.code(), Some(0));
    let first: Value = serde_json::from_slice(&r.stdout).unwrap();
    let p = f.case.join("identity.json");
    let mut map = read(&p);
    for c in map["concepts"].as_array_mut().unwrap() {
        c["display_name"] = json!(format!("任意名字 {}", c["canonical_id"]));
    }
    for k in ["concepts", "concept_mappings", "atom_mappings"] {
        map[k].as_array_mut().unwrap().reverse();
    }
    write(&p, &map);
    let p = f.case.join("coordination.json");
    let mut case = read(&p);
    case["inputs"].as_array_mut().unwrap().reverse();
    write(&p, &case);
    f.output = f.root.join("reordered-output");
    f.freeze();
    let r = f.coordinate();
    assert_eq!(
        r.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let second: Value = serde_json::from_slice(&r.stdout).unwrap();
    for k in ["occurrences", "facts", "relations", "totals", "inputs"] {
        assert_eq!(first[k], second[k], "{k}");
    }
    f.assert_protected();
}
#[test]
fn legal_two_source_case_has_one_fact() {
    let mut f = PreparedCase::baseline();
    let p = f.case.join("coordination.json");
    let mut case = read(&p);
    case["inputs"].as_array_mut().unwrap().pop();
    write(&p, &case);
    fs::rename(f.case.join("bundle-c"), f.root.join("unused-bundle-c")).unwrap();
    let p = f.case.join("identity.json");
    let mut map = read(&p);
    let mass = map["concepts"][2]["canonical_id"].clone();
    map["concepts"]
        .as_array_mut()
        .unwrap()
        .retain(|c| c["canonical_id"] != mass);
    for k in ["concept_mappings", "atom_mappings"] {
        map[k]
            .as_array_mut()
            .unwrap()
            .retain(|m| m["input_id"] != "c");
    }
    write(&p, &map);
    f.freeze();
    let r = f.coordinate();
    assert_eq!(
        r.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["totals"]["canonical_facts"], 1);
    assert_eq!(v["totals"]["independent_sources"], 2);
    assert_eq!(v["totals"]["source_occurrences"], 2);
    f.assert_protected();
}
#[test]
fn same_sha_binding_swap_rejected() {
    let f = PreparedCase::baseline();
    let p = f.case.join("coordination.json");
    let mut c = read(&p);
    assert_eq!(
        c["inputs"][0]["binding"]["content_sha256"],
        c["inputs"][1]["binding"]["content_sha256"]
    );
    c["inputs"][1]["binding"] = c["inputs"][0]["binding"].clone();
    write(&p, &c);
    reject(f, 3, "COORD_SOURCE_REVIEW");
}
#[test]
fn copied_actual_unit_rejected() {
    let f = PreparedCase::baseline();
    for n in FILES {
        fs::copy(
            f.case.join("bundle-a").join(n),
            f.case.join("bundle-b").join(n),
        )
        .unwrap();
    }
    let p = f.case.join("coordination.json");
    let mut c = read(&p);
    c["inputs"][1]["binding"] = c["inputs"][0]["binding"].clone();
    write(&p, &c);
    repin(&f);
    reject(f, 2, "COORD_DUPLICATE_INPUT");
}
#[test]
fn repinned_ir_candidate_measurement_and_extra_evidence_rejected() {
    for kind in ["ir", "measurement", "evidence"] {
        let f = PreparedCase::baseline();
        let dir = f.case.join("bundle-b");
        let n = if kind == "ir" {
            "knowledge-ir.json"
        } else {
            "candidates-original.json"
        };
        let p = dir.join(n);
        let mut v = read(&p);
        if kind == "ir" {
            v["atoms"][0]["atom"]["content"] = json!("同步伪造的 IR 内容");
        } else if kind == "measurement" {
            v["payload"]["atoms"][0]["measurement"]["value"] = json!("99");
        } else {
            let mut e = v["payload"]["evidence"][0].clone();
            e["evidence_id"] = json!("second-support");
            v["payload"]["evidence"].as_array_mut().unwrap().push(e);
            v["payload"]["atoms"][0]["atom"]["evidence_links"]
                .as_array_mut()
                .unwrap()
                .push(json!({"evidence_id":"second-support","relation":"supports"}));
        }
        write(&p, &v);
        refresh_old_fingerprints(&f);
        reject(f, 3, "COORD_SOURCE_REVIEW");
    }
}
#[test]
fn repinned_fake_old_counts_and_markdown_rejected() {
    for value in [Value::Null, json!(0), json!(9)] {
        let f = PreparedCase::baseline();
        let dir = f.case.join("bundle-b");
        let p = dir.join("verification.json");
        let mut v = read(&p);
        v["totals"]["candidate_atoms"] = value;
        write(&p, &v);
        let md = fs::read_to_string(dir.join("verification.md")).unwrap();
        let prefix = md.split("```json\n").next().unwrap();
        fs::write(
            dir.join("verification.md"),
            format!(
                "{prefix}```json\n{}\n```\n",
                String::from_utf8(encode(&v)).unwrap()
            ),
        )
        .unwrap();
        repin(&f);
        reject(f, 3, "COORD_SOURCE_REVIEW");
    }
}
#[test]
fn author_condition_and_unit_differences_require_review() {
    for (from, to) in [("20", "30"), ("m/s", "km/s")] {
        let f = PreparedCase::custom(|id, source, gold| {
            if id == "b" {
                *source = source.replace(from, to);
                let s = serde_json::to_string(gold).unwrap().replace(from, to);
                *gold = serde_json::from_str(&s).unwrap();
            }
        });
        let r = f.coordinate();
        assert_eq!(
            r.status.code(),
            Some(3),
            "{}",
            String::from_utf8_lossy(&r.stderr)
        );
        let v: Value = serde_json::from_slice(&r.stdout).unwrap();
        assert_eq!(v["status"], "needs_review");
        assert_eq!(v["totals"]["canonical_facts"], Value::Null);
        assert_eq!(v["totals"]["retained_fact_groups"], 3);
        assert_eq!(v["occurrences"].as_array().unwrap().len(), 3);
        assert!(!f.output.join("coordination-ir.json").exists());
        f.assert_protected();
    }
}
#[test]
fn mapping_missing_unknown_duplicate_wrong_role_rejected() {
    for kind in ["missing", "unknown", "duplicate", "role"] {
        let f = PreparedCase::baseline();
        let p = f.case.join("identity.json");
        let mut map = read(&p);
        match kind {
            "missing" => {
                map["concept_mappings"].as_array_mut().unwrap().pop();
            }
            "unknown" => map["atom_mappings"][0]["original_atom_id"] = json!("unknown"),
            "duplicate" => {
                let c = map["concept_mappings"][0].clone();
                map["concept_mappings"].as_array_mut().unwrap().push(c);
            }
            _ => {
                let old = map["atom_mappings"][0]["object_id"].clone();
                map["atom_mappings"][0]["object_id"] =
                    map["atom_mappings"][0]["property_id"].clone();
                map["atom_mappings"][0]["property_id"] = old;
            }
        }
        write(&p, &map);
        reject(f, 2, "COORD_IDENTITY");
    }
}
#[test]
fn strict_schema_extra_duplicate_nullable_and_inventory_rejected() {
    for kind in ["extra", "duplicate", "nullable", "inventory"] {
        let f = PreparedCase::baseline();
        let p = f.case.join("coordination.json");
        let mut c = read(&p);
        match kind {
            "extra" => {
                c["unknown"] = json!(1);
                write(&p, &c)
            }
            "duplicate" => {
                let b = fs::read_to_string(&p).unwrap().replacen(
                    "\"schema_version\": 1",
                    "\"schema_version\":1,\"schema_version\":1",
                    1,
                );
                fs::write(&p, b).unwrap();
            }
            "nullable" => {
                c["inputs"][0]["binding"]
                    .as_object_mut()
                    .unwrap()
                    .remove("parse_id");
                write(&p, &c)
            }
            _ => fs::write(f.case.join("unknown-human.txt"), b"keep").unwrap(),
        };
        reject(f, 2, "COORD_SCHEMA");
    }
}
#[test]
fn output_collision_overlap_hardlink_and_junction_preserve_inputs() {
    let f = PreparedCase::baseline();
    fs::create_dir(&f.output).unwrap();
    fs::write(f.output.join("human.txt"), b"human output").unwrap();
    let p = f.output.join("human.txt");
    reject(f, 2, "EXTRACT_OUTPUT_EXISTS");
    assert_eq!(fs::read(p).unwrap(), b"human output");
    let mut f = PreparedCase::baseline();
    f.output = f.case.join("embedded");
    reject(f, 2, "EXTRACT_OUTPUT_OVERLAP");
    let f = PreparedCase::baseline();
    fs::hard_link(
        f.case.join("identity.json"),
        f.root.join("actual-map-alias.json"),
    )
    .unwrap();
    reject(f, 2, "COORD_INPUT_PATH");
}

#[test]
fn same_source_distinct_registered_versions_require_review() {
    let f = PreparedCase::versions(
        |id, source, _| {
            if id == "b" {
                source.push_str("\n\n来源 B 的后续普通版本记录。");
            }
        },
        true,
    );
    let r = f.coordinate();
    assert_eq!(
        r.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&r.stderr)
    );
    let v: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(v["totals"]["independent_sources"], 2);
    assert_eq!(v["totals"]["canonical_facts"], Value::Null);
    assert!(
        v["relations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["relation"] == "needs_review")
    );
    f.assert_protected();
}
#[cfg(windows)]
#[test]
fn actual_junction_case_and_output_parent_are_rejected() {
    for input in [true, false] {
        let mut f = PreparedCase::baseline();
        let junction = f.root.join("actual-junction");
        let target = if input {
            f.case.clone()
        } else {
            f.project.clone()
        };
        let r = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .output()
            .unwrap();
        f.save_cli("actual-junction-create", &r);
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        if input {
            f.case = junction;
        } else {
            f.output = junction.join("output");
        }
        let r = f.coordinate();
        assert_eq!(
            r.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&r.stderr)
        );
        assert!(!f.output.join("coordination-ir.json").exists());
        f.assert_protected();
    }
}
#[test]
fn knowledge_help_and_invalid_arguments_are_filesystem_free() {
    for args in [
        vec!["knowledge"],
        vec!["knowledge", "--help"],
        vec!["knowledge", "coordinate", "--help"],
    ] {
        let r = std::process::Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(r.status.code(), Some(0));
        assert!(String::from_utf8_lossy(&r.stdout).contains("pending"));
    }
    for args in [
        vec!["knowledge", "wrong"],
        vec!["knowledge", "coordinate"],
        vec!["knowledge", "coordinate", "a", "b", "c", "extra"],
        vec!["knowledge", "coordinate", "-a", "b", "c"],
    ] {
        let r = std::process::Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(r.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&r.stderr).contains("CLI_INVALID_ARGUMENT"));
    }
}
