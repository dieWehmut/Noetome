//! 有界检索的账本：所有者、冻结语料、预算预留、七类终止理由与检查点恢复。
//!
//! 磁盘布局与 B08-T01 的候选队列同构：
//!
//! ```text
//! <root>/control/owner.json        所有权记录（root/父目录/三个文件的身份）
//! <root>/control/retrieval.sqlite  账本：metadata / units / attempts / controls / termination
//! <root>/control/run.lock          写入锁
//! ```
//!
//! 账本只允许这四项之外没有任何条目；控制目录只允许三个文件。
//! 任何陌生条目、被替换的身份、被改写的记录都只读拒绝并保留现场。
//!
//! 预算的记法沿用 B07-T02 的保守口径：**预留即计入消耗**。一次尝试在扫描
//! 之前就把次数、字节与耗时写进账本，扫描之后只填结果，绝不减少已记的额度。
//! 因此进程在扫描中途被杀，重启后那条 `reserved` 尝试仍然占着额度，只能被
//! 补结算一次；不会被重复计费，也不会凭空多出额度。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};

use crate::compile_identity::{Identity, identity};
use crate::project::{self, ProjectError};
use crate::retrieval_model::*;
use crate::text::sha256;
use crate::vault_store::{
    canonical, joined, now, read_bounded, resolve_caller_path, write_new_file, write_replace,
};

pub(crate) const APP: i64 = 0x4e525452;
pub(crate) const CONTROL: &str = "control";
pub(crate) const OWNER: &str = "owner.json";
pub(crate) const DATABASE: &str = "retrieval.sqlite";
pub(crate) const LOCK: &str = "run.lock";
pub(crate) const MAX_ENTRIES: usize = 64;

const SCHEMA: &str = "CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL,metadata_json TEXT NOT NULL);CREATE TABLE units(query_id TEXT PRIMARY KEY,position INTEGER NOT NULL CHECK(position>=0),state TEXT NOT NULL,payload_json TEXT NOT NULL);CREATE TABLE attempts(attempt INTEGER PRIMARY KEY CHECK(attempt>0),query_id TEXT NOT NULL,state TEXT NOT NULL,payload_json TEXT NOT NULL);CREATE TABLE controls(sequence INTEGER PRIMARY KEY AUTOINCREMENT,action TEXT NOT NULL CHECK(action IN ('pause','resume')),at TEXT NOT NULL);CREATE TABLE termination(singleton INTEGER PRIMARY KEY CHECK(singleton=1),reason TEXT NOT NULL,note TEXT NOT NULL,at TEXT NOT NULL);";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Owner {
    pub(crate) policy: String,
    pub(crate) producer: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) root: String,
    pub(crate) root_identity: Identity,
    pub(crate) parent_identity: Identity,
    pub(crate) control_identity: Identity,
    pub(crate) owner_identity: Identity,
    pub(crate) database_identity: Identity,
    pub(crate) lock_identity: Identity,
    pub(crate) plan_manifest_sha256: String,
    pub(crate) corpus_manifest_sha256: String,
    pub(crate) created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Metadata {
    pub(crate) policy: String,
    pub(crate) producer: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) plan: Plan,
    pub(crate) corpus: CorpusManifest,
    pub(crate) corpus_root: String,
    pub(crate) plan_manifest_sha256: String,
    pub(crate) corpus_manifest_sha256: String,
    pub(crate) created_at: String,
}

pub(crate) struct RetrievalStore {
    pub(crate) root: PathBuf,
    pub(crate) owner: Owner,
    pub(crate) metadata: Metadata,
    connection: Connection,
    _lock: Option<fs::File>,
}

pub(crate) fn schema_map() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for statement in SCHEMA.split(';') {
        let text = statement.trim();
        if text.is_empty() {
            continue;
        }
        let body = text.strip_prefix("CREATE TABLE ").unwrap_or(text);
        let name = body.split('(').next().unwrap_or_default().trim().to_owned();
        out.insert(name, text.to_owned());
    }
    out
}

fn compact(text: &str) -> String {
    text.chars()
        .filter(|value| !value.is_whitespace())
        .collect()
}

fn verify_schema(connection: &Connection) -> Result<(), ProjectError> {
    for (pragma, expected) in [
        ("user_version", i64::from(SCHEMA_VERSION)),
        ("application_id", APP),
        ("synchronous", 2),
    ] {
        let value: i64 = connection
            .query_row(&format!("PRAGMA {pragma}"), [], |row| row.get(0))
            .map_err(ProjectError::database)?;
        if value != expected {
            return Err(integrity(format!(
                "检索账本的 {pragma}={value}，要求 {expected}；未知结构只读拒绝，不迁移不修复。"
            )));
        }
    }
    let mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .map_err(ProjectError::database)?;
    if mode.to_ascii_lowercase() != "delete" {
        return Err(integrity(format!(
            "检索账本的 journal_mode={mode}，要求 delete；只读拒绝，不修复。"
        )));
    }
    let mut statement = connection
        .prepare("SELECT name,sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name")
        .map_err(ProjectError::database)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(ProjectError::database)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(ProjectError::database)?;
    let expected = schema_map();
    if rows.len() != expected.len() {
        return Err(integrity(format!(
            "检索账本的表集合与契约不符：实际 {:?}，要求 {:?}；只读拒绝，不迁移不修复。",
            rows.iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            expected.keys().map(String::as_str).collect::<Vec<_>>()
        )));
    }
    for (name, sql) in rows {
        let Some(want) = expected.get(&name) else {
            return Err(integrity(format!(
                "检索账本出现未知表 {name}；只读拒绝，不迁移不修复。"
            )));
        };
        if compact(&sql) != compact(want) {
            return Err(integrity(format!(
                "检索账本表 {name} 的定义与契约不符；只读拒绝，不迁移不修复。"
            )));
        }
    }
    Ok(())
}

/// 严格解析检索账本自己的记录：UTF-8、字段集完全一致、且必须是规范编码。
pub(crate) fn strict_record<T>(bytes: &[u8]) -> Result<T, ProjectError>
where
    T: serde::de::DeserializeOwned + Serialize,
{
    let text = std::str::from_utf8(bytes).map_err(|_| integrity("检索记录必须是UTF-8文本。"))?;
    let value: T = serde_json::from_str(text)
        .map_err(|error| integrity(format!("检索记录不是严格JSON：{error}。")))?;
    if canonical(&value)?.as_str() != text {
        return Err(integrity("检索记录非规范编码、重复键或未知字段。"));
    }
    Ok(value)
}

fn read_metadata_json(connection: &Connection) -> Result<String, ProjectError> {
    let count: i64 = connection
        .query_row("SELECT count(*) FROM metadata", [], |row| row.get(0))
        .map_err(ProjectError::database)?;
    if count != 1 {
        return Err(integrity(format!(
            "检索账本 metadata 必须有且只有一行，实际 {count} 行；只读拒绝，不修复。"
        )));
    }
    let (schema_version, metadata_json): (i64, String) = connection
        .query_row(
            "SELECT schema_version,metadata_json FROM metadata",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(ProjectError::database)?;
    if schema_version != i64::from(SCHEMA_VERSION) {
        return Err(integrity(format!(
            "检索账本 metadata 的 schema_version={schema_version}，要求 {SCHEMA_VERSION}；只读拒绝。"
        )));
    }
    Ok(metadata_json)
}

fn require_dir(path: &Path) -> Result<(), ProjectError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| ProjectError::io(path, error))?;
    if project::is_link(&metadata) || !metadata.is_dir() {
        return Err(integrity(format!(
            "{} 必须是普通目录（不允许符号链接或文件）；保留现场并拒绝继续。",
            path.display()
        )));
    }
    Ok(())
}

fn names(dir: &Path) -> Result<BTreeSet<String>, ProjectError> {
    let mut out = BTreeSet::new();
    for entry in fs::read_dir(dir).map_err(|error| ProjectError::io(dir, error))? {
        let entry = entry.map_err(|error| ProjectError::io(dir, error))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| ProjectError::unsafe_path(&entry.path()))?;
        out.insert(name);
        if out.len() > MAX_ENTRIES {
            return Err(integrity(format!(
                "{} 的条目超过 {MAX_ENTRIES} 个；保留现场并拒绝继续。",
                dir.display()
            )));
        }
    }
    Ok(out)
}

fn check_identity(path: &Path, expected: &Identity, label: &str) -> Result<(), ProjectError> {
    let actual = identity(path)?;
    if &actual != expected {
        return Err(integrity(format!(
            "{label}的持久身份与所有权记录不符（adopt 之后可能被替换）；保留现场并拒绝继续。"
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
    options
        .open(path)
        .map_err(|error| ProjectError::io(path, error))
}

// ---------------------------------------------------------------------------
// 打开
// ---------------------------------------------------------------------------

impl RetrievalStore {
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
                "{} 不存在，不是已建立的检索根；未修改任何文件。",
                root.display()
            )));
        }
        require_dir(&root)?;
        let control = root.join(CONTROL);
        if !project::exists(&control)? {
            return Err(unknown_root(format!(
                "{} 不是检索根（缺少 control/）：陌生目录保持只读，未接管、未修改任何文件。",
                root.display()
            )));
        }
        require_dir(&control)?;
        let control_names = names(&control)?;
        let control_allowed: BTreeSet<String> = [OWNER, DATABASE, LOCK]
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        if control_names != control_allowed {
            let extra: Vec<String> = control_names
                .difference(&control_allowed)
                .cloned()
                .collect();
            return Err(integrity(format!(
                "control/ 只允许 owner.json、retrieval.sqlite、run.lock；未知文件、目录或数据库 sidecar（{extra:?}）保留原样并拒绝继续。"
            )));
        }
        let root_names = names(&root)?;
        if root_names.len() != 1 || !root_names.contains(CONTROL) {
            let extra: Vec<String> = root_names
                .iter()
                .filter(|name| name.as_str() != CONTROL)
                .cloned()
                .collect();
            return Err(integrity(format!(
                "检索根只允许 control/；未知条目 {extra:?} 保留原样并拒绝继续。"
            )));
        }

        let owner_path = control.join(OWNER);
        project::checked_file(&owner_path)?;
        let owner_bytes = read_bounded(&owner_path, MAX_BYTES)?;
        let owner: Owner = strict_record(&owner_bytes)?;
        if owner.policy != POLICY || owner.producer != PRODUCER {
            return Err(integrity(
                "control/owner.json 的 policy 或 producer 与本契约不符；保留现场并拒绝继续。",
            ));
        }
        if owner.schema_version != SCHEMA_VERSION {
            return Err(integrity(format!(
                "control/owner.json 的 schema_version={}，要求 {SCHEMA_VERSION}；只读拒绝，不迁移。",
                owner.schema_version
            )));
        }

        let root_path = fs::canonicalize(&root).map_err(|error| ProjectError::io(&root, error))?;
        let root_text = root_path.to_string_lossy().into_owned();
        if root_text != owner.root {
            return Err(integrity(format!(
                "检索根现在解析为 {}，与所有权记录 {} 不符；保留现场并拒绝继续。",
                root_path.display(),
                owner.root
            )));
        }
        check_identity(&root_path, &owner.root_identity, "检索根")?;
        check_identity(&control, &owner.control_identity, "control/ 目录")?;
        check_identity(&owner_path, &owner.owner_identity, "所有权记录")?;
        check_identity(&control.join(LOCK), &owner.lock_identity, "写入锁")?;

        let database_path = control.join(DATABASE);
        project::checked_file(&database_path)?;
        check_identity(&database_path, &owner.database_identity, "检索账本")?;
        let flags = if writable {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        } else {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        };
        let connection =
            Connection::open_with_flags(&database_path, flags | OpenFlags::SQLITE_OPEN_NO_MUTEX)
                .map_err(ProjectError::database)?;
        if writable {
            connection
                .execute_batch("PRAGMA journal_mode=DELETE;PRAGMA synchronous=FULL;")
                .map_err(ProjectError::database)?;
        }
        verify_schema(&connection)?;
        let metadata_json = read_metadata_json(&connection)?;
        let metadata: Metadata = strict_record(metadata_json.as_bytes())?;
        if metadata.policy != POLICY || metadata.producer != PRODUCER {
            return Err(integrity(
                "检索账本的 policy 或 producer 与本契约不符；保留现场并拒绝继续。",
            ));
        }
        if metadata.case_id != owner.case_id
            || metadata.created_at != owner.created_at
            || metadata.plan_manifest_sha256 != owner.plan_manifest_sha256
            || metadata.corpus_manifest_sha256 != owner.corpus_manifest_sha256
        {
            return Err(integrity(
                "检索账本与所有权记录的 case、作者清单指纹或语料指纹不一致；保留现场并拒绝继续。",
            ));
        }
        Ok(Self {
            root: root_path,
            owner,
            metadata,
            connection,
            _lock: None,
        })
    }

    pub(crate) fn lock(&mut self) -> Result<(), ProjectError> {
        let path = self.root.join(CONTROL).join(LOCK);
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3).custom_flags(0x00200000);
        }
        let file = options
            .open(&path)
            .map_err(|error| ProjectError::io(&path, error))?;
        if file
            .metadata()
            .map_err(|error| ProjectError::io(&path, error))?
            .len()
            != 0
        {
            return Err(integrity("写入锁必须是空的普通文件；保留现场并拒绝继续。"));
        }
        let start = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => {
                    self._lock = Some(file);
                    return Ok(());
                }
                Err(std::fs::TryLockError::WouldBlock)
                    if start.elapsed() < Duration::from_secs(5) =>
                {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(failure) => {
                    return Err(busy(format!(
                        "写入锁不可取得（{failure}）；未写入任何字节，请关闭另一个检索进程后重试。"
                    )));
                }
            }
        }
    }

    pub(crate) fn units(&self) -> Result<Vec<Unit>, ProjectError> {
        let mut statement = self
            .connection
            .prepare("SELECT payload_json FROM units ORDER BY position")
            .map_err(ProjectError::database)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(ProjectError::database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(ProjectError::database)?;
        let mut out = Vec::with_capacity(rows.len());
        for raw in rows {
            out.push(strict_record(raw.as_bytes())?);
        }
        Ok(out)
    }

    pub(crate) fn attempts(&self) -> Result<Vec<Attempt>, ProjectError> {
        let mut statement = self
            .connection
            .prepare("SELECT payload_json FROM attempts ORDER BY attempt")
            .map_err(ProjectError::database)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(ProjectError::database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(ProjectError::database)?;
        let mut out = Vec::with_capacity(rows.len());
        for raw in rows {
            out.push(strict_record(raw.as_bytes())?);
        }
        Ok(out)
    }

    /// 暂停是否生效：最后一条控制是 pause。
    pub(crate) fn paused(&self) -> Result<bool, ProjectError> {
        let mut statement = self
            .connection
            .prepare("SELECT action FROM controls ORDER BY sequence DESC LIMIT 1")
            .map_err(ProjectError::database)?;
        let mut rows = statement.query([]).map_err(ProjectError::database)?;
        match rows.next().map_err(ProjectError::database)? {
            Some(row) => {
                let action: String = row.get(0).map_err(ProjectError::database)?;
                Ok(action == "pause")
            }
            None => Ok(false),
        }
    }

    pub(crate) fn termination(&self) -> Result<Option<Termination>, ProjectError> {
        let mut statement = self
            .connection
            .prepare("SELECT reason,note,at FROM termination")
            .map_err(ProjectError::database)?;
        let mut rows = statement.query([]).map_err(ProjectError::database)?;
        match rows.next().map_err(ProjectError::database)? {
            Some(row) => Ok(Some(Termination {
                reason: row.get(0).map_err(ProjectError::database)?,
                note: row.get(1).map_err(ProjectError::database)?,
                at: row.get(2).map_err(ProjectError::database)?,
            })),
            None => Ok(None),
        }
    }

    fn write_unit(&self, unit: &Unit) -> Result<(), ProjectError> {
        self.connection
            .execute(
                "UPDATE units SET state=?1,payload_json=?2 WHERE query_id=?3",
                params![unit.state, canonical(unit)?, unit.query_id],
            )
            .map_err(ProjectError::database)?;
        Ok(())
    }

    fn write_attempt(&self, attempt: &Attempt) -> Result<(), ProjectError> {
        self.connection
            .execute(
                "INSERT INTO attempts(attempt,query_id,state,payload_json) VALUES(?1,?2,?3,?4)",
                params![
                    i64::try_from(attempt.attempt).map_err(|_| integrity("尝试序号溢出。"))?,
                    attempt.query_id,
                    attempt.state,
                    canonical(attempt)?
                ],
            )
            .map_err(ProjectError::database)?;
        Ok(())
    }

    fn update_attempt(&self, attempt: &Attempt) -> Result<(), ProjectError> {
        let changed = self
            .connection
            .execute(
                "UPDATE attempts SET state=?1,payload_json=?2 WHERE attempt=?3",
                params![
                    attempt.state,
                    canonical(attempt)?,
                    i64::try_from(attempt.attempt).map_err(|_| integrity("尝试序号溢出。"))?
                ],
            )
            .map_err(ProjectError::database)?;
        if changed != 1 {
            return Err(integrity(format!(
                "尝试 {} 在账本里不存在；保留现场并拒绝继续。",
                attempt.attempt
            )));
        }
        Ok(())
    }

    fn control(&self, action: &str) -> Result<(), ProjectError> {
        let at = now()?;
        self.connection
            .execute(
                "INSERT INTO controls(action,at) VALUES(?1,?2)",
                params![action, at],
            )
            .map_err(ProjectError::database)?;
        Ok(())
    }

    fn record_termination(&self, termination: &Termination) -> Result<(), ProjectError> {
        let existing: i64 = self
            .connection
            .query_row("SELECT count(*) FROM termination", [], |row| row.get(0))
            .map_err(ProjectError::database)?;
        if existing != 0 {
            return Err(argument(
                "本轮已经记录过终止理由；终止只写一次，未修改任何字节。",
            ));
        }
        self.connection
            .execute(
                "INSERT INTO termination(singleton,reason,note,at) VALUES(1,?1,?2,?3)",
                params![termination.reason, termination.note, termination.at],
            )
            .map_err(ProjectError::database)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// adopt
// ---------------------------------------------------------------------------

pub fn adopt(plan_path: &Path, root: &Path) -> Result<String, ProjectError> {
    let plan_path = resolve_caller_path(plan_path)?;
    project::checked_file(&plan_path)?;
    let plan_bytes = read_bounded(&plan_path, MAX_BYTES)?;
    let plan = Plan::parse(&plan_bytes)?;
    let package = plan_path
        .parent()
        .ok_or_else(|| argument("作者声明没有所在目录；未创建任何文件。"))?
        .to_path_buf();

    let corpus_root = package.join(&plan.corpus.root);
    require_dir(&corpus_root)?;
    let manifest_path = package.join(&plan.corpus.manifest);
    project::checked_file(&manifest_path)?;
    let manifest_bytes = read_bounded(&manifest_path, MAX_BYTES)?;
    let corpus = CorpusManifest::parse(&manifest_bytes)?;
    if corpus.case_id != plan.case_id {
        return Err(argument(format!(
            "语料清单的 case_id={} 与作者声明的 case_id={} 不一致；未创建任何文件。",
            corpus.case_id, plan.case_id
        )));
    }
    verify_corpus(&corpus_root, &corpus)?;

    let root = resolve_caller_path(root)?;
    if project::exists(&root)? {
        return Err(argument(format!(
            "检索根必须尚不存在：{}；未修改任何文件。",
            root.display()
        )));
    }
    let parent = root
        .parent()
        .ok_or_else(|| argument("检索根没有父目录；未创建任何文件。"))?
        .to_path_buf();
    if !project::exists(&parent)? {
        return Err(argument(format!(
            "检索根的父目录必须已存在：{}；未创建任何文件。",
            parent.display()
        )));
    }
    require_dir(&parent)?;
    let parent_identity = identity(&parent)?;
    fs::create_dir(&root).map_err(|error| ProjectError::io(&root, error))?;
    let created = identity(&root)?;
    let outcome = adopt_into(
        &root,
        &created,
        plan,
        corpus,
        &corpus_root,
        sha256(&plan_bytes),
        sha256(&manifest_bytes),
        parent_identity,
    );
    match outcome {
        Ok(summary) => Ok(summary),
        Err(error) => {
            // 只有本次新建的根会被删掉；父目录既有内容永不删除。
            if identity(&root).ok().as_ref() == Some(&created) {
                let _ = fs::remove_dir_all(&root);
            }
            Err(error)
        }
    }
}

fn verify_corpus(corpus_root: &Path, corpus: &CorpusManifest) -> Result<(), ProjectError> {
    for document in &corpus.documents {
        let path = joined(corpus_root, &document.path);
        project::checked_file(&path)?;
        let bytes = read_bounded(&path, MAX_BYTES)?;
        if bytes.len() as u64 != document.byte_length {
            return Err(corpus_mismatch(format!(
                "语料 {} 的长度是 {}，清单要求 {}；冻结语料被改过，未创建任何文件。",
                document.path,
                bytes.len(),
                document.byte_length
            )));
        }
        if sha256(&bytes) != document.sha256 {
            return Err(corpus_mismatch(format!(
                "语料 {} 的 SHA256 与清单不符；冻结语料被改过，未创建任何文件。",
                document.path
            )));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn adopt_into(
    root: &Path,
    created: &Identity,
    plan: Plan,
    corpus: CorpusManifest,
    corpus_root: &Path,
    plan_manifest_sha256: String,
    corpus_manifest_sha256: String,
    parent_identity: Identity,
) -> Result<String, ProjectError> {
    let root_path = fs::canonicalize(root).map_err(|error| ProjectError::io(root, error))?;
    let corpus_root_text = fs::canonicalize(corpus_root)
        .map_err(|error| ProjectError::io(corpus_root, error))?
        .to_string_lossy()
        .into_owned();
    let created_at = now()?;
    let control = root_path.join(CONTROL);
    fs::create_dir(&control).map_err(|error| ProjectError::io(&control, error))?;
    let control_identity = identity(&control)?;

    let lock_path = control.join(LOCK);
    let lock = open_new(&lock_path)?;
    lock.sync_all()
        .map_err(|error| ProjectError::io(&lock_path, error))?;
    let lock_identity = identity(&lock_path)?;
    drop(lock);

    let database_path = control.join(DATABASE);
    let database = open_new(&database_path)?;
    database
        .sync_all()
        .map_err(|error| ProjectError::io(&database_path, error))?;
    let database_identity = identity(&database_path)?;
    drop(database);

    let owner_path = control.join(OWNER);
    write_new_file(&owner_path, b"")?;
    let owner_identity = identity(&owner_path)?;

    let metadata = Metadata {
        policy: POLICY.to_owned(),
        producer: PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: plan.case_id.clone(),
        plan: plan.clone(),
        corpus: corpus.clone(),
        corpus_root: corpus_root_text,
        plan_manifest_sha256: plan_manifest_sha256.clone(),
        corpus_manifest_sha256: corpus_manifest_sha256.clone(),
        created_at: created_at.clone(),
    };

    {
        let connection = Connection::open_with_flags(
            &database_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(ProjectError::database)?;
        connection
            .execute_batch("PRAGMA journal_mode=DELETE;PRAGMA synchronous=FULL;")
            .map_err(ProjectError::database)?;
        connection
            .execute_batch(&format!(
                "PRAGMA user_version = {SCHEMA_VERSION};PRAGMA application_id = {APP};{SCHEMA}"
            ))
            .map_err(ProjectError::database)?;
        connection
            .execute(
                "INSERT INTO metadata(singleton,schema_version,metadata_json) VALUES(1,?1,?2)",
                params![i64::from(SCHEMA_VERSION), canonical(&metadata)?],
            )
            .map_err(ProjectError::database)?;
        for (position, query) in plan.queries.iter().enumerate() {
            let unit = Unit {
                query_id: query.query_id.clone(),
                position: u64::try_from(position).map_err(|_| integrity("查询序号溢出。"))?,
                state: UNIT_PENDING.to_owned(),
                query: query.clone(),
            };
            connection
                .execute(
                    "INSERT INTO units(query_id,position,state,payload_json) VALUES(?1,?2,?3,?4)",
                    params![
                        unit.query_id,
                        i64::try_from(position).map_err(|_| integrity("查询序号溢出。"))?,
                        unit.state,
                        canonical(&unit)?
                    ],
                )
                .map_err(ProjectError::database)?;
        }
        verify_schema(&connection)?;
    }

    let owner = Owner {
        policy: POLICY.to_owned(),
        producer: PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: plan.case_id.clone(),
        root: root_path.to_string_lossy().into_owned(),
        root_identity: created.clone(),
        parent_identity,
        control_identity,
        owner_identity,
        database_identity,
        lock_identity,
        plan_manifest_sha256,
        corpus_manifest_sha256,
        created_at,
    };
    write_replace(&owner_path, canonical(&owner)?.as_bytes())?;

    let pending = plan.queries.len();
    let mut lines = Vec::new();
    lines.push(format!(
        "检索根 {}：已建立（policy={POLICY}，结构版本 {SCHEMA_VERSION}）",
        root_path.display()
    ));
    lines.push(format!(
        "本轮目标：{} 个主题（{}）；外部检索：{}",
        plan.goal_subjects.len(),
        plan.goal_subjects.join("、"),
        if plan.external_retrieval {
            "允许"
        } else {
            "已关闭（只在冻结语料内检索，缺口保留待核验）"
        }
    ));
    lines.push(format!(
        "冻结语料：{} 个文档、共 {} 字节（清单 SHA256 {}）",
        corpus.documents.len(),
        corpus.total_bytes(),
        &owner.corpus_manifest_sha256[..16]
    ));
    lines.push(format!(
        "预算上限：查询 {} 次、单次 {} 字节/{} 毫秒、累计 {} 字节/{} 毫秒",
        plan.limits.max_queries,
        plan.limits.max_query_bytes,
        plan.limits.max_query_millis,
        plan.limits.max_total_bytes,
        plan.limits.max_total_millis
    ));
    lines.push(format!("待执行查询：{pending} 条；已用预算：0 次"));
    lines.push(
        "终止理由只能由账本证据支撑：目标满足、无可靠新证据、边际收益不足、预算不足、用户暂停、网络受限、需要人工判断。"
            .to_owned(),
    );
    Ok(lines.join("\n"))
}

// ---------------------------------------------------------------------------
// 执行
// ---------------------------------------------------------------------------

/// 一次尝试的执行结果，供 CLI 与测试观察。
pub(crate) struct StepOutput {
    pub(crate) attempt: u64,
    pub(crate) query_id: String,
    pub(crate) outcome: String,
    pub(crate) hits: u64,
    pub(crate) new_hits: u64,
    pub(crate) budget_exhausted: bool,
}

/// 执行一条待结算的查询：先持久预留，再扫描，最后结算。
///
/// 若账本里已有 `reserved` 尝试（上次在扫描途中被杀），本次只补结算那一条，
/// 不会再为同一查询预留第二次额度。
pub(crate) fn step(root: &Path) -> Result<StepOutput, ProjectError> {
    let mut store = RetrievalStore::open(root)?;
    store.lock()?;
    if let Some(termination) = store.termination()? {
        return Err(argument(format!(
            "本轮已在 {} 记录终止理由「{}」，不再调度新查询；未修改任何字节。",
            termination.at,
            reason_text(&termination.reason)
        )));
    }
    if store.paused()? {
        return Err(paused(
            "检索已暂停：不再调度新查询，已结算结果与未完成目标保留；请先 resume。",
        ));
    }

    let units = store.units()?;
    let attempts = store.attempts()?;
    if let Some(reserved) = attempts.iter().find(|item| item.state == ATTEMPT_RESERVED) {
        return finish_reserved(&store, &units, reserved.clone());
    }

    let pending = units
        .iter()
        .find(|unit| unit.state == UNIT_PENDING)
        .cloned()
        .ok_or_else(|| argument("没有待执行查询；本轮查询已全部结算，未修改任何字节。"))?;

    let view = BudgetView::compute(&store.metadata.plan.limits, &attempts);
    let (queries, bytes, millis) = estimate_reservation(&store.metadata.plan.limits, &pending);
    if !view.admits(queries, bytes, millis) {
        return Ok(StepOutput {
            attempt: 0,
            query_id: pending.query_id.clone(),
            outcome: String::new(),
            hits: 0,
            new_hits: 0,
            budget_exhausted: true,
        });
    }

    let attempt_number = u64::try_from(attempts.len()).unwrap_or(u64::MAX) + 1;
    let reserved = Attempt {
        attempt: attempt_number,
        query_id: pending.query_id.clone(),
        reserved_queries: queries,
        reserved_bytes: bytes,
        reserved_millis: millis,
        state: ATTEMPT_RESERVED.to_owned(),
        settled_bytes: None,
        hits: None,
        new_hits: 0,
        outcome: None,
        hit_keys: Vec::new(),
        at: now()?,
    };
    // 预留必须先落账，然后才允许扫描：崩溃只会留下一条待补结算的尝试。
    store.write_attempt(&reserved)?;
    finish_reserved(&store, &units, reserved)
}

fn finish_reserved(
    store: &RetrievalStore,
    units: &[Unit],
    mut attempt: Attempt,
) -> Result<StepOutput, ProjectError> {
    let unit = units
        .iter()
        .find(|unit| unit.query_id == attempt.query_id)
        .cloned()
        .ok_or_else(|| {
            integrity(format!(
                "尝试 {} 指向的查询 {} 不在账本里；保留现场并拒绝继续。",
                attempt.attempt, attempt.query_id
            ))
        })?;
    let plan = &store.metadata.plan;
    let corpus = &store.metadata.corpus;

    let settled = if unit.query.requires_external {
        // 需要外部来源的查询在关闭外部检索时不执行，也不发起任何网络请求。
        None
    } else {
        let mut documents = Vec::with_capacity(corpus.documents.len());
        for document in &corpus.documents {
            let path = joined(Path::new(&store.metadata.corpus_root), &document.path);
            let bytes = read_bounded(&path, MAX_BYTES)?;
            if bytes.len() as u64 != document.byte_length || sha256(&bytes) != document.sha256 {
                return Err(corpus_mismatch(format!(
                    "语料 {} 在检索期间与冻结清单不符；本轮停止，未结算这条尝试。",
                    document.path
                )));
            }
            documents.push((document.path.clone(), bytes));
        }
        Some(scan(&documents, &unit.query.terms, plan.limits.max_hits))
    };

    let previous: BTreeSet<String> = store
        .attempts()?
        .into_iter()
        .filter(|item| item.attempt != attempt.attempt && item.state == ATTEMPT_SETTLED)
        .flat_map(|item| item.hit_keys)
        .collect();

    let (outcome, hits, new_hits, hit_keys, settled_bytes) = match settled {
        Some(hits) => {
            let keys: Vec<String> = hits.iter().map(hit_key).collect();
            let fresh = keys.iter().filter(|key| !previous.contains(*key)).count() as u64;
            let outcome = if hits.is_empty() {
                OUTCOME_NO_HITS
            } else if fresh == 0 {
                OUTCOME_DUPLICATES
            } else {
                OUTCOME_EVIDENCE
            };
            let settled_bytes = hits.iter().map(|hit| hit.snippet.len() as u64).sum();
            (
                outcome.to_owned(),
                hits.len() as u64,
                fresh,
                keys,
                settled_bytes,
            )
        }
        None => (
            if plan.external_retrieval {
                OUTCOME_EXTERNAL.to_owned()
            } else {
                OUTCOME_EXTERNAL.to_owned()
            },
            0,
            0,
            Vec::new(),
            0,
        ),
    };

    // 冲突：同一 conflict_group 下已结算的查询拿到了不同的证据文档集合。
    let outcome = if outcome == OUTCOME_EVIDENCE {
        let group = unit.query.conflict_group.clone();
        match group {
            None => outcome,
            Some(group) => {
                let mut conflict = false;
                for other in store.attempts()? {
                    if other.attempt == attempt.attempt || other.state != ATTEMPT_SETTLED {
                        continue;
                    }
                    if other.outcome.as_deref() != Some(OUTCOME_EVIDENCE) {
                        continue;
                    }
                    let Some(other_unit) =
                        units.iter().find(|item| item.query_id == other.query_id)
                    else {
                        continue;
                    };
                    if other_unit.query.conflict_group.as_deref() != Some(group.as_str()) {
                        continue;
                    }
                    let mut mine: Vec<&String> = hit_keys.iter().collect();
                    let mut theirs: Vec<&String> = other.hit_keys.iter().collect();
                    mine.sort();
                    theirs.sort();
                    if mine != theirs {
                        conflict = true;
                        break;
                    }
                }
                if conflict {
                    OUTCOME_CONFLICT.to_owned()
                } else {
                    outcome
                }
            }
        }
    } else {
        outcome
    };

    attempt.state = ATTEMPT_SETTLED.to_owned();
    attempt.settled_bytes = Some(settled_bytes);
    attempt.hits = Some(hits);
    attempt.new_hits = new_hits;
    attempt.outcome = Some(outcome.clone());
    attempt.hit_keys = hit_keys;
    store.update_attempt(&attempt)?;
    let mut settled_unit = unit.clone();
    settled_unit.state = if outcome == OUTCOME_EXTERNAL {
        UNIT_GAPPED.to_owned()
    } else {
        UNIT_SETTLED.to_owned()
    };
    store.write_unit(&settled_unit)?;

    Ok(StepOutput {
        attempt: attempt.attempt,
        query_id: attempt.query_id,
        outcome,
        hits,
        new_hits,
        budget_exhausted: false,
    })
}

/// 单步执行并输出一条人类可读的结果。
pub fn step_text(root: &Path) -> Result<String, ProjectError> {
    let output = step(root)?;
    if output.budget_exhausted {
        return Ok(format!(
            "预算不足：下一条查询 {} 已超出剩余额度（查询/字节/耗时三维任一不足即停止）。\n已结算结果与未完成目标保留；请记录终止理由或提高上限后 resume。\n",
            output.query_id
        ));
    }
    Ok(format!(
        "尝试 #{} · 查询 {}：{}（命中 {}，其中新增 {}）。\n",
        output.attempt,
        output.query_id,
        outcome_text(&output.outcome),
        output.hits,
        output.new_hits
    ))
}

/// 连续执行直到没有待执行查询、预算到限、暂停或已记终止理由。
pub fn run(root: &Path) -> Result<String, ProjectError> {
    let mut steps = 0u64;
    let mut records: Vec<serde_json::Value> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let stopped = loop {
        match step(root) {
            Ok(output) if output.budget_exhausted => {
                notes.push(format!(
                    "预算不足：下一条查询 {} 已超出剩余额度，停止调度新查询；在途结果保留。",
                    output.query_id
                ));
                break "budget_exhausted";
            }
            Ok(output) => {
                steps += 1;
                records.push(serde_json::json!({
                    "query_id": output.query_id,
                    "outcome": output.outcome,
                    "hits": output.hits,
                    "new_hits": output.new_hits,
                }));
                notes.push(format!(
                    "查询 {}：{}（命中 {}，其中新增 {}）",
                    output.query_id,
                    outcome_text(&output.outcome),
                    output.hits,
                    output.new_hits
                ));
            }
            Err(error) if error.code() == "RETRIEVAL_PAUSED" => {
                notes.push(error.to_string());
                break "user_paused";
            }
            Err(error) if error.code() == "RETRIEVAL_ARGUMENT" => {
                // 没有待执行查询或已记终止理由都属于正常收束。
                notes.push(error.to_string());
                break "already_stopped";
            }
            Err(error) => return Err(error),
        }
    };
    let store = RetrievalStore::open_readonly(root)?;
    let attempts = store.attempts()?;
    let view = BudgetView::compute(&store.metadata.plan.limits, &attempts);
    let payload = serde_json::json!({
        "steps": steps,
        "stopped": stopped,
        "records": records,
        "notes": notes,
        "budget": view,
        "network_requests": 0,
        "model_calls": 0,
    });
    Ok(serde_json::to_string_pretty(&payload)
        .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}")))
}

pub fn pause(root: &Path) -> Result<String, ProjectError> {
    let mut store = RetrievalStore::open(root)?;
    store.lock()?;
    store.control("pause")?;
    Ok(format!(
        "检索已暂停：不再调度新查询；已预留的尝试保留，resume 后只补结算那一条。\n"
    ))
}

pub fn resume(root: &Path) -> Result<String, ProjectError> {
    let mut store = RetrievalStore::open(root)?;
    store.lock()?;
    let pending_reserved = store
        .attempts()?
        .into_iter()
        .filter(|item| item.state == ATTEMPT_RESERVED)
        .count();
    store.control("resume")?;
    Ok(format!(
        "检索已恢复：{pending_reserved} 条已预留尝试待补结算；预算按预留口径继续计数，不重复扣减。\n"
    ))
}

/// 记录终止理由。理由必须有账本证据支撑，否则拒绝且不写任何字节。
pub fn finish(root: &Path, reason_path: &Path) -> Result<String, ProjectError> {
    let reason_path = resolve_caller_path(reason_path)?;
    project::checked_file(&reason_path)?;
    let bytes = read_bounded(&reason_path, MAX_BYTES)?;
    let record: TerminationFile = serde_json::from_slice(&bytes)
        .map_err(|error| argument(format!("终止理由文件不是严格JSON：{error}。")))?;
    record.validate()?;

    let mut store = RetrievalStore::open(root)?;
    store.lock()?;
    if let Some(existing) = store.termination()? {
        return Err(argument(format!(
            "本轮已经在 {} 记录终止理由「{}」；终止只写一次，未修改任何字节。",
            existing.at,
            reason_text(&existing.reason)
        )));
    }
    let units = store.units()?;
    let attempts = store.attempts()?;
    let view = BudgetView::compute(&store.metadata.plan.limits, &attempts);
    reason_supported(
        &record.reason,
        &view,
        &store.metadata.plan.limits,
        &units,
        &attempts,
        &store.metadata.plan.goals(),
        store.paused()?,
        store.metadata.plan.external_retrieval,
    )?;
    let termination = Termination {
        reason: record.reason.clone(),
        note: record.note.clone(),
        at: now()?,
    };
    store.record_termination(&termination)?;

    let pending = units
        .iter()
        .filter(|unit| unit.state == UNIT_PENDING)
        .count();
    let gapped = units
        .iter()
        .filter(|unit| unit.state == UNIT_GAPPED)
        .count();
    Ok(format!(
        "已记录终止理由「{}」：{}\n未完成目标：{pending} 条查询保持待执行；待核验缺口：{gapped} 条（需要外部来源）。\n检查点与未完成目标保留，可随时 resume 继续。\n",
        reason_text(&termination.reason),
        termination.note
    ))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminationFile {
    pub(crate) reason: String,
    pub(crate) note: String,
}

impl TerminationFile {
    fn validate(&self) -> Result<(), ProjectError> {
        if !REASONS.contains(&self.reason.as_str()) {
            return Err(argument(format!(
                "终止理由必须是以下之一：{}；实际 {}。",
                REASONS.join("、"),
                self.reason
            )));
        }
        if self.note.trim().is_empty() || self.note.chars().count() > MAX_TEXT {
            return Err(argument(format!(
                "终止理由的 note 必须有 1 到 {MAX_TEXT} 个字符。"
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 报告
// ---------------------------------------------------------------------------

pub fn list(root: &Path) -> Result<String, ProjectError> {
    let store = RetrievalStore::open_readonly(root)?;
    let units = store.units()?;
    let attempts = store.attempts()?;
    let termination = store.termination()?;
    let view = BudgetView::compute(&store.metadata.plan.limits, &attempts);
    let payload = serde_json::json!({
        "case_id": store.metadata.case_id,
        "policy": store.metadata.policy,
        "external_retrieval": store.metadata.plan.external_retrieval,
        "goals": store.metadata.plan.goal_subjects,
        "corpus": {
            "root": store.metadata.corpus_root,
            "documents": store.metadata.corpus.documents.len(),
            "bytes": store.metadata.corpus.total_bytes(),
            "manifest_sha256": store.metadata.corpus_manifest_sha256,
        },
        "limits": store.metadata.plan.limits,
        "budget": view,
        "paused": store.paused()?,
        "termination": termination,
        "units": units.iter().map(|unit| serde_json::json!({
            "query_id": unit.query_id,
            "subject": unit.query.subject,
            "scope_class": unit.query.scope_class,
            "requires_external": unit.query.requires_external,
            "state": unit.state,
        })).collect::<Vec<_>>(),
        "attempts": attempts,
        "totals": totals_by_outcome(&attempts),
        "network_requests": 0,
        "model_calls": 0,
    });
    Ok(serde_json::to_string_pretty(&payload)
        .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}")))
}

pub fn status(root: &Path) -> Result<String, ProjectError> {
    let store = RetrievalStore::open_readonly(root)?;
    let units = store.units()?;
    let attempts = store.attempts()?;
    let termination = store.termination()?;
    let view = BudgetView::compute(&store.metadata.plan.limits, &attempts);
    let plan = &store.metadata.plan;

    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    for unit in &units {
        *counts.entry(unit.state.as_str()).or_insert(0) += 1;
    }
    let totals = totals_by_outcome(&attempts);
    let mut lines = Vec::new();
    lines.push(format!(
        "检索根 {}：已建立（policy={}，结构版本 {}）",
        store.root.display(),
        store.metadata.policy,
        SCHEMA_VERSION
    ));
    lines.push(format!(
        "所有权：作者清单 SHA256 {}；语料清单 SHA256 {}；adopt 于 {}",
        &store.owner.plan_manifest_sha256[..16],
        &store.owner.corpus_manifest_sha256[..16],
        store.owner.created_at
    ));
    lines.push(format!(
        "本轮目标：{} 个主题（{}）（case {}）",
        plan.goal_subjects.len(),
        plan.goal_subjects.join("、"),
        store.metadata.case_id
    ));
    let authorized = plan
        .queries
        .iter()
        .filter(|query| query.scope_class == SCOPE_AUTHORIZED)
        .count();
    let prerequisites = plan.queries.len().saturating_sub(authorized);
    lines.push(format!(
        "授权范围：{} 条查询（授权目标 {authorized} 条、必需前提 {prerequisites} 条）；越界主题不在检索计划内",
        plan.queries.len()
    ));
    lines.push(format!(
        "外部检索：{}",
        if plan.external_retrieval {
            "允许（本轮仍只在冻结语料内检索）"
        } else {
            "已关闭：零网络请求，缺口保留待核验"
        }
    ));
    lines.push(format!(
        "冻结语料：{} 个文档、共 {} 字节",
        store.metadata.corpus.documents.len(),
        store.metadata.corpus.total_bytes()
    ));
    lines.push(format!(
        "预算：查询 {}/{}、字节 {}/{}、耗时 {}/{} 毫秒（预留即计入，未用额度不回收）",
        view.queries_used,
        plan.limits.max_queries,
        view.bytes_used,
        plan.limits.max_total_bytes,
        view.millis_used,
        plan.limits.max_total_millis
    ));
    lines.push(format!(
        "查询状态：待执行 {} 条、已结算 {} 条、待核验缺口 {} 条",
        counts.get(UNIT_PENDING).copied().unwrap_or(0),
        counts.get(UNIT_SETTLED).copied().unwrap_or(0),
        counts.get(UNIT_GAPPED).copied().unwrap_or(0)
    ));
    let mut outcome_line = String::from("已结算结果：");
    if totals.is_empty() {
        outcome_line.push_str("尚无");
    } else {
        let parts: Vec<String> = totals
            .iter()
            .map(|(outcome, count)| format!("{} {count} 条", outcome_text(outcome)))
            .collect();
        outcome_line.push_str(&parts.join("、"));
    }
    lines.push(outcome_line);
    lines.push(format!(
        "暂停控制：{}",
        if store.paused()? {
            "已暂停"
        } else {
            "运行中"
        }
    ));
    match termination {
        Some(termination) => lines.push(format!(
            "终止理由：{}（记录于 {}）。说明：{}",
            reason_text(&termination.reason),
            termination.at,
            termination.note
        )),
        None => lines.push("终止理由：尚未记录（本轮未收束）".to_owned()),
    }
    lines.push(
        "检索与模型调用：本目录从不联网、不调用模型；命中只是证据线索，不写入知识库或托管 Vault。"
            .to_owned(),
    );
    lines.push(
        "四维状态：研究检索单列，不计入主任务的完成或失败；未批准候选不改变本轮必做集合。"
            .to_owned(),
    );
    Ok(lines.join("\n"))
}
