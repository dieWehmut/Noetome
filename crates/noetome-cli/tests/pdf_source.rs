//! B03-T03: twelve permanent groups using the actual CLI, SQLite and owned
//! snapshots. Fake adapters never count as PDF persistence acceptance.
use std::fs;
use std::path::Path;
use std::sync::atomic::Ordering;

use rusqlite::{Connection, params};
use serde_json::{Value, json};

#[path = "support/pdf_source_checks.rs"]
mod checks;
use checks::*;

fn schema1_and_schema2_migration_preserve_legacy_history() {
    let sandbox = Sandbox::new("01-migration");
    let direct = sandbox.project("direct schema1");
    let direct_config = read_json(&direct.join("project.json"));
    let pdf = sandbox.import(&direct, "N01", "direct.pdf");
    assert_eq!(pdf["project_id"], direct_config["project_id"]);
    assert_eq!(
        read_json(&direct.join("project.json"))["created_at"],
        direct_config["created_at"]
    );
    sandbox.inspect(&direct, &pdf);

    let project = sandbox.project("legacy schema2");
    let first = sandbox.root.join("legacy 中文.md");
    fs::write(&first, "# 手写标题\r\n\r\n旧 UTF8 正文。\r\n").unwrap();
    let one = json_output(sandbox.text(&project, &first, None), 0);
    fs::write(&first, "# 新版本\n\n保留第一次的身份与范围。\n").unwrap();
    let two = json_output(sandbox.text(&project, &first, None), 0);
    assert_eq!(one["source_id"], two["source_id"]);
    let before_config = read_json(&project.join("project.json"));
    assert_eq!(before_config["schema_version"], 2);
    let names = [
        "sources",
        "source_versions",
        "source_spans",
        "source_imports",
    ];
    let db = database(&project);
    let before_rows: Vec<_> = names.iter().map(|name| table(&db, name)).collect();
    drop(db);
    let cache_before = content(&project.join(".noetome-sources"));
    let imported = sandbox.import(&project, "N02", "upgrade.pdf");
    let db = database(&project);
    for (name, before) in names.iter().zip(&before_rows) {
        let after = table(&db, name);
        assert_eq!(
            &after[..before.len()],
            before,
            "migration changed ordered legacy {name} rows"
        );
    }
    drop(db);
    let cache_after = content(&project.join(".noetome-sources"));
    for (path, bytes) in cache_before {
        assert_eq!(cache_after.get(&path), Some(&bytes));
    }
    let after_config = read_json(&project.join("project.json"));
    for key in ["project_id", "created_at", "state", "database"] {
        assert_eq!(before_config[key], after_config[key]);
    }
    sandbox.inspect(&project, &one);
    sandbox.inspect(&project, &two);
    sandbox.inspect(&project, &imported);
    let text_after = json_output(sandbox.text(&project, &first, None), 0);
    assert_eq!(text_after["schema_version"], 1);
    assert_eq!(text_after["version_id"], two["version_id"]);
    assert_eq!(
        read_json(&project.join("project.json"))["schema_version"],
        3,
        "text import downgraded schema3"
    );
}

fn full_persisted_author_checks_survive_original_input_removal() {
    let sandbox = Sandbox::new("02-persisted-author");
    let project = sandbox.project("project");
    let report = sandbox.import(&project, "N02", "选择多页.pdf");
    let source = sandbox.root.join("选择多页.pdf");
    fs::remove_file(&source).unwrap();
    let db = database(&project);
    assert_eq!(count(&db, "pdf_pages"), 3);
    assert!(count(&db, "pdf_regions") >= 3 && count(&db, "pdf_artifacts") >= 4);
    drop(db);
    let report = sandbox.inspect(&project, &report);
    verify_persisted(&project, &report, Some("N02"));
    assert!(!source.exists());
}

fn repeat_and_rename_reuse_every_version_parse_and_snapshot_identity() {
    let sandbox = Sandbox::new("03-repeat-rename");
    let project = sandbox.project("project");
    let first = sandbox.import(&project, "N01", "first.pdf");
    let before = snapshot(&project.join(".noetome-pdf-parses"));
    let raw_before = snapshot(&project.join(".noetome-sources"));
    for input in ["first.pdf", "renamed source with spaces.PDF"] {
        let path = sandbox.selected("N01", input);
        // This real executable fails runtime validation on a cache miss; its
        // success here proves a ready cache hit does not launch Python.
        let repeated = json_output(sandbox.pdf(&project, &path, &different_python(), &[]), 0);
        verify_persisted(&project, &repeated, Some("N01"));
        for key in ["source_id", "version_id"] {
            assert_eq!(repeated[key], first[key]);
        }
        assert_ne!(repeated["import_id"], first["import_id"]);
        assert_eq!(
            repeated["pdf_parse"]["parse_id"],
            first["pdf_parse"]["parse_id"]
        );
        assert_eq!(repeated["raw_acquisition"]["reused"], true);
        assert_eq!(repeated["pdf_parse"]["reused"], true);
        assert_eq!(snapshot(&project.join(".noetome-pdf-parses")), before);
        assert_eq!(snapshot(&project.join(".noetome-sources")), raw_before);
        sandbox.inspect(&project, &repeated);
    }
    let db = database(&project);
    assert_eq!(count(&db, "source_versions"), 1);
    assert_eq!(count(&db, "pdf_parses"), 1);
    assert_eq!(count(&db, "pdf_imports"), 3);
}

fn revisions_return_to_old_and_explicit_source_ids_do_not_mix() {
    let sandbox = Sandbox::new("04-revisions-explicit-family");
    let project = sandbox.project("project");
    let source = sandbox.selected("N01", "book.pdf");
    let first = json_output(sandbox.pdf(&project, &source, &python(), &[]), 0);
    fs::copy(fixture("N02"), &source).unwrap();
    let second = json_output(sandbox.pdf(&project, &source, &python(), &[]), 0);
    assert_eq!(first["source_id"], second["source_id"]);
    assert_ne!(first["version_id"], second["version_id"]);
    fs::copy(fixture("N01"), &source).unwrap();
    let old = json_output(sandbox.pdf(&project, &source, &python(), &[]), 0);
    assert_eq!(old["version_id"], first["version_id"]);
    assert_eq!(old["pdf_parse"]["parse_id"], first["pdf_parse"]["parse_id"]);
    verify_persisted(&project, &old, Some("N01"));

    let other = sandbox.import(&project, "P02", "independent.pdf");
    assert_ne!(other["source_id"], first["source_id"]);
    let other_id = other["source_id"].as_str().unwrap();
    let explicit = json_output(
        sandbox.pdf(&project, &source, &python(), &["--source-id", other_id]),
        0,
    );
    assert_eq!(explicit["source_id"], other["source_id"]);
    assert_ne!(explicit["version_id"], first["version_id"]);
    assert_ne!(
        explicit["pdf_parse"]["parse_id"],
        first["pdf_parse"]["parse_id"]
    );
    verify_persisted(&project, &explicit, Some("N01"));
    let db = database(&project);
    for id in [first["source_id"].as_str().unwrap(), other_id] {
        let revisions: Vec<_> = table(&db, "source_versions")
            .iter()
            .filter(|v| v["source_id"] == id)
            .map(|v| v["revision"].as_u64().unwrap())
            .collect();
        assert_eq!(revisions, vec![1, 2]);
    }
    drop(db);
    // Different bytes in another media family are accepted under the selected source.
    let text = sandbox.root.join("handwritten.txt");
    fs::write(&text, "不同字节的文本版本。\n").unwrap();
    let text_report = json_output(sandbox.text(&project, &text, Some(other_id)), 0);
    assert_eq!(text_report["source_id"], other["source_id"]);
    assert_eq!(text_report["version"]["revision"], 3);
    sandbox.inspect(&project, &explicit);
    sandbox.inspect(&project, &text_report);

    // Identical bytes stored as text cannot be relabeled into PDF-acquired.
    let family_project = sandbox.project("family collision");
    let same_text = sandbox.root.join("same bytes.txt");
    fs::write(&same_text, b"ASCII text accepted as UTF8\n").unwrap();
    let text_first = json_output(sandbox.text(&family_project, &same_text, None), 0);
    let same_pdf = sandbox.root.join("same bytes.pdf");
    fs::copy(&same_text, &same_pdf).unwrap();
    let before = content(&family_project);
    rejected(
        sandbox.pdf(
            &family_project,
            &same_pdf,
            &python(),
            &["--source-id", text_first["source_id"].as_str().unwrap()],
        ),
        2,
        "SOURCE_MEDIA_CONFLICT",
    );
    assert_eq!(
        content(&family_project),
        before,
        "family conflict fabricated a new version"
    );
    sandbox.inspect(&family_project, &text_first);
}

fn real_policy_changes_separate_cache_then_return_to_original_ready_result() {
    let sandbox = Sandbox::new("05-policy-cache");
    let project = sandbox.project("project");
    let input = sandbox.selected("N01", "policy.pdf");
    let standard = json_output(sandbox.pdf(&project, &input, &python(), &[]), 0);
    let narrow = json_output(
        sandbox.pdf(&project, &input, &python(), &["--max-pages", "1"]),
        0,
    );
    verify_persisted(&project, &standard, Some("N01"));
    verify_persisted(&project, &narrow, Some("N01"));
    assert_eq!(standard["version_id"], narrow["version_id"]);
    assert_ne!(
        standard["pdf_parse"]["parse_id"],
        narrow["pdf_parse"]["parse_id"]
    );
    assert_eq!(narrow["pdf_parse"]["reused"], false);
    assert_eq!(
        standard["pdf_parse"]["config"]["pdf_policy"]["max_pages"],
        63
    );
    assert_eq!(narrow["pdf_parse"]["config"]["pdf_policy"]["max_pages"], 1);
    let before = snapshot(&project.join(".noetome-pdf-parses"));
    let restored = json_output(
        sandbox.pdf(
            &project,
            &input,
            &python(),
            &["--max-pages", "63", "--timeout-ms", "0"],
        ),
        0,
    );
    assert_eq!(
        restored["pdf_parse"]["parse_id"],
        standard["pdf_parse"]["parse_id"]
    );
    assert_eq!(restored["pdf_parse"]["reused"], true);
    assert_eq!(snapshot(&project.join(".noetome-pdf-parses")), before);
    let db = database(&project);
    let parses = table(&db, "pdf_parses");
    assert_eq!(parses.len(), 2);
    assert_ne!(parses[0]["cache_key"], parses[1]["cache_key"]);
    assert_eq!(parses[0]["ordinal"], 0);
    assert_eq!(parses[1]["ordinal"], 1);
}

fn actual_unparseable_and_encrypted_retries_append_immutable_failures() {
    let sandbox = Sandbox::new("06-real-failed-retry");
    let project = sandbox.project("project");
    for id in ["E03", "E04"] {
        let first = sandbox.import(&project, id, &format!("{id}.pdf"));
        assert_eq!(first["raw_acquisition"]["status"], "acquired");
        assert_eq!(first["pdf_parse"]["status"], "failed");
        let old_parse = snapshot(&project.join(format!(
            ".noetome-pdf-parses/{}",
            first["pdf_parse"]["parse_id"].as_str().unwrap()
        )));
        let input = sandbox.root.join(format!("{id}.pdf"));
        let second = json_output(sandbox.pdf(&project, &input, &python(), &[]), 3);
        verify_persisted(&project, &second, Some(id));
        assert_eq!(second["version_id"], first["version_id"]);
        assert_eq!(second["raw_acquisition"]["reused"], true);
        assert_ne!(
            second["pdf_parse"]["parse_id"],
            first["pdf_parse"]["parse_id"]
        );
        assert_eq!(second["pdf_parse"]["reused"], false);
        assert_eq!(
            snapshot(&project.join(format!(
                ".noetome-pdf-parses/{}",
                first["pdf_parse"]["parse_id"].as_str().unwrap()
            ))),
            old_parse
        );
        sandbox.inspect(&project, &first);
        sandbox.inspect(&project, &second);
        let db = database(&project);
        let parses: Vec<_> = table(&db, "pdf_parses")
            .into_iter()
            .filter(|v| v["version_id"] == first["version_id"])
            .collect();
        assert_eq!(parses.len(), 2);
        assert_eq!(parses[0]["ordinal"], 0);
        assert_eq!(parses[1]["ordinal"], 1);
        assert_eq!(parses[0]["cache_key"], parses[1]["cache_key"]);
    }
}

fn actual_runtime_failure_keeps_raw_then_recovers_with_the_pinned_python() {
    let sandbox = Sandbox::new("07-runtime-retry");
    let project = sandbox.project("project");
    let input = sandbox.selected("N01", "retry.pdf");
    let failed = json_output(sandbox.pdf(&project, &input, &different_python(), &[]), 2);
    verify_persisted(&project, &failed, None);
    assert_eq!(failed["raw_acquisition"]["status"], "acquired");
    assert_eq!(failed["pdf_parse"]["status"], "failed");
    let host = &failed["pdf_parse"]["evidence"]["host_report"];
    assert!(host["response"].is_null());
    assert_eq!(
        host["native_exit_code"], 2,
        "actual alternate Python must fail its runtime check with native2"
    );
    assert!(host["pid"].as_u64().is_some());
    let first_tree = snapshot(&project.join(format!(
        ".noetome-pdf-parses/{}",
        failed["pdf_parse"]["parse_id"].as_str().unwrap()
    )));
    let ready = json_output(sandbox.pdf(&project, &input, &python(), &[]), 0);
    verify_persisted(&project, &ready, Some("N01"));
    assert_eq!(ready["version_id"], failed["version_id"]);
    assert_ne!(
        ready["pdf_parse"]["parse_id"],
        failed["pdf_parse"]["parse_id"]
    );
    assert_eq!(
        snapshot(&project.join(format!(
            ".noetome-pdf-parses/{}",
            failed["pdf_parse"]["parse_id"].as_str().unwrap()
        ))),
        first_tree
    );
    sandbox.inspect(&project, &failed);
    sandbox.inspect(&project, &ready);
}

fn incomplete_acquisition_has_no_fabricated_version_parse_pages_or_resources() {
    let sandbox = Sandbox::new("08-incomplete-acquisition");
    let project = sandbox.project("project");
    let missing = sandbox.root.join("not yet present.pdf");
    let failed = json_output(sandbox.pdf(&project, &missing, &python(), &[]), 3);
    verify_persisted(&project, &failed, None);
    assert!(failed["source_id"].is_null() && failed["version_id"].is_null());
    fs::copy(fixture("N01"), &missing).unwrap();
    let ready = json_output(sandbox.pdf(&project, &missing, &python(), &[]), 0);
    verify_persisted(&project, &ready, Some("N01"));
    sandbox.inspect(&project, &failed);
    let large = sandbox.root.join("bounded 65MiB.pdf");
    let large_file = fs::File::create(&large).unwrap();
    large_file.set_len(65 * 1024 * 1024).unwrap();
    drop(large_file);
    let too_big = json_output(sandbox.pdf(&project, &large, &python(), &[]), 3);
    verify_persisted(&project, &too_big, None);
    assert!(too_big["version_id"].is_null() && too_big["pdf_parse"]["parse_id"].is_null());
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let locked = sandbox.selected("N02", "locked input.pdf");
        let guard = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&locked)
            .unwrap();
        let lock_failure = json_output(sandbox.pdf(&project, &locked, &python(), &[]), 3);
        verify_persisted(&project, &lock_failure, None);
        assert!(lock_failure["version_id"].is_null());
        drop(guard);
        assert_eq!(
            sha(&fs::read(&locked).unwrap()),
            case("N02")["raw"]["sha256"].as_str().unwrap()
        );
        let recovered = json_output(sandbox.pdf(&project, &locked, &python(), &[]), 0);
        verify_persisted(&project, &recovered, Some("N02"));
        sandbox.inspect(&project, &lock_failure);
    }
    // Missing explicit Python and invalid policy are argument errors before reads/writes.
    let before = content(&project);
    let no_python = sandbox.run(&[
        "source".into(),
        "import".into(),
        project.as_os_str().to_owned(),
        missing.as_os_str().to_owned(),
    ]);
    assert_eq!(no_python.status.code(), Some(2));
    assert!(no_python.stdout.is_empty());
    assert_eq!(content(&project), before);
    let invalid = sandbox.pdf(&project, &missing, &python(), &["--max-pages", "64"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    assert_eq!(content(&project), before);
}

fn every_review_page_glyph_geometry_and_partial_failure_is_persisted() {
    let sandbox = Sandbox::new("09-review-geometries");
    let project = sandbox.project("project");
    for id in [
        "E01", "E02", "C01", "C02", "C03", "C04", "P01", "P02", "P03", "P04", "F01",
    ] {
        let report = sandbox.import(&project, id, &format!("{id}.pdf"));
        sandbox.inspect(&project, &report);
        if case(id)["expected_label"] == "needs_review" {
            let prior = snapshot(&project.join(format!(
                ".noetome-pdf-parses/{}",
                report["pdf_parse"]["parse_id"].as_str().unwrap()
            )));
            let again = json_output(
                sandbox.pdf(
                    &project,
                    &sandbox.root.join(format!("{id}.pdf")),
                    &python(),
                    &[],
                ),
                3,
            );
            verify_persisted(&project, &again, Some(id));
            assert_eq!(again["version_id"], report["version_id"]);
            assert_ne!(
                again["pdf_parse"]["parse_id"],
                report["pdf_parse"]["parse_id"]
            );
            assert_eq!(again["pdf_parse"]["reused"], false);
            assert_eq!(
                snapshot(&project.join(format!(
                    ".noetome-pdf-parses/{}",
                    report["pdf_parse"]["parse_id"].as_str().unwrap()
                ))),
                prior
            );
        }
        if id == "F01" {
            let db = database(&project);
            let pages: Vec<_> = table(&db, "pdf_pages")
                .into_iter()
                .filter(|v| v["parse_id"] == report["pdf_parse"]["parse_id"])
                .collect();
            assert_eq!(pages.len(), 3);
            assert_eq!(pages[1]["status"], "failed");
            assert!(pages[1]["text_artifact_id"].is_null());
        }
    }
}

fn rewrite_evidence(project: &Path, report: &Value, mutate: impl FnOnce(&mut Value, &Connection)) {
    let parse_id = report["pdf_parse"]["parse_id"].as_str().unwrap();
    let path = project.join(report["pdf_parse"]["report_path"].as_str().unwrap());
    let mut evidence = read_json(&path);
    let db = Connection::open(project.join("project.sqlite")).unwrap();
    mutate(&mut evidence, &db);
    write_json(&path, &evidence);
    let bytes = fs::read(&path).unwrap();
    db.execute(
        "UPDATE pdf_parses SET report_sha256=?1,report_byte_length=?2 WHERE parse_id=?3",
        params![sha(&bytes), bytes.len() as i64, parse_id],
    )
    .unwrap();
}
fn synchronized_tampering_still_rejects_semantic_corruption_without_writes() {
    let sandbox = Sandbox::new("10-synchronized-tampering");
    let baseline = sandbox.project("baseline");
    let report = sandbox.import(&baseline, "N02", "input.pdf");
    let parse_id = report["pdf_parse"]["parse_id"].as_str().unwrap();
    for name in [
        "raw",
        "text",
        "bbox",
        "middle-page",
        "unknown-json",
        "escaped-path",
        "native-exit-null",
        "pid-null",
    ] {
        let project = sandbox.root.join(format!("mutation-{name}"));
        copy_tree(&baseline, &project);
        match name {
            "raw" => {
                let path = project.join(
                    report["raw_acquisition"]["version"]["raw_snapshot"]
                        .as_str()
                        .unwrap(),
                );
                let mut bytes = fs::read(&path).unwrap();
                bytes[0] ^= 1;
                fs::write(path, bytes).unwrap();
            }
            "text" => rewrite_evidence(&project, &report, |evidence, db| {
                let response = &mut evidence["host_report"]["response"];
                let artifact_id = response["pages"][0]["text_artifact"]["artifact_id"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                let artifact = response["artifacts"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|v| v["artifact_id"] == artifact_id)
                    .unwrap();
                let path = project
                    .join(format!(".noetome-pdf-parses/{parse_id}"))
                    .join(artifact["relative_path"].as_str().unwrap());
                let mut bytes = fs::read(&path).unwrap();
                let at = bytes.iter().position(u8::is_ascii_alphabetic).unwrap();
                bytes[at] = if bytes[at] == b'X' { b'Y' } else { b'X' };
                fs::write(&path, &bytes).unwrap();
                let hash = sha(&bytes);
                artifact["sha256"] = json!(hash);
                for page in response["pages"].as_array_mut().unwrap() {
                    if page["text_artifact"]["artifact_id"] == artifact_id {
                        page["text_artifact"]["sha256"] = json!(hash);
                    }
                    for region in page["regions"].as_array_mut().unwrap() {
                        if region["content"]["artifact_id"] == artifact_id {
                            region["content"]["sha256"] = json!(hash);
                        }
                    }
                }
                db.execute(
                    "UPDATE pdf_artifacts SET sha256=?1 WHERE parse_id=?2 AND artifact_id=?3",
                    params![hash, parse_id, artifact_id],
                )
                .unwrap();
                db.execute("UPDATE pdf_pages SET text_artifact_sha256=?1 WHERE parse_id=?2 AND text_artifact_id=?3",params![hash,parse_id,artifact_id]).unwrap();
                db.execute(
                    "UPDATE pdf_regions SET sha256=?1 WHERE parse_id=?2 AND artifact_id=?3",
                    params![hash, parse_id, artifact_id],
                )
                .unwrap();
            }),
            "bbox" => rewrite_evidence(&project, &report, |evidence, db| {
                let region = &mut evidence["host_report"]["response"]["pages"][0]["regions"][0];
                let region_id = region["region_id"].as_str().unwrap().to_owned();
                for coordinate in region["locator"]["bbox_pt"].as_array_mut().unwrap() {
                    *coordinate = json!(coordinate.as_f64().unwrap() + 1.0);
                }
                db.execute(
                    "UPDATE pdf_regions SET bbox_json=?1 WHERE parse_id=?2 AND region_id=?3",
                    params![
                        serde_json::to_string(&region["locator"]["bbox_pt"]).unwrap(),
                        parse_id,
                        region_id
                    ],
                )
                .unwrap();
            }),
            "middle-page" => rewrite_evidence(&project, &report, |evidence, db| {
                evidence["host_report"]["response"]["pages"]
                    .as_array_mut()
                    .unwrap()
                    .remove(1);
                db.execute(
                    "DELETE FROM pdf_regions WHERE parse_id=?1 AND physical_page_index=1",
                    [parse_id],
                )
                .unwrap();
                db.execute(
                    "DELETE FROM pdf_pages WHERE parse_id=?1 AND physical_page_index=1",
                    [parse_id],
                )
                .unwrap();
            }),
            "unknown-json" => rewrite_evidence(&project, &report, |evidence, _| {
                evidence["config"]["unrecognized"] = json!(true);
            }),
            "escaped-path" => rewrite_evidence(&project, &report, |evidence, db| {
                let artifact = &mut evidence["host_report"]["response"]["artifacts"][0];
                let id = artifact["artifact_id"].as_str().unwrap().to_owned();
                artifact["relative_path"] = json!("../../人工外部哨兵.md");
                db.execute("UPDATE pdf_artifacts SET relative_path=?1 WHERE parse_id=?2 AND artifact_id=?3",params!["../../人工外部哨兵.md",parse_id,id]).unwrap();
            }),
            "native-exit-null" => rewrite_evidence(&project, &report, |evidence, _| {
                evidence["host_report"]["native_exit_code"] = Value::Null;
            }),
            "pid-null" => rewrite_evidence(&project, &report, |evidence, _| {
                evidence["host_report"]["pid"] = Value::Null;
            }),
            _ => unreachable!(),
        }
        sandbox.rejected_inspect(&project, &report);
    }
    sandbox.inspect(&baseline, &report);
    verify_persisted(&baseline, &report, Some("N02"));
}

fn strict_schema_unknown_cache_and_windows_aliases_keep_evidence() {
    let sandbox = Sandbox::new("11-strict-schema-cache-windows");
    let baseline = sandbox.project("baseline");
    let report = sandbox.import(&baseline, "N01", "input.pdf");
    for name in [
        "unknown-table",
        "unknown-index",
        "paired-version",
        "unknown-cache",
        "unknown-marker",
        "owner-field",
        "unsupported-options",
        "report-too-large",
    ] {
        let project = sandbox.root.join(name);
        copy_tree(&baseline, &project);
        match name {
            "unknown-table" => {
                Connection::open(project.join("project.sqlite"))
                    .unwrap()
                    .execute_batch("CREATE TABLE unrecognized(value TEXT);")
                    .unwrap();
            }
            "unknown-index" => {
                Connection::open(project.join("project.sqlite"))
                    .unwrap()
                    .execute_batch("CREATE INDEX unrecognized_index ON pdf_pages(status);")
                    .unwrap();
            }
            "paired-version" => {
                let mut config = read_json(&project.join("project.json"));
                config["schema_version"] = json!(4);
                write_json(&project.join("project.json"), &config);
            }
            "unknown-cache" => {
                fs::write(
                    project.join(".noetome-pdf-parses/unrecognized.bin"),
                    b"retain unknown cache bytes",
                )
                .unwrap();
            }
            "unknown-marker" => {
                fs::create_dir(project.join(".noetome-writing")).unwrap();
                fs::write(
                    project.join(".noetome-writing/unknown.bin"),
                    b"retain interrupted/unknown owner",
                )
                .unwrap();
            }
            "owner-field" => {
                let path = project.join(".noetome-pdf-parses/.owner.json");
                let mut owner = read_json(&path);
                owner["unknown"] = json!(true);
                write_json(&path, &owner);
            }
            "unsupported-options" => rewrite_evidence(&project, &report, |evidence, db| {
                evidence["config"]["options_version"] = json!("unimplemented-options-v99");
                db.execute(
                    "UPDATE pdf_parses SET config_json=?1 WHERE parse_id=?2",
                    params![
                        serde_json::to_string(&evidence["config"]).unwrap(),
                        report["pdf_parse"]["parse_id"].as_str().unwrap()
                    ],
                )
                .unwrap();
            }),
            "report-too-large" => {
                let path = project.join(report["pdf_parse"]["report_path"].as_str().unwrap());
                let mut bytes = fs::read(&path).unwrap();
                bytes.resize(16 * 1024 * 1024 + 1, b' ');
                fs::write(&path, &bytes).unwrap();
                Connection::open(project.join("project.sqlite"))
                    .unwrap()
                    .execute(
                        "UPDATE pdf_parses SET report_sha256=?1,report_byte_length=?2",
                        params![sha(&bytes), bytes.len() as i64],
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        sandbox.rejected_inspect(&project, &report);
    }
    #[cfg(windows)]
    {
        for (name, relative) in [
            (
                "raw-hardlink",
                report["raw_acquisition"]["version"]["raw_snapshot"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            ),
            (
                "artifact-hardlink",
                format!(
                        ".noetome-pdf-parses/{}/{}",
                        report["pdf_parse"]["parse_id"].as_str().unwrap(),
                        report["pdf_parse"]["evidence"]["host_report"]["response"]["artifacts"][0]
                            ["relative_path"]
                            .as_str()
                            .unwrap()
                    ),
            ),
        ] {
            let project = sandbox.root.join(name);
            copy_tree(&baseline, &project);
            let alias = sandbox.root.join(format!("{name}-external alias.bin"));
            fs::hard_link(project.join(relative), &alias).unwrap();
            sandbox.rejected_inspect(&project, &report);
        }
        let project = sandbox.root.join("cache-junction");
        copy_tree(&baseline, &project);
        let cache = project.join(".noetome-pdf-parses");
        let retained = project.join("retained-cache");
        fs::rename(&cache, &retained).unwrap();
        let outside = sandbox.root.join("outside cache target");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("human.bin"), b"keep outside target intact").unwrap();
        let dos = |path: &Path| {
            path.to_string_lossy()
                .strip_prefix(r"\\?\")
                .unwrap_or(&path.to_string_lossy())
                .to_owned()
        };
        let output = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(dos(&cache))
            .arg(dos(&outside))
            .output()
            .unwrap();
        assert!(output.status.success());
        sandbox.rejected_inspect(&project, &report);
        fs::remove_dir(&cache).unwrap();
        assert_eq!(
            fs::read(outside.join("human.bin")).unwrap(),
            b"keep outside target intact"
        );
    }
    sandbox.inspect(&baseline, &report);
}

fn actual_windows_replace_denial_rolls_back_published_raw_and_parse() {
    let sandbox = Sandbox::new("12-real-write-failure");
    let project = sandbox.project("project");
    let first = sandbox.import(&project, "N01", "first.pdf");
    let second = sandbox.selected("N02", "second.pdf");
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let guard = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(project.join("project.sqlite"))
            .unwrap();
        let before = content(&sandbox.root);
        let output = sandbox.pdf(&project, &second, &python(), &[]);
        assert_eq!(output.status.code(), Some(3));
        assert!(output.stdout.is_empty() && !output.stderr.is_empty());
        drop(guard);
        assert_eq!(
            content(&sandbox.root),
            before,
            "known failure changed old bytes or left owned partially published snapshots"
        );
        sandbox.inspect(&project, &first);
        verify_persisted(&project, &first, Some("N01"));
        let retry = json_output(sandbox.pdf(&project, &second, &python(), &[]), 0);
        verify_persisted(&project, &retry, Some("N02"));
    }
    #[cfg(not(windows))]
    {
        let _ = (first, second);
        panic!("this acceptance group requires actual Windows deny-delete semantics");
    }
}

fn main() {
    let tests: [(&str, fn()); 12] = [
        (
            "schema1_and_schema2_migration_preserve_legacy_history",
            schema1_and_schema2_migration_preserve_legacy_history,
        ),
        (
            "full_persisted_author_checks_survive_original_input_removal",
            full_persisted_author_checks_survive_original_input_removal,
        ),
        (
            "repeat_and_rename_reuse_every_version_parse_and_snapshot_identity",
            repeat_and_rename_reuse_every_version_parse_and_snapshot_identity,
        ),
        (
            "revisions_return_to_old_and_explicit_source_ids_do_not_mix",
            revisions_return_to_old_and_explicit_source_ids_do_not_mix,
        ),
        (
            "real_policy_changes_separate_cache_then_return_to_original_ready_result",
            real_policy_changes_separate_cache_then_return_to_original_ready_result,
        ),
        (
            "actual_unparseable_and_encrypted_retries_append_immutable_failures",
            actual_unparseable_and_encrypted_retries_append_immutable_failures,
        ),
        (
            "actual_runtime_failure_keeps_raw_then_recovers_with_the_pinned_python",
            actual_runtime_failure_keeps_raw_then_recovers_with_the_pinned_python,
        ),
        (
            "incomplete_acquisition_has_no_fabricated_version_parse_pages_or_resources",
            incomplete_acquisition_has_no_fabricated_version_parse_pages_or_resources,
        ),
        (
            "every_review_page_glyph_geometry_and_partial_failure_is_persisted",
            every_review_page_glyph_geometry_and_partial_failure_is_persisted,
        ),
        (
            "synchronized_tampering_still_rejects_semantic_corruption_without_writes",
            synchronized_tampering_still_rejects_semantic_corruption_without_writes,
        ),
        (
            "strict_schema_unknown_cache_and_windows_aliases_keep_evidence",
            strict_schema_unknown_cache_and_windows_aliases_keep_evidence,
        ),
        (
            "actual_windows_replace_denial_rolls_back_published_raw_and_parse",
            actual_windows_replace_denial_rolls_back_published_raw_and_parse,
        ),
    ];
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|v| v == "--list") {
        for (name, _) in tests {
            println!("{name}: test");
        }
        return;
    }
    python();
    let filter = args
        .windows(2)
        .find(|v| v[0] == "--exact")
        .map(|v| v[1].as_str());
    let mut passed = 0;
    let mut failed = 0;
    let mut results = Vec::new();
    for (name, test) in tests {
        if filter.is_some_and(|v| v != name) {
            continue;
        }
        print!("test {name} ... ");
        let started = std::time::Instant::now();
        if std::panic::catch_unwind(test).is_ok() {
            passed += 1;
            println!("ok");
            results.push(
                json!({"name":name,"passed":true,"elapsed_ms":started.elapsed().as_millis()}),
            );
        } else {
            failed += 1;
            println!("FAILED");
            results.push(
                json!({"name":name,"passed":false,"elapsed_ms":started.elapsed().as_millis()}),
            );
        }
    }
    println!(
        "actual source CLI calls: {}",
        CLI_CALLS.load(Ordering::Relaxed)
    );
    println!("test result: {passed} passed; {failed} failed");
    let summary = repo()
        .join(".artifacts/B03-T03-tests")
        .join(format!("run-{}.json", uuid::Uuid::new_v4()));
    fs::create_dir_all(summary.parent().unwrap()).unwrap();
    write_json(
        &summary,
        &json!({"schema_version":1,"executable":env!("CARGO_BIN_EXE_noetome"),"executable_sha256":sha(&fs::read(env!("CARGO_BIN_EXE_noetome")).unwrap()),"python":python(),"fixture_manifest_sha256":sha(&fs::read(repo().join("fixtures/pdf-text-v1/expected.json")).unwrap()),"cli_calls":CLI_CALLS.load(Ordering::Relaxed),"passed":passed,"failed":failed,"groups":results}),
    );
    println!("run summary: {}", summary.display());
    if passed + failed == 0 || failed > 0 {
        std::process::exit(1);
    }
}
