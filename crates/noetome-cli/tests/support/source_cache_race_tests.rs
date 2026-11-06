// Source-cache races use synthetic 16 MiB raw captures and an observed Windows
// sharing violation. They establish the cache boundary, not PDF text quality.
use super::*;

#[derive(Debug, PartialEq, Eq)]
enum Mutation {
    Applied,
    Blocked,
}

fn mutate_when_raw_is_guarded(
    raw: PathBuf,
    target: PathBuf,
    mode: u8,
) -> std::thread::JoinHandle<Mutation> {
    use std::os::windows::fs::OpenOptionsExt;
    let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(0);
    let (start_sender, start_receiver) = std::sync::mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        let mut probe = fs::OpenOptions::new();
        probe.write(true).share_mode(7);
        let mut deadline = None;
        loop {
            // Start validation only after this worker has prepared the actual
            // raw-file probe and entered its loop. The observation deadline
            // starts after the caller releases this ready worker, not at spawn.
            let deadline = *deadline.get_or_insert_with(|| {
                ready_sender.send(()).unwrap();
                start_receiver.recv().unwrap();
                std::time::Instant::now() + std::time::Duration::from_secs(5)
            });
            match probe.open(&raw) {
                Ok(file) => drop(file),
                Err(error) if error.raw_os_error() == Some(32) => {
                    let mutation = match mode {
                        0 => fs::write(&target, b"unknown cache sentinel"),
                        1 => {
                            fs::create_dir(target.parent().unwrap()).unwrap();
                            fs::write(&target, b"unknown cache sentinel")
                        }
                        2 => fs::write(
                            &target,
                            b"{\"schema_version\":1,\"project_id\":\"foreign-project\"}",
                        ),
                        3 => {
                            let bytes = fs::read(&target).unwrap();
                            // The replacement is ours and is outside the source
                            // cache; a blocked deletion leaves its inventory intact.
                            let replacement = target
                                .parent().unwrap().parent().unwrap()
                                .join("test-owner-replacement");
                            fs::write(&replacement, bytes).unwrap();
                            match fs::remove_file(&target) {
                                Ok(()) => fs::rename(&replacement, &target),
                                Err(error) => {
                                    fs::remove_file(&replacement).unwrap();
                                    Err(error)
                                }
                            }
                        }
                        _ => unreachable!(),
                    };
                    let outcome = match mutation {
                        Ok(()) => Mutation::Applied,
                        Err(error) if mode >= 2 && error.raw_os_error() == Some(32) => {
                            Mutation::Blocked
                        }
                        Err(error) => panic!("unexpected cache mutation failure: {error}"),
                    };
                    eprintln!("Observed live raw sharing violation; mode={mode}, outcome={outcome:?}");
                    return outcome;
                }
                Err(error) => panic!("unexpected raw probe error: {error}"),
            }
            assert!(std::time::Instant::now() < deadline, "no live raw guard observed");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });
    ready_receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("raw-cache mutation worker did not become ready");
    start_sender.send(()).unwrap();
    worker
}

fn large_host() -> (Sandbox, Connection, crate::source_model::Store) {
    let sandbox = Sandbox::new();
    host_snapshots(&sandbox.0);
    let connection = host_database();
    let raw = vec![b'x'; 16 * 1024 * 1024];
    let hash = crate::text::sha256(&raw);
    let fingerprint = Fingerprint { sha256: hash.clone(), byte_length: raw.len() as u64 };
    let key = cache_key(&fingerprint, &ParserIdentity::pinned(), &"a".repeat(64), DEFAULT_IMPORT_OPTIONS).unwrap();
    connection.execute("UPDATE source_versions SET content_sha256=?1,byte_length=?2", params![hash, raw.len() as i64]).unwrap();
    connection.execute("UPDATE pdf_parses SET cache_key=?1", params![key]).unwrap();
    fs::write(sandbox.0.join(".noetome-sources").join(VERSION).join("raw.pdf"), raw).unwrap();
    let store = crate::source_model::Store::load(&connection, PROJECT).unwrap();
    store.validate_snapshots(&sandbox.0, PROJECT, None).unwrap();
    (sandbox, connection, store)
}

#[test]
fn unknown_source_cache_entry_added_during_raw_read_is_rejected() {
    let (sandbox, _connection, store) = large_host();
    let cache = sandbox.0.join(".noetome-sources");
    let target = cache.join("human-review.txt");
    let worker = mutate_when_raw_is_guarded(cache.join(VERSION).join("raw.pdf"), target.clone(), 0);
    let result = store.validate_snapshots(&sandbox.0, PROJECT, None);
    assert_eq!(worker.join().unwrap(), Mutation::Applied);
    assert_eq!(fs::read(&target).unwrap(), b"unknown cache sentinel");
    assert!(store.validate_snapshots(&sandbox.0, PROJECT, None).is_err());
    assert!(result.is_err(), "new unindexed source-cache entry escaped final verification");
}

#[test]
fn initially_absent_source_cache_created_during_staged_read_is_rejected() {
    let (sandbox, _connection, store) = large_host();
    let cache = sandbox.0.join(".noetome-sources");
    let writer = sandbox.0.join(".noetome-writing");
    fs::create_dir(&writer).unwrap();
    let stage = writer.join("version");
    fs::rename(cache.join(VERSION), &stage).unwrap();
    fs::remove_file(cache.join(".owner.json")).unwrap();
    fs::remove_dir(&cache).unwrap();
    store.validate_snapshots(&sandbox.0, PROJECT, Some((VERSION, &stage))).unwrap();
    let target = cache.join("human-review.txt");
    let worker = mutate_when_raw_is_guarded(stage.join("raw.pdf"), target.clone(), 1);
    let result = store.validate_snapshots(&sandbox.0, PROJECT, Some((VERSION, &stage)));
    assert_eq!(worker.join().unwrap(), Mutation::Applied);
    assert_eq!(fs::read(&target).unwrap(), b"unknown cache sentinel");
    assert!(store.validate_snapshots(&sandbox.0, PROJECT, Some((VERSION, &stage))).is_err());
    assert!(result.is_err(), "a new source-cache root escaped the final absence check");
}

fn owner_trial(mode: u8) {
    let sandbox = Sandbox::new();
    let connection = host_database();
    connection.execute_batch("DELETE FROM pdf_imports; DELETE FROM pdf_resources; DELETE FROM pdf_parses;").unwrap();
    let raw = vec![b'x'; 16 * 1024 * 1024];
    let hash = crate::text::sha256(&raw);
    connection.execute("UPDATE source_versions SET media_type='text/plain',display_name='synthetic.txt',content_sha256=?1,byte_length=?2,normalized_sha256=?1,normalization_version='utf8-newlines-v1',ingest_status='ready',raw_snapshot=?3,normalized_snapshot=?4", params![hash, raw.len() as i64, format!(".noetome-sources/{VERSION}/raw.bin"), format!(".noetome-sources/{VERSION}/normalized.txt")]).unwrap();
    connection.execute("INSERT INTO source_spans VALUES('d4cc1111-0812-4bac-9fc1-c71782669045',?1,0,'text',0,?2,1,1,?3,'blocks-v1','parsed')", params![VERSION, raw.len() as i64, hash]).unwrap();
    connection.execute("INSERT INTO source_imports(rowid,import_id,imported_at,selected_path,source_id,version_id,status,reused,span_count,pending_extraction_count,knowledge_status,error_code,error_message) VALUES(1,?1,?2,'D:/selected/synthetic.txt',?3,?4,'ready',0,1,1,'pending',NULL,NULL)", params![IMPORT,TIME,SOURCE,VERSION]).unwrap();
    connection.execute("INSERT INTO pdf_imports(rowid,import_id,imported_at,selected_path,source_id,version_id,parse_id,status,reused,parse_reused,acquisition_status,error_code,error_message) VALUES(2,'12facf7a-4fbd-42a7-8c2f-f96208bf740e',?1,'D:/selected/missing.pdf',NULL,NULL,NULL,'failed',0,0,'failed','SOURCE_READ_ERROR','controlled unavailable selected PDF')", params![TIME]).unwrap();
    let cache = sandbox.0.join(".noetome-sources");
    fs::create_dir(&cache).unwrap();
    fs::write(cache.join(".owner.json"), format!("{{\"schema_version\":1,\"project_id\":\"{PROJECT}\"}}")).unwrap();
    let version = cache.join(VERSION);
    fs::create_dir(&version).unwrap();
    fs::write(version.join("raw.bin"), &raw).unwrap();
    fs::write(version.join("normalized.txt"), &raw).unwrap();
    let store = crate::source_model::Store::load(&connection, PROJECT).unwrap();
    store.validate_snapshots(&sandbox.0, PROJECT, None).unwrap();
    let owner = cache.join(".owner.json");
    let before_identity = same_file::Handle::from_path(&owner).unwrap();
    let before_bytes = fs::read(&owner).unwrap();
    let worker = mutate_when_raw_is_guarded(version.join("raw.bin"), owner.clone(), mode);
    let result = store.validate_snapshots(&sandbox.0, PROJECT, None);
    match worker.join().unwrap() {
        Mutation::Blocked => {
            result.unwrap();
            assert_eq!(fs::read(&owner).unwrap(), before_bytes);
            assert_eq!(same_file::Handle::from_path(&owner).unwrap(), before_identity);
            store.validate_snapshots(&sandbox.0, PROJECT, None).unwrap();
        }
        Mutation::Applied => {
            if mode == 2 {
                assert_ne!(fs::read(&owner).unwrap(), before_bytes);
                assert!(store.validate_snapshots(&sandbox.0, PROJECT, None).is_err());
            } else {
                assert_eq!(fs::read(&owner).unwrap(), before_bytes);
                assert_ne!(same_file::Handle::from_path(&owner).unwrap(), before_identity);
                store.validate_snapshots(&sandbox.0, PROJECT, None).unwrap();
            }
            assert!(result.is_err(), "source-cache owner changed during validation without rejection");
        }
    }
}

#[test]
fn owner_bytes_are_guarded_without_an_acquired_pdf_version() {
    owner_trial(2);
}

#[test]
fn owner_identity_is_guarded_without_an_acquired_pdf_version() {
    owner_trial(3);
}
