//! 托管测试集合（Vault）的存储层。
//!
//! 这一层只回答「这个目录是不是一个已接管的托管根、里面有什么、账本记了什么」，
//! 不含审批与发布判定（那在 `vault_update`）。所有连接都先核对 application_id、
//! user_version 与三张表的结构；未知布局一律保留现场并拒绝，不迁移、不修复。
//!
//! 目录契约只有四个顶层名字：`control/`、`baseline/`、`vault/`、`releases/`。
//! `baseline/` 是 adopt 冻结的字节，`vault/` 是人可以直接编辑的当前盘面，
//! 二者与候选目录一起构成三路字节。`vault/` 允许人类无关内容（例如 `.obsidian/`），
//! 但拒绝链接、数据库文件与 sidecar；`baseline/`、`releases/` 对未知文件严格。

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    thread,
    time::{Duration, Instant, UNIX_EPOCH},
};

use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    compile_identity::{Identity, identity},
    project::{self, ProjectError},
    run_model::FileStamp,
    text::sha256,
    vault_model::{Author, Refusal},
};

/// 托管账本的 application_id，对应 ASCII `NVUL`。
pub(crate) const APP: i64 = 0x4e56554c;
/// 唯一的账本结构版本；出现别的版本只读拒绝，不迁移。
pub(crate) const SCHEMA_VERSION: u32 = 1;
/// 记账用的生产者标记，避免把别的工具写坏的库当成自己的。
pub(crate) const PRODUCER: &str = "noetome-managed-vault";

pub(crate) const CONTROL: &str = "control";
pub(crate) const OWNER: &str = "owner.json";
pub(crate) const DATABASE: &str = "vault.sqlite";
pub(crate) const LOCK: &str = "update.lock";
pub(crate) const BASELINE: &str = "baseline";
pub(crate) const VAULT: &str = "vault";
pub(crate) const RELEASES: &str = "releases";
pub(crate) const RELEASE_RECORD: &str = "release.json";
pub(crate) const RELEASE_CANDIDATE: &str = "candidate";
pub(crate) const RELEASE_LOG: &str = "commit.log";

/// 单个托管文件与记录文件的读取上限；超出即拒绝，不做流式部分读取。
pub(crate) const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// 一个目录树允许出现的条目上限，防止把整块磁盘当托管根扫描。
const MAX_ENTRIES: usize = 4096;

const SCHEMA: &str = "CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL,metadata_json TEXT NOT NULL);CREATE TABLE plans(plan_id TEXT PRIMARY KEY,kind TEXT NOT NULL,plan_sha256 TEXT NOT NULL UNIQUE,payload_json TEXT NOT NULL,created_at TEXT NOT NULL);CREATE TABLE releases(sequence INTEGER PRIMARY KEY CHECK(sequence>0),release_id TEXT NOT NULL UNIQUE,approval_sha256 TEXT NOT NULL,status TEXT NOT NULL,entries_json TEXT NOT NULL,at TEXT NOT NULL);";

// ---------------------------------------------------------------------------
// 错误
// ---------------------------------------------------------------------------

pub(crate) fn error(code: &'static str, text: impl Into<String>) -> ProjectError {
    ProjectError::new(code, 3, text)
}

/// 调用参数或现场前提不成立：没写任何字节，exit 2。
pub(crate) fn argument(text: impl Into<String>) -> ProjectError {
    ProjectError::new("VAULT_ARGUMENT", 2, text)
}

/// 布局、账本或身份与契约不符：保留现场，exit 3。
pub(crate) fn integrity(text: impl Into<String>) -> ProjectError {
    ProjectError::new("VAULT_INTEGRITY", 3, text)
}

/// 把 `vault_model` 的拒绝码原样交给 CLI，不换码、不降级。
pub(crate) fn refuse(refusal: Refusal) -> ProjectError {
    ProjectError::new(refusal.code, 3, refusal.message)
}

fn busy(text: impl Into<String>) -> ProjectError {
    error("VAULT_BUSY", text)
}

// ---------------------------------------------------------------------------
// 规范化 JSON / 时间
// ---------------------------------------------------------------------------

pub(crate) fn canonical<T: Serialize>(value: &T) -> Result<String, ProjectError> {
    serde_json::to_string(value)
        .map_err(|error| integrity(format!("无法序列化托管记录：{error}。")))
}

/// 严格读取：必须是 UTF-8、必须能按已知字段反序列化，且重新序列化后逐字节相同
/// （这一条同时排除重复键、非规范空白与未知字段）。
pub(crate) fn strict<T>(bytes: &[u8]) -> Result<T, ProjectError>
where
    T: serde::de::DeserializeOwned + Serialize,
{
    let text = std::str::from_utf8(bytes).map_err(|_| integrity("托管记录必须是UTF-8文本。"))?;
    let value: T = serde_json::from_str(text)
        .map_err(|error| integrity(format!("托管记录不是严格JSON：{error}。")))?;
    if canonical(&value)?.as_str() != text {
        return Err(integrity("托管记录非规范编码、重复键或未知字段。"));
    }
    Ok(value)
}

pub(crate) fn now() -> Result<String, ProjectError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| integrity(format!("无法记录UTC时间：{error}。")))
}

// ---------------------------------------------------------------------------
// 路径与字节
// ---------------------------------------------------------------------------

/// 把托管相对路径（永远是 `/` 分隔）拼到某个根下。
pub(crate) fn joined(root: &Path, relative: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
    }
    path
}

fn relative(root: &Path, path: &Path) -> Result<String, ProjectError> {
    let inner = path
        .strip_prefix(root)
        .map_err(|_| integrity(format!("{} 不在 {} 内。", path.display(), root.display())))?;
    let mut parts = Vec::new();
    for component in inner.components() {
        match component {
            Component::Normal(name) => parts.push(
                name.to_str()
                    .ok_or_else(|| ProjectError::unsafe_path(path))?
                    .to_owned(),
            ),
            _ => return Err(ProjectError::unsafe_path(path)),
        }
    }
    Ok(parts.join("/"))
}

pub(crate) fn create_parent_dirs(path: &Path) -> Result<(), ProjectError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| ProjectError::io(parent, error))?;
    }
    Ok(())
}

pub(crate) fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, ProjectError> {
    let mut file = project::read_guard(path)?;
    let metadata = file
        .metadata()
        .map_err(|error| ProjectError::io(path, error))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(integrity(format!(
            "{} 不是普通文件或超过读取上限 {limit} 字节；保留现场并拒绝继续。",
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ProjectError::io(path, error))?;
    if bytes.len() as u64 > limit {
        return Err(integrity(format!(
            "{} 在读取期间超过上限 {limit} 字节；保留现场并拒绝继续。",
            path.display()
        )));
    }
    Ok(bytes)
}

fn write_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x00200000);
    }
    options
}

/// 解析调用方给出的路径：只在词法层面消掉 `.` 与 `..`（绝不跟随符号链接），
/// 再把结果交给仓库既有的严格检查。这样从 `CARGO_MANIFEST_DIR` 拼出来的
/// `crates/noetome-cli/../../fixtures/...` 这类入参可用，而「不许符号链接、
/// 不许路径跳转」的判定仍然逐组件执行在真正会被写入的路径上。
pub(crate) fn resolve_caller_path(path: &Path) -> Result<PathBuf, ProjectError> {
    // `..` 只能弹回调用方自己给出的那几层：相对路径的锚点是它的起点目录，
    // 绝对路径的锚点是文件系统根。弹过锚点就拒绝——`a/../../b` 必须报错，
    // 不能被悄悄折成调用方目录之外的位置。
    let mut base = PathBuf::new();
    let mut depth = 0usize;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => base.push(prefix.as_os_str()),
            Component::RootDir => base.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                if depth == 0 {
                    return Err(ProjectError::unsafe_path(path));
                }
                base.pop();
                depth -= 1;
            }
            Component::Normal(name) => {
                base.push(name);
                depth += 1;
            }
        }
    }
    let resolved = if base.is_absolute() {
        base
    } else {
        std::env::current_dir()
            .map_err(|failure| ProjectError::io(path, failure))?
            .join(base)
    };
    project::check_components(&resolved)
}

/// 只创建不覆盖：目标已存在即失败。写完 sync 再读回逐字节比对。
pub(crate) fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    project::check_components(path)?;
    if project::exists(path)? {
        return Err(argument(format!(
            "写入目标必须尚不存在：{}；未修改任何文件。",
            path.display()
        )));
    }
    create_parent_dirs(path)?;
    let mut options = write_options();
    options.create_new(true);
    let mut file = options
        .open(path)
        .map_err(|error| ProjectError::io(path, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| ProjectError::io(path, error))?;
    drop(file);
    let read_back = read_bounded(path, MAX_BYTES)?;
    if read_back != bytes {
        return Err(integrity(format!(
            "写入后读回不一致：{}；保留现场并拒绝继续。",
            path.display()
        )));
    }
    Ok(())
}

/// 覆盖已有文件的内容（保留 inode，供 owner.json 与逐项落盘使用），
/// 同样写完 sync 再读回比对。
pub(crate) fn write_replace(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    project::check_components(path)?;
    project::checked_file(path)?;
    create_parent_dirs(path)?;
    let mut options = write_options();
    options.truncate(true);
    let mut file = options
        .open(path)
        .map_err(|error| ProjectError::io(path, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| ProjectError::io(path, error))?;
    drop(file);
    let read_back = read_bounded(path, MAX_BYTES)?;
    if read_back != bytes {
        return Err(integrity(format!(
            "写入后读回不一致：{}；保留现场并拒绝继续。",
            path.display()
        )));
    }
    Ok(())
}

/// 追加一行审计记录；日志只追加，从不重写。
pub(crate) fn append_bytes(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    project::check_components(path)?;
    if project::exists(path)? {
        project::checked_file(path)?;
    }
    create_parent_dirs(path)?;
    let mut options = write_options();
    options.create(true).append(true);
    let mut file = options
        .open(path)
        .map_err(|error| ProjectError::io(path, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| ProjectError::io(path, error))?;
    Ok(())
}

fn open_new(path: &Path) -> Result<File, ProjectError> {
    let mut options = write_options();
    options.read(true).create_new(true);
    options
        .open(path)
        .map_err(|error| ProjectError::io(path, error))
}

// ---------------------------------------------------------------------------
// 目录树
// ---------------------------------------------------------------------------

pub(crate) struct Tree {
    pub(crate) files: BTreeMap<String, PathBuf>,
    pub(crate) dirs: BTreeSet<String>,
}

/// 逐层遍历一棵目录树，只接受普通文件与普通目录；链接、reparse 与特殊文件立刻拒绝。
pub(crate) fn tree(root: &Path) -> Result<Tree, ProjectError> {
    require_dir(root)?;
    let mut files = BTreeMap::new();
    let mut dirs = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    let mut seen = 0usize;
    while let Some(dir) = stack.pop() {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&dir).map_err(|error| ProjectError::io(&dir, error))? {
            let entry = entry.map_err(|error| ProjectError::io(&dir, error))?;
            entries.push(entry.path());
        }
        entries.sort();
        for path in entries {
            seen += 1;
            if seen > MAX_ENTRIES {
                return Err(integrity(format!(
                    "{} 下的条目超过上限 {MAX_ENTRIES}；保留现场并拒绝继续。",
                    root.display()
                )));
            }
            let metadata =
                fs::symlink_metadata(&path).map_err(|error| ProjectError::io(&path, error))?;
            if project::is_link(&metadata) {
                return Err(ProjectError::unsafe_path(&path));
            }
            let key = relative(root, &path)?;
            if metadata.is_dir() {
                dirs.insert(key);
                stack.push(path);
            } else if metadata.is_file() {
                files.insert(key, path);
            } else {
                return Err(integrity(format!(
                    "{} 既不是普通文件也不是普通目录；保留现场并拒绝继续。",
                    path.display()
                )));
            }
        }
    }
    Ok(Tree { files, dirs })
}

fn require_dir(path: &Path) -> Result<(), ProjectError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| ProjectError::io(path, error))?;
    if project::is_link(&metadata) || !metadata.is_dir() {
        return Err(ProjectError::unsafe_path(path));
    }
    Ok(())
}

fn names(dir: &Path) -> Result<BTreeSet<String>, ProjectError> {
    let mut out = BTreeSet::new();
    for entry in fs::read_dir(dir).map_err(|error| ProjectError::io(dir, error))? {
        let entry = entry.map_err(|error| ProjectError::io(dir, error))?;
        out.insert(
            entry
                .file_name()
                .to_str()
                .ok_or_else(|| ProjectError::unsafe_path(dir))?
                .to_owned(),
        );
    }
    Ok(out)
}

fn name_set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn is_release_id(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

// ---------------------------------------------------------------------------
// 记录
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BaselineFile {
    pub(crate) path: String,
    pub(crate) sha256: String,
    pub(crate) byte_length: u64,
}

/// `control/owner.json`：谁拥有这个根，以及四个关键目录/文件的持久身份。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Owner {
    pub(crate) policy: String,
    pub(crate) producer: String,
    pub(crate) schema_version: u32,
    pub(crate) root: String,
    pub(crate) root_identity: Identity,
    pub(crate) parent_identity: Identity,
    pub(crate) control_identity: Identity,
    pub(crate) owner_identity: Identity,
    pub(crate) baseline_identity: Identity,
    pub(crate) vault_identity: Identity,
    pub(crate) lock_identity: Identity,
    pub(crate) database_identity: Identity,
    pub(crate) author_manifest_sha256: String,
    pub(crate) baseline_manifest_sha256: String,
    pub(crate) created_at: String,
}

/// `control/vault.sqlite` 的 metadata 行：作者清单原文与冻结后的基准清单。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Metadata {
    pub(crate) policy: String,
    pub(crate) producer: String,
    pub(crate) schema_version: u32,
    pub(crate) root: String,
    pub(crate) parent_identity: Identity,
    pub(crate) created_at: String,
    pub(crate) author: Author,
    pub(crate) author_manifest_sha256: String,
    pub(crate) author_canonical_sha256: String,
    pub(crate) baseline_manifest_sha256: String,
    pub(crate) baseline_files: Vec<BaselineFile>,
}

/// 计划/批准共同绑定的单路径现场指纹；`None` 表示该路径当时不存在。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StampSlot {
    pub(crate) path: String,
    pub(crate) stamp: Option<FileStamp>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlanRow {
    pub(crate) plan_id: String,
    pub(crate) kind: String,
    pub(crate) payload_json: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ReleaseRow {
    pub(crate) sequence: u64,
    pub(crate) release_id: String,
    pub(crate) status: String,
    pub(crate) entries_json: String,
    pub(crate) at: String,
}

#[derive(Default)]
pub(crate) struct Layout {
    pub(crate) incomplete_releases: Vec<String>,
}

// ---------------------------------------------------------------------------
// 打开与校验
// ---------------------------------------------------------------------------

pub(crate) struct VaultStore {
    root: PathBuf,
    owner_bytes: Vec<u8>,
    owner: Owner,
    metadata_json: String,
    pub(crate) metadata: Metadata,
    pub(crate) author: Author,
    connection: Connection,
    layout: Layout,
    baseline_missing: bool,
    baseline_damage: Option<String>,
    vault_files: BTreeMap<String, PathBuf>,
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

fn check_identity(path: &Path, expected: &Identity, label: &str) -> Result<(), ProjectError> {
    let actual = identity(path)?;
    if &actual != expected {
        return Err(integrity(format!(
            "{label}的持久身份与所有权记录不符（adopt 之后可能被替换）；保留现场并拒绝继续。"
        )));
    }
    Ok(())
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
                "托管账本的 {pragma}={value}，要求 {expected}；未知结构只读拒绝，不迁移不修复。"
            )));
        }
    }
    let mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .map_err(ProjectError::database)?;
    if mode.to_ascii_lowercase() != "delete" {
        return Err(integrity(format!(
            "托管账本的 journal_mode={mode}，要求 delete；只读拒绝，不修复。"
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
            "托管账本的表集合与契约不符：实际 {:?}，要求 {:?}；只读拒绝，不迁移不修复。",
            rows.iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            expected.keys().map(String::as_str).collect::<Vec<_>>()
        )));
    }
    for (name, sql) in rows {
        let Some(want) = expected.get(&name) else {
            return Err(integrity(format!(
                "托管账本出现未知表 {name}；只读拒绝，不迁移不修复。"
            )));
        };
        if compact(&sql) != compact(want) {
            return Err(integrity(format!(
                "托管账本表 {name} 的定义与契约不符；只读拒绝，不迁移不修复。"
            )));
        }
    }
    Ok(())
}

fn read_metadata_json(connection: &Connection) -> Result<String, ProjectError> {
    let count: i64 = connection
        .query_row("SELECT count(*) FROM metadata", [], |row| row.get(0))
        .map_err(ProjectError::database)?;
    if count != 1 {
        return Err(integrity(format!(
            "托管账本 metadata 必须有且只有一行，实际 {count} 行；只读拒绝，不修复。"
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
        return Err(integrity("托管账本 metadata 的 singleton 必须是 1。"));
    }
    if schema_version != i64::from(SCHEMA_VERSION) {
        return Err(integrity(format!(
            "托管账本 metadata 的 schema_version={schema_version}，要求 {SCHEMA_VERSION}；只读拒绝。"
        )));
    }
    Ok(metadata_json)
}

fn verify_baseline(base: &Path, files: &[BaselineFile]) -> Result<(), String> {
    let tree = tree(base).map_err(|error| error.to_string())?;
    for dir in &tree.dirs {
        if !tree
            .files
            .keys()
            .any(|file| file.starts_with(&format!("{dir}/")))
        {
            return Err(format!("基准里出现未知空目录 {dir}"));
        }
    }
    let expected: BTreeSet<&str> = files.iter().map(|file| file.path.as_str()).collect();
    let actual: BTreeSet<&str> = tree.files.keys().map(|path| path.as_str()).collect();
    if expected != actual {
        let missing: Vec<&&str> = expected.difference(&actual).collect();
        let extra: Vec<&&str> = actual.difference(&expected).collect();
        return Err(format!("缺少 {missing:?}，多出 {extra:?}"));
    }
    for file in files {
        let path = tree
            .files
            .get(&file.path)
            .ok_or_else(|| format!("缺少 {}", file.path))?;
        let bytes = read_bounded(path, MAX_BYTES).map_err(|error| error.to_string())?;
        if bytes.len() as u64 != file.byte_length || sha256(&bytes) != file.sha256 {
            return Err(format!("{} 的字节与 adopt 冻结清单不符", file.path));
        }
    }
    Ok(())
}

fn verify_releases(root: &Path) -> Result<Layout, ProjectError> {
    let base = root.join(RELEASES);
    if !project::exists(&base)? {
        return Ok(Layout::default());
    }
    require_dir(&base)?;
    let mut incomplete = Vec::new();
    for id in names(&base)? {
        let dir = base.join(&id);
        if !is_release_id(&id) {
            return Err(integrity(format!(
                "releases/ 下只允许 64 位十六进制发布 id；未知条目 {id} 保留原样并拒绝继续。"
            )));
        }
        require_dir(&dir)?;
        let inner = names(&dir)?;
        let allowed = name_set(&[RELEASE_RECORD, RELEASE_CANDIDATE, RELEASE_LOG]);
        let extra: Vec<&String> = inner.difference(&allowed).collect();
        if !extra.is_empty() {
            return Err(integrity(format!(
                "releases/{id}/ 只允许 release.json、candidate/、commit.log；未知条目 {extra:?} 保留原样并拒绝继续。"
            )));
        }
        if !inner.contains(RELEASE_RECORD) {
            incomplete.push(id);
            continue;
        }
        project::checked_file(&dir.join(RELEASE_RECORD))?;
        if inner.contains(RELEASE_CANDIDATE) {
            require_dir(&dir.join(RELEASE_CANDIDATE))?;
        }
        if inner.contains(RELEASE_LOG) {
            project::checked_file(&dir.join(RELEASE_LOG))?;
        }
    }
    Ok(Layout {
        incomplete_releases: incomplete,
    })
}

fn verify_vault(root: &Path) -> Result<BTreeMap<String, PathBuf>, ProjectError> {
    let base = root.join(VAULT);
    require_dir(&base)?;
    let tree = tree(&base)?;
    for path in tree.files.keys() {
        let name = path.rsplit('/').next().unwrap_or(path);
        if name == DATABASE || name.starts_with("vault.sqlite-") {
            return Err(integrity(format!(
                "托管根之外不得出现库文件或 sidecar：vault/{path}；保留现样并拒绝继续。"
            )));
        }
    }
    Ok(tree.files)
}

impl VaultStore {
    pub(crate) fn open(root: &Path) -> Result<Self, ProjectError> {
        Self::open_mode(root, true)
    }

    pub(crate) fn open_readonly(root: &Path) -> Result<Self, ProjectError> {
        Self::open_mode(root, false)
    }

    fn open_mode(root: &Path, writable: bool) -> Result<Self, ProjectError> {
        let root = resolve_caller_path(root)?;
        if !project::exists(&root)? {
            return Err(error(
                "VAULT_NOT_MANAGED",
                format!(
                    "{} 不存在，不是已接管的托管根；未修改任何文件。",
                    root.display()
                ),
            ));
        }
        require_dir(&root)?;
        let control = root.join(CONTROL);
        if !project::exists(&control)? {
            return Err(error(
                "VAULT_NOT_MANAGED",
                format!(
                    "{} 不是托管根（缺少 control/）：陌生 Vault 保持只读，未接管、未修改任何文件。",
                    root.display()
                ),
            ));
        }
        require_dir(&control)?;
        let control_names = names(&control)?;
        let control_allowed = name_set(&[OWNER, DATABASE, LOCK]);
        if control_names != control_allowed {
            let extra: Vec<&String> = control_names.difference(&control_allowed).collect();
            return Err(integrity(format!(
                "control/ 只允许 owner.json、vault.sqlite、update.lock；未知文件、目录或数据库 sidecar（{extra:?}）保留原样并拒绝继续。"
            )));
        }

        let owner_path = control.join(OWNER);
        project::checked_file(&owner_path)?;
        let owner_bytes = read_bounded(&owner_path, MAX_BYTES)?;
        let owner: Owner = strict(&owner_bytes)?;
        if owner.policy != crate::vault_model::POLICY || owner.producer != PRODUCER {
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

        let root_names = names(&root)?;
        let root_allowed = name_set(&[CONTROL, BASELINE, VAULT, RELEASES]);
        let unknown: Vec<&String> = root_names.difference(&root_allowed).collect();
        if !unknown.is_empty() {
            return Err(integrity(format!(
                "托管根只允许 control/、baseline/、vault/、releases/；未知条目 {unknown:?} 保留原样并拒绝继续。"
            )));
        }
        if !root_names.contains(VAULT) {
            return Err(integrity("托管根缺少 vault/；保留现场并拒绝继续，不重建。"));
        }

        let root_path = fs::canonicalize(&root).map_err(|error| ProjectError::io(&root, error))?;
        let root_text = root_path.to_string_lossy().into_owned();
        if root_text != owner.root {
            return Err(integrity(format!(
                "托管根现在解析为 {}，与所有权记录 {} 不符；保留现场并拒绝继续。",
                root_path.display(),
                owner.root
            )));
        }
        check_identity(&root_path, &owner.root_identity, "托管根")?;
        check_identity(&control, &owner.control_identity, "control/ 目录")?;
        check_identity(&owner_path, &owner.owner_identity, "所有权记录")?;
        check_identity(&control.join(LOCK), &owner.lock_identity, "更新锁")?;
        check_identity(&root_path.join(VAULT), &owner.vault_identity, "vault/ 目录")?;

        let mut baseline_missing = false;
        if root_names.contains(BASELINE) {
            require_dir(&root_path.join(BASELINE))?;
            check_identity(
                &root_path.join(BASELINE),
                &owner.baseline_identity,
                "baseline/ 目录",
            )?;
        } else {
            baseline_missing = true;
        }

        let database_path = control.join(DATABASE);
        project::checked_file(&database_path)?;
        check_identity(&database_path, &owner.database_identity, "托管账本")?;
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
        let metadata: Metadata = strict(metadata_json.as_bytes())?;
        if metadata.policy != crate::vault_model::POLICY || metadata.producer != PRODUCER {
            return Err(integrity(
                "托管账本的 policy 或 producer 与本契约不符；保留现场并拒绝继续。",
            ));
        }
        if metadata.schema_version != SCHEMA_VERSION {
            return Err(integrity(format!(
                "托管账本的 schema_version={}，要求 {SCHEMA_VERSION}；只读拒绝。",
                metadata.schema_version
            )));
        }
        if metadata.root != owner.root {
            return Err(integrity(
                "托管账本记录的根路径与所有权记录不符；保留现场并拒绝继续。",
            ));
        }
        if metadata.author_manifest_sha256 != owner.author_manifest_sha256
            || metadata.baseline_manifest_sha256 != owner.baseline_manifest_sha256
        {
            return Err(integrity(
                "所有权记录与托管账本记录的清单指纹不一致；保留现场并拒绝继续。",
            ));
        }
        if metadata.parent_identity != owner.parent_identity {
            return Err(integrity(
                "所有权记录与托管账本记录的父目录身份不一致；保留现场并拒绝继续。",
            ));
        }
        let recomputed = sha256(canonical(&metadata.baseline_files)?.as_bytes());
        if recomputed != metadata.baseline_manifest_sha256 {
            return Err(integrity(
                "托管账本里的基准清单指纹无法自洽；保留现场并拒绝继续。",
            ));
        }
        let author = metadata.author.clone();
        if author.policy != crate::vault_model::POLICY {
            return Err(integrity("托管账本里的作者清单 policy 不符；只读拒绝。"));
        }
        let author_bytes = canonical(&author)?;
        if sha256(author_bytes.as_bytes()) != metadata.author_canonical_sha256 {
            return Err(integrity(
                "托管账本里的作者清单与记录的 SHA256 不符；保留现场并拒绝继续。",
            ));
        }

        let mut baseline_damage = None;
        if !baseline_missing {
            if let Err(detail) =
                verify_baseline(&root_path.join(BASELINE), &metadata.baseline_files)
            {
                baseline_damage = Some(detail);
            }
        }
        let layout = verify_releases(&root_path)?;
        let vault_files = verify_vault(&root_path)?;
        Ok(Self {
            root: root_path,
            owner_bytes,
            owner,
            metadata_json,
            metadata,
            author,
            connection,
            layout,
            baseline_missing,
            baseline_damage,
            vault_files,
        })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn owner_sha256(&self) -> String {
        sha256(&self.owner_bytes)
    }

    pub(crate) fn metadata_sha256(&self) -> String {
        sha256(self.metadata_json.as_bytes())
    }

    pub(crate) fn owner_file(&self) -> &Owner {
        &self.owner
    }

    pub(crate) fn incomplete_releases(&self) -> &[String] {
        &self.layout.incomplete_releases
    }

    pub(crate) fn vault_path(&self, relative: &str) -> PathBuf {
        joined(&self.root.join(VAULT), relative)
    }

    pub(crate) fn release_dir(&self, release_id: &str) -> PathBuf {
        self.root.join(RELEASES).join(release_id)
    }

    /// 台账之外的 vault 内容：人类既有文件必须原样保留，也要如实计入报告。
    pub(crate) fn unrelated_files(&self) -> Vec<String> {
        let managed = self.author.managed();
        self.vault_files
            .keys()
            .filter(|path| !managed.contains(*path))
            .cloned()
            .collect()
    }

    /// 写入前的统一前提：基准缺失或损坏时一律只读。
    pub(crate) fn require_baseline(&self) -> Result<(), ProjectError> {
        if self.baseline_missing {
            return Err(error(
                "VAULT_NO_BASELINE",
                format!(
                    "{} 缺少 adopt 冻结的 baseline/：不推定任何基准字节，只读；未执行任何写入。",
                    self.root.display()
                ),
            ));
        }
        if let Some(detail) = &self.baseline_damage {
            return Err(error(
                "VAULT_BASELINE_MISMATCH",
                format!(
                    "baseline/ 与 adopt 冻结清单不符（{detail}）；只读，未执行任何写入。请恢复基准或改用新的托管根。"
                ),
            ));
        }
        Ok(())
    }

    pub(crate) fn current_bytes(&self) -> Result<BTreeMap<String, Vec<u8>>, ProjectError> {
        let mut out = BTreeMap::new();
        for path in self.author.managed() {
            let file = joined(&self.root.join(VAULT), &path);
            if project::exists(&file)? {
                out.insert(path, read_bounded(&file, MAX_BYTES)?);
            }
        }
        Ok(out)
    }

    pub(crate) fn baseline_bytes(&self) -> Result<BTreeMap<String, Vec<u8>>, ProjectError> {
        self.require_baseline()?;
        let mut out = BTreeMap::new();
        for path in self.author.managed() {
            let file = joined(&self.root.join(BASELINE), &path);
            if project::exists(&file)? {
                out.insert(path, read_bounded(&file, MAX_BYTES)?);
            }
        }
        Ok(out)
    }

    fn stamp(&self, path: &Path) -> Result<Option<FileStamp>, ProjectError> {
        project::check_components(path)?;
        if !project::exists(path)? {
            return Ok(None);
        }
        project::checked_file(path)?;
        let before = identity(path)?;
        let mut file = project::read_guard(path)?;
        let metadata = file
            .metadata()
            .map_err(|error| ProjectError::io(path, error))?;
        if metadata.len() > MAX_BYTES {
            return Err(integrity(format!(
                "{} 超过托管单文件上限；保留现样并拒绝继续。",
                path.display()
            )));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        Read::by_ref(&mut file)
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| ProjectError::io(path, error))?;
        if bytes.len() as u64 != metadata.len() {
            return Err(error(
                "VAULT_FILE_CHANGED",
                format!("读取 {} 期间内容长度变化；请重试。", path.display()),
            ));
        }
        drop(file);
        let after = identity(path)?;
        if before != after {
            return Err(error(
                "VAULT_FILE_CHANGED",
                format!("读取 {} 期间文件被替换；请重试。", path.display()),
            ));
        }
        let modified = metadata
            .modified()
            .map_err(|error| ProjectError::io(path, error))?
            .duration_since(UNIX_EPOCH)
            .map_err(|error| ProjectError::io(path, error))?
            .as_nanos()
            .to_string();
        Ok(Some(FileStamp {
            identity: before,
            sha256: sha256(&bytes),
            byte_length: bytes.len() as u64,
            modified,
        }))
    }

    /// 按路径集合采集 `vault/` 现场的持久指纹，顺序稳定。
    pub(crate) fn stamps(&self, paths: &BTreeSet<String>) -> Result<Vec<StampSlot>, ProjectError> {
        let mut out = Vec::with_capacity(paths.len());
        for path in paths {
            out.push(StampSlot {
                path: path.clone(),
                stamp: self.stamp(&self.vault_path(path))?,
            });
        }
        Ok(out)
    }

    /// 取得发布锁；锁文件本身也是身份校验的一部分。
    pub(crate) fn lock(&self) -> Result<File, ProjectError> {
        let path = self.root.join(CONTROL).join(LOCK);
        let mut options = write_options();
        options.read(true);
        let file = options
            .open(&path)
            .map_err(|error| ProjectError::io(&path, error))?;
        if file
            .metadata()
            .map_err(|error| ProjectError::io(&path, error))?
            .len()
            != 0
        {
            return Err(integrity("更新锁必须是空的普通文件；保留现场并拒绝继续。"));
        }
        let start = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(file),
                Err(std::fs::TryLockError::WouldBlock)
                    if start.elapsed() < Duration::from_secs(5) =>
                {
                    thread::sleep(Duration::from_millis(2));
                }
                Err(failure) => {
                    return Err(busy(format!(
                        "更新锁不可取得（{failure}）；未写入任何字节，请关闭另一个发布进程后重试。"
                    )));
                }
            }
        }
    }

    pub(crate) fn plan_record(&self, plan_sha256: &str) -> Result<Option<PlanRow>, ProjectError> {
        let mut statement = self
            .connection
            .prepare("SELECT plan_id,kind,payload_json FROM plans WHERE plan_sha256=?1")
            .map_err(ProjectError::database)?;
        let mut rows = statement
            .query(params![plan_sha256])
            .map_err(ProjectError::database)?;
        match rows.next().map_err(ProjectError::database)? {
            Some(row) => Ok(Some(PlanRow {
                plan_id: row.get(0).map_err(ProjectError::database)?,
                kind: row.get(1).map_err(ProjectError::database)?,
                payload_json: row.get(2).map_err(ProjectError::database)?,
            })),
            None => Ok(None),
        }
    }

    /// 计划正文的 SHA256 就是 plan_id；同一份正文重复登记是幂等的。
    pub(crate) fn insert_plan(
        &self,
        kind: &str,
        payload_json: &str,
        created_at: &str,
    ) -> Result<String, ProjectError> {
        let plan_sha256 = sha256(payload_json.as_bytes());
        let inserted = self.connection.execute(
            "INSERT INTO plans(plan_id,kind,plan_sha256,payload_json,created_at) VALUES(?1,?2,?3,?4,?5)",
            params![plan_sha256, kind, plan_sha256, payload_json, created_at],
        );
        match inserted {
            Ok(_) => Ok(plan_sha256),
            Err(failure) => {
                if let Some(row) = self.plan_record(&plan_sha256)? {
                    if row.kind == kind && row.payload_json == payload_json {
                        return Ok(plan_sha256);
                    }
                }
                Err(ProjectError::database(failure))
            }
        }
    }

    pub(crate) fn release_record(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseRow>, ProjectError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT sequence,release_id,status,entries_json,at FROM releases WHERE release_id=?1",
            )
            .map_err(ProjectError::database)?;
        let mut rows = statement
            .query(params![release_id])
            .map_err(ProjectError::database)?;
        match rows.next().map_err(ProjectError::database)? {
            Some(row) => Ok(Some(ReleaseRow {
                sequence: row.get::<_, i64>(0).map_err(ProjectError::database)? as u64,
                release_id: row.get(1).map_err(ProjectError::database)?,
                status: row.get(2).map_err(ProjectError::database)?,
                entries_json: row.get(3).map_err(ProjectError::database)?,
                at: row.get(4).map_err(ProjectError::database)?,
            })),
            None => Ok(None),
        }
    }

    pub(crate) fn releases(&self) -> Result<Vec<ReleaseRow>, ProjectError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT sequence,release_id,status,entries_json,at FROM releases ORDER BY sequence",
            )
            .map_err(ProjectError::database)?;
        let rows = statement
            .query_map([], |row| {
                Ok(ReleaseRow {
                    sequence: row.get::<_, i64>(0)? as u64,
                    release_id: row.get(1)?,
                    status: row.get(2)?,
                    entries_json: row.get(3)?,
                    at: row.get(4)?,
                })
            })
            .map_err(ProjectError::database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(ProjectError::database)?;
        Ok(rows)
    }

    /// 先写 pending 记录再动 `vault/`：中断后账本仍能说明这次发布停在哪。
    pub(crate) fn begin_release(
        &self,
        release_id: &str,
        approval_sha256: &str,
        entries_json: &str,
        at: &str,
    ) -> Result<u64, ProjectError> {
        let next: i64 = self
            .connection
            .query_row(
                "SELECT coalesce(max(sequence),0)+1 FROM releases",
                [],
                |row| row.get(0),
            )
            .map_err(ProjectError::database)?;
        self.connection
            .execute(
                "INSERT INTO releases(sequence,release_id,approval_sha256,status,entries_json,at) VALUES(?1,?2,?3,'pending',?4,?5)",
                params![next, release_id, approval_sha256, entries_json, at],
            )
            .map_err(ProjectError::database)?;
        Ok(next as u64)
    }

    pub(crate) fn set_release_entries(
        &self,
        release_id: &str,
        entries_json: &str,
    ) -> Result<(), ProjectError> {
        let changed = self
            .connection
            .execute(
                "UPDATE releases SET entries_json=?2 WHERE release_id=?1 AND status='pending'",
                params![release_id, entries_json],
            )
            .map_err(ProjectError::database)?;
        if changed != 1 {
            return Err(integrity(format!(
                "发布账本未能更新 {release_id} 的逐项状态；保留现场并拒绝继续。"
            )));
        }
        Ok(())
    }

    pub(crate) fn finish_release(
        &self,
        release_id: &str,
        status: &str,
    ) -> Result<(), ProjectError> {
        let changed = self
            .connection
            .execute(
                "UPDATE releases SET status=?2 WHERE release_id=?1 AND status='pending'",
                params![release_id, status],
            )
            .map_err(ProjectError::database)?;
        if changed != 1 {
            return Err(integrity(format!(
                "发布账本未能把 {release_id} 标记为 {status}；保留现场并拒绝继续。"
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// adopt
// ---------------------------------------------------------------------------

/// `vault adopt <author.json> <尚不存在的根>`：把作者包里的基准字节冻结成
/// `baseline/`，同时写出初始已发布态 `vault/`，并登记两份清单指纹。
pub fn adopt(author_path: &Path, root: &Path) -> Result<String, ProjectError> {
    let author_path = resolve_caller_path(author_path)?;
    project::checked_file(&author_path)?;
    let author_bytes = read_bounded(&author_path, MAX_BYTES)?;
    let author = Author::parse(&author_bytes).map_err(refuse)?;
    let author_manifest_sha256 = sha256(&author_bytes);
    let package = author_path
        .parent()
        .ok_or_else(|| argument("作者包清单没有父目录；未创建任何文件。"))?
        .to_path_buf();
    let root = resolve_caller_path(root)?;
    if project::exists(&root)? {
        return Err(argument(format!(
            "托管根必须尚不存在：{}；未修改任何文件。",
            root.display()
        )));
    }
    let parent = root
        .parent()
        .ok_or_else(|| argument("托管根没有父目录；未创建任何文件。"))?
        .to_path_buf();
    if !project::exists(&parent)? {
        return Err(argument(format!(
            "托管根的父目录必须已存在：{}；未创建任何文件。",
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
        &package,
        parent_identity,
    ) {
        Ok(summary) => Ok(summary),
        Err(failure) => {
            // 只清理本次创建、且仍然是同一个目录的现场；父目录里既有内容永不删除。
            match identity(&root) {
                Ok(current) if current == created => {
                    let _ = fs::remove_dir_all(&root);
                }
                _ => {}
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
    package: &Path,
    parent_identity: Identity,
) -> Result<String, ProjectError> {
    let root_path = fs::canonicalize(root).map_err(|error| ProjectError::io(root, error))?;
    let author_canonical_sha256 = sha256(canonical(&author)?.as_bytes());
    let created_at = now()?;
    let note_count = author.notes.len();
    let resource_count = author.resources.len();
    for directory in [CONTROL, BASELINE, VAULT] {
        let path = root_path.join(directory);
        fs::create_dir(&path).map_err(|error| ProjectError::io(&path, error))?;
    }
    let lock_path = root_path.join(CONTROL).join(LOCK);
    let lock_file = open_new(&lock_path)?;
    lock_file
        .sync_all()
        .map_err(|error| ProjectError::io(&lock_path, error))?;
    drop(lock_file);
    let owner_path = root_path.join(CONTROL).join(OWNER);
    let owner_file = open_new(&owner_path)?;
    drop(owner_file);
    let owner_identity = identity(&owner_path)?;
    let database_path = root_path.join(CONTROL).join(DATABASE);
    let database_file = open_new(&database_path)?;
    drop(database_file);
    let database_identity = identity(&database_path)?;

    let digests = author.baseline_digests();
    let mut files: Vec<BaselineFile> = Vec::new();
    for (managed_path, package_path) in author.baselines() {
        let (want_sha256, want_length) = digests
            .get(&managed_path)
            .cloned()
            .ok_or_else(|| integrity(format!("作者清单缺少 {managed_path} 的基准指纹。")))?;
        let source = joined(package, &package_path);
        project::check_components(&source)?;
        project::checked_file(&source)?;
        let bytes = read_bounded(&source, MAX_BYTES)?;
        if sha256(&bytes) != want_sha256 || bytes.len() as u64 != want_length {
            return Err(error(
                "VAULT_BASELINE_MISMATCH",
                format!(
                    "作者包里的基准 {package_path} 与 author.json 声明不符（声明 {want_length} 字节 / {want_sha256}，实际 {} 字节）；未建立托管根。",
                    bytes.len()
                ),
            ));
        }
        for directory in [BASELINE, VAULT] {
            let target = joined(&root_path.join(directory), &managed_path);
            write_new_file(&target, &bytes)?;
            let read_back = read_bounded(&target, MAX_BYTES)?;
            if read_back != bytes {
                return Err(error(
                    "VAULT_BASELINE_MISMATCH",
                    format!("{directory}/{managed_path} 写入后读回不一致；未建立托管根。"),
                ));
            }
        }
        files.push(BaselineFile {
            path: managed_path,
            sha256: sha256(&bytes),
            byte_length: bytes.len() as u64,
        });
    }
    let baseline_manifest_sha256 = sha256(canonical(&files)?.as_bytes());

    let metadata = Metadata {
        policy: crate::vault_model::POLICY.to_owned(),
        producer: PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        root: root_path.to_string_lossy().into_owned(),
        parent_identity: parent_identity.clone(),
        created_at: created_at.clone(),
        author: author.clone(),
        author_manifest_sha256: author_manifest_sha256.clone(),
        author_canonical_sha256: author_canonical_sha256.clone(),
        baseline_manifest_sha256: baseline_manifest_sha256.clone(),
        baseline_files: files.clone(),
    };
    let metadata_json = canonical(&metadata)?;
    {
        let connection = Connection::open_with_flags(
            &database_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(ProjectError::database)?;
        connection
            .execute_batch(&format!(
                "PRAGMA journal_mode=DELETE;PRAGMA synchronous=FULL;PRAGMA application_id={APP};PRAGMA user_version={SCHEMA_VERSION};BEGIN IMMEDIATE;{SCHEMA}"
            ))
            .map_err(ProjectError::database)?;
        connection
            .execute(
                "INSERT INTO metadata(singleton,schema_version,metadata_json) VALUES(1,?1,?2)",
                params![i64::from(SCHEMA_VERSION), metadata_json],
            )
            .map_err(ProjectError::database)?;
        connection
            .execute_batch("COMMIT")
            .map_err(ProjectError::database)?;
    }

    let owner = Owner {
        policy: crate::vault_model::POLICY.to_owned(),
        producer: PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        root: root_path.to_string_lossy().into_owned(),
        root_identity: created.clone(),
        parent_identity,
        control_identity: identity(&root_path.join(CONTROL))?,
        owner_identity,
        baseline_identity: identity(&root_path.join(BASELINE))?,
        vault_identity: identity(&root_path.join(VAULT))?,
        lock_identity: identity(&lock_path)?,
        database_identity,
        author_manifest_sha256: author_manifest_sha256.clone(),
        baseline_manifest_sha256: baseline_manifest_sha256.clone(),
        created_at: created_at.clone(),
    };
    write_replace(&owner_path, canonical(&owner)?.as_bytes())?;

    // 自检：用正式读取路径复核身份、账本结构与冻结基准。
    let store = VaultStore::open(&root_path)?;
    store.require_baseline()?;
    let verified = store.current_bytes()?;
    drop(store);
    if verified.len() != files.len() {
        return Err(integrity(
            "adopt 自检发现 vault/ 的托管文件数与基准不一致；未建立可用托管根。",
        ));
    }
    Ok(format!(
        "已接管托管测试集合：{}\npolicy={}；基准笔记 {note_count} 个、附件 {resource_count} 个；基准清单 SHA256 {baseline_manifest_sha256}\nvault/ 是 baseline/ 的逐字节副本，当前即初始已发布态；未批准覆盖：0\n下一步：准备候选目录（含 operations.json）后执行 vault plan。\n",
        root_path.display(),
        crate::vault_model::POLICY
    ))
}

// ---------------------------------------------------------------------------
// status
// ---------------------------------------------------------------------------

/// `vault status <root>`：纯只读报告。不修库、不重放、不发布、不补基准。
pub fn status(root: &Path) -> Result<String, ProjectError> {
    let store = VaultStore::open_readonly(root)?;
    let author = &store.author;
    let metadata = &store.metadata;
    let mut lines = Vec::new();
    lines.push(format!(
        "托管根 {}：已接管（policy={}，结构版本 {}）",
        store.root.display(),
        metadata.policy,
        metadata.schema_version
    ));
    lines.push(format!(
        "所有权：作者清单 SHA256 {}；基准清单 SHA256 {}；adopt 于 {}",
        metadata.author_manifest_sha256, metadata.baseline_manifest_sha256, metadata.created_at
    ));
    let baseline_count = metadata.baseline_files.len();
    match (&store.baseline_missing, &store.baseline_damage) {
        (true, _) => {
            lines.push("基准：缺失（只读；不推定基准字节，任何写入都会被拒绝）".to_owned())
        }
        (false, Some(detail)) => lines.push(format!(
            "基准：{baseline_count} 个文件，但与 adopt 冻结清单不符（{detail}）；只读，写入被拒绝"
        )),
        (false, None) => lines.push(format!(
            "基准：{baseline_count} 个文件，逐项字节与 adopt 冻结清单一致"
        )),
    }
    lines.push(format!(
        "托管集合：笔记 {} 个、资源 {} 个、声明链接 {} 条；允许操作 {}",
        author.notes.len(),
        author.resources.len(),
        author.links.len(),
        author.operations.join("/")
    ));

    let current = store.current_bytes()?;
    let mut differences = Vec::new();
    for file in &metadata.baseline_files {
        match current.get(&file.path) {
            None => differences.push(format!("{} 缺失", file.path)),
            Some(bytes) if sha256(bytes) != file.sha256 => {
                differences.push(format!("{} 已改写", file.path))
            }
            Some(_) => {}
        }
    }
    if differences.is_empty() {
        lines.push("当前差异：0 项（vault 与基准一致，即已发布态）".to_owned());
    } else {
        lines.push(format!(
            "当前差异：{} 项（{}）",
            differences.len(),
            differences.join("；")
        ));
    }

    let mut intact = 0usize;
    let mut broken = Vec::new();
    for link in &author.links {
        let kept = current
            .get(&link.from)
            .is_some_and(|bytes| String::from_utf8_lossy(bytes).contains(link.raw.as_str()))
            && current.contains_key(&link.resolves_to);
        if kept {
            intact += 1;
        } else {
            broken.push(format!("{}→{}", link.raw, link.resolves_to));
        }
    }
    if broken.is_empty() {
        lines.push(format!("链接：{intact} 条声明的相对链接全部可解析"));
    } else {
        lines.push(format!(
            "链接：{intact} 条可解析，{} 条不可解析（{}）",
            broken.len(),
            broken.join("；")
        ));
    }

    let unrelated = store.unrelated_files();
    if unrelated.is_empty() {
        lines.push("无关内容：0 个（vault 只含托管文件）".to_owned());
    } else {
        let sample: Vec<&str> = unrelated.iter().take(3).map(String::as_str).collect();
        lines.push(format!(
            "无关内容：{} 个（{} 等）保持原样，不参与计划与发布",
            unrelated.len(),
            sample.join("、")
        ));
    }
    lines.push("未批准覆盖：0".to_owned());

    let releases = store.releases()?;
    let published: Vec<&ReleaseRow> = releases
        .iter()
        .filter(|row| row.status == "published")
        .collect();
    let pending: Vec<&ReleaseRow> = releases
        .iter()
        .filter(|row| row.status != "published")
        .collect();
    match published.last() {
        Some(row) => lines.push(format!(
            "发布：已发布 {} 个版本；最近 #{} release_id={} status=published（逐项条目 {} 条，{}）",
            published.len(),
            row.sequence,
            row.release_id,
            count_entries(&row.entries_json),
            row.at
        )),
        None => lines.push("发布：还没有已发布版本".to_owned()),
    }

    let mut recovery = Vec::new();
    for row in &pending {
        recovery.push(format!(
            "release_id={} status={}（{} 起）",
            row.release_id, row.status, row.at
        ));
    }
    for id in store.incomplete_releases() {
        recovery.push(format!("releases/{id}/ 只有暂存目录、没有发布记录"));
    }
    if recovery.is_empty() {
        lines.push("待恢复：否；status 只读，未写入任何字节".to_owned());
    } else {
        lines.push(format!(
            "待恢复：是；needs_recovery。{}",
            recovery.join("；")
        ));
        lines.push(
            "说明：重跑对应批准文件的 vault publish 可续完未确认条目；已确认条目不会再写一次"
                .to_owned(),
        );
    }
    Ok(format!("{}\n", lines.join("\n")))
}

fn count_entries(entries_json: &str) -> usize {
    serde_json::from_str::<Vec<serde_json::Value>>(entries_json).map_or(0, |entries| entries.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        fs::canonicalize(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/managed-vault-update-v1"),
        )
        .expect("fixture")
    }

    fn scratch(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("noetome-vault-{tag}-{}", uuid::Uuid::new_v4()))
    }

    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), &target).unwrap();
            }
        }
    }

    /// 记录一棵树的全部文件字节，用来证明「拒绝时一个字节都没变」。
    fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        if !root.exists() {
            return out;
        }
        let tree = tree(root).unwrap();
        for (path, file) in tree.files {
            out.insert(path, fs::read(file).unwrap());
        }
        out
    }

    fn fresh(tag: &str) -> (PathBuf, PathBuf) {
        let base = scratch(tag);
        fs::create_dir_all(&base).unwrap();
        let root = base.join("root");
        (base, root)
    }

    /// 调用方（集成测试）常用 `CARGO_MANIFEST_DIR + ../../fixtures/...` 这种带
    /// `..` 的路径；它必须能用，但符号链接仍然必须被拒绝，且 `..` 只做词法折叠。
    #[test]
    fn a_dot_dot_caller_path_is_normalized_while_links_stay_refused() {
        let base = scratch("caller-path");
        fs::create_dir_all(&base).unwrap();
        let package = fixture();
        // PathBuf::push normalizes `..` immediately for Windows verbatim paths.
        // Construct the caller's raw spelling before asking our resolver to fold it.
        let mut dotted = package.as_os_str().to_os_string();
        for component in [".", "基线占位", "..", "author.json"] {
            dotted.push(std::path::MAIN_SEPARATOR.to_string());
            dotted.push(component);
        }
        let dotted = PathBuf::from(dotted);
        assert!(dotted.to_string_lossy().contains(".."), "用例必须真的带 ..");
        let root = base.join("根");
        let summary = adopt(&dotted, &root).expect("带 .. 的调用方路径应当可用");
        assert!(summary.contains("未批准覆盖：0"), "{summary}");
        assert!(root.join("control").join("owner.json").is_file());

        // `..` 只做词法折叠：落在它折叠之后的位置，不会真的爬出父目录。
        fs::create_dir_all(base.join("子")).unwrap();
        let folded = base.join("子").join("..").join("第二个根");
        let author = package.join("author.json");
        adopt(&author, &folded).expect("词法折叠后的根应当可创建");
        assert!(base.join("第二个根").is_dir());
        assert!(!base.join("子").join("第二个根").exists());

        #[cfg(unix)]
        {
            let link = base.join("链");
            std::os::unix::fs::symlink(&root, &link).unwrap();
            let failure = status(&link).expect_err("符号链接必须被拒绝");
            assert_eq!(failure.code(), "PROJECT_UNSAFE_PATH", "{failure}");
        }
    }

    /// 词法折叠只允许弹回调用方自己给出的层级为止：相对路径的锚点是它的起点
    /// 目录，绝对路径的锚点是文件系统根。弹过锚点就必须拒绝，不能被悄悄折成
    /// 调用方目录之外的位置。
    #[test]
    fn a_caller_path_that_climbs_above_its_own_anchor_is_refused() {
        for hostile in ["a/../../b", "a/b/../../../c", "./../out", "..", "../兄弟"] {
            let failure = resolve_caller_path(Path::new(hostile))
                .expect_err(&format!("{hostile} 弹过了调用方锚点，必须被拒绝"));
            assert_eq!(
                failure.code(),
                "PROJECT_UNSAFE_PATH",
                "{hostile}: {failure}"
            );
        }
        // 绝对路径的锚点是根：在根之上再弹同样拒绝，不静默夹取。
        let failure = resolve_caller_path(Path::new("/../../etc")).expect_err("根之上必须拒绝");
        assert_eq!(failure.code(), "PROJECT_UNSAFE_PATH", "{failure}");

        // 只用掉自己给出的层级则仍然可用，且落点就在起点目录内部。
        let here = std::env::current_dir().unwrap();
        assert_eq!(
            resolve_caller_path(Path::new("子/../第二个根")).unwrap(),
            here.join("第二个根")
        );
        assert_eq!(resolve_caller_path(Path::new("笔记/..")).unwrap(), here);

        // 绝对路径只折叠它自己的层级：弹出的那层必须是它真的走过的目录。
        let base = scratch("caller-anchor");
        fs::create_dir_all(base.join("a")).unwrap();
        fs::create_dir_all(base.join("b")).unwrap();
        assert_eq!(
            fs::canonicalize(resolve_caller_path(&base.join("a").join("..").join("b")).unwrap())
                .unwrap(),
            fs::canonicalize(&base).unwrap().join("b")
        );
    }

    #[test]
    fn adopt_freezes_baseline_and_the_initial_vault_is_a_byte_copy() {
        let (base, root) = fresh("adopt-ok");
        let summary = adopt(&fixture().join("author.json"), &root).unwrap();
        assert!(summary.contains("已接管"), "{summary}");
        assert!(summary.contains("未批准覆盖：0"), "{summary}");
        for name in [
            "control/owner.json",
            "control/vault.sqlite",
            "control/update.lock",
        ] {
            assert!(root.join(name).is_file(), "{name}");
        }
        assert!(!root.join("releases").exists(), "adopt 不得预设发布目录");
        for path in ["笔记/力学.md", "笔记/光学.md", "附件/光谱.png"] {
            let frozen = fs::read(fixture().join("baseline").join(path)).unwrap();
            assert_eq!(
                fs::read(root.join("baseline").join(path)).unwrap(),
                frozen,
                "baseline {path}"
            );
            assert_eq!(
                fs::read(root.join("vault").join(path)).unwrap(),
                frozen,
                "vault {path} 必须是基准的逐字节副本"
            );
        }
        let owner: Owner = strict(&fs::read(root.join("control/owner.json")).unwrap()).unwrap();
        assert_eq!(
            owner.author_manifest_sha256,
            sha256(&fs::read(fixture().join("author.json")).unwrap())
        );
        assert_eq!(
            identity(&root).unwrap(),
            owner.root_identity,
            "根的身份必须能被复核"
        );
        assert_eq!(identity(&root.join("vault")).unwrap(), owner.vault_identity);
        let text = status(&root).unwrap();
        assert!(text.contains("未批准覆盖：0"), "{text}");
        assert!(text.contains("当前差异：0 项"), "{text}");
        assert!(text.contains("待恢复：否"), "{text}");
        assert!(!text.contains("needs_recovery"), "{text}");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn adopt_refuses_an_existing_root_or_a_missing_parent() {
        let (base, root) = fresh("adopt-refuse");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("人类内容.md"), b"keep\n").unwrap();
        let failure = adopt(&fixture().join("author.json"), &root).unwrap_err();
        assert_eq!(failure.code(), "VAULT_ARGUMENT");
        assert_eq!(failure.exit_code(), 2);
        assert_eq!(snapshot(&root).len(), 1, "已存在的根不得被写入");
        let missing = base.join("absent").join("root");
        let failure = adopt(&fixture().join("author.json"), &missing).unwrap_err();
        assert_eq!(failure.code(), "VAULT_ARGUMENT");
        assert!(
            !base.join("absent").exists(),
            "父目录缺失时不得创建任何目录"
        );
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn adopt_leaves_no_root_when_the_author_baseline_disagrees() {
        let base = scratch("adopt-mismatch");
        fs::create_dir_all(&base).unwrap();
        let package = base.join("package");
        copy_tree(&fixture(), &package);
        let tampered = package.join("baseline/笔记/力学.md");
        let mut bytes = fs::read(&tampered).unwrap();
        bytes.push(b'x');
        fs::write(&tampered, bytes).unwrap();
        let root = base.join("root");
        let failure = adopt(&package.join("author.json"), &root).unwrap_err();
        assert_eq!(failure.code(), "VAULT_BASELINE_MISMATCH");
        assert!(!root.exists(), "清单不符时不得留下托管根");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_stranger_vault_is_refused_and_keeps_every_byte() {
        let base = scratch("stranger");
        fs::create_dir_all(base.join("笔记")).unwrap();
        fs::create_dir_all(base.join(".obsidian")).unwrap();
        fs::write(base.join("笔记/陌生.md"), b"human bytes\n").unwrap();
        fs::write(base.join(".obsidian/app.json"), b"{\"a\":1}\n").unwrap();
        let before = snapshot(&base);
        let failure = status(&base).unwrap_err();
        assert_eq!(failure.code(), "VAULT_NOT_MANAGED");
        assert_eq!(snapshot(&base), before, "陌生 Vault 必须一个字节都不变");
        let failure = status(&base.join("不存在")).unwrap_err();
        assert_eq!(failure.code(), "VAULT_NOT_MANAGED");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn status_writes_no_bytes_at_all() {
        let (base, root) = fresh("status-readonly");
        adopt(&fixture().join("author.json"), &root).unwrap();
        let before = snapshot(&root);
        let text = status(&root).unwrap();
        assert!(text.contains("未批准覆盖：0"), "{text}");
        assert!(text.contains("已接管"), "{text}");
        assert_eq!(snapshot(&root), before, "status 不得写入任何字节");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_root_without_baseline_stays_read_only() {
        let (base, root) = fresh("no-baseline");
        adopt(&fixture().join("author.json"), &root).unwrap();
        fs::remove_dir_all(root.join("baseline")).unwrap();
        let text = status(&root).unwrap();
        assert!(text.contains("基准：缺失"), "{text}");
        let store = VaultStore::open(&root).unwrap();
        let failure = store.require_baseline().unwrap_err();
        assert_eq!(failure.code(), "VAULT_NO_BASELINE");
        // 只读报告仍然能读出当前磁盘的三个托管文件，但绝不据此推定基准。
        assert_eq!(store.current_bytes().unwrap().len(), 3);
        assert_eq!(
            store.baseline_bytes().unwrap_err().code(),
            "VAULT_NO_BASELINE"
        );
        drop(store); // Release SQLite before removing its files on Windows.
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn unknown_entries_sidecars_and_empty_dirs_are_refused_without_changes() {
        let (base, root) = fresh("layout-refuse");
        adopt(&fixture().join("author.json"), &root).unwrap();
        for (name, create) in [
            ("control sidecar", 0),
            ("unknown root file", 1),
            ("unknown empty dir", 2),
        ] {
            let path = match create {
                0 => root.join("control/vault.sqlite-journal"),
                1 => root.join("extra.txt"),
                _ => root.join("空目录"),
            };
            if create == 2 {
                fs::create_dir(&path).unwrap();
            } else {
                fs::write(&path, b"x").unwrap();
            }
            let before = snapshot(&root);
            let failure = status(&root).unwrap_err();
            assert_eq!(failure.code(), "VAULT_INTEGRITY", "{name}");
            assert_eq!(snapshot(&root), before, "{name} 必须原样保留");
            if create == 2 {
                fs::remove_dir(&path).unwrap();
            } else {
                fs::remove_file(&path).unwrap();
            }
        }
        status(&root).unwrap();
        fs::remove_dir_all(&base).unwrap();
    }
}
