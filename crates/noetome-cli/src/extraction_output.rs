//! Capability-held artifacts in a brand-new, explicitly selected test root.
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

fn capture_ancestors(
    expected: &BTreeMap<PathBuf, crate::compile_identity::Identity>,
) -> Result<Vec<Directory>, ProjectError> {
    let mut held = Vec::new();
    for (path, identity) in expected {
        if crate::compile_identity::identity(path)? != *identity {
            return Err(changed());
        }
        let directory =
            Directory::capture(path, "EXTRACT_OUTPUT_CHANGED").map_err(|_| changed())?;
        if crate::compile_identity::identity(path)? != *identity {
            return Err(changed());
        }
        directory
            .verify("EXTRACT_OUTPUT_CHANGED")
            .map_err(|_| changed())?;
        held.push(directory);
    }
    Ok(held)
}

struct Artifact {
    path: PathBuf,
    file: File,
    identity: same_file::Handle,
    modified: SystemTime,
    fingerprint: Fingerprint,
}

impl Artifact {
    fn capture(path: &Path, expected: &crate::run_model::FileStamp) -> Result<Self, ProjectError> {
        project::check_components(path).map_err(|_| changed())?;
        project::checked_file(path).map_err(|_| changed())?;
        if crate::compile_identity::identity(path)? != expected.identity {
            return Err(changed());
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1).custom_flags(0x00200000);
        }
        let mut file = options.open(path).map_err(|e| ProjectError::io(path, e))?;
        check_links(&file, "EXTRACT_OUTPUT_CHANGED").map_err(|_| changed())?;
        let identity = same_file::Handle::from_file(file.try_clone().map_err(|_| changed())?)
            .map_err(|_| changed())?;
        let mut bytes = Vec::new();
        (&mut file)
            .take(FILE_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| changed())?;
        let modified = file
            .metadata()
            .and_then(|m| m.modified())
            .map_err(|_| changed())?;
        if bytes.len() as u64 != expected.byte_length
            || sha256(&bytes) != expected.sha256
            || modified
                .duration_since(UNIX_EPOCH)
                .map_err(|_| changed())?
                .as_nanos()
                .to_string()
                != expected.modified
            || crate::compile_identity::identity(path)? != expected.identity
        {
            return Err(changed());
        }
        let value = Self {
            path: path.to_path_buf(),
            file,
            identity,
            modified,
            fingerprint: Fingerprint {
                sha256: expected.sha256.clone(),
                byte_length: expected.byte_length,
            },
        };
        value.verify()?;
        Ok(value)
    }
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
    ancestors: Vec<Directory>,
    files: BTreeMap<String, Artifact>,
}

impl OutputRoot {
    /// Only a budget-sealed, never-dispatched request may reopen its three
    /// original files. This is not a general existing-output adoption path.
    pub(crate) fn resume_prepared_api(
        path: &Path,
        mode: &str,
        attempt: &crate::request_budget_model::AttemptView,
        run: &crate::request_budget_model::RunView,
    ) -> Result<Self, ProjectError> {
        let expected_names = BTreeSet::from([
            ".owner.json".to_string(),
            "request.json".to_string(),
            "request-body.json".to_string(),
        ]);
        let sealed = &attempt.sealed_files;
        if !matches!(
            attempt.state,
            crate::request_budget_model::AttemptState::Prepared
                | crate::request_budget_model::AttemptState::Reserved
        ) || attempt.transport_sealed
            || attempt.receipt.is_some()
            || attempt.plan.key.run_id != run.spec.run_id
            || sealed.keys().cloned().collect::<BTreeSet<_>>() != expected_names
        {
            return Err(changed());
        }
        let root = project::check_components(path)?;
        let run_root = project::check_components(Path::new(&run.spec.run_root))?;
        let relative = Path::new(&attempt.plan.attempt_root);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            || root != run_root.join(relative)
        {
            return Err(changed());
        }
        let mut expected_ancestors = BTreeMap::from([
            (run_root.clone(), run.identity.root.clone()),
            (
                run_root.parent().ok_or_else(changed)?.to_path_buf(),
                run.identity.parent.clone(),
            ),
            (root.clone(), attempt.identity.root.clone()),
            (
                root.parent().ok_or_else(changed)?.to_path_buf(),
                attempt.identity.parent.clone(),
            ),
        ]);
        let mut prefix = PathBuf::new();
        let mut expected_prefixes = BTreeSet::new();
        for component in relative.components() {
            prefix.push(component);
            let key = prefix.to_str().ok_or_else(changed)?.replace('\\', "/");
            expected_prefixes.insert(key.clone());
            let id = attempt.identity.directories.get(&key).ok_or_else(changed)?;
            let absolute = run_root.join(&prefix);
            if let Some(prior) = expected_ancestors.insert(absolute, id.clone()) {
                if prior != *id {
                    return Err(changed());
                }
            }
        }
        if attempt
            .identity
            .directories
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>()
            != expected_prefixes
        {
            return Err(changed());
        }
        let ancestors = capture_ancestors(&expected_ancestors)?;
        let parent =
            Directory::capture(root.parent().ok_or_else(changed)?, "EXTRACT_OUTPUT_CHANGED")
                .map_err(|_| changed())?;
        let directory =
            Directory::capture(&root, "EXTRACT_OUTPUT_CHANGED").map_err(|_| changed())?;
        let mut files = BTreeMap::new();
        for (name, pin) in sealed {
            files.insert(name.clone(), Artifact::capture(&root.join(name), pin)?);
        }
        let value = Self {
            root,
            parent,
            directory,
            ancestors,
            files,
        };
        value.verify()?;
        let expected_owner = serde_json::json!({"schema_version":1,"producer":"api","request_id":attempt.plan.key.request_id,"purpose":"unpublished_extraction_test","transport_mode":mode});
        let owner: serde_json::Value =
            serde_json::from_slice(&value.read(".owner.json")?).map_err(|_| changed())?;
        if owner != expected_owner {
            return Err(changed());
        }
        Ok(value)
    }
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

    pub(crate) fn create_api_guarded(
        path: &Path,
        request_id: &str,
        transport_mode: &str,
        expected: &BTreeMap<PathBuf, crate::compile_identity::Identity>,
    ) -> Result<Self, ProjectError> {
        Self::create_owned_guarded(path, request_id, Some(transport_mode), expected)
    }

    pub(crate) fn create_review(path: &Path, request_id: &str) -> Result<Self, ProjectError> {
        Self::create_owned(path, request_id, Some("source_review"))
    }

    pub(crate) fn create_coordination(path: &Path, request_id: &str) -> Result<Self, ProjectError> {
        Self::create_owned(path, request_id, Some("knowledge_coordination"))
    }
    pub(crate) fn create_presentation(path: &Path, request_id: &str) -> Result<Self, ProjectError> {
        Self::create_owned(path, request_id, Some("knowledge_presentation"))
    }
    pub(crate) fn create_process_review(
        path: &Path,
        request_id: &str,
    ) -> Result<Self, ProjectError> {
        Self::create_owned(path, request_id, Some("process_source_review"))
    }
    pub(crate) fn create_flow(path: &Path, request_id: &str) -> Result<Self, ProjectError> {
        Self::create_owned(path, request_id, Some("knowledge_flow"))
    }
    pub(crate) fn create_formula(path: &Path, request_id: &str) -> Result<Self, ProjectError> {
        Self::create_owned(path, request_id, Some("knowledge_formula"))
    }
    pub(crate) fn create_image(path: &Path, request_id: &str) -> Result<Self, ProjectError> {
        Self::create_owned(path, request_id, Some("knowledge_image"))
    }
    pub(crate) fn create_scan(path: &Path, request_id: &str) -> Result<Self, ProjectError> {
        Self::create_owned(path, request_id, Some("knowledge_scan_formula"))
    }
    pub(crate) fn discard_scan_result(&mut self) -> Result<(), ProjectError> {
        let mut first_error = None;
        for name in [
            "scan.json",
            "scan.md",
            "plan.json",
            "compiled.tex",
            "扫描公式预览.md",
        ] {
            if let Err(error) = self.discard_names_inner(&[name], true) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    pub(crate) fn discard_image_result(&mut self) -> Result<(), ProjectError> {
        let mut first_error = None;
        for name in [
            "image.json",
            "image.md",
            "plan.json",
            "图像预览.md",
            "redrawn.svg",
        ] {
            if let Err(error) = self.discard_names_inner(&[name], true) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    pub(crate) fn discard_formula_result(&mut self) -> Result<(), ProjectError> {
        let mut first_error = None;
        for name in [
            "formula.json",
            "formula.md",
            "plan.json",
            "compiled.tex",
            "公式预览.md",
        ] {
            if let Err(error) = self.discard_names_inner(&[name], true) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    pub(crate) fn discard_process_result(&mut self) -> Result<(), ProjectError> {
        let mut names: Vec<String> = self
            .files
            .keys()
            .filter(|n| !n.starts_with('.'))
            .cloned()
            .collect();
        names.sort_by_key(|n| {
            (
                !matches!(
                    n.as_str(),
                    "review.json" | "review.md" | "flow.json" | "flow.md"
                ),
                n.clone(),
            )
        });
        let mut first_error = None;
        for name in names {
            if let Err(error) = self.discard_names_inner(&[name.as_str()], true) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    pub(crate) fn discard_presentation_result(&mut self) -> Result<(), ProjectError> {
        let names: Vec<String> = self
            .files
            .keys()
            .filter(|name| !name.starts_with('.'))
            .cloned()
            .collect();
        let names = ["presentation.json", "presentation.md"]
            .into_iter()
            .map(String::from)
            .chain(
                names
                    .into_iter()
                    .filter(|name| name != "presentation.json" && name != "presentation.md"),
            );
        // Continue proving each captured pathname even if another result changed.
        // Changed and unknown bytes remain; unchanged success reports can be removed.
        let mut first_error = None;
        for name in names {
            if let Err(error) = self.discard_names_inner(&[name.as_str()], true) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    pub(crate) fn discard_coordination_result(&mut self) -> Result<(), ProjectError> {
        self.discard_names_inner(
            &[
                "coordination-ir.json",
                "coordination.json",
                "coordination.md",
            ],
            true,
        )
    }
    fn create_owned(
        path: &Path,
        request_id: &str,
        mode: Option<&str>,
    ) -> Result<Self, ProjectError> {
        Self::create_owned_guarded(path, request_id, mode, &BTreeMap::new())
    }
    fn create_owned_guarded(
        path: &Path,
        request_id: &str,
        mode: Option<&str>,
        expected: &BTreeMap<PathBuf, crate::compile_identity::Identity>,
    ) -> Result<Self, ProjectError> {
        let ancestors = capture_ancestors(expected)?;
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
            ancestors,
            files: BTreeMap::new(),
        };
        let coordination = mode == Some("knowledge_coordination");
        let review = mode == Some("source_review");
        let presentation = mode == Some("knowledge_presentation");
        let process = mode == Some("process_source_review");
        let flow = mode == Some("knowledge_flow");
        let formula = mode == Some("knowledge_formula");
        let image = mode == Some("knowledge_image");
        let scan = mode == Some("knowledge_scan_formula");
        let mut owner = serde_json::json!({"schema_version":1,"producer":if scan {"knowledge_scan_formula"} else if image {"knowledge_image"} else if formula {"knowledge_formula"} else if flow {"knowledge_flow"} else if process {"process_source_review"} else if presentation {"knowledge_presentation"} else if coordination {"knowledge_coordination"} else if review {"source_review"} else if mode.is_some() {"api"} else {"fake"},"request_id":request_id,"purpose":if scan {"unpublished_scan_formula"} else if image {"unpublished_image"} else if formula {"unpublished_formula"} else if flow {"unpublished_flow"} else if process {"unpublished_process_source_review"} else if presentation {"unpublished_presentation"} else if coordination {"unpublished_coordination"} else if review {"unpublished_source_review"} else {"unpublished_extraction_test"}});
        if let Some(mode) = mode.filter(|_| {
            !review
                && !coordination
                && !presentation
                && !process
                && !flow
                && !formula
                && !image
                && !scan
        }) {
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
        for directory in &self.ancestors {
            directory
                .verify("EXTRACT_OUTPUT_CHANGED")
                .map_err(|_| changed())?;
        }
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
        for directory in &self.ancestors {
            directory
                .verify("EXTRACT_OUTPUT_CHANGED")
                .map_err(|_| changed())?;
        }
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

#[cfg(all(test, unix))]
mod presentation_cleanup_tests {
    use super::*;

    #[test]
    fn a_changed_plan_preserves_changed_and_unknown_bytes_but_removes_passed_reports() {
        let root = std::env::temp_dir().join(format!(
            "noetome-presentation-cleanup-{}",
            uuid::Uuid::new_v4()
        ));
        let mut output =
            OutputRoot::create_presentation(&root, &uuid::Uuid::new_v4().to_string()).unwrap();
        output
            .add(".selection-original.json", b"original selection")
            .unwrap();
        output.add("plan.json", b"validated plan").unwrap();
        output
            .add("presentation.json", b"{\"status\":\"passed\"}")
            .unwrap();
        output.add("presentation.md", b"passed").unwrap();
        fs::write(root.join("plan.json"), b"changed by another writer").unwrap();
        fs::write(root.join("human.md"), b"unknown human content").unwrap();
        assert_eq!(
            output.discard_presentation_result().unwrap_err().code(),
            "EXTRACT_OUTPUT_CHANGED"
        );
        assert_eq!(
            fs::read(root.join("plan.json")).unwrap(),
            b"changed by another writer"
        );
        assert_eq!(
            fs::read(root.join("human.md")).unwrap(),
            b"unknown human content"
        );
        assert_eq!(
            fs::read(root.join(".selection-original.json")).unwrap(),
            b"original selection"
        );
        assert!(
            !root.join("presentation.json").exists(),
            "passed JSON must be removed even when a captured plan changed"
        );
        assert!(
            !root.join("presentation.md").exists(),
            "passed Markdown must be removed even when a captured plan changed"
        );
        drop(output);
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod budget_output_tests {
    use super::*;

    #[test]
    fn guarded_api_rejects_replaced_parent_before_creating_any_output() {
        let base =
            std::env::temp_dir().join(format!("noetome-budget-parent-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&base).unwrap();
        let parent = base.join("selected parent");
        fs::create_dir(&parent).unwrap();
        let expected = BTreeMap::from([(
            parent.clone(),
            crate::compile_identity::identity(&parent).unwrap(),
        )]);
        fs::rename(&parent, base.join("preserved original parent")).unwrap();
        fs::create_dir(&parent).unwrap();
        let human = parent.join("human.txt");
        fs::write(&human, b"replacement belongs to another owner").unwrap();
        let output = parent.join("new attempt");
        assert_eq!(
            OutputRoot::create_api_guarded(
                &output,
                &uuid::Uuid::new_v4().to_string(),
                "local_http_test",
                &expected
            )
            .err()
            .unwrap()
            .code(),
            "EXTRACT_OUTPUT_CHANGED"
        );
        assert!(!output.exists());
        assert_eq!(
            fs::read(&human).unwrap(),
            b"replacement belongs to another owner"
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn held_api_ancestor_rejects_same_leaf_and_parent_moved_into_replacement() {
        let base =
            std::env::temp_dir().join(format!("noetome-budget-ancestor-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&base).unwrap();
        let ancestor = base.join("attempts");
        fs::create_dir(&ancestor).unwrap();
        let parent = ancestor.join("unit-a");
        fs::create_dir(&parent).unwrap();
        let expected = BTreeMap::from([
            (
                ancestor.clone(),
                crate::compile_identity::identity(&ancestor).unwrap(),
            ),
            (
                parent.clone(),
                crate::compile_identity::identity(&parent).unwrap(),
            ),
        ]);
        let root = parent.join("1");
        let mut output = OutputRoot::create_api_guarded(
            &root,
            &uuid::Uuid::new_v4().to_string(),
            "local_http_test",
            &expected,
        )
        .unwrap();
        output
            .add("request-body.json", b"unchanged original request")
            .unwrap();
        let leaf_id = crate::compile_identity::identity(&root).unwrap();
        let parent_id = crate::compile_identity::identity(&parent).unwrap();
        let preserved = base.join("preserved attempts");
        fs::rename(&ancestor, &preserved).unwrap();
        fs::create_dir(&ancestor).unwrap();
        fs::rename(preserved.join("unit-a"), &parent).unwrap();
        assert_eq!(crate::compile_identity::identity(&root).unwrap(), leaf_id);
        assert_eq!(
            crate::compile_identity::identity(&parent).unwrap(),
            parent_id
        );
        assert_eq!(
            output
                .add("validation.json", b"must not be saved")
                .unwrap_err()
                .code(),
            "EXTRACT_OUTPUT_CHANGED"
        );
        assert!(!root.join("validation.json").exists());
        assert_eq!(
            fs::read(root.join("request-body.json")).unwrap(),
            b"unchanged original request"
        );
        drop(output);
        fs::remove_dir_all(base).unwrap();
    }
}
