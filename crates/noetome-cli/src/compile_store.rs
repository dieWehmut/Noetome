//! Exclusive fixture output ownership and SQLite publication authority.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};
use uuid::{Uuid, Version};

use crate::compile_identity::{Identity, identity as persistent_identity};
use crate::compile_model::{CompileContext, CompileReport, ReportPaths};
use crate::fixture_input::{LoadedCase, valid_relative_path};
use crate::fixture_render::RenderedFile;
use crate::project::{self, ProjectError};

const DATABASE: &str = "state/compile.sqlite";
const APPLICATION_ID: i64 = 0x4e434d50;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
const REQUIRED_CHECKS: [&str; 8] = [
    "sources",
    "coverage",
    "knowledge",
    "note_plan",
    "expected",
    "rendering",
    "staging_output",
    "vault_output",
];
const SCHEMA: &str = "
CREATE TABLE compile_metadata (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 schema_version INTEGER NOT NULL CHECK(schema_version=1),
 producer TEXT NOT NULL CHECK(producer='fixture'),
 case_id TEXT NOT NULL, fixture_scope_id TEXT NOT NULL,
 input_hashes_json TEXT NOT NULL, versions_json TEXT NOT NULL
);
CREATE TABLE runs (
 run_id TEXT PRIMARY KEY, started_at TEXT NOT NULL, finished_at TEXT,
 stage TEXT NOT NULL, state TEXT NOT NULL,
 report_json_path TEXT, report_md_path TEXT, checks_json TEXT NOT NULL,
 counts_json TEXT NOT NULL, issues_json TEXT NOT NULL
);
CREATE TABLE releases (
 release_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id),
 status TEXT NOT NULL, ir_revision INTEGER NOT NULL, plan_revision INTEGER NOT NULL,
 renderer_version TEXT NOT NULL, manifest_json TEXT NOT NULL
);
CREATE TABLE coverage_records (
 run_id TEXT NOT NULL REFERENCES runs(run_id), span_id TEXT NOT NULL,
 disposition TEXT NOT NULL, atom_ids_json TEXT NOT NULL,
 required_atom_ids_json TEXT NOT NULL, duplicate_of_span_id TEXT,
 exclusion_reason TEXT, decision_source TEXT NOT NULL,
 policy_version TEXT NOT NULL, stage TEXT NOT NULL,
 PRIMARY KEY(run_id,span_id)
);
CREATE TABLE rendered_files (
 release_id TEXT NOT NULL REFERENCES releases(release_id), relative_path TEXT NOT NULL,
 note_id TEXT NOT NULL, plan_revision INTEGER NOT NULL,
 content_sha256 TEXT NOT NULL, byte_length INTEGER NOT NULL,
 mappings_json TEXT NOT NULL, links_json TEXT NOT NULL,
 PRIMARY KEY(release_id,relative_path)
);
CREATE TABLE publication_commits (
 release_id TEXT PRIMARY KEY REFERENCES releases(release_id),
 committed_at TEXT NOT NULL, report_json_path TEXT NOT NULL,
 report_json_sha256 TEXT NOT NULL, report_md_path TEXT NOT NULL,
 report_md_sha256 TEXT NOT NULL, manifest_sha256 TEXT NOT NULL
);";

fn error(code: &'static str, message: impl Into<String>) -> ProjectError {
    ProjectError::new(code, 3, message)
}
fn integrity(message: impl Into<String>) -> ProjectError {
    error("COMPILE_INTEGRITY", message)
}
fn database(error: rusqlite::Error) -> ProjectError {
    ProjectError::new(
        "COMPILE_DATABASE_ERROR",
        3,
        format!("编译状态事务失败：{error}；已保留现场。"),
    )
}
fn json<T: serde::Serialize + ?Sized>(value: &T) -> Result<String, ProjectError> {
    serde_json::to_string(value).map_err(|e| integrity(e.to_string()))
}
fn integer(value: u64) -> Result<i64, ProjectError> {
    i64::try_from(value).map_err(|_| integrity("计数或revision超过SQLite整数范围。"))
}
fn enum_text<T: serde::Serialize>(value: &T) -> Result<String, ProjectError> {
    serde_json::to_value(value)
        .map_err(|e| integrity(e.to_string()))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| integrity("枚举必须为固定字符串。"))
}
fn utc(value: &str) -> bool {
    OffsetDateTime::parse(value, &Rfc3339).is_ok_and(|time| time.offset() == UtcOffset::UTC)
}
fn uuid(value: &str) -> bool {
    Uuid::parse_str(value)
        .is_ok_and(|id| id.get_version() == Some(Version::Random) && id.to_string() == value)
}
fn components(path: &Path) -> Vec<String> {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect()
}
fn compact_sql(sql: &str) -> String {
    sql.chars().filter(|c| !c.is_whitespace()).collect()
}
struct Directory {
    path: PathBuf,
    identity: same_file::Handle,
    _guard: Option<File>,
}
impl Directory {
    fn capture(path: &Path, protect: bool) -> Result<Self, ProjectError> {
        project::check_components(path)?;
        let metadata = fs::symlink_metadata(path).map_err(|e| ProjectError::io(path, e))?;
        if !metadata.is_dir() || project::is_link(&metadata) {
            return Err(ProjectError::unsafe_path(path));
        }
        let identity = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
        #[cfg(windows)]
        let guard = if protect {
            use std::os::windows::fs::OpenOptionsExt;
            let file = OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(0x02200000)
                .open(path)
                .map_err(|e| ProjectError::io(path, e))?;
            let opened = same_file::Handle::from_file(
                file.try_clone().map_err(|e| ProjectError::io(path, e))?,
            )
            .map_err(|e| ProjectError::io(path, e))?;
            if opened != identity
                || project::is_link(&file.metadata().map_err(|e| ProjectError::io(path, e))?)
            {
                return Err(integrity("目录打开时身份改变。"));
            }
            Some(file)
        } else {
            None
        };
        #[cfg(not(windows))]
        let guard = {
            let _ = protect;
            None
        };
        Ok(Self {
            path: path.to_path_buf(),
            identity,
            _guard: guard,
        })
    }
    fn verify(&self) -> Result<(), ProjectError> {
        project::check_components(&self.path)?;
        let metadata =
            fs::symlink_metadata(&self.path).map_err(|e| ProjectError::io(&self.path, e))?;
        let now = same_file::Handle::from_path(&self.path)
            .map_err(|e| ProjectError::io(&self.path, e))?;
        if !metadata.is_dir() || project::is_link(&metadata) || now != self.identity {
            return Err(integrity(format!("目录身份改变：{}", self.path.display())));
        }
        Ok(())
    }
}
struct OwnedFile {
    identity: same_file::Handle,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileSnapshot {
    pub(crate) identity: Identity,
    pub(crate) sha256: String,
    pub(crate) byte_length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DirectorySnapshot {
    pub(crate) path: PathBuf,
    pub(crate) identity: Identity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoreSnapshot {
    pub(crate) schema_version: u32,
    pub(crate) context: CompileContext,
    pub(crate) root: PathBuf,
    pub(crate) parent: DirectorySnapshot,
    pub(crate) directories: BTreeMap<String, Identity>,
    pub(crate) files: BTreeMap<String, FileSnapshot>,
    pub(crate) database: FileSnapshot,
    pub(crate) phase: String,
    pub(crate) failure_release_status: String,
    pub(crate) manifest: Vec<RenderedFile>,
    pub(crate) published: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileExpectation {
    pub(crate) sha256: String,
    pub(crate) byte_length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoreIntent {
    pub(crate) phase: String,
    pub(crate) manifest: Vec<RenderedFile>,
    pub(crate) files: BTreeMap<String, FileExpectation>,
}
fn read_file(path: &Path, writable_database: bool) -> Result<OwnedFile, ProjectError> {
    project::check_components(path)?;
    let before = project::checked_file(path)?;
    if before.len() > MAX_BYTES {
        return Err(integrity("文件超过64MiB读取上限。"));
    }
    let file = if writable_database {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3).custom_flags(0x00200000);
        }
        options.open(path).map_err(|e| ProjectError::io(path, e))?
    } else {
        project::read_guard(path)?
    };
    if project::is_link(&file.metadata().map_err(|e| ProjectError::io(path, e))?) {
        return Err(ProjectError::unsafe_path(path));
    }
    let identity =
        same_file::Handle::from_file(file.try_clone().map_err(|e| ProjectError::io(path, e))?)
            .map_err(|e| ProjectError::io(path, e))?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ProjectError::io(path, e))?;
    let after = project::checked_file(path)?;
    let current = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
    if bytes.len() as u64 > MAX_BYTES
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || current != identity
        || bytes.len() as u64 != after.len()
    {
        return Err(integrity("读取期间文件身份或内容改变。"));
    }
    // from_path retains identity while allowing later explicit replacement tests.
    Ok(OwnedFile {
        identity: current,
        bytes,
    })
}

pub(crate) struct Store {
    parent: Directory,
    directories: BTreeMap<String, Directory>,
    files: BTreeMap<String, OwnedFile>,
    connection: Connection,
    database_identity: same_file::Handle,
    _database_guard: File,
    database_sha256: Option<String>,
    recovery_shadow: bool,
    context: CompileContext,
    coverage_rows: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
    )>,
    expected_notes: BTreeMap<String, String>,
    phase: String,
    failure_release_status: String,
    manifest: Vec<RenderedFile>,
    published: bool,
}

impl Store {
    pub(crate) fn snapshot(&self) -> Result<StoreSnapshot, ProjectError> {
        self.ensure_integrity()?;
        if self.published {
            self.validate_committed_publication()?;
        }
        let directories = self
            .directories
            .iter()
            .map(|(name, directory)| Ok((name.clone(), persistent_identity(&directory.path)?)))
            .collect::<Result<_, ProjectError>>()?;
        let files = self
            .files
            .iter()
            .map(|(name, file)| {
                Ok((
                    name.clone(),
                    Self::file_snapshot(&self.root().join(name), &file.bytes)?,
                ))
            })
            .collect::<Result<_, ProjectError>>()?;
        let database = read_file(&self.root().join(DATABASE), true)?;
        let result = StoreSnapshot {
            schema_version: 1,
            context: self.context.clone(),
            root: self.root().to_path_buf(),
            parent: DirectorySnapshot {
                path: self.parent.path.clone(),
                identity: persistent_identity(&self.parent.path)?,
            },
            directories,
            files,
            database: Self::file_snapshot(&self.root().join(DATABASE), &database.bytes)?,
            phase: self.phase.clone(),
            failure_release_status: self.failure_release_status.clone(),
            manifest: self.manifest.clone(),
            published: self.published,
        };
        self.ensure_integrity()?;
        Ok(result)
    }

    fn file_snapshot(path: &Path, bytes: &[u8]) -> Result<FileSnapshot, ProjectError> {
        Ok(FileSnapshot {
            identity: persistent_identity(path)?,
            sha256: crate::text::sha256(bytes),
            byte_length: bytes.len() as u64,
        })
    }

    fn snapshot_layout(snapshot: &StoreSnapshot) -> Result<(), ProjectError> {
        if snapshot.schema_version != 1
            || !matches!(
                snapshot.phase.as_str(),
                "planned" | "staged" | "verified" | "failed" | "published"
            )
            || snapshot.published != (snapshot.phase == "published")
            || !matches!(
                snapshot.failure_release_status.as_str(),
                "staged" | "recovery_required"
            )
            || ["", "state", "staging", "vault", "reports"]
                .iter()
                .any(|name| !snapshot.directories.contains_key(*name))
        {
            return Err(integrity("检查点版本、阶段或必需目录不符。"));
        }
        let mut names = BTreeSet::new();
        for name in snapshot.directories.keys().chain(snapshot.files.keys()) {
            if name.is_empty() {
                continue;
            }
            let folded = name.to_lowercase();
            if !valid_relative_path(name)
                || !matches!(
                    name.split('/').next(),
                    Some("state" | "staging" | "vault" | "reports")
                )
                || !names.insert(folded.clone())
                || [
                    DATABASE,
                    "state/compile.sqlite-journal",
                    "state/compile.sqlite-wal",
                    "state/compile.sqlite-shm",
                ]
                .contains(&folded.as_str())
                || !Path::new(name)
                    .parent()
                    .and_then(Path::to_str)
                    .is_some_and(|parent| snapshot.directories.contains_key(parent))
            {
                return Err(integrity("检查点目录/文件路径、碰撞或父级不符。"));
            }
        }
        for file in snapshot
            .files
            .values()
            .chain(std::iter::once(&snapshot.database))
        {
            if file.byte_length > MAX_BYTES
                || file.sha256.len() != 64
                || !file
                    .sha256
                    .bytes()
                    .all(|ch| ch.is_ascii_digit() || (b'a'..=b'f').contains(&ch))
            {
                return Err(integrity("检查点文件指纹或读取上限不合法。"));
            }
        }
        Ok(())
    }

    /// Only the caller's confirmed, exact inventory is eligible. Unknown partial
    /// output remains untouched. The separate run journal authenticates this
    /// snapshot; this function never trusts a saved `passed` flag.
    pub(crate) fn resume(
        root: &Path,
        context: &CompileContext,
        case: &LoadedCase,
        snapshot: &StoreSnapshot,
        allow_publication_reconcile: bool,
    ) -> Result<Self, ProjectError> {
        Self::resume_snapshot(
            root,
            context,
            case,
            snapshot,
            allow_publication_reconcile,
            true,
            None,
        )
    }
    pub(crate) fn resume_readonly(
        root: &Path,
        context: &CompileContext,
        case: &LoadedCase,
        snapshot: &StoreSnapshot,
        reconcile: bool,
    ) -> Result<Self, ProjectError> {
        Self::resume_snapshot(root, context, case, snapshot, reconcile, false, None)
    }
    fn resume_snapshot(
        root: &Path,
        context: &CompileContext,
        case: &LoadedCase,
        snapshot: &StoreSnapshot,
        allow_publication_reconcile: bool,
        writable: bool,
        recovery_image: Option<&[u8]>,
    ) -> Result<Self, ProjectError> {
        Self::snapshot_layout(snapshot)?;
        Self::validate_context(context, case)?;
        if snapshot.context != *context {
            return Err(integrity("检查点不是同一Run/Release、输入或编译上下文。"));
        }
        case.check_unchanged()?;
        let root = project::check_components(root)?;
        let root = root
            .canonicalize()
            .map_err(|e| ProjectError::io(&root, e))?;
        let case_root = case
            .root()
            .canonicalize()
            .map_err(|e| ProjectError::io(case.root(), e))?;
        if components(&root).starts_with(&components(&case_root))
            || components(&case_root).starts_with(&components(&root))
            || root != snapshot.root
            || root.parent() != Some(snapshot.parent.path.as_path())
        {
            return Err(integrity("检查点根目录、父级或输入/输出范围不符。"));
        }
        let parent = Directory::capture(&snapshot.parent.path, true)?;
        if persistent_identity(&parent.path)? != snapshot.parent.identity {
            return Err(integrity("检查点父目录被替换。"));
        }
        let mut directories = BTreeMap::new();
        for (name, expected) in &snapshot.directories {
            let directory = Directory::capture(&root.join(name), name.is_empty())?;
            if persistent_identity(&directory.path)? != *expected {
                return Err(integrity(format!("检查点目录身份改变：{name}")));
            }
            directories.insert(name.clone(), directory);
        }
        let mut files = BTreeMap::new();
        for (name, expected) in &snapshot.files {
            let path = root.join(name);
            let actual = read_file(&path, false)?;
            if Self::file_snapshot(&path, &actual.bytes)? != *expected {
                return Err(integrity(format!("检查点文件身份或字节改变：{name}")));
            }
            files.insert(name.clone(), actual);
        }
        let db_path = root.join(DATABASE);
        let actual_db = read_file(&db_path, true)?;
        let actual_snapshot =
            Self::file_snapshot(&db_path, recovery_image.unwrap_or(&actual_db.bytes))?;
        if actual_snapshot.identity != snapshot.database.identity {
            return Err(integrity("检查点SQLite文件被替换。"));
        }
        let reconcile = actual_snapshot != snapshot.database;
        if reconcile
            && !(allow_publication_reconcile
                && snapshot.phase == "verified"
                && !snapshot.published
                && snapshot.files.contains_key("reports/validation.json")
                && snapshot.files.contains_key("reports/validation.md"))
        {
            return Err(integrity(
                "SQLite不等于确认检查点，且不属于允许的发布提交窗口。",
            ));
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3).custom_flags(0x00200000);
        }
        let database_guard = options
            .open(&db_path)
            .map_err(|e| ProjectError::io(&db_path, e))?;
        let mut uri =
            url::Url::from_file_path(&db_path).map_err(|_| ProjectError::unsafe_path(&db_path))?;
        uri.query_pairs_mut()
            .append_pair("mode", "ro")
            .append_pair("immutable", "1");
        let connection = if let Some(image) = recovery_image {
            let mut connection = Connection::open_in_memory().map_err(database)?;
            let mut buffered = image.to_vec();
            buffered.extend_from_slice(&[0; 20]);
            let byte_length = buffered.len();
            connection
                .deserialize_read_exact("main", std::io::Cursor::new(buffered), byte_length, true)
                .map_err(database)?;
            connection
        } else {
            Connection::open_with_flags(
                uri.as_str(),
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
            )
            .map_err(database)?
        };
        let mut store = Self {
            parent,
            directories,
            files,
            connection,
            database_identity: actual_db.identity,
            _database_guard: database_guard,
            database_sha256: Some(actual_snapshot.sha256),
            recovery_shadow: recovery_image.is_some(),
            context: context.clone(),
            coverage_rows: case
                .ir
                .coverage
                .iter()
                .map(|row| {
                    Ok((
                        row.span_id.clone(),
                        enum_text(&row.disposition)?,
                        json(&row.atom_ids)?,
                        json(&row.required_atom_ids)?,
                        row.duplicate_of_span_id.clone(),
                        row.exclusion_reason.clone(),
                        enum_text(&row.decision_source)?,
                        row.policy_version.clone(),
                    ))
                })
                .collect::<Result<_, ProjectError>>()?,
            expected_notes: case
                .plan
                .notes
                .iter()
                .map(|note| (note.output_path.clone(), note.note_id.clone()))
                .collect(),
            phase: if reconcile {
                "published".into()
            } else {
                snapshot.phase.clone()
            },
            failure_release_status: snapshot.failure_release_status.clone(),
            manifest: snapshot.manifest.clone(),
            published: reconcile || snapshot.published,
        };
        // Exact inventory is checked before any SQL pragma/query. Immutable
        // validation cannot recover a journal or create WAL/SHM side files.
        store.ensure_tree_for(recovery_image.is_some())?;
        store
            .connection
            .execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA temp_store=MEMORY;",
            )
            .map_err(database)?;
        let quick: String = store
            .connection
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(database)?;
        if quick != "ok" {
            return Err(integrity("检查点SQLite完整性检查失败。"));
        }
        if recovery_image.is_some() {
            store.ensure_schema_for(true)?;
            store.ensure_records()?;
        } else {
            store.ensure_integrity()?;
        }
        if !store.manifest.is_empty() {
            store.validate_manifest(&store.manifest, "staging/vault")?;
        }
        if matches!(store.phase.as_str(), "verified" | "published") {
            store.validate_manifest(&store.manifest, "vault")?;
        }
        if store.published {
            store.validate_committed_publication()?;
        }
        case.check_unchanged()?;
        store.ensure_tree_for(recovery_image.is_some())?;
        if store.published || !writable || recovery_image.is_some() {
            return Ok(store);
        }
        // The owner has now independently validated the quiescent original.
        // No schema migration, journal recovery or content replacement occurs.
        let writable = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(database)?;
        writable.busy_timeout(Duration::ZERO).map_err(database)?;
        writable
            .execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA temp_store=MEMORY;",
            )
            .map_err(database)?;
        store.connection = writable;
        store.ensure_integrity()?;
        Ok(store)
    }

    /// The sealed unit intent authorizes only these pending paths and one exact
    /// logical SQLite transition. Existing snapshot identities never change.
    pub(crate) fn resume_with_intent(
        root: &Path,
        context: &CompileContext,
        case: &LoadedCase,
        snapshot: Option<&StoreSnapshot>,
        intent: &StoreIntent,
        derived_files: &BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, ProjectError> {
        Self::resume_intent(
            root,
            context,
            case,
            snapshot,
            intent,
            derived_files,
            true,
            None,
        )
    }
    pub(crate) fn resume_intent_readonly(
        root: &Path,
        context: &CompileContext,
        case: &LoadedCase,
        snapshot: Option<&StoreSnapshot>,
        intent: &StoreIntent,
        derived_files: &BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, ProjectError> {
        Self::resume_intent(
            root,
            context,
            case,
            snapshot,
            intent,
            derived_files,
            false,
            None,
        )
    }
    fn resume_intent(
        root: &Path,
        context: &CompileContext,
        case: &LoadedCase,
        snapshot: Option<&StoreSnapshot>,
        intent: &StoreIntent,
        derived_files: &BTreeMap<String, Vec<u8>>,
        writable: bool,
        recovery_image: Option<&[u8]>,
    ) -> Result<Self, ProjectError> {
        Self::validate_context(context, case)?;
        case.check_unchanged()?;
        if !matches!(
            intent.phase.as_str(),
            "planned" | "staged" | "verified" | "published"
        ) || intent.files.keys().ne(derived_files.keys())
            || (intent.phase == "planned" && !intent.manifest.is_empty())
        {
            return Err(integrity("prepared intent阶段、派生文件集合或清单不合法。"));
        }
        for (name, expectation) in &intent.files {
            let bytes = &derived_files[name];
            if !valid_relative_path(name)
                || !matches!(
                    name.split('/').next(),
                    Some("state" | "staging" | "vault" | "reports")
                )
                || [
                    DATABASE,
                    "state/compile.sqlite-journal",
                    "state/compile.sqlite-wal",
                    "state/compile.sqlite-shm",
                ]
                .contains(&name.to_lowercase().as_str())
                || expectation.byte_length > MAX_BYTES
                || expectation.byte_length != bytes.len() as u64
                || expectation.sha256 != crate::text::sha256(bytes)
            {
                return Err(integrity(
                    "prepared intent路径或source-derived字节不等于封存期待。",
                ));
            }
        }
        if let Some(base) = snapshot {
            Self::snapshot_layout(base)?;
            let advance = matches!(
                (base.phase.as_str(), intent.phase.as_str()),
                ("planned", "staged") | ("staged", "verified") | ("verified", "published")
            );
            if base.context != *context
                || (base.phase != intent.phase && !advance)
                || (base.phase == intent.phase && base.manifest != intent.manifest)
                || (base.phase != "planned" && base.manifest != intent.manifest)
                || (base.published
                    && intent
                        .files
                        .keys()
                        .any(|name| !base.files.contains_key(name)))
            {
                return Err(integrity(
                    "prepared intent不是同一上下文和一个合法原状态/目标转换。",
                ));
            }
        } else if intent.phase != "planned"
            || !intent.manifest.is_empty()
            || !intent.files.is_empty()
        {
            return Err(integrity("无旧snapshot只能恢复完整planned初始化。"));
        }
        let checked_root = project::check_components(root)?;
        if !project::exists(&checked_root)? {
            if snapshot.is_none() && writable && recovery_image.is_none() {
                return Self::create(&checked_root, context, case);
            }
            return Err(integrity("已确认输出根缺失。"));
        }
        let root = checked_root
            .canonicalize()
            .map_err(|e| ProjectError::io(&checked_root, e))?;
        let parent = root
            .parent()
            .ok_or_else(|| ProjectError::unsafe_path(&root))?;
        let mut directories = BTreeMap::new();
        let mut files = BTreeMap::new();
        if let Some(base) = snapshot {
            if root != base.root
                || parent != base.parent.path
                || persistent_identity(parent)? != base.parent.identity
            {
                return Err(integrity("prepared恢复的根/父级不是原身份。"));
            }
            for (name, expected) in &base.directories {
                if persistent_identity(&root.join(name))? != *expected {
                    return Err(integrity("prepared恢复不能替换已有目录。"));
                }
                directories.insert(name.clone(), expected.clone());
            }
            for (name, expected) in &base.files {
                let actual = read_file(&root.join(name), false)?;
                if Self::file_snapshot(&root.join(name), &actual.bytes)? != *expected {
                    return Err(integrity("prepared恢复不能替换或改写已有文件。"));
                }
                files.insert(name.clone(), expected.clone());
            }
        } else {
            for name in ["", "state", "staging", "vault", "reports"] {
                directories.insert(name.into(), persistent_identity(&root.join(name))?);
            }
        }
        for (name, expectation) in &intent.files {
            if let Some(previous) = files.get(name) {
                if previous.sha256 != expectation.sha256
                    || previous.byte_length != expectation.byte_length
                {
                    return Err(integrity("prepared intent不能覆盖已确认文件。"));
                }
            }
            let path = root.join(name);
            project::check_components(&path)?;
            if project::exists(&path)? {
                let actual = read_file(&path, false)?;
                if actual.bytes != derived_files[name] {
                    return Err(integrity(
                        "pending文件不等于重新派生的完整字节；保留partial现场。",
                    ));
                }
                let captured = Self::file_snapshot(&path, &actual.bytes)?;
                if files.get(name).is_some_and(|old| old != &captured) {
                    return Err(integrity("已确认pending文件身份改变。"));
                }
                files.insert(name.clone(), captured);
            }
            let mut ancestor = Path::new(name).parent();
            while let Some(relative) = ancestor {
                let relative = relative
                    .to_str()
                    .ok_or_else(|| ProjectError::unsafe_path(&path))?;
                if !directories.contains_key(relative) && project::exists(&root.join(relative))? {
                    directories.insert(relative.into(), persistent_identity(&root.join(relative))?);
                }
                ancestor = Path::new(relative).parent();
            }
        }
        let db_path = root.join(DATABASE);
        let actual_db = read_file(&db_path, true)?;
        let database = Self::file_snapshot(&db_path, recovery_image.unwrap_or(&actual_db.bytes))?;
        let (phase, manifest, published, failure_status) = if let Some(base) = snapshot {
            if database.identity != base.database.identity {
                return Err(integrity("prepared恢复不能替换原SQLite身份。"));
            }
            if database == base.database {
                (
                    base.phase.clone(),
                    base.manifest.clone(),
                    base.published,
                    base.failure_release_status.clone(),
                )
            } else if base.phase != intent.phase {
                (
                    intent.phase.clone(),
                    intent.manifest.clone(),
                    intent.phase == "published",
                    base.failure_release_status.clone(),
                )
            } else {
                return Err(integrity("没有预定SQL阶段转换却发生SQLite字节改变。"));
            }
        } else {
            ("planned".into(), Vec::new(), false, "staged".into())
        };
        let proposed = StoreSnapshot {
            schema_version: 1,
            context: context.clone(),
            root: root.clone(),
            parent: DirectorySnapshot {
                path: parent.to_path_buf(),
                identity: persistent_identity(parent)?,
            },
            directories,
            files,
            database,
            phase,
            failure_release_status: failure_status,
            manifest,
            published,
        };
        // The proposal is not trusted: the original strict path independently
        // rechecks every ordinary entry and every exact six-table record.
        Self::resume_snapshot(
            &root,
            context,
            case,
            &proposed,
            false,
            writable,
            recovery_image,
        )
    }
    /// Called only by an explicitly resumed Run while its worker lock is held.
    /// All known files, source-derived pending paths and six-table SQL records
    /// are checked against a read-only original image before SQLite may write.
    pub(crate) fn recover_hot_journal(
        root: &Path,
        context: &CompileContext,
        case: &LoadedCase,
        snapshot: &StoreSnapshot,
        intent: Option<&StoreIntent>,
        derived_files: &BTreeMap<String, Vec<u8>>,
    ) -> Result<(), ProjectError> {
        let db = root.join(DATABASE);
        let journal = root.join("state/compile.sqlite-journal");
        if persistent_identity(&db)? != snapshot.database.identity {
            return Err(integrity("热日志数据库不是原已确认身份。"));
        }
        let witness = crate::run_recovery::RecoveryWitness::capture(&db, &journal)?;
        let image = crate::run_recovery::checked_recovery_image(&db, &journal)?;
        if image.len() as u64 != snapshot.database.byte_length
            || crate::text::sha256(&image) != snapshot.database.sha256
        {
            return Err(integrity("热日志原镜像不等于已有确认数据库的完整字节。"));
        }
        {
            let store = if let Some(intent) = intent {
                Self::resume_intent(
                    root,
                    context,
                    case,
                    Some(snapshot),
                    intent,
                    derived_files,
                    false,
                    Some(&image),
                )?
            } else {
                Self::resume_snapshot(root, context, case, snapshot, false, false, Some(&image))?
            };
            let targets = crate::fixture_compile::contracts(case).2;
            if !store.manifest.is_empty() {
                let staged =
                    crate::fixture_compile::read_files(&store, "staging/vault", &store.manifest)?;
                if !crate::fixture_render::validate_rendered(
                    case,
                    &targets,
                    &store.manifest,
                    &staged,
                )
                .is_empty()
                {
                    return Err(integrity("恢复前暂存不等于当前来源与语义。"));
                }
                if matches!(store.phase.as_str(), "verified" | "published") {
                    let vault =
                        crate::fixture_compile::read_files(&store, "vault", &store.manifest)?;
                    if !crate::fixture_render::validate_rendered(
                        case,
                        &targets,
                        &store.manifest,
                        &vault,
                    )
                    .is_empty()
                    {
                        return Err(integrity("恢复前Vault不等于当前来源与语义。"));
                    }
                }
            }
            case.check_unchanged()?;
            store.ensure_tree_for(true)?;
        }
        witness.verify(&db, &journal)?;
        // All read guards from the memory gate are now dropped on Windows.
        let connection = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(database)?;
        let quick: String = connection
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(database)?;
        if quick != "ok" {
            return Err(integrity("热日志恢复后的完整性检查失败。"));
        }
        drop(connection);
        if project::exists(&journal)? {
            return Err(integrity("SQLite未完成已拥有热日志恢复，保留现场。"));
        }
        if let Some(intent) = intent {
            Self::resume_intent(
                root,
                context,
                case,
                Some(snapshot),
                intent,
                derived_files,
                false,
                None,
            )?;
        } else {
            Self::resume_snapshot(root, context, case, snapshot, false, false, None)?;
        }
        Ok(())
    }
    pub(crate) fn preflight(case_root: &Path, output_root: &Path) -> Result<PathBuf, ProjectError> {
        let case = project::check_components(case_root)?;
        if !fs::symlink_metadata(&case)
            .map_err(|e| ProjectError::io(&case, e))?
            .is_dir()
        {
            return Err(ProjectError::unsafe_path(&case));
        }
        let case = case
            .canonicalize()
            .map_err(|e| ProjectError::io(&case, e))?;
        let output = project::check_components(output_root)?;
        let parent = output
            .parent()
            .ok_or_else(|| ProjectError::unsafe_path(&output))?;
        if !fs::symlink_metadata(parent)
            .map_err(|e| ProjectError::io(parent, e))?
            .is_dir()
        {
            return Err(ProjectError::unsafe_path(parent));
        }
        let parent = parent
            .canonicalize()
            .map_err(|e| ProjectError::io(parent, e))?;
        let output = parent.join(
            output
                .file_name()
                .ok_or_else(|| ProjectError::unsafe_path(&output))?,
        );
        project::check_components(&output)?;
        let a = components(&case);
        let b = components(&output);
        if a.starts_with(&b) || b.starts_with(&a) {
            return Err(error(
                "COMPILE_INPUT_OUTPUT_OVERLAP",
                "输入与输出不能互为祖先目录。",
            ));
        }
        if project::exists(&output)? {
            return Err(error(
                "COMPILE_OUTPUT_EXISTS",
                format!("输出根已存在：{}；仅接受全新目录。", output.display()),
            ));
        }
        Ok(output)
    }
    fn validate_context(context: &CompileContext, case: &LoadedCase) -> Result<(), ProjectError> {
        let hashes = BTreeMap::from([
            ("case_sha256".into(), case.case_sha256.clone()),
            ("ir_sha256".into(), case.ir_sha256.clone()),
            ("plan_sha256".into(), case.plan_sha256.clone()),
            ("expected_sha256".into(), case.expected_sha256.clone()),
        ]);
        let actual_counts = BTreeMap::from([
            ("sources", case.manifest.sources.len() as u64),
            ("spans", case.spans.len() as u64),
            ("atoms", case.ir.atoms.len() as u64),
            (
                "required_atoms",
                case.ir.atoms.iter().filter(|atom| atom.required).count() as u64,
            ),
            ("notes", case.plan.notes.len() as u64),
            (
                "nodes",
                case.plan
                    .notes
                    .iter()
                    .flat_map(|note| &note.sections)
                    .map(|section| section.ordered_nodes.len() as u64)
                    .sum(),
            ),
            (
                "included",
                case.ir
                    .coverage
                    .iter()
                    .filter(|row| {
                        row.disposition == crate::fixture_model::CoverageDisposition::Included
                    })
                    .count() as u64,
            ),
            (
                "excluded",
                case.ir
                    .coverage
                    .iter()
                    .filter(|row| {
                        row.disposition == crate::fixture_model::CoverageDisposition::Excluded
                    })
                    .count() as u64,
            ),
            (
                "duplicates",
                case.ir
                    .coverage
                    .iter()
                    .filter(|row| {
                        row.disposition == crate::fixture_model::CoverageDisposition::Duplicate
                    })
                    .count() as u64,
            ),
        ]);
        let counts_valid = [
            "sources",
            "spans",
            "atoms",
            "required_atoms",
            "notes",
            "nodes",
        ]
        .iter()
        .all(|key| context.counts.get(*key) == actual_counts.get(*key))
            && context
                .counts
                .iter()
                .all(|(key, value)| actual_counts.get(key.as_str()) == Some(value));
        if !uuid(&context.run_id)
            || !uuid(&context.release_id)
            || context.run_id == context.release_id
            || !utc(&context.started_at)
            || context.case_id != case.manifest.case_id
            || context.fixture_scope_id != case.ir.coverage_run_id
            || context.ir_revision != case.ir.revision
            || context.plan_revision != case.plan.revision
            || context.ir_revision == 0
            || context.plan_revision == 0
            || context.input_hashes != hashes
            || context.versions.get("renderer").map(String::as_str) != Some("fixture-markdown-v1")
            || !counts_valid
            || !case.source_issues.is_empty()
        {
            return Err(integrity(
                "编译上下文与已读取的fixture身份、版本、指纹或计数不符。",
            ));
        }
        Ok(())
    }
    pub(crate) fn create(
        root: &Path,
        context: &CompileContext,
        case: &LoadedCase,
    ) -> Result<Self, ProjectError> {
        let root = Self::preflight(case.root(), root)?;
        Self::validate_context(context, case)?;
        let ir_revision = integer(context.ir_revision)?;
        let plan_revision = integer(context.plan_revision)?;
        for count in context.counts.values() {
            integer(*count)?;
        }
        case.check_unchanged()?;
        let parent = Directory::capture(
            root.parent()
                .ok_or_else(|| ProjectError::unsafe_path(&root))?,
            true,
        )?;
        fs::create_dir(&root).map_err(|e| ProjectError::io(&root, e))?;
        let mut directories = BTreeMap::new();
        directories.insert("".into(), Directory::capture(&root, true)?);
        parent.verify()?;
        for name in ["state", "staging", "vault", "reports"] {
            let path = root.join(name);
            fs::create_dir(&path).map_err(|e| ProjectError::io(&path, e))?;
            directories.insert(name.into(), Directory::capture(&path, false)?);
        }
        let db_path = root.join(DATABASE);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3).custom_flags(0x00200000);
        }
        let database_guard = options
            .open(&db_path)
            .map_err(|e| ProjectError::io(&db_path, e))?;
        let database_identity =
            same_file::Handle::from_path(&db_path).map_err(|e| ProjectError::io(&db_path, e))?;
        let mut connection =
            Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)
                .map_err(database)?;
        connection.busy_timeout(Duration::ZERO).map_err(database)?;
        connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA temp_store=MEMORY;").map_err(database)?;
        let mut coverage_rows = Vec::new();
        for record in &case.ir.coverage {
            coverage_rows.push((
                record.span_id.clone(),
                enum_text(&record.disposition)?,
                json(&record.atom_ids)?,
                json(&record.required_atom_ids)?,
                record.duplicate_of_span_id.clone(),
                record.exclusion_reason.clone(),
                enum_text(&record.decision_source)?,
                record.policy_version.clone(),
            ));
        }
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database)?;
        tx.execute_batch(SCHEMA).map_err(database)?;
        tx.execute_batch(&format!(
            "PRAGMA user_version=1; PRAGMA application_id={APPLICATION_ID};"
        ))
        .map_err(database)?;
        tx.execute(
            "INSERT INTO compile_metadata VALUES(1,1,'fixture',?1,?2,?3,?4)",
            params![
                context.case_id,
                context.fixture_scope_id,
                json(&context.input_hashes)?,
                json(&context.versions)?
            ],
        )
        .map_err(database)?;
        tx.execute(
            "INSERT INTO runs VALUES(?1,?2,NULL,'planned','running',NULL,NULL,'{}',?3,'[]')",
            params![context.run_id, context.started_at, json(&context.counts)?],
        )
        .map_err(database)?;
        tx.execute(
            "INSERT INTO releases VALUES(?1,?2,'staged',?3,?4,?5,'[]')",
            params![
                context.release_id,
                context.run_id,
                ir_revision,
                plan_revision,
                context.versions["renderer"]
            ],
        )
        .map_err(database)?;
        for row in &coverage_rows {
            tx.execute(
                "INSERT INTO coverage_records VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'planned')",
                params![
                    context.run_id,
                    row.0,
                    row.1,
                    row.2,
                    row.3,
                    row.4,
                    row.5,
                    row.6,
                    row.7
                ],
            )
            .map_err(database)?;
        }
        tx.commit().map_err(database)?;
        let mut store = Self {
            parent,
            directories,
            files: BTreeMap::new(),
            connection,
            database_identity,
            _database_guard: database_guard,
            recovery_shadow: false,
            database_sha256: None,
            context: context.clone(),
            coverage_rows,
            expected_notes: case
                .plan
                .notes
                .iter()
                .map(|note| (note.output_path.clone(), note.note_id.clone()))
                .collect(),
            phase: "planned".into(),
            failure_release_status: "staged".into(),
            manifest: Vec::new(),
            published: false,
        };
        store.refresh_database()?;
        store.ensure_integrity()?;
        Ok(store)
    }
    pub(crate) fn root(&self) -> &Path {
        &self.directories[""].path
    }
    pub(crate) fn write_owned(&mut self, relative: &str, bytes: &[u8]) -> Result<(), ProjectError> {
        self.ensure_integrity()?;
        if self.published {
            return Err(integrity("发布事务已提交，不能继续写入。"));
        }
        if !valid_relative_path(relative)
            || relative == DATABASE
            || !matches!(
                relative.split('/').next(),
                Some("state" | "staging" | "vault" | "reports")
            )
            || bytes.len() as u64 > MAX_BYTES
        {
            return Err(ProjectError::unsafe_path(&self.root().join(relative)));
        }
        let folded = relative.to_lowercase();
        if self
            .files
            .keys()
            .chain(self.directories.keys())
            .any(|key| key.to_lowercase() == folded)
        {
            return Err(error("COMPILE_OUTPUT_EXISTS", "已存在的输出不能覆盖。"));
        }
        let parts: Vec<_> = relative.split('/').collect();
        for index in 1..parts.len() {
            let name = parts[..index].join("/");
            if self.directories.contains_key(&name) {
                continue;
            }
            if self
                .directories
                .keys()
                .chain(self.files.keys())
                .any(|key| key.to_lowercase() == name.to_lowercase())
            {
                return Err(error(
                    "COMPILE_OUTPUT_EXISTS",
                    "路径与已拥有目录或文件冲突。",
                ));
            }
            self.ensure_integrity()?;
            let path = self.root().join(&name);
            fs::create_dir(&path).map_err(|e| ProjectError::io(&path, e))?;
            self.directories
                .insert(name, Directory::capture(&path, false)?);
        }
        let path = self.root().join(relative);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1).custom_flags(0x00200000);
        }
        let mut file = options
            .open(&path)
            .map_err(|e| ProjectError::io(&path, e))?;
        let original =
            same_file::Handle::from_path(&path).map_err(|e| ProjectError::io(&path, e))?;
        let write_result = file.write_all(bytes).and_then(|()| file.sync_all());
        drop(file);
        let owned = read_file(&path, false)?;
        if owned.identity != original {
            return Err(integrity("写入结束时文件身份改变。"));
        }
        self.files.insert(relative.into(), owned);
        write_result.map_err(|e| ProjectError::io(&path, e))?;
        if self.files[relative].bytes != bytes {
            return Err(integrity("写入后重读字节不一致。"));
        }
        self.ensure_integrity()
    }
    pub(crate) fn read_owned(&self, relative: &str) -> Result<Vec<u8>, ProjectError> {
        self.ensure_integrity()?;
        if !valid_relative_path(relative) {
            return Err(ProjectError::unsafe_path(&self.root().join(relative)));
        }
        let expected = self
            .files
            .get(relative)
            .ok_or_else(|| integrity("只能重读本轮独占建立的文件。"))?;
        let actual = read_file(&self.root().join(relative), false)?;
        if actual.identity != expected.identity || actual.bytes != expected.bytes {
            return Err(integrity("已拥有文件的身份或内容改变。"));
        }
        Ok(actual.bytes)
    }
    pub(crate) fn ensure_integrity(&self) -> Result<(), ProjectError> {
        self.ensure_tree_for(self.recovery_shadow)?;
        if !self.connection.is_autocommit() {
            return Err(error(
                "COMPILE_RECOVERY_REQUIRED",
                "未结束事务不能用于只读验收。",
            ));
        }
        self.ensure_schema_for(self.recovery_shadow)?;
        self.ensure_records()?;
        if let Some(expected) = &self.database_sha256 {
            let actual = read_file(&self.root().join(DATABASE), true)?;
            if actual.identity != self.database_identity
                || (!self.recovery_shadow && crate::text::sha256(&actual.bytes) != *expected)
            {
                return Err(integrity("SQLite文件在事务边界之外改变。"));
            }
        }
        Ok(())
    }
    fn ensure_tree(&self) -> Result<(), ProjectError> {
        self.ensure_tree_for(false)
    }
    fn ensure_tree_for(&self, hot: bool) -> Result<(), ProjectError> {
        self.parent.verify()?;
        for directory in self.directories.values() {
            directory.verify()?;
        }
        let path = self.root().join(DATABASE);
        project::checked_file(&path)?;
        if same_file::Handle::from_path(&path).map_err(|e| ProjectError::io(&path, e))?
            != self.database_identity
        {
            return Err(integrity("SQLite文件身份改变。"));
        }
        for (relative, expected) in &self.files {
            let actual = read_file(&self.root().join(relative), false)?;
            if actual.identity != expected.identity || actual.bytes != expected.bytes {
                return Err(integrity(format!("输出身份或内容改变：{relative}")));
            }
        }
        for (relative, directory) in &self.directories {
            let mut expected = BTreeSet::new();
            for key in self
                .directories
                .keys()
                .chain(self.files.keys())
                .map(String::as_str)
                .chain(std::iter::once(DATABASE))
            {
                if !key.is_empty() && Path::new(key).parent() == Some(Path::new(relative)) {
                    expected.insert(
                        Path::new(key)
                            .file_name()
                            .ok_or_else(|| integrity("无效清单路径。"))?
                            .to_os_string(),
                    );
                }
            }
            if hot && relative == "state" {
                expected.insert("compile.sqlite-journal".into());
            }
            let actual: BTreeSet<_> = fs::read_dir(&directory.path)
                .map_err(|e| ProjectError::io(&directory.path, e))?
                .map(|entry| entry.map(|e| e.file_name()))
                .collect::<Result<_, _>>()
                .map_err(|e| ProjectError::io(&directory.path, e))?;
            if actual != expected {
                return Err(error(
                    "COMPILE_RECOVERY_REQUIRED",
                    format!(
                        "目录清单含未知或缺失内容：{}；保留现场，禁止发布。",
                        directory.path.display()
                    ),
                ));
            }
        }
        Ok(())
    }
    fn ensure_schema_for(&self, shadow: bool) -> Result<(), ProjectError> {
        let version: i64 = self
            .connection
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(database)?;
        let app: i64 = self
            .connection
            .query_row("PRAGMA application_id", [], |r| r.get(0))
            .map_err(database)?;
        let foreign_keys: i64 = self
            .connection
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .map_err(database)?;
        let synchronous: i64 = self
            .connection
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .map_err(database)?;
        let journal: String = self
            .connection
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .map_err(database)?;
        if version != 1
            || app != APPLICATION_ID
            || foreign_keys != 1
            || synchronous != 2
            || (journal != "delete" && !(shadow && journal == "memory"))
        {
            return Err(integrity("SQLite版本、应用或事务配置不符。"));
        }
        let expected: BTreeMap<_, _> = SCHEMA
            .split(';')
            .filter(|s| !s.trim().is_empty())
            .map(|sql| {
                (
                    sql.split_whitespace().nth(2).unwrap_or("").to_string(),
                    compact_sql(sql.trim()),
                )
            })
            .collect();
        let indexes: BTreeSet<_> = [
            "runs",
            "releases",
            "coverage_records",
            "rendered_files",
            "publication_commits",
        ]
        .iter()
        .map(|name| format!("sqlite_autoindex_{name}_1"))
        .collect();
        let mut tables = BTreeSet::new();
        let mut actual_indexes = BTreeSet::new();
        let mut statement = self
            .connection
            .prepare("SELECT type,name,sql FROM sqlite_schema ORDER BY name")
            .map_err(database)?;
        let rows = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(database)?;
        for row in rows {
            let (kind, name, sql) = row.map_err(database)?;
            if kind == "table"
                && sql
                    .as_ref()
                    .is_some_and(|sql| expected.get(&name) == Some(&compact_sql(sql)))
            {
                tables.insert(name);
            } else if kind == "index" && sql.is_none() && indexes.contains(&name) {
                actual_indexes.insert(name);
            } else {
                return Err(integrity("SQLite包含未知或改变的表、索引、触发器或视图。"));
            }
        }
        if tables != expected.keys().cloned().collect() || actual_indexes != indexes {
            return Err(integrity("SQLite完整schema清单不符。"));
        }
        let mut foreign = self
            .connection
            .prepare("PRAGMA foreign_key_check")
            .map_err(database)?;
        if foreign
            .query([])
            .map_err(database)?
            .next()
            .map_err(database)?
            .is_some()
        {
            return Err(integrity("SQLite外键不完整。"));
        }
        Ok(())
    }
    fn ensure_records(&self) -> Result<(), ProjectError> {
        let metadata: (i64, i64, String, String, String, String, String) = self.connection.query_row("SELECT singleton,schema_version,producer,case_id,fixture_scope_id,input_hashes_json,versions_json FROM compile_metadata", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).map_err(database)?;
        if metadata
            != (
                1,
                1,
                "fixture".into(),
                self.context.case_id.clone(),
                self.context.fixture_scope_id.clone(),
                json(&self.context.input_hashes)?,
                json(&self.context.versions)?,
            )
        {
            return Err(integrity("SQLite fixture作用域、指纹或版本改变。"));
        }
        let run: (String, String, String, String) = self
            .connection
            .query_row("SELECT run_id,started_at,stage,state FROM runs", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(database)?;
        let expected_stage = if self.phase == "published" {
            "finished"
        } else if self.phase == "failed" {
            "failed"
        } else {
            &self.phase
        };
        let expected_state = if self.phase == "published" {
            "succeeded"
        } else if self.phase == "failed" {
            "failed"
        } else {
            "running"
        };
        if run
            != (
                self.context.run_id.clone(),
                self.context.started_at.clone(),
                expected_stage.into(),
                expected_state.into(),
            )
        {
            return Err(integrity("Run身份或阶段转换不符。"));
        }
        if expected_state == "running" {
            let active: (Option<String>, Option<String>, Option<String>, String, String, String) = self.connection
                .query_row("SELECT finished_at,report_json_path,report_md_path,checks_json,counts_json,issues_json FROM runs", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)))
                .map_err(database)?;
            if active
                != (
                    None,
                    None,
                    None,
                    "{}".into(),
                    json(&self.context.counts)?,
                    "[]".into(),
                )
            {
                return Err(integrity("活动Run的完整初始审计字段被改变。"));
            }
        }
        let release: (String,String,String,i64,i64,String,String) = self.connection.query_row("SELECT release_id,run_id,status,ir_revision,plan_revision,renderer_version,manifest_json FROM releases", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).map_err(database)?;
        let release_status = match self.phase.as_str() {
            "published" => "published",
            "verified" => "verified",
            "failed" => &self.failure_release_status,
            _ => "staged",
        };
        if release
            != (
                self.context.release_id.clone(),
                self.context.run_id.clone(),
                release_status.into(),
                integer(self.context.ir_revision)?,
                integer(self.context.plan_revision)?,
                self.context.versions["renderer"].clone(),
                json(&self.manifest)?,
            )
        {
            return Err(integrity("Release身份、版本、状态或清单改变。"));
        }
        for table in ["compile_metadata", "runs", "releases"] {
            let count: i64 = self
                .connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .map_err(database)?;
            if count != 1 {
                return Err(integrity("SQLite身份表不是本轮唯一记录。"));
            }
        }
        let coverage_stage = if self.published {
            "validated"
        } else if self.manifest.is_empty() {
            "planned"
        } else {
            "rendered"
        };
        let mut statement = self.connection.prepare("SELECT run_id,span_id,disposition,atom_ids_json,required_atom_ids_json,duplicate_of_span_id,exclusion_reason,decision_source,policy_version,stage FROM coverage_records ORDER BY span_id").map_err(database)?;
        let actual = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                ))
            })
            .map_err(database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(database)?;
        let mut expected = self
            .coverage_rows
            .iter()
            .map(|r| {
                (
                    self.context.run_id.clone(),
                    r.0.clone(),
                    r.1.clone(),
                    r.2.clone(),
                    r.3.clone(),
                    r.4.clone(),
                    r.5.clone(),
                    r.6.clone(),
                    r.7.clone(),
                    coverage_stage.to_string(),
                )
            })
            .collect::<Vec<_>>();
        expected.sort_by(|a, b| a.1.cmp(&b.1));
        if actual != expected {
            return Err(integrity("Coverage完整位置或本轮Run身份改变。"));
        }
        let rendered_count: i64 = self
            .connection
            .query_row("SELECT count(*) FROM rendered_files", [], |r| r.get(0))
            .map_err(database)?;
        let publication_count: i64 = self
            .connection
            .query_row("SELECT count(*) FROM publication_commits", [], |r| r.get(0))
            .map_err(database)?;
        if rendered_count != self.manifest.len() as i64
            || publication_count != i64::from(self.published)
        {
            return Err(integrity("渲染清单或发布记录计数不符。"));
        }
        for file in &self.manifest {
            let row: (String,i64,String,i64,String,String) = self.connection.query_row("SELECT note_id,plan_revision,content_sha256,byte_length,mappings_json,links_json FROM rendered_files WHERE release_id=?1 AND relative_path=?2", params![self.context.release_id,file.relative_path], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(database)?;
            if row
                != (
                    file.note_id.clone(),
                    integer(file.plan_revision)?,
                    file.content_sha256.clone(),
                    integer(file.byte_length)?,
                    json(&file.content_mappings)?,
                    json(&file.links)?,
                )
            {
                return Err(integrity("SQLite渲染文件账本不符。"));
            }
        }
        Ok(())
    }
    fn refresh_database(&mut self) -> Result<(), ProjectError> {
        if !self.connection.is_autocommit() {
            return Err(error("COMPILE_RECOVERY_REQUIRED", "SQLite事务未结束。"));
        }
        let actual = read_file(&self.root().join(DATABASE), true)?;
        if actual.identity != self.database_identity {
            return Err(integrity("SQLite身份改变。"));
        }
        self.database_sha256 = Some(crate::text::sha256(&actual.bytes));
        Ok(())
    }
    fn transaction<F>(&mut self, publication: bool, action: F) -> Result<(), ProjectError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<(), ProjectError>,
    {
        self.ensure_integrity()?;
        let result = (|| {
            let tx = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(database)?;
            action(&tx)?;
            tx.commit().map_err(database)
        })();
        if let Err(original) = result {
            if !self.connection.is_autocommit() {
                self.connection
                    .execute_batch("ROLLBACK")
                    .map_err(database)?;
            }
            if !self.connection.is_autocommit() {
                return Err(error(
                    "COMPILE_RECOVERY_REQUIRED",
                    "事务回滚后仍未结束；禁止发布。",
                ));
            }
            self.refresh_database()?;
            return Err(original);
        }
        if publication {
            self.published = true;
            self.database_sha256 = None;
            Ok(())
        } else {
            self.refresh_database()
        }
    }
    fn validate_manifest(
        &self,
        manifest: &[RenderedFile],
        prefix: &str,
    ) -> Result<(), ProjectError> {
        if manifest.is_empty() || manifest.len() != self.expected_notes.len() {
            return Err(integrity("输出manifest不是全部计划笔记。"));
        }
        let mut paths = BTreeSet::new();
        for file in manifest {
            if !valid_relative_path(&file.relative_path)
                || !paths.insert(file.relative_path.clone())
                || self.expected_notes.get(&file.relative_path) != Some(&file.note_id)
                || file.plan_revision != self.context.plan_revision
            {
                return Err(integrity("manifest路径、笔记身份或revision不符。"));
            }
            let bytes = self.read_owned(&format!("{prefix}/{}", file.relative_path))?;
            if bytes.len() as u64 != file.byte_length
                || crate::text::sha256(&bytes) != file.content_sha256
            {
                return Err(integrity("实际输出SHA或字节数不符。"));
            }
            integer(file.byte_length)?;
        }
        let prefix = format!("{prefix}/");
        let actual: BTreeSet<_> = self
            .files
            .keys()
            .filter_map(|key| key.strip_prefix(&prefix).map(str::to_owned))
            .collect();
        if actual != paths {
            return Err(integrity("实际Vault精确清单不符。"));
        }
        Ok(())
    }
    pub(crate) fn record_staged(&mut self, manifest: &[RenderedFile]) -> Result<(), ProjectError> {
        if self.phase != "planned" {
            return Err(integrity("暂存登记阶段不合法。"));
        }
        self.validate_manifest(manifest, "staging/vault")?;
        let context = self.context.clone();
        let manifest_json = json(manifest)?;
        self.transaction(false, |tx| {
            if tx.execute("UPDATE runs SET stage='staged' WHERE run_id=?1 AND state='running' AND stage='planned'", [&context.run_id]).map_err(database)? != 1 { return Err(integrity("Run暂存转换失败。")); }
            if tx.execute("UPDATE releases SET manifest_json=?1 WHERE release_id=?2 AND status='staged'", params![manifest_json,context.release_id]).map_err(database)? != 1 { return Err(integrity("Release暂存转换失败。")); }
            for file in manifest { tx.execute("INSERT INTO rendered_files VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![context.release_id,file.relative_path,file.note_id,integer(file.plan_revision)?,file.content_sha256,integer(file.byte_length)?,json(&file.content_mappings)?,json(&file.links)?]).map_err(database)?; }
            tx.execute("UPDATE coverage_records SET stage='rendered' WHERE run_id=?1", [&context.run_id]).map_err(database)?; Ok(())
        })?;
        self.phase = "staged".into();
        self.manifest = manifest.to_vec();
        self.ensure_integrity()
    }
    pub(crate) fn record_verified(
        &mut self,
        manifest: &[RenderedFile],
    ) -> Result<(), ProjectError> {
        if self.phase != "staged" || manifest != self.manifest {
            return Err(integrity("Vault验证登记阶段或清单不合法。"));
        }
        self.validate_manifest(manifest, "vault")?;
        let context = self.context.clone();
        self.transaction(false, |tx| {
            if tx.execute("UPDATE releases SET status='verified' WHERE release_id=?1 AND status='staged'", [&context.release_id]).map_err(database)? != 1 { return Err(integrity("Release验证转换失败。")); }
            if tx.execute("UPDATE runs SET stage='verified' WHERE run_id=?1 AND state='running' AND stage='staged'", [&context.run_id]).map_err(database)? != 1 { return Err(integrity("Run验证转换失败。")); } Ok(())
        })?;
        self.phase = "verified".into();
        self.ensure_integrity()
    }
    pub(crate) fn record_failure(
        &mut self,
        report: &CompileReport,
        release_status: &str,
        paths: &ReportPaths,
    ) -> Result<(), ProjectError> {
        if self.published || !matches!(release_status, "staged" | "recovery_required") {
            return Err(integrity("失败登记不能宣称已发布。"));
        }
        self.validate_report(report, paths, false)?;
        let context = self.context.clone();
        self.transaction(false, |tx| {
            let count: i64 = tx.query_row("SELECT count(*) FROM publication_commits", [], |r| r.get(0)).map_err(database)?;
            if count != 0 { return Err(integrity("存在发布记录，不能再登记失败。")); }
            if tx.execute("UPDATE runs SET finished_at=?1,stage='failed',state='failed',report_json_path=?2,report_md_path=?3,checks_json=?4,counts_json=?5,issues_json=?6 WHERE run_id=?7 AND state='running'", params![report.finished_at,paths.json,paths.markdown,json(&report.checks)?,json(&report.counts)?,json(&report.issues)?,context.run_id]).map_err(database)? != 1 { return Err(integrity("Run失败转换不合法。")); }
            if tx.execute("UPDATE releases SET status=?1 WHERE release_id=?2 AND status IN ('staged','verified')", params![release_status,context.release_id]).map_err(database)? != 1 { return Err(integrity("Release失败转换不合法。")); } Ok(())
        })?;
        self.phase = "failed".into();
        self.failure_release_status = release_status.into();
        self.ensure_integrity()
    }
    fn validate_report(
        &self,
        report: &CompileReport,
        paths: &ReportPaths,
        publication: bool,
    ) -> Result<(String, String), ProjectError> {
        let allowed_paths = (paths.json == "reports/validation.json"
            && paths.markdown == "reports/validation.md")
            || (!publication
                && paths.json == "reports/failure.json"
                && paths.markdown == "reports/failure.md");
        let dates_ordered = OffsetDateTime::parse(&report.started_at, &Rfc3339)
            .ok()
            .zip(OffsetDateTime::parse(&report.finished_at, &Rfc3339).ok())
            .is_some_and(|(start, end)| start <= end);
        if !allowed_paths
            || report.schema_version != 1
            || report.producer != "fixture"
            || report.case_id != self.context.case_id
            || report.run_id != self.context.run_id
            || report.release_id != self.context.release_id
            || report.started_at != self.context.started_at
            || !utc(&report.finished_at)
            || !dates_ordered
            || report.input_hashes != self.context.input_hashes
            || report.versions != self.context.versions
            || report.publication.database != DATABASE
            || report.publication.commit_table != "publication_commits"
            || report.publication.release_id != self.context.release_id
            || self
                .context
                .counts
                .iter()
                .any(|(key, value)| report.counts.get(key) != Some(value))
            || report.counts.get("issues") != Some(&(report.issues.len() as u64))
        {
            return Err(integrity("报告身份、版本、UTC、基础计数或发布依据不符。"));
        }
        let actual = self.read_owned(&paths.json)?;
        let decoded: CompileReport = serde_json::from_slice(&actual)
            .map_err(|e| integrity(format!("实际JSON报告不能解码：{e}")))?;
        if decoded != *report {
            return Err(integrity("磁盘JSON报告与实际提交事实不一致。"));
        }
        let markdown = self.read_owned(&paths.markdown)?;
        if markdown != crate::compile_report::markdown(report).as_bytes() {
            return Err(integrity("Markdown报告与同一typed验证快照不一致。"));
        }
        if publication {
            if REQUIRED_CHECKS.iter().any(|name| {
                report
                    .checks
                    .get(*name)
                    .is_none_or(|check| check.status != "passed" || check.issue_count != 0)
            }) || report.checks.iter().any(|(name, check)| {
                if name == "scientific_quality" {
                    check.status != "not_run" || check.issue_count != 0
                } else {
                    check.status != "passed" || check.issue_count != 0
                }
            }) || !report.checks.contains_key("scientific_quality")
                || report.result != "validated"
                || !report.issues.is_empty()
                || report.files != self.manifest
                || report.dimensions.len() != 4
                || [
                    "source_coverage",
                    "knowledge_compilation",
                    "quality_validation",
                ]
                .iter()
                .any(|name| {
                    report.dimensions.get(*name).is_none_or(|dimension| {
                        dimension.state != "complete" || dimension.reason.trim().is_empty()
                    })
                })
                || report
                    .dimensions
                    .get("optional_research")
                    .is_none_or(|dimension| {
                        dimension.state != "not_applicable" || dimension.reason.trim().is_empty()
                    })
                || report.counts.get("rendered_files") != Some(&(self.manifest.len() as u64))
                || report.counts.get("mappings")
                    != Some(
                        &(self
                            .manifest
                            .iter()
                            .map(|file| file.content_mappings.len() as u64)
                            .sum()),
                    )
                || report.counts.get("links")
                    != Some(
                        &(self
                            .manifest
                            .iter()
                            .map(|file| file.links.len() as u64)
                            .sum()),
                    )
            {
                return Err(integrity(
                    "failed、not_run、缺检查、非完整核心维度或实际输出计数不符，禁止published。",
                ));
            }
        } else if report.result != "failed" {
            return Err(integrity("失败登记需要failed验证快照。"));
        }
        Ok((crate::text::sha256(&actual), crate::text::sha256(&markdown)))
    }
    pub(crate) fn commit_publication(
        &mut self,
        report: &CompileReport,
        manifest: &[RenderedFile],
        paths: &ReportPaths,
    ) -> Result<(), ProjectError> {
        if self.phase != "verified" || self.published || manifest != self.manifest {
            return Err(integrity("最后发布只能从verified和同一清单转换。"));
        }
        self.validate_manifest(manifest, "vault")?;
        let (json_sha, markdown_sha) = self.validate_report(report, paths, true)?;
        self.ensure_integrity()?;
        let manifest_json = json(manifest)?;
        let manifest_sha = crate::text::sha256(manifest_json.as_bytes());
        let context = self.context.clone();
        let spans = self.coverage_rows.len();
        self.transaction(true, |tx| {
            tx.execute("INSERT INTO publication_commits VALUES(?1,?2,?3,?4,?5,?6,?7)", params![context.release_id,report.finished_at,paths.json,json_sha,paths.markdown,markdown_sha,manifest_sha]).map_err(database)?;
            if tx.execute("UPDATE runs SET finished_at=?1,stage='finished',state='succeeded',report_json_path=?2,report_md_path=?3,checks_json=?4,counts_json=?5,issues_json=?6 WHERE run_id=?7 AND state='running' AND stage='verified'", params![report.finished_at,paths.json,paths.markdown,json(&report.checks)?,json(&report.counts)?,json(&report.issues)?,context.run_id]).map_err(database)? != 1 { return Err(integrity("Run发布转换失败。")); }
            if tx.execute("UPDATE releases SET status='published' WHERE release_id=?1 AND status='verified'", [&context.release_id]).map_err(database)? != 1 { return Err(integrity("Release发布转换失败。")); }
            if tx.execute("UPDATE coverage_records SET stage='validated' WHERE run_id=?1 AND stage='rendered'", [&context.run_id]).map_err(database)? != spans { return Err(integrity("Coverage不是全部本轮位置。")); } Ok(())
        })?;
        // No required filesystem operation follows the one authoritative commit.
        self.phase = "published".into();
        Ok(())
    }
    fn validate_committed_publication(&self) -> Result<(), ProjectError> {
        if !self.published || self.phase != "published" {
            return Err(integrity("已提交发布的恢复身份不符。"));
        }
        let paths = ReportPaths {
            json: "reports/validation.json".into(),
            markdown: "reports/validation.md".into(),
        };
        let report: CompileReport = serde_json::from_slice(&self.read_owned(&paths.json)?)
            .map_err(|e| integrity(format!("已提交报告不能严格解码：{e}")))?;
        self.validate_manifest(&self.manifest, "staging/vault")?;
        self.validate_manifest(&self.manifest, "vault")?;
        let (json_sha, markdown_sha) = self.validate_report(&report, &paths, true)?;
        let expected_run = (
            report.finished_at.clone(),
            paths.json.clone(),
            paths.markdown.clone(),
            json(&report.checks)?,
            json(&report.counts)?,
            json(&report.issues)?,
        );
        let actual_run: (String, String, String, String, String, String) = self.connection
            .query_row("SELECT finished_at,report_json_path,report_md_path,checks_json,counts_json,issues_json FROM runs WHERE run_id=?1", [&self.context.run_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)))
            .map_err(database)?;
        let expected_commit = (
            self.context.release_id.clone(),
            report.finished_at,
            paths.json,
            json_sha,
            paths.markdown,
            markdown_sha,
            crate::text::sha256(json(&self.manifest)?.as_bytes()),
        );
        let actual_commit: (String, String, String, String, String, String, String) = self.connection
            .query_row("SELECT release_id,committed_at,report_json_path,report_json_sha256,report_md_path,report_md_sha256,manifest_sha256 FROM publication_commits", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)))
            .map_err(database)?;
        if actual_run != expected_run || actual_commit != expected_commit {
            return Err(integrity(
                "唯一发布提交、完整Run事实及原双报告不属于同一确认结果。",
            ));
        }
        Ok(())
    }
    pub(crate) fn recover(
        &mut self,
        report: &CompileReport,
        release_status: &str,
        paths: &ReportPaths,
        cause: &ProjectError,
    ) -> Result<(), ProjectError> {
        if self.published
            || !matches!(release_status, "staged" | "verified" | "recovery_required")
            || report.run_id != self.context.run_id
            || report.release_id != self.context.release_id
        {
            return Err(integrity("恢复诊断身份或状态不符。"));
        }
        // SQLite can be unavailable here. Only independently confirmed ownership
        // allows the diagnostic; never issue SQL that could recover a hot journal.
        self.ensure_tree()?;
        let value = serde_json::json!({"schema_version":1,"producer":"fixture","run_id":self.context.run_id,"release_id":self.context.release_id,"stage":self.phase,"release_status":release_status,"recovery_required":true,"reports":paths,"cause":{"code":cause.code(),"message":cause.to_string()}});
        let bytes = serde_json::to_vec_pretty(&value).map_err(|e| integrity(e.to_string()))?;
        self.write_recovery(&bytes)
    }
    fn write_recovery(&mut self, bytes: &[u8]) -> Result<(), ProjectError> {
        let relative = "state/recovery.json";
        let path = self.root().join(relative);
        if self.files.contains_key(relative) || project::exists(&path)? {
            return Err(error(
                "COMPILE_OUTPUT_EXISTS",
                "恢复诊断已经存在，不能覆盖。",
            ));
        }
        self.ensure_tree()?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1).custom_flags(0x00200000);
        }
        let mut file = options
            .open(&path)
            .map_err(|e| ProjectError::io(&path, e))?;
        let original =
            same_file::Handle::from_path(&path).map_err(|e| ProjectError::io(&path, e))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| ProjectError::io(&path, e))?;
        drop(file);
        let owned = read_file(&path, false)?;
        if owned.identity != original || owned.bytes != bytes {
            return Err(integrity("恢复诊断身份或字节改变。"));
        }
        self.files.insert(relative.into(), owned);
        self.ensure_tree()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use uuid::Uuid;

    struct Sandbox(PathBuf);
    impl Sandbox {
        fn new() -> Self {
            let parent =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/compile-store-tests");
            fs::create_dir_all(&parent).unwrap();
            let root = parent.join(Uuid::new_v4().to_string());
            fs::create_dir(&root).unwrap();
            Self(root.canonicalize().unwrap())
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            let allowed = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/compile-store-tests")
                .canonicalize()
                .unwrap();
            assert!(self.0.starts_with(&allowed) && self.0 != allowed);
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fixture_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/text-compare-v1")
            .canonicalize()
            .unwrap()
    }
    fn context(case: &LoadedCase) -> CompileContext {
        CompileContext {
            case_id: case.manifest.case_id.clone(),
            run_id: Uuid::new_v4().to_string(),
            release_id: Uuid::new_v4().to_string(),
            started_at: "2026-11-01T10:00:00Z".into(),
            fixture_scope_id: case.ir.coverage_run_id.clone(),
            ir_revision: case.ir.revision,
            plan_revision: case.plan.revision,
            input_hashes: BTreeMap::from([
                ("case_sha256".into(), case.case_sha256.clone()),
                ("ir_sha256".into(), case.ir_sha256.clone()),
                ("plan_sha256".into(), case.plan_sha256.clone()),
                ("expected_sha256".into(), case.expected_sha256.clone()),
            ]),
            versions: BTreeMap::from([("renderer".into(), "fixture-markdown-v1".into())]),
            counts: BTreeMap::from([
                ("sources".into(), case.manifest.sources.len() as u64),
                ("spans".into(), case.spans.len() as u64),
                ("atoms".into(), case.ir.atoms.len() as u64),
                (
                    "required_atoms".into(),
                    case.ir.atoms.iter().filter(|atom| atom.required).count() as u64,
                ),
                ("notes".into(), case.plan.notes.len() as u64),
                (
                    "nodes".into(),
                    case.plan
                        .notes
                        .iter()
                        .flat_map(|note| &note.sections)
                        .map(|section| section.ordered_nodes.len() as u64)
                        .sum(),
                ),
            ]),
        }
    }
    #[test]
    fn preflight_selects_an_absent_sibling_without_writing() {
        let sandbox = Sandbox::new();
        let output = sandbox.0.join("全新 编译");
        let checked =
            Store::preflight(&fixture_root(), &output).expect("safe absent root is accepted");
        assert_eq!(checked, output);
        assert!(!output.exists());
    }
    #[test]
    fn creation_persists_independent_scope_and_actual_counts() {
        let sandbox = Sandbox::new();
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        let context = context(&case);
        let store = Store::create(&sandbox.0.join("output"), &context, &case)
            .expect("new output establishes compile state");
        let connection = rusqlite::Connection::open_with_flags(
            store.root().join("state/compile.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        let row: (String, String) = connection
            .query_row("SELECT run_id,counts_json FROM runs", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(row.0, context.run_id);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&row.1).unwrap()["spans"],
            12
        );
        let scope: String = connection
            .query_row("SELECT fixture_scope_id FROM compile_metadata", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(scope, "R8s");
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM coverage_records WHERE run_id=?1",
                [&context.run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 12);
        assert_eq!(
            connection
                .query_row("SELECT status FROM releases", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "staged"
        );
    }
    fn new_store(sandbox: &Sandbox) -> Store {
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        Store::create(&sandbox.0.join("output"), &context(&case), &case).unwrap()
    }
    fn staged_store(sandbox: &Sandbox) -> (Store, Vec<RenderedFile>) {
        let mut store = new_store(sandbox);
        let mut manifest = Vec::new();
        // The store tests ownership and transactional authority. Text semantics
        // are independently exercised by fixture_render's literal expectations.
        for (path, note_id) in store.expected_notes.clone() {
            let bytes = format!("# Store test {note_id}\n").into_bytes();
            store
                .write_owned(&format!("staging/vault/{path}"), &bytes)
                .unwrap();
            manifest.push(RenderedFile {
                relative_path: path,
                note_id,
                plan_revision: store.context.plan_revision,
                content_sha256: crate::text::sha256(&bytes),
                byte_length: bytes.len() as u64,
                content_mappings: Vec::new(),
                links: Vec::new(),
            });
        }
        store.record_staged(&manifest).unwrap();
        (store, manifest)
    }
    fn verified_store(sandbox: &Sandbox) -> (Store, Vec<RenderedFile>) {
        let (mut store, manifest) = staged_store(sandbox);
        for file in &manifest {
            let bytes = store
                .read_owned(&format!("staging/vault/{}", file.relative_path))
                .unwrap();
            store
                .write_owned(&format!("vault/{}", file.relative_path), &bytes)
                .unwrap();
        }
        store.record_verified(&manifest).unwrap();
        (store, manifest)
    }
    fn report(store: &Store, manifest: &[RenderedFile]) -> CompileReport {
        use crate::compile_model::{CheckFact, Dimension, PublicationRef};
        let mut checks: BTreeMap<_, _> = REQUIRED_CHECKS
            .iter()
            .map(|name| {
                (
                    (*name).to_string(),
                    CheckFact {
                        status: "passed".into(),
                        issue_count: 0,
                    },
                )
            })
            .collect();
        checks.insert(
            "scientific_quality".into(),
            CheckFact {
                status: "not_run".into(),
                issue_count: 0,
            },
        );
        let dimensions = [
            "source_coverage",
            "knowledge_compilation",
            "quality_validation",
            "optional_research",
        ]
        .iter()
        .map(|name| {
            (
                (*name).to_string(),
                Dimension {
                    state: if *name == "optional_research" {
                        "not_applicable"
                    } else {
                        "complete"
                    }
                    .into(),
                    reason: "Storage unit test snapshot".into(),
                    counts: BTreeMap::new(),
                },
            )
        })
        .collect();
        let mut counts = store.context.counts.clone();
        counts.extend([
            ("rendered_files".into(), manifest.len() as u64),
            ("mappings".into(), 0),
            ("links".into(), 0),
            ("issues".into(), 0),
        ]);
        CompileReport {
            schema_version: 1,
            producer: "fixture".into(),
            case_id: store.context.case_id.clone(),
            run_id: store.context.run_id.clone(),
            release_id: store.context.release_id.clone(),
            started_at: store.context.started_at.clone(),
            finished_at: "2026-11-01T10:00:01Z".into(),
            result: "validated".into(),
            dimensions,
            checks,
            counts,
            issues: Vec::new(),
            files: manifest.to_vec(),
            input_hashes: store.context.input_hashes.clone(),
            versions: store.context.versions.clone(),
            publication: PublicationRef {
                database: DATABASE.into(),
                commit_table: "publication_commits".into(),
                release_id: store.context.release_id.clone(),
            },
        }
    }
    fn write_report(store: &mut Store, report: &CompileReport) -> ReportPaths {
        let paths = ReportPaths {
            json: "reports/validation.json".into(),
            markdown: "reports/validation.md".into(),
        };
        store
            .write_owned(&paths.json, &serde_json::to_vec_pretty(report).unwrap())
            .unwrap();
        store
            .write_owned(
                &paths.markdown,
                crate::compile_report::markdown(report).as_bytes(),
            )
            .unwrap();
        paths
    }
    fn publication_count(store: &Store) -> i64 {
        store
            .connection
            .query_row("SELECT count(*) FROM publication_commits", [], |r| r.get(0))
            .unwrap()
    }
    #[test]
    fn snapshot_roundtrip_reopens_confirmed_partial_inventory_without_plaintext() {
        let sandbox = Sandbox::new();
        let mut store = new_store(&sandbox);
        let body = b"DO_NOT_PERSIST_ARTIFACT_TEXT_IN_THE_SNAPSHOT\n";
        store.write_owned("state/confirmed-unit.txt", body).unwrap();
        let context = store.context.clone();
        let snapshot = store.snapshot().unwrap();
        let encoded = serde_json::to_string(&snapshot).unwrap();
        assert!(!encoded.contains("DO_NOT_PERSIST_ARTIFACT_TEXT_IN_THE_SNAPSHOT"));
        let decoded: StoreSnapshot = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, snapshot);
        drop(store);
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        let resumed = Store::resume(&snapshot.root, &context, &case, &snapshot, false).unwrap();
        assert_eq!(
            resumed.read_owned("state/confirmed-unit.txt").unwrap(),
            body
        );
        assert_eq!(resumed.snapshot().unwrap(), snapshot);
        assert_eq!(publication_count(&resumed), 0);
    }
    #[test]
    fn snapshot_schema_and_changed_context_are_rejected_before_reuse() {
        let sandbox = Sandbox::new();
        let store = new_store(&sandbox);
        let snapshot = store.snapshot().unwrap();
        let context = store.context.clone();
        let before = fs::read(store.root().join(DATABASE)).unwrap();
        let mut value = serde_json::to_value(&snapshot).unwrap();
        value["unknown_field"] = true.into();
        assert!(serde_json::from_value::<StoreSnapshot>(value).is_err());
        let mut value = serde_json::to_value(&snapshot).unwrap();
        value["parent"]["identity"]["unknown_field"] = true.into();
        assert!(serde_json::from_value::<StoreSnapshot>(value).is_err());
        drop(store);
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        let mut invalid = snapshot.clone();
        invalid.schema_version = 2;
        assert!(Store::resume(&snapshot.root, &context, &case, &invalid, false).is_err());
        let mut changed = context.clone();
        changed
            .versions
            .insert("compiler".into(), "a-different-compiler".into());
        assert!(Store::resume(&snapshot.root, &changed, &case, &snapshot, false).is_err());
        assert_eq!(fs::read(snapshot.root.join(DATABASE)).unwrap(), before);
    }
    #[test]
    fn snapshot_verified_resume_commits_exactly_once_and_keeps_original_ids() {
        let sandbox = Sandbox::new();
        let (mut store, manifest) = verified_store(&sandbox);
        let facts = report(&store, &manifest);
        let paths = write_report(&mut store, &facts);
        let context = store.context.clone();
        let snapshot = store.snapshot().unwrap();
        drop(store);
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        let mut resumed = Store::resume(&snapshot.root, &context, &case, &snapshot, false).unwrap();
        resumed
            .commit_publication(&facts, &manifest, &paths)
            .unwrap();
        assert_eq!(publication_count(&resumed), 1);
        let published = resumed.snapshot().unwrap();
        assert_eq!(published.context.run_id, context.run_id);
        assert_eq!(published.context.release_id, context.release_id);
        assert_eq!(published.context.started_at, context.started_at);
        assert_eq!(published.manifest, manifest);
        drop(resumed);
        let mut resumed =
            Store::resume(&published.root, &context, &case, &published, false).unwrap();
        assert!(resumed.published);
        let before = fs::read(published.root.join(DATABASE)).unwrap();
        assert!(
            resumed
                .commit_publication(&facts, &manifest, &paths)
                .is_err()
        );
        assert_eq!(publication_count(&resumed), 1);
        assert_eq!(fs::read(published.root.join(DATABASE)).unwrap(), before);
    }
    #[test]
    fn snapshot_reconciles_only_a_fully_bound_publication_commit_window() {
        let sandbox = Sandbox::new();
        let (mut store, manifest) = verified_store(&sandbox);
        let facts = report(&store, &manifest);
        let paths = write_report(&mut store, &facts);
        let context = store.context.clone();
        let snapshot = store.snapshot().unwrap();
        store.commit_publication(&facts, &manifest, &paths).unwrap();
        let committed = fs::read(store.root().join(DATABASE)).unwrap();
        drop(store);
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        assert!(Store::resume(&snapshot.root, &context, &case, &snapshot, false).is_err());
        let resumed = Store::resume(&snapshot.root, &context, &case, &snapshot, true).unwrap();
        assert!(resumed.published);
        assert_eq!(publication_count(&resumed), 1);
        drop(resumed);
        let resumed = Store::resume(&snapshot.root, &context, &case, &snapshot, true).unwrap();
        assert!(resumed.published);
        assert_eq!(publication_count(&resumed), 1);
        assert_eq!(fs::read(snapshot.root.join(DATABASE)).unwrap(), committed);
    }
    #[test]
    fn snapshot_reconciliation_rejects_a_forged_commit_hash_even_with_one_row() {
        let sandbox = Sandbox::new();
        let (mut store, manifest) = verified_store(&sandbox);
        let facts = report(&store, &manifest);
        let paths = write_report(&mut store, &facts);
        let context = store.context.clone();
        let snapshot = store.snapshot().unwrap();
        store.commit_publication(&facts, &manifest, &paths).unwrap();
        drop(store);
        let connection = Connection::open(snapshot.root.join(DATABASE)).unwrap();
        connection
            .execute(
                "UPDATE publication_commits SET report_md_sha256=?1",
                ["0".repeat(64)],
            )
            .unwrap();
        connection.close().unwrap();
        let corrupted = fs::read(snapshot.root.join(DATABASE)).unwrap();
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        assert!(Store::resume(&snapshot.root, &context, &case, &snapshot, true).is_err());
        assert_eq!(fs::read(snapshot.root.join(DATABASE)).unwrap(), corrupted);
    }
    #[test]
    fn snapshot_same_bytes_replacement_and_hardlink_are_not_owned() {
        for database_file in [false, true] {
            let sandbox = Sandbox::new();
            let mut store = new_store(&sandbox);
            store
                .write_owned("state/unit.txt", b"confirmed bytes\n")
                .unwrap();
            let context = store.context.clone();
            let snapshot = store.snapshot().unwrap();
            let path = snapshot.root.join(if database_file {
                DATABASE
            } else {
                "state/unit.txt"
            });
            let original = fs::read(&path).unwrap();
            drop(store);
            let retained = sandbox.0.join("retained-original");
            fs::rename(&path, &retained).unwrap();
            fs::write(&path, &original).unwrap();
            let case = crate::fixture_input::load(&fixture_root()).unwrap();
            assert!(Store::resume(&snapshot.root, &context, &case, &snapshot, false).is_err());
            assert_eq!(fs::read(retained).unwrap(), original);
            assert_eq!(fs::read(path).unwrap(), original);
        }
        let sandbox = Sandbox::new();
        let mut store = new_store(&sandbox);
        store
            .write_owned("state/unit.txt", b"confirmed bytes\n")
            .unwrap();
        let context = store.context.clone();
        let snapshot = store.snapshot().unwrap();
        drop(store);
        fs::hard_link(
            snapshot.root.join("state/unit.txt"),
            sandbox.0.join("external-alias"),
        )
        .unwrap();
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        assert!(Store::resume(&snapshot.root, &context, &case, &snapshot, false).is_err());
        assert_eq!(
            fs::read(sandbox.0.join("external-alias")).unwrap(),
            b"confirmed bytes\n"
        );
    }
    #[test]
    fn snapshot_empty_directory_replacement_and_unknown_partial_are_preserved() {
        let sandbox = Sandbox::new();
        let store = new_store(&sandbox);
        let context = store.context.clone();
        let snapshot = store.snapshot().unwrap();
        let before = fs::read(store.root().join(DATABASE)).unwrap();
        drop(store);
        fs::rename(
            snapshot.root.join("reports"),
            sandbox.0.join("retained-reports"),
        )
        .unwrap();
        fs::create_dir(snapshot.root.join("reports")).unwrap();
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        assert!(Store::resume(&snapshot.root, &context, &case, &snapshot, false).is_err());
        assert!(sandbox.0.join("retained-reports").is_dir());
        assert_eq!(fs::read(snapshot.root.join(DATABASE)).unwrap(), before);
        let sandbox = Sandbox::new();
        let store = new_store(&sandbox);
        let context = store.context.clone();
        let snapshot = store.snapshot().unwrap();
        let before = fs::read(store.root().join(DATABASE)).unwrap();
        drop(store);
        fs::write(
            snapshot.root.join("vault/unconfirmed.md"),
            b"preserve unconfirmed artifact\n",
        )
        .unwrap();
        assert_eq!(
            Store::resume(&snapshot.root, &context, &case, &snapshot, false)
                .err()
                .unwrap()
                .code(),
            "COMPILE_RECOVERY_REQUIRED"
        );
        assert_eq!(
            fs::read(snapshot.root.join("vault/unconfirmed.md")).unwrap(),
            b"preserve unconfirmed artifact\n"
        );
        assert_eq!(fs::read(snapshot.root.join(DATABASE)).unwrap(), before);
    }
    fn pending_staging(store: &Store) -> BTreeMap<String, Vec<u8>> {
        store
            .expected_notes
            .iter()
            .map(|(path, id)| {
                (
                    format!("staging/vault/{path}"),
                    format!("# Store test {id}\n").into_bytes(),
                )
            })
            .collect()
    }
    fn intent(
        phase: &str,
        manifest: &[RenderedFile],
        files: &BTreeMap<String, Vec<u8>>,
    ) -> StoreIntent {
        StoreIntent {
            phase: phase.into(),
            manifest: manifest.to_vec(),
            files: files
                .iter()
                .map(|(path, bytes)| {
                    (
                        path.clone(),
                        FileExpectation {
                            sha256: crate::text::sha256(bytes),
                            byte_length: bytes.len() as u64,
                        },
                    )
                })
                .collect(),
        }
    }
    #[test]
    fn snapshot_intent_recovers_first_synced_file_and_staged_commit_gap() {
        let sandbox = Sandbox::new();
        let mut store = new_store(&sandbox);
        let context = store.context.clone();
        let before_files = store.snapshot().unwrap();
        let derived = pending_staging(&store);
        let prepared = intent("planned", &[], &derived);
        let (first, body) = derived.iter().next().unwrap();
        store.write_owned(first, body).unwrap();
        drop(store);
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        assert!(Store::resume(&before_files.root, &context, &case, &before_files, false).is_err());
        let mut resumed = Store::resume_with_intent(
            &before_files.root,
            &context,
            &case,
            Some(&before_files),
            &prepared,
            &derived,
        )
        .unwrap();
        assert_eq!(resumed.read_owned(first).unwrap(), *body);
        assert_eq!(resumed.snapshot().unwrap().files.len(), 1);
        for (path, bytes) in &derived {
            if path != first {
                resumed.write_owned(path, bytes).unwrap();
            }
        }
        let manifest: Vec<_> = resumed
            .expected_notes
            .iter()
            .map(|(path, note_id)| {
                let bytes = &derived[&format!("staging/vault/{path}")];
                RenderedFile {
                    relative_path: path.clone(),
                    note_id: note_id.clone(),
                    plan_revision: context.plan_revision,
                    content_sha256: crate::text::sha256(bytes),
                    byte_length: bytes.len() as u64,
                    content_mappings: vec![],
                    links: vec![],
                }
            })
            .collect();
        let before_sql = resumed.snapshot().unwrap();
        let staged = intent("staged", &manifest, &BTreeMap::new());
        resumed.record_staged(&manifest).unwrap();
        drop(resumed);
        let mut resumed = Store::resume_with_intent(
            &before_sql.root,
            &context,
            &case,
            Some(&before_sql),
            &staged,
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(resumed.phase, "staged");
        assert_eq!(resumed.manifest, manifest);
        assert!(resumed.record_staged(&manifest).is_err());
        assert_eq!(publication_count(&resumed), 0);
    }
    #[test]
    fn snapshot_intent_recovers_verified_and_published_commits_exactly_once() {
        let sandbox = Sandbox::new();
        let (mut store, manifest) = staged_store(&sandbox);
        let context = store.context.clone();
        let staged = store.snapshot().unwrap();
        let derived: BTreeMap<_, _> = manifest
            .iter()
            .map(|file| {
                (
                    format!("vault/{}", file.relative_path),
                    store
                        .read_owned(&format!("staging/vault/{}", file.relative_path))
                        .unwrap(),
                )
            })
            .collect();
        let verified = intent("verified", &manifest, &derived);
        for (path, body) in &derived {
            store.write_owned(path, body).unwrap();
        }
        store.record_verified(&manifest).unwrap();
        drop(store);
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        let mut resumed = Store::resume_with_intent(
            &staged.root,
            &context,
            &case,
            Some(&staged),
            &verified,
            &derived,
        )
        .unwrap();
        assert_eq!(resumed.phase, "verified");
        assert!(resumed.record_verified(&manifest).is_err());
        let facts = report(&resumed, &manifest);
        let paths = write_report(&mut resumed, &facts);
        let before_commit = resumed.snapshot().unwrap();
        let publish = intent("published", &manifest, &BTreeMap::new());
        resumed
            .commit_publication(&facts, &manifest, &paths)
            .unwrap();
        let original = fs::read(resumed.root().join(DATABASE)).unwrap();
        drop(resumed);
        let mut resumed = Store::resume_with_intent(
            &before_commit.root,
            &context,
            &case,
            Some(&before_commit),
            &publish,
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(resumed.published);
        assert_eq!(publication_count(&resumed), 1);
        assert!(
            resumed
                .commit_publication(&facts, &manifest, &paths)
                .is_err()
        );
        assert_eq!(
            fs::read(before_commit.root.join(DATABASE)).unwrap(),
            original
        );
    }
    #[test]
    fn snapshot_intent_rechecks_complete_initialization_and_restarts_only_absent_root() {
        let sandbox = Sandbox::new();
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        let context = context(&case);
        let root = sandbox.0.join("pending-output");
        let prepared = intent("planned", &[], &BTreeMap::new());
        let store =
            Store::resume_with_intent(&root, &context, &case, None, &prepared, &BTreeMap::new())
                .unwrap();
        let before = store.snapshot().unwrap();
        let original = fs::read(root.join(DATABASE)).unwrap();
        drop(store);
        let resumed =
            Store::resume_with_intent(&root, &context, &case, None, &prepared, &BTreeMap::new())
                .unwrap();
        assert_eq!(resumed.snapshot().unwrap(), before);
        assert_eq!(resumed.context.run_id, context.run_id);
        assert_eq!(resumed.context.release_id, context.release_id);
        assert_eq!(fs::read(root.join(DATABASE)).unwrap(), original);
        drop(resumed);
        fs::write(root.join("vault/unregistered.md"), b"unowned\n").unwrap();
        assert!(
            Store::resume_with_intent(&root, &context, &case, None, &prepared, &BTreeMap::new())
                .is_err()
        );
        assert_eq!(
            fs::read(root.join("vault/unregistered.md")).unwrap(),
            b"unowned\n"
        );
        assert_eq!(fs::read(root.join(DATABASE)).unwrap(), original);
    }
    #[test]
    fn snapshot_intent_rejects_truncated_unknown_and_unsealed_pending_files() {
        let sandbox = Sandbox::new();
        let store = new_store(&sandbox);
        let context = store.context.clone();
        let snapshot = store.snapshot().unwrap();
        drop(store);
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        let derived = BTreeMap::from([(
            "state/pending.txt".into(),
            b"complete expected artifact\n".to_vec(),
        )]);
        let prepared = intent("planned", &[], &derived);
        let original = fs::read(snapshot.root.join(DATABASE)).unwrap();
        fs::write(snapshot.root.join("state/pending.txt"), b"truncated").unwrap();
        assert!(
            Store::resume_with_intent(
                &snapshot.root,
                &context,
                &case,
                Some(&snapshot),
                &prepared,
                &derived
            )
            .is_err()
        );
        assert_eq!(
            fs::read(snapshot.root.join("state/pending.txt")).unwrap(),
            b"truncated"
        );
        fs::write(
            snapshot.root.join("state/pending.txt"),
            &derived["state/pending.txt"],
        )
        .unwrap();
        let forged =
            BTreeMap::from([("state/pending.txt".into(), b"forged derived bytes".to_vec())]);
        assert!(
            Store::resume_with_intent(
                &snapshot.root,
                &context,
                &case,
                Some(&snapshot),
                &prepared,
                &forged
            )
            .is_err()
        );
        fs::write(
            snapshot.root.join("state/unregistered.txt"),
            b"preserve external extra",
        )
        .unwrap();
        assert!(
            Store::resume_with_intent(
                &snapshot.root,
                &context,
                &case,
                Some(&snapshot),
                &prepared,
                &derived
            )
            .is_err()
        );
        assert_eq!(
            fs::read(snapshot.root.join("state/unregistered.txt")).unwrap(),
            b"preserve external extra"
        );
        assert_eq!(fs::read(snapshot.root.join(DATABASE)).unwrap(), original);
    }
    #[test]
    fn snapshot_intent_does_not_mask_unrelated_sql_changes_or_old_identity_changes() {
        let sandbox = Sandbox::new();
        let (mut store, manifest) = staged_store(&sandbox);
        let context = store.context.clone();
        let snapshot = store.snapshot().unwrap();
        let derived: BTreeMap<_, _> = manifest
            .iter()
            .map(|file| {
                (
                    format!("vault/{}", file.relative_path),
                    store
                        .read_owned(&format!("staging/vault/{}", file.relative_path))
                        .unwrap(),
                )
            })
            .collect();
        for (name, body) in &derived {
            store.write_owned(name, body).unwrap();
        }
        store.record_verified(&manifest).unwrap();
        drop(store);
        let connection = Connection::open(snapshot.root.join(DATABASE)).unwrap();
        connection
            .execute("UPDATE runs SET checks_json='{}',counts_json='{}'", [])
            .unwrap();
        connection.close().unwrap();
        let original = fs::read(snapshot.root.join(DATABASE)).unwrap();
        let prepared = intent("verified", &manifest, &derived);
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        assert!(
            Store::resume_with_intent(
                &snapshot.root,
                &context,
                &case,
                Some(&snapshot),
                &prepared,
                &derived
            )
            .is_err()
        );
        assert_eq!(fs::read(snapshot.root.join(DATABASE)).unwrap(), original);

        let sandbox = Sandbox::new();
        let (store, manifest) = staged_store(&sandbox);
        let context = store.context.clone();
        let snapshot = store.snapshot().unwrap();
        let derived: BTreeMap<_, _> = manifest
            .iter()
            .map(|file| {
                (
                    format!("vault/{}", file.relative_path),
                    store
                        .read_owned(&format!("staging/vault/{}", file.relative_path))
                        .unwrap(),
                )
            })
            .collect();
        let old_path = snapshot.root.join(snapshot.files.keys().next().unwrap());
        let old_bytes = fs::read(&old_path).unwrap();
        let original = fs::read(snapshot.root.join(DATABASE)).unwrap();
        drop(store);
        let retained = sandbox.0.join("retained-original.md");
        fs::rename(&old_path, &retained).unwrap();
        fs::write(&old_path, &old_bytes).unwrap();
        let prepared = intent("verified", &manifest, &derived);
        assert!(
            Store::resume_with_intent(
                &snapshot.root,
                &context,
                &case,
                Some(&snapshot),
                &prepared,
                &derived
            )
            .is_err()
        );
        assert_eq!(fs::read(&old_path).unwrap(), old_bytes);
        assert_eq!(fs::read(&retained).unwrap(), old_bytes);
        assert_eq!(fs::read(snapshot.root.join(DATABASE)).unwrap(), original);
    }
    #[test]
    fn snapshot_resume_refuses_a_real_hot_journal_without_recovery_writes() {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};

        let sandbox = Sandbox::new();
        let store = new_store(&sandbox);
        let context = store.context.clone();
        let snapshot = store.snapshot().unwrap();
        let db = snapshot.root.join(DATABASE);
        let before = fs::read(&db).unwrap();
        drop(store);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "compile_store::tests::hot_journal_child",
                "--nocapture",
            ])
            .env("NOETOME_TEST_STORE_HOT_JOURNAL_ROOT", &snapshot.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut reached = false;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap() == 0 {
                break;
            }
            if line.trim() == "NOETOME_STORE_HOT_JOURNAL_READY" {
                reached = true;
                break;
            }
        }
        if !reached {
            let output = child.wait_with_output().unwrap();
            panic!(
                "child did not reach a live SQLite write: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert!(child.try_wait().unwrap().is_none());
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        let journal = snapshot.root.join("state/compile.sqlite-journal");
        let journal_bytes = fs::read(&journal).unwrap();
        let spilled = fs::read(&db).unwrap();
        assert_ne!(spilled, before);
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        assert!(Store::resume(&snapshot.root, &context, &case, &snapshot, false).is_err());
        assert_eq!(fs::read(&db).unwrap(), spilled);
        assert_eq!(fs::read(&journal).unwrap(), journal_bytes);

        // Even synchronizing the saved SHA to the dirty main file cannot turn
        // the real journal into an authorized ordinary inventory entry.
        let mut synchronized = snapshot.clone();
        synchronized.database = Store::file_snapshot(&db, &spilled).unwrap();
        assert_eq!(
            Store::resume(&snapshot.root, &context, &case, &synchronized, false)
                .err()
                .unwrap()
                .code(),
            "COMPILE_RECOVERY_REQUIRED"
        );
        let prepared = intent("planned", &[], &BTreeMap::new());
        assert_eq!(
            Store::resume_with_intent(
                &snapshot.root,
                &context,
                &case,
                Some(&synchronized),
                &prepared,
                &BTreeMap::new()
            )
            .err()
            .unwrap()
            .code(),
            "COMPILE_RECOVERY_REQUIRED"
        );
        assert_eq!(fs::read(&db).unwrap(), spilled);
        assert_eq!(fs::read(&journal).unwrap(), journal_bytes);
    }
    #[test]
    fn existing_and_input_child_roots_are_rejected_without_writes() {
        let sandbox = Sandbox::new();
        let sentinel = sandbox.0.join("sentinel");
        fs::write(&sentinel, b"preserve").unwrap();
        assert_eq!(
            Store::preflight(&fixture_root(), &sandbox.0)
                .unwrap_err()
                .code(),
            "COMPILE_OUTPUT_EXISTS"
        );
        assert_eq!(
            Store::preflight(&fixture_root(), &fixture_root().join("uncreated-output"))
                .unwrap_err()
                .code(),
            "COMPILE_INPUT_OUTPUT_OVERLAP"
        );
        assert_eq!(fs::read(sentinel).unwrap(), b"preserve");
        assert!(!fixture_root().join("uncreated-output").exists());
    }
    #[test]
    fn same_root_and_existing_input_ancestors_use_the_overlap_error() {
        let input = fixture_root();
        for output in [&input, input.parent().unwrap()] {
            assert_eq!(
                Store::preflight(&input, output).unwrap_err().code(),
                "COMPILE_INPUT_OUTPUT_OVERLAP"
            );
        }
    }
    #[test]
    fn raw_parent_traversal_is_rejected_before_filesystem_access() {
        let sandbox = Sandbox::new();
        fs::create_dir(sandbox.0.join("ordinary")).unwrap();
        // Appending OS text preserves ParentDir. PathBuf::join on Windows
        // verbatim canonical roots would normalize it before preflight sees it.
        let mut raw = sandbox.0.as_os_str().to_os_string();
        raw.push(format!(
            "{0}ordinary{0}..{0}escaped-target",
            std::path::MAIN_SEPARATOR
        ));
        let output = PathBuf::from(raw);
        assert!(
            output
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        );
        assert_eq!(
            Store::preflight(&fixture_root(), &output)
                .unwrap_err()
                .code(),
            "PROJECT_UNSAFE_PATH"
        );
        assert!(!sandbox.0.join("escaped-target").exists());
    }
    #[test]
    fn incorrect_or_missing_base_counts_fail_before_creating_output() {
        let case = crate::fixture_input::load(&fixture_root()).unwrap();
        for key in ["sources", "atoms", "required_atoms", "notes", "nodes"] {
            for remove in [false, true] {
                let sandbox = Sandbox::new();
                let mut context = context(&case);
                if remove {
                    context.counts.remove(key);
                } else {
                    *context.counts.get_mut(key).unwrap() += 1;
                }
                let output = sandbox.0.join("output");
                assert!(
                    Store::create(&output, &context, &case).is_err(),
                    "{key} remove={remove}"
                );
                assert!(!output.exists());
            }
        }
    }
    #[test]
    fn exclusive_file_write_preserves_existing_bytes() {
        let sandbox = Sandbox::new();
        let mut store = new_store(&sandbox);
        store.write_owned("reports/字节.txt", b"first").unwrap();
        assert_eq!(
            store
                .write_owned("reports/字节.txt", b"second")
                .unwrap_err()
                .code(),
            "COMPILE_OUTPUT_EXISTS"
        );
        assert_eq!(
            store
                .write_owned("reports/字节.TXT", b"second")
                .unwrap_err()
                .code(),
            "COMPILE_OUTPUT_EXISTS"
        );
        assert_eq!(store.read_owned("reports/字节.txt").unwrap(), b"first");
        for path in [
            "reports/../escape",
            "state/compile.sqlite",
            "reports/NUL.txt",
            "reports//x",
        ] {
            assert!(store.write_owned(path, b"no").is_err());
        }
        assert_eq!(publication_count(&store), 0);
    }
    #[test]
    fn unknown_file_is_preserved_and_blocks_publication() {
        let sandbox = Sandbox::new();
        let (mut store, manifest) = verified_store(&sandbox);
        let facts = report(&store, &manifest);
        let paths = write_report(&mut store, &facts);
        let unknown = store.root().join("vault/unknown.md");
        fs::write(&unknown, b"external note").unwrap();
        assert!(store.commit_publication(&facts, &manifest, &paths).is_err());
        assert_eq!(fs::read(unknown).unwrap(), b"external note");
        assert_eq!(publication_count(&store), 0);
    }
    #[test]
    fn same_bytes_with_a_new_file_identity_are_rejected() {
        let sandbox = Sandbox::new();
        let mut store = new_store(&sandbox);
        store
            .write_owned("reports/original.txt", b"same bytes")
            .unwrap();
        let original = store.root().join("reports/original.txt");
        fs::rename(&original, store.root().join("reports/old.txt")).unwrap();
        fs::write(&original, b"same bytes").unwrap();
        fs::remove_file(store.root().join("reports/old.txt")).unwrap();
        assert!(store.read_owned("reports/original.txt").is_err());
        assert_eq!(fs::read(original).unwrap(), b"same bytes");
    }
    #[test]
    fn same_empty_directory_name_with_new_identity_is_rejected() {
        let sandbox = Sandbox::new();
        let store = new_store(&sandbox);
        let vault = store.root().join("vault");
        fs::rename(&vault, store.root().join("old-vault")).unwrap();
        fs::create_dir(&vault).unwrap();
        fs::remove_dir(store.root().join("old-vault")).unwrap();
        assert!(store.ensure_integrity().is_err());
        assert!(vault.is_dir());
    }
    #[test]
    fn publication_atomically_binds_both_report_bytes_and_manifest() {
        let sandbox = Sandbox::new();
        let (mut store, manifest) = verified_store(&sandbox);
        let facts = report(&store, &manifest);
        let paths = write_report(&mut store, &facts);
        assert_eq!(publication_count(&store), 0);
        store.commit_publication(&facts, &manifest, &paths).unwrap();
        let connection = Connection::open_with_flags(
            store.root().join(DATABASE),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let committed:(String,String,String)=connection.query_row("SELECT report_json_sha256,report_md_sha256,manifest_sha256 FROM publication_commits",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(
            committed.0,
            crate::text::sha256(&fs::read(store.root().join(&paths.json)).unwrap())
        );
        assert_eq!(
            committed.1,
            crate::text::sha256(&fs::read(store.root().join(&paths.markdown)).unwrap())
        );
        assert_eq!(
            committed.2,
            crate::text::sha256(&serde_json::to_vec(&manifest).unwrap())
        );
        assert_eq!(
            connection
                .query_row("SELECT state || '/' || stage FROM runs", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "succeeded/finished"
        );
        assert_eq!(
            connection
                .query_row("SELECT status FROM releases", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "published"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM coverage_records WHERE stage='validated'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            12
        );
        assert!(store.write_owned("reports/after.txt", b"no").is_err());
        store.ensure_integrity().unwrap();
    }
    #[test]
    fn rendering_failure_records_failed_run_and_staged_release() {
        let sandbox = Sandbox::new();
        let mut store = new_store(&sandbox);
        let mut facts = report(&store, &[]);
        facts.result = "failed".into();
        facts.issues.push(crate::fixture_model::Issue::new(
            "RENDER_TEXT_SYNTAX",
            Vec::new(),
            "storage failure test",
        ));
        facts.counts.insert("issues".into(), 1);
        facts.checks.get_mut("rendering").unwrap().status = "failed".into();
        facts.checks.get_mut("rendering").unwrap().issue_count = 1;
        let paths = write_report(&mut store, &facts);
        store.record_failure(&facts, "staged", &paths).unwrap();
        assert_eq!(
            store
                .connection
                .query_row("SELECT state || '/' || stage FROM runs", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "failed/failed"
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT status FROM releases", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "staged"
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT report_json_path FROM runs", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            paths.json
        );
        assert_eq!(publication_count(&store), 0);
        store.ensure_integrity().unwrap();
    }
    #[test]
    fn recovery_refuses_to_write_when_an_owned_directory_is_replaced() {
        let sandbox = Sandbox::new();
        let mut store = new_store(&sandbox);
        let facts = report(&store, &[]);
        let paths = ReportPaths {
            json: "reports/failure.json".into(),
            markdown: "reports/failure.md".into(),
        };
        let reports = store.root().join("reports");
        fs::rename(&reports, store.root().join("original-reports")).unwrap();
        fs::create_dir(&reports).unwrap();
        assert!(
            store
                .recover(
                    &facts,
                    "recovery_required",
                    &paths,
                    &integrity("test identity replacement")
                )
                .is_err()
        );
        assert!(!store.root().join("state/recovery.json").exists());
        assert!(store.root().join("state/compile.sqlite").exists());
    }
    #[test]
    fn missing_failed_or_not_run_required_checks_block_publication() {
        for status in ["missing", "failed", "not_run"] {
            let sandbox = Sandbox::new();
            let (mut store, manifest) = verified_store(&sandbox);
            let mut facts = report(&store, &manifest);
            if status == "missing" {
                facts.checks.remove("vault_output");
            } else {
                facts.checks.get_mut("vault_output").unwrap().status = status.into();
            }
            let paths = write_report(&mut store, &facts);
            assert!(
                store.commit_publication(&facts, &manifest, &paths).is_err(),
                "{status}"
            );
            assert_eq!(publication_count(&store), 0);
        }
    }
    #[test]
    fn sqlite_reader_makes_final_commit_busy_and_rolls_back_publication() {
        let sandbox = Sandbox::new();
        let (mut store, manifest) = verified_store(&sandbox);
        let facts = report(&store, &manifest);
        let paths = write_report(&mut store, &facts);
        let reader = Connection::open_with_flags(
            store.root().join(DATABASE),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        reader.execute_batch("BEGIN").unwrap();
        let count: i64 = reader
            .query_row("SELECT count(*) FROM releases", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let failure = store
            .commit_publication(&facts, &manifest, &paths)
            .unwrap_err();
        assert_eq!(failure.code(), "COMPILE_DATABASE_ERROR");
        assert!(store.connection.is_autocommit());
        assert_eq!(publication_count(&store), 0);
        assert_eq!(
            store
                .connection
                .query_row("SELECT status FROM releases", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "verified"
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT state FROM runs", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "running"
        );
        store
            .recover(&facts, "recovery_required", &paths, &failure)
            .unwrap();
        let recovery: serde_json::Value =
            serde_json::from_slice(&fs::read(store.root().join("state/recovery.json")).unwrap())
                .unwrap();
        assert_eq!(recovery["run_id"], store.context.run_id);
        assert_eq!(recovery["recovery_required"], true);
        reader.execute_batch("ROLLBACK").unwrap();
        store.ensure_integrity().unwrap();
    }
    #[test]
    fn active_sqlite_journal_is_rejected_without_recovery_reads() {
        let sandbox = Sandbox::new();
        let store = new_store(&sandbox);
        let db = store.root().join(DATABASE);
        let before = fs::read(&db).unwrap();
        let writer = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
        writer
            .execute_batch("BEGIN IMMEDIATE; UPDATE runs SET stage='outside-transaction';")
            .unwrap();
        assert!(store.root().join("state/compile.sqlite-journal").exists());
        assert_eq!(
            store.ensure_integrity().unwrap_err().code(),
            "COMPILE_RECOVERY_REQUIRED"
        );
        assert_eq!(fs::read(&db).unwrap(), before);
        writer.execute_batch("ROLLBACK").unwrap();
        store.ensure_integrity().unwrap();
    }
    #[test]
    fn hot_journal_child() {
        use std::io::{BufRead, Write};
        let Some(selected_root) = std::env::var_os("NOETOME_TEST_STORE_HOT_JOURNAL_ROOT") else {
            return;
        };
        let root = PathBuf::from(selected_root);
        let allowed = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/compile-store-tests")
            .canonicalize()
            .unwrap();
        assert!(root.starts_with(&allowed) && root != allowed);
        let writer =
            Connection::open_with_flags(root.join(DATABASE), OpenFlags::SQLITE_OPEN_READ_WRITE)
                .unwrap();
        writer
            .execute_batch("PRAGMA journal_mode=DELETE; PRAGMA cache_size=1; BEGIN IMMEDIATE")
            .unwrap();
        // Overflow pages force main-file spill before COMMIT. Killing this child
        // leaves a genuine hot rollback journal rather than a fake sidecar.
        writer
            .execute(
                "UPDATE runs SET counts_json=?1",
                [format!(
                    "{{\"test_spill\":\"{}\"}}",
                    "a".repeat(2 * 1024 * 1024)
                )],
            )
            .unwrap();
        println!("NOETOME_STORE_HOT_JOURNAL_READY");
        std::io::stdout().flush().unwrap();
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).unwrap();
        writer.execute_batch("ROLLBACK").unwrap();
    }
    #[test]
    fn killed_sqlite_writer_leaves_a_hot_journal_that_read_checks_preserve() {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        let sandbox = Sandbox::new();
        let store = new_store(&sandbox);
        let db = store.root().join(DATABASE);
        let before = fs::read(&db).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "compile_store::tests::hot_journal_child",
                "--nocapture",
            ])
            .env("NOETOME_TEST_STORE_HOT_JOURNAL_ROOT", store.root())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut reached = false;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap() == 0 {
                break;
            }
            if line.trim() == "NOETOME_STORE_HOT_JOURNAL_READY" {
                reached = true;
                break;
            }
        }
        if !reached {
            let output = child.wait_with_output().unwrap();
            panic!(
                "child did not reach a live SQLite write: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert!(child.try_wait().unwrap().is_none());
        child.kill().unwrap();
        let killed = child.wait().unwrap();
        assert!(!killed.success());
        let journal = store.root().join("state/compile.sqlite-journal");
        let journal_before = fs::read(&journal).unwrap();
        let spilled = fs::read(&db).unwrap();
        assert_ne!(spilled, before);
        assert_eq!(
            store.ensure_integrity().unwrap_err().code(),
            "COMPILE_RECOVERY_REQUIRED"
        );
        assert_eq!(fs::read(&db).unwrap(), spilled);
        assert_eq!(fs::read(&journal).unwrap(), journal_before);
        // Do not open a recovering connection. Ownership Drop releases handles;
        // Sandbox cleanup removes only this new, verified test directory.
    }
    #[test]
    fn mismatched_markdown_cannot_be_published_as_the_same_snapshot() {
        let sandbox = Sandbox::new();
        let (mut store, manifest) = verified_store(&sandbox);
        let facts = report(&store, &manifest);
        let paths = ReportPaths {
            json: "reports/validation.json".into(),
            markdown: "reports/validation.md".into(),
        };
        store
            .write_owned(&paths.json, &serde_json::to_vec(&facts).unwrap())
            .unwrap();
        store
            .write_owned(
                &paths.markdown,
                b"# unrelated successful report\nresult: published\n",
            )
            .unwrap();
        assert!(store.commit_publication(&facts, &manifest, &paths).is_err());
        assert_eq!(publication_count(&store), 0);
    }
    #[cfg(windows)]
    #[test]
    fn windows_active_file_writer_blocks_read_and_keeps_bytes() {
        use std::os::windows::fs::OpenOptionsExt;
        let sandbox = Sandbox::new();
        let mut store = new_store(&sandbox);
        store
            .write_owned("reports/locked.txt", b"original bytes")
            .unwrap();
        let path = store.root().join("reports/locked.txt");
        let lock = OpenOptions::new()
            .write(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        assert!(store.read_owned("reports/locked.txt").is_err());
        drop(lock);
        assert_eq!(fs::read(&path).unwrap(), b"original bytes");
        store.ensure_integrity().unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn windows_protected_root_cannot_be_moved_during_a_run() {
        let sandbox = Sandbox::new();
        let store = new_store(&sandbox);
        let original = store.root().to_path_buf();
        let moved = sandbox.0.join("moved");
        assert!(fs::rename(&original, &moved).is_err());
        assert!(original.exists());
        assert!(!moved.exists());
        store.ensure_integrity().unwrap();
    }
}
