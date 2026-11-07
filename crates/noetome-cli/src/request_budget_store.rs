//! Owned, append-only request accounting. No credentials or HTTP live here.
//! A short OS lock protects pathname validation from a concurrent SQLite writer;
//! admission itself is recomputed inside BEGIN IMMEDIATE, never from cached totals.

use crate::{
    compile_identity::{Identity, identity},
    project::{self, ProjectError},
    request_budget_model::*,
    run_model::FileStamp,
    text::sha256,
};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    thread,
    time::{Duration, Instant, UNIX_EPOCH},
};
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};

const DB: &str = "control/budget.sqlite";
const OWNER: &str = "control/owner.json";
const LOCK: &str = "control/authority.lock";
const POLICY: &str = "control/policy.json";
const APP: i64 = 0x4e425544;
const BYTE_LIMIT: u64 = 64 * 1024 * 1024;
const SCHEMA: &str = "
CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),metadata_json TEXT NOT NULL);
CREATE TABLE events(sequence INTEGER PRIMARY KEY CHECK(sequence>0),previous_sha256 TEXT NOT NULL,payload_json TEXT NOT NULL,payload_sha256 TEXT NOT NULL);";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    schema_version: u32,
    producer: String,
    root: String,
    root_identity: Identity,
    parent_identity: Identity,
    control_identity: Identity,
    database_identity: Identity,
    owner_identity: Identity,
    lock_identity: Identity,
    policy_file: FileStamp,
    compiler_fingerprint: String,
    authority: AuthoritySpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema_version: u32,
    producer: String,
    global_budget: BudgetLimits,
    opening: OpeningBalance,
}

fn policy(bytes: &[u8], spec: &AuthoritySpec) -> Result<(), ProjectError> {
    if bytes.len() as u64 > BYTE_LIMIT || sha256(bytes) != spec.policy_sha256 {
        return Err(invalid("owned policy原字节与初始策略SHA不符。"));
    }
    let value: Policy = serde_json::from_slice(bytes).map_err(|e| invalid(e.to_string()))?;
    if value.schema_version != SCHEMA_VERSION
        || value.producer != "synthetic_api_budget"
        || value.global_budget != spec.global_limits
        || value.opening != spec.opening
    {
        return Err(invalid("owned policy与初始global caps/opening不符。"));
    }
    authority(spec)
}

pub(crate) struct BudgetStore {
    root: PathBuf,
    owner: Owner,
    owner_bytes: Vec<u8>,
    connection: Connection,
    writable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ReceiptRetryDecision {
    Unchanged,
    Scheduled(RetrySchedule),
    Finish {
        outcome: UnitOutcome,
        diagnostic: String,
    },
}

fn error(code: &'static str, text: impl Into<String>) -> ProjectError {
    ProjectError::new(code, 3, text)
}
fn invalid(text: impl Into<String>) -> ProjectError {
    error("REQUEST_BUDGET_INTEGRITY", text)
}
fn database(e: rusqlite::Error) -> ProjectError {
    error(
        "REQUEST_BUDGET_DATABASE",
        format!("预算事务失败，保留现场：{e}"),
    )
}
fn encode<T: Serialize>(value: &T) -> Result<String, ProjectError> {
    serde_json::to_string(value).map_err(|e| invalid(e.to_string()))
}
fn strict<T: serde::de::DeserializeOwned + Serialize>(raw: &str) -> Result<T, ProjectError> {
    let value: T = serde_json::from_str(raw).map_err(|e| invalid(e.to_string()))?;
    if encode(&value)? != raw {
        return Err(invalid("权威JSON非规范编码、重复键或未知字段。"));
    }
    Ok(value)
}
fn utc(value: &str) -> Result<OffsetDateTime, ProjectError> {
    let at = OffsetDateTime::parse(value, &Rfc3339).map_err(|_| invalid("时间必须是明确UTC。"))?;
    if at.offset() != UtcOffset::UTC {
        return Err(invalid("时间必须是明确UTC。"));
    }
    Ok(at)
}
fn now() -> Result<String, ProjectError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| invalid(e.to_string()))
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
fn uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|u| u.to_string() == value)
}
fn reason(value: &str) -> Result<(), ProjectError> {
    if value.is_empty() || value.len() > 2048 || value.chars().any(char::is_control) {
        return Err(invalid("诊断必须是有界非空文本。"));
    }
    Ok(())
}
fn plus(a: u64, b: u64) -> Result<u64, ProjectError> {
    a.checked_add(b).ok_or_else(|| invalid("预算整数溢出。"))
}
fn limits(l: &BudgetLimits) -> Result<(), ProjectError> {
    if l.max_requests > 1_000_000 || l.max_cost_usd.is_some() {
        return Err(invalid("本轮仅固定Token/次数预算，价格必须为null。"));
    }
    Ok(())
}
fn authority(spec: &AuthoritySpec) -> Result<(), ProjectError> {
    limits(&spec.global_limits)?;
    if spec.schema_version != SCHEMA_VERSION
        || !name(&spec.authority_id)
        || !hash(&spec.policy_sha256)
        || plus(spec.opening.accounted_tokens, spec.opening.held_tokens)?
            > spec.global_limits.max_total_tokens
        || plus(spec.opening.dispatched_requests, spec.opening.held_requests)?
            > spec.global_limits.max_requests
    {
        return Err(invalid("初始authority、上限或opening不合法。"));
    }
    Ok(())
}
fn relative(value: &str) -> Result<&Path, ProjectError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 1024
        || value.contains('\\')
        || value.contains(':')
        || path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || path.to_str() != Some(value)
        || value
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(invalid("attempt路径必须为普通相对路径。"));
    }
    Ok(path)
}
fn pin(pin: &FilePin) -> Result<(), ProjectError> {
    if !hash(&pin.sha256) || pin.byte_length > BYTE_LIMIT {
        return Err(invalid("原件pin无效或超限。"));
    }
    Ok(())
}
fn stamp(path: &Path) -> Result<(FileStamp, Vec<u8>), ProjectError> {
    stamp_mode(path, true)
}
fn read_identity(
    path: &Path,
    _file: &File,
    metadata: &fs::Metadata,
) -> Result<Identity, ProjectError> {
    if project::is_link(metadata) || !metadata.is_file() {
        return Err(ProjectError::unsafe_path(path));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(invalid("原件必须是普通单硬链接文件。"));
        }
        Ok(Identity::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(windows)]
    {
        let information =
            winapi_util::file::information(_file).map_err(|e| ProjectError::io(path, e))?;
        if information.file_attributes() & 0x400 != 0 || information.number_of_links() != 1 {
            return Err(invalid("原件拒绝reparse或多硬链接文件。"));
        }
        Ok(Identity::Windows {
            volume: information.volume_serial_number(),
            index: information.file_index(),
        })
    }
    #[cfg(not(any(unix, windows)))]
    Err(error(
        "COMPILE_IDENTITY_UNSUPPORTED",
        "当前平台没有已确认的持久文件身份实现。",
    ))
}
pub(super) fn stamp_mode(path: &Path, owned: bool) -> Result<(FileStamp, Vec<u8>), ProjectError> {
    project::check_components(path)?;
    project::checked_file(path)?;
    let initial = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
    let file = if owned {
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
    stamp_opened(path, file, initial, || Ok(()))
}
fn stamp_opened(
    path: &Path,
    mut file: File,
    initial: same_file::Handle,
    after_snapshot: impl FnOnce() -> Result<(), ProjectError>,
) -> Result<(FileStamp, Vec<u8>), ProjectError> {
    // The identity and bytes come from this same read handle. Keep all three
    // pathname/read handles alive until the final comparison, including NTFS.
    let opened =
        same_file::Handle::from_file(file.try_clone().map_err(|e| ProjectError::io(path, e))?)
            .map_err(|e| ProjectError::io(path, e))?;
    if initial != opened {
        return Err(invalid("打开原件期间身份变化。"));
    }
    let before = file.metadata().map_err(|e| ProjectError::io(path, e))?;
    let id = read_identity(path, &file, &before)?;
    let modified = before.modified().map_err(|e| ProjectError::io(path, e))?;
    if before.len() > BYTE_LIMIT {
        return Err(invalid("原件超过64MiB。"));
    }
    after_snapshot()?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(BYTE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ProjectError::io(path, e))?;
    let after = file.metadata().map_err(|e| ProjectError::io(path, e))?;
    if before.len() != bytes.len() as u64
        || after.len() != before.len()
        || bytes.len() as u64 > BYTE_LIMIT
        || after.modified().map_err(|e| ProjectError::io(path, e))? != modified
        || read_identity(path, &file, &after)? != id
    {
        return Err(invalid("读取期间原件变化。"));
    }
    let current = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
    let current_metadata = current
        .as_file()
        .metadata()
        .map_err(|e| ProjectError::io(path, e))?;
    if current != opened
        || read_identity(path, current.as_file(), &current_metadata)? != id
        || current_metadata.len() != before.len()
        || current_metadata
            .modified()
            .map_err(|e| ProjectError::io(path, e))?
            != modified
    {
        return Err(invalid("读取期间原件路径身份变化。"));
    }
    project::check_components(path)?;
    project::checked_file(path)?;
    let modified = modified
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("原件时间早于epoch。"))?
        .as_nanos()
        .to_string();
    Ok((
        FileStamp {
            identity: id,
            sha256: sha256(&bytes),
            byte_length: bytes.len() as u64,
            modified,
        },
        bytes,
    ))
}
fn matches_pin(stamp: &FileStamp, pin: &FilePin) -> bool {
    stamp.sha256 == pin.sha256 && stamp.byte_length == pin.byte_length
}
fn registered_handle_identity(
    path: &Path,
    _file: &File,
    metadata: &fs::Metadata,
) -> Result<Identity, ProjectError> {
    if project::is_link(metadata) || !(metadata.is_file() || metadata.is_dir()) {
        return Err(ProjectError::unsafe_path(path));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.is_file() && metadata.nlink() != 1 {
            return Err(ProjectError::new(
                "COMPILE_INTEGRITY",
                3,
                "拥有的普通文件必须只有一个硬链接；已保留现场。",
            ));
        }
        Ok(Identity::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(windows)]
    {
        let information =
            winapi_util::file::information(_file).map_err(|e| ProjectError::io(path, e))?;
        if information.file_attributes() & 0x400 != 0
            || (metadata.is_file() && information.number_of_links() != 1)
        {
            return Err(ProjectError::new(
                "COMPILE_INTEGRITY",
                3,
                "持久身份拒绝reparse或多硬链接文件；已保留现场。",
            ));
        }
        Ok(Identity::Windows {
            volume: information.volume_serial_number(),
            index: information.file_index(),
        })
    }
    #[cfg(not(any(unix, windows)))]
    Err(ProjectError::new(
        "COMPILE_IDENTITY_UNSUPPORTED",
        3,
        "当前平台没有已确认的持久文件身份实现。",
    ))
}
pub(super) fn registered_identity_matches(
    path: &Path,
    expected: &Identity,
) -> Result<bool, ProjectError> {
    project::check_components(path)?;
    let before = fs::symlink_metadata(path).map_err(|e| ProjectError::io(path, e))?;
    if project::is_link(&before) || !(before.is_file() || before.is_dir()) {
        return Err(ProjectError::unsafe_path(path));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x02200000);
    }
    let file = options.open(path).map_err(|e| ProjectError::io(path, e))?;
    registered_identity_opened(path, expected, file, before, || Ok(()))
}
fn registered_identity_opened(
    path: &Path,
    expected: &Identity,
    file: File,
    before: fs::Metadata,
    after_snapshot: impl FnOnce() -> Result<(), ProjectError>,
) -> Result<bool, ProjectError> {
    // Revalidation uses an already trusted ID, not a new pathname observation.
    // Keep the protected actual handle and independent final pathname handle
    // alive together. This function never reads file data or binds mtime.
    let opened_metadata = file.metadata().map_err(|e| ProjectError::io(path, e))?;
    if registered_handle_identity(path, &file, &opened_metadata)? != *expected
        || opened_metadata.is_dir() != before.is_dir()
        || opened_metadata.is_file() != before.is_file()
    {
        return Ok(false);
    }
    let opened =
        same_file::Handle::from_file(file.try_clone().map_err(|e| ProjectError::io(path, e))?)
            .map_err(|e| ProjectError::io(path, e))?;
    after_snapshot()?;
    let after = file.metadata().map_err(|e| ProjectError::io(path, e))?;
    if registered_handle_identity(path, &file, &after)? != *expected
        || after.is_dir() != before.is_dir()
        || after.is_file() != before.is_file()
    {
        return Ok(false);
    }
    let current = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
    project::check_components(path)?;
    let current_metadata = current
        .as_file()
        .metadata()
        .map_err(|e| ProjectError::io(path, e))?;
    let current_id = registered_handle_identity(path, current.as_file(), &current_metadata)?;
    let final_leaf = fs::symlink_metadata(path).map_err(|e| ProjectError::io(path, e))?;
    if project::is_link(&final_leaf) || !(final_leaf.is_file() || final_leaf.is_dir()) {
        return Err(ProjectError::unsafe_path(path));
    }
    let matched = current == opened
        && current_id == *expected
        && current_metadata.is_dir() == before.is_dir()
        && current_metadata.is_file() == before.is_file()
        && final_leaf.is_dir() == before.is_dir()
        && final_leaf.is_file() == before.is_file();
    project::check_components(path)?;
    Ok(matched)
}
fn empty_stamp(path: &Path) -> Result<FileStamp, ProjectError> {
    // NTFS can reject ReadFile on a locked zero-byte worker file. Its emptiness
    // is witnessed by unchanged ordinary-file metadata and real handle IDs.
    let id = identity(path)?;
    let before = project::checked_file(path)?;
    if before.len() != 0 {
        return Err(invalid("worker锁必须是空普通单link文件。"));
    }
    let modified = before.modified().map_err(|e| ProjectError::io(path, e))?;
    let after = project::checked_file(path)?;
    if after.len() != 0
        || after.modified().map_err(|e| ProjectError::io(path, e))? != modified
        || identity(path)? != id
    {
        return Err(invalid("空worker锁读取期间变化。"));
    }
    Ok(FileStamp {
        identity: id,
        sha256: sha256(&[]),
        byte_length: 0,
        modified: modified
            .duration_since(UNIX_EPOCH)
            .map_err(|_| invalid("worker时间早于epoch。"))?
            .as_nanos()
            .to_string(),
    })
}
fn open_file(path: &Path, create: bool) -> Result<File, ProjectError> {
    project::check_components(path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(create);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x00200000);
    }
    let file = options.open(path).map_err(|e| ProjectError::io(path, e))?;
    identity(path)?;
    Ok(file)
}
fn lock(root: &Path) -> Result<File, ProjectError> {
    let path = root.join(LOCK);
    let file = open_file(&path, false)?;
    if file
        .metadata()
        .map_err(|e| ProjectError::io(&path, e))?
        .len()
        != 0
    {
        return Err(invalid("authority锁必须是空普通单link文件。"));
    }
    let start = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if start.elapsed() < Duration::from_secs(5) => {
                thread::sleep(Duration::from_millis(2))
            }
            Err(e) => return Err(error("REQUEST_BUDGET_BUSY", format!("预算锁不可取得：{e}"))),
        }
    }
}
fn names(path: &Path) -> Result<BTreeSet<String>, ProjectError> {
    fs::read_dir(path)
        .map_err(|e| ProjectError::io(path, e))?
        .map(|entry| {
            let e = entry.map_err(|e| ProjectError::io(path, e))?;
            e.file_name()
                .into_string()
                .map_err(|_| invalid("未知非UTF8文件名。"))
        })
        .collect()
}
fn layout(root: &Path, owner: &Owner, bytes: &[u8]) -> Result<(), ProjectError> {
    if owner.schema_version != SCHEMA_VERSION
        || owner.producer != "synthetic_request_budget"
        || !hash(&owner.compiler_fingerprint)
        || fs::canonicalize(root)
            .map_err(|e| ProjectError::io(root, e))?
            .to_string_lossy()
            != owner.root
        || !registered_identity_matches(root, &owner.root_identity)?
        || !registered_identity_matches(
            root.parent()
                .ok_or_else(|| invalid("authority无父目录。"))?,
            &owner.parent_identity,
        )?
        || !registered_identity_matches(&root.join("control"), &owner.control_identity)?
        || !registered_identity_matches(&root.join(LOCK), &owner.lock_identity)?
        || names(root)? != BTreeSet::from(["control".into()])
        || names(&root.join("control"))?
            != BTreeSet::from([
                "budget.sqlite".into(),
                "owner.json".into(),
                "authority.lock".into(),
                "policy.json".into(),
            ])
    {
        return Err(invalid(
            "authority身份、原字节、库存或SQLite sidecar变化；保留现场。",
        ));
    }
    // Prove the existing root and database before opening owner/policy text.
    // A single owner read binds both its actual identity and original bytes.
    database_header(&root.join(DB), &owner.database_identity)?;
    let (owner_stamp, owner_bytes) = stamp(&root.join(OWNER))?;
    if owner_stamp.identity != owner.owner_identity || owner_bytes != bytes {
        return Err(invalid("authority owner身份或原字节变化；保留现场。"));
    }
    let (policy_stamp, policy_bytes) = stamp(&root.join(POLICY))?;
    if policy_stamp != owner.policy_file {
        return Err(invalid("owned policy身份、时间、SHA或长度变化；保留现场。"));
    }
    policy(&policy_bytes, &owner.authority)
}
fn database_header(path: &Path, expected: &Identity) -> Result<(), ProjectError> {
    project::check_components(path)?;
    project::checked_file(path)?;
    let initial = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x00200000);
    }
    let file = options.open(path).map_err(|e| ProjectError::io(path, e))?;
    database_header_opened(path, expected, file, initial, || Ok(()))
}
fn database_header_opened(
    path: &Path,
    expected: &Identity,
    mut file: File,
    initial: same_file::Handle,
    after_snapshot: impl FnOnce() -> Result<(), ProjectError>,
) -> Result<(), ProjectError> {
    // Keep pathname/read/current handles alive through the final comparison.
    // The database stays mutable between transactions; only this 100-byte read
    // must observe the registered single-link file with stable length/mtime.
    let opened =
        same_file::Handle::from_file(file.try_clone().map_err(|e| ProjectError::io(path, e))?)
            .map_err(|e| ProjectError::io(path, e))?;
    let before = file.metadata().map_err(|e| ProjectError::io(path, e))?;
    let id = read_identity(path, &file, &before)?;
    let modified = before.modified().map_err(|e| ProjectError::io(path, e))?;
    if initial != opened || &id != expected || !(100..=BYTE_LIMIT).contains(&before.len()) {
        return Err(invalid("预算DB打开身份或100B至64MiB长度门槛不等。"));
    }
    after_snapshot()?;
    let mut header = [0_u8; 100];
    file.read_exact(&mut header)
        .map_err(|e| ProjectError::io(path, e))?;
    let after = file.metadata().map_err(|e| ProjectError::io(path, e))?;
    if read_identity(path, &file, &after)? != id
        || after.len() != before.len()
        || after.modified().map_err(|e| ProjectError::io(path, e))? != modified
    {
        return Err(invalid("读取预算DB header期间身份、长度或mtime变化。"));
    }
    let current = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
    let current_metadata = current
        .as_file()
        .metadata()
        .map_err(|e| ProjectError::io(path, e))?;
    if current != opened
        || read_identity(path, current.as_file(), &current_metadata)? != id
        || current_metadata.len() != before.len()
        || current_metadata
            .modified()
            .map_err(|e| ProjectError::io(path, e))?
            != modified
    {
        return Err(invalid("读取预算DB header期间路径身份、长度或mtime变化。"));
    }
    project::check_components(path)?;
    project::checked_file(path)?;
    if &header[..16] != b"SQLite format 3\0" || header[18] != 1 || header[19] != 1 {
        return Err(invalid("预算DB不是确认的DELETE journal原件。"));
    }
    Ok(())
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn schema(c: &Connection, bytes: &[u8]) -> Result<(), ProjectError> {
    for (pragma, expected) in [
        ("user_version", 1_i64),
        ("application_id", APP),
        ("synchronous", 2),
    ] {
        let n: i64 = c
            .pragma_query_value(None, pragma, |r| r.get(0))
            .map_err(database)?;
        if n != expected {
            return Err(invalid("SQLite契约pragma不符。"));
        }
    }
    let mode: String = c
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .map_err(database)?;
    if mode.to_lowercase() != "delete" {
        return Err(invalid("仅支持DELETE journal；未知sidecar不恢复。"));
    }
    let expected: BTreeMap<String, String> = SCHEMA
        .split(';')
        .filter_map(|s| {
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                let name = s.strip_prefix("CREATE TABLE ")?.split('(').next()?;
                Some((name.into(), compact(s)))
            }
        })
        .collect();
    let mut stmt = c
        .prepare("SELECT name,sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name")
        .map_err(database)?;
    let actual: BTreeMap<String, String> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(database)?
        .map(|r| r.map(|(n, s)| (n, compact(&s))).map_err(database))
        .collect::<Result<_, _>>()?;
    if actual != expected {
        return Err(invalid("预算完整两表schema不符。"));
    }
    let rows: i64 = c
        .query_row("SELECT count(*) FROM metadata", [], |r| r.get(0))
        .map_err(database)?;
    let stored: String = c
        .query_row(
            "SELECT metadata_json FROM metadata WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .map_err(database)?;
    if rows != 1 || stored.as_bytes() != bytes {
        return Err(invalid("预算初始metadata被替换。"));
    }
    Ok(())
}

impl BudgetStore {
    pub(crate) fn create(
        root: &Path,
        spec: &AuthoritySpec,
        policy_bytes: &[u8],
    ) -> Result<Self, ProjectError> {
        authority(spec)?;
        policy(policy_bytes, spec)?;
        let root = project::check_components(root)?;
        if project::exists(&root)? {
            return Err(invalid("authority须是明确不存在的新根。"));
        }
        let parent = root
            .parent()
            .ok_or_else(|| invalid("authority无父目录。"))?;
        let parent_id = identity(parent)?;
        fs::create_dir(&root).map_err(|e| ProjectError::io(&root, e))?;
        let root = fs::canonicalize(&root).map_err(|e| ProjectError::io(&root, e))?;
        fs::create_dir(root.join("control")).map_err(|e| ProjectError::io(&root, e))?;
        let _lock_file = open_file(&root.join(LOCK), true)?;
        let mut owner_file = open_file(&root.join(OWNER), true)?;
        let mut policy_file = open_file(&root.join(POLICY), true)?;
        policy_file
            .write_all(policy_bytes)
            .and_then(|_| policy_file.sync_all())
            .map_err(|e| ProjectError::io(&root.join(POLICY), e))?;
        let connection = Connection::open_with_flags(
            root.join(DB),
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(database)?;
        let owner = Owner {
            schema_version: SCHEMA_VERSION,
            producer: "synthetic_request_budget".into(),
            root: root.to_string_lossy().into(),
            root_identity: identity(&root)?,
            parent_identity: parent_id,
            control_identity: identity(&root.join("control"))?,
            database_identity: identity(&root.join(DB))?,
            owner_identity: identity(&root.join(OWNER))?,
            lock_identity: identity(&root.join(LOCK))?,
            policy_file: stamp(&root.join(POLICY))?.0,
            compiler_fingerprint: env!("NOETOME_RUN_COMPILER_FINGERPRINT").into(),
            authority: spec.clone(),
        };
        let owner_bytes = encode(&owner)?.into_bytes();
        owner_file
            .write_all(&owner_bytes)
            .and_then(|_| owner_file.sync_all())
            .map_err(|e| ProjectError::io(&root.join(OWNER), e))?;
        connection.execute_batch(&format!("PRAGMA journal_mode=DELETE;PRAGMA synchronous=FULL;PRAGMA application_id={APP};PRAGMA user_version=1;BEGIN IMMEDIATE;{SCHEMA}")).map_err(database)?;
        connection
            .execute(
                "INSERT INTO metadata VALUES(1,?1)",
                [std::str::from_utf8(&owner_bytes).map_err(|_| invalid("owner UTF8。"))?],
            )
            .map_err(database)?;
        connection.execute_batch("COMMIT").map_err(database)?;
        let store = Self {
            root,
            owner,
            owner_bytes,
            connection,
            writable: true,
        };
        store.view()?;
        Ok(store)
    }
    pub(crate) fn open(root: &Path) -> Result<Self, ProjectError> {
        Self::open_mode(root, true)
    }
    pub(crate) fn open_readonly(root: &Path) -> Result<Self, ProjectError> {
        Self::open_mode(root, false)
    }
    fn open_mode(root: &Path, writable: bool) -> Result<Self, ProjectError> {
        let root = project::check_components(root)?;
        let root = fs::canonicalize(&root).map_err(|e| ProjectError::io(&root, e))?;
        let _guard = lock(&root)?;
        let bytes = stamp(&root.join(OWNER))?.1;
        let owner: Owner =
            strict(std::str::from_utf8(&bytes).map_err(|_| invalid("owner UTF8。"))?)?;
        layout(&root, &owner, &bytes)?;
        let connection = Connection::open_with_flags(
            root.join(DB),
            (if writable {
                OpenFlags::SQLITE_OPEN_READ_WRITE
            } else {
                OpenFlags::SQLITE_OPEN_READ_ONLY
            }) | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(database)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(database)?;
        schema(&connection, &bytes)?;
        witnesses(&load(&connection, &owner.authority)?)?;
        Ok(Self {
            root,
            owner,
            owner_bytes: bytes,
            connection,
            writable,
        })
    }
    pub(crate) fn authority_matches(&self, expected: &AuthoritySpec) -> Result<(), ProjectError> {
        if &self.owner.authority != expected {
            return Err(invalid("已有authority上限/opening/策略不能重置。"));
        }
        Ok(())
    }
    pub(crate) fn verify_version(&self) -> Result<(), ProjectError> {
        if self.owner.compiler_fingerprint != env!("NOETOME_RUN_COMPILER_FINGERPRINT") {
            return Err(error(
                "REQUEST_BUDGET_VERSION",
                "编译指纹改变，保留旧累计Known/Unknown，禁止新请求。",
            ));
        }
        Ok(())
    }
    pub(crate) fn view(&self) -> Result<BudgetView, ProjectError> {
        let _guard = lock(&self.root)?;
        layout(&self.root, &self.owner, &self.owner_bytes)?;
        schema(&self.connection, &self.owner_bytes)?;
        let view = load(&self.connection, &self.owner.authority)?;
        witnesses(&view)?;
        Ok(view)
    }
    fn update<R>(
        &mut self,
        decide: impl FnOnce(&BudgetView, &str) -> Result<(R, Option<BudgetEvent>), ProjectError>,
    ) -> Result<R, ProjectError> {
        self.update_checked(|_| Ok(()), decide)
    }
    fn update_checked<R>(
        &mut self,
        guard: impl FnOnce(&BudgetView) -> Result<(), ProjectError>,
        decide: impl FnOnce(&BudgetView, &str) -> Result<(R, Option<BudgetEvent>), ProjectError>,
    ) -> Result<R, ProjectError> {
        if !self.writable {
            return Err(error(
                "REQUEST_BUDGET_READ_ONLY",
                "只读status连接禁止追加预算事件。",
            ));
        }
        let _guard = lock(&self.root)?;
        layout(&self.root, &self.owner, &self.owner_bytes)?;
        schema(&self.connection, &self.owner_bytes)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database)?;
        let mut view = load(&tx, &self.owner.authority)?;
        witnesses(&view)?;
        guard(&view)?;
        let at = now()?;
        let (result, event) = decide(&view, &at)?;
        if let Some(event) = event {
            if view.sequence >= MAX_EVENTS {
                return Err(error("REQUEST_BUDGET_LIMIT", "预算events达到4096上限。"));
            }
            let record = EventRecord {
                schema_version: SCHEMA_VERSION,
                authority_id: self.owner.authority.authority_id.clone(),
                sequence: view.sequence + 1,
                previous_sha256: view.last_sha256.clone(),
                at_utc: at,
                event,
            };
            apply(&mut view, &record)?;
            let raw = encode(&record)?;
            if raw.len() > 4 * 1024 * 1024 {
                return Err(invalid("预算event超过4MiB。"));
            }
            tx.execute(
                "INSERT INTO events VALUES(?1,?2,?3,?4)",
                params![
                    record.sequence as i64,
                    record.previous_sha256,
                    raw,
                    sha256(raw.as_bytes())
                ],
            )
            .map_err(database)?;
        }
        tx.commit().map_err(database)?;
        layout(&self.root, &self.owner, &self.owner_bytes)?;
        Ok(result)
    }
    pub(crate) fn register_run(&mut self, spec: &RunSpec) -> Result<(), ProjectError> {
        self.verify_version()?;
        validate_run(spec)?;
        verify_inputs(spec)?;
        let root = Path::new(&spec.run_root);
        if root.starts_with(&self.root)
            || self.root.starts_with(root)
            || fs::canonicalize(root)
                .map_err(|e| ProjectError::io(root, e))?
                .to_string_lossy()
                != spec.run_root
        {
            return Err(invalid("Run根须规范且与authority根分离。"));
        }
        let ids = RunIdentity {
            root: identity(root)?,
            parent: identity(root.parent().ok_or_else(|| invalid("Run无父目录。"))?)?,
            control: identity(&root.join("control"))?,
            attempts: identity(&root.join("attempts"))?,
            owner: stamp(&root.join("control/owner.json"))?.0,
            worker: empty_stamp(&root.join("control/worker.lock"))?,
            units: spec
                .units
                .iter()
                .map(|u| {
                    Ok((
                        u.unit_id.clone(),
                        identity(&root.join("attempts").join(&u.unit_id))?,
                    ))
                })
                .collect::<Result<_, ProjectError>>()?,
        };
        self.update(|view, _| {
            if let Some(old) = view.runs.iter().find(|r| r.spec.run_id == spec.run_id) {
                if old.spec == *spec && old.identity == ids {
                    return Ok(((), None));
                }
                return Err(invalid("同Run身份/输入/预算不可重登记。"));
            }
            if view.runs.len() >= 64 || view.runs.iter().any(|r| r.spec.run_root == spec.run_root) {
                return Err(invalid("Run数量超限或根重复。"));
            }
            Ok((
                (),
                Some(BudgetEvent::RunRegistered {
                    spec: spec.clone(),
                    identity: ids,
                }),
            ))
        })
    }
    pub(crate) fn prepare(&mut self, plan: &AttemptPlan) -> Result<(), ProjectError> {
        self.verify_version()?;
        validate_plan(plan)?;
        self.update(|view, _| {
            if let Ok(old) = attempt(view, &plan.key) {
                if old.plan != *plan {
                    return Err(invalid("同attempt不能更换请求或预留。"));
                }
                return Ok(((), None));
            }
            let run = run(view, &plan.key.run_id)?;
            verify_inputs(&run.spec)?;
            let path = Path::new(&run.spec.run_root).join(relative(&plan.attempt_root)?);
            if names(&path)?
                != BTreeSet::from([
                    ".owner.json".into(),
                    "request.json".into(),
                    "request-body.json".into(),
                ])
            {
                return Err(invalid("首次Prepared出现未注册原件。"));
            }
            let ids = AttemptIdentity {
                root: identity(&path)?,
                parent: identity(path.parent().ok_or_else(|| invalid("attempt无父目录。"))?)?,
                directories: directory_chain(Path::new(&run.spec.run_root), &plan.attempt_root)?,
            };
            let mut files = BTreeMap::new();
            for (name, p) in [
                ("request.json", &plan.request_metadata),
                ("request-body.json", &plan.request_body),
            ] {
                let (s, _) = stamp(&path.join(name))?;
                if !matches_pin(&s, p) {
                    return Err(invalid("实际请求原件与sealed plan不等。"));
                }
                files.insert(name.into(), s);
            }
            let (s, bytes) = stamp(&path.join(".owner.json"))?;
            let owner: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| invalid("attempt owner JSON。"))?;
            if owner["request_id"] != plan.key.request_id
                || owner["producer"] != "api"
                || owner["transport_mode"] != "local_http_test"
            {
                return Err(invalid("attempt非当前人工API原件。"));
            }
            files.insert(".owner.json".into(), s);
            verify_request(&path, plan)?;
            Ok((
                (),
                Some(BudgetEvent::Prepared {
                    plan: plan.clone(),
                    identity: ids,
                    files,
                }),
            ))
        })
    }
    pub(crate) fn reserve(
        &mut self,
        plan: &AttemptPlan,
    ) -> Result<ReservationDecision, ProjectError> {
        self.prepare(plan)?;
        self.reserve_prepared_checked(plan, |_| Ok(()))
    }
    /// Prepared already has its own durable boundary. Admission still rebuilds
    /// the complete authority inside this new transaction, without a redundant
    /// no-event Prepared transaction or a caller's cached balance.
    pub(crate) fn reserve_prepared_checked(
        &mut self,
        plan: &AttemptPlan,
        guard: impl FnOnce(&BudgetView) -> Result<(), ProjectError>,
    ) -> Result<ReservationDecision, ProjectError> {
        self.verify_version()?;
        validate_plan(plan)?;
        self.update_checked(guard, |view, at| {
            let a = attempt(view, &plan.key)?;
            if a.plan != *plan {
                return Err(invalid("同attempt不能更换请求或预留。"));
            }
            if a.state != AttemptState::Prepared {
                return Ok((ReservationDecision::AlreadyReserved, None));
            }
            if let Some((scope, reason)) = admission(view, plan, at)? {
                return Ok((ReservationDecision::Denied { scope, reason }, None));
            }
            verify_inputs(&run(view, &plan.key.run_id)?.spec)?;
            Ok((
                ReservationDecision::Reserved,
                Some(BudgetEvent::Reserved {
                    key: plan.key.clone(),
                }),
            ))
        })
    }
    pub(crate) fn mark_may_send(
        &mut self,
        key: &AttemptKey,
    ) -> Result<DispatchDecision, ProjectError> {
        self.mark_may_send_checked(key, |_| Ok(()))
    }
    pub(crate) fn mark_may_send_checked(
        &mut self,
        key: &AttemptKey,
        guard: impl FnOnce(&BudgetView) -> Result<(), ProjectError>,
    ) -> Result<DispatchDecision, ProjectError> {
        self.verify_version()?;
        self.update_checked(guard, |view, at| {
            let r = run(view, &key.run_id)?;
            let u = unit(r, &key.unit_id)?;
            let a = attempt(view, key)?;
            if a.state != AttemptState::Reserved {
                return Ok((DispatchDecision::AlreadyMaySend, None));
            }
            if view.global_stopped
                || r.budget_stopped
                || r.outcome.is_some()
                || u.outcome.is_some()
                || utc(at)? >= utc(&r.spec.deadline_utc)?
                || utc(at)? >= utc(&u.spec.deadline_utc)?
                || retry_not_due(u, key, at)?
            {
                return Ok((DispatchDecision::Denied, None));
            }
            verify_inputs(&r.spec)?;
            Ok((
                DispatchDecision::Permitted,
                Some(BudgetEvent::MaySend { key: key.clone() }),
            ))
        })
    }
    /// The API calls this after owned raw bytes and framing metadata sync, and
    /// before exposing response_original_sync. Unsealed recovery bytes cannot
    /// become a confirmed receipt merely by describing themselves as original.
    pub(crate) fn seal_transport(
        &mut self,
        key: &AttemptKey,
    ) -> Result<SettlementDecision, ProjectError> {
        self.update(|v, _| {
            let a = attempt(v, key)?;
            let r = run(v, &key.run_id)?;
            let path = Path::new(&r.spec.run_root).join(&a.plan.attempt_root);
            let files = transport_files(&path, key)?;
            if a.transport_sealed {
                if files.iter().all(|(n, s)| a.sealed_files.get(n) == Some(s)) {
                    return Ok((SettlementDecision::AlreadySettled, None));
                }
                return Err(invalid("已seal传输原件不能替换。"));
            }
            if a.state != AttemptState::MaySend {
                return Err(invalid(
                    "仅活跃MaySend可首次seal实际传输；恢复不接管无锚附加物。",
                ));
            }
            Ok((
                SettlementDecision::Applied,
                Some(BudgetEvent::TransportSealed {
                    key: key.clone(),
                    files,
                }),
            ))
        })
    }
    /// Only the recovery caller that has excluded the old worker may use this.
    pub(crate) fn release_reserved(
        &mut self,
        key: &AttemptKey,
        diagnostic: &str,
    ) -> Result<SettlementDecision, ProjectError> {
        reason(diagnostic)?;
        self.update(|v, _| {
            let a = attempt(v, key)?;
            if a.state == AttemptState::NotSent
                && a.receipt.is_none()
                && a.unknown_reason.as_deref() == Some(diagnostic)
            {
                return Ok((SettlementDecision::AlreadySettled, None));
            }
            if !matches!(a.state, AttemptState::Prepared | AttemptState::Reserved) {
                return Err(invalid("仅未授予发送的Prepared/Reserved可显式恢复释放。"));
            }
            Ok((
                SettlementDecision::Applied,
                Some(BudgetEvent::ReservedReleased {
                    key: key.clone(),
                    reason: diagnostic.into(),
                }),
            ))
        })
    }
    pub(crate) fn retain_unknown(
        &mut self,
        key: &AttemptKey,
        diagnostic: &str,
    ) -> Result<SettlementDecision, ProjectError> {
        reason(diagnostic)?;
        self.update(|v, _| {
            let a = attempt(v, key)?;
            if a.state == AttemptState::Unknown
                && a.receipt.is_none()
                && a.unknown_reason.as_deref() == Some(diagnostic)
            {
                return Ok((SettlementDecision::AlreadySettled, None));
            }
            if a.state != AttemptState::MaySend {
                return Err(invalid("未知承诺只来自MaySend，不能覆盖Known。"));
            }
            Ok((
                SettlementDecision::Applied,
                Some(BudgetEvent::UnknownRetained {
                    key: key.clone(),
                    reason: diagnostic.into(),
                }),
            ))
        })
    }
    pub(crate) fn settle(
        &mut self,
        key: &AttemptKey,
        receipt: &AttemptReceipt,
    ) -> Result<SettlementDecision, ProjectError> {
        validate_receipt(receipt)?;
        self.update(|v, _| {
            let a = attempt(v, key)?;
            if a.receipt.as_ref() == Some(receipt) {
                return Ok((SettlementDecision::AlreadySettled, None));
            }
            if matches!(a.state, AttemptState::Known | AttemptState::NotSent)
                || (a.receipt.is_some() && receipt.usage.is_none())
            {
                return Err(invalid("同attempt不同结算payload拒绝。"));
            }
            if matches!(a.state, AttemptState::Prepared | AttemptState::Reserved)
                && receipt.network_attempts != 0
            {
                return Err(invalid("未MaySend不得凭report声称已发送。"));
            }
            if receipt.network_attempts != 0 && !a.transport_sealed {
                return Err(invalid("实际响应无持久transport锚，不得收养为确认原件。"));
            }
            if receipt.network_attempts == 0
                && matches!(a.state, AttemptState::MaySend | AttemptState::Unknown)
                && !a.transport_sealed
            {
                return Err(invalid(
                    "已授予发送的自报0次不能退款；必须既存seal的原transport0。",
                ));
            }
            let r = run(v, &key.run_id)?;
            let path = Path::new(&r.spec.run_root).join(&a.plan.attempt_root);
            let files = receipt_files(&path, &a.plan, receipt)?;
            Ok((
                SettlementDecision::Applied,
                Some(BudgetEvent::ReceiptSettled {
                    key: key.clone(),
                    receipt: receipt.clone(),
                    files,
                }),
            ))
        })
    }
    pub(crate) fn schedule_retry(
        &mut self,
        key: &AttemptKey,
        retry_at_utc: &str,
        error_code: &str,
    ) -> Result<(), ProjectError> {
        self.verify_version()?;
        let schedule = RetrySchedule {
            retry_at_utc: retry_at_utc.into(),
            error_code: error_code.into(),
        };
        self.update(|v, at| {
            let a = attempt(v, key)?;
            if a.retry.as_ref() == Some(&schedule) {
                return Ok(((), None));
            }
            verify_inputs(&run(v, &key.run_id)?.spec)?;
            validate_retry(v, key, &schedule, at)?;
            Ok((
                (),
                Some(BudgetEvent::RetryScheduled {
                    key: key.clone(),
                    schedule,
                }),
            ))
        })
    }
    /// Calculate the required minimum against the same transaction's event time.
    pub(crate) fn schedule_retry_after(
        &mut self,
        key: &AttemptKey,
        error_code: &str,
    ) -> Result<RetrySchedule, ProjectError> {
        self.verify_version()?;
        self.update(|v, at| {
            let a = attempt(v, key)?;
            if let Some(old) = &a.retry {
                if old.error_code == error_code {
                    return Ok((old.clone(), None));
                }
                return Err(invalid("重试原分类不可更换。"));
            }
            let r = run(v, &key.run_id)?;
            verify_inputs(&r.spec)?;
            let delay = r
                .spec
                .retry_base_delay_ms
                .checked_mul(
                    1_u64
                        .checked_shl(key.attempt_no - 1)
                        .ok_or_else(|| invalid("退避溢出。"))?,
                )
                .ok_or_else(|| invalid("退避溢出。"))?
                .min(r.spec.retry_max_delay_ms);
            let due = utc(at)?
                .checked_add(time::Duration::milliseconds(delay as i64))
                .ok_or_else(|| invalid("退避时间溢出。"))?;
            let schedule = RetrySchedule {
                retry_at_utc: due.format(&Rfc3339).map_err(|e| invalid(e.to_string()))?,
                error_code: error_code.into(),
            };
            validate_retry(v, key, &schedule, at)?;
            Ok((
                schedule.clone(),
                Some(BudgetEvent::RetryScheduled {
                    key: key.clone(),
                    schedule,
                }),
            ))
        })
    }
    /// Inspect an API's authoritative receipt and, if eligible, schedule its
    /// retry in one fresh transaction. Settlement and terminal unit commits
    /// remain separate; no caller receipt or balance is accepted as evidence.
    pub(super) fn schedule_receipt_retry_checked(
        &mut self,
        key: &AttemptKey,
        guard: impl FnOnce(&BudgetView) -> Result<(), ProjectError>,
        stop_requested: impl FnOnce() -> bool,
    ) -> Result<ReceiptRetryDecision, ProjectError> {
        self.verify_version()?;
        self.update_checked(
            |view| {
                guard(view)?;
                verify_inputs(&run(view, &key.run_id)?.spec)
            },
            |view, at| {
                let run = run(view, &key.run_id)?;
                let unit = unit(run, &key.unit_id)?;
                let attempt = attempt(view, key)?;
                // A terminal unit must not hide an API that exited without its
                // exact Run/unit/attempt/request receipt.
                let receipt = attempt.receipt.as_ref().ok_or_else(|| {
                    invalid("API exited without an authoritative final receipt; original promise retained。")
                })?;
                if unit.outcome.is_some() || attempt.retry.is_some() {
                    return Ok((ReceiptRetryDecision::Unchanged, None));
                }
                if unit.attempts.last().map(|a| &a.plan.key) != Some(key) {
                    return Err(invalid("仅当前latest完整attempt身份可决定receipt重试。"));
                }
                let finish = |outcome, diagnostic: &str| {
                    Ok((
                        ReceiptRetryDecision::Finish {
                            outcome,
                            diagnostic: diagnostic.into(),
                        },
                        None,
                    ))
                };
                if receipt.candidate_status == CandidateStatus::Validated {
                    return finish(
                        UnitOutcome::Succeeded,
                        "Strict source-bound candidates confirmed; knowledge remains pending",
                    );
                }
                let code = receipt
                    .error_code
                    .as_deref()
                    .ok_or_else(|| invalid("失败receipt缺分类。"))?;
                if code.starts_with("REQUEST_BUDGET") || code == "REQUEST_RUN_BUDGET_STOP" {
                    return finish(UnitOutcome::BudgetStopped, code);
                }
                let retryable_receipt = receipt.usage.is_none()
                    && !(receipt.response_complete && receipt.http_status == Some(200))
                    && retryable(code);
                if !retryable_receipt || key.attempt_no >= unit.spec.max_attempts {
                    return finish(UnitOutcome::Failed, code);
                }
                if view.global_stopped || run.budget_stopped {
                    return finish(
                        UnitOutcome::BudgetStopped,
                        "Confirmed overrun stops all new sends",
                    );
                }
                if stop_requested() {
                    return Err(error(
                        "REQUEST_RUN_STOPPED",
                        "其他worker要求停止新请求。",
                    ));
                }
                let at = utc(at)?;
                if at >= utc(&unit.spec.deadline_utc)? || at >= utc(&run.spec.deadline_utc)? {
                    return finish(UnitOutcome::Failed, "Unit absolute deadline expired");
                }
                let delay = run
                    .spec
                    .retry_base_delay_ms
                    .checked_mul(
                        1_u64
                            .checked_shl(key.attempt_no - 1)
                            .ok_or_else(|| invalid("退避溢出。"))?,
                    )
                    .ok_or_else(|| invalid("退避溢出。"))?
                    .min(run.spec.retry_max_delay_ms);
                let due = at
                    .checked_add(time::Duration::milliseconds(delay as i64))
                    .ok_or_else(|| invalid("退避时间溢出。"))?;
                if due >= utc(&unit.spec.deadline_utc)? {
                    return finish(
                        UnitOutcome::Failed,
                        "No retry fits the original absolute deadline",
                    );
                }
                let schedule = RetrySchedule {
                    retry_at_utc: due.format(&Rfc3339).map_err(|e| invalid(e.to_string()))?,
                    error_code: code.into(),
                };
                let at = at.format(&Rfc3339).map_err(|e| invalid(e.to_string()))?;
                validate_retry(view, key, &schedule, &at)?;
                Ok((
                    ReceiptRetryDecision::Scheduled(schedule.clone()),
                    Some(BudgetEvent::RetryScheduled {
                        key: key.clone(),
                        schedule,
                    }),
                ))
            },
        )
    }
    pub(crate) fn finish_unit(
        &mut self,
        run_id: &str,
        unit_id: &str,
        outcome: UnitOutcome,
        diagnostic: &str,
    ) -> Result<(), ProjectError> {
        reason(diagnostic)?;
        self.update(|v, _| {
            let old = unit(run(v, run_id)?, unit_id)?;
            if old.outcome == Some(outcome) && old.diagnostic.as_deref() == Some(diagnostic) {
                return Ok(((), None));
            }
            Ok((
                (),
                Some(BudgetEvent::UnitFinished {
                    run_id: run_id.into(),
                    unit_id: unit_id.into(),
                    outcome,
                    diagnostic: diagnostic.into(),
                }),
            ))
        })
    }
    pub(crate) fn finish_run(
        &mut self,
        run_id: &str,
        outcome: RunOutcome,
        diagnostic: &str,
    ) -> Result<(), ProjectError> {
        reason(diagnostic)?;
        self.update(|v, _| {
            let old = run(v, run_id)?;
            if old.outcome == Some(outcome) && old.diagnostic.as_deref() == Some(diagnostic) {
                return Ok(((), None));
            }
            Ok((
                (),
                Some(BudgetEvent::RunFinished {
                    run_id: run_id.into(),
                    outcome,
                    diagnostic: diagnostic.into(),
                }),
            ))
        })
    }
    pub(crate) fn invalidate_run(
        &mut self,
        run_id: &str,
        diagnostic: &str,
    ) -> Result<(), ProjectError> {
        reason(diagnostic)?;
        self.update(|v, _| {
            let old = run(v, run_id)?;
            if old.outcome == Some(RunOutcome::Invalidated)
                && old.diagnostic.as_deref() == Some(diagnostic)
            {
                return Ok(((), None));
            }
            Ok((
                (),
                Some(BudgetEvent::RunInvalidated {
                    run_id: run_id.into(),
                    diagnostic: diagnostic.into(),
                }),
            ))
        })
    }
}

fn validate_run(spec: &RunSpec) -> Result<(), ProjectError> {
    limits(&spec.run_limits)?;
    let path = Path::new(&spec.run_root);
    if !uuid(&spec.run_id)
        || !hash(&spec.case_sha256)
        || !path.is_absolute()
        || spec.units.is_empty()
        || spec.units.len() > 4
        || spec.input_pins.is_empty()
        || spec.input_pins.len() > 128
        || spec.retry_base_delay_ms == 0
        || spec.retry_base_delay_ms > spec.retry_max_delay_ms
        || spec.retry_max_delay_ms > 60_000
    {
        return Err(invalid("Run契约、units或选取库存不合法。"));
    }
    let deadline = utc(&spec.deadline_utc)?;
    let mut ids = BTreeSet::new();
    for u in &spec.units {
        if !name(&u.unit_id)
            || !ids.insert(&u.unit_id)
            || !(1..=4).contains(&u.max_attempts)
            || utc(&u.deadline_utc)? > deadline
        {
            return Err(invalid("Unit身份、次数或期限不合法。"));
        }
    }
    for (p, s) in &spec.input_pins {
        if !Path::new(p).is_absolute()
            || !hash(&s.sha256)
            || s.byte_length > BYTE_LIMIT
            || s.modified.parse::<u128>().is_err()
        {
            return Err(invalid("输入pin需绝对路径、真实身份/epoch纳秒和SHA。"));
        }
    }
    Ok(())
}
fn verify_inputs(spec: &RunSpec) -> Result<(), ProjectError> {
    for (p, expected) in &spec.input_pins {
        if stamp_mode(Path::new(p), false)?.0 != *expected {
            return Err(error(
                "REQUEST_BUDGET_INPUT_CHANGED",
                "所选输入身份/mtime/字节改变，拒绝新发送。",
            ));
        }
    }
    Ok(())
}
fn validate_plan(plan: &AttemptPlan) -> Result<(), ProjectError> {
    pin(&plan.request_body)?;
    pin(&plan.request_metadata)?;
    relative(&plan.attempt_root)?;
    if !uuid(&plan.key.run_id)
        || !uuid(&plan.key.request_id)
        || !name(&plan.key.unit_id)
        || !(1..=4).contains(&plan.key.attempt_no)
        || !(16..=8192).contains(&plan.max_output_tokens)
        || plan.reservation_policy != RESERVATION_POLICY
        || plan.input_reserved_tokens != plus(plan.request_body.byte_length, 1024)?
        || plan.reserved_tokens != plus(plan.input_reserved_tokens, plan.max_output_tokens)?
    {
        return Err(invalid("attempt身份或实际UTF8预留算式不符。"));
    }
    Ok(())
}
fn run<'a>(view: &'a BudgetView, id: &str) -> Result<&'a RunView, ProjectError> {
    view.runs
        .iter()
        .find(|r| r.spec.run_id == id)
        .ok_or_else(|| invalid("未知Run。"))
}
fn unit<'a>(run: &'a RunView, id: &str) -> Result<&'a UnitView, ProjectError> {
    run.units
        .iter()
        .find(|u| u.spec.unit_id == id)
        .ok_or_else(|| invalid("未知Unit。"))
}
fn attempt<'a>(view: &'a BudgetView, key: &AttemptKey) -> Result<&'a AttemptView, ProjectError> {
    let u = unit(run(view, &key.run_id)?, &key.unit_id)?;
    let a = u
        .attempts
        .iter()
        .find(|a| a.plan.key.attempt_no == key.attempt_no)
        .ok_or_else(|| invalid("未知attempt。"))?;
    if &a.plan.key != key {
        return Err(invalid("逻辑attempt的request_id改变。"));
    }
    Ok(a)
}
fn attempt_mut<'a>(
    view: &'a mut BudgetView,
    key: &AttemptKey,
) -> Result<&'a mut AttemptView, ProjectError> {
    let r = view
        .runs
        .iter_mut()
        .find(|r| r.spec.run_id == key.run_id)
        .ok_or_else(|| invalid("未知Run。"))?;
    let u = r
        .units
        .iter_mut()
        .find(|u| u.spec.unit_id == key.unit_id)
        .ok_or_else(|| invalid("未知Unit。"))?;
    let a = u
        .attempts
        .iter_mut()
        .find(|a| a.plan.key.attempt_no == key.attempt_no)
        .ok_or_else(|| invalid("未知attempt。"))?;
    if a.plan.key != *key {
        return Err(invalid("逻辑attempt的request_id改变。"));
    }
    Ok(a)
}
fn attempts(view: &BudgetView) -> impl Iterator<Item = &AttemptView> {
    view.runs
        .iter()
        .flat_map(|r| &r.units)
        .flat_map(|u| &u.attempts)
}
fn zero() -> BudgetTotals {
    BudgetTotals {
        accounted_tokens: 0,
        held_tokens: 0,
        dispatched_requests: 0,
        held_requests: 0,
        remaining_tokens: 0,
        remaining_requests: 0,
    }
}
fn add_attempt(t: &mut BudgetTotals, a: &AttemptView) -> Result<(), ProjectError> {
    match a.state {
        AttemptState::Prepared | AttemptState::NotSent => (),
        AttemptState::Reserved => {
            t.held_tokens = plus(t.held_tokens, a.plan.reserved_tokens)?;
            t.held_requests = plus(t.held_requests, 1)?;
        }
        AttemptState::MaySend | AttemptState::Unknown => {
            t.held_tokens = plus(t.held_tokens, a.plan.reserved_tokens)?;
            t.dispatched_requests = plus(t.dispatched_requests, 1)?;
        }
        AttemptState::Known => {
            let usage = a
                .receipt
                .as_ref()
                .and_then(|r| r.usage.as_ref())
                .ok_or_else(|| invalid("Known无确认usage。"))?;
            t.accounted_tokens = plus(t.accounted_tokens, usage.total_tokens)?;
            t.dispatched_requests = plus(t.dispatched_requests, 1)?;
        }
    }
    Ok(())
}
fn remaining(t: &mut BudgetTotals, l: &BudgetLimits) -> Result<(), ProjectError> {
    t.remaining_tokens = l
        .max_total_tokens
        .saturating_sub(plus(t.accounted_tokens, t.held_tokens)?);
    t.remaining_requests = l
        .max_requests
        .saturating_sub(plus(t.dispatched_requests, t.held_requests)?);
    Ok(())
}
fn run_totals(r: &RunView) -> Result<BudgetTotals, ProjectError> {
    let mut t = zero();
    for a in r.units.iter().flat_map(|u| &u.attempts) {
        add_attempt(&mut t, a)?;
    }
    remaining(&mut t, &r.spec.run_limits)?;
    Ok(t)
}
fn recalculate(view: &mut BudgetView) -> Result<(), ProjectError> {
    let o = &view.authority.opening;
    let mut t = BudgetTotals {
        accounted_tokens: o.accounted_tokens,
        held_tokens: o.held_tokens,
        dispatched_requests: o.dispatched_requests,
        held_requests: o.held_requests,
        remaining_tokens: 0,
        remaining_requests: 0,
    };
    for a in attempts(view) {
        add_attempt(&mut t, a)?;
    }
    remaining(&mut t, &view.authority.global_limits)?;
    if plus(t.accounted_tokens, t.held_tokens)? > view.authority.global_limits.max_total_tokens
        || plus(t.dispatched_requests, t.held_requests)? > view.authority.global_limits.max_requests
    {
        view.global_stopped = true;
    }
    view.totals = t;
    for r in &mut view.runs {
        let t = run_totals(r)?;
        if plus(t.accounted_tokens, t.held_tokens)? > r.spec.run_limits.max_total_tokens
            || plus(t.dispatched_requests, t.held_requests)? > r.spec.run_limits.max_requests
        {
            r.budget_stopped = true;
        }
    }
    Ok(())
}
fn admission(
    v: &BudgetView,
    plan: &AttemptPlan,
    at: &str,
) -> Result<Option<(DenialScope, String)>, ProjectError> {
    let r = run(v, &plan.key.run_id)?;
    let u = unit(r, &plan.key.unit_id)?;
    if v.global_stopped || r.budget_stopped || r.outcome.is_some() || u.outcome.is_some() {
        return Ok(Some((
            DenialScope::Terminal,
            "Run/global/unit已停止新发送。".into(),
        )));
    }
    if utc(at)? >= utc(&r.spec.deadline_utc)? || utc(at)? >= utc(&u.spec.deadline_utc)? {
        return Ok(Some((DenialScope::Deadline, "Run/unit期限已到。".into())));
    }
    if plan.key.attempt_no > u.spec.max_attempts {
        return Ok(Some((
            DenialScope::UnitAttempts,
            "Unit逻辑attempt上限已到。".into(),
        )));
    }
    if plan.key.attempt_no >= 2 {
        let previous = u
            .attempts
            .iter()
            .find(|a| a.plan.key.attempt_no == plan.key.attempt_no - 1)
            .ok_or_else(|| invalid("重试缺前attempt。"))?;
        let scheduled = previous
            .retry
            .as_ref()
            .ok_or_else(|| invalid("重试未持久调度。"))?;
        if utc(at)? < utc(&scheduled.retry_at_utc)? {
            return Ok(Some((DenialScope::RetryNotDue, "重试退避未到。".into())));
        }
        let mut retry = zero();
        for a in u
            .attempts
            .iter()
            .filter(|a| a.plan.key.attempt_no >= 2 && a.plan.key != plan.key)
        {
            add_attempt(&mut retry, a)?;
        }
        if plus(
            plus(retry.accounted_tokens, retry.held_tokens)?,
            plan.reserved_tokens,
        )? > u.spec.max_retry_tokens
        {
            return Ok(Some((
                DenialScope::UnitRetryTokens,
                "Unit独立重试Token不足。".into(),
            )));
        }
    }
    if plan.reserved_tokens > v.totals.remaining_tokens || v.totals.remaining_requests == 0 {
        return Ok(Some((
            DenialScope::Global,
            "全局当前可用额度不足；承诺未退款前不发送。".into(),
        )));
    }
    let totals = run_totals(r)?;
    if plan.reserved_tokens > totals.remaining_tokens || totals.remaining_requests == 0 {
        return Ok(Some((DenialScope::Run, "Run当前可用额度不足。".into())));
    }
    Ok(None)
}
fn retry_not_due(u: &UnitView, key: &AttemptKey, at: &str) -> Result<bool, ProjectError> {
    if key.attempt_no < 2 {
        return Ok(false);
    }
    let previous = u
        .attempts
        .iter()
        .find(|a| a.plan.key.attempt_no == key.attempt_no - 1)
        .ok_or_else(|| invalid("发送重试缺前attempt。"))?;
    let scheduled = previous
        .retry
        .as_ref()
        .ok_or_else(|| invalid("发送重试缺持久退避。"))?;
    Ok(utc(at)? < utc(&scheduled.retry_at_utc)?)
}
fn validate_receipt(r: &AttemptReceipt) -> Result<(), ProjectError> {
    pin(&r.report)?;
    if let Some(p) = &r.response {
        pin(p)?;
    }
    if r.network_attempts > 1
        || r.price_usd.is_some()
        || r.http_status.is_some_and(|s| !(100..=599).contains(&s))
    {
        return Err(invalid("receipt网络次数/价格/HTTP不合法。"));
    }
    if let Some(usage) = &r.usage {
        if plus(usage.input_tokens, usage.output_tokens)? != usage.total_tokens
            || r.network_attempts != 1
            || !r.response_complete
            || r.http_status != Some(200)
            || r.response.is_none()
        {
            return Err(invalid("Known需完整200原响应及精确usage。"));
        }
    }
    if r.network_attempts == 0
        && (r.usage.is_some()
            || r.http_status.is_some()
            || r.response_complete
            || r.response.is_some()
            || r.response_id.is_some()
            || r.http_request_id.is_some())
    {
        return Err(invalid("未发送receipt不能含远端结果。"));
    }
    for text in [&r.response_id, &r.http_request_id, &r.error_code]
        .into_iter()
        .flatten()
    {
        reason(text)?;
    }
    match r.candidate_status {
        CandidateStatus::Validated if r.error_code.is_none() && r.usage.is_some() => (),
        CandidateStatus::Failed if r.error_code.is_some() => (),
        _ => return Err(invalid("候选结果/诊断/usage互不相容。")),
    }
    Ok(())
}
fn retryable(code: &str) -> bool {
    matches!(
        code,
        "EXTRACT_API_HTTP_RATE_LIMIT"
            | "EXTRACT_API_HTTP_SERVER"
            | "EXTRACT_TIMEOUT"
            | "EXTRACT_API_TRANSPORT"
            | "EXTRACT_API_TRUNCATED"
            | "REQUEST_INTERRUPTED"
    )
}
fn validate_retry(
    v: &BudgetView,
    key: &AttemptKey,
    s: &RetrySchedule,
    at: &str,
) -> Result<(), ProjectError> {
    let r = run(v, &key.run_id)?;
    let u = unit(r, &key.unit_id)?;
    let a = attempt(v, key)?;
    let last = u
        .attempts
        .last()
        .ok_or_else(|| invalid("无attempt可重试。"))?;
    let delay = r
        .spec
        .retry_base_delay_ms
        .checked_mul(
            1_u64
                .checked_shl(key.attempt_no - 1)
                .ok_or_else(|| invalid("退避溢出。"))?,
        )
        .ok_or_else(|| invalid("退避溢出。"))?
        .min(r.spec.retry_max_delay_ms);
    let earliest = utc(at)?
        .checked_add(time::Duration::milliseconds(delay as i64))
        .ok_or_else(|| invalid("重试时间溢出。"))?;
    if last.plan.key != *key
        || a.retry.is_some()
        || !matches!(
            a.state,
            AttemptState::Known | AttemptState::Unknown | AttemptState::NotSent
        )
        || key.attempt_no >= u.spec.max_attempts
        || !retryable(&s.error_code)
        || r.outcome.is_some()
        || u.outcome.is_some()
        || r.budget_stopped
        || v.global_stopped
        || utc(&s.retry_at_utc)? < earliest
        || utc(&s.retry_at_utc)? >= utc(&u.spec.deadline_utc)?
    {
        return Err(invalid("重试状态、分类、次数、退避或期限不合法。"));
    }
    if let Some(receipt) = &a.receipt {
        if s.error_code == "REQUEST_INTERRUPTED" {
            return Err(invalid("已有final report不能伪装中断重试。"));
        }
        if receipt.candidate_status != CandidateStatus::Failed
            || receipt.error_code.as_ref() != Some(&s.error_code)
            || receipt.usage.is_some()
            || (receipt.response_complete && receipt.http_status == Some(200))
        {
            return Err(invalid(
                "普通重试须为未取得已知用量、非完整200的原HTTP故障。",
            ));
        }
    } else if a.state != AttemptState::Unknown
        || s.error_code != "REQUEST_INTERRUPTED"
        || a.unknown_reason.as_ref() != Some(&s.error_code)
    {
        return Err(invalid(
            "无receipt仅明确旧worker失去发送能力的REQUEST_INTERRUPTED可重试。",
        ));
    }
    Ok(())
}

fn load(c: &Connection, spec: &AuthoritySpec) -> Result<BudgetView, ProjectError> {
    let mut view = BudgetView {
        schema_version: SCHEMA_VERSION,
        authority: spec.clone(),
        sequence: 0,
        last_sha256: "0".repeat(64),
        global_stopped: false,
        totals: zero(),
        runs: Vec::new(),
        price_usd: None,
    };
    recalculate(&mut view)?;
    let count: i64 = c
        .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
        .map_err(database)?;
    if count < 0 || count as u64 > MAX_EVENTS {
        return Err(invalid("event数量超限。"));
    }
    let mut stmt = c.prepare("SELECT sequence,previous_sha256,payload_json,payload_sha256 FROM events ORDER BY sequence").map_err(database)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(database)?;
    let mut previous_time = None;
    for row in rows {
        let (sequence, previous, raw, hash) = row.map_err(database)?;
        if raw.len() > 4 * 1024 * 1024
            || sequence != (view.sequence + 1) as i64
            || previous != view.last_sha256
            || sha256(raw.as_bytes()) != hash
        {
            return Err(invalid("event sequence/SHA链被修改。"));
        }
        let record: EventRecord = strict(&raw)?;
        let time = utc(&record.at_utc)?;
        if previous_time.is_some_and(|old| time < old) {
            return Err(invalid("event UTC时间倒退。"));
        }
        previous_time = Some(time);
        apply(&mut view, &record)?;
        view.sequence = sequence as u64;
        view.last_sha256 = hash;
    }
    Ok(view)
}
fn apply(v: &mut BudgetView, r: &EventRecord) -> Result<(), ProjectError> {
    if r.schema_version != SCHEMA_VERSION
        || r.authority_id != v.authority.authority_id
        || r.sequence != v.sequence + 1
        || r.previous_sha256 != v.last_sha256
    {
        return Err(invalid("event绑定不符。"));
    }
    utc(&r.at_utc)?;
    match &r.event {
        BudgetEvent::RunRegistered { spec, identity } => {
            validate_run(spec)?;
            if identity.worker.byte_length != 0
                || identity.worker.sha256 != sha256(&[])
                || !hash(&identity.owner.sha256)
                || identity.owner.byte_length > BYTE_LIMIT
                || identity.units.keys().cloned().collect::<BTreeSet<_>>()
                    != spec
                        .units
                        .iter()
                        .map(|u| u.unit_id.clone())
                        .collect::<BTreeSet<_>>()
            {
                return Err(invalid("Run注册控制原件见证不完整。"));
            }
            if v.runs.len() >= 64
                || v.runs
                    .iter()
                    .any(|r| r.spec.run_id == spec.run_id || r.spec.run_root == spec.run_root)
            {
                return Err(invalid("Run登记重复或超限。"));
            }
            v.runs.push(RunView {
                spec: spec.clone(),
                identity: identity.clone(),
                units: spec
                    .units
                    .iter()
                    .map(|u| UnitView {
                        spec: u.clone(),
                        attempts: Vec::new(),
                        outcome: None,
                        diagnostic: None,
                    })
                    .collect(),
                outcome: None,
                diagnostic: None,
                budget_stopped: false,
            });
        }
        BudgetEvent::Prepared {
            plan,
            identity,
            files,
        } => {
            validate_plan(plan)?;
            let old = run(v, &plan.key.run_id)?;
            let u = unit(old, &plan.key.unit_id)?;
            if old.outcome.is_some()
                || u.outcome.is_some()
                || plan.key.attempt_no != u.attempts.len() as u32 + 1
                || plan.key.attempt_no > u.spec.max_attempts
                || attempts(v).any(|a| {
                    a.plan.key.request_id == plan.key.request_id
                        || (a.plan.key.run_id == plan.key.run_id
                            && a.plan.attempt_root == plan.attempt_root)
                })
                || files.keys().cloned().collect::<BTreeSet<_>>()
                    != BTreeSet::from([
                        ".owner.json".into(),
                        "request.json".into(),
                        "request-body.json".into(),
                    ])
                || !matches_pin(
                    files
                        .get("request.json")
                        .ok_or_else(|| invalid("缺metadata witness。"))?,
                    &plan.request_metadata,
                )
                || !matches_pin(
                    files
                        .get("request-body.json")
                        .ok_or_else(|| invalid("缺body witness。"))?,
                    &plan.request_body,
                )
                || identity
                    .directories
                    .keys()
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    != directory_prefixes(&plan.attempt_root)?
                || identity.directories.get(&plan.attempt_root) != Some(&identity.root)
            {
                return Err(invalid("Prepared身份、逻辑attempt号或原件库存不符。"));
            }
            if plan.key.attempt_no >= 2
                && u.attempts.last().and_then(|a| a.retry.as_ref()).is_none()
            {
                return Err(invalid("新attempt未有前次持久重试安排。"));
            }
            let run = v
                .runs
                .iter_mut()
                .find(|r| r.spec.run_id == plan.key.run_id)
                .ok_or_else(|| invalid("未知Run。"))?;
            let unit = run
                .units
                .iter_mut()
                .find(|u| u.spec.unit_id == plan.key.unit_id)
                .ok_or_else(|| invalid("未知Unit。"))?;
            unit.attempts.push(AttemptView {
                plan: plan.clone(),
                identity: identity.clone(),
                sealed_files: files.clone(),
                transport_sealed: false,
                state: AttemptState::Prepared,
                receipt: None,
                unknown_reason: None,
                retry: None,
            });
        }
        BudgetEvent::Reserved { key } => {
            let a = attempt(v, key)?;
            if a.state != AttemptState::Prepared || admission(v, &a.plan, &r.at_utc)?.is_some() {
                return Err(invalid("Reserved无合法Prepared或超当时额度。"));
            }
            attempt_mut(v, key)?.state = AttemptState::Reserved;
        }
        BudgetEvent::MaySend { key } => {
            let run = run(v, &key.run_id)?;
            let unit = unit(run, &key.unit_id)?;
            if attempt(v, key)?.state != AttemptState::Reserved
                || v.global_stopped
                || run.budget_stopped
                || run.outcome.is_some()
                || unit.outcome.is_some()
                || utc(&r.at_utc)? >= utc(&run.spec.deadline_utc)?
                || utc(&r.at_utc)? >= utc(&unit.spec.deadline_utc)?
                || retry_not_due(unit, key, &r.at_utc)?
            {
                return Err(invalid("MaySend重复或晚于终态/期限。"));
            }
            attempt_mut(v, key)?.state = AttemptState::MaySend;
        }
        BudgetEvent::TransportSealed { key, files } => {
            let a = attempt_mut(v, key)?;
            if a.state != AttemptState::MaySend
                || a.transport_sealed
                || !files.contains_key("transport-receipt.json")
                || files
                    .keys()
                    .any(|n| !matches!(n.as_str(), "transport-receipt.json" | "response.bin"))
            {
                return Err(invalid("transport锚重复、错误phase或未知文件。"));
            }
            for (n, s) in files {
                if a.sealed_files.get(n).is_some_and(|old| old != s) {
                    return Err(invalid("transport锚改写既存原件。"));
                }
                a.sealed_files.insert(n.clone(), s.clone());
            }
            a.transport_sealed = true;
        }
        BudgetEvent::ReservedReleased { key, reason: why } => {
            reason(why)?;
            let a = attempt_mut(v, key)?;
            if !matches!(a.state, AttemptState::Prepared | AttemptState::Reserved) {
                return Err(invalid("释放了已MaySend承诺。"));
            }
            a.state = AttemptState::NotSent;
            a.unknown_reason = Some(why.clone());
        }
        BudgetEvent::UnknownRetained { key, reason: why } => {
            reason(why)?;
            let a = attempt_mut(v, key)?;
            if a.state != AttemptState::MaySend {
                return Err(invalid("未知事件来源状态不符。"));
            }
            a.state = AttemptState::Unknown;
            a.unknown_reason = Some(why.clone());
        }
        BudgetEvent::ReceiptSettled {
            key,
            receipt,
            files,
        } => {
            validate_receipt(receipt)?;
            let a = attempt(v, key)?;
            if matches!(a.state, AttemptState::Known | AttemptState::NotSent)
                || (a.receipt.is_some() && receipt.usage.is_none())
                || (matches!(a.state, AttemptState::Prepared | AttemptState::Reserved)
                    && receipt.network_attempts != 0)
                || (receipt.network_attempts != 0 && !a.transport_sealed)
                || (receipt.network_attempts == 0
                    && matches!(a.state, AttemptState::MaySend | AttemptState::Unknown)
                    && !a.transport_sealed)
                || !files
                    .get("validation.json")
                    .or_else(|| files.get("recovery-validation.json"))
                    .is_some_and(|s| matches_pin(s, &receipt.report))
            {
                return Err(invalid("结算状态或report witness不符。"));
            }
            let over = receipt.usage.as_ref().is_some_and(|u| {
                u.input_tokens > a.plan.input_reserved_tokens
                    || u.output_tokens > a.plan.max_output_tokens
                    || u.total_tokens > a.plan.reserved_tokens
            });
            let a = attempt_mut(v, key)?;
            for (name, stamp) in files {
                if a.sealed_files.get(name).is_some_and(|old| old != stamp) {
                    return Err(invalid("结算改写既存原件身份/字节。"));
                }
                a.sealed_files.insert(name.clone(), stamp.clone());
            }
            a.state = if receipt.network_attempts == 0 {
                AttemptState::NotSent
            } else if receipt.usage.is_some() {
                AttemptState::Known
            } else {
                AttemptState::Unknown
            };
            a.receipt = Some(receipt.clone());
            if over {
                v.runs
                    .iter_mut()
                    .find(|r| r.spec.run_id == key.run_id)
                    .ok_or_else(|| invalid("未知Run。"))?
                    .budget_stopped = true;
            }
        }
        BudgetEvent::RetryScheduled { key, schedule } => {
            validate_retry(v, key, schedule, &r.at_utc)?;
            attempt_mut(v, key)?.retry = Some(schedule.clone());
        }
        BudgetEvent::UnitFinished {
            run_id,
            unit_id,
            outcome,
            diagnostic,
        } => {
            reason(diagnostic)?;
            let run = v
                .runs
                .iter_mut()
                .find(|r| &r.spec.run_id == run_id)
                .ok_or_else(|| invalid("未知Run。"))?;
            let u = run
                .units
                .iter_mut()
                .find(|u| &u.spec.unit_id == unit_id)
                .ok_or_else(|| invalid("未知Unit。"))?;
            if u.outcome.is_some() {
                return Err(invalid("Unit终态重复/改变。"));
            }
            if *outcome == UnitOutcome::Succeeded
                && !u.attempts.last().is_some_and(|a| {
                    a.state == AttemptState::Known
                        && a.receipt
                            .as_ref()
                            .is_some_and(|r| r.candidate_status == CandidateStatus::Validated)
                })
            {
                return Err(invalid("Unit成功缺已确认候选。"));
            }
            u.outcome = Some(*outcome);
            u.diagnostic = Some(diagnostic.clone());
        }
        BudgetEvent::RunFinished {
            run_id,
            outcome,
            diagnostic,
        } => {
            reason(diagnostic)?;
            let run = v
                .runs
                .iter_mut()
                .find(|r| &r.spec.run_id == run_id)
                .ok_or_else(|| invalid("未知Run。"))?;
            if run.outcome.is_some() {
                return Err(invalid("Run终态重复/改变。"));
            }
            if *outcome == RunOutcome::Succeeded
                && run
                    .units
                    .iter()
                    .any(|u| u.outcome != Some(UnitOutcome::Succeeded))
            {
                return Err(invalid("Run成功缺各Unit确认。"));
            }
            run.outcome = Some(*outcome);
            run.diagnostic = Some(diagnostic.clone());
        }
        BudgetEvent::RunInvalidated { run_id, diagnostic } => {
            reason(diagnostic)?;
            let run = v
                .runs
                .iter_mut()
                .find(|r| &r.spec.run_id == run_id)
                .ok_or_else(|| invalid("未知Run。"))?;
            if run.outcome == Some(RunOutcome::Invalidated) {
                return Err(invalid("重复失效事件。"));
            }
            run.outcome = Some(RunOutcome::Invalidated);
            run.diagnostic = Some(diagnostic.clone());
        }
    }
    recalculate(v)
}

fn witnesses(v: &BudgetView) -> Result<(), ProjectError> {
    for run in &v.runs {
        let root = Path::new(&run.spec.run_root);
        if !registered_identity_matches(root, &run.identity.root)?
            || !registered_identity_matches(
                root.parent().ok_or_else(|| invalid("Run无父目录。"))?,
                &run.identity.parent,
            )?
            || !registered_identity_matches(&root.join("control"), &run.identity.control)?
            || !registered_identity_matches(&root.join("attempts"), &run.identity.attempts)?
            || stamp(&root.join("control/owner.json"))?.0 != run.identity.owner
            || empty_stamp(&root.join("control/worker.lock"))? != run.identity.worker
            || names(root)? != BTreeSet::from(["control".into(), "attempts".into()])
            || names(&root.join("control"))?
                != BTreeSet::from(["owner.json".into(), "worker.lock".into()])
            || names(&root.join("attempts"))?
                != run.identity.units.keys().cloned().collect::<BTreeSet<_>>()
        {
            return Err(invalid("已登记Run根身份改变。"));
        }
        for (name, expected) in &run.identity.units {
            if !registered_identity_matches(&root.join("attempts").join(name), expected)? {
                return Err(invalid("Run已登记unit目录被替换。"));
            }
        }
        for a in run.units.iter().flat_map(|u| &u.attempts) {
            let root = root.join(relative(&a.plan.attempt_root)?);
            if !registered_identity_matches(&root, &a.identity.root)?
                || !registered_identity_matches(
                    root.parent().ok_or_else(|| invalid("attempt无父目录。"))?,
                    &a.identity.parent,
                )?
                || !known_expected_chain(
                    Path::new(&run.spec.run_root),
                    &a.plan.attempt_root,
                    &a.identity.directories,
                )?
            {
                return Err(invalid("attempt或已确认祖先目录身份被替换。"));
            }
            for (name, expected) in &a.sealed_files {
                if !allowed_file(name) || stamp(&root.join(name))?.0 != *expected {
                    return Err(invalid("sealed API原件身份/mtime/字节变化。"));
                }
            }
            for name in names(&root)? {
                if !allowed_file(&name) {
                    return Err(invalid("attempt出现未知文件。"));
                }
                identity(&root.join(&name))?;
                project::checked_file(&root.join(&name))?;
            }
            verify_request(&root, &a.plan)?;
            if a.transport_sealed {
                for (n, s) in transport_files(&root, &a.plan.key)? {
                    if a.sealed_files.get(&n) != Some(&s) {
                        return Err(invalid("原传输/framing与持久锚不等。"));
                    }
                }
            }
            if let Some(receipt) = &a.receipt {
                let _ = receipt_files(&root, &a.plan, receipt)?;
            }
        }
    }
    Ok(())
}
fn allowed_file(name: &str) -> bool {
    matches!(
        name,
        ".owner.json"
            | "request.json"
            | "request-body.json"
            | "validation.json"
            | "validation.md"
            | "response.bin"
            | "candidates.json"
            | "transport-receipt.json"
            | "recovery-validation.json"
    )
}
fn directory_prefixes(value: &str) -> Result<BTreeSet<String>, ProjectError> {
    relative(value)?;
    let mut parts = Vec::new();
    let mut out = BTreeSet::new();
    for segment in value.split('/') {
        parts.push(segment);
        out.insert(parts.join("/"));
    }
    Ok(out)
}
fn directory_chain(root: &Path, value: &str) -> Result<BTreeMap<String, Identity>, ProjectError> {
    let mut out = BTreeMap::new();
    for p in directory_prefixes(value)? {
        let path = root.join(&p);
        if !fs::symlink_metadata(&path)
            .map_err(|e| ProjectError::io(&path, e))?
            .is_dir()
        {
            return Err(invalid("attempt祖先必须为普通目录。"));
        }
        out.insert(p, identity(&path)?);
    }
    Ok(out)
}
fn known_expected_chain(
    root: &Path,
    value: &str,
    expected: &BTreeMap<String, Identity>,
) -> Result<bool, ProjectError> {
    let prefixes = directory_prefixes(value)?;
    if prefixes != expected.keys().cloned().collect::<BTreeSet<_>>() {
        return Ok(false);
    }
    for prefix in prefixes {
        let path = root.join(&prefix);
        if !fs::symlink_metadata(&path)
            .map_err(|e| ProjectError::io(&path, e))?
            .is_dir()
        {
            return Err(invalid("attempt祖先必须为普通目录。"));
        }
        if !registered_identity_matches(&path, &expected[&prefix])? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn json(path: &Path) -> Result<serde_json::Value, ProjectError> {
    serde_json::from_slice(&stamp(path)?.1).map_err(|_| invalid("原API JSON不可解析。"))
}
fn transport_files(
    root: &Path,
    key: &AttemptKey,
) -> Result<BTreeMap<String, FileStamp>, ProjectError> {
    let (s, bytes) = stamp(&root.join("transport-receipt.json"))?;
    let t: TransportReceipt =
        serde_json::from_slice(&bytes).map_err(|e| invalid(format!("transport严格DTO：{e}")))?;
    pin(&t.response)?;
    if t.schema_version != 1
        || t.producer != "synthetic_api_transport_receipt"
        || t.request_id != key.request_id
        || t.network_attempts > 1
        || t.http_status.is_some_and(|n| !(100..=599).contains(&n))
        || t.omission_reason
            .as_deref()
            .is_some_and(|s| s != "omitted_credential_echo")
        || t.response_file
            .as_deref()
            .is_some_and(|s| s != "response.bin")
    {
        return Err(invalid("传输原件身份/nullable/状态无效。"));
    }
    if t.network_attempts == 0
        && (t.http_status.is_some()
            || t.http_request_id.is_some()
            || t.response_complete
            || t.response_file.is_some()
            || t.response.byte_length != 0
            || t.response.sha256 != sha256(&[]))
    {
        return Err(invalid("未发送transport有远端数据。"));
    }
    if t.network_attempts == 1 && t.response_file.is_none() && t.omission_reason.is_none() {
        return Err(invalid("实际响应无据遗漏。"));
    }
    let mut files = BTreeMap::from([("transport-receipt.json".into(), s)]);
    if t.response_file.is_some() {
        let (s, _) = stamp(&root.join("response.bin"))?;
        if t.omission_reason.is_some() || !matches_pin(&s, &t.response) {
            return Err(invalid("实际response与transport不等。"));
        }
        files.insert("response.bin".into(), s);
    } else if project::exists(&root.join("response.bin"))? {
        return Err(invalid("transport未列出实际response。"));
    }
    Ok(files)
}
fn verify_request(root: &Path, p: &AttemptPlan) -> Result<(), ProjectError> {
    let metadata = json(&root.join("request.json"))?;
    let endpoint = metadata["endpoint"]
        .as_str()
        .and_then(|s| url::Url::parse(s).ok())
        .ok_or_else(|| invalid("所选端点缺失。"))?;
    let loopback = matches!(endpoint.host(),Some(url::Host::Ipv4(ip)) if ip==std::net::Ipv4Addr::LOCALHOST)
        || matches!(endpoint.host(),Some(url::Host::Ipv6(ip)) if ip==std::net::Ipv6Addr::LOCALHOST);
    if metadata["request_id"] != p.key.request_id
        || metadata["transport_mode"] != "local_http_test"
        || metadata["provider"] != "openai_responses"
        || metadata["reservation_policy"] != p.reservation_policy
        || metadata["input_reserved_tokens"] != p.input_reserved_tokens
        || metadata["reserved_tokens"] != p.reserved_tokens
        || metadata["limits"]["max_output_tokens"] != p.max_output_tokens
        || metadata["request_body"]["sha256"] != p.request_body.sha256
        || metadata["request_body"]["byte_length"] != p.request_body.byte_length
        || metadata["request_strategy"] != "openai-responses-strict-json-schema-v1"
        || endpoint.scheme() != "http"
        || !loopback
        || endpoint.port().is_none()
        || endpoint.path() != "/v1/responses"
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(invalid("预算计划与原API实际请求不等。"));
    }
    Ok(())
}
fn receipt_files(
    root: &Path,
    p: &AttemptPlan,
    receipt: &AttemptReceipt,
) -> Result<BTreeMap<String, FileStamp>, ProjectError> {
    let report_name = if project::exists(&root.join("validation.json"))? {
        "validation.json"
    } else {
        "recovery-validation.json"
    };
    let (s, bytes) = stamp(&root.join(report_name))?;
    if !matches_pin(&s, &receipt.report) {
        return Err(invalid("原最终/恢复报告pin不等。"));
    }
    let report: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| invalid("最终报告JSON。"))?;
    let actual_usage = if receipt.usage.is_some() {
        serde_json::to_value(&receipt.usage).map_err(|_| invalid("usage编码。"))?
    } else {
        serde_json::Value::Null
    };
    let actual_error = report["errors"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v["code"].as_str())
        .map(str::to_owned);
    if report["request_id"] != p.key.request_id
        || report["transport_mode"] != "local_http_test"
        || report["provider"] != "openai_responses"
        || report["usage"]["network_attempts"] != receipt.network_attempts
        || report["http_status"] != serde_json::json!(receipt.http_status)
        || report["response_complete"] != receipt.response_complete
        || report["response_id"] != serde_json::json!(receipt.response_id)
        || report["http_request_id"] != serde_json::json!(receipt.http_request_id)
        || report["usage"]["price_usd"] != serde_json::Value::Null
        || report["usage"]["real_model_calls"] != 0
        || report["usage"]["paid_calls"] != 0
        || report["published"] != false
        || report["knowledge_status"] != "pending"
        || report["usage"]["input_reserved_tokens"] != p.input_reserved_tokens
        || report["usage"]["reserved_tokens"] != p.reserved_tokens
        || actual_error != receipt.error_code
        || (receipt.candidate_status == CandidateStatus::Validated)
            != (report["status"] == "candidates_validated")
    {
        return Err(invalid("receipt字段与安全原报告不等。"));
    }
    // Normalized usage may carry additional truthful detail fields; reconcile the
    // three accounting integers and independently parse the original wire body.
    if receipt.usage.is_some() {
        for field in ["input_tokens", "output_tokens", "total_tokens"] {
            if report["usage"]["model_usage"][field] != actual_usage[field] {
                return Err(invalid("原报告usage与结算不等。"));
            }
        }
        if report["usage"]["model_usage_status"] != "validated" {
            return Err(invalid("用量未独立核验。"));
        }
    } else if !report["usage"]["model_usage"].is_null() {
        return Err(invalid("不能把已知usage降为未知。"));
    }
    let mut files = BTreeMap::from([(report_name.into(), s)]);
    if report_name == "validation.json" {
        let (s, md) = stamp(&root.join("validation.md"))?;
        let text = std::str::from_utf8(&md).map_err(|_| invalid("双报告MD不是UTF8。"))?;
        let embedded = text
            .split("```json\n")
            .nth(1)
            .and_then(|t| t.split("\n```\n").next())
            .ok_or_else(|| invalid("双报告缺完整JSON。"))?;
        if embedded.as_bytes() != bytes {
            return Err(invalid("双报告原字节不等。"));
        }
        files.insert("validation.md".into(), s);
    }
    if let Some(expected) = &receipt.response {
        let (s, body) = stamp(&root.join("response.bin"))?;
        if !matches_pin(&s, expected)
            || report["response_receipt"]["sha256"] != expected.sha256
            || report["response_receipt"]["byte_length"] != expected.byte_length
        {
            return Err(invalid("实际响应原件与receipt不等。"));
        }
        if let Some(usage) = &receipt.usage {
            let metadata = crate::extraction_api_wire::response_metadata(&body)?;
            if metadata.id
                != receipt
                    .response_id
                    .clone()
                    .ok_or_else(|| invalid("缺response identity。"))?
                || metadata.usage["input_tokens"] != usage.input_tokens
                || metadata.usage["output_tokens"] != usage.output_tokens
                || metadata.usage["total_tokens"] != usage.total_tokens
                || report["actual_model"] != metadata.model
                || report["service_tier"] != serde_json::json!(metadata.service_tier)
            {
                return Err(invalid("原wire独立用量与结算不等。"));
            }
        }
        if receipt.candidate_status == CandidateStatus::Validated {
            validate_candidate(root, p, &body, &report)?;
        }
        files.insert("response.bin".into(), s);
    } else if project::exists(&root.join("response.bin"))? {
        return Err(invalid("存在原response但receipt遗漏。"));
    }
    for name in ["transport-receipt.json", "candidates.json"] {
        if project::exists(&root.join(name))? {
            files.insert(name.into(), stamp(&root.join(name))?.0);
        }
    }
    if report_name == "recovery-validation.json" && !files.contains_key("transport-receipt.json") {
        return Err(invalid("恢复报告缺明确传输原件。"));
    }
    if files.contains_key("transport-receipt.json") {
        let transport: TransportReceipt =
            serde_json::from_slice(&stamp(&root.join("transport-receipt.json"))?.1)
                .map_err(|e| invalid(format!("transport严格DTO：{e}")))?;
        pin(&transport.response)?;
        if transport.schema_version != 1
            || transport.producer != "synthetic_api_transport_receipt"
            || transport.request_id != p.key.request_id
            || transport.network_attempts != receipt.network_attempts
            || transport.http_status != receipt.http_status
            || transport.http_request_id != receipt.http_request_id
            || transport.response_complete != receipt.response_complete
            || transport.elapsed_ms != receipt.elapsed_ms
            || transport.response_file.as_deref()
                != receipt.response.as_ref().map(|_| "response.bin")
            || report["response_receipt"]["sha256"] != transport.response.sha256
            || report["response_receipt"]["byte_length"] != transport.response.byte_length
            || report["response_receipt"]["omission_reason"]
                != serde_json::json!(transport.omission_reason)
            || transport.network_attempts > 1
        {
            return Err(invalid(
                "安全最终report与原始transport framing/身份/耗时不等。",
            ));
        }
        if let Some(response) = &receipt.response {
            if transport.response != *response || transport.omission_reason.is_some() {
                return Err(invalid("transport原response pin/遗漏声明不相容。"));
            }
        } else if transport.network_attempts > 0
            && transport.omission_reason.as_deref() != Some("omitted_credential_echo")
        {
            return Err(invalid("已尝试的原响应不能无据遗漏。"));
        }
        if transport.network_attempts == 0
            && (transport.http_status.is_some()
                || transport.response_complete
                || transport.response_file.is_some()
                || transport.http_request_id.is_some()
                || transport.response.byte_length != 0
                || transport.response.sha256 != sha256(&[]))
        {
            return Err(invalid("未发送transport不得包含远端原件。"));
        }
        if transport.error_code.is_some()
            && transport.error_code != receipt.error_code
            && !(receipt.error_code.as_deref() == Some("EXTRACT_API_CREDENTIAL_ECHO")
                && transport.omission_reason.as_deref() == Some("omitted_credential_echo"))
        {
            return Err(invalid("传输故障与最终诊断不等。"));
        }
        if receipt.usage.is_some()
            && (transport.error_code.is_some() || transport.omission_reason.is_some())
        {
            return Err(invalid("Known不能基于故障/遗漏transport。"));
        }
    } else if receipt.network_attempts != 0 || receipt.elapsed_ms != 0 {
        return Err(invalid("实际发送/耗时缺transport原始见证。"));
    }
    Ok(files)
}

fn validate_candidate(
    root: &Path,
    plan: &AttemptPlan,
    body: &[u8],
    report: &serde_json::Value,
) -> Result<(), ProjectError> {
    let parsed = crate::extraction_api_wire::decode_with_strategy(
        body,
        crate::extraction_api_config::RequestStrategy::OpenAiStrict,
    )?;
    let request = json(&root.join("request.json"))?;
    let binding: crate::extraction_model::SourceBinding =
        serde_json::from_value(request["binding"].clone())
            .map_err(|_| invalid("原请求binding无效。"))?;
    let units: Vec<crate::extraction_model::RequestUnit> =
        serde_json::from_value(request["units"].clone())
            .map_err(|_| invalid("原请求units无效。"))?;
    let raw = json(&root.join("candidates.json"))?;
    let candidate =
        serde_json::to_value(&parsed.candidates).map_err(|_| invalid("原候选编码失败。"))?;
    if parsed.candidates.request_id != plan.key.request_id
        || parsed.candidates.binding != binding
        || raw["schema_version"] != 1
        || raw["producer"] != "api"
        || raw["transport_mode"] != "local_http_test"
        || raw["request_id"] != plan.key.request_id
        || raw["binding"] != request["binding"]
        || raw["payload"] != candidate
        || report["validated_atoms"] != parsed.candidates.atoms.len()
        || report["response_id"] != parsed.id
        || report["usage"]["model_usage"] != parsed.usage
    {
        return Err(invalid("已批准候选与原wire/请求绑定不等。"));
    }
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let control = crate::sidecar_process::Control::new(60_000, cancelled.clone());
    crate::extraction::validate_candidates(
        &units,
        &parsed.candidates.concepts,
        &parsed.candidates.evidence,
        &parsed.candidates.atoms,
        &control,
        &cancelled,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        base: PathBuf,
        authority: PathBuf,
        run: RunSpec,
        spec: AuthoritySpec,
        base_identity: Identity,
    }
    impl Fixture {
        fn new(cap: u64) -> Self {
            let target = std::env::var_os("CARGO_TARGET_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap().join("target"));
            let base = target
                .join("request-budget-tests")
                .join(uuid::Uuid::new_v4().to_string());
            fs::create_dir_all(&base).unwrap();
            let base = fs::canonicalize(base).unwrap();
            let runroot = base.join("run-one");
            fs::create_dir(&runroot).unwrap();
            fs::create_dir(runroot.join("control")).unwrap();
            fs::create_dir(runroot.join("attempts")).unwrap();
            fs::write(
                runroot.join("control/owner.json"),
                b"literal store-only test owner bytes\n",
            )
            .unwrap();
            fs::write(runroot.join("control/worker.lock"), []).unwrap();
            for unit in ["first", "second"] {
                fs::create_dir(runroot.join("attempts").join(unit)).unwrap();
            }
            let input = base.join("selected-author.txt");
            fs::write(&input, b"literal synthetic selected author\n").unwrap();
            let deadline = (OffsetDateTime::now_utc() + time::Duration::seconds(30))
                .format(&Rfc3339)
                .unwrap();
            let run = RunSpec {
                run_id: uuid::Uuid::new_v4().to_string(),
                run_root: runroot.to_string_lossy().into(),
                case_sha256: sha256(b"literal unit-test case"),
                input_pins: BTreeMap::from([(
                    input.to_string_lossy().into(),
                    stamp(&input).unwrap().0,
                )]),
                run_limits: BudgetLimits {
                    max_requests: 20,
                    max_total_tokens: 100_000,
                    max_cost_usd: None,
                },
                deadline_utc: deadline.clone(),
                retry_base_delay_ms: 200,
                retry_max_delay_ms: 800,
                units: ["first", "second"]
                    .into_iter()
                    .map(|id| UnitSpec {
                        unit_id: id.into(),
                        max_attempts: 4,
                        max_retry_tokens: 50_000,
                        deadline_utc: deadline.clone(),
                    })
                    .collect(),
            };
            let mut spec = AuthoritySpec {
                schema_version: 1,
                authority_id: "synthetic-unit-authority".into(),
                global_limits: BudgetLimits {
                    max_requests: 20,
                    max_total_tokens: cap,
                    max_cost_usd: None,
                },
                opening: OpeningBalance {
                    accounted_tokens: 0,
                    held_tokens: 0,
                    dispatched_requests: 0,
                    held_requests: 0,
                },
                policy_sha256: String::new(),
            };
            spec.policy_sha256 = sha256(&Self::policy(&spec));
            Self {
                authority: base.join("authority"),
                base_identity: identity(&base).unwrap(),
                base,
                run,
                spec,
            }
        }
        fn policy(spec: &AuthoritySpec) -> Vec<u8> {
            serde_json::to_vec_pretty(&serde_json::json!({"schema_version":1,"producer":"synthetic_api_budget","global_budget":spec.global_limits,"opening":spec.opening})).unwrap()
        }
        fn store(&self) -> BudgetStore {
            let mut s = BudgetStore::create(&self.authority, &self.spec, &Self::policy(&self.spec))
                .unwrap();
            s.register_run(&self.run).unwrap();
            s
        }
        fn plan(&self, unit: &str, no: u32) -> AttemptPlan {
            let dir = format!("attempts/{unit}/{no}");
            let root = Path::new(&self.run.run_root).join(&dir);
            fs::create_dir(&root).unwrap();
            let body = b"{\"literal\":\"synthetic request body\"}";
            let body_pin = FilePin {
                sha256: sha256(body),
                byte_length: body.len() as u64,
            };
            let key = AttemptKey {
                run_id: self.run.run_id.clone(),
                unit_id: unit.into(),
                attempt_no: no,
                request_id: uuid::Uuid::new_v4().to_string(),
            };
            let reserved = body.len() as u64 + 1024 + 512;
            let metadata = serde_json::json!({"request_id":key.request_id,"transport_mode":"local_http_test","provider":"openai_responses","request_strategy":"openai-responses-strict-json-schema-v1",
                "endpoint":"http://127.0.0.1:9/v1/responses","request_body":body_pin,"limits":{"max_output_tokens":512},"input_reserved_tokens":body.len()+1024,
                "reserved_tokens":reserved,"reservation_policy":RESERVATION_POLICY});
            let bytes = serde_json::to_vec_pretty(&metadata).unwrap();
            fs::write(root.join("request.json"), &bytes).unwrap();
            fs::write(root.join("request-body.json"), body).unwrap();
            fs::write(root.join(".owner.json"),serde_json::to_vec(&serde_json::json!({"request_id":key.request_id,"producer":"api","transport_mode":"local_http_test"})).unwrap()).unwrap();
            AttemptPlan {
                key,
                attempt_root: dir,
                request_body: body_pin,
                request_metadata: FilePin {
                    sha256: sha256(&bytes),
                    byte_length: bytes.len() as u64,
                },
                input_reserved_tokens: body.len() as u64 + 1024,
                max_output_tokens: 512,
                reserved_tokens: reserved,
                reservation_policy: RESERVATION_POLICY.into(),
            }
        }
        fn receipt(
            &self,
            p: &AttemptPlan,
            usage: Option<TokenUsage>,
            network: u32,
            error_code: &str,
        ) -> AttemptReceipt {
            let root = Path::new(&self.run.run_root).join(&p.attempt_root);
            let body = if let Some(u) = &usage {
                serde_json::to_vec(&serde_json::json!({"id":"resp-shared","model":"noetome-test-model","service_tier":null,"usage":u,"status":"completed","output":[]})).unwrap()
            } else {
                b"temporarily unavailable".to_vec()
            };
            let response = (network == 1).then(|| FilePin {
                sha256: sha256(&body),
                byte_length: body.len() as u64,
            });
            if network == 1 {
                fs::write(root.join("response.bin"), &body).unwrap();
            }
            let status = (network == 1).then_some(if usage.is_some() { 200 } else { 503 });
            let response_id = usage.as_ref().map(|_| "resp-shared".into());
            let http_id = (network == 1).then(|| "req-shared".into());
            let elapsed = if network == 1 { 3 } else { 0 };
            let report = serde_json::json!({"request_id":p.key.request_id,"transport_mode":"local_http_test","provider":"openai_responses","knowledge_status":"pending","published":false,"validated_atoms":0,
                "status":"failed","actual_model":if usage.is_some(){Some("noetome-test-model")}else{None},"service_tier":null,"response_id":response_id,"http_request_id":http_id,"http_status":status,"response_complete":network==1,
                "errors":[{"code":error_code,"message":"synthetic unit witness"}],"usage":{"network_attempts":network,"real_model_calls":0,"paid_calls":0,"price_usd":null,"model_usage":usage,
                "model_usage_status":if usage.is_some(){"validated"}else{"not_validated"},"input_reserved_tokens":p.input_reserved_tokens,"reserved_tokens":p.reserved_tokens},
                "response_receipt":response.as_ref().map(|pin|serde_json::json!({"sha256":pin.sha256,"byte_length":pin.byte_length,"omission_reason":null}))});
            let bytes = serde_json::to_vec_pretty(&report).unwrap();
            fs::write(root.join("validation.json"), &bytes).unwrap();
            let md = format!(
                "synthetic witness\n```json\n{}\n```\n",
                std::str::from_utf8(&bytes).unwrap()
            );
            fs::write(root.join("validation.md"), md).unwrap();
            if network == 1 {
                let transport = TransportReceipt {
                    schema_version: 1,
                    producer: "synthetic_api_transport_receipt".into(),
                    request_id: p.key.request_id.clone(),
                    http_status: status,
                    http_request_id: http_id.clone(),
                    response_complete: true,
                    network_attempts: 1,
                    response: response.clone().unwrap(),
                    response_file: Some("response.bin".into()),
                    error_code: if usage.is_some() {
                        None
                    } else {
                        Some(error_code.into())
                    },
                    omission_reason: None,
                    elapsed_ms: elapsed,
                };
                fs::write(
                    root.join("transport-receipt.json"),
                    serde_json::to_vec_pretty(&transport).unwrap(),
                )
                .unwrap();
            }
            AttemptReceipt {
                report: FilePin {
                    sha256: sha256(&bytes),
                    byte_length: bytes.len() as u64,
                },
                response,
                network_attempts: network,
                http_status: status,
                response_complete: network == 1,
                response_id,
                http_request_id: http_id,
                usage,
                candidate_status: CandidateStatus::Failed,
                error_code: Some(error_code.into()),
                elapsed_ms: elapsed,
                price_usd: None,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if !thread::panicking()
                && identity(&self.base).ok().as_ref() == Some(&self.base_identity)
            {
                let _ = fs::remove_dir_all(&self.base);
            }
        }
    }
    fn usage() -> TokenUsage {
        TokenUsage {
            input_tokens: 600,
            output_tokens: 100,
            total_tokens: 700,
        }
    }

    #[test]
    fn budget_stamp_rejects_another_same_byte_read_handle() {
        let f = Fixture::new(100_000);
        let path = f.base.join("stamp-original");
        let other = f.base.join("stamp-replacement");
        fs::write(&path, b"same literal bytes").unwrap();
        fs::write(&other, b"same literal bytes").unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        OpenOptions::new()
            .write(true)
            .open(&other)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert_eq!(fs::read(&path).unwrap(), fs::read(&other).unwrap());
        assert_eq!(fs::metadata(&other).unwrap().modified().unwrap(), modified);
        let initial = same_file::Handle::from_path(&path).unwrap();
        assert!(stamp_opened(&path, File::open(&other).unwrap(), initial, || Ok(())).is_err());
    }
    #[test]
    fn budget_stamp_rejects_a_hardlink_added_after_snapshot() {
        let f = Fixture::new(100_000);
        let path = f.base.join("stamp-original");
        fs::write(&path, b"literal bytes remain unchanged").unwrap();
        let initial = same_file::Handle::from_path(&path).unwrap();
        let file = File::open(&path).unwrap();
        let result = stamp_opened(&path, file, initial, || {
            fs::hard_link(&path, f.base.join("new-link")).map_err(|e| ProjectError::io(&path, e))
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"literal bytes remain unchanged");
    }
    #[test]
    fn budget_stamp_rejects_growth_or_truncation_with_restored_mtime() {
        let f = Fixture::new(100_000);
        for replacement in [b"longer literal replacement".as_slice(), b"x".as_slice()] {
            let path = f.base.join("stamp-original");
            fs::write(&path, b"literal original").unwrap();
            let modified = fs::metadata(&path).unwrap().modified().unwrap();
            let initial = same_file::Handle::from_path(&path).unwrap();
            let file = File::open(&path).unwrap();
            let result = stamp_opened(&path, file, initial, || {
                fs::write(&path, replacement).map_err(|e| ProjectError::io(&path, e))?;
                OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .map_err(|e| ProjectError::io(&path, e))?
                    .set_times(fs::FileTimes::new().set_modified(modified))
                    .map_err(|e| ProjectError::io(&path, e))
            });
            assert!(result.is_err());
            assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        }
    }
    #[test]
    fn budget_stamp_binds_full_raw_bytes_and_rejects_over_limit() {
        let f = Fixture::new(100_000);
        let path = f.base.join("stamp-original");
        for bytes in [Vec::new(), (0..4096).map(|n| (n % 251) as u8).collect()] {
            fs::write(&path, &bytes).unwrap();
            let (pin, actual) = stamp(&path).unwrap();
            assert_eq!(actual, bytes);
            assert_eq!(pin.identity, identity(&path).unwrap());
            assert_eq!(pin.byte_length, bytes.len() as u64);
            assert_eq!(pin.sha256, sha256(&bytes));
        }
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(BYTE_LIMIT + 1)
            .unwrap();
        assert!(stamp(&path).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn budget_stamp_rejects_same_bytes_mtime_leaf_replacement_during_read() {
        let f = Fixture::new(100_000);
        let path = f.base.join("stamp-original");
        fs::write(&path, b"same literal bytes").unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let initial = same_file::Handle::from_path(&path).unwrap();
        let file = File::open(&path).unwrap();
        let result = stamp_opened(&path, file, initial, || {
            fs::rename(&path, f.base.join("old-original"))
                .map_err(|e| ProjectError::io(&path, e))?;
            fs::write(&path, b"same literal bytes").map_err(|e| ProjectError::io(&path, e))?;
            OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(|e| ProjectError::io(&path, e))?
                .set_times(fs::FileTimes::new().set_modified(modified))
                .map_err(|e| ProjectError::io(&path, e))
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"same literal bytes");
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    }
    #[cfg(unix)]
    #[test]
    fn budget_stamp_rejects_leaf_and_ancestor_symlinks() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new(100_000);
        let parent = f.base.join("stamp-parent");
        fs::create_dir(&parent).unwrap();
        let path = parent.join("original");
        fs::write(&path, b"literal bytes").unwrap();
        let leaf = f.base.join("leaf-link");
        symlink(&path, &leaf).unwrap();
        assert!(stamp(&leaf).is_err());
        let initial = same_file::Handle::from_path(&path).unwrap();
        let file = File::open(&path).unwrap();
        let result = stamp_opened(&path, file, initial, || {
            let saved = f.base.join("original-parent");
            fs::rename(&parent, &saved).map_err(|e| ProjectError::io(&parent, e))?;
            symlink(&saved, &parent).map_err(|e| ProjectError::io(&parent, e))
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"literal bytes");
    }
    #[cfg(windows)]
    #[test]
    fn budget_stamp_keeps_owned_and_external_share_modes() {
        let f = Fixture::new(100_000);
        let path = f.base.join("stamp-original");
        fs::write(&path, b"literal bytes").unwrap();
        let writer = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert_eq!(stamp_mode(&path, true).unwrap().1, b"literal bytes");
        assert!(stamp_mode(&path, false).is_err());
        drop(writer);
        assert_eq!(stamp_mode(&path, false).unwrap().1, b"literal bytes");
    }

    #[test]
    fn budget_registered_identity_checks_known_file_directory_without_byte_binding() {
        let f = Fixture::new(100_000);
        let path = f.base.join("known-identity-file");
        fs::write(&path, b"original bytes").unwrap();
        let file_id = identity(&path).unwrap();
        let dir_id = identity(&f.base).unwrap();
        assert!(registered_identity_matches(&path, &file_id).unwrap());
        assert!(registered_identity_matches(&f.base, &dir_id).unwrap());
        fs::write(&path, b"different length and truthful mutable bytes").unwrap();
        assert!(registered_identity_matches(&path, &file_id).unwrap());
        assert!(!registered_identity_matches(&path, &dir_id).unwrap());
        assert!(!registered_identity_matches(&f.base, &file_id).unwrap());
    }

    #[test]
    fn budget_registered_identity_rejects_foreign_opened_handle_before_callback() {
        let f = Fixture::new(100_000);
        let path = f.base.join("expected-identity-file");
        let foreign = f.base.join("foreign-identity-file");
        fs::write(&path, b"same bytes").unwrap();
        fs::write(&foreign, b"same bytes").unwrap();
        let expected = identity(&path).unwrap();
        let wrong = identity(&foreign).unwrap();
        assert!(!registered_identity_matches(&path, &wrong).unwrap());
        let called = std::cell::Cell::new(false);
        let result = registered_identity_opened(
            &path,
            &expected,
            File::open(&foreign).unwrap(),
            fs::symlink_metadata(&path).unwrap(),
            || {
                called.set(true);
                Ok(())
            },
        );
        assert!(!result.unwrap());
        assert!(!called.get());
        assert_eq!(fs::read(&path).unwrap(), b"same bytes");
        assert_eq!(fs::read(&foreign).unwrap(), b"same bytes");
    }

    #[test]
    fn budget_registered_identity_rejects_same_bytes_mtime_path_replacement() {
        let f = Fixture::new(100_000);
        let path = f.base.join("registered-original");
        fs::write(&path, b"same registered bytes").unwrap();
        let expected = identity(&path).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let before = fs::symlink_metadata(&path).unwrap();
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // Test-only deletion sharing exposes a real replacement window;
            // the production registered revalidator always keeps share3.
            options.share_mode(7);
        }
        let result = registered_identity_opened(
            &path,
            &expected,
            options.open(&path).unwrap(),
            before,
            || {
                fs::rename(&path, f.base.join("retained-registered-original"))
                    .map_err(|e| ProjectError::io(&path, e))?;
                fs::write(&path, b"same registered bytes")
                    .map_err(|e| ProjectError::io(&path, e))?;
                OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .map_err(|e| ProjectError::io(&path, e))?
                    .set_times(fs::FileTimes::new().set_modified(modified))
                    .map_err(|e| ProjectError::io(&path, e))
            },
        );
        assert!(!result.unwrap());
        assert_eq!(fs::read(&path).unwrap(), b"same registered bytes");
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        assert_ne!(identity(&path).unwrap(), expected);
    }

    #[test]
    fn budget_registered_identity_rejects_links_and_changed_ancestor() {
        let f = Fixture::new(100_000);
        let parent = f.base.join("registered-parent");
        fs::create_dir(&parent).unwrap();
        let path = parent.join("original");
        fs::write(&path, b"registered literal bytes").unwrap();
        let expected = identity(&path).unwrap();
        let result = registered_identity_opened(
            &path,
            &expected,
            File::open(&path).unwrap(),
            fs::symlink_metadata(&path).unwrap(),
            || {
                fs::hard_link(&path, f.base.join("unexpected-second-link"))
                    .map_err(|e| ProjectError::io(&path, e))
            },
        );
        assert_eq!(result.unwrap_err().code(), "COMPILE_INTEGRITY");
        assert_eq!(
            registered_identity_matches(&path, &expected)
                .unwrap_err()
                .code(),
            "COMPILE_INTEGRITY"
        );
        fs::remove_file(f.base.join("unexpected-second-link")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let leaf = f.base.join("registered-leaf-link");
            symlink(&path, &leaf).unwrap();
            assert_eq!(
                registered_identity_matches(&leaf, &expected)
                    .unwrap_err()
                    .code(),
                "PROJECT_UNSAFE_PATH"
            );
            let result = registered_identity_opened(
                &path,
                &expected,
                File::open(&path).unwrap(),
                fs::symlink_metadata(&path).unwrap(),
                || {
                    let saved = f.base.join("retained-original-parent");
                    fs::rename(&parent, &saved).map_err(|e| ProjectError::io(&parent, e))?;
                    symlink(&saved, &parent).map_err(|e| ProjectError::io(&parent, e))
                },
            );
            assert_eq!(result.unwrap_err().code(), "PROJECT_UNSAFE_PATH");
        }
        #[cfg(windows)]
        {
            let junction = f.base.join("registered-junction");
            let junction_text = junction.to_str().unwrap();
            let parent_text = parent.to_str().unwrap();
            let output = std::process::Command::new("cmd.exe")
                .args(["/D", "/C", "mklink", "/J"])
                // canonicalize gives a verbatim path; cmd's builtin receives
                // the equivalent ordinary local path only for this setup.
                .arg(junction_text.strip_prefix(r"\\?\").unwrap_or(junction_text))
                .arg(parent_text.strip_prefix(r"\\?\").unwrap_or(parent_text))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "synthetic junction setup: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                registered_identity_matches(&junction.join("original"), &expected)
                    .unwrap_err()
                    .code(),
                "PROJECT_UNSAFE_PATH"
            );
            fs::remove_dir(&junction).unwrap();
        }
    }

    #[test]
    fn budget_registered_identity_requires_complete_chain_and_keeps_windows_sharing() {
        let f = Fixture::new(100_000);
        let plan = f.plan("first", 1);
        let root = Path::new(&f.run.run_root);
        let expected = directory_chain(root, &plan.attempt_root).unwrap();
        assert!(known_expected_chain(root, &plan.attempt_root, &expected).unwrap());
        let mut missing = expected.clone();
        missing.remove("attempts");
        assert!(!known_expected_chain(root, &plan.attempt_root, &missing).unwrap());
        let mut extra = expected.clone();
        extra.insert("unexpected-prefix".into(), identity(root).unwrap());
        assert!(!known_expected_chain(root, &plan.attempt_root, &extra).unwrap());
        let mut wrong = expected.clone();
        wrong.insert("attempts".into(), identity(root).unwrap());
        assert!(!known_expected_chain(root, &plan.attempt_root, &wrong).unwrap());
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            for bytes in [b"".as_slice(), b"x".as_slice()] {
                let path = f.base.join("registered-held-lock");
                fs::write(&path, bytes).unwrap();
                let expected = identity(&path).unwrap();
                let held = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .share_mode(1)
                    .open(&path)
                    .unwrap();
                held.try_lock().unwrap();
                // Prove a real denied nonempty data read, and also exercise
                // zero-byte locks without assuming a library's EOF shortcut.
                if !bytes.is_empty() {
                    assert!(fs::read(&path).is_err());
                }
                assert!(registered_identity_matches(&path, &expected).unwrap());
                held.unlock().unwrap();
            }
        }
    }

    fn literal_database_header() -> [u8; 100] {
        let mut bytes = [0_u8; 100];
        bytes[..16].copy_from_slice(b"SQLite format 3\0");
        bytes[18] = 1;
        bytes[19] = 1;
        bytes
    }

    #[test]
    fn budget_db_header_accepts_registered_identity_and_size_boundaries() {
        let f = Fixture::new(100_000);
        let path = f.base.join("header-original");
        let header = literal_database_header();
        fs::write(&path, header).unwrap();
        let expected = identity(&path).unwrap();
        for len in [100, BYTE_LIMIT] {
            OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(len)
                .unwrap();
            database_header(&path, &expected).unwrap();
            assert_eq!(fs::metadata(&path).unwrap().len(), len);
        }
    }

    #[test]
    fn budget_db_header_rejects_same_bytes_mtime_identity_replacement() {
        let f = Fixture::new(100_000);
        let path = f.base.join("header-original");
        let header = literal_database_header();
        fs::write(&path, header).unwrap();
        let expected = identity(&path).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let initial = same_file::Handle::from_path(&path).unwrap();
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // Test-only deletion sharing permits a real pathname replacement;
            // production database_header keeps the stricter owned share3.
            options.share_mode(7);
        }
        let file = options.open(&path).unwrap();
        let result = database_header_opened(&path, &expected, file, initial, || {
            fs::rename(&path, f.base.join("header-original-retained"))
                .map_err(|e| ProjectError::io(&path, e))?;
            fs::write(&path, header).map_err(|e| ProjectError::io(&path, e))?;
            OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(|e| ProjectError::io(&path, e))?
                .set_times(fs::FileTimes::new().set_modified(modified))
                .map_err(|e| ProjectError::io(&path, e))
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), header);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        assert_ne!(identity(&path).unwrap(), expected);
    }

    #[test]
    fn budget_db_header_rejects_post_snapshot_links_lengths_and_mtime() {
        let f = Fixture::new(100_000);
        for mutation in ["hardlink", "grow", "shrink", "mtime"] {
            let path = f.base.join(format!("header-{mutation}"));
            fs::write(&path, literal_database_header()).unwrap();
            let expected = identity(&path).unwrap();
            let modified = fs::metadata(&path).unwrap().modified().unwrap();
            let initial = same_file::Handle::from_path(&path).unwrap();
            let file = File::open(&path).unwrap();
            let result = database_header_opened(&path, &expected, file, initial, || {
                if mutation == "hardlink" {
                    return fs::hard_link(&path, f.base.join("header-second-link"))
                        .map_err(|e| ProjectError::io(&path, e));
                }
                let writer = OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .map_err(|e| ProjectError::io(&path, e))?;
                if mutation == "mtime" {
                    writer
                        .set_times(
                            fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(20)),
                        )
                        .map_err(|e| ProjectError::io(&path, e))
                } else {
                    writer
                        .set_len(if mutation == "grow" { 101 } else { 99 })
                        .and_then(|_| writer.set_times(fs::FileTimes::new().set_modified(modified)))
                        .map_err(|e| ProjectError::io(&path, e))
                }
            });
            assert!(result.is_err(), "mutation {mutation}");
        }
    }

    #[test]
    fn budget_db_header_rejects_wal_short_over_limit_and_wrong_identity_before_owner() {
        let f = Fixture::new(100_000);
        let path = f.base.join("header-invalid");
        for (name, len, wal) in [
            ("short", 99, false),
            ("over-limit", BYTE_LIMIT + 1, false),
            ("wal", 100, true),
        ] {
            let mut header = literal_database_header();
            if wal {
                header[18] = 2;
                header[19] = 2;
            }
            fs::write(&path, header).unwrap();
            OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(len)
                .unwrap();
            let expected = identity(&path).unwrap();
            assert!(database_header(&path, &expected).is_err(), "{name}");
        }
        fs::write(&path, literal_database_header()).unwrap();
        let other = f.base.join("other-header");
        fs::write(&other, literal_database_header()).unwrap();
        assert!(database_header(&path, &identity(&other).unwrap()).is_err());

        let store = f.store();
        let owner = store.owner.clone();
        let owner_bytes = store.owner_bytes.clone();
        drop(store);
        let db = f.authority.join(DB);
        let mut original = fs::read(&db).unwrap();
        original[18] = 2;
        original[19] = 2;
        fs::write(&db, &original).unwrap();
        // Keep the same allowed name but make opening owner unsafe. A DB/WAL
        // failure must be observed before any read of that owner pathname.
        let owner_path = f.authority.join(OWNER);
        fs::remove_file(&owner_path).unwrap();
        fs::create_dir(&owner_path).unwrap();
        let rejected = layout(&f.authority, &owner, &owner_bytes).unwrap_err();
        assert_eq!(rejected.code(), "REQUEST_BUDGET_INTEGRITY");
        assert!(rejected.to_string().contains("DELETE journal"));
        assert_eq!(fs::read(&db).unwrap(), original);
        assert_eq!(
            names(&f.authority.join("control")).unwrap(),
            BTreeSet::from([
                "budget.sqlite".into(),
                "owner.json".into(),
                "authority.lock".into(),
                "policy.json".into(),
            ])
        );
    }

    #[test]
    fn budget_checked_reserve_requires_the_original_prepared_plan() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        assert!(s.reserve_prepared_checked(&p, |_| Ok(())).is_err());
        assert_eq!(s.view().unwrap().sequence, 1);
        s.prepare(&p).unwrap();
        let mut changed = p.clone();
        changed.request_body.sha256 = sha256(b"different body");
        assert!(s.reserve_prepared_checked(&changed, |_| Ok(())).is_err());
        assert_eq!(s.view().unwrap().sequence, 2);
        assert_eq!(
            s.reserve_prepared_checked(&p, |_| Ok(())).unwrap(),
            ReservationDecision::Reserved
        );
        assert_eq!(s.view().unwrap().sequence, 3);
    }

    #[test]
    fn budget_checked_guards_reject_before_reserve_or_send_commit() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.prepare(&p).unwrap();
        let before = s.view().unwrap();
        assert!(
            s.reserve_prepared_checked(&p, |view| {
                assert_eq!(view, &before);
                Err(invalid("guard rejected reservation"))
            })
            .is_err()
        );
        assert_eq!(s.view().unwrap(), before);
        s.reserve_prepared_checked(&p, |_| Ok(())).unwrap();
        let reserved = s.view().unwrap();
        assert!(
            s.mark_may_send_checked(&p.key, |view| {
                assert_eq!(view, &reserved);
                Err(invalid("guard rejected send"))
            })
            .is_err()
        );
        assert_eq!(s.view().unwrap(), reserved);
        assert_eq!(
            s.mark_may_send_checked(&p.key, |_| Ok(())).unwrap(),
            DispatchDecision::Permitted
        );
    }

    #[test]
    fn budget_checked_reserve_rebuilds_another_workers_latest_commitment() {
        let mut f = Fixture::new(100_000);
        let p = f.plan("first", 1);
        let q = f.plan("second", 1);
        f.spec.global_limits.max_total_tokens = p.reserved_tokens * 2 - 1;
        f.spec.policy_sha256 = sha256(&Fixture::policy(&f.spec));
        let mut s = f.store();
        s.prepare(&p).unwrap();
        let stale = s.view().unwrap();
        let mut other = BudgetStore::open(&f.authority).unwrap();
        assert_eq!(other.reserve(&q).unwrap(), ReservationDecision::Reserved);
        assert!(matches!(
            s.reserve_prepared_checked(&p, |view| {
                assert!(view.sequence > stale.sequence);
                assert_eq!(view.totals.held_tokens, q.reserved_tokens);
                Ok(())
            })
            .unwrap(),
            ReservationDecision::Denied {
                scope: DenialScope::Global,
                ..
            }
        ));
        assert_eq!(s.view().unwrap().totals.held_tokens, q.reserved_tokens);
    }

    #[test]
    fn budget_checked_guard_precedes_decision_clock_and_input_recheck() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let completed = std::cell::Cell::new(OffsetDateTime::now_utc());
        s.update_checked(
            |_| {
                thread::sleep(Duration::from_millis(20));
                completed.set(OffsetDateTime::now_utc());
                Ok(())
            },
            |_, at| {
                assert!(utc(at).unwrap() >= completed.get());
                Ok(((), None))
            },
        )
        .unwrap();
        let p = f.plan("first", 1);
        s.prepare(&p).unwrap();
        let input = Path::new(f.run.input_pins.keys().next().unwrap());
        assert!(
            s.reserve_prepared_checked(&p, |_| {
                fs::write(input, b"changed during checked guard\n").unwrap();
                Ok(())
            })
            .is_err()
        );
        let v = s.view().unwrap();
        assert_eq!(attempt(&v, &p.key).unwrap().state, AttemptState::Prepared);
        assert_eq!(v.totals.held_tokens, 0);
    }

    #[test]
    fn budget_prepared_denial_seals_originals_without_commitments() {
        let f = Fixture::new(1);
        let mut s = f.store();
        let p = f.plan("first", 1);
        assert!(matches!(
            s.reserve(&p).unwrap(),
            ReservationDecision::Denied {
                scope: DenialScope::Global,
                ..
            }
        ));
        let first = s.view().unwrap();
        assert_eq!(first.sequence, 2);
        assert_eq!(
            attempt(&first, &p.key).unwrap().state,
            AttemptState::Prepared
        );
        assert_eq!(first.totals.held_tokens, 0);
        assert_eq!(first.totals.held_requests, 0);
        assert!(matches!(
            s.reserve(&p).unwrap(),
            ReservationDecision::Denied { .. }
        ));
        assert_eq!(s.view().unwrap().sequence, 2);
        let r = f.receipt(&p, None, 0, "EXTRACT_BUDGET_STOP");
        s.settle(&p.key, &r).unwrap();
        assert_eq!(s.view().unwrap().totals.dispatched_requests, 0);
    }
    #[test]
    fn budget_two_threads_cannot_reserve_the_same_remaining_capacity() {
        let mut f = Fixture::new(100_000);
        let p = f.plan("first", 1);
        let q = f.plan("second", 1);
        f.spec.global_limits.max_total_tokens = p.reserved_tokens * 2 - 1;
        f.spec.policy_sha256 = sha256(&Fixture::policy(&f.spec));
        let s = f.store();
        drop(s);
        let barrier = std::sync::Barrier::new(2);
        let results = thread::scope(|scope| {
            let a = scope.spawn(|| {
                let mut s = BudgetStore::open(&f.authority).unwrap();
                barrier.wait();
                s.reserve(&p).unwrap()
            });
            let b = scope.spawn(|| {
                let mut s = BudgetStore::open(&f.authority).unwrap();
                barrier.wait();
                s.reserve(&q).unwrap()
            });
            [a.join().unwrap(), b.join().unwrap()]
        });
        assert_eq!(
            results
                .iter()
                .filter(|r| **r == ReservationDecision::Reserved)
                .count(),
            1
        );
        let v = BudgetStore::open(&f.authority).unwrap().view().unwrap();
        assert_eq!(v.totals.held_tokens, p.reserved_tokens);
        assert_eq!(v.totals.held_requests, 1);
    }
    #[test]
    fn budget_near_cap_known_refund_still_cannot_admit_one_token_too_many() {
        let mut f = Fixture::new(100_000);
        let p = f.plan("first", 1);
        let q = f.plan("second", 1);
        f.spec.global_limits.max_total_tokens = p.reserved_tokens + 699;
        f.spec.policy_sha256 = sha256(&Fixture::policy(&f.spec));
        let mut s = f.store();
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let r = f.receipt(&p, Some(usage()), 1, "EXTRACT_API_CANDIDATES");
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &r).unwrap();
        assert_eq!(
            s.view().unwrap().totals.remaining_tokens,
            p.reserved_tokens - 1
        );
        assert!(matches!(
            s.reserve(&q).unwrap(),
            ReservationDecision::Denied {
                scope: DenialScope::Global,
                ..
            }
        ));
    }
    #[test]
    fn budget_dispatch_permission_is_not_an_idempotent_second_send() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        assert_eq!(
            s.mark_may_send(&p.key).unwrap(),
            DispatchDecision::Permitted
        );
        let seq = s.view().unwrap().sequence;
        assert_eq!(
            s.mark_may_send(&p.key).unwrap(),
            DispatchDecision::AlreadyMaySend
        );
        assert_eq!(s.view().unwrap().sequence, seq);
        s.retain_unknown(&p.key, "REQUEST_INTERRUPTED").unwrap();
        assert!(s.release_reserved(&p.key, "recovery").is_err());
        assert_eq!(s.view().unwrap().totals.held_tokens, p.reserved_tokens);
    }
    #[test]
    fn budget_same_response_sha_is_charged_for_each_distinct_request() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        let q = f.plan("second", 1);
        assert_ne!(p.key, q.key);
        let mut pins = Vec::new();
        for p in [&p, &q] {
            s.reserve(p).unwrap();
            s.mark_may_send(&p.key).unwrap();
            let r = f.receipt(p, Some(usage()), 1, "EXTRACT_API_CANDIDATES");
            pins.push(r.response.clone());
            s.seal_transport(&p.key).unwrap();
            s.settle(&p.key, &r).unwrap();
        }
        assert_eq!(pins[0], pins[1]);
        let v = s.view().unwrap();
        assert_eq!(v.totals.accounted_tokens, 1400);
        assert_eq!(v.totals.dispatched_requests, 2);
        assert_eq!(v.totals.held_tokens, 0);
    }
    #[test]
    fn budget_duplicate_settlement_after_reopen_is_once_and_conflicts_refuse() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let r = f.receipt(&p, Some(usage()), 1, "EXTRACT_API_CANDIDATES");
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &r).unwrap();
        drop(s);
        let mut s = BudgetStore::open(&f.authority).unwrap();
        let before = s.view().unwrap();
        assert_eq!(
            s.settle(&p.key, &r).unwrap(),
            SettlementDecision::AlreadySettled
        );
        assert_eq!(s.view().unwrap(), before);
        let mut changed = r.clone();
        changed.usage.as_mut().unwrap().input_tokens += 1;
        changed.usage.as_mut().unwrap().total_tokens += 1;
        assert!(s.settle(&p.key, &changed).is_err());
        assert_eq!(s.view().unwrap(), before);
    }
    #[test]
    fn budget_unknown_without_final_report_can_later_settle_same_raw_identity() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let mut r = f.receipt(&p, Some(usage()), 1, "EXTRACT_API_CANDIDATES");
        s.seal_transport(&p.key).unwrap();
        let dir = Path::new(&f.run.run_root).join(&p.attempt_root);
        fs::rename(
            dir.join("validation.json"),
            dir.join("recovery-validation.json"),
        )
        .unwrap();
        fs::remove_file(dir.join("validation.md")).unwrap();
        s.retain_unknown(&p.key, "REQUEST_INTERRUPTED").unwrap();
        assert_eq!(s.view().unwrap().totals.held_tokens, p.reserved_tokens);
        r.report.sha256 = stamp(&dir.join("recovery-validation.json"))
            .unwrap()
            .0
            .sha256;
        s.settle(&p.key, &r).unwrap();
        assert_eq!(s.view().unwrap().totals.accounted_tokens, 700);
        assert_eq!(s.view().unwrap().totals.held_tokens, 0);
    }
    #[test]
    fn budget_known_usage_over_reservation_is_saved_and_stops_new_dispatch() {
        let mut f = Fixture::new(100_000);
        let p = f.plan("first", 1);
        f.spec.global_limits.max_total_tokens = p.reserved_tokens;
        f.spec.policy_sha256 = sha256(&Fixture::policy(&f.spec));
        let mut s = f.store();
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let r = f.receipt(
            &p,
            Some(TokenUsage {
                input_tokens: 1,
                output_tokens: 2000,
                total_tokens: 2001,
            }),
            1,
            "EXTRACT_BUDGET_STOP",
        );
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &r).unwrap();
        let v = s.view().unwrap();
        assert_eq!(v.totals.accounted_tokens, 2001);
        assert!(v.global_stopped);
        assert!(run(&v, &p.key.run_id).unwrap().budget_stopped);
        assert_eq!(v.totals.remaining_tokens, 0);
    }
    #[test]
    fn budget_wire_candidate_error_is_not_retryable_but_http_retry_has_durable_due() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let r = f.receipt(&p, Some(usage()), 1, "EXTRACT_API_CANDIDATES");
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &r).unwrap();
        assert!(
            s.schedule_retry_after(&p.key, "EXTRACT_API_CANDIDATES")
                .is_err()
        );
        let q = f.plan("second", 1);
        s.reserve(&q).unwrap();
        s.mark_may_send(&q.key).unwrap();
        let r = f.receipt(&q, None, 1, "EXTRACT_API_HTTP_SERVER");
        s.seal_transport(&q.key).unwrap();
        s.settle(&q.key, &r).unwrap();
        let due = s
            .schedule_retry_after(&q.key, "EXTRACT_API_HTTP_SERVER")
            .unwrap();
        let q2 = f.plan("second", 2);
        assert!(matches!(
            s.reserve(&q2).unwrap(),
            ReservationDecision::Denied {
                scope: DenialScope::RetryNotDue,
                ..
            }
        ));
        while OffsetDateTime::now_utc() < utc(&due.retry_at_utc).unwrap() {
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(s.reserve(&q2).unwrap(), ReservationDecision::Reserved);
        assert_eq!(
            s.view().unwrap().totals.held_tokens,
            q.reserved_tokens + q2.reserved_tokens
        );
    }
    #[test]
    fn budget_receipt_retry_checked_revalidates_inputs_after_guard_without_append() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let receipt = f.receipt(&p, None, 1, "EXTRACT_API_HTTP_SERVER");
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &receipt).unwrap();
        let before = fs::read(f.authority.join(DB)).unwrap();
        let input = f.run.input_pins.keys().next().unwrap();
        let mut reached_guard = false;
        let e = s
            .schedule_receipt_retry_checked(
                &p.key,
                |_| {
                    reached_guard = true;
                    fs::write(input, b"changed after complete witness, before decision").unwrap();
                    Ok(())
                },
                || panic!("changed input must fail before retry decision"),
            )
            .unwrap_err();
        assert!(reached_guard);
        assert_eq!(e.code(), "REQUEST_BUDGET_INPUT_CHANGED");
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
    }
    #[test]
    fn budget_receipt_retry_checked_samples_clock_after_the_complete_guard() {
        let mut f = Fixture::new(100_000);
        let deadline = OffsetDateTime::now_utc() + time::Duration::seconds(4);
        f.run.deadline_utc = deadline.format(&Rfc3339).unwrap();
        for unit in &mut f.run.units {
            unit.deadline_utc = f.run.deadline_utc.clone();
        }
        f.run.retry_base_delay_ms = 3000;
        f.run.retry_max_delay_ms = 3000;
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let receipt = f.receipt(&p, None, 1, "EXTRACT_API_HTTP_SERVER");
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &receipt).unwrap();
        let before = fs::read(f.authority.join(DB)).unwrap();
        let threshold = deadline - time::Duration::milliseconds(2990);
        let mut reached_guard = false;
        let result = s
            .schedule_receipt_retry_checked(
                &p.key,
                |_| {
                    reached_guard = true;
                    while OffsetDateTime::now_utc() < threshold {
                        thread::sleep(Duration::from_millis(2));
                    }
                    assert!(OffsetDateTime::now_utc() < deadline);
                    Ok(())
                },
                || false,
            )
            .unwrap();
        assert!(reached_guard);
        assert_eq!(
            result,
            ReceiptRetryDecision::Finish {
                outcome: UnitOutcome::Failed,
                diagnostic: "No retry fits the original absolute deadline".into(),
            }
        );
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
        assert!(attempt(&s.view().unwrap(), &p.key).unwrap().retry.is_none());
    }
    #[test]
    fn budget_receipt_retry_checked_rebuilds_another_writers_terminal_or_overrun() {
        let f = Fixture::new(100_000);
        let mut writer = f.store();
        let p = f.plan("first", 1);
        writer.reserve(&p).unwrap();
        writer.mark_may_send(&p.key).unwrap();
        let receipt = f.receipt(&p, None, 1, "EXTRACT_API_HTTP_SERVER");
        writer.seal_transport(&p.key).unwrap();
        writer.settle(&p.key, &receipt).unwrap();
        let mut reader = BudgetStore::open(&f.authority).unwrap();
        assert!(
            unit(
                run(&reader.view().unwrap(), &p.key.run_id).unwrap(),
                "first"
            )
            .unwrap()
            .outcome
            .is_none()
        );
        writer
            .finish_unit(&p.key.run_id, "first", UnitOutcome::Failed, "other writer")
            .unwrap();
        let before = fs::read(f.authority.join(DB)).unwrap();
        assert_eq!(
            reader
                .schedule_receipt_retry_checked(
                    &p.key,
                    |_| Ok(()),
                    || { panic!("confirmed terminal must precede stop/retry") }
                )
                .unwrap(),
            ReceiptRetryDecision::Unchanged
        );
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);

        let f = Fixture::new(4000);
        let mut writer = f.store();
        let p = f.plan("first", 1);
        let q = f.plan("second", 1);
        writer.reserve(&p).unwrap();
        writer.mark_may_send(&p.key).unwrap();
        writer.reserve(&q).unwrap();
        writer.mark_may_send(&q.key).unwrap();
        let receipt = f.receipt(&p, None, 1, "EXTRACT_API_HTTP_SERVER");
        writer.seal_transport(&p.key).unwrap();
        writer.settle(&p.key, &receipt).unwrap();
        let mut reader = BudgetStore::open(&f.authority).unwrap();
        assert!(!reader.view().unwrap().global_stopped);
        let overrun = f.receipt(
            &q,
            Some(TokenUsage {
                input_tokens: 1,
                output_tokens: 6000,
                total_tokens: 6001,
            }),
            1,
            "EXTRACT_BUDGET_STOP",
        );
        writer.seal_transport(&q.key).unwrap();
        writer.settle(&q.key, &overrun).unwrap();
        let before = fs::read(f.authority.join(DB)).unwrap();
        assert_eq!(
            reader
                .schedule_receipt_retry_checked(
                    &p.key,
                    |_| Ok(()),
                    || { panic!("confirmed overrun must precede stop/retry") }
                )
                .unwrap(),
            ReceiptRetryDecision::Finish {
                outcome: UnitOutcome::BudgetStopped,
                diagnostic: "Confirmed overrun stops all new sends".into(),
            }
        );
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
    }
    #[test]
    fn budget_receipt_retry_checked_requires_full_key_receipt_before_terminal() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let receipt = f.receipt(&p, None, 1, "EXTRACT_API_HTTP_SERVER");
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &receipt).unwrap();
        s.finish_unit(
            &p.key.run_id,
            "first",
            UnitOutcome::Failed,
            "already terminal",
        )
        .unwrap();
        let mut foreign = p.key.clone();
        foreign.request_id = uuid::Uuid::new_v4().to_string();
        let before = fs::read(f.authority.join(DB)).unwrap();
        assert!(
            s.schedule_receipt_retry_checked(&foreign, |_| Ok(()), || false)
                .is_err()
        );
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
        let q = f.plan("second", 1);
        s.reserve(&q).unwrap();
        s.finish_unit(&q.key.run_id, "second", UnitOutcome::Failed, "no receipt")
            .unwrap();
        let before = fs::read(f.authority.join(DB)).unwrap();
        let e = s
            .schedule_receipt_retry_checked(&q.key, |_| Ok(()), || false)
            .unwrap_err();
        assert!(
            e.to_string()
                .contains("without an authoritative final receipt")
        );
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
    }
    #[test]
    fn budget_receipt_retry_checked_appends_once_and_reopens_idempotently() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let receipt = f.receipt(&p, None, 1, "EXTRACT_API_HTTP_SERVER");
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &receipt).unwrap();
        let before = s.view().unwrap();
        let result = s
            .schedule_receipt_retry_checked(&p.key, |_| Ok(()), || false)
            .unwrap();
        let ReceiptRetryDecision::Scheduled(schedule) = result else {
            panic!("eligible original HTTP failure must schedule")
        };
        let scheduled = s.view().unwrap();
        assert_eq!(scheduled.sequence, before.sequence + 1);
        assert_eq!(scheduled.totals, before.totals);
        assert_eq!(
            attempt(&scheduled, &p.key).unwrap().retry.as_ref(),
            Some(&schedule)
        );
        let original_db = fs::read(f.authority.join(DB)).unwrap();
        assert_eq!(
            s.schedule_receipt_retry_checked(
                &p.key,
                |_| Ok(()),
                || { panic!("existing retry must precede stop/retry") }
            )
            .unwrap(),
            ReceiptRetryDecision::Unchanged
        );
        drop(s);
        let mut s = BudgetStore::open(&f.authority).unwrap();
        assert_eq!(
            s.schedule_receipt_retry_checked(&p.key, |_| Ok(()), || false)
                .unwrap(),
            ReceiptRetryDecision::Unchanged
        );
        assert_eq!(s.view().unwrap(), scheduled);
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), original_db);
    }
    #[test]
    fn budget_receipt_retry_checked_complete_200_is_final_and_stop_only_blocks_retry() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let receipt = f.receipt(&p, Some(usage()), 1, "EXTRACT_TIMEOUT");
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &receipt).unwrap();
        let before = fs::read(f.authority.join(DB)).unwrap();
        assert_eq!(
            s.schedule_receipt_retry_checked(
                &p.key,
                |_| Ok(()),
                || { panic!("complete 200 with known usage is not a retry-stage stop") }
            )
            .unwrap(),
            ReceiptRetryDecision::Finish {
                outcome: UnitOutcome::Failed,
                diagnostic: "EXTRACT_TIMEOUT".into(),
            }
        );
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
        let q = f.plan("second", 1);
        s.reserve(&q).unwrap();
        s.mark_may_send(&q.key).unwrap();
        let receipt = f.receipt(&q, None, 1, "EXTRACT_API_HTTP_SERVER");
        s.seal_transport(&q.key).unwrap();
        s.settle(&q.key, &receipt).unwrap();
        let before = fs::read(f.authority.join(DB)).unwrap();
        assert_eq!(
            s.schedule_receipt_retry_checked(&q.key, |_| Ok(()), || true)
                .unwrap_err()
                .code(),
            "REQUEST_RUN_STOPPED"
        );
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
    }
    fn refuse_local_timeout_retry_and_rehashed_event(
        f: &Fixture,
        s: &mut BudgetStore,
        p: &AttemptPlan,
    ) {
        let original = fs::read(f.authority.join(DB)).unwrap();
        assert!(s.schedule_retry_after(&p.key, "EXTRACT_TIMEOUT").is_err());
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), original);
        let v = s.view().unwrap();
        let at = now().unwrap();
        let record = EventRecord {
            schema_version: SCHEMA_VERSION,
            authority_id: f.spec.authority_id.clone(),
            sequence: v.sequence + 1,
            previous_sha256: v.last_sha256.clone(),
            at_utc: at.clone(),
            event: BudgetEvent::RetryScheduled {
                key: p.key.clone(),
                schedule: RetrySchedule {
                    retry_at_utc: (utc(&at).unwrap() + time::Duration::milliseconds(500))
                        .format(&Rfc3339)
                        .unwrap(),
                    error_code: "EXTRACT_TIMEOUT".into(),
                },
            },
        };
        let raw = encode(&record).unwrap();
        s.connection
            .execute(
                "INSERT INTO events VALUES(?1,?2,?3,?4)",
                params![
                    record.sequence as i64,
                    record.previous_sha256,
                    raw,
                    sha256(raw.as_bytes())
                ],
            )
            .unwrap();
        let attacked = fs::read(f.authority.join(DB)).unwrap();
        assert!(s.view().is_err());
        assert!(BudgetStore::open_readonly(&f.authority).is_err());
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), attacked);
    }
    #[test]
    fn budget_complete_200_known_usage_local_timeout_cannot_schedule_or_replay_retry() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let r = f.receipt(&p, Some(usage()), 1, "EXTRACT_TIMEOUT");
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &r).unwrap();
        let v = s.view().unwrap();
        assert_eq!(v.totals.accounted_tokens, 700);
        assert_eq!(attempt(&v, &p.key).unwrap().state, AttemptState::Known);
        refuse_local_timeout_retry_and_rehashed_event(&f, &mut s, &p);
    }
    #[test]
    fn budget_complete_200_unknown_usage_local_timeout_cannot_schedule_or_replay_retry() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let mut r = f.receipt(&p, None, 1, "EXTRACT_TIMEOUT");
        r.http_status = Some(200);
        let root = Path::new(&f.run.run_root).join(&p.attempt_root);
        let mut t: TransportReceipt =
            serde_json::from_slice(&fs::read(root.join("transport-receipt.json")).unwrap())
                .unwrap();
        t.http_status = Some(200);
        fs::write(
            root.join("transport-receipt.json"),
            serde_json::to_vec_pretty(&t).unwrap(),
        )
        .unwrap();
        let mut report = json(&root.join("validation.json")).unwrap();
        report["http_status"] = serde_json::json!(200);
        let bytes = serde_json::to_vec_pretty(&report).unwrap();
        fs::write(root.join("validation.json"), &bytes).unwrap();
        fs::write(
            root.join("validation.md"),
            format!(
                "synthetic witness\n```json\n{}\n```\n",
                std::str::from_utf8(&bytes).unwrap()
            ),
        )
        .unwrap();
        r.report = FilePin {
            sha256: sha256(&bytes),
            byte_length: bytes.len() as u64,
        };
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &r).unwrap();
        let v = s.view().unwrap();
        assert_eq!(v.totals.held_tokens, p.reserved_tokens);
        assert_eq!(attempt(&v, &p.key).unwrap().state, AttemptState::Unknown);
        refuse_local_timeout_retry_and_rehashed_event(&f, &mut s, &p);
    }
    #[test]
    fn budget_terminal_check_blocks_late_send_and_preserves_held_attempt() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.finish_run(&p.key.run_id, RunOutcome::BudgetStopped, "limit reached")
            .unwrap();
        assert_eq!(s.mark_may_send(&p.key).unwrap(), DispatchDecision::Denied);
        assert_eq!(s.view().unwrap().totals.held_requests, 1);
        s.release_reserved(&p.key, "recovery excluded worker")
            .unwrap();
        let v = s.view().unwrap();
        assert_eq!(v.totals.held_tokens, 0);
        assert_eq!(
            unit(run(&v, &p.key.run_id).unwrap(), "first")
                .unwrap()
                .attempts
                .len(),
            1
        );
    }
    #[test]
    fn budget_opening_and_version_are_not_reset_on_new_run() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let r = f.receipt(&p, Some(usage()), 1, "EXTRACT_API_CANDIDATES");
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &r).unwrap();
        let mut different = f.spec.clone();
        different.opening.accounted_tokens = 1;
        assert!(s.authority_matches(&different).is_err());
        assert!(BudgetStore::create(&f.authority, &f.spec, &Fixture::policy(&f.spec)).is_err());
        let mut owner = s.owner.clone();
        owner.compiler_fingerprint = "f".repeat(64);
        let raw = encode(&owner).unwrap();
        fs::write(f.authority.join(OWNER), &raw).unwrap();
        s.connection
            .execute("UPDATE metadata SET metadata_json=?1", [&raw])
            .unwrap();
        drop(s);
        let mut s = BudgetStore::open(&f.authority).unwrap();
        assert_eq!(s.view().unwrap().totals.accounted_tokens, 700);
        assert_eq!(
            s.verify_version().unwrap_err().code(),
            "REQUEST_BUDGET_VERSION"
        );
        assert_eq!(
            s.register_run(&f.run).unwrap_err().code(),
            "REQUEST_BUDGET_VERSION"
        );
    }
    #[test]
    fn budget_synchronized_owner_metadata_caps_or_opening_cannot_reset_owned_policy() {
        for change_opening in [false, true] {
            let f = Fixture::new(100_000);
            let mut s = f.store();
            let p = f.plan("first", 1);
            s.reserve(&p).unwrap();
            s.mark_may_send(&p.key).unwrap();
            let r = f.receipt(&p, Some(usage()), 1, "EXTRACT_API_CANDIDATES");
            s.seal_transport(&p.key).unwrap();
            s.settle(&p.key, &r).unwrap();
            assert_eq!(s.view().unwrap().totals.accounted_tokens, 700);
            let original_policy = fs::read(f.authority.join(POLICY)).unwrap();
            let mut owner = s.owner.clone();
            if change_opening {
                owner.authority.opening.accounted_tokens = 100;
            } else {
                owner.authority.global_limits.max_total_tokens += 100;
            }
            let raw = encode(&owner).unwrap();
            fs::write(f.authority.join(OWNER), &raw).unwrap();
            s.connection
                .execute("UPDATE metadata SET metadata_json=?1", [&raw])
                .unwrap();
            drop(s);
            let attacked_db = fs::read(f.authority.join(DB)).unwrap();
            assert!(BudgetStore::open(&f.authority).is_err());
            assert!(BudgetStore::open_readonly(&f.authority).is_err());
            assert_eq!(fs::read(f.authority.join(DB)).unwrap(), attacked_db);
            assert_eq!(fs::read(f.authority.join(OWNER)).unwrap(), raw.as_bytes());
            assert_eq!(fs::read(f.authority.join(POLICY)).unwrap(), original_policy);
        }
    }
    #[test]
    fn budget_owned_policy_same_bytes_replacement_and_mtime_change_are_rejected() {
        let f = Fixture::new(100_000);
        let s = f.store();
        let before = fs::read(f.authority.join(DB)).unwrap();
        let path = f.authority.join(POLICY);
        let original = fs::read(&path).unwrap();
        let saved = f.base.join("original-policy");
        fs::rename(&path, &saved).unwrap();
        fs::write(&path, &original).unwrap();
        assert!(s.view().is_err());
        assert!(BudgetStore::open(&f.authority).is_err());
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
        fs::remove_file(&path).unwrap();
        fs::rename(&saved, &path).unwrap();
        let file = OpenOptions::new().write(true).open(&path).unwrap();
        let modified = file.metadata().unwrap().modified().unwrap() + Duration::from_secs(1);
        file.set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        drop(file);
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(s.view().is_err());
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
    }
    #[test]
    fn budget_run_control_replacement_cannot_self_authenticate_in_a_new_owner() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        s.retain_unknown(&p.key, "REQUEST_INTERRUPTED").unwrap();
        let view = s.view().unwrap();
        assert_eq!(view.totals.held_tokens, p.reserved_tokens);
        let before = fs::read(f.authority.join(DB)).unwrap();
        let root = Path::new(&f.run.run_root);
        fs::rename(root.join("control"), f.base.join("original-run-control")).unwrap();
        fs::create_dir(root.join("control")).unwrap();
        fs::write(root.join("control/worker.lock"), []).unwrap();
        let forged = serde_json::to_vec(&serde_json::json!({"run_id":f.run.run_id,"root":f.run.run_root,"control_identity":identity(&root.join("control")).unwrap(),"worker_identity":identity(&root.join("control/worker.lock")).unwrap()})).unwrap();
        fs::write(root.join("control/owner.json"), &forged).unwrap();
        assert_eq!(identity(root).unwrap(), view.runs[0].identity.root);
        assert_eq!(
            identity(root.parent().unwrap()).unwrap(),
            view.runs[0].identity.parent
        );
        assert!(s.view().is_err());
        assert!(s.register_run(&f.run).is_err());
        assert!(BudgetStore::open(&f.authority).is_err());
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
        assert_eq!(fs::read(root.join("control/owner.json")).unwrap(), forged);
    }
    #[test]
    fn budget_registered_run_owner_changed_in_place_is_not_a_new_trust_anchor() {
        let f = Fixture::new(100_000);
        let s = f.store();
        let before = fs::read(f.authority.join(DB)).unwrap();
        let path = Path::new(&f.run.run_root).join("control/owner.json");
        let id = identity(&path).unwrap();
        fs::write(&path, b"different same-inode store-only owner bytes\n").unwrap();
        assert_eq!(identity(&path).unwrap(), id);
        assert!(s.view().is_err());
        assert!(BudgetStore::open_readonly(&f.authority).is_err());
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
    }
    #[test]
    fn budget_actual_artifact_replacement_and_sidecar_refuse_without_sqlite_recovery() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        let before = fs::read(f.authority.join(DB)).unwrap();
        let body = Path::new(&f.run.run_root)
            .join(&p.attempt_root)
            .join("request-body.json");
        let bytes = fs::read(&body).unwrap();
        let saved = f.base.join("saved-original-body");
        fs::rename(&body, &saved).unwrap();
        fs::write(&body, &bytes).unwrap();
        assert!(s.view().is_err());
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
        fs::remove_file(&body).unwrap();
        fs::rename(saved, &body).unwrap();
        let journal = f.authority.join("control/budget.sqlite-journal");
        fs::write(&journal, b"foreign hot-journal witness").unwrap();
        assert!(BudgetStore::open(&f.authority).is_err());
        assert_eq!(fs::read(&journal).unwrap(), b"foreign hot-journal witness");
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
    }
    #[test]
    fn budget_rehashed_accounting_forgery_still_disagrees_with_raw_response() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let r = f.receipt(&p, Some(usage()), 1, "EXTRACT_API_CANDIDATES");
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &r).unwrap();
        let raw: String = s
            .connection
            .query_row(
                "SELECT payload_json FROM events ORDER BY sequence DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let mut record: EventRecord = serde_json::from_str(&raw).unwrap();
        if let BudgetEvent::ReceiptSettled { receipt, .. } = &mut record.event {
            receipt.usage = Some(TokenUsage {
                input_tokens: 1,
                output_tokens: 0,
                total_tokens: 1,
            });
        } else {
            panic!("expected settlement event");
        }
        let raw = encode(&record).unwrap();
        s.connection
            .execute(
                "UPDATE events SET payload_json=?1,payload_sha256=?2 WHERE sequence=?3",
                params![raw, sha256(raw.as_bytes()), record.sequence as i64],
            )
            .unwrap();
        drop(s);
        assert!(BudgetStore::open(&f.authority).is_err());
    }
    #[test]
    fn budget_unsealed_recovery_response_is_unknown_and_cannot_be_adopted() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let r = f.receipt(&p, Some(usage()), 1, "EXTRACT_API_CANDIDATES");
        assert!(s.settle(&p.key, &r).is_err());
        s.retain_unknown(&p.key, "REQUEST_INTERRUPTED").unwrap();
        assert!(s.seal_transport(&p.key).is_err());
        assert!(s.settle(&p.key, &r).is_err());
        assert_eq!(s.view().unwrap().totals.held_tokens, p.reserved_tokens);
        assert_eq!(s.view().unwrap().totals.accounted_tokens, 0);
    }
    #[test]
    fn budget_sealed_transport_sync_forgery_cannot_replace_body_or_framing() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let r = f.receipt(&p, Some(usage()), 1, "EXTRACT_API_CANDIDATES");
        s.seal_transport(&p.key).unwrap();
        let root = Path::new(&f.run.run_root).join(&p.attempt_root);
        let mut t: TransportReceipt =
            serde_json::from_slice(&fs::read(root.join("transport-receipt.json")).unwrap())
                .unwrap();
        t.response_complete = false;
        fs::write(
            root.join("transport-receipt.json"),
            serde_json::to_vec_pretty(&t).unwrap(),
        )
        .unwrap();
        let mut forged = r;
        forged.response_complete = false;
        forged.usage = None;
        assert!(s.settle(&p.key, &forged).is_err());
        assert!(s.view().is_err());
    }
    #[test]
    fn budget_input_change_stops_dispatch_without_refunding_existing_request() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        let path = f.run.input_pins.keys().next().unwrap();
        fs::write(path, b"changed selected author\n").unwrap();
        assert_eq!(
            s.mark_may_send(&p.key).unwrap_err().code(),
            "REQUEST_BUDGET_INPUT_CHANGED"
        );
        assert_eq!(s.view().unwrap().totals.held_tokens, p.reserved_tokens);
        s.invalidate_run(&p.key.run_id, "selected input changed")
            .unwrap();
        assert_eq!(s.view().unwrap().totals.held_tokens, p.reserved_tokens);
    }
    #[test]
    fn budget_may_send_and_unknown_cannot_refund_a_self_reported_zero() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let r = f.receipt(&p, None, 0, "EXTRACT_API_TRANSPORT");
        assert!(s.settle(&p.key, &r).is_err());
        s.retain_unknown(&p.key, "REQUEST_INTERRUPTED").unwrap();
        assert!(s.settle(&p.key, &r).is_err());
        let v = s.view().unwrap();
        assert_eq!(v.totals.held_tokens, p.reserved_tokens);
        assert_eq!(v.totals.dispatched_requests, 1);
    }
    #[test]
    fn budget_may_send_refund_requires_the_sealed_original_zero_transport() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        s.mark_may_send(&p.key).unwrap();
        let mut r = f.receipt(&p, None, 0, "EXTRACT_API_TRANSPORT");
        let root = Path::new(&f.run.run_root).join(&p.attempt_root);
        let t = TransportReceipt {
            schema_version: 1,
            producer: "synthetic_api_transport_receipt".into(),
            request_id: p.key.request_id.clone(),
            http_status: None,
            http_request_id: None,
            response_complete: false,
            network_attempts: 0,
            response: FilePin {
                sha256: sha256(&[]),
                byte_length: 0,
            },
            response_file: None,
            error_code: Some("EXTRACT_API_TRANSPORT".into()),
            omission_reason: None,
            elapsed_ms: 0,
        };
        fs::write(
            root.join("transport-receipt.json"),
            serde_json::to_vec_pretty(&t).unwrap(),
        )
        .unwrap();
        let mut report = json(&root.join("validation.json")).unwrap();
        report["response_receipt"] =
            serde_json::json!({"sha256":sha256(&[]),"byte_length":0,"omission_reason":null});
        let bytes = serde_json::to_vec_pretty(&report).unwrap();
        fs::write(root.join("validation.json"), &bytes).unwrap();
        fs::write(
            root.join("validation.md"),
            format!(
                "synthetic\n```json\n{}\n```\n",
                std::str::from_utf8(&bytes).unwrap()
            ),
        )
        .unwrap();
        r.report = FilePin {
            sha256: sha256(&bytes),
            byte_length: bytes.len() as u64,
        };
        s.seal_transport(&p.key).unwrap();
        s.settle(&p.key, &r).unwrap();
        let v = s.view().unwrap();
        assert_eq!(v.totals.held_tokens, 0);
        assert_eq!(v.totals.dispatched_requests, 0);
    }
    #[test]
    fn budget_replaced_intermediate_ancestor_is_not_hidden_by_same_leaf_ids() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let mut p = f.plan("first", 1);
        let root = Path::new(&f.run.run_root);
        fs::create_dir(root.join("attempts/first/outer")).unwrap();
        fs::create_dir(root.join("attempts/first/outer/inner")).unwrap();
        fs::rename(
            root.join(&p.attempt_root),
            root.join("attempts/first/outer/inner/1"),
        )
        .unwrap();
        p.attempt_root = "attempts/first/outer/inner/1".into();
        s.reserve(&p).unwrap();
        let before = s.view().unwrap();
        let old = f.base.join("old-outer");
        fs::rename(root.join("attempts/first/outer"), &old).unwrap();
        fs::create_dir(root.join("attempts/first/outer")).unwrap();
        fs::rename(old.join("inner"), root.join("attempts/first/outer/inner")).unwrap();
        assert_eq!(
            identity(&root.join(&p.attempt_root)).unwrap(),
            attempt(&before, &p.key).unwrap().identity.root
        );
        assert_eq!(
            identity(&root.join("attempts/first/outer/inner")).unwrap(),
            attempt(&before, &p.key).unwrap().identity.parent
        );
        assert!(s.view().is_err());
    }
    #[cfg(unix)]
    #[test]
    fn budget_hardlinked_request_original_is_rejected() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.reserve(&p).unwrap();
        let body = Path::new(&f.run.run_root)
            .join(&p.attempt_root)
            .join("request-body.json");
        fs::hard_link(&body, f.base.join("other-link")).unwrap();
        assert!(s.view().is_err());
    }
    #[test]
    fn budget_prepared_restart_reserves_same_key_without_another_logical_attempt() {
        let f = Fixture::new(100_000);
        let mut s = f.store();
        let p = f.plan("first", 1);
        s.prepare(&p).unwrap();
        let before = s.view().unwrap();
        assert_eq!(before.sequence, 2);
        assert_eq!(before.totals.held_tokens, 0);
        drop(s);
        let mut s = BudgetStore::open(&f.authority).unwrap();
        s.prepare(&p).unwrap();
        assert_eq!(s.view().unwrap(), before);
        assert_eq!(s.reserve(&p).unwrap(), ReservationDecision::Reserved);
        let view = s.view().unwrap();
        assert_eq!(view.sequence, 3);
        assert_eq!(
            unit(run(&view, &p.key.run_id).unwrap(), "first")
                .unwrap()
                .attempts
                .len(),
            1
        );
    }
    #[test]
    fn budget_readonly_status_cannot_mutate_sqlite_or_append_an_event() {
        let f = Fixture::new(100_000);
        let s = f.store();
        drop(s);
        let before = fs::read(f.authority.join(DB)).unwrap();
        let mut ro = BudgetStore::open_readonly(&f.authority).unwrap();
        let p = f.plan("first", 1);
        assert_eq!(
            ro.prepare(&p).unwrap_err().code(),
            "REQUEST_BUDGET_READ_ONLY"
        );
        assert!(
            ro.connection
                .execute("UPDATE metadata SET metadata_json='changed'", [])
                .is_err()
        );
        assert_eq!(ro.view().unwrap().sequence, 1);
        assert_eq!(fs::read(f.authority.join(DB)).unwrap(), before);
    }
}
