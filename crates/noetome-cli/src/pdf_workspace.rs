//! Exclusive outer sandbox and immutable report guards; no recursive cleanup.
use crate::{
    project,
    sidecar_model::{Limits, SidecarError},
    sidecar_process::{Control, RunSession, ValidatedResponse},
    sidecar_workspace::{Directory, GuardedFile},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const REPORT_LIMIT: u64 = 16 * 1024 * 1024;
pub(crate) enum ParserOwner<'a> {
    Validated(&'a ValidatedResponse),
    Failed(&'a RunSession),
}
impl ParserOwner<'_> {
    fn verify(&self, control: &Control) -> Result<(), SidecarError> {
        match self {
            Self::Validated(response) => response.recheck(control),
            Self::Failed(session) => session.verify(control),
        }
    }
}
/// Byte receipt and live capabilities acquired before the original parse
/// owners are dropped. Writer keeps these guards until its final owned cleanup.
pub(crate) struct CapturedTree {
    pub(crate) root: PathBuf,
    pub(crate) files: BTreeMap<String, Vec<u8>>,
    directories: BTreeMap<String, Directory>,
    file_guards: BTreeMap<String, GuardedFile>,
}
impl CapturedTree {
    pub(crate) fn prune_empty_native_directories(
        &mut self,
        control: &Control,
    ) -> Result<(), SidecarError> {
        self.verify(control)?;
        for name in ["parser/pages", "parser"] {
            let prefix = format!("{name}/");
            if self.directories.contains_key(name)
                && !self
                    .files
                    .keys()
                    .chain(self.directories.keys())
                    .any(|path| path.starts_with(&prefix))
            {
                self.verify(control)?;
                self.directories
                    .remove(name)
                    .expect("known captured directory")
                    .remove_empty("PDF_CAPTURE")?;
                self.verify(control)?;
            }
        }
        self.verify(control)
    }
    pub(crate) fn directory_paths(&self) -> Vec<PathBuf> {
        self.directories
            .keys()
            .map(|name| self.root.join(name))
            .collect()
    }
    pub(crate) fn verify(&self, control: &Control) -> Result<(), SidecarError> {
        control.check()?;
        for (relative, directory) in &self.directories {
            directory.verify("PDF_CAPTURE")?;
            let parent = self.root.join(relative);
            let mut expected = BTreeSet::new();
            for name in self.files.keys().chain(self.directories.keys()) {
                let path = self.root.join(name);
                if path.parent() == Some(parent.as_path()) {
                    expected.insert(path.file_name().unwrap().to_owned());
                }
            }
            let actual = fs::read_dir(&parent)
                .map_err(|e| SidecarError::new("PDF_CAPTURE", e))?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<Result<BTreeSet<_>, _>>()
                .map_err(|e| SidecarError::new("PDF_CAPTURE", e))?;
            if actual != expected {
                return Err(SidecarError::new(
                    "PDF_CAPTURE",
                    "captured tree inventory changed",
                ));
            }
        }
        for guard in self.file_guards.values() {
            guard.verify(control, "PDF_CAPTURE")?;
        }
        control.check()
    }
}
pub(crate) struct Workspace {
    pub root: PathBuf,
    root_guard: Directory,
    parent_guard: Directory,
}
impl Workspace {
    pub(crate) fn capture(
        &self,
        owner: Option<ParserOwner<'_>>,
        complete: bool,
        control: &Control,
    ) -> Result<CapturedTree, SidecarError> {
        self.verify(control)?;
        if let Some(owner) = &owner {
            owner.verify(control)?;
        }
        let mut tree = CapturedTree {
            root: self.root.clone(),
            files: BTreeMap::new(),
            directories: BTreeMap::new(),
            file_guards: BTreeMap::new(),
        };
        tree.directories.insert(
            String::new(),
            Directory::capture(&self.root, "PDF_CAPTURE")?,
        );
        let mut stack = vec![String::new()];
        let mut total = 0u64;
        while let Some(relative) = stack.pop() {
            for entry in fs::read_dir(self.root.join(&relative))
                .map_err(|e| SidecarError::new("PDF_CAPTURE", e))?
            {
                control.check()?;
                let entry = entry.map_err(|e| SidecarError::new("PDF_CAPTURE", e))?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| SidecarError::new("PDF_CAPTURE", "non-UTF8 parse entry"))?;
                let name = if relative.is_empty() {
                    name
                } else {
                    format!("{relative}/{name}")
                };
                let meta = fs::symlink_metadata(entry.path())
                    .map_err(|e| SidecarError::new("PDF_CAPTURE", e))?;
                if project::is_link(&meta) {
                    return Err(SidecarError::new("PDF_CAPTURE", "linked parse entry"));
                }
                if meta.is_dir() {
                    if owner.is_none() || !matches!(name.as_str(), "parser" | "parser/pages") {
                        return Err(SidecarError::new(
                            "PDF_CAPTURE",
                            "unowned or unknown parse directory",
                        ));
                    }
                    tree.directories.insert(
                        name.clone(),
                        Directory::capture(&entry.path(), "PDF_CAPTURE")?,
                    );
                    stack.push(name);
                } else if meta.is_file() {
                    let parser_file =
                        matches!(name.as_str(), "parser/raw.pdf" | "parser/pdf-manifest.json")
                            || name
                                .strip_prefix("parser/pages/")
                                .and_then(|name| name.strip_suffix(".txt"))
                                .is_some_and(|index| {
                                    index.len() == 6
                                        && index.bytes().all(|byte| byte.is_ascii_digit())
                                        && index
                                            .parse::<usize>()
                                            .is_ok_and(|index| index < crate::pdf_model::MAX_PAGES)
                                });
                    let report_file = if complete {
                        matches!(name.as_str(), "parse-result.json" | "parse-result.md")
                    } else {
                        name == "failure.json"
                    };
                    if (!parser_file || owner.is_none()) && !report_file {
                        return Err(SidecarError::new(
                            "PDF_CAPTURE",
                            "unknown parse file; ownership not adopted",
                        ));
                    }
                    // JSON control escaping can turn 64KiB of legitimate
                    // diagnostics into384KiB; include the bounded error text.
                    let max = if name == "failure.json" {
                        512 * 1024
                    } else {
                        Limits::default().resource_bytes
                    };
                    let (guard, bytes) =
                        GuardedFile::open(&entry.path(), max, control, "PDF_CAPTURE")?;
                    total = total.checked_add(bytes.len() as u64).ok_or_else(|| {
                        SidecarError::new("SIDECAR_LIMIT", "parse capture overflow")
                    })?;
                    if total > 96 * 1024 * 1024
                        || tree.files.len() >= Limits::default().resources as usize + 3
                    {
                        return Err(SidecarError::new("SIDECAR_LIMIT", "parse capture limit"));
                    }
                    tree.files.insert(name.clone(), bytes);
                    tree.file_guards.insert(name, guard);
                } else {
                    return Err(SidecarError::new("PDF_CAPTURE", "non-regular parse entry"));
                }
            }
        }
        if complete {
            self.complete_layout(control)?;
        } else if !tree.files.contains_key("failure.json") {
            return Err(SidecarError::new(
                "PDF_CAPTURE",
                "missing owned failure record",
            ));
        }
        tree.verify(control)?;
        if let Some(owner) = &owner {
            owner.verify(control)?;
        }
        self.verify(control)?;
        Ok(tree)
    }
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
