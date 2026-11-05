use serde_json::{Value, json};
use std::fs;
#[path = "support/process_case.rs"]
mod support;
use support::*;

#[test]
fn command_parsing_keeps_os_paths_and_requires_exact_new_entrypoint_arity() {
    use noetome_cli::{CliCommand, parse};
    let args = |words: &[&str]| {
        words
            .iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>()
    };
    assert!(matches!(
        parse(&args(&[
            "knowledge",
            "process-review",
            "--",
            "-项目",
            "中文 case",
            "新 review"
        ])),
        Ok(CliCommand::ProcessReview { .. })
    ));
    assert!(matches!(
        parse(&args(&[
            "knowledge",
            "flow",
            "项目",
            "case",
            "review",
            "profile.json",
            "Vault"
        ])),
        Ok(CliCommand::ProcessFlow { .. })
    ));
    for words in [
        vec!["knowledge", "process-review", "-项目", "case", "new"],
        vec!["knowledge", "flow", "项目", "case", "review", "Vault"],
        vec!["knowledge", "process-review", "p", "c", "o", "extra"],
    ] {
        assert!(parse(&args(&words)).is_err());
    }
    for verb in ["flow", "process-review"] {
        assert!(matches!(
            parse(&args(&["knowledge", verb, "--help"])),
            Ok(CliCommand::KnowledgeHelp)
        ));
    }
}

fn assert_error(result: std::process::Output, code: &str) {
    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.starts_with(code), "expected {code}, got {stderr}");
}
fn canonical_array(value: &Value, key: &str) -> Vec<Value> {
    let mut values = value.as_array().unwrap().clone();
    values.sort_by(|a, b| a[key].as_str().cmp(&b[key].as_str()));
    values
}
#[test]
fn ordinary_cli_covers_all_frozen_sources_graphs_fallbacks_and_semantic_blocks() {
    let authors = read(repo().join("fixtures/process-flow-v1/authors.json"));
    for author in authors["cases"].as_array().unwrap() {
        let name = author["name"].as_str().unwrap();
        let case = ProcessCase::prepare(name);
        let before = case.protected();
        let review = case.review();
        let expected_review = case.expected["review_status"] == "passed";
        assert_eq!(
            review.status.code(),
            Some(if expected_review { 0 } else { 3 }),
            "{name}: {}",
            String::from_utf8_lossy(&review.stderr)
        );
        let reported = read(case.review.join("review.json"));
        assert_eq!(reported["status"], case.expected["review_status"]);
        assert_eq!(
            case.review.join("process-ir.json").exists(),
            expected_review
        );
        assert_snapshot(&before);
        let before = case.protected();
        let flow = case.flow();
        let success = case.expected["flow_status"] == "passed";
        assert_eq!(
            flow.status.code(),
            Some(if success { 0 } else { 3 }),
            "{name}: {}",
            String::from_utf8_lossy(&flow.stderr)
        );
        let report = read(case.output.join("flow.json"));
        assert_eq!(report["published"], false);
        assert_eq!(report["knowledge_status"], "pending");
        assert_eq!(report["scientific_quality"], "not_run");
        assert_eq!(report["representation"], case.expected["representation"]);
        assert_eq!(case.output.join("plan.json").exists(), success);
        if !expected_review {
            assert_eq!(report["checks"]["model_support"], "not_run");
            assert_eq!(report["issue_codes"], json!(["FLOW_SOURCE_UNVERIFIED"]));
        } else {
            assert_eq!(report["issue_codes"], case.expected["issues"], "{name}");
            assert_eq!(
                report["process_document"]["process"],
                case.expected["process"]
            );
            assert_eq!(
                canonical_array(&report["process_document"]["steps"], "id"),
                canonical_array(&case.expected["nodes"], "id")
            );
            assert_eq!(
                canonical_array(&report["process_document"]["relations"], "id"),
                canonical_array(&case.expected["edges"], "id")
            );
            for (key, total) in case.expected["totals"].as_object().unwrap() {
                if !["parsed_statements", "unparsed_statements"].contains(&key.as_str()) {
                    assert_eq!(report["totals"][key], *total, "{name}/{key}");
                }
            }
        }
        if success {
            let plan = read(case.output.join("plan.json"));
            assert_eq!(plan["document"], report["process_document"]);
            assert_eq!(report["files"].as_array().unwrap().len(), 2);
            for file in report["files"].as_array().unwrap() {
                let bytes =
                    fs::read(case.output.join(file["relative_path"].as_str().unwrap())).unwrap();
                assert_eq!(hash(&bytes), file["content_sha256"]);
            }
            let overview = fs::read_to_string(case.output.join("流程总览.md")).unwrap();
            assert_eq!(
                overview.contains("```mermaid"),
                case.expected["representation"] == "flowchart"
            );
        } else {
            assert_eq!(report["files"], json!([]));
        }
        assert_snapshot(&before);
    }
}
#[test]
fn changed_saved_review_and_synchronized_gold_are_rebuilt_against_actual_source() {
    for scenario in [
        "ir",
        "report",
        "markdown",
        "gold-and-pin",
        "extra-file",
        "missing-file",
    ] {
        let case = ProcessCase::ready("linear");
        match scenario {
            "ir" => {
                let p = case.review.join("process-ir.json");
                let mut v = read(&p);
                v["document"]["relations"][0]["left"] = json!("s2");
                write(p, &v);
            }
            "report" => {
                let p = case.review.join("review.json");
                let mut v = read(&p);
                v["network_calls"] = json!(1);
                write(p, &v);
            }
            "markdown" => {
                let p = case.review.join("review.md");
                fs::write(
                    &p,
                    format!("{}\nhuman edit", fs::read_to_string(&p).unwrap()),
                )
                .unwrap();
            }
            "gold-and-pin" => {
                let p = case.case.join("gold.json");
                let mut v = read(&p);
                v["steps"][0]["outputs"] = json!(["invented-output"]);
                write(&p, &v);
                let p = case.case.join("process-review.json");
                let mut v = read(&p);
                v["gold_sha256"] = json!(hash(&fs::read(case.case.join("gold.json")).unwrap()));
                write(p, &v);
            }
            "extra-file" => fs::write(case.review.join("human.txt"), "preserve").unwrap(),
            _ => fs::remove_file(case.review.join("process-ir.json")).unwrap(),
        }
        let before = case.protected();
        let result = case.flow();
        assert!(!result.status.success(), "{scenario}");
        assert!(!case.output.exists());
        assert_snapshot(&before);
    }
}
#[test]
fn strict_case_gold_and_profile_fields_cannot_be_relaxed() {
    for scenario in [
        "case-unknown",
        "gold-null-missing",
        "gold-schema",
        "profile-unknown",
        "profile-encoding",
    ] {
        let case = ProcessCase::prepare("linear");
        let (path, key, value) = match scenario {
            "case-unknown" => (case.case.join("process-review.json"), "extra", json!(true)),
            "gold-schema" => (case.case.join("gold.json"), "schema_version", json!(2)),
            "profile-unknown" => (case.profile.clone(), "extra", json!(1)),
            "profile-encoding" => (case.profile.clone(), "label_encoding", json!("raw_mermaid")),
            _ => (case.case.join("gold.json"), "", Value::Null),
        };
        if path == case.profile {
            assert!(case.review().status.success());
        }
        let mut v = read(&path);
        if key.is_empty() {
            v["steps"][0].as_object_mut().unwrap().remove("criterion");
        } else {
            v[key] = value;
        }
        write(&path, &v);
        if path.ends_with("gold.json") {
            let p = case.case.join("process-review.json");
            let mut m = read(&p);
            m["gold_sha256"] = json!(hash(&fs::read(&path).unwrap()));
            write(p, &m);
        }
        let before = case.protected();
        let result = if path == case.profile {
            case.flow()
        } else {
            case.review()
        };
        assert!(!result.status.success(), "{scenario}");
        assert!(!case.output.exists());
        assert_snapshot(&before);
    }
}
#[test]
fn new_roots_collisions_and_profile_reuse_are_checked_without_writes() {
    let case = ProcessCase::ready("linear");
    let before = case.protected();
    assert_error(
        case.flow_to(&case.project.join("nested")),
        "EXTRACT_OUTPUT_OVERLAP",
    );
    assert_error(
        case.flow_to(&case.case.join("nested")),
        "EXTRACT_OUTPUT_OVERLAP",
    );
    assert_error(
        case.flow_to(&case.review.join("nested")),
        "EXTRACT_OUTPUT_OVERLAP",
    );
    fs::create_dir(&case.output).unwrap();
    fs::write(case.output.join("human.txt"), "human sentinel").unwrap();
    assert_error(case.flow(), "EXTRACT_OUTPUT_EXISTS");
    assert_eq!(
        fs::read_to_string(case.output.join("human.txt")).unwrap(),
        "human sentinel"
    );
    assert_snapshot(&before);
}
#[test]
fn hardlinked_profile_and_linked_case_ancestors_are_rejected() {
    let case = ProcessCase::ready("linear");
    let before = case.protected();
    fs::hard_link(&case.profile, case.root.join("profile-alias.json")).unwrap();
    assert_error(case.flow(), "PROCESS_PROFILE_PATH");
    assert!(!case.output.exists());
    assert_snapshot(&before);
    #[cfg(unix)]
    {
        fs::remove_file(case.root.join("profile-alias.json")).unwrap();
        let linked = case.root.join("linked-case");
        std::os::unix::fs::symlink(&case.case, &linked).unwrap();
        let result = case.command(
            "linked-case",
            &[
                "knowledge",
                "flow",
                case.project.to_str().unwrap(),
                linked.to_str().unwrap(),
                case.review.to_str().unwrap(),
                case.profile.to_str().unwrap(),
                case.output.to_str().unwrap(),
            ],
        );
        assert_error(result, "PROCESS_INPUT");
        assert!(!case.output.exists());
        assert_snapshot(&before);
    }
}
