//! Application-only window preferences; no project or Vault paths are accepted.
use std::fs::{self, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "desktop-preferences.json";
const MAX_BYTES: u64 = 4096;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseBehavior {
    HideToTray,
    Exit,
}

impl CloseBehavior {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HideToTray => "hide_to_tray",
            Self::Exit => "exit",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "hide_to_tray" => Ok(Self::HideToTray),
            "exit" => Ok(Self::Exit),
            _ => Err(
                "DESKTOP_SETTINGS_INVALID_VALUE：关闭行为只能选择隐藏到托盘或直接退出。".to_owned(),
            ),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsFile {
    schema_version: u32,
    close_behavior: CloseBehavior,
}

struct Snapshot {
    bytes: Vec<u8>,
    identity: same_file::Handle,
}

struct Store {
    directory: PathBuf,
    current: CloseBehavior,
    snapshot: Option<Snapshot>,
}

pub struct PreferencesState {
    directory: Option<PathBuf>,
    store: Mutex<Result<Store, String>>,
}

impl PreferencesState {
    pub fn load(directory: PathBuf) -> Self {
        Self {
            store: Mutex::new(Store::load(directory.clone())),
            directory: Some(directory),
        }
    }

    pub fn unavailable(error: String) -> Self {
        Self {
            directory: None,
            store: Mutex::new(Err(format!(
                "DESKTOP_SETTINGS_UNAVAILABLE：无法确定应用设置目录：{error}"
            ))),
        }
    }

    pub fn get(&self) -> Result<String, String> {
        let guard = self.store.lock().map_err(|_| unavailable())?;
        guard
            .as_ref()
            .map(|store| store.current.as_str().to_owned())
            .map_err(Clone::clone)
    }

    pub fn reload(&self) -> Result<String, String> {
        let Some(directory) = &self.directory else {
            return self.get();
        };
        let mut guard = self.store.lock().map_err(|_| unavailable())?;
        match Store::load(directory.clone()) {
            Ok(store) => {
                let behavior = store.current.as_str().to_owned();
                *guard = Ok(store);
                Ok(behavior)
            }
            Err(error) => {
                // A failed retry never discards a previously effective choice
                // or its snapshot. Unknown on-disk content remains untouched.
                if guard.is_err() {
                    *guard = Err(error.clone());
                }
                Err(error)
            }
        }
    }

    pub fn set(&self, value: &str) -> Result<String, String> {
        let desired = CloseBehavior::parse(value)?;
        let mut guard = self.store.lock().map_err(|_| unavailable())?;
        let store = guard.as_mut().map_err(|error| error.clone())?;
        store.save(desired)?;
        Ok(store.current.as_str().to_owned())
    }

    pub fn effective(&self) -> CloseBehavior {
        // Corrupt/unsupported preferences do not destroy the only window.
        self.store
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().ok().map(|store| store.current))
            .unwrap_or(CloseBehavior::HideToTray)
    }
}

fn unavailable() -> String {
    "DESKTOP_SETTINGS_UNAVAILABLE：窗口设置当前不可用；关闭窗口将保留应用到托盘。".to_owned()
}

fn io_error(path: &Path, error: impl std::fmt::Display) -> String {
    format!(
        "DESKTOP_SETTINGS_IO_ERROR：无法读写窗口设置 {}：{error}；没有将失败操作记为保存成功。",
        path.display()
    )
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

fn check_directory(directory: &Path) -> Result<(), String> {
    if !directory.is_absolute()
        || directory
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("DESKTOP_SETTINGS_UNSAFE_PATH：应用设置必须位于普通应用数据目录。".to_owned());
    }
    let mut cursor = PathBuf::new();
    for component in directory.components() {
        cursor.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) if is_link(&metadata) || !metadata.is_dir() => {
                return Err(format!(
                    "DESKTOP_SETTINGS_UNSAFE_PATH：设置目录包含链接或非目录对象：{}；现有内容保留。",
                    cursor.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(&cursor, error)),
        }
    }
    Ok(())
}

fn read_snapshot(path: &Path) -> Result<Option<Snapshot>, String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(path, error)),
        Ok(metadata) if is_link(&metadata) || !metadata.is_file() => {
            return Err(format!(
                "DESKTOP_SETTINGS_UNSAFE_PATH：设置路径不是普通文件：{}；不会覆盖。",
                path.display()
            ));
        }
        Ok(metadata) if metadata.len() > MAX_BYTES => {
            return Err(
                "DESKTOP_SETTINGS_INVALID_FILE：设置文件超过4 KiB上限；现有内容保留。".to_owned(),
            );
        }
        Ok(_) => {}
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x00200000); // OPEN_REPARSE_POINT
    }
    let file = options.open(path).map_err(|error| io_error(path, error))?;
    if is_link(&file.metadata().map_err(|error| io_error(path, error))?) {
        return Err("DESKTOP_SETTINGS_UNSAFE_PATH：设置文件是链接；现有内容保留。".to_owned());
    }
    let opened =
        same_file::Handle::from_file(file.try_clone().map_err(|error| io_error(path, error))?)
            .map_err(|error| io_error(path, error))?;
    // This identity handle allows delete sharing, unlike the temporary read
    // guard. It remains alive to distinguish same-byte replacement files.
    let identity = same_file::Handle::from_path(path).map_err(|error| io_error(path, error))?;
    if opened != identity {
        return Err(conflict());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error(path, error))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(
            "DESKTOP_SETTINGS_INVALID_FILE：设置文件超过4 KiB上限；现有内容保留。".to_owned(),
        );
    }
    Ok(Some(Snapshot { bytes, identity }))
}

fn conflict() -> String {
    "DESKTOP_SETTINGS_CONFLICT：设置文件在读取后被修改、删除或替换；现有文件和已生效设置保留，请重新打开应用后检查。".to_owned()
}

fn matches_snapshot(left: &Option<Snapshot>, right: &Option<Snapshot>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left.bytes == right.bytes && left.identity == right.identity,
        _ => false,
    }
}

impl Store {
    fn load(directory: PathBuf) -> Result<Self, String> {
        check_directory(&directory)?;
        let snapshot = read_snapshot(&directory.join(FILE_NAME))?;
        let current = match &snapshot {
            None => CloseBehavior::HideToTray,
            Some(snapshot) => {
                let file: SettingsFile = serde_json::from_slice(&snapshot.bytes)
                    .map_err(|error| format!("DESKTOP_SETTINGS_INVALID_FILE：窗口设置格式无效：{error}；现有文件保留。"))?;
                if file.schema_version != 1 {
                    return Err(format!(
                        "DESKTOP_SETTINGS_UNSUPPORTED_VERSION：不支持窗口设置版本{}；没有迁移或覆盖。",
                        file.schema_version
                    ));
                }
                file.close_behavior
            }
        };
        Ok(Self {
            directory,
            current,
            snapshot,
        })
    }

    fn save(&mut self, desired: CloseBehavior) -> Result<(), String> {
        check_directory(&self.directory)?;
        let path = self.directory.join(FILE_NAME);
        if !matches_snapshot(&self.snapshot, &read_snapshot(&path)?) {
            return Err(conflict());
        }
        if self.current == desired && self.snapshot.is_some() {
            return Ok(());
        }
        fs::create_dir_all(&self.directory).map_err(|error| io_error(&self.directory, error))?;
        check_directory(&self.directory)?;
        let mut bytes = serde_json::to_vec_pretty(&SettingsFile {
            schema_version: 1,
            close_behavior: desired,
        })
        .map_err(|error| io_error(&path, error))?;
        bytes.push(b'\n');
        let staged = OwnedTemporary::create(&self.directory, bytes)?;
        let next_identity = same_file::Handle::from_path(&staged.path)
            .map_err(|error| io_error(&staged.path, error))?;
        // Recheck the original after writing the candidate; never replace an
        // unknown modification or a newly appeared target.
        check_directory(&self.directory)?;
        if !matches_snapshot(&self.snapshot, &read_snapshot(&path)?) {
            return Err(conflict());
        }
        if self.snapshot.is_some() {
            // Same-directory rename atomically replaces an existing file on
            // Windows and Unix. Never delete the original before publication.
            fs::rename(&staged.path, &path).map_err(|error| io_error(&path, error))?;
        } else {
            // First publication must fail if another file has appeared.
            fs::hard_link(&staged.path, &path).map_err(|error| io_error(&path, error))?;
        }
        self.current = desired;
        self.snapshot = Some(Snapshot {
            bytes: staged.bytes.clone(),
            identity: next_identity,
        });
        Ok(())
    }
}

struct OwnedTemporary {
    path: PathBuf,
    bytes: Vec<u8>,
    identity: Option<same_file::Handle>,
}

impl OwnedTemporary {
    fn create(directory: &Path, bytes: Vec<u8>) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            ".desktop-preferences-{}-{nonce}-{sequence}.tmp",
            std::process::id()
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| io_error(&path, error))?;
        let identity =
            same_file::Handle::from_path(&path).map_err(|error| io_error(&path, error))?;
        let temporary = Self {
            path,
            bytes,
            identity: Some(identity),
        };
        file.write_all(&temporary.bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| io_error(&temporary.path, error))?;
        Ok(temporary)
    }
}

impl Drop for OwnedTemporary {
    fn drop(&mut self) {
        // Only the unchanged exclusive temporary is removable. Never recursively
        // clean the application directory or delete a replacement/unknown file.
        let Ok(Some(current)) = read_snapshot(&self.path) else {
            return;
        };
        let expected = self.identity.as_ref();
        if expected.is_some_and(|identity| identity == &current.identity)
            && current.bytes == self.bytes
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Sandbox(PathBuf);
    impl Sandbox {
        fn new() -> Self {
            let parent =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/desktop-preference-tests");
            fs::create_dir_all(&parent).unwrap();
            let root = parent.canonicalize().unwrap().join(format!(
                "{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fs::read_dir(root)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                (path.file_name().unwrap().into(), fs::read(path).unwrap())
            })
            .collect()
    }

    #[test]
    fn missing_settings_use_safe_default_without_writing_and_valid_choice_persists() {
        let sandbox = Sandbox::new();
        let sentinel = sandbox.0.join("existing.txt");
        fs::write(&sentinel, b"preserve application content").unwrap();
        let initial = snapshot(&sandbox.0);
        let state = PreferencesState::load(sandbox.0.clone());
        assert_eq!(state.get().unwrap(), "hide_to_tray");
        assert_eq!(state.effective(), CloseBehavior::HideToTray);
        assert_eq!(snapshot(&sandbox.0), initial);
        assert_eq!(state.set("exit").unwrap(), "exit");
        assert_eq!(
            PreferencesState::load(sandbox.0.clone()).get().unwrap(),
            "exit"
        );
        assert_eq!(
            fs::read(&sentinel).unwrap(),
            b"preserve application content"
        );
        assert_eq!(fs::read_dir(&sandbox.0).unwrap().count(), 2);
    }

    #[test]
    fn saving_replaces_an_existing_settings_file_and_does_not_leave_temporary_files() {
        let sandbox = Sandbox::new();
        let state = PreferencesState::load(sandbox.0.clone());
        state.set("exit").unwrap();
        let original = fs::read(sandbox.0.join(FILE_NAME)).unwrap();
        state.set("hide_to_tray").unwrap();
        assert_ne!(fs::read(sandbox.0.join(FILE_NAME)).unwrap(), original);
        assert_eq!(state.get().unwrap(), "hide_to_tray");
        assert_eq!(
            PreferencesState::load(sandbox.0.clone()).get().unwrap(),
            "hide_to_tray"
        );
        assert_eq!(fs::read_dir(&sandbox.0).unwrap().count(), 1);
    }

    #[test]
    fn invalid_values_do_not_change_memory_or_saved_bytes() {
        let sandbox = Sandbox::new();
        let state = PreferencesState::load(sandbox.0.clone());
        state.set("exit").unwrap();
        let original = snapshot(&sandbox.0);
        for value in ["", "EXIT", "minimize", "hide", "exit; whoami"] {
            assert!(
                state
                    .set(value)
                    .unwrap_err()
                    .starts_with("DESKTOP_SETTINGS_INVALID_VALUE")
            );
            assert_eq!(state.get().unwrap(), "exit");
            assert_eq!(snapshot(&sandbox.0), original);
        }
    }

    #[cfg(windows)]
    #[test]
    fn reload_recovers_after_a_windows_read_lock_blocks_initial_loading() {
        use std::os::windows::fs::OpenOptionsExt;

        let sandbox = Sandbox::new();
        let path = sandbox.0.join(FILE_NAME);
        let bytes = br#"{"schema_version":1,"close_behavior":"exit"}"#;
        fs::write(&path, bytes).unwrap();
        let deny_read = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        let state = PreferencesState::load(sandbox.0.clone());
        let initial_error = state.get().unwrap_err();
        assert!(initial_error.starts_with("DESKTOP_SETTINGS_IO_ERROR"));
        assert_eq!(state.effective(), CloseBehavior::HideToTray);
        assert!(
            state
                .reload()
                .unwrap_err()
                .starts_with("DESKTOP_SETTINGS_IO_ERROR")
        );

        drop(deny_read);
        assert_eq!(state.get().unwrap_err(), initial_error);
        assert_eq!(state.reload().unwrap(), "exit");
        assert_eq!(state.get().unwrap(), "exit");
        assert_eq!(state.effective(), CloseBehavior::Exit);
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert_eq!(fs::read_dir(&sandbox.0).unwrap().count(), 1);
    }

    #[test]
    fn reload_rejects_corruption_and_preserves_the_previous_effective_choice() {
        let sandbox = Sandbox::new();
        let path = sandbox.0.join(FILE_NAME);
        fs::write(&path, br#"{"schema_version":1,"close_behavior":"exit"}"#).unwrap();
        let state = PreferencesState::load(sandbox.0.clone());
        assert_eq!(state.get().unwrap(), "exit");
        let corrupt = b"unknown settings must remain unchanged";
        fs::write(&path, corrupt).unwrap();

        let error = state.reload().unwrap_err();
        assert!(error.starts_with("DESKTOP_SETTINGS_INVALID_FILE"));
        assert_eq!(state.get().unwrap(), "exit");
        assert_eq!(state.effective(), CloseBehavior::Exit);
        assert_eq!(fs::read(&path).unwrap(), corrupt);
        assert!(
            state
                .set("hide_to_tray")
                .unwrap_err()
                .starts_with("DESKTOP_SETTINGS_CONFLICT")
        );
        assert_eq!(fs::read(path).unwrap(), corrupt);
        assert_eq!(fs::read_dir(&sandbox.0).unwrap().count(), 1);
    }

    #[test]
    fn reload_updates_a_cached_choice_saved_by_an_independent_state_without_writing() {
        let sandbox = Sandbox::new();
        let path = sandbox.0.join(FILE_NAME);
        let old = PreferencesState::load(sandbox.0.clone());
        assert_eq!(old.get().unwrap(), "hide_to_tray");
        assert_eq!(old.effective(), CloseBehavior::HideToTray);
        assert!(!path.exists());
        let independent = PreferencesState::load(sandbox.0.clone());

        independent.set("exit").unwrap();
        let saved_exit = fs::read(&path).unwrap();
        assert_eq!(old.get().unwrap(), "hide_to_tray");
        assert_eq!(old.reload().unwrap(), "exit");
        assert_eq!(old.effective(), CloseBehavior::Exit);
        assert_eq!(fs::read(&path).unwrap(), saved_exit);

        independent.set("hide_to_tray").unwrap();
        let saved_hide = fs::read(&path).unwrap();
        assert_ne!(saved_hide, saved_exit);
        assert_eq!(old.get().unwrap(), "exit");
        assert_eq!(old.reload().unwrap(), "hide_to_tray");
        assert_eq!(old.effective(), CloseBehavior::HideToTray);
        assert_eq!(fs::read(path).unwrap(), saved_hide);
        assert_eq!(fs::read_dir(&sandbox.0).unwrap().count(), 1);
    }

    #[test]
    fn corrupt_unsupported_and_unknown_settings_are_preserved_with_safe_close_fallback() {
        let sandbox = Sandbox::new();
        for bytes in [
            b"not json".as_slice(),
            br#"{"schema_version":2,"close_behavior":"exit"}"#,
            br#"{"schema_version":1,"close_behavior":"invalid"}"#,
            br#"{"schema_version":1,"close_behavior":"exit","unexpected":true}"#,
            br#"{"schema_version":1}"#,
        ] {
            fs::write(sandbox.0.join(FILE_NAME), bytes).unwrap();
            let state = PreferencesState::load(sandbox.0.clone());
            assert!(state.get().is_err());
            assert!(state.set("exit").is_err());
            assert_eq!(state.effective(), CloseBehavior::HideToTray);
            assert_eq!(fs::read(sandbox.0.join(FILE_NAME)).unwrap(), bytes);
            assert_eq!(fs::read_dir(&sandbox.0).unwrap().count(), 1);
        }
    }

    #[test]
    fn external_edits_deletions_and_same_byte_replacements_are_not_overwritten() {
        let sandbox = Sandbox::new();
        for change in ["edit", "delete", "replace"] {
            let root = sandbox.0.join(change);
            fs::create_dir(&root).unwrap();
            let state = PreferencesState::load(root.clone());
            state.set("exit").unwrap();
            let path = root.join(FILE_NAME);
            let original = fs::read(&path).unwrap();
            match change {
                "edit" => fs::write(&path, b"preserve modified settings").unwrap(),
                "delete" => fs::remove_file(&path).unwrap(),
                "replace" => {
                    fs::remove_file(&path).unwrap();
                    fs::write(&path, original).unwrap();
                }
                _ => unreachable!(),
            }
            let before = snapshot(&root);
            assert!(
                state
                    .set("hide_to_tray")
                    .unwrap_err()
                    .starts_with("DESKTOP_SETTINGS_CONFLICT")
            );
            assert_eq!(state.get().unwrap(), "exit");
            assert_eq!(snapshot(&root), before);
        }
    }

    #[test]
    fn directory_collision_and_oversized_settings_are_preserved() {
        let sandbox = Sandbox::new();
        let collision = sandbox.0.join(FILE_NAME);
        fs::create_dir(&collision).unwrap();
        fs::write(collision.join("human.txt"), b"preserve").unwrap();
        let state = PreferencesState::load(sandbox.0.clone());
        assert!(
            state
                .get()
                .unwrap_err()
                .starts_with("DESKTOP_SETTINGS_UNSAFE_PATH")
        );
        assert!(state.set("exit").is_err());
        assert_eq!(fs::read(collision.join("human.txt")).unwrap(), b"preserve");
        fs::remove_file(collision.join("human.txt")).unwrap();
        fs::remove_dir(collision).unwrap();
        let large = vec![b'x'; (MAX_BYTES + 1) as usize];
        fs::write(sandbox.0.join(FILE_NAME), &large).unwrap();
        let state = PreferencesState::load(sandbox.0.clone());
        assert!(
            state
                .get()
                .unwrap_err()
                .starts_with("DESKTOP_SETTINGS_INVALID_FILE")
        );
        assert!(state.set("exit").is_err());
        assert_eq!(fs::read(sandbox.0.join(FILE_NAME)).unwrap(), large);
    }

    #[cfg(windows)]
    #[test]
    fn actual_windows_replace_failure_keeps_old_file_and_effective_choice() {
        use std::os::windows::fs::OpenOptionsExt;
        let sandbox = Sandbox::new();
        let state = PreferencesState::load(sandbox.0.clone());
        state.set("exit").unwrap();
        let path = sandbox.0.join(FILE_NAME);
        let original = snapshot(&sandbox.0);
        let deny_delete = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        let error = state.set("hide_to_tray").unwrap_err();
        assert!(error.starts_with("DESKTOP_SETTINGS_IO_ERROR"), "{error}");
        assert_eq!(state.get().unwrap(), "exit");
        assert_eq!(state.effective(), CloseBehavior::Exit);
        assert_eq!(snapshot(&sandbox.0), original);
        drop(deny_delete);
        state.set("hide_to_tray").unwrap();
        assert_eq!(
            PreferencesState::load(sandbox.0.clone()).get().unwrap(),
            "hide_to_tray"
        );
    }

    #[cfg(windows)]
    #[test]
    fn junction_settings_directory_is_rejected_without_touching_its_target() {
        let sandbox = Sandbox::new();
        let target = sandbox.0.join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("sentinel.txt"), b"outside settings").unwrap();
        let junction = sandbox.0.join("junction");
        let output = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let before = snapshot(&target);
        let state = PreferencesState::load(junction.clone());
        assert!(
            state
                .get()
                .unwrap_err()
                .starts_with("DESKTOP_SETTINGS_UNSAFE_PATH")
        );
        assert!(state.set("exit").is_err());
        assert_eq!(snapshot(&target), before);
        fs::remove_dir(junction).unwrap();
    }
}
