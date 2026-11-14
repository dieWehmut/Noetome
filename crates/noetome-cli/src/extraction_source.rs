//! Immutable source capability for one extraction request.
use crate::{
    extraction_model::{ExtractionLimits, Locator, RequestUnit, Selection, SourceBinding},
    project::{self, ProjectError},
    sidecar_model::SidecarError,
    sidecar_process::Control,
    sidecar_workspace::{Directory, GuardedFile},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::SystemTime,
};

const SOURCE_FILES: usize = 4096;
const SOURCE_BYTES: u64 = 512 * 1024 * 1024;
const DATABASE_BYTES: u64 = 128 * 1024 * 1024;
const FILE_BYTES: u64 = 96 * 1024 * 1024;
const INVENTORY_ENTRIES: usize = 16384;
const CHANGED: &str = "EXTRACT_SOURCE_CHANGED";
fn failure(code: &'static str, exit: u8, message: impl Into<String>) -> ProjectError {
    ProjectError::new(code, exit, message)
}
fn changed(error: impl ToString) -> ProjectError {
    failure(CHANGED, 3, error.to_string())
}
fn bounded_control() -> Control {
    Control::new(30000, Arc::new(AtomicBool::new(false)))
}
fn capture_error(error: SidecarError) -> ProjectError {
    if error.code == "SIDECAR_LIMIT" {
        failure("EXTRACT_SOURCE_LIMIT", 2, error.to_string())
    } else {
        changed(error)
    }
}
struct Entry {
    identity: same_file::Handle,
    directory: bool,
    len: u64,
    modified: SystemTime,
}
fn cache_path(relative: &Path) -> bool {
    relative.starts_with(".noetome-sources") || relative.starts_with(".noetome-pdf-parses")
}
// Only metadata is observed outside the explicit configuration/database/cache.
// Walk iteratively; even an unknown directory tree cannot grow the call stack.
fn inventory(root: &Path, control: &Control) -> Result<BTreeMap<PathBuf, Entry>, ProjectError> {
    let mut entries = BTreeMap::new();
    let mut todo = vec![PathBuf::new()];
    let mut cache_files = 0usize;
    let mut cache_bytes = 0u64;
    while let Some(relative) = todo.pop() {
        control.check().map_err(changed)?;
        if entries.len() >= INVENTORY_ENTRIES || relative.components().count() > 32 {
            return Err(failure(
                "EXTRACT_SOURCE_LIMIT",
                2,
                "来源清单条目或目录深度超限。",
            ));
        }
        let path = root.join(&relative);
        let meta = fs::symlink_metadata(&path).map_err(|e| ProjectError::io(&path, e))?;
        if project::is_link(&meta) || (!meta.is_dir() && !meta.is_file()) {
            return Err(ProjectError::unsafe_path(&path));
        }
        if relative == Path::new(".noetome-corpus") && meta.is_dir() {
            // The production repository has its own mutable transactions and
            // guards. Legacy extraction binds only its selected legacy caches;
            // unrelated OCR progress must not invalidate that source snapshot.
            continue;
        }
        if meta.is_file() && cache_path(&relative) {
            cache_files += 1;
            cache_bytes = cache_bytes
                .checked_add(meta.len())
                .ok_or_else(|| failure("EXTRACT_SOURCE_LIMIT", 2, "来源快照总字节溢出。"))?;
            if cache_files > SOURCE_FILES || cache_bytes > SOURCE_BYTES {
                return Err(failure(
                    "EXTRACT_SOURCE_LIMIT",
                    2,
                    "来源快照超过4096文件或512 MiB。",
                ));
            }
        }
        if meta.is_dir() {
            // Bound pending work as well as visited entries before allocating.
            for child in fs::read_dir(&path).map_err(|e| ProjectError::io(&path, e))? {
                control.check().map_err(changed)?;
                let child = child.map_err(|e| ProjectError::io(&path, e))?;
                if todo.len() + entries.len() >= INVENTORY_ENTRIES {
                    return Err(failure("EXTRACT_SOURCE_LIMIT", 2, "来源清单条目超限。"));
                }
                todo.push(relative.join(child.file_name()));
            }
        }
        entries.insert(
            relative,
            Entry {
                identity: same_file::Handle::from_path(&path)
                    .map_err(|e| ProjectError::io(&path, e))?,
                directory: meta.is_dir(),
                len: meta.len(),
                modified: meta.modified().map_err(|e| ProjectError::io(&path, e))?,
            },
        );
    }
    Ok(entries)
}
fn same_inventory(before: &BTreeMap<PathBuf, Entry>, after: &BTreeMap<PathBuf, Entry>) -> bool {
    before.len() == after.len()
        && before.iter().all(|(path, a)| {
            after.get(path).is_some_and(|b| {
                a.identity == b.identity
                    && a.directory == b.directory
                    && a.len == b.len
                    && a.modified == b.modified
            })
        })
}
fn bounded_text(
    body: &str,
    units: &[RequestUnit],
    limits: &ExtractionLimits,
) -> Result<String, ProjectError> {
    let total = units.iter().try_fold(body.len() as u64, |sum, u| {
        sum.checked_add(u.text.len() as u64)
    });
    if total.is_none_or(|n| n > limits.max_input_bytes) {
        return Err(failure(
            "EXTRACT_INPUT_LIMIT",
            3,
            "所选正文超过请求输入字节限额。",
        ));
    }
    Ok(body.to_owned())
}

pub(crate) struct SourceSession {
    pub(crate) binding: SourceBinding,
    pub(crate) units: Vec<RequestUnit>,
    root: PathBuf,
    inventory: BTreeMap<PathBuf, Entry>,
    directories: Vec<Directory>,
    files: Vec<GuardedFile>,
    _cache_read_guards: Vec<fs::File>,
}
impl SourceSession {
    pub(crate) fn open(
        project: &Path,
        import_id: &str,
        selection: &[Selection],
        limits: &ExtractionLimits,
    ) -> Result<Self, ProjectError> {
        if selection.is_empty() {
            return Err(failure(
                "EXTRACT_SELECTION",
                2,
                "明确选择至少一个来源单位。",
            ));
        }
        if limits.max_units == 0
            || limits.max_units > 64
            || selection.len() > limits.max_units
            || limits.max_input_bytes == 0
            || limits.max_input_bytes > 1024 * 1024
        {
            return Err(failure(
                "EXTRACT_INPUT_LIMIT",
                3,
                "请求单位或输入字节限额超限。",
            ));
        }
        let root = project::check_components(project)?;
        let root = root
            .canonicalize()
            .map_err(|e| ProjectError::io(&root, e))?;
        let control = bounded_control();
        let root_guard = Directory::capture(&root, CHANGED).map_err(capture_error)?;
        let before = inventory(&root, &control)?;
        let mut directories = vec![root_guard];
        let mut files = Vec::new();
        let mut cache_read_guards = Vec::new();
        // Capture the full bounded cache before current_store can read a single
        // source record or snapshot. These capabilities live until final audit.
        for (relative, entry) in &before {
            if relative.as_os_str().is_empty() {
                continue;
            }
            let path = root.join(relative);
            if entry.directory && cache_path(relative) {
                directories.push(Directory::capture(&path, CHANGED).map_err(capture_error)?);
            }
            if !entry.directory
                && (cache_path(relative)
                    || relative == Path::new(project::CONFIG)
                    || relative == Path::new(project::DATABASE))
            {
                if cache_path(relative) {
                    // Retain the read capability without consuming unknown
                    // cache bytes. current_store first proves exact ownership
                    // and the registered snapshot inventory.
                    let guard = project::read_guard(&path)?;
                    crate::sidecar_workspace::check_links(&guard, CHANGED)
                        .map_err(capture_error)?;
                    let opened = same_file::Handle::from_file(
                        guard.try_clone().map_err(|e| ProjectError::io(&path, e))?,
                    )
                    .map_err(|e| ProjectError::io(&path, e))?;
                    if opened != entry.identity {
                        return Err(changed("快照身份在打开期间变化。"));
                    }
                    cache_read_guards.push(guard);
                    continue;
                }
                let max = if relative == Path::new(project::DATABASE) {
                    DATABASE_BYTES
                } else if relative == Path::new(project::CONFIG) {
                    64 * 1024
                } else {
                    FILE_BYTES
                };
                let (guard, _) =
                    GuardedFile::open(&path, max, &control, CHANGED).map_err(capture_error)?;
                files.push(guard);
            }
        }
        if !same_inventory(&before, &inventory(&root, &control)?) {
            return Err(changed("来源在读取前发生变化。"));
        }
        let (info, store) = crate::source::current_store(&root)?;
        // Semantic loading succeeded: cache files are now proven registered.
        for (relative, entry) in &before {
            if !entry.directory && cache_path(relative) {
                let (guard, _) =
                    GuardedFile::open(&root.join(relative), FILE_BYTES, &control, CHANGED)
                        .map_err(capture_error)?;
                files.push(guard);
            }
        }
        // On hosts without mandatory sharing protection, recheck the known
        // source SHA contract after acquiring byte receipts, closing the gap
        // between semantic loading and fingerprint capture.
        store.validate_snapshots(&root, &info.project_id, None)?;
        let unavailable = || {
            failure(
                "EXTRACT_SOURCE_UNAVAILABLE",
                2,
                "来源未成功获取、解析或没有已确认的文本。",
            )
        };
        let mut units = Vec::new();
        let binding;
        let mut selected = BTreeSet::new();
        if let Some(report) = store.reports.iter().find(|r| r.import_id == import_id) {
            let v = report.version.as_ref().ok_or_else(unavailable)?;
            if v.ingest_status != "ready" || report.status == "failed" {
                return Err(unavailable());
            }
            let snapshot = v.normalized_snapshot.as_ref().ok_or_else(unavailable)?;
            let hash = v.normalized_sha256.as_ref().ok_or_else(unavailable)?;
            let (_, bytes) = GuardedFile::open(&root.join(snapshot), FILE_BYTES, &control, CHANGED)
                .map_err(capture_error)?;
            let text = std::str::from_utf8(&bytes).map_err(|_| unavailable())?;
            binding = SourceBinding {
                project_id: info.project_id,
                import_id: import_id.into(),
                source_id: v.source_id.clone(),
                version_id: v.version_id.clone(),
                content_sha256: v.content_sha256.clone(),
                normalized_sha256: Some(hash.clone()),
                parse_id: None,
                manifest_sha256: None,
            };
            for s in selection {
                let Selection::Text { ordinal } = s else {
                    return Err(failure(
                        "EXTRACT_SELECTION",
                        2,
                        "文本来源只能选择Span ordinal。",
                    ));
                };
                if !selected.insert(format!("text:{ordinal}")) {
                    return Err(failure("EXTRACT_SELECTION", 2, "来源单位选择不得重复。"));
                }
                let span = store
                    .spans
                    .get(&v.version_id)
                    .and_then(|spans| spans.iter().find(|span| span.ordinal == *ordinal))
                    .ok_or_else(|| failure("EXTRACT_SELECTION", 2, "Span ordinal不存在。"))?;
                if span.parse_status == crate::text::ParseStatus::Failed {
                    return Err(unavailable());
                }
                let body = text
                    .get(span.start_byte..span.end_byte)
                    .filter(|t| !t.trim().is_empty())
                    .ok_or_else(unavailable)?;
                let body = bounded_text(body, &units, limits)?;
                units.push(RequestUnit {
                    unit_id: format!("unit-{}", units.len()),
                    locator: Locator::Text {
                        span_id: span.span_id.clone(),
                        ordinal: *ordinal,
                        resource_sha256: hash.clone(),
                    },
                    source_status: span.parse_status.as_str().into(),
                    start_byte: span.start_byte,
                    end_byte: span.end_byte,
                    text: body,
                });
            }
        } else if let Some(report) = store.pdf.reports.iter().find(|r| r.import_id == import_id) {
            let v = report.version.as_ref().ok_or_else(unavailable)?;
            let parse_id = report.pdf.parse_id.as_ref().ok_or_else(unavailable)?;
            if report.acquisition_status != "acquired"
                || !matches!(report.pdf.status.as_str(), "parsed" | "needs_review")
            {
                return Err(unavailable());
            }
            let attempt = store.pdf.attempts.get(parse_id).ok_or_else(unavailable)?;
            binding = SourceBinding {
                project_id: info.project_id,
                import_id: import_id.into(),
                source_id: v.source_id.clone(),
                version_id: v.version_id.clone(),
                content_sha256: v.content_sha256.clone(),
                normalized_sha256: None,
                parse_id: Some(parse_id.clone()),
                manifest_sha256: Some(attempt.manifest_sha256.clone().ok_or_else(unavailable)?),
            };
            for s in selection {
                let Selection::Pdf {
                    page_index,
                    region_id,
                } = s
                else {
                    return Err(failure(
                        "EXTRACT_SELECTION",
                        2,
                        "PDF必须选择物理页和真实区域。",
                    ));
                };
                if !selected.insert(format!("pdf:{page_index}:{region_id}")) {
                    return Err(failure("EXTRACT_SELECTION", 2, "来源单位选择不得重复。"));
                }
                let page = store
                    .pdf
                    .pages
                    .get(parse_id)
                    .and_then(|pages| pages.get(page_index))
                    .ok_or_else(|| failure("EXTRACT_SELECTION", 2, "物理页面不存在。"))?;
                if page.status == crate::sidecar_model::ParseStatus::Failed
                    || page.geometry.is_none()
                    || page.text.is_none()
                {
                    return Err(unavailable());
                }
                let region = store
                    .pdf
                    .regions
                    .get(parse_id)
                    .and_then(|regions| regions.get(region_id))
                    .filter(|r| r.physical_page_index == *page_index)
                    .ok_or_else(|| failure("EXTRACT_SELECTION", 2, "所选页面内没有该区域。"))?;
                let resource = store
                    .pdf
                    .resources
                    .get(parse_id)
                    .and_then(|resources| resources.get(&region.text.resource_id))
                    .filter(|r| r.confidence == "confirmed" && r.kind == "utf8")
                    .ok_or_else(unavailable)?;
                let (_, bytes) = GuardedFile::open(
                    &root
                        .join(&attempt.snapshot_path)
                        .join(&resource.relative_path),
                    FILE_BYTES,
                    &control,
                    CHANGED,
                )
                .map_err(capture_error)?;
                let text = std::str::from_utf8(&bytes).map_err(|_| unavailable())?;
                let start = usize::try_from(region.text.start_byte).map_err(|_| unavailable())?;
                let end = usize::try_from(region.text.end_byte).map_err(|_| unavailable())?;
                let body = text
                    .get(start..end)
                    .filter(|t| !t.trim().is_empty())
                    .ok_or_else(unavailable)?;
                let body = bounded_text(body, &units, limits)?;
                units.push(RequestUnit {
                    unit_id: format!("unit-{}", units.len()),
                    locator: Locator::Pdf {
                        parse_id: parse_id.clone(),
                        page_index: *page_index,
                        region_id: region_id.clone(),
                        resource_id: resource.resource_id.clone(),
                        resource_sha256: resource.sha256.clone(),
                        bbox: region.bbox,
                    },
                    source_status: crate::pdf_source_model::status_name(&page.status).into(),
                    start_byte: start,
                    end_byte: end,
                    text: body,
                });
            }
        } else {
            return Err(unavailable());
        }
        let session = Self {
            binding,
            units,
            root,
            inventory: before,
            directories,
            files,
            _cache_read_guards: cache_read_guards,
        };
        session.verify()?;
        Ok(session)
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn verify(&self) -> Result<(), ProjectError> {
        // A fresh local inspection window works after timeout/cancellation of
        // the separate fake request; it never reuses that request's timer.
        let control = bounded_control();
        for directory in &self.directories {
            directory.verify(CHANGED).map_err(changed)?;
        }
        let after = inventory(&self.root, &control).map_err(changed)?;
        if !same_inventory(&self.inventory, &after) {
            return Err(changed("来源身份、mtime或递归清单发生变化。"));
        }
        for file in &self.files {
            file.verify(&control, CHANGED).map_err(changed)?;
        }
        let after = inventory(&self.root, &control).map_err(changed)?;
        if !same_inventory(&self.inventory, &after) {
            return Err(changed("来源在最终读取期间发生变化。"));
        }
        for directory in &self.directories {
            directory.verify(CHANGED).map_err(changed)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn limits() -> ExtractionLimits {
        ExtractionLimits {
            max_units: 8,
            max_input_bytes: 4096,
            max_response_bytes: 32768,
            max_output_tokens: 8192,
            timeout_ms: 2000,
        }
    }
    fn registered() -> (PathBuf, String) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("target/compiler-temp")
            .join(format!("extract-source-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let project = dir.join("project");
        crate::project::initialize(&project).unwrap();
        let input = dir.join("source.md");
        fs::write(&input, "中文定义；单位 ms，条件固定。\n").unwrap();
        let result = crate::source::import(&project, &input, None).unwrap();
        let report: serde_json::Value = serde_json::from_str(&result.json).unwrap();
        (project, report["import_id"].as_str().unwrap().into())
    }
    #[test]
    fn session_uses_real_utf8_span_and_checks_new_project_inventory() {
        let (project, id) = registered();
        let session =
            SourceSession::open(&project, &id, &[Selection::Text { ordinal: 0 }], &limits())
                .unwrap();
        assert_eq!(session.units.len(), 1);
        assert_eq!(session.units[0].text, "中文定义；单位 ms，条件固定。\n");
        session.verify().unwrap();
        fs::write(project.join("private.bin"), b"unknown private bytes").unwrap();
        assert_eq!(
            session.verify().unwrap_err().code(),
            "EXTRACT_SOURCE_CHANGED"
        );
    }
    #[test]
    fn request_selection_and_input_limits_are_enforced() {
        let (project, id) = registered();
        assert_eq!(
            SourceSession::open(&project, &id, &[Selection::Text { ordinal: 99 }], &limits())
                .err()
                .unwrap()
                .code(),
            "EXTRACT_SELECTION"
        );
        let mut small = limits();
        small.max_input_bytes = 1;
        assert_eq!(
            SourceSession::open(&project, &id, &[Selection::Text { ordinal: 0 }], &small)
                .err()
                .unwrap()
                .code(),
            "EXTRACT_INPUT_LIMIT"
        );
    }
    #[test]
    fn source_cache_caps_run_before_semantic_loading() {
        let (project, id) = registered();
        let extra = project.join(".noetome-sources").join("unknown.bin");
        fs::File::create(&extra)
            .unwrap()
            .set_len(SOURCE_BYTES + 1)
            .unwrap();
        assert_eq!(
            SourceSession::open(&project, &id, &[Selection::Text { ordinal: 0 }], &limits())
                .err()
                .unwrap()
                .code(),
            "EXTRACT_SOURCE_LIMIT"
        );
    }
    #[test]
    fn project_database_is_bounded_before_hashing_or_semantic_loading() {
        let (project, id) = registered();
        fs::OpenOptions::new()
            .write(true)
            .open(project.join(project::DATABASE))
            .unwrap()
            .set_len(128 * 1024 * 1024 + 1)
            .unwrap();
        let error =
            SourceSession::open(&project, &id, &[Selection::Text { ordinal: 0 }], &limits())
                .err()
                .expect("oversized SQLite must fail before loading");
        assert_eq!(error.code(), "EXTRACT_SOURCE_LIMIT");
        assert_eq!(error.exit_code(), 2);
    }
    #[test]
    fn registered_cache_hard_link_is_rejected_before_consuming_changed_target() {
        let (project, id) = registered();
        let report = crate::source::inspect_output(&project, &id).unwrap();
        let report: serde_json::Value = serde_json::from_str(&report.json).unwrap();
        let raw = project.join(report["version"]["raw_snapshot"].as_str().unwrap());
        let outside = project.parent().unwrap().join("outside-private.bin");
        let private = vec![b'x'; fs::metadata(&raw).unwrap().len() as usize];
        fs::write(&outside, &private).unwrap();
        fs::remove_file(&raw).unwrap();
        fs::hard_link(&outside, &raw).unwrap();
        let error =
            SourceSession::open(&project, &id, &[Selection::Text { ordinal: 0 }], &limits())
                .err()
                .expect("linked cache must fail before semantic snapshot loading");
        assert_eq!(error.code(), "EXTRACT_SOURCE_CHANGED");
        assert_eq!(fs::read(&outside).unwrap(), private);
    }
    #[test]
    fn source_cache_file_count_is_bounded_before_semantic_loading() {
        let (project, id) = registered();
        let cache = project.join(".noetome-sources");
        for n in 0..SOURCE_FILES {
            fs::File::create(cache.join(format!("unknown-{n}"))).unwrap();
        }
        assert_eq!(
            SourceSession::open(&project, &id, &[Selection::Text { ordinal: 0 }], &limits())
                .err()
                .unwrap()
                .code(),
            "EXTRACT_SOURCE_LIMIT"
        );
    }
    #[test]
    fn failed_utf8_and_failed_pdf_never_supply_request_units() {
        let (project, _) = registered();
        let input = project.parent().unwrap().join("invalid.txt");
        fs::write(&input, [255u8]).unwrap();
        let result = crate::source::import(&project, &input, None).unwrap();
        let report: serde_json::Value = serde_json::from_str(&result.json).unwrap();
        assert_eq!(
            SourceSession::open(
                &project,
                report["import_id"].as_str().unwrap(),
                &[Selection::Text { ordinal: 0 }],
                &limits()
            )
            .err()
            .unwrap()
            .code(),
            "EXTRACT_SOURCE_UNAVAILABLE"
        );
        let pdf = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fixtures/pdf-text-v1/damaged.pdf");
        let result = crate::source::import(&project, &pdf, None).unwrap();
        let report: serde_json::Value = serde_json::from_str(&result.json).unwrap();
        assert_eq!(report["pdf"]["status"], "failed");
        assert_eq!(
            SourceSession::open(
                &project,
                report["import_id"].as_str().unwrap(),
                &[Selection::Pdf {
                    page_index: 0,
                    region_id: "missing".into()
                }],
                &limits()
            )
            .err()
            .unwrap()
            .code(),
            "EXTRACT_SOURCE_UNAVAILABLE"
        );
    }
    #[cfg(not(windows))]
    #[test]
    fn unix_snapshot_byte_changes_are_detected_by_retained_receipt() {
        let (project, id) = registered();
        let session =
            SourceSession::open(&project, &id, &[Selection::Text { ordinal: 0 }], &limits())
                .unwrap();
        let raw = project
            .join(".noetome-sources")
            .join(&session.binding.version_id)
            .join("raw.bin");
        fs::write(raw, b"changed bytes").unwrap();
        assert_eq!(
            session.verify().unwrap_err().code(),
            "EXTRACT_SOURCE_CHANGED"
        );
    }
    #[test]
    fn unknown_private_project_files_use_metadata_only() {
        let (project, id) = registered();
        fs::File::create(project.join("private.bin"))
            .unwrap()
            .set_len(SOURCE_BYTES + 1)
            .unwrap();
        let session =
            SourceSession::open(&project, &id, &[Selection::Text { ordinal: 0 }], &limits())
                .unwrap();
        session.verify().unwrap();
    }
    #[test]
    fn unknown_cache_bytes_are_not_hashed_before_semantic_inventory_rejection() {
        let (project, id) = registered();
        fs::File::create(project.join(".noetome-sources/private.bin"))
            .unwrap()
            .set_len(FILE_BYTES + 1)
            .unwrap();
        let error =
            SourceSession::open(&project, &id, &[Selection::Text { ordinal: 0 }], &limits())
                .err()
                .unwrap();
        assert_eq!(error.code(), "SOURCE_SNAPSHOT_CONFLICT");
    }
    #[test]
    fn actual_pdf_parse_produces_page_resource_and_bbox_without_legacy_span() {
        let (project, _) = registered();
        let pdf = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("fixtures/pdf-text-v1/single-page.pdf");
        let result = crate::source::import(&project, &pdf, None).unwrap();
        let report: serde_json::Value = serde_json::from_str(&result.json).unwrap();
        assert_eq!(report["pdf"]["status"], "parsed", "{report}");
        let (_, store) = crate::source::current_store(&project).unwrap();
        let parse = report["pdf"]["parse_id"].as_str().unwrap();
        let region = store.pdf.regions[parse].values().next().unwrap();
        let selection = Selection::Pdf {
            page_index: region.physical_page_index,
            region_id: region.region_id.clone(),
        };
        let session = SourceSession::open(
            &project,
            report["import_id"].as_str().unwrap(),
            &[selection],
            &limits(),
        )
        .unwrap();
        assert_eq!(session.binding.parse_id.as_deref(), Some(parse));
        assert_eq!(session.binding.normalized_sha256, None);
        assert!(!session.units[0].text.is_empty());
        assert!(matches!(session.units[0].locator,Locator::Pdf{bbox,..} if bbox==region.bbox));
        session.verify().unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn session_holds_config_database_and_snapshot_against_writers() {
        let (project, id) = registered();
        let session =
            SourceSession::open(&project, &id, &[Selection::Text { ordinal: 0 }], &limits())
                .unwrap();
        let snapshot = project
            .join(".noetome-sources")
            .join(&session.binding.version_id)
            .join("normalized.txt");
        for path in [
            project.join("project.json"),
            project.join("project.sqlite"),
            snapshot,
        ] {
            assert!(
                fs::OpenOptions::new().write(true).open(&path).is_err(),
                "writer acquired {}",
                path.display()
            );
        }
    }
}
