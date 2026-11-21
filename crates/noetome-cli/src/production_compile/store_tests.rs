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
            scope_schema_version: None,
            page_ranges: None,
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
            request_strategy_version: None,
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
        let content = if prepared.strategy_version == ANCHOR_REQUEST_STRATEGY {
            // Author the v3 wire response with whole-unit anchors. Keep the
            // independently authored exact slices for commit_batch to compare.
            let mut anchored = candidate.clone();
            for evidence in &mut anchored.evidence {
                evidence.excerpt_start_byte = unit.start_byte;
                evidence.excerpt_end_byte = unit.end_byte;
                evidence.excerpt_sha256.clear();
            }
            encoded(&anchored).unwrap()
        } else {
            encoded(&candidate).unwrap()
        };
        let response = serde_json::to_vec(&json!({
            "id":"chatcmpl-authored","object":"chat.completion","model":self.config.model,
            "choices":[{"index":0,"finish_reason":"stop",
                "message":{"role":"assistant","content":content}}],
            "usage":{"prompt_tokens":100,"completion_tokens":50,"total_tokens":150}
        }))
        .unwrap();
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

fn authored_excerpt(
    unit: &Unit,
    id: &str,
    quote: &str,
) -> crate::extraction_model::CandidateEvidence {
    let start = unit.start_byte + unit.text.find(quote).unwrap();
    crate::extraction_model::CandidateEvidence {
        evidence_id: id.into(),
        unit_id: unit.unit_id.clone(),
        excerpt_start_byte: start,
        excerpt_end_byte: start + quote.len(),
        excerpt_sha256: sha256(quote.as_bytes()),
        quote: quote.into(),
    }
}

// Reassign the UUIDs produced by the real commit, keeping row identity and all
// other data fixed. This makes the comparison to UUID sorting deterministic.
fn reverse_atom_uuids(store: &Store, run: &str, receiver_order: &[String]) -> Vec<String> {
    let tx = store.transaction().unwrap();
    let atoms: BTreeMap<String, domain::KnowledgeAtom> = {
        let mut statement = tx
            .prepare("SELECT json FROM atoms WHERE run_id=?1")
            .unwrap();
        let rows = statement
            .query_map([run], |row| row.get::<_, String>(0))
            .unwrap();
        rows.map(|raw| {
            let atom: domain::KnowledgeAtom = decode(&raw.unwrap()).unwrap();
            (atom.content.clone(), atom)
        })
        .collect()
    };
    assert_eq!(atoms.len(), receiver_order.len());
    let mut ids = atoms
        .values()
        .map(|atom| atom.atom_id.clone())
        .collect::<Vec<_>>();
    ids.sort_unstable_by(|left, right| right.cmp(left));
    assert!(ids.windows(2).all(|pair| pair[0] > pair[1]));
    tx.execute_batch("PRAGMA defer_foreign_keys=ON").unwrap();
    tx.execute(
        "UPDATE atoms SET atom_id='fixture:' || atom_id WHERE run_id=?1",
        [run],
    )
    .unwrap();
    tx.execute(
        "UPDATE atom_concepts SET atom_id='fixture:' || atom_id WHERE run_id=?1",
        [run],
    )
    .unwrap();
    for (content, id) in receiver_order.iter().zip(&ids) {
        let mut atom = atoms[content].clone();
        let temporary = format!("fixture:{}", atom.atom_id);
        atom.atom_id = id.clone();
        let raw = encoded(&atom).unwrap();
        assert_eq!(
            tx.execute(
                "UPDATE atoms SET atom_id=?1,json=?2,sha256=?3 WHERE run_id=?4 AND atom_id=?5",
                params![id, raw, sha256(raw.as_bytes()), run, temporary],
            )
            .unwrap(),
            1
        );
        assert_eq!(
            tx.execute(
                "UPDATE atom_concepts SET atom_id=?1 WHERE run_id=?2 AND atom_id=?3",
                params![id, run, temporary],
            )
            .unwrap(),
            1
        );
    }
    tx.commit().unwrap();
    ids
}

fn authored_ordered_batch(
    fixture: &Fixture,
    store: &Store,
    run: &str,
    unit: &Unit,
    contexts: &[ConceptContext],
    contents: &[String],
) -> (Attempt, AcceptedBatch) {
    fixture.sealed(store, run, unit, contexts, |candidates| {
        let template = candidates.atoms[0].clone();
        candidates.evidence = contents
            .iter()
            .enumerate()
            .map(|(index, content)| {
                authored_excerpt(unit, &format!("local_excerpt_{index}"), content)
            })
            .collect();
        candidates.atoms = contents
            .iter()
            .enumerate()
            .map(|(index, content)| {
                let mut candidate = template.clone();
                candidate.atom.atom_id = format!("local_atom_{:02}", contents.len() - index);
                candidate.atom.kind = if index % 2 == 0 {
                    domain::AtomKind::Statement
                } else {
                    domain::AtomKind::Definition
                };
                candidate.atom.content = content.clone();
                candidate.atom.evidence_links[0].evidence_id = format!("local_excerpt_{index}");
                candidate
            })
            .collect();
    })
}

#[test]
fn precise_atom_order_survives_retry_reopen_and_paginated_mixed_kind_export() {
    assert_atom_order_survives_retry_reopen_and_paginated_export(PRECISE_REQUEST_STRATEGY);
}

#[test]
fn anchor_atom_order_survives_retry_reopen_and_paginated_mixed_kind_export() {
    assert_atom_order_survives_retry_reopen_and_paginated_export(ANCHOR_REQUEST_STRATEGY);
}

fn assert_atom_order_survives_retry_reopen_and_paginated_export(strategy: u32) {
    // Make the export cross its 50-item boundary and commit the three units
    // out of physical order, including a second attempt on the first page.
    let contents = (1..=52)
        .map(|step| {
            let tag = if step % 2 == 0 { 'A' } else { 'Z' };
            format!("Diffusion step {tag}{step:02}.")
        })
        .collect::<Vec<_>>();
    let first_page = contents[..2].concat();
    let middle_unit = contents[2..49].concat();
    let last_unit = contents[49..].concat();
    let second_page = format!("{middle_unit}{last_unit}");
    let mut fixture = Fixture::new(&[(&first_page, "text_ready"), (&second_page, "text_ready")]);
    fixture.config.request_strategy_version = Some(strategy);
    drop(fixture.authority());
    let (store, run) = fixture.run();
    let first = fixture.unit(1);
    let mut middle = fixture.unit(2);
    middle.text = middle_unit;
    middle.end_byte = middle.text.len();
    let mut last = fixture.unit(2);
    last.start_byte = middle.end_byte;
    last.text = last_unit;
    for units in [vec![first.clone()], vec![middle.clone(), last.clone()]] {
        let unit = &units[0];
        store
            .save_page(
                &run,
                &fixture.scope,
                unit.page_number,
                &unit.page_sha256,
                "text_ready",
                units.clone(),
                vec![],
            )
            .unwrap();
    }
    store
        .begin_attempt(&run, &first, &Uuid::new_v4().to_string(), &"a".repeat(64))
        .unwrap();
    store
        .mark_unit_failed(
            &run,
            &first.unit_id,
            "AUTHORED_FAILURE",
            "fixture request failed",
        )
        .unwrap();
    let (attempt, batch) =
        authored_ordered_batch(&fixture, &store, &run, &last, &[], &contents[49..]);
    store.commit_batch(&run, &last, &attempt, batch).unwrap();
    let contexts = store.context_concepts(&run, "Diffusion", 1).unwrap();
    assert_eq!(contexts.len(), 1);
    let concept = &contexts[0].concept.concept_id;
    let (attempt, batch) =
        authored_ordered_batch(&fixture, &store, &run, &middle, &contexts, &contents[2..49]);
    store.commit_batch(&run, &middle, &attempt, batch).unwrap();
    store.retry_failed(&run).unwrap();
    let (attempt, batch) =
        authored_ordered_batch(&fixture, &store, &run, &first, &contexts, &contents[..2]);
    assert_eq!(attempt.attempt_no, 2);
    store.commit_batch(&run, &first, &attempt, batch).unwrap();
    let ids = reverse_atom_uuids(&store, &run, &contents);
    drop(store);

    let reopened = Store::open(&fixture.project).unwrap().unwrap();
    let summary = reopened.summary(&run).unwrap();
    assert_eq!(
        (
            summary.request_strategy_version,
            summary.units_confirmed,
            summary.atoms
        ),
        (strategy, 3, 52)
    );
    for (offset, end) in [(0, 50), (50, 52), (52, 52)] {
        let result = reopened
            .query(&run, "atoms", offset as u64, 50, concept)
            .unwrap();
        assert_eq!(result["total"], 52);
        let items = result["items"].as_array().unwrap();
        assert_eq!(items.len(), end - offset);
        for (row, position) in items.iter().zip(offset..end) {
            assert_eq!(row["content"], contents[position]);
            assert_eq!(row["atom_id"], ids[position]);
        }
    }
    drop(reopened);

    let vault = fixture._temp.path().join("ordered-export");
    super::super::export(&fixture.project, &run, &vault).unwrap();
    let note = fs::read_to_string(vault.join("知识").join(format!("{concept}.md"))).unwrap();
    let paragraphs = note
        .lines()
        .filter(|line| line.starts_with("Diffusion step "))
        .collect::<Vec<_>>();
    assert_eq!(
        paragraphs,
        contents.iter().map(String::as_str).collect::<Vec<_>>()
    );
    let blocks = fs::read_to_string(vault.join(".noetome-index/note-blocks.ndjson")).unwrap();
    assert_eq!(blocks.lines().count(), ids.len());
    for (ordinal, line) in blocks.lines().enumerate() {
        let block: Value = serde_json::from_str(line).unwrap();
        assert_eq!(block["atom_id"], ids[ordinal]);
        assert_eq!(block["block_ordinal"], ordinal);
    }
}

#[test]
fn precise_measurement_and_evidence_relations_survive_ir_and_markdown() {
    assert_measurement_and_evidence_relations_survive_ir_and_markdown(PRECISE_REQUEST_STRATEGY);
}

#[test]
fn anchor_measurement_and_evidence_relations_survive_ir_and_markdown() {
    assert_measurement_and_evidence_relations_survive_ir_and_markdown(ANCHOR_REQUEST_STRATEGY);
}

fn assert_measurement_and_evidence_relations_survive_ir_and_markdown(strategy: u32) {
    const SUPPORT: &str = "在 20 °C、避光时，校准读数 $x_i$ 为 2.5 mmol·L⁻¹。";
    const QUALIFY: &str = "本量程只适用于透明样品。";
    const REFUTE: &str = "另一组在相同条件下测得 3.1 mmol·L⁻¹。";
    let original = format!("合成记录。{SUPPORT}{QUALIFY}{REFUTE}记录完毕。");
    let mut fixture = Fixture::new(&[
        ("Other authored page.", "text_ready"),
        (&original, "text_ready"),
    ]);
    fixture.config.request_strategy_version = Some(strategy);
    drop(fixture.authority());
    let (store, run) = fixture.run();
    let units = fixture.prepare_all(&store, &run);
    let unit = &units[1];
    let evidence = [
        ("local_support", "supports", SUPPORT),
        ("local_limit", "qualifies", QUALIFY),
        ("local_counter", "refutes", REFUTE),
    ];
    let qualifiers = ["mmol·L⁻¹", "20 °C", "避光", "透明样品"];
    let measurement = json!({"value_status":"known", "value":"2.5", "unit":"mmol·L⁻¹",
        "conditions":["20 °C", "避光"]});
    let (attempt, batch) = fixture.sealed(&store, &run, unit, &[], |candidates| {
        candidates.evidence = evidence
            .iter()
            .map(|&(id, _, quote)| authored_excerpt(unit, id, quote))
            .collect();
        let candidate = &mut candidates.atoms[0];
        candidate.atom.kind = domain::AtomKind::ComparisonValue;
        candidate.atom.value_status = domain::ValueStatus::Known;
        candidate.atom.content = SUPPORT.into();
        candidate.atom.qualifiers = qualifiers.iter().map(|value| (*value).into()).collect();
        candidate.atom.evidence_links = serde_json::from_value(json!(
            evidence
                .iter()
                .map(|&(id, relation, _)| json!({"evidence_id":id,"relation":relation}))
                .collect::<Vec<_>>()
        ))
        .unwrap();
        candidate.measurement = Some(serde_json::from_value(measurement.clone()).unwrap());
    });
    store.commit_batch(&run, unit, &attempt, batch).unwrap();
    drop(store);

    let reopened = Store::open(&fixture.project).unwrap().unwrap();
    let atoms = reopened.query(&run, "atoms", 0, 1, "").unwrap();
    assert_eq!(atoms["total"], 1);
    let atom = &atoms["items"][0];
    assert_eq!(atom["content"], SUPPORT);
    assert_eq!(atom["qualifiers"], json!(qualifiers));
    assert_eq!(atom["measurement"], measurement);
    assert_eq!(atom["verification"], "unverified");
    let links = atom["evidence_links"].as_array().unwrap();
    assert_eq!(links.len(), evidence.len());
    let mut ids = BTreeSet::new();
    for (link, (_, relation, quote)) in links.iter().zip(evidence) {
        assert_eq!(link["relation"], relation);
        let id = link["evidence_id"].as_str().unwrap();
        assert!(ids.insert(id));
        let row = reopened.evidence(&run, id).unwrap();
        let start = original.find(quote).unwrap();
        assert_eq!(row["source_id"], fixture.scope.source_id);
        assert_eq!(row["page_number"], 2);
        assert_eq!(row["page_sha256"], unit.page_sha256);
        assert_eq!(row["start_byte"], start);
        assert_eq!(row["end_byte"], start + quote.len());
        assert_eq!(row["quote"], quote);
        assert_eq!(row["excerpt_sha256"], sha256(quote.as_bytes()));
    }
    let issues = reopened
        .query(&run, "issues", 0, 50, "ATOM_REFUTATION_UNRESOLVED")
        .unwrap();
    assert_eq!(issues["total"], 1);
    let issue = &issues["items"][0];
    assert_eq!(issue["code"], "ATOM_REFUTATION_UNRESOLVED");
    assert_eq!(
        issue["scope"],
        format!("atom:{}", atom["atom_id"].as_str().unwrap())
    );
    assert_eq!(issue["resolution"], "pending");
    let summary = reopened.summary(&run).unwrap();
    assert_eq!(summary.request_strategy_version, strategy);
    assert_eq!(summary.scientific_quality, "not_run");
    drop(reopened);

    let concept = atom["concept_ids"][0].as_str().unwrap();
    let vault = fixture._temp.path().join("relations-export");
    super::super::export(&fixture.project, &run, &vault).unwrap();
    let note = fs::read_to_string(vault.join("知识").join(format!("{concept}.md"))).unwrap();
    for expected in [
        r"在 20 °C、避光时，校准读数 \$x\_i\$ 为 2.5 mmol·L⁻¹。".to_string(),
        "数值：2.5 mmol·L⁻¹。".into(),
        "测量条件：20 °C；避光。".into(),
        format!(r"来源支持（物理页 2）：在 20 °C、避光时，校准读数 \$x\_i\$ 为 2.5 mmol·L⁻¹。"),
        format!("限定依据（物理页 2）：{QUALIFY}"),
        format!("反证依据，尚待核对（物理页 2）：{REFUTE}"),
    ] {
        assert!(
            note.lines().any(|line| line == expected),
            "Missing note line {expected:?}"
        );
    }
    let blocks = fs::read_to_string(vault.join(".noetome-index/note-blocks.ndjson")).unwrap();
    let block: Value = serde_json::from_str(blocks.trim()).unwrap();
    assert_eq!(block["atom_id"], atom["atom_id"]);
    assert_eq!(block["evidence_links"], atom["evidence_links"]);
}

#[test]
fn precise_export_keeps_source_and_candidate_markdown_literal() {
    assert_export_keeps_source_and_candidate_markdown_literal(PRECISE_REQUEST_STRATEGY);
}

#[test]
fn anchor_export_keeps_source_and_candidate_markdown_literal() {
    assert_export_keeps_source_and_candidate_markdown_literal(ANCHOR_REQUEST_STRATEGY);
}

fn assert_export_keeps_source_and_candidate_markdown_literal(precise_strategy: u32) {
    const QUOTE: &str = "源%%保留%% &amp; $x_i$\n---\n1. 引文\n# 原页";
    const TITLE: &str = "标题 &amp;\n---\n+ 续行";
    const ALIAS: &str = "别名 &amp;\n# 别名续行";
    const CONTENT: &str = "正文 &amp; $x_i$ (p) | ~条件~\n---\n1. 第一项\n====\n- 第二项\n# 段落\n    四个空格\n\t制表符";
    const QUALIFIER: &str = "限定 &amp; (原样)\n+ 适用条件\n2) 顺序";
    let original = format!("页首：{QUOTE}\n页尾。");
    let mut fixture = Fixture::new(&[(&original, "text_ready")]);
    drop(fixture.authority());

    for (strategy, directory) in [
        (Some(precise_strategy), "precise-literal"),
        (None, "legacy-literal"),
    ] {
        fixture.config.request_strategy_version = strategy;
        let (store, run) = fixture.run();
        let units = fixture.prepare_all(&store, &run);
        let unit = &units[0];
        let precise = strategy.is_some();
        let (attempt, batch) = fixture.sealed(&store, &run, unit, &[], |candidates| {
            candidates.concepts[0].canonical_name = TITLE.into();
            candidates.concepts[0].aliases = vec![ALIAS.into()];
            candidates.atoms[0].atom.content = CONTENT.into();
            candidates.atoms[0].atom.qualifiers = vec![QUALIFIER.into()];
            if precise {
                candidates.evidence[0] = authored_excerpt(unit, "local_evidence", QUOTE);
            }
        });
        store.commit_batch(&run, unit, &attempt, batch).unwrap();
        drop(store);

        let output = fixture._temp.path().join(directory);
        let report = super::super::export(&fixture.project, &run, &output).unwrap();
        assert_eq!(
            report["summary"]["request_strategy_version"],
            strategy.unwrap_or(1)
        );
        let read_index = |name: &str| -> Value {
            let raw = fs::read_to_string(output.join(".noetome-index").join(name)).unwrap();
            serde_json::from_str(&raw).unwrap()
        };
        let entry = read_index("notes.ndjson");
        let id = entry["note_id"].as_str().unwrap();
        assert_eq!(entry["title"], TITLE);
        let atom = read_index("atoms.ndjson");
        assert_eq!(atom["content"], CONTENT);
        assert_eq!(atom["qualifiers"], json!([QUALIFIER]));
        let evidence = read_index("evidence.ndjson");
        let quote = if precise { QUOTE } else { original.as_str() };
        assert_eq!(evidence["quote"], quote);
        assert_eq!(evidence["excerpt_sha256"], sha256(quote.as_bytes()));
        assert_eq!(evidence["page_sha256"], fixture.pages[0].2);

        let (title, alias, content, qualifier, section, source) = if precise {
            (
                "标题 \\&amp;  \n\\---  \n\\+ 续行",
                "别名 \\&amp;  \n\\# 别名续行",
                concat!(
                    "正文 \\&amp; \\$x\\_i\\$ \\(p\\) \\| \\~条件\\~  \n",
                    "\\---  \n1\\. 第一项  \n\\====  \n\\- 第二项  \n\\# 段落  \n",
                    "&#32;   四个空格  \n&#9;制表符"
                ),
                "限定 \\&amp; \\(原样\\)  \n\\+ 适用条件  \n2\\) 顺序",
                "知识与过程",
                concat!(
                    "来源支持（物理页 1）：源\\%\\%保留\\%\\% \\&amp; \\$x\\_i\\$  \n",
                    "\\---  \n1\\. 引文  \n\\# 原页\n\n"
                ),
            )
        } else {
            (
                TITLE,
                "别名 &amp;\n\\# 别名续行",
                concat!(
                    "正文 &amp; $x\\_i$ (p) | ~条件~\n---\n1. 第一项\n====\n- 第二项\n",
                    "\\# 段落\n    四个空格\n\t制表符"
                ),
                QUALIFIER,
                "概念与定义",
                "",
            )
        };
        let note = fs::read_to_string(output.join("知识").join(format!("{id}.md"))).unwrap();
        assert_eq!(
            note,
            format!(
                "# {title}\n\n相关称谓：{alias}。\n\n## {section}\n\n{content}\n\n适用条件、单位与例外：{qualifier}。\n\n{source}"
            )
        );
        let overview = fs::read_to_string(output.join("学习笔记.md")).unwrap();
        assert!(overview.ends_with(&format!("- [{title}](知识/{id}.md)\n")));
    }
}

#[test]
fn precise_export_pins_needs_review_page_without_source_issues() {
    let mut fixture = Fixture::new(&[("", "needs_review")]);
    fixture.config.request_strategy_version = Some(2);
    drop(fixture.authority());
    let (store, run) = fixture.run();
    assert!(fixture.prepare_all(&store, &run).is_empty());
    drop(store);

    let output = fixture._temp.path().join("empty-issues-location");
    super::super::export(&fixture.project, &run, &output).unwrap();
    let raw = fs::read_to_string(output.join(".noetome-index/source-locations.ndjson")).unwrap();
    let record: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(record["run_id"], run);
    assert_eq!(record["source_id"], fixture.scope.source_id);
    assert_eq!(record["source_sha256"], fixture.scope.source_sha256);
    assert_eq!(record["page_number"], 1);
    assert_eq!(record["page_sha256"], fixture.pages[0].2);
    assert_eq!(record["status"], "needs_review");
    assert_eq!(record["units_failed"], 0);
    assert_eq!(record["units_pending"], 0);
    assert_eq!(record["source_issues"], json!([]));
    assert_eq!(record["regions"], json!([]));
    assert_eq!(record["localization"], "page_only");
    assert_eq!(record["issue_region_mapping"], "unresolved");

    let report = fs::read_to_string(output.join("处理范围.md")).unwrap();
    let (_, page) = report.split_once("\n### 物理页 1\n").unwrap();
    assert!(page.contains(&fixture.pages[0].2));
    assert!(page.contains(r"页状态：needs\_review"));
    assert!(page.contains("未提供逐项问题代码"));
    assert!(page.contains("只固定到整页"));
    assert!(!page.contains("SOURCE_REVIEW"));
    assert!(!page.contains("- 原图区域 "));
}

#[test]
fn precise_export_preserves_unresolved_source_locations_and_legacy_output() {
    assert_export_preserves_unresolved_source_locations_and_legacy_output(PRECISE_REQUEST_STRATEGY);
}

#[test]
fn anchor_export_preserves_unresolved_source_locations_and_legacy_output() {
    assert_export_preserves_unresolved_source_locations_and_legacy_output(ANCHOR_REQUEST_STRATEGY);
}

fn assert_export_preserves_unresolved_source_locations_and_legacy_output(strategy: u32) {
    const INDEX: &str = ".noetome-index/source-locations.ndjson";
    let mut fixture = Fixture::new(&[("", "needs_ocr"); 4]);
    fixture.scope.scope_schema_version = Some(1);
    fixture.scope.page_ranges = Some(vec![PageRange {
        start_page: 2,
        end_page: 4,
    }]);
    fixture.config.request_strategy_version = Some(strategy);
    let source_id = &fixture.scope.source_id;
    let geometry = json!({"coordinate_space":"pdf-display-pt","crop_box":[0,0,792,612]});
    let image = json!({"ordinal":0,"kind":"image","text":"","bbox":[10,20,200,100],"assets":[]});
    let equation = json!({"ordinal":2,"region_id":"saved-equation-2","kind":"equation",
        "text":"","bbox":[25,140,380,190],"bbox_scope":"region","assets":[]});
    let text_region = |ordinal| {
        json!({"ordinal":ordinal,"kind":"text","text":"",
        "bbox":[20,110,310,130]})
    };
    let source_issues = [
        json!([{"code":"OUTSIDE-PAGE","message":"范围外的候选图。"}]),
        json!([{"code":"VISUAL-REVIEW","message":"图义尚待核对。"},
               {"code":"FORMULA-REVIEW","message":"公式符号尚待核对。"}]),
        json!([{"code":"PAGE-ONLY","message":"没有可靠的局部坐标。"}]),
    ];
    let regions = [
        json!([image.clone()]),
        json!([image.clone(), text_region(1), equation.clone()]),
        json!([text_region(0)]),
    ];
    // Author the saved source pages before either compile run pins their hashes.
    let corpus_db =
        Connection::open(fixture.project.join(".noetome-corpus/corpus.sqlite")).unwrap();
    for (index, (regions, issues)) in regions.iter().zip(&source_issues).enumerate() {
        let number = index + 1;
        let page = json!({"schema_version":1,"source_id":source_id,"page_number":number,
            "status":"needs_ocr","text":"","width_pt":612,"height_pt":792,"rotation":90,
            "geometry":geometry,"regions":regions,"issues":issues});
        let raw = encoded(&page).unwrap();
        let sha = sha256(raw.as_bytes());
        assert_eq!(
            corpus_db.execute(
                "UPDATE pages SET page_json=?1,page_sha256=?2 WHERE source_id=?3 AND page_number=?4",
                params![raw, sha, source_id, number as u32],
            ).unwrap(),
            1
        );
        fixture.pages[index].2 = sha;
    }
    drop(corpus_db);

    drop(fixture.authority());
    let (store, precise_run) = fixture.run();
    let mut legacy_config = fixture.config.clone();
    legacy_config.request_strategy_version = None;
    let legacy_run = store
        .create_run(&legacy_config, std::slice::from_ref(&fixture.scope))
        .unwrap();
    for run in [&precise_run, &legacy_run] {
        for number in 2..=3 {
            store
                .save_page(
                    run,
                    &fixture.scope,
                    number,
                    &fixture.pages[number as usize - 1].2,
                    "needs_ocr",
                    vec![],
                    vec![],
                )
                .unwrap();
        }
    }
    drop(store);

    let output = fixture._temp.path().join("precise-source-locations");
    let report = super::super::export(&fixture.project, &precise_run, &output).unwrap();
    assert_eq!(report["summary"]["request_strategy_version"], strategy);
    assert_eq!(report["summary"]["selected_pages"], 3);
    assert_eq!(report["summary"]["out_of_scope_pages"], 1);
    assert!(
        report["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == INDEX)
    );
    let records = fs::read_to_string(output.join(INDEX))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2);
    let note = fs::read_to_string(output.join("处理范围.md")).unwrap();
    let (_, unresolved) = note.split_once("\n### 物理页 2\n").unwrap();
    let (with_candidates, page_only) = unresolved.split_once("\n### 物理页 3\n").unwrap();
    for (record, (number, markdown)) in records
        .iter()
        .zip([(2usize, with_candidates), (3, page_only)])
    {
        let sha = &fixture.pages[number - 1].2;
        assert_eq!(record["schema_version"], 1);
        assert_eq!(record["run_id"], precise_run);
        assert_eq!(record["source_id"], *source_id);
        assert_eq!(record["source_sha256"], fixture.scope.source_sha256);
        assert_eq!(record["page_number"], number);
        assert_eq!(record["page_sha256"], *sha);
        assert_eq!(record["status"], "needs_ocr");
        assert_eq!(
            record["geometry"],
            json!({"width_pt":612,"height_pt":792,"rotation":90,"geometry":geometry})
        );
        assert_eq!(record["source_issues"], source_issues[number - 1]);
        assert_eq!(record["issue_region_mapping"], "unresolved");
        assert_eq!(record["note_path"], "处理范围.md");
        for text in [
            source_id.as_str(),
            fixture.scope.source_sha256.as_str(),
            sha.as_str(),
            "pdf-display-pt",
        ] {
            assert!(
                markdown.contains(text),
                "Page {number} report missing {text:?}"
            );
        }
    }
    assert_eq!(records[0]["localization"], "page_with_candidate_regions");
    assert_eq!(records[0]["regions"], json!([image, equation]));
    assert!(with_candidates.contains("页级问题尚未逐项对应到区域"));
    assert!(with_candidates.contains("VISUAL-REVIEW：图义尚待核对。"));
    assert!(with_candidates.contains("FORMULA-REVIEW：公式符号尚待核对。"));
    assert!(with_candidates.contains(&format!("{}:r0", fixture.pages[1].2)));
    assert!(with_candidates.contains("saved-equation-2"));
    assert!(with_candidates.contains(r"\[25,140,380,190\]"));
    assert_eq!(records[1]["localization"], "page_only");
    assert_eq!(records[1]["regions"], json!([]));
    assert!(page_only.contains("PAGE-ONLY：没有可靠的局部坐标。"));
    assert!(page_only.contains("该问题目前只固定到整页"));
    assert!(!page_only.contains("- 原图区域 "));
    assert!(!note.contains("OUTSIDE-PAGE"));
    assert!(!note.contains("物理页 1"));
    assert!(note.contains(&format!("物理页 4（来源 {source_id}）：尚未准备")));

    let output = fixture._temp.path().join("legacy-source-locations");
    let legacy = super::super::export(&fixture.project, &legacy_run, &output).unwrap();
    assert_eq!(legacy["summary"]["request_strategy_version"], 1);
    assert!(!output.join(INDEX).exists());
    assert!(
        !legacy["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == INDEX)
    );
    let note = fs::read_to_string(output.join("处理范围.md")).unwrap();
    assert!(!note.contains("尚未解决的页与原图定位"));
}

#[test]
fn missing_strategy_still_pages_legacy_atoms_by_uuid() {
    let contents = [
        "Diffusion first claim.".to_string(),
        "Diffusion second claim.".to_string(),
    ];
    let original = contents.concat();
    let fixture = Fixture::new(&[(&original, "text_ready")]);
    let (store, run) = fixture.run();
    let unit = fixture.prepare_all(&store, &run).remove(0);
    let (attempt, batch) = fixture.sealed(&store, &run, &unit, &[], |candidates| {
        let template = candidates.atoms[0].clone();
        candidates.atoms = contents
            .iter()
            .enumerate()
            .map(|(index, text)| {
                let mut candidate = template.clone();
                candidate.atom.atom_id = format!("local_atom_{index}");
                candidate.atom.content = text.clone();
                candidate
            })
            .collect();
    });
    store.commit_batch(&run, &unit, &attempt, batch).unwrap();
    let ids = reverse_atom_uuids(&store, &run, &contents);
    drop(store);
    let reopened = Store::open(&fixture.project).unwrap().unwrap();
    assert_eq!(
        reopened.config(&run).unwrap().request_strategy_version,
        None
    );
    assert_eq!(reopened.summary(&run).unwrap().request_strategy_version, 1);
    for (offset, original) in [1, 0].into_iter().enumerate() {
        let result = reopened.query(&run, "atoms", offset as u64, 1, "").unwrap();
        assert_eq!(result["total"], 2);
        assert_eq!(result["items"].as_array().unwrap().len(), 1);
        assert_eq!(result["items"][0]["atom_id"], ids[original]);
        assert_eq!(result["items"][0]["content"], contents[original]);
    }
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
fn compact_saved_response_recovers_once_and_exports_exact_evidence() {
    const PREFIX: &str = "固定条件。";
    const QUOTE: &str = "扩散沿浓度梯度发生";
    let source = format!("{PREFIX}{QUOTE}。另有独立段落。");
    let mut fixture = Fixture::new(&[(&source, "text_ready")]);
    fixture.config.request_strategy_version = Some(COMPACT_REQUEST_STRATEGY);
    let (store, run) = fixture.run();
    let unit = fixture.prepare_all(&store, &run).remove(0);
    let request = Uuid::new_v4().to_string();
    let prepared = provider::prepare(
        store.project_id(),
        &run,
        &request,
        &unit,
        &fixture.config,
        &[],
    )
    .unwrap();
    let content = encoded(&json!({"request_id":request,"atoms":[{
        "concept":"扩散", "kind":"definition", "content":"扩散沿浓度梯度发生。",
        "qualifiers":["固定条件"],"evidence":[{"relation":"supports","quote":QUOTE}],
        "measurement":null
    }]}))
    .unwrap();
    let response = serde_json::to_vec(&json!({
        "id":"chatcmpl-authored-compact", "object":"chat.completion", "model":fixture.config.model,
        "choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":content}}],
        "usage":{"prompt_tokens":100,"completion_tokens":50,"total_tokens":150}
    })).unwrap();
    let request_pin = sha256(&prepared.body);
    let mut attempt = store
        .begin_attempt(&run, &unit, &request, &request_pin)
        .unwrap();
    let original_dir = store.root.join(&attempt.directory);
    let receipt = serde_json::to_vec(&provider::Receipt {
        schema_version: 1,
        request_id: request,
        request_sha256: request_pin,
        status: Some(200),
        network_attempts: 1,
        complete: true,
        response_sha256: sha256(&response),
        response_bytes: response.len() as u64,
        failure_code: None,
    })
    .unwrap();
    fs::write(original_dir.join("request-body.json"), &prepared.body).unwrap();
    fs::write(original_dir.join("response.bin"), &response).unwrap();
    fs::write(original_dir.join("receipt.json"), &receipt).unwrap();
    attempt.state = "sent".into();
    store.update_attempt(&run, &attempt).unwrap();
    let mut budget = fixture.authority();
    fixture.reserve_sent(&mut budget, &run, &attempt);
    assert_eq!(store.summary(&run).unwrap().ir_revision, 0);
    assert_eq!(budget.status().unwrap().pending_reserved_tokens, 2000);
    drop(store);
    drop(budget);

    let store = Store::open(&fixture.project).unwrap().unwrap();
    let config = store.config(&run).unwrap();
    let mut budget = BudgetAuthority::open(&config.usage_ledger).unwrap();
    assert_eq!(config.strategy_version().unwrap(), COMPACT_REQUEST_STRATEGY);
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    for _ in 0..2 {
        super::super::execute(
            &fixture.project,
            &run,
            &store,
            &config,
            "noetome-test-artificial-credential-never-in-source",
            &mut budget,
            &cancel,
        )
        .unwrap();
        let summary = store.summary(&run).unwrap();
        assert_eq!(
            (
                summary.ir_revision,
                summary.units_confirmed,
                summary.atoms,
                summary.evidence
            ),
            (1, 1, 1, 1)
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
        let attempts = store.attempts(&run).unwrap();
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].state, "accepted");
        assert!(store.attempts_needing_recovery(&run).unwrap().is_empty());
    }
    drop(store);
    drop(budget);
    assert_eq!(
        fs::read(original_dir.join("request-body.json")).unwrap(),
        prepared.body
    );
    assert_eq!(
        fs::read(original_dir.join("response.bin")).unwrap(),
        response
    );
    assert_eq!(
        fs::read(original_dir.join("receipt.json")).unwrap(),
        receipt
    );

    let output = fixture._temp.path().join("compact-recovered-vault");
    let exported = super::super::export(&fixture.project, &run, &output).unwrap();
    assert_eq!(
        exported["summary"]["request_strategy_version"],
        COMPACT_REQUEST_STRATEGY
    );
    let read_index = |name: &str| -> Value {
        let raw = fs::read_to_string(output.join(".noetome-index").join(name)).unwrap();
        serde_json::from_str(&raw).unwrap()
    };
    let atom = read_index("atoms.ndjson");
    let excerpt = read_index("evidence.ndjson");
    assert_eq!(atom["verification"], "unverified");
    assert_eq!(
        atom["evidence_links"],
        json!([{"evidence_id":excerpt["evidence_id"],"relation":"supports"}])
    );
    assert_eq!(excerpt["page_sha256"], unit.page_sha256);
    assert_eq!(excerpt["start_byte"], PREFIX.len());
    assert_eq!(excerpt["end_byte"], PREFIX.len() + QUOTE.len());
    assert_eq!(excerpt["quote"], QUOTE);
    assert_eq!(excerpt["excerpt_sha256"], sha256(QUOTE.as_bytes()));
    let note_id = read_index("notes.ndjson")["note_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let note = fs::read_to_string(output.join("知识").join(format!("{note_id}.md"))).unwrap();
    assert!(note.contains("来源支持（物理页 1）：扩散沿浓度梯度发生"));
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
    let initial_summary = store.summary(&run).unwrap();
    assert_eq!(
        (
            initial_summary.selected_pages,
            initial_summary.prepared_pages,
            initial_summary.unprepared_pages,
            initial_summary.failed_pages
        ),
        (3, 0, 3, 0)
    );
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
    let final_summary = store.summary(&run).unwrap();
    assert_eq!(
        (
            final_summary.prepared_pages,
            final_summary.unprepared_pages,
            final_summary.failed_pages
        ),
        (3, 0, 1)
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
fn scope_page_ordinals_support_last_u32_physical_page() {
    let mut source = SourceScope {
        source_id: "authored-scope-boundary".into(),
        source_sha256: "a".repeat(64),
        display_name: "authored.pdf".into(),
        byte_length: 1,
        physical_pages: u32::MAX,
        scope_schema_version: None,
        page_ranges: None,
    };
    source.validate_ranges().unwrap();
    assert_eq!(source.selected_pages(), u64::from(u32::MAX));
    assert_eq!(source.page_at(u32::MAX - 1), Some(u32::MAX));
    assert_eq!(source.page_at(u32::MAX), None);
    source.scope_schema_version = Some(1);
    source.page_ranges = Some(vec![PageRange {
        start_page: 1,
        end_page: u32::MAX,
    }]);
    source.validate_ranges().unwrap();
    assert_eq!(source.selected_pages(), u64::from(u32::MAX));
    assert_eq!(source.page_at(u32::MAX - 1), Some(u32::MAX));
    assert_eq!(source.page_at(u32::MAX), None);
    source.page_ranges = Some(vec![
        PageRange {
            start_page: 2,
            end_page: 3,
        },
        PageRange {
            start_page: u32::MAX,
            end_page: u32::MAX,
        },
    ]);
    source.validate_ranges().unwrap();
    assert_eq!(source.selected_pages(), 3);
    assert_eq!(
        (
            source.page_at(0),
            source.page_at(1),
            source.page_at(2),
            source.page_at(3)
        ),
        (Some(2), Some(3), Some(u32::MAX), None)
    );
    for page in [0, 1, 4, u32::MAX - 1] {
        assert!(!source.contains_page(page));
    }
    assert!(source.contains_page(u32::MAX));
}

#[test]
fn scope_metadata_rejects_partial_and_unknown_versions() {
    let legacy = json!({"source_id":"authored-scope-metadata","source_sha256":"a".repeat(64),
        "display_name":"authored.pdf","byte_length":1,"physical_pages":3});
    let mut source: SourceScope = serde_json::from_value(legacy.clone()).unwrap();
    source.validate_ranges().unwrap();
    assert_eq!(serde_json::to_value(&source).unwrap(), legacy);
    let range = PageRange {
        start_page: 2,
        end_page: 3,
    };
    for (version, ranges) in [
        (Some(1), None),
        (None, Some(vec![range.clone()])),
        (Some(2), Some(vec![range])),
        (Some(1), Some(vec![])),
    ] {
        source.scope_schema_version = version;
        source.page_ranges = ranges;
        assert_eq!(
            source.validate_ranges().unwrap_err().code(),
            "COMPILE_SCOPE"
        );
    }
    source.scope_schema_version = None;
    source.page_ranges = None;
    source.physical_pages = 0;
    assert_eq!(
        source.validate_ranges().unwrap_err().code(),
        "COMPILE_SCOPE"
    );
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
