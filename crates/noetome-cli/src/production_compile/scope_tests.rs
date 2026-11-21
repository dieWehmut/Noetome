//! Authored corpus/budget fixtures only: no PDF parser, HTTP call, real model or scientific proof.
use super::*;
use rusqlite::{Connection, params};
use std::path::PathBuf;
use uuid::Uuid;

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("noetome-chapter-scope-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct AuthoredSource {
    scope: SourceScope,
    pages: Vec<(Value, String)>,
}
struct Fixture {
    _temp: TempDir,
    project: PathBuf,
    source: AuthoredSource,
    config: CompileConfig,
    baseline: Vec<u8>,
}
impl Fixture {
    fn new(pages: &[(&str, &str)]) -> Self {
        let temp = TempDir::new();
        let project = temp.0.join("project");
        let info = project::initialize(&project).unwrap();
        let corpus_root = project.join(".noetome-corpus");
        fs::create_dir(&corpus_root).unwrap();
        let db = Connection::open(corpus_root.join("corpus.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE metadata(singleton INTEGER PRIMARY KEY,project_id TEXT,schema_version INTEGER); CREATE TABLE sources(source_id TEXT PRIMARY KEY,display_name TEXT,selected_path TEXT,content_sha256 TEXT,byte_length INTEGER,status TEXT,page_count INTEGER,created_at TEXT,pause_requested INTEGER,error_code TEXT,error_message TEXT,preflight_json TEXT); CREATE TABLE pages(source_id TEXT,page_number INTEGER,status TEXT,page_json TEXT,page_sha256 TEXT,PRIMARY KEY(source_id,page_number)); PRAGMA application_id=1313817424; PRAGMA user_version=1;").unwrap();
        db.execute("INSERT INTO metadata VALUES(1,?1,1)", [&info.project_id])
            .unwrap();
        drop(db);
        let source = Self::insert_source(&project, pages);
        let budget_root = temp.0.join("synthetic-external-budget");
        fs::create_dir(&budget_root).unwrap();
        let ledger = budget_root.join("authored.usage.json");
        let baseline = serde_json::to_vec(&json!({
            "schema_version":1,"approved_total_tokens":1000000,"accounted_tokens":100,
            "pending_reserved_tokens":0,"remaining_tokens":999900,"model_attempts":1,
            "records":[{"invocation_id":"authored-history","reservation_tokens":2000,
                "confirmed_total_tokens":100,"network_attempts":1,"status":"settled_valid_usage_text"}],
            "updated_at":"2026-11-20T00:00:00Z"
        })).unwrap();
        fs::write(&ledger, &baseline).unwrap();
        let config = CompileConfig {
            schema_version: 1,
            endpoint: "http://127.0.0.1:1/v1/chat/completions".into(),
            model: "noetome-test-chapter-scope".into(),
            credential_env: "NOETOME_TEST_CHAPTER_SCOPE_KEY".into(),
            credential_mode: None,
            reasoning_effort: None,
            transport_mode: "local_http_test".into(),
            usage_ledger: ledger,
            max_chunk_bytes: 1024,
            max_output_tokens: 512,
            request_timeout_ms: 1000,
            request_strategy_version: None,
        };
        Self {
            _temp: temp,
            project,
            source,
            config,
            baseline,
        }
    }
    fn insert_source(project: &Path, pages: &[(&str, &str)]) -> AuthoredSource {
        let id = Uuid::new_v4().to_string();
        let root = project.join(".noetome-corpus");
        fs::create_dir(root.join(&id)).unwrap();
        let original = b"Authored immutable chapter scope fixture; deliberately not a PDF.";
        fs::write(root.join(&id).join("raw.pdf"), original).unwrap();
        let scope = SourceScope {
            source_id: id,
            source_sha256: sha256(original),
            display_name: "authored-chapter.pdf".into(),
            byte_length: original.len() as u64,
            physical_pages: pages.len() as u32,
            scope_schema_version: None,
            page_ranges: None,
        };
        let db = Connection::open(root.join("corpus.sqlite")).unwrap();
        db.execute("INSERT INTO sources VALUES(?1,'authored-chapter.pdf','explicit-synthetic-selection',?2,?3,'needs_review',?4,'2026-11-20T00:00:00Z',0,NULL,NULL,NULL)", params![scope.source_id,scope.source_sha256,scope.byte_length as i64,scope.physical_pages]).unwrap();
        let pages = pages.iter().enumerate().map(|(index, (text, status))| {
            let page = json!({"schema_version":1,"source_id":scope.source_id,"page_number":index+1,
                "status":status,"text":text,"width_pt":612.0,"height_pt":792.0,
                "geometry":{},"regions":[],"issues":[]});
            let raw = serde_json::to_string(&page).unwrap();
            let hash = sha256(raw.as_bytes());
            db.execute("INSERT INTO pages VALUES(?1,?2,?3,?4,?5)",
                params![scope.source_id,index as i64+1,status,raw,hash]).unwrap();
            (page, hash)
        }).collect();
        AuthoredSource { scope, pages }
    }
    fn selection(&self, ranges: &[(u32, u32)]) -> CompileScope {
        CompileScope {
            schema_version: 1,
            sources: vec![selection(&self.source, ranges)],
        }
    }
    fn create(&self, scope: CompileScope) -> RunSummary {
        create_scoped(
            &self.project,
            &[self.source.scope.source_id.clone()],
            self.config.clone(),
            Some(scope),
        )
        .unwrap()
    }
    fn save(&self, store: &Store, run: &str, scope: &SourceScope, number: u32) -> Vec<Unit> {
        save_source_page(
            store,
            run,
            &self.source,
            scope,
            number,
            self.config.max_chunk_bytes,
        )
    }
    fn no_create_side_effects(&self) {
        assert!(!self.project.join(".noetome-compile").exists());
        assert!(
            !crate::production_budget::default_database_path(&self.config.usage_ledger).exists()
        );
        assert!(
            !crate::production_budget::authority_marker_path(&self.config.usage_ledger).exists()
        );
        assert_eq!(fs::read(&self.config.usage_ledger).unwrap(), self.baseline);
        assert_eq!(
            fs::read_dir(self.config.usage_ledger.parent().unwrap())
                .unwrap()
                .count(),
            1
        );
    }
}
fn selection(source: &AuthoredSource, ranges: &[(u32, u32)]) -> SourceSelection {
    SourceSelection {
        source_id: source.scope.source_id.clone(),
        page_ranges: ranges
            .iter()
            .map(|&(start_page, end_page)| PageRange {
                start_page,
                end_page,
            })
            .collect(),
    }
}
fn save_source_page(
    store: &Store,
    run: &str,
    source: &AuthoredSource,
    scope: &SourceScope,
    number: u32,
    max: usize,
) -> Vec<Unit> {
    let (page, hash) = &source.pages[number as usize - 1];
    let status = page["status"].as_str().unwrap();
    let units = if matches!(status, "text_ready" | "ocr_ready") {
        chunks(scope, page, hash, max, LEGACY_REQUEST_STRATEGY)
            .unwrap()
            .0
    } else {
        vec![]
    };
    store
        .save_page(run, scope, number, hash, status, units.clone(), vec![])
        .unwrap();
    units
}
fn saved_inputs(store: &Store, run: &str) -> (String, String, String, String) {
    let db = Connection::open(store.root.join("compile.sqlite")).unwrap();
    db.query_row("SELECT r.config_json,r.config_sha256,s.scope_json,s.scope_sha256 FROM runs r JOIN sources s ON s.run_id=r.run_id WHERE r.run_id=?1 ORDER BY s.ordinal LIMIT 1", [run],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap()
}

#[test]
fn command_parse_keeps_legacy_whole_book_create_paths() {
    let args = [
        "create",
        "project with spaces",
        "selected sources.json",
        "compile config.json",
    ]
    .map(std::ffi::OsString::from);
    match super::command::parse(&args).unwrap() {
        super::command::Command::Create(project, selection, config, scope) => {
            assert_eq!(project, PathBuf::from("project with spaces"));
            assert_eq!(selection, PathBuf::from("selected sources.json"));
            assert_eq!(config, PathBuf::from("compile config.json"));
            assert_eq!(scope, None);
        }
        other => panic!("Expected legacy create, got {other:?}"),
    }
}

#[test]
fn command_parse_keeps_explicit_trailing_scope_path() {
    let args = [
        "create",
        "项目 with spaces",
        "selected sources.json",
        "compile config.json",
        "--scope",
        "chapters/第一章 ranges.json",
    ]
    .map(std::ffi::OsString::from);
    match super::command::parse(&args).unwrap() {
        super::command::Command::Create(project, selection, config, scope) => {
            assert_eq!(project, PathBuf::from("项目 with spaces"));
            assert_eq!(selection, PathBuf::from("selected sources.json"));
            assert_eq!(config, PathBuf::from("compile config.json"));
            assert_eq!(scope, Some(PathBuf::from("chapters/第一章 ranges.json")));
        }
        other => panic!("Expected scoped create, got {other:?}"),
    }
}

#[test]
fn command_parse_rejects_missing_duplicate_unknown_and_extra_scope_arguments() {
    let invalid: &[&[&str]] = &[
        &["create"],
        &["create", "project", "sources.json"],
        &["create", "", "sources.json", "config.json"],
        &["create", "project", "", "config.json"],
        &["create", "project", "sources.json", ""],
        &[
            "create",
            "project",
            "sources.json",
            "config.json",
            "--scope",
        ],
        &[
            "create",
            "project",
            "sources.json",
            "config.json",
            "--scope",
            "",
        ],
        &[
            "create",
            "project",
            "sources.json",
            "config.json",
            "--unknown",
            "scope.json",
        ],
        &[
            "create",
            "project",
            "sources.json",
            "config.json",
            "--scopes",
            "scope.json",
        ],
        &[
            "create",
            "project",
            "sources.json",
            "config.json",
            "--scope=scope.json",
        ],
        &[
            "create",
            "project",
            "sources.json",
            "config.json",
            "scope.json",
        ],
        &[
            "create",
            "project",
            "sources.json",
            "config.json",
            "--scope",
            "scope.json",
            "extra",
        ],
        &[
            "create",
            "project",
            "sources.json",
            "config.json",
            "--scope",
            "scope.json",
            "--scope",
            "other.json",
        ],
        &[
            "create",
            "--scope",
            "scope.json",
            "project",
            "sources.json",
            "config.json",
        ],
        &["run", "project", "run-id", "--scope", "scope.json"],
        &["status", "project", "run-id", "--scope", "scope.json"],
        &[
            "control",
            "project",
            "run-id",
            "pause",
            "--scope",
            "scope.json",
        ],
    ];
    for case in invalid {
        let args = case
            .iter()
            .map(|value| std::ffi::OsString::from(*value))
            .collect::<Vec<_>>();
        assert_eq!(
            super::command::parse(&args).unwrap_err(),
            crate::INVALID_ARGUMENT,
            "{case:?}"
        );
    }
}

#[test]
fn invalid_public_scopes_reject_before_any_compile_or_budget_migration() {
    let fixture = Fixture::new(&[("one", "text_ready"); 8]);
    let id = fixture.source.scope.source_id.clone();
    for (name, ranges) in [
        ("empty", vec![]),
        ("zero", vec![(0, 2)]),
        ("reversed", vec![(4, 3)]),
        ("past_end", vec![(8, 9)]),
        ("duplicate", vec![(3, 4), (3, 4)]),
        ("overlap", vec![(2, 5), (4, 6)]),
        ("shared_endpoint", vec![(2, 4), (4, 6)]),
    ] {
        let result = create_scoped(
            &fixture.project,
            std::slice::from_ref(&id),
            fixture.config.clone(),
            Some(fixture.selection(&ranges)),
        );
        assert_eq!(result.unwrap_err().code(), "COMPILE_SCOPE", "{name}");
        fixture.no_create_side_effects();
    }
    let other = Uuid::new_v4().to_string();
    let mut wrong_version = fixture.selection(&[(2, 3)]);
    wrong_version.schema_version = 2;
    let mut wrong_source = fixture.selection(&[(2, 3)]);
    wrong_source.sources[0].source_id = other.clone();
    let duplicate_sources = CompileScope {
        schema_version: 1,
        sources: vec![
            selection(&fixture.source, &[(2, 2)]),
            selection(&fixture.source, &[(3, 3)]),
        ],
    };
    for (ids, scope) in [
        (
            vec![id.clone()],
            CompileScope {
                schema_version: 1,
                sources: vec![],
            },
        ),
        (vec![id.clone()], wrong_version),
        (vec![id.clone()], wrong_source),
        (vec![id.clone(), other], duplicate_sources),
    ] {
        assert_eq!(
            create_scoped(&fixture.project, &ids, fixture.config.clone(), Some(scope))
                .unwrap_err()
                .code(),
            "COMPILE_SCOPE"
        );
        fixture.no_create_side_effects();
    }
    for ids in [vec![], vec![id.clone(), id]] {
        assert_eq!(
            create_scoped(&fixture.project, &ids, fixture.config.clone(), None)
                .unwrap_err()
                .code(),
            "COMPILE_SELECTION"
        );
        fixture.no_create_side_effects();
    }
}

#[test]
fn unsupported_request_strategy_rejects_without_creating_authority_or_run() {
    let fixture = Fixture::new(&[("one", "text_ready"); 3]);
    for version in [0, 99, u32::MAX] {
        let mut config = fixture.config.clone();
        config.request_strategy_version = Some(version);
        assert_eq!(
            create_scoped(
                &fixture.project,
                &[fixture.source.scope.source_id.clone()],
                config,
                Some(fixture.selection(&[(2, 3)]))
            )
            .unwrap_err()
            .code(),
            "COMPILE_REQUEST_STRATEGY"
        );
        fixture.no_create_side_effects();
    }
}

#[test]
fn noncontiguous_physical_pages_keep_original_identity_and_only_selected_units_attempts() {
    let fixture = Fixture::new(&[("Authored page", "text_ready"); 8]);
    let summary = fixture.create(fixture.selection(&[(7, 7), (3, 4)]));
    assert_eq!(
        (
            summary.physical_pages,
            summary.selected_pages,
            summary.out_of_scope_pages
        ),
        (8, 3, 5)
    );
    assert_eq!(
        (
            summary.prepared_pages,
            summary.unprepared_pages,
            summary.unresolved_pages
        ),
        (0, 3, 3)
    );
    assert_eq!(summary.request_strategy_version, 3);
    let scope = &summary.sources[0];
    assert_eq!(scope.scope_schema_version, Some(1));
    assert_eq!(
        scope.page_ranges,
        Some(vec![
            PageRange {
                start_page: 3,
                end_page: 4
            },
            PageRange {
                start_page: 7,
                end_page: 7
            }
        ])
    );
    assert_eq!(scope.source_sha256, fixture.source.scope.source_sha256);
    assert_eq!(scope.physical_pages, 8);
    let run = &summary.run_id;
    let store = required(&fixture.project).unwrap();
    assert_eq!(store.config(run).unwrap().request_strategy_version, Some(3));
    for number in [3, 4, 7] {
        assert_eq!(store.next_unprepared_page(run).unwrap().unwrap().1, number);
        let units = fixture.save(&store, run, scope, number);
        if number == 3 {
            let request = Uuid::new_v4().to_string();
            let prepared = provider::prepare(
                store.project_id(),
                run,
                &request,
                &units[0],
                &store.config(run).unwrap(),
                &[],
            )
            .unwrap();
            store
                .begin_attempt(run, &units[0], &request, &sha256(&prepared.body))
                .unwrap();
        }
    }
    assert!(store.next_unprepared_page(run).unwrap().is_none());
    let pages = store.query(run, "pages", 0, 20, "").unwrap();
    assert_eq!(pages["total"], 8);
    for page in pages["items"].as_array().unwrap() {
        let number = page["page_number"].as_u64().unwrap();
        let selected = [3, 4, 7].contains(&number);
        assert_eq!(page["in_scope"], selected);
        if !selected {
            assert_eq!(page["status"], "out_of_scope");
            assert_eq!(page["units_total"], 0);
            assert_eq!(page["units_failed"], 0);
            assert!(page["page_sha256"].is_null());
        }
    }
    let units = store.query(run, "units", 0, 20, "").unwrap();
    assert_eq!(
        units["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u["page_number"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![3, 4, 7]
    );
    let attempts = store.attempts(run).unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(
        store.unit(run, &attempts[0].unit_id).unwrap().page_number,
        3
    );
    let after = store.summary(run).unwrap();
    assert_eq!(
        (after.prepared_pages, after.unprepared_pages, after.attempts),
        (3, 0, 1)
    );
    assert_eq!(after.scientific_quality, "not_run");
    assert_eq!(
        fs::read(&fixture.config.usage_ledger).unwrap(),
        fixture.baseline
    );
    assert_eq!(
        corpus::verify_source(&fixture.project, &scope.source_id)
            .unwrap()
            .page_count,
        Some(8)
    );
}

#[test]
fn selected_ocr_failed_unprepared_counts_ignore_outside_pages_and_deduplicate_failed_units() {
    let long_page = "Authored learning text. ".repeat(100);
    let fixture = Fixture::new(&[
        ("", "needs_ocr"),
        (&long_page, "text_ready"),
        ("", "needs_ocr"),
        ("", "failed"),
        ("", "failed"),
        ("", "text_ready"),
        ("outside", "text_ready"),
    ]);
    let created = fixture.create(fixture.selection(&[(5, 6), (2, 3)]));
    let run = &created.run_id;
    let scope = &created.sources[0];
    let store = required(&fixture.project).unwrap();
    let units = fixture.save(&store, run, scope, 2);
    assert!(units.len() > 1);
    for unit in &units {
        store
            .mark_unit_failed(
                run,
                &unit.unit_id,
                "AUTHORED_FAILURE",
                "Synthetic failure, no provider call",
            )
            .unwrap();
    }
    fixture.save(&store, run, scope, 3);
    let partial = store.summary(run).unwrap();
    assert_eq!(
        (
            partial.physical_pages,
            partial.selected_pages,
            partial.out_of_scope_pages
        ),
        (7, 4, 3)
    );
    assert_eq!(
        (
            partial.prepared_pages,
            partial.unprepared_pages,
            partial.pending_ocr_pages,
            partial.failed_pages,
            partial.unresolved_pages
        ),
        (2, 2, 1, 1, 4)
    );
    assert_eq!(partial.units_failed, units.len() as u64);
    fixture.save(&store, run, scope, 5);
    fixture.save(&store, run, scope, 6);
    let after = store.summary(run).unwrap();
    assert_eq!(
        (
            after.prepared_pages,
            after.unprepared_pages,
            after.pending_ocr_pages,
            after.failed_pages,
            after.unresolved_pages
        ),
        (4, 0, 1, 2, 3)
    );
    assert_eq!(after.attempts, 0);
    store.retry_failed(run).unwrap();
    let retried = store.summary(run).unwrap();
    assert_eq!(
        (
            retried.failed_pages,
            retried.units_failed,
            retried.unresolved_pages
        ),
        (1, 0, 3)
    );
}

#[test]
fn pause_reopen_and_retry_keep_scope_bytes_and_next_selected_physical_page() {
    let fixture = Fixture::new(&[("Authored page", "text_ready"); 6]);
    let created = fixture.create(fixture.selection(&[(3, 3), (5, 5)]));
    let run = &created.run_id;
    let store = required(&fixture.project).unwrap();
    let pinned = saved_inputs(&store, run);
    let units = fixture.save(&store, run, &created.sources[0], 3);
    store.pause(run).unwrap();
    assert_eq!(
        store
            .begin_attempt(
                run,
                &units[0],
                &Uuid::new_v4().to_string(),
                &sha256(b"not sent")
            )
            .unwrap_err()
            .code(),
        "COMPILE_STOPPED"
    );
    drop(store);
    let reopened = required(&fixture.project).unwrap();
    assert_eq!(reopened.summary(run).unwrap().status, "paused");
    assert_eq!(reopened.next_unprepared_page(run).unwrap().unwrap().1, 5);
    reopened.resume(run).unwrap();
    reopened
        .mark_unit_failed(
            run,
            &units[0].unit_id,
            "AUTHORED_FAILURE",
            "No model was called",
        )
        .unwrap();
    reopened.retry_failed(run).unwrap();
    fixture.save(&reopened, run, &created.sources[0], 5);
    assert!(reopened.next_unprepared_page(run).unwrap().is_none());
    assert_eq!(saved_inputs(&reopened, run), pinned);
    let summary = reopened.summary(run).unwrap();
    assert_eq!(
        (
            summary.physical_pages,
            summary.selected_pages,
            summary.out_of_scope_pages,
            summary.prepared_pages,
            summary.unprepared_pages
        ),
        (6, 2, 4, 2, 0)
    );
    let units = reopened.query(run, "units", 0, 20, "").unwrap();
    assert_eq!(
        units["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u["page_number"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![3, 5]
    );
    assert_eq!(summary.attempts, 0);
}

#[test]
fn save_and_attempt_boundaries_refuse_scope_expansion() {
    let fixture = Fixture::new(&[("Authored page", "text_ready"); 6]);
    let created = fixture.create(fixture.selection(&[(3, 3), (5, 5)]));
    let run = &created.run_id;
    let store = required(&fixture.project).unwrap();
    let scope = &created.sources[0];
    let (page, hash) = &fixture.source.pages[0];
    let outside = chunks(
        scope,
        page,
        hash,
        fixture.config.max_chunk_bytes,
        LEGACY_REQUEST_STRATEGY,
    )
    .unwrap()
    .0;
    assert!(
        store
            .save_page(run, scope, 1, hash, "text_ready", outside, vec![])
            .is_err()
    );
    let mut expanded = scope.clone();
    expanded.page_ranges = Some(vec![PageRange {
        start_page: 1,
        end_page: 6,
    }]);
    let (page, hash) = &fixture.source.pages[2];
    assert!(
        store
            .save_page(
                run,
                &expanded,
                3,
                hash,
                "text_ready",
                chunks(
                    &expanded,
                    page,
                    hash,
                    fixture.config.max_chunk_bytes,
                    LEGACY_REQUEST_STRATEGY
                )
                .unwrap()
                .0,
                vec![]
            )
            .is_err()
    );
    assert_eq!(store.summary(run).unwrap().prepared_pages, 0);
    let mut forged = fixture.save(&store, run, scope, 3).remove(0);
    forged.page_number = 1;
    assert!(
        store
            .begin_attempt(
                run,
                &forged,
                &Uuid::new_v4().to_string(),
                &sha256(b"not sent")
            )
            .is_err()
    );
    assert_eq!(store.summary(run).unwrap().attempts, 0);
    assert_eq!(store.next_unprepared_page(run).unwrap().unwrap().1, 5);
}

#[test]
fn source_selection_order_and_independent_page_ranges_survive_multiple_sources() {
    let fixture = Fixture::new(&[("First authored source", "text_ready"); 5]);
    let second = Fixture::insert_source(
        &fixture.project,
        &[("Second authored source", "text_ready"); 4],
    );
    let ids = vec![
        second.scope.source_id.clone(),
        fixture.source.scope.source_id.clone(),
    ];
    let scope = CompileScope {
        schema_version: 1,
        sources: vec![
            selection(&fixture.source, &[(2, 2)]),
            selection(&second, &[(4, 4), (2, 2)]),
        ],
    };
    let created =
        create_scoped(&fixture.project, &ids, fixture.config.clone(), Some(scope)).unwrap();
    assert_eq!(
        (
            created.physical_pages,
            created.selected_pages,
            created.out_of_scope_pages
        ),
        (9, 3, 6)
    );
    assert_eq!(
        created
            .sources
            .iter()
            .map(|s| s.source_id.clone())
            .collect::<Vec<_>>(),
        ids
    );
    let store = required(&fixture.project).unwrap();
    for (source, scope, number) in [
        (&second, &created.sources[0], 2),
        (&second, &created.sources[0], 4),
        (&fixture.source, &created.sources[1], 2),
    ] {
        let next = store
            .next_unprepared_page(&created.run_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (next.0.source_id.as_str(), next.1),
            (scope.source_id.as_str(), number)
        );
        save_source_page(
            &store,
            &created.run_id,
            source,
            scope,
            number,
            fixture.config.max_chunk_bytes,
        );
    }
    assert!(
        store
            .next_unprepared_page(&created.run_id)
            .unwrap()
            .is_none()
    );
    let page = store.query(&created.run_id, "pages", 4, 2, "").unwrap();
    assert_eq!(page["total"], 9);
    assert_eq!(
        page["items"][0]["source_id"],
        fixture.source.scope.source_id
    );
    assert_eq!(page["items"][0]["page_number"], 1);
    assert_eq!(page["items"][0]["in_scope"], false);
    assert_eq!(page["items"][1]["page_number"], 2);
    assert_eq!(page["items"][1]["in_scope"], true);
}

#[test]
fn new_runs_pin_default_v3_and_explicit_versions_for_scoped_and_whole_book_entries() {
    for (requested, expected) in [(None, 3), (Some(1), 1), (Some(2), 2), (Some(3), 3)] {
        for scoped in [false, true] {
            let fixture = Fixture::new(&[("Authored strategy text", "text_ready"); 3]);
            let mut config = fixture.config.clone();
            config.request_strategy_version = requested;
            let input_bytes = serde_json::to_vec(&config).unwrap();
            let summary = create_scoped(
                &fixture.project,
                &[fixture.source.scope.source_id.clone()],
                config.clone(),
                scoped.then(|| fixture.selection(&[(2, 2)])),
            )
            .unwrap();
            assert_eq!(summary.request_strategy_version, expected);
            assert_eq!(summary.selected_pages, if scoped { 1 } else { 3 });
            assert_eq!(summary.attempts, 0);
            assert_eq!(serde_json::to_vec(&config).unwrap(), input_bytes);
            let store = required(&fixture.project).unwrap();
            let saved = store.config(&summary.run_id).unwrap();
            assert_eq!(saved.request_strategy_version, Some(expected));
            let before = saved_inputs(&store, &summary.run_id);
            store.pause(&summary.run_id).unwrap();
            store.resume(&summary.run_id).unwrap();
            assert_eq!(saved_inputs(&store, &summary.run_id), before);
            assert_eq!(
                store
                    .summary(&summary.run_id)
                    .unwrap()
                    .request_strategy_version,
                expected
            );
        }
    }
}

#[test]
fn legacy_whole_book_config_scope_request_bytes_and_unsettled_budget_survive_reopen() {
    let fixture = Fixture::new(&[("Authored legacy text", "text_ready"); 3]);
    let store = Store::create(&fixture.project).unwrap();
    let run = store
        .create_run(&fixture.config, std::slice::from_ref(&fixture.source.scope))
        .unwrap();
    // Noncanonical whitespace is intentional historical data: reads must retain original bytes.
    let raw_config = format!(
        "{}\n",
        serde_json::to_string_pretty(&fixture.config).unwrap()
    );
    let raw_scope = format!(
        "{}\n",
        serde_json::to_string_pretty(&fixture.source.scope).unwrap()
    );
    assert!(!raw_config.contains("request_strategy_version"));
    assert!(!raw_scope.contains("page_ranges"));
    assert!(!raw_scope.contains("scope_schema_version"));
    let db = Connection::open(store.root.join("compile.sqlite")).unwrap();
    db.execute(
        "UPDATE runs SET config_json=?2,config_sha256=?3 WHERE run_id=?1",
        params![run, raw_config, sha256(raw_config.as_bytes())],
    )
    .unwrap();
    db.execute(
        "UPDATE sources SET scope_json=?2,scope_sha256=?3 WHERE run_id=?1",
        params![run, raw_scope, sha256(raw_scope.as_bytes())],
    )
    .unwrap();
    drop(db);
    let before = saved_inputs(&store, &run);
    let mut first_unit = None;
    for number in 1..=3 {
        assert_eq!(store.next_unprepared_page(&run).unwrap().unwrap().1, number);
        let units = fixture.save(&store, &run, &fixture.source.scope, number);
        if first_unit.is_none() {
            first_unit = units.into_iter().next();
        }
    }
    let unit = first_unit.unwrap();
    let request_id = Uuid::new_v4().to_string();
    let prepared = provider::prepare(
        store.project_id(),
        &run,
        &request_id,
        &unit,
        &fixture.config,
        &[],
    )
    .unwrap();
    let mut explicit_v1 = fixture.config.clone();
    explicit_v1.request_strategy_version = Some(1);
    assert_eq!(
        provider::prepare(
            store.project_id(),
            &run,
            &request_id,
            &unit,
            &explicit_v1,
            &[]
        )
        .unwrap()
        .body,
        prepared.body
    );
    let request_hash = sha256(&prepared.body);
    let attempt = store
        .begin_attempt(&run, &unit, &request_id, &request_hash)
        .unwrap();
    let request_path = store.root.join(&attempt.directory).join("request.json");
    save_artifact(&request_path, &prepared.body).unwrap();
    let mut budget = BudgetAuthority::migrate(&fixture.config.usage_ledger).unwrap();
    let budget_key = key(&run, &attempt);
    budget
        .reserve(&BudgetRequest {
            key: budget_key.clone(),
            request_sha256: request_hash.clone(),
            reserved_tokens: 2000,
        })
        .unwrap();
    budget.mark_sent(&budget_key).unwrap();
    let authored_receipt = b"Authored unknown-usage fixture; no HTTP request was made.";
    save_artifact(
        &store
            .root
            .join(&attempt.directory)
            .join("authored-budget-receipt.txt"),
        authored_receipt,
    )
    .unwrap();
    let unknown = budget
        .settle(
            &budget_key,
            &BudgetSettlement {
                network_attempts: 1,
                usage: None,
                receipt_sha256: sha256(authored_receipt),
            },
        )
        .unwrap();
    assert_eq!(unknown.state, BudgetAttemptState::Unknown);
    let budget_before = serde_json::to_value(budget.status().unwrap()).unwrap();
    drop(budget);
    drop(store);
    let reopened = required(&fixture.project).unwrap();
    let restored_config = reopened.config(&run).unwrap();
    assert_eq!(restored_config.request_strategy_version, None);
    let summary = reopened.summary(&run).unwrap();
    assert_eq!(
        (
            summary.physical_pages,
            summary.selected_pages,
            summary.out_of_scope_pages,
            summary.prepared_pages,
            summary.unprepared_pages
        ),
        (3, 3, 0, 3, 0)
    );
    assert_eq!(summary.request_strategy_version, 1);
    assert_eq!(summary.sources[0].scope_schema_version, None);
    assert_eq!(summary.sources[0].page_ranges, None);
    assert!(reopened.next_unprepared_page(&run).unwrap().is_none());
    reopened.pause(&run).unwrap();
    reopened.resume(&run).unwrap();
    assert_eq!(saved_inputs(&reopened, &run), before);
    assert_eq!(
        read_artifact(&request_path, 256 * 1024).unwrap(),
        prepared.body
    );
    assert_eq!(
        reopened.attempts(&run).unwrap()[0].request_sha256,
        request_hash
    );
    assert_eq!(
        provider::prepare(
            reopened.project_id(),
            &run,
            &request_id,
            &unit,
            &restored_config,
            &[]
        )
        .unwrap()
        .body,
        prepared.body
    );
    let mut restored_budget = BudgetAuthority::open(&fixture.config.usage_ledger).unwrap();
    assert_eq!(
        restored_budget.attempt(&budget_key).unwrap().unwrap().state,
        BudgetAttemptState::Unknown
    );
    let budget_after = serde_json::to_value(restored_budget.status().unwrap()).unwrap();
    for field in [
        "approved_total_tokens",
        "accounted_tokens",
        "pending_reserved_tokens",
        "remaining_tokens",
        "history_count",
        "model_attempts",
    ] {
        assert_eq!(budget_after[field], budget_before[field], "{field}");
    }
    assert_eq!(budget_after["accounted_tokens"], 100);
    assert_eq!(budget_after["pending_reserved_tokens"], 2000);
    assert_eq!(
        fs::read(&fixture.config.usage_ledger).unwrap(),
        fixture.baseline
    );
}
