//! Artificial ledgers only. These tests never read local model configuration or HTTP.
use super::*;
use serde_json::json;
use std::{
    sync::{Arc, Barrier},
    thread,
};

struct Fixture {
    root: PathBuf,
    ledger: PathBuf,
    bytes: Vec<u8>,
}
impl Fixture {
    fn new(cap: u64, history: &[u64], held: u64) -> Self {
        let root = std::env::temp_dir().join(format!(
            "noetome-production-budget-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&root).unwrap();
        let ledger = root.join("selected.usage.json");
        let mut records:Vec<Value>=history.iter().enumerate().map(|(i,n)|json!({"invocation_id":format!("history-{i}"),"reservation_tokens":100000,"confirmed_total_tokens":n,"network_attempts":1,"status":"settled_valid_usage_text","preserved_extra":{"field":"original history"}})).collect();
        if held > 0 {
            records.push(json!({"invocation_id":"old-unknown","reservation_tokens":held,"confirmed_total_tokens":null,"network_attempts":1,"status":"usage_unknown"}));
        }
        let accounted: u64 = history.iter().sum();
        let value = json!({"schema_version":1,"approved_total_tokens":cap,"accounted_tokens":accounted,"pending_reserved_tokens":held,"remaining_tokens":cap-accounted-held,"model_attempts":history.len()+usize::from(held>0),"records":records,"updated_at":"2026-11-16T00:00:00Z"});
        let bytes = serde_json::to_vec_pretty(&value).unwrap();
        fs::write(&ledger, &bytes).unwrap();
        Self {
            root,
            ledger,
            bytes,
        }
    }
    fn migrate(&self) -> BudgetAuthority {
        BudgetAuthority::migrate(&self.ledger).unwrap()
    }
    fn assert_original(&self) {
        assert_eq!(fs::read(&self.ledger).unwrap(), self.bytes);
        assert!(fs::metadata(&self.ledger).unwrap().permissions().readonly());
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Ok(metadata) = fs::metadata(&self.ledger) {
            let mut permission = metadata.permissions();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                permission.set_mode(0o600);
            }
            #[cfg(not(unix))]
            permission.set_readonly(false);
            let _ = fs::set_permissions(&self.ledger, permission);
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn request(run: &str, unit: &str, tokens: u64) -> BudgetRequest {
    BudgetRequest {
        key: BudgetAttemptKey {
            run_id: run.into(),
            unit_id: unit.into(),
            attempt_no: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
        },
        request_sha256: sha256(format!("{run}:{unit}").as_bytes()),
        reserved_tokens: tokens,
    }
}
fn known(tokens: u64) -> BudgetSettlement {
    BudgetSettlement {
        network_attempts: 1,
        usage: Some(BudgetUsage {
            input_tokens: tokens / 2,
            output_tokens: tokens - tokens / 2,
            total_tokens: tokens,
        }),
        receipt_sha256: sha256(format!("usage:{tokens}").as_bytes()),
    }
}
fn unknown() -> BudgetSettlement {
    BudgetSettlement {
        network_attempts: 1,
        usage: None,
        receipt_sha256: sha256(b"trusted unknown transport"),
    }
}
fn no_request() -> BudgetSettlement {
    BudgetSettlement {
        network_attempts: 0,
        usage: None,
        receipt_sha256: sha256(b"trusted no-network transport"),
    }
}

#[test]
fn identity_guards_preserve_another_connections_sqlite_transaction_lock() {
    const MODE: &str = "NOETOME_TEST_BUDGET_LOCK_PROBE";
    const PATH: &str = "NOETOME_TEST_BUDGET_LOCK_DATABASE";
    if std::env::var(MODE).as_deref() == Ok("synthetic-child") {
        let path = PathBuf::from(std::env::var_os(PATH).expect("selected synthetic database"));
        let mut connection = Connection::open(&path).unwrap();
        connection.busy_timeout(Duration::from_millis(50)).unwrap();
        let attempt = connection.transaction_with_behavior(TransactionBehavior::Immediate);
        assert!(
            attempt.is_err(),
            "another process acquired the writer lock while the original transaction remained active"
        );
        return;
    }
    let fixture = Fixture::new(1000, &[100], 0);
    let mut writer = fixture.migrate();
    let mut reader = BudgetAuthority::open(&fixture.ledger).unwrap();
    let transaction = writer
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    // This second authority shares our process but not our SQLite connection.
    // Identity checks must not release the first connection's POSIX locks.
    reader.guard().unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "production_budget::tests::identity_guards_preserve_another_connections_sqlite_transaction_lock", "--nocapture"])
        .env(MODE, "synthetic-child")
        .env(PATH, &writer.database)
        .output().unwrap();
    drop(transaction);
    assert!(
        result.status.success(),
        "independent process lock probe failed: {}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    fixture.assert_original();
}

#[test]
fn linked_sqlite_journal_is_rejected_before_reserving_or_touching_its_target() {
    let fixture = Fixture::new(1000, &[100], 0);
    let mut authority = fixture.migrate();
    let protected = fixture.root.join("protected-synthetic-file");
    let original = b"ordinary synthetic target must stay unchanged";
    fs::write(&protected, original).unwrap();
    let mut journal_name = authority.database.as_os_str().to_os_string();
    journal_name.push("-journal");
    let journal = PathBuf::from(journal_name);
    fs::hard_link(&protected, &journal).unwrap();
    assert!(
        authority
            .reserve(&request("run", "linked-journal", 100))
            .is_err()
    );
    assert!(BudgetAuthority::open(&fixture.ledger).is_err());
    assert_eq!(fs::read(&protected).unwrap(), original);
    fs::remove_file(journal).unwrap();
    assert_eq!(authority.status().unwrap().attempt_count, 0);
    fixture.assert_original();
}

#[test]
fn complete_ten_record_baseline_is_preserved_without_new_quota() {
    let fixture = Fixture::new(
        10_000_000,
        &[936, 9683, 4912, 4783, 3479, 6304, 654, 776, 1070, 588],
        0,
    );
    let mut authority = fixture.migrate();
    let state = authority.status().unwrap();
    assert_eq!(
        (
            state.approved_total_tokens,
            state.accounted_tokens,
            state.pending_reserved_tokens,
            state.remaining_tokens,
            state.model_attempts,
            state.history_count
        ),
        (10_000_000, 33185, 0, 9966815, 10, 10)
    );
    assert_eq!(state.baseline_sha256, sha256(&fixture.bytes));
    fixture.assert_original();
    let stored: Vec<u8> = authority
        .connection
        .query_row("SELECT baseline FROM metadata", [], |r| r.get(0))
        .unwrap();
    assert_eq!(stored, fixture.bytes);
    assert!(BudgetAuthority::migrate(&fixture.ledger).is_err());
    drop(authority);
    assert_eq!(
        BudgetAuthority::open(&fixture.ledger)
            .unwrap()
            .status()
            .unwrap(),
        state
    );
}

#[test]
fn near_limit_known_usage_releases_only_unused_reservation() {
    let fixture = Fixture::new(1000, &[100], 0);
    let mut authority = fixture.migrate();
    let first = request("run1", "one", 850);
    authority.reserve(&first).unwrap();
    authority.mark_sent(&first.key).unwrap();
    assert!(authority.reserve(&request("run2", "two", 51)).is_err());
    authority.settle(&first.key, &known(700)).unwrap();
    let state = authority.status().unwrap();
    assert_eq!(
        (
            state.accounted_tokens,
            state.remaining_tokens,
            state.model_attempts
        ),
        (800, 200, 2)
    );
    authority.reserve(&request("run2", "two", 200)).unwrap();
    assert!(authority.status().unwrap().stopped);
}

#[test]
fn unknown_and_prior_pending_survive_reopen_and_new_attempts() {
    let fixture = Fixture::new(1000, &[100], 100);
    let mut authority = fixture.migrate();
    let first = request("run", "unit", 500);
    authority.reserve(&first).unwrap();
    authority.mark_sent(&first.key).unwrap();
    authority.settle(&first.key, &unknown()).unwrap();
    let state = authority.status().unwrap();
    assert_eq!(
        (
            state.accounted_tokens,
            state.pending_reserved_tokens,
            state.remaining_tokens,
            state.model_attempts
        ),
        (100, 600, 300, 3)
    );
    drop(authority);
    let mut reopened = BudgetAuthority::open(&fixture.ledger).unwrap();
    assert_eq!(reopened.status().unwrap(), state);
    assert_eq!(
        reopened.attempt(&first.key).unwrap().unwrap().state,
        BudgetAttemptState::Unknown
    );
    assert!(reopened.settle(&first.key, &no_request()).is_err());
    assert!(reopened.settle(&first.key, &known(2)).is_err());
    let mut retry = first.clone();
    retry.key.attempt_no = 2;
    retry.key.request_id = uuid::Uuid::new_v4().to_string();
    retry.reserved_tokens = 300;
    reopened.reserve(&retry).unwrap();
    assert_eq!(reopened.status().unwrap().pending_reserved_tokens, 900);
}

#[test]
fn sent_without_a_receipt_remains_pending_after_root_exit() {
    let fixture = Fixture::new(1000, &[], 0);
    let mut authority = fixture.migrate();
    let first = request("run", "unit", 400);
    authority.reserve(&first).unwrap();
    authority.mark_sent(&first.key).unwrap();
    drop(authority);
    let mut reopened = BudgetAuthority::open(&fixture.ledger).unwrap();
    assert_eq!(
        reopened.attempt(&first.key).unwrap().unwrap().state,
        BudgetAttemptState::Sent
    );
    assert_eq!(reopened.status().unwrap().pending_reserved_tokens, 400);
    assert!(reopened.mark_sent(&first.key).is_err());
}

#[test]
fn settlement_is_exactly_idempotent_and_conflicts_are_rejected() {
    let fixture = Fixture::new(1000, &[], 0);
    let mut authority = fixture.migrate();
    let first = request("run", "unit", 400);
    authority.reserve(&first).unwrap();
    authority.reserve(&first).unwrap();
    authority.mark_sent(&first.key).unwrap();
    let receipt = known(100);
    let settled = authority.settle(&first.key, &receipt).unwrap();
    let state = authority.status().unwrap();
    assert_eq!(authority.settle(&first.key, &receipt).unwrap(), settled);
    assert_eq!(authority.status().unwrap(), state);
    let mut altered = receipt.clone();
    altered.receipt_sha256 = sha256(b"different response");
    assert!(authority.settle(&first.key, &altered).is_err());
    assert!(authority.settle(&first.key, &known(101)).is_err());
    drop(authority);
    assert_eq!(
        BudgetAuthority::open(&fixture.ledger)
            .unwrap()
            .settle(&first.key, &receipt)
            .unwrap(),
        settled
    );
}

#[test]
fn actual_overrun_is_recorded_and_blocks_new_reserve_and_send() {
    let fixture = Fixture::new(1000, &[100], 0);
    let mut authority = fixture.migrate();
    let first = request("run", "one", 800);
    let second = request("run", "two", 100);
    authority.reserve(&first).unwrap();
    authority.reserve(&second).unwrap();
    authority.mark_sent(&first.key).unwrap();
    authority.settle(&first.key, &known(1200)).unwrap();
    let state = authority.status().unwrap();
    assert_eq!(
        (
            state.accounted_tokens,
            state.pending_reserved_tokens,
            state.remaining_tokens,
            state.overrun_tokens
        ),
        (1300, 100, 0, 400)
    );
    assert!(state.stopped);
    assert!(authority.reserve(&request("next", "unit", 1)).is_err());
    assert!(authority.mark_sent(&second.key).is_err());
    authority.settle(&second.key, &no_request()).unwrap();
    assert_eq!(authority.status().unwrap().overrun_tokens, 300);
}

#[test]
fn no_network_proof_releases_reserved_or_sent_without_invented_usage() {
    let fixture = Fixture::new(1000, &[], 0);
    let mut authority = fixture.migrate();
    for (unit, sent) in [("prepared", false), ("may-send", true)] {
        let first = request("run", unit, 400);
        authority.reserve(&first).unwrap();
        if sent {
            authority.mark_sent(&first.key).unwrap();
        }
        assert_eq!(
            authority.settle(&first.key, &no_request()).unwrap().state,
            BudgetAttemptState::NotSent
        );
    }
    let state = authority.status().unwrap();
    assert_eq!(
        (
            state.accounted_tokens,
            state.pending_reserved_tokens,
            state.remaining_tokens,
            state.model_attempts
        ),
        (0, 0, 1000, 0)
    );
}

#[test]
fn multiple_authorities_compete_atomically_across_runs() {
    let fixture = Fixture::new(1000, &[100], 0);
    drop(fixture.migrate());
    let barrier = Arc::new(Barrier::new(8));
    let ledger = fixture.ledger.clone();
    let workers: Vec<_> = (0..8)
        .map(|i| {
            let barrier = barrier.clone();
            let ledger = ledger.clone();
            thread::spawn(move || {
                let mut authority = BudgetAuthority::open(&ledger).unwrap();
                barrier.wait();
                authority
                    .reserve(&request(&format!("run{i}"), "unit", 200))
                    .is_ok()
            })
        })
        .collect();
    let admitted = workers
        .into_iter()
        .map(|w| w.join().unwrap())
        .filter(|v| *v)
        .count();
    assert_eq!(admitted, 4);
    let state = BudgetAuthority::open(&fixture.ledger)
        .unwrap()
        .status()
        .unwrap();
    assert_eq!(
        (
            state.pending_reserved_tokens,
            state.remaining_tokens,
            state.attempt_count
        ),
        (800, 100, 4)
    );
}

#[test]
fn request_uuid_and_logical_attempt_cannot_fork_or_change_body() {
    let fixture = Fixture::new(1000, &[], 0);
    let mut authority = fixture.migrate();
    let first = request("run", "unit", 100);
    authority.reserve(&first).unwrap();
    let mut changed = first.clone();
    changed.request_sha256 = sha256(b"changed");
    assert!(authority.reserve(&changed).is_err());
    let mut changed = first.clone();
    changed.key.request_id = uuid::Uuid::new_v4().to_string();
    assert!(authority.reserve(&changed).is_err());
    let mut changed = first.clone();
    changed.key.run_id = "other".into();
    assert!(authority.reserve(&changed).is_err());
}

#[test]
fn invalid_usage_and_unmarked_network_receipt_do_not_mutate_budget() {
    let fixture = Fixture::new(1000, &[], 0);
    let mut authority = fixture.migrate();
    let first = request("run", "unit", 400);
    authority.reserve(&first).unwrap();
    let state = authority.status().unwrap();
    assert!(authority.settle(&first.key, &known(100)).is_err());
    authority.mark_sent(&first.key).unwrap();
    let mut invalid = known(100);
    invalid.usage.as_mut().unwrap().total_tokens = 101;
    assert!(authority.settle(&first.key, &invalid).is_err());
    let mut invalid = no_request();
    invalid.usage = Some(BudgetUsage {
        input_tokens: 1,
        output_tokens: 0,
        total_tokens: 1,
    });
    assert!(authority.settle(&first.key, &invalid).is_err());
    assert_eq!(
        authority.status().unwrap().pending_reserved_tokens,
        state.pending_reserved_tokens
    );
}

#[test]
fn history_owner_baseline_event_and_schema_tampering_are_rejected() {
    for mutation in [
        "history", "owner", "baseline", "event", "schema", "head", "trigger",
    ] {
        let fixture = Fixture::new(1000, &[100], 0);
        let mut authority = fixture.migrate();
        let first = request("run", "unit", 100);
        authority.reserve(&first).unwrap();
        drop(authority);
        let connection = Connection::open(default_database_path(&fixture.ledger)).unwrap();
        match mutation {
            "history" => connection
                .execute("UPDATE history SET record_json='{}'", [])
                .unwrap(),
            "owner" => connection
                .execute("UPDATE metadata SET owner_json='{}'", [])
                .unwrap(),
            "baseline" => connection
                .execute("UPDATE metadata SET baseline=x'00'", [])
                .unwrap(),
            "event" => connection
                .execute("UPDATE events SET payload_json='{}'", [])
                .unwrap(),
            "head" => connection.execute("DELETE FROM events", []).unwrap(),
            "schema" => connection
                .execute("CREATE TABLE unrecognized(value TEXT)", [])
                .unwrap(),
            "trigger" => connection
                .execute(
                    "CREATE TRIGGER tamper AFTER INSERT ON events BEGIN DELETE FROM history; END",
                    [],
                )
                .unwrap(),
            _ => unreachable!(),
        };
        drop(connection);
        assert!(
            BudgetAuthority::open(&fixture.ledger).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn malformed_history_arithmetic_and_duplicate_nested_keys_never_migrate() {
    for mutation in [
        "accounted",
        "pending",
        "attempts",
        "duplicate",
        "unknown_status",
    ] {
        let fixture = Fixture::new(1000, &[100], 0);
        let mut ledger: Value = serde_json::from_slice(&fixture.bytes).unwrap();
        match mutation {
            "accounted" => ledger["accounted_tokens"] = json!(99),
            "pending" => ledger["pending_reserved_tokens"] = json!(1),
            "attempts" => ledger["model_attempts"] = json!(0),
            "unknown_status" => ledger["records"][0]["status"] = json!("invented"),
            "duplicate" => {}
            _ => unreachable!(),
        }
        let raw = if mutation == "duplicate" {
            serde_json::to_string(&ledger).unwrap().replace(
                "\"field\":\"original history\"",
                "\"field\":\"first\",\"field\":\"original history\"",
            )
        } else {
            serde_json::to_string(&ledger).unwrap()
        };
        fs::write(&fixture.ledger, raw).unwrap();
        assert!(
            BudgetAuthority::migrate(&fixture.ledger).is_err(),
            "{mutation}"
        );
        assert!(!authority_marker_path(&fixture.ledger).exists());
        assert!(!default_database_path(&fixture.ledger).exists());
    }
}

#[test]
fn partial_or_missing_authority_never_silently_reinitializes() {
    let fixture = Fixture::new(1000, &[], 0);
    fs::write(
        authority_marker_path(&fixture.ledger),
        b"{\"migration_incomplete\":true}",
    )
    .unwrap();
    assert!(BudgetAuthority::migrate(&fixture.ledger).is_err());
    assert!(BudgetAuthority::open(&fixture.ledger).is_err());
    assert!(!default_database_path(&fixture.ledger).exists());
    let fixture = Fixture::new(1000, &[], 0);
    drop(fixture.migrate());
    fs::remove_file(default_database_path(&fixture.ledger)).unwrap();
    assert!(BudgetAuthority::migrate(&fixture.ledger).is_err());
    assert!(BudgetAuthority::open(&fixture.ledger).is_err());
    fixture.assert_original();
}

#[test]
fn removing_readonly_attribute_is_not_repaired_on_open() {
    let fixture = Fixture::new(1000, &[], 0);
    drop(fixture.migrate());
    let mut permissions = fs::metadata(&fixture.ledger).unwrap().permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o600);
    }
    #[cfg(not(unix))]
    permissions.set_readonly(false);
    fs::set_permissions(&fixture.ledger, permissions).unwrap();
    assert!(BudgetAuthority::open(&fixture.ledger).is_err());
    assert!(
        !fs::metadata(&fixture.ledger)
            .unwrap()
            .permissions()
            .readonly()
    );
}

#[cfg(windows)]
#[test]
fn migrated_json_refuses_legacy_dotnet_readwrite_open_with_zero_requests() {
    let fixture = Fixture::new(1000, &[], 0);
    drop(fixture.migrate());
    let escaped = fixture.ledger.display().to_string().replace('\'', "''");
    let command = format!(
        "$ErrorActionPreference='Stop';try{{$s=[IO.File]::Open('{escaped}',[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::Read);$s.Dispose();exit 1}}catch [UnauthorizedAccessException]{{[Console]::WriteLine('legacy-write-refused;network_attempts=0');exit 0}}"
    );
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &command])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("legacy-write-refused;network_attempts=0")
    );
    fixture.assert_original();
}
