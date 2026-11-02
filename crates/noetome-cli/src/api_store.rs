//! Byte ownership for the API single-request protocol. API semantics live above this layer.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use same_file::Handle;
use serde::{Deserialize, Serialize};

use crate::knowledge_extract::ModelError;
use crate::{project, sidecar, source_model};

const PROTOCOL: &str = "noetome-api-single-v1";
const PENDING_REPORT: &str = ".report.pending.json";
const SMALL_CAP: u64 = 64 * 1024;
const LARGE_CAP: u64 = 512 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    schema_version: u32,
    protocol: String,
    request_id: String,
    request_sha256: String,
}

fn require(condition: bool, code: &'static str) -> Result<(), ModelError> {
    if condition { Ok(()) } else { Err(code.into()) }
}
fn cancelled(cancel: &AtomicBool) -> Result<(), ModelError> {
    require(!cancel.load(Ordering::Acquire), "MODEL_API_CANCELLED")
}
fn owned_error(error: sidecar::SidecarError) -> ModelError {
    match error.code {
        "SIDECAR_CANCELLED" => "MODEL_API_CANCELLED".into(),
        _ => "MODEL_API_OUTPUT_CHANGED".into(),
    }
}
fn cap(name: &str) -> Result<u64, ModelError> {
    match name {
        "owner.json"
        | "config.json"
        | "authorization.json"
        | "request.json"
        | "reservation.json"
        | "attempt.json"
        | "http-request.json"
        | "http-observation.json"
        | "report.json" => Ok(SMALL_CAP),
        "http-response.bin" | "response.json" | "candidate.json" => Ok(LARGE_CAP),
        _ => Err("MODEL_API_FILE_NAME_INVALID".into()),
    }
}
fn owner_valid(owner: &Owner) -> Result<(), ModelError> {
    require(
        owner.schema_version == 2
            && owner.protocol == PROTOCOL
            && source_model::valid_uuid(&owner.request_id)
            && source_model::valid_hash(&owner.request_sha256),
        "MODEL_API_OWNER_INVALID",
    )
}
fn parse_owner(bytes: &[u8]) -> Result<Owner, ModelError> {
    // Derived serde structs can deserialize a positional array. Require the
    // named object before strict deserialization, which also rejects duplicates.
    require(
        bytes.iter().find(|byte| !byte.is_ascii_whitespace()) == Some(&b'{'),
        "MODEL_API_OWNER_INVALID",
    )?;
    let owner: Owner =
        serde_json::from_slice(bytes).map_err(|_| ModelError::from("MODEL_API_OWNER_INVALID"))?;
    owner_valid(&owner)?;
    Ok(owner)
}
fn checked_path(path: &Path) -> Result<PathBuf, ModelError> {
    #[cfg(windows)]
    require(
        path.components().all(|component| match component {
            std::path::Component::Normal(name) => name
                .to_str()
                .is_some_and(|name| !name.contains(['/', '\\'])),
            _ => true,
        }),
        "MODEL_API_OUTPUT_INVALID",
    )?;
    sidecar::checked_path(path).map_err(|_| "MODEL_API_OUTPUT_INVALID".into())
}
struct DirectoryGuard {
    path: PathBuf,
    file: File,
    identity: Handle,
}
fn guard_directory(path: &Path) -> Result<DirectoryGuard, ModelError> {
    checked_path(path)?;
    let metadata =
        fs::symlink_metadata(path).map_err(|_| ModelError::from("MODEL_API_OUTPUT_INVALID"))?;
    require(
        metadata.is_dir() && !project::is_link(&metadata),
        "MODEL_API_OUTPUT_INVALID",
    )?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // A metadata-only access=0 handle permits directory rename on Windows.
        // Retain actual READ access and omit FILE_SHARE_DELETE.
        options.share_mode(3).custom_flags(0x02200000);
    }
    let file = options
        .open(path)
        .map_err(|_| ModelError::from("MODEL_API_OUTPUT_INVALID"))?;
    let identity = Handle::from_file(
        file.try_clone()
            .map_err(|_| ModelError::from("MODEL_API_IO_FAILED"))?,
    )
    .map_err(|_| ModelError::from("MODEL_API_IO_FAILED"))?;
    let guard = DirectoryGuard {
        path: path.into(),
        file,
        identity,
    };
    verify_dirs(std::slice::from_ref(&guard))?;
    Ok(guard)
}
fn guard_chain(path: &Path) -> Result<Vec<DirectoryGuard>, ModelError> {
    let mut paths: Vec<_> = path.ancestors().collect();
    paths.reverse();
    paths.into_iter().map(guard_directory).collect()
}
fn verify_dirs(dirs: &[DirectoryGuard]) -> Result<(), ModelError> {
    for guard in dirs {
        let metadata = guard
            .file
            .metadata()
            .map_err(|_| ModelError::from("MODEL_API_OUTPUT_CHANGED"))?;
        require(
            metadata.is_dir() && !project::is_link(&metadata),
            "MODEL_API_OUTPUT_CHANGED",
        )?;
        sidecar::ensure_identity(&guard.path, &guard.identity, "MODEL_API_OUTPUT_CHANGED")
            .map_err(|_| ModelError::from("MODEL_API_OUTPUT_CHANGED"))?;
    }
    Ok(())
}
fn exact_inventory(root: &Path, names: &BTreeSet<String>) -> Result<(), ModelError> {
    source_model::exact_names(root, &names.iter().map(std::ffi::OsString::from).collect())
        .map_err(|_| "MODEL_API_INVENTORY_INVALID".into())
}

struct OutputLocation {
    root: PathBuf,
    dirs: Vec<DirectoryGuard>,
    project_dirs: Vec<DirectoryGuard>,
}
fn prepare_location(project_root: &Path, output: &Path) -> Result<OutputLocation, ModelError> {
    let project_root = checked_path(project_root)?;
    let root = checked_path(output)?;
    require(
        !project::exists(&root).map_err(|_| ModelError::from("MODEL_API_OUTPUT_INVALID"))?,
        "MODEL_API_OUTPUT_EXISTS",
    )?;
    let parent = root
        .parent()
        .ok_or(ModelError::from("MODEL_API_OUTPUT_INVALID"))?;
    let project_dirs =
        guard_chain(&project_root).map_err(|_| ModelError::from("MODEL_API_OUTPUT_INVALID"))?;
    let project_identity = &project_dirs
        .last()
        .ok_or(ModelError::from("MODEL_API_OUTPUT_INVALID"))?
        .identity;
    let dirs = guard_chain(parent).map_err(|_| ModelError::from("MODEL_API_OUTPUT_INVALID"))?;
    // Actual directory identities include Windows case and spelling aliases.
    // A new root cannot contain an existing project, and its parent chain may
    // not contain the project identity. Keep both complete chains until drop.
    require(
        dirs.iter().all(|guard| &guard.identity != project_identity),
        "MODEL_API_OUTPUT_OVERLAP",
    )?;
    verify_dirs(&dirs)?;
    verify_dirs(&project_dirs)?;
    Ok(OutputLocation {
        root,
        dirs,
        project_dirs,
    })
}

pub(crate) struct ApiStore {
    root: PathBuf,
    dirs: Vec<DirectoryGuard>,
    project_dirs: Vec<DirectoryGuard>,
    files: BTreeMap<String, sidecar::OwnedFile>,
    sealed: bool,
}

pub(crate) fn preflight(project_root: &Path, output: &Path) -> Result<PathBuf, ModelError> {
    Ok(prepare_location(project_root, output)?.root)
}
impl ApiStore {
    pub(crate) fn create(
        project_root: &Path,
        output: &Path,
        request_id: &str,
        request_sha256: &str,
        cancel: &AtomicBool,
    ) -> Result<Self, ModelError> {
        cancelled(cancel)?;
        let owner = Owner {
            schema_version: 2,
            protocol: PROTOCOL.into(),
            request_id: request_id.into(),
            request_sha256: request_sha256.into(),
        };
        owner_valid(&owner)?;
        let owner_bytes = serde_json::to_vec_pretty(&owner)
            .map_err(|_| ModelError::from("MODEL_API_OWNER_INVALID"))?;
        let OutputLocation {
            root,
            mut dirs,
            project_dirs,
        } = prepare_location(project_root, output)?;
        cancelled(cancel)?;
        verify_dirs(&dirs)?;
        verify_dirs(&project_dirs)?;
        fs::create_dir(&root).map_err(|_| ModelError::from("MODEL_API_OUTPUT_EXISTS"))?;
        dirs.push(
            guard_directory(&root).map_err(|_| ModelError::from("MODEL_API_OUTPUT_CHANGED"))?,
        );
        let mut store = Self {
            root,
            dirs,
            project_dirs,
            files: BTreeMap::new(),
            sealed: false,
        };
        store.verify(cancel)?;
        let file = sidecar::OwnedFile::create(store.root.join("owner.json"), &owner_bytes, cancel)
            .map_err(owned_error)?;
        store.files.insert("owner.json".into(), file);
        store.verify(cancel)?;
        Ok(store)
    }
    pub(crate) fn put(
        &mut self,
        name: &str,
        bytes: &[u8],
        cancel: &AtomicBool,
    ) -> Result<(), ModelError> {
        cancelled(cancel)?;
        let limit = cap(name)?;
        require(
            !matches!(name, "owner.json" | "report.json"),
            "MODEL_API_FILE_NAME_INVALID",
        )?;
        require(bytes.len() as u64 <= limit, "MODEL_API_FILE_TOO_LARGE")?;
        require(
            !self.sealed && !self.files.contains_key(name),
            "MODEL_API_OUTPUT_CHANGED",
        )?;
        self.verify(cancel)?;
        let file =
            sidecar::OwnedFile::create(self.root.join(name), bytes, cancel).map_err(owned_error)?;
        self.files.insert(name.into(), file);
        self.verify(cancel)
    }
    pub(crate) fn commit_report(
        &mut self,
        bytes: &[u8],
        cancel: &AtomicBool,
    ) -> Result<(), ModelError> {
        cancelled(cancel)?;
        require(bytes.len() as u64 <= SMALL_CAP, "MODEL_API_FILE_TOO_LARGE")?;
        require(
            !project::exists(&self.root.join("report.json"))
                .map_err(|_| ModelError::from("MODEL_API_OUTPUT_CHANGED"))?,
            "MODEL_API_REPORT_EXISTS",
        )?;
        require(!self.sealed, "MODEL_API_OUTPUT_CHANGED")?;
        self.verify(cancel)?;
        let pending = sidecar::OwnedFile::pending(self.root.join(PENDING_REPORT), bytes, cancel)
            .map_err(owned_error)?;
        self.sealed = true;
        self.files.insert(PENDING_REPORT.into(), pending);
        self.verify(cancel)?;
        cancelled(cancel)?;
        let pending = self
            .files
            .remove(PENDING_REPORT)
            .ok_or(ModelError::from("MODEL_API_OUTPUT_CHANGED"))?;
        let report = pending
            .publish(self.root.join("report.json"))
            .map_err(owned_error)?;
        self.files.insert("report.json".into(), report);
        self.verify(cancel)
    }
    pub(crate) fn verify(&self, cancel: &AtomicBool) -> Result<(), ModelError> {
        cancelled(cancel)?;
        verify_dirs(&self.project_dirs)?;
        verify_dirs(&self.dirs)?;
        let names = self.files.keys().cloned().collect();
        exact_inventory(&self.root, &names)?;
        for file in self.files.values() {
            file.verify(cancel).map_err(owned_error)?;
        }
        exact_inventory(&self.root, &names)?;
        verify_dirs(&self.dirs)?;
        verify_dirs(&self.project_dirs)?;
        cancelled(cancel)
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn read_owned(&self, name: &str) -> Result<Vec<u8>, ModelError> {
        let limit = cap(name)?;
        let cancel = AtomicBool::new(false);
        self.verify(&cancel)?;
        let file = self
            .files
            .get(name)
            .ok_or(ModelError::from("MODEL_API_INVENTORY_INVALID"))?;
        // Reuse the original WRITE handle. A second Windows read guard denies
        // WRITE sharing and would conflict with this store's own retained guard.
        let bytes = file.read_owned(limit).map_err(owned_error)?;
        self.verify(&cancel)?;
        Ok(bytes)
    }
}

struct SnapshotFile {
    path: PathBuf,
    file: File,
    identity: Handle,
    length: u64,
    sha: String,
    bytes: Vec<u8>,
}
fn bounded_read(file: &File, limit: u64, cancel: &AtomicBool) -> Result<Vec<u8>, ModelError> {
    cancelled(cancel)?;
    let length = file
        .metadata()
        .map_err(|_| ModelError::from("MODEL_API_OUTPUT_CHANGED"))?
        .len();
    require(length <= limit, "MODEL_API_FILE_TOO_LARGE")?;
    let mut reader = file
        .try_clone()
        .map_err(|_| ModelError::from("MODEL_API_IO_FAILED"))?;
    reader
        .seek(SeekFrom::Start(0))
        .map_err(|_| ModelError::from("MODEL_API_IO_FAILED"))?;
    let mut reader = reader.take(limit.saturating_add(1));
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        cancelled(cancel)?;
        let count = match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err("MODEL_API_IO_FAILED".into()),
        };
        require(
            bytes.len() as u64 + count as u64 <= limit,
            "MODEL_API_FILE_TOO_LARGE",
        )?;
        bytes.extend_from_slice(&chunk[..count]);
    }
    require(bytes.len() as u64 == length, "MODEL_API_OUTPUT_CHANGED")?;
    cancelled(cancel)?;
    Ok(bytes)
}
fn single_link(file: &File) -> Result<(), ModelError> {
    require(
        noetome_sidecar_platform::file_link_count(file)
            .map_err(|_| ModelError::from("MODEL_API_IO_FAILED"))?
            == 1,
        "MODEL_API_OUTPUT_CHANGED",
    )
}
impl SnapshotFile {
    fn open(path: PathBuf, limit: u64, cancel: &AtomicBool) -> Result<Self, ModelError> {
        cancelled(cancel)?;
        checked_path(&path)?;
        let before = project::checked_file(&path)
            .map_err(|_| ModelError::from("MODEL_API_OUTPUT_CHANGED"))?;
        require(before.len() <= limit, "MODEL_API_FILE_TOO_LARGE")?;
        let file =
            project::read_guard(&path).map_err(|_| ModelError::from("MODEL_API_OUTPUT_CHANGED"))?;
        single_link(&file)?;
        let identity = Handle::from_file(
            file.try_clone()
                .map_err(|_| ModelError::from("MODEL_API_IO_FAILED"))?,
        )
        .map_err(|_| ModelError::from("MODEL_API_IO_FAILED"))?;
        sidecar::ensure_identity(&path, &identity, "MODEL_API_OUTPUT_CHANGED")
            .map_err(|_| ModelError::from("MODEL_API_OUTPUT_CHANGED"))?;
        let bytes = bounded_read(&file, limit, cancel)?;
        require(
            before.len() == bytes.len() as u64,
            "MODEL_API_OUTPUT_CHANGED",
        )?;
        let result = Self {
            path,
            file,
            identity,
            length: bytes.len() as u64,
            sha: sidecar::digest(&bytes),
            bytes,
        };
        result.verify(cancel)?;
        Ok(result)
    }
    fn verify(&self, cancel: &AtomicBool) -> Result<(), ModelError> {
        cancelled(cancel)?;
        sidecar::ensure_identity(&self.path, &self.identity, "MODEL_API_OUTPUT_CHANGED")
            .map_err(|_| ModelError::from("MODEL_API_OUTPUT_CHANGED"))?;
        single_link(&self.file)?;
        let bytes = bounded_read(&self.file, self.length, cancel)?;
        require(
            bytes.len() as u64 == self.length && sidecar::digest(&bytes) == self.sha,
            "MODEL_API_OUTPUT_CHANGED",
        )?;
        single_link(&self.file)?;
        sidecar::ensure_identity(&self.path, &self.identity, "MODEL_API_OUTPUT_CHANGED")
            .map_err(|_| ModelError::from("MODEL_API_OUTPUT_CHANGED"))?;
        cancelled(cancel)
    }
}

/// Retains every original read handle and every ancestor identity through the
/// caller's complete semantic verification. Deliberately neither Clone nor Serialize.
pub(crate) struct ApiSnapshot {
    root: PathBuf,
    dirs: Vec<DirectoryGuard>,
    files: BTreeMap<String, SnapshotFile>,
    names: BTreeSet<String>,
}
impl ApiSnapshot {
    pub(crate) fn open(
        root: &Path,
        expected_names: &BTreeSet<String>,
        cancel: &AtomicBool,
    ) -> Result<Self, ModelError> {
        cancelled(cancel)?;
        let root = checked_path(root)?;
        let dirs = guard_chain(&root).map_err(|_| ModelError::from("MODEL_API_OUTPUT_INVALID"))?;
        let owner = SnapshotFile::open(root.join("owner.json"), SMALL_CAP, cancel)?;
        parse_owner(&owner.bytes)?;
        let report = SnapshotFile::open(root.join("report.json"), SMALL_CAP, cancel)?;
        require(
            expected_names.contains("owner.json")
                && expected_names.contains("report.json")
                && expected_names.iter().all(|name| cap(name).is_ok()),
            "MODEL_API_INVENTORY_INVALID",
        )?;
        exact_inventory(&root, expected_names)?;
        let mut files =
            BTreeMap::from([("owner.json".into(), owner), ("report.json".into(), report)]);
        for name in expected_names {
            if files.contains_key(name) {
                continue;
            }
            let file = SnapshotFile::open(root.join(name), cap(name)?, cancel)?;
            require(
                files.values().all(|old| old.identity != file.identity),
                "MODEL_API_OUTPUT_CHANGED",
            )?;
            files.insert(name.clone(), file);
        }
        let snapshot = Self {
            root,
            dirs,
            files,
            names: expected_names.clone(),
        };
        snapshot.verify(cancel)?;
        Ok(snapshot)
    }
    pub(crate) fn read(&self, name: &str) -> Result<&[u8], ModelError> {
        cap(name)?;
        let cancel = AtomicBool::new(false);
        self.verify(&cancel)?;
        let bytes = &self
            .files
            .get(name)
            .ok_or(ModelError::from("MODEL_API_INVENTORY_INVALID"))?
            .bytes;
        self.verify(&cancel)?;
        Ok(bytes)
    }
    pub(crate) fn verify(&self, cancel: &AtomicBool) -> Result<(), ModelError> {
        cancelled(cancel)?;
        verify_dirs(&self.dirs)?;
        exact_inventory(&self.root, &self.names)?;
        for file in self.files.values() {
            file.verify(cancel)?;
        }
        verify_dirs(&self.dirs)?;
        exact_inventory(&self.root, &self.names)?;
        cancelled(cancel)
    }
}

/// A bounded pre-reader peek for choosing the expected inventory. This does not
/// authorize a report: the caller must reacquire ApiSnapshot and compare the
/// report bytes before interpreting the fixed snapshot.
pub(crate) fn read_report(root: &Path) -> Result<Vec<u8>, ModelError> {
    let cancel = AtomicBool::new(false);
    let root = checked_path(root)?;
    let dirs = guard_chain(&root).map_err(|_| ModelError::from("MODEL_API_OUTPUT_INVALID"))?;
    let owner = SnapshotFile::open(root.join("owner.json"), SMALL_CAP, &cancel)?;
    parse_owner(&owner.bytes)?;
    let report = SnapshotFile::open(root.join("report.json"), SMALL_CAP, &cancel)?;
    owner.verify(&cancel)?;
    report.verify(&cancel)?;
    verify_dirs(&dirs)?;
    Ok(report.bytes)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs::{self, File, OpenOptions};
    use std::sync::atomic::Ordering;
    use uuid::Uuid;

    const ID: &str = "6bc3714a-18f0-4808-9d80-a26e2cdb9291";
    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    struct Sandbox {
        root: PathBuf,
        project: PathBuf,
        output: PathBuf,
    }
    impl Sandbox {
        fn new() -> Self {
            let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/api-store-tests");
            fs::create_dir_all(&parent).unwrap();
            let root = parent
                .canonicalize()
                .unwrap()
                .join(Uuid::new_v4().to_string());
            fs::create_dir(&root).unwrap();
            let project = root.join("project");
            fs::create_dir(&project).unwrap();
            fs::write(
                project.join("sentinel.txt"),
                b"selected source remains unchanged",
            )
            .unwrap();
            fs::write(root.join("sentinel.txt"), b"outside remains unchanged").unwrap();
            let output = root.join("api");
            Self {
                root,
                project,
                output,
            }
        }
        fn create(&self) -> ApiStore {
            ApiStore::create(
                &self.project,
                &self.output,
                ID,
                SHA,
                &AtomicBool::new(false),
            )
            .unwrap()
        }
        fn completed(&self) {
            let mut store = self.create();
            store
                .put("request.json", b"{}", &AtomicBool::new(false))
                .unwrap();
            store
                .commit_report(b"{\"result\":\"fixture\"}", &AtomicBool::new(false))
                .unwrap();
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            assert_eq!(
                fs::read(self.project.join("sentinel.txt")).unwrap(),
                b"selected source remains unchanged"
            );
            assert_eq!(
                fs::read(self.root.join("sentinel.txt")).unwrap(),
                b"outside remains unchanged"
            );
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
    fn names(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|name| (*name).into()).collect()
    }
    fn inventory(root: &Path) -> BTreeMap<String, (Vec<u8>, std::time::SystemTime)> {
        fs::read_dir(root)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                let name = entry.file_name().to_str().unwrap().to_owned();
                (
                    name,
                    (
                        fs::read(entry.path()).unwrap(),
                        entry.metadata().unwrap().modified().unwrap(),
                    ),
                )
            })
            .collect()
    }
    fn err<T>(value: Result<T, ModelError>) -> ModelError {
        match value {
            Err(error) => error,
            Ok(_) => panic!("expected protected rejection"),
        }
    }

    #[test]
    fn owner_only_creation_then_original_handle_reads_and_final_report_reload() {
        let sandbox = Sandbox::new();
        let cancel = AtomicBool::new(false);
        let mut store = sandbox.create();
        assert_eq!(store.root(), sandbox.output);
        assert_eq!(
            inventory(store.root())
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            names(&["owner.json"])
        );
        let owner: serde_json::Value =
            serde_json::from_slice(&store.read_owned("owner.json").unwrap()).unwrap();
        assert_eq!(
            owner,
            serde_json::json!({"schema_version":2,"protocol":"noetome-api-single-v1","request_id":ID,"request_sha256":SHA})
        );
        store
            .put("request.json", b"fixture request bytes", &cancel)
            .unwrap();
        for _ in 0..3 {
            assert_eq!(
                store.read_owned("request.json").unwrap(),
                b"fixture request bytes"
            );
        }
        store
            .commit_report(b"fixture final report bytes", &cancel)
            .unwrap();
        assert!(!store.root().join(".report.pending.json").exists());
        assert_eq!(
            store.read_owned("report.json").unwrap(),
            b"fixture final report bytes"
        );
        let before = inventory(store.root());
        drop(store);
        assert_eq!(
            read_report(&sandbox.output).unwrap(),
            b"fixture final report bytes"
        );
        let snapshot = ApiSnapshot::open(
            &sandbox.output,
            &names(&["owner.json", "request.json", "report.json"]),
            &cancel,
        )
        .unwrap();
        assert_eq!(
            snapshot.read("request.json").unwrap(),
            b"fixture request bytes"
        );
        for _ in 0..3 {
            snapshot.verify(&cancel).unwrap();
        }
        assert_eq!(inventory(&sandbox.output), before);
    }

    #[test]
    fn existing_roots_and_unknown_entries_preserve_every_byte() {
        let sandbox = Sandbox::new();
        fs::create_dir(&sandbox.output).unwrap();
        fs::write(sandbox.output.join("human.txt"), b"human content").unwrap();
        let before = inventory(&sandbox.output);
        assert_eq!(
            err(ApiStore::create(
                &sandbox.project,
                &sandbox.output,
                ID,
                SHA,
                &AtomicBool::new(false)
            ))
            .code,
            "MODEL_API_OUTPUT_EXISTS"
        );
        assert_eq!(inventory(&sandbox.output), before);
        let other = sandbox.root.join("unknown");
        let mut store =
            ApiStore::create(&sandbox.project, &other, ID, SHA, &AtomicBool::new(false)).unwrap();
        fs::write(other.join("unknown.txt"), b"protected unknown").unwrap();
        let before = inventory(&other);
        assert_eq!(
            store
                .put("request.json", b"{}", &AtomicBool::new(false))
                .unwrap_err()
                .code,
            "MODEL_API_INVENTORY_INVALID"
        );
        assert_eq!(inventory(&other), before);
    }

    #[test]
    fn project_overlap_and_invalid_paths_are_rejected_before_creation() {
        let sandbox = Sandbox::new();
        for output in [
            sandbox.project.join("api"),
            PathBuf::from(format!("{}\\..\\escape", sandbox.project.display())),
        ] {
            let error = err(ApiStore::create(
                &sandbox.project,
                &output,
                ID,
                SHA,
                &AtomicBool::new(false),
            ));
            assert!(matches!(
                error.code,
                "MODEL_API_OUTPUT_OVERLAP" | "MODEL_API_OUTPUT_INVALID"
            ));
            assert!(!sandbox.project.join("api").exists());
        }
        for name in [
            "CON",
            "trailing.",
            "trailing ",
            "file:stream",
            "missing-parent/api",
        ] {
            assert!(
                preflight(&sandbox.project, &sandbox.root.join(name)).is_err(),
                "accepted invalid path {name}"
            );
            assert_eq!(
                err(preflight(&sandbox.project, &sandbox.root.join(name))).code,
                "MODEL_API_OUTPUT_INVALID"
            );
            assert!(!sandbox.root.join(name).exists());
        }
        let case_alias =
            PathBuf::from(sandbox.project.to_str().unwrap().to_uppercase()).join("aliased");
        assert_eq!(
            err(preflight(&sandbox.project, &case_alias)).code,
            "MODEL_API_OUTPUT_OVERLAP"
        );
    }

    #[test]
    fn invalid_identity_or_cancel_does_not_create_output() {
        let sandbox = Sandbox::new();
        for (id, sha) in [("bad-id", SHA), (ID, "bad-sha")] {
            assert_eq!(
                err(ApiStore::create(
                    &sandbox.project,
                    &sandbox.output,
                    id,
                    sha,
                    &AtomicBool::new(false)
                ))
                .code,
                "MODEL_API_OWNER_INVALID"
            );
            assert!(!sandbox.output.exists());
        }
        assert_eq!(
            err(ApiStore::create(
                &sandbox.project,
                &sandbox.output,
                ID,
                SHA,
                &AtomicBool::new(true)
            ))
            .code,
            "MODEL_API_CANCELLED"
        );
        assert!(!sandbox.output.exists());
    }

    #[test]
    fn all_external_names_use_allowlist_and_owned_names_cannot_be_overwritten() {
        let sandbox = Sandbox::new();
        let cancel = AtomicBool::new(false);
        let mut store = sandbox.create();
        for name in [
            "../escape",
            "request.JSON",
            "unknown.json",
            "request.json:stream",
            "",
            "nested/request.json",
            ".report.pending.json",
            "owner.json",
            "report.json",
        ] {
            let before = inventory(store.root());
            assert_eq!(
                store.put(name, b"{}", &cancel).unwrap_err().code,
                "MODEL_API_FILE_NAME_INVALID"
            );
            assert_eq!(inventory(store.root()), before);
        }
        store.put("config.json", b"original", &cancel).unwrap();
        assert!(store.put("config.json", b"replacement", &cancel).is_err());
        assert_eq!(store.read_owned("config.json").unwrap(), b"original");
        assert_eq!(
            err(store.read_owned("../owner.json")).code,
            "MODEL_API_FILE_NAME_INVALID"
        );
        store.commit_report(b"{}", &cancel).unwrap();
        assert!(
            store
                .put("request.json", b"after final report", &cancel)
                .is_err()
        );
        drop(store);
        let snapshot = ApiSnapshot::open(
            &sandbox.output,
            &names(&["owner.json", "config.json", "report.json"]),
            &cancel,
        )
        .unwrap();
        assert_eq!(
            err(snapshot.read("../report.json")).code,
            "MODEL_API_FILE_NAME_INVALID"
        );
        assert_eq!(
            err(snapshot.read("candidate.json")).code,
            "MODEL_API_INVENTORY_INVALID"
        );
    }

    #[test]
    fn every_writable_file_accepts_its_cap_and_rejects_cap_plus_one() {
        let sandbox = Sandbox::new();
        let cancel = AtomicBool::new(false);
        let mut store = sandbox.create();
        for (name, cap) in [
            ("config.json", 65536),
            ("authorization.json", 65536),
            ("request.json", 65536),
            ("reservation.json", 65536),
            ("attempt.json", 65536),
            ("http-request.json", 65536),
            ("http-observation.json", 65536),
            ("http-response.bin", 524288),
            ("response.json", 524288),
            ("candidate.json", 524288),
        ] {
            assert_eq!(
                store
                    .put(name, &vec![b'x'; cap + 1], &cancel)
                    .unwrap_err()
                    .code,
                "MODEL_API_FILE_TOO_LARGE"
            );
            assert!(!store.root().join(name).exists());
            store.put(name, &vec![b'x'; cap], &cancel).unwrap();
            assert_eq!(store.read_owned(name).unwrap().len(), cap);
        }
        assert_eq!(
            store
                .commit_report(&vec![b'x'; 65537], &cancel)
                .unwrap_err()
                .code,
            "MODEL_API_FILE_TOO_LARGE"
        );
        assert!(!store.root().join("report.json").exists());
        store.commit_report(&vec![b'x'; 65536], &cancel).unwrap();
        let expected = inventory(store.root()).keys().cloned().collect();
        drop(store);
        let snapshot = ApiSnapshot::open(&sandbox.output, &expected, &cancel).unwrap();
        snapshot.verify(&cancel).unwrap();
        assert_eq!(snapshot.read("report.json").unwrap().len(), 65536);
    }

    #[test]
    fn cancelled_write_and_report_preserve_inventory_without_publication() {
        let sandbox = Sandbox::new();
        let cancel = AtomicBool::new(false);
        let mut store = sandbox.create();
        let before = inventory(store.root());
        cancel.store(true, Ordering::Release);
        assert_eq!(
            store.put("request.json", b"{}", &cancel).unwrap_err().code,
            "MODEL_API_CANCELLED"
        );
        assert_eq!(
            store.commit_report(b"{}", &cancel).unwrap_err().code,
            "MODEL_API_CANCELLED"
        );
        assert_eq!(inventory(store.root()), before);
        cancel.store(false, Ordering::Release);
        store.commit_report(b"original", &cancel).unwrap();
        let before = inventory(store.root());
        assert_eq!(
            store
                .commit_report(b"replacement", &cancel)
                .unwrap_err()
                .code,
            "MODEL_API_REPORT_EXISTS"
        );
        assert_eq!(inventory(store.root()), before);
    }

    #[test]
    fn externally_existing_report_is_never_replaced() {
        let sandbox = Sandbox::new();
        let mut store = sandbox.create();
        fs::write(
            store.root().join("report.json"),
            b"protected existing report",
        )
        .unwrap();
        let before = inventory(store.root());
        assert!(
            store
                .commit_report(b"replacement", &AtomicBool::new(false))
                .is_err()
        );
        assert_eq!(inventory(store.root()), before);
        assert!(!store.root().join(".report.pending.json").exists());
    }

    #[test]
    fn snapshot_requires_strict_v2_owner_report_and_exact_allowed_inventory() {
        let cancel = AtomicBool::new(false);
        let expected = names(&["owner.json", "request.json", "report.json"]);
        for owner in [
            format!(
                "{{\"schema_version\":1,\"protocol\":\"noetome-model-single-v1\",\"request_id\":\"{ID}\",\"request_sha256\":\"{SHA}\"}}"
            ),
            format!(
                "{{\"schema_version\":2,\"protocol\":\"noetome-api-single-v1\",\"request_id\":\"{ID}\"}}"
            ),
            format!(
                "{{\"schema_version\":2,\"schema_version\":2,\"protocol\":\"noetome-api-single-v1\",\"request_id\":\"{ID}\",\"request_sha256\":\"{SHA}\"}}"
            ),
            format!(
                "{{\"schema_version\":2,\"protocol\":\"noetome-api-single-v1\",\"request_id\":\"{ID}\",\"request_sha256\":\"{SHA}\",\"extra\":0}}"
            ),
            format!("[2,\"noetome-api-single-v1\",\"{ID}\",\"{SHA}\"]"),
        ] {
            let sandbox = Sandbox::new();
            sandbox.completed();
            fs::write(sandbox.output.join("owner.json"), owner).unwrap();
            assert_eq!(
                err(ApiSnapshot::open(&sandbox.output, &expected, &cancel)).code,
                "MODEL_API_OWNER_INVALID"
            );
        }
        let sandbox = Sandbox::new();
        sandbox.completed();
        let before = inventory(&sandbox.output);
        for wanted in [
            names(&["request.json", "report.json"]),
            names(&["owner.json", "request.json"]),
            names(&["owner.json", "request.json", "report.json", "unknown.json"]),
        ] {
            assert_eq!(
                err(ApiSnapshot::open(&sandbox.output, &wanted, &cancel)).code,
                "MODEL_API_INVENTORY_INVALID"
            );
        }
        assert_eq!(inventory(&sandbox.output), before);
        fs::remove_file(sandbox.output.join("report.json")).unwrap();
        assert!(ApiSnapshot::open(&sandbox.output, &expected, &cancel).is_err());
        assert!(read_report(&sandbox.output).is_err());
    }

    #[test]
    fn snapshot_rejects_unknown_directory_and_hardlinks_before_exposing_bytes() {
        let sandbox = Sandbox::new();
        sandbox.completed();
        let expected = names(&["owner.json", "request.json", "report.json"]);
        fs::create_dir(sandbox.output.join("unexpected")).unwrap();
        assert_eq!(
            err(ApiSnapshot::open(
                &sandbox.output,
                &expected,
                &AtomicBool::new(false)
            ))
            .code,
            "MODEL_API_INVENTORY_INVALID"
        );
        fs::remove_dir(sandbox.output.join("unexpected")).unwrap();
        for name in ["owner.json", "request.json", "report.json"] {
            let link = sandbox.root.join("link");
            fs::hard_link(sandbox.output.join(name), &link).unwrap();
            assert!(
                ApiSnapshot::open(&sandbox.output, &expected, &AtomicBool::new(false)).is_err()
            );
            fs::remove_file(link).unwrap();
        }
        ApiSnapshot::open(&sandbox.output, &expected, &AtomicBool::new(false)).unwrap();
    }

    #[test]
    fn snapshot_and_report_peek_reject_oversize_before_reading() {
        let sandbox = Sandbox::new();
        sandbox.completed();
        let cancel = AtomicBool::new(false);
        let expected = names(&["owner.json", "request.json", "report.json"]);
        fs::write(sandbox.output.join("request.json"), vec![b'x'; 65537]).unwrap();
        assert_eq!(
            err(ApiSnapshot::open(&sandbox.output, &expected, &cancel)).code,
            "MODEL_API_FILE_TOO_LARGE"
        );
        fs::write(sandbox.output.join("request.json"), b"{}").unwrap();
        fs::write(sandbox.output.join("report.json"), vec![b'x'; 65537]).unwrap();
        assert_eq!(
            err(read_report(&sandbox.output)).code,
            "MODEL_API_FILE_TOO_LARGE"
        );
        assert_eq!(
            err(ApiSnapshot::open(&sandbox.output, &expected, &cancel)).code,
            "MODEL_API_FILE_TOO_LARGE"
        );
    }

    #[test]
    fn writer_native_guards_deny_file_write_rename_and_ancestor_replacement_then_release_on_drop() {
        let sandbox = Sandbox::new();
        let cancel = AtomicBool::new(false);
        let mut store = sandbox.create();
        store.put("request.json", b"original", &cancel).unwrap();
        let selected = store.root().join("request.json");
        assert!(OpenOptions::new().write(true).open(&selected).is_err());
        assert!(fs::rename(&selected, store.root().join("renamed.json")).is_err());
        assert!(fs::rename(store.root(), sandbox.root.join("replaced")).is_err());
        assert!(fs::rename(&sandbox.project, sandbox.root.join("replaced-project")).is_err());
        assert!(fs::rename(&sandbox.root, sandbox.root.with_extension("moved")).is_err());
        store.verify(&cancel).unwrap();
        drop(store);
        fs::write(&selected, b"after drop").unwrap();
        let moved = sandbox.root.join("moved-output");
        fs::rename(&sandbox.output, &moved).unwrap();
        fs::rename(&moved, &sandbox.output).unwrap();
    }

    #[test]
    fn snapshot_native_guards_survive_all_reads_and_release_on_drop() {
        let sandbox = Sandbox::new();
        sandbox.completed();
        let cancel = AtomicBool::new(false);
        let snapshot = ApiSnapshot::open(
            &sandbox.output,
            &names(&["owner.json", "request.json", "report.json"]),
            &cancel,
        )
        .unwrap();
        for name in ["owner.json", "request.json", "report.json"] {
            snapshot.read(name).unwrap();
            assert!(
                OpenOptions::new()
                    .write(true)
                    .open(sandbox.output.join(name))
                    .is_err()
            );
            assert!(
                fs::rename(
                    sandbox.output.join(name),
                    sandbox.output.join("replacement")
                )
                .is_err()
            );
            let link = sandbox.root.join("late-link");
            match fs::hard_link(sandbox.output.join(name), &link) {
                Ok(()) => {
                    assert!(snapshot.verify(&cancel).is_err());
                    fs::remove_file(link).unwrap();
                }
                Err(_) => snapshot.verify(&cancel).unwrap(),
            }
        }
        assert!(fs::rename(&sandbox.output, sandbox.root.join("replacement-root")).is_err());
        assert!(fs::rename(&sandbox.root, sandbox.root.with_extension("moved")).is_err());
        fs::write(sandbox.output.join("unknown"), b"preserve").unwrap();
        assert_eq!(
            snapshot.verify(&cancel).unwrap_err().code,
            "MODEL_API_INVENTORY_INVALID"
        );
        assert!(snapshot.read("request.json").is_err());
        fs::remove_file(sandbox.output.join("unknown")).unwrap();
        cancel.store(true, Ordering::Release);
        assert_eq!(
            snapshot.verify(&cancel).unwrap_err().code,
            "MODEL_API_CANCELLED"
        );
        cancel.store(false, Ordering::Release);
        snapshot.verify(&cancel).unwrap();
        drop(snapshot);
        fs::write(sandbox.output.join("request.json"), b"changed after drop").unwrap();
    }

    #[test]
    fn native_junction_root_or_ancestor_is_rejected_without_touching_target() {
        use std::process::Command;
        let sandbox = Sandbox::new();
        sandbox.completed();
        let junction = sandbox.root.join("junction");
        let dos = |path: &Path| {
            path.to_string_lossy()
                .strip_prefix(r"\\?\")
                .unwrap_or(&path.to_string_lossy())
                .to_owned()
        };
        let status = Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(dos(&junction))
            .arg(dos(&sandbox.output))
            .output()
            .unwrap();
        assert!(status.status.success());
        let before = inventory(&sandbox.output);
        assert_eq!(
            err(preflight(&sandbox.project, &junction.join("child"))).code,
            "MODEL_API_OUTPUT_INVALID"
        );
        assert!(
            ApiSnapshot::open(
                &junction,
                &names(&["owner.json", "request.json", "report.json"]),
                &AtomicBool::new(false)
            )
            .is_err()
        );
        assert_eq!(inventory(&sandbox.output), before);
        fs::remove_dir(&junction).unwrap();
        let _ = File::open(sandbox.output.join("owner.json")).unwrap();
    }
}
