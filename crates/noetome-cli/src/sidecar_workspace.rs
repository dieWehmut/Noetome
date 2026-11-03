use super::sidecar_process::Control;
use crate::{fixture_input::valid_relative_path, project, sidecar_model::*};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
fn err(code: &'static str, message: impl ToString) -> SidecarError {
    SidecarError::new(code, message)
}
#[derive(Debug)]
pub(crate) struct Directory {
    path: PathBuf,
    identity: same_file::Handle,
    _guard: Option<File>,
}
impl Directory {
    pub(crate) fn capture(path: &Path, code: &'static str) -> Result<Self, SidecarError> {
        project::check_components(path).map_err(|e| err(code, e))?;
        let meta = fs::symlink_metadata(path).map_err(|e| err(code, e))?;
        if !meta.is_dir() || project::is_link(&meta) {
            return Err(err(code, "not a plain directory"));
        }
        let identity = same_file::Handle::from_path(path).map_err(|e| err(code, e))?;
        #[cfg(windows)]
        let guard = {
            use std::os::windows::fs::OpenOptionsExt;
            let f = OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(0x02200000)
                .open(path)
                .map_err(|e| err(code, e))?;
            let opened = same_file::Handle::from_file(f.try_clone().map_err(|e| err(code, e))?)
                .map_err(|e| err(code, e))?;
            if opened != identity || project::is_link(&f.metadata().map_err(|e| err(code, e))?) {
                return Err(err(code, "directory changed on open"));
            }
            Some(f)
        };
        #[cfg(not(windows))]
        let guard = None;
        Ok(Self {
            path: path.to_owned(),
            identity,
            _guard: guard,
        })
    }
    pub(crate) fn verify(&self, code: &'static str) -> Result<(), SidecarError> {
        project::check_components(&self.path).map_err(|e| err(code, e))?;
        let meta = fs::symlink_metadata(&self.path).map_err(|e| err(code, e))?;
        if !meta.is_dir()
            || project::is_link(&meta)
            || same_file::Handle::from_path(&self.path).map_err(|e| err(code, e))? != self.identity
        {
            return Err(err(code, "directory identity changed"));
        }
        Ok(())
    }
    pub(crate) fn remove_empty(self, code: &'static str) -> Result<(), SidecarError> {
        self.verify(code)?;
        let Self {
            path,
            identity,
            _guard,
        } = self;
        // A read capability deliberately denies deletion. The byte receipt's
        // parent capability remains alive while known owned cleanup releases
        // this one guard and checks the same identity again.
        drop(_guard);
        project::check_components(&path).map_err(|e| err(code, e))?;
        let metadata = fs::symlink_metadata(&path).map_err(|e| err(code, e))?;
        if !metadata.is_dir()
            || project::is_link(&metadata)
            || same_file::Handle::from_path(&path).map_err(|e| err(code, e))? != identity
        {
            return Err(err(code, "owned empty directory changed before cleanup"));
        }
        // Never recurse: an added file or child makes this fail intact.
        fs::remove_dir(&path).map_err(|e| err(code, e))
    }
}
#[derive(Debug)]
pub(crate) struct GuardedFile {
    path: PathBuf,
    file: File,
    identity: same_file::Handle,
    fingerprint: Fingerprint,
}
impl GuardedFile {
    pub(crate) fn open(
        path: &Path,
        max: u64,
        control: &Control,
        code: &'static str,
    ) -> Result<(Self, Vec<u8>), SidecarError> {
        control.check()?;
        project::check_components(path).map_err(|e| err(code, e))?;
        let meta = project::checked_file(path).map_err(|e| err(code, e))?;
        if meta.len() > max {
            return Err(err("SIDECAR_LIMIT", "file byte limit"));
        }
        let mut file = project::read_guard(path).map_err(|e| err(code, e))?;
        check_links(&file, code)?;
        let identity = same_file::Handle::from_file(file.try_clone().map_err(|e| err(code, e))?)
            .map_err(|e| err(code, e))?;
        if identity != same_file::Handle::from_path(path).map_err(|e| err(code, e))? {
            return Err(err(code, "file changed during open"));
        }
        let mut bytes = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            control.check()?;
            let n = file.read(&mut buf).map_err(|e| err(code, e))?;
            if n == 0 {
                break;
            }
            if bytes.len() as u64 + n as u64 > max {
                return Err(err("SIDECAR_LIMIT", "file byte limit"));
            }
            bytes.extend_from_slice(&buf[..n]);
        }
        let fingerprint = Fingerprint {
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            byte_length: bytes.len() as u64,
        };
        let guard = Self {
            path: path.to_owned(),
            file,
            identity,
            fingerprint,
        };
        guard.verify(control, code)?;
        Ok((guard, bytes))
    }
    pub(crate) fn verify(&self, control: &Control, code: &'static str) -> Result<(), SidecarError> {
        control.check()?;
        project::check_components(&self.path).map_err(|e| err(code, e))?;
        project::checked_file(&self.path).map_err(|e| err(code, e))?;
        check_links(&self.file, code)?;
        if same_file::Handle::from_path(&self.path).map_err(|e| err(code, e))? != self.identity {
            return Err(err(code, "file identity changed"));
        }
        let mut f = self.file.try_clone().map_err(|e| err(code, e))?;
        f.seek(SeekFrom::Start(0)).map_err(|e| err(code, e))?;
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut buf = [0u8; 8192];
        loop {
            control.check()?;
            let n = f.read(&mut buf).map_err(|e| err(code, e))?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > self.fingerprint.byte_length {
                return Err(err(code, "file length changed"));
            }
            hash.update(&buf[..n]);
        }
        if count != self.fingerprint.byte_length
            || format!("{:x}", hash.finalize()) != self.fingerprint.sha256
        {
            return Err(err(code, "file bytes changed"));
        }
        Ok(())
    }
}
pub(crate) fn check_links(file: &File, code: &'static str) -> Result<(), SidecarError> {
    #[cfg(windows)]
    let count = winapi_util::file::information(file)
        .map_err(|e| err(code, e))?
        .number_of_links() as u64;
    #[cfg(unix)]
    let count = {
        use std::os::unix::fs::MetadataExt;
        file.metadata().map_err(|e| err(code, e))?.nlink()
    };
    if count != 1 {
        return Err(err(code, "multiple hard links rejected"));
    }
    Ok(())
}
#[derive(Debug)]
pub(crate) struct Prepared {
    input: GuardedFile,
    input_parent: Directory,
    root: Directory,
    parent: Directory,
    pub(crate) request: Request,
}
impl Prepared {
    pub(crate) fn new(
        input: &Path,
        workspace: &Path,
        limits: Limits,
        control: &Control,
    ) -> Result<Self, SidecarError> {
        Self::prepare(input, workspace, limits, control, true)
    }
    /// Internal read-only recheck of a persisted PDF resource directory.
    /// Never sent as a new process request or exposed as a public fake entry.
    pub(crate) fn existing(
        input: &Path,
        workspace: &Path,
        limits: Limits,
        control: &Control,
    ) -> Result<Self, SidecarError> {
        Self::prepare(input, workspace, limits, control, false)
    }
    fn prepare(
        input: &Path,
        workspace: &Path,
        limits: Limits,
        control: &Control,
        create: bool,
    ) -> Result<Self, SidecarError> {
        let input = project::check_components(input).map_err(|e| err("SIDECAR_INPUT", e))?;
        let workspace =
            project::check_components(workspace).map_err(|e| err("SIDECAR_WORKSPACE", e))?;
        let input_parent = Directory::capture(
            input
                .parent()
                .ok_or_else(|| err("SIDECAR_INPUT", "no parent"))?,
            &"SIDECAR_INPUT",
        )?;
        let (input, _) = GuardedFile::open(&input, limits.input_bytes, control, &"SIDECAR_INPUT")?;
        let parent = Directory::capture(
            workspace
                .parent()
                .ok_or_else(|| err("SIDECAR_WORKSPACE", "no parent"))?,
            &"SIDECAR_WORKSPACE",
        )?;
        control.check()?;
        if create {
            fs::create_dir(&workspace).map_err(|e| err("SIDECAR_WORKSPACE", e))?;
        }
        let root = Directory::capture(&workspace, &"SIDECAR_WORKSPACE")?;
        let request = Request {
            protocol_version: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
            input: Input {
                path: input
                    .path
                    .to_str()
                    .ok_or_else(|| err("SIDECAR_INPUT", "non UTF8 path"))?
                    .to_owned(),
                access: InputAccess::ReadOnlySingleFile,
                fingerprint: input.fingerprint.clone(),
            },
            workspace: Workspace {
                path: workspace
                    .to_str()
                    .ok_or_else(|| err("SIDECAR_WORKSPACE", "non UTF8 path"))?
                    .to_owned(),
                capability_id: uuid::Uuid::new_v4().to_string(),
                access: WorkspaceAccess::ExclusiveNewDirectory,
            },
            limits,
        };
        Ok(Self {
            input,
            input_parent,
            root,
            parent,
            request,
        })
    }
    pub(crate) fn verify(&self, control: &Control) -> Result<(), SidecarError> {
        self.input_parent.verify(&"SIDECAR_INPUT")?;
        self.input.verify(control, &"SIDECAR_INPUT")?;
        self.parent.verify(&"SIDECAR_WORKSPACE")?;
        self.root.verify(&"SIDECAR_WORKSPACE")
    }
    pub(crate) fn resources(
        &self,
        response: &Response,
        control: &Control,
    ) -> Result<ResourceGuards, SidecarError> {
        self.verify(control)?;
        let limits = &self.request.limits;
        if response.resources.len() > limits.resources as usize {
            return Err(err("SIDECAR_LIMIT", "resource count limit"));
        }
        let mut expected = BTreeSet::new();
        let mut dirs = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut total = 0u64;
        for r in &response.resources {
            if !valid_relative_path(&r.relative_path)
                || !ids.insert(r.resource_id.clone())
                || !valid_id(&r.resource_id)
                || !expected.insert(r.relative_path.to_lowercase())
            {
                return Err(err("SIDECAR_RESOURCE", "invalid or colliding path/id"));
            }
            let pieces: Vec<_> = r.relative_path.split('/').collect();
            if pieces.len() > 33 {
                return Err(err("SIDECAR_LIMIT", "resource directory depth exceeds 32"));
            }
            for end in 1..pieces.len() {
                dirs.insert(pieces[..end].join("/").to_lowercase());
            }
        }
        if !expected.is_disjoint(&dirs) {
            return Err(err("SIDECAR_RESOURCE", "file/directory prefix collision"));
        }
        let mut actual_files = BTreeSet::new();
        let mut actual_dirs = BTreeSet::new();
        let mut directories = Vec::new();
        self.walk(
            control,
            &expected,
            &dirs,
            &mut actual_files,
            &mut actual_dirs,
            &mut directories,
        )?;
        if actual_files != expected || actual_dirs != dirs {
            return Err(err(
                "SIDECAR_RESOURCE",
                "workspace is not exact resource manifest",
            ));
        }
        let mut files = Vec::new();
        let mut bytes = BTreeMap::new();
        for r in &response.resources {
            control.check()?;
            let (guard, data) = GuardedFile::open(
                &self.root.path.join(&r.relative_path),
                limits.resource_bytes,
                control,
                &"SIDECAR_RESOURCE",
            )?;
            total = total
                .checked_add(data.len() as u64)
                .ok_or_else(|| err("SIDECAR_LIMIT", "resource sum overflow"))?;
            if total > limits.total_resource_bytes {
                return Err(err("SIDECAR_LIMIT", "total resources byte limit"));
            }
            if guard.fingerprint.sha256 != r.fingerprint.sha256
                || guard.fingerprint.byte_length != r.fingerprint.byte_length
            {
                return Err(err("SIDECAR_RESOURCE", "resource SHA/length mismatch"));
            }
            let resource = if r.kind == ResourceKind::Utf8Text {
                ResourceData::Text(
                    String::from_utf8(data)
                        .map_err(|_| err("SIDECAR_RESOURCE", "invalid UTF8 text resource"))?,
                )
            } else {
                ResourceData::Binary(data)
            };
            control.check()?;
            bytes.insert(r.resource_id.clone(), resource);
            files.push(guard);
        }
        let mut final_files = BTreeSet::new();
        let mut final_dirs = BTreeSet::new();
        let mut final_guards = Vec::new();
        self.walk(
            control,
            &expected,
            &dirs,
            &mut final_files,
            &mut final_dirs,
            &mut final_guards,
        )?;
        if final_files != expected || final_dirs != dirs {
            return Err(err("SIDECAR_RESOURCE", "resource manifest changed"));
        }
        self.verify(control)?;
        for f in &files {
            f.verify(control, &"SIDECAR_RESOURCE")?;
        }
        Ok(ResourceGuards {
            _files: files,
            _directories: directories,
            bytes,
        })
    }
    fn walk(
        &self,
        control: &Control,
        expected_files: &BTreeSet<String>,
        expected_dirs: &BTreeSet<String>,
        files: &mut BTreeSet<String>,
        dirs: &mut BTreeSet<String>,
        guards: &mut Vec<Directory>,
    ) -> Result<(), SidecarError> {
        let mut pending = vec![(self.root.path.clone(), String::new())];
        while let Some((path, prefix)) = pending.pop() {
            control.check()?;
            for entry in fs::read_dir(path).map_err(|e| err("SIDECAR_RESOURCE", e))? {
                control.check()?;
                let entry = entry.map_err(|e| err("SIDECAR_RESOURCE", e))?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| err("SIDECAR_RESOURCE", "non UTF8 resource"))?;
                let relative = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                };
                if !valid_relative_path(&relative) {
                    return Err(err("SIDECAR_RESOURCE", "unsafe workspace entry"));
                }
                let folded = relative.to_lowercase();
                let meta =
                    fs::symlink_metadata(entry.path()).map_err(|e| err("SIDECAR_RESOURCE", e))?;
                if project::is_link(&meta) {
                    return Err(err("SIDECAR_RESOURCE", "linked workspace entry"));
                }
                if meta.is_dir() {
                    if !expected_dirs.contains(&folded) || !dirs.insert(folded) {
                        return Err(err("SIDECAR_RESOURCE", "unexpected or colliding directory"));
                    }
                    guards.push(Directory::capture(&entry.path(), &"SIDECAR_RESOURCE")?);
                    pending.push((entry.path(), relative));
                } else if meta.is_file() {
                    if !expected_files.contains(&folded) || !files.insert(folded) {
                        return Err(err("SIDECAR_RESOURCE", "unexpected or colliding file"));
                    }
                } else {
                    return Err(err("SIDECAR_RESOURCE", "nonregular workspace entry"));
                }
            }
        }
        for directory in guards {
            directory.verify(&"SIDECAR_RESOURCE")?;
        }
        Ok(())
    }
}
#[derive(Debug)]
pub(crate) struct ResourceGuards {
    _files: Vec<GuardedFile>,
    _directories: Vec<Directory>,
    pub(crate) bytes: BTreeMap<String, ResourceData>,
}
pub(crate) fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

#[derive(Debug)]
pub(crate) enum ResourceData {
    Text(String),
    Binary(Vec<u8>),
}
impl ResourceData {
    pub(crate) fn byte_length(&self) -> usize {
        match self {
            Self::Text(text) => text.len(),
            Self::Binary(bytes) => bytes.len(),
        }
    }
    pub(crate) fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.as_str()),
            Self::Binary(_) => None,
        }
    }
}
