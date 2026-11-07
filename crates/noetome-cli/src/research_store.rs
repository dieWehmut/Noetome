//! 研究候选队列的存储层。
//!
//! 这一层只回答「这个目录是不是一个已建立的研究队列根、里面记了什么」，
//! 不含候选归并与决定判定（那在 `research_model`）。目录契约只有 `control/`：
//! `control/owner.json` 记所有权与冻结的本轮目标，`control/research.sqlite` 记
//! 当前候选与只追加的事件，`control/queue.lock` 是写入锁。未知布局一律保留现场
//! 并拒绝，不迁移、不修复。
//!
//! 队列操作不联网、不调用模型：这里没有任何 HTTP 或模型代码。

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    compile_identity::{Identity, identity},
    project::{self, ProjectError},
    research_model::{
        Applied, Author, Counters, DecisionFile, Limits, Objective, ObservationFile, POLICY,
        PRODUCER, SCHEMA_VERSION, StateRecord, argument, integrity, subject_key,
    },
    text::sha256,
    vault_store::{canonical, now, read_bounded, resolve_caller_path},
};

/// 研究队列账本的 application_id，对应 ASCII `NRQH`。
pub(crate) const APP: i64 = 0x4e525148;

pub(crate) const CONTROL: &str = "control";
pub(crate) const OWNER: &str = "owner.json";
pub(crate) const DATABASE: &str = "research.sqlite";
pub(crate) const LOCK: &str = "queue.lock";

/// 单个记录文件的读取上限；超出即拒绝，不做流式部分读取。
pub(crate) const MAX_BYTES: u64 = 4 * 1024 * 1024;
/// 一个目录允许出现的条目上限。
const MAX_ENTRIES: usize = 64;

pub(crate) const EVENT_ADOPT: &str = "adopt";
pub(crate) const EVENT_OBSERVATION: &str = "observation";
pub(crate) const EVENT_DECISION: &str = "decision";

const SCHEMA: &str = "CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL,metadata_json TEXT NOT NULL);CREATE TABLE candidates(subject TEXT PRIMARY KEY,position INTEGER NOT NULL CHECK(position>=0),state TEXT NOT NULL,payload_json TEXT NOT NULL);CREATE TABLE events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,kind TEXT NOT NULL,round TEXT NOT NULL,payload_json TEXT NOT NULL,at TEXT NOT NULL);";

// ---------------------------------------------------------------------------
// 记录
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub(crate) author_manifest_sha256: String,
    pub(crate) objective: Objective,
    pub(crate) limits: Limits,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Metadata {
    pub(crate) policy: String,
    pub(crate) producer: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) author_manifest_sha256: String,
    pub(crate) objective: Objective,
    pub(crate) limits: Limits,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventPayload {
    pub(crate) round: String,
    pub(crate) applied: Vec<Applied>,
    pub(crate) counters: Counters,
}

pub(crate) struct ResearchStore {
    pub(crate) root: PathBuf,
    pub(crate) owner: Owner,
    pub(crate) metadata: Metadata,
    connection: Connection,
    _lock: Option<File>,
}

// ---------------------------------------------------------------------------
// sqlite 结构
// ---------------------------------------------------------------------------

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
                "研究队列账本的 {pragma}={value}，要求 {expected}；未知结构只读拒绝，不迁移不修复。"
            )));
        }
    }
    let mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .map_err(ProjectError::database)?;
    if mode.to_ascii_lowercase() != "delete" {
        return Err(integrity(format!(
            "研究队列账本的 journal_mode={mode}，要求 delete；只读拒绝，不修复。"
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
            "研究队列账本的表集合与契约不符：实际 {:?}，要求 {:?}；只读拒绝，不迁移不修复。",
            rows.iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            expected.keys().map(String::as_str).collect::<Vec<_>>()
        )));
    }
    for (name, sql) in rows {
        let Some(want) = expected.get(&name) else {
            return Err(integrity(format!(
                "研究队列账本出现未知表 {name}；只读拒绝，不迁移不修复。"
            )));
        };
        if compact(&sql) != compact(want) {
            return Err(integrity(format!(
                "研究队列账本表 {name} 的定义与契约不符；只读拒绝，不迁移不修复。"
            )));
        }
    }
    Ok(())
}

/// 严格解析研究队列自己的记录：UTF-8、字段集完全一致、且必须是规范编码。
///
/// 与 `vault_store::strict` 同一套字节级判据，但错误码用本子系统的
/// `RESEARCH_INTEGRITY`，调用方不必从 `VAULT_*` 反推是哪个账本坏了。
pub(crate) fn strict_record<T>(bytes: &[u8]) -> Result<T, ProjectError>
where
    T: serde::de::DeserializeOwned + Serialize,
{
    let text =
        std::str::from_utf8(bytes).map_err(|_| integrity("研究队列记录必须是UTF-8文本。"))?;
    let value: T = serde_json::from_str(text)
        .map_err(|error| integrity(format!("研究队列记录不是严格JSON：{error}。")))?;
    if canonical(&value)?.as_str() != text {
        return Err(integrity("研究队列记录非规范编码、重复键或未知字段。"));
    }
    Ok(value)
}

fn read_metadata_json(connection: &Connection) -> Result<String, ProjectError> {
    let count: i64 = connection
        .query_row("SELECT count(*) FROM metadata", [], |row| row.get(0))
        .map_err(ProjectError::database)?;
    if count != 1 {
        return Err(integrity(format!(
            "研究队列账本 metadata 必须有且只有一行，实际 {count} 行；只读拒绝，不修复。"
        )));
    }
    let (singleton, schema_version, metadata_json): (i64, i64, String) = connection
        .query_row(
            "SELECT singleton,schema_version,metadata_json FROM metadata",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(ProjectError::database)?;
    if singleton != 1 {
        return Err(integrity("研究队列账本 metadata 的 singleton 必须是 1。"));
    }
    if schema_version != i64::from(SCHEMA_VERSION) {
        return Err(integrity(format!(
            "研究队列账本 metadata 的 schema_version={schema_version}，要求 {SCHEMA_VERSION}；只读拒绝。"
        )));
    }
    Ok(metadata_json)
}

// ---------------------------------------------------------------------------
// 目录与身份
// ---------------------------------------------------------------------------

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

fn open_new(path: &Path) -> Result<File, ProjectError> {
    let mut options = std::fs::OpenOptions::new();
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
// 打开与读取
// ---------------------------------------------------------------------------

impl ResearchStore {
    pub(crate) fn open(root: &Path) -> Result<Self, ProjectError> {
        Self::open_mode(root, true)
    }

    pub(crate) fn open_readonly(root: &Path) -> Result<Self, ProjectError> {
        Self::open_mode(root, false)
    }

    fn open_mode(root: &Path, writable: bool) -> Result<Self, ProjectError> {
        let root = resolve_caller_path(root)?;
        if !project::exists(&root)? {
            return Err(crate::research_model::unknown_root(format!(
                "{} 不存在，不是已建立的研究队列根；未修改任何文件。",
                root.display()
            )));
        }
        require_dir(&root)?;
        let control = root.join(CONTROL);
        if !project::exists(&control)? {
            return Err(crate::research_model::unknown_root(format!(
                "{} 不是研究队列根（缺少 control/）：陌生目录保持只读，未接管、未修改任何文件。",
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
                "control/ 只允许 owner.json、research.sqlite、queue.lock；未知文件、目录或数据库 sidecar（{extra:?}）保留原样并拒绝继续。"
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
                "研究队列根只允许 control/；未知条目 {extra:?} 保留原样并拒绝继续。"
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
                "研究队列根现在解析为 {}，与所有权记录 {} 不符；保留现场并拒绝继续。",
                root_path.display(),
                owner.root
            )));
        }
        check_identity(&root_path, &owner.root_identity, "研究队列根")?;
        check_identity(&control, &owner.control_identity, "control/ 目录")?;
        check_identity(&owner_path, &owner.owner_identity, "所有权记录")?;
        check_identity(&control.join(LOCK), &owner.lock_identity, "写入锁")?;

        let database_path = control.join(DATABASE);
        project::checked_file(&database_path)?;
        check_identity(&database_path, &owner.database_identity, "研究队列账本")?;
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
                "研究队列账本的 policy 或 producer 与本契约不符；保留现场并拒绝继续。",
            ));
        }
        if metadata.case_id != owner.case_id
            || metadata.author_manifest_sha256 != owner.author_manifest_sha256
            || metadata.created_at != owner.created_at
            || canonical(&metadata.objective)? != canonical(&owner.objective)?
            || canonical(&metadata.limits)? != canonical(&owner.limits)?
        {
            return Err(integrity(
                "研究队列账本与所有权记录的 case、作者清单指纹或冻结目标不一致；保留现场并拒绝继续。",
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

    /// 取得写入锁；锁文件本身也是身份校验的一部分。
    pub(crate) fn lock(&mut self) -> Result<(), ProjectError> {
        let path = self.root.join(CONTROL).join(LOCK);
        let mut options = std::fs::OpenOptions::new();
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
                    thread::sleep(Duration::from_millis(2));
                }
                Err(failure) => {
                    return Err(crate::research_model::busy(format!(
                        "写入锁不可取得（{failure}）；未写入任何字节，请关闭另一个队列进程后重试。"
                    )));
                }
            }
        }
    }

    /// 从账本重建当前状态；计数只由事件流推导，不另存一份。
    pub(crate) fn load(&self) -> Result<StateRecord, ProjectError> {
        let mut statement = self
            .connection
            .prepare("SELECT payload_json FROM candidates ORDER BY position")
            .map_err(ProjectError::database)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(ProjectError::database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(ProjectError::database)?;
        let mut candidates = Vec::with_capacity(rows.len());
        for raw in rows {
            candidates.push(strict_record(raw.as_bytes())?);
        }
        let counters: Counters = {
            let mut statement = self
                .connection
                .prepare("SELECT payload_json FROM events ORDER BY sequence DESC LIMIT 1")
                .map_err(ProjectError::database)?;
            let mut rows = statement.query([]).map_err(ProjectError::database)?;
            match rows.next().map_err(ProjectError::database)? {
                Some(row) => {
                    let raw: String = row.get(0).map_err(ProjectError::database)?;
                    let payload: EventPayload = strict_record(raw.as_bytes())?;
                    payload.counters
                }
                None => Counters::default(),
            }
        };
        Ok(StateRecord {
            policy: self.metadata.policy.clone(),
            schema_version: self.metadata.schema_version,
            case_id: self.metadata.case_id.clone(),
            objective: self.metadata.objective.clone(),
            limits: self.metadata.limits.clone(),
            candidates,
            counters,
        })
    }

    pub(crate) fn event_count(&self) -> Result<u64, ProjectError> {
        let count: i64 = self
            .connection
            .query_row("SELECT count(*) FROM events", [], |row| row.get(0))
            .map_err(ProjectError::database)?;
        u64::try_from(count).map_err(|_| integrity("事件计数为负，账本不可信。"))
    }

    /// 把整份状态与一条事件一起落账；DELETE+INSERT 与事件在同一事务里。
    fn persist(
        &self,
        state: &StateRecord,
        kind: &str,
        round: &str,
        applied: &[Applied],
        at: &str,
    ) -> Result<(), ProjectError> {
        let payload = EventPayload {
            round: round.to_owned(),
            applied: applied.to_vec(),
            counters: state.counters.clone(),
        };
        let payload_json = canonical(&payload)?;
        let mut rows = Vec::with_capacity(state.candidates.len());
        for (position, candidate) in state.candidates.iter().enumerate() {
            rows.push((
                subject_key(&candidate.subject),
                i64::try_from(position).map_err(|_| integrity("候选序号溢出。"))?,
                candidate.state.clone(),
                canonical(candidate)?,
            ));
        }
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(ProjectError::database)?;
        transaction
            .execute("DELETE FROM candidates", [])
            .map_err(ProjectError::database)?;
        for (subject, position, state_text, json) in rows {
            transaction
                .execute(
                    "INSERT INTO candidates(subject,position,state,payload_json) VALUES(?1,?2,?3,?4)",
                    params![subject, position, state_text, json],
                )
                .map_err(ProjectError::database)?;
        }
        transaction
            .execute(
                "INSERT INTO events(kind,round,payload_json,at) VALUES(?1,?2,?3,?4)",
                params![kind, round, payload_json, at],
            )
            .map_err(ProjectError::database)?;
        transaction.commit().map_err(ProjectError::database)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 公开动词
// ---------------------------------------------------------------------------

pub fn adopt(author_path: &Path, root: &Path) -> Result<String, ProjectError> {
    let author_path = resolve_caller_path(author_path)?;
    project::checked_file(&author_path)?;
    let author_bytes = read_bounded(&author_path, MAX_BYTES)?;
    let author = Author::parse(&author_bytes)?;
    let author_manifest_sha256 = sha256(&author_bytes);
    let root = resolve_caller_path(root)?;
    if project::exists(&root)? {
        return Err(argument(format!(
            "研究队列根必须尚不存在：{}；未修改任何文件。",
            root.display()
        )));
    }
    let parent = root
        .parent()
        .ok_or_else(|| argument("研究队列根没有父目录；未创建任何文件。"))?
        .to_path_buf();
    if !project::exists(&parent)? {
        return Err(argument(format!(
            "研究队列根的父目录必须已存在：{}；未创建任何文件。",
            parent.display()
        )));
    }
    require_dir(&parent)?;
    let parent_identity = identity(&parent)?;
    fs::create_dir(&root).map_err(|error| ProjectError::io(&root, error))?;
    let created = identity(&root)?;
    match adopt_into(
        &root,
        &created,
        author,
        author_manifest_sha256,
        parent_identity,
        &author_path,
    ) {
        Ok(summary) => Ok(summary),
        Err(failure) => {
            // 只清理本次创建、且仍然是同一个目录的现场；父目录里既有内容永不删除。
            if let Ok(current) = identity(&root)
                && current == created
            {
                let _ = fs::remove_dir_all(&root);
            }
            Err(failure)
        }
    }
}

fn adopt_into(
    root: &Path,
    created: &Identity,
    author: Author,
    author_manifest_sha256: String,
    parent_identity: Identity,
    author_path: &Path,
) -> Result<String, ProjectError> {
    let root_path = fs::canonicalize(root).map_err(|error| ProjectError::io(root, error))?;
    let created_at = now()?;
    let control = root_path.join(CONTROL);
    fs::create_dir(&control).map_err(|error| ProjectError::io(&control, error))?;
    let control_identity = identity(&control)?;
    let lock_path = control.join(LOCK);
    let lock_file = open_new(&lock_path)?;
    lock_file
        .sync_all()
        .map_err(|error| ProjectError::io(&lock_path, error))?;
    let lock_identity = identity(&lock_path)?;
    drop(lock_file);
    let owner_path = control.join(OWNER);
    let owner_file = open_new(&owner_path)?;
    let owner_identity = identity(&owner_path)?;
    drop(owner_file);
    let database_path = control.join(DATABASE);
    let database_file = open_new(&database_path)?;
    let database_identity = identity(&database_path)?;
    drop(database_file);

    let state = StateRecord::new(&author, &created_at)?;
    let round = format!("adopt-{created_at}");
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
            "PRAGMA user_version={};\nPRAGMA application_id={};\n{SCHEMA}",
            SCHEMA_VERSION, APP
        ))
        .map_err(ProjectError::database)?;
    let metadata = Metadata {
        policy: POLICY.to_owned(),
        producer: PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: author.case_id.clone(),
        author_manifest_sha256: author_manifest_sha256.clone(),
        objective: author.objective.clone(),
        limits: author.limits.clone(),
        created_at: created_at.clone(),
    };
    connection
        .execute(
            "INSERT INTO metadata(singleton,schema_version,metadata_json) VALUES(1,?1,?2)",
            params![i64::from(SCHEMA_VERSION), canonical(&metadata)?],
        )
        .map_err(ProjectError::database)?;
    for (position, candidate) in state.candidates.iter().enumerate() {
        connection
            .execute(
                "INSERT INTO candidates(subject,position,state,payload_json) VALUES(?1,?2,?3,?4)",
                params![
                    subject_key(&candidate.subject),
                    i64::try_from(position).map_err(|_| integrity("候选序号溢出。"))?,
                    candidate.state.clone(),
                    canonical(candidate)?
                ],
            )
            .map_err(ProjectError::database)?;
    }
    let payload = EventPayload {
        round: round.clone(),
        applied: Vec::new(),
        counters: state.counters.clone(),
    };
    connection
        .execute(
            "INSERT INTO events(kind,round,payload_json,at) VALUES(?1,?2,?3,?4)",
            params![EVENT_ADOPT, round, canonical(&payload)?, created_at],
        )
        .map_err(ProjectError::database)?;
    drop(connection);

    let owner = Owner {
        policy: POLICY.to_owned(),
        producer: PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: author.case_id.clone(),
        root: root_path.to_string_lossy().into_owned(),
        root_identity: created.clone(),
        parent_identity,
        control_identity,
        owner_identity,
        database_identity,
        lock_identity,
        author_manifest_sha256: author_manifest_sha256.clone(),
        objective: author.objective.clone(),
        limits: author.limits.clone(),
        created_at: created_at.clone(),
    };
    crate::vault_store::write_replace(&owner_path, canonical(&owner)?.as_bytes())?;

    // 自检：刚建立的根必须能被只读打开，且候选数、计数与授权集合都符合预期。
    let store = ResearchStore::open_readonly(&root_path)?;
    let reloaded = store.load()?;
    if reloaded.candidates.len() != state.candidates.len()
        || reloaded.counters != state.counters
        || reloaded.must_do() != state.must_do()
        || store.event_count()? != 1
        || canonical(&reloaded.objective)? != canonical(&author.objective)?
    {
        return Err(integrity(
            "研究队列根建立后自检不一致；保留现场并拒绝继续。",
        ));
    }

    let totals = reloaded.totals();
    Ok(format!(
        "研究队列 {}：已建立（policy={POLICY}，结构版本 {SCHEMA_VERSION}）\n作者包：{}（SHA256 {author_manifest_sha256}）\n本轮目标：{}\n必做集合：{} 项（授权目标 {}、必需前提 {}）；只由 adopt 冻结，之后不接受增删\n候选队列：{} 项（越界主题 {} 项只进队列）\n检索与模型调用：0 次（adopt 只读作者包，不联网、不调用模型）",
        root_path.display(),
        author_path.display(),
        reloaded.objective.summary,
        reloaded.must_do().len(),
        reloaded.objective.authorized_targets.len(),
        reloaded.objective.required_prerequisites.len(),
        totals.get("total").copied().unwrap_or_default(),
        totals.get("out_of_scope").copied().unwrap_or_default(),
    ))
}

fn read_file(path: &Path) -> Result<Vec<u8>, ProjectError> {
    let path = resolve_caller_path(path)?;
    project::checked_file(&path)?;
    read_bounded(&path, MAX_BYTES)
}

pub fn observe(root: &Path, file: &Path) -> Result<String, ProjectError> {
    let bytes = read_file(file)?;
    let input = ObservationFile::parse(&bytes)?;
    let mut store = ResearchStore::open(root)?;
    store.lock()?;
    let mut state = store.load()?;
    let at = now()?;
    let applied = state.apply_observations(&input, &at)?;
    store.persist(&state, EVENT_OBSERVATION, &input.round, &applied, &at)?;
    Ok(render_applied(&input.round, &applied, &state))
}

pub fn decide(root: &Path, file: &Path) -> Result<String, ProjectError> {
    let bytes = read_file(file)?;
    let input = DecisionFile::parse(&bytes)?;
    let mut store = ResearchStore::open(root)?;
    store.lock()?;
    let mut state = store.load()?;
    let at = now()?;
    let applied = state.apply_decisions(&input.decisions, &at)?;
    store.persist(&state, EVENT_DECISION, &input.round, &applied, &at)?;
    Ok(render_applied(&input.round, &applied, &state))
}

fn counters_json(counters: &Counters) -> Value {
    json!({
        "observations": counters.observations,
        "blocked_repeats": counters.blocked_repeats,
        "reopenings": counters.reopenings,
        "merges": counters.merges,
        "decisions": counters.decisions,
    })
}

fn render_applied(round: &str, applied: &[Applied], state: &StateRecord) -> String {
    let value = json!({
        "round": round,
        "applied": applied
            .iter()
            .map(|item| json!({
                "subject": item.subject,
                "outcome": item.outcome,
                "detail": item.detail,
            }))
            .collect::<Vec<_>>(),
        "candidates": state.totals(),
        "counters": counters_json(&state.counters),
        "must_do": state.must_do().into_iter().collect::<Vec<_>>(),
        "queue_only": state.queue_only().into_iter().collect::<Vec<_>>(),
        "retrieval_calls": 0,
        "model_calls": 0,
    });
    serde_json::to_string_pretty(&value)
        .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"))
}

pub fn list(root: &Path) -> Result<String, ProjectError> {
    let store = ResearchStore::open_readonly(root)?;
    let state = store.load()?;
    let candidates: Vec<Value> = state
        .candidates
        .iter()
        .map(|candidate| {
            json!({
                "subject": candidate.subject,
                "scope_class": candidate.scope_class,
                "state": candidate.state,
                "reason": candidate.reason,
                "relation": candidate.relation,
                "benefit": candidate.benefit,
                "source_leads": candidate.source_leads,
                "estimated_cost": candidate.estimated_cost,
                "depends_on": candidate.depends_on,
                "scope_note": candidate.scope_note,
                "evidence": candidate
                    .evidence
                    .iter()
                    .map(|item| json!({"round": item.round, "at": item.at, "note": item.note, "leads": item.leads}))
                    .collect::<Vec<_>>(),
                "rejections": candidate
                    .rejections
                    .iter()
                    .map(|item| json!({"at": item.at, "reason": item.reason}))
                    .collect::<Vec<_>>(),
                "scope_revisions": candidate
                    .scope_revisions
                    .iter()
                    .map(|item| json!({"at": item.at, "reason": item.reason, "before": item.before, "after": item.after}))
                    .collect::<Vec<_>>(),
                "merged_into": candidate.merged_into,
            })
        })
        .collect();
    let value = json!({
        "case_id": state.case_id,
        "objective": {
            "summary": state.objective.summary,
            "authorized_targets": state.objective.authorized_targets,
            "required_prerequisites": state.objective.required_prerequisites,
        },
        "must_do": state.must_do().into_iter().collect::<Vec<_>>(),
        "queue_only": state.queue_only().into_iter().collect::<Vec<_>>(),
        "counters": counters_json(&state.counters),
        "candidates": candidates,
        "retrieval_calls": 0,
        "model_calls": 0,
    });
    Ok(serde_json::to_string_pretty(&value)
        .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}")))
}

pub fn status(root: &Path) -> Result<String, ProjectError> {
    let store = ResearchStore::open_readonly(root)?;
    let state = store.load()?;
    let totals = state.totals();
    let rejected: usize = state
        .candidates
        .iter()
        .map(|candidate| candidate.rejections.len())
        .sum();
    let mut lines = Vec::new();
    lines.push(format!(
        "研究队列 {}：已建立（policy={}，结构版本 {}）",
        store.root.display(),
        store.metadata.policy,
        store.metadata.schema_version
    ));
    lines.push(format!(
        "本轮目标：{}（case {}）",
        state.objective.summary, state.case_id
    ));
    lines.push(format!(
        "所有权：作者清单 SHA256 {}；adopt 于 {}",
        store.metadata.author_manifest_sha256, store.metadata.created_at
    ));
    lines.push(format!(
        "必做集合：{} 项（授权目标 {}、必需前提 {}）；只由 adopt 冻结，之后不接受增删",
        state.must_do().len(),
        state.objective.authorized_targets.len(),
        state.objective.required_prerequisites.len()
    ));
    lines.push(format!(
        "候选队列：{} 项（proposed {}、accepted {}、rejected {}、deferred {}、merged {}）",
        totals.get("total").copied().unwrap_or_default(),
        totals.get("proposed").copied().unwrap_or_default(),
        totals.get("accepted").copied().unwrap_or_default(),
        totals.get("rejected").copied().unwrap_or_default(),
        totals.get("deferred").copied().unwrap_or_default(),
        totals.get("merged").copied().unwrap_or_default(),
    ));
    lines.push(format!(
        "范围分布：授权目标内 {} 项、必需前提 {} 项、越界主题 {} 项（越界只进队列，不进入本轮必做集合）",
        totals
            .get("authorized_target")
            .copied()
            .unwrap_or_default(),
        totals
            .get("required_prerequisite")
            .copied()
            .unwrap_or_default(),
        totals.get("out_of_scope").copied().unwrap_or_default(),
    ));
    lines.push(format!(
        "拒绝记忆：{rejected} 条；无新理由重提被挡回 {} 次；带新理由重入 {} 次；合并 {} 次；接受/拒绝/延后/改范围共 {} 次",
        state.counters.blocked_repeats,
        state.counters.reopenings,
        state.counters.merges,
        state.counters.decisions,
    ));
    lines.push(format!(
        "事件：{} 条（adopt 1 条 + 观测 {} 次 + 决定 {} 次）",
        store.event_count()?,
        state.counters.observations,
        state.counters.decisions,
    ));
    lines.push(
        "检索与模型调用：0 次（本目录从不联网、不调用模型；候选只是线索，不是已验证知识）"
            .to_owned(),
    );
    lines.push(
        "四维状态：研究候选单列，不计入主任务的完成或失败；未批准候选不改变本轮必做集合。"
            .to_owned(),
    );
    Ok(lines.join("\n"))
}
