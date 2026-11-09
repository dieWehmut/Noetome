//! `vault plan|approve|publish|rollback`：批准绑定、三段式发布与可续完的回滚。
//!
//! 三份字节必须同时在场才谈得上计划：`baseline/`（adopt 冻结）、`vault/`（当前磁盘，
//! 人可以手工改）与候选目录。计划把这三份字节算成逐项条目与分类影响；批准把「计划正文、
//! 受影响路径的现场指纹、完整影响清单、所有权与账本摘要」四样绑在一起；发布先分级暂存
//! 再逐项落盘，中途真的被终止时账本停在 `pending`，`status` 报 `needs_recovery`，
//! 重跑同一份批准可以续完。这里不实现任何「先写后校验、失败再修」的路径：核对不过就
//! 保留现场并拒绝，未批准覆盖恒为 0。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::project::{self, ProjectError};
use crate::text::sha256;
use crate::vault_model::{
    Analysis, Author, CANDIDATE_MISSING, Candidate, DELETE_NEEDS_FLAG, Entry, EntryAction,
    IMPACT_ATTACHMENT, IMPACT_BODY, IMPACT_LINK, Impact, MAX_FILE_BYTES, MAX_PLAN_BYTES, Operation,
    PATH_OUTSIDE, POLICY, ROLLBACK_CONFLICT, ResolvedLink, analyze, canonical_impacts,
    link_targets, resolve_link,
};
use crate::vault_store::{
    BASELINE, RELEASE_CANDIDATE, RELEASE_LOG, RELEASE_RECORD, SCHEMA_VERSION, StampSlot, VAULT,
    VaultStore, append_bytes, argument, canonical, create_parent_dirs, error, integrity, joined,
    now, read_bounded, refuse, resolve_caller_path, strict, tree, write_new_file, write_replace,
};

const KIND_UPDATE: &str = "update";
const KIND_ROLLBACK: &str = "rollback";
const SOURCE_CANDIDATE: &str = "candidate";
const SOURCE_BASELINE: &str = "baseline";
const STATE_PENDING: &str = "pending";
const STATE_WRITTEN: &str = "written";
const STATE_DELETED: &str = "deleted";
const STATUS_PENDING: &str = "pending";
const STATUS_PUBLISHED: &str = "published";
const OPERATIONS_FILE: &str = "operations.json";

// ---------------------------------------------------------------------------
// 记录
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFile {
    path: String,
    sha256: String,
    byte_length: u64,
}

/// 发布时要读的字节从哪来。计划只记摘要与位置，不复制字节，发布时逐项复核。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceSpec {
    kind: String,
    root: String,
    files: Vec<SourceFile>,
}

/// 账本里的计划正文；它的 SHA256 就是 `plan_id`，也是批准唯一指向的东西。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanPayload {
    policy: String,
    schema_version: u32,
    kind: String,
    case_id: String,
    summary: String,
    operations: Vec<Operation>,
    impacts: Vec<Impact>,
    impacts_sha256: String,
    entries: Vec<Entry>,
    stamps: Vec<StampSlot>,
    final_paths: Vec<String>,
    final_links: Vec<ResolvedLink>,
    source: SourceSpec,
    owner_sha256: String,
    metadata_sha256: String,
    author_manifest_sha256: String,
    baseline_manifest_sha256: String,
    created_at: String,
}

/// 人看的那份计划文件；`approve` 读它，并逐项与账本正文对照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanView {
    policy: String,
    schema_version: u32,
    plan_id: String,
    plan_sha256: String,
    kind: String,
    case_id: String,
    summary: String,
    operations: Vec<Operation>,
    impacts: Vec<Impact>,
    impacts_sha256: String,
    entries: Vec<Entry>,
    stamps: Vec<StampSlot>,
    final_paths: Vec<String>,
    source: SourceSpec,
    owner_sha256: String,
    metadata_sha256: String,
    created_at: String,
    affected_paths: Vec<String>,
}

/// 批准：计划正文摘要 + 四样绑定 + 显式删除清单 + 「是否已写过」标记。
/// 批准文件自身的 SHA256 由 `publish` 现算，不写进文件（避免自指摘要）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApprovalRecord {
    policy: String,
    schema_version: u32,
    plan_id: String,
    plan_sha256: String,
    kind: String,
    case_id: String,
    summary: String,
    operations: Vec<Operation>,
    impacts: Vec<Impact>,
    impacts_sha256: String,
    entries: Vec<Entry>,
    stamps: Vec<StampSlot>,
    source: SourceSpec,
    owner_sha256: String,
    metadata_sha256: String,
    approved_deletions: Vec<String>,
    vault_written: bool,
    created_at: String,
}

/// 一次发布留在 `releases/<id>/release.json` 里的自证材料。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseRecord {
    policy: String,
    schema_version: u32,
    release_id: String,
    approval_sha256: String,
    plan_id: String,
    plan_sha256: String,
    kind: String,
    case_id: String,
    entries: Vec<Entry>,
    impacts: Vec<Impact>,
    source: SourceSpec,
    created_at: String,
}

/// 逐项落盘状态；每落一项就整份写回账本，中断后能说清停在哪。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseEntryState {
    action: EntryAction,
    path: String,
    sha256: String,
    byte_length: u64,
    state: String,
}

// ---------------------------------------------------------------------------
// plan
// ---------------------------------------------------------------------------

/// `vault plan <root> <候选目录> <计划文件>`：只读 `vault/`，把三份字节算成影响与条目。
pub fn plan(root: &Path, candidate: &Path, plan_out: &Path) -> Result<String, ProjectError> {
    let candidate_root = resolve_caller_path(candidate)?;
    let plan_path = resolve_caller_path(plan_out)?;
    require_new_file(&plan_path)?;
    if plan_path == candidate_root {
        return Err(argument("计划文件不能就是候选目录本身；未写出任何字节。"));
    }
    let store = VaultStore::open(root)?;
    store.require_baseline()?;
    require_dir(&candidate_root)?;
    let operations_path = candidate_root.join(OPERATIONS_FILE);
    if !project::exists(&operations_path)? {
        return Err(ProjectError::new(
            CANDIDATE_MISSING,
            3,
            format!(
                "候选目录里没有 {OPERATIONS_FILE}：{}；未写出任何字节。",
                candidate_root.display()
            ),
        ));
    }
    project::checked_file(&operations_path)?;
    let operations_bytes = read_bounded(&operations_path, MAX_PLAN_BYTES)?;
    let files = candidate_files(&candidate_root)?;
    let candidate_record = Candidate::parse(&operations_bytes, files).map_err(refuse)?;
    let current = store.current_bytes()?;
    let baseline = store.baseline_bytes()?;
    let analysis =
        analyze(&store.author, &candidate_record, &current, &baseline).map_err(refuse)?;
    let affected = affected_paths(&analysis);
    let stamps = store.stamps(&affected)?;
    let impacts_sha256 = sha256(canonical_impacts(&analysis.impacts).as_bytes());
    let created_at = now()?;
    let final_paths: Vec<String> = analysis.final_paths.iter().cloned().collect();
    let payload = PlanPayload {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        kind: KIND_UPDATE.to_owned(),
        case_id: candidate_record.case_id.clone(),
        summary: candidate_record.summary.clone(),
        operations: candidate_record.operations.clone(),
        impacts: analysis.impacts.clone(),
        impacts_sha256,
        entries: analysis.entries.clone(),
        stamps: stamps.clone(),
        final_paths: final_paths.clone(),
        final_links: analysis.final_links.clone(),
        source: SourceSpec {
            kind: SOURCE_CANDIDATE.to_owned(),
            root: absolute(&candidate_root)?,
            files: source_files(&analysis),
        },
        owner_sha256: store.owner_sha256(),
        metadata_sha256: store.metadata_sha256(),
        author_manifest_sha256: store.owner_file().author_manifest_sha256.clone(),
        baseline_manifest_sha256: store.owner_file().baseline_manifest_sha256.clone(),
        created_at: created_at.clone(),
    };
    let payload_json = canonical(&payload)?;
    let plan_sha256 = sha256(payload_json.as_bytes());
    let view = PlanView {
        policy: payload.policy.clone(),
        schema_version: payload.schema_version,
        plan_id: plan_sha256.clone(),
        plan_sha256: plan_sha256.clone(),
        kind: payload.kind.clone(),
        case_id: payload.case_id.clone(),
        summary: payload.summary.clone(),
        operations: payload.operations.clone(),
        impacts: payload.impacts.clone(),
        impacts_sha256: payload.impacts_sha256.clone(),
        entries: payload.entries.clone(),
        stamps: stamps.clone(),
        final_paths,
        source: payload.source.clone(),
        owner_sha256: payload.owner_sha256.clone(),
        metadata_sha256: payload.metadata_sha256.clone(),
        created_at,
        affected_paths: affected.iter().cloned().collect(),
    };
    let text = canonical(&view)?;
    let _lock = store.lock()?;
    store.insert_plan(KIND_UPDATE, &payload_json, &payload.created_at)?;
    write_new_file(&plan_path, text.as_bytes())?;
    let writes = analysis
        .entries
        .iter()
        .filter(|entry| entry.action == EntryAction::Write)
        .count();
    Ok(format!(
        "计划已写出：plan_id={plan_sha256}（{KIND_UPDATE}）\n候选：{}（{} 项操作）\n影响：{} 项（{}）\n条目：{} 条（写入 {writes}、删除 {}）\n未批准覆盖：0\n",
        payload.case_id,
        payload.operations.len(),
        payload.impacts.len(),
        categories(&payload.impacts),
        payload.entries.len(),
        payload.entries.len() - writes,
    ))
}

// ---------------------------------------------------------------------------
// approve
// ---------------------------------------------------------------------------

/// `vault approve <root> <计划文件> <批准文件>`：把计划正文与现场字节绑在一起，
/// 不写 `vault/` 一个字节。
pub fn approve(root: &Path, plan: &Path, approval_out: &Path) -> Result<String, ProjectError> {
    let plan_path = resolve_caller_path(plan)?;
    let approval_path = resolve_caller_path(approval_out)?;
    require_new_file(&approval_path)?;
    if approval_path == plan_path {
        return Err(argument("批准文件不能就是计划文件本身；未写出任何字节。"));
    }
    let store = VaultStore::open(root)?;
    store.require_baseline()?;
    project::checked_file(&plan_path)?;
    let plan_bytes = read_bounded(&plan_path, MAX_PLAN_BYTES)?;
    let view: PlanView = strict(&plan_bytes)?;
    if view.policy != POLICY || view.schema_version != SCHEMA_VERSION {
        return Err(integrity(format!(
            "计划文件的 policy 或结构版本不受支持（{} / {}）；未写出批准。",
            view.policy, view.schema_version
        )));
    }
    if view.plan_id != view.plan_sha256 {
        return Err(integrity(
            "计划文件里的 plan_id 与 plan_sha256 不一致；未写出批准。",
        ));
    }
    let row = store.plan_record(&view.plan_sha256)?.ok_or_else(|| {
        error(
            "VAULT_PLAN_UNKNOWN",
            format!(
                "计划 {} 不在托管账本里；未写出批准，请重新 plan。",
                view.plan_sha256
            ),
        )
    })?;
    if row.kind != view.kind || row.plan_id != view.plan_id {
        return Err(integrity("账本里的计划记录与计划文件不一致；未写出批准。"));
    }
    let payload: PlanPayload = strict(row.payload_json.as_bytes())?;
    if payload.kind != view.kind
        || payload.case_id != view.case_id
        || payload.summary != view.summary
        || payload.operations != view.operations
        || payload.impacts != view.impacts
        || payload.impacts_sha256 != view.impacts_sha256
        || payload.entries != view.entries
        || payload.stamps != view.stamps
        || payload.final_paths != view.final_paths
        || payload.source != view.source
        || payload.owner_sha256 != view.owner_sha256
        || payload.metadata_sha256 != view.metadata_sha256
        || payload.created_at != view.created_at
    {
        return Err(integrity(
            "计划文件与账本登记的计划正文不一致；未写出批准。",
        ));
    }
    if sha256(canonical_impacts(&payload.impacts).as_bytes()) != payload.impacts_sha256 {
        return Err(integrity(
            "计划里的影响清单摘要与影响清单本身不一致；未写出批准。",
        ));
    }
    let _lock = store.lock()?;
    if payload.owner_sha256 != store.owner_sha256() {
        return Err(stale("所有权记录 owner.json 在 plan 之后发生了变化"));
    }
    if payload.metadata_sha256 != store.metadata_sha256() {
        return Err(stale("账本元数据在 plan 之后发生了变化"));
    }
    let mut bound = BTreeSet::new();
    for slot in &payload.stamps {
        bound.insert(slot.path.clone());
    }
    let actual = store.stamps(&bound)?;
    let drifted = stamp_drift(&payload.stamps, &actual);
    if !drifted.is_empty() {
        return Err(stale(format!(
            "托管现场在 plan 之后发生变化：{}",
            drifted.join("、")
        )));
    }
    let mut approved_deletions: Vec<String> = payload
        .entries
        .iter()
        .filter(|entry| entry.action == EntryAction::Delete)
        .map(|entry| entry.path.clone())
        .collect();
    approved_deletions.sort();
    approved_deletions.dedup();
    let record = ApprovalRecord {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        plan_id: view.plan_sha256.clone(),
        plan_sha256: view.plan_sha256.clone(),
        kind: payload.kind.clone(),
        case_id: payload.case_id.clone(),
        summary: payload.summary.clone(),
        operations: payload.operations.clone(),
        impacts: payload.impacts.clone(),
        impacts_sha256: payload.impacts_sha256.clone(),
        entries: payload.entries.clone(),
        stamps: payload.stamps.clone(),
        source: payload.source.clone(),
        owner_sha256: payload.owner_sha256.clone(),
        metadata_sha256: payload.metadata_sha256.clone(),
        approved_deletions,
        vault_written: false,
        created_at: now()?,
    };
    let text = canonical(&record)?;
    let digest = sha256(text.as_bytes());
    drop(_lock);
    write_new_file(&approval_path, text.as_bytes())?;
    Ok(format!(
        "批准已写出：approval={digest}（绑定计划 {}）\n绑定：计划正文 SHA256、{} 个受影响路径的现场指纹、{} 项影响清单、所有权与账本摘要\n说明：approve 未写入 vault/；publish 会在写入任何字节前再次核对，任何差异都会拒绝。\n未批准覆盖：0\n",
        record.plan_id,
        record.stamps.len(),
        record.impacts.len(),
    ))
}

// ---------------------------------------------------------------------------
// publish
// ---------------------------------------------------------------------------

/// `vault publish <root> <批准文件>`：分级暂存、复核、逐项落盘；中断可续。
pub fn publish(root: &Path, approval: &Path) -> Result<String, ProjectError> {
    let approval_path = resolve_caller_path(approval)?;
    let store = VaultStore::open(root)?;
    store.require_baseline()?;
    project::checked_file(&approval_path)?;
    let approval_bytes = read_bounded(&approval_path, MAX_PLAN_BYTES)?;
    let approval_sha256 = sha256(&approval_bytes);
    let record: ApprovalRecord = strict(&approval_bytes)?;
    if record.policy != POLICY || record.schema_version != SCHEMA_VERSION {
        return Err(integrity(format!(
            "批准文件的 policy 或结构版本不受支持（{} / {}）；vault 未写入任何字节。",
            record.policy, record.schema_version
        )));
    }
    if record.vault_written {
        return Err(integrity(
            "批准文件标记为已写入过；同一份批准不能重复使用，请重新 plan 与 approve。",
        ));
    }
    if record.plan_id != record.plan_sha256 || record.plan_id.len() != 64 {
        return Err(integrity(
            "批准文件里的 plan_id 与 plan_sha256 不一致；vault 未写入任何字节。",
        ));
    }
    if record.kind != KIND_UPDATE && record.kind != KIND_ROLLBACK {
        return Err(integrity(format!(
            "批准文件的计划类型 {} 未知；vault 未写入任何字节。",
            record.kind
        )));
    }
    let expected_source = if record.kind == KIND_ROLLBACK {
        SOURCE_BASELINE
    } else {
        SOURCE_CANDIDATE
    };
    if record.source.kind != expected_source {
        return Err(integrity(format!(
            "{} 类型的批准只能从 {expected_source} 取字节，实际是 {}；vault 未写入任何字节。",
            record.kind, record.source.kind
        )));
    }
    if sha256(canonical_impacts(&record.impacts).as_bytes()) != record.impacts_sha256 {
        return Err(integrity(
            "批准里的影响清单摘要与影响清单本身不一致；vault 未写入任何字节。",
        ));
    }
    for entry in record
        .entries
        .iter()
        .filter(|entry| entry.action == EntryAction::Delete)
    {
        if !record
            .approved_deletions
            .iter()
            .any(|path| path == &entry.path)
        {
            return Err(ProjectError::new(
                DELETE_NEEDS_FLAG,
                3,
                format!(
                    "批准文件没有显式批准删除 {}；vault 未写入任何字节。",
                    entry.path
                ),
            ));
        }
    }
    let row = store.plan_record(&record.plan_sha256)?.ok_or_else(|| {
        error(
            "VAULT_PLAN_UNKNOWN",
            format!(
                "批准所指向的计划 {} 不在托管账本里；vault 未写入任何字节。",
                record.plan_sha256
            ),
        )
    })?;
    if row.kind != record.kind || row.plan_id != record.plan_id {
        return Err(integrity(
            "账本里的计划记录与批准不一致；vault 未写入任何字节。",
        ));
    }
    let payload: PlanPayload = strict(row.payload_json.as_bytes())?;
    if payload.kind != record.kind
        || payload.case_id != record.case_id
        || payload.summary != record.summary
        || payload.operations != record.operations
        || payload.impacts != record.impacts
        || payload.impacts_sha256 != record.impacts_sha256
        || payload.entries != record.entries
        || payload.stamps != record.stamps
        || payload.source != record.source
        || payload.owner_sha256 != record.owner_sha256
        || payload.metadata_sha256 != record.metadata_sha256
    {
        return Err(integrity(
            "计划账本正文与批准不一致；vault 未写入任何字节。",
        ));
    }

    if record.owner_sha256 != store.owner_sha256() {
        return Err(stale("所有权记录 owner.json 与批准绑定的指纹不符"));
    }
    if record.metadata_sha256 != store.metadata_sha256() {
        return Err(stale("账本元数据与批准绑定的指纹不符"));
    }

    let _lock = store.lock()?;

    // 已经发布过的批准是幂等报告，而不是再写一次；这一步必须在现场指纹核对之前，
    // 因为发布本身就会改变被绑定的字节。
    let mut release_row = store.release_record(&approval_sha256)?;
    if let Some(current) = &release_row {
        if current.status == STATUS_PUBLISHED {
            return Ok(format!(
                "已发布过：release_id={} status={STATUS_PUBLISHED}（第 {} 个版本，{}；本次未写入任何字节，逐项条目 {} 条）\n未批准覆盖：0\n",
                current.release_id,
                current.sequence,
                current.at,
                record.entries.len(),
            ));
        }
        if current.status != STATUS_PENDING {
            return Err(integrity(format!(
                "发布账本里 {} 的状态 {} 未知；vault 未写入任何字节。",
                approval_sha256, current.status
            )));
        }
    }
    let mut states: Vec<ReleaseEntryState> = match &release_row {
        Some(current) => {
            let stored: Vec<ReleaseEntryState> = strict(current.entries_json.as_bytes())?;
            if !same_states(&stored, &record.entries) {
                return Err(integrity(format!(
                    "发布账本里 {} 的逐项记录与本次批准不一致；vault 未写入任何字节。",
                    approval_sha256
                )));
            }
            stored
        }
        None => states_from(&record.entries),
    };

    let mut current = store.current_bytes()?;
    for entry in &record.entries {
        if current.contains_key(&entry.path) {
            continue;
        }
        let path = store.vault_path(&entry.path);
        if project::exists(&path)? {
            current.insert(entry.path.clone(), read_bounded(&path, MAX_FILE_BYTES)?);
        }
    }
    let mut pending_paths = BTreeSet::new();
    for (state, entry) in states.iter().zip(record.entries.iter()) {
        match entry.action {
            EntryAction::Write => match state.state.as_str() {
                STATE_WRITTEN => {
                    let landed = current.get(&entry.path);
                    if landed.map(|bytes| sha256(bytes)).as_deref() != Some(entry.sha256.as_str())
                        || landed.map(|bytes| bytes.len()).unwrap_or(0) as u64 != entry.byte_length
                    {
                        return Err(error(
                            "VAULT_COMMIT_MISMATCH",
                            format!(
                                "已确认条目 {} 的现场字节与候选不一致（可能是发布之外的人工改写）；保留现场，请重新 plan。",
                                entry.path
                            ),
                        ));
                    }
                }
                STATE_PENDING => {
                    pending_paths.insert(entry.path.clone());
                }
                other => {
                    return Err(integrity(format!(
                        "条目 {} 的状态 {other} 未知；vault 未写入任何字节。",
                        entry.path
                    )));
                }
            },
            EntryAction::Delete => match state.state.as_str() {
                STATE_DELETED => {
                    if current.contains_key(&entry.path) {
                        return Err(error(
                            "VAULT_COMMIT_MISMATCH",
                            format!(
                                "已确认删除的 {} 又出现在 vault/ 里；保留现场，请重新 plan。",
                                entry.path
                            ),
                        ));
                    }
                }
                STATE_PENDING => {
                    pending_paths.insert(entry.path.clone());
                }
                other => {
                    return Err(integrity(format!(
                        "条目 {} 的状态 {other} 未知；vault 未写入任何字节。",
                        entry.path
                    )));
                }
            },
        }
    }

    // 现场指纹只核对「接下来真的要写」的路径：已确认的条目字节本来就该变过。
    if !pending_paths.is_empty() {
        for path in &pending_paths {
            if !record.stamps.iter().any(|slot| &slot.path == path) {
                return Err(integrity(format!(
                    "批准没有绑定待写路径 {path} 的现场指纹；vault 未写入任何字节。"
                )));
            }
        }
        let actual = store.stamps(&pending_paths)?;
        let bound: Vec<StampSlot> = record
            .stamps
            .iter()
            .filter(|slot| pending_paths.contains(&slot.path))
            .cloned()
            .collect();
        let drifted = stamp_drift(&bound, &actual);
        if !drifted.is_empty() {
            return Err(stale(format!(
                "托管现场在 approve 之后发生变化：{}",
                drifted.join("、")
            )));
        }
    }

    // 第一段：把字节读齐并逐项复核摘要，然后分级暂存到 releases/<id>/candidate/。
    let write_entries: Vec<&Entry> = record
        .entries
        .iter()
        .filter(|entry| entry.action == EntryAction::Write)
        .collect();
    let mut staged: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    match record.source.kind.as_str() {
        SOURCE_CANDIDATE => {
            let source_root = PathBuf::from(&record.source.root);
            if !source_root.is_absolute() {
                return Err(argument(
                    "批准里的候选来源不是绝对路径；vault 未写入任何字节。",
                ));
            }
            for entry in &write_entries {
                let source_path = joined(&source_root, &entry.path);
                if !project::exists(&source_path)? {
                    return Err(ProjectError::new(
                        CANDIDATE_MISSING,
                        3,
                        format!(
                            "候选源文件 {} 已缺失；vault 未写入任何字节，请重新 plan 与 approve。",
                            entry.path
                        ),
                    ));
                }
                project::checked_file(&source_path)?;
                let bytes = read_bounded(&source_path, MAX_FILE_BYTES)?;
                if sha256(&bytes) != entry.sha256 || bytes.len() as u64 != entry.byte_length {
                    return Err(error(
                        "VAULT_SOURCE_CHANGED",
                        format!(
                            "候选源文件 {} 在 plan 之后被改动；vault 未写入任何字节，请重新 plan 与 approve。",
                            entry.path
                        ),
                    ));
                }
                staged.insert(entry.path.clone(), bytes);
            }
        }
        SOURCE_BASELINE => {
            let baseline = store.baseline_bytes()?;
            for entry in &write_entries {
                let bytes = baseline.get(&entry.path).cloned().ok_or_else(|| {
                    error(
                        "VAULT_NO_BASELINE",
                        format!(
                            "基准里没有 {}，无法回滚该路径；vault 未写入任何字节。",
                            entry.path
                        ),
                    )
                })?;
                if sha256(&bytes) != entry.sha256 {
                    return Err(error(
                        "VAULT_BASELINE_MISMATCH",
                        format!(
                            "基准里 {} 的字节与批准不一致；vault 未写入任何字节。",
                            entry.path
                        ),
                    ));
                }
                staged.insert(entry.path.clone(), bytes);
            }
        }
        other => {
            return Err(integrity(format!(
                "批准里的来源类型 {other} 未知；vault 未写入任何字节。"
            )));
        }
    }

    let release_dir = store.release_dir(&approval_sha256);
    let candidate_dir = release_dir.join(RELEASE_CANDIDATE);
    let record_path = release_dir.join(RELEASE_RECORD);
    let created_at = if project::exists(&record_path)? {
        project::checked_file(&record_path)?;
        let existing: ReleaseRecord = strict(&read_bounded(&record_path, MAX_PLAN_BYTES)?)?;
        if existing.release_id != approval_sha256
            || existing.approval_sha256 != approval_sha256
            || existing.plan_sha256 != record.plan_sha256
            || existing.entries != record.entries
        {
            return Err(integrity(format!(
                "releases/{approval_sha256}/{RELEASE_RECORD} 与本次批准不一致；vault 未写入任何字节。"
            )));
        }
        existing.created_at
    } else {
        let at = now()?;
        let release_record = ReleaseRecord {
            policy: POLICY.to_owned(),
            schema_version: SCHEMA_VERSION,
            release_id: approval_sha256.clone(),
            approval_sha256: approval_sha256.clone(),
            plan_id: record.plan_id.clone(),
            plan_sha256: record.plan_sha256.clone(),
            kind: record.kind.clone(),
            case_id: record.case_id.clone(),
            entries: record.entries.clone(),
            impacts: record.impacts.clone(),
            source: record.source.clone(),
            created_at: at.clone(),
        };
        write_new_file(&record_path, canonical(&release_record)?.as_bytes())?;
        at
    };
    for (path, bytes) in &staged {
        let target = joined(&candidate_dir, path);
        if project::exists(&target)? {
            project::checked_file(&target)?;
            if &read_bounded(&target, MAX_FILE_BYTES)? == bytes {
                continue;
            }
            fs::remove_file(&target).map_err(|failure| ProjectError::io(&target, failure))?;
        }
        write_new_file(&target, bytes)?;
    }

    // 第二段：从暂存区读回并重建终局，逐项核对内容、附件与链接索引。
    let mut restaged: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for entry in &write_entries {
        let target = joined(&candidate_dir, &entry.path);
        project::checked_file(&target)?;
        let bytes = read_bounded(&target, MAX_FILE_BYTES)?;
        if sha256(&bytes) != entry.sha256 || bytes.len() as u64 != entry.byte_length {
            return Err(error(
                "VAULT_STAGE_MISMATCH",
                format!(
                    "暂存的 {} 读回后与候选字节不一致；vault 未写入任何字节。",
                    entry.path
                ),
            ));
        }
        if is_png(&store.author, &entry.path) {
            validate_png(&bytes).map_err(|reason| {
                error(
                    "VAULT_STAGE_MISMATCH",
                    format!(
                        "暂存的附件 {} 不能按 PNG 结构解析（{reason}）；vault 未写入任何字节。",
                        entry.path
                    ),
                )
            })?;
        }
        restaged.insert(entry.path.clone(), bytes);
    }
    let managed = store.author.managed();
    let mut introduced = BTreeSet::new();
    for operation in &payload.operations {
        if let Operation::Rename { to, .. } = operation {
            if !managed.contains(to) {
                introduced.insert(to.clone());
            }
        }
    }
    for entry in &record.entries {
        if !managed.contains(&entry.path) && !introduced.contains(&entry.path) {
            return Err(ProjectError::new(
                PATH_OUTSIDE,
                3,
                format!(
                    "批准里出现托管集合之外的路径 {}；vault 未写入任何字节。",
                    entry.path
                ),
            ));
        }
    }
    let landing = compute_final_state(&store.author, &current, &record.entries, &restaged)?;
    let final_paths: BTreeSet<String> = landing
        .iter()
        .filter(|(_, bytes)| bytes.is_some())
        .map(|(path, _)| path.clone())
        .collect();
    let planned: BTreeSet<String> = payload.final_paths.iter().cloned().collect();
    if final_paths != planned {
        return Err(error(
            "VAULT_STAGE_MISMATCH",
            format!(
                "暂存后重建的托管路径集合与计划不一致（多 {} 个、少 {} 个）；vault 未写入任何字节。",
                final_paths.difference(&planned).count(),
                planned.difference(&final_paths).count()
            ),
        ));
    }
    let links = collect_links(&store.author, &landing, &final_paths);
    if normalized_links(&links) != normalized_links(&payload.final_links) {
        return Err(error(
            "VAULT_STAGE_MISMATCH",
            "暂存后重建的链接解析与计划不一致；vault 未写入任何字节，请重新 plan。",
        ));
    }
    for impact in &record.impacts {
        if !landing.contains_key(&impact.path)
            && !record.entries.iter().any(|entry| entry.path == impact.path)
        {
            return Err(error(
                "VAULT_STAGE_MISMATCH",
                format!(
                    "影响清单里的 {} 既不在逐项条目也不在终局托管集合中；vault 未写入任何字节。",
                    impact.path
                ),
            ));
        }
    }

    // 第三段：先登记 pending，再逐项落盘；每落一项就把状态写回账本。
    let entries_json = canonical(&states)?;
    let sequence = match &release_row {
        Some(current) => current.sequence,
        None => store.begin_release(
            &approval_sha256,
            &approval_sha256,
            &entries_json,
            &created_at,
        )?,
    };
    release_row = store.release_record(&approval_sha256)?;
    let mut confirmed = states
        .iter()
        .filter(|state| state.state != STATE_PENDING)
        .count();
    pause_after_pending();
    for (index, entry) in record.entries.iter().enumerate() {
        if states[index].state != STATE_PENDING {
            continue;
        }
        match entry.action {
            EntryAction::Write => {
                let Some(bytes) = restaged.get(&entry.path) else {
                    return Err(integrity(format!(
                        "条目 {} 缺少已暂存的字节；保留现场并拒绝继续。",
                        entry.path
                    )));
                };
                write_vault_file(&store, &entry.path, bytes)?;
                let landed = read_bounded(&store.vault_path(&entry.path), MAX_FILE_BYTES)?;
                if sha256(&landed) != entry.sha256 || landed.len() as u64 != entry.byte_length {
                    return Err(error(
                        "VAULT_COMMIT_MISMATCH",
                        format!(
                            "落盘后 {} 的读回字节与候选不一致；保留现场，status 会报 needs_recovery。",
                            entry.path
                        ),
                    ));
                }
                states[index].state = STATE_WRITTEN.to_owned();
            }
            EntryAction::Delete => {
                let target = store.vault_path(&entry.path);
                if project::exists(&target)? {
                    project::checked_file(&target)?;
                    fs::remove_file(&target)
                        .map_err(|failure| ProjectError::io(&target, failure))?;
                }
                states[index].state = STATE_DELETED.to_owned();
            }
        }
        store.set_release_entries(&approval_sha256, &canonical(&states)?)?;
        append_log(&release_dir, sequence, index, entry, &states[index].state)?;
        confirmed += 1;
        crash_after_entries(confirmed);
    }
    store.finish_release(&approval_sha256, STATUS_PUBLISHED)?;
    let at = release_row
        .as_ref()
        .map(|row| row.at.clone())
        .unwrap_or_else(|| created_at.clone());
    Ok(format!(
        "已发布：release_id={approval_sha256} status={STATUS_PUBLISHED}（第 {sequence} 个版本，逐项条目 {} 条，{at}）\n计划：{}\n影响：{} 项（{}）\n说明：vault/ 里被本次写到的路径现在逐字节等于候选；未触碰的无关内容与 .obsidian 配置保持原样。\n未批准覆盖：0\n",
        record.entries.len(),
        record.plan_id,
        record.impacts.len(),
        categories(&record.impacts),
    ))
}

// ---------------------------------------------------------------------------
// rollback
// ---------------------------------------------------------------------------

/// `vault rollback <root> <回滚计划文件>`：只写一份反向计划，不动 `vault/`。
/// 发布之后被人工改过的路径不会被这份计划覆盖，而是直接报 `rollback-conflict`。
pub fn rollback(root: &Path, rollback_plan_out: &Path) -> Result<String, ProjectError> {
    let plan_path = resolve_caller_path(rollback_plan_out)?;
    require_new_file(&plan_path)?;
    let store = VaultStore::open(root)?;
    store.require_baseline()?;
    let _lock = store.lock()?;
    let latest = store
        .releases()?
        .into_iter()
        .filter(|row| row.status == STATUS_PUBLISHED)
        .next_back()
        .ok_or_else(|| {
            error(
                "VAULT_NO_RELEASE",
                "还没有已发布版本，无法生成回滚计划；未写出任何字节。",
            )
        })?;
    let release_dir = store.release_dir(&latest.release_id);
    let record_path = release_dir.join(RELEASE_RECORD);
    if !project::exists(&record_path)? {
        return Err(integrity(format!(
            "releases/{}/{RELEASE_RECORD} 缺失，无法确定该版本写过什么；未写出任何字节。",
            latest.release_id
        )));
    }
    project::checked_file(&record_path)?;
    let release: ReleaseRecord = strict(&read_bounded(&record_path, MAX_PLAN_BYTES)?)?;
    let stored: Vec<ReleaseEntryState> = strict(latest.entries_json.as_bytes())?;
    if release.release_id != latest.release_id
        || !same_states(&stored, &release.entries)
        || release.approval_sha256 != latest.release_id
    {
        return Err(integrity(format!(
            "releases/{} 的自证材料与账本不一致；未写出任何字节。",
            latest.release_id
        )));
    }
    let current = store.current_bytes()?;
    let baseline = store.baseline_bytes()?;
    let mut want: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut conflicts: BTreeSet<String> = BTreeSet::new();
    for entry in &release.entries {
        match entry.action {
            EntryAction::Write => match current.get(&entry.path) {
                None => {
                    // 发布后被人工删掉也算改动，不能替人恢复。
                    conflicts.insert(entry.path.clone());
                }
                Some(bytes) if sha256(bytes) == entry.sha256 => {
                    let target = baseline.get(&entry.path).cloned().ok_or_else(|| {
                        error(
                            "VAULT_NO_BASELINE",
                            format!("基准里没有 {}，无法回滚该路径。", entry.path),
                        )
                    })?;
                    want.insert(entry.path.clone(), target);
                }
                Some(_) => {
                    conflicts.insert(entry.path.clone());
                }
            },
            EntryAction::Delete => {
                if current.contains_key(&entry.path) {
                    conflicts.insert(entry.path.clone());
                } else if let Some(bytes) = baseline.get(&entry.path) {
                    want.insert(entry.path.clone(), bytes.clone());
                } else {
                    conflicts.insert(entry.path.clone());
                }
            }
        }
    }
    if !conflicts.is_empty() {
        return Err(ProjectError::new(
            ROLLBACK_CONFLICT,
            3,
            format!(
                "以下 {} 个路径在发布之后被人工改过（或无法按 adopt 冻结的基准恢复），回滚会覆盖这些改动，因此停止：{}；未写出任何字节。",
                conflicts.len(),
                conflicts.iter().cloned().collect::<Vec<_>>().join("、"),
            ),
        ));
    }
    if want.is_empty() {
        return Err(error(
            "VAULT_ROLLBACK_EMPTY",
            format!(
                "最新发布 #{} 写过的路径已经全部与基准一致，没有需要回滚的条目；未写出任何字节。",
                latest.sequence
            ),
        ));
    }
    let mut entries: Vec<Entry> = Vec::new();
    let mut source_files: Vec<SourceFile> = Vec::new();
    let mut impacts: Vec<Impact> = Vec::new();
    let notes = store.author.notes_set();
    for (path, bytes) in &want {
        let digest = sha256(bytes);
        let previous = current
            .get(path)
            .map(|bytes| sha256(bytes))
            .unwrap_or_default();
        entries.push(Entry {
            action: EntryAction::Write,
            path: path.clone(),
            sha256: digest.clone(),
            byte_length: bytes.len() as u64,
            previous_sha256: previous,
        });
        source_files.push(SourceFile {
            path: path.clone(),
            sha256: digest,
            byte_length: bytes.len() as u64,
        });
        if notes.contains(path) {
            impacts.push(Impact {
                category: IMPACT_BODY.to_owned(),
                path: path.clone(),
                detail: "回滚正文到 adopt 冻结的基准字节".to_owned(),
            });
            impacts.push(Impact {
                category: IMPACT_LINK.to_owned(),
                path: path.clone(),
                detail: "回滚后链接集合回到基准".to_owned(),
            });
        } else {
            impacts.push(Impact {
                category: IMPACT_ATTACHMENT.to_owned(),
                path: path.clone(),
                detail: "回滚附件到 adopt 冻结的基准字节".to_owned(),
            });
        }
    }
    impacts.sort();
    impacts.dedup();
    let managed = store.author.managed();
    let mut landing: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
    for path in &managed {
        landing.insert(path.clone(), current.get(path).cloned());
    }
    for entry in &entries {
        if let Some(bytes) = want.get(&entry.path) {
            landing.insert(entry.path.clone(), Some(bytes.clone()));
        }
    }
    let final_paths: BTreeSet<String> = landing
        .iter()
        .filter(|(_, bytes)| bytes.is_some())
        .map(|(path, _)| path.clone())
        .collect();
    let final_links = collect_links(&store.author, &landing, &final_paths);
    let mut affected: BTreeSet<String> = BTreeSet::new();
    for entry in &entries {
        affected.insert(entry.path.clone());
    }
    for impact in &impacts {
        affected.insert(impact.path.clone());
    }
    let stamps = store.stamps(&affected)?;
    let impacts_sha256 = sha256(canonical_impacts(&impacts).as_bytes());
    let created_at = now()?;
    let final_paths: Vec<String> = final_paths.into_iter().collect();
    let payload = PlanPayload {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        kind: KIND_ROLLBACK.to_owned(),
        case_id: format!("rollback-{}", latest.release_id),
        summary: format!(
            "回滚最新发布 #{}（release_id={}）写过的路径到 adopt 冻结的基准字节",
            latest.sequence, latest.release_id
        ),
        operations: Vec::new(),
        impacts: impacts.clone(),
        impacts_sha256,
        entries: entries.clone(),
        stamps: stamps.clone(),
        final_paths: final_paths.clone(),
        final_links,
        source: SourceSpec {
            kind: SOURCE_BASELINE.to_owned(),
            root: absolute(&store.root().join(BASELINE))?,
            files: source_files,
        },
        owner_sha256: store.owner_sha256(),
        metadata_sha256: store.metadata_sha256(),
        author_manifest_sha256: store.owner_file().author_manifest_sha256.clone(),
        baseline_manifest_sha256: store.owner_file().baseline_manifest_sha256.clone(),
        created_at: created_at.clone(),
    };
    let payload_json = canonical(&payload)?;
    let plan_sha256 = sha256(payload_json.as_bytes());
    let view = PlanView {
        policy: payload.policy.clone(),
        schema_version: payload.schema_version,
        plan_id: plan_sha256.clone(),
        plan_sha256: plan_sha256.clone(),
        kind: payload.kind.clone(),
        case_id: payload.case_id.clone(),
        summary: payload.summary.clone(),
        operations: Vec::new(),
        impacts: impacts.clone(),
        impacts_sha256: payload.impacts_sha256.clone(),
        entries: entries.clone(),
        stamps,
        final_paths,
        source: payload.source.clone(),
        owner_sha256: payload.owner_sha256.clone(),
        metadata_sha256: payload.metadata_sha256.clone(),
        created_at,
        affected_paths: affected.iter().cloned().collect(),
    };
    let text = canonical(&view)?;
    store.insert_plan(KIND_ROLLBACK, &payload_json, &payload.created_at)?;
    write_new_file(&plan_path, text.as_bytes())?;
    Ok(format!(
        "回滚计划已写出：plan_id={plan_sha256}（{KIND_ROLLBACK}，针对 release #{}）\n来源：adopt 冻结的基准字节\n影响：{} 项（{}）\n条目：{} 条（全部为写回基准）\n说明：回滚计划同样需要 approve 才会生效；这份计划没有改动 vault/ 一个字节。\n未批准覆盖：0\n",
        latest.sequence,
        impacts.len(),
        categories(&impacts),
        entries.len(),
    ))
}

// ---------------------------------------------------------------------------
// 公共小工具
// ---------------------------------------------------------------------------

fn require_dir(path: &Path) -> Result<(), ProjectError> {
    let metadata = fs::symlink_metadata(path).map_err(|failure| ProjectError::io(path, failure))?;
    if project::is_link(&metadata) || !metadata.is_dir() {
        return Err(argument(format!("不是普通目录：{}", path.display())));
    }
    Ok(())
}

fn require_new_file(path: &Path) -> Result<(), ProjectError> {
    if project::exists(path)? {
        return Err(argument(format!(
            "目标文件已存在，拒绝覆盖：{}",
            path.display()
        )));
    }
    let parent = path
        .parent()
        .ok_or_else(|| argument(format!("路径没有上级目录：{}", path.display())))?;
    if !project::exists(parent)? {
        return Err(argument(format!("上级目录不存在：{}", parent.display())));
    }
    Ok(())
}

fn absolute(path: &Path) -> Result<String, ProjectError> {
    path.to_str()
        .map(|text| text.to_owned())
        .ok_or_else(|| argument(format!("路径不是合法 UTF-8：{}", path.display())))
}

/// 候选目录只允许 `operations.json` 与它显式列出的字节。
fn candidate_files(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, ProjectError> {
    let tree = tree(root)?;
    let mut out = BTreeMap::new();
    for (relative, path) in &tree.files {
        if relative == OPERATIONS_FILE {
            continue;
        }
        out.insert(relative.clone(), read_bounded(path, MAX_FILE_BYTES)?);
    }
    Ok(out)
}

fn affected_paths(analysis: &Analysis) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for entry in &analysis.entries {
        out.insert(entry.path.clone());
    }
    for impact in &analysis.impacts {
        out.insert(impact.path.clone());
    }
    out
}

fn source_files(analysis: &Analysis) -> Vec<SourceFile> {
    analysis
        .entries
        .iter()
        .filter(|entry| entry.action == EntryAction::Write)
        .map(|entry| SourceFile {
            path: entry.path.clone(),
            sha256: entry.sha256.clone(),
            byte_length: entry.byte_length,
        })
        .collect()
}

fn states_from(entries: &[Entry]) -> Vec<ReleaseEntryState> {
    entries
        .iter()
        .map(|entry| ReleaseEntryState {
            action: entry.action,
            path: entry.path.clone(),
            sha256: entry.sha256.clone(),
            byte_length: entry.byte_length,
            state: STATE_PENDING.to_owned(),
        })
        .collect()
}

fn same_states(states: &[ReleaseEntryState], entries: &[Entry]) -> bool {
    states.len() == entries.len()
        && states.iter().zip(entries.iter()).all(|(state, entry)| {
            state.action == entry.action
                && state.path == entry.path
                && state.sha256 == entry.sha256
                && state.byte_length == entry.byte_length
        })
}

fn stamp_drift(bound: &[StampSlot], actual: &[StampSlot]) -> Vec<String> {
    let observed: BTreeMap<&str, &Option<crate::run_model::FileStamp>> = actual
        .iter()
        .map(|slot| (slot.path.as_str(), &slot.stamp))
        .collect();
    let mut drifted = Vec::new();
    for slot in bound {
        match observed.get(slot.path.as_str()) {
            Some(stamp) if **stamp == slot.stamp => {}
            _ => drifted.push(slot.path.clone()),
        }
    }
    drifted
}

fn categories(impacts: &[Impact]) -> String {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for impact in impacts {
        seen.insert(impact.category.as_str());
    }
    seen.into_iter().collect::<Vec<_>>().join("/")
}

fn is_png(author: &Author, path: &str) -> bool {
    author
        .resources
        .iter()
        .any(|resource| resource.path == path && resource.media_type == "image/png")
}

/// 只做结构解析，不猜像素含义：拒绝动画分块、要求 IEND 正好落在文件末尾。
fn validate_png(bytes: &[u8]) -> Result<(), String> {
    const MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("文件超过 8 MiB".to_owned());
    }
    if !bytes.starts_with(&MAGIC) {
        return Err("缺少 PNG 魔数".to_owned());
    }
    let mut offset = MAGIC.len();
    loop {
        if offset + 12 > bytes.len() {
            return Err("分块头被截断".to_owned());
        }
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let kind = &bytes[offset + 4..offset + 8];
        let end = offset
            .checked_add(12)
            .and_then(|value| value.checked_add(length))
            .ok_or_else(|| "分块长度溢出".to_owned())?;
        if end > bytes.len() {
            return Err("分块越过文件末尾".to_owned());
        }
        if kind == b"acTL" || kind == b"fcTL" || kind == b"fdAT" {
            return Err("拒绝 APNG 动画分块".to_owned());
        }
        if kind == b"IEND" {
            if length != 0 || end != bytes.len() {
                return Err("IEND 分块长度非零或不在文件末尾".to_owned());
            }
            break;
        }
        offset = end;
    }
    let decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: 32 * 1024 * 1024,
        },
    );
    let mut reader = decoder.read_info().map_err(|failure| failure.to_string())?;
    if reader.info().animation_control.is_some() {
        return Err("拒绝动画 PNG".to_owned());
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| "无法确定解码缓冲大小".to_owned())?;
    if size > 32 * 1024 * 1024 {
        return Err("解码缓冲超过上限".to_owned());
    }
    let mut decoded = vec![0u8; size];
    reader
        .next_frame(&mut decoded)
        .map_err(|failure| failure.to_string())?;
    reader.finish().map_err(|failure| failure.to_string())?;
    Ok(())
}

fn normalized_links(links: &[ResolvedLink]) -> Vec<(String, String, String, bool)> {
    let mut out: Vec<(String, String, String, bool)> = links
        .iter()
        .map(|link| {
            (
                link.from.clone(),
                link.raw.clone(),
                link.resolves_to.clone(),
                link.present,
            )
        })
        .collect();
    out.sort();
    out
}

fn collect_links(
    author: &Author,
    state: &BTreeMap<String, Option<Vec<u8>>>,
    paths: &BTreeSet<String>,
) -> Vec<ResolvedLink> {
    let notes = author.notes_set();
    let mut links: Vec<ResolvedLink> = Vec::new();
    for path in paths.iter().filter(|path| notes.contains(*path)) {
        let Some(Some(bytes)) = state.get(path) else {
            continue;
        };
        let text = String::from_utf8_lossy(bytes);
        for raw in link_targets(&text) {
            let Some(resolved) = resolve_link(path, &raw) else {
                continue;
            };
            links.push(ResolvedLink {
                from: path.clone(),
                raw: raw.clone(),
                resolves_to: resolved.clone(),
                present: paths.contains(&resolved),
            });
        }
    }
    links.sort();
    links
}

fn compute_final_state(
    author: &Author,
    current: &BTreeMap<String, Vec<u8>>,
    entries: &[Entry],
    staged: &BTreeMap<String, Vec<u8>>,
) -> Result<BTreeMap<String, Option<Vec<u8>>>, ProjectError> {
    let mut state: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
    for path in author.managed() {
        state.insert(path.clone(), current.get(&path).cloned());
    }
    for entry in entries {
        match entry.action {
            EntryAction::Write => {
                let bytes = staged.get(&entry.path).cloned().ok_or_else(|| {
                    integrity(format!(
                        "条目 {} 缺少已暂存的字节；vault 未写入任何字节。",
                        entry.path
                    ))
                })?;
                state.insert(entry.path.clone(), Some(bytes));
            }
            EntryAction::Delete => {
                state.insert(entry.path.clone(), None);
            }
        }
    }
    Ok(state)
}

fn write_vault_file(store: &VaultStore, relative: &str, bytes: &[u8]) -> Result<(), ProjectError> {
    // Reject unsafe spellings before PathBuf::push can normalize them on
    // Windows verbatim roots. The persisted contract always uses `/`.
    if relative.is_empty()
        || relative.contains('\\')
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(argument(format!(
            "路径必须是托管根内的规范相对路径：{relative}；vault 未写入任何字节。"
        )));
    }
    let vault = store.root().join(VAULT);
    let safe = project::check_components(&store.vault_path(relative))?;
    if safe == vault || !safe.starts_with(&vault) {
        return Err(argument(format!(
            "路径 {relative} 逃出了托管根；vault 未写入任何字节。"
        )));
    }
    create_parent_dirs(&safe)?;
    if project::exists(&safe)? {
        project::checked_file(&safe)?;
        write_replace(&safe, bytes)
    } else {
        write_new_file(&safe, bytes)
    }
}

fn append_log(
    release_dir: &Path,
    sequence: u64,
    index: usize,
    entry: &Entry,
    state: &str,
) -> Result<(), ProjectError> {
    let at = now()?;
    let path_json = serde_json::to_string(&entry.path)
        .map_err(|failure| integrity(format!("无法记录提交日志：{failure}")))?;
    let at_json = serde_json::to_string(&at)
        .map_err(|failure| integrity(format!("无法记录提交日志：{failure}")))?;
    let line = format!(
        "{{\"at\":{at_json},\"sequence\":{sequence},\"index\":{index},\"action\":\"{}\",\"path\":{path_json},\"sha256\":\"{}\",\"byte_length\":{},\"state\":\"{state}\"}}\n",
        entry.action.as_str(),
        entry.sha256,
        entry.byte_length,
    );
    append_bytes(&release_dir.join(RELEASE_LOG), line.as_bytes())
}

fn stale(text: impl Into<String>) -> ProjectError {
    ProjectError::new(crate::vault_model::APPROVAL_STALE, 3, text)
}

/// 仅测试用的暂停点：`pending` 记录已经落账、还没开始逐项写 `vault/`。
fn pause_after_pending() {
    if std::env::var("NOETOME_VAULT_TEST_PAUSE_AFTER_PENDING").is_err() {
        return;
    }
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "NOETOME_VAULT_TEST_READY");
    let _ = stdout.flush();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

#[cfg(test)]
fn crash_after_entries(confirmed: usize) {
    let Ok(value) = std::env::var("NOETOME_VAULT_CRASH_AFTER_ENTRIES") else {
        return;
    };
    if let Ok(limit) = value.parse::<usize>() {
        if confirmed == limit {
            let _ = std::io::stdout().flush();
            std::process::abort();
        }
    }
}

#[cfg(not(test))]
fn crash_after_entries(_confirmed: usize) {}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        fs::canonicalize(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/managed-vault-update-v1"),
        )
        .expect("fixture")
    }

    fn scratch(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "noetome-vault-update-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("scratch");
        path
    }

    fn fresh(tag: &str) -> (PathBuf, PathBuf) {
        let base = scratch(tag);
        let root = base.join("root");
        let summary =
            crate::vault_store::adopt(&fixture().join("author.json"), &root).expect("adopt");
        assert!(summary.contains("已接管"), "{summary}");
        (base, root)
    }

    fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            for entry in fs::read_dir(dir).expect("read_dir") {
                let path = entry.expect("entry").path();
                let metadata = fs::symlink_metadata(&path).expect("metadata");
                if metadata.is_dir() {
                    walk(root, &path, out);
                } else {
                    let key = path
                        .strip_prefix(root)
                        .expect("prefix")
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.insert(key, fs::read(&path).expect("read"));
                }
            }
        }
        walk(root, root, &mut out);
        out
    }

    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).expect("copy dir");
        for entry in fs::read_dir(from).expect("read dir") {
            let entry = entry.expect("entry");
            let target = to.join(entry.file_name());
            if entry.file_type().expect("type").is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), &target).expect("copy file");
            }
        }
    }

    fn plans(base: &Path) -> PathBuf {
        let path = base.join("plans");
        fs::create_dir_all(&path).expect("plans");
        path
    }

    fn publish_full(base: &Path, root: &Path, case: &str) -> PathBuf {
        let tag = uuid::Uuid::new_v4();
        let plan_path = plans(base).join(format!("{case}-{tag}.plan.json"));
        let summary = plan(root, &fixture().join("candidates").join(case), &plan_path)
            .unwrap_or_else(|failure| panic!("{case}：{failure}"));
        assert!(summary.contains("plan_id="), "{summary}");
        let approval_path = plans(base).join(format!("{case}-{tag}.approval.json"));
        let summary = approve(root, &plan_path, &approval_path).expect("approve");
        assert!(summary.contains("未批准覆盖：0"), "{summary}");
        approval_path
    }

    fn release_id(summary: &str) -> String {
        summary
            .split("release_id=")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .expect("release id")
            .to_owned()
    }

    #[test]
    fn every_frozen_case_decision_is_reproduced() {
        let (base, root) = fresh("cases");
        let cases: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture().join("cases.json")).expect("cases"))
                .expect("json");
        for case in cases["cases"].as_array().expect("array") {
            let id = case["id"].as_str().expect("id");
            let candidate_dir = fixture().join(case["candidate"].as_str().expect("candidate"));
            let plan_path = plans(&base).join(format!("{id}.plan.json"));
            let outcome = plan(&root, &candidate_dir, &plan_path);
            match case["expect"]["decision"].as_str().expect("decision") {
                "approvable" => {
                    let summary = outcome.unwrap_or_else(|failure| panic!("{id}：{failure}"));
                    assert!(summary.contains("计划已写出"), "{id}：{summary}");
                    assert!(plan_path.exists(), "{id}：计划文件缺失");
                }
                "refuse" => {
                    let failure = match outcome {
                        Ok(_) => panic!("{id}：本应拒绝"),
                        Err(failure) => failure,
                    };
                    let refusal = case["expect"]["refusal"].as_str().expect("refusal");
                    assert_eq!(failure.code(), refusal, "{id}");
                    assert!(!plan_path.exists(), "{id}：拒绝时不应写出计划文件");
                }
                other => panic!("未知判定 {other}"),
            }
        }
    }

    #[test]
    fn unsafe_write_spellings_leave_the_entire_managed_root_unchanged() {
        let (_base, root) = fresh("write-spellings");
        let store = VaultStore::open(&root).unwrap();
        let before = snapshot(&root);
        for relative in [
            "",
            "../outside.md",
            "/outside.md",
            "笔记/../../outside.md",
            "笔记//力学.md",
            "笔记/./力学.md",
            "笔记\\..\\outside.md",
        ] {
            assert!(
                write_vault_file(&store, relative, b"must not land").is_err(),
                "{relative}"
            );
            assert_eq!(
                snapshot(&root),
                before,
                "unsafe spelling wrote bytes: {relative}"
            );
        }
    }

    #[test]
    fn publishing_the_edit_body_case_lands_the_candidate_bytes() {
        let (base, root) = fresh("edit-body");
        let approval_path = publish_full(&base, &root, "edit-body");
        let summary = publish(&root, &approval_path).expect("publish");
        assert!(summary.contains("release_id="), "{summary}");
        assert!(summary.contains("status=published"), "{summary}");
        assert!(summary.contains("未批准覆盖：0"), "{summary}");
        let candidate_bytes = fs::read(fixture().join("candidates/edit-body").join("笔记/力学.md"))
            .expect("candidate file");
        let target = root.join("vault").join("笔记/力学.md");
        assert_eq!(fs::read(&target).expect("vault file"), candidate_bytes);
        let log = fs::read_to_string(
            root.join("releases")
                .join(release_id(&summary))
                .join("commit.log"),
        )
        .expect("commit log");
        assert!(log.contains("笔记/力学.md"), "{log}");
        assert!(log.contains("\"state\":\"written\""), "{log}");
        // 再发布一次是幂等报告，不再写字节。
        let again = publish(&root, &approval_path).expect("republish");
        assert!(again.contains("status=published"), "{again}");
        assert_eq!(fs::read(&target).expect("vault file"), candidate_bytes);
    }

    #[test]
    fn publishing_a_rename_writes_adds_and_deletes() {
        let (base, root) = fresh("rename");
        let approval_path = publish_full(&base, &root, "rename-linked");
        let summary = publish(&root, &approval_path).expect("publish");
        assert!(summary.contains("status=published"), "{summary}");
        let vault = root.join("vault");
        assert!(vault.join("笔记/几何光学.md").exists());
        assert!(!vault.join("笔记/光学.md").exists());
        assert_eq!(
            fs::read(vault.join("笔记/几何光学.md")).expect("note"),
            fs::read(
                fixture()
                    .join("candidates/rename-linked")
                    .join("笔记/几何光学.md")
            )
            .expect("src")
        );
        let log = fs::read_to_string(
            root.join("releases")
                .join(release_id(&summary))
                .join("commit.log"),
        )
        .expect("commit log");
        assert!(log.contains("\"state\":\"deleted\""), "{log}");
        let status_text = crate::vault_store::status(&root).expect("status");
        assert!(!status_text.contains("needs_recovery"), "{status_text}");
    }

    #[test]
    fn an_edit_after_approval_invalidates_the_approval() {
        let (base, root) = fresh("stale");
        let approval_path = publish_full(&base, &root, "edit-after-approval");
        let target = root.join("vault").join("笔记/力学.md");
        fs::write(&target, "人工改写\n".as_bytes()).expect("mutate");
        let before = snapshot(&root.join("vault"));
        let failure = publish(&root, &approval_path).expect_err("应当拒绝");
        assert_eq!(failure.code(), crate::vault_model::APPROVAL_STALE);
        assert!(failure.to_string().contains("笔记/力学.md"), "{failure}");
        assert_eq!(
            snapshot(&root.join("vault")),
            before,
            "拒绝时不应改动 vault/"
        );
    }

    #[test]
    fn a_candidate_changed_after_approval_leaves_the_vault_untouched() {
        let (base, root) = fresh("source-changed");
        let candidate = base.join("candidate");
        copy_tree(&fixture().join("candidates").join("edit-body"), &candidate);
        let tag = uuid::Uuid::new_v4();
        let plan_path = plans(&base).join(format!("{tag}.plan.json"));
        plan(&root, &candidate, &plan_path).expect("plan");
        let approval_path = plans(&base).join(format!("{tag}.approval.json"));
        approve(&root, &plan_path, &approval_path).expect("approve");
        fs::write(candidate.join("笔记/力学.md"), "被改掉\n").expect("mutate source");
        let before = snapshot(&root.join("vault"));
        let failure = publish(&root, &approval_path).expect_err("应当拒绝");
        assert_eq!(failure.code(), "VAULT_SOURCE_CHANGED");
        assert_eq!(
            snapshot(&root.join("vault")),
            before,
            "拒绝时不应改动 vault/"
        );
    }

    #[test]
    fn publishing_keeps_unrelated_content_and_obsidian_config_byte_identical() {
        let (base, root) = fresh("unrelated");
        let vault = root.join("vault");
        fs::create_dir_all(vault.join(".obsidian")).expect("obsidian dir");
        fs::write(
            vault.join(".obsidian").join("app.json"),
            "{\"theme\":\"moonstone\"}\n",
        )
        .expect("config");
        fs::write(vault.join("笔记").join("陌生.md"), "陌生笔记\n").expect("stranger");
        let config_before = fs::read(vault.join(".obsidian").join("app.json")).expect("config");
        let stranger_before = fs::read(vault.join("笔记").join("陌生.md")).expect("stranger");
        let approval_path = publish_full(&base, &root, "add-link");
        let summary = publish(&root, &approval_path).expect("publish");
        assert!(summary.contains("status=published"), "{summary}");
        assert_eq!(
            fs::read(vault.join(".obsidian").join("app.json")).expect("config"),
            config_before
        );
        assert_eq!(
            fs::read(vault.join("笔记").join("陌生.md")).expect("stranger"),
            stranger_before
        );
        let status_text = crate::vault_store::status(&root).expect("status");
        assert!(status_text.contains("无关内容：2 个"), "{status_text}");
    }

    /// 子进程 Worker：在 commit 中途被 abort，父测试据此证明「每个边界真的终止」。
    #[test]
    #[ignore]
    fn crash_worker() {
        let Some(root) = std::env::var_os("NOETOME_VAULT_CRASH_ROOT") else {
            return;
        };
        let approval = std::env::var_os("NOETOME_VAULT_CRASH_APPROVAL").expect("approval");
        let _ = publish(Path::new(&root), Path::new(&approval));
        panic!("子进程本应在逐项落盘途中被 abort");
    }

    #[test]
    fn a_real_abort_mid_commit_reports_needs_recovery_and_then_resumes() {
        let (base, root) = fresh("crash");
        let approval_path = publish_full(&base, &root, "rename-linked");
        let status = std::process::Command::new(std::env::current_exe().expect("current_exe"))
            .args([
                "--exact",
                "vault_update::tests::crash_worker",
                "--ignored",
                "--nocapture",
            ])
            .env("NOETOME_VAULT_CRASH_ROOT", &root)
            .env("NOETOME_VAULT_CRASH_APPROVAL", &approval_path)
            .env("NOETOME_VAULT_CRASH_AFTER_ENTRIES", "2")
            .status()
            .expect("spawn");
        assert!(!status.success(), "子进程本应异常终止：{status:?}");
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(status.signal(), Some(6), "应当收到 SIGABRT");
        }
        let text = crate::vault_store::status(&root).expect("status");
        assert!(text.contains("needs_recovery"), "{text}");
        let resumed = publish(&root, &approval_path).expect("resume");
        assert!(resumed.contains("status=published"), "{resumed}");
        let after = crate::vault_store::status(&root).expect("status");
        assert!(!after.contains("needs_recovery"), "{after}");
        assert_eq!(
            fs::read(root.join("vault").join("笔记/力学.md")).expect("note"),
            fs::read(
                fixture()
                    .join("candidates/rename-linked")
                    .join("笔记/力学.md")
            )
            .expect("src")
        );
        let log = fs::read_to_string(
            root.join("releases")
                .join(release_id(&resumed))
                .join("commit.log"),
        )
        .expect("commit log");
        assert_eq!(log.lines().count(), 3, "{log}");
    }

    #[test]
    fn rollback_emits_a_reverse_plan_and_detects_later_manual_edits() {
        let (base, root) = fresh("rollback");
        let before = snapshot(&root.join("vault"));
        let approval_path = publish_full(&base, &root, "edit-body");
        publish(&root, &approval_path).expect("publish");
        assert_ne!(snapshot(&root.join("vault")), before, "发布应改变 vault");

        let plan_path = plans(&base).join("rollback.plan.json");
        let summary = rollback(&root, &plan_path).expect("rollback plan");
        assert!(summary.contains("回滚计划已写出"), "{summary}");
        let approval = plans(&base).join("rollback.approval.json");
        approve(&root, &plan_path, &approval).expect("approve rollback");
        let published = publish(&root, &approval).expect("publish rollback");
        assert!(published.contains("status=published"), "{published}");
        assert_eq!(
            snapshot(&root.join("vault")),
            before,
            "回滚后应逐字节回到基准"
        );

        // 再发布一次 edit-body，然后手工改掉，回滚必须拒绝覆盖人工改动。
        let approval_two = publish_full(&base, &root, "edit-body");
        publish(&root, &approval_two).expect("publish again");
        fs::write(root.join("vault").join("笔记/力学.md"), "又改了\n").expect("mutate");
        let conflict_plan = plans(&base).join("rollback-conflict.plan.json");
        let failure = rollback(&root, &conflict_plan).expect_err("应当报冲突");
        assert_eq!(failure.code(), crate::vault_model::ROLLBACK_CONFLICT);
        assert!(failure.to_string().contains("笔记/力学.md"), "{failure}");
        assert!(!conflict_plan.exists());
    }

    #[test]
    fn plan_and_approve_refuse_to_overwrite_existing_files() {
        let (base, root) = fresh("guards");
        let plan_path = plans(&base).join("guard.plan.json");
        plan(&root, &fixture().join("candidates/edit-body"), &plan_path).expect("plan");
        let failure = plan(&root, &fixture().join("candidates/edit-body"), &plan_path)
            .expect_err("计划文件已存在");
        assert_eq!(failure.code(), "VAULT_ARGUMENT");
        let missing = base.join("nope").join("plan.json");
        let failure = plan(&root, &fixture().join("candidates/edit-body"), &missing)
            .expect_err("上级目录缺失");
        assert_eq!(failure.code(), "VAULT_ARGUMENT");
        let failure = plan(
            &root,
            &base.join("no-such-candidate"),
            &plans(&base).join("x.json"),
        )
        .expect_err("候选目录缺失");
        assert_eq!(failure.code(), "PROJECT_IO_ERROR");
    }
}
