use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use uuid::Uuid;
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(args)
        .output()
        .unwrap()
}
struct Case {
    root: PathBuf,
    project: PathBuf,
    case: PathBuf,
    output: PathBuf,
    id: String,
    database: Vec<u8>,
    inputs: Vec<(PathBuf, Vec<u8>)>,
}
impl Case {
    fn new(change: impl FnOnce(&mut Value, &mut Value)) -> Self {
        let fixture = repo().join("fixtures/extraction-review-v1");
        Self::custom(
            &fs::read_to_string(fixture.join("source.txt")).unwrap(),
            &fs::read(fixture.join("gold.json")).unwrap(),
            0,
            change,
        )
    }
    fn custom(
        source_text: &str,
        gold_bytes: &[u8],
        ordinal: usize,
        change: impl FnOnce(&mut Value, &mut Value),
    ) -> Self {
        let parent = repo().join("target/compiler-temp");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!("review-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let project = root.join("source 中文");
        assert!(
            cli(&["project", "init", project.to_str().unwrap()])
                .status
                .success()
        );
        let fixture = repo().join("fixtures/extraction-review-v1");
        let source_file = root.join("selected-source.txt");
        fs::write(&source_file, source_text).unwrap();
        let run = cli(&[
            "source",
            "import",
            project.to_str().unwrap(),
            source_file.to_str().unwrap(),
        ]);
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let imported: Value = serde_json::from_slice(&run.stdout).unwrap();
        let config: Value =
            serde_json::from_slice(&fs::read(project.join("project.json")).unwrap()).unwrap();
        let version = &imported["version"];
        let binding = json!({"project_id":config["project_id"],"import_id":imported["import_id"],"source_id":version["source_id"],"version_id":version["version_id"],"content_sha256":version["content_sha256"],"normalized_sha256":version["normalized_sha256"],"parse_id":null,"manifest_sha256":null});
        let case = root.join("review inputs");
        fs::create_dir(&case).unwrap();
        fs::write(case.join("gold.json"), gold_bytes).unwrap();
        let mut candidate: Value =
            serde_json::from_slice(&fs::read(fixture.join("candidates.json")).unwrap()).unwrap();
        candidate["binding"] = binding.clone();
        candidate["payload"]["binding"] = binding;
        let gold: Value = serde_json::from_slice(gold_bytes).unwrap();
        let quote = gold["quote"].as_str().unwrap();
        let start = source_text.find(quote).unwrap();
        candidate["payload"]["evidence"][0] = json!({"evidence_id":"evidence-speed","unit_id":"unit-0","excerpt_start_byte":start,"excerpt_end_byte":start+quote.len(),"excerpt_sha256":hash(quote.as_bytes()),"quote":quote});
        let mut decision: Value =
            serde_json::from_slice(&fs::read(fixture.join("decision.json")).unwrap()).unwrap();
        change(&mut candidate, &mut decision);
        fs::write(
            case.join("candidates.json"),
            serde_json::to_vec(&candidate).unwrap(),
        )
        .unwrap();
        fs::write(
            case.join("decision.json"),
            serde_json::to_vec(&decision).unwrap(),
        )
        .unwrap();
        let mut review: Value =
            serde_json::from_slice(&fs::read(fixture.join("review.json")).unwrap()).unwrap();
        review["gold_sha256"] = json!(hash(&gold_bytes));
        review["selection"] = json!([{"kind":"text","ordinal":ordinal}]);
        fs::write(
            case.join("review.json"),
            serde_json::to_vec(&review).unwrap(),
        )
        .unwrap();
        let database = fs::read(project.join("project.sqlite")).unwrap();
        let inputs = [
            "review.json",
            "candidates.json",
            "gold.json",
            "decision.json",
        ]
        .into_iter()
        .map(|n| {
            let p = case.join(n);
            let b = fs::read(&p).unwrap();
            (p, b)
        })
        .collect();
        Self {
            output: root.join("new output"),
            id: imported["import_id"].as_str().unwrap().into(),
            root,
            project,
            case,
            database,
            inputs,
        }
    }
    fn run(&self) -> Output {
        cli(&[
            "extract",
            "verify",
            self.project.to_str().unwrap(),
            &self.id,
            self.case.to_str().unwrap(),
            self.output.to_str().unwrap(),
        ])
    }
    fn report(&self, code: i32) -> Value {
        let run = self.run();
        fs::write(self.root.join("cli-stdout.json"), &run.stdout).unwrap();
        fs::write(self.root.join("cli-stderr.txt"), &run.stderr).unwrap();
        fs::write(
            self.root.join("cli-exit.txt"),
            run.status.code().unwrap().to_string(),
        )
        .unwrap();
        assert_eq!(
            run.status.code(),
            Some(code),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        let v: Value = serde_json::from_slice(&run.stdout).unwrap();
        assert_eq!(
            fs::read(self.project.join("project.sqlite")).unwrap(),
            self.database
        );
        for (p, b) in &self.inputs {
            assert_eq!(fs::read(p).unwrap(), *b);
        }
        v
    }
    fn rejected(&self, expected: &str) {
        let report = self.report(3);
        assert!(report.to_string().contains(expected), "{report}");
        assert!(!self.output.join("knowledge-ir.json").exists());
        assert_eq!(report["totals"]["expected_atoms"], 1);
    }
}
#[test]
fn author_exception_passes_ordinary_cli() {
    let c = Case::new(|_, _| {});
    let r = c.report(0);
    assert_eq!(r["totals"]["passed"], 1);
    assert_eq!(r["totals"]["candidate_atoms"], 1);
    assert_eq!(r["quality"]["exception"]["expected"], 1);
    assert_eq!(r["published"], false);
    assert!(c.output.join("knowledge-ir.json").is_file());
    let md = fs::read_to_string(c.output.join("verification.md")).unwrap();
    let embedded = md
        .split("```json\n")
        .nth(1)
        .unwrap()
        .split("\n```")
        .next()
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(embedded).unwrap(), r);
}
#[test]
fn missing_both_condition_fields_fails() {
    Case::new(|c, _| {
        c["payload"]["atoms"][0]["measurement"]["conditions"] = json!([]);
        c["payload"]["atoms"][0]["atom"]["qualifiers"] = json!(["m/s", "结冰时不适用"]);
    })
    .rejected("VERIFY_CONDITION");
}
#[test]
fn missing_both_unit_fields_fails() {
    Case::new(|c, _| {
        c["payload"]["atoms"][0]["measurement"]["unit"] = Value::Null;
        c["payload"]["atoms"][0]["atom"]["qualifiers"] =
            json!(["温度 20 ℃", "仅适用样例 A", "结冰时不适用"]);
    })
    .rejected("VERIFY_UNIT");
}
#[test]
fn missing_exception_fails() {
    Case::new(|c, _| {
        c["payload"]["atoms"][0]["atom"]["qualifiers"] =
            json!(["m/s", "温度 20 ℃", "仅适用样例 A"]);
        c["payload"]["atoms"][0]["atom"]["content"] =
            json!("样例 A 的速度为 10 m/s，条件为温度 20 ℃且仅适用样例 A。");
    })
    .rejected("VERIFY_EXCEPTION");
}
#[test]
fn right_quote_wrong_fact_fails() {
    Case::new(|c, _| c["payload"]["atoms"][0]["measurement"]["value"] = json!("11"))
        .rejected("VERIFY_FACT");
    Case::new(|c, _| {
        c["payload"]["atoms"][0]["atom"]["content"] =
            json!("样例 A 的速度不为 10 m/s，条件为温度 20 ℃且仅适用样例 A；例外为结冰时不适用。")
    })
    .rejected("VERIFY_CONTENT");
}
#[test]
fn broken_concept_link_fails() {
    Case::new(|c, _| c["payload"]["atoms"][0]["atom"]["concept_ids"] = json!([]))
        .rejected("VERIFY_CONCEPT");
}
#[test]
fn semantic_duplicate_new_id_fails() {
    let c = Case::new(|c, _| {
        let mut duplicate = c["payload"]["atoms"][0].clone();
        duplicate["atom"]["atom_id"] = json!("duplicate-new-id");
        c["payload"]["atoms"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
    });
    let r = c.report(3);
    assert_eq!(r["totals"]["candidate_atoms"], 2);
    assert_eq!(r["totals"]["duplicate_atoms"], 1);
    assert!(r.to_string().contains("VERIFY_DUPLICATE"));
    assert!(!c.output.join("knowledge-ir.json").exists());
}
#[test]
fn omitted_candidate_is_unprocessed() {
    let c = Case::new(|c, d| {
        c["payload"]["atoms"] = json!([]);
        d["atom_ids"] = json!([]);
    });
    let r = c.report(3);
    assert_eq!(r["totals"]["unprocessed"], 1);
    assert_eq!(r["totals"]["candidate_atoms"], 0);
    assert!(r.to_string().contains("VERIFY_MISSING"));
    assert!(!c.output.join("knowledge-ir.json").exists());
}
#[test]
fn excluded_pending_and_review_are_not_passed() {
    for (disposition, status) in [
        ("excluded", "failed"),
        ("pending", "unprocessed"),
        ("needs_review", "needs_review"),
    ] {
        let c = Case::new(|_, d| {
            d["disposition"] = json!(disposition);
            d["exclusion_reason"] = json!("工程理由");
        });
        let r = c.report(3);
        assert_eq!(r["totals"][status], 1);
        assert!(!c.output.join("knowledge-ir.json").exists());
    }
}
#[test]
fn strict_candidate_and_evidence_errors_have_reports() {
    Case::new(|c, _| c["payload"]["hidden"] = json!(1)).rejected("VERIFY_SCHEMA");
    Case::new(|c, _| c["binding"]["parse_id"] = json!("wrong-parse")).rejected("VERIFY_BINDING");
    Case::new(|c, _| c["payload"]["evidence"][0]["excerpt_start_byte"] = json!(1))
        .rejected("EXTRACT_EVIDENCE_RANGE");
    Case::new(|c, _| c["payload"]["evidence"][0]["excerpt_sha256"] = json!("0".repeat(64)))
        .rejected("EXTRACT_EVIDENCE_SHA");
}
#[test]
fn invalid_evidence_sha_cannot_report_evidence_passed() {
    let c = Case::new(|c, _| c["payload"]["evidence"][0]["excerpt_sha256"] = json!("0".repeat(64)));
    let r = c.report(3);
    assert_eq!(r["quality"]["evidence"]["passed"], 0);
    assert_eq!(r["quality"]["evidence"]["failed"], 1);
    assert!(
        r["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["object_id"] == "evidence-speed"
                && c["status"] == "failed"
                && c["start_byte"] == 0
                && c["end_byte"] == 124)
    );
}
#[test]
fn existing_embedded_and_hardlinked_paths_rejected() {
    let c = Case::new(|_, _| {});
    fs::create_dir(&c.output).unwrap();
    assert_eq!(c.run().status.code(), Some(2));
    let mut c = Case::new(|_, _| {});
    c.output = c.project.join("embedded");
    assert_eq!(c.run().status.code(), Some(2));
    let c = Case::new(|_, _| {});
    fs::hard_link(
        c.case.join("candidates.json"),
        c.root.join("actual-hardlink.json"),
    )
    .unwrap();
    assert!(!c.run().status.success());
    assert!(!c.output.join("knowledge-ir.json").exists());
}

#[test]
fn original_gold1_split_concepts_and_context_pass_without_repair() {
    let fixture = repo().join("fixtures/extraction-gateway-v1");
    let source = fs::read_to_string(fixture.join("source.txt")).unwrap();
    let gold = fs::read(fixture.join("expected.json")).unwrap();
    let response: Value =
        serde_json::from_slice(&fs::read(fixture.join("response.json")).unwrap()).unwrap();
    let c = Case::custom(&source, &gold, 0, |c, _| {
        c["payload"]["atoms"] = response["atoms"].clone();
        c["payload"]["atoms"][0]["atom"]["concept_ids"] =
            json!(["subject", "property", "temperature"]);
        c["payload"]["atoms"][0]["atom"]["content"] =
            json!("在本夹具的样例 A 中，速度为 10 m/s，条件是温度 20 ℃，且仅适用样例 A。");
        c["payload"]["atoms"][0]["measurement"]["conditions"]
            .as_array_mut()
            .unwrap()
            .push(json!("在本夹具的样例 A 中"));
        c["payload"]["atoms"][0]["atom"]["qualifiers"]
            .as_array_mut()
            .unwrap()
            .push(json!("在本夹具的样例 A 中"));
        c["payload"]["concepts"] = json!([{"concept_id":"subject","revision":1,"canonical_name":"样例 A","aliases":[],"needs_full_explanation":true},{"concept_id":"property","revision":1,"canonical_name":"速度","aliases":[],"needs_full_explanation":true},{"concept_id":"temperature","revision":1,"canonical_name":"温度","aliases":[],"needs_full_explanation":false}]);
    });
    let r = c.report(0);
    assert_eq!(r["quality"]["exception"]["expected"], 0);
    assert_eq!(r["quality"]["exception"]["status"], "not_applicable");
    assert_eq!(r["totals"]["additional_source_concepts"], 1);
    assert_eq!(fs::read(c.output.join("gold-original.json")).unwrap(), gold);
}
#[test]
fn unknown_author_value_stays_review_and_nonzero_utf8_span_passes() {
    let fixture = repo().join("fixtures/extraction-review-v1");
    let known_gold = fs::read(fixture.join("gold.json")).unwrap();
    let source = fs::read_to_string(fixture.join("source.txt")).unwrap();
    let prefix = "前言工程记录。\n\n";
    let c = Case::custom(&format!("{prefix}{source}"), &known_gold, 2, |_, _| {});
    let r = c.report(0);
    assert!(r["scope"]["units"][0]["start_byte"].as_u64().unwrap() > 0);
    let mut gold: Value = serde_json::from_slice(&known_gold).unwrap();
    gold["value_status"] = json!("unknown");
    gold["value"] = Value::Null;
    gold["unit"] = Value::Null;
    gold["atom_content"] =
        json!("样例 A 的速度未知，条件为温度 20 ℃且仅适用样例 A；例外为结冰时不适用。");
    gold["quote"] =
        json!("在本夹具的样例 A 中，速度未知。条件是温度 20 ℃、仅适用样例 A；例外为结冰时不适用。");
    let source = format!("{}\n工程未知例外。", gold["quote"].as_str().unwrap());
    let c = Case::custom(&source, &serde_json::to_vec(&gold).unwrap(), 0, |c, _| {
        c["payload"]["atoms"][0]["atom"]["content"] = gold["atom_content"].clone();
        c["payload"]["atoms"][0]["atom"]["value_status"] = json!("unknown");
        c["payload"]["atoms"][0]["atom"]["qualifiers"] =
            json!(["温度 20 ℃", "仅适用样例 A", "结冰时不适用"]);
        c["payload"]["atoms"][0]["measurement"]["value_status"] = json!("unknown");
        c["payload"]["atoms"][0]["measurement"]["value"] = Value::Null;
        c["payload"]["atoms"][0]["measurement"]["unit"] = Value::Null;
    });
    let r = c.report(3);
    assert_eq!(r["totals"]["needs_review"], 1);
    assert!(!c.output.join("knowledge-ir.json").exists());
}
#[test]
fn irrelevant_quote_and_extra_body_assertions_fail() {
    Case::new(|c,_|{let quote="这是独立作者的工程例外测试，不是科学质量基准。";let source=fs::read_to_string(repo().join("fixtures/extraction-review-v1/source.txt")).unwrap();let start=source.find(quote).unwrap();c["payload"]["evidence"][0]=json!({"evidence_id":"evidence-speed","unit_id":"unit-0","excerpt_start_byte":start,"excerpt_end_byte":start+quote.len(),"quote":quote,"excerpt_sha256":hash(quote.as_bytes())});}).rejected("VERIFY_EVIDENCE");
    Case::new(|c, _| {
        let old = c["payload"]["atoms"][0]["atom"]["content"]
            .as_str()
            .unwrap()
            .to_owned();
        c["payload"]["atoms"][0]["atom"]["content"] = json!(format!("{old}速度为 999 m/s。"));
    })
    .rejected("VERIFY_CONTENT");
}
#[test]
fn failed_primary_cannot_borrow_duplicate_quality_passes() {
    let c = Case::new(|c, _| {
        let mut good = c["payload"]["atoms"][0].clone();
        good["atom"]["atom_id"] = json!("extra-good");
        c["payload"]["atoms"][0]["measurement"]["conditions"] = json!([]);
        c["payload"]["atoms"][0]["atom"]["qualifiers"] = json!(["m/s", "结冰时不适用"]);
        c["payload"]["atoms"].as_array_mut().unwrap().push(good);
    });
    let r = c.report(3);
    assert_eq!(r["quality"]["condition"]["passed"], 0);
    assert_eq!(r["quality"]["condition"]["failed"], 2);
}
#[test]
fn malformed_candidate_preserves_actual_objects_and_source_mismatch_quality() {
    let c = Case::new(|c, _| {
        c["payload"]["atoms"][0]
            .as_object_mut()
            .unwrap()
            .remove("measurement")
            .map(|_| ())
            .unwrap()
    });
    let r = c.report(3);
    assert_eq!(r["candidate_results"][0]["object_id"], "atom-speed");
    assert_eq!(r["candidate_results"][0]["status"], "unprocessed");
    let c = Case::new(|c, _| {
        c["binding"]["source_id"] = json!("other-source");
        c["payload"]["binding"]["source_id"] = json!("other-source");
    });
    let r = c.report(3);
    assert_eq!(r["quality"]["binding"]["passed"], 0);
    assert_eq!(r["quality"]["binding"]["failed"], 1);
}
#[test]
fn strict_review_gold_decision_and_wrapper_inputs_are_rejected() {
    for file in [
        "review.json",
        "gold.json",
        "decision.json",
        "candidates.json",
    ] {
        for kind in ["unknown", "duplicate", "trailing", "missing"] {
            let c = Case::new(|_, _| {});
            let path = c.case.join(file);
            let old = fs::read_to_string(&path).unwrap();
            let mut value: Value = serde_json::from_str(&old).unwrap();
            let bad = match kind {
                "unknown" => {
                    value["extra"] = json!(1);
                    serde_json::to_string(&value).unwrap()
                }
                "duplicate" => old.replacen(
                    "\"schema_version\":",
                    "\"schema_version\":1,\"schema_version\":",
                    1,
                ),
                "trailing" => format!("{old} {{}}"),
                _ => {
                    value.as_object_mut().unwrap().remove("schema_version");
                    serde_json::to_string(&value).unwrap()
                }
            };
            fs::write(&path, &bad).unwrap();
            if file == "gold.json" {
                let p = c.case.join("review.json");
                let mut review: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
                review["gold_sha256"] = json!(hash(bad.as_bytes()));
                fs::write(p, serde_json::to_vec(&review).unwrap()).unwrap();
            }
            let run = c.run();
            assert!(!run.status.success(), "{file} {kind}");
            assert!(!c.output.join("knowledge-ir.json").exists());
        }
    }
}
#[test]
fn author_gold_cannot_use_substring_of_wrong_source_value() {
    let fixture = repo().join("fixtures/extraction-review-v1");
    let mut g: Value =
        serde_json::from_slice(&fs::read(fixture.join("gold.json")).unwrap()).unwrap();
    let quote = g["quote"].as_str().unwrap().replace("10 m/s", "110 m/s");
    g["quote"] = json!(quote);
    let c = Case::custom(
        &format!("{quote}\n工程测量。"),
        &serde_json::to_vec(&g).unwrap(),
        0,
        |_, _| {},
    );
    let run = c.run();
    assert_eq!(
        run.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    assert!(String::from_utf8_lossy(&run.stderr).contains("VERIFY_GOLD"));
    assert!(!c.output.exists());
}
#[test]
fn real_needs_review_source_remains_review() {
    let fixture = repo().join("fixtures/extraction-review-v1");
    let source = format!(
        "```\n{}",
        fs::read_to_string(fixture.join("source.txt")).unwrap()
    );
    let gold = fs::read(fixture.join("gold.json")).unwrap();
    let c = Case::custom(&source, &gold, 0, |_, _| {});
    let r = c.report(3);
    assert_eq!(r["scope"]["units"][0]["source_status"], "needs_review");
    assert_eq!(r["totals"]["needs_review"], 1);
    assert!(!c.output.join("knowledge-ir.json").exists());
}
