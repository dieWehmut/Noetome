//! Retained original file and directory capabilities for the selected API inputs.

use super::{ArtifactFingerprint, ModelError};
use crate::{project, sidecar};
use same_file::Handle;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

fn require(value: bool, code: &'static str) -> Result<(), ModelError> {
    if value { Ok(()) } else { Err(code.into()) }
}
pub(super) fn path(path: &Path) -> Result<PathBuf, ModelError> {
    #[cfg(windows)]
    require(
        path.components().all(|part| match part {
            std::path::Component::Normal(name) => name
                .to_str()
                .is_some_and(|text| !text.contains(['/', '\\'])),
            _ => true,
        }),
        "MODEL_API_PATH_INVALID",
    )?;
    sidecar::checked_path(path).map_err(|_| "MODEL_API_PATH_INVALID".into())
}

pub(super) struct Directory {
    path: PathBuf,
    file: File,
    identity: Handle,
}
impl Directory {
    fn open(path: &Path) -> Result<Self, ModelError> {
        let path = self::path(path)?;
        let meta =
            fs::symlink_metadata(&path).map_err(|_| ModelError::from("MODEL_API_PATH_INVALID"))?;
        require(
            meta.is_dir() && !project::is_link(&meta),
            "MODEL_API_PATH_INVALID",
        )?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(3).custom_flags(0x02200000);
        }
        let file = options
            .open(&path)
            .map_err(|_| ModelError::from("MODEL_API_PATH_INVALID"))?;
        let identity = Handle::from_file(
            file.try_clone()
                .map_err(|_| ModelError::from("MODEL_API_PATH_INVALID"))?,
        )
        .map_err(|_| ModelError::from("MODEL_API_PATH_INVALID"))?;
        let result = Self {
            path,
            file,
            identity,
        };
        result.verify()?;
        Ok(result)
    }
    pub(super) fn verify(&self) -> Result<(), ModelError> {
        let meta = self
            .file
            .metadata()
            .map_err(|_| ModelError::from("MODEL_API_INPUT_CHANGED"))?;
        require(
            meta.is_dir() && !project::is_link(&meta),
            "MODEL_API_INPUT_CHANGED",
        )?;
        sidecar::ensure_identity(&self.path, &self.identity, "MODEL_API_INPUT_CHANGED")
            .map_err(|_| "MODEL_API_INPUT_CHANGED".into())
    }
}
pub(super) fn chain(path: &Path) -> Result<Vec<Directory>, ModelError> {
    let mut paths: Vec<_> = path.ancestors().collect();
    paths.reverse();
    paths.into_iter().map(Directory::open).collect()
}
pub(super) fn verify_chain(dirs: &[Directory]) -> Result<(), ModelError> {
    for dir in dirs {
        dir.verify()?;
    }
    Ok(())
}

pub(super) struct Input {
    pub(super) path: PathBuf,
    pub(super) bytes: Vec<u8>,
    pub(super) fingerprint: ArtifactFingerprint,
    file: File,
    identity: Handle,
    dirs: Vec<Directory>,
}
fn read(file: &File, limit: u64) -> Result<Vec<u8>, ModelError> {
    let length = file
        .metadata()
        .map_err(|_| ModelError::from("MODEL_API_INPUT_CHANGED"))?
        .len();
    require(length <= limit, "MODEL_API_INPUT_TOO_LARGE")?;
    let mut reader = file
        .try_clone()
        .map_err(|_| ModelError::from("MODEL_API_INPUT_CHANGED"))?;
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|_| ModelError::from("MODEL_API_INPUT_CHANGED"))?;
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ModelError::from("MODEL_API_INPUT_CHANGED"))?;
    require(
        bytes.len() as u64 == length && bytes.len() as u64 <= limit,
        "MODEL_API_INPUT_CHANGED",
    )?;
    Ok(bytes)
}
fn links(file: &File) -> Result<(), ModelError> {
    require(
        noetome_sidecar_platform::file_link_count(file)
            .map_err(|_| ModelError::from("MODEL_API_INPUT_CHANGED"))?
            == 1,
        "MODEL_API_INPUT_CHANGED",
    )
}
impl Input {
    pub(super) fn open(path: &Path, limit: u64) -> Result<Self, ModelError> {
        let path = self::path(path)?;
        let dirs = chain(
            path.parent()
                .ok_or(ModelError::from("MODEL_API_PATH_INVALID"))?,
        )?;
        let before =
            project::checked_file(&path).map_err(|_| ModelError::from("MODEL_API_PATH_INVALID"))?;
        require(before.len() <= limit, "MODEL_API_INPUT_TOO_LARGE")?;
        let file =
            project::read_guard(&path).map_err(|_| ModelError::from("MODEL_API_INPUT_CHANGED"))?;
        links(&file)?;
        let identity = Handle::from_file(
            file.try_clone()
                .map_err(|_| ModelError::from("MODEL_API_INPUT_CHANGED"))?,
        )
        .map_err(|_| ModelError::from("MODEL_API_INPUT_CHANGED"))?;
        let bytes = read(&file, limit)?;
        require(
            bytes.len() as u64 == before.len(),
            "MODEL_API_INPUT_CHANGED",
        )?;
        let fingerprint = ArtifactFingerprint {
            sha256: sidecar::digest(&bytes),
            byte_length: bytes.len() as u64,
        };
        let input = Self {
            path,
            bytes,
            fingerprint,
            file,
            identity,
            dirs,
        };
        input.verify()?;
        Ok(input)
    }
    pub(super) fn verify(&self) -> Result<(), ModelError> {
        verify_chain(&self.dirs)?;
        sidecar::ensure_identity(&self.path, &self.identity, "MODEL_API_INPUT_CHANGED")
            .map_err(|_| ModelError::from("MODEL_API_INPUT_CHANGED"))?;
        links(&self.file)?;
        let bytes = read(&self.file, self.fingerprint.byte_length)?;
        require(
            sidecar::digest(&bytes) == self.fingerprint.sha256,
            "MODEL_API_INPUT_CHANGED",
        )?;
        links(&self.file)?;
        verify_chain(&self.dirs)
    }
}
