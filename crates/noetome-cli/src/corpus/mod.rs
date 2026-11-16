//! Production source repository shared by CLI and desktop. Legacy fixture stores
//! retain their original contracts; real books are read and committed one page at a time.
mod assets;
pub mod command;
mod export;
mod ocr;
#[cfg(windows)]
mod process_host;
mod store;
#[cfg(test)]
mod tests;
mod worker;

use crate::project::{self, ProjectError};
pub use export::{copy_source, export_source};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};
use store::Repository;
use uuid::Uuid;

pub const MAX_PAGE_BYTES: u64 = 16 * 1024 * 1024;
pub const BATCH_PAGES: u32 = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failure {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSummary {
    pub source_id: String,
    pub display_name: String,
    pub content_sha256: Option<String>,
    pub byte_length: u64,
    pub status: String,
    pub page_count: Option<u32>,
    pub processed_pages: u32,
    pub text_pages: u32,
    pub ocr_pages: u32,
    pub needs_ocr_pages: u32,
    pub review_pages: u32,
    pub failed_pages: u32,
    pub blank_pages: u32,
    pub error: Option<Failure>,
    pub created_at: String,
}

fn error(code: &'static str, message: impl Into<String>) -> ProjectError {
    ProjectError::new(code, 3, message)
}
fn io(path: &Path, err: impl std::fmt::Display) -> ProjectError {
    ProjectError::io(path, err)
}

/// Sampling is a preflight only. It does not assert whole-book readability.
pub fn preflight(path: &Path) -> Result<Value, ProjectError> {
    let path = project::check_components(path)?;
    project::checked_file(&path)?;
    worker::preflight(&path).map(|mut flight| {
        let capability = ocr::capabilities();
        flight["limits"]["ocr_available"] = capability["available"].clone();
        flight["ocr"] = capability;
        flight
    })
}

pub fn list(project: &Path) -> Result<Vec<SourceSummary>, ProjectError> {
    let Some(repo) = Repository::open_existing(project)? else {
        return Ok(vec![]);
    };
    repo.list()
}

pub fn status(project: &Path, id: &str) -> Result<SourceSummary, ProjectError> {
    Repository::required(project)?.summary(id)
}

/// Copy while hashing, keeping memory independent of the size of the book.
/// A failed acquisition never modifies an existing immutable source version.
pub fn import(project: &Path, path: &Path) -> Result<SourceSummary, ProjectError> {
    let path = project::check_components(path)?;
    project::checked_file(&path)?;
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        return Err(error(
            "CORPUS_FORMAT",
            "整书资料库当前接受PDF；MD/TXT请使用文本资料入口。",
        ));
    }
    let repo = Repository::create(project)?;
    if path.starts_with(&repo.root) || export::has_ancestor_identity(&path, &repo.root)? {
        return Err(error("CORPUS_SOURCE_PATH", "请选择资料库外的原始PDF。"));
    }
    let id = Uuid::new_v4().to_string();
    let source_dir = repo.source_directory(&id)?;
    fs::create_dir(&source_dir).map_err(|e| io(&source_dir, e))?;
    let mut lease = Some(repo.lease(&id)?);
    let before = project::checked_file(&path)?;
    repo.insert_source(&id, &path, before.len())?;
    let result = (|| {
        let mut input = project::read_guard(&path)?;
        let original_handle =
            same_file::Handle::from_file(input.try_clone().map_err(|e| io(&path, e))?)
                .map_err(|e| io(&path, e))?;
        let partial = source_dir.join("raw.acquiring");
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .map_err(|e| io(&partial, e))?;
        let mut hash = Sha256::new();
        let mut buffer = vec![0; 1024 * 1024];
        let mut length = 0u64;
        loop {
            let count = input.read(&mut buffer).map_err(|e| io(&path, e))?;
            if count == 0 {
                break;
            }
            output
                .write_all(&buffer[..count])
                .map_err(|e| io(&partial, e))?;
            hash.update(&buffer[..count]);
            length += count as u64;
        }
        output.sync_all().map_err(|e| io(&partial, e))?;
        drop(output);
        let after = input.metadata().map_err(|e| io(&path, e))?;
        if length != before.len()
            || before.len() != after.len()
            || before.modified().ok() != after.modified().ok()
            || original_handle != same_file::Handle::from_path(&path).map_err(|e| io(&path, e))?
        {
            return Err(error(
                "CORPUS_SOURCE_CHANGED",
                "获取期间原件发生变化，未确认此版本；请待同步结束后重新导入。",
            ));
        }
        let digest = format!("{:x}", hash.finalize());
        if let Some(existing) = repo.find_hash(&digest)? {
            // Only our just-created row and unpublished partial are removed.
            repo.delete_acquiring(&id)?;
            fs::remove_file(&partial).map_err(|e| io(&partial, e))?;
            drop(lease.take());
            fs::remove_file(source_dir.join("worker.lock")).map_err(|e| io(&source_dir, e))?;
            fs::remove_dir(&source_dir).map_err(|e| io(&source_dir, e))?;
            return repo.summary(&existing);
        }
        let raw = source_dir.join("raw.pdf");
        fs::rename(&partial, &raw).map_err(|e| io(&raw, e))?;
        repo.acquired(&id, &digest, length)?;
        let flight = worker::preflight(&raw)?;
        repo.register_pages(&id, &flight)?;
        repo.summary(&id)
    })();
    if let Err(ref e) = result {
        repo.fail(&id, e.code(), &e.to_string())?;
    }
    result
}

/// The durable pause request is checked between pages and while the worker runs.
pub fn pause(project: &Path, id: &str) -> Result<SourceSummary, ProjectError> {
    let repo = Repository::required(project)?;
    repo.request_pause(id)?;
    repo.summary(id)
}

pub fn pages(project: &Path, id: &str, offset: u32, limit: u32) -> Result<Value, ProjectError> {
    if !(1..=50).contains(&limit) {
        return Err(error(
            "CORPUS_PAGE_QUERY",
            "每次查询1到50页；整书页数不受此分页限制。",
        ));
    }
    let repo = Repository::required(project)?;
    let summary = repo.summary(id)?;
    Ok(
        json!({"source_id":id,"offset":offset,"limit":limit,"total":summary.page_count,"pages":repo.pages(id,offset,limit)?}),
    )
}

/// Read exactly the saved page bytes named by an evidence digest. A newer OCR
/// revision does not replace this input, and legacy JSON is never rewritten to
/// add a revision field. The digest is the stored JSON SHA, not page_revision.
pub fn page_revision(
    project: &Path,
    id: &str,
    page_number: u32,
    page_sha256: &str,
) -> Result<Value, ProjectError> {
    Repository::required(project)?.page_revision(id, page_number, page_sha256)
}

/// Return the current confirmed page with its original stored-byte digest.
/// Re-encoding the decoded page cannot recover a legacy JSON byte fingerprint.
pub fn page_snapshot(project: &Path, id: &str, page_number: u32) -> Result<Value, ProjectError> {
    Repository::required(project)?.page_snapshot(id, page_number)
}

/// Verify the immutable saved original using bounded streaming reads. This is
/// a read operation: failed verification does not change source or page state.
pub fn verify_source(project: &Path, id: &str) -> Result<SourceSummary, ProjectError> {
    let repo = Repository::required(project)?;
    let summary = repo.summary(id)?;
    let expected = summary
        .content_sha256
        .as_deref()
        .ok_or_else(|| error("CORPUS_NOT_ACQUIRED", "原件尚未确认，不能核对来源。"))?;
    let raw = repo.source_directory(id)?.join("raw.pdf");
    let guard = project::read_guard(&raw)?;
    let before = guard.metadata().map_err(|e| io(&raw, e))?;
    let identity = same_file::Handle::from_file(guard.try_clone().map_err(|e| io(&raw, e))?)
        .map_err(|e| io(&raw, e))?;
    let actual = hash_file(&raw, || Ok(false))?;
    let after = guard.metadata().map_err(|e| io(&raw, e))?;
    if actual.as_deref() != Some(expected)
        || before.len() != summary.byte_length
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || identity != same_file::Handle::from_path(&raw).map_err(|e| io(&raw, e))?
    {
        return Err(error(
            "CORPUS_SOURCE_CHANGED",
            "保存原件的身份、字节数或SHA256不匹配；未修改来源记录。",
        ));
    }
    Ok(summary)
}

pub fn process(
    project: &Path,
    id: &str,
    cancel: Arc<AtomicBool>,
) -> Result<SourceSummary, ProjectError> {
    process_attempt(project, id, cancel, false)
}

/// Explicit retries keep failed revisions and retry each failed page once.
/// Ordinary resume only schedules unconfirmed text and pending OCR pages.
pub fn retry(
    project: &Path,
    id: &str,
    cancel: Arc<AtomicBool>,
) -> Result<SourceSummary, ProjectError> {
    process_attempt(project, id, cancel, true)
}

fn process_attempt(
    project: &Path,
    id: &str,
    cancel: Arc<AtomicBool>,
    retry_failed: bool,
) -> Result<SourceSummary, ProjectError> {
    let repo = Repository::required(project)?;
    let mut summary = repo.summary(id)?;
    let _lease = repo.lease(id)?;
    let raw = repo.source_directory(id)?.join("raw.pdf");
    let expected = match summary.content_sha256.as_deref() {
        Some(expected) => expected,
        None => {
            repo.fail(id, "CORPUS_NOT_ACQUIRED", "原件尚未获取成功，请重新导入。")?;
            return Err(error(
                "CORPUS_NOT_ACQUIRED",
                "原件尚未获取成功，请重新导入。",
            ));
        }
    };
    let expected = expected.to_owned();
    repo.start(id)?;
    let result = (|| {
        let raw_guard = project::read_guard(&raw)?;
        let raw_metadata = raw_guard.metadata().map_err(|e| io(&raw, e))?;
        let raw_identity =
            same_file::Handle::from_file(raw_guard.try_clone().map_err(|e| io(&raw, e))?)
                .map_err(|e| io(&raw, e))?;
        let stopped = || {
            if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                repo.request_pause(id)?;
            }
            repo.paused(id)
        };
        let actual = hash_file(&raw, stopped)?;
        if actual.is_none() {
            repo.finish(id)?;
            return repo.summary(id);
        }
        if actual.as_deref() != Some(expected.as_str()) {
            repo.fail(id, "CORPUS_SOURCE_CHANGED", "保存原件的摘要不匹配。")?;
            return Err(error(
                "CORPUS_SOURCE_CHANGED",
                "已保存原件的SHA256不匹配；已停止解析并保留现场。",
            ));
        }
        if retry_failed {
            repo.retry_failed(id)?;
        }
        if summary.page_count.is_none() {
            repo.register_pages(id, &worker::preflight(&raw)?)?;
            summary = repo.summary(id)?;
        }
        if summary.page_count.is_none() {
            return Ok(summary);
        }
        while let Some((start, end)) = repo.next_batch(id, BATCH_PAGES)? {
            if stopped()? {
                break;
            }
            let run = worker::parse_pages(&raw, start, end, stopped)?;
            let current = raw_guard.metadata().map_err(|e| io(&raw, e))?;
            if current.len() != raw_metadata.len()
                || current.modified().ok() != raw_metadata.modified().ok()
                || raw_identity != same_file::Handle::from_path(&raw).map_err(|e| io(&raw, e))?
            {
                return Err(error(
                    "CORPUS_SOURCE_CHANGED",
                    "解析期间保存原件发生变化，本批结果未确认。",
                ));
            }
            // Each validated page is independently durable, including pages
            // completed before a pause or a worker failure.
            for page in run.pages {
                repo.commit_page(id, page)?;
            }
            if run.cancelled {
                break;
            }
            if let Some(failure) = run.failure {
                repo.fail(id, &failure.code, &failure.message)?;
                return repo.summary(id);
            }
        }
        if !stopped()? {
            if let Some(runtime) = ocr::runtime()? {
                let parent = repo.source_directory(id)?.join("ocr");
                project::check_components(&parent)?;
                fs::create_dir_all(&parent).map_err(|e| io(&parent, e))?;
                while let Some(page) = repo.next_ocr(id)? {
                    if stopped()? {
                        break;
                    }
                    let number = page["page_number"].as_u64().unwrap_or(0) as u32;
                    let result = runtime.page(
                        &raw,
                        &expected,
                        number,
                        &page["geometry"],
                        &parent,
                        stopped,
                    )?;
                    let Some(result) = result else { break };
                    let current = raw_guard.metadata().map_err(|e| io(&raw, e))?;
                    if current.len() != raw_metadata.len()
                        || current.modified().ok() != raw_metadata.modified().ok()
                        || raw_identity
                            != same_file::Handle::from_path(&raw).map_err(|e| io(&raw, e))?
                    {
                        return Err(error(
                            "CORPUS_SOURCE_CHANGED",
                            "OCR期间原件发生变化，结果没有确认。",
                        ));
                    }
                    repo.commit_ocr_page(id, result)?;
                }
            }
        }
        repo.finish(id)?;
        repo.summary(id)
    })();
    if let Err(ref e) = result {
        repo.fail(id, e.code(), &e.to_string())?;
    }
    result
}

fn hash_file(
    path: &Path,
    mut cancelled: impl FnMut() -> Result<bool, ProjectError>,
) -> Result<Option<String>, ProjectError> {
    let mut file = project::read_guard(path)?;
    let mut hash = Sha256::new();
    let mut bytes = vec![0; 1024 * 1024];
    loop {
        if cancelled()? {
            return Ok(None);
        }
        let n = file.read(&mut bytes).map_err(|e| io(path, e))?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    Ok(Some(format!("{:x}", hash.finalize())))
}
