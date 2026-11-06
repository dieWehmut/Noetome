//! Bounded execution journal. The output database remains the publication authority.
use crate::{
    compile_identity::identity,
    compile_store::Store,
    fixture_input::LoadedCase,
    project::{self, ProjectError},
    run_model::*,
    text::sha256,
};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

const DB: &str = "control/run.sqlite";
const OWNER: &str = "control/owner.json";
const LOCK: &str = "control/worker.lock";
const APP: i64 = 0x4e52554e;
const SCHEMA: &str = "
CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL CHECK(schema_version=1),metadata_json TEXT NOT NULL);
CREATE TABLE checkpoints(sequence INTEGER PRIMARY KEY CHECK(sequence>0),previous_sha256 TEXT NOT NULL,payload_json TEXT NOT NULL,payload_sha256 TEXT NOT NULL);
CREATE TABLE controls(sequence INTEGER PRIMARY KEY CHECK(sequence>0),run_id TEXT NOT NULL,action TEXT NOT NULL CHECK(action IN ('pause','cancel')),requested_at TEXT NOT NULL);";

pub(crate) fn error(code: &'static str, text: impl Into<String>) -> ProjectError {
    ProjectError::new(code, 3, text)
}
fn db_error(e: rusqlite::Error) -> ProjectError {
    error(
        "RUN_DATABASE_ERROR",
        format!("运行日志事务失败，保留现场：{e}"),
    )
}
fn encode<T: serde::Serialize>(v: &T) -> Result<String, ProjectError> {
    serde_json::to_string(v).map_err(|e| error("RUN_SCHEMA", e.to_string()))
}
fn decode<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, ProjectError> {
    serde_json::from_str(s).map_err(|e| error("RUN_SCHEMA", e.to_string()))
}
pub(crate) fn now() -> Result<String, ProjectError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| error("RUN_TIME", e.to_string()))
}
fn utc(s: &str) -> bool {
    OffsetDateTime::parse(s, &Rfc3339).is_ok_and(|t| t.offset() == UtcOffset::UTC)
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn ordinary(path: &Path, writable: bool) -> Result<File, ProjectError> {
    project::check_components(path)?;
    project::checked_file(path)?;
    let before = identity(path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(writable);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x00200000);
    }
    let file = options.open(path).map_err(|e| ProjectError::io(path, e))?;
    if before != identity(path)? {
        return Err(error("RUN_INTEGRITY", "打开期间文件身份改变。"));
    }
    Ok(file)
}
fn create_file(path: &Path, bytes: &[u8]) -> Result<File, ProjectError> {
    project::check_components(path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x00200000);
    }
    let mut file = options.open(path).map_err(|e| ProjectError::io(path, e))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| ProjectError::io(path, e))?;
    Ok(file)
}
fn read_small(path: &Path) -> Result<Vec<u8>, ProjectError> {
    let mut file = project::read_guard(path)?;
    if file
        .metadata()
        .map_err(|e| ProjectError::io(path, e))?
        .len()
        > 8 * 1024 * 1024
    {
        return Err(error("RUN_LIMIT", "运行元数据超过8MiB。"));
    }
    let mut bytes = vec![];
    Read::by_ref(&mut file)
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ProjectError::io(path, e))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(error("RUN_LIMIT", "运行元数据超过8MiB。"));
    }
    Ok(bytes)
}
fn stamp(path: &Path) -> Result<FileStamp, ProjectError> {
    let id = identity(path)?;
    let mut file = project::read_guard(path)?;
    let before = file.metadata().map_err(|e| ProjectError::io(path, e))?;
    if before.len() > 64 * 1024 * 1024 {
        return Err(error("RUN_LIMIT", "选取输入超过64MiB。"));
    }
    let mut bytes = vec![];
    Read::by_ref(&mut file)
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ProjectError::io(path, e))?;
    let after = file.metadata().map_err(|e| ProjectError::io(path, e))?;
    if before.len() != bytes.len() as u64
        || before.modified().ok() != after.modified().ok()
        || id != identity(path)?
    {
        return Err(error("RUN_INPUT_CHANGED", "选取输入在读取时改变。"));
    }
    let modified = before
        .modified()
        .map_err(|e| ProjectError::io(path, e))?
        .duration_since(UNIX_EPOCH)
        .map_err(|_| error("RUN_TIME", "输入修改时间早于epoch。"))?
        .as_nanos()
        .to_string();
    Ok(FileStamp {
        identity: id,
        sha256: sha256(&bytes),
        byte_length: bytes.len() as u64,
        modified,
    })
}
pub(crate) fn inputs(case: &LoadedCase) -> Result<BTreeMap<String, FileStamp>, ProjectError> {
    let paths = [
        "case.json".to_owned(),
        case.manifest.ir_file.clone(),
        case.manifest.note_plan_file.clone(),
        case.manifest.expected_file.clone(),
    ]
    .into_iter()
    .chain(
        case.manifest
            .sources
            .iter()
            .map(|s| s.relative_path.clone()),
    );
    let mut out = BTreeMap::new();
    for p in paths {
        if out
            .insert(p.clone(), stamp(&case.root().join(&p))?)
            .is_some()
        {
            return Err(error("RUN_SCHEMA", "重复选取输入。"));
        }
    }
    let mut expected = BTreeMap::from([
        ("case.json".to_owned(), case.case_sha256.clone()),
        (case.manifest.ir_file.clone(), case.ir_sha256.clone()),
        (
            case.manifest.note_plan_file.clone(),
            case.plan_sha256.clone(),
        ),
        (
            case.manifest.expected_file.clone(),
            case.expected_sha256.clone(),
        ),
    ]);
    for source in &case.manifest.sources {
        expected.insert(source.relative_path.clone(), source.content_sha256.clone());
    }
    if out.len() != expected.len()
        || expected
            .iter()
            .any(|(p, hash)| out.get(p).is_none_or(|stamp| &stamp.sha256 != hash))
    {
        return Err(error(
            "RUN_INPUT_CHANGED",
            "实际输入字节与本次已加载来源契约不符。",
        ));
    }
    case.check_unchanged()?;
    Ok(out)
}

pub(crate) struct Journal {
    root: PathBuf,
    pub(crate) metadata: RunMetadata,
    pub(crate) state: RunState,
    pub(crate) sequence: u64,
    last_hash: String,
    connection: Connection,
    _database_guard: File,
    _owner_guard: File,
    worker: File,
    owner_bytes: Vec<u8>,
    locked: Cell<bool>,
}
impl Journal {
    pub(crate) fn create(
        case: &LoadedCase,
        root: &Path,
        context: crate::compile_model::CompileContext,
        paused: bool,
    ) -> Result<Self, ProjectError> {
        let root = Store::preflight(case.root(), root)?;
        let root_text = root
            .to_str()
            .ok_or_else(|| error("RUN_PATH", "路径须有效UTF8。"))?
            .to_owned();
        let case_text = case
            .root()
            .to_str()
            .ok_or_else(|| error("RUN_PATH", "路径须有效UTF8。"))?
            .to_owned();
        let selected = inputs(case)?;
        fs::create_dir(&root).map_err(|e| ProjectError::io(&root, e))?;
        fs::create_dir(root.join("control")).map_err(|e| ProjectError::io(&root, e))?;
        let database = create_file(&root.join(DB), b"")?;
        let worker = create_file(&root.join(LOCK), b"")?;
        let owner = Owner {
            schema_version: 1,
            producer: "fixture_run".into(),
            run_id: context.run_id.clone(),
            release_id: context.release_id.clone(),
            root: root_text,
            case_root: case_text,
            root_identity: identity(&root)?,
            parent_identity: identity(root.parent().unwrap())?,
            control_identity: identity(&root.join("control"))?,
            database_identity: identity(&root.join(DB))?,
            lock_identity: identity(&root.join(LOCK))?,
            case_identity: identity(case.root())?,
            compiler_fingerprint: env!("NOETOME_RUN_COMPILER_FINGERPRINT").into(),
            policy_version: POLICY.into(),
        };
        let owner_bytes = format!("{}\n", encode(&owner)?).into_bytes();
        drop(create_file(&root.join(OWNER), &owner_bytes)?);
        let owner_guard = project::read_guard(&root.join(OWNER))?;
        let metadata = RunMetadata {
            owner,
            owner_file_identity: identity(&root.join(OWNER))?,
            context,
            inputs: selected,
        };
        let mut connection =
            Connection::open_with_flags(root.join(DB), OpenFlags::SQLITE_OPEN_READ_WRITE)
                .map_err(db_error)?;
        configure(&connection)?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        tx.execute_batch(SCHEMA).map_err(db_error)?;
        tx.execute_batch(&format!(
            "PRAGMA user_version=1; PRAGMA application_id={APP};"
        ))
        .map_err(db_error)?;
        tx.execute("INSERT INTO metadata VALUES(1,1,?1)", [encode(&metadata)?])
            .map_err(db_error)?;
        let state = RunState::initial(paused);
        let cp = Checkpoint {
            schema_version: 1,
            sequence: 1,
            event: "created".into(),
            at: now()?,
            run_id: metadata.owner.run_id.clone(),
            state: state.clone(),
        };
        let raw = encode(&cp)?;
        let hash = sha256(raw.as_bytes());
        tx.execute(
            "INSERT INTO checkpoints VALUES(1,'',?1,?2)",
            params![raw, hash],
        )
        .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        let journal = Self {
            root,
            metadata,
            state,
            sequence: 1,
            last_hash: hash,
            connection,
            _database_guard: database,
            _owner_guard: owner_guard,
            worker,
            owner_bytes,
            locked: Cell::new(false),
        };
        journal.verify()?;
        Ok(journal)
    }
    pub(crate) fn open(root: &Path, writable: bool) -> Result<Self, ProjectError> {
        Self::open_inner(root, writable, None)
    }
    pub(crate) fn open_for_resume(root: &Path, case: &Path) -> Result<Self, ProjectError> {
        Self::open_inner(root, true, Some(case))
    }
    fn open_inner(
        root: &Path,
        writable: bool,
        recovery_case: Option<&Path>,
    ) -> Result<Self, ProjectError> {
        let root = project::check_components(root)?
            .canonicalize()
            .map_err(|e| ProjectError::io(root, e))?;
        let root_text = root
            .to_str()
            .ok_or_else(|| error("RUN_PATH", "路径须有效UTF8。"))?;
        let owner_bytes = read_small(&root.join(OWNER))?;
        let owner: Owner =
            serde_json::from_slice(&owner_bytes).map_err(|e| error("RUN_SCHEMA", e.to_string()))?;
        if owner.schema_version != 1
            || owner.producer != "fixture_run"
            || owner.root != root_text
            || owner.policy_version != POLICY
        {
            return Err(error("RUN_OWNER", "不是当前协议明确拥有的运行根。"));
        }
        if let Some(case) = recovery_case {
            check_requested_case(case, &owner)?;
        }
        let hot =
            project::exists(&root.join("control/run.sqlite-journal"))? && recovery_case.is_some();
        verify_owner_layout(&root, &owner, &owner_bytes, hot)?;
        let worker = ordinary(&root.join(LOCK), writable)?;
        if hot {
            worker
                .try_lock()
                .map_err(|e| error("RUN_BUSY", format!("恢复前不能取得独占运行锁：{e}")))?;
            let witness = crate::run_recovery::RecoveryWitness::capture(
                &root.join(DB),
                &root.join("control/run.sqlite-journal"),
            )?;
            crate::run_recovery::validate_hot_journal(
                &root.join(DB),
                &root.join("control/run.sqlite-journal"),
            )?;
            verify_owner_layout(&root, &owner, &owner_bytes, true)?;
            witness.verify(&root.join(DB), &root.join("control/run.sqlite-journal"))?;
            // The read-only original image is now bound. Only SQLite performs
            // the rollback, while this worker lock remains exclusively held.
            let recovery =
                Connection::open_with_flags(root.join(DB), OpenFlags::SQLITE_OPEN_READ_WRITE)
                    .map_err(db_error)?;
            let quick: String = recovery
                .query_row("PRAGMA quick_check", [], |r| r.get(0))
                .map_err(db_error)?;
            if quick != "ok" {
                return Err(error("RUN_INTEGRITY", "SQLite恢复后完整性检查失败。"));
            }
            drop(recovery);
            verify_owner(&root, &owner, &owner_bytes)?;
        }
        let database = ordinary(&root.join(DB), writable)?;
        let owner_guard = project::read_guard(&root.join(OWNER))?;
        let connection = Connection::open_with_flags(
            root.join(DB),
            if writable {
                OpenFlags::SQLITE_OPEN_READ_WRITE
            } else {
                OpenFlags::SQLITE_OPEN_READ_ONLY
            },
        )
        .map_err(db_error)?;
        configure(&connection)?;
        schema(&connection)?;
        let raw: String = connection
            .query_row(
                "SELECT metadata_json FROM metadata WHERE singleton=1 AND schema_version=1",
                [],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        let metadata: RunMetadata = decode(&raw)?;
        if metadata.owner != owner
            || identity(&root.join(OWNER))? != metadata.owner_file_identity
            || metadata.context.run_id != owner.run_id
            || metadata.context.release_id != owner.release_id
            || metadata.inputs.len() < 4
        {
            return Err(error("RUN_INTEGRITY", "元数据与原所有权不符。"));
        }
        let (sequence, last_hash, state) = history(&connection, &owner.run_id)?;
        let result = Self {
            root,
            metadata,
            state,
            sequence,
            last_hash,
            connection,
            _database_guard: database,
            _owner_guard: owner_guard,
            worker,
            owner_bytes,
            locked: Cell::new(hot),
        };
        result.verify()?;
        Ok(result)
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn acquire(&self) -> Result<(), ProjectError> {
        if self.try_acquire() {
            Ok(())
        } else {
            Err(error("RUN_BUSY", "已有执行者或无法取得运行锁。"))
        }
    }
    pub(crate) fn try_acquire(&self) -> bool {
        if self.locked.get() {
            true
        } else if self.worker.try_lock().is_ok() {
            self.locked.set(true);
            true
        } else {
            false
        }
    }
    pub(crate) fn reload(&mut self) -> Result<(), ProjectError> {
        verify_owner(&self.root, &self.metadata.owner, &self.owner_bytes)?;
        schema(&self.connection)?;
        let (n, hash, state) = history(&self.connection, &self.metadata.owner.run_id)?;
        self.sequence = n;
        self.last_hash = hash;
        self.state = state;
        self.verify()
    }
    pub(crate) fn verify(&self) -> Result<(), ProjectError> {
        verify_owner(&self.root, &self.metadata.owner, &self.owner_bytes)?;
        schema(&self.connection)?;
        let raw: String = self
            .connection
            .query_row(
                "SELECT metadata_json FROM metadata WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if decode::<RunMetadata>(&raw)? != self.metadata
            || identity(&self.root.join(OWNER))? != self.metadata.owner_file_identity
        {
            return Err(error(
                "RUN_INTEGRITY",
                "持久元数据或所有权文件在已持有期间被外部改变。",
            ));
        }
        let (n, hash, state) = history(&self.connection, &self.metadata.owner.run_id)?;
        if n != self.sequence || hash != self.last_hash || state != self.state {
            return Err(error("RUN_INTEGRITY", "持久检查点在已持有期间被外部改变。"));
        }
        Ok(())
    }
    pub(crate) fn verify_inputs(&self, case: &LoadedCase) -> Result<(), ProjectError> {
        let owner = &self.metadata.owner;
        if case.root().to_string_lossy() != owner.case_root
            || identity(case.root())? != owner.case_identity
            || inputs(case)? != self.metadata.inputs
            || owner.compiler_fingerprint != env!("NOETOME_RUN_COMPILER_FINGERPRINT")
        {
            return Err(error(
                "RUN_INPUT_INVALIDATED",
                "输入、编译版本或根身份已改变，依赖结果失效；请为新版本创建新的运行，保留原现场。",
            ));
        }
        let current = crate::fixture_compile::context(case)?;
        let previous = &self.metadata.context;
        if current.case_id != previous.case_id
            || current.fixture_scope_id != previous.fixture_scope_id
            || current.input_hashes != previous.input_hashes
            || current.versions != previous.versions
            || current.counts != previous.counts
            || current.ir_revision != previous.ir_revision
            || current.plan_revision != previous.plan_revision
        {
            return Err(error("RUN_INPUT_INVALIDATED", "输入与策略版本失配。"));
        }
        Ok(())
    }
    pub(crate) fn append(&mut self, event: &str, state: RunState) -> Result<(), ProjectError> {
        self.verify()?;
        if self.sequence >= 512 {
            return Err(error("RUN_LIMIT", "检查点达到512上限，保留已确认结果。"));
        }
        validate_state(&state)?;
        if self.sequence > 0 {
            transition(&self.state, &state, event)?;
        }
        let cp = Checkpoint {
            schema_version: 1,
            sequence: self.sequence + 1,
            event: event.into(),
            at: now()?,
            run_id: self.metadata.owner.run_id.clone(),
            state: state.clone(),
        };
        let raw = encode(&cp)?;
        if raw.len() > 8 * 1024 * 1024 {
            return Err(error("RUN_LIMIT", "检查点超过8MiB。"));
        }
        let hash = sha256(raw.as_bytes());
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        journal_original_metadata(&tx)?;
        tx.execute(
            "INSERT INTO checkpoints VALUES(?1,?2,?3,?4)",
            params![cp.sequence as i64, self.last_hash, raw, hash],
        )
        .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        self.sequence = cp.sequence;
        self.last_hash = hash;
        self.state = state;
        Ok(())
    }
    pub(crate) fn latest_control(&self) -> Result<(u64, Option<String>), ProjectError> {
        let controls = control_chain(&self.connection, &self.metadata.owner.run_id)?;
        Ok((
            controls.len() as u64,
            effective_control(&controls).map(str::to_owned),
        ))
    }
    pub(crate) fn request(&mut self, action: &str) -> Result<u64, ProjectError> {
        self.request_with_before_transaction(action, || {})
    }
    pub(crate) fn request_with_before_transaction(
        &mut self,
        action: &str,
        before_transaction: impl FnOnce(),
    ) -> Result<u64, ProjectError> {
        self.reload()?;
        if matches!(
            self.state.status,
            RunStatus::Cancelled
                | RunStatus::Succeeded
                | RunStatus::Invalidated
                | RunStatus::Failed
        ) {
            return Err(error("RUN_TERMINAL", "终态运行不接受新控制。"));
        }
        if !matches!(action, "pause" | "cancel") {
            return Err(error("RUN_SCHEMA", "未知控制。"));
        }
        before_transaction();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(db_error)?;
        // Another worker may finish after the earlier read. Serialize the
        // final admission check with checkpoint writers before any mutation.
        let (_, _, current) = history(&tx, &self.metadata.owner.run_id)?;
        if matches!(
            current.status,
            RunStatus::Cancelled
                | RunStatus::Succeeded
                | RunStatus::Invalidated
                | RunStatus::Failed
        ) {
            return Err(error(
                "RUN_TERMINAL",
                "取得写事务时运行已结束，未写入控制请求。",
            ));
        }
        journal_original_metadata(&tx)?;
        let n: u32 = tx
            .query_row(
                "SELECT coalesce(max(sequence),0)+1 FROM controls",
                [],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if n > 128 {
            return Err(error("RUN_LIMIT", "控制请求达到128上限。"));
        }
        tx.execute(
            "INSERT INTO controls VALUES(?1,?2,?3,?4)",
            params![n, self.metadata.owner.run_id, action, now()?],
        )
        .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        Ok(u64::from(n))
    }
}
fn journal_original_metadata(tx: &rusqlite::Transaction<'_>) -> Result<(), ProjectError> {
    // Back up the original owned record, including overflow pages, before any
    // other mutation. The final raw JSON remains exactly unchanged. A hot
    // journal without complete original metadata provenance is never adopted.
    let original: String = tx
        .query_row(
            "SELECT metadata_json FROM metadata WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .map_err(db_error)?;
    tx.execute("UPDATE metadata SET metadata_json=zeroblob(length(CAST(metadata_json AS BLOB))) WHERE singleton=1",[]).map_err(db_error)?;
    tx.execute(
        "UPDATE metadata SET metadata_json=?1 WHERE singleton=1",
        [original],
    )
    .map_err(db_error)?;
    Ok(())
}
fn configure(c: &Connection) -> Result<(), ProjectError> {
    c.busy_timeout(Duration::from_millis(250))
        .map_err(db_error)?;
    c.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA temp_store=MEMORY;")
        .map_err(db_error)
}
fn schema(c: &Connection) -> Result<(), ProjectError> {
    schema_for(c, false)
}
pub(crate) fn validate_recovery_chain(c: &Connection, id: &str) -> Result<(), ProjectError> {
    schema_for(c, true)?;
    history(c, id)?;
    Ok(())
}
pub(crate) fn recovery_state(c: &Connection, id: &str) -> Result<RunState, ProjectError> {
    schema_for(c, true)?;
    Ok(history(c, id)?.2)
}
fn schema_for(c: &Connection, shadow: bool) -> Result<(), ProjectError> {
    if !c.is_autocommit() {
        return Err(error("RUN_RECOVERY_REQUIRED", "日志尚有活动事务。"));
    }
    let app: i64 = c
        .query_row("PRAGMA application_id", [], |r| r.get(0))
        .map_err(db_error)?;
    let version: i64 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(db_error)?;
    let journal: String = c
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .map_err(db_error)?;
    if app != APP || version != 1 || (journal != "delete" && !(shadow && journal == "memory")) {
        return Err(error("RUN_SCHEMA", "运行日志版本或SQLite策略不符。"));
    }
    let expected: BTreeMap<_, _> = SCHEMA
        .split(';')
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            (
                s.split_whitespace()
                    .nth(2)
                    .unwrap_or("")
                    .split('(')
                    .next()
                    .unwrap_or("")
                    .to_string(),
                compact(s),
            )
        })
        .collect();
    let mut actual = BTreeMap::new();
    let mut stmt = c
        .prepare("SELECT type,name,sql FROM sqlite_schema ORDER BY name")
        .map_err(db_error)?;
    for row in stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(db_error)?
    {
        let (kind, name, sql) = row.map_err(db_error)?;
        if kind != "table" {
            return Err(error("RUN_SCHEMA", "未知索引、视图或触发器。"));
        }
        actual.insert(
            name,
            compact(&sql.ok_or_else(|| error("RUN_SCHEMA", "缺少schema定义。"))?),
        );
    }
    if actual != expected {
        return Err(error("RUN_SCHEMA", "完整运行日志schema不符。"));
    }
    let n: i64 = c
        .query_row("SELECT count(*) FROM metadata", [], |r| r.get(0))
        .map_err(db_error)?;
    if n != 1 {
        return Err(error("RUN_SCHEMA", "元数据非单例。"));
    }
    Ok(())
}
fn verify_owner(root: &Path, o: &Owner, bytes: &[u8]) -> Result<(), ProjectError> {
    verify_owner_layout(root, o, bytes, false)
}
fn verify_owner_layout(
    root: &Path,
    o: &Owner,
    bytes: &[u8],
    hot: bool,
) -> Result<(), ProjectError> {
    project::check_components(root)?;
    if identity(root)? != o.root_identity
        || identity(
            root.parent()
                .ok_or_else(|| error("RUN_PATH", "没有父目录。"))?,
        )? != o.parent_identity
        || identity(&root.join("control"))? != o.control_identity
        || identity(&root.join(DB))? != o.database_identity
        || identity(&root.join(LOCK))? != o.lock_identity
        || fs::metadata(root.join(LOCK))
            .map_err(|e| ProjectError::io(&root.join(LOCK), e))?
            .len()
            != 0
        || read_small(&root.join(OWNER))? != bytes
    {
        return Err(error(
            "RUN_OWNER",
            "运行根、父目录、控制文件或原所有权已改变。",
        ));
    }
    let top: BTreeSet<_> = fs::read_dir(root)
        .map_err(|e| ProjectError::io(root, e))?
        .map(|r| r.map(|e| e.file_name()))
        .collect::<Result<_, _>>()
        .map_err(|e| ProjectError::io(root, e))?;
    let base: BTreeSet<_> = ["control"].iter().map(std::ffi::OsString::from).collect();
    let with_output: BTreeSet<_> = ["control", "output"]
        .iter()
        .map(std::ffi::OsString::from)
        .collect();
    if top != base && top != with_output {
        return Err(error("RUN_OWNER", "运行根有未知内容。"));
    }
    let control = root.join("control");
    let actual: BTreeSet<_> = fs::read_dir(&control)
        .map_err(|e| ProjectError::io(&control, e))?
        .map(|r| r.map(|e| e.file_name()))
        .collect::<Result<_, _>>()
        .map_err(|e| ProjectError::io(&control, e))?;
    let mut expected: BTreeSet<_> = ["run.sqlite", "owner.json", "worker.lock"]
        .iter()
        .map(std::ffi::OsString::from)
        .collect();
    if hot {
        expected.insert("run.sqlite-journal".into());
    }
    if actual != expected {
        return Err(error(
            "RUN_RECOVERY_REQUIRED",
            "控制目录有未知内容或SQLite未完成侧文件，保留现场。",
        ));
    }
    Ok(())
}
pub(crate) fn check_requested_case(case: &Path, owner: &Owner) -> Result<(), ProjectError> {
    let requested = project::check_components(case)?;
    if requested != Path::new(&owner.case_root)
        && requested.canonicalize().ok().as_deref() != Some(Path::new(&owner.case_root))
    {
        return Err(error(
            "RUN_CASE_MISMATCH",
            "继续命令必须指向原先明确选取的fixture根；错误参数未改变原运行。",
        ));
    }
    Ok(())
}
fn control_chain(c: &Connection, id: &str) -> Result<Vec<String>, ProjectError> {
    let mut statement = c
        .prepare("SELECT sequence,run_id,action,requested_at FROM controls ORDER BY sequence")
        .map_err(db_error)?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                u64::from(r.get::<_, u32>(0)?),
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(db_error)?;
    let mut actions = Vec::new();
    for row in rows {
        let (seq, run_id, action, at) = row.map_err(db_error)?;
        if seq != actions.len() as u64 + 1
            || seq > 128
            || run_id != id
            || !matches!(action.as_str(), "pause" | "cancel")
            || !utc(&at)
        {
            return Err(error("RUN_SCHEMA", "控制请求链无效。"));
        }
        actions.push(action);
    }
    Ok(actions)
}
fn effective_control(actions: &[String]) -> Option<&str> {
    if actions.iter().any(|a| a == "cancel") {
        Some("cancel")
    } else {
        actions.last().map(String::as_str)
    }
}
fn validate_state(s: &RunState) -> Result<(), ProjectError> {
    if s.schema_version != 1 || s.units.len() != 6 || s.acknowledged_control > 128 {
        return Err(error("RUN_SCHEMA", "工作图或版本不符。"));
    }
    let confirmed = s.confirmed();
    if s.pending_intent.is_some()
        && (confirmed >= 6
            || s.units[confirmed].state != UnitStatus::Running
            || s.units[confirmed].attempts == 0)
    {
        return Err(error("RUN_SCHEMA", "pending intent没有对应在途单元。"));
    }
    if confirmed >= 1 && s.output_snapshot.is_none() {
        return Err(error("RUN_SCHEMA", "确认单元缺少持久输出依据。"));
    }
    if confirmed >= 2
        && (s.manifest.is_empty()
            || s.output_snapshot
                .as_ref()
                .is_none_or(|snap| snap.manifest != s.manifest))
    {
        return Err(error("RUN_SCHEMA", "确认渲染缺少完整清单。"));
    }
    if confirmed >= 1
        && ["sources", "coverage", "knowledge", "note_plan", "expected"]
            .iter()
            .any(|name| {
                s.checks
                    .get(*name)
                    .is_none_or(|check| check.status != "passed" || check.issue_count != 0)
            })
    {
        return Err(error("RUN_SCHEMA", "来源确认缺少真实契约检查。"));
    }
    if confirmed >= 5 && s.validation_report.is_none() {
        return Err(error("RUN_SCHEMA", "确认报告缺少事实原件。"));
    }
    if confirmed == 6
        && (s
            .output_snapshot
            .as_ref()
            .is_none_or(|snap| !snap.published)
            || ["rendering", "staging_output", "vault_output"]
                .iter()
                .any(|name| {
                    s.checks
                        .get(*name)
                        .is_none_or(|check| check.status != "passed" || check.issue_count != 0)
                }))
    {
        return Err(error("RUN_SCHEMA", "确认发布缺少完整检查和事务依据。"));
    }
    for (i, u) in s.units.iter().enumerate() {
        let deps = if i == 0 {
            vec![]
        } else {
            vec![UNITS[i - 1].to_owned()]
        };
        if u.unit_id != UNITS[i]
            || u.dependencies != deps
            || u.attempts > 512
            || (i < confirmed && u.attempts == 0)
            || (i > confirmed && (u.state != UnitStatus::Pending || u.attempts != 0))
            || (i == confirmed && u.state == UnitStatus::Confirmed)
            || u.output_paths
                .iter()
                .any(|p| !crate::fixture_input::valid_relative_path(p))
        {
            return Err(error("RUN_SCHEMA", "单元、依赖、尝试或输出状态不符。"));
        }
    }
    if (confirmed == 6) != (s.status == RunStatus::Succeeded)
        && !matches!(s.status, RunStatus::Invalidated | RunStatus::Failed)
    {
        return Err(error("RUN_SCHEMA", "运行成功与完整工作图不符。"));
    }
    Ok(())
}
fn transition(old: &RunState, new: &RunState, event: &str) -> Result<(), ProjectError> {
    if !matches!(
        event,
        "unit_started"
            | "unit_prepared"
            | "unit_progress"
            | "unit_confirmed"
            | "resumed"
            | "paused"
            | "cancelled"
            | "invalidated"
            | "failed"
            | "publication_reconciled"
            | "completed_control"
    ) || old.status == RunStatus::Cancelled
        || old.status == RunStatus::Invalidated
        || (old.status == RunStatus::Failed && !matches!(event, "resumed" | "invalidated"))
        || new.acknowledged_control < old.acknowledged_control
    {
        return Err(error("RUN_TRANSITION", "终态或事件不允许转换。"));
    }
    let event_status = match event {
        "unit_started" | "unit_prepared" | "unit_progress" => new.status == RunStatus::Running,
        "resumed" => new.status == RunStatus::Queued,
        "paused" => new.status == RunStatus::Paused,
        "cancelled" => new.status == RunStatus::Cancelled,
        "invalidated" => new.status == RunStatus::Invalidated,
        "failed" => new.status == RunStatus::Failed,
        "completed_control" => {
            old.status == RunStatus::Succeeded
                && new.status == RunStatus::Succeeded
                && old.confirmed() == 6
                && new.diagnostic.as_deref() == Some("completed before control took effect")
        }
        "unit_confirmed" | "publication_reconciled" => {
            new.status
                == if new.confirmed() == 6 {
                    RunStatus::Succeeded
                } else {
                    RunStatus::Running
                }
        }
        _ => false,
    };
    if !event_status {
        return Err(error("RUN_TRANSITION", "事件与运行状态不符。"));
    }
    if (matches!(
        event,
        "unit_started"
            | "unit_prepared"
            | "unit_progress"
            | "unit_confirmed"
            | "publication_reconciled"
            | "failed"
            | "invalidated"
    ) && new.acknowledged_control != old.acknowledged_control)
        || (matches!(event, "paused" | "cancelled" | "completed_control")
            && new.acknowledged_control <= old.acknowledged_control)
    {
        return Err(error(
            "RUN_TRANSITION",
            "工作事件不能确认控制请求；控制事件须前进到实际请求。",
        ));
    }
    let count = old.confirmed();
    if old.units[..count] != new.units[..count]
        || new.confirmed() < count
        || new.confirmed() > count + 1
    {
        return Err(error("RUN_TRANSITION", "已确认单元被改写或重复确认。"));
    }
    if event == "unit_started" {
        if count >= 6
            || new.confirmed() != count
            || new.units[count].attempts != old.units[count].attempts + 1
            || new.units[count].state != UnitStatus::Running
        {
            return Err(error("RUN_TRANSITION", "单元开始记录无效。"));
        }
    } else if matches!(event, "unit_confirmed" | "publication_reconciled") {
        if count >= 6
            || new.confirmed() != count + 1
            || old.units[count].state != UnitStatus::Running
            || new.units[count].attempts != old.units[count].attempts
        {
            return Err(error("RUN_TRANSITION", "确认没有对应实际开始。"));
        }
    } else if matches!(event, "unit_progress" | "unit_prepared") {
        if old.units != new.units || new.status != RunStatus::Running {
            return Err(error("RUN_TRANSITION", "在途检查点改变单元身份。"));
        }
    } else if old.units != new.units
        || old.manifest != new.manifest
        || old.output_snapshot != new.output_snapshot
        || old.pending_intent != new.pending_intent
        || old.validation_report != new.validation_report
        || old.checks != new.checks
    {
        return Err(error("RUN_TRANSITION", "控制事件改变了已保存结果。"));
    }
    Ok(())
}
fn history(c: &Connection, id: &str) -> Result<(u64, String, RunState), ProjectError> {
    let controls = control_chain(c, id)?;
    let mut stmt=c.prepare("SELECT sequence,previous_sha256,payload_json,payload_sha256 FROM checkpoints ORDER BY sequence").map_err(db_error)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                u64::from(r.get::<_, u32>(0)?),
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(db_error)?;
    let mut seq = 0;
    let mut last = String::new();
    let mut previous: Option<RunState> = None;
    for row in rows {
        let (n, prev, raw, hash) = row.map_err(db_error)?;
        if n != seq + 1
            || n > 512
            || prev != last
            || hash != sha256(raw.as_bytes())
            || raw.len() > 8 * 1024 * 1024
        {
            return Err(error("RUN_INTEGRITY", "检查点链失配。"));
        }
        let cp: Checkpoint = decode(&raw)?;
        if cp.sequence != n || cp.run_id != id || cp.schema_version != 1 || !utc(&cp.at) {
            return Err(error("RUN_SCHEMA", "检查点身份或时间不符。"));
        }
        validate_state(&cp.state)?;
        if cp.state.acknowledged_control > controls.len() as u64 {
            return Err(error("RUN_INTEGRITY", "检查点确认了不存在的控制请求。"));
        }
        if cp.event == "resumed"
            && effective_control(&controls[..cp.state.acknowledged_control as usize])
                == Some("cancel")
        {
            return Err(error(
                "RUN_TRANSITION",
                "继续事件不能跳过已取消的控制前缀。",
            ));
        }
        if cp.event == "completed_control" && cp.state.acknowledged_control == 0 {
            return Err(error("RUN_TRANSITION", "完成后控制确认须对应实际请求。"));
        }
        if matches!(cp.event.as_str(), "paused" | "cancelled") {
            if cp.state.acknowledged_control == 0 {
                return Err(error("RUN_TRANSITION", "控制事件没有对应请求。"));
            }
            let action = effective_control(&controls[..cp.state.acknowledged_control as usize]);
            if (cp.event == "paused" && action != Some("pause"))
                || (cp.event == "cancelled" && action != Some("cancel"))
            {
                return Err(error("RUN_TRANSITION", "控制确认与请求动作不符。"));
            }
        }
        if let Some(old) = &previous {
            transition(old, &cp.state, &cp.event)?;
        } else if cp.event != "created"
            || !matches!(cp.state.status, RunStatus::Queued | RunStatus::Paused)
            || cp.state != RunState::initial(cp.state.status == RunStatus::Paused)
        {
            return Err(error("RUN_SCHEMA", "缺少严格原始运行创建记录。"));
        }
        previous = Some(cp.state);
        seq = n;
        last = hash;
    }
    Ok((
        seq,
        last,
        previous.ok_or_else(|| error("RUN_SCHEMA", "运行没有持久检查点。"))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Sandbox(PathBuf);
    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn selected() -> (Sandbox, LoadedCase, Journal) {
        let base = std::env::temp_dir().join(format!("noetome-run-state-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&base).unwrap();
        fn copy(from: &Path, to: &Path) {
            fs::create_dir(to).unwrap();
            for entry in fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &to.join(entry.file_name()));
                } else {
                    fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
                }
            }
        }
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/run-checkpoint-v1/fixture"),
            &base.join("case"),
        );
        let case = crate::fixture_input::load(&base.join("case")).unwrap();
        let journal = Journal::create(
            &case,
            &base.join("run"),
            crate::fixture_compile::context(&case).unwrap(),
            true,
        )
        .unwrap();
        (Sandbox(base), case, journal)
    }
    fn forge(
        c: &Connection,
        sequence: u64,
        state: RunState,
        event: &str,
        id: &str,
        previous: &str,
    ) {
        let cp = Checkpoint {
            schema_version: 1,
            sequence,
            event: event.into(),
            at: now().unwrap(),
            run_id: id.into(),
            state,
        };
        let raw = encode(&cp).unwrap();
        c.execute(
            "INSERT OR REPLACE INTO checkpoints VALUES(?1,?2,?3,?4)",
            params![sequence as i64, previous, raw, sha256(raw.as_bytes())],
        )
        .unwrap();
    }
    #[test]
    fn actual_cancel_then_pause_prefix_reopens_cancelled_without_rewriting_confirmations() {
        let (_sandbox, _case, mut journal) = selected();
        assert_eq!(journal.request("cancel").unwrap(), 1);
        assert_eq!(journal.request("pause").unwrap(), 2);
        assert_eq!(
            journal.latest_control().unwrap(),
            (2, Some("cancel".into()))
        );
        let mut state = journal.state.clone();
        state.status = RunStatus::Cancelled;
        state.acknowledged_control = 2;
        journal.append("cancelled", state).unwrap();
        let root = journal.root.clone();
        drop(journal);
        let db = fs::read(root.join(DB)).unwrap();
        let reopened = Journal::open(&root, false).unwrap();
        assert_eq!(reopened.state.status, RunStatus::Cancelled);
        assert_eq!(reopened.state.acknowledged_control, 2);
        assert_eq!(reopened.state.units, RunState::initial(true).units);
        assert_eq!(fs::read(root.join(DB)).unwrap(), db);
    }
    #[test]
    fn reconstructed_hashes_cannot_turn_created_seed_into_terminal_or_started_work() {
        for status in [RunStatus::Cancelled, RunStatus::Running] {
            let (_sandbox, _case, journal) = selected();
            let root = journal.root.clone();
            let id = journal.metadata.owner.run_id.clone();
            drop(journal);
            let c = Connection::open(root.join(DB)).unwrap();
            let mut state = RunState::initial(true);
            state.status = status;
            if status == RunStatus::Running {
                state.units[0].state = UnitStatus::Running;
                state.units[0].attempts = 1;
            }
            forge(&c, 1, state, "created", &id, "");
            drop(c);
            let before = fs::read(root.join(DB)).unwrap();
            assert_eq!(
                Journal::open(&root, false).err().unwrap().code(),
                "RUN_SCHEMA"
            );
            assert_eq!(fs::read(root.join(DB)).unwrap(), before);
        }
    }
    #[test]
    fn reconstructed_work_or_resume_cannot_acknowledge_and_skip_cancel() {
        for event in ["unit_started", "resumed"] {
            let (_sandbox, _case, mut journal) = selected();
            journal.request("cancel").unwrap();
            let root = journal.root.clone();
            let id = journal.metadata.owner.run_id.clone();
            let prev = journal.last_hash.clone();
            let mut state = journal.state.clone();
            drop(journal);
            state.acknowledged_control = 1;
            state.status = if event == "unit_started" {
                RunStatus::Running
            } else {
                RunStatus::Queued
            };
            if event == "unit_started" {
                state.units[0].state = UnitStatus::Running;
                state.units[0].attempts = 1;
            }
            let c = Connection::open(root.join(DB)).unwrap();
            forge(&c, 2, state, event, &id, &prev);
            drop(c);
            let before = fs::read(root.join(DB)).unwrap();
            assert_eq!(
                Journal::open(&root, false).err().unwrap().code(),
                "RUN_TRANSITION"
            );
            assert_eq!(fs::read(root.join(DB)).unwrap(), before);
        }
    }
    #[test]
    fn unsealed_worker_bytes_are_refused_without_touching_database() {
        let (_sandbox, _case, journal) = selected();
        let root = journal.root.clone();
        drop(journal);
        fs::write(root.join(LOCK), b"foreign lock data").unwrap();
        let before = fs::read(root.join(DB)).unwrap();
        assert_eq!(
            Journal::open(&root, false).err().unwrap().code(),
            "RUN_OWNER"
        );
        assert_eq!(fs::read(root.join(DB)).unwrap(), before);
        assert_eq!(fs::read(root.join(LOCK)).unwrap(), b"foreign lock data");
    }
}
