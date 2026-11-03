//! Capability-held artifacts in a brand-new, explicitly selected test root.
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::project::{self, ProjectError};
use crate::sidecar_model::Fingerprint;
use crate::sidecar_workspace::{Directory, check_links};
use crate::text::sha256;

const FILE_LIMIT: usize = 8 * 1024 * 1024;

fn changed() -> ProjectError {
    ProjectError::new(
        "EXTRACT_OUTPUT_CHANGED",
        3,
        "输出目录或已持有文件发生变化；保留未知现场，没有验收候选。",
    )
}

struct Artifact {
    path: PathBuf,
    file: File,
    identity: same_file::Handle,
    modified: SystemTime,
    fingerprint: Fingerprint,
}

impl Artifact {
    fn create(path: &Path, bytes: &[u8]) -> Result<Self, ProjectError> {
        if bytes.len() > FILE_LIMIT {
            return Err(ProjectError::new(
                "EXTRACT_SAVE_LIMIT",
                3,
                "单份审计文件超过8 MiB本地边界。",
            ));
        }
        project::check_components(path)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1).custom_flags(0x00200000);
        }
        let mut file = options.open(path).map_err(|e| ProjectError::io(path, e))?;
        let identity =
            same_file::Handle::from_file(file.try_clone().map_err(|e| ProjectError::io(path, e))?)
                .map_err(|e| ProjectError::io(path, e))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| ProjectError::io(path, e))?;
        let modified = file
            .metadata()
            .and_then(|m| m.modified())
            .map_err(|e| ProjectError::io(path, e))?;
        let value = Self {
            path: path.to_path_buf(),
            file,
            identity,
            modified,
            fingerprint: Fingerprint {
                sha256: sha256(bytes),
                byte_length: bytes.len() as u64,
            },
        };
        value.verify()?;
        Ok(value)
    }

    fn read(&self) -> Result<Vec<u8>, ProjectError> {
        self.read_with_links(false)
    }

    fn read_with_links(&self, owned_cleanup: bool) -> Result<Vec<u8>, ProjectError> {
        project::check_components(&self.path).map_err(|_| changed())?;
        project::checked_file(&self.path).map_err(|_| changed())?;
        if !owned_cleanup {
            check_links(&self.file, "EXTRACT_OUTPUT_CHANGED").map_err(|_| changed())?;
        }
        if same_file::Handle::from_path(&self.path).map_err(|_| changed())? != self.identity {
            return Err(changed());
        }
        let mut reader = self.file.try_clone().map_err(|_| changed())?;
        reader.seek(SeekFrom::Start(0)).map_err(|_| changed())?;
        let mut bytes = Vec::new();
        reader
            .take(FILE_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| changed())?;
        let metadata = self.file.metadata().map_err(|_| changed())?;
        if bytes.len() as u64 != self.fingerprint.byte_length
            || sha256(&bytes) != self.fingerprint.sha256
            || metadata.modified().map_err(|_| changed())? != self.modified
            || same_file::Handle::from_path(&self.path).map_err(|_| changed())? != self.identity
        {
            return Err(changed());
        }
        Ok(bytes)
    }

    fn verify(&self) -> Result<(), ProjectError> {
        self.read().map(|_| ())
    }

    fn remove(self) -> Result<(), ProjectError> {
        self.remove_with_links(false)
    }

    fn remove_with_links(self, owned_cleanup: bool) -> Result<(), ProjectError> {
        // A newly added hardlink invalidates the run but must not prevent
        // removing this captured pathname. Preserve the new alias; still
        // prove original bytes, timestamp, parent and file identity here.
        self.read_with_links(owned_cleanup)?;
        // Keep a path identity receipt while releasing only our deny-write/
        // deny-delete aliases. Parent directory capability remains alive.
        let receipt = same_file::Handle::from_path(&self.path).map_err(|_| changed())?;
        let Self {
            path,
            file,
            identity,
            ..
        } = self;
        drop(file);
        drop(identity);
        project::check_components(&path).map_err(|_| changed())?;
        project::checked_file(&path).map_err(|_| changed())?;
        if same_file::Handle::from_path(&path).map_err(|_| changed())? != receipt {
            return Err(changed());
        }
        fs::remove_file(&path).map_err(|_| changed())
    }
}

pub(crate) struct OutputRoot {
    root: PathBuf,
    parent: Directory,
    directory: Directory,
    files: BTreeMap<String, Artifact>,
}

impl OutputRoot {
    pub(crate) fn create(path: &Path, request_id: &str) -> Result<Self, ProjectError> {
        Self::create_owned(path, request_id, None)
    }

    pub(crate) fn create_api(
        path: &Path,
        request_id: &str,
        transport_mode: &str,
    ) -> Result<Self, ProjectError> {
        Self::create_owned(path, request_id, Some(transport_mode))
    }

    pub(crate) fn create_review(path: &Path, request_id: &str) -> Result<Self, ProjectError> {
        Self::create_owned(path, request_id, Some("source_review"))
    }

    fn create_owned(
        path: &Path,
        request_id: &str,
        mode: Option<&str>,
    ) -> Result<Self, ProjectError> {
        let root = project::check_components(path)?;
        if project::exists(&root)? {
            return Err(ProjectError::new(
                "EXTRACT_OUTPUT_EXISTS",
                2,
                "输出根已存在；请选择新的测试目录。",
            ));
        }
        let parent_path = root.parent().ok_or_else(changed)?;
        let parent = Directory::capture(parent_path, "EXTRACT_OUTPUT_CHANGED").map_err(|_| {
            ProjectError::new("EXTRACT_OUTPUT_PARENT", 2, "输出父目录须为已有普通目录。")
        })?;
        fs::create_dir(&root).map_err(|e| ProjectError::io(&root, e))?;
        let directory =
            Directory::capture(&root, "EXTRACT_OUTPUT_CHANGED").map_err(|_| changed())?;
        let mut value = Self {
            root,
            parent,
            directory,
            files: BTreeMap::new(),
        };
        let review = mode == Some("source_review");
        let mut owner = serde_json::json!({"schema_version":1,"producer":if review {"source_review"} else if mode.is_some() {"api"} else {"fake"},"request_id":request_id,"purpose":if review {"unpublished_source_review"} else {"unpublished_extraction_test"}});
        if let Some(mode) = mode.filter(|_| !review) {
            owner["transport_mode"] = serde_json::json!(mode);
        }
        let owner = serde_json::to_vec_pretty(&owner).map_err(|_| changed())?;
        value.add(".owner.json", &owner)?;
        Ok(value)
    }

    pub(crate) fn add(&mut self, name: &str, bytes: &[u8]) -> Result<(), ProjectError> {
        if !crate::fixture_input::valid_relative_path(name)
            || name.contains('/')
            || self.files.contains_key(name)
        {
            return Err(changed());
        }
        self.verify()?;
        let artifact = Artifact::create(&self.root.join(name), bytes)?;
        self.files.insert(name.into(), artifact);
        self.verify()
    }

    pub(crate) fn read(&self, name: &str) -> Result<Vec<u8>, ProjectError> {
        self.files.get(name).ok_or_else(changed)?.read()
    }

    pub(crate) fn verify(&self) -> Result<(), ProjectError> {
        self.parent
            .verify("EXTRACT_OUTPUT_CHANGED")
            .map_err(|_| changed())?;
        self.directory
            .verify("EXTRACT_OUTPUT_CHANGED")
            .map_err(|_| changed())?;
        let actual = fs::read_dir(&self.root)
            .map_err(|_| changed())?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<Result<BTreeSet<_>, _>>()
            .map_err(|_| changed())?;
        let expected = self
            .files
            .keys()
            .map(std::ffi::OsString::from)
            .collect::<BTreeSet<_>>();
        if actual != expected {
            return Err(changed());
        }
        for file in self.files.values() {
            file.verify()?;
        }
        self.directory
            .verify("EXTRACT_OUTPUT_CHANGED")
            .map_err(|_| changed())?;
        Ok(())
    }

    pub(crate) fn fingerprints(&self) -> BTreeMap<String, Fingerprint> {
        self.files
            .iter()
            .map(|(name, file)| (name.clone(), file.fingerprint.clone()))
            .collect()
    }

    pub(crate) fn discard_result(&mut self) -> Result<(), ProjectError> {
        self.discard_names(&["candidates.json", "validation.json", "validation.md"])
    }

    pub(crate) fn discard_review_result(&mut self) -> Result<(), ProjectError> {
        self.discard_names_inner(
            &["knowledge-ir.json", "verification.json", "verification.md"],
            true,
        )
    }

    fn discard_names(&mut self, names: &[&str]) -> Result<(), ProjectError> {
        self.discard_names_inner(names, false)
    }

    fn discard_names_inner(&mut self, names: &[&str], review: bool) -> Result<(), ProjectError> {
        self.parent
            .verify("EXTRACT_OUTPUT_CHANGED")
            .map_err(|_| changed())?;
        self.directory
            .verify("EXTRACT_OUTPUT_CHANGED")
            .map_err(|_| changed())?;
        // Remove only captured result artifacts. Never enumerate/delegate a
        // deletion of unknown files, and never recursively clean a failed run.
        for name in names {
            if let Some(file) = self.files.remove(*name) {
                if review {
                    file.remove_with_links(true)?;
                } else {
                    file.remove()?;
                }
            }
        }
        Ok(())
    }
}

/// Resolve membership by real directory identity (including Windows aliases),
/// rather than case-sensitive string-prefix comparisons.
pub(crate) fn inside(path: &Path, root: &Path) -> Result<bool, ProjectError> {
    let root_identity =
        same_file::Handle::from_path(root).map_err(|e| ProjectError::io(root, e))?;
    let mut cursor = path.parent();
    while let Some(parent) = cursor {
        if project::exists(parent)?
            && same_file::Handle::from_path(parent).map_err(|e| ProjectError::io(parent, e))?
                == root_identity
        {
            return Ok(true);
        }
        cursor = parent.parent();
    }
    Ok(false)
}
