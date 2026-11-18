//! Durable compilation of fixed real-source pages. Historical fixture workflows
//! retain their original contracts; model output never grants scientific review.
pub mod command;
mod export;
mod model;
mod provider;
mod store;
pub use export::export;
pub use model::{CompileConfig, RunSummary};

use crate::{
    corpus,
    production_budget::{
        BudgetAttemptKey, BudgetAttemptState, BudgetAuthority, BudgetRequest, BudgetSettlement,
    },
    project::{self, ProjectError},
    text::sha256,
};
use model::*;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use store::Store;

pub(super) fn error(code: &'static str, message: impl Into<String>) -> ProjectError {
    ProjectError::new(code, 3, message)
}
fn required(project: &Path) -> Result<Store, ProjectError> {
    Store::open(project)?.ok_or_else(|| error("COMPILE_NOT_FOUND", "项目没有持久编译记录。"))
}

pub fn create(
    project: &Path,
    source_ids: &[String],
    config: CompileConfig,
) -> Result<RunSummary, ProjectError> {
    provider::validate_config(&config)?;
    if source_ids.is_empty()
        || source_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != source_ids.len()
    {
        return Err(error(
            "COMPILE_SELECTION",
            "请选择不重复的真实来源；范围为所有物理页。",
        ));
    }
    let mut sources = Vec::new();
    for id in source_ids {
        let source = corpus::verify_source(project, id)?;
        sources.push(SourceScope {
            source_id: id.clone(),
            source_sha256: source
                .content_sha256
                .ok_or_else(|| error("COMPILE_SOURCE", "来源原件尚未确认。"))?,
            display_name: source.display_name,
            byte_length: source.byte_length,
            physical_pages: source
                .page_count
                .ok_or_else(|| error("COMPILE_SOURCE", "来源物理页数未知。"))?,
        });
    }
    // This is an explicit migration of the existing authority, never a new cap.
    // Missing history, partial migration or modified ownership fails closed.
    let _budget = if project::exists(&crate::production_budget::authority_marker_path(
        &config.usage_ledger,
    ))? || project::exists(&crate::production_budget::default_database_path(
        &config.usage_ledger,
    ))? {
        BudgetAuthority::open(&config.usage_ledger)?
    } else {
        BudgetAuthority::migrate(&config.usage_ledger)?
    };
    let store = Store::create(project)?;
    let id = store.create_run(&config, &sources)?;
    status(project, &id)
}

pub fn list(project: &Path) -> Result<Vec<RunSummary>, ProjectError> {
    match Store::open(project)? {
        Some(store) => store.list(),
        None => Ok(Vec::new()),
    }
}
pub fn status(project: &Path, run_id: &str) -> Result<RunSummary, ProjectError> {
    let store = required(project)?;
    let mut summary = store.summary(run_id)?;
    let config = store.config(run_id)?;
    let mut budget = BudgetAuthority::open(&config.usage_ledger)?;
    summary.budget = Some(
        serde_json::to_value(budget.status()?)
            .map_err(|_| error("COMPILE_ENCODING", "预算状态编码失败。"))?,
    );
    Ok(summary)
}
pub fn control(project: &Path, run_id: &str, action: &str) -> Result<RunSummary, ProjectError> {
    let store = required(project)?;
    match action {
        "pause" => store.pause(run_id)?,
        "cancel" => store.cancel(run_id)?,
        "retry" => store.retry_failed(run_id)?,
        _ => {
            return Err(error(
                "COMPILE_CONTROL",
                "支持暂停、取消和显式失败重试。继续请运行现有任务。",
            ));
        }
    }
    status(project, run_id)
}
pub fn query(
    project: &Path,
    run_id: &str,
    kind: &str,
    offset: u64,
    limit: u32,
    search: &str,
) -> Result<Value, ProjectError> {
    required(project)?.query(run_id, kind, offset, limit, search)
}
pub fn evidence(project: &Path, run_id: &str, id: &str) -> Result<Value, ProjectError> {
    let evidence = required(project)?.evidence(run_id, id)?;
    let source = evidence["source_id"]
        .as_str()
        .ok_or_else(|| error("COMPILE_EVIDENCE", "证据来源缺失。"))?;
    let number = evidence["page_number"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| error("COMPILE_EVIDENCE", "证据物理页号无效。"))?;
    let sha = evidence["page_sha256"]
        .as_str()
        .ok_or_else(|| error("COMPILE_EVIDENCE", "证据页版本缺失。"))?;
    let page = corpus::page_revision(project, source, number, sha)?;
    Ok(json!({"evidence":evidence,"page":page}))
}

fn key(run_id: &str, attempt: &Attempt) -> BudgetAttemptKey {
    BudgetAttemptKey {
        run_id: run_id.into(),
        unit_id: attempt.unit_id.clone(),
        attempt_no: attempt.attempt_no,
        request_id: attempt.request_id.clone(),
    }
}
pub(super) fn read_artifact(path: &Path, limit: u64) -> Result<Vec<u8>, ProjectError> {
    project::check_components(path)?;
    project::checked_file(path)?;
    let identity = crate::compile_identity::identity(path)?;
    let mut guard = project::read_guard(path)?;
    crate::sidecar_workspace::check_links(&guard, "COMPILE_INTEGRITY")
        .map_err(|_| error("COMPILE_INTEGRITY", "原件拥有多个硬链接或不能核对链接数。"))?;
    let opened =
        same_file::Handle::from_file(guard.try_clone().map_err(|e| ProjectError::io(path, e))?)
            .map_err(|e| ProjectError::io(path, e))?;
    let before = guard.metadata().map_err(|e| ProjectError::io(path, e))?;
    if before.len() > limit {
        return Err(error(
            "COMPILE_ARTIFACT_LIMIT",
            "单次请求原件超过已声明限制。",
        ));
    }
    let mut bytes = Vec::new();
    (&mut guard)
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| ProjectError::io(path, e))?;
    let after = guard.metadata().map_err(|e| ProjectError::io(path, e))?;
    if bytes.len() as u64 != before.len()
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || identity != crate::compile_identity::identity(path)?
        || opened != same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?
    {
        return Err(error(
            "COMPILE_ARTIFACT_CHANGED",
            "单次原件在读取期间变化或超限。",
        ));
    }
    Ok(bytes)
}
fn save_artifact(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    project::check_components(path)?;
    if path.exists() {
        if read_artifact(path, bytes.len() as u64)? == bytes {
            return Ok(());
        }
        return Err(error(
            "COMPILE_ARTIFACT_CHANGED",
            "已有请求原件不同，不覆盖历史。",
        ));
    }
    let mut file = store::guarded(path, true, false)?;
    file.write_all(bytes)
        .map_err(|e| ProjectError::io(path, e))?;
    file.sync_all().map_err(|e| ProjectError::io(path, e))?;
    let mut guard = crate::compile_identity::read_after_write(path, file)?;
    let mut saved = Vec::new();
    (&mut guard)
        .take(bytes.len() as u64 + 1)
        .read_to_end(&mut saved)
        .map_err(|e| ProjectError::io(path, e))?;
    if saved != bytes {
        return Err(error("COMPILE_ARTIFACT_CHANGED", "保存原件重读不一致。"));
    }
    Ok(())
}

fn chunks(
    source: &SourceScope,
    page: &Value,
    page_sha: &str,
    max: usize,
) -> Result<Vec<Unit>, ProjectError> {
    let text = page["text"].as_str().unwrap_or("");
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let page_number = page["page_number"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| error("COMPILE_PAGE", "来源页号未知。"))?;
    let width = page["width_pt"]
        .as_f64()
        .ok_or_else(|| error("COMPILE_PAGE", "来源页宽未知。"))?;
    let height = page["height_pt"]
        .as_f64()
        .ok_or_else(|| error("COMPILE_PAGE", "来源页高未知。"))?;
    let issues = page["issues"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| i["code"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    let mut units = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + max).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            return Err(error("COMPILE_UTF8", "片段上限不足以容纳一个字符。"));
        }
        if end < text.len() {
            if let Some(newline) = text[start..end].rfind('\n').filter(|i| *i > max / 2) {
                end = start + newline + 1;
            }
        }
        units.push(Unit {
            unit_id: uuid::Uuid::new_v4().to_string(),
            source_id: source.source_id.clone(),
            source_sha256: source.source_sha256.clone(),
            page_number,
            page_sha256: page_sha.into(),
            bbox: [0.0, 0.0, width, height],
            source_status: page["status"].as_str().unwrap_or("pending").into(),
            start_byte: start,
            end_byte: end,
            text: text[start..end].into(),
            issue_codes: issues.clone(),
        });
        start = end;
    }
    Ok(units)
}

fn check_stop(store: &Store, id: &str, cancel: &AtomicBool) -> Result<(), ProjectError> {
    if cancel.load(Ordering::SeqCst) || store.should_stop(id)? {
        return Err(error(
            "COMPILE_PAUSED",
            "停止新调度；在途消耗按实际回执保存。",
        ));
    }
    Ok(())
}

fn finish_source_pages(
    project: &Path,
    run_id: &str,
    source_id: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<(), ProjectError> {
    let finished = Arc::new(AtomicBool::new(false));
    let parser_cancel = Arc::new(AtomicBool::new(false));
    let watcher_project = project.to_owned();
    let watcher_run = run_id.to_owned();
    let watcher_finished = finished.clone();
    let watcher_cancel = parser_cancel.clone();
    let outer_cancel = cancel.clone();
    let watcher = std::thread::spawn(move || {
        let control = required(&watcher_project);
        while !watcher_finished.load(Ordering::SeqCst) {
            if outer_cancel.load(Ordering::SeqCst) {
                watcher_cancel.store(true, Ordering::SeqCst);
                break;
            }
            match &control {
                Ok(store) if store.should_stop(&watcher_run).unwrap_or(true) => {
                    watcher_cancel.store(true, Ordering::SeqCst);
                    break;
                }
                Err(_) => {
                    watcher_cancel.store(true, Ordering::SeqCst);
                    break;
                }
                _ => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    });
    if cancel.load(Ordering::SeqCst) {
        parser_cancel.store(true, Ordering::SeqCst);
    }
    let result = corpus::process(project, source_id, parser_cancel);
    finished.store(true, Ordering::SeqCst);
    let _ = watcher.join();
    result.map(|_| ())
}

pub fn run(
    project: &Path,
    run_id: &str,
    cancel: Arc<AtomicBool>,
) -> Result<RunSummary, ProjectError> {
    let store = required(project)?;
    let _lease = store.lease(run_id)?;
    let previous = store.summary(run_id)?;
    if previous.status != "cancelled" && previous.error.is_some() {
        // A service/protocol failure closes scheduling until the operator
        // explicitly retries. Reopening alone never clears that stop reason.
        drop(_lease);
        return status(project, run_id);
    }
    let config = store.config(run_id)?;
    provider::validate_config(&config)?;
    let credential = if config.transport_mode == "local_openai" {
        String::new()
    } else {
        std::env::var(&config.credential_env)
            .map_err(|_| error("COMPILE_CREDENTIAL", "所选凭据未注入，未发送模型请求。"))?
    };
    if config.transport_mode != "local_openai"
        && (credential.is_empty()
            || credential.len() > 8192
            || !credential.bytes().all(|b| (33..=126).contains(&b)))
    {
        return Err(error(
            "COMPILE_CREDENTIAL",
            "所选凭据格式无效，未发送模型请求。",
        ));
    }
    let mut budget = BudgetAuthority::open(&config.usage_ledger)?;
    store.resume(run_id)?;
    let outcome = execute(
        project,
        run_id,
        &store,
        &config,
        &credential,
        &mut budget,
        &cancel,
    );
    if let Err(e) = outcome {
        if e.code() == "COMPILE_PAUSED" {
            if cancel.load(Ordering::SeqCst) {
                store.pause(run_id)?;
            }
        } else {
            store.set_error(run_id, e.code(), &e.to_string())?;
            store.set_status(
                run_id,
                if e.code().contains("BUDGET") {
                    "budget_stop"
                } else {
                    "failed"
                },
                "extracting",
            )?;
        }
    }
    drop(_lease);
    status(project, run_id)
}

const MAX_CONSECUTIVE_UNIT_FAILURES: usize = 5;

fn unit_scoped_failure(e: &ProjectError) -> bool {
    matches!(
        e.code(),
        "COMPILE_REQUEST_FAILED" | "COMPILE_CANDIDATE_FAILED"
    )
}

fn execute(
    project: &Path,
    run_id: &str,
    store: &Store,
    config: &CompileConfig,
    credential: &str,
    budget: &mut BudgetAuthority,
    cancel: &Arc<AtomicBool>,
) -> Result<(), ProjectError> {
    for source in &store.summary(run_id)?.sources {
        let current = corpus::verify_source(project, &source.source_id)?;
        if current.content_sha256.as_deref() != Some(&source.source_sha256)
            || current.page_count != Some(source.physical_pages)
        {
            return Err(error(
                "COMPILE_SOURCE_CHANGED",
                "原件快照或全书页分母变化。",
            ));
        }
        if current.processed_pages < source.physical_pages {
            store.set_status(run_id, "running", "preparing")?;
            check_stop(store, run_id, cancel)?;
            finish_source_pages(project, run_id, &source.source_id, cancel)?;
            check_stop(store, run_id, cancel)?;
        }
    }
    store.set_status(run_id, "running", "preparing")?;
    while let Some((source, number)) = store.next_unprepared_page(run_id)? {
        check_stop(store, run_id, cancel)?;
        let snapshot = corpus::page_snapshot(project, &source.source_id, number)?;
        let page = &snapshot["page"];
        let sha = snapshot["page_sha256"]
            .as_str()
            .ok_or_else(|| error("COMPILE_PAGE", "页原字节摘要缺失。"))?;
        let units = chunks(&source, page, sha, config.max_chunk_bytes)?;
        let issues = page["issues"].as_array().cloned().unwrap_or_default();
        store.save_page(
            run_id,
            &source,
            number,
            sha,
            page["status"].as_str().unwrap_or("pending"),
            units,
            issues,
        )?;
    }
    store.set_status(run_id, "running", "extracting")?;
    for attempt in store.attempts_needing_recovery(run_id)? {
        check_stop(store, run_id, cancel)?;
        let unit = store.unit(run_id, &attempt.unit_id)?;
        let directory = store.root.join(&attempt.directory);
        if directory.join("receipt.json").exists() {
            finish(
                project, run_id, store, config, budget, &unit, &attempt, credential,
            )?;
        } else {
            let state = budget.attempt(&key(run_id, &attempt))?;
            if state.is_some_and(|a| {
                matches!(
                    a.state,
                    BudgetAttemptState::Sent | BudgetAttemptState::Unknown
                )
            }) {
                store.mark_unit_failed(
                    run_id,
                    &unit.unit_id,
                    "COMPILE_USAGE_UNKNOWN",
                    "已发送请求缺完整回执，保留旧预留；显式重试另建attempt。",
                )?;
            } else {
                if let Some(pending) = budget.attempt(&key(run_id, &attempt))? {
                    if pending.state == BudgetAttemptState::Reserved {
                        let bytes =
                            read_artifact(&directory.join("request-body.json"), 256 * 1024)?;
                        if sha256(&bytes) != attempt.request_sha256 {
                            return Err(error(
                                "COMPILE_REQUEST_CHANGED",
                                "未发送请求原件变化，不释放预留。",
                            ));
                        }
                        let proof=serde_json::to_vec(&json!({"schema_version":1,"request_id":attempt.request_id,
                            "request_sha256":attempt.request_sha256,"network_attempts":0,"reason":"Durable budget state remained Reserved; mark_sent was never committed"})).unwrap();
                        save_artifact(&directory.join("not-sent.json"), &proof)?;
                        budget.settle(
                            &key(run_id, &attempt),
                            &BudgetSettlement {
                                network_attempts: 0,
                                usage: None,
                                receipt_sha256: sha256(&proof),
                            },
                        )?;
                    }
                }
                store.mark_unit_failed(
                    run_id,
                    &unit.unit_id,
                    "COMPILE_NOT_SENT",
                    "准备中断，未确认发送；重试使用新attempt且历史保留。",
                )?;
            }
        }
    }
    let mut consecutive_failures = 0usize;
    while let Some(unit) = store.next_unit(run_id)? {
        check_stop(store, run_id, cancel)?;
        let current_page = corpus::page_revision(
            project,
            &unit.source_id,
            unit.page_number,
            &unit.page_sha256,
        )?;
        if current_page["text"]
            .as_str()
            .and_then(|t| t.get(unit.start_byte..unit.end_byte))
            != Some(&unit.text)
        {
            return Err(error(
                "COMPILE_SOURCE_CHANGED",
                "固定来源切片与准备记录不同。",
            ));
        }
        let context = store.context_concepts(run_id, &unit.text, 16)?;
        let request_id = uuid::Uuid::new_v4().to_string();
        let prepared = provider::prepare(
            store.project_id(),
            run_id,
            &request_id,
            &unit,
            config,
            &context,
        )?;
        if !credential.is_empty()
            && crate::extraction_api::credential_echo(&prepared.body, credential)
        {
            return Err(error("COMPILE_CREDENTIAL_ECHO", "请求含外部凭据，未发送。"));
        }
        let request_sha = sha256(&prepared.body);
        let mut attempt = store.begin_attempt(run_id, &unit, &request_id, &request_sha)?;
        let directory = store.root.join(&attempt.directory);
        save_artifact(&directory.join("request-body.json"), &prepared.body)?;
        let reservation = (prepared.body.len() as u64)
            .checked_add(1024)
            .and_then(|n| n.checked_add(config.max_output_tokens))
            .ok_or_else(|| error("COMPILE_BUDGET", "请求保守预留溢出。"))?;
        budget.reserve(&BudgetRequest {
            key: key(run_id, &attempt),
            request_sha256: request_sha.clone(),
            reserved_tokens: reservation,
        })?;
        check_stop(store, run_id, cancel)?;
        budget.mark_sent(&key(run_id, &attempt))?;
        attempt.state = "sent".into();
        store.update_attempt(run_id, &attempt)?;
        let transport = crate::extraction_api_transport::send(
            &config.endpoint,
            credential,
            &prepared.body,
            Duration::from_millis(config.request_timeout_ms),
            1024 * 1024,
            &|| check_stop(store, run_id, cancel),
        )?;
        if !credential.is_empty()
            && crate::extraction_api::credential_echo(&transport.body, credential)
        {
            store.mark_unit_failed(
                run_id,
                &unit.unit_id,
                "COMPILE_CREDENTIAL_ECHO",
                "模型回显凭据，未保存泄密原件；预留保持未知。",
            )?;
            return Err(error(
                "COMPILE_CREDENTIAL_ECHO",
                "模型回显凭据；已停止新请求，预留保持未知。",
            ));
        }
        save_artifact(&directory.join("response.bin"), &transport.body)?;
        let receipt = provider::Receipt {
            schema_version: 1,
            request_id,
            request_sha256: request_sha,
            status: transport.status,
            network_attempts: transport.attempts,
            complete: transport.complete,
            response_sha256: sha256(&transport.body),
            response_bytes: transport.body.len() as u64,
            failure_code: transport.failure.as_ref().map(|e| e.code().into()),
        };
        let bytes = serde_json::to_vec(&receipt)
            .map_err(|_| error("COMPILE_ENCODING", "回执编码失败。"))?;
        save_artifact(&directory.join("receipt.json"), &bytes)?;
        attempt.state = "response_saved".into();
        attempt.response_sha256 = Some(receipt.response_sha256);
        attempt.receipt_sha256 = Some(sha256(&bytes));
        store.update_attempt(run_id, &attempt)?;
        match finish(
            project, run_id, store, config, budget, &unit, &attempt, credential,
        ) {
            Ok(()) => consecutive_failures = 0,
            Err(e) if unit_scoped_failure(&e) => {
                consecutive_failures += 1;
                if consecutive_failures >= MAX_CONSECUTIVE_UNIT_FAILURES {
                    return Err(error(
                        "COMPILE_UNIT_FAILURES",
                        format!("连续 {consecutive_failures} 个片段失败，停止新批次：{e}"),
                    ));
                }
            }
            Err(e) => return Err(e),
        }
    }
    let summary = store.summary(run_id)?;
    store.set_status(
        run_id,
        if summary.units_failed > 0 || summary.unresolved_pages > 0 || summary.pending_ocr_pages > 0
        {
            "needs_review"
        } else {
            "completed"
        },
        "finished",
    )
}

fn finish(
    project: &Path,
    run_id: &str,
    store: &Store,
    config: &CompileConfig,
    budget: &mut BudgetAuthority,
    unit: &Unit,
    attempt: &Attempt,
    credential: &str,
) -> Result<(), ProjectError> {
    let directory = store.root.join(&attempt.directory);
    let receipt_bytes = read_artifact(&directory.join("receipt.json"), 65536)?;
    let receipt: provider::Receipt = serde_json::from_slice(&receipt_bytes)
        .map_err(|_| error("COMPILE_RECEIPT", "回执不可读。"))?;
    let response = read_artifact(&directory.join("response.bin"), 1024 * 1024)?;
    let body = read_artifact(&directory.join("request-body.json"), 256 * 1024)?;
    if receipt.schema_version != 1
        || receipt.network_attempts > 1
        || receipt.status.is_some_and(|s| !(100..=599).contains(&s))
        || sha256(&body) != attempt.request_sha256
        || receipt.request_sha256 != attempt.request_sha256
        || receipt.request_id != attempt.request_id
        || receipt.response_sha256 != sha256(&response)
        || receipt.response_bytes != response.len() as u64
        || attempt
            .response_sha256
            .as_ref()
            .is_some_and(|s| s != &receipt.response_sha256)
        || attempt
            .receipt_sha256
            .as_ref()
            .is_some_and(|s| s != &sha256(&receipt_bytes))
        || !credential.is_empty() && crate::extraction_api::credential_echo(&response, credential)
    {
        return Err(error("COMPILE_RECEIPT", "请求/响应/回执原件不一致。"));
    }
    // A crash can occur after both files are durable but before these pins were
    // recorded. Seal the verified originals before committing their knowledge.
    let mut sealed = attempt.clone();
    sealed.state = "response_saved".into();
    sealed.response_sha256 = Some(receipt.response_sha256.clone());
    sealed.receipt_sha256 = Some(sha256(&receipt_bytes));
    store.update_attempt(run_id, &sealed)?;
    budget.settle(
        &key(run_id, attempt),
        &BudgetSettlement {
            network_attempts: receipt.network_attempts,
            usage: if receipt.complete {
                provider::usage(&response)
            } else {
                None
            },
            receipt_sha256: sha256(&receipt_bytes),
        },
    )?;
    if !receipt.complete || receipt.failure_code.is_some() || receipt.status != Some(200) {
        store.mark_unit_failed(
            run_id,
            &unit.unit_id,
            receipt.failure_code.as_deref().unwrap_or("COMPILE_HTTP"),
            "请求未完整成功；原件与实际/未知用量保留。",
        )?;
        if receipt.failure_code.as_deref() == Some("COMPILE_PAUSED") {
            return Err(error(
                "COMPILE_PAUSED",
                "在途请求已停止并保留回执与未知预留。",
            ));
        }
        return Err(error(
            "COMPILE_REQUEST_FAILED",
            format!(
                "请求失败（HTTP {:?}，{}）；已停止新批次，实际/未知用量保留。",
                receipt.status,
                receipt.failure_code.as_deref().unwrap_or("COMPILE_HTTP")
            ),
        ));
    }
    let payload: Value =
        serde_json::from_slice(&body).map_err(|_| error("COMPILE_REQUEST", "请求原件不可读。"))?;
    let input: Value = serde_json::from_str(
        payload["messages"][1]["content"]
            .as_str()
            .ok_or_else(|| error("COMPILE_REQUEST", "请求上下文缺失。"))?,
    )
    .map_err(|_| error("COMPILE_REQUEST", "请求上下文不可读。"))?;
    let context: Vec<ConceptContext> =
        serde_json::from_value(input["request"]["existing_concepts"].clone())
            .map_err(|_| error("COMPILE_REQUEST", "原语义上下文不可读。"))?;
    let prepared = provider::prepare(
        store.project_id(),
        run_id,
        &attempt.request_id,
        unit,
        config,
        &context,
    )?;
    if prepared.body != body {
        return Err(error(
            "COMPILE_REQUEST_CHANGED",
            "原策略或上下文发生变化，不重写已发送请求。",
        ));
    }
    corpus::page_revision(
        project,
        &unit.source_id,
        unit.page_number,
        &unit.page_sha256,
    )?;
    match provider::decode(&response, &prepared, &attempt.request_id) {
        Ok(candidates) if candidates.atoms.is_empty() => store.mark_unit_failed(
            run_id,
            &unit.unit_id,
            "COMPILE_NO_ATOMS",
            "本片段无可确认候选；保留未解决范围，不假报知识覆盖。",
        ),
        Ok(candidates) => store.commit_batch(
            run_id,
            unit,
            &sealed,
            AcceptedBatch {
                candidates,
                allowed_existing_concepts: context.into_iter().map(|c| c.concept).collect(),
            },
        ),
        Err(e) => {
            store.mark_unit_failed(run_id, &unit.unit_id, e.code(), &e.to_string())?;
            Err(error(
                "COMPILE_CANDIDATE_FAILED",
                format!("候选协议校验失败；停止新批次并保留原件：{e}"),
            ))
        }
    }
}
