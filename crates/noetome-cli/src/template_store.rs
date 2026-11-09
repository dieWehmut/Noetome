//! 主题阅读目标与结构模板的冻结根与确定性输出（B12-T02）。
//!
//! 冻结根只允许 `control/`，里面是结构声明原文、冻结输入摘要与所有权清单。
//! 构建只读声明，不写笔记正文；输出根固定四个文件，同一冻结输入的两次构建
//! 必须字节相同。有结构性缺口时仍然写出完整报告，只把退出码置为非零：
//! 缺口必须逐条可见，藏进拒绝里就永远不会被列名。

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::compile_identity::{Identity, identity};
use crate::project::{self, ProjectError};
use crate::template_model::*;
use crate::template_model::{
    argument, duplicate, inconsistent, integrity, unexplained, unknown_root,
};
use crate::text;

pub(crate) const APP_PRODUCER: &str = "noetome-template";
pub(crate) const OUTPUT_PRODUCER: &str = "structure_template_output";
pub(crate) const CONTROL: &str = "control";
pub(crate) const MANIFEST: &str = "manifest.json";
pub(crate) const INPUTS: &str = "inputs.json";
pub(crate) const PROJECT: &str = "project.json";
pub(crate) const PLAN: &str = "plan.json";
pub(crate) const REPORT: &str = "report.json";
pub(crate) const MARKDOWN: &str = "report.md";
pub(crate) const OUTPUT_MANIFEST: &str = "manifest.json";
pub(crate) const ROOT_ENTRIES: [&str; 1] = ["control"];
pub(crate) const CONTROL_ENTRIES: [&str; 3] = ["inputs.json", "manifest.json", "project.json"];
pub(crate) const OUTPUT_ENTRIES: [&str; 4] =
    ["manifest.json", "plan.json", "report.json", "report.md"];

fn canonical<T: Serialize>(value: &T) -> Result<String, ProjectError> {
    serde_json::to_string(value).map_err(|error| integrity(format!("无法序列化记录：{error}。")))
}

fn strict<T>(bytes: &[u8], label: &str) -> Result<T, ProjectError>
where
    T: DeserializeOwned + Serialize,
{
    let text = std::str::from_utf8(bytes)
        .map_err(|error| integrity(format!("{label} 必须是 UTF-8 文本：{error}。")))?;
    let value: T = serde_json::from_str(text).map_err(|error| {
        integrity(format!(
            "{label} 无法解析（未知字段、缺字段或类型不符）：{error}。"
        ))
    })?;
    if canonical(&value)? != text {
        return Err(integrity(format!(
            "{label} 不是规范编码（存在未知字段、重复键或多余空白）：{text}"
        )));
    }
    Ok(value)
}

fn now() -> Result<String, ProjectError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| integrity(format!("无法生成时间戳：{error}。")))
}

fn read_file(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>, ProjectError> {
    let mut file = project::read_guard(path)
        .map_err(|error| integrity(format!("无法读取{label} {}：{error}。", path.display())))?;
    let mut bytes = Vec::new();
    let mut limited = (&mut file).take(limit + 1);
    limited
        .read_to_end(&mut bytes)
        .map_err(|error| integrity(format!("读取{label}失败：{error}。")))?;
    if bytes.len() as u64 > limit {
        return Err(integrity(format!(
            "{label} 超过 {limit} 字节上限：{}。",
            path.display()
        )));
    }
    Ok(bytes)
}

fn create_parent(path: &Path) -> Result<(), ProjectError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| integrity(format!("无法创建目录 {}：{error}。", parent.display())))?;
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8], label: &str) -> Result<(), ProjectError> {
    project::check_components(path)?;
    create_parent(path)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                argument(format!("{label}已存在，不会覆盖：{}。", path.display()))
            } else {
                integrity(format!("无法写入{label} {}：{error}。", path.display()))
            }
        })?;
    file.write_all(bytes)
        .map_err(|error| integrity(format!("写入{label}失败：{error}。")))?;
    file.sync_all()
        .map_err(|error| integrity(format!("同步{label}失败：{error}。")))?;
    let _readback_guard = crate::compile_identity::read_after_write(path, file)?;
    let readback = read_file(path, bytes.len() as u64, label)?;
    if readback != bytes {
        return Err(integrity(format!("写入{label}后读回不一致。")));
    }
    Ok(())
}

fn require_dir(path: &Path, label: &str) -> Result<(), ProjectError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| integrity(format!("无法访问{label} {}：{error}。", path.display())))?;
    if project::is_link(&metadata) {
        return Err(argument(format!(
            "{label} 不能是符号链接：{}。",
            path.display()
        )));
    }
    if !metadata.is_dir() {
        return Err(argument(format!("{label} 不是目录：{}。", path.display())));
    }
    Ok(())
}

fn names(dir: &Path) -> Result<BTreeMap<String, PathBuf>, ProjectError> {
    let mut out = BTreeMap::new();
    let entries = fs::read_dir(dir)
        .map_err(|error| integrity(format!("无法列出目录 {}：{error}。", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|error| integrity(format!("无法读取目录项：{error}。")))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| integrity(format!("目录 {} 里有非 UTF-8 文件名。", dir.display())))?;
        out.insert(name, entry.path());
    }
    Ok(out)
}

fn check_identity(path: &Path, expected: &Identity, label: &str) -> Result<(), ProjectError> {
    let actual = identity(path)?;
    if &actual != expected {
        return Err(integrity(format!(
            "{label} 的身份已变化：{}。",
            path.display()
        )));
    }
    Ok(())
}

fn resolve(path: &Path) -> Result<PathBuf, ProjectError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| integrity(format!("无法取得当前目录：{error}。")))?
            .join(path)
    };
    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err(ProjectError::unsafe_path(&absolute));
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

fn short(digest: &str) -> &str {
    digest.get(..16).unwrap_or(digest)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Inputs {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) project_sha256: String,
    pub(crate) templates: usize,
    pub(crate) topics: usize,
    pub(crate) goals: usize,
    pub(crate) concepts: usize,
    pub(crate) atoms: usize,
}

fn inputs_of(declaration: &Plan, project_sha256: &str) -> Inputs {
    Inputs {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: declaration.case_id.clone(),
        project_sha256: project_sha256.to_owned(),
        templates: declaration.templates.len(),
        topics: declaration.topics.len(),
        goals: declaration
            .topics
            .iter()
            .map(|topic| topic.goals.len())
            .sum(),
        concepts: declaration.concepts.len(),
        atoms: declaration.atoms.len(),
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub(crate) policy: String,
    pub(crate) producer: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) root: String,
    pub(crate) root_identity: Identity,
    pub(crate) parent_identity: Identity,
    pub(crate) control_identity: Identity,
    pub(crate) manifest_identity: Identity,
    pub(crate) inputs_identity: Identity,
    pub(crate) project_sha256: String,
    pub(crate) inputs_sha256: String,
    pub(crate) created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OutputManifest {
    pub(crate) policy: String,
    pub(crate) producer: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) source_root: String,
    pub(crate) source_inputs_sha256: String,
    pub(crate) plan_sha256: String,
    pub(crate) report_sha256: String,
    pub(crate) markdown_sha256: String,
    pub(crate) verdict: String,
    pub(crate) counts: Counts,
}

pub(crate) struct Frozen {
    pub(crate) root: PathBuf,
    pub(crate) manifest: Manifest,
    pub(crate) inputs: Inputs,
    pub(crate) declaration: Plan,
}

impl Frozen {
    pub(crate) fn plan(&self) -> Result<Planner, ProjectError> {
        plan(&self.declaration)
    }
}

pub fn adopt(project_path: &Path, root: &Path) -> Result<String, ProjectError> {
    let project_bytes = read_file(project_path, MAX_BYTES as u64, "结构声明")?;
    let project_text = std::str::from_utf8(&project_bytes)
        .map_err(|error| argument(format!("结构声明必须是UTF-8文本：{error}。")))?;
    let declaration = Plan::parse(project_text)?;
    // 先在内存里完整跑一遍规划：空章节、模板外章节缺理由、章节乱序都必须在建根之前报出来。
    let planner = plan(&declaration)?;
    let project_sha256 = text::sha256(&project_bytes);
    let inputs = inputs_of(&declaration, &project_sha256);
    let inputs_bytes = canonical(&inputs)?.into_bytes();

    let target = resolve(root)?;
    if fs::symlink_metadata(&target).is_ok() {
        return Err(argument(format!(
            "结构根必须尚不存在：{}；请选择一个新目录。",
            target.display()
        )));
    }
    let parent = target
        .parent()
        .ok_or_else(|| argument("结构根必须有父目录。".to_owned()))?
        .to_path_buf();
    require_dir(&parent, "结构根的父目录")?;
    let parent_identity = identity(&parent)?;
    fs::create_dir(&target)
        .map_err(|error| integrity(format!("无法创建结构根 {}：{error}。", target.display())))?;
    let root_identity = identity(&target)?;
    let outcome = adopt_into(AdoptInput {
        target: &target,
        project_bytes: &project_bytes,
        inputs_bytes: &inputs_bytes,
        declaration: &declaration,
        root_identity: &root_identity,
        parent_identity: &parent_identity,
        project_sha256,
    });
    match outcome {
        Ok(manifest) => Ok(adopt_summary(&manifest, &planner)),
        Err(error) => {
            if identity(&target).ok().as_ref() == Some(&root_identity) {
                let _ = fs::remove_dir_all(&target);
            }
            Err(error)
        }
    }
}

struct AdoptInput<'a> {
    target: &'a Path,
    project_bytes: &'a [u8],
    inputs_bytes: &'a [u8],
    declaration: &'a Plan,
    root_identity: &'a Identity,
    parent_identity: &'a Identity,
    project_sha256: String,
}

fn adopt_into(input: AdoptInput<'_>) -> Result<Manifest, ProjectError> {
    let root = fs::canonicalize(input.target)
        .map_err(|error| integrity(format!("无法定位结构根：{error}。")))?;
    let control = root.join(CONTROL);
    fs::create_dir(&control).map_err(|error| integrity(format!("无法创建控制目录：{error}。")))?;
    let control_identity = identity(&control)?;
    write_new(&control.join(PROJECT), input.project_bytes, "冻结结构声明")?;
    write_new(&control.join(INPUTS), input.inputs_bytes, "冻结输入摘要")?;
    write_new(&control.join(MANIFEST), b"", "冻结清单")?;
    let manifest_path = control.join(MANIFEST);
    let manifest_identity = identity(&manifest_path)?;
    let manifest = Manifest {
        policy: POLICY.to_owned(),
        producer: APP_PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: input.declaration.case_id.clone(),
        root: root.display().to_string(),
        root_identity: input.root_identity.clone(),
        parent_identity: input.parent_identity.clone(),
        control_identity,
        manifest_identity,
        inputs_identity: identity(&control.join(INPUTS))?,
        project_sha256: input.project_sha256,
        inputs_sha256: text::sha256(input.inputs_bytes),
        created_at: now()?,
    };
    let bytes = canonical(&manifest)?.into_bytes();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&manifest_path)
        .map_err(|error| integrity(format!("无法写入冻结清单：{error}。")))?;
    file.write_all(&bytes)
        .map_err(|error| integrity(format!("写入冻结清单失败：{error}。")))?;
    file.sync_all()
        .map_err(|error| integrity(format!("同步冻结清单失败：{error}。")))?;
    let _readback_guard = crate::compile_identity::read_after_write(&manifest_path, file)?;
    let readback = read_file(&manifest_path, MAX_BYTES as u64, "冻结清单")?;
    if readback != bytes {
        return Err(integrity(
            "写入冻结清单后读回不一致；保留现场并拒绝继续。".to_owned(),
        ));
    }
    Ok(manifest)
}

fn adopt_summary(manifest: &Manifest, planner: &Planner) -> String {
    let counts = &planner.report.counts;
    let mut out = String::new();
    out.push_str(&format!(
        "结构根 {}：已建立（policy={}，规划版本 {}）\n",
        manifest.root, manifest.policy, PLAN_VERSION
    ));
    out.push_str(&format!(
        "所有权：结构声明 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        short(&manifest.project_sha256),
        short(&manifest.inputs_sha256),
        manifest.created_at
    ));
    out.push_str(&format!(
        "冻结声明：{} 个模板、{} 个主题、{} 个阅读目标、{} 个概念、{} 个必要原子\n",
        counts.templates, counts.topics, counts.goals, counts.concepts, counts.atoms
    ));
    out.push_str(&format!(
        "结构规划：{} 份笔记、{} 个章节；主解释 {} 个、引用 {} 处、嵌入 {} 处\n",
        counts.notes, counts.sections, counts.explanations, counts.references, counts.embeds
    ));
    out.push_str(&format!(
        "模板符合性：模板外章节 {} 处、省略章节 {} 处；偏差合计 {} 处、缺口 {} 条\n",
        counts.special_sections, counts.omitted_sections, counts.deviations, counts.findings
    ));
    out.push_str(&format!(
        "结论：{}；模型调用与网络请求：0 次\n",
        verdict_text(&planner.report.verdict)
    ));
    out.push_str("结构根只保存冻结声明，不生成知识、不写正文、不写 Vault。\n");
    out
}

fn verdict_text(verdict: &str) -> &'static str {
    if verdict == "pass" {
        "通过（每个概念恰有一处完整解释、模板符合且同类结构一致）"
    } else {
        "存在缺口，已逐条列出（报告仍然完整）"
    }
}

pub(crate) fn open(root: &Path) -> Result<Frozen, ProjectError> {
    let target = resolve(root)?;
    let metadata = fs::symlink_metadata(&target).map_err(|_| {
        unknown_root(format!(
            "{} 不存在，不是已建立的结构根；未修改任何文件。",
            target.display()
        ))
    })?;
    if project::is_link(&metadata) || !metadata.is_dir() {
        return Err(unknown_root(format!(
            "{} 不是目录，不是已建立的结构根。",
            target.display()
        )));
    }
    let root = fs::canonicalize(&target)
        .map_err(|error| integrity(format!("无法定位结构根：{error}。")))?;
    let top = names(&root)?;
    if !top.contains_key(CONTROL) {
        return Err(unknown_root(format!(
            "{} 不是结构根（缺少 control/）：陌生目录保持只读，未接管、未修改任何文件。",
            root.display()
        )));
    }
    let top_names: Vec<String> = top.keys().cloned().collect();
    if top_names != ROOT_ENTRIES {
        return Err(integrity(format!(
            "结构根顶层只允许 control，实际是 {top_names:?}。"
        )));
    }
    let control = root.join(CONTROL);
    require_dir(&control, "控制目录")?;
    let entries = names(&control)?;
    let entry_names: Vec<String> = entries.keys().cloned().collect();
    if entry_names != CONTROL_ENTRIES {
        return Err(integrity(format!(
            "控制目录只允许 {CONTROL_ENTRIES:?}，实际是 {entry_names:?}。"
        )));
    }
    let manifest = strict::<Manifest>(
        &read_file(&control.join(MANIFEST), MAX_BYTES as u64, "冻结清单")?,
        "冻结清单",
    )?;
    if manifest.policy != POLICY
        || manifest.producer != APP_PRODUCER
        || manifest.schema_version != SCHEMA_VERSION
    {
        return Err(integrity(format!(
            "冻结清单声明的 policy/producer/schema_version 不是本模块的合同：{} / {} / {}。",
            manifest.policy, manifest.producer, manifest.schema_version
        )));
    }
    if Path::new(&manifest.root) != root {
        return Err(integrity(format!(
            "冻结清单记录的根是 {}，与当前路径 {} 不一致：根被移动过。",
            manifest.root,
            root.display()
        )));
    }
    check_identity(&root, &manifest.root_identity, "结构根")?;
    if let Some(parent) = root.parent() {
        check_identity(parent, &manifest.parent_identity, "结构根的父目录")?;
    }
    check_identity(&control, &manifest.control_identity, "控制目录")?;
    check_identity(
        &control.join(MANIFEST),
        &manifest.manifest_identity,
        "冻结清单",
    )?;
    check_identity(
        &control.join(INPUTS),
        &manifest.inputs_identity,
        "冻结输入摘要",
    )?;

    let project_bytes = read_file(&control.join(PROJECT), MAX_BYTES as u64, "冻结结构声明")?;
    if text::sha256(&project_bytes) != manifest.project_sha256 {
        return Err(integrity(
            "冻结结构声明的摘要与冻结清单不一致：声明被改过。".to_owned(),
        ));
    }
    let project_text = std::str::from_utf8(&project_bytes)
        .map_err(|error| integrity(format!("冻结结构声明必须是UTF-8文本：{error}。")))?;
    let declaration = Plan::parse(project_text)?;
    if declaration.case_id != manifest.case_id {
        return Err(integrity(format!(
            "冻结声明的 case_id {} 与清单记录的 {} 不一致。",
            declaration.case_id, manifest.case_id
        )));
    }
    let inputs_bytes = read_file(&control.join(INPUTS), MAX_BYTES as u64, "冻结输入摘要")?;
    if text::sha256(&inputs_bytes) != manifest.inputs_sha256 {
        return Err(integrity(
            "冻结输入摘要的摘要与冻结清单不一致：输入清单被改过。".to_owned(),
        ));
    }
    let inputs = strict::<Inputs>(&inputs_bytes, "冻结输入摘要")?;
    if inputs.policy != POLICY || inputs.case_id != manifest.case_id {
        return Err(integrity(format!(
            "冻结输入摘要的 policy/case_id 与清单不一致：{} / {}。",
            inputs.policy, inputs.case_id
        )));
    }
    if inputs.project_sha256 != manifest.project_sha256 {
        return Err(integrity(
            "冻结输入摘要记录的声明摘要与清单不一致。".to_owned(),
        ));
    }
    if (
        inputs.templates,
        inputs.topics,
        inputs.goals,
        inputs.concepts,
        inputs.atoms,
    ) != (
        declaration.templates.len(),
        declaration.topics.len(),
        declaration
            .topics
            .iter()
            .map(|topic| topic.goals.len())
            .sum::<usize>(),
        declaration.concepts.len(),
        declaration.atoms.len(),
    ) {
        return Err(integrity(
            "冻结输入摘要记录的规模与声明不一致：输入清单被改过。".to_owned(),
        ));
    }
    Ok(Frozen {
        root,
        manifest,
        inputs,
        declaration,
    })
}

pub fn build(root: &Path, output: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let planner = frozen.plan()?;
    let target = resolve(output)?;
    if fs::symlink_metadata(&target).is_ok() {
        return Err(argument(format!(
            "输出根必须尚不存在：{}；请选择一个新目录。",
            target.display()
        )));
    }
    if target.starts_with(&frozen.root) || frozen.root.starts_with(&target) {
        return Err(argument(
            "输出根不能与结构根互相包含：派生报告必须写在结构根之外。".to_owned(),
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| argument("输出根必须有父目录。".to_owned()))?
        .to_path_buf();
    require_dir(&parent, "输出根的父目录")?;
    fs::create_dir(&target)
        .map_err(|error| integrity(format!("无法创建输出根 {}：{error}。", target.display())))?;
    match write_output(&frozen, &planner, &target) {
        Ok(manifest) => {
            if manifest.verdict == "pass" {
                Ok(build_summary(&manifest, &target))
            } else {
                Err(finding_error(&planner, &manifest, &target))
            }
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&target);
            Err(error)
        }
    }
}

fn finding_error(planner: &Planner, manifest: &OutputManifest, target: &Path) -> ProjectError {
    let first = planner
        .report
        .findings
        .first()
        .map(|finding| finding.kind.clone())
        .unwrap_or_default();
    let make = match first.as_str() {
        FINDING_UNEXPLAINED => unexplained,
        FINDING_DUPLICATE => duplicate,
        _ => inconsistent,
    };
    let hint = if first == FINDING_INCONSISTENT {
        "同类同层级主题的章节序列必须一致，差异必须声明理由。"
    } else {
        "每个概念必须恰有一处完整解释。"
    };
    make(format!(
        "结构规划完成但存在 {} 条缺口，报告已写出：{}。结论：{}；{}",
        manifest.counts.findings,
        target.display(),
        verdict_text(&manifest.verdict),
        hint
    ))
}

fn write_output(
    frozen: &Frozen,
    planner: &Planner,
    target: &Path,
) -> Result<OutputManifest, ProjectError> {
    let plan_bytes = format!("{}\n", canonical(&planner.plan_file)?).into_bytes();
    let report_bytes = format!("{}\n", canonical(&planner.report)?).into_bytes();
    let markdown_bytes = render_markdown(planner).into_bytes();
    write_new(&target.join(PLAN), &plan_bytes, "规划结果")?;
    write_new(&target.join(REPORT), &report_bytes, "规划报告")?;
    write_new(&target.join(MARKDOWN), &markdown_bytes, "规划说明")?;
    let manifest = OutputManifest {
        policy: POLICY.to_owned(),
        producer: OUTPUT_PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: frozen.declaration.case_id.clone(),
        source_root: frozen.root.display().to_string(),
        source_inputs_sha256: frozen.manifest.inputs_sha256.clone(),
        plan_sha256: text::sha256(&plan_bytes),
        report_sha256: text::sha256(&report_bytes),
        markdown_sha256: text::sha256(&markdown_bytes),
        verdict: planner.report.verdict.clone(),
        counts: planner.report.counts.clone(),
    };
    let manifest_bytes = format!("{}\n", canonical(&manifest)?).into_bytes();
    write_new(&target.join(OUTPUT_MANIFEST), &manifest_bytes, "输出清单")?;
    let written: Vec<String> = names(target)?.into_keys().collect();
    let expected: Vec<String> = OUTPUT_ENTRIES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    if written != expected {
        return Err(integrity(format!(
            "输出根的文件集合与合同不符：实际 {written:?}，应为 {expected:?}。"
        )));
    }
    Ok(manifest)
}

fn build_summary(manifest: &OutputManifest, output: &Path) -> String {
    let counts = &manifest.counts;
    let mut out = String::new();
    out.push_str(&format!(
        "输出根 {}：已生成（policy={}，规划版本 {}）\n",
        output.display(),
        manifest.policy,
        PLAN_VERSION
    ));
    out.push_str(&format!("结构声明：{}\n", manifest.source_root));
    out.push_str(&format!(
        "结构规划：{} 份笔记、{} 个章节；主解释 {} 个、引用 {} 处、嵌入 {} 处\n",
        counts.notes, counts.sections, counts.explanations, counts.references, counts.embeds
    ));
    out.push_str(&format!(
        "模板符合性：模板外章节 {} 处、省略章节 {} 处；偏差合计 {} 处、缺口 {} 条\n",
        counts.special_sections, counts.omitted_sections, counts.deviations, counts.findings
    ));
    out.push_str(&format!(
        "文件：plan.json {}；report.json {}；report.md {}\n",
        short(&manifest.plan_sha256),
        short(&manifest.report_sha256),
        short(&manifest.markdown_sha256)
    ));
    out.push_str("模型调用与网络请求：0 次；输出根只含派生规划，不写项目、不写 Vault。\n");
    out
}

pub fn status(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let planner = frozen.plan()?;
    let counts = &planner.report.counts;
    let mut out = String::new();
    out.push_str(&format!(
        "结构根 {}：已建立（policy={}，规划版本 {}）\n",
        frozen.root.display(),
        frozen.manifest.policy,
        PLAN_VERSION
    ));
    out.push_str(&format!(
        "所有权：结构声明 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        short(&frozen.manifest.project_sha256),
        short(&frozen.manifest.inputs_sha256),
        frozen.manifest.created_at
    ));
    out.push_str(&format!(
        "冻结声明：{} 个模板、{} 个主题、{} 个阅读目标、{} 个概念、{} 个必要原子\n",
        counts.templates, counts.topics, counts.goals, counts.concepts, counts.atoms
    ));
    out.push_str(&format!(
        "结构规划：{} 份笔记、{} 个章节；主解释 {} 个、引用 {} 处、嵌入 {} 处\n",
        counts.notes, counts.sections, counts.explanations, counts.references, counts.embeds
    ));
    out.push_str(&format!(
        "模板符合性：模板外章节 {} 处、省略章节 {} 处；偏差合计 {} 处；缺口 {} 条\n",
        counts.special_sections, counts.omitted_sections, counts.deviations, counts.findings
    ));
    for goal in &planner.plan_file.goals {
        out.push_str(&format!(
            "  - {}：{} 个章节、{} 个原子、{} 处引用\n",
            goal.path,
            goal.sections.len(),
            goal.atoms.len(),
            goal.references.len()
        ));
    }
    out.push_str(&format!(
        "结论：{}；模型调用与网络请求：0 次（结构规划只读声明，不生成知识）。\n",
        verdict_text(&planner.report.verdict)
    ));
    Ok(out)
}

pub fn list(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let planner = frozen.plan()?;
    let mut out: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    out.insert("policy".to_owned(), POLICY.into());
    out.insert("producer".to_owned(), APP_PRODUCER.into());
    out.insert("plan_version".to_owned(), PLAN_VERSION.into());
    out.insert("schema_version".to_owned(), SCHEMA_VERSION.into());
    out.insert("case_id".to_owned(), frozen.manifest.case_id.clone().into());
    out.insert("root".to_owned(), frozen.manifest.root.clone().into());
    out.insert(
        "created_at".to_owned(),
        frozen.manifest.created_at.clone().into(),
    );
    out.insert(
        "project_sha256".to_owned(),
        frozen.manifest.project_sha256.clone().into(),
    );
    out.insert(
        "inputs_sha256".to_owned(),
        frozen.manifest.inputs_sha256.clone().into(),
    );
    out.insert("verdict".to_owned(), planner.report.verdict.clone().into());
    out.insert(
        "counts".to_owned(),
        serde_json::to_value(&planner.report.counts)
            .map_err(|error| integrity(format!("无法序列化计数：{error}。")))?,
    );
    out.insert(
        "topics".to_owned(),
        serde_json::to_value(&planner.plan_file.topics)
            .map_err(|error| integrity(format!("无法序列化主题：{error}。")))?,
    );
    out.insert(
        "goals".to_owned(),
        serde_json::to_value(&planner.plan_file.goals)
            .map_err(|error| integrity(format!("无法序列化目标：{error}。")))?,
    );
    out.insert(
        "explanations".to_owned(),
        serde_json::to_value(&planner.plan_file.explanations)
            .map_err(|error| integrity(format!("无法序列化解释位置：{error}。")))?,
    );
    out.insert(
        "deviations".to_owned(),
        serde_json::to_value(&planner.report.deviations)
            .map_err(|error| integrity(format!("无法序列化偏差：{error}。")))?,
    );
    out.insert(
        "findings".to_owned(),
        serde_json::to_value(&planner.report.findings)
            .map_err(|error| integrity(format!("无法序列化缺口：{error}。")))?,
    );
    serde_json::to_string_pretty(&out)
        .map_err(|error| integrity(format!("无法序列化清单：{error}。")))
}
