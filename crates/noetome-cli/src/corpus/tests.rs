use super::*;
struct TestProject(std::path::PathBuf);
impl TestProject {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("noetome-corpus-test-{}", Uuid::new_v4()));
        project::initialize(&path).unwrap();
        Self(path)
    }
    fn source(&self, count: u32) -> (Repository, String) {
        let repo = Repository::create(&self.0).unwrap();
        let id = Uuid::new_v4().to_string();
        fs::create_dir(repo.source_directory(&id).unwrap()).unwrap();
        repo.insert_source(&id, Path::new("synthetic-test.pdf"), 12)
            .unwrap();
        repo.acquired(&id, &"a".repeat(64), 12).unwrap();
        repo.register_pages(&id, &json!({"status":"ready","page_count":count}))
            .unwrap();
        (repo, id)
    }
}
impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn page(n: u32, state: &str) -> Value {
    json!({"schema_version":1,"page_number":n,"status":state,"text":"Authored test text","regions":[{"ordinal":0,"kind":"text","text":"Authored test text","bbox":[10,20,100,40]}],"width_pt":200,"height_pt":300,"rotation":0,"geometry":{},"issues":[]})
}

#[test]
fn empty_list_does_not_create_a_repository() {
    let p = TestProject::new();
    assert!(list(&p.0).unwrap().is_empty());
    assert!(!p.0.join(".noetome-corpus").exists());
}
#[test]
fn durable_pages_are_paginated_and_holes_are_resumable() {
    let p = TestProject::new();
    let (repo, id) = p.source(3648);
    repo.commit_page(&id, page(1, "text_ready")).unwrap();
    repo.commit_page(&id, page(3, "needs_review")).unwrap();
    assert_eq!(repo.next_batch(&id, 8).unwrap(), Some((2, 2)));
    repo.commit_page(&id, page(2, "text_ready")).unwrap();
    assert_eq!(repo.next_batch(&id, 8).unwrap(), Some((4, 11)));
    drop(repo);
    let value = pages(&p.0, &id, 2, 2).unwrap();
    assert_eq!(value["total"], 3648);
    assert_eq!(value["pages"][0]["page_number"], 3);
    assert_eq!(value["pages"][1]["status"], "pending");
    let summary = status(&p.0, &id).unwrap();
    assert_eq!(summary.processed_pages, 3);
    assert_eq!(summary.review_pages, 1);
}
#[test]
fn process_lease_is_exclusive_and_recovers_with_stale_file() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    let lease = repo.lease(&id).unwrap();
    assert_eq!(repo.lease(&id).unwrap_err().code(), "CORPUS_BUSY");
    drop(lease);
    assert!(
        repo.source_directory(&id)
            .unwrap()
            .join("worker.lock")
            .exists()
    );
    assert!(repo.lease(&id).is_ok());
}
#[test]
fn pause_and_review_are_persistent_and_not_false_ready() {
    let p = TestProject::new();
    let (repo, id) = p.source(2);
    repo.start(&id).unwrap();
    pause(&p.0, &id).unwrap();
    assert!(repo.paused(&id).unwrap());
    repo.commit_page(&id, page(1, "text_ready")).unwrap();
    repo.finish(&id).unwrap();
    assert_eq!(status(&p.0, &id).unwrap().status, "paused");
    repo.start(&id).unwrap();
    assert!(!repo.paused(&id).unwrap());
    repo.commit_page(&id, page(2, "needs_ocr")).unwrap();
    repo.finish(&id).unwrap();
    assert_eq!(status(&p.0, &id).unwrap().status, "needs_review");
}
#[test]
fn invalid_geometry_and_page_identity_never_commit() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    let mut bad = page(1, "text_ready");
    bad["regions"][0]["bbox"][2] = json!(900);
    assert_eq!(
        repo.commit_page(&id, bad).unwrap_err().code(),
        "CORPUS_PAGE_INVALID"
    );
    assert_eq!(
        repo.commit_page(&id, page(2, "text_ready"))
            .unwrap_err()
            .code(),
        "CORPUS_PAGE_CONFLICT"
    );
    assert_eq!(status(&p.0, &id).unwrap().processed_pages, 0);
    repo.commit_page(&id, page(1, "text_ready")).unwrap();
    assert_eq!(
        repo.commit_page(&id, page(1, "text_ready"))
            .unwrap_err()
            .code(),
        "CORPUS_PAGE_CONFLICT"
    );
}
#[test]
fn changing_saved_page_is_detected_on_read() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    repo.commit_page(&id, page(1, "text_ready")).unwrap();
    drop(repo);
    let db = rusqlite::Connection::open(p.0.join(".noetome-corpus/corpus.sqlite")).unwrap();
    db.execute("UPDATE pages SET page_json='{}'", []).unwrap();
    drop(db);
    assert_eq!(
        pages(&p.0, &id, 0, 1).unwrap_err().code(),
        "CORPUS_PAGE_CHANGED"
    );
}
#[test]
fn foreign_project_database_is_rejected() {
    let p = TestProject::new();
    let (repo, _) = p.source(1);
    drop(repo);
    let db = rusqlite::Connection::open(p.0.join(".noetome-corpus/corpus.sqlite")).unwrap();
    db.execute("UPDATE metadata SET project_id='foreign'", [])
        .unwrap();
    drop(db);
    assert_eq!(list(&p.0).unwrap_err().code(), "CORPUS_SCHEMA");
}

#[test]
fn ocr_revision_retains_original_text_and_distinct_region_identity() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    repo.commit_page(&id, page(1, "needs_ocr")).unwrap();
    let old = pages(&p.0, &id, 0, 1).unwrap()["pages"][0].clone();
    let mut recognized = page(1, "ocr_ready");
    recognized["text"] = json!("Recognized authored test");
    recognized["regions"][0]["text"] = recognized["text"].clone();
    recognized["parser"] = json!({"engine":"mineru","engine_version":"4.2.1"});
    repo.commit_ocr_page(&id, recognized).unwrap();
    let current = pages(&p.0, &id, 0, 1).unwrap()["pages"][0].clone();
    assert_ne!(
        old["regions"][0]["region_id"],
        current["regions"][0]["region_id"]
    );
    let db = rusqlite::Connection::open(p.0.join(".noetome-corpus/corpus.sqlite")).unwrap();
    let revisions: u32 = db
        .query_row(
            "SELECT count(*) FROM page_revisions WHERE source_id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(revisions, 2);
}

#[test]
fn failed_ocr_retry_restores_original_and_retains_each_attempt() {
    let p = TestProject::new();
    let (repo, id) = p.source(2);
    let mut original = page(1, "needs_ocr");
    original["parser"] = json!({"engine":"pdfplumber","adapter_sha256":"test-adapter"});
    repo.commit_page(&id, original).unwrap();
    repo.commit_page(&id, page(2, "text_ready")).unwrap();
    let before = repo.pages(&id, 0, 2).unwrap();
    let mut failed = page(1, "failed");
    failed["parser"] = json!({"engine":"mineru"});
    failed["issues"] = json!([{"code":"OCR_TEST_FAILURE","message":"Synthetic host failure"}]);
    repo.commit_ocr_page(&id, failed).unwrap();
    assert_eq!(repo.summary(&id).unwrap().failed_pages, 1);
    repo.retry_failed(&id).unwrap();
    assert_eq!(repo.next_ocr(&id).unwrap().unwrap(), before[0]);
    assert_eq!(repo.pages(&id, 1, 1).unwrap()[0], before[1]);
    assert_eq!(repo.summary(&id).unwrap().needs_ocr_pages, 1);
    let db = rusqlite::Connection::open(p.0.join(".noetome-corpus/corpus.sqlite")).unwrap();
    let count: u32 = db
        .query_row(
            "SELECT count(*) FROM page_revisions WHERE source_id=?1 AND page_number=1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn cancelled_ocr_pending_source_stays_durably_paused() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    let raw = repo.source_directory(&id).unwrap().join("raw.pdf");
    fs::write(&raw, b"host-only synthetic input").unwrap();
    repo.acquired(&id, &crate::text::sha256(b"host-only synthetic input"), 25)
        .unwrap();
    repo.commit_page(&id, page(1, "needs_ocr")).unwrap();
    let paused = process(&p.0, &id, Arc::new(AtomicBool::new(true))).unwrap();
    assert_eq!(paused.status, "paused");
    assert_eq!(paused.needs_ocr_pages, 1);
    assert!(repo.paused(&id).unwrap());
}

#[test]
fn parser_fingerprint_changes_page_revision_without_mutating_old_records() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    let mut original = page(1, "needs_ocr");
    original["parser"] = json!({"engine":"pdfplumber","adapter_sha256":"first"});
    repo.commit_page(&id, original).unwrap();
    let old = repo.pages(&id, 0, 1).unwrap()[0].clone();
    let mut update = page(1, "ocr_ready");
    update["parser"] = json!({"engine":"mineru","adapter_sha256":"second"});
    repo.commit_ocr_page(&id, update).unwrap();
    let new = repo.pages(&id, 0, 1).unwrap()[0].clone();
    assert_ne!(old["page_revision"], new["page_revision"]);
    let db = rusqlite::Connection::open(p.0.join(".noetome-corpus/corpus.sqlite")).unwrap();
    let archived: String = db
        .query_row(
            "SELECT page_json FROM page_revisions WHERE page_sha256=?1",
            [crate::text::sha256(old.to_string().as_bytes())],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&archived).unwrap(), old);
}

#[test]
fn declared_ocr_assets_are_relative_and_rechecked_without_following_links() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    repo.commit_page(&id, page(1, "needs_ocr")).unwrap();
    let output = repo.source_directory(&id).unwrap().join("ocr/job/output");
    fs::create_dir_all(&output).unwrap();
    let asset = output.join("page.png");
    fs::write(&asset, b"explicit synthetic raster bytes").unwrap();
    let mut recognized = page(1, "ocr_ready");
    recognized["parser"] = json!({"engine":"mineru"});
    recognized["artifact_root"] = json!(output);
    recognized["assets"] = json!([{"path":"page.png","sha256":crate::text::sha256(b"explicit synthetic raster bytes")}]);
    repo.commit_ocr_page(&id, recognized).unwrap();
    let saved = repo.pages(&id, 0, 1).unwrap()[0].clone();
    assert!(!Path::new(saved["artifact_root"].as_str().unwrap()).is_absolute());
    assert_eq!(saved["assets"][0]["byte_length"], 31);
    fs::write(&asset, b"changed raster").unwrap();
    assert_eq!(
        repo.pages(&id, 0, 1).unwrap_err().code(),
        "CORPUS_ASSET_CHANGED"
    );
    fs::remove_file(&asset).unwrap();
    assert!(repo.pages(&id, 0, 1).is_err());
    #[cfg(unix)]
    {
        let outside =
            p.0.parent()
                .unwrap()
                .join(format!("asset-outside-{}", Uuid::new_v4()));
        fs::write(&outside, b"explicit synthetic raster bytes").unwrap();
        std::os::unix::fs::symlink(&outside, &asset).unwrap();
        assert!(repo.pages(&id, 0, 1).is_err());
        fs::remove_file(outside).unwrap();
    }
}

#[test]
fn acquiring_without_a_process_is_a_read_only_interrupted_projection() {
    let p = TestProject::new();
    let repo = Repository::create(&p.0).unwrap();
    let id = Uuid::new_v4().to_string();
    fs::create_dir(repo.source_directory(&id).unwrap()).unwrap();
    let lease = repo.lease(&id).unwrap();
    repo.insert_source(&id, Path::new("synthetic-acquisition.pdf"), 1024)
        .unwrap();
    assert_eq!(repo.summary(&id).unwrap().status, "acquiring");
    drop(lease);
    assert_eq!(repo.summary(&id).unwrap().status, "interrupted");
    let db = rusqlite::Connection::open(p.0.join(".noetome-corpus/corpus.sqlite")).unwrap();
    let status: String = db
        .query_row("SELECT status FROM sources WHERE source_id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(status, "acquiring");
}

#[test]
fn source_package_copies_only_declared_assets_and_preserves_page_evidence() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    let raw = repo.source_directory(&id).unwrap().join("raw.pdf");
    fs::write(&raw, b"synthetic raw package input").unwrap();
    repo.acquired(
        &id,
        &crate::text::sha256(b"synthetic raw package input"),
        27,
    )
    .unwrap();
    repo.commit_page(&id, page(1, "needs_ocr")).unwrap();
    let output = repo.source_directory(&id).unwrap().join("ocr/job/output");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("page.png"), b"synthetic declared image").unwrap();
    fs::write(output.join("unselected-private.bin"), b"must not be copied").unwrap();
    let mut recognized = page(1, "needs_review");
    recognized["parser"] = json!({"engine":"mineru"});
    recognized["artifact_root"] = json!(output);
    recognized["assets"] =
        json!([{"path":"page.png","sha256":crate::text::sha256(b"synthetic declared image")}]);
    repo.commit_ocr_page(&id, recognized).unwrap();
    let current = repo.pages(&id, 0, 1).unwrap()[0].clone();
    let destination =
        p.0.parent()
            .unwrap()
            .join(format!("source-package-{}", Uuid::new_v4()));
    let manifest = export_source(&p.0, &id, &destination).unwrap();
    assert_eq!(manifest["page_status_counts"]["needs_review"], 1);
    let exported: Value =
        serde_json::from_slice(&fs::read(destination.join("pages/000001.json")).unwrap()).unwrap();
    assert_eq!(exported, current);
    let relative = Path::new(exported["artifact_root"].as_str().unwrap());
    assert_eq!(
        fs::read(destination.join(relative).join("page.png")).unwrap(),
        b"synthetic declared image"
    );
    assert!(
        !destination
            .join(relative)
            .join("unselected-private.bin")
            .exists()
    );
    assert_eq!(repo.pages(&id, 0, 1).unwrap()[0], current);
    fs::remove_dir_all(destination).unwrap();
}

#[test]
fn legacy_unpinned_raw_ocr_structure_remains_readable_but_cannot_export_complete() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    let raw = repo.source_directory(&id).unwrap().join("raw.pdf");
    fs::write(&raw, b"synthetic legacy source").unwrap();
    repo.acquired(&id, &crate::text::sha256(b"synthetic legacy source"), 23)
        .unwrap();
    repo.commit_page(&id, page(1, "needs_ocr")).unwrap();
    let output = repo
        .source_directory(&id)
        .unwrap()
        .join("ocr/legacy-job/output");
    fs::create_dir_all(output.join("raw")).unwrap();
    fs::write(output.join("page.png"), b"synthetic legacy page PNG").unwrap();
    fs::write(
        output.join("raw/middle_json.json"),
        br#"{"pages":[{"page_idx":0}]}"#,
    )
    .unwrap();
    let mut recognized = page(1, "needs_review");
    recognized["parser"] = json!({"engine":"mineru","engine_version":"4.2.1"});
    recognized["artifact_root"] = json!(output);
    recognized["assets"] =
        json!([{"path":"page.png","sha256":crate::text::sha256(b"synthetic legacy page PNG")}]);
    repo.commit_ocr_page(&id, recognized).unwrap();
    // Reproduce the saved pre-prepare_assets JSON shape without running the
    // current writer, which deliberately pins raw_output for every new page.
    let mut legacy = repo.pages(&id, 0, 1).unwrap()[0].clone();
    legacy["artifact_root"] = json!(output);
    legacy["raw_output"] = json!("raw/middle_json.json");
    legacy["assets"][0]
        .as_object_mut()
        .unwrap()
        .remove("byte_length");
    let saved = legacy.to_string();
    let db = rusqlite::Connection::open(p.0.join(".noetome-corpus/corpus.sqlite")).unwrap();
    db.execute(
        "UPDATE pages SET page_json=?2,page_sha256=?3 WHERE source_id=?1 AND page_number=1",
        rusqlite::params![id, saved, crate::text::sha256(saved.as_bytes())],
    )
    .unwrap();
    assert_eq!(repo.pages(&id, 0, 1).unwrap()[0], legacy);
    let destination =
        p.0.parent()
            .unwrap()
            .join(format!("legacy-source-package-{}", Uuid::new_v4()));
    assert_eq!(
        export_source(&p.0, &id, &destination).unwrap_err().code(),
        "CORPUS_ASSET_UNPINNED"
    );
    assert!(destination.join("source-package.incomplete.json").is_file());
    assert!(!destination.join("source-package.json").exists());
    let after: String = db
        .query_row(
            "SELECT page_json FROM pages WHERE source_id=?1 AND page_number=1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after, saved);
    assert_eq!(
        repo.pages(legacy["source_id"].as_str().unwrap(), 0, 1)
            .unwrap()[0],
        legacy
    );
    fs::remove_dir_all(destination).unwrap();
}

#[test]
fn fixed_page_digests_read_current_and_original_after_ocr_without_fallback() {
    let p = TestProject::new();
    let (repo, id) = p.source(2);
    repo.commit_page(&id, page(1, "needs_ocr")).unwrap();
    let original = page_snapshot(&p.0, &id, 1).unwrap();
    let original_sha = original["page_sha256"].as_str().unwrap();
    assert_eq!(
        page_revision(&p.0, &id, 1, original_sha).unwrap(),
        original["page"]
    );
    // The parser identity and the fingerprint of complete JSON are different.
    assert_ne!(original["page"]["page_revision"], original["page_sha256"]);
    let mut recognized = page(1, "ocr_ready");
    recognized["text"] = json!("New recognized text");
    recognized["regions"][0]["text"] = recognized["text"].clone();
    recognized["parser"] = json!({"engine":"mineru"});
    repo.commit_ocr_page(&id, recognized).unwrap();
    let current = page_snapshot(&p.0, &id, 1).unwrap();
    assert_ne!(current["page_sha256"], original["page_sha256"]);
    assert_eq!(
        page_revision(&p.0, &id, 1, original_sha).unwrap(),
        original["page"]
    );
    assert_eq!(
        page_revision(&p.0, &id, 1, current["page_sha256"].as_str().unwrap()).unwrap(),
        current["page"]
    );
    for sha in [
        &"0".repeat(64),
        original["page"]["page_revision"].as_str().unwrap(),
    ] {
        assert_eq!(
            page_revision(&p.0, &id, 1, sha).unwrap_err().code(),
            "CORPUS_PAGE_REVISION_NOT_FOUND"
        );
    }
    assert_eq!(
        page_revision(&p.0, &id, 2, original_sha)
            .unwrap_err()
            .code(),
        "CORPUS_PAGE_REVISION_NOT_FOUND"
    );
    assert_eq!(
        page_snapshot(&p.0, &id, 2).unwrap_err().code(),
        "CORPUS_PAGE_NOT_CONFIRMED"
    );
    for number in [0, 3] {
        assert_eq!(
            page_revision(&p.0, &id, number, original_sha)
                .unwrap_err()
                .code(),
            "CORPUS_PAGE_QUERY"
        );
        assert_eq!(
            page_snapshot(&p.0, &id, number).unwrap_err().code(),
            "CORPUS_PAGE_QUERY"
        );
    }
    for sha in ["short", &"A".repeat(64), &"g".repeat(64)] {
        assert_eq!(
            page_revision(&p.0, &id, 1, sha).unwrap_err().code(),
            "CORPUS_PAGE_QUERY"
        );
    }
    assert_eq!(
        page_revision(&p.0, &Uuid::new_v4().to_string(), 1, original_sha)
            .unwrap_err()
            .code(),
        "CORPUS_NOT_FOUND"
    );
}

#[test]
fn legacy_page_snapshot_keeps_original_json_fingerprint_and_missing_revision() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    repo.commit_page(&id, page(1, "needs_ocr")).unwrap();
    let mut legacy = repo.pages(&id, 0, 1).unwrap()[0].clone();
    legacy.as_object_mut().unwrap().remove("page_revision");
    let raw = format!("{}\n", serde_json::to_string_pretty(&legacy).unwrap());
    let sha = crate::text::sha256(raw.as_bytes());
    assert_ne!(sha, crate::text::sha256(legacy.to_string().as_bytes()));
    let db = rusqlite::Connection::open(p.0.join(".noetome-corpus/corpus.sqlite")).unwrap();
    db.execute(
        "UPDATE pages SET page_json=?2,page_sha256=?3 WHERE source_id=?1",
        rusqlite::params![id, raw, sha],
    )
    .unwrap();
    let snapshot = page_snapshot(&p.0, &id, 1).unwrap();
    assert_eq!(snapshot["page_sha256"], sha);
    assert_eq!(snapshot["page"], legacy);
    assert!(snapshot["page"].get("page_revision").is_none());
    assert_eq!(page_revision(&p.0, &id, 1, &sha).unwrap(), legacy);
    repo.commit_ocr_page(&id, page(1, "ocr_ready")).unwrap();
    assert_eq!(page_revision(&p.0, &id, 1, &sha).unwrap(), legacy);
    let saved: String = db.query_row("SELECT page_json FROM page_revisions WHERE source_id=?1 AND page_number=1 AND page_sha256=?2", rusqlite::params![id, sha], |r| r.get(0)).unwrap();
    assert_eq!(saved, raw);
}

#[test]
fn corrupt_current_or_history_cannot_hide_behind_another_matching_digest() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    repo.commit_page(&id, page(1, "needs_ocr")).unwrap();
    let old = repo.page_snapshot(&id, 1).unwrap();
    repo.commit_ocr_page(&id, page(1, "ocr_ready")).unwrap();
    let new = repo.page_snapshot(&id, 1).unwrap();
    let db = rusqlite::Connection::open(p.0.join(".noetome-corpus/corpus.sqlite")).unwrap();
    db.execute(
        "UPDATE page_revisions SET page_json='{}' WHERE source_id=?1 AND page_sha256=?2",
        rusqlite::params![id, old["page_sha256"].as_str().unwrap()],
    )
    .unwrap();
    assert_eq!(
        page_revision(&p.0, &id, 1, old["page_sha256"].as_str().unwrap())
            .unwrap_err()
            .code(),
        "CORPUS_PAGE_CHANGED"
    );
    assert_eq!(
        page_revision(&p.0, &id, 1, new["page_sha256"].as_str().unwrap()).unwrap(),
        new["page"]
    );
    // The untouched history copy of the current page must not conceal damage
    // in its current copy, or permit a snapshot to invent a new digest.
    db.execute("UPDATE pages SET page_json='{}' WHERE source_id=?1", [&id])
        .unwrap();
    assert_eq!(
        page_revision(&p.0, &id, 1, new["page_sha256"].as_str().unwrap())
            .unwrap_err()
            .code(),
        "CORPUS_PAGE_CHANGED"
    );
    assert_eq!(
        page_snapshot(&p.0, &id, 1).unwrap_err().code(),
        "CORPUS_PAGE_CHANGED"
    );
}

#[test]
fn fixed_history_rechecks_declared_assets_even_after_a_new_page_is_saved() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    let output = repo
        .source_directory(&id)
        .unwrap()
        .join("ocr/pinned/output");
    fs::create_dir_all(output.join("raw")).unwrap();
    let asset = output.join("page.png");
    let raw_asset = output.join("raw/structure.json");
    fs::write(&asset, b"historical synthetic raster").unwrap();
    fs::write(&raw_asset, b"historical synthetic structure").unwrap();
    let mut original = page(1, "needs_ocr");
    original["artifact_root"] = json!(output);
    original["raw_output"] = json!("raw/structure.json");
    original["assets"] =
        json!([{"path":"page.png","sha256":crate::text::sha256(b"historical synthetic raster")}]);
    repo.commit_page(&id, original).unwrap();
    let old = repo.page_snapshot(&id, 1).unwrap();
    let sha = old["page_sha256"].as_str().unwrap();
    repo.commit_ocr_page(&id, page(1, "ocr_ready")).unwrap();
    assert_eq!(page_revision(&p.0, &id, 1, sha).unwrap(), old["page"]);
    fs::write(&raw_asset, b"changed structure").unwrap();
    assert_eq!(
        page_revision(&p.0, &id, 1, sha).unwrap_err().code(),
        "CORPUS_ASSET_CHANGED"
    );
    fs::write(&raw_asset, b"historical synthetic structure").unwrap();
    fs::remove_file(&asset).unwrap();
    assert_eq!(
        page_revision(&p.0, &id, 1, sha).unwrap_err().code(),
        "CORPUS_ASSET_CHANGED"
    );
    fs::write(&asset, b"historical synthetic raster").unwrap();
    assert!(page_revision(&p.0, &id, 1, sha).is_ok());
    assert!(page_snapshot(&p.0, &id, 1).is_ok());
}

#[test]
fn fixed_page_rejects_wrong_page_source_and_escaping_asset_even_with_consistent_json_hash() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    repo.commit_page(&id, page(1, "text_ready")).unwrap();
    let original = repo.pages(&id, 0, 1).unwrap()[0].clone();
    let db = rusqlite::Connection::open(p.0.join(".noetome-corpus/corpus.sqlite")).unwrap();
    for field in ["page_number", "source_id"] {
        let mut changed = original.clone();
        changed[field] = if field == "page_number" {
            json!(2)
        } else {
            json!(Uuid::new_v4().to_string())
        };
        let raw = changed.to_string();
        let sha = crate::text::sha256(raw.as_bytes());
        db.execute(
            "INSERT INTO page_revisions VALUES(?1,1,?2,?3)",
            rusqlite::params![id, sha, raw],
        )
        .unwrap();
        assert_eq!(
            page_revision(&p.0, &id, 1, &sha).unwrap_err().code(),
            "CORPUS_PAGE_INVALID"
        );
    }
    let mut changed = original.clone();
    changed["regions"][0]["bbox"][2] = json!(900);
    let raw = changed.to_string();
    let sha = crate::text::sha256(raw.as_bytes());
    db.execute(
        "INSERT INTO page_revisions VALUES(?1,1,?2,?3)",
        rusqlite::params![id, sha, raw],
    )
    .unwrap();
    assert_eq!(
        page_revision(&p.0, &id, 1, &sha).unwrap_err().code(),
        "CORPUS_PAGE_INVALID"
    );
    let mut changed = original.clone();
    changed["artifact_root"] = json!(".noetome-corpus/../outside");
    changed["assets"] = json!([{"path":"page.png","sha256":"a".repeat(64)}]);
    let raw = changed.to_string();
    let sha = crate::text::sha256(raw.as_bytes());
    db.execute(
        "INSERT INTO page_revisions VALUES(?1,1,?2,?3)",
        rusqlite::params![id, sha, raw],
    )
    .unwrap();
    assert_eq!(
        page_revision(&p.0, &id, 1, &sha).unwrap_err().code(),
        "CORPUS_ASSET_PATH"
    );
    assert_eq!(page_snapshot(&p.0, &id, 1).unwrap()["page"], original);
}

#[test]
fn source_verification_streams_saved_original_and_preserves_state_on_failure() {
    let p = TestProject::new();
    let (repo, id) = p.source(1);
    repo.commit_page(&id, page(1, "text_ready")).unwrap();
    let raw = repo.source_directory(&id).unwrap().join("raw.pdf");
    let bytes = vec![b's'; 3 * 1024 * 1024 + 17];
    fs::write(&raw, &bytes).unwrap();
    repo.acquired(&id, &crate::text::sha256(&bytes), bytes.len() as u64)
        .unwrap();
    let before = serde_json::to_value(repo.summary(&id).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(verify_source(&p.0, &id).unwrap()).unwrap(),
        before
    );
    let mut changed = bytes.clone();
    changed[1024 * 1024 + 3] = b'x';
    fs::write(&raw, &changed).unwrap();
    assert_eq!(
        verify_source(&p.0, &id).unwrap_err().code(),
        "CORPUS_SOURCE_CHANGED"
    );
    assert_eq!(
        serde_json::to_value(repo.summary(&id).unwrap()).unwrap(),
        before
    );
    // Intact page evidence does not make a corrupt original pass verification.
    assert!(page_snapshot(&p.0, &id, 1).is_ok());
    fs::remove_file(&raw).unwrap();
    assert!(verify_source(&p.0, &id).is_err());
    assert_eq!(
        serde_json::to_value(repo.summary(&id).unwrap()).unwrap(),
        before
    );
    fs::write(&raw, bytes).unwrap();
    assert!(verify_source(&p.0, &id).is_ok());
}
