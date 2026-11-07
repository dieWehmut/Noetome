//! Revision store: gap problems, verified IR versions, and dependent-representation recompiles.
use crate::{
    compile_identity::identity,
    project::{self, ProjectError},
    revision_model::*,
    text::sha256,
    vault_store::{
        canonical, now, read_bounded, resolve_caller_path, write_new_file, write_replace,
    },
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const APP: i64 = 0x4e525652;
pub(crate) const CONTROL: &str = "control";
pub(crate) const OWNER: &str = "owner.json";
pub(crate) const DATABASE: &str = "revision.sqlite";
pub(crate) const LOCK: &str = "run.lock";
pub(crate) const RENDER: &str = "render";
pub(crate) const MAX_ENTRIES: usize = 64;

pub(crate) const SCHEMA: &str = "CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL,metadata_json TEXT NOT NULL);CREATE TABLE gaps(gap_id TEXT PRIMARY KEY,position INTEGER NOT NULL CHECK(position>=0),status TEXT NOT NULL,payload_json TEXT NOT NULL);CREATE TABLE revisions(revision_id TEXT PRIMARY KEY,sequence INTEGER NOT NULL CHECK(sequence>0),gap_id TEXT NOT NULL,payload_json TEXT NOT NULL);CREATE TABLE representations(representation_id TEXT PRIMARY KEY,position INTEGER NOT NULL CHECK(position>=0),atom_ids_json TEXT NOT NULL,render_sha256 TEXT NOT NULL,render TEXT NOT NULL);CREATE TABLE ir_versions(version INTEGER PRIMARY KEY CHECK(version>=1),atoms_sha256 TEXT NOT NULL,payload_json TEXT NOT NULL,at TEXT NOT NULL);";

fn error(text: impl Into<String>) -> ProjectError {
    ProjectError::new("REVISION_ERROR", 3, text)
}

/// 账本层的数据库故障沿用项目级分类：损坏的账本必须与别的能力报同一个错误码
/// （`PROJECT_INVALID_DATABASE`），占用与 IO 才有各自的专用码。
fn db_error(error: rusqlite::Error) -> ProjectError {
    ProjectError::database(error)
}

fn encode<T: Serialize>(value: &T) -> Result<String, ProjectError> {
    serde_json::to_string(value)
        .map_err(|error| ProjectError::new("REVISION_SCHEMA", 3, format!("序列化失败：{error}。")))
}

fn decode<T: DeserializeOwned>(text: &str) -> Result<T, ProjectError> {
    serde_json::from_str(text).map_err(|error| {
        ProjectError::new("REVISION_SCHEMA", 3, format!("反序列化失败：{error}。"))
    })
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

pub(crate) fn schema_map() -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for statement in SCHEMA.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        let rest = trimmed
            .strip_prefix("CREATE TABLE ")
            .expect("schema statements start with CREATE TABLE");
        let name = rest.split('(').next().expect("table name").trim();
        map.insert(name.to_owned(), compact(trimmed));
    }
    map
}

fn strict_record<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, ProjectError> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| integrity("修订账本记录必须是UTF-8文本。"))?;
    let value: T = serde_json::from_str(text)
        .map_err(|error| integrity(format!("修订账本记录不是严格JSON：{error}。")))?;
    if canonical(&value)?.as_str() != text {
        return Err(integrity("修订账本记录非规范编码、重复键或未知字段。"));
    }
    Ok(value)
}

fn require_dir(path: &Path) -> Result<(), ProjectError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        ProjectError::new(
            "REVISION_ERROR",
            3,
            format!("无法读取 {}：{error}。", path.display()),
        )
    })?;
    if project::is_link(&metadata) || !metadata.is_dir() {
        return Err(error(format!("{} 必须是普通目录。", path.display())));
    }
    Ok(())
}

fn names(path: &Path) -> Result<BTreeSet<String>, ProjectError> {
    let mut set = BTreeSet::new();
    let entries = fs::read_dir(path).map_err(|error| {
        ProjectError::new(
            "REVISION_ERROR",
            3,
            format!("无法列出 {}：{error}。", path.display()),
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            ProjectError::new(
                "REVISION_ERROR",
                3,
                format!("无法列出 {}：{error}。", path.display()),
            )
        })?;
        set.insert(
            entry
                .file_name()
                .into_string()
                .map_err(|_| integrity("修订根里出现非 UTF-8 文件名。"))?,
        );
    }
    Ok(set)
}

fn name_set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn check_identity(
    path: &Path,
    expected: &crate::compile_identity::Identity,
    label: &str,
) -> Result<(), ProjectError> {
    let actual = identity(path)?;
    if &actual != expected {
        return Err(integrity(format!(
            "{label} 的文件身份已改变：{}。",
            path.display()
        )));
    }
    Ok(())
}

fn open_new(path: &Path) -> Result<fs::File, ProjectError> {
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x00200000);
    }
    options.open(path).map_err(|error| {
        ProjectError::new(
            "REVISION_ERROR",
            3,
            format!("无法创建 {}：{error}。", path.display()),
        )
    })
}

fn verify_schema(connection: &Connection) -> Result<(), ProjectError> {
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(db_error)?;
    if version != i64::from(SCHEMA_VERSION) {
        return Err(integrity(format!(
            "修订账本 user_version 应为 {SCHEMA_VERSION}，实际 {version}。"
        )));
    }
    let application: i64 = connection
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .map_err(db_error)?;
    if application != APP {
        return Err(integrity(format!(
            "修订账本 application_id 应为 {APP}，实际 {application}。"
        )));
    }
    let synchronous: i64 = connection
        .query_row("PRAGMA synchronous", [], |row| row.get(0))
        .map_err(db_error)?;
    if synchronous != 2 {
        return Err(integrity("修订账本 synchronous 必须为 FULL。"));
    }
    let mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .map_err(db_error)?;
    if mode.to_ascii_lowercase() != "delete" {
        return Err(integrity(format!(
            "修订账本 journal_mode 必须是 delete，实际 {mode}。"
        )));
    }
    let mut actual = BTreeMap::new();
    {
        let mut statement = connection
            .prepare("SELECT name, sql FROM sqlite_master WHERE type = 'table'")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })
            .map_err(db_error)?;
        for row in rows {
            let (name, sql) = row.map_err(db_error)?;
            if name.starts_with("sqlite_") {
                continue;
            }
            actual.insert(name, compact(sql.as_deref().unwrap_or_default()));
        }
    }
    let expected = schema_map();
    if actual != expected {
        return Err(integrity("修订账本的表定义与冻结定义不一致。"));
    }
    Ok(())
}

fn read_metadata_json(connection: &Connection) -> Result<String, ProjectError> {
    let mut statement = connection
        .prepare("SELECT singleton, schema_version, metadata_json FROM metadata")
        .map_err(db_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(db_error)?;
    let mut collected = Vec::new();
    for row in rows {
        collected.push(row.map_err(db_error)?);
    }
    if collected.len() != 1 {
        return Err(integrity(format!(
            "修订账本 metadata 必须恰好一行，实际 {} 行。",
            collected.len()
        )));
    }
    let (singleton, version, text) = collected.into_iter().next().expect("one row");
    if singleton != 1 || version != i64::from(SCHEMA_VERSION) {
        return Err(integrity(
            "修订账本 metadata 的 singleton 或 schema_version 不符。",
        ));
    }
    Ok(text)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Owner {
    pub(crate) policy: String,
    pub(crate) producer: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) root: String,
    pub(crate) root_identity: crate::compile_identity::Identity,
    pub(crate) parent_identity: crate::compile_identity::Identity,
    pub(crate) control_identity: crate::compile_identity::Identity,
    pub(crate) owner_identity: crate::compile_identity::Identity,
    pub(crate) database_identity: crate::compile_identity::Identity,
    pub(crate) lock_identity: crate::compile_identity::Identity,
    pub(crate) author_manifest_sha256: String,
    pub(crate) corpus_manifest_sha256: String,
    pub(crate) created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Metadata {
    pub(crate) policy: String,
    pub(crate) producer: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) author: Author,
    pub(crate) corpus: CorpusManifest,
    pub(crate) author_manifest_sha256: String,
    pub(crate) corpus_manifest_sha256: String,
    pub(crate) created_at: String,
}

pub(crate) struct RevisionStore {
    pub(crate) root: PathBuf,
    pub(crate) owner: Owner,
    pub(crate) metadata: Metadata,
    pub(crate) connection: Connection,
    _lock: Option<fs::File>,
}

impl RevisionStore {
    pub(crate) fn open(root: &Path) -> Result<Self, ProjectError> {
        Self::open_mode(root, true)
    }

    pub(crate) fn open_readonly(root: &Path) -> Result<Self, ProjectError> {
        Self::open_mode(root, false)
    }

    fn open_mode(root: &Path, writable: bool) -> Result<Self, ProjectError> {
        let root = resolve_caller_path(root)?;
        if !project::exists(&root)? {
            return Err(unknown_root(format!(
                "{} 不是修订根：目录不存在。",
                root.display()
            )));
        }
        require_dir(&root)?;
        if names(&root)? != name_set(&[CONTROL, RENDER]) {
            return Err(error(format!(
                "{} 不是修订根：顶层只允许 {CONTROL}/ 与 {RENDER}/。",
                root.display()
            )));
        }
        let control = root.join(CONTROL);
        require_dir(&control)?;
        if names(&control)? != name_set(&[OWNER, DATABASE, LOCK]) {
            return Err(error(format!(
                "{} 不是修订根：{CONTROL}/ 只允许 {OWNER}、{DATABASE}、{LOCK}。",
                control.display()
            )));
        }
        let owner_path = control.join(OWNER);
        let owner: Owner = strict_record(&read_bounded(&owner_path, MAX_BYTES)?)?;
        if owner.policy != POLICY
            || owner.producer != PRODUCER
            || owner.schema_version != SCHEMA_VERSION
        {
            return Err(error(format!(
                "{} 不是修订根：policy/producer/schema_version 不符。",
                root.display()
            )));
        }
        let canonical_root = fs::canonicalize(&root).map_err(|error| {
            ProjectError::new(
                "REVISION_ERROR",
                3,
                format!("无法解析 {}：{error}。", root.display()),
            )
        })?;
        if owner.root != canonical_root.to_string_lossy() {
            return Err(error(format!(
                "{} 不是修订根：记录里的根是 {}。",
                root.display(),
                owner.root
            )));
        }
        check_identity(&root, &owner.root_identity, "修订根")?;
        check_identity(&control, &owner.control_identity, "control 目录")?;
        check_identity(&owner_path, &owner.owner_identity, "owner.json")?;
        check_identity(
            &control.join(DATABASE),
            &owner.database_identity,
            "修订账本",
        )?;
        check_identity(&control.join(LOCK), &owner.lock_identity, "run.lock")?;
        let flags = if writable {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        } else {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        };
        let connection = Connection::open_with_flags(
            control.join(DATABASE),
            flags | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| {
            ProjectError::new("REVISION_ERROR", 3, format!("无法打开修订账本：{error}。"))
        })?;
        verify_schema(&connection)?;
        let metadata: Metadata = strict_record(&read_metadata_json(&connection)?.as_bytes())?;
        if metadata.policy != POLICY
            || metadata.producer != PRODUCER
            || metadata.schema_version != SCHEMA_VERSION
            || metadata.case_id != owner.case_id
            || metadata.created_at != owner.created_at
            || metadata.author_manifest_sha256 != owner.author_manifest_sha256
            || metadata.corpus_manifest_sha256 != owner.corpus_manifest_sha256
        {
            return Err(integrity("修订账本 metadata 与 owner.json 不一致。"));
        }
        Ok(RevisionStore {
            root,
            owner,
            metadata,
            connection,
            _lock: None,
        })
    }

    pub(crate) fn lock(&mut self) -> Result<(), ProjectError> {
        if self._lock.is_some() {
            return Ok(());
        }
        let path = self.root.join(CONTROL).join(LOCK);
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3).custom_flags(0x00200000);
        }
        let file = options.open(&path).map_err(|error| {
            ProjectError::new("REVISION_ERROR", 3, format!("无法打开锁文件：{error}。"))
        })?;
        if file
            .metadata()
            .map_err(|error| {
                ProjectError::new("REVISION_ERROR", 3, format!("无法读取锁文件：{error}。"))
            })?
            .len()
            != 0
        {
            return Err(integrity(
                "run.lock 必须是空的普通文件；保留现场并拒绝继续。",
            ));
        }
        let start = std::time::Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => {
                    self._lock = Some(file);
                    return Ok(());
                }
                Err(std::fs::TryLockError::WouldBlock)
                    if start.elapsed() < std::time::Duration::from_secs(5) =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                Err(failure) => {
                    return Err(busy(format!(
                        "写入锁不可取得（{failure}）；未写入任何字节，请关闭另一个修订进程后重试。"
                    )));
                }
            }
        }
    }

    fn gaps(&self) -> Result<Vec<GapRecord>, ProjectError> {
        let mut statement = self
            .connection
            .prepare("SELECT gap_id, position, status, payload_json FROM gaps ORDER BY position")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(db_error)?;
        let mut gaps = Vec::new();
        for (index, row) in rows.enumerate() {
            let (gap_id, position, status, payload) = row.map_err(db_error)?;
            if position != index as i64 {
                return Err(integrity(format!("缺口位置不连续：{gap_id}。")));
            }
            let record: GapRecord = decode(&payload)?;
            if record.gap_id != gap_id || record.status != status {
                return Err(integrity(format!("缺口 {gap_id} 的列与记录不一致。")));
            }
            gaps.push(record);
        }
        Ok(gaps)
    }

    fn revisions(&self) -> Result<Vec<RevisionRecord>, ProjectError> {
        let mut statement = self
            .connection
            .prepare("SELECT revision_id, sequence, gap_id, payload_json FROM revisions ORDER BY sequence")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(db_error)?;
        let mut revisions = Vec::new();
        for (index, row) in rows.enumerate() {
            let (revision_id, sequence, gap_id, payload) = row.map_err(db_error)?;
            if sequence != (index + 1) as i64 {
                return Err(integrity(format!("修订序号不连续：{revision_id}。")));
            }
            let record: RevisionRecord = decode(&payload)?;
            if record.revision_id != revision_id || record.gap_id != gap_id {
                return Err(integrity(format!("修订 {revision_id} 的列与记录不一致。")));
            }
            revisions.push(record);
        }
        Ok(revisions)
    }

    fn representations(&self) -> Result<Vec<Representation>, ProjectError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT representation_id, position, atom_ids_json FROM representations ORDER BY position",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(db_error)?;
        let mut result = Vec::new();
        for (index, row) in rows.enumerate() {
            let (representation_id, position, atom_ids_json) = row.map_err(db_error)?;
            if position != index as i64 {
                return Err(integrity(format!("表示位置不连续：{representation_id}。")));
            }
            let atom_ids: Vec<String> = decode(&atom_ids_json)?;
            let author_representation = self
                .metadata
                .author
                .representations
                .iter()
                .find(|item| item.representation_id == representation_id)
                .ok_or_else(|| {
                    integrity(format!("账本里的表示 {representation_id} 不在作者清单里。"))
                })?;
            if author_representation.atom_ids != atom_ids {
                return Err(integrity(format!(
                    "表示 {representation_id} 的原子列表与作者清单不一致。"
                )));
            }
            result.push(author_representation.clone());
        }
        Ok(result)
    }

    fn renders(&self) -> Result<BTreeMap<String, (String, String)>, ProjectError> {
        let mut statement = self
            .connection
            .prepare("SELECT representation_id, render_sha256, render FROM representations")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(db_error)?;
        let mut map = BTreeMap::new();
        for row in rows {
            let (id, digest, render) = row.map_err(db_error)?;
            if sha256(render.as_bytes()) != digest {
                return Err(integrity(format!("表示 {id} 的渲染字节与记录的摘要不符。")));
            }
            map.insert(id, (digest, render));
        }
        Ok(map)
    }

    fn latest_version(&self) -> Result<IrVersion, ProjectError> {
        let mut statement = self
            .connection
            .prepare("SELECT version, atoms_sha256, payload_json, at FROM ir_versions ORDER BY version DESC LIMIT 1")
            .map_err(db_error)?;
        let row = statement
            .query_row([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .optional()
            .map_err(db_error)?;
        let (version, digest, payload, at) =
            row.ok_or_else(|| integrity("修订账本没有任何 IR 版本。"))?;
        let value: IrVersion = decode(&payload)?;
        if value.version as i64 != version || value.atoms_sha256 != digest || value.at != at {
            return Err(integrity("最新 IR 版本的列与记录不一致。"));
        }
        Ok(value)
    }
}

fn verify_corpus(package: &Path, manifest: &CorpusManifest) -> Result<(), ProjectError> {
    for document in &manifest.documents {
        let path = package.join(&document.path);
        let bytes = read_bounded(&path, MAX_BYTES)
            .map_err(|_| corpus_mismatch(format!("冻结语料缺少 {}。", document.path)))?;
        if bytes.len() as u64 != document.byte_length || sha256(&bytes) != document.sha256 {
            return Err(corpus_mismatch(format!(
                "冻结语料 {} 的字节与清单不符。",
                document.path
            )));
        }
    }
    Ok(())
}

fn read_file(path: &Path, label: &str) -> Result<Vec<u8>, ProjectError> {
    let resolved = resolve_caller_path(path)?;
    project::checked_file(&resolved)?;
    read_bounded(&resolved, MAX_BYTES).map_err(|error| {
        argument(format!(
            "无法读取{label} {}：{}。",
            resolved.display(),
            error
        ))
    })
}

pub fn adopt(author_path: &Path, root: &Path) -> Result<String, ProjectError> {
    let resolved_author = resolve_caller_path(author_path)?;
    project::checked_file(&resolved_author)?;
    let author_bytes = read_bounded(&resolved_author, MAX_BYTES)?;
    let author = Author::parse(&author_bytes)?;
    let package = resolved_author
        .parent()
        .ok_or_else(|| argument("作者清单必须位于作者包目录内。"))?
        .to_path_buf();
    let corpus_root = package.join(&author.corpus.root);
    require_dir(&corpus_root)?;
    let manifest_path = package.join(&author.corpus.manifest);
    let manifest_bytes = read_bounded(&manifest_path, MAX_BYTES)?;
    let manifest = CorpusManifest::parse(&manifest_bytes)?;
    if manifest.case_id != author.case_id {
        return Err(argument("作者清单与语料清单的 case_id 不一致。"));
    }
    verify_corpus(&corpus_root, &manifest)?;
    for atom in &author.atoms {
        for evidence in &atom.evidence {
            if !manifest.paths().contains(&evidence.source) {
                return Err(argument(format!(
                    "原子 {} 的证据来源 {} 不在冻结语料清单内。",
                    atom.atom_id, evidence.source
                )));
            }
        }
    }
    let root = resolve_caller_path(root)?;
    if project::exists(&root)? {
        return Err(argument(format!(
            "修订根必须尚不存在：{}；未修改任何文件。",
            root.display()
        )));
    }
    let parent = root
        .parent()
        .ok_or_else(|| argument("修订根必须有父目录。"))?
        .to_path_buf();
    require_dir(&parent)?;
    let parent_identity = identity(&parent)?;
    fs::create_dir(&root).map_err(|error| {
        ProjectError::new(
            "REVISION_ERROR",
            3,
            format!("无法创建 {}：{error}。", root.display()),
        )
    })?;
    let created = identity(&root)?;
    match adopt_into(
        &root,
        &author,
        &manifest,
        &corpus_root,
        sha256(&author_bytes),
        sha256(&manifest_bytes),
        parent_identity,
        &created,
    ) {
        Ok(output) => Ok(output),
        Err(failure) => {
            let unchanged = identity(&root)
                .map(|current| current == created)
                .unwrap_or(false);
            if unchanged {
                let _ = fs::remove_dir_all(&root);
            }
            Err(failure)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn adopt_into(
    root: &Path,
    author: &Author,
    manifest: &CorpusManifest,
    corpus_root: &Path,
    author_manifest_sha256: String,
    corpus_manifest_sha256: String,
    parent_identity: crate::compile_identity::Identity,
    created: &crate::compile_identity::Identity,
) -> Result<String, ProjectError> {
    let canonical_root = fs::canonicalize(root).map_err(|error| {
        ProjectError::new(
            "REVISION_ERROR",
            3,
            format!("无法解析 {}：{error}。", root.display()),
        )
    })?;
    fs::canonicalize(corpus_root).map_err(|error| {
        ProjectError::new("REVISION_ERROR", 3, format!("无法解析语料目录：{error}。"))
    })?;
    let at = now()?;
    let control = root.join(CONTROL);
    let render_dir = root.join(RENDER);
    fs::create_dir(&control).map_err(|error| {
        ProjectError::new("REVISION_ERROR", 3, format!("无法创建 control：{error}。"))
    })?;
    fs::create_dir(&render_dir).map_err(|error| {
        ProjectError::new("REVISION_ERROR", 3, format!("无法创建 render：{error}。"))
    })?;
    let lock_path = control.join(LOCK);
    {
        let file = open_new(&lock_path)?;
        file.sync_all().map_err(|error| {
            ProjectError::new("REVISION_ERROR", 3, format!("无法同步锁文件：{error}。"))
        })?;
    }
    let lock_identity = identity(&lock_path)?;
    let database_path = control.join(DATABASE);
    {
        let file = open_new(&database_path)?;
        file.sync_all().map_err(|error| {
            ProjectError::new("REVISION_ERROR", 3, format!("无法同步账本：{error}。"))
        })?;
    }
    let database_identity = identity(&database_path)?;
    let owner_path = control.join(OWNER);
    write_new_file(&owner_path, b"")?;
    let owner_identity = identity(&owner_path)?;
    let control_identity = identity(&control)?;

    let connection = Connection::open_with_flags(
        &database_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| ProjectError::new("REVISION_ERROR", 3, format!("无法打开账本：{error}。")))?;
    connection
        .execute_batch("PRAGMA journal_mode=DELETE;PRAGMA synchronous=FULL;")
        .map_err(db_error)?;
    connection
        .execute_batch(&format!(
            "PRAGMA user_version = {SCHEMA_VERSION};PRAGMA application_id = {APP};{SCHEMA}"
        ))
        .map_err(db_error)?;

    let metadata = Metadata {
        policy: POLICY.into(),
        producer: PRODUCER.into(),
        schema_version: SCHEMA_VERSION,
        case_id: author.case_id.clone(),
        author: author.clone(),
        corpus: manifest.clone(),
        author_manifest_sha256: author_manifest_sha256.clone(),
        corpus_manifest_sha256: corpus_manifest_sha256.clone(),
        created_at: at.clone(),
    };
    connection
        .execute(
            "INSERT INTO metadata(singleton, schema_version, metadata_json) VALUES (1, ?1, ?2)",
            params![i64::from(SCHEMA_VERSION), canonical(&metadata)?],
        )
        .map_err(db_error)?;

    let atoms = atoms_map(&author.atoms);
    for (position, representation) in author.representations.iter().enumerate() {
        let text = render(representation, &atoms)?;
        write_new_file(
            &render_dir.join(format!("{}.md", representation.representation_id)),
            text.as_bytes(),
        )?;
        connection
            .execute(
                "INSERT INTO representations(representation_id, position, atom_ids_json, render_sha256, render) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    representation.representation_id,
                    position as i64,
                    encode(&representation.atom_ids)?,
                    sha256(text.as_bytes()),
                    text
                ],
            )
            .map_err(db_error)?;
    }
    let version = IrVersion {
        version: 1,
        atoms_sha256: sha256(encode(&author.atoms)?.as_bytes()),
        atoms: author.atoms.clone(),
        at: at.clone(),
    };
    connection
        .execute(
            "INSERT INTO ir_versions(version, atoms_sha256, payload_json, at) VALUES (?1, ?2, ?3, ?4)",
            params![
                i64::from(version.version),
                version.atoms_sha256,
                canonical(&version)?,
                version.at
            ],
        )
        .map_err(db_error)?;
    verify_schema(&connection)?;

    let owner = Owner {
        policy: POLICY.into(),
        producer: PRODUCER.into(),
        schema_version: SCHEMA_VERSION,
        case_id: author.case_id.clone(),
        root: canonical_root.to_string_lossy().into_owned(),
        root_identity: created.clone(),
        parent_identity,
        control_identity,
        owner_identity,
        database_identity,
        lock_identity,
        author_manifest_sha256,
        corpus_manifest_sha256,
        created_at: at,
    };
    write_replace(&owner_path, canonical(&owner)?.as_bytes())?;

    let store = RevisionStore::open_readonly(root)?;
    if store.gaps()?.len() != 0 {
        return Err(integrity("新建修订根里不应有缺口记录。"));
    }
    let version = store.latest_version()?;
    if version.version != 1 {
        return Err(integrity("新建修订根的 IR 版本应为 1。"));
    }

    let pending = version
        .atoms
        .iter()
        .filter(|atom| atom.verification() == PENDING)
        .count();
    let mut lines = Vec::new();
    lines.push(format!(
        "修订根 {}：已建立（policy={POLICY}，结构版本 {SCHEMA_VERSION}）",
        root.display()
    ));
    lines.push(format!(
        "本轮范围：{} 个原子、{} 个表示（{}）；外部检索与模型调用：0 次",
        author.atoms.len(),
        author.representations.len(),
        author
            .representations
            .iter()
            .map(|item| kind_text(&item.kind))
            .collect::<Vec<_>>()
            .join("、")
    ));
    lines.push(format!(
        "冻结语料：{} 个文档、共 {} 字节（清单 SHA256 {}）",
        manifest.documents.len(),
        manifest.total_bytes(),
        &store.owner.corpus_manifest_sha256[..16]
    ));
    lines.push(format!(
        "IR 版本：v1（{}；原子摘要 {}）；尚未有缺口与修订",
        at_label(&version.at),
        &version.atoms_sha256[..16]
    ));
    lines.push(format!(
        "初始核验：已核验 {} 个原子、待核验 {pending} 个原子",
        version.atoms.len() - pending
    ));
    lines.push(format!(
        "表示已渲染到 {RENDER}/：{}",
        author
            .representations
            .iter()
            .map(|item| format!("{}.md", item.representation_id))
            .collect::<Vec<_>>()
            .join("、")
    ));
    Ok(lines.join("\n"))
}

fn at_label(at: &str) -> String {
    at.split('.').next().unwrap_or(at).to_owned()
}

pub fn gap(root: &Path, file: &Path) -> Result<String, ProjectError> {
    let input = GapInput::parse(&read_file(file, "缺口文件")?)?;
    let mut store = RevisionStore::open(root)?;
    store.lock()?;
    let representations = store.representations()?;
    let author = &store.metadata.author;
    let representation = representations
        .iter()
        .find(|item| item.representation_id == input.representation_id)
        .ok_or_else(|| argument(format!("表示 {} 不在作者清单里。", input.representation_id)))?;
    if !representation
        .atom_ids
        .iter()
        .any(|item| item == &input.atom_id)
    {
        return Err(argument(format!(
            "缺口声明在表示 {} 发现，但该表示不引用原子 {}。",
            input.representation_id, input.atom_id
        )));
    }
    if !author.atom_ids().contains(&input.atom_id) {
        return Err(argument(format!("原子 {} 不在作者清单里。", input.atom_id)));
    }
    let mut impact = input.impact.clone();
    impact.sort();
    let declared: BTreeSet<String> = impact.iter().cloned().collect();
    let actual = dependents(&representations, &input.atom_id);
    if declared != actual {
        return Err(argument(format!(
            "缺口影响范围必须恰好等于引用该原子的表示：声明 {}，实际 {}。",
            impact.join("、"),
            actual.into_iter().collect::<Vec<_>>().join("、")
        )));
    }
    if !declared.contains(&input.representation_id) {
        return Err(argument("缺口发现位置必须包含在影响范围里。"));
    }
    let gaps = store.gaps()?;
    if gaps.iter().any(|item| item.gap_id == input.gap_id) {
        return Err(argument(format!(
            "缺口 {} 已存在；缺口编号必须唯一，未修改任何字节。",
            input.gap_id
        )));
    }
    if gaps.len() >= MAX_ENTRIES {
        return Err(argument(format!(
            "缺口数已达上限 {MAX_ENTRIES}；本轮停止扩大，保留现场。"
        )));
    }
    let at = now()?;
    let record = GapRecord::from_input(input, at.clone());
    store
        .connection
        .execute(
            "INSERT INTO gaps(gap_id, position, status, payload_json) VALUES (?1, ?2, ?3, ?4)",
            params![
                record.gap_id,
                gaps.len() as i64,
                record.status,
                canonical(&record)?
            ],
        )
        .map_err(db_error)?;
    let mut lines = Vec::new();
    lines.push(format!(
        "已登记缺口 {}：{}（发现于表示 {}）",
        record.gap_id, record.reason, representation.representation_id
    ));
    lines.push(format!(
        "来源：{}；影响范围：{}",
        record
            .source
            .iter()
            .map(EvidenceRef::text)
            .collect::<Vec<_>>()
            .join("、"),
        record.impact.join("、")
    ));
    lines.push(format!(
        "状态：{}；尚未修订。缺口问题与研究候选是不同实体：缺口必须回到修订流程，候选只在研究队列里等待授权。",
        reason_text(&record.status)
    ));
    lines.push(format!("登记时间：{}", at_label(&at)));
    Ok(lines.join("\n"))
}

pub fn revise(root: &Path, file: &Path) -> Result<String, ProjectError> {
    let input = RevisionInput::parse(&read_file(file, "修订文件")?)?;
    let mut store = RevisionStore::open(root)?;
    store.lock()?;
    let gaps = store.gaps()?;
    let index = gaps
        .iter()
        .position(|item| item.gap_id == input.gap_id)
        .ok_or_else(|| argument(format!("缺口 {} 不存在。", input.gap_id)))?;
    let mut record = gaps[index].clone();
    if record.status == GAP_ESCALATED {
        return Err(escalated(format!(
            "缺口 {} 已升级为人工判断（已修订 {} 次，上限 {} 次）；不再接受自动修订，未修改任何字节。",
            record.gap_id, record.revisions, store.metadata.author.limits.max_revisions
        )));
    }
    if record.atom_id != input.atom_id {
        return Err(argument(format!(
            "缺口 {} 涉及原子 {}，修订却指向 {}。",
            record.gap_id, record.atom_id, input.atom_id
        )));
    }
    let revisions = store.revisions()?;
    if revisions
        .iter()
        .any(|item| item.revision_id == input.revision_id)
    {
        return Err(argument(format!(
            "修订 {} 已存在；修订编号必须唯一，未修改任何字节。",
            input.revision_id
        )));
    }
    let latest = store.latest_version()?;
    let mut atoms = atoms_map(&latest.atoms);
    let atom = atoms
        .get(&input.atom_id)
        .cloned()
        .ok_or_else(|| integrity(format!("原子 {} 不在最新 IR 里。", input.atom_id)))?;

    // 无新证据不得反复推翻：如果这次修订把值改回该原子历史上出现过的旧值，
    // 而证据集合没有带来任何新来源，就拒绝。
    let mut history: BTreeSet<String> = BTreeSet::new();
    for revision in revisions.iter().filter(|item| item.gap_id == record.gap_id) {
        history.extend(evidence_keys(&revision.evidence));
    }
    let previous_values: BTreeSet<String> = revisions
        .iter()
        .filter(|item| item.atom_id == input.atom_id)
        .map(|item| item.value.clone())
        .collect();
    let new_keys = evidence_keys(&input.evidence);
    let overturn = previous_values.contains(&input.value) && input.value != atom.value;
    if overturn && !new_keys.difference(&history).next().is_some() {
        return Err(no_new_evidence(format!(
            "原子 {} 要改回先前版本已核验过的值「{}」，但这次修订没有带来任何新来源；无新证据不循环推翻，未修改任何字节。",
            input.atom_id, input.value
        )));
    }
    if new_keys.is_empty() && !overturn {
        // 无来源补充：允许，但结果标为待核验，不发布为结论。
    }
    if !new_keys.is_subset(&store.metadata.corpus.paths()) && !new_keys.is_empty() {
        for evidence in &input.evidence {
            if !store.metadata.corpus.paths().contains(&evidence.source) {
                return Err(argument(format!(
                    "修订证据来源 {} 不在冻结语料清单内。",
                    evidence.source
                )));
            }
        }
    }

    let mut revised = atom.clone();
    revised.value = input.value.clone();
    revised.unit = input.unit.clone();
    revised.evidence = input.evidence.clone();
    atoms.insert(input.atom_id.clone(), revised.clone());

    if revised.value == atom.value
        && revised.unit == atom.unit
        && evidence_keys(&revised.evidence) == evidence_keys(&atom.evidence)
    {
        return Err(argument(format!(
            "修订 {} 没有改变任何内容（值、单位、证据都与当前 IR 相同）；不生成新的 IR 版本，未修改任何字节。",
            input.revision_id
        )));
    }

    let representations = store.representations()?;
    let affected = dependents(&representations, &input.atom_id);
    if affected.is_empty() {
        return Err(integrity(format!(
            "原子 {} 没有被任何表示引用；写作反馈无法验证修订效果。",
            input.atom_id
        )));
    }
    let previous_renders = store.renders()?;
    let mut recompiled = Vec::new();
    let mut unchanged = Vec::new();
    let mut changed_text: BTreeMap<String, String> = BTreeMap::new();
    for representation in &representations {
        let text = render(representation, &atoms)?;
        let before = previous_renders
            .get(&representation.representation_id)
            .ok_or_else(|| {
                integrity(format!(
                    "表示 {} 没有渲染字节。",
                    representation.representation_id
                ))
            })?;
        if affected.contains(&representation.representation_id) {
            if text != before.1 {
                changed_text.insert(representation.representation_id.clone(), text.clone());
            }
            recompiled.push(representation.representation_id.clone());
        } else if text != before.1 {
            return Err(integrity(format!(
                "表示 {} 未引用原子 {}，重编译后字节却变了。",
                representation.representation_id, input.atom_id
            )));
        } else {
            unchanged.push(representation.representation_id.clone());
        }
    }

    let at = now()?;
    let version_number = latest.version + 1;
    let atoms_list: Vec<Atom> = atoms.values().cloned().collect();
    let version = IrVersion {
        version: version_number,
        atoms_sha256: sha256(encode(&atoms_list)?.as_bytes()),
        atoms: atoms_list,
        at: at.clone(),
    };
    let verification = revised.verification();
    record.revisions += 1;
    record.status = if verification == PENDING {
        GAP_PENDING
    } else {
        GAP_RESOLVED
    }
    .into();
    record.updated_at = at.clone();
    let over_limit = record.revisions > store.metadata.author.limits.max_revisions;
    if over_limit {
        record.status = GAP_ESCALATED.into();
        record.severity = SEVERITY_MANUAL.into();
    }
    let revision_record = RevisionRecord {
        revision_id: input.revision_id.clone(),
        gap_id: record.gap_id.clone(),
        atom_id: input.atom_id.clone(),
        value: input.value.clone(),
        unit: input.unit.clone(),
        evidence: input.evidence.clone(),
        reason: input.reason.clone(),
        ir_version: version_number,
        recompiled: recompiled.clone(),
        unchanged: unchanged.clone(),
        at: at.clone(),
    };

    let transaction = store.connection.transaction().map_err(db_error)?;
    transaction
        .execute(
            "UPDATE gaps SET status = ?2, payload_json = ?3 WHERE gap_id = ?1",
            params![record.gap_id, record.status, canonical(&record)?],
        )
        .map_err(db_error)?;
    transaction
        .execute(
            "INSERT INTO revisions(revision_id, sequence, gap_id, payload_json) VALUES (?1, ?2, ?3, ?4)",
            params![
                revision_record.revision_id,
                (revisions.len() + 1) as i64,
                revision_record.gap_id,
                canonical(&revision_record)?
            ],
        )
        .map_err(db_error)?;
    for (id, text) in &changed_text {
        transaction
            .execute(
                "UPDATE representations SET render_sha256 = ?2, render = ?3 WHERE representation_id = ?1",
                params![id, sha256(text.as_bytes()), text],
            )
            .map_err(db_error)?;
    }
    transaction
        .execute(
            "INSERT INTO ir_versions(version, atoms_sha256, payload_json, at) VALUES (?1, ?2, ?3, ?4)",
            params![
                i64::from(version.version),
                version.atoms_sha256,
                canonical(&version)?,
                version.at
            ],
        )
        .map_err(db_error)?;
    transaction.commit().map_err(db_error)?;

    for (id, text) in &changed_text {
        write_replace(
            &store.root.join(RENDER).join(format!("{id}.md")),
            text.as_bytes(),
        )?;
    }

    let mut lines = Vec::new();
    lines.push(format!(
        "已记修订 {}：原子 {} 的 {} 改为「{}」（{}）",
        revision_record.revision_id,
        input.atom_id,
        revised.property,
        revised.value,
        if verification == PENDING {
            "无来源补充，标为待核验，不发布为结论"
        } else {
            "证据已核验"
        }
    ));
    lines.push(format!(
        "新 IR 版本：v{version_number}（原子摘要 {}）；缺口 {} 现在 {}",
        &version.atoms_sha256[..16],
        record.gap_id,
        reason_text(&record.status)
    ));
    lines.push(format!(
        "已重编译 {} 个表示：{}",
        recompiled.len(),
        recompiled.join("、")
    ));
    lines.push(format!(
        "未受影响表示 {} 个，字节保持不变：{}",
        unchanged.len(),
        if unchanged.is_empty() {
            "无".to_owned()
        } else {
            unchanged.join("、")
        }
    ));
    lines.push(format!(
        "本缺口已修订 {} 次（上限 {} 次）{}",
        record.revisions,
        store.metadata.author.limits.max_revisions,
        if over_limit {
            "；已超出上限，升级为人工判断，后续修订需人工处理"
        } else {
            ""
        }
    ));
    lines.push(format!("时间：{}", at_label(&at)));
    Ok(lines.join("\n"))
}

pub fn list(root: &Path) -> Result<String, ProjectError> {
    let store = RevisionStore::open_readonly(root)?;
    let gaps = store.gaps()?;
    let revisions = store.revisions()?;
    let representations = store.representations()?;
    let version = store.latest_version()?;
    let mut value = serde_json::Map::new();
    value.insert(
        "case_id".into(),
        serde_json::Value::from(store.metadata.case_id.clone()),
    );
    value.insert("policy".into(), serde_json::Value::from(POLICY));
    value.insert(
        "limits".into(),
        serde_json::to_value(&store.metadata.author.limits)
            .map_err(|failure| integrity(format!("序列化失败：{failure}。")))?,
    );
    value.insert(
        "ir_version".into(),
        serde_json::Value::from(version.version),
    );
    value.insert(
        "ir_atoms_sha256".into(),
        serde_json::Value::from(version.atoms_sha256.clone()),
    );
    value.insert(
        "atoms".into(),
        serde_json::to_value(&version.atoms)
            .map_err(|failure| integrity(format!("序列化失败：{failure}。")))?,
    );
    value.insert(
        "verification".into(),
        serde_json::to_value(version.verification())
            .map_err(|failure| integrity(format!("序列化失败：{failure}。")))?,
    );
    value.insert(
        "representations".into(),
        serde_json::to_value(&representations)
            .map_err(|failure| integrity(format!("序列化失败：{failure}。")))?,
    );
    value.insert(
        "gaps".into(),
        serde_json::to_value(&gaps)
            .map_err(|failure| integrity(format!("序列化失败：{failure}。")))?,
    );
    value.insert(
        "revisions".into(),
        serde_json::to_value(&revisions)
            .map_err(|failure| integrity(format!("序列化失败：{failure}。")))?,
    );
    value.insert("network_calls".into(), serde_json::Value::from(0));
    value.insert("model_calls".into(), serde_json::Value::from(0));
    serde_json::to_string_pretty(&serde_json::Value::Object(value))
        .map_err(|failure| integrity(format!("序列化失败：{failure}。")))
}

pub fn status(root: &Path) -> Result<String, ProjectError> {
    let store = RevisionStore::open_readonly(root)?;
    let gaps = store.gaps()?;
    let revisions = store.revisions()?;
    let representations = store.representations()?;
    let version = store.latest_version()?;
    let mut open = 0;
    let mut resolved = 0;
    let mut pending = 0;
    let mut escalated = 0;
    for gap in &gaps {
        match gap.status.as_str() {
            GAP_OPEN => open += 1,
            GAP_RESOLVED => resolved += 1,
            GAP_PENDING => pending += 1,
            GAP_ESCALATED => escalated += 1,
            other => return Err(integrity(format!("未知缺口状态 {other}。"))),
        }
    }
    let verification = version.verification();
    let verified = verification.get(VERIFIED).copied().unwrap_or(0);
    let unverified = verification.get(PENDING).copied().unwrap_or(0);
    let mut lines = Vec::new();
    lines.push(format!(
        "修订根 {}：已建立（policy={POLICY}，结构版本 {SCHEMA_VERSION}）",
        root.display()
    ));
    lines.push(format!(
        "本轮范围：{} 个原子、{} 个表示（{}）（case {}）",
        version.atoms.len(),
        representations.len(),
        representations
            .iter()
            .map(|item| kind_text(&item.kind))
            .collect::<Vec<_>>()
            .join("、"),
        store.metadata.case_id
    ));
    lines.push(format!(
        "所有权：作者清单 SHA256 {}；语料清单 SHA256 {}；adopt 于 {}",
        &store.owner.author_manifest_sha256[..16],
        &store.owner.corpus_manifest_sha256[..16],
        at_label(&store.owner.created_at)
    ));
    lines.push(format!(
        "冻结语料：{} 个文档、共 {} 字节（清单由 adopt 绑定，之后不接受增删）",
        store.metadata.corpus.documents.len(),
        store.metadata.corpus.total_bytes()
    ));
    lines.push(format!(
        "当前 IR 版本：v{}（{}）；原子摘要 {}",
        version.version,
        at_label(&version.at),
        &version.atoms_sha256[..16]
    ));
    lines.push(format!(
        "原子核验：已核验 {verified} 个、待核验 {unverified} 个（无来源内容标为待核验，不发布为结论）"
    ));
    lines.push(format!(
        "缺口：{} 条（待处理 {open}、已解决 {resolved}、待核验 {pending}、已升级 {escalated}）",
        gaps.len()
    ));
    lines.push(format!(
        "修订：{} 次（单缺口上限 {} 次；超限升级为人工判断）",
        revisions.len(),
        store.metadata.author.limits.max_revisions
    ));
    lines.push(format!(
        "重编译范围：{} 个表示；命中只作为证据线索写回原子，不改写研究授权范围",
        representations.len()
    ));
    lines.push(
        "缺口问题与研究候选分开存放：缺口必须回到修订流程，候选只在研究队列里等待授权。".to_owned(),
    );
    lines.push("检索与模型调用：0 次（本目录从不联网、不调用模型）".to_owned());
    lines.push(
        "四维状态：修订与重编译结果单列，不计入主任务的完成或失败；重编译成功不等于知识正确。"
            .to_owned(),
    );
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_declares_five_frozen_tables() {
        let map = schema_map();
        let names: Vec<&str> = map.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            vec![
                "gaps",
                "ir_versions",
                "metadata",
                "representations",
                "revisions"
            ]
        );
    }

    #[test]
    fn compact_strips_all_whitespace() {
        assert_eq!(compact("a  b\n c\t"), "abc");
    }
}
