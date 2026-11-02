//! Exclusive outer sandbox and immutable report guards; no recursive cleanup.
use crate::{
    project,
    sidecar_model::SidecarError,
    sidecar_process::Control,
    sidecar_workspace::{Directory, GuardedFile},
};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const REPORT_LIMIT: u64 = 16 * 1024 * 1024;
pub(crate) struct Workspace {
    pub root: PathBuf,
    root_guard: Directory,
    parent_guard: Directory,
}
impl Workspace {
    pub(crate) fn create(root: &Path, control: &Control) -> Result<Self, SidecarError> {
        Self::open(root, control, true)
    }
    pub(crate) fn inspect(root: &Path, control: &Control) -> Result<Self, SidecarError> {
        Self::open(root, control, false)
    }
    fn open(root: &Path, control: &Control, create: bool) -> Result<Self, SidecarError> {
        control.check()?;
        let root = project::check_components(root)
            .map_err(|e| SidecarError::new("SIDECAR_WORKSPACE", e))?;
        let parent = root
            .parent()
            .ok_or_else(|| SidecarError::new("SIDECAR_WORKSPACE", "no sandbox parent"))?;
        let parent_guard = Directory::capture(parent, &"SIDECAR_WORKSPACE")?;
        if create {
            fs::create_dir(&root).map_err(|e| SidecarError::new("SIDECAR_WORKSPACE", e))?;
        }
        let root_guard = Directory::capture(&root, &"SIDECAR_WORKSPACE")?;
        let workspace = Self {
            root,
            root_guard,
            parent_guard,
        };
        workspace.verify(control)?;
        Ok(workspace)
    }
    pub(crate) fn verify(&self, control: &Control) -> Result<(), SidecarError> {
        control.check()?;
        self.parent_guard.verify(&"SIDECAR_WORKSPACE")?;
        self.root_guard.verify(&"SIDECAR_WORKSPACE")
    }
    pub(crate) fn read(
        &self,
        name: &str,
        control: &Control,
    ) -> Result<(GuardedFile, Vec<u8>), SidecarError> {
        self.verify(control)?;
        GuardedFile::open(&self.root.join(name), REPORT_LIMIT, control, &"PDF_REPORT")
    }
    pub(crate) fn write(
        &self,
        name: &str,
        bytes: &[u8],
        control: &Control,
    ) -> Result<(), SidecarError> {
        self.verify(control)?;
        if bytes.len() as u64 > REPORT_LIMIT {
            return Err(SidecarError::new("SIDECAR_LIMIT", "PDF report byte limit"));
        }
        let path = self.root.join(name);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| SidecarError::new("PDF_WRITE_INCOMPLETE", e))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| SidecarError::new("PDF_WRITE_INCOMPLETE", e))?;
        drop(file);
        let (_guard, actual) = self.read(name, control)?;
        if actual != bytes {
            return Err(SidecarError::new(
                "PDF_WRITE_INCOMPLETE",
                "report bytes changed after writing",
            ));
        }
        self.verify(control)
    }
    pub(crate) fn complete_layout(&self, control: &Control) -> Result<(), SidecarError> {
        self.verify(control)?;
        let mut names = BTreeSet::new();
        for entry in
            fs::read_dir(&self.root).map_err(|e| SidecarError::new("SIDECAR_WORKSPACE", e))?
        {
            control.check()?;
            let entry = entry.map_err(|e| SidecarError::new("SIDECAR_WORKSPACE", e))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| SidecarError::new("SIDECAR_WORKSPACE", "non-UTF8 sandbox entry"))?;
            let meta = fs::symlink_metadata(entry.path())
                .map_err(|e| SidecarError::new("SIDECAR_WORKSPACE", e))?;
            if project::is_link(&meta)
                || !names.insert(name.clone())
                || (name == "parser" && !meta.is_dir())
                || (name != "parser" && !meta.is_file())
            {
                return Err(SidecarError::new(
                    "SIDECAR_WORKSPACE",
                    "unsafe sandbox entry",
                ));
            }
        }
        let expected = BTreeSet::from([
            "parser".to_string(),
            "parse-result.json".to_string(),
            "parse-result.md".to_string(),
        ]);
        if names != expected {
            return Err(SidecarError::new(
                "PDF_WORKSPACE_INCOMPLETE",
                "sandbox is not a complete exact report/parse layout",
            ));
        }
        self.verify(control)
    }
}
