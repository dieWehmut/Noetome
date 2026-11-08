//! Frozen audit journal root and deterministic outputs for the `journal` verb.

use crate::compile_identity::{Identity, identity};
use crate::journal_model::{
    self, Declaration, EventRecord, FailureRecord, JournalFile, MAX_BYTES, POLICY, Report,
    TaskRecord,
};
use crate::project::ProjectError;
use serde::{Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const APP_PRODUCER: &str = "noetome-journal";
pub(crate) const OUTPUT_PRODUCER: &str = "audit_journal_output";
const CONTROL: &str = "control";
const MANIFEST: &str = "manifest.json";
const INPUTS: &str = "inputs.json";
const PROJECT: &str = "project.json";
const JOURNAL: &str = "journal.json";
const REPORT: &str = "report.json";
const MARKDOWN: &str = "report.md";
const OUTPUT_MANIFEST: &str = "manifest.json";
const ROOT_ENTRIES: [&str; 1] = [CONTROL];
const CONTROL_ENTRIES: [&str; 3] = [INPUTS, MANIFEST, PROJECT];
const OUTPUT_ENTRIES: [&str; 4] = [JOURNAL, MANIFEST, MARKDOWN, REPORT];

fn canonical<T: Serialize>(value: &T, label: &str) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec(value)
        .map_err(|error| journal_model::integrity(format!("{label}无法序列化：{error}。")))
}

fn strict<T: DeserializeOwned + Serialize>(bytes: &[u8], label: &str) -> Result<T, ProjectError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| journal_model::integrity(format!("{label}不是合法 UTF-8：{error}。")))?;
    let value: T = serde_json::from_str(text)
        .map_err(|error| journal_model::integrity(format!("{label}不是合法 JSON：{error}。")))?;
    let reencoded = serde_json::to_string(&value)
        .map_err(|error| journal_model::integrity(format!("{label}无法重新编码：{error}。")))?;
    if reencoded != text {
        return Err(journal_model::integrity(format!(
            "{label}不是规范编码：冻结的字节与重新编码的结果不一致。"
        )));
    }
    Ok(value)
}

fn now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default();
    format!("{seconds}")
}

fn read_file(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>, ProjectError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        journal_model::integrity(format!("无法读取{label} {}：{error}。", path.display()))
    })?;
    if !metadata.is_file() {
        return Err(journal_model::integrity(format!(
            "{label} {} 不是普通文件。",
            path.display()
        )));
    }
    if metadata.len() > limit {
        return Err(journal_model::integrity(format!(
            "{label} {} 有 {} 字节，超过上限 {limit}。",
            path.display(),
            metadata.len()
        )));
    }
    let mut file = fs::File::open(path).map_err(|error| {
        journal_model::integrity(format!("无法打开{label} {}：{error}。", path.display()))
    })?;
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            journal_model::integrity(format!("读取{label} {} 失败：{error}。", path.display()))
        })?;
    if bytes.len() as u64 > limit {
        return Err(journal_model::integrity(format!(
            "{label} {} 读取时超过上限 {limit}。",
            path.display()
        )));
    }
    Ok(bytes)
}

fn create_parent(path: &Path, label: &str) -> Result<PathBuf, ProjectError> {
    let parent = path.parent().ok_or_else(|| {
        journal_model::argument(format!("{label}必须是一个目录名：{}。", path.display()))
    })?;
    fs::canonicalize(parent).map_err(|error| {
        journal_model::integrity(format!(
            "无法访问{label}的父目录 {}：{error}。",
            parent.display()
        ))
    })
}

fn write_new(path: &Path, bytes: &[u8], label: &str) -> Result<(), ProjectError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            journal_model::integrity(format!("无法新建{label} {}：{error}。", path.display()))
        })?;
    file.write_all(bytes).map_err(|error| {
        journal_model::integrity(format!("写入{label} {} 失败：{error}。", path.display()))
    })?;
    file.sync_all().map_err(|error| {
        journal_model::integrity(format!("同步{label} {} 失败：{error}。", path.display()))
    })?;
    let back = fs::read(path).map_err(|error| {
        journal_model::integrity(format!("回读{label} {} 失败：{error}。", path.display()))
    })?;
    if back != bytes {
        return Err(journal_model::integrity(format!(
            "{label} {} 写入后回读不一致。",
            path.display()
        )));
    }
    Ok(())
}

fn require_dir(path: &Path, label: &str) -> Result<(), ProjectError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        journal_model::integrity(format!("无法访问{label} {}：{error}。", path.display()))
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(journal_model::integrity(format!(
            "{label} {} 必须是一个真实目录。",
            path.display()
        )));
    }
    Ok(())
}

fn names(path: &Path) -> Result<Vec<String>, ProjectError> {
    let mut entries = Vec::new();
    let reader = fs::read_dir(path).map_err(|error| {
        journal_model::integrity(format!("无法列出 {}：{error}。", path.display()))
    })?;
    for entry in reader {
        let entry = entry.map_err(|error| {
            journal_model::integrity(format!("无法读取 {} 的条目：{error}。", path.display()))
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        entries.push(name);
    }
    entries.sort();
    Ok(entries)
}

fn resolve(root: &Path) -> Result<PathBuf, ProjectError> {
    let text = root.to_string_lossy();
    if text.is_empty() {
        return Err(journal_model::argument("审计根不能为空。"));
    }
    let mut path = PathBuf::new();
    for component in root.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !path.pop() {
                    return Err(ProjectError::unsafe_path(root));
                }
            }
            other => path.push(other.as_os_str()),
        }
    }
    Ok(path)
}

fn short(digest: &str) -> String {
    digest.chars().take(16).collect()
}

fn digest_of(bytes: &[u8]) -> String {
    crate::text::sha256(bytes)
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub policy: String,
    pub producer: String,
    pub schema_version: u32,
    pub case_id: String,
    pub root: String,
    pub root_identity: Identity,
    pub parent_identity: Identity,
    pub control_identity: Identity,
    pub manifest_identity: Identity,
    pub inputs_identity: Identity,
    pub project_sha256: String,
    pub inputs_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Inputs {
    pub policy: String,
    pub schema_version: u32,
    pub case_id: String,
    pub project_sha256: String,
    pub tasks: usize,
    pub events: usize,
    pub sources: usize,
    pub objects: usize,
    pub files: usize,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OutputManifest {
    pub policy: String,
    pub producer: String,
    pub schema_version: u32,
    pub case_id: String,
    pub source_root: String,
    pub source_inputs_sha256: String,
    pub journal_sha256: String,
    pub report_sha256: String,
    pub markdown_sha256: String,
    pub counts: journal_model::Counts,
}

impl Inputs {
    fn of(declaration: &Declaration, project_sha256: &str) -> Self {
        Self {
            policy: POLICY.to_owned(),
            schema_version: journal_model::SCHEMA_VERSION,
            case_id: declaration.case_id.clone(),
            project_sha256: project_sha256.to_owned(),
            tasks: declaration.tasks.len(),
            events: declaration.events.len(),
            sources: declaration.sources.len(),
            objects: declaration.objects.len(),
            files: declaration.files.len(),
        }
    }
}

pub(crate) struct Frozen {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub inputs: Inputs,
    pub declaration: Declaration,
}

impl Frozen {
    fn journal(&self) -> Result<journal_model::Journal, ProjectError> {
        journal_model::journal(&self.declaration)
    }
}

fn adopt_into(
    root: &Path,
    declaration: &Declaration,
    project_bytes: &[u8],
    project_sha256: &str,
    inputs: &Inputs,
) -> Result<(), ProjectError> {
    fs::create_dir(root).map_err(|error| {
        journal_model::integrity(format!("无法建立审计根 {}：{error}。", root.display()))
    })?;
    let control = root.join(CONTROL);
    fs::create_dir(&control).map_err(|error| {
        journal_model::integrity(format!("无法建立 {}：{error}。", control.display()))
    })?;
    write_new(&control.join(PROJECT), project_bytes, "工程声明副本")?;
    let inputs_bytes = canonical(inputs, "冻结输入")?;
    write_new(&control.join(INPUTS), &inputs_bytes, "冻结输入")?;
    // Manifest is written last: it is the file everything else is validated
    // against, so it must never exist for a half-written root.
    write_new(&control.join(MANIFEST), b"", "审计根清单")?;
    let canonical_root = fs::canonicalize(root).map_err(|error| {
        journal_model::integrity(format!("无法确定审计根 {}：{error}。", root.display()))
    })?;
    let parent = canonical_root
        .parent()
        .ok_or_else(|| journal_model::argument("审计根必须有父目录。"))?;
    let manifest_path = control.join(MANIFEST);
    let manifest = Manifest {
        policy: POLICY.to_owned(),
        producer: APP_PRODUCER.to_owned(),
        schema_version: journal_model::SCHEMA_VERSION,
        case_id: declaration.case_id.clone(),
        root: canonical_root.to_string_lossy().into_owned(),
        root_identity: identity(&canonical_root)?,
        parent_identity: identity(parent)?,
        control_identity: identity(&control)?,
        manifest_identity: identity(&manifest_path)?,
        inputs_identity: identity(&control.join(INPUTS))?,
        project_sha256: project_sha256.to_owned(),
        inputs_sha256: digest_of(&inputs_bytes),
        created_at: now(),
    };
    let bytes = canonical(&manifest, "审计根清单")?;
    fs::write(&manifest_path, &bytes)
        .map_err(|error| journal_model::integrity(format!("写入审计根清单失败：{error}。")))?;
    let back = read_file(&manifest_path, MAX_BYTES, "审计根清单")?;
    if back != bytes {
        return Err(journal_model::integrity("审计根清单写入后回读不一致。"));
    }
    Ok(())
}

fn check_identity(path: &Path, expected: &Identity, label: &str) -> Result<(), ProjectError> {
    let actual = identity(path)?;
    if &actual != expected {
        return Err(journal_model::integrity(format!(
            "{label} 的身份已变化：{}。",
            path.display()
        )));
    }
    Ok(())
}

/// Freeze a declaration into a new root. Validation and a full dry-run of the
/// journal happen *before* anything is created, so a refused declaration never
/// leaves a half-built root behind.
pub fn adopt(project: &Path, root: &Path) -> Result<String, ProjectError> {
    let target = resolve(root)?;
    let package = project
        .parent()
        .ok_or_else(|| journal_model::argument("工程声明必须有一个父目录。"))?;
    let package = fs::canonicalize(package).map_err(|error| {
        journal_model::integrity(format!(
            "无法访问工程声明的目录 {}：{error}。",
            package.display()
        ))
    })?;
    let project_path = package.join(
        project
            .file_name()
            .ok_or_else(|| journal_model::argument("工程声明必须是一个文件名。"))?,
    );
    let bytes = read_file(&project_path, MAX_BYTES, "工程声明")?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| journal_model::argument(format!("工程声明不是合法 UTF-8：{error}。")))?;
    let declaration = Declaration::parse(text)?;
    // Dry run: a declaration that cannot produce a journal must not create a root.
    let preview = journal_model::journal(&declaration)?;
    let project_sha256 = digest_of(&bytes);
    let inputs = Inputs::of(&declaration, &project_sha256);

    if target.exists() {
        return Err(journal_model::argument(format!(
            "审计根必须尚不存在：{}；请选择一个新目录。",
            target.display()
        )));
    }
    let parent = create_parent(&target, "审计根")?;
    let resolved = parent.join(
        target
            .file_name()
            .ok_or_else(|| journal_model::argument("审计根必须是一个目录名。"))?,
    );
    require_dir(&parent, "审计根的父目录")?;

    adopt_into(&resolved, &declaration, &bytes, &project_sha256, &inputs)?;
    let back = open(&resolved)?;
    if back.inputs.case_id != declaration.case_id {
        return Err(journal_model::integrity("审计根清单与声明不一致。"));
    }
    let counts = preview.report.counts;
    Ok(format!(
        "审计事件日志 {}：已建立（policy={}，日志版本 {}）\n\
         所有权：工程声明 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n\
         冻结来源：{} 个任务、{} 条事件（started {} / failed {} / retried {} / resolved {}）\n\
         定位范围：来源 {} 个、知识对象 {} 个、受影响文件 {} 个\n\
         失败定位：{} 条失败事件，其中未处理 {} 条\n\
         凭据检查：声明里没有疑似凭据字样（否则整包拒绝，不做过滤后继续）\n\
         模型调用与网络请求：0 次；本根只保存冻结声明与派生日志，不写项目、不写 Vault。\n",
        resolved.display(),
        back.manifest.policy,
        journal_model::JOURNAL_VERSION,
        short(&back.manifest.project_sha256),
        short(&back.manifest.inputs_sha256),
        back.manifest.created_at,
        back.inputs.tasks,
        back.inputs.events,
        counts.started,
        counts.failed,
        counts.retried,
        counts.resolved,
        back.inputs.sources,
        back.inputs.objects,
        back.inputs.files,
        counts.located_failures,
        counts.unresolved_failures
    ))
}

pub(crate) fn open(root: &Path) -> Result<Frozen, ProjectError> {
    let resolved = resolve(root)?;
    if !resolved.exists() {
        return Err(journal_model::unknown_root(format!(
            "{} 不存在，不是已建立的审计根；未修改任何文件。",
            resolved.display()
        )));
    }
    require_dir(&resolved, "审计根")?;
    let top: BTreeMap<String, PathBuf> = names(&resolved)
        .map_err(|_| {
            journal_model::unknown_root(format!(
                "{} 不是审计根（无法列出内容）；未修改任何文件。",
                resolved.display()
            ))
        })?
        .into_iter()
        .map(|name| (name.clone(), resolved.join(name)))
        .collect();
    if !top.contains_key(CONTROL) {
        return Err(journal_model::unknown_root(format!(
            "{} 不是审计根（缺少 control/）：陌生目录保持只读，未接管、未修改任何文件。",
            resolved.display()
        )));
    }
    if top.keys().cloned().collect::<Vec<_>>() != ROOT_ENTRIES.to_vec() {
        return Err(journal_model::integrity(format!(
            "审计根顶层只允许 control，实际是 {:?}。",
            top.keys().collect::<Vec<_>>()
        )));
    }
    let control = resolved.join(CONTROL);
    require_dir(&control, "控制目录")?;
    let entries = names(&control)?;
    let mut required_control: Vec<String> = CONTROL_ENTRIES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    required_control.sort();
    if entries != required_control {
        return Err(journal_model::integrity(format!(
            "控制目录必须恰好是 {CONTROL_ENTRIES:?}，实际是 {entries:?}。"
        )));
    }
    let bytes = read_file(&control.join(MANIFEST), MAX_BYTES, "审计根清单")?;
    let manifest: Manifest = strict(&bytes, "审计根清单")?;
    if manifest.policy != POLICY {
        return Err(journal_model::integrity(format!(
            "审计根清单的 policy 必须是 {POLICY}：收到 {}。",
            manifest.policy
        )));
    }
    if manifest.producer != APP_PRODUCER {
        return Err(journal_model::integrity(format!(
            "审计根清单的 producer 必须是 {APP_PRODUCER}：收到 {}。",
            manifest.producer
        )));
    }
    if manifest.schema_version != journal_model::SCHEMA_VERSION {
        return Err(journal_model::integrity(format!(
            "审计根清单的 schema_version 必须是 {}：收到 {}。",
            journal_model::SCHEMA_VERSION,
            manifest.schema_version
        )));
    }
    let canonical_root = fs::canonicalize(&resolved).map_err(|error| {
        journal_model::integrity(format!("无法确定审计根 {}：{error}。", resolved.display()))
    })?;
    if manifest.root != canonical_root.to_string_lossy() {
        return Err(journal_model::integrity(format!(
            "审计根清单登记的根是 {}，实际是 {}；这个根被移动过。",
            manifest.root,
            canonical_root.display()
        )));
    }
    check_identity(&canonical_root, &manifest.root_identity, "审计根")?;
    let parent = canonical_root
        .parent()
        .ok_or_else(|| journal_model::integrity("审计根必须有父目录。"))?;
    check_identity(parent, &manifest.parent_identity, "审计根的父目录")?;
    check_identity(&control, &manifest.control_identity, "控制目录")?;
    check_identity(
        &control.join(MANIFEST),
        &manifest.manifest_identity,
        "审计根清单",
    )?;
    check_identity(&control.join(INPUTS), &manifest.inputs_identity, "冻结输入")?;

    let project_bytes = read_file(&control.join(PROJECT), MAX_BYTES, "工程声明副本")?;
    if digest_of(&project_bytes) != manifest.project_sha256 {
        return Err(journal_model::integrity(
            "工程声明副本的摘要与清单不一致：冻结的输入已经被改动。",
        ));
    }
    let project_text = std::str::from_utf8(&project_bytes).map_err(|error| {
        journal_model::integrity(format!("工程声明副本不是合法 UTF-8：{error}。"))
    })?;
    let declaration = Declaration::parse(project_text)?;
    if declaration.case_id != manifest.case_id {
        return Err(journal_model::integrity(format!(
            "声明的 case_id 是 {}，清单登记的是 {}。",
            declaration.case_id, manifest.case_id
        )));
    }
    let inputs_bytes = read_file(&control.join(INPUTS), MAX_BYTES, "冻结输入")?;
    if digest_of(&inputs_bytes) != manifest.inputs_sha256 {
        return Err(journal_model::integrity(
            "冻结输入的摘要与清单不一致：冻结的输入已经被改动。",
        ));
    }
    let inputs: Inputs = strict(&inputs_bytes, "冻结输入")?;
    if inputs.case_id != declaration.case_id {
        return Err(journal_model::integrity(
            "冻结输入的 case_id 与声明不一致。",
        ));
    }
    if inputs.project_sha256 != manifest.project_sha256 {
        return Err(journal_model::integrity(
            "冻结输入登记的声明摘要与清单不一致。",
        ));
    }
    if inputs.tasks != declaration.tasks.len()
        || inputs.events != declaration.events.len()
        || inputs.sources != declaration.sources.len()
        || inputs.objects != declaration.objects.len()
        || inputs.files != declaration.files.len()
    {
        return Err(journal_model::integrity(
            "冻结输入的规模与声明不一致：这个根不是用这份声明建立的。",
        ));
    }
    Ok(Frozen {
        root: canonical_root,
        manifest,
        inputs,
        declaration,
    })
}

fn write_output(
    output: &Path,
    frozen: &Frozen,
    value: &journal_model::Journal,
) -> Result<(), ProjectError> {
    let journal_bytes = {
        let mut bytes = canonical(&value.journal, "事件日志")?;
        bytes.push(b'\n');
        bytes
    };
    let report_bytes = {
        let mut bytes = canonical(&value.report, "报告")?;
        bytes.push(b'\n');
        bytes
    };
    let markdown = journal_model::render_markdown(value);
    write_new(&output.join(JOURNAL), &journal_bytes, "事件日志")?;
    write_new(&output.join(REPORT), &report_bytes, "报告")?;
    write_new(&output.join(MARKDOWN), markdown.as_bytes(), "报告说明")?;
    let manifest = OutputManifest {
        policy: POLICY.to_owned(),
        producer: OUTPUT_PRODUCER.to_owned(),
        schema_version: journal_model::SCHEMA_VERSION,
        case_id: frozen.declaration.case_id.clone(),
        source_root: frozen.root.to_string_lossy().into_owned(),
        source_inputs_sha256: frozen.manifest.inputs_sha256.clone(),
        journal_sha256: digest_of(&journal_bytes),
        report_sha256: digest_of(&report_bytes),
        markdown_sha256: digest_of(markdown.as_bytes()),
        counts: value.report.counts.clone(),
    };
    let bytes = {
        let mut bytes = canonical(&manifest, "输出清单")?;
        bytes.push(b'\n');
        bytes
    };
    write_new(&output.join(OUTPUT_MANIFEST), &bytes, "输出清单")?;
    let entries = names(output)?;
    let mut expected: Vec<String> = OUTPUT_ENTRIES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    expected.sort();
    if entries != expected {
        return Err(journal_model::integrity(format!(
            "输出根必须恰好是 {expected:?}，实际是 {entries:?}。"
        )));
    }
    Ok(())
}

pub fn build(root: &Path, output: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let target = resolve(output)?;
    if target.exists() {
        return Err(journal_model::argument(format!(
            "输出根必须尚不存在：{}；请选择一个新目录。",
            target.display()
        )));
    }
    let parent = create_parent(&target, "输出根")?;
    require_dir(&parent, "输出根的父目录")?;
    let resolved = parent.join(
        target
            .file_name()
            .ok_or_else(|| journal_model::argument("输出根必须是一个目录名。"))?,
    );
    let frozen_root = fs::canonicalize(&frozen.root).unwrap_or_else(|_| frozen.root.clone());
    if resolved == frozen_root
        || resolved.starts_with(&frozen_root)
        || frozen_root.starts_with(&resolved)
    {
        return Err(journal_model::argument(format!(
            "输出根不能与审计根互相包含：{} 与 {}。",
            resolved.display(),
            frozen_root.display()
        )));
    }
    let value = frozen.journal()?;
    fs::create_dir(&resolved).map_err(|error| {
        journal_model::integrity(format!("无法建立输出根 {}：{error}。", resolved.display()))
    })?;
    if let Err(error) = write_output(&resolved, &frozen, &value) {
        let _ = fs::remove_dir_all(&resolved);
        return Err(error);
    }
    let counts = &value.report.counts;
    Ok(format!(
        "审计事件日志输出 {}：已生成（policy={}，日志版本 {}）\n\
         事件：{} 个任务、{} 条事件（started {} / failed {} / retried {} / resolved {}）\n\
         失败定位：{} 条失败事件，其中未处理 {} 条\n\
         文件：journal.json {}；report.json {}；report.md {}\n\
         模型调用与网络请求：0 次；输出根只含派生日志，不写项目、不写 Vault。\n",
        resolved.display(),
        POLICY,
        journal_model::JOURNAL_VERSION,
        counts.tasks,
        counts.events,
        counts.started,
        counts.failed,
        counts.retried,
        counts.resolved,
        counts.located_failures,
        counts.unresolved_failures,
        short(&digest_of(&{
            let mut bytes = canonical(&value.journal, "事件日志")?;
            bytes.push(b'\n');
            bytes
        })),
        short(&digest_of(&{
            let mut bytes = canonical(&value.report, "报告")?;
            bytes.push(b'\n');
            bytes
        })),
        short(&digest_of(
            journal_model::render_markdown(&value).as_bytes()
        ))
    ))
}

pub fn status(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let value = frozen.journal()?;
    let counts = &value.report.counts;
    let mut out = String::new();
    out.push_str(&format!(
        "审计事件日志 {}：已建立（policy={}，日志版本 {}）\n",
        frozen.root.display(),
        frozen.manifest.policy,
        journal_model::JOURNAL_VERSION
    ));
    out.push_str(&format!(
        "所有权：工程声明 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        short(&frozen.manifest.project_sha256),
        short(&frozen.manifest.inputs_sha256),
        frozen.manifest.created_at
    ));
    out.push_str(&format!(
        "冻结规模：{} 个任务、{} 条事件、{} 个来源、{} 个知识对象、{} 个受影响文件\n",
        counts.tasks, counts.events, counts.sources, counts.objects, counts.files
    ));
    out.push_str(&format!(
        "事件构成：started {} / failed {} / retried {} / resolved {}\n",
        counts.started, counts.failed, counts.retried, counts.resolved
    ));
    out.push_str(&format!(
        "失败定位：{} 条失败事件，其中未处理 {} 条\n",
        counts.located_failures, counts.unresolved_failures
    ));
    for task in &value.journal.tasks {
        out.push_str(&format!(
            "  - {}（{}）：{} 条事件、{} 条失败，当前结果 {}\n",
            task.task_id, task.title, task.events, task.failures, task.outcome
        ));
    }
    out.push_str("凭据检查：声明里没有疑似凭据字样\n");
    out.push_str("模型调用与网络请求：0 次；只读重算，未修改任何文件。\n");
    Ok(out)
}

pub fn list(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let value = frozen.journal()?;
    let mut payload: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    payload.insert(
        "policy".to_owned(),
        serde_json::Value::String(POLICY.to_owned()),
    );
    payload.insert(
        "producer".to_owned(),
        serde_json::Value::String(APP_PRODUCER.to_owned()),
    );
    payload.insert(
        "journal_version".to_owned(),
        serde_json::Value::Number(journal_model::JOURNAL_VERSION.into()),
    );
    payload.insert(
        "schema_version".to_owned(),
        serde_json::Value::Number(journal_model::SCHEMA_VERSION.into()),
    );
    payload.insert(
        "case_id".to_owned(),
        serde_json::Value::String(frozen.declaration.case_id.clone()),
    );
    payload.insert(
        "root".to_owned(),
        serde_json::Value::String(frozen.manifest.root.clone()),
    );
    payload.insert(
        "created_at".to_owned(),
        serde_json::Value::String(frozen.manifest.created_at.clone()),
    );
    payload.insert(
        "project_sha256".to_owned(),
        serde_json::Value::String(frozen.manifest.project_sha256.clone()),
    );
    payload.insert(
        "inputs_sha256".to_owned(),
        serde_json::Value::String(frozen.manifest.inputs_sha256.clone()),
    );
    payload.insert(
        "counts".to_owned(),
        serde_json::to_value(&value.report.counts)
            .map_err(|error| journal_model::integrity(format!("计数无法序列化：{error}。")))?,
    );
    payload.insert(
        "tasks".to_owned(),
        serde_json::to_value(&value.journal.tasks)
            .map_err(|error| journal_model::integrity(format!("任务记录无法序列化：{error}。")))?,
    );
    payload.insert(
        "failures".to_owned(),
        serde_json::to_value(&value.journal.failures)
            .map_err(|error| journal_model::integrity(format!("失败记录无法序列化：{error}。")))?,
    );
    payload.insert(
        "model_calls".to_owned(),
        serde_json::Value::Number(0u32.into()),
    );
    payload.insert(
        "network_requests".to_owned(),
        serde_json::Value::Number(0u32.into()),
    );
    serde_json::to_string_pretty(&payload)
        .map_err(|error| journal_model::integrity(format!("列表无法序列化：{error}。")))
}

// Keep the re-exported record types referenced so the module stays coherent if a
// future task projects them differently.
#[allow(dead_code)]
fn _record_types(
    _task: &TaskRecord,
    _event: &EventRecord,
    _failure: &FailureRecord,
    _file: &JournalFile,
    _report: &Report,
) {
}
