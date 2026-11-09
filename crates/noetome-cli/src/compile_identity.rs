//! Persistent identity of an actual ordinary file/directory handle.

use std::fs::OpenOptions;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::project::{self, ProjectError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "platform", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Identity {
    Unix { device: u64, inode: u64 },
    Windows { volume: u64, index: u64 },
}

pub(crate) fn identity(path: &Path) -> Result<Identity, ProjectError> {
    project::check_components(path)?;
    let before = std::fs::symlink_metadata(path).map_err(|e| ProjectError::io(path, e))?;
    if project::is_link(&before) || !(before.is_file() || before.is_dir()) {
        return Err(ProjectError::unsafe_path(path));
    }
    let initial = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x02200000);
    }
    let file = options.open(path).map_err(|e| ProjectError::io(path, e))?;
    let metadata = file.metadata().map_err(|e| ProjectError::io(path, e))?;
    if project::is_link(&metadata) || metadata.is_dir() != before.is_dir() {
        return Err(ProjectError::unsafe_path(path));
    }
    let opened =
        same_file::Handle::from_file(file.try_clone().map_err(|e| ProjectError::io(path, e))?)
            .map_err(|e| ProjectError::io(path, e))?;
    let current = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
    project::check_components(path)?;
    if initial != opened || current != opened {
        return Err(ProjectError::new(
            "COMPILE_INTEGRITY",
            3,
            "读取持久身份期间文件或目录被替换；已保留现场。",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let after = file.metadata().map_err(|e| ProjectError::io(path, e))?;
        if after.is_file() && after.nlink() != 1 {
            return Err(ProjectError::new(
                "COMPILE_INTEGRITY",
                3,
                "拥有的普通文件必须只有一个硬链接；已保留现场。",
            ));
        }
        Ok(Identity::Unix {
            device: after.dev(),
            inode: after.ino(),
        })
    }
    #[cfg(windows)]
    {
        let information =
            winapi_util::file::information(&file).map_err(|e| ProjectError::io(path, e))?;
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

/// Close a completed writer and retain the same file as a strict read capability.
/// The independent, read-only identity receipt bridges the close/reopen interval.
pub(crate) fn read_after_write(
    path: &Path,
    writer: std::fs::File,
) -> Result<std::fs::File, ProjectError> {
    read_after_write_inner(path, writer, || {})
}

fn read_after_write_inner(
    path: &Path,
    writer: std::fs::File,
    after_close: impl FnOnce(),
) -> Result<std::fs::File, ProjectError> {
    let changed = || {
        ProjectError::new(
            "COMPILE_INTEGRITY",
            3,
            "写入后读回期间文件身份变化；已保留现场。",
        )
    };
    project::check_components(path)?;
    project::checked_file(path)?;
    let receipt = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
    let written =
        same_file::Handle::from_file(writer.try_clone().map_err(|e| ProjectError::io(path, e))?)
            .map_err(|e| ProjectError::io(path, e))?;
    if written != receipt {
        return Err(changed());
    }
    crate::sidecar_workspace::check_links(&writer, "COMPILE_INTEGRITY").map_err(|_| changed())?;
    // A clone of the writer still holds WRITE access on Windows. Release both
    // writing handles before opening the deny-write/deny-delete read guard.
    drop(written);
    drop(writer);
    after_close();
    let reader = project::read_guard(path)?;
    let opened =
        same_file::Handle::from_file(reader.try_clone().map_err(|e| ProjectError::io(path, e))?)
            .map_err(|e| ProjectError::io(path, e))?;
    let current = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
    project::check_components(path)?;
    project::checked_file(path)?;
    crate::sidecar_workspace::check_links(&reader, "COMPILE_INTEGRITY").map_err(|_| changed())?;
    if opened != receipt || current != receipt {
        return Err(changed());
    }
    Ok(reader)
}

#[cfg(test)]
mod write_read_tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("noetome-write-read-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn path(&self) -> PathBuf {
            self.0.join("笔记 with spaces.md")
        }
        fn writer(&self) -> fs::File {
            let mut writer = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.path())
                .unwrap();
            writer.write_all(b"identical saved bytes\n").unwrap();
            writer.sync_all().unwrap();
            writer
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn completed_writer_transitions_to_the_same_strict_read_capability() {
        let fixture = Fixture::new();
        let writer = fixture.writer();
        let path = fixture.path();
        let reader = read_after_write(&path, writer).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"identical saved bytes\n");
        #[cfg(windows)]
        {
            assert_eq!(
                OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap_err()
                    .raw_os_error(),
                Some(32)
            );
            assert_eq!(fs::remove_file(&path).unwrap_err().raw_os_error(), Some(32));
        }
        drop(reader);
        OpenOptions::new().write(true).open(path).unwrap();
    }

    #[test]
    fn identical_bytes_replaced_between_writer_close_and_reader_open_are_refused() {
        let fixture = Fixture::new();
        let writer = fixture.writer();
        let path = fixture.path();
        let replacement = fixture.0.join("replacement.md");
        fs::write(&replacement, b"identical saved bytes\n").unwrap();
        let error = read_after_write_inner(&path, writer, || {
            fs::rename(&path, fixture.0.join("original.md")).unwrap();
            fs::rename(&replacement, &path).unwrap();
        })
        .unwrap_err();
        assert_eq!(error.code(), "COMPILE_INTEGRITY");
        assert_eq!(fs::read(&path).unwrap(), b"identical saved bytes\n");
        assert_eq!(
            fs::read(fixture.0.join("original.md")).unwrap(),
            b"identical saved bytes\n"
        );
    }
}
