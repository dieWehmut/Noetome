//! Authored corpus and Chat fixtures verify persistence; no PDF parser or model is run.
use super::*;
use crate::production_budget::{
    BudgetAttemptState, BudgetAuthority, BudgetRequest, BudgetSettlement,
};
use crate::production_compile::provider;
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("noetome-production-store-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    _temp: TempDir,
    project: PathBuf,
    scope: SourceScope,
    pages: Vec<(String, String, String)>,
    config: CompileConfig,
}
impl Fixture {
    fn new(texts: &[(&str, &str)]) -> Self {
        let temp = TempDir::new();
        let project = temp.path().join("project");
        let info = project::initialize(&project).unwrap();
        let root = project.join(".noetome-corpus");
        fs::create_dir(&root).unwrap();
        let id = Uuid::new_v4().to_string();
        fs::create_dir(root.join(&id)).unwrap();
        // This sentinel is explicitly authored input, not evidence of PDF processing.
        let original = b"authored immutable source bytes for Store transaction tests";
        fs::write(root.join(&id).join("raw.pdf"), original).unwrap();
        let scope = SourceScope {
            source_id: id,
            source_sha256: sha256(original),
            display_name: "authored.pdf".into(),
            byte_length: original.len() as u64,
            physical_pages: texts.len() as u32,
        };
        let db = Connection::open(root.join("corpus.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE metadata(singleton INTEGER PRIMARY KEY,project_id TEXT,schema_version INTEGER); CREATE TABLE sources(source_id TEXT PRIMARY KEY,display_name TEXT,selected_path TEXT,content_sha256 TEXT,byte_length INTEGER,status TEXT,page_count INTEGER,created_at TEXT,pause_requested INTEGER,error_code TEXT,error_message TEXT,preflight_json TEXT); CREATE TABLE pages(source_id TEXT,page_number INTEGER,status TEXT,page_json TEXT,page_sha256 TEXT,PRIMARY KEY(source_id,page_number)); PRAGMA application_id=1313817424; PRAGMA user_version=1;").unwrap();
        db.execute("INSERT INTO metadata VALUES(1,?1,1)", [&info.project_id])
            .unwrap();
        db.execute("INSERT INTO sources VALUES(?1,'authored.pdf','explicit-test-selection',?2,?3,'needs_review',?4,'2026-11-16T00:00:00Z',0,NULL,NULL,NULL)", params![scope.source_id,scope.source_sha256,scope.byte_length as i64,scope.physical_pages]).unwrap();
        let mut pages = vec![];
        for (ordinal, (text, status)) in texts.iter().enumerate() {
            let page = json!({"schema_version":1,"page_number":ordinal+1,"source_id":scope.source_id,"status":status,"text":text,"width_pt":612.0,"height_pt":792.0,"geometry":{},"regions":[],"issues":[]});
            let raw = encoded(&page).unwrap();
            let sha = sha256(raw.as_bytes());
            db.execute(
                "INSERT INTO pages VALUES(?1,?2,?3,?4,?5)",
                params![scope.source_id, ordinal as i64 + 1, status, raw, sha],
            )
            .unwrap();
            pages.push((text.to_string(), status.to_string(), sha));
        }
        let config = CompileConfig {
            schema_version: 1,
            endpoint: "http://127.0.0.1:1/v1/chat/completions".into(),
            model: "noetome-test-store".into(),
            credential_mode: None,
            reasoning_effort: None,
            credential_env: "NOETOME_TEST_STORE_KEY".into(),
            transport_mode: "local_http_test".into(),
            usage_ledger: temp.path().join("unused-authority.json"),
            max_chunk_bytes: 1024,
            max_output_tokens: 512,
            request_timeout_ms: 1000,
        };
        Self {
            _temp: temp,
            project,
            scope,
            pages,
            config,
        }
    }
    fn run(&self) -> (Store, String) {
        let store = Store::create(&self.project).unwrap();
        let run = store
            .create_run(&self.config, std::slice::from_ref(&self.scope))
            .unwrap();
        (store, run)
    }
    fn authority(&self) -> BudgetAuthority {
        // An artificial cumulative history proves recovery preserves the opening balance.
        let original = json!({"schema_version":1,"approved_total_tokens":1000000,"accounted_tokens":100,"pending_reserved_tokens":0,"remaining_tokens":999900,"model_attempts":1,"records":[{"invocation_id":"authored-history","reservation_tokens":2000,"confirmed_total_tokens":100,"network_attempts":1,"status":"settled_valid_usage_text"}],"updated_at":"2026-11-16T00:00:00Z"});
        fs::write(
            &self.config.usage_ledger,
            serde_json::to_vec(&original).unwrap(),
        )
        .unwrap();
        BudgetAuthority::migrate(&self.config.usage_ledger).unwrap()
    }
    fn reserve_sent(&self, budget: &mut BudgetAuthority, run: &str, attempt: &Attempt) {
        let key = super::super::key(run, attempt);
        budget
            .reserve(&BudgetRequest {
                key: key.clone(),
                request_sha256: attempt.request_sha256.clone(),
                reserved_tokens: 2000,
            })
            .unwrap();
        budget.mark_sent(&key).unwrap();
    }
    fn unit(&self, number: u32) -> Unit {
        let (text, status, sha) = &self.pages[number as usize - 1];
        Unit {
            unit_id: Uuid::new_v4().to_string(),
            source_id: self.scope.source_id.clone(),
            source_sha256: self.scope.source_sha256.clone(),
            page_number: number,
            page_sha256: sha.clone(),
            bbox: [0.0, 0.0, 612.0, 792.0],
            source_status: status.clone(),
            start_byte: 0,
            end_byte: text.len(),
            text: text.clone(),
            issue_codes: vec![],
        }
    }
    fn prepare_all(&self, store: &Store, run: &str) -> Vec<Unit> {
        self.pages
            .iter()
            .enumerate()
            .filter_map(|(ordinal, (_, status, sha))| {
                let number = ordinal as u32 + 1;
                let units = if matches!(status.as_str(), "text_ready" | "ocr_ready") {
                    vec![self.unit(number)]
                } else {
                    vec![]
                };
                store
                    .save_page(run, &self.scope, number, sha, status, units.clone(), vec![])
                    .unwrap();
                units.into_iter().next()
            })
            .collect()
    }
    fn candidate(
        &self,
        store: &Store,
        run: &str,
        unit: &Unit,
        request: &str,
        contexts: &[ConceptContext],
    ) -> crate::extraction_api_wire::ApiCandidates {
        let prepared = provider::prepare(
            store.project_id(),
            run,
            request,
            unit,
            &self.config,
            contexts,
        )
        .unwrap();
        let concept=contexts.first().map(|c|serde_json::to_value(&c.concept).unwrap()).unwrap_or_else(||json!({"concept_id":"local_concept","revision":1,"canonical_name":"Diffusion","aliases":[],"needs_full_explanation":true}));
        serde_json::from_value(json!({"schema_version":1,"request_id":request,"binding":prepared.binding,"concepts":[concept.clone()],"evidence":[{"evidence_id":"local_evidence","unit_id":unit.unit_id,"excerpt_start_byte":unit.start_byte,"excerpt_end_byte":unit.end_byte,"excerpt_sha256":sha256(unit.text.as_bytes()),"quote":unit.text}],"atoms":[{"atom":{"atom_id":"local_atom","revision":1,"kind":"definition","concept_ids":[concept["concept_id"]],"content":unit.text,"qualifiers":[],"evidence_links":[{"evidence_id":"local_evidence","relation":"supports"}],"required":false,"verification":"unverified","value_status":"not_applicable"},"measurement":null}]})).unwrap()
    }
    fn sealed(
        &self,
        store: &Store,
        run: &str,
        unit: &Unit,
        contexts: &[ConceptContext],
        mutate: impl FnOnce(&mut crate::extraction_api_wire::ApiCandidates),
    ) -> (Attempt, AcceptedBatch) {
        self.originals(store, run, unit, contexts, true, mutate)
    }
    fn originals(
        &self,
        store: &Store,
        run: &str,
        unit: &Unit,
        contexts: &[ConceptContext],
        seal: bool,
        mutate: impl FnOnce(&mut crate::extraction_api_wire::ApiCandidates),
    ) -> (Attempt, AcceptedBatch) {
        let request = Uuid::new_v4().to_string();
        let prepared = provider::prepare(
            store.project_id(),
            run,
            &request,
            unit,
            &self.config,
            contexts,
        )
        .unwrap();
        let mut candidate = self.candidate(store, run, unit, &request, contexts);
        mutate(&mut candidate);
        let response=serde_json::to_vec(&json!({"id":"chatcmpl-authored","object":"chat.completion","model":self.config.model,"choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":encoded(&candidate).unwrap()}}],"usage":{"prompt_tokens":100,"completion_tokens":50,"total_tokens":150}})).unwrap();
        let mut attempt = store
            .begin_attempt(run, unit, &request, &sha256(&prepared.body))
            .unwrap();
        let directory = store.root.join(&attempt.directory);
        fs::write(directory.join("request-body.json"), &prepared.body).unwrap();
        fs::write(directory.join("response.bin"), &response).unwrap();
        let receipt = provider::Receipt {
            schema_version: 1,
            request_id: request,
            request_sha256: attempt.request_sha256.clone(),
            status: Some(200),
            network_attempts: 1,
            complete: true,
            response_sha256: sha256(&response),
            response_bytes: response.len() as u64,
            failure_code: None,
        };
        let receipt = serde_json::to_vec(&receipt).unwrap();
        fs::write(directory.join("receipt.json"), &receipt).unwrap();
        attempt.state = if seal { "response_saved" } else { "sent" }.into();
        if seal {
            attempt.response_sha256 = Some(sha256(&response));
            attempt.receipt_sha256 = Some(sha256(&receipt));
        }
        store.update_attempt(run, &attempt).unwrap();
        (
            attempt,
            AcceptedBatch {
                candidates: candidate,
                allowed_existing_concepts: contexts.iter().map(|c| c.concept.clone()).collect(),
            },
        )
    }
}

// Preserve unsealed originals to reproduce a crash after receipt persistence.
fn rewrite_authored_receipt(
    store: &Store,
    attempt: &Attempt,
    input: u64,
    output: u64,
    status: u16,
    complete: bool,
    failure: Option<&str>,
) {
    let directory = store.root.join(&attempt.directory);
    let mut response: Value =
        serde_json::from_slice(&fs::read(directory.join("response.bin")).unwrap()).unwrap();
    response["usage"] = json!({
        "prompt_tokens": input,
        "completion_tokens": output,
        "total_tokens": input + output,
    });
    let bytes = serde_json::to_vec(&response).unwrap();
    fs::write(directory.join("response.bin"), &bytes).unwrap();
    let mut receipt: provider::Receipt =
        serde_json::from_slice(&fs::read(directory.join("receipt.json")).unwrap()).unwrap();
    receipt.status = Some(status);
    receipt.complete = complete;
    receipt.failure_code = failure.map(str::to_string);
    receipt.response_sha256 = sha256(&bytes);
    receipt.response_bytes = bytes.len() as u64;
    fs::write(
        directory.join("receipt.json"),
        serde_json::to_vec(&receipt).unwrap(),
    )
    .unwrap();
}

fn saved_recovery_fixture(outcomes: &[&str]) -> (Fixture, String, Vec<Attempt>) {
    saved_recovery_fixture_with_transport(outcomes, "local_http_test")
}

fn saved_recovery_fixture_with_transport(
    outcomes: &[&str],
    transport_mode: &str,
) -> (Fixture, String, Vec<Attempt>) {
    let texts = vec![
        (
            "Diffusion retains authored recovery evidence.",
            "text_ready"
        );
        outcomes.len()
    ];
    let mut fixture = Fixture::new(&texts);
    fixture.config.transport_mode = transport_mode.into();
    if transport_mode == "local_openai" {
        fixture.config.credential_mode = Some("none".into());
        fixture.config.credential_env.clear();
        fixture.config.usage_ledger = fixture
            .config
            .usage_ledger
            .with_file_name("local-openai.usage.json");
    }
    let mut budget = fixture.authority();
    let (store, run) = fixture.run();
    let mut attempts = Vec::new();
    for (ordinal, outcome) in outcomes.iter().enumerate() {
        let number = ordinal as u32 + 1;
        let mut unit = fixture.unit(number);
        // Recovery currently orders by unit_id, so the fixture must not depend on random UUID order.
        unit.unit_id = format!("00000000-0000-4000-8000-{number:012}");
        store
            .save_page(
                &run,
                &fixture.scope,
                number,
                &fixture.pages[ordinal].2,
                "text_ready",
                vec![unit.clone()],
                vec![],
            )
            .unwrap();
        let (attempt, _) = fixture.originals(&store, &run, &unit, &[], false, |candidate| {
            if *outcome == "candidate_failure" {
                candidate.atoms[0].atom.required = true;
            }
        });
        match *outcome {
            "request_failure" => rewrite_authored_receipt(
                &store,
                &attempt,
                100,
                50,
                500,
                true,
                Some("EXTRACT_API_SERVER"),
            ),
            "budget_failure" => {
                rewrite_authored_receipt(&store, &attempt, 100, 513, 200, true, None)
            }
            "paused" => rewrite_authored_receipt(
                &store,
                &attempt,
                100,
                50,
                200,
                false,
                Some("COMPILE_PAUSED"),
            ),
            "changed_receipt" => {
                fs::write(
                    store.root.join(&attempt.directory).join("response.bin"),
                    b"authored response changed after receipt persistence",
                )
                .unwrap();
            }
            "candidate_failure" | "success" => {}
            other => panic!("unknown authored recovery outcome: {other}"),
        }
        fixture.reserve_sent(&mut budget, &run, &attempt);
        attempts.push(attempt);
    }
    drop(store);
    drop(budget);
    (fixture, run, attempts)
}

#[test]
fn page_cursor_preserves_fixed_inputs_and_distinguishes_ocr_and_unprepared() {
    let fixture = Fixture::new(&[
        ("Diffusion follows a concentration gradient.", "text_ready"),
        ("", "needs_ocr"),
        ("另一个固定段落。", "ocr_ready"),
    ]);
    let (store, run) = fixture.run();
    assert_eq!(store.summary(&run).unwrap().unresolved_pages, 3);
    assert_eq!(store.next_unprepared_page(&run).unwrap().unwrap().1, 1);
    let unit = fixture.unit(1);
    let sha = &fixture.pages[0].2;
    store
        .save_page(
            &run,
            &fixture.scope,
            1,
            sha,
            "text_ready",
            vec![unit.clone()],
            vec![],
        )
        .unwrap();
    store
        .save_page(
            &run,
            &fixture.scope,
            1,
            sha,
            "text_ready",
            vec![unit.clone()],
            vec![],
        )
        .unwrap();
    let mut changed = unit.clone();
    changed.text.push('x');
    assert!(
        store
            .save_page(
                &run,
                &fixture.scope,
                1,
                sha,
                "text_ready",
                vec![changed],
                vec![]
            )
            .is_err()
    );
    assert_eq!(store.next_unprepared_page(&run).unwrap().unwrap().1, 2);
    store
        .save_page(
            &run,
            &fixture.scope,
            2,
            &fixture.pages[1].2,
            "needs_ocr",
            vec![],
            vec![json!({"code":"NEEDS_OCR"})],
        )
        .unwrap();
    let unit3 = fixture.unit(3);
    let mut truncated = unit3.clone();
    truncated.end_byte -= 3;
    truncated.text.truncate(truncated.end_byte);
    assert!(
        store
            .save_page(
                &run,
                &fixture.scope,
                3,
                &fixture.pages[2].2,
                "ocr_ready",
                vec![truncated],
                vec![]
            )
            .is_err()
    );
    let mut wrong_box = unit3.clone();
    wrong_box.bbox = [0.0; 4];
    assert!(
        store
            .save_page(
                &run,
                &fixture.scope,
                3,
                &fixture.pages[2].2,
                "ocr_ready",
                vec![wrong_box],
                vec![]
            )
            .is_err()
    );
    store
        .save_page(
            &run,
            &fixture.scope,
            3,
            &fixture.pages[2].2,
            "ocr_ready",
            vec![unit3],
            vec![],
        )
        .unwrap();
    assert!(store.next_unprepared_page(&run).unwrap().is_none());
    let summary = store.summary(&run).unwrap();
    assert_eq!(
        (
            summary.prepared_pages,
            summary.pending_ocr_pages,
            summary.unresolved_pages,
            summary.units_total
        ),
        (3, 1, 3, 2)
    );
    assert_eq!(
        store.next_unit(&run).unwrap().unwrap().unit_id,
        unit.unit_id
    );
    assert_eq!(
        corpus::verify_source(&fixture.project, &fixture.scope.source_id)
            .unwrap()
            .content_sha256,
        Some(fixture.scope.source_sha256.clone())
    );
}

#[test]
fn confirmed_batch_maps_ids_atomically_and_reopens_without_duplicates() {
    let fixture = Fixture::new(&[("Diffusion follows a concentration gradient.", "text_ready")]);
    let (store, run) = fixture.run();
    let unit = fixture.prepare_all(&store, &run).remove(0);
    let (attempt, batch) = fixture.sealed(&store, &run, &unit, &[], |_| {});
    let repeat = AcceptedBatch {
        candidates: batch.candidates.clone(),
        allowed_existing_concepts: vec![],
    };
    store.commit_batch(&run, &unit, &attempt, batch).unwrap();
    store.commit_batch(&run, &unit, &attempt, repeat).unwrap();
    let concepts = store.query(&run, "concepts", 0, 50, "").unwrap();
    let atoms = store.query(&run, "atoms", 0, 50, "").unwrap();
    let evidence = store.query(&run, "evidence", 0, 50, "").unwrap();
    for (kind, field) in [
        (&concepts, "concept_id"),
        (&atoms, "atom_id"),
        (&evidence, "evidence_id"),
    ] {
        assert_eq!(kind["total"], 1);
        uuid(kind["items"][0][field].as_str().unwrap()).unwrap();
    }
    assert_eq!(
        atoms["items"][0]["concept_ids"][0],
        concepts["items"][0]["concept_id"]
    );
    assert_eq!(
        atoms["items"][0]["evidence_links"][0]["evidence_id"],
        evidence["items"][0]["evidence_id"]
    );
    assert_eq!(atoms["items"][0]["verification"], "unverified");
    assert_eq!(evidence["items"][0]["page_sha256"], unit.page_sha256);
    assert_eq!(evidence["items"][0]["quote"], unit.text);
    assert_eq!(
        evidence["items"][0]["bbox"],
        json!([0.0, 0.0, 612.0, 792.0])
    );
    assert_eq!(store.summary(&run).unwrap().ir_revision, 1);
    assert!(store.next_unit(&run).unwrap().is_none());
    assert!(store.attempts_needing_recovery(&run).unwrap().is_empty());
    store
        .mark_unit_failed(
            &run,
            &unit.unit_id,
            "LATE_ERROR",
            "must not undo accepted knowledge",
        )
        .unwrap();
    assert_eq!(store.attempts(&run).unwrap()[0].state, "accepted");
    assert_eq!(store.summary(&run).unwrap().units_confirmed, 1);
    let id = evidence["items"][0]["evidence_id"].as_str().unwrap();
    assert_eq!(store.evidence(&run, id).unwrap(), evidence["items"][0]);
    drop(store);
    let reopened = Store::open(&fixture.project).unwrap().unwrap();
    assert_eq!(
        reopened.query(&run, "concepts", 0, 50, "").unwrap(),
        concepts
    );
    assert_eq!(reopened.query(&run, "atoms", 0, 50, "").unwrap(), atoms);
    assert_eq!(reopened.summary(&run).unwrap().units_confirmed, 1);
}

#[test]
fn same_name_is_independent_unless_request_explicitly_supplies_existing_semantics() {
    let fixture = Fixture::new(&[
        ("Diffusion first meaning.", "text_ready"),
        ("Diffusion second meaning.", "text_ready"),
        ("Diffusion explicit same meaning.", "text_ready"),
    ]);
    let (store, run) = fixture.run();
    let units = fixture.prepare_all(&store, &run);
    for unit in &units[..2] {
        let (a, b) = fixture.sealed(&store, &run, unit, &[], |_| {});
        store.commit_batch(&run, unit, &a, b).unwrap();
    }
    assert_eq!(
        store.query(&run, "concepts", 0, 50, "").unwrap()["total"],
        2
    );
    assert_eq!(
        store
            .query(&run, "issues", 0, 50, "CONCEPT_IDENTITY_UNRESOLVED")
            .unwrap()["total"],
        1
    );
    let contexts = store.context_concepts(&run, "Diffusion", 1).unwrap();
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0].sample_atoms.len(), 1);
    let (a, b) = fixture.sealed(&store, &run, &units[2], &contexts, |_| {});
    store.commit_batch(&run, &units[2], &a, b).unwrap();
    assert_eq!(
        store.query(&run, "concepts", 0, 50, "").unwrap()["total"],
        2
    );
    assert_eq!(store.summary(&run).unwrap().atoms, 3);
    assert_eq!(
        store.context_concepts(&run, "Diffusion", 1).unwrap()[0]
            .sample_atoms
            .len(),
        2
    );
    assert!(store.context_concepts(&run, "Diffusion", 17).is_err());
    assert_eq!(
        store.query(&run, "atoms", 1, 1, "Diffusion").unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for limit in [0, 51] {
        assert!(store.query(&run, "atoms", 0, limit, "").is_err());
    }
    assert!(store.query(&run, "runs", 0, 1, "").is_err());
}

#[test]
fn malformed_late_atom_and_edited_originals_do_not_leave_partial_ir() {
    for defect in ["atom", "request", "response", "receipt", "context"] {
        let fixture =
            Fixture::new(&[("Diffusion follows its documented conditions.", "text_ready")]);
        let (store, run) = fixture.run();
        let unit = fixture.prepare_all(&store, &run).remove(0);
        let (a, mut b) = fixture.sealed(&store, &run, &unit, &[], |c| {
            if defect == "atom" {
                let mut late = c.atoms[0].clone();
                late.atom.atom_id = "bad_late".into();
                late.atom.verification = domain::Verification::FixtureReviewed;
                c.atoms.push(late);
            }
        });
        if matches!(defect, "request" | "response" | "receipt") {
            let name = match defect {
                "request" => "request-body.json",
                "response" => "response.bin",
                _ => "receipt.json",
            };
            let path = store.root.join(&a.directory).join(name);
            let mut bytes = fs::read(&path).unwrap();
            bytes.push(b' ');
            fs::write(path, bytes).unwrap();
        }
        if defect == "context" {
            b.allowed_existing_concepts = b.candidates.concepts.clone();
        }
        assert!(store.commit_batch(&run, &unit, &a, b).is_err(), "{defect}");
        let s = store.summary(&run).unwrap();
        assert_eq!(
            (
                s.ir_revision,
                s.concepts,
                s.atoms,
                s.evidence,
                s.units_confirmed
            ),
            (0, 0, 0, 0, 0),
            "{defect}"
        );
        assert_eq!(store.attempts_needing_recovery(&run).unwrap().len(), 1);
    }
}

#[test]
fn controls_and_attempt_limit_keep_unknown_history_and_cancel_is_terminal() {
    let fixture = Fixture::new(&[("Diffusion text.", "text_ready")]);
    let (store, run) = fixture.run();
    let unit = fixture.prepare_all(&store, &run).remove(0);
    store.set_status(&run, "running", "extract").unwrap();
    assert_eq!(store.summary(&run).unwrap().status, "interrupted");
    let lease = store.lease(&run).unwrap();
    assert_eq!(store.summary(&run).unwrap().status, "running");
    assert!(store.lease(&run).is_err());
    store.pause(&run).unwrap();
    assert!(store.should_stop(&run).unwrap());
    assert_eq!(store.summary(&run).unwrap().status, "paused");
    store.set_status(&run, "running", "extracting").unwrap();
    assert!(
        store.should_stop(&run).unwrap(),
        "stage updates must not erase a concurrent pause"
    );
    store.resume(&run).unwrap();
    assert!(!store.should_stop(&run).unwrap());
    let mut first = store
        .begin_attempt(&run, &unit, &Uuid::new_v4().to_string(), &"a".repeat(64))
        .unwrap();
    first.state = "unknown".into();
    store.update_attempt(&run, &first).unwrap();
    assert_eq!(
        store
            .begin_attempt(&run, &unit, &Uuid::new_v4().to_string(), &"b".repeat(64))
            .unwrap_err()
            .code(),
        "COMPILE_ATTEMPT_RECOVERY_REQUIRED"
    );
    store
        .mark_unit_failed(&run, &unit.unit_id, "AUTHORED_FAILURE", "known failure")
        .unwrap();
    assert!(store.attempts_needing_recovery(&run).unwrap().is_empty());
    assert_eq!(store.attempts(&run).unwrap()[0].state, "failed");
    store.retry_failed(&run).unwrap();
    let mut second = store
        .begin_attempt(&run, &unit, &Uuid::new_v4().to_string(), &"b".repeat(64))
        .unwrap();
    assert_eq!(second.attempt_no, 2);
    second.state = "failed".into();
    store.update_attempt(&run, &second).unwrap();
    store
        .mark_unit_failed(
            &run,
            &unit.unit_id,
            "SECOND_FAILURE",
            "known second failure",
        )
        .unwrap();
    store.retry_failed(&run).unwrap();
    assert!(store.next_unit(&run).unwrap().is_none());
    assert_eq!(store.attempts(&run).unwrap().len(), 2);
    store.cancel(&run).unwrap();
    assert!(store.resume(&run).is_err());
    assert!(store.should_stop(&run).unwrap());
    drop(lease);
}

#[test]
fn database_abort_after_first_atom_rolls_back_entities_batch_and_revision_together() {
    let fixture = Fixture::new(&[("Diffusion follows its documented conditions.", "text_ready")]);
    let (store, run) = fixture.run();
    let unit = fixture.prepare_all(&store, &run).remove(0);
    let (attempt, batch) = fixture.sealed(&store, &run, &unit, &[], |c| {
        let mut second = c.atoms[0].clone();
        second.atom.atom_id = "second_local_atom".into();
        c.atoms.push(second);
    });
    store.connection.execute_batch("CREATE TRIGGER authored_abort BEFORE INSERT ON atoms WHEN (SELECT count(*) FROM atoms)=1 BEGIN SELECT RAISE(ABORT,'authored late storage failure'); END;").unwrap();
    assert!(store.commit_batch(&run, &unit, &attempt, batch).is_err());
    let s = store.summary(&run).unwrap();
    assert_eq!(
        (
            s.ir_revision,
            s.concepts,
            s.atoms,
            s.evidence,
            s.units_confirmed
        ),
        (0, 0, 0, 0, 0)
    );
    assert_eq!(
        store
            .connection
            .query_row("SELECT count(*) FROM batches", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        store.attempts_needing_recovery(&run).unwrap()[0].state,
        "response_saved"
    );
}

#[test]
fn changed_saved_semantics_are_rejected_for_queries_and_reuse() {
    let fixture = Fixture::new(&[
        ("Diffusion first meaning.", "text_ready"),
        ("Diffusion reused meaning.", "text_ready"),
    ]);
    let (store, run) = fixture.run();
    let units = fixture.prepare_all(&store, &run);
    let (a, b) = fixture.sealed(&store, &run, &units[0], &[], |_| {});
    store.commit_batch(&run, &units[0], &a, b).unwrap();
    let context = store.context_concepts(&run, "Diffusion", 1).unwrap();
    let (a, b) = fixture.sealed(&store, &run, &units[1], &context, |_| {});
    store
        .connection
        .execute(
            "UPDATE concepts SET sha256=?1 WHERE run_id=?2",
            params!["0".repeat(64), run],
        )
        .unwrap();
    assert!(store.query(&run, "concepts", 0, 50, "").is_err());
    assert!(store.context_concepts(&run, "Diffusion", 1).is_err());
    assert!(store.commit_batch(&run, &units[1], &a, b).is_err());
    assert_eq!(store.summary(&run).unwrap().units_confirmed, 1);
}

#[test]
fn stored_configuration_scope_and_measurement_bytes_are_pinned() {
    for field in ["config", "scope", "measurement"] {
        let fixture = Fixture::new(&[("Diffusion first meaning.", "text_ready")]);
        let (store, run) = fixture.run();
        let unit = fixture.prepare_all(&store, &run).remove(0);
        match field {
            "config" => {
                store
                    .connection
                    .execute(
                        "UPDATE runs SET config_json=config_json||' ' WHERE run_id=?1",
                        [&run],
                    )
                    .unwrap();
                assert!(store.config(&run).is_err());
            }
            "scope" => {
                store
                    .connection
                    .execute(
                        "UPDATE sources SET scope_json=scope_json||' ' WHERE run_id=?1",
                        [&run],
                    )
                    .unwrap();
                assert!(store.summary(&run).is_err());
                assert!(store.next_unprepared_page(&run).is_err());
            }
            _ => {
                let (a, b) = fixture.sealed(&store, &run, &unit, &[], |_| {});
                store.commit_batch(&run, &unit, &a, b).unwrap();
                store
                    .connection
                    .execute(
                        "UPDATE atoms SET measurement_json='null ' WHERE run_id=?1",
                        [&run],
                    )
                    .unwrap();
                assert!(store.query(&run, "atoms", 0, 50, "").is_err());
            }
        }
    }
}

#[test]
fn completed_receipt_without_saved_pins_recovers_budget_and_one_stable_batch() {
    let fixture = Fixture::new(&[("Diffusion recovered original meaning.", "text_ready")]);
    let (store, run) = fixture.run();
    let unit = fixture.prepare_all(&store, &run).remove(0);
    let mut budget = fixture.authority();
    let (attempt, _) = fixture.originals(&store, &run, &unit, &[], false, |_| {});
    fixture.reserve_sent(&mut budget, &run, &attempt);
    assert!(attempt.response_sha256.is_none());
    assert!(attempt.receipt_sha256.is_none());
    assert_eq!(budget.status().unwrap().pending_reserved_tokens, 2000);
    super::super::finish(
        &fixture.project,
        &run,
        &store,
        &fixture.config,
        &mut budget,
        &unit,
        &attempt,
        "noetome-test-artificial-credential-never-in-source",
    )
    .unwrap();
    let summary = store.summary(&run).unwrap();
    assert_eq!(
        (
            summary.ir_revision,
            summary.units_confirmed,
            summary.concepts,
            summary.atoms,
            summary.evidence
        ),
        (1, 1, 1, 1, 1)
    );
    let status = budget.status().unwrap();
    assert_eq!(
        (
            status.accounted_tokens,
            status.pending_reserved_tokens,
            status.model_attempts
        ),
        (250, 0, 2)
    );
    let saved = store.attempts(&run).unwrap().remove(0);
    assert_eq!(saved.state, "accepted");
    assert!(saved.response_sha256.is_some());
    assert!(saved.receipt_sha256.is_some());
    let concepts = store.query(&run, "concepts", 0, 50, "").unwrap();
    let balance = encoded(&status).unwrap();
    // Accepted requests are absent from ordinary resume; a private duplicate cannot demote them.
    assert!(store.attempts_needing_recovery(&run).unwrap().is_empty());
    assert!(
        super::super::finish(
            &fixture.project,
            &run,
            &store,
            &fixture.config,
            &mut budget,
            &unit,
            &saved,
            "noetome-test-artificial-credential-never-in-source"
        )
        .is_err()
    );
    assert_eq!(store.query(&run, "concepts", 0, 50, "").unwrap(), concepts);
    assert_eq!(encoded(&budget.status().unwrap()).unwrap(), balance);
    drop(store);
    drop(budget);
    let reopened = Store::open(&fixture.project).unwrap().unwrap();
    let mut authority = BudgetAuthority::open(&fixture.config.usage_ledger).unwrap();
    assert!(reopened.attempts_needing_recovery(&run).unwrap().is_empty());
    assert_eq!(
        reopened.query(&run, "concepts", 0, 50, "").unwrap(),
        concepts
    );
    assert_eq!(encoded(&authority.status().unwrap()).unwrap(), balance);
}

#[test]
fn already_settled_receipt_before_ir_commit_replays_without_second_charge() {
    let fixture = Fixture::new(&[("Diffusion replayed original meaning.", "text_ready")]);
    let (store, run) = fixture.run();
    let unit = fixture.prepare_all(&store, &run).remove(0);
    let mut budget = fixture.authority();
    let (attempt, _) = fixture.sealed(&store, &run, &unit, &[], |_| {});
    fixture.reserve_sent(&mut budget, &run, &attempt);
    let response = fs::read(store.root.join(&attempt.directory).join("response.bin")).unwrap();
    let settlement = BudgetSettlement {
        network_attempts: 1,
        usage: provider::usage(&response),
        receipt_sha256: attempt.receipt_sha256.clone().unwrap(),
    };
    let key = super::super::key(&run, &attempt);
    budget.settle(&key, &settlement).unwrap();
    let paid = encoded(&budget.status().unwrap()).unwrap();
    assert_eq!(store.summary(&run).unwrap().ir_revision, 0);
    drop(store);
    drop(budget);
    // Restart after the ledger commit, before the knowledge transaction.
    let store = Store::open(&fixture.project).unwrap().unwrap();
    let mut budget = BudgetAuthority::open(&fixture.config.usage_ledger).unwrap();
    let recover = store.attempts_needing_recovery(&run).unwrap().remove(0);
    super::super::finish(
        &fixture.project,
        &run,
        &store,
        &fixture.config,
        &mut budget,
        &unit,
        &recover,
        "noetome-test-artificial-credential-never-in-source",
    )
    .unwrap();
    assert_eq!(encoded(&budget.status().unwrap()).unwrap(), paid);
    assert_eq!(store.summary(&run).unwrap().ir_revision, 1);
    assert_eq!(
        budget.attempt(&key).unwrap().unwrap().state,
        BudgetAttemptState::Known
    );
    budget.settle(&key, &settlement).unwrap();
    assert_eq!(encoded(&budget.status().unwrap()).unwrap(), paid);
    assert!(store.attempts_needing_recovery(&run).unwrap().is_empty());
    assert_eq!(store.query(&run, "atoms", 0, 50, "").unwrap()["total"], 1);
}

#[test]
fn pausing_global_failure_preserves_stop_and_only_explicit_retry_clears_it() {
    let fixture = Fixture::new(&[(
        "Diffusion retained after authentication failure.",
        "text_ready",
    )]);
    let (store, run) = fixture.run();
    let unit = fixture.prepare_all(&store, &run).remove(0);
    store
        .mark_unit_failed(
            &run,
            &unit.unit_id,
            "COMPILE_REQUEST_FAILED",
            "authored auth failure",
        )
        .unwrap();
    store
        .set_error(&run, "COMPILE_REQUEST_FAILED", "authored auth failure")
        .unwrap();
    store.set_status(&run, "failed", "extracting").unwrap();
    store.pause(&run).unwrap();
    let stopped = store.summary(&run).unwrap();
    assert_eq!(stopped.status, "failed");
    assert_eq!(stopped.error.unwrap().code, "COMPILE_REQUEST_FAILED");
    assert!(store.should_stop(&run).unwrap());
    let mut authority = fixture.authority();
    let before = encoded(&authority.status().unwrap()).unwrap();
    let resumed = super::super::run(
        &fixture.project,
        &run,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .unwrap();
    assert_eq!(resumed.status, "failed");
    assert_eq!(resumed.error.unwrap().code, "COMPILE_REQUEST_FAILED");
    assert_eq!(encoded(&authority.status().unwrap()).unwrap(), before);
    assert_eq!(
        store.summary(&run).unwrap().error.unwrap().message,
        "authored auth failure"
    );
    store.retry_failed(&run).unwrap();
    let retry = store.summary(&run).unwrap();
    assert_eq!(retry.status, "queued");
    assert!(retry.error.is_none());
    assert!(!store.should_stop(&run).unwrap());
    store
        .set_error(&run, "COMPILE_BUDGET", "authored budget stop")
        .unwrap();
    store.set_status(&run, "budget_stop", "extracting").unwrap();
    store.pause(&run).unwrap();
    assert_eq!(store.summary(&run).unwrap().status, "budget_stop");
    store.cancel(&run).unwrap();
    assert_eq!(store.summary(&run).unwrap().status, "cancelled");
}

#[test]
fn second_store_watchers_close_without_losing_first_connection_controls_or_writes() {
    let fixture = Fixture::new(&[(
        "Diffusion controlled across live connections.",
        "text_ready",
    )]);
    let (store, run) = fixture.run();
    fixture.prepare_all(&store, &run);
    let _lease = store.lease(&run).unwrap();
    let project = fixture.project.clone();
    let observed_run = run.clone();
    std::thread::spawn(move || {
        for _ in 0..5 {
            let watcher = Store::open(&project).unwrap().unwrap();
            assert!(!watcher.should_stop(&observed_run).unwrap());
        }
        let control = Store::open(&project).unwrap().unwrap();
        control.pause(&observed_run).unwrap();
    })
    .join()
    .unwrap();
    assert!(store.should_stop(&run).unwrap());
    for index in 0..5 {
        store.resume(&run).unwrap();
        store
            .set_error(
                &run,
                "AUTHORED_WATCHER_WRITE",
                &format!("persistent write {index}"),
            )
            .unwrap();
        let project = fixture.project.clone();
        let observed_run = run.clone();
        let value = std::thread::spawn(move || {
            Store::open(&project)
                .unwrap()
                .unwrap()
                .summary(&observed_run)
                .unwrap()
                .error
                .unwrap()
                .message
        })
        .join()
        .unwrap();
        assert_eq!(value, format!("persistent write {index}"));
    }
    assert!(store.root.join("compile.sqlite-wal").is_file());
    assert!(store.root.join("compile.sqlite-shm").is_file());
}

#[test]
fn external_sqlite_control_keeps_live_wal_mapping_and_first_connection_sees_flags() {
    let fixture = Fixture::new(&[(
        "Diffusion controlled from an independent process.",
        "text_ready",
    )]);
    let (store, run) = fixture.run();
    fixture.prepare_all(&store, &run);
    let _lease = store.lease(&run).unwrap();
    let wal = store.root.join("compile.sqlite-wal");
    let shm = store.root.join("compile.sqlite-shm");
    let wal_identity = sqlite_identity(&wal).unwrap();
    let shm_identity = sqlite_identity(&shm).unwrap();
    // Repeated status checks previously opened/closed live DB and SHM files,
    // releasing POSIX locks and letting this controller delete the worker's WAL.
    for _ in 0..3 {
        assert!(!store.should_stop(&run).unwrap());
    }
    let python = std::env::var_os("NOETOME_TEST_PYTHON").unwrap_or_else(|| "python".into());
    let controller=std::process::Command::new(python).args(["-I","-B","-c","import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute('UPDATE runs SET pause_requested=1 WHERE run_id=?',(sys.argv[2],)); c.commit(); c.close()"]).arg(store.root.join("compile.sqlite")).arg(&run).output().unwrap();
    assert!(
        controller.status.success(),
        "independent SQLite controller: {}",
        String::from_utf8_lossy(&controller.stderr)
    );
    assert!(
        store.should_stop(&run).unwrap(),
        "live worker must see the controller's committed flag"
    );
    assert_eq!(sqlite_identity(&wal).unwrap(), wal_identity);
    assert_eq!(sqlite_identity(&shm).unwrap(), shm_identity);
    store.resume(&run).unwrap();
    assert!(!store.should_stop(&run).unwrap());
    store
        .set_error(
            &run,
            "AFTER_EXTERNAL_CONTROL",
            "first connection still writes same database",
        )
        .unwrap();
    assert_eq!(
        Store::open(&fixture.project)
            .unwrap()
            .unwrap()
            .summary(&run)
            .unwrap()
            .error
            .unwrap()
            .code,
        "AFTER_EXTERNAL_CONTROL"
    );
}

#[cfg(unix)]
#[test]
fn held_guard_rejects_wrong_kind_and_nonordinary_socket_before_opening() {
    let temp = TempDir::new();
    let file = temp.path().join("ordinary");
    fs::write(&file, b"ordinary").unwrap();
    let directory = temp.path().join("directory");
    fs::create_dir(&directory).unwrap();
    assert_eq!(
        guarded(&file, false, true).unwrap_err().code(),
        "PROJECT_UNSAFE_PATH"
    );
    assert_eq!(
        guarded(&directory, false, false).unwrap_err().code(),
        "PROJECT_UNSAFE_PATH"
    );
    let socket = temp.path().join("socket");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    assert_eq!(
        guarded(&socket, false, false).unwrap_err().code(),
        "PROJECT_UNSAFE_PATH"
    );
}

#[test]
fn scope_queries_include_unprepared_pages_and_count_pending_or_failed_page_once() {
    let fixture = Fixture::new(&[
        ("Diffusion first meaning.", "text_ready"),
        ("Diffusion second meaning.", "needs_review"),
        ("", "blank"),
    ]);
    let (store, run) = fixture.run();
    let initial = store.query(&run, "pages", 0, 2, "").unwrap();
    assert_eq!(initial["total"], 3);
    assert_eq!(initial["items"].as_array().unwrap().len(), 2);
    assert_eq!(initial["items"][0]["status"], "unprepared");
    assert!(initial["items"][0]["page_sha256"].is_null());
    let first = fixture.unit(1);
    let mut second = fixture.unit(2);
    second.issue_codes.push("SOURCE_REVIEW".into());
    store
        .save_page(
            &run,
            &fixture.scope,
            1,
            &fixture.pages[0].2,
            "text_ready",
            vec![first.clone()],
            vec![],
        )
        .unwrap();
    store
        .save_page(
            &run,
            &fixture.scope,
            2,
            &fixture.pages[1].2,
            "needs_review",
            vec![second.clone()],
            vec![json!({"code":"SOURCE_REVIEW"})],
        )
        .unwrap();
    store
        .save_page(
            &run,
            &fixture.scope,
            3,
            &fixture.pages[2].2,
            "blank",
            vec![],
            vec![],
        )
        .unwrap();
    assert_eq!(store.summary(&run).unwrap().unresolved_pages, 2);
    let (a, b) = fixture.sealed(&store, &run, &first, &[], |_| {});
    store.commit_batch(&run, &first, &a, b).unwrap();
    store
        .mark_unit_failed(
            &run,
            &second.unit_id,
            "AUTHORED_FAILURE",
            "failed comparison",
        )
        .unwrap();
    assert_eq!(
        store.summary(&run).unwrap().unresolved_pages,
        1,
        "source review and failed unit describe the same page"
    );
    let pages = store.query(&run, "pages", 1, 50, "").unwrap();
    assert_eq!(pages["items"][0]["page_number"], 2);
    assert_eq!(pages["items"][0]["units_failed"], 1);
    assert_eq!(pages["items"][1]["status"], "blank");
    let units = store.query(&run, "units", 0, 50, "").unwrap();
    assert_eq!(units["total"], 2);
    assert_eq!(units["items"][0]["status"], "confirmed");
    assert_eq!(units["items"][1]["status"], "failed");
    assert!(units["items"][0].get("text").is_none());
    assert_eq!(units["items"][1]["issue_codes"], json!(["SOURCE_REVIEW"]));
    assert_eq!(
        units["items"][0]["excerpt_sha256"],
        sha256(first.text.as_bytes())
    );
    assert!(store.query(&run, "pages", 0, 1, "Diffusion").is_err());
}

#[test]
fn foreign_root_database_and_attempt_replacements_are_rejected() {
    let fixture = Fixture::new(&[("Diffusion text.", "text_ready")]);
    let rogue = fixture.project.join(".noetome-compile");
    fs::create_dir(&rogue).unwrap();
    fs::write(rogue.join("valuable.txt"), b"preserve").unwrap();
    assert!(Store::create(&fixture.project).is_err());
    assert_eq!(fs::read(rogue.join("valuable.txt")).unwrap(), b"preserve");
    fs::remove_file(rogue.join("valuable.txt")).unwrap();
    fs::remove_dir(rogue).unwrap();
    let (store, run) = fixture.run();
    let unit = fixture.prepare_all(&store, &run).remove(0);
    let a = store
        .begin_attempt(&run, &unit, &Uuid::new_v4().to_string(), &"a".repeat(64))
        .unwrap();
    let path = store.root.join(&a.directory);
    fs::rename(&path, path.with_extension("retained")).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(store.update_attempt(&run, &a).is_err());
    drop(store);
    let db = fixture.project.join(".noetome-compile/compile.sqlite");
    let saved = db.with_extension("retained");
    fs::rename(&db, &saved).unwrap();
    fs::copy(&saved, &db).unwrap();
    assert!(Store::open(&fixture.project).is_err());
}

#[cfg(unix)]
#[test]
fn real_symlinks_hardlinks_and_wal_redirection_reject_without_target_writes() {
    use std::os::unix::fs::symlink;
    for alias in ["root", "database", "wal"] {
        let fixture = Fixture::new(&[("Diffusion text.", "text_ready")]);
        let outside = fixture._temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let valuable = outside.join("valuable");
        fs::write(&valuable, b"do not touch").unwrap();
        if alias == "root" {
            symlink(&outside, fixture.project.join(".noetome-compile")).unwrap();
        } else {
            let (store, _) = fixture.run();
            drop(store);
            let root = fixture.project.join(".noetome-compile");
            if alias == "database" {
                fs::remove_file(root.join("compile.sqlite")).unwrap();
                fs::hard_link(&valuable, root.join("compile.sqlite")).unwrap();
            } else {
                symlink(&valuable, root.join("compile.sqlite-wal")).unwrap();
            }
        }
        assert!(Store::create(&fixture.project).is_err(), "{alias}");
        assert_eq!(fs::read(&valuable).unwrap(), b"do not touch");
    }
}

#[cfg(windows)]
#[test]
fn native_directory_junction_and_database_hardlink_reject_without_target_writes() {
    for alias in ["junction", "hardlink"] {
        let fixture = Fixture::new(&[("Diffusion text.", "text_ready")]);
        let outside = fixture._temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let valuable = outside.join("valuable");
        fs::write(&valuable, b"do not touch").unwrap();
        let root = fixture.project.join(".noetome-compile");
        if alias == "junction" {
            let result = std::process::Command::new("cmd.exe")
                .args(["/D", "/C", "mklink", "/J"])
                .arg(&root)
                .arg(&outside)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "actual native junction setup: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        } else {
            let (store, _) = fixture.run();
            drop(store);
            fs::remove_file(root.join("compile.sqlite")).unwrap();
            fs::hard_link(&valuable, root.join("compile.sqlite")).unwrap();
        }
        assert!(Store::create(&fixture.project).is_err(), "{alias}");
        assert_eq!(fs::read(&valuable).unwrap(), b"do not touch");
        if alias == "junction" {
            fs::remove_dir(&root).unwrap();
        }
    }
}

#[test]
fn local_openai_fake_http_smoke() {
    use std::io::{Read, Write};
    let mut fixture = Fixture::new(&[("Authored local smoke text.", "text_ready")]);
    fixture.config.transport_mode = "local_openai".into();
    fixture.config.credential_mode = Some("none".into());
    fixture.config.credential_env.clear();
    fixture.config.model = "authored-local-model".into();
    fixture.config.usage_ledger = fixture._temp.path().join("local-openai.usage.json");
    let budget = fixture.authority();
    drop(budget);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    fixture.config.endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut raw = Vec::new();
        let (offset, length) = loop {
            let mut buf = [0; 4096];
            let n = socket.read(&mut buf).unwrap();
            assert!(n > 0);
            raw.extend_from_slice(&buf[..n]);
            if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&raw[..i]).to_lowercase();
                assert!(!headers.contains("authorization:"));
                let length: usize = headers
                    .lines()
                    .find_map(|l| {
                        l.strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap();
                if raw.len() >= i + 4 + length {
                    break (i + 4, length);
                }
            }
        };
        let body: Value = serde_json::from_slice(&raw[offset..offset + length]).unwrap();
        let input: Value =
            serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
        let candidates = json!({"schema_version":1,"request_id":input["request"]["request_id"],"binding":input["request"]["binding"],"concepts":[],"evidence":[],"atoms":[]});
        let response=json!({"id":"fake-local-response","model":"authored-local-model","object":"chat.completion","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":candidates.to_string()}}],"usage":{"prompt_tokens":12,"completion_tokens":8,"total_tokens":20,"completion_tokens_details":{"reasoning_tokens":3}}}).to_string();
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
    });
    let (_store, run) = fixture.run();
    super::super::run(
        &fixture.project,
        &run,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .unwrap();
    server.join().unwrap();
    assert_eq!(
        BudgetAuthority::open(&fixture.config.usage_ledger)
            .unwrap()
            .status()
            .unwrap()
            .accounted_tokens,
        120
    );
}

#[test]
fn local_http_test_credential_rejects_non_test_values() {
    let fixture = Fixture::new(&[("Authored credential validation.", "text_ready")]);
    let config = &fixture.config;
    super::super::validate_credential(config, "noetome-test-authored-credential").unwrap();
    for credential in [
        "sk-authored-non-test-credential",
        "",
        "noetome-test-has space",
        "noetome-test-has\nnewline",
    ] {
        let failure = super::super::validate_credential(config, credential).unwrap_err();
        assert_eq!(failure.code(), "COMPILE_CREDENTIAL", "{credential:?}");
    }
    let overlong = format!("noetome-test-{}", "x".repeat(8192));
    assert_eq!(
        super::super::validate_credential(config, &overlong)
            .unwrap_err()
            .code(),
        "COMPILE_CREDENTIAL"
    );
}

#[test]
fn finish_rejects_usage_over_request_limits_after_accounting() {
    for case in [
        "exact_limits",
        "input_over_limit",
        "output_over_limit",
        "total_over_reservation",
        "large_output_over_limit",
    ] {
        let fixture = Fixture::new(&[("Diffusion has authored budget evidence.", "text_ready")]);
        let (store, run) = fixture.run();
        let unit = fixture.prepare_all(&store, &run).remove(0);
        let (attempt, _) = fixture.originals(&store, &run, &unit, &[], false, |_| {});
        let input_limit = fs::read(
            store
                .root
                .join(&attempt.directory)
                .join("request-body.json"),
        )
        .unwrap()
        .len() as u64
            + 1024;
        let output_limit = fixture.config.max_output_tokens;
        let (input, output, reserved) = match case {
            "exact_limits" => (input_limit, output_limit, input_limit + output_limit),
            "input_over_limit" => (input_limit + 1, 0, input_limit + output_limit),
            "output_over_limit" => (12, output_limit + 1, input_limit + output_limit),
            "total_over_reservation" => (12, 8, 19),
            "large_output_over_limit" => (12, 100000, input_limit + output_limit),
            _ => unreachable!(),
        };
        rewrite_authored_receipt(&store, &attempt, input, output, 200, true, None);
        let mut budget = fixture.authority();
        let key = super::super::key(&run, &attempt);
        budget
            .reserve(&BudgetRequest {
                key: key.clone(),
                request_sha256: attempt.request_sha256.clone(),
                reserved_tokens: reserved,
            })
            .unwrap();
        budget.mark_sent(&key).unwrap();
        let result = super::super::finish(
            &fixture.project,
            &run,
            &store,
            &fixture.config,
            &mut budget,
            &unit,
            &attempt,
            "noetome-test-artificial-credential-never-in-source",
        );
        if case == "exact_limits" {
            result.unwrap();
            assert_eq!(store.summary(&run).unwrap().units_confirmed, 1);
            assert_eq!(store.summary(&run).unwrap().ir_revision, 1);
        } else {
            assert_eq!(result.unwrap_err().code(), "COMPILE_BUDGET_STOP", "{case}");
            let summary = store.summary(&run).unwrap();
            assert_eq!(
                (summary.ir_revision, summary.units_confirmed, summary.atoms),
                (0, 0, 0),
                "{case}"
            );
        }
        let paid = budget.status().unwrap();
        assert_eq!(paid.accounted_tokens, 100 + input + output, "{case}");
        assert_eq!(paid.pending_reserved_tokens, 0, "{case}");
        assert_eq!(paid.model_attempts, 2, "{case}");
        assert!(
            paid.remaining_tokens > 0,
            "fixture must not stop through cumulative exhaustion"
        );
        assert_eq!(
            budget.attempt(&key).unwrap().unwrap().state,
            BudgetAttemptState::Known
        );
        if case != "exact_limits" {
            // A replay after accounting must neither charge twice nor commit forbidden knowledge.
            assert!(
                super::super::finish(
                    &fixture.project,
                    &run,
                    &store,
                    &fixture.config,
                    &mut budget,
                    &unit,
                    &attempt,
                    "noetome-test-artificial-credential-never-in-source",
                )
                .is_err()
            );
            assert_eq!(
                budget.status().unwrap().accounted_tokens,
                paid.accounted_tokens
            );
            assert_eq!(store.summary(&run).unwrap().atoms, 0);
        }
    }
}

#[test]
fn recovery_continues_after_saved_request_and_candidate_failures() {
    let (fixture, run, _) =
        saved_recovery_fixture(&["request_failure", "candidate_failure", "success"]);
    let store = Store::open(&fixture.project).unwrap().unwrap();
    let mut budget = BudgetAuthority::open(&fixture.config.usage_ledger).unwrap();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    super::super::execute(
        &fixture.project,
        &run,
        &store,
        &fixture.config,
        "noetome-test-artificial-credential-never-in-source",
        &mut budget,
        &cancel,
    )
    .unwrap();
    let summary = store.summary(&run).unwrap();
    assert_eq!(summary.status, "needs_review");
    assert_eq!(
        (
            summary.units_failed,
            summary.units_confirmed,
            summary.atoms,
            summary.ir_revision
        ),
        (2, 1, 1, 1)
    );
    let paid = budget.status().unwrap();
    assert_eq!(
        (
            paid.accounted_tokens,
            paid.pending_reserved_tokens,
            paid.model_attempts
        ),
        (550, 0, 4)
    );
    assert!(store.attempts_needing_recovery(&run).unwrap().is_empty());
    drop(store);
    drop(budget);
    let store = Store::open(&fixture.project).unwrap().unwrap();
    let mut budget = BudgetAuthority::open(&fixture.config.usage_ledger).unwrap();
    super::super::execute(
        &fixture.project,
        &run,
        &store,
        &fixture.config,
        "noetome-test-artificial-credential-never-in-source",
        &mut budget,
        &cancel,
    )
    .unwrap();
    assert_eq!(
        encoded(&budget.status().unwrap()).unwrap(),
        encoded(&paid).unwrap()
    );
    assert_eq!(store.summary(&run).unwrap().ir_revision, 1);
    assert_eq!(store.attempts(&run).unwrap().len(), 3);
}

#[test]
fn recovery_stops_at_five_consecutive_saved_failures() {
    let (fixture, run, attempts) = saved_recovery_fixture(&[
        "request_failure",
        "candidate_failure",
        "request_failure",
        "candidate_failure",
        "request_failure",
        "success",
    ]);
    let store = Store::open(&fixture.project).unwrap().unwrap();
    let mut budget = BudgetAuthority::open(&fixture.config.usage_ledger).unwrap();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let failure = super::super::execute(
        &fixture.project,
        &run,
        &store,
        &fixture.config,
        "noetome-test-artificial-credential-never-in-source",
        &mut budget,
        &cancel,
    )
    .unwrap_err();
    assert_eq!(failure.code(), "COMPILE_UNIT_FAILURES");
    let summary = store.summary(&run).unwrap();
    assert_eq!(
        (
            summary.units_failed,
            summary.units_confirmed,
            summary.ir_revision
        ),
        (5, 0, 0)
    );
    let paid = budget.status().unwrap();
    assert_eq!(
        (
            paid.accounted_tokens,
            paid.pending_reserved_tokens,
            paid.model_attempts
        ),
        (850, 2000, 7)
    );
    assert_eq!(store.attempts_needing_recovery(&run).unwrap().len(), 1);
    assert_eq!(
        budget
            .attempt(&super::super::key(&run, &attempts[5]))
            .unwrap()
            .unwrap()
            .state,
        BudgetAttemptState::Sent
    );
}

#[test]
fn recovery_success_resets_consecutive_saved_failure_count() {
    let (fixture, run, _) = saved_recovery_fixture(&[
        "request_failure",
        "candidate_failure",
        "request_failure",
        "candidate_failure",
        "success",
        "request_failure",
        "candidate_failure",
        "request_failure",
        "candidate_failure",
        "success",
    ]);
    let store = Store::open(&fixture.project).unwrap().unwrap();
    let mut budget = BudgetAuthority::open(&fixture.config.usage_ledger).unwrap();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    super::super::execute(
        &fixture.project,
        &run,
        &store,
        &fixture.config,
        "noetome-test-artificial-credential-never-in-source",
        &mut budget,
        &cancel,
    )
    .unwrap();
    let summary = store.summary(&run).unwrap();
    assert_eq!(
        (
            summary.units_failed,
            summary.units_confirmed,
            summary.atoms,
            summary.ir_revision
        ),
        (8, 2, 2, 2)
    );
    assert_eq!(summary.status, "needs_review");
    let paid = budget.status().unwrap();
    assert_eq!(
        (
            paid.accounted_tokens,
            paid.pending_reserved_tokens,
            paid.model_attempts
        ),
        (1600, 0, 11)
    );
}

#[test]
fn recovery_budget_failure_stops_before_later_receipt() {
    let (fixture, run, attempts) = saved_recovery_fixture(&["budget_failure", "success"]);
    let store = Store::open(&fixture.project).unwrap().unwrap();
    let mut budget = BudgetAuthority::open(&fixture.config.usage_ledger).unwrap();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let failure = super::super::execute(
        &fixture.project,
        &run,
        &store,
        &fixture.config,
        "noetome-test-artificial-credential-never-in-source",
        &mut budget,
        &cancel,
    )
    .unwrap_err();
    assert_eq!(failure.code(), "COMPILE_BUDGET_STOP");
    let summary = store.summary(&run).unwrap();
    assert_eq!(
        (summary.units_confirmed, summary.atoms, summary.ir_revision),
        (0, 0, 0)
    );
    let paid = budget.status().unwrap();
    assert_eq!(
        (paid.accounted_tokens, paid.pending_reserved_tokens),
        (713, 2000)
    );
    assert_eq!(
        budget
            .attempt(&super::super::key(&run, &attempts[0]))
            .unwrap()
            .unwrap()
            .state,
        BudgetAttemptState::Known
    );
    assert_eq!(
        budget
            .attempt(&super::super::key(&run, &attempts[1]))
            .unwrap()
            .unwrap()
            .state,
        BudgetAttemptState::Sent
    );
}

#[test]
fn budget_stop_survives_crash_before_run_error_handler() {
    let (fixture, run, attempts) =
        saved_recovery_fixture_with_transport(&["budget_failure", "success"], "local_openai");
    let store = Store::open(&fixture.project).unwrap().unwrap();
    let mut budget = BudgetAuthority::open(&fixture.config.usage_ledger).unwrap();
    let unit = store.unit(&run, &attempts[0].unit_id).unwrap();
    // Simulate a crash after finish returns but before run can persist its error.
    // Both responses already exist, so reopening must not contact any provider.
    let failure = super::super::finish(
        &fixture.project,
        &run,
        &store,
        &fixture.config,
        &mut budget,
        &unit,
        &attempts[0],
        "",
    )
    .unwrap_err();
    assert_eq!(failure.code(), "COMPILE_BUDGET_STOP");
    let paid = budget.status().unwrap();
    assert_eq!(
        (paid.accounted_tokens, paid.pending_reserved_tokens),
        (713, 2000)
    );
    assert!(paid.remaining_tokens > 0);
    let before_budget = encoded(&paid).unwrap();
    let crashed = store.summary(&run).unwrap();
    assert_eq!(crashed.status, "budget_stop");
    assert_eq!(crashed.error.unwrap().code, "COMPILE_BUDGET_STOP");
    drop(store);
    drop(budget);

    // Reopen through the public entry point twice, without manually setting a run error.
    for _ in 0..2 {
        let resumed = super::super::run(
            &fixture.project,
            &run,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(resumed.status, "budget_stop");
        assert_eq!(resumed.error.unwrap().code, "COMPILE_BUDGET_STOP");
        assert_eq!(
            (resumed.units_confirmed, resumed.atoms, resumed.ir_revision),
            (0, 0, 0)
        );
        let reopened = Store::open(&fixture.project).unwrap().unwrap();
        let persisted = reopened.summary(&run).unwrap();
        assert_eq!(persisted.status, "budget_stop");
        assert_eq!(persisted.error.unwrap().code, "COMPILE_BUDGET_STOP");
        assert_eq!(
            reopened
                .attempt(&run, &attempts[1].request_id)
                .unwrap()
                .state,
            "sent"
        );
        let attempt_count: i64 = reopened
            .connection
            .query_row(
                "SELECT COUNT(*) FROM attempts WHERE run_id=?1",
                [&run],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attempt_count, 2);
        let mut reopened_budget = BudgetAuthority::open(&fixture.config.usage_ledger).unwrap();
        assert_eq!(
            encoded(&reopened_budget.status().unwrap()).unwrap(),
            before_budget
        );
        assert_eq!(
            reopened_budget
                .attempt(&super::super::key(&run, &attempts[0]))
                .unwrap()
                .unwrap()
                .state,
            BudgetAttemptState::Known
        );
        assert_eq!(
            reopened_budget
                .attempt(&super::super::key(&run, &attempts[1]))
                .unwrap()
                .unwrap()
                .state,
            BudgetAttemptState::Sent
        );
    }
}

#[test]
fn recovery_global_failure_stops_before_later_receipt() {
    for (outcome, expected) in [
        ("changed_receipt", "COMPILE_RECEIPT"),
        ("paused", "COMPILE_PAUSED"),
    ] {
        let (fixture, run, attempts) = saved_recovery_fixture(&[outcome, "success"]);
        let store = Store::open(&fixture.project).unwrap().unwrap();
        let mut budget = BudgetAuthority::open(&fixture.config.usage_ledger).unwrap();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let failure = super::super::execute(
            &fixture.project,
            &run,
            &store,
            &fixture.config,
            "noetome-test-artificial-credential-never-in-source",
            &mut budget,
            &cancel,
        )
        .unwrap_err();
        assert_eq!(failure.code(), expected, "{outcome}");
        let summary = store.summary(&run).unwrap();
        assert_eq!(
            (summary.units_confirmed, summary.atoms, summary.ir_revision),
            (0, 0, 0)
        );
        let paid = budget.status().unwrap();
        assert_eq!(
            (paid.accounted_tokens, paid.pending_reserved_tokens),
            (100, 4000)
        );
        assert_eq!(
            budget
                .attempt(&super::super::key(&run, &attempts[1]))
                .unwrap()
                .unwrap()
                .state,
            BudgetAttemptState::Sent
        );
    }
}
