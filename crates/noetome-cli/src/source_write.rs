//! Protected, recoverable publication for source-registration writes.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::pdf_parse::{self, PdfLimits, PdfParseOutput, PdfParseReport, PdfParserSpec};
use crate::project::{self, ProjectError, ProjectInfo};

const MARKER: &str = ".noetome-writing";
const CACHE: &str = ".noetome-sources";
const PDF_CACHE: &str = ".noetome-pdf-parses";
const OWNER: &str = ".owner.json";
const CANDIDATE_DB: &str = "candidate-project.sqlite";
const CANDIDATE_CONFIG: &str = "candidate-project.json";

fn incomplete() -> ProjectError {
    ProjectError::new(
        "SOURCE_WRITE_INCOMPLETE",
        3,
        "来源写入未完成或现场内容、身份发生变化；现有内容、写入标记和可用备份已保留。请检查原项目，使用新的测试项目重试，不要自动接管残留。",
    )
}

fn snapshot_conflict() -> ProjectError {
    ProjectError::new(
        "SOURCE_SNAPSHOT_CONFLICT",
        3,
        "来源快照目录已存在、所有权不匹配或含未知内容；不会覆盖或接管现有目录。",
    )
}

struct Snapshot {
    path: PathBuf,
    bytes: Vec<u8>,
    identity: same_file::Handle,
    single_link: bool,
}

impl Snapshot {
    fn capture(path: &Path, guard: &File) -> Result<Self, ProjectError> {
        project::check_components(path)?;
        project::checked_file(path)?;
        let opened = same_file::Handle::from_file(
            guard
                .try_clone()
                .map_err(|error| ProjectError::io(path, error))?,
        )
        .map_err(|error| ProjectError::io(path, error))?;
        // The clone keeps deny-delete sharing only within this function. The
        // retained identity handle comes from_path and permits later renames.
        let identity =
            same_file::Handle::from_path(path).map_err(|error| ProjectError::io(path, error))?;
        if opened != identity {
            return Err(incomplete());
        }
        let bytes = fs::read(path).map_err(|error| ProjectError::io(path, error))?;
        Ok(Self {
            path: path.into(),
            bytes,
            identity,
            single_link: false,
        })
    }
    fn read(path: &Path) -> Result<Self, ProjectError> {
        let guard = project::read_guard(path)?;
        Self::capture(path, &guard)
    }
    fn matches(&self) -> bool {
        if self.single_link {
            return Self::read_single(&self.path, self.bytes.len() as u64).is_ok_and(|current| {
                current.identity == self.identity && current.bytes == self.bytes
            });
        }
        project::check_components(&self.path).is_ok()
            && project::checked_file(&self.path).is_ok()
            && same_file::Handle::from_path(&self.path)
                .is_ok_and(|current| current == self.identity)
            && fs::read(&self.path).ok().as_ref() == Some(&self.bytes)
    }
    fn read_single(path: &Path, limit: u64) -> Result<Self, ProjectError> {
        Self::read_single_with_host(path, limit, false)
    }
    fn read_single_with_host(
        path: &Path,
        limit: u64,
        host_write_guard: bool,
    ) -> Result<Self, ProjectError> {
        project::check_components(path)?;
        let metadata = project::checked_file(path)?;
        if metadata.len() > limit {
            return Err(incomplete());
        }
        let guard = if host_write_guard {
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                // Original Host guards retain WRITE and report DELETE access.
                // Their share1 excludes external changes throughout this read;
                // share7 here only permits coexistence with those capabilities.
                options.share_mode(7).custom_flags(0x00200000);
            }
            options
                .open(path)
                .map_err(|error| ProjectError::io(path, error))?
        } else {
            project::read_guard(path)?
        };
        let identity =
            same_file::Handle::from_file(guard.try_clone().map_err(|e| ProjectError::io(path, e))?)
                .map_err(|e| ProjectError::io(path, e))?;
        let current = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
        if current != identity
            || noetome_sidecar_platform::file_link_count(&guard)
                .map_err(|e| ProjectError::io(path, e))?
                != 1
        {
            return Err(incomplete());
        }
        let mut bytes = Vec::new();
        (&guard)
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|e| ProjectError::io(path, e))?;
        if bytes.len() as u64 > limit
            || bytes.len() as u64 != metadata.len()
            || noetome_sidecar_platform::file_link_count(&guard)
                .map_err(|e| ProjectError::io(path, e))?
                != 1
            || same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?
                != identity
        {
            return Err(incomplete());
        }
        // Keep a share-delete identity so publication and rollback can rename.
        Ok(Self {
            path: path.into(),
            bytes,
            identity: current,
            single_link: true,
        })
    }
}

struct Directory {
    path: PathBuf,
    identity: same_file::Handle,
}
impl Directory {
    fn read(path: &Path) -> Result<Self, ProjectError> {
        project::check_components(path)?;
        let metadata = fs::symlink_metadata(path).map_err(|error| ProjectError::io(path, error))?;
        if project::is_link(&metadata) || !metadata.is_dir() {
            return Err(ProjectError::unsafe_path(path));
        }
        let identity =
            same_file::Handle::from_path(path).map_err(|error| ProjectError::io(path, error))?;
        Ok(Self {
            path: path.into(),
            identity,
        })
    }
    fn matches(&self) -> bool {
        Self::read(&self.path).is_ok_and(|current| current.identity == self.identity)
    }
}

struct Original {
    snapshot: Snapshot,
    backup: PathBuf,
    published: bool,
}
struct StagedVersion {
    staging: PathBuf,
    destination: PathBuf,
    normalized: bool,
    copy: bool,
}
struct StagedParse {
    staging: PathBuf,
    destination: PathBuf,
    names: Vec<String>,
}
struct CacheGuard {
    directory: Directory,
    owner: Snapshot,
    names: BTreeSet<OsString>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheOwner {
    schema_version: u32,
    project_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParserOwner {
    schema_version: u32,
    request_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WriteStage {
    Prepared,
    VersionPublished,
    DatabasePublished,
    ConfigPublished,
    Validated,
    Cleanup,
}

pub(crate) struct Writer {
    root: Directory,
    marker: Directory,
    root_names: BTreeSet<OsString>,
    info: ProjectInfo,
    candidate: PathBuf,
    originals: Vec<Original>,
    read_guards: Vec<File>,
    files: Vec<Snapshot>,
    directories: Vec<Directory>,
    cache_guards: Vec<CacheGuard>,
    version: Option<StagedVersion>,
    parse: Option<StagedParse>,
    prepared: bool,
    finished: bool,
    preserve: bool,
    recoverable_backups: BTreeSet<PathBuf>,
    #[cfg(test)]
    hook: Option<Box<dyn FnMut(WriteStage, &Path) -> Result<(), ProjectError>>>,
}

impl Writer {
    pub(crate) fn begin(path: &Path) -> Result<Self, ProjectError> {
        let info = project::inspect(path)?;
        let absolute = project::check_components(path)?;
        let root = Directory::read(&absolute)?;
        let root_names = directory_names(&absolute)?;
        let marker_path = absolute.join(MARKER);
        if !root.matches() {
            return Err(incomplete());
        }
        fs::create_dir(&marker_path).map_err(|error| ProjectError::io(&marker_path, error))?;
        let marker = Directory::read(&marker_path)?;
        let marker_entry = Directory::read(&marker_path)?;
        let mut writer = Self {
            root,
            marker,
            root_names,
            info,
            candidate: marker_path.join(CANDIDATE_DB),
            originals: Vec::new(),
            read_guards: Vec::new(),
            files: Vec::new(),
            directories: vec![marker_entry],
            cache_guards: Vec::new(),
            version: None,
            parse: None,
            prepared: false,
            finished: false,
            preserve: false,
            recoverable_backups: BTreeSet::new(),
            #[cfg(test)]
            hook: None,
        };
        writer.ensure_owners()?;
        writer.info = project::inspect_for_writer(writer.root(), &writer.marker.identity)?;
        for (name, backup_name) in [
            (project::CONFIG, "old-project.json"),
            (project::DATABASE, "old-project.sqlite"),
        ] {
            let path = absolute.join(name);
            let guard = project::read_guard(&path)?;
            let snapshot = Snapshot::capture(&path, &guard)?;
            writer.read_guards.push(guard);
            writer.originals.push(Original {
                snapshot,
                backup: marker_path.join(backup_name),
                published: false,
            });
        }
        let mut descriptor = serde_json::to_vec(&CacheOwner {
            schema_version: 1,
            project_id: writer.info.project_id.clone(),
        })
        .map_err(|error| ProjectError::io(&marker_path, error))?;
        descriptor.push(b'\n');
        writer.create_file(&marker_path.join("operation.json"), &descriptor)?;
        for index in 0..writer.originals.len() {
            let path = writer.originals[index].backup.clone();
            let bytes = writer.originals[index].snapshot.bytes.clone();
            writer.create_file(&path, &bytes)?;
        }
        writer.ensure_integrity()?;
        Ok(writer)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root.path
    }
    pub(crate) fn info(&self) -> &ProjectInfo {
        &self.info
    }
    pub(crate) fn candidate_path(&self) -> &Path {
        &self.candidate
    }

    fn ensure_owners(&self) -> Result<(), ProjectError> {
        if self.finished || !self.root.matches() || !self.marker.matches() {
            return Err(incomplete());
        }
        Ok(())
    }

    fn ensure_integrity(&self) -> Result<(), ProjectError> {
        self.ensure_owners()?;
        if self.files.iter().any(|file| !file.matches())
            || self
                .originals
                .iter()
                .any(|original| !original.published && !original.snapshot.matches())
            || self
                .directories
                .iter()
                .any(|directory| !directory.matches())
        {
            return Err(incomplete());
        }
        let mut expected_root = self.root_names.clone();
        expected_root.insert(OsString::from(MARKER));
        for directory in &self.directories {
            if directory.path.parent() == Some(self.root()) {
                expected_root.insert(directory.path.file_name().ok_or_else(incomplete)?.into());
            }
        }
        if directory_names(self.root())? != expected_root {
            return Err(incomplete());
        }
        for directory in &self.directories {
            let mut expected = BTreeSet::new();
            for file in &self.files {
                if file.path.parent() == Some(directory.path.as_path()) {
                    expected.insert(file.path.file_name().ok_or_else(incomplete)?.to_os_string());
                }
            }
            for child in &self.directories {
                if child.path.parent() == Some(directory.path.as_path()) {
                    expected.insert(
                        child
                            .path
                            .file_name()
                            .ok_or_else(incomplete)?
                            .to_os_string(),
                    );
                }
            }
            if directory_names(&directory.path)? != expected {
                return Err(incomplete());
            }
        }
        for cache in &self.cache_guards {
            if !cache.directory.matches() || !cache.owner.matches() {
                return Err(incomplete());
            }
            let mut expected = cache.names.clone();
            for directory in &self.directories {
                if directory.path.parent() == Some(cache.directory.path.as_path()) {
                    expected.insert(
                        directory
                            .path
                            .file_name()
                            .ok_or_else(incomplete)?
                            .to_os_string(),
                    );
                }
            }
            if directory_names(&cache.directory.path)? != expected {
                return Err(incomplete());
            }
        }
        Ok(())
    }

    fn create_directory(&mut self, path: &Path) -> Result<(), ProjectError> {
        self.ensure_owners()?;
        project::check_components(path)?;
        fs::create_dir(path).map_err(|error| ProjectError::io(path, error))?;
        self.directories.push(Directory::read(path)?);
        Ok(())
    }

    fn create_file(&mut self, path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
        self.ensure_owners()?;
        project::check_components(path)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1).custom_flags(0x00200000);
        }
        let mut file = options
            .open(path)
            .map_err(|error| ProjectError::io(path, error))?;
        let opened = same_file::Handle::from_file(
            file.try_clone()
                .map_err(|error| ProjectError::io(path, error))?,
        )
        .map_err(|error| ProjectError::io(path, error))?;
        let result = file.write_all(bytes).and_then(|_| file.sync_all());
        drop(file);
        let shared =
            same_file::Handle::from_path(path).map_err(|error| ProjectError::io(path, error))?;
        if shared != opened {
            return Err(incomplete());
        }
        // Drop the write-access clone before opening a deny-write read guard:
        // Windows sharing checks are symmetric even within this process.
        drop(opened);
        let current = Snapshot::read(path)?;
        if current.identity != shared {
            return Err(incomplete());
        }
        self.files.push(current);
        result.map_err(|error| ProjectError::io(path, error))
    }

    fn create_single_file(&mut self, path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
        self.create_file(path, bytes)?;
        let index = self
            .files
            .iter()
            .position(|file| file.path == path)
            .ok_or_else(incomplete)?;
        // Even a failed first single-link check must retain that requirement
        // during rollback, so an externally added alias is preserved.
        self.files[index].single_link = true;
        let current = Snapshot::read_single(path, bytes.len() as u64)?;
        if current.identity != self.files[index].identity || current.bytes != bytes {
            return Err(incomplete());
        }
        self.files[index] = current;
        Ok(())
    }

    fn refresh_candidate(&mut self) -> Result<(), ProjectError> {
        self.ensure_owners()?;
        let index = self
            .files
            .iter()
            .position(|file| file.path == self.candidate)
            .ok_or_else(incomplete)?;
        let current = Snapshot::read(&self.candidate)?;
        if current.identity != self.files[index].identity {
            return Err(incomplete());
        }
        self.files[index] = current;
        Ok(())
    }

    pub(crate) fn build_candidate<T>(
        &mut self,
        operation: impl FnOnce(&mut Connection, &ProjectInfo) -> Result<T, ProjectError>,
    ) -> Result<T, ProjectError> {
        self.ensure_integrity()?;
        if self.prepared || self.files.iter().any(|file| file.path == self.candidate) {
            return Err(incomplete());
        }
        let bytes = self
            .originals
            .iter()
            .find(|original| original.snapshot.path.file_name() == Some(project::DATABASE.as_ref()))
            .ok_or_else(incomplete)?
            .snapshot
            .bytes
            .clone();
        let candidate = self.candidate.clone();
        self.create_file(&candidate, &bytes)?;
        let mut connection =
            Connection::open_with_flags(&candidate, OpenFlags::SQLITE_OPEN_READ_WRITE)
                .map_err(ProjectError::database)?;
        let result = (|| {
            connection
                .execute_batch(
                    "PRAGMA foreign_keys=ON; PRAGMA journal_mode=DELETE; PRAGMA temp_store=MEMORY;",
                )
                .map_err(ProjectError::database)?;
            operation(&mut connection, &self.info)
        })();
        let close = connection
            .close()
            .map_err(|(_, error)| ProjectError::database(error));
        self.refresh_candidate()?;
        close?;
        self.ensure_integrity()?;
        let result = result?;
        self.prepared = true;
        self.run_hook(WriteStage::Prepared)?;
        Ok(result)
    }

    pub(crate) fn stage_version(
        &mut self,
        version_id: &str,
        raw: &[u8],
        normalized: Option<&[u8]>,
    ) -> Result<PathBuf, ProjectError> {
        self.ensure_integrity()?;
        if !valid_uuid(version_id) {
            return Err(ProjectError::new(
                "SOURCE_INVALID_VERSION_ID",
                2,
                "内部快照版本必须是标准UUID v4，不能由外部输出路径指定。",
            ));
        }
        if self.version.is_some() {
            return Err(incomplete());
        }
        let staging = self.marker.path.join("version");
        let destination = self.root().join(CACHE).join(version_id);
        self.create_directory(&staging)?;
        self.create_file(&staging.join("raw.bin"), raw)?;
        if let Some(normalized) = normalized {
            self.create_file(&staging.join("normalized.txt"), normalized)?;
        }
        self.version = Some(StagedVersion {
            staging,
            destination: destination.clone(),
            normalized: normalized.is_some(),
            copy: false,
        });
        self.ensure_integrity()?;
        Ok(destination)
    }

    pub(crate) fn stage_pdf_raw(
        &mut self,
        version_id: &str,
        raw: &[u8],
    ) -> Result<PathBuf, ProjectError> {
        self.ensure_integrity()?;
        if raw.len() > 64 * 1024 * 1024 {
            return Err(snapshot_conflict());
        }
        if !valid_uuid(version_id) {
            return Err(ProjectError::new(
                "SOURCE_INVALID_VERSION_ID",
                2,
                "内部快照版本必须是标准UUID v4。",
            ));
        }
        if self.version.is_some() {
            return Err(incomplete());
        }
        let staging = self.marker.path.join("version");
        let destination = self.root().join(CACHE).join(version_id);
        self.create_directory(&staging)?;
        self.create_single_file(&staging.join("raw.bin"), raw)?;
        self.version = Some(StagedVersion {
            staging,
            destination: destination.clone(),
            normalized: false,
            copy: true,
        });
        self.ensure_integrity()?;
        Ok(destination)
    }

    pub(crate) fn run_pdf_parser(
        &mut self,
        parser: &PdfParserSpec,
        raw: &[u8],
        limits: PdfLimits,
        cancel: &AtomicBool,
    ) -> Result<PdfParseOutput, ProjectError> {
        self.ensure_integrity()?;
        if cancel.load(Ordering::Acquire) {
            self.preserve = true;
            return Err(ProjectError::new(
                "SIDECAR_CANCELLED",
                3,
                "PDF解析已取消；写入现场已保留。",
            ));
        }
        if raw.len() as u64 > limits.transport.max_input_bytes {
            return Err(ProjectError::new(
                "SIDECAR_INPUT_LIMIT",
                3,
                "已选PDF字节超过读取政策。",
            ));
        }
        let selected = self.marker.path.join("selected.pdf");
        let workspace = self.marker.path.join("parser");
        self.create_single_file(&selected, raw)?;
        self.ensure_integrity()?;
        // Keep our selected raw immutable until T02's own input guard has been
        // handed back. T02 owns its new child tree throughout execution.
        let selected_guard = project::read_guard(&selected)?;
        let result = pdf_parse::parse_pdf_retained(parser, &selected, &workspace, limits, cancel);
        let handoff = match result {
            Ok(handoff) => handoff,
            Err(error) => {
                if cancel.load(Ordering::Acquire)
                    || matches!(error.code, "SIDECAR_CANCELLED" | "SIDECAR_CLEANUP_FAILED")
                    || !matches!(fs::symlink_metadata(&workspace),Err(ref e) if e.kind()==std::io::ErrorKind::NotFound)
                {
                    self.preserve = true;
                }
                return Err(ProjectError::new(
                    error.code,
                    pdf_parse::error_exit_code(&error),
                    error.message,
                ));
            }
        };
        let adopted = (|| {
            handoff.receipt.verify(cancel).map_err(|error| {
                ProjectError::new(
                    error.code,
                    pdf_parse::error_exit_code(&error),
                    error.message,
                )
            })?;
            let report = &handoff.output.report;
            if report.schema_version != 1
                || report.producer != crate::sidecar::Producer::Parser
                || report.report_path != "parse.json"
                || report.input.relative_path != "input/raw.bin"
                || report.input.byte_length != raw.len() as u64
                || report.input.sha256 != crate::sidecar::digest(raw)
                || report.request.input != report.input
                || !valid_uuid(&report.request.request_id)
            {
                return Err(incomplete());
            }
            let dirs = vec![
                Directory::read(&workspace)?,
                Directory::read(&workspace.join("input"))?,
                Directory::read(&workspace.join("artifacts"))?,
            ];
            let expected = BTreeSet::from(
                [
                    ".owner.json",
                    "request.json",
                    "input",
                    "artifacts",
                    "parse.json",
                ]
                .map(OsString::from),
            );
            if directory_names(&workspace)? != expected
                || directory_names(&workspace.join("input"))?
                    != BTreeSet::from([OsString::from("raw.bin")])
            {
                return Err(incomplete());
            }
            let owner = Snapshot::read_single_with_host(&workspace.join(OWNER), 4096, true)?;
            let descriptor: ParserOwner =
                serde_json::from_slice(&owner.bytes).map_err(|_| incomplete())?;
            if descriptor.schema_version != 1 || descriptor.request_id != report.request.request_id
            {
                return Err(incomplete());
            }
            let request = Snapshot::read_single_with_host(
                &workspace.join("request.json"),
                limits.transport.max_request_bytes as u64,
                true,
            )?;
            let actual_request: crate::pdf_parse::PdfRequest =
                serde_json::from_slice(&request.bytes).map_err(|_| incomplete())?;
            if actual_request != report.request {
                return Err(incomplete());
            }
            let input = Snapshot::read_single_with_host(
                &workspace.join("input/raw.bin"),
                raw.len() as u64,
                true,
            )?;
            let formal = Snapshot::read_single_with_host(
                &workspace.join("parse.json"),
                8 * 1024 * 1024,
                true,
            )?;
            if input.bytes != raw || formal.bytes != handoff.output.json.as_bytes() {
                return Err(incomplete());
            }
            let mut files = vec![owner, request, input, formal];
            let mut names = BTreeSet::new();
            let mut folded = BTreeSet::new();
            let mut total = 0u64;
            let artifacts = report
                .response
                .as_ref()
                .map_or(&[][..], |response| response.artifacts.as_slice());
            if artifacts.len() > limits.transport.max_artifacts as usize {
                return Err(incomplete());
            }
            for artifact in artifacts {
                let extension = match artifact.kind {
                    crate::sidecar::ArtifactKind::Binary => "bin",
                    crate::sidecar::ArtifactKind::Utf8Text => "txt",
                };
                if !valid_artifact_path(&artifact.relative_path)
                    || artifact.relative_path
                        != format!("artifacts/{}.{extension}", artifact.artifact_id)
                    || !folded.insert(artifact.relative_path.to_ascii_lowercase())
                    || artifact.byte_length > limits.transport.max_artifact_bytes
                {
                    return Err(incomplete());
                }
                total = total
                    .checked_add(artifact.byte_length)
                    .ok_or_else(incomplete)?;
                if total > limits.transport.max_total_artifact_bytes {
                    return Err(incomplete());
                }
                names.insert(OsString::from(
                    artifact
                        .relative_path
                        .strip_prefix("artifacts/")
                        .ok_or_else(incomplete)?,
                ));
                let snapshot = Snapshot::read_single_with_host(
                    &workspace.join(&artifact.relative_path),
                    artifact.byte_length,
                    true,
                )?;
                if snapshot.bytes.len() as u64 != artifact.byte_length
                    || crate::sidecar::digest(&snapshot.bytes) != artifact.sha256
                {
                    return Err(incomplete());
                }
                files.push(snapshot);
            }
            if directory_names(&workspace.join("artifacts"))? != names {
                return Err(incomplete());
            }
            handoff.receipt.verify(cancel).map_err(|error| {
                ProjectError::new(
                    error.code,
                    pdf_parse::error_exit_code(&error),
                    error.message,
                )
            })?;
            if dirs.iter().any(|dir| !dir.matches())
                || files.iter().any(|file| {
                    !Snapshot::read_single_with_host(&file.path, file.bytes.len() as u64, true)
                        .is_ok_and(|current| {
                            current.identity == file.identity && current.bytes == file.bytes
                        })
                })
            {
                return Err(incomplete());
            }
            self.directories.extend(dirs);
            self.files.extend(files);
            Ok(())
        })();
        if let Err(error) = adopted {
            self.preserve = true;
            return Err(error);
        }
        let pdf_parse::PdfParseHandoff { output, receipt } = handoff;
        drop(receipt);
        drop(selected_guard);
        if let Err(error) = self.ensure_integrity() {
            self.preserve = true;
            return Err(error);
        }
        Ok(output)
    }

    pub(crate) fn pdf_artifact_bytes(
        &mut self,
        report: &PdfParseReport,
    ) -> Result<BTreeMap<String, Vec<u8>>, ProjectError> {
        self.ensure_integrity()?;
        let workspace = self.marker.path.join("parser");
        let mut report_bytes = serde_json::to_vec_pretty(report).map_err(|_| incomplete())?;
        report_bytes.push(b'\n');
        let formal = self
            .files
            .iter()
            .find(|file| file.path == workspace.join("parse.json"))
            .ok_or_else(incomplete)?;
        if !formal.single_link || formal.bytes != report_bytes {
            return Err(incomplete());
        }
        let mut bytes = BTreeMap::new();
        for artifact in report
            .response
            .as_ref()
            .map_or(&[][..], |response| response.artifacts.as_slice())
        {
            if !valid_artifact_path(&artifact.relative_path) {
                return Err(incomplete());
            }
            let snapshot = self
                .files
                .iter()
                .find(|file| file.path == workspace.join(&artifact.relative_path))
                .ok_or_else(incomplete)?;
            if !snapshot.single_link
                || snapshot.bytes.len() as u64 != artifact.byte_length
                || crate::sidecar::digest(&snapshot.bytes) != artifact.sha256
                || bytes
                    .insert(artifact.relative_path.clone(), snapshot.bytes.clone())
                    .is_some()
            {
                return Err(incomplete());
            }
        }
        self.ensure_integrity()?;
        Ok(bytes)
    }

    pub(crate) fn stage_pdf_parse(
        &mut self,
        parse_id: &str,
        files: &BTreeMap<String, Vec<u8>>,
    ) -> Result<PathBuf, ProjectError> {
        self.ensure_integrity()?;
        if !valid_uuid(parse_id) {
            return Err(ProjectError::new(
                "SOURCE_INVALID_PARSE_ID",
                2,
                "内部解析快照必须是标准UUID v4。",
            ));
        }
        let mut folded = BTreeSet::new();
        if self.parse.is_some()
            || !files.contains_key("report.json")
            || files.iter().any(|(name, bytes)| {
                !folded.insert(name.to_ascii_lowercase())
                    || if name == "report.json" {
                        bytes.len() > 16 * 1024 * 1024
                    } else {
                        !valid_artifact_path(name) || bytes.len() > 16 * 1024 * 1024
                    }
            })
            || files.len() > 65
            || files
                .iter()
                .filter(|(name, _)| name.as_str() != "report.json")
                .map(|(_, bytes)| bytes.len() as u64)
                .sum::<u64>()
                > 64 * 1024 * 1024
        {
            return Err(snapshot_conflict());
        }
        let staging = self.marker.path.join("parse");
        let destination = self.root().join(PDF_CACHE).join(parse_id);
        self.create_directory(&staging)?;
        self.create_directory(&staging.join("artifacts"))?;
        for (name, bytes) in files {
            self.create_single_file(&staging.join(name), bytes)?;
        }
        self.parse = Some(StagedParse {
            staging,
            destination: destination.clone(),
            names: files.keys().cloned().collect(),
        });
        self.ensure_integrity()?;
        Ok(destination)
    }

    fn prepare_cache(&mut self, name: &str) -> Result<(), ProjectError> {
        let cache_path = self.root().join(name);
        if project::exists(&cache_path)? {
            let directory = Directory::read(&cache_path).map_err(|_| snapshot_conflict())?;
            let owner_path = cache_path.join(OWNER);
            if project::checked_file(&owner_path)
                .map_err(|_| snapshot_conflict())?
                .len()
                > 4096
            {
                return Err(snapshot_conflict());
            }
            let owner = if name == PDF_CACHE {
                Snapshot::read_single(&owner_path, 4096)
            } else {
                Snapshot::read(&owner_path)
            }
            .map_err(|_| snapshot_conflict())?;
            if owner.bytes.len() > 4096 {
                return Err(snapshot_conflict());
            }
            let value: CacheOwner =
                serde_json::from_slice(&owner.bytes).map_err(|_| snapshot_conflict())?;
            if value.schema_version != 1 || value.project_id != self.info.project_id {
                return Err(snapshot_conflict());
            }
            let names = directory_names(&cache_path)?;
            self.cache_guards.push(CacheGuard {
                directory,
                owner,
                names,
            });
        } else {
            self.create_directory(&cache_path)?;
            let mut bytes = serde_json::to_vec(&CacheOwner {
                schema_version: 1,
                project_id: self.info.project_id.clone(),
            })
            .map_err(|error| ProjectError::io(&cache_path, error))?;
            bytes.push(b'\n');
            if name == PDF_CACHE {
                self.create_single_file(&cache_path.join(OWNER), &bytes)?;
            } else {
                self.create_file(&cache_path.join(OWNER), &bytes)?;
            }
        }
        self.ensure_integrity()
    }

    fn publish_version(&mut self) -> Result<(), ProjectError> {
        if let Some(version) = &self.version {
            let staging = version.staging.clone();
            let destination = version.destination.clone();
            let normalized = version.normalized;
            let copy = version.copy;
            self.prepare_cache(CACHE)?;
            if project::exists(&destination)? {
                return Err(snapshot_conflict());
            }
            self.create_directory(&destination)?;
            for name in ["raw.bin", "normalized.txt"] {
                if name == "normalized.txt" && !normalized {
                    continue;
                }
                self.ensure_integrity()?;
                let staged = staging.join(name);
                let target = destination.join(name);
                let original = self
                    .files
                    .iter()
                    .find(|file| file.path == staged)
                    .ok_or_else(incomplete)?;
                if !original.matches() {
                    return Err(incomplete());
                }
                if copy {
                    let bytes = original.bytes.clone();
                    let identity = &original.identity;
                    let identity = same_file::Handle::from_path(&staged)
                        .map_err(|e| ProjectError::io(&staged, e))
                        .and_then(|current| {
                            if &current == identity {
                                Ok(current)
                            } else {
                                Err(incomplete())
                            }
                        })?;
                    self.create_single_file(&target, &bytes)?;
                    let current = self.files.last().ok_or_else(incomplete)?;
                    if current.identity == identity {
                        return Err(incomplete());
                    }
                    continue;
                }
                // Directory creation and each file link are exclusive. This cannot
                // replace an existing file, and staged links survive until cleanup.
                fs::hard_link(&staged, &target)
                    .map_err(|error| ProjectError::io(&target, error))?;
                let current = Snapshot::read(&target)?;
                if current.identity != original.identity || current.bytes != original.bytes {
                    return Err(incomplete());
                }
                self.files.push(current);
            }
        }
        if let Some(parse) = &self.parse {
            let staging = parse.staging.clone();
            let destination = parse.destination.clone();
            let names = parse.names.clone();
            self.prepare_cache(PDF_CACHE)?;
            if project::exists(&destination)? {
                return Err(snapshot_conflict());
            }
            self.create_directory(&destination)?;
            self.create_directory(&destination.join("artifacts"))?;
            for name in names {
                self.ensure_integrity()?;
                let staged = staging.join(&name);
                let original = self
                    .files
                    .iter()
                    .find(|file| file.path == staged)
                    .ok_or_else(incomplete)?;
                if !original.matches() || !original.single_link {
                    return Err(incomplete());
                }
                let bytes = original.bytes.clone();
                let identity = same_file::Handle::from_path(&staged)
                    .map_err(|e| ProjectError::io(&staged, e))?;
                self.create_single_file(&destination.join(name), &bytes)?;
                if self.files.last().ok_or_else(incomplete)?.identity == identity {
                    return Err(incomplete());
                }
            }
        }
        self.ensure_integrity()?;
        if self.version.is_some() || self.parse.is_some() {
            self.run_hook(WriteStage::VersionPublished)?;
        }
        Ok(())
    }

    fn publish_file(&mut self, staged: &Path, original_index: usize) -> Result<(), ProjectError> {
        self.ensure_integrity()?;
        let index = self
            .files
            .iter()
            .position(|file| file.path == staged)
            .ok_or_else(incomplete)?;
        let destination = self.originals[original_index].snapshot.path.clone();
        if !self.files[index].matches() || !self.originals[original_index].snapshot.matches() {
            return Err(incomplete());
        }
        fs::rename(staged, &destination).map_err(|error| ProjectError::io(&destination, error))?;
        self.files[index].path = destination;
        self.originals[original_index].published = true;
        self.ensure_integrity()
    }

    pub(crate) fn commit(
        &mut self,
        new_info: &ProjectInfo,
        mut validate: impl FnMut(&Path, &same_file::Handle) -> Result<(), ProjectError>,
    ) -> Result<(), ProjectError> {
        self.ensure_integrity()?;
        if !self.prepared {
            return Err(incomplete());
        }
        if new_info.project_id != self.info.project_id
            || new_info.created_at != self.info.created_at
            || new_info.state != self.info.state
            || new_info.database != self.info.database
            || !matches!(
                (self.info.schema_version, new_info.schema_version),
                (1, 1 | 2 | 3) | (2, 2 | 3) | (3, 3)
            )
        {
            return Err(ProjectError::new(
                "SOURCE_INVALID_PROJECT_INFO",
                2,
                "来源写入不能改变项目身份、创建时间、基础状态或数据库路径；版本只允许保留或升级受支持的Schema1/2/3。",
            ));
        }
        let candidate_config = self.marker.path.join(CANDIDATE_CONFIG);
        let mut bytes = serde_json::to_vec(new_info)
            .map_err(|error| ProjectError::io(&candidate_config, error))?;
        bytes.push(b'\n');
        self.create_file(&candidate_config, &bytes)?;
        self.publish_version()?;
        self.ensure_integrity()?;
        // Shared identity handles remain, but deny-write/delete preparation
        // guards must be released before replacing the original pair.
        self.read_guards.clear();
        let database_index = self
            .originals
            .iter()
            .position(|original| {
                original.snapshot.path.file_name() == Some(project::DATABASE.as_ref())
            })
            .ok_or_else(incomplete)?;
        let config_index = self
            .originals
            .iter()
            .position(|original| {
                original.snapshot.path.file_name() == Some(project::CONFIG.as_ref())
            })
            .ok_or_else(incomplete)?;
        let candidate = self.candidate.clone();
        self.publish_file(&candidate, database_index)?;
        self.run_hook(WriteStage::DatabasePublished)?;
        self.publish_file(&candidate_config, config_index)?;
        self.run_hook(WriteStage::ConfigPublished)?;
        validate(self.root(), &self.marker.identity)?;
        self.ensure_integrity()?;
        self.run_hook(WriteStage::Validated)?;
        self.run_hook(WriteStage::Cleanup)?;
        if let Err(error) = self.remove_owned(true) {
            // Some deletions may already have succeeded. Never report success
            // or let Drop discard the remaining transaction on this path.
            self.preserve = true;
            self.recreate_missing_backups();
            return Err(error);
        }
        self.finished = true;
        Ok(())
    }

    fn restore_original(&mut self, index: usize) -> Result<(), ProjectError> {
        if !self.originals[index].published {
            return Ok(());
        }
        self.ensure_integrity()?;
        let target = self.originals[index].snapshot.path.clone();
        let backup_path = self.originals[index].backup.clone();
        let backup_index = self
            .files
            .iter()
            .position(|file| file.path == backup_path)
            .ok_or_else(incomplete)?;
        if self.files[backup_index].bytes != self.originals[index].snapshot.bytes {
            return Err(incomplete());
        }
        let target_index = self
            .files
            .iter()
            .position(|file| file.path == target)
            .ok_or_else(incomplete)?;
        if !self.files[target_index].matches() || !self.files[backup_index].matches() {
            return Err(incomplete());
        }
        fs::rename(&backup_path, &target).map_err(|_| incomplete())?;
        self.recoverable_backups.insert(backup_path.clone());
        let mut backup = self.files.remove(backup_index);
        self.files.retain(|file| file.path != target);
        backup.path = target;
        self.originals[index].snapshot = backup;
        self.originals[index].published = false;
        self.ensure_integrity()
    }

    fn remove_owned(&mut self, marker_only: bool) -> Result<(), ProjectError> {
        self.ensure_integrity()?;
        for index in (0..self.files.len()).rev() {
            if marker_only && !self.files[index].path.starts_with(&self.marker.path) {
                continue;
            }
            self.ensure_owners()?;
            if !self.files[index].matches() {
                return Err(incomplete());
            }
            fs::remove_file(&self.files[index].path).map_err(|_| incomplete())?;
            if self
                .originals
                .iter()
                .any(|original| original.backup == self.files[index].path)
            {
                self.recoverable_backups
                    .insert(self.files[index].path.clone());
            }
            self.files.remove(index);
        }
        let mut paths: Vec<_> = self
            .directories
            .iter()
            .filter(|directory| !marker_only || directory.path.starts_with(&self.marker.path))
            .map(|directory| directory.path.clone())
            .collect();
        // The marker is always last, including when cache and marker roots
        // have equal depth. Directory removal is non-recursive and fails on
        // any unknown content introduced after the preceding inventory check.
        paths.sort_by_key(|path| {
            (
                path == &self.marker.path,
                std::cmp::Reverse(path.components().count()),
            )
        });
        for path in paths {
            self.ensure_owners()?;
            let index = self
                .directories
                .iter()
                .position(|directory| directory.path == path)
                .ok_or_else(incomplete)?;
            if !self.directories[index].matches() {
                return Err(incomplete());
            }
            fs::remove_dir(&path).map_err(|_| incomplete())?;
            self.directories.remove(index);
        }
        Ok(())
    }

    fn recreate_missing_backups(&mut self) {
        if self.ensure_owners().is_err() {
            return;
        }
        for index in 0..self.originals.len() {
            let path = self.originals[index].backup.clone();
            // A missing backup is repairable only when this Writer moved or
            // removed it. Never recreate a file deleted by an external actor.
            if self.recoverable_backups.contains(&path)
                && matches!(project::exists(&path), Ok(false))
            {
                let bytes = self.originals[index].snapshot.bytes.clone();
                let _ = self.create_file(&path, &bytes);
            }
        }
    }

    pub(crate) fn abort(&mut self) -> Result<(), ProjectError> {
        if self.finished {
            return Ok(());
        }
        if self.preserve {
            return Err(incomplete());
        }
        let result = (|| {
            self.ensure_integrity()?;
            self.read_guards.clear();
            for index in 0..self.originals.len() {
                self.restore_original(index)?;
            }
            self.remove_owned(false)?;
            self.finished = true;
            Ok(())
        })();
        if result.is_err() {
            self.preserve = true;
            self.recreate_missing_backups();
        }
        result
    }

    fn run_hook(&mut self, stage: WriteStage) -> Result<(), ProjectError> {
        #[cfg(test)]
        if let Some(hook) = &mut self.hook {
            return hook(stage, &self.root.path);
        }
        let _ = stage;
        Ok(())
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        if !self.finished && !self.preserve {
            let _ = self.abort();
        }
    }
}

fn directory_names(path: &Path) -> Result<BTreeSet<OsString>, ProjectError> {
    fs::read_dir(path)
        .map_err(|error| ProjectError::io(path, error))?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name())
                .map_err(|error| ProjectError::io(path, error))
        })
        .collect()
}

fn valid_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| {
        id.get_version_num() == 4
            && id.get_variant() == uuid::Variant::RFC4122
            && id.to_string() == value
    })
}

fn valid_artifact_path(value: &str) -> bool {
    let Some(name) = value.strip_prefix("artifacts/") else {
        return false;
    };
    let Some((id, extension)) = name.rsplit_once('.') else {
        return false;
    };
    !id.is_empty()
        && id.len() <= 128
        && id.as_bytes()[0].is_ascii_alphanumeric()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        && matches!(extension, "bin" | "txt")
        && crate::fixture_input::valid_relative_path(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project;
    use std::collections::BTreeMap;
    use std::fs;
    use uuid::Uuid;

    struct Sandbox(PathBuf);
    impl Sandbox {
        fn new() -> Self {
            let parent =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.artifacts/B03-T03-writer/tests");
            fs::create_dir_all(&parent).unwrap();
            let root = parent
                .canonicalize()
                .unwrap()
                .join(Uuid::new_v4().to_string());
            fs::create_dir(&root).unwrap();
            fs::write(root.join("outside-sentinel.txt"), b"keep unchanged").unwrap();
            Self(root)
        }
        fn project(&self, name: &str) -> (PathBuf, ProjectInfo) {
            let root = self.0.join(name);
            let info = project::initialize(&root).unwrap();
            (root, info)
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn collect(root: &Path, cursor: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(cursor).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                assert!(!entry.file_type().unwrap().is_symlink());
                if entry.file_type().unwrap().is_dir() {
                    result.insert(path.strip_prefix(root).unwrap().into(), Vec::new());
                    collect(root, &path, result);
                } else {
                    result.insert(
                        path.strip_prefix(root).unwrap().into(),
                        fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut result = BTreeMap::new();
        collect(path, path, &mut result);
        result
    }
    fn upgrade(
        connection: &mut rusqlite::Connection,
        info: &ProjectInfo,
    ) -> Result<ProjectInfo, ProjectError> {
        let transaction = connection.transaction().map_err(ProjectError::database)?;
        transaction
            .execute_batch("DROP TABLE project_metadata;")
            .map_err(ProjectError::database)?;
        transaction
            .execute_batch(crate::source_schema::SCHEMA)
            .map_err(ProjectError::database)?;
        transaction.execute("INSERT INTO project_metadata (singleton, project_id, schema_version, created_at, state) VALUES (1, ?1, 2, ?2, ?3)", rusqlite::params![info.project_id, info.created_at, info.state]).map_err(ProjectError::database)?;
        transaction.commit().map_err(ProjectError::database)?;
        let mut updated = info.clone();
        updated.schema_version = 2;
        Ok(updated)
    }
    #[test]
    fn beginning_then_aborting_preserves_original_project_and_sentinel() {
        let sandbox = Sandbox::new();
        let (root, info) = sandbox.project("abort");
        let before = snapshot(&sandbox.0);
        let mut writer = Writer::begin(&root).unwrap();
        assert_eq!(writer.info(), &info);
        assert!(root.join(".noetome-writing/old-project.json").is_file());
        assert!(root.join(".noetome-writing/old-project.sqlite").is_file());
        assert_eq!(
            project::inspect(&root).unwrap_err().code(),
            "PROJECT_INCOMPLETE"
        );
        writer.abort().unwrap();
        assert_eq!(snapshot(&sandbox.0), before);
        assert_eq!(project::inspect(&root).unwrap(), info);
    }
    #[test]
    fn callback_failure_does_not_change_original_database_or_config() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("callback-failure");
        let before = snapshot(&sandbox.0);
        let mut writer = Writer::begin(&root).unwrap();
        let result: Result<(), ProjectError> = writer.build_candidate(|connection, _| {
            connection
                .execute_batch("CREATE TABLE temporary_test_owned (value TEXT)")
                .map_err(ProjectError::database)?;
            Err(ProjectError::new(
                "TEST_CONTROLLED_FAILURE",
                3,
                "callback deliberately failed",
            ))
        });
        assert_eq!(result.unwrap_err().code(), "TEST_CONTROLLED_FAILURE");
        writer.abort().unwrap();
        assert_eq!(snapshot(&sandbox.0), before);
    }

    #[test]
    fn drop_rolls_back_a_known_callback_failure_without_an_explicit_abort() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("drop-callback-failure");
        let before = snapshot(&sandbox.0);
        {
            let mut writer = Writer::begin(&root).unwrap();
            let result: Result<(), ProjectError> = writer.build_candidate(|connection, _| {
                connection
                    .execute_batch("CREATE TABLE test_owned_failure (value TEXT)")
                    .map_err(ProjectError::database)?;
                Err(ProjectError::new(
                    "TEST_CONTROLLED_FAILURE",
                    3,
                    "known callback failed",
                ))
            });
            assert_eq!(result.unwrap_err().code(), "TEST_CONTROLLED_FAILURE");
        }
        assert_eq!(snapshot(&sandbox.0), before);
        assert_eq!(project::inspect(&root).unwrap().schema_version, 1);
    }
    #[test]
    fn successful_commit_upgrades_identity_and_publishes_owned_snapshots() {
        let sandbox = Sandbox::new();
        let (root, info) = sandbox.project("commit");
        let version = Uuid::new_v4().to_string();
        let mut writer = Writer::begin(&root).unwrap();
        let updated = writer.build_candidate(upgrade).unwrap();
        let final_directory = writer
            .stage_version(&version, b"raw\r\n", Some(b"raw\n"))
            .unwrap();
        assert_eq!(
            final_directory,
            root.join(".noetome-sources").join(&version)
        );
        let mut validation_ran = false;
        writer
            .commit(&updated, |path, marker| {
                validation_ran = true;
                assert_eq!(project::inspect_for_writer(path, marker)?, updated);
                assert_eq!(
                    fs::read(final_directory.join("raw.bin")).unwrap(),
                    b"raw\r\n"
                );
                assert_eq!(
                    fs::read(final_directory.join("normalized.txt")).unwrap(),
                    b"raw\n"
                );
                Ok(())
            })
            .unwrap();
        assert!(validation_ran);
        assert_eq!(project::inspect(&root).unwrap(), updated);
        assert_eq!(updated.project_id, info.project_id);
        assert_eq!(updated.created_at, info.created_at);
        assert!(!root.join(".noetome-writing").exists());
        let owner: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(".noetome-sources/.owner.json")).unwrap())
                .unwrap();
        assert_eq!(
            owner,
            serde_json::json!({"schema_version": 1, "project_id": info.project_id})
        );
        assert_eq!(
            fs::read(sandbox.0.join("outside-sentinel.txt")).unwrap(),
            b"keep unchanged"
        );
    }

    fn validate_project(path: &Path, marker: &same_file::Handle) -> Result<(), ProjectError> {
        project::inspect_for_writer(path, marker).map(|_| ())
    }

    #[test]
    fn pdf_raw_publication_is_an_independent_single_link_copy() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("pdf-copy");
        let version = Uuid::new_v4().to_string();
        let mut writer = Writer::begin(&root).unwrap();
        let updated = writer.build_candidate(upgrade).unwrap();
        let destination = writer.stage_pdf_raw(&version, b"owned PDF raw").unwrap();
        let staging = root.join(MARKER).join("version/raw.bin");
        writer
            .commit(&updated, |path, marker| {
                validate_project(path, marker)?;
                assert!(!same_file::is_same_file(&staging, destination.join("raw.bin")).unwrap());
                let staged = File::open(&staging).unwrap();
                let published = File::open(destination.join("raw.bin")).unwrap();
                assert_eq!(
                    noetome_sidecar_platform::file_link_count(&staged).unwrap(),
                    1
                );
                assert_eq!(
                    noetome_sidecar_platform::file_link_count(&published).unwrap(),
                    1
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(
            fs::read(destination.join("raw.bin")).unwrap(),
            b"owned PDF raw"
        );
    }

    fn pdf_files() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            ("report.json".into(), b"{}\n".to_vec()),
            ("artifacts/A1.bin".into(), b"inventory".to_vec()),
            ("artifacts/A2.txt".into(), b"text".to_vec()),
        ])
    }

    fn selected_real_parser() -> PdfParserSpec {
        PdfParserSpec {
            python: PathBuf::from(
                std::env::var_os("NOETOME_TEST_PDF_PYTHON")
                    .expect("Set NOETOME_TEST_PDF_PYTHON to an explicitly selected pinned runtime"),
            ),
            adapter: pdf_parse::default_adapter().unwrap(),
        }
    }

    #[test]
    fn pdf_real_parser_handoff_ready_and_failed_trees_are_owned_and_removable() {
        let sandbox = Sandbox::new();
        for filename in ["single-column.pdf", "damaged.pdf"] {
            let (root, _) = sandbox.project(filename);
            let before = snapshot(&sandbox.0);
            let raw = fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../fixtures/pdf-text-v1")
                    .join(filename),
            )
            .unwrap();
            let mut writer = Writer::begin(&root).unwrap();
            let output = writer
                .run_pdf_parser(
                    &selected_real_parser(),
                    &raw,
                    PdfLimits::default(),
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(output.report.input.sha256, crate::sidecar::digest(&raw));
            assert_eq!(output.report.input.byte_length, raw.len() as u64);
            assert_eq!(output.failed, filename == "damaged.pdf");
            let actual = writer.pdf_artifact_bytes(&output.report).unwrap();
            assert_eq!(actual.len(), output.report.verified_artifact_count as usize);
            for (name, bytes) in actual {
                assert_eq!(
                    fs::read(root.join(MARKER).join("parser").join(name)).unwrap(),
                    bytes
                );
            }
            writer.ensure_integrity().unwrap();
            writer.abort().unwrap();
            assert_eq!(snapshot(&sandbox.0), before);
        }
    }

    #[test]
    fn pdf_preflight_without_a_parser_tree_can_abort_but_cancel_preserves() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("pdf-preflight");
        let before = snapshot(&sandbox.0);
        let mut writer = Writer::begin(&root).unwrap();
        let mut parser = selected_real_parser();
        parser.python = PathBuf::from("untrusted-relative-python");
        assert_eq!(
            writer
                .run_pdf_parser(
                    &parser,
                    b"PDF raw",
                    PdfLimits::default(),
                    &AtomicBool::new(false)
                )
                .unwrap_err()
                .code(),
            "PDF_RUNTIME_INVALID"
        );
        assert!(!root.join(MARKER).join("parser").exists());
        writer.abort().unwrap();
        assert_eq!(snapshot(&sandbox.0), before);
        let mut writer = Writer::begin(&root).unwrap();
        assert_eq!(
            writer
                .run_pdf_parser(
                    &parser,
                    b"PDF raw",
                    PdfLimits::default(),
                    &AtomicBool::new(true)
                )
                .unwrap_err()
                .code(),
            "SIDECAR_CANCELLED"
        );
        assert!(writer.preserve);
        let preserved = snapshot(&sandbox.0);
        drop(writer);
        assert_eq!(snapshot(&sandbox.0), preserved);
    }

    #[test]
    fn pdf_actual_unknown_parser_output_preserves_the_unadopted_tree() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("pdf-untracked");
        let mut parser = selected_real_parser();
        // An explicitly marked fault adapter exercises native process output,
        // not PDF correctness. Its unknown file must never become owned data.
        parser.adapter = sandbox.0.join("fault_unknown.py");
        fs::write(&parser.adapter,b"from pathlib import Path\nPath('unknown.txt').write_bytes(b'preserve unknown')\nprint('{}')\n").unwrap();
        let mut writer = Writer::begin(&root).unwrap();
        assert!(
            writer
                .run_pdf_parser(
                    &parser,
                    b"PDF raw",
                    PdfLimits::default(),
                    &AtomicBool::new(false)
                )
                .is_err()
        );
        assert!(writer.preserve);
        assert_eq!(
            fs::read(root.join(MARKER).join("parser/unknown.txt")).unwrap(),
            b"preserve unknown"
        );
        let retained = snapshot(&sandbox.0);
        drop(writer);
        assert_eq!(snapshot(&sandbox.0), retained);
        assert_eq!(
            project::inspect(&root).unwrap_err().code(),
            "PROJECT_INCOMPLETE"
        );
    }

    #[test]
    fn pdf_native_exit_two_is_a_retained_failed_report_and_can_abort() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("pdf-native-two");
        let mut parser = selected_real_parser();
        parser.adapter = sandbox.0.join("fault_exit_two.py");
        fs::write(
            &parser.adapter,
            b"import sys\nsys.stderr.write('explicit runtime fault')\nsys.exit(2)\n",
        )
        .unwrap();
        let before = snapshot(&sandbox.0);
        let mut writer = Writer::begin(&root).unwrap();
        let output = writer
            .run_pdf_parser(
                &parser,
                b"PDF raw",
                PdfLimits::default(),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert!(output.failed);
        assert_eq!(output.exit_code, 2);
        assert_eq!(output.report.native_exit_code, Some(2));
        assert!(output.report.response.is_none());
        assert!(
            writer
                .pdf_artifact_bytes(&output.report)
                .unwrap()
                .is_empty()
        );
        assert!(!writer.preserve);
        writer.abort().unwrap();
        assert_eq!(snapshot(&sandbox.0), before);
    }

    #[test]
    fn pdf_schema3_actual_kill_keeps_both_trees_candidate_and_backups() {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        const CHILD_ROOT: &str = "NOETOME_PDF_WRITER_CHILD_ROOT";
        const CHILD_STAGE: &str = "NOETOME_PDF_WRITER_CHILD_STAGE";
        const CHILD_READY: &str = "NOETOME_PDF_WRITER_CHILD_READY";
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let stage_name = std::env::var(CHILD_STAGE).unwrap();
            let ready = PathBuf::from(std::env::var_os(CHILD_READY).unwrap());
            let mut writer = Writer::begin(Path::new(&root)).unwrap();
            writer
                .stage_pdf_raw(&Uuid::new_v4().to_string(), b"PDF raw")
                .unwrap();
            writer
                .stage_pdf_parse(&Uuid::new_v4().to_string(), &pdf_files())
                .unwrap();
            writer.hook = Some(Box::new(move |stage, _| {
                if format!("{stage:?}") == stage_name {
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&ready)
                        .unwrap();
                    file.write_all(b"Schema3 with two staged trees reached\n")
                        .unwrap();
                    file.sync_all().unwrap();
                    loop {
                        std::thread::park_timeout(Duration::from_secs(1));
                    }
                }
                Ok(())
            }));
            let updated = writer
                .build_candidate(crate::pdf_source_schema::upgrade)
                .unwrap();
            writer.commit(&updated, validate_project).unwrap();
            panic!("missing native kill stage");
        }
        let sandbox = Sandbox::new();
        for stage in [
            WriteStage::Prepared,
            WriteStage::DatabasePublished,
            WriteStage::ConfigPublished,
        ] {
            let (root, _) = sandbox.project(&format!("pdf-kill-{stage:?}"));
            let old_config = fs::read(root.join(project::CONFIG)).unwrap();
            let old_database = fs::read(root.join(project::DATABASE)).unwrap();
            let ready = sandbox.0.join(format!("pdf-ready-{stage:?}"));
            let mut child=Command::new(std::env::current_exe().unwrap()).args(["--exact","source_write::tests::pdf_schema3_actual_kill_keeps_both_trees_candidate_and_backups","--nocapture"])
                .env(CHILD_ROOT,&root).env(CHILD_STAGE,format!("{stage:?}")).env(CHILD_READY,&ready)
                .current_dir(&sandbox.0).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            while !ready.exists() && Instant::now() < deadline {
                if child.try_wait().unwrap().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            if !ready.exists() {
                let _ = child.kill();
                panic!(
                    "PDF child did not reach {stage:?}: {}",
                    child.wait().unwrap()
                );
            }
            assert!(child.try_wait().unwrap().is_none());
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            assert_eq!(
                fs::read(root.join(MARKER).join("old-project.json")).unwrap(),
                old_config
            );
            assert_eq!(
                fs::read(root.join(MARKER).join("old-project.sqlite")).unwrap(),
                old_database
            );
            assert_eq!(
                fs::read(root.join(MARKER).join("version/raw.bin")).unwrap(),
                b"PDF raw"
            );
            assert_eq!(
                fs::read(root.join(MARKER).join("parse/report.json")).unwrap(),
                b"{}\n"
            );
            let database = if stage == WriteStage::Prepared {
                root.join(MARKER).join(CANDIDATE_DB)
            } else {
                root.join(project::DATABASE)
            };
            let connection =
                Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
            assert_eq!(
                connection
                    .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                    .unwrap(),
                3
            );
            drop(connection);
            if stage != WriteStage::Prepared {
                for cache in [CACHE, PDF_CACHE] {
                    assert_eq!(directory_names(&root.join(cache)).unwrap().len(), 2);
                    assert!(root.join(cache).join(OWNER).is_file());
                }
            }
            let preserved = snapshot(&sandbox.0);
            assert_eq!(
                project::inspect(&root).unwrap_err().code(),
                "PROJECT_INCOMPLETE"
            );
            assert_eq!(snapshot(&sandbox.0), preserved);
        }
    }

    #[test]
    fn pdf_raw_and_parse_publish_as_two_independent_exact_trees() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("pdf-two-trees");
        let mut writer = Writer::begin(&root).unwrap();
        let updated = writer.build_candidate(upgrade).unwrap();
        let raw = writer
            .stage_pdf_raw(&Uuid::new_v4().to_string(), b"PDF raw")
            .unwrap();
        let parse = writer
            .stage_pdf_parse(&Uuid::new_v4().to_string(), &pdf_files())
            .unwrap();
        writer
            .commit(&updated, |path, marker| {
                validate_project(path, marker)?;
                for (staged, published) in [
                    (
                        root.join(MARKER).join("version/raw.bin"),
                        raw.join("raw.bin"),
                    ),
                    (
                        root.join(MARKER).join("parse/report.json"),
                        parse.join("report.json"),
                    ),
                    (
                        root.join(MARKER).join("parse/artifacts/A1.bin"),
                        parse.join("artifacts/A1.bin"),
                    ),
                    (
                        root.join(MARKER).join("parse/artifacts/A2.txt"),
                        parse.join("artifacts/A2.txt"),
                    ),
                ] {
                    assert!(!same_file::is_same_file(&staged, &published).unwrap());
                    assert_eq!(
                        noetome_sidecar_platform::file_link_count(&File::open(staged).unwrap())
                            .unwrap(),
                        1
                    );
                    assert_eq!(
                        noetome_sidecar_platform::file_link_count(&File::open(published).unwrap())
                            .unwrap(),
                        1
                    );
                }
                assert_eq!(
                    directory_names(&parse).unwrap(),
                    BTreeSet::from([OsString::from("artifacts"), OsString::from("report.json")])
                );
                assert_eq!(
                    directory_names(&raw).unwrap(),
                    BTreeSet::from([OsString::from("raw.bin")])
                );
                Ok(())
            })
            .unwrap();
        assert!(!root.join(MARKER).exists());
        assert_eq!(fs::read(raw.join("raw.bin")).unwrap(), b"PDF raw");
        for (name, bytes) in pdf_files() {
            assert_eq!(fs::read(parse.join(name)).unwrap(), bytes);
        }
        for cache in [CACHE, PDF_CACHE] {
            let owner: CacheOwner =
                serde_json::from_slice(&fs::read(root.join(cache).join(OWNER)).unwrap()).unwrap();
            assert_eq!(owner.project_id, updated.project_id);
            assert_eq!(owner.schema_version, 1);
        }
    }

    #[test]
    fn pdf_parse_rejects_unsafe_or_incomplete_inventory_before_creating_a_tree() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("bad-parse-path");
        let mut writer = Writer::begin(&root).unwrap();
        for files in [
            BTreeMap::new(),
            BTreeMap::from([("unknown.txt".into(), vec![])]),
            BTreeMap::from([
                ("report.json".into(), vec![]),
                ("artifacts/../escape.bin".into(), vec![]),
            ]),
            BTreeMap::from([
                ("report.json".into(), vec![]),
                ("artifacts/A.bin".into(), vec![]),
                ("artifacts/a.bin".into(), vec![]),
            ]),
            BTreeMap::from([("report.json".into(), vec![]), ("raw.bin".into(), vec![])]),
        ] {
            let before = snapshot(&sandbox.0);
            assert!(
                writer
                    .stage_pdf_parse(&Uuid::new_v4().to_string(), &files)
                    .is_err()
            );
            assert_eq!(snapshot(&sandbox.0), before);
        }
        writer.abort().unwrap();
    }

    #[test]
    fn pdf_additional_hard_link_and_unknown_parse_content_preserve_the_scene() {
        let sandbox = Sandbox::new();
        for hard_link in [true, false] {
            let (root, _) = sandbox.project(&format!("pdf-preserve-{hard_link}"));
            let old_config = fs::read(root.join(project::CONFIG)).unwrap();
            let old_database = fs::read(root.join(project::DATABASE)).unwrap();
            let mut writer = Writer::begin(&root).unwrap();
            let updated = writer.build_candidate(upgrade).unwrap();
            let raw = writer
                .stage_pdf_raw(&Uuid::new_v4().to_string(), b"PDF raw")
                .unwrap();
            let parse = writer
                .stage_pdf_parse(&Uuid::new_v4().to_string(), &pdf_files())
                .unwrap();
            let raw_hook = raw.clone();
            let parse_hook = parse.clone();
            let alias = sandbox.0.join("external-raw-alias");
            let alias_hook = alias.clone();
            writer.hook = Some(Box::new(move |stage, _| {
                if stage == WriteStage::VersionPublished {
                    if hard_link {
                        fs::hard_link(raw_hook.join("raw.bin"), &alias_hook).unwrap();
                    } else {
                        fs::write(parse_hook.join("unknown.txt"), b"must survive").unwrap();
                    }
                }
                Ok(())
            }));
            assert_eq!(
                writer
                    .commit(&updated, validate_project)
                    .unwrap_err()
                    .code(),
                "SOURCE_WRITE_INCOMPLETE"
            );
            assert_eq!(
                writer.abort().unwrap_err().code(),
                "SOURCE_WRITE_INCOMPLETE"
            );
            let retained = snapshot(&sandbox.0);
            drop(writer);
            assert_eq!(snapshot(&sandbox.0), retained);
            assert_eq!(fs::read(root.join(project::CONFIG)).unwrap(), old_config);
            assert_eq!(
                fs::read(root.join(project::DATABASE)).unwrap(),
                old_database
            );
            assert!(root.join(MARKER).exists());
            assert_eq!(fs::read(raw.join("raw.bin")).unwrap(), b"PDF raw");
            if hard_link {
                fs::remove_file(alias).unwrap();
            }
        }
    }

    #[test]
    fn pdf_schema3_dual_tree_failures_restore_all_six_stages() {
        let sandbox = Sandbox::new();
        for point in [
            WriteStage::Prepared,
            WriteStage::VersionPublished,
            WriteStage::DatabasePublished,
            WriteStage::ConfigPublished,
            WriteStage::Validated,
            WriteStage::Cleanup,
        ] {
            let (root, _) = sandbox.project(&format!("pdf-six-{point:?}"));
            let before = snapshot(&sandbox.0);
            let mut writer = Writer::begin(&root).unwrap();
            writer
                .stage_pdf_raw(&Uuid::new_v4().to_string(), b"PDF raw")
                .unwrap();
            writer
                .stage_pdf_parse(&Uuid::new_v4().to_string(), &pdf_files())
                .unwrap();
            writer.hook = Some(Box::new(move |stage, _| {
                if stage == point {
                    Err(ProjectError::new(
                        "TEST_CONTROLLED_FAILURE",
                        3,
                        "PDF two-tree stage failure",
                    ))
                } else {
                    Ok(())
                }
            }));
            let candidate = writer.build_candidate(crate::pdf_source_schema::upgrade);
            if point == WriteStage::Prepared {
                assert_eq!(candidate.unwrap_err().code(), "TEST_CONTROLLED_FAILURE");
            } else {
                let updated = candidate.unwrap();
                assert_eq!(updated.schema_version, 3);
                assert_eq!(
                    writer
                        .commit(&updated, validate_project)
                        .unwrap_err()
                        .code(),
                    "TEST_CONTROLLED_FAILURE"
                );
            }
            writer.abort().unwrap();
            assert_eq!(snapshot(&sandbox.0), before);
            assert_eq!(project::inspect(&root).unwrap().schema_version, 1);
        }
    }

    #[test]
    fn controlled_failures_at_all_six_stages_restore_exact_original_bytes() {
        let sandbox = Sandbox::new();
        for point in [
            WriteStage::Prepared,
            WriteStage::VersionPublished,
            WriteStage::DatabasePublished,
            WriteStage::ConfigPublished,
            WriteStage::Validated,
            WriteStage::Cleanup,
        ] {
            let (root, _) = sandbox.project(&format!("failure-{point:?}"));
            let before = snapshot(&sandbox.0);
            let mut writer = Writer::begin(&root).unwrap();
            writer.hook = Some(Box::new(move |stage, _| {
                if stage == point {
                    Err(ProjectError::new(
                        "TEST_CONTROLLED_FAILURE",
                        3,
                        "known test failure",
                    ))
                } else {
                    Ok(())
                }
            }));
            let candidate = writer.build_candidate(upgrade);
            if point == WriteStage::Prepared {
                assert_eq!(candidate.unwrap_err().code(), "TEST_CONTROLLED_FAILURE");
            } else {
                let updated = candidate.unwrap();
                let version = Uuid::new_v4().to_string();
                writer
                    .stage_version(&version, b"raw", Some(b"raw"))
                    .unwrap();
                assert_eq!(
                    writer
                        .commit(&updated, validate_project)
                        .unwrap_err()
                        .code(),
                    "TEST_CONTROLLED_FAILURE"
                );
            }
            writer.abort().unwrap();
            assert_eq!(
                snapshot(&sandbox.0),
                before,
                "rollback changed files at {point:?}"
            );
            assert_eq!(project::inspect(&root).unwrap().schema_version, 1);
        }
    }

    #[test]
    fn candidate_replacement_and_unknown_transaction_or_root_content_are_preserved() {
        let sandbox = Sandbox::new();
        for change in [
            "same-byte-candidate",
            "unknown-marker-child",
            "unknown-root-child",
        ] {
            let (root, _) = sandbox.project(change);
            let mut writer = Writer::begin(&root).unwrap();
            writer.hook = Some(Box::new(move |stage, root| {
                if stage != WriteStage::Prepared {
                    return Ok(());
                }
                let marker = root.join(MARKER);
                match change {
                    "same-byte-candidate" => {
                        let path = marker.join(CANDIDATE_DB);
                        let bytes = fs::read(&path).unwrap();
                        fs::remove_file(&path).unwrap();
                        fs::write(path, bytes).unwrap();
                    }
                    "unknown-marker-child" => {
                        fs::write(marker.join("human-note.md"), b"unknown human content").unwrap();
                    }
                    "unknown-root-child" => {
                        fs::write(root.join("human-note.md"), b"unknown human content").unwrap();
                    }
                    _ => unreachable!(),
                }
                Err(ProjectError::new(
                    "TEST_CONTROLLED_FAILURE",
                    3,
                    "external test change",
                ))
            }));
            assert_eq!(
                writer.build_candidate(upgrade).unwrap_err().code(),
                "TEST_CONTROLLED_FAILURE"
            );
            let before = snapshot(&sandbox.0);
            assert_eq!(
                writer.abort().unwrap_err().code(),
                "SOURCE_WRITE_INCOMPLETE"
            );
            drop(writer);
            assert_eq!(
                snapshot(&sandbox.0),
                before,
                "deleted unknown/replaced {change}"
            );
            assert!(root.join(MARKER).join("old-project.sqlite").exists());
            assert_eq!(
                project::inspect(&root).unwrap_err().code(),
                "PROJECT_INCOMPLETE"
            );
        }
    }

    #[test]
    fn externally_deleted_backup_is_not_recreated_during_abort() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("external-backup-deletion");
        let mut writer = Writer::begin(&root).unwrap();
        writer.build_candidate(upgrade).unwrap();
        let backup = root.join(MARKER).join("old-project.sqlite");
        fs::remove_file(&backup).unwrap();
        let before = snapshot(&sandbox.0);
        assert_eq!(
            writer.abort().unwrap_err().code(),
            "SOURCE_WRITE_INCOMPLETE"
        );
        drop(writer);
        assert_eq!(snapshot(&sandbox.0), before);
        assert!(!backup.exists());
        assert!(root.join(MARKER).exists());
    }

    #[test]
    fn same_byte_replacement_of_published_targets_keeps_backups_and_marker() {
        let sandbox = Sandbox::new();
        for point in [WriteStage::DatabasePublished, WriteStage::ConfigPublished] {
            let (root, _) = sandbox.project(&format!("replace-{point:?}"));
            let mut writer = Writer::begin(&root).unwrap();
            let updated = writer.build_candidate(upgrade).unwrap();
            writer.hook = Some(Box::new(move |stage, root| {
                if stage == point {
                    let target = root.join(if point == WriteStage::DatabasePublished {
                        project::DATABASE
                    } else {
                        project::CONFIG
                    });
                    let bytes = fs::read(&target).unwrap();
                    fs::remove_file(&target).unwrap();
                    fs::write(target, bytes).unwrap();
                    return Err(ProjectError::new(
                        "TEST_CONTROLLED_FAILURE",
                        3,
                        "same byte target replacement",
                    ));
                }
                Ok(())
            }));
            assert_eq!(
                writer
                    .commit(&updated, validate_project)
                    .unwrap_err()
                    .code(),
                "TEST_CONTROLLED_FAILURE"
            );
            let before = snapshot(&sandbox.0);
            assert_eq!(
                writer.abort().unwrap_err().code(),
                "SOURCE_WRITE_INCOMPLETE"
            );
            drop(writer);
            assert_eq!(snapshot(&sandbox.0), before);
            assert!(root.join(MARKER).join("old-project.sqlite").exists());
            assert!(root.join(MARKER).join("old-project.json").exists());
        }
    }

    #[test]
    fn a_replaced_project_directory_and_its_original_moved_tree_are_untouched() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("original-directory");
        let moved = sandbox.0.join("moved-original-directory");
        let destination = moved.clone();
        let mut writer = Writer::begin(&root).unwrap();
        let updated = writer.build_candidate(upgrade).unwrap();
        writer.hook = Some(Box::new(move |stage, root| {
            if stage == WriteStage::Validated {
                // Windows rejects moving a nonempty directory containing open
                // file handles even when its file handles permit delete sharing.
                // Move the still-shared files separately, move the empty directory,
                // then reconstruct the exact original tree at its new location.
                let holding = destination.with_file_name("holding-original-files");
                fs::create_dir(&holding).unwrap();
                let holding_files = holding.join("marker-files");
                fs::create_dir(&holding_files).unwrap();
                let marker = root.join(MARKER);
                for entry in fs::read_dir(&marker).unwrap() {
                    let entry = entry.unwrap();
                    assert!(entry.file_type().unwrap().is_file());
                    fs::rename(entry.path(), holding_files.join(entry.file_name())).unwrap();
                }
                fs::rename(&marker, holding.join("marker-directory")).unwrap();
                for name in [project::CONFIG, project::DATABASE] {
                    fs::rename(root.join(name), holding.join(name)).unwrap();
                }
                fs::rename(root, &destination).unwrap();
                fs::rename(holding.join("marker-directory"), destination.join(MARKER)).unwrap();
                for entry in fs::read_dir(&holding_files).unwrap() {
                    let entry = entry.unwrap();
                    fs::rename(
                        entry.path(),
                        destination.join(MARKER).join(entry.file_name()),
                    )
                    .unwrap();
                }
                fs::remove_dir(&holding_files).unwrap();
                for name in [project::CONFIG, project::DATABASE] {
                    fs::rename(holding.join(name), destination.join(name)).unwrap();
                }
                fs::remove_dir(&holding).unwrap();
                fs::create_dir(root).unwrap();
                fs::write(root.join("replacement-note.md"), b"keep replacement tree").unwrap();
                return Err(ProjectError::new(
                    "TEST_CONTROLLED_FAILURE",
                    3,
                    "project directory replaced",
                ));
            }
            Ok(())
        }));
        assert_eq!(
            writer
                .commit(&updated, validate_project)
                .unwrap_err()
                .code(),
            "TEST_CONTROLLED_FAILURE"
        );
        let before = snapshot(&sandbox.0);
        assert_eq!(
            writer.abort().unwrap_err().code(),
            "SOURCE_WRITE_INCOMPLETE"
        );
        drop(writer);
        assert_eq!(snapshot(&sandbox.0), before);
        assert!(moved.join(MARKER).join("old-project.sqlite").exists());
        assert!(moved.join(MARKER).join("old-project.json").exists());
    }

    #[test]
    fn unknown_published_snapshot_content_is_preserved_without_rollback_deletion() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("unknown-cache");
        let mut writer = Writer::begin(&root).unwrap();
        let updated = writer.build_candidate(upgrade).unwrap();
        let final_directory = writer
            .stage_version(&Uuid::new_v4().to_string(), b"source", Some(b"source"))
            .unwrap();
        let changed = final_directory.clone();
        writer.hook = Some(Box::new(move |stage, _| {
            if stage == WriteStage::VersionPublished {
                fs::write(changed.join("human-note.md"), b"unmanaged snapshot child").unwrap();
                return Err(ProjectError::new(
                    "TEST_CONTROLLED_FAILURE",
                    3,
                    "unknown snapshot child",
                ));
            }
            Ok(())
        }));
        assert_eq!(
            writer
                .commit(&updated, validate_project)
                .unwrap_err()
                .code(),
            "TEST_CONTROLLED_FAILURE"
        );
        let before = snapshot(&sandbox.0);
        assert_eq!(
            writer.abort().unwrap_err().code(),
            "SOURCE_WRITE_INCOMPLETE"
        );
        drop(writer);
        assert_eq!(snapshot(&sandbox.0), before);
        assert!(final_directory.join("human-note.md").exists());
    }

    #[test]
    fn preexisting_unowned_cache_and_version_collision_are_never_taken_over() {
        let sandbox = Sandbox::new();
        for kind in [
            "unmarked",
            "wrong-owner",
            "future-owner",
            "oversized-owner",
            "version-collision",
        ] {
            let (root, info) = sandbox.project(kind);
            let cache = root.join(CACHE);
            fs::create_dir(&cache).unwrap();
            let version = Uuid::new_v4().to_string();
            match kind {
                "unmarked" => {
                    fs::write(cache.join("human-note.md"), b"keep unmarked cache").unwrap();
                }
                "wrong-owner" => {
                    fs::write(cache.join(OWNER), serde_json::to_vec(&serde_json::json!({"schema_version":1,"project_id":Uuid::new_v4().to_string()})).unwrap()).unwrap();
                }
                "future-owner" => {
                    fs::write(
                        cache.join(OWNER),
                        serde_json::to_vec(
                            &serde_json::json!({"schema_version":99,"project_id":info.project_id}),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                }
                "oversized-owner" => {
                    fs::write(cache.join(OWNER), vec![b'x'; 4097]).unwrap();
                }
                "version-collision" => {
                    fs::write(
                        cache.join(OWNER),
                        serde_json::to_vec(
                            &serde_json::json!({"schema_version":1,"project_id":info.project_id}),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                    fs::create_dir(cache.join(&version)).unwrap();
                    fs::write(
                        cache.join(&version).join("human-note.md"),
                        b"keep existing version",
                    )
                    .unwrap();
                }
                _ => unreachable!(),
            }
            let before = snapshot(&sandbox.0);
            let mut writer = Writer::begin(&root).unwrap();
            let updated = writer.build_candidate(upgrade).unwrap();
            writer
                .stage_version(&version, b"new raw", Some(b"new raw"))
                .unwrap();
            assert_eq!(
                writer
                    .commit(&updated, validate_project)
                    .unwrap_err()
                    .code(),
                "SOURCE_SNAPSHOT_CONFLICT"
            );
            writer.abort().unwrap();
            assert_eq!(snapshot(&sandbox.0), before, "modified preexisting {kind}");
        }
    }

    #[test]
    fn invalid_internal_ids_or_project_identity_changes_do_not_publish() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("invalid-info");
        let before = snapshot(&sandbox.0);
        let mut writer = Writer::begin(&root).unwrap();
        let updated = writer.build_candidate(upgrade).unwrap();
        assert!(writer.candidate_path().is_file());
        for id in [
            "../outside",
            "00000000-0000-4000-0000-000000000000",
            "not-an-id",
        ] {
            assert_eq!(
                writer.stage_version(id, b"raw", None).unwrap_err().code(),
                "SOURCE_INVALID_VERSION_ID"
            );
        }
        let mut changed = updated.clone();
        changed.project_id = Uuid::new_v4().to_string();
        assert_eq!(
            writer
                .commit(&changed, validate_project)
                .unwrap_err()
                .code(),
            "SOURCE_INVALID_PROJECT_INFO"
        );
        writer.abort().unwrap();
        assert_eq!(snapshot(&sandbox.0), before);
    }

    #[test]
    fn second_commit_preserves_existing_cache_and_raw_only_versions() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("repeat-cache");
        let first_directory = {
            let mut writer = Writer::begin(&root).unwrap();
            let updated = writer.build_candidate(upgrade).unwrap();
            let directory = writer
                .stage_version(&Uuid::new_v4().to_string(), b"\xff", None)
                .unwrap();
            writer.commit(&updated, validate_project).unwrap();
            directory
        };
        assert!(!first_directory.join("normalized.txt").exists());
        let first_bytes = snapshot(&root.join(CACHE));
        let before = snapshot(&sandbox.0);
        let mut writer = Writer::begin(&root).unwrap();
        let unchanged_info = writer.build_candidate(|_, info| Ok(info.clone())).unwrap();
        let new_directory = writer
            .stage_version(&Uuid::new_v4().to_string(), b"raw2", Some(b"raw2"))
            .unwrap();
        writer.hook = Some(Box::new(|stage, _| {
            if stage == WriteStage::ConfigPublished {
                Err(ProjectError::new(
                    "TEST_CONTROLLED_FAILURE",
                    3,
                    "second publication failed",
                ))
            } else {
                Ok(())
            }
        }));
        assert_eq!(
            writer
                .commit(&unchanged_info, validate_project)
                .unwrap_err()
                .code(),
            "TEST_CONTROLLED_FAILURE"
        );
        writer.abort().unwrap();
        assert_eq!(snapshot(&root.join(CACHE)), first_bytes);
        assert_eq!(snapshot(&sandbox.0), before);
        assert!(!new_directory.exists());
    }

    #[cfg(windows)]
    #[test]
    fn actual_windows_delete_sharing_failure_restores_old_project() {
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("windows-publish-lock");
        let before = snapshot(&sandbox.0);
        let mut writer = Writer::begin(&root).unwrap();
        let updated = writer.build_candidate(upgrade).unwrap();
        writer
            .stage_version(&Uuid::new_v4().to_string(), b"raw", None)
            .unwrap();
        let external_guard = project::read_guard(&root.join(project::DATABASE)).unwrap();
        let error = writer.commit(&updated, validate_project).unwrap_err();
        assert_eq!(error.exit_code(), 3);
        writer.abort().unwrap();
        assert_eq!(snapshot(&sandbox.0), before);
        drop(external_guard);
    }

    #[cfg(windows)]
    #[test]
    fn actual_cleanup_delete_failure_keeps_validated_files_marker_and_both_backups() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("windows-cleanup-lock");
        let old_config = fs::read(root.join(project::CONFIG)).unwrap();
        let old_database = fs::read(root.join(project::DATABASE)).unwrap();
        let mut writer = Writer::begin(&root).unwrap();
        let updated = writer.build_candidate(upgrade).unwrap();
        let held = Rc::new(RefCell::new(None));
        let held_in_hook = held.clone();
        let removed_identity = Rc::new(RefCell::new(None));
        let removed_identity_in_hook = removed_identity.clone();
        writer.hook = Some(Box::new(move |stage, root| {
            if stage == WriteStage::Validated {
                *removed_identity_in_hook.borrow_mut() = Some(
                    same_file::Handle::from_path(root.join(MARKER).join("old-project.sqlite"))
                        .map_err(|error| ProjectError::io(root, error))?,
                );
                *held_in_hook.borrow_mut() = Some(project::read_guard(
                    &root.join(MARKER).join("old-project.json"),
                )?);
            }
            Ok(())
        }));
        assert_eq!(
            writer
                .commit(&updated, validate_project)
                .unwrap_err()
                .code(),
            "SOURCE_WRITE_INCOMPLETE"
        );
        let after_failure = snapshot(&sandbox.0);
        assert_eq!(
            writer.abort().unwrap_err().code(),
            "SOURCE_WRITE_INCOMPLETE"
        );
        drop(writer);
        assert_eq!(snapshot(&sandbox.0), after_failure);
        assert_eq!(
            fs::read(root.join(MARKER).join("old-project.json")).unwrap(),
            old_config
        );
        assert_eq!(
            fs::read(root.join(MARKER).join("old-project.sqlite")).unwrap(),
            old_database
        );
        let recreated =
            same_file::Handle::from_path(root.join(MARKER).join("old-project.sqlite")).unwrap();
        assert_ne!(
            &recreated,
            removed_identity.borrow().as_ref().unwrap(),
            "test did not exercise deleted-then-recreated database backup"
        );
        assert_eq!(
            project::inspect(&root).unwrap_err().code(),
            "PROJECT_INCOMPLETE"
        );
        drop(held);
    }

    #[cfg(windows)]
    #[test]
    fn actual_abort_snapshot_delete_failure_restores_pair_and_retains_both_backups() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let sandbox = Sandbox::new();
        let (root, _) = sandbox.project("windows-abort-cleanup-lock");
        let old_config = fs::read(root.join(project::CONFIG)).unwrap();
        let old_database = fs::read(root.join(project::DATABASE)).unwrap();
        let mut writer = Writer::begin(&root).unwrap();
        let updated = writer.build_candidate(upgrade).unwrap();
        let final_directory = writer
            .stage_version(&Uuid::new_v4().to_string(), b"source", Some(b"source"))
            .unwrap();
        let lock_target = final_directory.join("normalized.txt");
        let held = Rc::new(RefCell::new(None));
        let held_in_hook = held.clone();
        writer.hook = Some(Box::new(move |stage, _| {
            if stage == WriteStage::Validated {
                *held_in_hook.borrow_mut() = Some(project::read_guard(&lock_target)?);
                return Err(ProjectError::new(
                    "TEST_CONTROLLED_FAILURE",
                    3,
                    "validated stage interrupted",
                ));
            }
            Ok(())
        }));
        assert_eq!(
            writer
                .commit(&updated, validate_project)
                .unwrap_err()
                .code(),
            "TEST_CONTROLLED_FAILURE"
        );
        assert_eq!(
            writer.abort().unwrap_err().code(),
            "SOURCE_WRITE_INCOMPLETE"
        );
        assert_eq!(fs::read(root.join(project::CONFIG)).unwrap(), old_config);
        assert_eq!(
            fs::read(root.join(project::DATABASE)).unwrap(),
            old_database
        );
        assert_eq!(
            fs::read(root.join(MARKER).join("old-project.json")).unwrap(),
            old_config
        );
        assert_eq!(
            fs::read(root.join(MARKER).join("old-project.sqlite")).unwrap(),
            old_database
        );
        assert!(final_directory.join("normalized.txt").exists());
        let retained = snapshot(&sandbox.0);
        drop(writer);
        assert_eq!(snapshot(&sandbox.0), retained);
        assert_eq!(
            project::inspect(&root).unwrap_err().code(),
            "PROJECT_INCOMPLETE"
        );
        drop(held);
    }

    #[test]
    fn actual_process_kill_preserves_backups_and_blocks_inspection_at_three_stages() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        const CHILD_ROOT: &str = "NOETOME_WRITER_UNIT_CHILD_ROOT";
        const CHILD_STAGE: &str = "NOETOME_WRITER_UNIT_CHILD_STAGE";
        const CHILD_READY: &str = "NOETOME_WRITER_UNIT_CHILD_READY";
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let stage_name = std::env::var(CHILD_STAGE).unwrap();
            let ready = PathBuf::from(std::env::var_os(CHILD_READY).unwrap());
            let mut writer = Writer::begin(Path::new(&root)).unwrap();
            writer.hook = Some(Box::new(move |stage, _| {
                if format!("{stage:?}") == stage_name {
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&ready)
                        .unwrap();
                    file.write_all(b"writer stage reached\n").unwrap();
                    file.sync_all().unwrap();
                    loop {
                        std::thread::park_timeout(Duration::from_secs(1));
                    }
                }
                Ok(())
            }));
            let updated = writer.build_candidate(upgrade).unwrap();
            writer
                .stage_version(&Uuid::new_v4().to_string(), b"source", Some(b"source"))
                .unwrap();
            writer.commit(&updated, validate_project).unwrap();
            panic!("child never reached requested writer stage");
        }
        let sandbox = Sandbox::new();
        for stage in [
            WriteStage::Prepared,
            WriteStage::DatabasePublished,
            WriteStage::ConfigPublished,
        ] {
            let (root, _) = sandbox.project(&format!("crash-{stage:?}"));
            let old_config = fs::read(root.join(project::CONFIG)).unwrap();
            let old_database = fs::read(root.join(project::DATABASE)).unwrap();
            let ready = sandbox.0.join(format!("ready-{stage:?}"));
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "source_write::tests::actual_process_kill_preserves_backups_and_blocks_inspection_at_three_stages", "--nocapture"])
                .env(CHILD_ROOT, &root).env(CHILD_STAGE, format!("{stage:?}")).env(CHILD_READY, &ready)
                .current_dir(&sandbox.0).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            while !ready.exists() && Instant::now() < deadline {
                if child.try_wait().unwrap().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            if !ready.exists() {
                let _ = child.kill();
                let status = child.wait().unwrap();
                panic!("child did not reach {stage:?}: {status}");
            }
            assert!(
                child.try_wait().unwrap().is_none(),
                "readiness does not prove a live process"
            );
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            assert_eq!(
                fs::read(root.join(MARKER).join("old-project.json")).unwrap(),
                old_config
            );
            assert_eq!(
                fs::read(root.join(MARKER).join("old-project.sqlite")).unwrap(),
                old_database
            );
            assert!(root.join(MARKER).join("operation.json").exists());
            let before_inspect = snapshot(&sandbox.0);
            assert_eq!(
                project::inspect(&root).unwrap_err().code(),
                "PROJECT_INCOMPLETE"
            );
            assert_eq!(snapshot(&sandbox.0), before_inspect);
            let (retry, _) = sandbox.project(&format!("retry-{stage:?}"));
            let mut retry_writer = Writer::begin(&retry).unwrap();
            let updated = retry_writer.build_candidate(upgrade).unwrap();
            retry_writer.commit(&updated, validate_project).unwrap();
            assert_eq!(project::inspect(&retry).unwrap(), updated);
        }
    }
}
