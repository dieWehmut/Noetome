//! Explicit isolated application data; the default Windows profile is unchanged.
use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const ENVIRONMENT: &str = "NOETOME_DESKTOP_DATA_ROOT";
const MARKER: &str = ".noetome-desktop-data.json";
const MAX_MARKER: u64 = 8192;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    schema_version: u32,
    producer: String,
    purpose: String,
    root: String,
}

pub struct IsolatedData {
    root: PathBuf,
    // On Windows these handles deny write/delete sharing for the directory and
    // owner marker for the lifetime of the application.
    _directory: File,
    _marker: File,
    _ancestors: Vec<File>,
}
impl IsolatedData {
    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn refused(message: impl std::fmt::Display) -> String {
    format!("DESKTOP_DATA_UNSAFE：独立应用数据目录不可用：{message}；没有使用默认用户目录替代。")
}
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
fn ordinary_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !name.is_empty()
        && !matches!(name, "." | "..")
        && !name.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*', '\0'])
        && !name.chars().any(char::is_control)
        && !name.ends_with(['.', ' '])
        && !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem[3..].chars().count() == 1
            && stem[3..]
                .chars()
                .next()
                .is_some_and(|c| "123456789¹²³".contains(c)))
}
fn check(path: &Path) -> Result<(), String> {
    let original = path.to_str().ok_or_else(|| refused("路径不是有效文字"))?;
    if !path.is_absolute()
        || original.is_empty()
        || original
            .split(|c| c == '/' || (cfg!(windows) && c == '\\'))
            .any(|c| matches!(c, "." | ".."))
    {
        return Err(refused("须为不含 . 或 .. 的绝对路径"));
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        if let Component::Normal(name) = component {
            if !name.to_str().is_some_and(ordinary_name) {
                return Err(refused("路径包含 Windows 设备名或不安全名称"));
            }
        }
        #[cfg(windows)]
        if let Component::Prefix(prefix) = component {
            use std::path::Prefix;
            if !matches!(
                prefix.kind(),
                Prefix::Disk(_)
                    | Prefix::VerbatimDisk(_)
                    | Prefix::UNC(_, _)
                    | Prefix::VerbatimUNC(_, _)
            ) {
                return Err(refused("不接受设备命名空间"));
            }
        }
        current.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(m) if is_link(&m) || !m.is_dir() => {
                return Err(refused("路径包含链接、重解析点或非目录"));
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(refused(e)),
        }
    }
    noetome_cli::extraction_preview::ensure_outside_vault(path).map_err(refused)
}
fn directory(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x02200000);
    }
    let file = options.open(path).map_err(refused)?;
    let metadata = file.metadata().map_err(refused)?;
    if is_link(&metadata) || !metadata.is_dir() {
        return Err(refused("持有目录不是普通目录"));
    }
    Ok(file)
}
fn ancestors(path: &Path) -> Result<Vec<File>, String> {
    path.ancestors()
        .skip(1)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(directory)
        .collect()
}
fn owner_file(path: &Path, creating: bool) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    if creating {
        options.write(true).create_new(true);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x00200000);
    }
    options.open(path).map_err(refused)
}
fn marker_bytes(
    path: &Path,
    file: &mut File,
    identity: &same_file::Handle,
) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(refused)?;
    if is_link(&metadata)
        || !metadata.is_file()
        || metadata.len() > MAX_MARKER
        || is_link(&file.metadata().map_err(refused)?)
    {
        return Err(refused("所有权记录不是普通受保护文件"));
    }
    #[cfg(windows)]
    let links = winapi_util::file::information(&*file)
        .map_err(refused)?
        .number_of_links() as u64;
    #[cfg(unix)]
    let links = {
        use std::os::unix::fs::MetadataExt;
        file.metadata().map_err(refused)?.nlink()
    };
    if links != 1
        || same_file::Handle::from_file(file.try_clone().map_err(refused)?).map_err(refused)?
            != *identity
        || same_file::Handle::from_path(path).map_err(refused)? != *identity
    {
        return Err(refused("所有权记录身份变化或有多个硬链接"));
    }
    file.seek(SeekFrom::Start(0)).map_err(refused)?;
    let mut bytes = Vec::new();
    Read::by_ref(file)
        .take(MAX_MARKER + 1)
        .read_to_end(&mut bytes)
        .map_err(refused)?;
    if bytes.len() as u64 > MAX_MARKER {
        return Err(refused("所有权记录超过大小限制"));
    }
    Ok(bytes)
}
fn handoff_marker(
    path: &Path,
    writer: File,
    expected: &[u8],
    between: impl FnOnce(),
) -> Result<File, String> {
    let writer_identity =
        same_file::Handle::from_file(writer.try_clone().map_err(refused)?).map_err(refused)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Permit our still-live writer while retaining delete protection.
        options.share_mode(3).custom_flags(0x00200000);
    }
    let mut transition = options.open(path).map_err(refused)?;
    if marker_bytes(path, &mut transition, &writer_identity)? != expected {
        return Err(refused("写入后的所有权原件不一致"));
    }
    let identity =
        same_file::Handle::from_file(transition.try_clone().map_err(refused)?).map_err(refused)?;
    drop(writer_identity);
    drop(writer);
    between();
    let mut readonly = owner_file(path, false)?;
    if marker_bytes(path, &mut readonly, &identity)? != expected {
        return Err(refused("所有权记录在只读交接时改变"));
    }
    Ok(readonly)
}
fn existing(path: &Path) -> Result<IsolatedData, String> {
    check(path)?;
    let ancestors = ancestors(path)?;
    let root = fs::canonicalize(path).map_err(refused)?;
    let directory = directory(&root)?;
    let marker = root.join(MARKER);
    let metadata = fs::symlink_metadata(&marker).map_err(refused)?;
    if is_link(&metadata) || !metadata.is_file() || metadata.len() > MAX_MARKER {
        return Err(refused("已有目录没有有效的独立应用所有权记录"));
    }
    let mut file = owner_file(&marker, false)?;
    let marker_identity =
        same_file::Handle::from_file(file.try_clone().map_err(refused)?).map_err(refused)?;
    let bytes = marker_bytes(&marker, &mut file, &marker_identity)?;
    let owner: Owner = serde_json::from_slice(&bytes).map_err(refused)?;
    if bytes.len() as u64 > MAX_MARKER
        || owner.schema_version != 1
        || owner.producer != "noetome_desktop"
        || owner.purpose != "isolated_application_data"
        || owner.root != root.to_string_lossy()
        || same_file::Handle::from_file(directory.try_clone().map_err(refused)?).map_err(refused)?
            != same_file::Handle::from_path(&root).map_err(refused)?
    {
        return Err(refused("独立应用所有权或目录身份不一致"));
    }
    Ok(IsolatedData {
        root,
        _directory: directory,
        _marker: file,
        _ancestors: ancestors,
    })
}
pub fn prepare_from_selection(selected: Option<OsString>) -> Result<Option<IsolatedData>, String> {
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = PathBuf::from(selected);
    check(&path)?;
    match fs::symlink_metadata(&path) {
        Ok(_) => existing(&path).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Only the selected final component is created. Existing parents
            // and unmarked existing roots are never adopted or overwritten.
            let ancestors = ancestors(&path)?;
            fs::create_dir(&path).map_err(refused)?;
            let root = fs::canonicalize(&path).map_err(refused)?;
            let directory = directory(&root)?;
            let mut file = owner_file(&root.join(MARKER), true)?;
            let bytes = serde_json::to_vec(&Owner {
                schema_version: 1,
                producer: "noetome_desktop".into(),
                purpose: "isolated_application_data".into(),
                root: root.to_string_lossy().into_owned(),
            })
            .map_err(refused)?;
            file.write_all(&bytes).map_err(refused)?;
            file.sync_all().map_err(refused)?;
            let file = handoff_marker(&root.join(MARKER), file, &bytes, || {})?;
            check(&root)?;
            Ok(Some(IsolatedData {
                root,
                _directory: directory,
                _marker: file,
                _ancestors: ancestors,
            }))
        }
        Err(e) => Err(refused(e)),
    }
}
pub fn prepare_from_environment() -> Result<Option<IsolatedData>, String> {
    prepare_from_selection(std::env::var_os(ENVIRONMENT))
}
pub fn model_root_override() -> Result<Option<PathBuf>, String> {
    let Some(path) = std::env::var_os(ENVIRONMENT) else {
        return Ok(None);
    };
    let owned = existing(&PathBuf::from(path))?;
    Ok(Some(owned.root.join("model-profile")))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn example(test: impl FnOnce(&Path)) {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/desktop-data-tests");
        fs::create_dir_all(&base).unwrap();
        let parent = fs::canonicalize(base).unwrap().join(format!(
            "case-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&parent).unwrap();
        test(&parent);
        fs::remove_dir_all(parent).unwrap();
    }
    #[test]
    fn absent_override_preserves_default_and_new_cjk_root_can_be_reopened() {
        assert!(prepare_from_selection(None).unwrap().is_none());
        example(|parent| {
            let path = parent.join("独立 应用目录");
            let owned = prepare_from_selection(Some(path.clone().into_os_string()))
                .unwrap()
                .unwrap();
            let canonical = owned.root().to_owned();
            drop(owned);
            fs::write(
                path.join("desktop-preferences.json"),
                b"author temporary preferences",
            )
            .unwrap();
            let owner_before = fs::read(path.join(MARKER)).unwrap();
            let resumed = prepare_from_selection(Some(path.clone().into_os_string()))
                .unwrap()
                .unwrap();
            assert_eq!(resumed.root(), canonical);
            assert_eq!(fs::read(path.join(MARKER)).unwrap(), owner_before);
            assert_eq!(
                fs::read(path.join("desktop-preferences.json")).unwrap(),
                b"author temporary preferences"
            );
            drop(resumed);
        });
    }
    #[test]
    fn unsafe_spellings_and_reserved_names_refuse_before_creating_children() {
        example(|parent| {
            fs::write(parent.join("sentinel"), b"keep").unwrap();
            for name in [
                "../escape",
                "./dot",
                "CON",
                "nul.txt",
                "COM1",
                "LPT9",
                "COM¹",
                "trailing.",
                "trailing ",
                "bad:name",
                "bad|name",
                "bad\nname",
            ] {
                let raw = PathBuf::from(format!("{}/{name}", parent.display()));
                assert!(
                    prepare_from_selection(Some(raw.into_os_string())).is_err(),
                    "accepted {name}"
                );
            }
            assert!(prepare_from_selection(Some(OsString::from("relative"))).is_err());
            assert!(prepare_from_selection(Some(OsString::new())).is_err());
            assert_eq!(fs::read(parent.join("sentinel")).unwrap(), b"keep");
            assert_eq!(fs::read_dir(parent).unwrap().count(), 1);
        });
    }
    #[test]
    fn existing_empty_unknown_or_corrupt_root_is_preserved() {
        example(|parent| {
            let root = parent.join("unknown");
            fs::create_dir(&root).unwrap();
            assert!(prepare_from_selection(Some(root.clone().into_os_string())).is_err());
            fs::write(root.join("private.txt"), b"manual content").unwrap();
            fs::write(root.join(MARKER), b"{bad").unwrap();
            assert!(prepare_from_selection(Some(root.clone().into_os_string())).is_err());
            assert_eq!(
                fs::read(root.join("private.txt")).unwrap(),
                b"manual content"
            );
            assert_eq!(fs::read(root.join(MARKER)).unwrap(), b"{bad");
        });
    }
    #[test]
    fn owner_from_another_root_and_existing_vault_are_not_adopted() {
        example(|parent| {
            let one = parent.join("one");
            let owned = prepare_from_selection(Some(one.clone().into_os_string()))
                .unwrap()
                .unwrap();
            drop(owned);
            let two = parent.join("two");
            fs::create_dir(&two).unwrap();
            fs::copy(one.join(MARKER), two.join(MARKER)).unwrap();
            assert!(prepare_from_selection(Some(two.clone().into_os_string())).is_err());
            fs::create_dir(parent.join(".obsidian")).unwrap();
            assert!(prepare_from_selection(Some(parent.join("refused").into_os_string())).is_err());
            assert!(!parent.join("refused").exists());
        });
    }
    #[test]
    fn ownership_marker_with_a_hardlink_is_not_reopened() {
        example(|parent| {
            let root = parent.join("hardlinked");
            let owned = prepare_from_selection(Some(root.clone().into_os_string()))
                .unwrap()
                .unwrap();
            drop(owned);
            let extra = parent.join("extra-owner-link");
            fs::hard_link(root.join(MARKER), &extra).unwrap();
            let before = fs::read(&extra).unwrap();
            assert!(prepare_from_selection(Some(root.into_os_string())).is_err());
            assert_eq!(fs::read(extra).unwrap(), before);
        });
    }
    #[test]
    fn marker_change_during_writer_to_readonly_handoff_is_rejected() {
        example(|parent| {
            let root = parent.join("handoff");
            fs::create_dir(&root).unwrap();
            let marker = root.join(MARKER);
            let mut writer = owner_file(&marker, true).unwrap();
            let expected = b"original owner receipt";
            writer.write_all(expected).unwrap();
            writer.sync_all().unwrap();
            let result = handoff_marker(&marker, writer, expected, || {
                #[cfg(windows)]
                {
                    assert!(fs::rename(&marker, root.join("preserved")).is_err());
                    fs::write(&marker, b"changed during handoff").unwrap();
                }
                #[cfg(not(windows))]
                {
                    fs::rename(&marker, root.join("preserved")).unwrap();
                    fs::write(&marker, expected).unwrap();
                }
            });
            assert!(result.is_err());
        });
    }
    #[cfg(unix)]
    #[test]
    fn ancestor_or_marker_symlink_is_refused_without_mutation() {
        example(|parent| {
            let target = parent.join("target");
            fs::create_dir(&target).unwrap();
            let alias = parent.join("alias");
            std::os::unix::fs::symlink(&target, &alias).unwrap();
            assert!(prepare_from_selection(Some(alias.join("new").into_os_string())).is_err());
            assert!(!target.join("new").exists());
            let selected = parent.join("selected");
            fs::create_dir(&selected).unwrap();
            let sentinel = parent.join("sentinel");
            fs::write(&sentinel, b"manual").unwrap();
            std::os::unix::fs::symlink(&sentinel, selected.join(MARKER)).unwrap();
            assert!(prepare_from_selection(Some(selected.into_os_string())).is_err());
            assert_eq!(fs::read(sentinel).unwrap(), b"manual");
        });
    }
    #[cfg(windows)]
    #[test]
    fn live_root_and_marker_cannot_be_renamed_or_modified() {
        example(|parent| {
            let root = parent.join("held");
            let owned = prepare_from_selection(Some(root.clone().into_os_string()))
                .unwrap()
                .unwrap();
            assert!(fs::rename(&root, parent.join("moved")).is_err());
            assert!(
                OpenOptions::new()
                    .write(true)
                    .open(root.join(MARKER))
                    .is_err()
            );
            assert!(fs::remove_file(root.join(MARKER)).is_err());
            drop(owned);
        });
    }
}
