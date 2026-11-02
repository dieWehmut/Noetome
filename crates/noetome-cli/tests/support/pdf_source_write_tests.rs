//! Real PDF registration failures at Writer boundaries. Fault hooks and child
//! controls exist only in this test module; the ordinary CLI gets no switches.
use super::*;
use crate::source;
use rusqlite::types::Value as SqlValue;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::Metadata;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

const SENTINEL: &[u8] = b"Human bytes outside the PDF registration project.\r\n";
const OLD_RAW: &[u8] = b"# Previous source\r\n\r\nKeep this text history.\n";
const OLD_NORMAL: &[u8] = b"# Previous source\n\nKeep this text history.\n";
const PDF_SHA: &str = "1a8bbfe535892b74e2ec1e0fdb6f99320d98e3492441f5f8ae339a57644fc457";
const HUMAN: &[u8] = b"Unregistered human content: do not overwrite or adopt.\n";
const CHILD_PROJECT: &str = "NOETOME_TEST_PDF_WRITER_PROJECT";
const CHILD_STAGE: &str = "NOETOME_TEST_PDF_WRITER_STAGE";
const CHILD_READY: &str = "NOETOME_TEST_PDF_WRITER_READY";
const CHILD_NONCE: &str = "NOETOME_TEST_PDF_WRITER_NONCE";

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_link(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        return metadata.file_attributes() & 0x400 != 0;
    }
    #[cfg(not(windows))]
    false
}

fn ordinary_tree(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if is_link(&metadata) {
        return false;
    }
    if metadata.is_file() {
        return true;
    }
    if !metadata.is_dir() {
        return false;
    }
    fs::read_dir(path).is_ok_and(|entries| {
        entries
            .into_iter()
            .all(|entry| entry.is_ok_and(|entry| ordinary_tree(&entry.path())))
    })
}

struct Scratch {
    root: PathBuf,
    parent: PathBuf,
    nonce: String,
    owner: Vec<u8>,
    identity: same_file::Handle,
    pdf: PathBuf,
    raw_pdf: Vec<u8>,
}

impl Scratch {
    fn new() -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/compiler-temp")
            .canonicalize()
            .expect("cargo_local supplies the project-local compiler temporary root");
        assert!(!is_link(&fs::symlink_metadata(&parent).unwrap()));
        let nonce = Uuid::new_v4().to_string();
        let root = parent.join(format!("noetome-pdf-writer-{nonce}"));
        fs::create_dir(&root).unwrap();
        let owner = serde_json::to_vec(&json!({"nonce": nonce, "root": root})).unwrap();
        fs::write(root.join(".test-owner.json"), &owner).unwrap();
        fs::write(root.join("external-sentinel.bin"), SENTINEL).unwrap();
        let raw_pdf = fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/pdf-text-v1/single-page.pdf"),
        )
        .unwrap();
        assert_eq!(sha(&raw_pdf), PDF_SHA, "use the frozen real authoring PDF");
        let pdf = root.join("selected.pdf");
        fs::write(&pdf, &raw_pdf).unwrap();
        let identity = same_file::Handle::from_path(&root).unwrap();
        Self {
            root,
            parent,
            nonce,
            owner,
            identity,
            pdf,
            raw_pdf,
        }
    }

    fn seed(&self, name: &str) -> Seed {
        let project = self.root.join(name);
        project::initialize(&project).unwrap();
        let old_input = self.root.join(format!("{name}-old.md"));
        fs::write(&old_input, OLD_RAW).unwrap();
        let imported = source::import(&project, &old_input, None).unwrap();
        assert!(imported.failure.is_none());
        let report: Value = serde_json::from_str(&imported.json).unwrap();
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["status"], "ready");
        assert_eq!(report["knowledge_status"], "pending");
        let spans = report["spans"].as_array().unwrap();
        assert_eq!(spans.len(), 3);
        // The normalized literal is 43 bytes: heading including LF [0,18),
        // the blank formatting line [18,19), then text including LF [19,43).
        for (span, (kind, start, end, line)) in spans.iter().zip([
            ("heading", 0, 18, 1),
            ("formatting", 18, 19, 2),
            ("text", 19, 43, 3),
        ]) {
            assert_eq!(span["kind"], kind);
            assert_eq!(span["start_byte"], start);
            assert_eq!(span["end_byte"], end);
            assert_eq!(span["start_line"], line);
            assert_eq!(span["end_line"], line);
        }
        let old_source = report["source_id"].as_str().unwrap().to_owned();
        let raw_path = project.join(report["version"]["raw_snapshot"].as_str().unwrap());
        let normalized_path =
            project.join(report["version"]["normalized_snapshot"].as_str().unwrap());
        assert_eq!(fs::read(&raw_path).unwrap(), OLD_RAW);
        assert_eq!(fs::read(&normalized_path).unwrap(), OLD_NORMAL);
        assert_eq!(report["version"]["content_sha256"], sha(OLD_RAW));
        assert_eq!(report["version"]["normalized_sha256"], sha(OLD_NORMAL));
        let info = project::inspect(&project).unwrap();
        assert_eq!(info.schema_version, 2);
        let old_config = fs::read(project.join(project::CONFIG)).unwrap();
        let old_database = fs::read(project.join(project::DATABASE)).unwrap();
        let old_rows = history_rows(&project.join(project::DATABASE), &old_source);
        Seed {
            project,
            info,
            report,
            report_json: imported.json,
            old_source,
            raw_path,
            normalized_path,
            old_config,
            old_database,
            old_rows,
        }
    }

    fn sentinel(&self) {
        assert_eq!(
            fs::read(self.root.join("external-sentinel.bin")).unwrap(),
            SENTINEL
        );
        assert_eq!(fs::read(&self.pdf).unwrap(), self.raw_pdf);
        assert_eq!(
            fs::read(self.root.join(".test-owner.json")).unwrap(),
            self.owner
        );
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Verify the absolute deletion target, owner bytes, root identity and
        // every native entry before recursive cleanup of this test's own root.
        let checked = self.root.canonicalize().ok().is_some_and(|root| {
            root.parent() == Some(self.parent.as_path())
                && root.file_name().is_some_and(|name| {
                    name.to_string_lossy() == format!("noetome-pdf-writer-{}", self.nonce)
                })
                && same_file::Handle::from_path(&root).is_ok_and(|current| current == self.identity)
                && fs::read(root.join(".test-owner.json")).ok().as_ref() == Some(&self.owner)
                && fs::read(root.join("external-sentinel.bin")).ok().as_deref() == Some(SENTINEL)
                && ordinary_tree(&root)
        });
        if checked {
            if let Err(error) = fs::remove_dir_all(&self.root) {
                eprintln!("owned PDF Writer test root preserved: {error}");
            }
        } else {
            eprintln!(
                "PDF Writer test root changed; preserving {}",
                self.root.display()
            );
        }
    }
}

type History = BTreeMap<&'static str, Vec<Vec<SqlValue>>>;

fn history_rows(database: &Path, source_id: &str) -> History {
    let connection =
        Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let mut rows = BTreeMap::new();
    for (table, query) in [
        (
            "sources",
            "SELECT * FROM sources WHERE source_id=?1 ORDER BY rowid",
        ),
        (
            "source_versions",
            "SELECT * FROM source_versions WHERE source_id=?1 ORDER BY rowid",
        ),
        (
            "source_spans",
            "SELECT s.* FROM source_spans s JOIN source_versions v ON v.version_id=s.source_version_id WHERE v.source_id=?1 ORDER BY s.rowid",
        ),
        (
            "source_imports",
            "SELECT * FROM source_imports WHERE source_id=?1 ORDER BY rowid",
        ),
    ] {
        let mut statement = connection.prepare(query).unwrap();
        let columns = statement.column_count();
        let values = statement
            .query_map([source_id], |row| {
                (0..columns)
                    .map(|index| row.get::<_, SqlValue>(index))
                    .collect()
            })
            .unwrap()
            .collect::<Result<Vec<Vec<SqlValue>>, _>>()
            .unwrap();
        rows.insert(table, values);
    }
    rows
}

struct Seed {
    project: PathBuf,
    info: ProjectInfo,
    report: Value,
    report_json: String,
    old_source: String,
    raw_path: PathBuf,
    normalized_path: PathBuf,
    old_config: Vec<u8>,
    old_database: Vec<u8>,
    old_rows: History,
}

impl Seed {
    fn old_snapshots(&self) {
        assert_eq!(fs::read(&self.raw_path).unwrap(), OLD_RAW);
        assert_eq!(fs::read(&self.normalized_path).unwrap(), OLD_NORMAL);
    }

    fn backups(&self) {
        let marker = self.project.join(MARKER);
        assert_eq!(
            fs::read(marker.join("old-project.json")).unwrap(),
            self.old_config
        );
        assert_eq!(
            fs::read(marker.join("old-project.sqlite")).unwrap(),
            self.old_database
        );
        assert!(marker.join("operation.json").is_file());
        assert_eq!(
            history_rows(&marker.join("old-project.sqlite"), &self.old_source),
            self.old_rows
        );
        self.old_snapshots();
    }

    fn blocked_read_only(&self) {
        let before = tree(&self.project, true);
        assert_eq!(
            project::inspect(&self.project).unwrap_err().code(),
            "PROJECT_INCOMPLETE"
        );
        assert_eq!(
            source::inspect(&self.project, self.report["import_id"].as_str().unwrap())
                .unwrap_err()
                .code(),
            "PROJECT_INCOMPLETE"
        );
        assert_eq!(
            tree(&self.project, true),
            before,
            "inspection must preserve recovery files"
        );
    }

    fn restored(&self, before: &BTreeMap<PathBuf, Entry>) {
        assert!(
            !self.project.join(MARKER).exists(),
            "a known PDF abort must remove its owned writing marker"
        );
        assert_eq!(tree(&self.project, false), *before);
        assert_eq!(project::inspect(&self.project).unwrap(), self.info);
        self.old_snapshots();
        assert_eq!(
            history_rows(&self.project.join(project::DATABASE), &self.old_source),
            self.old_rows
        );
        let readonly_before = tree(&self.project, true);
        assert_eq!(
            source::inspect(&self.project, self.report["import_id"].as_str().unwrap()).unwrap(),
            self.report_json
        );
        assert_eq!(tree(&self.project, true), readonly_before);
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Directory,
    File(Vec<u8>, Option<SystemTime>),
}

fn tree(root: &Path, modified: bool) -> BTreeMap<PathBuf, Entry> {
    fn collect(root: &Path, at: &Path, modified: bool, found: &mut BTreeMap<PathBuf, Entry>) {
        let identity = same_file::Handle::from_path(at).unwrap();
        for entry in fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            assert!(
                !is_link(&metadata),
                "refuse to traverse a native link/reparse entry"
            );
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if metadata.is_dir() {
                found.insert(relative, Entry::Directory);
                collect(root, &path, modified, found);
            } else {
                assert!(metadata.is_file());
                found.insert(
                    relative,
                    Entry::File(
                        fs::read(&path).unwrap(),
                        modified.then(|| metadata.modified().unwrap()),
                    ),
                );
            }
        }
        assert_eq!(same_file::Handle::from_path(at).unwrap(), identity);
    }
    let mut found = BTreeMap::new();
    collect(root, root, modified, &mut found);
    found
}

type Hook = Box<dyn FnMut(WriteStage, &Path) -> Result<(), ProjectError>>;

struct HookReset;
impl HookReset {
    fn install(hook: Hook) -> Self {
        TEST_NEXT_HOOK.with(|next| {
            assert!(
                next.borrow().is_none(),
                "a prior test left an unconsumed hook"
            );
            *next.borrow_mut() = Some(hook);
        });
        Self
    }
}
impl Drop for HookReset {
    fn drop(&mut self) {
        TEST_NEXT_HOOK.with(|next| *next.borrow_mut() = None);
    }
}

fn pdf_version(root: &Path) -> PathBuf {
    let directories: Vec<_> = fs::read_dir(root.join(CACHE))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir() && path.join("raw.pdf").is_file())
        .collect();
    assert_eq!(
        directories.len(),
        1,
        "one actual PDF raw snapshot must be published"
    );
    assert!(valid_uuid(
        directories[0].file_name().unwrap().to_str().unwrap()
    ));
    assert_eq!(
        sha(&fs::read(directories[0].join("raw.pdf")).unwrap()),
        PDF_SHA
    );
    directories[0].clone()
}

fn pdf_parse(root: &Path) -> PathBuf {
    let directories: Vec<_> = fs::read_dir(root.join(".noetome-pdf-parses"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    assert_eq!(
        directories.len(),
        1,
        "one complete actual parse tree must be published"
    );
    let directory = &directories[0];
    assert!(valid_uuid(directory.file_name().unwrap().to_str().unwrap()));
    assert_eq!(
        sha(&fs::read(directory.join("parser/raw.pdf")).unwrap()),
        PDF_SHA
    );
    let report: Value =
        serde_json::from_slice(&fs::read(directory.join("parse-result.json")).unwrap()).unwrap();
    assert_eq!(report["report_kind"], "pdf_parse_sandbox");
    assert_eq!(report["status"], "parsed");
    assert_eq!(report["page_count"], 1);
    assert_eq!(report["input_fingerprint"]["sha256"], PDF_SHA);
    assert!(directory.join("parse-result.md").is_file());
    assert!(directory.join("parser/pdf-manifest.json").is_file());
    directory.clone()
}

fn published_at(root: &Path, stage: WriteStage) {
    if matches!(
        stage,
        WriteStage::Prepared | WriteStage::VersionPublished | WriteStage::ParsePublished
    ) {
        assert!(root.join(MARKER).join("candidate-project.sqlite").is_file());
    }
    if stage != WriteStage::Prepared {
        pdf_version(root);
    }
    if matches!(
        stage,
        WriteStage::ParsePublished
            | WriteStage::DatabasePublished
            | WriteStage::ConfigPublished
            | WriteStage::Validated
            | WriteStage::Cleanup
    ) {
        pdf_parse(root);
    }
    if matches!(
        stage,
        WriteStage::DatabasePublished
            | WriteStage::ConfigPublished
            | WriteStage::Validated
            | WriteStage::Cleanup
    ) {
        let connection = Connection::open_with_flags(
            root.join(project::DATABASE),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let schema: u32 = connection
            .query_row("SELECT schema_version FROM project_metadata", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            schema, 3,
            "the database must be published before announcing its stage"
        );
    }
    if matches!(
        stage,
        WriteStage::ConfigPublished | WriteStage::Validated | WriteStage::Cleanup
    ) {
        let config: Value =
            serde_json::from_slice(&fs::read(root.join(project::CONFIG)).unwrap()).unwrap();
        assert_eq!(config["schema_version"], 3);
    }
}

fn succeeds(scratch: &Scratch, project: &Path) {
    let result = source::import_with_options(project, &scratch.pdf, None, false).unwrap();
    assert!(
        result.failure.is_none(),
        "fresh valid registration must succeed"
    );
    let report: Value = serde_json::from_str(&result.json).unwrap();
    assert_eq!(report["schema_version"], 2);
    assert_eq!(report["status"], "ready");
    assert_eq!(report["acquisition_status"], "acquired");
    assert_eq!(report["pdf"]["status"], "parsed");
    assert_eq!(report["pdf"]["page_count"], 1);
    assert_eq!(report["knowledge_status"], "pending");
    let before = tree(project, true);
    let inspected = source::inspect(project, report["import_id"].as_str().unwrap()).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&inspected).unwrap(), report);
    assert_eq!(tree(project, true), before);
    scratch.sentinel();
}

#[test]
fn controlled_pdf_write_errors_restore_old_text_and_both_project_files_at_seven_stages() {
    let scratch = Scratch::new();
    for point in [
        WriteStage::Prepared,
        WriteStage::VersionPublished,
        WriteStage::ParsePublished,
        WriteStage::DatabasePublished,
        WriteStage::ConfigPublished,
        WriteStage::Validated,
        WriteStage::Cleanup,
    ] {
        let seed = scratch.seed(&format!("abort-{point:?}"));
        let before = tree(&seed.project, false);
        let reached = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&reached);
        let hook = HookReset::install(Box::new(move |stage, root| {
            if stage == point {
                published_at(root, stage);
                observed.store(true, Ordering::SeqCst);
                return Err(ProjectError::new(
                    "PDF_TEST_ABORT",
                    3,
                    "controlled PDF registration failure",
                ));
            }
            Ok(())
        }));
        let error = source::import_with_options(&seed.project, &scratch.pdf, None, false)
            .err()
            .expect("injected Writer failure must prevent success");
        drop(hook);
        assert_eq!(
            error.code(),
            "PDF_TEST_ABORT",
            "wrong failure before {point:?}: {error}"
        );
        assert!(
            reached.load(Ordering::SeqCst),
            "must actually reach {point:?}"
        );
        seed.restored(&before);
        scratch.sentinel();
        // A known abort releases its marker; the same valid project is usable.
        succeeds(&scratch, &seed.project);
    }
}

#[test]
fn unregistered_files_at_pdf_publication_preserve_human_bytes_and_recovery_backups() {
    let scratch = Scratch::new();
    for point in [WriteStage::VersionPublished, WriteStage::ParsePublished] {
        let seed = scratch.seed(&format!("unknown-{point:?}"));
        let reached = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&reached);
        let human_path = Rc::new(RefCell::new(None));
        let saved_path = Rc::clone(&human_path);
        let hook = HookReset::install(Box::new(move |stage, root| {
            if stage == point {
                published_at(root, stage);
                let directory = if stage == WriteStage::VersionPublished {
                    pdf_version(root)
                } else {
                    pdf_parse(root)
                };
                let path = directory.join("human.keep");
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .unwrap();
                file.write_all(HUMAN).unwrap();
                file.sync_all().unwrap();
                *saved_path.borrow_mut() = Some(path);
                observed.store(true, Ordering::SeqCst);
            }
            Ok(())
        }));
        let error = source::import_with_options(&seed.project, &scratch.pdf, None, false)
            .err()
            .expect("unregistered publication content must prevent success");
        drop(hook);
        assert!(
            reached.load(Ordering::SeqCst),
            "missing actual {point:?}: {error}"
        );
        assert_eq!(error.code(), "SOURCE_WRITE_INCOMPLETE");
        assert_eq!(
            fs::read(human_path.borrow().as_ref().unwrap()).unwrap(),
            HUMAN
        );
        seed.backups();
        seed.blocked_read_only();
        scratch.sentinel();
        let retry = scratch.seed(&format!("independent-unknown-{point:?}"));
        succeeds(&scratch, &retry.project);
    }
}

#[cfg(windows)]
#[test]
fn real_windows_delete_locks_on_published_pdf_files_preserve_recovery_backups() {
    use std::os::windows::fs::OpenOptionsExt;
    let scratch = Scratch::new();
    for point in [WriteStage::VersionPublished, WriteStage::ParsePublished] {
        let seed = scratch.seed(&format!("locked-{point:?}"));
        let held = Rc::new(RefCell::new(None::<File>));
        let external = Rc::clone(&held);
        let reached = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&reached);
        let hook = HookReset::install(Box::new(move |stage, root| {
            if stage == point {
                published_at(root, stage);
                let path = if stage == WriteStage::VersionPublished {
                    pdf_version(root).join("raw.pdf")
                } else {
                    pdf_parse(root).join("parse-result.json")
                };
                *external.borrow_mut() = Some(
                    OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .open(path)
                        .unwrap(),
                );
                observed.store(true, Ordering::SeqCst);
                return Err(ProjectError::new(
                    "PDF_TEST_ABORT",
                    3,
                    "real delete-sharing lock survives rollback",
                ));
            }
            Ok(())
        }));
        let error = source::import_with_options(&seed.project, &scratch.pdf, None, false)
            .err()
            .expect("locked published PDF file must prevent complete rollback");
        drop(hook);
        assert!(
            reached.load(Ordering::SeqCst),
            "missing actual {point:?}: {error}"
        );
        assert!(held.borrow().is_some());
        assert_eq!(error.code(), "SOURCE_WRITE_INCOMPLETE");
        seed.backups();
        seed.blocked_read_only();
        drop(held.borrow_mut().take());
        seed.blocked_read_only();
        scratch.sentinel();
        let retry = scratch.seed(&format!("independent-locked-{point:?}"));
        succeeds(&scratch, &retry.project);
    }
}

struct LiveChild(Child);
impl Drop for LiveChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn child_stage(value: &str) -> WriteStage {
    match value {
        "Prepared" => WriteStage::Prepared,
        "VersionPublished" => WriteStage::VersionPublished,
        "ParsePublished" => WriteStage::ParsePublished,
        "DatabasePublished" => WriteStage::DatabasePublished,
        "ConfigPublished" => WriteStage::ConfigPublished,
        _ => panic!("unsupported test child stage"),
    }
}

#[test]
fn pdf_writer_interruption_child() {
    let Some(root) = std::env::var_os(CHILD_PROJECT) else {
        return;
    };
    let project = PathBuf::from(root);
    let point = child_stage(&std::env::var(CHILD_STAGE).unwrap());
    let ready = PathBuf::from(std::env::var_os(CHILD_READY).unwrap());
    let nonce = std::env::var(CHILD_NONCE).unwrap();
    let scratch = project.parent().unwrap();
    let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/compiler-temp")
        .canonicalize()
        .unwrap();
    assert_eq!(
        scratch.canonicalize().unwrap().parent(),
        Some(parent.as_path())
    );
    assert_eq!(
        scratch.file_name().unwrap().to_string_lossy(),
        format!("noetome-pdf-writer-{nonce}")
    );
    let owner: Value =
        serde_json::from_slice(&fs::read(scratch.join(".test-owner.json")).unwrap()).unwrap();
    assert_eq!(owner, json!({"nonce": nonce, "root": scratch}));
    assert_eq!(ready.parent(), Some(scratch));
    assert!(!ready.exists());
    assert!(ordinary_tree(scratch));
    let pdf = scratch.join("selected.pdf");
    assert_eq!(sha(&fs::read(&pdf).unwrap()), PDF_SHA);
    let receipt_project = project.clone();
    let hook = HookReset::install(Box::new(move |stage, root| {
        if stage == point {
            assert_eq!(root, receipt_project);
            published_at(root, stage);
            let receipt =
                serde_json::to_vec(&json!({"schema_version": 1, "pid": std::process::id(),
                "stage": format!("{stage:?}"), "project": root, "nonce": nonce}))
                .unwrap();
            let temporary = ready.with_extension("pending");
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .unwrap();
            // Keep an empty staging window so existence is never mistaken for
            // complete acknowledgement. Only the closed file is published.
            file.sync_all().unwrap();
            std::thread::sleep(Duration::from_millis(200));
            file.write_all(&receipt).unwrap();
            file.sync_all().unwrap();
            drop(file);
            fs::rename(&temporary, &ready).unwrap();
            loop {
                std::thread::park_timeout(Duration::from_secs(1));
            }
        }
        Ok(())
    }));
    let result = source::import_with_options(&project, &pdf, None, false);
    drop(hook);
    match result {
        Ok(result) => panic!("real child never reached {point:?}: {}", result.json),
        Err(error) => panic!("real child failed before {point:?}: {error}"),
    }
}

#[test]
fn real_pdf_writer_process_kill_preserves_old_history_at_five_publish_stages() {
    let scratch = Scratch::new();
    for point in [
        WriteStage::Prepared,
        WriteStage::VersionPublished,
        WriteStage::ParsePublished,
        WriteStage::DatabasePublished,
        WriteStage::ConfigPublished,
    ] {
        let seed = scratch.seed(&format!("interrupted-{point:?}"));
        let ready = scratch.root.join(format!("ready-{point:?}.json"));
        let stdout_path = scratch.root.join(format!("child-{point:?}.stdout"));
        let stderr_path = scratch.root.join(format!("child-{point:?}.stderr"));
        let stdout = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stdout_path)
            .unwrap();
        let stderr = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stderr_path)
            .unwrap();
        let mut child = LiveChild(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "source_write::pdf_tests::pdf_writer_interruption_child",
                    "--nocapture",
                ])
                .env(CHILD_PROJECT, &seed.project)
                .env(CHILD_STAGE, format!("{point:?}"))
                .env(CHILD_READY, &ready)
                .env(CHILD_NONCE, &scratch.nonce)
                .current_dir(&scratch.root)
                .stdin(Stdio::null())
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr))
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        while !ready.exists() && Instant::now() < deadline {
            if child.0.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if !ready.exists() {
            let _ = child.0.kill();
            let status = child.0.wait().unwrap();
            panic!(
                "real child did not reach {point:?} ({status}):\n{}\n{}",
                String::from_utf8_lossy(&fs::read(&stdout_path).unwrap()),
                String::from_utf8_lossy(&fs::read(&stderr_path).unwrap())
            );
        }
        let receipt: Value = serde_json::from_slice(&fs::read(&ready).unwrap()).unwrap();
        assert_eq!(
            receipt,
            json!({"schema_version": 1, "pid": child.0.id(),
            "stage": format!("{point:?}"), "project": seed.project, "nonce": scratch.nonce})
        );
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "complete receipt must precede a live PID check"
        );
        published_at(&seed.project, point);
        child.0.kill().unwrap();
        let killed = child.0.wait().unwrap();
        assert!(!killed.success(), "the real child must be terminated");
        assert!(
            child.0.try_wait().unwrap().is_some(),
            "the child must be reaped"
        );
        seed.backups();
        assert_eq!(
            history_rows(&seed.project.join(project::DATABASE), &seed.old_source),
            seed.old_rows
        );
        seed.blocked_read_only();
        scratch.sentinel();
        let retry = scratch.seed(&format!("independent-interrupted-{point:?}"));
        succeeds(&scratch, &retry.project);
        eprintln!(
            "PDF_WRITER_INTERRUPTION_FACT {}",
            json!({
                "stage": format!("{point:?}"), "pid": receipt["pid"],
                "readiness_published": true, "alive_before_kill": true,
                "exited_and_reaped": true, "exit_code": killed.code(),
                "old_config_sha256": sha(&seed.old_config),
                "old_database_sha256": sha(&seed.old_database),
                "selected_pdf_sha256": PDF_SHA,
                "old_history_and_snapshots_preserved": true,
                "inspection_read_only": true, "independent_retry_ready": true
            })
        );
    }
}
