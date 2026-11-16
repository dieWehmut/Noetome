//! Production token authority migrated from the selected cumulative JSON ledger.
//! Credentials and HTTP are deliberately outside this module. The original JSON
//! bytes and complete history stay immutable; all new accounting is transactional.

use crate::{
    compile_identity::{Identity, identity, sqlite_identity},
    project::{self, ProjectError},
    text::sha256,
};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const APPLICATION_ID: i64 = 0x4e505544;
const SCHEMA_VERSION: u32 = 1;
const BASELINE_LIMIT: u64 = 16 * 1024 * 1024;
const SCHEMA: &str = "
CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),owner_json TEXT NOT NULL,baseline BLOB NOT NULL,head_sequence INTEGER NOT NULL,head_sha256 TEXT NOT NULL);
CREATE TABLE history(ordinal INTEGER PRIMARY KEY CHECK(ordinal>=0),record_json TEXT NOT NULL,record_sha256 TEXT NOT NULL);
CREATE TABLE events(sequence INTEGER PRIMARY KEY CHECK(sequence>0),previous_sha256 TEXT NOT NULL,payload_json TEXT NOT NULL,payload_sha256 TEXT NOT NULL);";

macro_rules! object {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $(pub $field: $ty),* }
    };
}

object!(BudgetAttemptKey {
    run_id: String,
    unit_id: String,
    attempt_no: u32,
    request_id: String
});
object!(BudgetRequest {
    key: BudgetAttemptKey,
    request_sha256: String,
    reserved_tokens: u64
});
object!(BudgetUsage {
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64
});
object!(BudgetSettlement { network_attempts: u32, usage: Option<BudgetUsage>, receipt_sha256: String });

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetAttemptState {
    Reserved,
    Sent,
    Unknown,
    Known,
    NotSent,
}

object!(BudgetAttempt { request: BudgetRequest, state: BudgetAttemptState, settlement: Option<BudgetSettlement> });
object!(BudgetStatus {
    schema_version: u32,
    authority_id: String,
    baseline_sha256: String,
    approved_total_tokens: u64,
    accounted_tokens: u64,
    pending_reserved_tokens: u64,
    remaining_tokens: u64,
    overrun_tokens: u64,
    model_attempts: u64,
    history_count: u64,
    attempt_count: u64,
    stopped: bool
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    schema_version: u32,
    producer: String,
    authority_id: String,
    baseline_sha256: String,
    baseline_bytes: u64,
    baseline_readonly: bool,
    ledger_identity: Identity,
    parent_identity: Identity,
    database_identity: Identity,
    marker_identity: Identity,
    approved_total_tokens: u64,
    accounted_tokens: u64,
    pending_reserved_tokens: u64,
    model_attempts: u64,
    history_count: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyLedger {
    schema_version: u32,
    approved_total_tokens: u64,
    accounted_tokens: u64,
    pending_reserved_tokens: u64,
    remaining_tokens: u64,
    model_attempts: u64,
    records: Vec<Value>,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Event {
    Reserved {
        request: BudgetRequest,
    },
    Sent {
        key: BudgetAttemptKey,
    },
    Settled {
        key: BudgetAttemptKey,
        settlement: BudgetSettlement,
    },
}

struct Projection {
    status: BudgetStatus,
    attempts: BTreeMap<String, BudgetAttempt>,
    sequence: u64,
    last_sha256: String,
}

/// Keep the baseline deny-write capability for the lifetime of this authority.
/// Each operation rechecks the original bytes, ownership and complete event chain.
pub struct BudgetAuthority {
    ledger: PathBuf,
    database: PathBuf,
    marker: PathBuf,
    baseline_guard: File,
    owner: Owner,
    owner_json: String,
    connection: Connection,
}

fn invalid(message: impl Into<String>) -> ProjectError {
    ProjectError::new("PRODUCTION_BUDGET_INTEGRITY", 3, message)
}
fn database(error: rusqlite::Error) -> ProjectError {
    ProjectError::new(
        "PRODUCTION_BUDGET_DATABASE",
        3,
        format!("生产额度事务失败，现场保留：{error}"),
    )
}
fn encode<T: Serialize>(value: &T) -> Result<String, ProjectError> {
    serde_json::to_string(value).map_err(|_| invalid("预算记录不能编码。"))
}
fn canonical<T: Serialize + serde::de::DeserializeOwned>(raw: &str) -> Result<T, ProjectError> {
    let value: T = serde_json::from_str(raw).map_err(|_| invalid("预算记录字段或JSON无效。"))?;
    if encode(&value)? != raw {
        return Err(invalid("预算记录重复键或非规范编码。"));
    }
    Ok(value)
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
fn key(value: &BudgetAttemptKey) -> Result<String, ProjectError> {
    if !token(&value.run_id)
        || !token(&value.unit_id)
        || !token(&value.request_id)
        || value.attempt_no == 0
    {
        return Err(invalid("请求身份必须完整且有正attempt号。"));
    }
    encode(value)
}
fn usage(value: &BudgetUsage) -> Result<(), ProjectError> {
    if value.input_tokens.checked_add(value.output_tokens) != Some(value.total_tokens) {
        return Err(invalid(
            "有效usage必须满足input+output=total；reasoning应已包含在output。",
        ));
    }
    Ok(())
}
fn settlement(value: &BudgetSettlement) -> Result<(), ProjectError> {
    if !hash(&value.receipt_sha256) || value.network_attempts > 1 {
        return Err(invalid("结算须绑定完整传输收据SHA及单次网络尝试。"));
    }
    if let Some(ref actual) = value.usage {
        usage(actual)?;
        if value.network_attempts == 0 && actual.total_tokens != 0 {
            return Err(invalid("无网络尝试不能结算非零usage。"));
        }
    }
    Ok(())
}
fn add(a: u64, b: u64) -> Result<u64, ProjectError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("累计额度整数溢出。"))
}

/// Suffixes preserve the selected ledger name and keep all authority files beside it.
pub fn default_database_path(ledger: &Path) -> PathBuf {
    let mut value = ledger.as_os_str().to_os_string();
    value.push(".production.sqlite");
    value.into()
}
pub fn authority_marker_path(ledger: &Path) -> PathBuf {
    let mut value = ledger.as_os_str().to_os_string();
    value.push(".production-authority.json");
    value.into()
}

fn read_baseline(file: &mut File) -> Result<Vec<u8>, ProjectError> {
    use std::io::{Seek, SeekFrom};
    let length = file
        .metadata()
        .map_err(|_| invalid("不能读取原账本属性。"))?
        .len();
    if length == 0 || length > BASELINE_LIMIT {
        return Err(invalid("原账本为空或超出迁移读取限额。"));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| invalid("不能定位原账本。"))?;
    let mut bytes = Vec::new();
    file.take(BASELINE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("不能读取原账本。"))?;
    if bytes.len() as u64 != length {
        return Err(invalid("原账本读取期间长度改变。"));
    }
    Ok(bytes)
}

fn legacy(bytes: &[u8]) -> Result<LegacyLedger, ProjectError> {
    // Value normally keeps the last duplicate object field. Validate recursively
    // before preserving opaque historical fields so migration cannot hide one.
    unique_json(bytes)?;
    let ledger: LegacyLedger =
        serde_json::from_slice(bytes).map_err(|_| invalid("累计原账本字段或JSON无效。"))?;
    if ledger.schema_version != 1
        || ledger.approved_total_tokens == 0
        || ledger.approved_total_tokens > i64::MAX as u64
        || ledger.updated_at.is_empty()
    {
        return Err(invalid("累计原账本策略或版本无效。"));
    }
    let mut confirmed = 0u64;
    let mut pending = 0u64;
    let mut attempts = 0u64;
    let mut ids = BTreeSet::new();
    for record in &ledger.records {
        let id = record
            .get("invocation_id")
            .and_then(Value::as_str)
            .filter(|v| token(v))
            .ok_or_else(|| invalid("原历史缺少请求身份。"))?;
        if !ids.insert(id) {
            return Err(invalid("原历史请求身份重复。"));
        }
        let reservation = record
            .get("reservation_tokens")
            .and_then(Value::as_u64)
            .filter(|v| *v > 0)
            .ok_or_else(|| invalid("原历史预留无效。"))?;
        let network = match record.get("network_attempts") {
            Some(Value::Null) => None,
            Some(v) => Some(
                v.as_u64()
                    .filter(|v| *v <= 1)
                    .ok_or_else(|| invalid("原历史网络尝试无效。"))?,
            ),
            None => return Err(invalid("原历史缺少网络尝试。")),
        };
        let status = record
            .get("status")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("原历史状态无效。"))?;
        match record.get("confirmed_total_tokens") {
            Some(Value::Null) => {
                if !matches!(status, "reserved" | "usage_unknown") || network == Some(0) {
                    return Err(invalid("原未决历史无效。"));
                }
                pending = add(pending, reservation)?;
            }
            Some(v) => {
                let actual = v.as_u64().ok_or_else(|| invalid("原历史用量无效。"))?;
                if status == "settled_no_request" {
                    if network != Some(0) || actual != 0 {
                        return Err(invalid("原无请求历史无效。"));
                    }
                } else if !matches!(
                    status,
                    "settled_valid_usage_response_failed"
                        | "settled_valid_usage_candidates_validated"
                        | "settled_valid_usage_scan"
                        | "settled_valid_usage_text"
                ) || network != Some(1)
                {
                    return Err(invalid("原已结算历史无效。"));
                }
                confirmed = add(confirmed, actual)?;
            }
            None => return Err(invalid("原历史缺少用量字段。")),
        }
        attempts = add(attempts, network.unwrap_or(0))?;
    }
    let occupied = add(confirmed, pending)?;
    if confirmed != ledger.accounted_tokens
        || pending != ledger.pending_reserved_tokens
        || attempts != ledger.model_attempts
        || occupied > ledger.approved_total_tokens
        || ledger.remaining_tokens != ledger.approved_total_tokens - occupied
    {
        return Err(invalid("原账本汇总与完整历史不一致，拒绝新额度初始化。"));
    }
    Ok(ledger)
}

fn unique_json(bytes: &[u8]) -> Result<(), ProjectError> {
    struct Unique;
    impl<'de> Deserialize<'de> for Unique {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = Unique;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("JSON without duplicate object keys")
                }
                fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_none<E: serde::de::Error>(self) -> Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_unit<E: serde::de::Error>(self) -> Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(
                    self,
                    mut sequence: A,
                ) -> Result<Unique, A::Error> {
                    while sequence.next_element::<Unique>()?.is_some() {}
                    Ok(Unique)
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> Result<Unique, A::Error> {
                    let mut names = BTreeSet::new();
                    while let Some(name) = map.next_key::<String>()? {
                        if !names.insert(name.to_lowercase()) {
                            return Err(serde::de::Error::custom("duplicate ledger JSON field"));
                        }
                        map.next_value::<Unique>()?;
                    }
                    Ok(Unique)
                }
            }
            deserializer.deserialize_any(Visitor)
        }
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    Unique::deserialize(&mut deserializer)
        .map_err(|_| invalid("累计原账本包含重复键或无效JSON。"))?;
    deserializer
        .end()
        .map_err(|_| invalid("原账本JSON尾部无效。"))
}

fn sqlite_sidecars(path: &Path) -> Result<(), ProjectError> {
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        let sidecar = PathBuf::from(name);
        project::check_components(&sidecar)?;
        if project::exists(&sidecar)? {
            sqlite_identity(&sidecar)?;
        }
    }
    Ok(())
}

fn open_connection(path: &Path, writable: bool) -> Result<Connection, ProjectError> {
    sqlite_identity(path)?;
    sqlite_sidecars(path)?;
    let flags = if writable {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let connection = Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(database)?;
    connection
        .busy_timeout(Duration::from_secs(10))
        .map_err(database)?;
    connection
        .execute_batch(
            "PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;",
        )
        .map_err(database)?;
    Ok(connection)
}

impl BudgetAuthority {
    /// Explicit one-time migration. A partial marker/database is preserved and
    /// blocks both retry initialization and legacy helpers; it is never reset.
    pub fn migrate(ledger: &Path) -> Result<Self, ProjectError> {
        let ledger = project::check_components(ledger)?;
        let database_path = default_database_path(&ledger);
        let marker = authority_marker_path(&ledger);
        if database_path
            .try_exists()
            .map_err(|_| invalid("不能检查生产账本。"))?
            || marker
                .try_exists()
                .map_err(|_| invalid("不能检查生产标记。"))?
        {
            return Err(ProjectError::new(
                "PRODUCTION_BUDGET_EXISTS",
                3,
                "生产额度权威或迁移现场已存在；只能显式open，不能重新初始化。",
            ));
        }
        let mut baseline_guard = project::read_guard(&ledger)?;
        let baseline = read_baseline(&mut baseline_guard)?;
        let initial = legacy(&baseline)?;
        let mut marker_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
            .map_err(|e| ProjectError::io(&marker, e))?;
        marker_file
            .write_all(b"{\"migration_incomplete\":true}\n")
            .map_err(|e| ProjectError::io(&marker, e))?;
        marker_file
            .sync_all()
            .map_err(|e| ProjectError::io(&marker, e))?;
        let database_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&database_path)
            .map_err(|e| ProjectError::io(&database_path, e))?;
        database_file
            .sync_all()
            .map_err(|e| ProjectError::io(&database_path, e))?;
        // Freezing the bytes also prevents the already installed legacy helper
        // from reopening the old JSON for write while the new app is closed.
        let mut permissions = fs::metadata(&ledger)
            .map_err(|e| ProjectError::io(&ledger, e))?
            .permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&ledger, permissions).map_err(|e| ProjectError::io(&ledger, e))?;
        let owner = Owner {
            schema_version: SCHEMA_VERSION,
            producer: "noetome-production-budget".into(),
            authority_id: uuid::Uuid::new_v4().to_string(),
            baseline_sha256: sha256(&baseline),
            baseline_bytes: baseline.len() as u64,
            baseline_readonly: true,
            ledger_identity: identity(&ledger)?,
            parent_identity: identity(ledger.parent().ok_or_else(|| invalid("原账本无父目录。"))?)?,
            database_identity: sqlite_identity(&database_path)?,
            marker_identity: identity(&marker)?,
            approved_total_tokens: initial.approved_total_tokens,
            accounted_tokens: initial.accounted_tokens,
            pending_reserved_tokens: initial.pending_reserved_tokens,
            model_attempts: initial.model_attempts,
            history_count: initial.records.len() as u64,
        };
        drop(database_file);
        let owner_json = encode(&owner)?;
        let mut connection = open_connection(&database_path, true)?;
        connection.execute_batch(&format!("PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version={SCHEMA_VERSION}; PRAGMA journal_mode=DELETE;" )).map_err(database)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database)?;
        transaction.execute_batch(SCHEMA).map_err(database)?;
        transaction
            .execute(
                "INSERT INTO metadata VALUES(1,?1,?2,0,?3)",
                params![owner_json, baseline, owner.baseline_sha256],
            )
            .map_err(database)?;
        for (ordinal, record) in initial.records.iter().enumerate() {
            let raw = encode(record)?;
            transaction
                .execute(
                    "INSERT INTO history VALUES(?1,?2,?3)",
                    params![ordinal as i64, raw, sha256(raw.as_bytes())],
                )
                .map_err(database)?;
        }
        transaction.commit().map_err(database)?;
        use std::io::{Seek, SeekFrom};
        marker_file
            .seek(SeekFrom::Start(0))
            .map_err(|e| ProjectError::io(&marker, e))?;
        marker_file
            .set_len(0)
            .map_err(|e| ProjectError::io(&marker, e))?;
        marker_file
            .write_all(owner_json.as_bytes())
            .map_err(|e| ProjectError::io(&marker, e))?;
        marker_file
            .sync_all()
            .map_err(|e| ProjectError::io(&marker, e))?;
        drop(marker_file);
        let mut authority = Self {
            ledger,
            database: database_path,
            marker,
            baseline_guard,
            owner,
            owner_json,
            connection,
        };
        authority.status()?;
        Ok(authority)
    }

    pub fn open(ledger: &Path) -> Result<Self, ProjectError> {
        let ledger = project::check_components(ledger)?;
        let database_path = default_database_path(&ledger);
        let marker = authority_marker_path(&ledger);
        let mut baseline_guard = project::read_guard(&ledger)?;
        project::checked_file(&marker)?;
        project::checked_file(&database_path)?;
        let owner_json =
            String::from_utf8(fs::read(&marker).map_err(|e| ProjectError::io(&marker, e))?)
                .map_err(|_| invalid("生产额度标记不是UTF-8。"))?;
        let owner: Owner = canonical(&owner_json)?;
        if owner.schema_version != SCHEMA_VERSION
            || owner.producer != "noetome-production-budget"
            || !hash(&owner.baseline_sha256)
            || !owner.baseline_readonly
        {
            return Err(invalid("生产额度owner策略无效；不重新初始化。"));
        }
        let baseline = read_baseline(&mut baseline_guard)?;
        if baseline.len() as u64 != owner.baseline_bytes
            || sha256(&baseline) != owner.baseline_sha256
        {
            return Err(invalid("原累计账本字节不再匹配迁移基线。"));
        }
        if sqlite_identity(&database_path)? != owner.database_identity {
            return Err(invalid("生产预算数据库身份改变，未打开替换内容。"));
        }
        let connection = open_connection(&database_path, true)?;
        let mut authority = Self {
            ledger,
            database: database_path,
            marker,
            baseline_guard,
            owner,
            owner_json,
            connection,
        };
        authority.status()?;
        Ok(authority)
    }

    fn guard(&mut self) -> Result<(), ProjectError> {
        sqlite_sidecars(&self.database)?;
        if !fs::metadata(&self.ledger)
            .map_err(|e| ProjectError::io(&self.ledger, e))?
            .permissions()
            .readonly()
        {
            return Err(invalid(
                "原累计账本只读属性被取消；不会静默恢复或重新发放余额。",
            ));
        }
        if identity(&self.ledger)? != self.owner.ledger_identity
            || identity(
                self.ledger
                    .parent()
                    .ok_or_else(|| invalid("原账本无父目录。"))?,
            )? != self.owner.parent_identity
            || sqlite_identity(&self.database)? != self.owner.database_identity
            || identity(&self.marker)? != self.owner.marker_identity
        {
            return Err(invalid("生产额度owner文件身份改变；现场保留。"));
        }
        if fs::read(&self.marker).map_err(|e| ProjectError::io(&self.marker, e))?
            != self.owner_json.as_bytes()
        {
            return Err(invalid("生产额度owner原件改变。"));
        }
        let baseline = read_baseline(&mut self.baseline_guard)?;
        if baseline.len() as u64 != self.owner.baseline_bytes
            || sha256(&baseline) != self.owner.baseline_sha256
        {
            return Err(invalid("原累计账本与迁移基线不符。"));
        }
        Ok(())
    }

    pub fn status(&mut self) -> Result<BudgetStatus, ProjectError> {
        self.guard()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(database)?;
        let status = projected(&transaction, &self.owner, &self.owner_json)?.status;
        transaction.commit().map_err(database)?;
        self.guard()?;
        Ok(status)
    }

    /// Idempotent reservation never restores a settled reservation. Different
    /// request IDs for the same logical attempt are refused across all runs.
    pub fn reserve(&mut self, request: &BudgetRequest) -> Result<BudgetAttempt, ProjectError> {
        let encoded_key = key(&request.key)?;
        if !hash(&request.request_sha256)
            || request.reserved_tokens == 0
            || request.reserved_tokens > i64::MAX as u64
        {
            return Err(invalid("请求SHA或Token预留无效。"));
        }
        self.guard()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database)?;
        let view = projected(&transaction, &self.owner, &self.owner_json)?;
        let result = if let Some(previous) = view.attempts.get(&encoded_key) {
            if previous.request != *request {
                return Err(invalid("重复请求身份的正文SHA或预留改变。"));
            }
            previous.clone()
        } else {
            for previous in view.attempts.values() {
                let k = &previous.request.key;
                if k.request_id == request.key.request_id
                    || (k.run_id == request.key.run_id
                        && k.unit_id == request.key.unit_id
                        && k.attempt_no == request.key.attempt_no)
                {
                    return Err(invalid("请求UUID或逻辑attempt已经被另一请求占用。"));
                }
            }
            if view.status.stopped || request.reserved_tokens > view.status.remaining_tokens {
                return Err(ProjectError::new(
                    "PRODUCTION_BUDGET_STOP",
                    3,
                    "累计剩余额度不足；已有消耗、未知承诺及未完成工作保留。",
                ));
            }
            append(
                &transaction,
                &view,
                Event::Reserved {
                    request: request.clone(),
                },
            )?;
            BudgetAttempt {
                request: request.clone(),
                state: BudgetAttemptState::Reserved,
                settlement: None,
            }
        };
        transaction.commit().map_err(database)?;
        self.guard()?;
        Ok(result)
    }

    /// A successful return grants send permission once. A repeat call cannot
    /// grant it again, even after restart or with an otherwise identical body.
    pub fn mark_sent(&mut self, attempt: &BudgetAttemptKey) -> Result<(), ProjectError> {
        let encoded_key = key(attempt)?;
        self.guard()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database)?;
        let view = projected(&transaction, &self.owner, &self.owner_json)?;
        let previous = view
            .attempts
            .get(&encoded_key)
            .ok_or_else(|| invalid("请求未预留。"))?;
        if previous.state != BudgetAttemptState::Reserved {
            return Err(ProjectError::new(
                "PRODUCTION_BUDGET_ALREADY_SENT",
                3,
                "原attempt发送许可已使用或结算；不得重发同一请求。",
            ));
        }
        if view.status.overrun_tokens > 0 {
            return Err(ProjectError::new(
                "PRODUCTION_BUDGET_STOP",
                3,
                "实际消耗已超累计上限；停止新增发送。",
            ));
        }
        append(
            &transaction,
            &view,
            Event::Sent {
                key: attempt.clone(),
            },
        )?;
        transaction.commit().map_err(database)?;
        self.guard()?;
        Ok(())
    }

    /// The caller must persist and validate the bound transport receipt first.
    /// Unknown is a terminal accounting record: only an exactly equal repeat is
    /// idempotent. NoRequest requires trusted proof of zero network attempts.
    pub fn settle(
        &mut self,
        attempt: &BudgetAttemptKey,
        receipt: &BudgetSettlement,
    ) -> Result<BudgetAttempt, ProjectError> {
        let encoded_key = key(attempt)?;
        settlement(receipt)?;
        self.guard()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database)?;
        let view = projected(&transaction, &self.owner, &self.owner_json)?;
        let previous = view
            .attempts
            .get(&encoded_key)
            .ok_or_else(|| invalid("结算请求没有原预留。"))?;
        let result = if let Some(ref settled) = previous.settlement {
            if settled != receipt {
                return Err(invalid("重复结算身份、用量或传输收据改变。"));
            }
            previous.clone()
        } else {
            if receipt.network_attempts > 0 && previous.state != BudgetAttemptState::Sent {
                return Err(invalid("网络用量结算缺少持久发送许可。"));
            }
            let state = settled_state(receipt);
            append(
                &transaction,
                &view,
                Event::Settled {
                    key: attempt.clone(),
                    settlement: receipt.clone(),
                },
            )?;
            BudgetAttempt {
                request: previous.request.clone(),
                state,
                settlement: Some(receipt.clone()),
            }
        };
        transaction.commit().map_err(database)?;
        self.guard()?;
        Ok(result)
    }

    pub fn attempt(
        &mut self,
        attempt: &BudgetAttemptKey,
    ) -> Result<Option<BudgetAttempt>, ProjectError> {
        let encoded_key = key(attempt)?;
        self.guard()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(database)?;
        let value = projected(&transaction, &self.owner, &self.owner_json)?
            .attempts
            .get(&encoded_key)
            .cloned();
        transaction.commit().map_err(database)?;
        self.guard()?;
        Ok(value)
    }
}

fn settled_state(receipt: &BudgetSettlement) -> BudgetAttemptState {
    if receipt.network_attempts == 0 {
        BudgetAttemptState::NotSent
    } else if receipt.usage.is_some() {
        BudgetAttemptState::Known
    } else {
        BudgetAttemptState::Unknown
    }
}

fn projected(
    connection: &Connection,
    owner: &Owner,
    owner_json: &str,
) -> Result<Projection, ProjectError> {
    let application: i64 = connection
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .map_err(database)?;
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(database)?;
    let mut statement = connection
        .prepare("SELECT name,sql FROM sqlite_schema WHERE type='table' ORDER BY name")
        .map_err(database)?;
    let tables = statement
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(database)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database)?;
    let expected = ["events", "history", "metadata"];
    let other_objects: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type<>'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )
        .map_err(database)?;
    if application != APPLICATION_ID
        || version != SCHEMA_VERSION
        || tables.len() != expected.len()
        || tables.iter().map(|v| v.0.as_str()).ne(expected)
        || other_objects != 0
    {
        return Err(invalid("生产额度数据库Schema或未知表无效。"));
    }
    for (name, sql) in &tables {
        let expected_sql = SCHEMA
            .lines()
            .find(|line| line.starts_with(&format!("CREATE TABLE {name}(")))
            .ok_or_else(|| invalid("未知生产表。"))?
            .trim_end_matches(';');
        if sql != expected_sql {
            return Err(invalid("生产额度表字段或约束被改变。"));
        }
    }
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM metadata", [], |r| r.get(0))
        .map_err(database)?;
    if count != 1 {
        return Err(invalid("生产额度metadata不是唯一初始记录。"));
    }
    let (stored_owner, baseline, head, head_sha): (String, Vec<u8>, i64, String) = connection
        .query_row(
            "SELECT owner_json,baseline,head_sequence,head_sha256 FROM metadata WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(database)?;
    let head = u64::try_from(head).map_err(|_| invalid("生产事件head不能为负。"))?;
    if stored_owner != owner_json
        || baseline.len() as u64 != owner.baseline_bytes
        || sha256(&baseline) != owner.baseline_sha256
    {
        return Err(invalid("SQLite基线或owner与只读原件不符。"));
    }
    let original = legacy(&baseline)?;
    if original.approved_total_tokens != owner.approved_total_tokens
        || original.accounted_tokens != owner.accounted_tokens
        || original.pending_reserved_tokens != owner.pending_reserved_tokens
        || original.model_attempts != owner.model_attempts
        || original.records.len() as u64 != owner.history_count
    {
        return Err(invalid("原历史opening与owner不符。"));
    }
    let mut history = connection
        .prepare("SELECT ordinal,record_json,record_sha256 FROM history ORDER BY ordinal")
        .map_err(database)?;
    let rows = history
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(database)?;
    let mut seen = 0usize;
    for row in rows {
        let (ordinal, raw, digest) = row.map_err(database)?;
        let ordinal = usize::try_from(ordinal).map_err(|_| invalid("原历史序号不能为负。"))?;
        if ordinal != seen
            || original
                .records
                .get(ordinal)
                .map(encode)
                .transpose()?
                .as_deref()
                != Some(raw.as_str())
            || sha256(raw.as_bytes()) != digest
        {
            return Err(invalid("完整原历史记录缺失、替换或摘要无效。"));
        }
        seen += 1;
    }
    if seen != original.records.len() {
        return Err(invalid("原历史条目丢失。"));
    }
    let mut view = Projection {
        status: BudgetStatus {
            schema_version: SCHEMA_VERSION,
            authority_id: owner.authority_id.clone(),
            baseline_sha256: owner.baseline_sha256.clone(),
            approved_total_tokens: owner.approved_total_tokens,
            accounted_tokens: owner.accounted_tokens,
            pending_reserved_tokens: owner.pending_reserved_tokens,
            remaining_tokens: 0,
            overrun_tokens: 0,
            model_attempts: owner.model_attempts,
            history_count: owner.history_count,
            attempt_count: 0,
            stopped: false,
        },
        attempts: BTreeMap::new(),
        sequence: 0,
        last_sha256: owner.baseline_sha256.clone(),
    };
    let mut events=connection.prepare("SELECT sequence,previous_sha256,payload_json,payload_sha256 FROM events ORDER BY sequence").map_err(database)?;
    let rows = events
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(database)?;
    for row in rows {
        let (sequence, previous, raw, digest) = row.map_err(database)?;
        let sequence = u64::try_from(sequence).map_err(|_| invalid("生产事件序号不能为负。"))?;
        if sequence != add(view.sequence, 1)?
            || previous != view.last_sha256
            || digest != event_sha(sequence, &previous, &raw)
        {
            return Err(invalid("生产预算事件链缺失、重排或摘要被改变。"));
        }
        let event: Event = canonical(&raw)?;
        apply(&mut view, event)?;
        view.sequence = sequence;
        view.last_sha256 = digest;
    }
    if view.sequence != head || view.last_sha256 != head_sha {
        return Err(invalid("生产事件链与事务head不一致。"));
    }
    for attempt in view.attempts.values() {
        match attempt.state {
            BudgetAttemptState::Reserved => {
                view.status.pending_reserved_tokens = add(
                    view.status.pending_reserved_tokens,
                    attempt.request.reserved_tokens,
                )?
            }
            BudgetAttemptState::Sent | BudgetAttemptState::Unknown => {
                view.status.pending_reserved_tokens = add(
                    view.status.pending_reserved_tokens,
                    attempt.request.reserved_tokens,
                )?;
                view.status.model_attempts = add(view.status.model_attempts, 1)?;
            }
            BudgetAttemptState::Known => {
                let actual = attempt
                    .settlement
                    .as_ref()
                    .and_then(|r| r.usage.as_ref())
                    .ok_or_else(|| invalid("已知用量缺少usage。"))?;
                view.status.accounted_tokens =
                    add(view.status.accounted_tokens, actual.total_tokens)?;
                view.status.model_attempts = add(view.status.model_attempts, 1)?;
            }
            BudgetAttemptState::NotSent => {}
        }
    }
    let occupied = add(
        view.status.accounted_tokens,
        view.status.pending_reserved_tokens,
    )?;
    view.status.remaining_tokens = owner.approved_total_tokens.saturating_sub(occupied);
    view.status.overrun_tokens = occupied.saturating_sub(owner.approved_total_tokens);
    view.status.attempt_count = view.attempts.len() as u64;
    view.status.stopped = view.status.remaining_tokens == 0;
    Ok(view)
}

fn apply(view: &mut Projection, event: Event) -> Result<(), ProjectError> {
    match event {
        Event::Reserved { request } => {
            let encoded = key(&request.key)?;
            if !hash(&request.request_sha256)
                || request.reserved_tokens == 0
                || view.attempts.contains_key(&encoded)
            {
                return Err(invalid("非法或重复预留事件。"));
            }
            if view.attempts.values().any(|a| {
                a.request.key.request_id == request.key.request_id
                    || (a.request.key.run_id == request.key.run_id
                        && a.request.key.unit_id == request.key.unit_id
                        && a.request.key.attempt_no == request.key.attempt_no)
            }) {
                return Err(invalid("历史事件逻辑attempt身份冲突。"));
            }
            view.attempts.insert(
                encoded,
                BudgetAttempt {
                    request,
                    state: BudgetAttemptState::Reserved,
                    settlement: None,
                },
            );
        }
        Event::Sent { key: attempt } => {
            let previous = view
                .attempts
                .get_mut(&key(&attempt)?)
                .ok_or_else(|| invalid("发送事件没有预留。"))?;
            if previous.state != BudgetAttemptState::Reserved {
                return Err(invalid("原请求被重复授权发送。"));
            }
            previous.state = BudgetAttemptState::Sent;
        }
        Event::Settled {
            key: attempt,
            settlement: receipt,
        } => {
            settlement(&receipt)?;
            let previous = view
                .attempts
                .get_mut(&key(&attempt)?)
                .ok_or_else(|| invalid("结算事件没有预留。"))?;
            if previous.settlement.is_some()
                || (receipt.network_attempts > 0 && previous.state != BudgetAttemptState::Sent)
            {
                return Err(invalid("重复或无发送许可结算事件。"));
            }
            previous.state = settled_state(&receipt);
            previous.settlement = Some(receipt);
        }
    }
    Ok(())
}
fn event_sha(sequence: u64, previous: &str, raw: &str) -> String {
    sha256(format!("{sequence}\n{previous}\n{raw}").as_bytes())
}
fn append(connection: &Connection, view: &Projection, event: Event) -> Result<(), ProjectError> {
    let sequence = add(view.sequence, 1)?;
    if sequence > i64::MAX as u64 {
        return Err(invalid("事件序列溢出。"));
    }
    let raw = encode(&event)?;
    let digest = event_sha(sequence, &view.last_sha256, &raw);
    connection
        .execute(
            "INSERT INTO events VALUES(?1,?2,?3,?4)",
            params![sequence as i64, view.last_sha256, raw, digest],
        )
        .map_err(database)?;
    connection
        .execute(
            "UPDATE metadata SET head_sequence=?1,head_sha256=?2 WHERE singleton=1",
            params![sequence as i64, digest],
        )
        .map_err(database)?;
    Ok(())
}

#[cfg(test)]
#[path = "production_budget_tests.rs"]
mod tests;
