//! Minimal empty-project persistence. No sources, runs, or knowledge are created.
use std::fmt;
use std::fs::{self, Metadata, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;
pub const APPLICATION_ID: u32 = 0x4e4f4554;
const CONFIG: &str = "project.json";
const DATABASE: &str = "project.sqlite";
const MARKER: &str = ".noetome-initializing";
const CONFIG_STAGE: &str = ".project.json.initializing";
const DATABASE_STAGE: &str = ".project.sqlite.initializing";
const SCHEMA: &str = "
CREATE TABLE project_metadata (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    project_id TEXT NOT NULL UNIQUE,
    schema_version INTEGER NOT NULL CHECK (schema_version = 1),
    created_at TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state = 'initialized')
);
PRAGMA user_version = 1;
PRAGMA application_id = 1313817940;
";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfo {
    pub project_id: String,
    pub schema_version: u32,
    pub created_at: String,
    pub state: String,
    pub database: String,
}

#[derive(Debug)]
pub struct ProjectError {
    code: &'static str,
    exit_code: u8,
    message: String,
}

impl ProjectError {
    pub fn code(&self) -> &'static str {
        self.code
    }
    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }
    fn new(code: &'static str, exit_code: u8, message: impl Into<String>) -> Self {
        Self {
            code,
            exit_code,
            message: message.into(),
        }
    }
    fn io(path: &Path, error: impl fmt::Display) -> Self {
        Self::new(
            "PROJECT_IO_ERROR",
            3,
            format!("无法访问项目路径 {}：{error}", path.display()),
        )
    }
    fn incomplete(path: &Path) -> Self {
        Self::new(
            "PROJECT_INCOMPLETE",
            3,
            format!(
                "项目初始化未完成：{}。现有文件保留原样；请检查该目录，并选择一个新的目录重新初始化。",
                path.display()
            ),
        )
    }
    fn unsafe_path(path: &Path) -> Self {
        Self::new(
            "PROJECT_UNSAFE_PATH",
            2,
            format!(
                "项目路径必须是普通目录和文件，不允许符号链接、junction、路径跳转或不安全文件名：{}",
                path.display()
            ),
        )
    }
    fn invalid_database(error: impl fmt::Display) -> Self {
        Self::new(
            "PROJECT_INVALID_DATABASE",
            2,
            format!("项目数据库无效：{error}；只读检查没有修复或迁移数据库。"),
        )
    }
    fn database(error: rusqlite::Error) -> Self {
        use rusqlite::ErrorCode;
        if let rusqlite::Error::SqliteFailure(ref failure, _) = error {
            if matches!(
                failure.code,
                ErrorCode::DatabaseBusy
                    | ErrorCode::DatabaseLocked
                    | ErrorCode::FileLockingProtocolFailed
            ) {
                return Self::new(
                    "PROJECT_DATABASE_BUSY",
                    3,
                    format!("数据库被占用或事务尚未完成：{error}；请关闭写入程序后重试。"),
                );
            }
            if matches!(
                failure.code,
                ErrorCode::PermissionDenied
                    | ErrorCode::ReadOnly
                    | ErrorCode::SystemIoFailure
                    | ErrorCode::DiskFull
                    | ErrorCode::CannotOpen
                    | ErrorCode::OutOfMemory
                    | ErrorCode::OperationInterrupted
                    | ErrorCode::OperationAborted
                    | ErrorCode::NoLargeFileSupport
            ) {
                return Self::new(
                    "PROJECT_IO_ERROR",
                    3,
                    format!("数据库无法完成读写操作：{error}；没有把部分写入记为成功。"),
                );
            }
            if matches!(
                failure.code,
                ErrorCode::InternalMalfunction | ErrorCode::ApiMisuse
            ) {
                return Self::new(
                    "PROJECT_DATABASE_ERROR",
                    1,
                    format!("数据库内部操作失败：{error}。"),
                );
            }
        }
        Self::invalid_database(error)
    }
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}：{}", self.code, self.message)
    }
}
impl std::error::Error for ProjectError {}

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

fn check_components(path: &Path) -> Result<PathBuf, ProjectError> {
    if path.as_os_str().is_empty()
        || path.to_str().is_none()
        || path.components().any(|c| matches!(c, Component::ParentDir))
    {
        return Err(ProjectError::unsafe_path(path));
    }
    #[cfg(windows)]
    {
        use std::path::Prefix;
        if path.has_root() != path.is_absolute() || (path.components().any(|c| matches!(c, Component::Prefix(_))) && !path.is_absolute())
            || path.components().any(|component| matches!(component, Component::Prefix(prefix)
                if !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_) | Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _)))) {
            return Err(ProjectError::unsafe_path(path));
        }
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| ProjectError::io(path, e))?
            .join(path)
    };
    let mut cursor = PathBuf::new();
    for component in absolute.components() {
        cursor.push(component.as_os_str());
        // A drive prefix alone (especially \\?\D:) is not an addressable
        // directory. Check its allowed kind above, then inspect only after the
        // following RootDir component has produced the complete volume root.
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        if let Component::Normal(name) = component {
            let name = name
                .to_str()
                .ok_or_else(|| ProjectError::unsafe_path(path))?;
            #[cfg(windows)]
            {
                let base = name
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(' ')
                    .to_ascii_uppercase();
                if name.ends_with(['.', ' '])
                    || name.chars().any(|ch| {
                        ch <= '\u{1f}' || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*')
                    })
                    || ["CON", "PRN", "AUX", "NUL", "CLOCK$"].contains(&base.as_str())
                    || base
                        .strip_prefix("COM")
                        .or_else(|| base.strip_prefix("LPT"))
                        .map(|suffix| {
                            matches!(
                                suffix,
                                "1" | "2"
                                    | "3"
                                    | "4"
                                    | "5"
                                    | "6"
                                    | "7"
                                    | "8"
                                    | "9"
                                    | "¹"
                                    | "²"
                                    | "³"
                            )
                        })
                        .unwrap_or(false)
                {
                    return Err(ProjectError::unsafe_path(path));
                }
            }
            let _ = name;
        }
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) if is_link(&metadata) => return Err(ProjectError::unsafe_path(&cursor)),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(ProjectError::io(&cursor, error)),
        }
    }
    Ok(absolute)
}

fn checked_file(path: &Path) -> Result<Metadata, ProjectError> {
    let metadata = fs::symlink_metadata(path).map_err(|e| ProjectError::io(path, e))?;
    if is_link(&metadata) || !metadata.is_file() {
        return Err(ProjectError::unsafe_path(path));
    }
    Ok(metadata)
}

fn read_guard(path: &Path) -> Result<fs::File, ProjectError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Other readers are allowed; writers and deleting/renaming this file
        // are not. immutable SQLite must not observe a concurrent writer.
        options.share_mode(1).custom_flags(0x00200000); // OPEN_REPARSE_POINT
    }
    let file = options.open(path).map_err(|error| {
        #[cfg(windows)]
        if matches!(error.raw_os_error(), Some(32 | 33)) {
            return ProjectError::new(
                "PROJECT_DATABASE_BUSY",
                3,
                format!(
                    "项目文件正在写入或被独占占用：{}；请关闭写入程序后重试。",
                    path.display()
                ),
            );
        }
        ProjectError::io(path, error)
    })?;
    if is_link(&file.metadata().map_err(|e| ProjectError::io(path, e))?) {
        return Err(ProjectError::unsafe_path(path));
    }
    Ok(file)
}

fn exists(path: &Path) -> Result<bool, ProjectError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(ProjectError::io(path, e)),
    }
}

fn is_incomplete(path: &Path) -> Result<bool, ProjectError> {
    for name in [MARKER, CONFIG_STAGE, DATABASE_STAGE] {
        if exists(&path.join(name))? {
            return Ok(true);
        }
    }
    Ok(exists(&path.join(CONFIG))? != exists(&path.join(DATABASE))?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitStage {
    Marker,
    Database,
    Config,
    Published,
}

struct OwnedFile {
    path: PathBuf,
    bytes: Vec<u8>,
    identity: same_file::Handle,
}
struct InitGuard {
    root: PathBuf,
    identity: same_file::Handle,
    files: Vec<OwnedFile>,
}
impl InitGuard {
    fn new(root: PathBuf) -> Result<Self, ProjectError> {
        let identity =
            same_file::Handle::from_path(&root).map_err(|e| ProjectError::io(&root, e))?;
        Ok(Self {
            root,
            identity,
            files: Vec::new(),
        })
    }
    fn ensure_root(&self) -> Result<(), ProjectError> {
        check_components(&self.root)?;
        let current = same_file::Handle::from_path(&self.root)
            .map_err(|e| ProjectError::io(&self.root, e))?;
        if current != self.identity {
            return Err(ProjectError::incomplete(&self.root));
        }
        Ok(())
    }
    fn record(&mut self, path: &Path) -> Result<(), ProjectError> {
        self.ensure_root()?;
        checked_file(path)?;
        let current = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
        let bytes = fs::read(path).map_err(|e| ProjectError::io(path, e))?;
        if let Some(old) = self.files.iter_mut().find(|file| file.path == path) {
            if current != old.identity {
                return Err(ProjectError::incomplete(&self.root));
            }
            old.bytes = bytes;
        } else {
            self.files.push(OwnedFile {
                path: path.to_path_buf(),
                bytes,
                identity: current,
            });
        }
        Ok(())
    }
    fn create(&mut self, path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
        self.ensure_root()?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1);
        }
        let mut file = options.open(path).map_err(|e| ProjectError::io(path, e))?;
        let original =
            same_file::Handle::from_file(file.try_clone().map_err(|e| ProjectError::io(path, e))?)
                .map_err(|e| ProjectError::io(path, e))?;
        // Record even a partial write as ours; unknown extra files or later edits
        // make rollback preserve the entire directory instead of deleting it.
        let result = file.write_all(bytes).and_then(|_| file.sync_all());
        drop(file);
        let current = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
        if current != original {
            return Err(ProjectError::incomplete(&self.root));
        }
        self.record(path)?;
        result.map_err(|e| ProjectError::io(path, e))
    }
    fn publish(&mut self, stage: &Path, target: &Path) -> Result<(), ProjectError> {
        self.ensure_root()?;
        let expected = self
            .files
            .iter()
            .find(|file| file.path == stage)
            .ok_or_else(|| ProjectError::incomplete(&self.root))?;
        if !self.file_matches(expected) {
            return Err(ProjectError::incomplete(&self.root));
        }
        // A same-volume hard link fails if the target exists on every platform.
        // Unlike rename it never replaces an existing user file or directory.
        fs::hard_link(stage, target).map_err(|e| ProjectError::io(target, e))?;
        self.record(target)?;
        fs::remove_file(stage).map_err(|e| ProjectError::io(stage, e))?;
        self.files.retain(|file| file.path != stage);
        Ok(())
    }
    fn file_matches(&self, file: &OwnedFile) -> bool {
        checked_file(&file.path).is_ok()
            && same_file::Handle::from_path(&file.path)
                .map(|current| current == file.identity)
                .unwrap_or(false)
            && fs::read(&file.path).ok().as_ref() == Some(&file.bytes)
    }
    fn unchanged(&self) -> bool {
        if self.ensure_root().is_err() {
            return false;
        }
        let Ok(metadata) = fs::symlink_metadata(&self.root) else {
            return false;
        };
        if is_link(&metadata) || !metadata.is_dir() {
            return false;
        }
        let Ok(entries) = fs::read_dir(&self.root) else {
            return false;
        };
        let mut count = 0;
        for entry in entries {
            let Ok(entry) = entry else {
                return false;
            };
            let path = entry.path();
            let Some(file) = self.files.iter().find(|file| file.path == path) else {
                return false;
            };
            if !self.file_matches(file) {
                return false;
            }
            count += 1;
        }
        count == self.files.len()
    }
    fn rollback(&self) -> bool {
        if !self.unchanged() {
            return false;
        }
        // Keep the marker until last. Never recursively remove a directory.
        for file in self.files.iter().rev() {
            if fs::remove_file(&file.path).is_err() {
                return false;
            }
        }
        fs::remove_dir(&self.root).is_ok()
    }
}

pub fn initialize(path: &Path) -> Result<ProjectInfo, ProjectError> {
    initialize_with(path, |_, _| Ok(()))
}

fn initialize_with(
    path: &Path,
    mut stage_hook: impl FnMut(InitStage, &Path) -> io::Result<()>,
) -> Result<ProjectInfo, ProjectError> {
    let absolute = check_components(path)?;
    if exists(&absolute)? {
        let metadata = fs::symlink_metadata(&absolute).map_err(|e| ProjectError::io(path, e))?;
        if metadata.is_dir() && is_incomplete(&absolute)? {
            return Err(ProjectError::incomplete(path));
        }
        return Err(ProjectError::new(
            "PROJECT_PATH_EXISTS",
            3,
            format!(
                "目标已存在，不会覆盖现有内容：{}；请选择尚不存在的新目录。",
                path.display()
            ),
        ));
    }
    let parent = absolute
        .parent()
        .ok_or_else(|| ProjectError::unsafe_path(path))?;
    if !fs::metadata(parent)
        .map_err(|e| ProjectError::io(parent, e))?
        .is_dir()
    {
        return Err(ProjectError::io(parent, "父路径不是目录"));
    }
    let root = parent
        .canonicalize()
        .map_err(|e| ProjectError::io(parent, e))?
        .join(
            absolute
                .file_name()
                .ok_or_else(|| ProjectError::unsafe_path(path))?,
        );
    let info = ProjectInfo {
        project_id: Uuid::new_v4().to_string(),
        schema_version: SCHEMA_VERSION,
        created_at: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|e| ProjectError::io(path, e))?,
        state: "initialized".to_owned(),
        database: DATABASE.to_owned(),
    };
    fs::create_dir(&root).map_err(|e| {
        if e.kind() == io::ErrorKind::AlreadyExists {
            ProjectError::new("PROJECT_PATH_EXISTS", 3, "目标在初始化前已出现；没有写入。")
        } else {
            ProjectError::io(&root, e)
        }
    })?;
    let mut guard = match InitGuard::new(root.clone()) {
        Ok(guard) => guard,
        Err(error) => {
            // Ownership could not be established; do not guess that the current
            // path still identifies the directory we created.
            return Err(error);
        }
    };
    let result = (|| {
        guard.create(
            &root.join(MARKER),
            format!("{}\n", info.project_id).as_bytes(),
        )?;
        stage_hook(InitStage::Marker, &root).map_err(|e| ProjectError::io(&root, e))?;
        let database_stage = root.join(DATABASE_STAGE);
        guard.create(&database_stage, b"")?;
        let mut connection =
            Connection::open_with_flags(&database_stage, OpenFlags::SQLITE_OPEN_READ_WRITE)
                .map_err(ProjectError::database)?;
        connection
            .execute_batch("PRAGMA journal_mode = DELETE; PRAGMA synchronous = FULL;")
            .map_err(ProjectError::database)?;
        let transaction = connection.transaction().map_err(ProjectError::database)?;
        transaction
            .execute_batch(SCHEMA)
            .map_err(ProjectError::database)?;
        transaction
            .execute(
                "INSERT INTO project_metadata VALUES (1, ?1, ?2, ?3, ?4)",
                params![
                    info.project_id,
                    info.schema_version,
                    info.created_at,
                    info.state
                ],
            )
            .map_err(ProjectError::database)?;
        transaction.commit().map_err(ProjectError::database)?;
        connection
            .close()
            .map_err(|(_, e)| ProjectError::database(e))?;
        OpenOptions::new()
            .write(true)
            .open(&database_stage)
            .and_then(|file| file.sync_all())
            .map_err(|e| ProjectError::io(&database_stage, e))?;
        guard.record(&database_stage)?;
        stage_hook(InitStage::Database, &root).map_err(|e| ProjectError::io(&root, e))?;
        let mut config_bytes =
            serde_json::to_vec_pretty(&info).map_err(|e| ProjectError::io(&root, e))?;
        config_bytes.push(b'\n');
        guard.create(&root.join(CONFIG_STAGE), &config_bytes)?;
        stage_hook(InitStage::Config, &root).map_err(|e| ProjectError::io(&root, e))?;
        guard.publish(&database_stage, &root.join(DATABASE))?;
        guard.publish(&root.join(CONFIG_STAGE), &root.join(CONFIG))?;
        stage_hook(InitStage::Published, &root).map_err(|e| ProjectError::io(&root, e))?;
        if !guard.unchanged() {
            return Err(ProjectError::incomplete(&root));
        }
        fs::remove_file(root.join(MARKER)).map_err(|e| ProjectError::io(&root, e))?;
        Ok(info.clone())
    })();
    match result {
        Ok(info) => Ok(info),
        Err(mut error) => {
            if !guard.rollback() {
                error.message.push_str(" 初始化未完成，现场含未知内容、改动或无法清理的文件，已保留；请选择新目录重试。");
            }
            Err(error)
        }
    }
}

fn reject_sidecars(root: &Path) -> Result<(), ProjectError> {
    for entry in fs::read_dir(root).map_err(|e| ProjectError::io(root, e))? {
        let entry = entry.map_err(|e| ProjectError::io(root, e))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        #[cfg(windows)]
        let name = name.trim_end_matches(['.', ' ']).to_ascii_lowercase();
        if [
            "project.sqlite-wal",
            "project.sqlite-shm",
            "project.sqlite-journal",
        ]
        .contains(&name.as_ref())
            || name.starts_with("project.sqlite-mj")
        {
            return Err(ProjectError::new(
                "PROJECT_DATABASE_BUSY",
                3,
                "数据库存在事务或待恢复文件；请先关闭项目写入程序并处理其事务，再只读检查。不会自动恢复或移除这些文件。",
            ));
        }
    }
    Ok(())
}

fn validate_info(info: &ProjectInfo) -> Result<(), ProjectError> {
    if info.schema_version != SCHEMA_VERSION {
        return Err(ProjectError::new(
            "PROJECT_UNSUPPORTED_SCHEMA",
            2,
            format!(
                "不支持项目 Schema {}；当前只支持 {}，没有执行迁移。",
                info.schema_version, SCHEMA_VERSION
            ),
        ));
    }
    if info.database != DATABASE {
        return Err(ProjectError::unsafe_path(Path::new(&info.database)));
    }
    let valid_id = Uuid::parse_str(&info.project_id)
        .map(|id| {
            id.get_version_num() == 4
                && id.get_variant() == uuid::Variant::RFC4122
                && id.to_string() == info.project_id
        })
        .unwrap_or(false);
    let valid_time = OffsetDateTime::parse(&info.created_at, &Rfc3339)
        .map(|t| t.offset() == UtcOffset::UTC)
        .unwrap_or(false);
    if !valid_id || !valid_time || info.state != "initialized" {
        return Err(ProjectError::new(
            "PROJECT_INVALID_CONFIG",
            2,
            "项目配置中的 ID、UTC 时间或状态不符合 Schema 1；没有修改配置。",
        ));
    }
    Ok(())
}

pub fn inspect(path: &Path) -> Result<ProjectInfo, ProjectError> {
    let root = check_components(path)?;
    let metadata = fs::symlink_metadata(&root).map_err(|e| ProjectError::io(&root, e))?;
    if !metadata.is_dir() {
        return Err(ProjectError::unsafe_path(&root));
    }
    if is_incomplete(&root)? {
        return Err(ProjectError::incomplete(&root));
    }
    reject_sidecars(&root)?;
    let config_path = root.join(CONFIG);
    let config_metadata = checked_file(&config_path)?;
    if config_metadata.len() > 65536 {
        return Err(ProjectError::new(
            "PROJECT_INVALID_CONFIG",
            2,
            "项目配置超出 Schema 1 元数据的 64 KiB 上限。",
        ));
    }
    let _config_read_guard = read_guard(&config_path)?;
    let config_bytes = fs::read(&config_path).map_err(|e| ProjectError::io(&config_path, e))?;
    let info: ProjectInfo = serde_json::from_slice(&config_bytes).map_err(|e| {
        ProjectError::new(
            "PROJECT_INVALID_CONFIG",
            2,
            format!("项目配置不是有效的 Schema 1 JSON：{e}；没有修改文件。"),
        )
    })?;
    validate_info(&info)?;
    let database_path = root.join(DATABASE);
    let before = checked_file(&database_path)?;
    let _database_read_guard = read_guard(&database_path)?;
    let canonical = database_path
        .canonicalize()
        .map_err(|e| ProjectError::io(&database_path, e))?;
    let mut uri = url::Url::from_file_path(&canonical)
        .map_err(|_| ProjectError::unsafe_path(&database_path))?;
    uri.query_pairs_mut()
        .append_pair("mode", "ro")
        .append_pair("immutable", "1");
    // SQLite immutable reads neither recover journals nor create WAL/shm files.
    // This command is for quiescent project state, not concurrent-writer snapshots.
    let connection = Connection::open_with_flags(
        uri.as_str(),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(ProjectError::database)?;
    connection
        .execute_batch("PRAGMA temp_store = MEMORY;")
        .map_err(ProjectError::database)?;
    let application_id: u32 = connection
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .map_err(ProjectError::database)?;
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(ProjectError::database)?;
    if version != SCHEMA_VERSION {
        return Err(ProjectError::new(
            "PROJECT_UNSUPPORTED_SCHEMA",
            2,
            format!("数据库 Schema 为 {version}，没有执行迁移。"),
        ));
    }
    if application_id != APPLICATION_ID {
        return Err(ProjectError::invalid_database("数据库应用标识不匹配"));
    }
    let tables = connection
        .prepare(
            "SELECT name, type FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*' ORDER BY name",
        )
        .and_then(|mut statement| {
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(ProjectError::database)?;
    if tables != [("project_metadata".to_owned(), "table".to_owned())] {
        return Err(ProjectError::invalid_database(
            "Schema 1 只允许真实的 project_metadata 基础表，没有其他表、视图或触发器",
        ));
    }
    let columns = connection
        .prepare("PRAGMA table_info(project_metadata)")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i32>(3)?,
                        row.get::<_, i32>(5)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(ProjectError::database)?;
    let expected_columns = [
        ("singleton", "INTEGER", 0, 1),
        ("project_id", "TEXT", 1, 0),
        ("schema_version", "INTEGER", 1, 0),
        ("created_at", "TEXT", 1, 0),
        ("state", "TEXT", 1, 0),
    ]
    .map(|(name, kind, not_null, pk)| (name.to_owned(), kind.to_owned(), not_null, pk));
    if columns != expected_columns {
        return Err(ProjectError::invalid_database(
            "基础表字段与 Schema 1 不一致",
        ));
    }
    let integrity: String = connection
        .pragma_query_value(None, "quick_check", |r| r.get(0))
        .map_err(ProjectError::database)?;
    if integrity != "ok" {
        return Err(ProjectError::invalid_database(integrity));
    }
    let count: i64 = connection
        .query_row("SELECT count(*) FROM project_metadata", [], |r| r.get(0))
        .map_err(ProjectError::database)?;
    if count != 1 {
        return Err(ProjectError::invalid_database("项目元数据必须恰有一条记录"));
    }
    let stored = connection.query_row("SELECT project_id, schema_version, created_at, state FROM project_metadata WHERE singleton = 1", [], |r| {
        Ok(ProjectInfo { project_id: r.get(0)?, schema_version: r.get(1)?, created_at: r.get(2)?, state: r.get(3)?, database: DATABASE.to_owned() })
    }).map_err(ProjectError::database)?;
    if stored != info {
        return Err(ProjectError::invalid_database(
            "配置与数据库中的项目身份、版本、时间或状态不一致",
        ));
    }
    connection
        .close()
        .map_err(|(_, e)| ProjectError::database(e))?;
    reject_sidecars(&root)?;
    let after = checked_file(&database_path)?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || fs::read(&config_path).map_err(|e| ProjectError::io(&config_path, e))? != config_bytes
    {
        return Err(ProjectError::new(
            "PROJECT_DATABASE_BUSY",
            3,
            "检查期间项目内容发生变化，请关闭写入程序后重试。",
        ));
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    struct Sandbox(PathBuf);
    impl Sandbox {
        fn new() -> Self {
            let parent =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/project-storage-tests");
            fs::create_dir_all(&parent).unwrap();
            let root = parent
                .canonicalize()
                .unwrap()
                .join(Uuid::new_v4().to_string());
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            // Exclusively created test root, not supplied by production inputs.
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fs::read_dir(root)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                (path.file_name().unwrap().into(), fs::read(&path).unwrap())
            })
            .collect()
    }

    #[test]
    fn handled_initialization_failures_clean_only_owned_files_and_allow_same_path_retry() {
        let sandbox = Sandbox::new();
        for point in [
            InitStage::Marker,
            InitStage::Database,
            InitStage::Config,
            InitStage::Published,
        ] {
            let target = sandbox.0.join(format!("failure-{point:?}"));
            let error = initialize_with(&target, |stage, _| {
                if stage == point {
                    Err(io::Error::other("test-owned I/O failure"))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
            assert_eq!(error.code(), "PROJECT_IO_ERROR");
            assert!(
                !target.exists(),
                "unchanged owned directory was not cleaned at {point:?}"
            );
            let initialized = initialize(&target).unwrap();
            assert_eq!(inspect(&target).unwrap(), initialized);
        }
    }

    #[test]
    fn rollback_preserves_unknown_content_changed_bytes_deleted_files_and_replaced_files() {
        let sandbox = Sandbox::new();
        for change in ["unknown", "edited", "deleted", "replaced"] {
            let target = sandbox.0.join(change);
            let error = initialize_with(&target, |stage, root| {
                if stage == InitStage::Config {
                    match change {
                        "unknown" => {
                            fs::write(root.join("human-note.md"), b"preserve human content")?
                        }
                        "edited" => fs::write(root.join(MARKER), b"preserve edited marker")?,
                        "deleted" => fs::remove_file(root.join(CONFIG_STAGE))?,
                        "replaced" => {
                            let original = fs::read(root.join(CONFIG_STAGE))?;
                            fs::remove_file(root.join(CONFIG_STAGE))?;
                            fs::write(root.join(CONFIG_STAGE), original)?;
                        }
                        _ => unreachable!(),
                    }
                    return Err(io::Error::other("stop after external test change"));
                }
                Ok(())
            })
            .unwrap_err();
            assert_eq!(error.exit_code(), 3);
            assert!(target.join(MARKER).exists());
            assert!(target.join(DATABASE_STAGE).exists());
            let before = snapshot(&target);
            assert_eq!(inspect(&target).unwrap_err().code(), "PROJECT_INCOMPLETE");
            assert_eq!(
                initialize(&target).unwrap_err().code(),
                "PROJECT_INCOMPLETE"
            );
            assert_eq!(snapshot(&target), before);
            assert!(initialize(&sandbox.0.join(format!("retry-{change}"))).is_ok());
        }
    }

    #[test]
    fn rollback_does_not_remove_a_replacement_empty_directory() {
        let sandbox = Sandbox::new();
        let target = sandbox.0.join("original");
        let moved = sandbox.0.join("moved-original");
        fs::create_dir(&target).unwrap();
        let guard = InitGuard::new(target.clone()).unwrap();
        // The before-marker phase owns no files yet. A same-content replacement
        // must still be preserved; contents alone cannot establish ownership.
        fs::rename(&target, &moved).unwrap();
        fs::create_dir(&target).unwrap();
        assert!(!guard.rollback());
        assert!(target.exists());
        assert!(moved.exists());
        assert!(fs::read_dir(&target).unwrap().next().is_none());
        assert_eq!(
            guard.ensure_root().unwrap_err().code(),
            "PROJECT_INCOMPLETE"
        );
    }

    #[test]
    fn actual_process_termination_preserves_partial_state_and_new_path_retry_succeeds() {
        const CHILD_ROOT: &str = "NOETOME_STORAGE_TEST_CHILD_ROOT";
        const CHILD_READY: &str = "NOETOME_STORAGE_TEST_CHILD_READY";
        const CHILD_STAGE: &str = "NOETOME_STORAGE_TEST_CHILD_STAGE";
        if let Some(target) = std::env::var_os(CHILD_ROOT) {
            let point = std::env::var(CHILD_STAGE).unwrap();
            initialize_with(Path::new(&target), |stage, _| {
                if format!("{stage:?}") == point {
                    let ready = std::env::var_os(CHILD_READY).unwrap();
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(ready)?;
                    file.write_all(b"real SQLite stage reached\n")?;
                    file.sync_all()?;
                    loop {
                        std::thread::park_timeout(Duration::from_secs(1));
                    }
                }
                Ok(())
            })
            .unwrap();
            panic!("child unexpectedly finished without reaching crash point");
        }

        let sandbox = Sandbox::new();
        for point in [InitStage::Database, InitStage::Config, InitStage::Published] {
            let target = sandbox.0.join(format!("crashed-{point:?}"));
            let ready = sandbox.0.join(format!("ready-{point:?}"));
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "project::tests::actual_process_termination_preserves_partial_state_and_new_path_retry_succeeds", "--nocapture"])
                .env(CHILD_ROOT, &target).env(CHILD_READY, &ready).env(CHILD_STAGE, format!("{point:?}"))
                .current_dir(&sandbox.0).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            while !ready.exists() && Instant::now() < deadline {
                if child.try_wait().unwrap().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            if !ready.exists() {
                let _ = child.kill();
                let status = child.wait().unwrap();
                panic!("child did not reach {point:?}; status={status}");
            }
            assert!(
                child.try_wait().unwrap().is_none(),
                "readiness alone is not a live process"
            );
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            let database = if point == InitStage::Published {
                DATABASE
            } else {
                DATABASE_STAGE
            };
            assert!(
                fs::read(target.join(database))
                    .unwrap()
                    .starts_with(b"SQLite format 3\0")
            );
            if point == InitStage::Config {
                assert!(target.join(CONFIG_STAGE).exists());
            }
            let before = snapshot(&target);
            assert_eq!(inspect(&target).unwrap_err().code(), "PROJECT_INCOMPLETE");
            assert_eq!(
                initialize(&target).unwrap_err().code(),
                "PROJECT_INCOMPLETE"
            );
            assert_eq!(snapshot(&target), before);
            let retry = sandbox.0.join(format!("recovered-{point:?}"));
            let info = initialize(&retry).unwrap();
            assert_eq!(inspect(&retry).unwrap(), info);
        }
    }

    #[test]
    fn configuration_requires_a_real_uuid_v4_rfc4122_variant() {
        let info = ProjectInfo {
            project_id: "00000000-0000-4000-0000-000000000000".to_owned(),
            schema_version: 1,
            created_at: "2026-11-01T00:00:00Z".to_owned(),
            state: "initialized".to_owned(),
            database: DATABASE.to_owned(),
        };
        assert_eq!(
            validate_info(&info).unwrap_err().code(),
            "PROJECT_INVALID_CONFIG"
        );
    }
}
