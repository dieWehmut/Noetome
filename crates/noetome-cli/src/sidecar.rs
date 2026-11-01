//! One selected input, one bounded child request, one verified response.
//! This transport's byte locators are not PDF or knowledge evidence.

use same_file::Handle;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use uuid::Uuid;

pub use crate::sidecar_model::*;

#[derive(Debug, Clone)]
pub struct AdapterSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub expected: AdapterIdentity,
}

#[derive(Debug, Clone, Copy)]
pub struct SidecarLimits {
    pub max_input_bytes: u64,
    pub max_request_bytes: usize,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub max_artifact_bytes: u64,
    pub max_total_artifact_bytes: u64,
    pub max_artifacts: u32,
    pub max_units: u32,
    pub max_issues: u32,
    pub timeout: Duration,
    pub cleanup_timeout: Duration,
}

impl Default for SidecarLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024 * 1024,
            max_request_bytes: 64 * 1024,
            max_stdout_bytes: 4 * 1024 * 1024,
            max_stderr_bytes: 256 * 1024,
            max_artifact_bytes: 16 * 1024 * 1024,
            max_total_artifact_bytes: 64 * 1024 * 1024,
            max_artifacts: 64,
            max_units: 4096,
            max_issues: 128,
            timeout: Duration::from_secs(30),
            cleanup_timeout: Duration::from_secs(2),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationScope {
    ProtocolOnly,
}

#[derive(Debug)]
pub struct VerifiedResponse {
    pub request: SidecarRequest,
    pub response: SidecarResponse,
    pub diagnostics: Vec<u8>,
    pub pid: u32,
    pub elapsed: Duration,
    pub validation_scope: ValidationScope,
}

#[derive(Debug)]
pub struct SidecarError {
    pub code: &'static str,
    pub message: String,
    pub diagnostics: Vec<u8>,
    pub pid: Option<u32>,
    pub violations: Vec<ProtocolViolation>,
    pub response: Option<SidecarResponse>,
}

impl SidecarError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            diagnostics: Vec::new(),
            pid: None,
            violations: Vec::new(),
            response: None,
        }
    }
}

impl fmt::Display for SidecarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for SidecarError {}

pub fn run_sidecar(
    adapter: &AdapterSpec,
    selected_input: &Path,
    new_workspace: &Path,
    limits: SidecarLimits,
    cancel: &AtomicBool,
) -> Result<VerifiedResponse, SidecarError> {
    #[cfg(not(windows))]
    return Err(SidecarError::new(
        "SIDECAR_UNSUPPORTED_PLATFORM",
        "The contained process host currently requires Windows 10 or later.",
    ));
    check_limits(&limits)?;
    if cancel.load(Ordering::Acquire) {
        return Err(SidecarError::new(
            "SIDECAR_CANCELLED",
            "Cancelled before reading the selected input.",
        ));
    }
    let input_path = checked_path(selected_input)?;
    let root_path = checked_path(new_workspace)?;
    if root_path.starts_with(&input_path) || input_path.starts_with(&root_path) {
        return Err(SidecarError::new(
            "SIDECAR_WORKSPACE_CONFLICT",
            "Input and workspace overlap.",
        ));
    }
    if path_exists(&root_path)? {
        return Err(SidecarError::new(
            "SIDECAR_WORKSPACE_CONFLICT",
            "The requested workspace already exists; its contents were preserved.",
        ));
    }
    let input_parent = input_path.parent().ok_or_else(|| {
        SidecarError::new("SIDECAR_INPUT_INVALID", "Input has no parent directory.")
    })?;
    let _input_dirs = guard_chain(input_parent)?;
    let input = open_input(&input_path)?;
    let input_identity = identity(&input)?;
    let raw = bounded_read(
        &input,
        limits.max_input_bytes,
        "SIDECAR_INPUT_LIMIT",
        cancel,
    )?;
    ensure_identity(&input_path, &input_identity, "SIDECAR_INPUT_INVALID")?;
    check_cancel(cancel)?;
    let request = SidecarRequest {
        schema_version: 1,
        protocol: "noetome-parser-single-v1".into(),
        request_id: Uuid::new_v4().to_string(),
        operation: Operation::InspectBytes,
        input: InputFingerprint {
            relative_path: "input/raw.bin".into(),
            byte_length: raw.len() as u64,
            sha256: digest(&raw),
        },
        workspace: WorkspacePolicy {
            artifact_directory: "artifacts".into(),
        },
        artifact_policy: ArtifactPolicy {
            max_file_bytes: limits.max_artifact_bytes,
            max_total_bytes: limits.max_total_artifact_bytes,
            max_files: limits.max_artifacts,
        },
    };
    let request_bytes = serde_json::to_vec(&request)
        .map_err(|e| SidecarError::new("SIDECAR_PROTOCOL_INVALID", e.to_string()))?;
    if request_bytes.len() > limits.max_request_bytes {
        return Err(SidecarError::new(
            "SIDECAR_REQUEST_LIMIT",
            "Serialized request exceeds the configured byte limit; no workspace was created.",
        ));
    }
    check_cancel(cancel)?;
    let workspace = Workspace::create(root_path, &request, &request_bytes, &raw, cancel)?;
    workspace.verify(cancel)?;
    let output =
        crate::sidecar_process::execute(adapter, &request_bytes, &workspace.root, &limits, cancel)?;
    if let Err(mut error) = check_cancel(cancel) {
        error.pid = Some(output.pid);
        error.diagnostics = output.stderr;
        return Err(error);
    }
    let response = serde_json::from_slice::<SidecarResponse>(&output.stdout).map_err(|error| {
        let mut failure = SidecarError::new(
            "SIDECAR_PROTOCOL_INVALID",
                format!(
                    "Expected exactly one strict UTF8 JSON response; decode category {:?} at line {}, column {}.",
                    error.classify(), error.line(), error.column()
                ),
        );
        failure.diagnostics = output.stderr.clone();
        failure.pid = Some(output.pid);
        failure
    })?;
    let verification = (|| {
        check_cancel(cancel)?;
        workspace.verify(cancel)?;
        ensure_identity(&input_path, &input_identity, "SIDECAR_INPUT_INVALID")?;
        let (artifact_bytes, artifact_guards) =
            workspace.read_artifacts(&response, &limits, cancel)?;
        check_cancel(cancel)?;
        let violations = crate::sidecar_model::validate_response(
            &request,
            &response,
            &adapter.expected,
            &raw,
            &artifact_bytes,
            &limits,
        );
        let malformed = violations.iter().any(|issue| {
            response.status == ResponseStatus::Complete
                || issue.rule_id != "SIDECAR_REPORTED_FAILURE"
        });
        if malformed {
            let mut error = SidecarError::new(
                "SIDECAR_PROTOCOL_INVALID",
                "Response failed identity, completeness, capability or byte-range validation.",
            );
            error.violations = violations;
            return Err(error);
        }
        for guard in &artifact_guards {
            guard.verify(cancel).map_err(|error| {
                if error.code == "SIDECAR_CANCELLED" {
                    error
                } else {
                    io_failure("SIDECAR_RESOURCE_INVALID", error)
                }
            })?;
        }
        workspace.verify(cancel)?;
        workspace.verify_artifact_inventory(&response)?;
        check_cancel(cancel)?;
        if response.status != ResponseStatus::Complete {
            let mut error = SidecarError::new(
                "SIDECAR_INCOMPLETE",
                "Validated partial/failed response is preserved; complete parsing was not reported.",
            );
            error.response = Some(response.clone());
            error.violations = violations;
            return Err(error);
        }
        Ok(())
    })();
    if let Err(mut error) = verification {
        error.diagnostics = output.stderr;
        error.pid = Some(output.pid);
        return Err(error);
    }
    Ok(VerifiedResponse {
        request,
        response,
        diagnostics: output.stderr,
        pid: output.pid,
        elapsed: output.elapsed,
        validation_scope: ValidationScope::ProtocolOnly,
    })
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), SidecarError> {
    if cancel.load(Ordering::Acquire) {
        Err(SidecarError::new(
            "SIDECAR_CANCELLED",
            "Cancelled before completing Host acquisition or verification.",
        ))
    } else {
        Ok(())
    }
}

fn check_limits(limits: &SidecarLimits) -> Result<(), SidecarError> {
    let ceiling = SidecarLimits::default();
    if limits.max_input_bytes > ceiling.max_input_bytes
        || limits.max_request_bytes > ceiling.max_request_bytes
        || limits.max_stdout_bytes > ceiling.max_stdout_bytes
        || limits.max_stderr_bytes > ceiling.max_stderr_bytes
        || limits.max_artifact_bytes > ceiling.max_artifact_bytes
        || limits.max_total_artifact_bytes > ceiling.max_total_artifact_bytes
        || limits.max_artifacts > ceiling.max_artifacts
        || limits.max_units > ceiling.max_units
        || limits.max_issues > ceiling.max_issues
        || limits.timeout > ceiling.timeout
        || limits.cleanup_timeout > ceiling.cleanup_timeout
        || limits.cleanup_timeout.is_zero()
    {
        return Err(SidecarError::new(
            "SIDECAR_LIMITS_INVALID",
            "Public host policies may only tighten the bounded defaults; cleanup requires a positive deadline.",
        ));
    }
    Ok(())
}

fn io_failure(code: &'static str, error: impl fmt::Display) -> SidecarError {
    SidecarError::new(code, error.to_string())
}

fn checked_path(path: &Path) -> Result<PathBuf, SidecarError> {
    let path =
        crate::project::check_components(path).map_err(|e| io_failure("SIDECAR_UNSAFE_PATH", e))?;
    if path.as_os_str().to_string_lossy().encode_utf16().count() > 1024 {
        return Err(SidecarError::new(
            "SIDECAR_UNSAFE_PATH",
            "Selected paths are limited to 1024 UTF16 code units.",
        ));
    }
    Ok(path)
}

fn path_exists(path: &Path) -> Result<bool, SidecarError> {
    crate::project::exists(path).map_err(|e| io_failure("SIDECAR_WORKSPACE_CONFLICT", e))
}

fn identity(file: &File) -> Result<Handle, SidecarError> {
    Handle::from_file(
        file.try_clone()
            .map_err(|e| io_failure("SIDECAR_RESOURCE_INVALID", e))?,
    )
    .map_err(|e| io_failure("SIDECAR_RESOURCE_INVALID", e))
}

fn ensure_identity(path: &Path, expected: &Handle, code: &'static str) -> Result<(), SidecarError> {
    checked_path(path)?;
    let actual = Handle::from_path(path).map_err(|e| io_failure(code, e))?;
    if &actual != expected {
        return Err(SidecarError::new(
            code,
            "An owned file or directory identity changed; the scene was preserved.",
        ));
    }
    Ok(())
}

fn guard_directory(path: &Path) -> Result<Handle, SidecarError> {
    checked_path(path)?;
    let meta =
        fs::symlink_metadata(path).map_err(|e| io_failure("SIDECAR_WORKSPACE_CONFLICT", e))?;
    if !meta.is_dir() || crate::project::is_link(&meta) {
        return Err(SidecarError::new(
            "SIDECAR_UNSAFE_PATH",
            "Directory must be ordinary, existing and free of reparse points.",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Metadata-only access. Permit child entries to be created, but deny
        // deleting/renaming the directory while its identity is in use.
        options
            .access_mode(0)
            .share_mode(3)
            .custom_flags(0x02200000);
    }
    let file = options
        .open(path)
        .map_err(|e| io_failure("SIDECAR_UNSAFE_PATH", e))?;
    let handle = identity(&file)?;
    ensure_identity(path, &handle, "SIDECAR_WORKSPACE_CONFLICT")?;
    Ok(handle)
}

fn guard_chain(path: &Path) -> Result<Vec<(PathBuf, Handle)>, SidecarError> {
    let mut paths: Vec<_> = path.ancestors().map(Path::to_path_buf).collect();
    paths.reverse();
    paths
        .into_iter()
        .map(|p| Ok((p.clone(), guard_directory(&p)?)))
        .collect()
}

fn open_input(path: &Path) -> Result<File, SidecarError> {
    crate::project::checked_file(path).map_err(|e| io_failure("SIDECAR_INPUT_INVALID", e))?;
    crate::project::read_guard(path).map_err(|e| io_failure("SIDECAR_INPUT_INVALID", e))
}

fn bounded_read(
    file: &File,
    limit: u64,
    code: &'static str,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, SidecarError> {
    check_cancel(cancel)?;
    let length = file.metadata().map_err(|e| io_failure(code, e))?.len();
    if length > limit {
        return Err(SidecarError::new(
            code,
            "File exceeds the configured byte limit; no complete fingerprint was invented.",
        ));
    }
    let mut reader = file.try_clone().map_err(|e| io_failure(code, e))?;
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|e| io_failure(code, e))?;
    let mut bytes = Vec::new();
    let mut reader = reader.take(limit.saturating_add(1));
    let mut chunk = [0u8; 64 * 1024];
    loop {
        check_cancel(cancel)?;
        let count = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_failure(code, error)),
        };
        if bytes.len() as u64 + count as u64 > limit {
            return Err(SidecarError::new(
                code,
                "File exceeded the configured byte limit while reading.",
            ));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    check_cancel(cancel)?;
    if bytes.len() as u64 > limit || bytes.len() as u64 != length {
        return Err(SidecarError::new(
            code,
            "File changed or exceeded its limit during the bounded read.",
        ));
    }
    Ok(bytes)
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

struct OwnedFile {
    path: PathBuf,
    file: File,
    identity: Handle,
    length: u64,
    sha: String,
}
impl OwnedFile {
    fn create(path: PathBuf, bytes: &[u8], cancel: &AtomicBool) -> Result<Self, SidecarError> {
        check_cancel(cancel)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // The retained Host handle permits child reads but excludes child
            // writes and deletion. It is never given to the child HANDLE_LIST.
            options.share_mode(1).custom_flags(0x00200000);
        }
        let mut file = options
            .open(&path)
            .map_err(|e| io_failure("SIDECAR_WORKSPACE_CONFLICT", e))?;
        let expected = identity(&file)?;
        for chunk in bytes.chunks(64 * 1024) {
            check_cancel(cancel)?;
            file.write_all(chunk)
                .map_err(|e| io_failure("SIDECAR_WORKSPACE_CONFLICT", e))?;
        }
        file.sync_all()
            .map_err(|e| io_failure("SIDECAR_WORKSPACE_CONFLICT", e))?;
        check_cancel(cancel)?;
        let result = Self {
            path,
            file,
            identity: expected,
            length: bytes.len() as u64,
            sha: digest(bytes),
        };
        result.verify(cancel)?;
        Ok(result)
    }
    fn verify(&self, cancel: &AtomicBool) -> Result<(), SidecarError> {
        check_cancel(cancel)?;
        ensure_identity(&self.path, &self.identity, "SIDECAR_WORKSPACE_CONFLICT")?;
        if noetome_sidecar_platform::file_link_count(&self.file)
            .map_err(|e| io_failure("SIDECAR_RESOURCE_INVALID", e))?
            != 1
        {
            return Err(SidecarError::new(
                "SIDECAR_RESOURCE_INVALID",
                "Owned protocol files must have exactly one link.",
            ));
        }
        let actual = bounded_read(
            &self.file,
            self.length,
            "SIDECAR_WORKSPACE_CONFLICT",
            cancel,
        )?;
        if digest(&actual) != self.sha {
            return Err(SidecarError::new(
                "SIDECAR_WORKSPACE_CONFLICT",
                "Owned protocol file contents changed.",
            ));
        }
        check_cancel(cancel)?;
        Ok(())
    }
}

struct Workspace {
    root: PathBuf,
    dirs: Vec<(PathBuf, Handle)>,
    files: Vec<OwnedFile>,
}
impl Workspace {
    fn create(
        root: PathBuf,
        request: &SidecarRequest,
        request_bytes: &[u8],
        raw: &[u8],
        cancel: &AtomicBool,
    ) -> Result<Self, SidecarError> {
        check_cancel(cancel)?;
        let parent = root.parent().ok_or_else(|| {
            SidecarError::new("SIDECAR_WORKSPACE_CONFLICT", "Workspace parent is missing.")
        })?;
        let mut dirs = guard_chain(parent)?;
        fs::create_dir(&root).map_err(|e| io_failure("SIDECAR_WORKSPACE_CONFLICT", e))?;
        dirs.push((root.clone(), guard_directory(&root)?));
        for directory in [root.join("input"), root.join("artifacts")] {
            fs::create_dir(&directory).map_err(|e| io_failure("SIDECAR_WORKSPACE_CONFLICT", e))?;
            dirs.push((directory.clone(), guard_directory(&directory)?));
        }
        let owner = serde_json::to_vec(
            &serde_json::json!({ "schema_version":1, "request_id":request.request_id }),
        )
        .map_err(|e| io_failure("SIDECAR_PROTOCOL_INVALID", e))?;
        let files = vec![
            OwnedFile::create(root.join(".owner.json"), &owner, cancel)?,
            OwnedFile::create(root.join("request.json"), request_bytes, cancel)?,
            OwnedFile::create(root.join("input/raw.bin"), raw, cancel)?,
        ];
        Ok(Self { root, dirs, files })
    }
    fn verify(&self, cancel: &AtomicBool) -> Result<(), SidecarError> {
        check_cancel(cancel)?;
        for (path, handle) in &self.dirs {
            ensure_identity(path, handle, "SIDECAR_WORKSPACE_CONFLICT")?;
        }
        exact_names(
            &self.root,
            &[".owner.json", "request.json", "input", "artifacts"],
            "SIDECAR_WORKSPACE_CONFLICT",
        )?;
        exact_names(
            &self.root.join("input"),
            &["raw.bin"],
            "SIDECAR_WORKSPACE_CONFLICT",
        )?;
        for file in &self.files {
            file.verify(cancel)?;
        }
        Ok(())
    }
    fn read_artifacts(
        &self,
        response: &SidecarResponse,
        limits: &SidecarLimits,
        cancel: &AtomicBool,
    ) -> Result<(BTreeMap<String, Vec<u8>>, Vec<OwnedFile>), SidecarError> {
        check_cancel(cancel)?;
        if response.artifacts.len() > limits.max_artifacts as usize {
            return Err(SidecarError::new(
                "SIDECAR_RESOURCE_INVALID",
                "Resource inventory exceeds the configured file count.",
            ));
        }
        let mut folds = BTreeSet::new();
        for artifact in &response.artifacts {
            check_cancel(cancel)?;
            let id = &artifact.artifact_id;
            let extension = match artifact.kind {
                ArtifactKind::Binary => "bin",
                ArtifactKind::Utf8Text => "txt",
            };
            if id.is_empty()
                || id.len() > 128
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                || !id.as_bytes()[0].is_ascii_alphanumeric()
                || artifact.relative_path != format!("artifacts/{id}.{extension}")
                || !folds.insert(artifact.relative_path.to_ascii_lowercase())
            {
                return Err(SidecarError::new(
                    "SIDECAR_RESOURCE_INVALID",
                    "Resource path is not a unique fixed relative artifact path.",
                ));
            }
            checked_path(&self.root.join(&artifact.relative_path))?;
        }
        self.verify_artifact_inventory(response)?;
        let mut bytes = BTreeMap::new();
        let mut guards = Vec::new();
        let mut total = 0u64;
        for artifact in &response.artifacts {
            check_cancel(cancel)?;
            let path = self.root.join(&artifact.relative_path);
            let file = open_input(&path).map_err(|e| io_failure("SIDECAR_RESOURCE_INVALID", e))?;
            if noetome_sidecar_platform::file_link_count(&file)
                .map_err(|e| io_failure("SIDECAR_RESOURCE_INVALID", e))?
                != 1
            {
                return Err(SidecarError::new(
                    "SIDECAR_RESOURCE_INVALID",
                    "Resources may not be hard links.",
                ));
            }
            let handle = identity(&file)?;
            let length = file
                .metadata()
                .map_err(|e| io_failure("SIDECAR_RESOURCE_INVALID", e))?
                .len();
            let next_total = total.checked_add(length).ok_or_else(|| {
                SidecarError::new(
                    "SIDECAR_RESOURCE_INVALID",
                    "Resource length sum overflowed.",
                )
            })?;
            if length != artifact.byte_length
                || length > limits.max_artifact_bytes
                || next_total > limits.max_total_artifact_bytes
            {
                return Err(SidecarError::new(
                    "SIDECAR_RESOURCE_INVALID",
                    "Actual resource inventory exceeds its byte policy or declared length; oversized files were not read.",
                ));
            }
            let remaining = limits.max_total_artifact_bytes - total;
            let data = bounded_read(
                &file,
                limits.max_artifact_bytes.min(remaining),
                "SIDECAR_RESOURCE_INVALID",
                cancel,
            )?;
            total = next_total;
            if data.len() as u64 != artifact.byte_length || digest(&data) != artifact.sha256 {
                return Err(SidecarError::new(
                    "SIDECAR_RESOURCE_INVALID",
                    "Resource length, SHA or aggregate limit disagrees with actual files.",
                ));
            }
            if guards
                .iter()
                .any(|old: &OwnedFile| &old.identity == &handle)
            {
                return Err(SidecarError::new(
                    "SIDECAR_RESOURCE_INVALID",
                    "Two resource entries refer to one file identity.",
                ));
            }
            bytes.insert(artifact.relative_path.clone(), data);
            guards.push(OwnedFile {
                path,
                file,
                identity: handle,
                length,
                sha: artifact.sha256.clone(),
            });
        }
        Ok((bytes, guards))
    }

    fn verify_artifact_inventory(&self, response: &SidecarResponse) -> Result<(), SidecarError> {
        let names: Vec<&str> = response
            .artifacts
            .iter()
            .map(|a| a.relative_path.strip_prefix("artifacts/").unwrap_or(""))
            .collect();
        exact_names(
            &self.root.join("artifacts"),
            &names,
            "SIDECAR_RESOURCE_INVALID",
        )
    }
}

fn exact_names(
    directory: &Path,
    expected: &[&str],
    code: &'static str,
) -> Result<(), SidecarError> {
    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(directory).map_err(|e| io_failure(code, e))? {
        let entry = entry.map_err(|e| io_failure(code, e))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| SidecarError::new(code, "Non-Unicode entry in the owned workspace."))?;
        if !actual.insert(name) || actual.len() > expected.len() {
            return Err(SidecarError::new(
                code,
                "Unknown or duplicate entry in the owned workspace; preserved.",
            ));
        }
    }
    let expected: BTreeSet<String> = expected.iter().map(|s| (*s).into()).collect();
    if actual != expected {
        return Err(SidecarError::new(
            code,
            "Workspace inventory has missing or unknown entries; preserved.",
        ));
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    struct Sandbox(PathBuf);
    impl Sandbox {
        fn new() -> Self {
            let parent =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/sidecar-core-tests");
            fs::create_dir_all(&parent).unwrap();
            let root = parent
                .canonicalize()
                .unwrap()
                .join(Uuid::new_v4().to_string());
            fs::create_dir(&root).unwrap();
            fs::write(root.join("input.bin"), b"abc").unwrap();
            Self(root)
        }
        fn spec(&self) -> AdapterSpec {
            AdapterSpec {
                program: self.0.join("unreachable-program.exe"),
                args: Vec::new(),
                expected: AdapterIdentity {
                    producer: Producer::Fake,
                    adapter_id: "identity-fake".into(),
                    adapter_version: "1".into(),
                    capabilities: Capabilities {
                        locator_kinds: vec![LocatorKind::InputBytes],
                        artifact_kinds: vec![ArtifactKind::Binary, ArtifactKind::Utf8Text],
                    },
                },
            }
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn existing_workspace_is_rejected_and_preserves_every_byte() {
        let sandbox = Sandbox::new();
        let output = sandbox.0.join("existing");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("sentinel"), b"manual content").unwrap();
        let error = run_sidecar(
            &sandbox.spec(),
            &sandbox.0.join("input.bin"),
            &output,
            SidecarLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(error.code, "SIDECAR_WORKSPACE_CONFLICT");
        assert_eq!(
            fs::read(output.join("sentinel")).unwrap(),
            b"manual content"
        );
        assert_eq!(fs::read(sandbox.0.join("input.bin")).unwrap(), b"abc");
        assert_eq!(fs::read_dir(output).unwrap().count(), 1);
    }

    #[test]
    fn input_limit_and_request_limit_fail_before_workspace_creation() {
        let sandbox = Sandbox::new();
        for (limit, code) in [
            (
                SidecarLimits {
                    max_input_bytes: 2,
                    ..SidecarLimits::default()
                },
                "SIDECAR_INPUT_LIMIT",
            ),
            (
                SidecarLimits {
                    max_request_bytes: 1,
                    ..SidecarLimits::default()
                },
                "SIDECAR_REQUEST_LIMIT",
            ),
        ] {
            let output = sandbox.0.join(Uuid::new_v4().to_string());
            let error = run_sidecar(
                &sandbox.spec(),
                &sandbox.0.join("input.bin"),
                &output,
                limit,
                &AtomicBool::new(false),
            )
            .unwrap_err();
            assert_eq!(error.code, code);
            assert!(!output.exists());
            assert_eq!(fs::read(sandbox.0.join("input.bin")).unwrap(), b"abc");
        }
    }

    #[test]
    fn raw_parent_traversal_never_creates_the_normalized_target() {
        let sandbox = Sandbox::new();
        // PathBuf::push normalizes parent components for verbatim Windows
        // prefixes; build the supplied lexical argument without that mutation.
        let candidate = PathBuf::from(format!("{}\\absent\\..\\escaped", sandbox.0.display()));
        let error = run_sidecar(
            &sandbox.spec(),
            &sandbox.0.join("input.bin"),
            &candidate,
            SidecarLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(error.code, "SIDECAR_UNSAFE_PATH");
        assert!(!sandbox.0.join("absent").exists());
        assert!(!sandbox.0.join("escaped").exists());
        assert_eq!(fs::read(sandbox.0.join("input.bin")).unwrap(), b"abc");
    }

    #[test]
    fn retained_snapshot_and_directory_handles_allow_reads_but_reject_mutation() {
        let sandbox = Sandbox::new();
        let request = SidecarRequest {
            schema_version: 1,
            protocol: "noetome-parser-single-v1".into(),
            request_id: Uuid::new_v4().to_string(),
            operation: Operation::InspectBytes,
            input: InputFingerprint {
                relative_path: "input/raw.bin".into(),
                byte_length: 3,
                sha256: digest(b"abc"),
            },
            workspace: WorkspacePolicy {
                artifact_directory: "artifacts".into(),
            },
            artifact_policy: ArtifactPolicy {
                max_file_bytes: 16 * 1024 * 1024,
                max_total_bytes: 64 * 1024 * 1024,
                max_files: 64,
            },
        };
        let root = sandbox.0.join("snapshot-workspace");
        let bytes = serde_json::to_vec(&request).unwrap();
        let cancel = AtomicBool::new(false);
        let workspace = Workspace::create(root.clone(), &request, &bytes, b"abc", &cancel).unwrap();
        assert_eq!(fs::read(root.join("input/raw.bin")).unwrap(), b"abc");
        assert!(fs::write(root.join("input/raw.bin"), b"changed").is_err());
        assert!(fs::rename(root.join("input"), root.join("moved-input")).is_err());
        assert!(fs::rename(&root, sandbox.0.join("replacement-root")).is_err());
        workspace.verify(&cancel).unwrap();
        assert_eq!(fs::read(sandbox.0.join("input.bin")).unwrap(), b"abc");
        drop(workspace);
        fs::write(root.join("input/raw.bin"), b"released").unwrap();
    }
}
