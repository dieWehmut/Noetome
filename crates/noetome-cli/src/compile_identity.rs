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
