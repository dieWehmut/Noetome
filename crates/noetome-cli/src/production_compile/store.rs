//! Project-owned production IR and incremental task persistence.
use super::{error, model::*};
use crate::{
    compile_identity::{Identity, identity, sqlite_identity},
    corpus, domain,
    project::{self, ProjectError},
    text::sha256,
};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

const APP: i64 = 0x4e504952;
const SCHEMA: &str = "
CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL CHECK(schema_version=1),project_id TEXT NOT NULL);
CREATE TABLE runs(run_id TEXT PRIMARY KEY,config_json TEXT NOT NULL,config_sha256 TEXT NOT NULL,status TEXT NOT NULL,stage TEXT NOT NULL,created_at TEXT NOT NULL,ir_revision INTEGER NOT NULL DEFAULT 0,pause_requested INTEGER NOT NULL DEFAULT 0,cancel_requested INTEGER NOT NULL DEFAULT 0,error_code TEXT,error_message TEXT,directory_identity TEXT NOT NULL);
CREATE TABLE sources(run_id TEXT NOT NULL REFERENCES runs(run_id),source_id TEXT NOT NULL,ordinal INTEGER NOT NULL,scope_json TEXT NOT NULL,scope_sha256 TEXT NOT NULL,prepared_pages INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(run_id,source_id),UNIQUE(run_id,ordinal));
CREATE TABLE pages(run_id TEXT NOT NULL,source_id TEXT NOT NULL,page_number INTEGER NOT NULL,page_sha256 TEXT NOT NULL,status TEXT NOT NULL,units_json TEXT NOT NULL,issues_json TEXT NOT NULL,PRIMARY KEY(run_id,source_id,page_number),FOREIGN KEY(run_id,source_id) REFERENCES sources(run_id,source_id));
CREATE TABLE units(run_id TEXT NOT NULL,unit_id TEXT NOT NULL,source_id TEXT NOT NULL,page_number INTEGER NOT NULL,ordinal INTEGER NOT NULL,unit_json TEXT NOT NULL,unit_sha256 TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'pending',PRIMARY KEY(run_id,unit_id),FOREIGN KEY(run_id,source_id,page_number) REFERENCES pages(run_id,source_id,page_number),UNIQUE(run_id,source_id,page_number,ordinal));
CREATE TABLE attempts(request_id TEXT PRIMARY KEY,run_id TEXT NOT NULL,unit_id TEXT NOT NULL,attempt_no INTEGER NOT NULL CHECK(attempt_no BETWEEN 1 AND 2),state TEXT NOT NULL,directory TEXT NOT NULL,request_sha256 TEXT NOT NULL,response_sha256 TEXT,receipt_sha256 TEXT,directory_identity TEXT NOT NULL,FOREIGN KEY(run_id,unit_id) REFERENCES units(run_id,unit_id),UNIQUE(run_id,unit_id,attempt_no));
CREATE TABLE batches(request_id TEXT PRIMARY KEY REFERENCES attempts(request_id),run_id TEXT NOT NULL,unit_id TEXT NOT NULL,batch_sha256 TEXT NOT NULL,ir_revision INTEGER NOT NULL);
CREATE TABLE concepts(run_id TEXT NOT NULL,concept_id TEXT NOT NULL,request_id TEXT NOT NULL REFERENCES attempts(request_id),local_id TEXT NOT NULL,name TEXT NOT NULL,json TEXT NOT NULL,sha256 TEXT NOT NULL,PRIMARY KEY(run_id,concept_id),UNIQUE(request_id,local_id));
CREATE TABLE atoms(run_id TEXT NOT NULL,atom_id TEXT NOT NULL,request_id TEXT NOT NULL REFERENCES attempts(request_id),local_id TEXT NOT NULL,json TEXT NOT NULL,sha256 TEXT NOT NULL,measurement_json TEXT NOT NULL,measurement_sha256 TEXT NOT NULL,PRIMARY KEY(run_id,atom_id),UNIQUE(request_id,local_id));
CREATE TABLE atom_concepts(run_id TEXT NOT NULL,atom_id TEXT NOT NULL,concept_id TEXT NOT NULL,PRIMARY KEY(run_id,atom_id,concept_id),FOREIGN KEY(run_id,atom_id) REFERENCES atoms(run_id,atom_id),FOREIGN KEY(run_id,concept_id) REFERENCES concepts(run_id,concept_id));
CREATE TABLE evidence(run_id TEXT NOT NULL,evidence_id TEXT NOT NULL,request_id TEXT NOT NULL REFERENCES attempts(request_id),local_id TEXT NOT NULL,json TEXT NOT NULL,sha256 TEXT NOT NULL,PRIMARY KEY(run_id,evidence_id),UNIQUE(request_id,local_id));
CREATE TABLE issues(issue_id TEXT PRIMARY KEY,run_id TEXT NOT NULL REFERENCES runs(run_id),scope TEXT NOT NULL,code TEXT NOT NULL,json TEXT NOT NULL,sha256 TEXT NOT NULL,UNIQUE(run_id,scope,code));
CREATE INDEX units_schedule ON units(run_id,status,source_id,page_number,ordinal);
CREATE INDEX attempts_recovery ON attempts(run_id,state);
CREATE INDEX concepts_name ON concepts(run_id,name,concept_id);
";

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Owner {
    schema_version: u32,
    producer: String,
    project_id: String,
    project: Identity,
    root: Identity,
    database: Identity,
}

pub(super) struct Store {
    pub(super) root: PathBuf,
    project: PathBuf,
    project_id: String,
    connection: Connection,
    owner: Owner,
    _database_guard: Option<File>,
    _directory_guards: Vec<File>,
}

fn db(e: rusqlite::Error) -> ProjectError {
    error("COMPILE_DATABASE", e.to_string())
}
fn invalid(message: impl Into<String>) -> ProjectError {
    error("COMPILE_INTEGRITY", message)
}
fn encoded<T: Serialize + ?Sized>(value: &T) -> Result<String, ProjectError> {
    serde_json::to_string(value).map_err(|e| invalid(e.to_string()))
}
fn decode<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, ProjectError> {
    serde_json::from_str(value).map_err(|e| invalid(e.to_string()))
}
fn pinned<T: serde::de::DeserializeOwned>(raw: &str, sha: &str) -> Result<T, ProjectError> {
    if sha256(raw.as_bytes()) != sha {
        return Err(invalid("保存输入的原字节摘要不匹配。"));
    }
    decode(raw)
}
fn integer(value: u64) -> Result<i64, ProjectError> {
    i64::try_from(value).map_err(|_| invalid("计数超过SQLite整数范围。"))
}
fn uuid(id: &str) -> Result<(), ProjectError> {
    if Uuid::parse_str(id)
        .is_ok_and(|v| v.to_string() == id && v.get_version() == Some(uuid::Version::Random))
    {
        Ok(())
    } else {
        Err(invalid("应用身份必须为规范UUID v4。"))
    }
}
fn now() -> Result<String, ProjectError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| invalid(e.to_string()))
}
pub(super) fn guarded(path: &Path, create: bool, directory: bool) -> Result<File, ProjectError> {
    project::check_components(path)?;
    if !create {
        let metadata = fs::symlink_metadata(path).map_err(|e| ProjectError::io(path, e))?;
        if project::is_link(&metadata)
            || metadata.is_dir() != directory
            || !(metadata.is_file() || metadata.is_dir())
        {
            return Err(ProjectError::unsafe_path(path));
        }
    }
    let mut options = OpenOptions::new();
    options.read(true);
    if !directory {
        options.write(true).create_new(create);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x02200000);
    }
    let file = options.open(path).map_err(|e| ProjectError::io(path, e))?;
    let opened_metadata = file.metadata().map_err(|e| ProjectError::io(path, e))?;
    if project::is_link(&opened_metadata)
        || opened_metadata.is_dir() != directory
        || !(opened_metadata.is_file() || opened_metadata.is_dir())
    {
        return Err(ProjectError::unsafe_path(path));
    }
    let current = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
    let opened =
        same_file::Handle::from_file(file.try_clone().map_err(|e| ProjectError::io(path, e))?)
            .map_err(|e| ProjectError::io(path, e))?;
    if current != opened {
        return Err(invalid("拥有文件在打开期间被替换。"));
    }
    identity(path)?;
    Ok(file)
}

impl Store {
    pub(super) fn open(project: &Path) -> Result<Option<Self>, ProjectError> {
        Self::open_inner(project, false)
    }
    pub(super) fn create(project: &Path) -> Result<Self, ProjectError> {
        Self::open_inner(project, true)?.ok_or_else(|| invalid("编译库创建失败。"))
    }
    fn open_inner(project_path: &Path, create: bool) -> Result<Option<Self>, ProjectError> {
        let info = project::inspect(project_path)?;
        let project = project::check_components(project_path)?
            .canonicalize()
            .map_err(|e| ProjectError::io(project_path, e))?;
        crate::extraction_preview::ensure_outside_vault(&project)?;
        let root = project.join(".noetome-compile");
        project::check_components(&root)?;
        let mut created = false;
        if !root.exists() {
            if !create {
                return Ok(None);
            }
            match fs::create_dir(&root) {
                Ok(()) => created = true,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(ProjectError::io(&root, e)),
            }
        }
        let dirs = vec![
            guarded(&project, false, true)?,
            guarded(&root, false, true)?,
        ];
        let lock_path = root.join("initialize.lock");
        let lock = if lock_path.exists() {
            guarded(&lock_path, false, false)?
        } else if created {
            guarded(&lock_path, true, false)?
        } else {
            return Err(invalid("既有编译目录缺初始化所有权，未接管。"));
        };
        lock.lock().map_err(|e| ProjectError::io(&lock_path, e))?;
        let database = root.join("compile.sqlite");
        let owner_path = root.join("owner.json");
        let database_guard;
        let owner;
        if created {
            let initial_database = guarded(&database, true, false)?;
            owner = Owner {
                schema_version: 1,
                producer: "production_compile".into(),
                project_id: info.project_id.clone(),
                project: identity(&project)?,
                root: identity(&root)?,
                database: sqlite_identity(&database)?,
            };
            let mut marker = guarded(&owner_path, true, false)?;
            marker
                .write_all(encoded(&owner)?.as_bytes())
                .map_err(|e| ProjectError::io(&owner_path, e))?;
            marker
                .sync_all()
                .map_err(|e| ProjectError::io(&owner_path, e))?;
            #[cfg(windows)]
            {
                database_guard = Some(initial_database);
            }
            #[cfg(not(windows))]
            {
                drop(initial_database);
                database_guard = None;
            }
        } else {
            project::checked_file(&owner_path)?;
            identity(&owner_path)?;
            let guard = project::read_guard(&owner_path)?;
            if guard
                .metadata()
                .map_err(|e| ProjectError::io(&owner_path, e))?
                .len()
                > 16384
            {
                return Err(invalid("所有权标记过大。"));
            }
            owner = decode(
                &fs::read_to_string(&owner_path).map_err(|e| ProjectError::io(&owner_path, e))?,
            )?;
            #[cfg(windows)]
            {
                database_guard = Some(guarded(&database, false, false)?);
            }
            #[cfg(not(windows))]
            {
                database_guard = None;
            }
            if owner.schema_version != 1
                || owner.producer != "production_compile"
                || owner.project_id != info.project_id
                || owner.project != identity(&project)?
                || owner.root != identity(&root)?
                || owner.database != sqlite_identity(&database)?
            {
                return Err(invalid("编译库的项目、目录或数据库持久身份不匹配。"));
            }
        }
        for suffix in ["-wal", "-shm", "-journal"] {
            let path = root.join(format!("compile.sqlite{suffix}"));
            project::check_components(&path)?;
            if path.exists() {
                sqlite_identity(&path)?;
            }
        }
        let connection = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(db)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(db)?;
        if created {
            let tx = connection.unchecked_transaction().map_err(db)?;
            tx.execute_batch(SCHEMA).map_err(db)?;
            tx.execute("INSERT INTO metadata VALUES(1,1,?1)", [&info.project_id])
                .map_err(db)?;
            tx.execute_batch(&format!(
                "PRAGMA application_id={APP}; PRAGMA user_version=1;"
            ))
            .map_err(db)?;
            tx.commit().map_err(db)?;
            fs::create_dir(root.join("runs")).map_err(|e| ProjectError::io(&root, e))?;
        }
        let metadata: (i64, String) = connection
            .query_row(
                "SELECT schema_version,project_id FROM metadata WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(db)?;
        let app: i64 = connection
            .query_row("PRAGMA application_id", [], |r| r.get(0))
            .map_err(db)?;
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(db)?;
        if metadata != (1, info.project_id.clone()) || app != APP || version != 1 {
            return Err(error("COMPILE_SCHEMA", "生产IR版本或项目绑定不匹配。"));
        }
        connection
            .execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
            )
            .map_err(db)?;
        project::check_components(&root.join("runs"))?;
        let store = Self {
            root,
            project,
            project_id: info.project_id,
            connection,
            owner,
            _database_guard: database_guard,
            _directory_guards: dirs,
        };
        store.verify()?;
        Ok(Some(store))
    }
    fn verify(&self) -> Result<(), ProjectError> {
        if identity(&self.project)? != self.owner.project
            || identity(&self.root)? != self.owner.root
            || sqlite_identity(&self.root.join("compile.sqlite"))? != self.owner.database
        {
            return Err(invalid("编译库拥有身份变化。"));
        }
        for suffix in ["-wal", "-shm", "-journal"] {
            let path = self.root.join(format!("compile.sqlite{suffix}"));
            project::check_components(&path)?;
            if path.exists() {
                sqlite_identity(&path)?;
            }
        }
        Ok(())
    }
    fn transaction(&self) -> Result<Transaction<'_>, ProjectError> {
        self.verify()?;
        Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate).map_err(db)
    }
    pub(super) fn run_directory(&self, run_id: &str) -> Result<PathBuf, ProjectError> {
        uuid(run_id)?;
        self.verify()?;
        let expected: String = self
            .connection
            .query_row(
                "SELECT directory_identity FROM runs WHERE run_id=?1",
                [run_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)?
            .ok_or_else(|| error("COMPILE_NOT_FOUND", "运行不在当前项目中。"))?;
        let directory = project::check_components(&self.root.join("runs").join(run_id))?;
        if identity(&directory)? != decode::<Identity>(&expected)? {
            return Err(invalid("运行目录的持久身份变化。"));
        }
        Ok(directory)
    }
    pub(super) fn lease(&self, run_id: &str) -> Result<File, ProjectError> {
        let path = self.run_directory(run_id)?.join("worker.lock");
        let file = if path.exists() {
            guarded(&path, false, false)?
        } else {
            guarded(&path, true, false)?
        };
        file.try_lock()
            .map_err(|_| error("COMPILE_BUSY", "本运行已有活动工作进程。"))?;
        Ok(file)
    }
    pub(super) fn create_run(
        &self,
        config: &CompileConfig,
        sources: &[SourceScope],
    ) -> Result<String, ProjectError> {
        if config.schema_version != 1 || sources.is_empty() {
            return Err(invalid("运行版本或来源范围为空。"));
        }
        let mut seen = BTreeSet::new();
        for source in sources {
            uuid(&source.source_id)?;
            if !seen.insert(&source.source_id)
                || !domain::is_sha(&source.source_sha256)
                || source.physical_pages == 0
            {
                return Err(invalid("来源范围身份重复或摘要/页数无效。"));
            }
            let actual = corpus::verify_source(&self.project, &source.source_id)?;
            if actual.content_sha256.as_deref() != Some(&source.source_sha256)
                || actual.byte_length != source.byte_length
                || actual.page_count != Some(source.physical_pages)
            {
                return Err(invalid("运行来源范围不符合当前确认原件。"));
            }
        }
        let run_id = Uuid::new_v4().to_string();
        let directory = self.root.join("runs").join(&run_id);
        project::check_components(&directory)?;
        fs::create_dir(&directory).map_err(|e| ProjectError::io(&directory, e))?;
        fs::create_dir(directory.join("attempts")).map_err(|e| ProjectError::io(&directory, e))?;
        let tx = self.transaction()?;
        let raw_config = encoded(config)?;
        tx.execute("INSERT INTO runs(run_id,config_json,config_sha256,status,stage,created_at,directory_identity) VALUES(?1,?2,?3,'queued','prepare',?4,?5)",params![run_id,raw_config,sha256(raw_config.as_bytes()),now()?,encoded(&identity(&directory)?)?]).map_err(db)?;
        for (ordinal, source) in sources.iter().enumerate() {
            let raw = encoded(source)?;
            tx.execute(
                "INSERT INTO sources(run_id,source_id,ordinal,scope_json,scope_sha256) VALUES(?1,?2,?3,?4,?5)",
                params![
                    run_id,
                    source.source_id,
                    integer(ordinal as u64)?,
                    raw,
                    sha256(raw.as_bytes())
                ],
            )
            .map_err(db)?;
        }
        tx.commit().map_err(db)?;
        Ok(run_id)
    }
    pub(super) fn config(&self, run_id: &str) -> Result<CompileConfig, ProjectError> {
        self.run_directory(run_id)?;
        let (raw, hash): (String, String) = self
            .connection
            .query_row(
                "SELECT config_json,config_sha256 FROM runs WHERE run_id=?1",
                [run_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(db)?;
        pinned(&raw, &hash)
    }
    fn sources(&self, run_id: &str) -> Result<Vec<SourceScope>, ProjectError> {
        let mut query = self
            .connection
            .prepare("SELECT scope_json,scope_sha256 FROM sources WHERE run_id=?1 ORDER BY ordinal")
            .map_err(db)?;
        query
            .query_map([run_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(db)?
            .map(|row| {
                let (raw, hash) = row.map_err(db)?;
                pinned(&raw, &hash)
            })
            .collect()
    }
    pub(super) fn list(&self) -> Result<Vec<RunSummary>, ProjectError> {
        self.verify()?;
        let mut query = self
            .connection
            .prepare("SELECT run_id FROM runs ORDER BY created_at DESC,run_id")
            .map_err(db)?;
        let ids = query
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(db)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db)?;
        ids.iter().map(|id| self.summary(id)).collect()
    }
    fn count(&self, sql: &str, run_id: &str) -> Result<u64, ProjectError> {
        let value: i64 = self
            .connection
            .query_row(sql, [run_id], |r| r.get(0))
            .map_err(db)?;
        u64::try_from(value).map_err(|_| invalid("负计数。"))
    }
    pub(super) fn summary(&self, run_id: &str) -> Result<RunSummary, ProjectError> {
        self.run_directory(run_id)?;
        let (mut status,stage,created_at,revision,code,message,paused,cancelled):(String,String,String,i64,Option<String>,Option<String>,bool,bool)=self.connection.query_row("SELECT status,stage,created_at,ir_revision,error_code,error_message,pause_requested!=0,cancel_requested!=0 FROM runs WHERE run_id=?1",[run_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).map_err(db)?;
        let sources = self.sources(run_id)?;
        let physical_pages = sources.iter().try_fold(0u64, |n, s| {
            n.checked_add(s.physical_pages as u64)
                .ok_or_else(|| invalid("物理页计数溢出。"))
        })?;
        let prepared_pages = self.count("SELECT count(*) FROM pages WHERE run_id=?1", run_id)?;
        if cancelled {
            status = "cancelled".into();
        } else if code.is_some()
            && matches!(status.as_str(), "failed" | "budget_stop" | "budget_stopped")
        {
            // A pause is a scheduling request, not permission to clear a global stop.
        } else if paused {
            status = "paused".into();
        } else if status == "running" && !self.active(run_id)? {
            status = "interrupted".into();
        }
        let atoms = self.count("SELECT count(*) FROM atoms WHERE run_id=?1", run_id)?;
        Ok(RunSummary{schema_version:1,run_id:run_id.into(),status,stage,sources,physical_pages,
            prepared_pages,
            units_total:self.count("SELECT count(*) FROM units WHERE run_id=?1",run_id)?,units_confirmed:self.count("SELECT count(*) FROM units WHERE run_id=?1 AND status='confirmed'",run_id)?,units_failed:self.count("SELECT count(*) FROM units WHERE run_id=?1 AND status='failed'",run_id)?,
            pending_ocr_pages:self.count("SELECT count(*) FROM pages WHERE run_id=?1 AND status='needs_ocr'",run_id)?,unresolved_pages:physical_pages.saturating_sub(prepared_pages)+self.count("SELECT count(*) FROM pages p WHERE p.run_id=?1 AND (p.status IN ('pending','needs_ocr','needs_review','failed') OR EXISTS (SELECT 1 FROM units u WHERE u.run_id=p.run_id AND u.source_id=p.source_id AND u.page_number=p.page_number AND u.status!='confirmed'))",run_id)?,
            concepts:self.count("SELECT count(*) FROM concepts WHERE run_id=?1",run_id)?,atoms,evidence:self.count("SELECT count(*) FROM evidence WHERE run_id=?1",run_id)?,attempts:self.count("SELECT count(*) FROM attempts WHERE run_id=?1",run_id)?,ir_revision:u64::try_from(revision).map_err(|_|invalid("IR修订号无效。"))?,knowledge_status:if atoms>0 {"unverified"}else{"pending"}.into(),scientific_quality:"not_run".into(),budget:None,error:code.map(|code|Failure{code,message:message.unwrap_or_default()}),created_at})
    }
    pub(super) fn next_unprepared_page(
        &self,
        run_id: &str,
    ) -> Result<Option<(SourceScope, u32)>, ProjectError> {
        self.run_directory(run_id)?;
        let mut query = self
            .connection
            .prepare(
                "SELECT scope_json,scope_sha256,prepared_pages FROM sources WHERE run_id=?1 ORDER BY ordinal",
            )
            .map_err(db)?;
        for row in query
            .query_map([run_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u32>(2)?,
                ))
            })
            .map_err(db)?
        {
            let (raw, hash, prepared) = row.map_err(db)?;
            let source: SourceScope = pinned(&raw, &hash)?;
            if prepared < source.physical_pages {
                return Ok(Some((source, prepared + 1)));
            }
        }
        Ok(None)
    }
    pub(super) fn save_page(
        &self,
        run_id: &str,
        source: &SourceScope,
        number: u32,
        page_sha: &str,
        status: &str,
        units: Vec<Unit>,
        issues: Vec<Value>,
    ) -> Result<(), ProjectError> {
        self.run_directory(run_id)?;
        if !domain::is_sha(page_sha) || number == 0 || number > source.physical_pages {
            return Err(invalid("页版本摘要或物理页号无效。"));
        }
        let selected = self
            .sources(run_id)?
            .into_iter()
            .find(|s| s.source_id == source.source_id)
            .ok_or_else(|| invalid("页来源未注册。"))?;
        if encoded(&selected)? != encoded(source)? {
            return Err(invalid("页来源范围改变。"));
        }
        let page = corpus::page_revision(&self.project, &source.source_id, number, page_sha)?;
        if page["status"] != status {
            return Err(invalid("页状态不符合固定来源页版本。"));
        }
        let text = page["text"]
            .as_str()
            .ok_or_else(|| invalid("固定页缺少文本。"))?;
        let mut cursor = 0;
        let mut ids = BTreeSet::new();
        for unit in &units {
            if !domain::is_id(&unit.unit_id)
                || !ids.insert(&unit.unit_id)
                || unit.source_id != source.source_id
                || unit.source_sha256 != source.source_sha256
                || unit.page_number != number
                || unit.page_sha256 != page_sha
                || unit.source_status != status
                || unit.start_byte != cursor
                || unit.end_byte <= cursor
                || text.get(unit.start_byte..unit.end_byte) != Some(unit.text.as_str())
                || unit.bbox
                    != [
                        0.0,
                        0.0,
                        page["width_pt"].as_f64().unwrap_or(0.0),
                        page["height_pt"].as_f64().unwrap_or(0.0),
                    ]
                || unit.bbox.iter().any(|n| !n.is_finite())
            {
                return Err(invalid(
                    "拆分单元必须完整连续覆盖固定页的真实UTF8文本及实际全页框。",
                ));
            }
            cursor = unit.end_byte;
        }
        if !units.is_empty() && cursor != text.len() {
            return Err(invalid("页拆分不得截断尾部正文。"));
        }
        if matches!(status, "text_ready" | "ocr_ready")
            && !text.trim().is_empty()
            && units.is_empty()
        {
            return Err(invalid("可读页不得遗漏全部正文。"));
        }
        let raw_units = encoded(&units)?;
        let raw_issues = encoded(&issues)?;
        let tx = self.transaction()?;
        let prior:Option<(String,String,String,String)>=tx.query_row("SELECT page_sha256,status,units_json,issues_json FROM pages WHERE run_id=?1 AND source_id=?2 AND page_number=?3",params![run_id,source.source_id,number],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(db)?;
        if let Some(prior) = prior {
            if prior == (page_sha.into(), status.into(), raw_units, raw_issues) {
                return Ok(());
            }
            return Err(invalid("已确认拆页输入发生变化；不覆盖。"));
        }
        let prepared: u32 = tx
            .query_row(
                "SELECT prepared_pages FROM sources WHERE run_id=?1 AND source_id=?2",
                params![run_id, source.source_id],
                |r| r.get(0),
            )
            .map_err(db)?;
        if number != prepared + 1 {
            return Err(invalid("拆页须按来源完整物理顺序确认。"));
        }
        tx.execute(
            "INSERT INTO pages VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                run_id,
                source.source_id,
                number,
                page_sha,
                status,
                raw_units,
                raw_issues
            ],
        )
        .map_err(db)?;
        for (ordinal, unit) in units.iter().enumerate() {
            let raw = encoded(unit)?;
            tx.execute("INSERT INTO units(run_id,unit_id,source_id,page_number,ordinal,unit_json,unit_sha256) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![run_id,unit.unit_id,source.source_id,number,integer(ordinal as u64)?,raw,sha256(raw.as_bytes())]).map_err(db)?;
        }
        for (ordinal, issue) in issues.iter().enumerate() {
            let code = issue["code"].as_str().unwrap_or("SOURCE_REVIEW");
            add_issue(
                &tx,
                run_id,
                &format!("page:{}:{number}:{ordinal}", source.source_id),
                code,
                issue,
            )?;
        }
        tx.execute(
            "UPDATE sources SET prepared_pages=?3 WHERE run_id=?1 AND source_id=?2",
            params![run_id, source.source_id, number],
        )
        .map_err(db)?;
        tx.commit().map_err(db)
    }
    pub(super) fn unit(&self, run_id: &str, id: &str) -> Result<Unit, ProjectError> {
        self.run_directory(run_id)?;
        let (raw, sha): (String, String) = self
            .connection
            .query_row(
                "SELECT unit_json,unit_sha256 FROM units WHERE run_id=?1 AND unit_id=?2",
                params![run_id, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(db)?
            .ok_or_else(|| error("COMPILE_UNIT_NOT_FOUND", "任务单元不存在。"))?;
        if sha256(raw.as_bytes()) != sha {
            return Err(invalid("任务单元保存摘要不符。"));
        }
        let unit: Unit = decode(&raw)?;
        if unit.unit_id != id {
            return Err(invalid("任务单元身份不符。"));
        }
        Ok(unit)
    }
    pub(super) fn next_unit(&self, run_id: &str) -> Result<Option<Unit>, ProjectError> {
        self.run_directory(run_id)?;
        let id:Option<String>=self.connection.query_row("SELECT u.unit_id FROM units u JOIN sources s ON s.run_id=u.run_id AND s.source_id=u.source_id WHERE u.run_id=?1 AND u.status='pending' ORDER BY s.ordinal,u.page_number,u.ordinal LIMIT 1",[run_id],|r|r.get(0)).optional().map_err(db)?;
        id.map(|id| self.unit(run_id, &id)).transpose()
    }
    pub(super) fn pause(&self, run_id: &str) -> Result<(), ProjectError> {
        self.control(run_id, false)
    }
    pub(super) fn cancel(&self, run_id: &str) -> Result<(), ProjectError> {
        self.control(run_id, true)
    }
    fn control(&self, run_id: &str, cancel: bool) -> Result<(), ProjectError> {
        self.run_directory(run_id)?;
        let tx = self.transaction()?;
        tx.execute(
            if cancel {
                "UPDATE runs SET cancel_requested=1,status='cancelled' WHERE run_id=?1"
            } else {
                "UPDATE runs SET pause_requested=1 WHERE run_id=?1"
            },
            [run_id],
        )
        .map_err(db)?;
        tx.commit().map_err(db)
    }
    pub(super) fn should_stop(&self, run_id: &str) -> Result<bool, ProjectError> {
        self.run_directory(run_id)?;
        self.connection
            .query_row(
                "SELECT pause_requested!=0 OR cancel_requested!=0 FROM runs WHERE run_id=?1",
                [run_id],
                |r| r.get(0),
            )
            .map_err(db)
    }
    pub(super) fn set_status(
        &self,
        run_id: &str,
        status: &str,
        stage: &str,
    ) -> Result<(), ProjectError> {
        self.run_directory(run_id)?;
        if ![
            "queued",
            "running",
            "paused",
            "cancelled",
            "failed",
            "ready",
            "needs_review",
            "budget_stopped",
            "budget_stop",
            "completed",
            "succeeded",
        ]
        .contains(&status)
            || !domain::is_id(stage)
        {
            return Err(invalid("运行状态或阶段不受支持。"));
        }
        let tx = self.transaction()?;
        let cancelled: bool = tx
            .query_row(
                "SELECT cancel_requested!=0 FROM runs WHERE run_id=?1",
                [run_id],
                |r| r.get(0),
            )
            .map_err(db)?;
        if cancelled && status != "cancelled" {
            return Err(error("COMPILE_CANCELLED", "已取消运行不恢复调度。"));
        }
        tx.execute(
            "UPDATE runs SET status=?2,stage=?3 WHERE run_id=?1",
            params![run_id, status, stage],
        )
        .map_err(db)?;
        tx.commit().map_err(db)
    }
    pub(super) fn set_error(
        &self,
        run_id: &str,
        code: &str,
        message: &str,
    ) -> Result<(), ProjectError> {
        self.run_directory(run_id)?;
        let tx = self.transaction()?;
        tx.execute(
            "UPDATE runs SET error_code=?2,error_message=?3 WHERE run_id=?1",
            params![run_id, code, message],
        )
        .map_err(db)?;
        tx.commit().map_err(db)
    }
    pub(super) fn project_id(&self) -> &str {
        &self.project_id
    }
    pub(super) fn resume(&self, run_id: &str) -> Result<(), ProjectError> {
        self.run_directory(run_id)?;
        let tx = self.transaction()?;
        let cancelled: bool = tx
            .query_row(
                "SELECT cancel_requested!=0 FROM runs WHERE run_id=?1",
                [run_id],
                |r| r.get(0),
            )
            .map_err(db)?;
        if cancelled {
            return Err(error("COMPILE_CANCELLED", "取消是终态，不能恢复。"));
        }
        tx.execute("UPDATE runs SET pause_requested=0,status='queued',error_code=NULL,error_message=NULL WHERE run_id=?1",[run_id]).map_err(db)?;
        tx.commit().map_err(db)
    }
    fn active(&self, run_id: &str) -> Result<bool, ProjectError> {
        let path = self.run_directory(run_id)?.join("worker.lock");
        if !path.exists() {
            return Ok(false);
        }
        let file = guarded(&path, false, false)?;
        match file.try_lock() {
            Ok(()) => {
                file.unlock().map_err(|e| ProjectError::io(&path, e))?;
                Ok(false)
            }
            Err(std::fs::TryLockError::WouldBlock) => Ok(true),
            Err(e) => Err(ProjectError::io(&path, e)),
        }
    }
    pub(super) fn retry_failed(&self, run_id: &str) -> Result<(), ProjectError> {
        self.run_directory(run_id)?;
        let tx = self.transaction()?;
        let cancelled: bool = tx
            .query_row(
                "SELECT cancel_requested!=0 FROM runs WHERE run_id=?1",
                [run_id],
                |r| r.get(0),
            )
            .map_err(db)?;
        if cancelled {
            return Err(error("COMPILE_CANCELLED", "已取消运行不能重试。"));
        }
        tx.execute("UPDATE units SET status='pending' WHERE run_id=?1 AND status='failed' AND (SELECT count(*) FROM attempts a WHERE a.run_id=units.run_id AND a.unit_id=units.unit_id)<2",[run_id]).map_err(db)?;
        tx.execute("UPDATE runs SET pause_requested=0,status='queued',error_code=NULL,error_message=NULL WHERE run_id=?1",[run_id]).map_err(db)?;
        tx.commit().map_err(db)
    }
    pub(super) fn begin_attempt(
        &self,
        run_id: &str,
        unit: &Unit,
        request_id: &str,
        request_sha: &str,
    ) -> Result<Attempt, ProjectError> {
        uuid(request_id)?;
        if !domain::is_sha(request_sha)
            || encoded(&self.unit(run_id, &unit.unit_id)?)? != encoded(unit)?
        {
            return Err(invalid("请求摘要或单元绑定改变。"));
        }
        let tx = self.transaction()?;
        let status: String = tx
            .query_row(
                "SELECT status FROM units WHERE run_id=?1 AND unit_id=?2",
                params![run_id, unit.unit_id],
                |r| r.get(0),
            )
            .map_err(db)?;
        let stopped: bool = tx
            .query_row(
                "SELECT pause_requested!=0 OR cancel_requested!=0 FROM runs WHERE run_id=?1",
                [run_id],
                |r| r.get(0),
            )
            .map_err(db)?;
        if status != "pending" || stopped {
            return Err(error(
                "COMPILE_STOPPED",
                "单元已终止、已确认或运行停止，不开始新请求。",
            ));
        }
        let active:i64=tx.query_row("SELECT count(*) FROM attempts WHERE run_id=?1 AND unit_id=?2 AND state NOT IN ('accepted','failed','no_request')",params![run_id,unit.unit_id],|r|r.get(0)).map_err(db)?;
        if active > 0 {
            return Err(error(
                "COMPILE_ATTEMPT_RECOVERY_REQUIRED",
                "原请求尚未恢复或结算，不开始第二个请求。",
            ));
        }
        let count: i64 = tx
            .query_row(
                "SELECT count(*) FROM attempts WHERE run_id=?1 AND unit_id=?2",
                params![run_id, unit.unit_id],
                |r| r.get(0),
            )
            .map_err(db)?;
        if !(0..2).contains(&count) {
            return Err(error("COMPILE_ATTEMPT_LIMIT", "本单元已达到两次请求上限。"));
        }
        let directory = PathBuf::from("runs")
            .join(run_id)
            .join("attempts")
            .join(request_id);
        let path = project::check_components(&self.root.join(&directory))?;
        fs::create_dir(&path).map_err(|e| ProjectError::io(&path, e))?;
        let attempt = Attempt {
            request_id: request_id.into(),
            unit_id: unit.unit_id.clone(),
            attempt_no: count as u32 + 1,
            state: "prepared".into(),
            directory,
            request_sha256: request_sha.into(),
            response_sha256: None,
            receipt_sha256: None,
        };
        tx.execute(
            "INSERT INTO attempts VALUES(?1,?2,?3,?4,?5,?6,?7,NULL,NULL,?8)",
            params![
                request_id,
                run_id,
                unit.unit_id,
                attempt.attempt_no,
                attempt.state,
                attempt.directory.to_string_lossy(),
                request_sha,
                encoded(&identity(&path)?)?
            ],
        )
        .map_err(db)?;
        tx.commit().map_err(db)?;
        Ok(attempt)
    }
    pub(super) fn attempts(&self, run_id: &str) -> Result<Vec<Attempt>, ProjectError> {
        self.run_directory(run_id)?;
        let mut query=self.connection.prepare("SELECT request_id,unit_id,attempt_no,state,directory,request_sha256,response_sha256,receipt_sha256 FROM attempts WHERE run_id=?1 ORDER BY unit_id,attempt_no").map_err(db)?;
        query
            .query_map([run_id], read_attempt)
            .map_err(db)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db)
    }
    pub(super) fn attempts_needing_recovery(
        &self,
        run_id: &str,
    ) -> Result<Vec<Attempt>, ProjectError> {
        self.run_directory(run_id)?;
        let mut query = self.connection.prepare("SELECT request_id,unit_id,attempt_no,state,directory,request_sha256,response_sha256,receipt_sha256 FROM attempts WHERE run_id=?1 AND state NOT IN ('accepted','failed','no_request') ORDER BY unit_id,attempt_no").map_err(db)?;
        query
            .query_map([run_id], read_attempt)
            .map_err(db)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db)
    }
    fn attempt(&self, run_id: &str, request_id: &str) -> Result<Attempt, ProjectError> {
        self.connection.query_row("SELECT request_id,unit_id,attempt_no,state,directory,request_sha256,response_sha256,receipt_sha256 FROM attempts WHERE run_id=?1 AND request_id=?2",params![run_id,request_id],read_attempt).optional().map_err(db)?.ok_or_else(||invalid("请求不在本运行中。"))
    }
    fn attempt_directory(&self, run_id: &str, attempt: &Attempt) -> Result<PathBuf, ProjectError> {
        self.run_directory(run_id)?;
        let expected = PathBuf::from("runs")
            .join(run_id)
            .join("attempts")
            .join(&attempt.request_id);
        if attempt.directory != expected {
            return Err(invalid("请求目录不符合应用拥有范围。"));
        }
        let saved: String = self
            .connection
            .query_row(
                "SELECT directory_identity FROM attempts WHERE run_id=?1 AND request_id=?2",
                params![run_id, attempt.request_id],
                |r| r.get(0),
            )
            .map_err(db)?;
        let path = project::check_components(&self.root.join(&expected))?;
        if identity(&path)? != decode::<Identity>(&saved)? {
            return Err(invalid("请求目录被替换。"));
        }
        Ok(path)
    }
    pub(super) fn update_attempt(
        &self,
        run_id: &str,
        attempt: &Attempt,
    ) -> Result<(), ProjectError> {
        self.run_directory(run_id)?;
        let old = self.attempt(run_id, &attempt.request_id)?;
        same_attempt(&old, attempt)?;
        let path = self.attempt_directory(run_id, attempt)?;
        if ![
            "prepared",
            "reserved",
            "may_send",
            "sending",
            "sent",
            "response_saved",
            "accepted",
            "failed",
            "unknown",
            "no_request",
        ]
        .contains(&attempt.state.as_str())
            || attempt
                .response_sha256
                .as_ref()
                .is_some_and(|s| !domain::is_sha(s))
            || attempt
                .receipt_sha256
                .as_ref()
                .is_some_and(|s| !domain::is_sha(s))
        {
            return Err(invalid("请求状态或原响应摘要无效。"));
        }
        if old.state == "accepted" || attempt.state == "accepted" {
            return Err(invalid("成功请求只能经原子IR提交确认。"));
        }
        if matches!(old.state.as_str(), "failed" | "no_request") && old.state != attempt.state
            || old.state == "response_saved"
                && !matches!(
                    attempt.state.as_str(),
                    "response_saved" | "failed" | "unknown"
                )
            || old.state == "unknown"
                && !matches!(
                    attempt.state.as_str(),
                    "unknown" | "response_saved" | "failed"
                )
            || matches!(old.state.as_str(), "sending" | "may_send" | "sent")
                && matches!(attempt.state.as_str(), "prepared" | "reserved")
        {
            return Err(invalid("请求状态不能退回重新发送阶段。"));
        }
        if old.response_sha256.is_some() && old.response_sha256 != attempt.response_sha256
            || old.receipt_sha256.is_some() && old.receipt_sha256 != attempt.receipt_sha256
        {
            return Err(invalid("已封存原响应或回执不能覆盖。"));
        }
        if let Some(sha) = &attempt.response_sha256 {
            let (_guard, bytes) = artifact(&path.join("response.bin"), 16 * 1024 * 1024)?;
            if sha256(&bytes) != *sha {
                return Err(invalid("实际原响应摘要不符。"));
            }
        }
        if let Some(sha) = &attempt.receipt_sha256 {
            let (_guard, bytes) = artifact(&path.join("receipt.json"), 65536)?;
            if sha256(&bytes) != *sha {
                return Err(invalid("实际回执摘要不符。"));
            }
        }
        let tx = self.transaction()?;
        tx.execute("UPDATE attempts SET state=?3,response_sha256=?4,receipt_sha256=?5 WHERE run_id=?1 AND request_id=?2",params![run_id,attempt.request_id,attempt.state,attempt.response_sha256,attempt.receipt_sha256]).map_err(db)?;
        tx.commit().map_err(db)
    }
    pub(super) fn mark_unit_failed(
        &self,
        run_id: &str,
        unit_id: &str,
        code: &str,
        message: &str,
    ) -> Result<(), ProjectError> {
        self.unit(run_id, unit_id)?;
        let tx = self.transaction()?;
        tx.execute("UPDATE units SET status='failed' WHERE run_id=?1 AND unit_id=?2 AND status!='confirmed'",params![run_id,unit_id]).map_err(db)?;
        tx.execute("UPDATE attempts SET state='failed' WHERE run_id=?1 AND unit_id=?2 AND state NOT IN ('accepted','failed','no_request')",params![run_id,unit_id]).map_err(db)?;
        add_issue(
            &tx,
            run_id,
            &format!("unit:{unit_id}"),
            code,
            &json!({"unit_id":unit_id,"message":message}),
        )?;
        tx.commit().map_err(db)
    }
    pub(super) fn commit_batch(
        &self,
        run_id: &str,
        unit: &Unit,
        attempt: &Attempt,
        batch: AcceptedBatch,
    ) -> Result<(), ProjectError> {
        self.run_directory(run_id)?;
        if encoded(&self.unit(run_id, &unit.unit_id)?)? != encoded(unit)? {
            return Err(invalid("接受批次的任务输入改变。"));
        }
        let old = self.attempt(run_id, &attempt.request_id)?;
        same_attempt(&old, attempt)?;
        let c = &batch.candidates;
        if c.schema_version != 1
            || c.request_id != attempt.request_id
            || attempt.unit_id != unit.unit_id
            || c.binding.project_id != self.project_id
            || c.binding.source_id != unit.source_id
            || c.binding.content_sha256 != unit.source_sha256
        {
            return Err(invalid("候选批次的请求、项目或来源绑定不符。"));
        }
        let hash=sha256(encoded(&json!({"candidates":c,"allowed_existing_concepts":batch.allowed_existing_concepts,"unit":unit,"request_sha256":attempt.request_sha256}))?.as_bytes());
        let tx = self.transaction()?;
        let prior: Option<String> = tx
            .query_row(
                "SELECT batch_sha256 FROM batches WHERE request_id=?1",
                [&attempt.request_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)?;
        if let Some(prior) = prior {
            if prior == hash {
                return Ok(());
            }
            return Err(invalid("同一请求已经确认不同候选，拒绝替换。"));
        }
        if old.state == "accepted" || old.state == "failed" || old.response_sha256.is_none() {
            return Err(invalid("请求尚无封存响应或已经终止。"));
        }
        let directory = self.attempt_directory(run_id, &old)?;
        let (_request_guard, request_bytes) =
            artifact(&directory.join("request-body.json"), 256 * 1024)?;
        let (_response_guard, response_bytes) =
            artifact(&directory.join("response.bin"), 16 * 1024 * 1024)?;
        let (_receipt_guard, receipt_bytes) = artifact(&directory.join("receipt.json"), 65536)?;
        if sha256(&request_bytes) != old.request_sha256
            || old.response_sha256.as_deref() != Some(sha256(&response_bytes).as_str())
            || old.receipt_sha256.as_deref() != Some(sha256(&receipt_bytes).as_str())
        {
            return Err(invalid("原请求、原响应或回执未与封存摘要绑定。"));
        }
        let receipt: super::provider::Receipt =
            serde_json::from_slice(&receipt_bytes).map_err(|e| invalid(e.to_string()))?;
        if receipt.schema_version != 1
            || receipt.request_id != attempt.request_id
            || receipt.request_sha256 != old.request_sha256
            || receipt.status != Some(200)
            || receipt.network_attempts != 1
            || !receipt.complete
            || receipt.failure_code.is_some()
            || receipt.response_sha256 != sha256(&response_bytes)
            || receipt.response_bytes != response_bytes.len() as u64
        {
            return Err(invalid("成功候选缺少完整200原传输回执。"));
        }
        let body: Value =
            serde_json::from_slice(&request_bytes).map_err(|e| invalid(e.to_string()))?;
        let content = body["messages"]
            .as_array()
            .and_then(|messages| messages.iter().find(|m| m["role"] == "user"))
            .and_then(|m| m["content"].as_str())
            .ok_or_else(|| invalid("原请求缺输入JSON。"))?;
        let input: Value = serde_json::from_str(content).map_err(|e| invalid(e.to_string()))?;
        let contexts: Vec<ConceptContext> =
            serde_json::from_value(input["request"]["existing_concepts"].clone())
                .map_err(|e| invalid(e.to_string()))?;
        let original_allowed = contexts
            .iter()
            .map(|context| &context.concept)
            .collect::<Vec<_>>();
        if encoded(&original_allowed)? != encoded(&batch.allowed_existing_concepts)? {
            return Err(invalid("接受概念上下文未精确绑定实际请求。"));
        }
        let prepared = super::provider::prepare(
            &self.project_id,
            run_id,
            &attempt.request_id,
            unit,
            &self.config(run_id)?,
            &contexts,
        )?;
        if prepared.body != request_bytes
            || encoded(&super::provider::decode(
                &response_bytes,
                &prepared,
                &attempt.request_id,
            )?)? != encoded(&batch.candidates)?
        {
            return Err(invalid("候选或请求输入不等于原件重验结果。"));
        }
        let page = corpus::page_revision(
            &self.project,
            &unit.source_id,
            unit.page_number,
            &unit.page_sha256,
        )?;
        if page["text"]
            .as_str()
            .and_then(|t| t.get(unit.start_byte..unit.end_byte))
            != Some(unit.text.as_str())
        {
            return Err(invalid("固定证据输入不符合原页正文。"));
        }
        let mut allowed = BTreeMap::new();
        for concept in &batch.allowed_existing_concepts {
            uuid(&concept.concept_id)?;
            let stored: Option<(String, String)> = tx
                .query_row(
                    "SELECT json,sha256 FROM concepts WHERE run_id=?1 AND concept_id=?2",
                    params![run_id, concept.concept_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db)?;
            let expected = encoded(concept)?;
            if stored
                .as_ref()
                .is_none_or(|(raw, pin)| raw != &expected || sha256(raw.as_bytes()) != *pin)
                || allowed
                    .insert(concept.concept_id.as_str(), concept)
                    .is_some()
            {
                return Err(invalid("提供的既存概念上下文未与保存身份精确绑定。"));
            }
        }
        let mut concepts = c.concepts.clone();
        for concept in allowed.values() {
            match concepts
                .iter()
                .find(|entry| entry.concept_id == concept.concept_id)
            {
                Some(entry) if encoded(entry)? != encoded(*concept)? => {
                    return Err(invalid("模型不得修改明确提供的既存概念修订。"));
                }
                Some(_) => (),
                None => concepts.push((*concept).clone()),
            }
        }
        let issues = domain::validate_basics(
            &concepts,
            c.evidence.iter().map(|e| e.evidence_id.as_str()),
            &c.atoms.iter().map(|a| &a.atom).collect::<Vec<_>>(),
        );
        if !issues.is_empty()
            || c.atoms.is_empty()
            || c.concepts.len() > 1024
            || c.atoms.len() > 1024
            || c.evidence.len() > 1024
        {
            return Err(error(
                "COMPILE_CANDIDATES",
                "候选结构、修订、明文或引用规则未通过。",
            ));
        }
        let mut evidence_map = BTreeMap::new();
        for evidence in &c.evidence {
            if evidence.unit_id != unit.unit_id
                || evidence.excerpt_start_byte < unit.start_byte
                || evidence.excerpt_end_byte > unit.end_byte
                || evidence.excerpt_start_byte >= evidence.excerpt_end_byte
            {
                return Err(invalid("证据不在本真实来源单元中。"));
            }
            let excerpt = unit
                .text
                .get(
                    evidence.excerpt_start_byte - unit.start_byte
                        ..evidence.excerpt_end_byte - unit.start_byte,
                )
                .ok_or_else(|| invalid("证据不是UTF8字符边界。"))?;
            if excerpt != evidence.quote || sha256(excerpt.as_bytes()) != evidence.excerpt_sha256 {
                return Err(invalid("证据引文或摘要与真实UTF8切片不符。"));
            }
            evidence_map.insert(evidence.evidence_id.as_str(), Uuid::new_v4().to_string());
        }
        let mut concept_map = BTreeMap::new();
        for concept in &concepts {
            if allowed.contains_key(concept.concept_id.as_str()) {
                concept_map.insert(concept.concept_id.as_str(), concept.concept_id.clone());
                continue;
            }
            let id = Uuid::new_v4().to_string();
            let mut saved = concept.clone();
            saved.concept_id = id.clone();
            saved.revision = 1;
            let same_name: i64 = tx
                .query_row(
                    "SELECT count(*) FROM concepts WHERE run_id=?1 AND name=?2",
                    params![run_id, concept.canonical_name],
                    |r| r.get(0),
                )
                .map_err(db)?;
            if same_name > 0 {
                add_issue(
                    &tx,
                    run_id,
                    &format!("concept:{id}"),
                    "CONCEPT_IDENTITY_UNRESOLVED",
                    &json!({"concept_id":id,"canonical_name":concept.canonical_name,"message":"同名不证明同一概念；已保留独立身份，等待语义协调。"}),
                )?;
            }
            let raw = encoded(&saved)?;
            tx.execute(
                "INSERT INTO concepts VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    run_id,
                    id,
                    attempt.request_id,
                    concept.concept_id,
                    concept.canonical_name,
                    raw,
                    sha256(raw.as_bytes())
                ],
            )
            .map_err(db)?;
            concept_map.insert(concept.concept_id.as_str(), id);
        }
        for e in &c.evidence {
            let id = &evidence_map[e.evidence_id.as_str()];
            let saved = json!({"schema_version":1,"evidence_id":id,"revision":1,"source_id":unit.source_id,"source_sha256":unit.source_sha256,"page_number":unit.page_number,"page_sha256":unit.page_sha256,"unit_id":unit.unit_id,"start_byte":e.excerpt_start_byte,"end_byte":e.excerpt_end_byte,"excerpt_sha256":e.excerpt_sha256,"quote":e.quote,"bbox":unit.bbox,"request_id":attempt.request_id});
            let raw = encoded(&saved)?;
            tx.execute(
                "INSERT INTO evidence VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    run_id,
                    id,
                    attempt.request_id,
                    e.evidence_id,
                    raw,
                    sha256(raw.as_bytes())
                ],
            )
            .map_err(db)?;
        }
        for candidate in &c.atoms {
            let atom = &candidate.atom;
            if atom.required
                || atom.verification != domain::Verification::Unverified
                || !atom.evidence_links.iter().any(|link| {
                    link.relation == domain::EvidenceRelation::Supports
                        && evidence_map.contains_key(link.evidence_id.as_str())
                })
            {
                return Err(invalid(
                    "模型原子必须保持unverified、required=false和有效supports。",
                ));
            }
            validate_measurement(candidate)?;
            let mut saved = atom.clone();
            saved.atom_id = Uuid::new_v4().to_string();
            saved.revision = 1;
            for id in &mut saved.concept_ids {
                *id = concept_map
                    .get(id.as_str())
                    .ok_or_else(|| invalid("概念身份未解析。"))?
                    .clone();
            }
            for link in &mut saved.evidence_links {
                link.evidence_id = evidence_map
                    .get(link.evidence_id.as_str())
                    .ok_or_else(|| invalid("证据身份未解析。"))?
                    .clone();
            }
            let raw = encoded(&saved)?;
            let measurement = encoded(&candidate.measurement)?;
            tx.execute(
                "INSERT INTO atoms VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    run_id,
                    saved.atom_id,
                    attempt.request_id,
                    atom.atom_id,
                    raw,
                    sha256(raw.as_bytes()),
                    measurement,
                    sha256(measurement.as_bytes())
                ],
            )
            .map_err(db)?;
            for concept in &saved.concept_ids {
                tx.execute(
                    "INSERT INTO atom_concepts VALUES(?1,?2,?3)",
                    params![run_id, saved.atom_id, concept],
                )
                .map_err(db)?;
            }
        }
        tx.execute(
            "UPDATE runs SET ir_revision=ir_revision+1 WHERE run_id=?1",
            [run_id],
        )
        .map_err(db)?;
        let revision: i64 = tx
            .query_row(
                "SELECT ir_revision FROM runs WHERE run_id=?1",
                [run_id],
                |r| r.get(0),
            )
            .map_err(db)?;
        tx.execute(
            "INSERT INTO batches VALUES(?1,?2,?3,?4,?5)",
            params![attempt.request_id, run_id, unit.unit_id, hash, revision],
        )
        .map_err(db)?;
        tx.execute(
            "UPDATE attempts SET state='accepted' WHERE request_id=?1",
            [&attempt.request_id],
        )
        .map_err(db)?;
        tx.execute(
            "UPDATE units SET status='confirmed' WHERE run_id=?1 AND unit_id=?2",
            params![run_id, unit.unit_id],
        )
        .map_err(db)?;
        tx.commit().map_err(db)
    }
    pub(super) fn query(
        &self,
        run_id: &str,
        kind: &str,
        offset: u64,
        limit: u32,
        search: &str,
    ) -> Result<Value, ProjectError> {
        self.run_directory(run_id)?;
        if !(1..=50).contains(&limit) || search.len() > 4096 {
            return Err(error(
                "COMPILE_QUERY",
                "查询须分页1至50项，搜索词不超过4096字节。",
            ));
        }
        if matches!(kind, "pages" | "units") {
            if !search.is_empty() {
                return Err(error("COMPILE_QUERY", "物理范围分页不提供全文搜索。"));
            }
            return self.scope_query(run_id, kind, offset, limit);
        }
        let table = match kind {
            "concepts" => "concepts",
            "atoms" => "atoms",
            "evidence" => "evidence",
            "issues" => "issues",
            _ => return Err(error("COMPILE_QUERY", "查询类型不受支持。")),
        };
        let sql = format!(
            "SELECT count(*) FROM {table} WHERE run_id=?1 AND instr(lower(json),lower(?2))>0"
        );
        let total: i64 = self
            .connection
            .query_row(&sql, params![run_id, search], |r| r.get(0))
            .map_err(db)?;
        let order = match kind {
            "concepts" => "concept_id",
            "atoms" => "atom_id",
            "evidence" => "evidence_id",
            _ => "issue_id",
        };
        let columns = if kind == "atoms" {
            "json,sha256,measurement_json,measurement_sha256"
        } else {
            "json,sha256,'null','unused'"
        };
        let sql = format!(
            "SELECT {columns} FROM {table} WHERE run_id=?1 AND instr(lower(json),lower(?2))>0 ORDER BY {order} LIMIT ?3 OFFSET ?4"
        );
        let mut query = self.connection.prepare(&sql).map_err(db)?;
        let mut items = vec![];
        for row in query
            .query_map(params![run_id, search, limit, integer(offset)?], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(db)?
        {
            let (raw, hash, measurement, measurement_sha) = row.map_err(db)?;
            if sha256(raw.as_bytes()) != hash {
                return Err(invalid("保存IR对象的字节摘要不符。"));
            }
            let mut value: Value = decode(&raw)?;
            if kind == "atoms" {
                value["measurement"] = pinned(&measurement, &measurement_sha)?;
            }
            items.push(value);
        }
        Ok(
            json!({"schema_version":1,"run_id":run_id,"kind":kind,"offset":offset,"limit":limit,"total":total,"items":items}),
        )
    }
    fn scope_query(
        &self,
        run_id: &str,
        kind: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Value, ProjectError> {
        let mut items = Vec::with_capacity(limit as usize);
        let total;
        if kind == "pages" {
            let sources = self.sources(run_id)?;
            total = sources.iter().try_fold(0u64, |n, source| {
                n.checked_add(u64::from(source.physical_pages))
                    .ok_or_else(|| invalid("物理范围计数溢出。"))
            })?;
            let mut skip = offset;
            for source in sources {
                let count = u64::from(source.physical_pages);
                if skip >= count {
                    skip -= count;
                    continue;
                }
                let first = skip as u32 + 1;
                skip = 0;
                for number in first..=source.physical_pages {
                    let saved:Option<(String,String,i64,i64,i64)>=self.connection.query_row("SELECT p.page_sha256,p.status,(SELECT count(*) FROM units u WHERE u.run_id=p.run_id AND u.source_id=p.source_id AND u.page_number=p.page_number),(SELECT count(*) FROM units u WHERE u.run_id=p.run_id AND u.source_id=p.source_id AND u.page_number=p.page_number AND u.status='confirmed'),(SELECT count(*) FROM units u WHERE u.run_id=p.run_id AND u.source_id=p.source_id AND u.page_number=p.page_number AND u.status='failed') FROM pages p WHERE p.run_id=?1 AND p.source_id=?2 AND p.page_number=?3",params![run_id,source.source_id,number],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(db)?;
                    let (sha, status, units, confirmed, failed) = saved
                        .map(|(sha, status, units, confirmed, failed)| {
                            (Some(sha), status, units, confirmed, failed)
                        })
                        .unwrap_or((None, "unprepared".into(), 0, 0, 0));
                    items.push(json!({"source_id":source.source_id,"source_sha256":source.source_sha256,"page_number":number,"page_sha256":sha,"status":status,"units_total":units,"units_confirmed":confirmed,"units_failed":failed}));
                    if items.len() == limit as usize {
                        break;
                    }
                }
                if items.len() == limit as usize {
                    break;
                }
            }
        } else {
            total = self.count("SELECT count(*) FROM units WHERE run_id=?1", run_id)?;
            let mut query=self.connection.prepare("SELECT u.unit_json,u.unit_sha256,u.status FROM units u JOIN sources s ON s.run_id=u.run_id AND s.source_id=u.source_id WHERE u.run_id=?1 ORDER BY s.ordinal,u.page_number,u.ordinal LIMIT ?2 OFFSET ?3").map_err(db)?;
            for row in query
                .query_map(params![run_id, limit, integer(offset)?], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })
                .map_err(db)?
            {
                let (raw, hash, status) = row.map_err(db)?;
                let unit: Unit = pinned(&raw, &hash)?;
                items.push(json!({"unit_id":unit.unit_id,"source_id":unit.source_id,"source_sha256":unit.source_sha256,"page_number":unit.page_number,"page_sha256":unit.page_sha256,"start_byte":unit.start_byte,"end_byte":unit.end_byte,"excerpt_sha256":sha256(unit.text.as_bytes()),"status":status,"issue_codes":unit.issue_codes}));
            }
        }
        Ok(
            json!({"schema_version":1,"run_id":run_id,"kind":kind,"offset":offset,"limit":limit,"total":total,"items":items}),
        )
    }
    pub(super) fn evidence(&self, run_id: &str, id: &str) -> Result<Value, ProjectError> {
        self.run_directory(run_id)?;
        uuid(id)?;
        let (raw, hash): (String, String) = self
            .connection
            .query_row(
                "SELECT json,sha256 FROM evidence WHERE run_id=?1 AND evidence_id=?2",
                params![run_id, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(db)?
            .ok_or_else(|| error("COMPILE_EVIDENCE_NOT_FOUND", "证据身份不在本运行中。"))?;
        if sha256(raw.as_bytes()) != hash {
            return Err(invalid("保存证据摘要不符。"));
        }
        decode(&raw)
    }
    pub(super) fn context_concepts(
        &self,
        run_id: &str,
        text: &str,
        limit: usize,
    ) -> Result<Vec<ConceptContext>, ProjectError> {
        self.run_directory(run_id)?;
        if !(1..=16).contains(&limit) {
            return Err(error("COMPILE_CONTEXT", "最多提供16项已有概念上下文。"));
        }
        let mut query=self.connection.prepare("SELECT concept_id,json,sha256 FROM concepts WHERE run_id=?1 AND instr(lower(?2),lower(name))>0 ORDER BY concept_id LIMIT ?3").map_err(db)?;
        let mut contexts = vec![];
        for row in query
            .query_map(params![run_id, text, integer(limit as u64)?], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(db)?
        {
            let (id, raw, hash) = row.map_err(db)?;
            if sha256(raw.as_bytes()) != hash {
                return Err(invalid("概念上下文字节摘要改变。"));
            }
            let concept = decode(&raw)?;
            let mut atoms=self.connection.prepare("SELECT a.json,a.sha256 FROM atoms a JOIN atom_concepts ac ON ac.run_id=a.run_id AND ac.atom_id=a.atom_id WHERE ac.run_id=?1 AND ac.concept_id=?2 ORDER BY a.atom_id LIMIT 3").map_err(db)?;
            let mut sample_atoms = vec![];
            for atom in atoms
                .query_map(params![run_id, id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(db)?
            {
                let (raw, hash) = atom.map_err(db)?;
                if sha256(raw.as_bytes()) != hash {
                    return Err(invalid("原子上下文摘要改变。"));
                }
                sample_atoms.push(decode(&raw)?);
            }
            contexts.push(ConceptContext {
                concept,
                sample_atoms,
            });
        }
        Ok(contexts)
    }
}

fn read_attempt(row: &rusqlite::Row<'_>) -> rusqlite::Result<Attempt> {
    Ok(Attempt {
        request_id: row.get(0)?,
        unit_id: row.get(1)?,
        attempt_no: row.get(2)?,
        state: row.get(3)?,
        directory: PathBuf::from(row.get::<_, String>(4)?),
        request_sha256: row.get(5)?,
        response_sha256: row.get(6)?,
        receipt_sha256: row.get(7)?,
    })
}
fn same_attempt(old: &Attempt, new: &Attempt) -> Result<(), ProjectError> {
    if old.request_id != new.request_id
        || old.unit_id != new.unit_id
        || old.attempt_no != new.attempt_no
        || old.directory != new.directory
        || old.request_sha256 != new.request_sha256
    {
        return Err(invalid("持久请求身份、目录或正文摘要变化。"));
    }
    Ok(())
}
fn artifact(path: &Path, limit: u64) -> Result<(File, Vec<u8>), ProjectError> {
    project::check_components(path)?;
    identity(path)?;
    let mut guard = project::read_guard(path)?;
    let before = guard.metadata().map_err(|e| ProjectError::io(path, e))?;
    if before.len() > limit {
        return Err(invalid("单请求原件超出边界，不读取或截断。"));
    }
    let handle =
        same_file::Handle::from_file(guard.try_clone().map_err(|e| ProjectError::io(path, e))?)
            .map_err(|e| ProjectError::io(path, e))?;
    let mut bytes = vec![];
    (&mut guard)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ProjectError::io(path, e))?;
    let after = guard.metadata().map_err(|e| ProjectError::io(path, e))?;
    if bytes.len() as u64 != before.len()
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || handle != same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?
    {
        return Err(invalid("单请求原件在读取期间变化。"));
    }
    identity(path)?;
    Ok((guard, bytes))
}
fn validate_measurement(
    candidate: &crate::extraction_model::CandidateAtom,
) -> Result<(), ProjectError> {
    use domain::ValueStatus;
    let atom = &candidate.atom;
    let plain = |s: &str| domain::nonempty(s) && domain::plain_text(s);
    if atom.kind == domain::AtomKind::ComparisonValue && candidate.measurement.is_none() {
        return Err(invalid("数值原子缺measurement。"));
    }
    if let Some(m) = &candidate.measurement {
        if m.value_status != atom.value_status {
            return Err(invalid("measurement状态不符。"));
        }
        match m.value_status {
            ValueStatus::Known
                if !m.value.as_deref().is_some_and(&plain)
                    || !m.unit.as_deref().is_some_and(&plain) =>
            {
                return Err(invalid("known数值或单位不是非空明文。"));
            }
            ValueStatus::Unknown if m.value.is_some() => {
                return Err(invalid("unknown数值必须为null。"));
            }
            ValueStatus::NotApplicable if m.value.is_some() || m.unit.is_some() => {
                return Err(invalid("not_applicable数值和单位须为null。"));
            }
            _ => (),
        }
        if m.unit
            .as_ref()
            .is_some_and(|s| !plain(s) || !atom.qualifiers.contains(s))
            || m.conditions
                .iter()
                .any(|s| !plain(s) || !atom.qualifiers.contains(s))
        {
            return Err(invalid("测量单位或条件未完整保留在限定信息中。"));
        }
    }
    Ok(())
}

fn add_issue(
    tx: &Transaction<'_>,
    run_id: &str,
    scope: &str,
    code: &str,
    value: &Value,
) -> Result<(), ProjectError> {
    let id = Uuid::new_v4().to_string();
    let raw = encoded(
        &json!({"issue_id":id,"scope":scope,"code":code,"detail":value,"resolution":"pending"}),
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO issues VALUES(?1,?2,?3,?4,?5,?6)",
        params![id, run_id, scope, code, raw, sha256(raw.as_bytes())],
    )
    .map_err(db)?;
    Ok(())
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
