use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::File;
#[cfg(windows)]
use std::fs::OpenOptions;
use std::fs::{self, Metadata, Permissions};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

struct Sandbox {
    root: PathBuf,
    junctions: RefCell<Vec<PathBuf>>,
}

impl Sandbox {
    fn new() -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/project-cli-tests");
        fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = parent.join(format!(
            "project-{}-{nonce}-{}",
            std::process::id(),
            NEXT_CASE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("create exclusive project test sandbox");
        for name in ["工作目录 with spaces", "home", "appdata", "localappdata"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        fs::write(root.join("人工内容.md"), "# 人工内容\n原样保留。\n").unwrap();
        Self {
            root,
            junctions: RefCell::new(Vec::new()),
        }
    }

    fn cwd(&self) -> PathBuf {
        self.root.join("工作目录 with spaces")
    }

    fn run(&self, arguments: &[OsString]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(arguments)
            .current_dir(self.cwd())
            .env("HOME", self.root.join("home"))
            .env("USERPROFILE", self.root.join("home"))
            .env("APPDATA", self.root.join("appdata"))
            .env("LOCALAPPDATA", self.root.join("localappdata"))
            .env("TMP", &self.root)
            .env("TEMP", &self.root)
            .env("TMPDIR", &self.root)
            .output()
            .expect("execute the compiled project CLI")
    }

    fn action(&self, action: &str, path: &Path) -> Output {
        self.run(&[
            OsString::from("project"),
            OsString::from(action),
            path.as_os_str().to_owned(),
        ])
    }

    fn init(&self, path: &Path) -> Value {
        let stdout = assert_success(self.action("init", path));
        let config = read_config(path);
        assert!(stdout.contains(config["project_id"].as_str().unwrap()));
        assert!(stdout.contains("initialized"));
        assert!(stdout.contains("空项目"));
        config
    }

    fn inspect_preserving(&self, path: &Path) -> String {
        let before = snapshot(&self.root);
        let stdout = assert_success(self.action("inspect", path));
        assert_eq!(snapshot(&self.root), before, "inspect modified the sandbox");
        stdout
    }

    fn rejects_preserving(&self, action: &str, path: &Path, exit_code: i32, code: &str) {
        let before = snapshot(&self.root);
        assert_error(self.action(action, path), exit_code, code);
        assert_eq!(
            snapshot(&self.root),
            before,
            "rejected {action} modified files at {}",
            path.display()
        );
    }

    #[cfg(windows)]
    fn junction(&self, link: &Path, target: &Path) {
        // These paths are test-owned and contain no command metacharacters.
        // Strip the canonical extended-path prefix for the cmd builtin only.
        let dos_path = |path: &Path| {
            let rendered = path.to_string_lossy();
            rendered
                .strip_prefix(r"\\?\")
                .unwrap_or(&rendered)
                .to_owned()
        };
        let output = Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(dos_path(link))
            .arg(dos_path(target))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "could not create test-owned junction: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        self.junctions.borrow_mut().push(link.to_owned());
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // Remove the junction itself before recursive cleanup; never follow it.
        for link in self.junctions.get_mut().iter().rev() {
            fs::remove_dir(link).expect("remove test-owned junction");
        }
        // This root was exclusively created under the canonical ignored parent.
        fs::remove_dir_all(&self.root).expect("remove test-owned sandbox");
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Directory(SystemTime),
    File {
        bytes: Vec<u8>,
        modified: SystemTime,
        readonly: bool,
    },
    #[cfg(windows)]
    LockedShm {
        prefix: Vec<u8>,
        lock_bytes: Vec<Option<u8>>,
        suffix: Vec<u8>,
        length: u64,
        modified: SystemTime,
        readonly: bool,
    },
    Link(Option<PathBuf>),
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Entry> {
    snapshot_files(root, false)
}

fn transaction_snapshot(root: &Path) -> BTreeMap<PathBuf, Entry> {
    snapshot_files(root, true)
}

fn snapshot_files(root: &Path, allow_locked_shm: bool) -> BTreeMap<PathBuf, Entry> {
    fn visit(
        root: &Path,
        path: &Path,
        entries: &mut BTreeMap<PathBuf, Entry>,
        allow_locked_shm: bool,
    ) {
        let metadata = fs::symlink_metadata(path).unwrap();
        let relative = path.strip_prefix(root).unwrap().to_owned();
        if is_link(&metadata) {
            entries.insert(relative, Entry::Link(fs::read_link(path).ok()));
        } else if metadata.is_dir() {
            entries.insert(relative, Entry::Directory(metadata.modified().unwrap()));
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), entries, allow_locked_shm);
            }
        } else {
            let bytes = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    #[cfg(windows)]
                    if allow_locked_shm
                        && error.raw_os_error() == Some(33)
                        && path
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .eq_ignore_ascii_case("project.sqlite-shm")
                    {
                        entries.insert(relative, locked_shm_snapshot(path, &metadata));
                        return;
                    }
                    panic!("could not snapshot {}: {error}", path.display());
                }
            };
            entries.insert(
                relative,
                Entry::File {
                    bytes,
                    modified: metadata.modified().unwrap(),
                    readonly: metadata.permissions().readonly(),
                },
            );
        }
    }
    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries, allow_locked_shm);
    entries
}

#[cfg(windows)]
fn locked_shm_snapshot(path: &Path, metadata: &Metadata) -> Entry {
    use std::io::{Read, Seek, SeekFrom};
    // SQLite's Windows WAL index reserves bytes 120..=128 for eight lock slots
    // and its deadman switch (WIN_SHM_BASE/SQLITE_SHM_NLOCK in sqlite3.c).
    // Read every other byte normally and record the readable values or lock
    // state of those reserved bytes. Do not bypass locks or omit the file.
    assert!(metadata.len() >= 129);
    let mut file = File::open(path).unwrap();
    let mut prefix = vec![0; 120];
    file.read_exact(&mut prefix).unwrap();
    let mut lock_bytes = Vec::new();
    for offset in 120..=128 {
        file.seek(SeekFrom::Start(offset)).unwrap();
        let mut byte = [0];
        match file.read_exact(&mut byte) {
            Ok(()) => lock_bytes.push(Some(byte[0])),
            Err(error) if error.raw_os_error() == Some(33) => lock_bytes.push(None),
            Err(error) => panic!("could not read WAL lock byte {offset}: {error}"),
        }
    }
    assert!(lock_bytes.iter().any(Option::is_none));
    file.seek(SeekFrom::Start(129)).unwrap();
    let mut suffix = Vec::new();
    file.read_to_end(&mut suffix).unwrap();
    assert_eq!(
        prefix.len() + lock_bytes.len() + suffix.len(),
        metadata.len() as usize
    );
    Entry::LockedShm {
        prefix,
        lock_bytes,
        suffix,
        length: metadata.len(),
        modified: metadata.modified().unwrap(),
        readonly: metadata.permissions().readonly(),
    }
}

fn assert_success(output: Output) -> String {
    assert_eq!(
        output.status.code(),
        Some(0),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains('\u{1b}'));
    stdout
}

fn assert_error(output: Output, exit_code: i32, code: &str) {
    assert_eq!(
        output.status.code(),
        Some(exit_code),
        "unexpected result: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains(code), "missing {code}: {stderr}");
    assert!(!stderr.contains('\u{1b}'));
}

fn read_config(project: &Path) -> Value {
    serde_json::from_slice(&fs::read(project.join("project.json")).unwrap()).unwrap()
}

fn write_config(project: &Path, config: &Value) {
    fs::write(
        project.join("project.json"),
        serde_json::to_vec_pretty(config).unwrap(),
    )
    .unwrap();
}

fn readonly_connection(project: &Path) -> Connection {
    Connection::open_with_flags(
        project.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
}

struct ReadonlyFiles(Vec<(PathBuf, Permissions)>);

impl ReadonlyFiles {
    fn new(paths: &[PathBuf]) -> Self {
        let mut original = Vec::new();
        for path in paths {
            let permissions = fs::metadata(path).unwrap().permissions();
            let mut readonly = permissions.clone();
            readonly.set_readonly(true);
            fs::set_permissions(path, readonly).unwrap();
            original.push((path.clone(), permissions));
        }
        Self(original)
    }
}

impl Drop for ReadonlyFiles {
    fn drop(&mut self) {
        for (path, permissions) in &self.0 {
            fs::set_permissions(path, permissions.clone()).unwrap();
        }
    }
}

#[test]
fn initialization_persists_matching_identity_schema_and_empty_state() {
    let sandbox = Sandbox::new();
    let project = sandbox.root.join("学习项目 with spaces");
    let before = snapshot(&sandbox.root);
    let config = sandbox.init(&project);
    let after = snapshot(&sandbox.root);
    for (path, entry) in before {
        // Adding the project changes the sandbox directory's mtime only.
        if path.as_os_str().is_empty() {
            continue;
        }
        assert_eq!(after.get(&path), Some(&entry), "changed existing {path:?}");
    }

    assert_eq!(config["schema_version"], 1);
    assert_eq!(config["state"], "initialized");
    assert_eq!(config["database"], "project.sqlite");
    assert_eq!(config.as_object().unwrap().len(), 5);
    let project_id = config["project_id"].as_str().unwrap();
    assert_eq!(Uuid::parse_str(project_id).unwrap().get_version_num(), 4);
    assert!(
        OffsetDateTime::parse(config["created_at"].as_str().unwrap(), &Rfc3339)
            .unwrap()
            .offset()
            .is_utc()
    );
    let db = readonly_connection(&project);
    let row: (String, u32, String, String) = db
        .query_row(
            "SELECT project_id, schema_version, created_at, state FROM project_metadata WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(row.0, project_id);
    assert_eq!(row.1, 1);
    assert_eq!(row.2, config["created_at"].as_str().unwrap());
    assert_eq!(row.3, "initialized");
    assert_eq!(
        db.query_row("SELECT count(*) FROM project_metadata", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.pragma_query_value(None, "application_id", |r| r.get::<_, u32>(0))
            .unwrap(),
        0x4e4f4554
    );
    let tables: Vec<String> = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(tables, ["project_metadata"]);
    assert_eq!(
        db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(db);
    let first = sandbox.inspect_preserving(&project);
    let second = sandbox.inspect_preserving(&project);
    assert_eq!(first, second);
    assert!(first.contains(project_id));
    assert!(first.contains("initialized"));
    assert!(first.contains("空项目"));

    let another = sandbox.root.join("第二个空项目");
    assert_ne!(sandbox.init(&another)["project_id"], config["project_id"]);
}

#[test]
fn relative_and_dash_prefixed_paths_are_supported_and_projects_can_move() {
    let sandbox = Sandbox::new();
    let relative = Path::new("相对项目 with spaces");
    assert_success(sandbox.action("init", relative));
    let original = sandbox.cwd().join(relative);
    let config = read_config(&original);
    let config_bytes = fs::read(original.join("project.json")).unwrap();
    assert!(
        config_bytes
            .windows(b"project.sqlite".len())
            .any(|w| w == b"project.sqlite")
    );
    assert!(
        !String::from_utf8(config_bytes.clone())
            .unwrap()
            .contains(&sandbox.root.display().to_string())
    );
    let moved = sandbox.root.join("搬移后的项目");
    fs::rename(&original, &moved).unwrap();
    let stdout = sandbox.inspect_preserving(&moved);
    assert!(stdout.contains(config["project_id"].as_str().unwrap()));
    assert_eq!(fs::read(moved.join("project.json")).unwrap(), config_bytes);

    let dash_path = OsString::from("-独立项目 with spaces");
    assert_success(sandbox.run(&[
        OsString::from("project"),
        OsString::from("init"),
        OsString::from("--"),
        dash_path.clone(),
    ]));
    assert_success(sandbox.run(&[
        OsString::from("project"),
        OsString::from("inspect"),
        OsString::from("--"),
        dash_path,
    ]));
}

#[test]
fn existing_targets_and_invalid_parents_preserve_all_content() {
    let sandbox = Sandbox::new();
    let project = sandbox.root.join("已有项目");
    sandbox.init(&project);
    sandbox.rejects_preserving("init", &project, 3, "PROJECT_PATH_EXISTS");
    let empty = sandbox.root.join("已有空目录");
    fs::create_dir(&empty).unwrap();
    sandbox.rejects_preserving("init", &empty, 3, "PROJECT_PATH_EXISTS");
    let unknown = sandbox.root.join("未知非空目录");
    fs::create_dir(&unknown).unwrap();
    fs::write(unknown.join("keep.txt"), b"keep unknown content").unwrap();
    sandbox.rejects_preserving("init", &unknown, 3, "PROJECT_PATH_EXISTS");
    let file = sandbox.root.join("occupied");
    fs::write(&file, b"existing file").unwrap();
    sandbox.rejects_preserving("init", &file, 3, "PROJECT_PATH_EXISTS");
    sandbox.rejects_preserving("init", &file.join("child"), 3, "PROJECT_IO_ERROR");
    sandbox.rejects_preserving(
        "init",
        &sandbox.root.join("missing-parent/child"),
        3,
        "PROJECT_IO_ERROR",
    );
}

#[test]
fn inspection_works_with_readonly_files_and_windows_write_sharing_denied() {
    let sandbox = Sandbox::new();
    let project = sandbox.root.join("只读项目");
    let config = sandbox.init(&project);
    let database = project.join("project.sqlite");
    let _readonly = ReadonlyFiles::new(&[project.join("project.json"), database.clone()]);
    #[cfg(windows)]
    let _deny_write = {
        use std::os::windows::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&database)
            .unwrap()
    };
    #[cfg(not(windows))]
    let _deny_write = File::open(&database).unwrap();
    let output = sandbox.inspect_preserving(&project);
    assert!(output.contains(config["project_id"].as_str().unwrap()));
}

#[test]
fn malformed_configs_are_rejected_without_repair_or_path_escape() {
    let sandbox = Sandbox::new();
    for (name, code) in [
        ("malformed", "PROJECT_INVALID_CONFIG"),
        ("missing-id", "PROJECT_INVALID_CONFIG"),
        ("unknown-field", "PROJECT_INVALID_CONFIG"),
        ("future-schema", "PROJECT_UNSUPPORTED_SCHEMA"),
        ("unknown-state", "PROJECT_INVALID_CONFIG"),
        ("invalid-time", "PROJECT_INVALID_CONFIG"),
        ("invalid-uuid-variant", "PROJECT_INVALID_CONFIG"),
        ("mismatched-id", "PROJECT_INVALID_DATABASE"),
        ("relative-escape", "PROJECT_UNSAFE_PATH"),
        ("absolute-database", "PROJECT_UNSAFE_PATH"),
    ] {
        let project = sandbox.root.join(name);
        let mut config = sandbox.init(&project);
        match name {
            "malformed" => fs::write(project.join("project.json"), b"{").unwrap(),
            "missing-id" => {
                config.as_object_mut().unwrap().remove("project_id");
                write_config(&project, &config);
            }
            "unknown-field" => {
                config["typo"] = json!(true);
                write_config(&project, &config);
            }
            "future-schema" => {
                config["schema_version"] = json!(2);
                write_config(&project, &config);
            }
            "unknown-state" => {
                config["state"] = json!("completed");
                write_config(&project, &config);
            }
            "invalid-time" => {
                config["created_at"] = json!("not a timestamp");
                write_config(&project, &config);
            }
            "invalid-uuid-variant" => {
                config["project_id"] = json!("00000000-0000-4000-0000-000000000000");
                write_config(&project, &config);
            }
            "mismatched-id" => {
                config["project_id"] = json!(Uuid::new_v4().to_string());
                write_config(&project, &config);
            }
            "relative-escape" => {
                config["database"] = json!("../outside.sqlite");
                write_config(&project, &config);
            }
            "absolute-database" => {
                config["database"] =
                    json!(sandbox.root.join("outside.sqlite").display().to_string());
                write_config(&project, &config);
            }
            _ => unreachable!(),
        }
        sandbox.rejects_preserving("inspect", &project, 2, code);
    }
}

#[test]
fn corrupt_or_mismatched_databases_are_rejected_without_migration() {
    let sandbox = Sandbox::new();
    for (name, code) in [
        ("corrupt", "PROJECT_INVALID_DATABASE"),
        ("missing-row", "PROJECT_INVALID_DATABASE"),
        ("database-id", "PROJECT_INVALID_DATABASE"),
        ("application-id", "PROJECT_INVALID_DATABASE"),
        ("database-schema", "PROJECT_UNSUPPORTED_SCHEMA"),
    ] {
        let project = sandbox.root.join(name);
        sandbox.init(&project);
        if name == "corrupt" {
            fs::write(project.join("project.sqlite"), b"not a SQLite database").unwrap();
        } else {
            let db = Connection::open(project.join("project.sqlite")).unwrap();
            match name {
                "missing-row" => {
                    db.execute("DELETE FROM project_metadata", []).unwrap();
                }
                "database-id" => {
                    db.execute(
                        "UPDATE project_metadata SET project_id=?1",
                        [Uuid::new_v4().to_string()],
                    )
                    .unwrap();
                }
                "application-id" => db.pragma_update(None, "application_id", 0).unwrap(),
                "database-schema" => db.pragma_update(None, "user_version", 2).unwrap(),
                _ => unreachable!(),
            }
        }
        sandbox.rejects_preserving("inspect", &project, 2, code);
    }
}

#[test]
fn incomplete_layouts_are_reported_and_not_cleaned_by_readonly_inspection() {
    let sandbox = Sandbox::new();
    for name in ["missing-config", "missing-database", "marker", "temporary"] {
        let project = sandbox.root.join(name);
        sandbox.init(&project);
        match name {
            "missing-config" => fs::remove_file(project.join("project.json")).unwrap(),
            "missing-database" => fs::remove_file(project.join("project.sqlite")).unwrap(),
            "marker" => fs::write(
                project.join(".noetome-initializing"),
                b"interrupted initialization marker",
            )
            .unwrap(),
            "temporary" => fs::write(
                project.join(".project.sqlite.initializing"),
                b"unfinished candidate",
            )
            .unwrap(),
            _ => unreachable!(),
        }
        sandbox.rejects_preserving("inspect", &project, 3, "PROJECT_INCOMPLETE");
        // Retrying must preserve the incomplete directory, including all remnants.
        let before = snapshot(&sandbox.root);
        let output = sandbox.action("init", &project);
        assert_eq!(output.status.code(), Some(3));
        assert!(output.stdout.is_empty());
        assert_eq!(snapshot(&sandbox.root), before);
    }
}

#[test]
fn active_rollback_and_wal_journals_are_blocked_without_touching_sqlite_files() {
    let sandbox = Sandbox::new();
    for mode in ["DELETE", "WAL"] {
        let project = sandbox.root.join(mode);
        sandbox.init(&project);
        let db = Connection::open(project.join("project.sqlite")).unwrap();
        db.pragma_update(None, "journal_mode", mode).unwrap();
        let database_before = fs::read(project.join("project.sqlite")).unwrap();
        let config_before = fs::read(project.join("project.json")).unwrap();
        db.execute_batch(
            "BEGIN IMMEDIATE; UPDATE project_metadata SET created_at='2026-01-01T00:00:00Z';",
        )
        .unwrap();
        let sidecar = if mode == "WAL" {
            "project.sqlite-wal"
        } else {
            "project.sqlite-journal"
        };
        assert!(
            project.join(sidecar).exists(),
            "SQLite did not create its active {mode} journal"
        );
        let before = transaction_snapshot(&sandbox.root);
        assert_error(
            sandbox.action("inspect", &project),
            3,
            "PROJECT_DATABASE_BUSY",
        );
        assert_eq!(
            transaction_snapshot(&sandbox.root),
            before,
            "blocked inspect changed SQLite files during an active {mode} transaction"
        );
        db.execute_batch("ROLLBACK;").unwrap();
        db.close().unwrap();
        assert_eq!(
            fs::read(project.join("project.sqlite")).unwrap(),
            database_before
        );
        assert_eq!(
            fs::read(project.join("project.json")).unwrap(),
            config_before
        );
        sandbox.inspect_preserving(&project);
    }
}

#[test]
fn project_help_and_invalid_syntax_have_no_filesystem_side_effects() {
    let sandbox = Sandbox::new();
    for values in [
        vec!["project"],
        vec!["project", "--help"],
        vec!["project", "-h"],
        vec!["project", "init", "--help"],
        vec!["project", "inspect", "--help"],
    ] {
        let before = snapshot(&sandbox.root);
        let output = sandbox.run(&values.iter().map(OsString::from).collect::<Vec<_>>());
        let stdout = assert_success(output);
        assert!(stdout.contains("project"));
        assert_eq!(snapshot(&sandbox.root), before);
    }
    for values in [
        vec!["project", "init"],
        vec!["project", "inspect"],
        vec!["project", "delete", "anything"],
        vec!["project", "init", "--bad"],
        vec!["project", "inspect", "", "extra"],
        vec!["project", "init", "--", ""],
        vec!["project", "init", "--", "path", "extra"],
    ] {
        let before = snapshot(&sandbox.root);
        assert_error(
            sandbox.run(&values.iter().map(OsString::from).collect::<Vec<_>>()),
            2,
            "CLI_INVALID_ARGUMENT",
        );
        assert_eq!(snapshot(&sandbox.root), before);
    }
}

#[cfg(windows)]
#[test]
fn windows_junction_roots_and_parents_are_rejected_without_writing_the_target() {
    let sandbox = Sandbox::new();
    let outside = sandbox.root.join("外部测试项目");
    sandbox.init(&outside);
    fs::write(outside.join("人工保护.md"), "# 外部人工内容\n不得改写。\n").unwrap();
    let link = sandbox.root.join("入口 junction with spaces");
    sandbox.junction(&link, &outside);
    sandbox.rejects_preserving("inspect", &link, 2, "PROJECT_UNSAFE_PATH");
    sandbox.rejects_preserving("init", &link, 2, "PROJECT_UNSAFE_PATH");
    sandbox.rejects_preserving("init", &link.join("不应创建"), 2, "PROJECT_UNSAFE_PATH");
    assert!(!outside.join("不应创建").exists());
}

#[cfg(windows)]
#[test]
fn case_insensitive_windows_sidecar_names_remain_blocking_and_untouched() {
    let sandbox = Sandbox::new();
    let project = sandbox.root.join("大写事务文件名");
    sandbox.init(&project);
    // On Windows this is the same filename as project.sqlite-wal. Presence is
    // sufficient to require recovery; these bytes are a retained test remnant.
    fs::write(
        project.join("PROJECT.SQLITE-WAL"),
        b"preserve retained sidecar",
    )
    .unwrap();
    sandbox.rejects_preserving("inspect", &project, 3, "PROJECT_DATABASE_BUSY");
}

#[cfg(unix)]
#[test]
fn symbolic_links_to_project_components_are_rejected_without_touching_targets() {
    use std::os::unix::fs::symlink;
    let sandbox = Sandbox::new();
    let outside = sandbox.root.join("outside-project");
    sandbox.init(&outside);
    for component in ["project.json", "project.sqlite"] {
        let project = sandbox.root.join(format!("linked-{component}"));
        sandbox.init(&project);
        fs::remove_file(project.join(component)).unwrap();
        symlink(outside.join(component), project.join(component)).unwrap();
        sandbox.rejects_preserving("inspect", &project, 2, "PROJECT_UNSAFE_PATH");
    }
}
