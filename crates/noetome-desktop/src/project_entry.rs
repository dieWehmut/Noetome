//! Desktop project entry: create a new project root and inspect an existing one.
//!
//! This module is a thin projection. Every decision about paths, ownership,
//! contents and refusal codes belongs to `noetome_cli::project`; the desktop
//! layer rejects dot components in the original selection before any Windows
//! path normalization, then forwards the call and copies the returned fields. It never
//! creates directories of its own, never repairs a damaged root, never writes
//! into a root that already exists, and never substitutes its own error code.

use std::path::Path;

use serde::Serialize;

/// The project fields exactly as the CLI reports them, plus the directory the
/// user picked. The frontend renders these fields verbatim: it must not derive
/// or invent a project id, timestamp or schema version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectSummary {
    pub path: String,
    pub project_id: String,
    pub schema_version: u32,
    pub created_at: String,
    pub state: String,
    pub database: String,
}

fn display_path(path: &Path) -> String {
    // For display only. A root that was just created or successfully inspected
    // always resolves; the fallback keeps an unusual but already-rejected path
    // readable instead of failing the whole call.
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn summarise(path: &Path, info: noetome_cli::project::ProjectInfo) -> ProjectSummary {
    ProjectSummary {
        path: display_path(path),
        project_id: info.project_id,
        schema_version: info.schema_version,
        created_at: info.created_at,
        state: info.state,
        database: info.database,
    }
}

fn validate_selected_path(path: &Path) -> Result<(), String> {
    // `Path::components` drops interior `.` and pushing onto a Windows
    // verbatim path can collapse `..`. Inspect the caller's original spelling
    // before the Core turns the selection into an absolute path or walks it.
    let valid = path.to_str().is_some_and(|selected| {
        !selected.is_empty()
            && !selected.contains('\0')
            && !selected
                .split(|character| character == '/' || (cfg!(windows) && character == '\\'))
                .any(|component| matches!(component, "." | ".."))
    });
    if valid {
        Ok(())
    } else {
        Err(format!(
            "PROJECT_UNSAFE_PATH：项目路径不能含路径跳转或不安全原始拼写：{}",
            path.display()
        ))
    }
}

/// Create a project at a directory that must not exist yet.
///
/// Refusals (`PROJECT_PATH_EXISTS`, `PROJECT_INCOMPLETE`, `PROJECT_UNSAFE_PATH`,
/// `PROJECT_IO_ERROR`, …) are returned as the CLI's own `CODE：message` text.
pub fn create(path: &Path) -> Result<ProjectSummary, String> {
    validate_selected_path(path)?;
    noetome_cli::extraction_preview::ensure_outside_vault(path)
        .map_err(|error| error.to_string())?;
    let info = noetome_cli::project::initialize(path).map_err(|error| error.to_string())?;
    Ok(summarise(path, info))
}

pub fn create_in_parent(parent: &Path, name: &str) -> Result<ProjectSummary, String> {
    validate_selected_path(parent)?;
    validate_name(name)?;
    if !parent.is_dir() {
        return Err("PROJECT_PARENT_MISSING：请选择已存在的父目录。".into());
    }
    create(&parent.join(name))
}

pub fn validate_name(name: &str) -> Result<(), String> {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if name.is_empty()
        || name.len() > 180
        || name.contains(['/', '\\', ':', '\0', '<', '>', '"', '|', '?', '*'])
        || name.chars().any(char::is_control)
        || matches!(name, "." | "..")
        || name.ends_with(['.', ' '])
        || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        Err(
            "PROJECT_UNSAFE_NAME：名称须为单个普通目录名，不能含跳转、分隔符或Windows设备名。"
                .into(),
        )
    } else {
        Ok(())
    }
}

/// Ask the operating system for a directory.
///
/// Only Windows builds carry a dialog dependency. Everywhere else the answer is
/// an explicit refusal so the interface can tell the user to type the path
/// instead of pretending a native picker exists.
#[cfg(windows)]
pub fn pick_directory() -> Result<Option<String>, String> {
    let picked = rfd::FileDialog::new()
        .set_title("选择项目目录")
        .pick_folder();
    Ok(picked.map(|path| path.to_string_lossy().into_owned()))
}

#[cfg(not(windows))]
pub fn pick_directory() -> Result<Option<String>, String> {
    Err(
        "DESKTOP_PROJECT_DIALOG_UNAVAILABLE：当前构建没有系统目录选择器，请直接输入项目路径。本次没有创建或修改任何目录。"
            .to_owned(),
    )
}

/// Read an existing project without modifying it.
pub fn open(path: &Path) -> Result<ProjectSummary, String> {
    validate_selected_path(path)?;
    let info = noetome_cli::project::inspect(path).map_err(|error| error.to_string())?;
    Ok(summarise(path, info))
}

#[cfg(test)]
mod tests {
    use super::{ProjectSummary, create, open};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    /// Every test writes beneath `target/desktop-project-tests`, which is
    /// ignored by Git, and removes only the directory it created itself.
    fn with_test_directory(name: &str, test: impl FnOnce(&Path)) {
        let parent =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/desktop-project-tests");
        fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = parent.join(format!("{name}-{}-{nonce}", std::process::id()));
        fs::create_dir(&root).expect("create exclusive test directory");
        test(&root);
        fs::remove_dir_all(root).unwrap();
    }

    /// Read-only snapshot: relative path → file bytes, directories as `dir:`.
    fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut files = BTreeMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(directory) = stack.pop() {
            for entry in fs::read_dir(&directory).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let metadata = fs::symlink_metadata(&path).unwrap();
                if metadata.is_dir() {
                    files.insert(format!("dir:{relative}"), Vec::new());
                    stack.push(path);
                } else {
                    files.insert(relative, fs::read(&path).unwrap());
                }
            }
        }
        files
    }

    fn sentinel(root: &Path) -> PathBuf {
        let path = root.join("outside-sentinel.txt");
        fs::write(&path, b"content outside the project root must never change").unwrap();
        path
    }

    fn config(path: &Path) -> serde_json::Value {
        let bytes = fs::read(path.join("project.json")).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn create_reports_fields_that_match_the_disk() {
        with_test_directory("create", |root| {
            let guard = sentinel(root);
            let before = snapshot(root);
            let target = root.join("我的项目 with spaces");
            let summary = create(&target).unwrap();
            assert_eq!(summary.schema_version, 1);
            assert_eq!(summary.state, "initialized");
            assert_eq!(summary.database, "project.sqlite");
            assert_eq!(
                summary.path,
                target.canonicalize().unwrap().display().to_string()
            );
            assert!(!summary.project_id.is_empty() && !summary.created_at.is_empty());

            let config = config(&target);
            assert_eq!(config["project_id"], summary.project_id.as_str());
            assert_eq!(config["created_at"], summary.created_at.as_str());
            assert_eq!(config["state"], summary.state.as_str());
            assert_eq!(config["schema_version"], 1);
            assert!(target.join("project.sqlite").is_file());

            // Opening the same root reports the identical fields.
            let reopened = open(&target).unwrap();
            assert_eq!(reopened, summary);
            // The sibling sentinel is untouched.
            assert_eq!(
                fs::read(&guard).unwrap(),
                b"content outside the project root must never change"
            );
            let after_sentinel = snapshot(root).remove("outside-sentinel.txt").unwrap();
            assert_eq!(after_sentinel, before["outside-sentinel.txt"]);
        });
    }

    #[test]
    fn create_refuses_an_existing_directory_and_preserves_it() {
        with_test_directory("existing", |root| {
            let target = root.join("已有目录");
            fs::create_dir(&target).unwrap();
            let file = target.join("notes.md");
            fs::write(&file, b"user content").unwrap();
            let before = snapshot(root);
            let error = create(&target).unwrap_err();
            assert!(error.starts_with("PROJECT_PATH_EXISTS"), "{error}");
            assert_eq!(snapshot(root), before);
        });
    }

    #[test]
    fn create_refuses_path_traversal_without_writing_anything() {
        with_test_directory("traversal", |root| {
            let guard = sentinel(root);
            let existing = root.join("已有项目 with spaces");
            create(&existing).unwrap();
            let before = snapshot(root);
            // Preserve the selected spelling. A `join("..")` on the canonical
            // Windows verbatim root would erase the traversal before `create`.
            let separator = std::path::MAIN_SEPARATOR;
            let selections = [
                format!(".{separator}已有项目 with spaces"),
                format!("..{separator}escaped-project"),
                format!("已有项目 with spaces{separator}.{separator}new-project"),
                format!("已有项目 with spaces{separator}..{separator}new-project"),
                format!("已有项目 with spaces{separator}."),
                format!("已有项目 with spaces{separator}.."),
                format!(".{separator}..{separator}escaped-project"),
            ];
            for selection in selections {
                let mut original = root.as_os_str().to_owned();
                original.push(separator.to_string());
                original.push(selection);
                let target = Path::new(&original);
                for operation in [create, open] {
                    let error = operation(target).unwrap_err();
                    assert!(error.starts_with("PROJECT_UNSAFE_PATH"), "{error}");
                    assert_eq!(snapshot(root), before);
                    assert_eq!(fs::read(&guard).unwrap(), before["outside-sentinel.txt"]);
                }
            }
        });
    }

    #[cfg(windows)]
    #[test]
    fn original_windows_mixed_separators_are_refused_before_io() {
        for selected in [
            r"C:\\project/../escaped",
            r"\\?\C:\project\.\nested",
            r"\\?\UNC\server\share\project\..\escaped",
        ] {
            for operation in [create, open] {
                let error = operation(Path::new(selected)).unwrap_err();
                assert!(error.starts_with("PROJECT_UNSAFE_PATH"), "{error}");
            }
        }
    }

    #[test]
    fn open_reports_missing_and_foreign_directories_with_the_cli_codes() {
        with_test_directory("missing", |root| {
            let before = snapshot(root);
            let error = open(&root.join("没有这个项目")).unwrap_err();
            assert!(error.starts_with("PROJECT_IO_ERROR"), "{error}");
            assert_eq!(snapshot(root), before);

            let foreign = root.join("外来目录");
            fs::create_dir(&foreign).unwrap();
            fs::write(foreign.join("readme.txt"), b"not a project").unwrap();
            let before = snapshot(root);
            let error = open(&foreign).unwrap_err();
            assert!(error.starts_with("PROJECT_IO_ERROR"), "{error}");
            assert_eq!(snapshot(root), before);
        });
    }

    #[test]
    fn open_refuses_incomplete_and_corrupt_projects_without_repairing_them() {
        with_test_directory("damaged", |root| {
            let incomplete = root.join("未完成");
            fs::create_dir(&incomplete).unwrap();
            fs::write(incomplete.join(".noetome-initializing"), b"id\n").unwrap();
            let before = snapshot(root);
            let error = open(&incomplete).unwrap_err();
            assert!(error.starts_with("PROJECT_INCOMPLETE"), "{error}");
            assert_eq!(snapshot(root), before);

            let corrupt = root.join("损坏");
            fs::create_dir(&corrupt).unwrap();
            fs::write(corrupt.join("project.json"), b"{ not json").unwrap();
            fs::write(corrupt.join("project.sqlite"), b"").unwrap();
            let before = snapshot(root);
            let error = open(&corrupt).unwrap_err();
            assert!(error.starts_with("PROJECT_INVALID_CONFIG"), "{error}");
            assert_eq!(snapshot(root), before);
        });
    }

    #[test]
    fn open_never_writes_to_a_valid_project() {
        with_test_directory("readonly", |root| {
            let target = root.join("项目");
            let created: ProjectSummary = create(&target).unwrap();
            let before = snapshot(&target);
            let opened = open(&target).unwrap();
            assert_eq!(opened, created);
            assert_eq!(snapshot(&target), before);
        });
    }
}
