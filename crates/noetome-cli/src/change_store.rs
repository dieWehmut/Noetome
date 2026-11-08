//! Frozen root and output root for the locked-change planner.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::change_model::{self, Declaration, Planner};
use crate::compile_identity::{Identity, identity};
use crate::project::ProjectError;

const APP_PRODUCER: &str = "noetome-change";
const OUTPUT_PRODUCER: &str = "locked_change_output";
const CONTROL: &str = "control";
const MANIFEST: &str = "manifest.json";
const INPUTS: &str = "inputs.json";
const PROJECT: &str = "project.json";
const CHANGES: &str = "changes.json";
const REPORT: &str = "report.json";
const MARKDOWN: &str = "report.md";
const OUTPUT_MANIFEST: &str = "manifest.json";
const ROOT_ENTRIES: [&str; 1] = ["control"];
const CONTROL_ENTRIES: [&str; 3] = ["inputs.json", "manifest.json", "project.json"];
const OUTPUT_ENTRIES: [&str; 4] = ["changes.json", "manifest.json", "report.json", "report.md"];

fn canonical(value: &serde_json::Value) -> Result<String, ProjectError> {
    serde_json::to_string(value)
        .map_err(|error| change_model::integrity(format!("无法编码 JSON：{error}")))
}

fn strict<T>(bytes: &[u8], label: &str) -> Result<T, ProjectError>
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    let text = std::str::from_utf8(bytes)
        .map_err(|_| change_model::integrity(format!("{label} 不是合法 UTF-8。")))?;
    let value: T = serde_json::from_str(text)
        .map_err(|error| change_model::integrity(format!("{label} 不是合法 JSON：{error}")))?;
    let mut canonical_value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| change_model::integrity(format!("{label} 不是合法 JSON：{error}")))?;
    let _ = &mut canonical_value;
    let expected =
        canonical(&serde_json::from_str(text).map_err(|error| {
            change_model::integrity(format!("{label} 不是合法 JSON：{error}"))
        })?)?;
    if text != expected {
        return Err(change_model::integrity(format!("{label} 不是规范编码。")));
    }
    Ok(value)
}

fn now() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default();
    format!("{millis}")
}

fn read_file(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>, ProjectError> {
    let file = fs::File::open(path).map_err(|error| {
        change_model::integrity(format!("无法读取{label} {}：{error}", path.display()))
    })?;
    let mut buffer = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut buffer)
        .map_err(|error| {
            change_model::integrity(format!("无法读取{label} {}：{error}", path.display()))
        })?;
    if buffer.len() as u64 > limit {
        return Err(change_model::integrity(format!("{label} 超过大小上限。")));
    }
    Ok(buffer)
}

fn require_dir(path: &Path, label: &str) -> Result<(), ProjectError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        change_model::integrity(format!("无法访问{label} {}：{error}", path.display()))
    })?;
    if !metadata.is_dir() {
        return Err(change_model::integrity(format!(
            "{label} 不是目录：{}。",
            path.display()
        )));
    }
    Ok(())
}

fn names(path: &Path) -> BTreeMap<String, PathBuf> {
    let mut map = BTreeMap::new();
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            map.insert(
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
            );
        }
    }
    map
}

fn resolve(root: &Path, relative: &str) -> Result<PathBuf, ProjectError> {
    let mut path = root.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(value) => path.push(value),
            Component::CurDir => {}
            Component::ParentDir => {
                if !path.pop() || !path.starts_with(root) {
                    return Err(ProjectError::unsafe_path(Path::new(relative)));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ProjectError::unsafe_path(Path::new(relative)));
            }
        }
    }
    Ok(path)
}

fn write_new(path: &Path, bytes: &[u8], label: &str) -> Result<(), ProjectError> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            change_model::integrity(format!("无法写入{label} {}：{error}", path.display()))
        })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            change_model::integrity(format!("无法写入{label} {}：{error}", path.display()))
        })?;
    let readback = read_file(path, bytes.len() as u64 + 1, label)?;
    if readback != bytes {
        return Err(change_model::integrity(format!(
            "{label} 写入后回读不一致。"
        )));
    }
    Ok(())
}

fn create_parent(path: &Path, label: &str) -> Result<(), ProjectError> {
    if let Some(parent) = path.parent() {
        require_dir(parent, label)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteDigest {
    note_id: String,
    path: String,
    fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inputs {
    policy: String,
    schema_version: u32,
    case_id: String,
    project_sha256: String,
    notes: usize,
    changes: usize,
    locks: usize,
    refactors: usize,
    unchanged: Vec<NoteDigest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    policy: String,
    producer: String,
    schema_version: u32,
    case_id: String,
    root: String,
    root_identity: Identity,
    parent_identity: Identity,
    control_identity: Identity,
    manifest_identity: Identity,
    inputs_identity: Identity,
    project_sha256: String,
    inputs_sha256: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputManifest {
    policy: String,
    producer: String,
    schema_version: u32,
    case_id: String,
    source_root: String,
    source_inputs_sha256: String,
    changes_sha256: String,
    report_sha256: String,
    markdown_sha256: String,
    verdict: String,
    counts: change_model::Counts,
}

struct Frozen {
    root: PathBuf,
    manifest: Manifest,
    inputs: Inputs,
    declaration: Declaration,
}

impl Frozen {
    fn plan(&self) -> Result<Planner, ProjectError> {
        change_model::plan(&self.declaration)
    }
}

fn digest(bytes: &[u8]) -> String {
    crate::text::sha256(bytes)
}

fn inputs_of(declaration: &Declaration, planner: &Planner, project_sha256: &str) -> Inputs {
    Inputs {
        policy: change_model::POLICY.to_owned(),
        schema_version: change_model::SCHEMA_VERSION,
        case_id: declaration.case_id.clone(),
        project_sha256: project_sha256.to_owned(),
        notes: declaration.notes.len(),
        changes: declaration.changes.len(),
        locks: declaration.locks.len(),
        refactors: declaration.refactors.len(),
        unchanged: planner
            .change_file
            .unchanged
            .iter()
            .map(|item| NoteDigest {
                note_id: item.note_id.clone(),
                path: item.path.clone(),
                fingerprint: item.fingerprint.clone(),
            })
            .collect(),
    }
}

pub fn adopt(project_path: &Path, root: &Path) -> Result<String, ProjectError> {
    let parent = project_path
        .parent()
        .ok_or_else(|| change_model::argument("变更声明必须位于一个目录里。"))?;
    let parent = fs::canonicalize(parent)
        .map_err(|error| change_model::integrity(format!("无法访问声明目录：{error}")))?;
    let project_name = project_path
        .file_name()
        .ok_or_else(|| change_model::argument("变更声明必须是一个文件。"))?
        .to_owned();
    let project_file = parent.join(&project_name);
    let bytes = read_file(&project_file, change_model::MAX_BYTES, "变更声明")?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| change_model::argument("变更声明不是合法 UTF-8。"))?;
    let declaration = Declaration::parse(text)?;
    let planner = change_model::plan(&declaration)?;
    if root.exists() {
        return Err(change_model::argument(format!(
            "变更根必须尚不存在：{}；请选择一个新目录。",
            root.display()
        )));
    }
    let target_name = root
        .file_name()
        .ok_or_else(|| change_model::argument("变更根必须是一个目录名。"))?;
    let target_parent = root
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let target_parent = fs::canonicalize(&target_parent).map_err(|error| {
        change_model::integrity(format!(
            "无法访问变更根的父目录 {}：{error}",
            target_parent.display()
        ))
    })?;
    require_dir(&target_parent, "变更根的父目录")?;
    let target = target_parent.join(target_name);
    let parent_identity = identity(&target_parent)?;
    fs::create_dir(&target)
        .map_err(|error| change_model::integrity(format!("无法建立变更根：{error}")))?;
    let root_identity = identity(&target)?;
    match adopt_into(
        &target,
        &declaration,
        &planner,
        &bytes,
        &root_identity,
        &parent_identity,
    ) {
        Ok(summary) => Ok(summary),
        Err(error) => {
            let _ = fs::remove_dir_all(&target);
            Err(error)
        }
    }
}

fn adopt_into(
    root: &Path,
    declaration: &Declaration,
    planner: &Planner,
    project_bytes: &[u8],
    root_identity: &Identity,
    parent_identity: &Identity,
) -> Result<String, ProjectError> {
    let control = root.join(CONTROL);
    fs::create_dir(&control)
        .map_err(|error| change_model::integrity(format!("无法建立 control/：{error}")))?;
    write_new(&control.join(PROJECT), project_bytes, "变更声明")?;
    let project_sha256 = digest(project_bytes);
    let inputs = inputs_of(declaration, planner, &project_sha256);
    let inputs_bytes = canonical(
        &serde_json::to_value(&inputs)
            .map_err(|error| change_model::integrity(format!("无法编码输入摘要：{error}")))?,
    )?
    .into_bytes();
    write_new(&control.join(INPUTS), &inputs_bytes, "输入摘要")?;
    let control_identity = identity(&control)?;
    write_new(&control.join(MANIFEST), b"", "清单")?;
    let manifest = Manifest {
        policy: change_model::POLICY.to_owned(),
        producer: APP_PRODUCER.to_owned(),
        schema_version: change_model::SCHEMA_VERSION,
        case_id: declaration.case_id.clone(),
        root: root.display().to_string(),
        root_identity: root_identity.clone(),
        parent_identity: parent_identity.clone(),
        control_identity,
        manifest_identity: identity(&control.join(MANIFEST))?,
        inputs_identity: identity(&control.join(INPUTS))?,
        project_sha256,
        inputs_sha256: digest(&inputs_bytes),
        created_at: now(),
    };
    let manifest_bytes = canonical(
        &serde_json::to_value(&manifest)
            .map_err(|error| change_model::integrity(format!("无法编码清单：{error}")))?,
    )?
    .into_bytes();
    fs::write(control.join(MANIFEST), &manifest_bytes)
        .map_err(|error| change_model::integrity(format!("无法写入清单：{error}")))?;
    let written = read_file(&control.join(MANIFEST), change_model::MAX_BYTES, "清单")?;
    if written != manifest_bytes {
        return Err(change_model::integrity("清单写入后读回不一致。"));
    }
    Ok(summary_text(root, planner))
}

fn summary_text(root: &Path, planner: &Planner) -> String {
    let counts = &planner.report.counts;
    format!(
        "变更根 {}：已建立（policy={}，规划版本 {}）\n\
         笔记 {} 份、变更 {} 条、锁定 {} 条\n\
         已应用 {} 条、转候选 {} 条、重构 {} 条（引用变化 {} 处）\n\
         未受影响的笔记 {} 份\n\
         结论：{}\n\
         变更根只保存冻结声明，不写正文、不写 Vault；模型调用与网络请求 0 次。\n",
        root.display(),
        change_model::POLICY,
        change_model::PLAN_VERSION,
        counts.notes,
        counts.changes,
        counts.locks,
        counts.applied,
        counts.candidates,
        counts.refactors,
        counts.reference_changes,
        counts.unchanged,
        verdict_text(&planner.report.verdict)
    )
}

fn verdict_text(verdict: &str) -> &'static str {
    if verdict == "pass" {
        "通过（锁定结构保持、变更只影响声明范围）"
    } else {
        "存在缺口，已逐条列出"
    }
}

fn open(root: &Path) -> Result<Frozen, ProjectError> {
    let metadata = fs::symlink_metadata(root).map_err(|_| {
        change_model::unknown_root(format!(
            "{} 不存在，不是已建立的变更根；未修改任何文件。",
            root.display()
        ))
    })?;
    if !metadata.is_dir() {
        return Err(change_model::unknown_root(format!(
            "{} 不是目录，不是已建立的变更根。",
            root.display()
        )));
    }
    let top = names(root);
    if !top.contains_key(CONTROL) {
        return Err(change_model::unknown_root(format!(
            "{} 不是变更根（缺少 control/）：陌生目录保持只读，未接管、未修改任何文件。",
            root.display()
        )));
    }
    let mut top_names: Vec<&str> = top.keys().map(String::as_str).collect();
    top_names.sort_unstable();
    if top_names != ROOT_ENTRIES {
        return Err(change_model::integrity(format!(
            "变更根顶层只允许 control，实际是 {top_names:?}。"
        )));
    }
    let control = root.join(CONTROL);
    let entries = names(&control);
    let mut entry_names: Vec<&str> = entries.keys().map(String::as_str).collect();
    entry_names.sort_unstable();
    if entry_names != CONTROL_ENTRIES {
        return Err(change_model::integrity(format!(
            "control 目录只允许 inputs.json、manifest.json、project.json，实际是 {entry_names:?}。"
        )));
    }
    let manifest: Manifest = strict(
        &read_file(&control.join(MANIFEST), change_model::MAX_BYTES, "清单")?,
        "清单",
    )?;
    if manifest.policy != change_model::POLICY
        || manifest.producer != APP_PRODUCER
        || manifest.schema_version != change_model::SCHEMA_VERSION
    {
        return Err(change_model::integrity("清单的固定字段与合同不符。"));
    }
    if manifest.root != root.display().to_string() {
        return Err(change_model::integrity(format!(
            "清单记录的根是 {}，与当前路径不符。",
            manifest.root
        )));
    }
    for (expected, path, label) in [
        (&manifest.root_identity, root, "变更根"),
        (&manifest.control_identity, control.as_path(), "control/"),
        (
            &manifest.manifest_identity,
            control.join(MANIFEST).as_path(),
            "清单",
        ),
        (
            &manifest.inputs_identity,
            control.join(INPUTS).as_path(),
            "输入摘要",
        ),
    ] {
        if &identity(path)? != expected {
            return Err(change_model::integrity(format!(
                "{label} 的身份与清单不符：目录或文件已被替换。"
            )));
        }
    }
    if let Some(parent) = root.parent() {
        if &identity(parent)? != &manifest.parent_identity {
            return Err(change_model::integrity(
                "变更根的父目录身份与清单不符：根可能被整体搬到了别处。",
            ));
        }
    }
    let project_bytes = read_file(&control.join(PROJECT), change_model::MAX_BYTES, "变更声明")?;
    if digest(&project_bytes) != manifest.project_sha256 {
        return Err(change_model::integrity("变更声明与清单摘要不符。"));
    }
    let inputs_bytes = read_file(&control.join(INPUTS), change_model::MAX_BYTES, "输入摘要")?;
    if digest(&inputs_bytes) != manifest.inputs_sha256 {
        return Err(change_model::integrity("输入摘要与清单摘要不符。"));
    }
    let inputs: Inputs = strict(&inputs_bytes, "输入摘要")?;
    if inputs.policy != change_model::POLICY || inputs.case_id != manifest.case_id {
        return Err(change_model::integrity("输入摘要的固定字段与清单不符。"));
    }
    let text = std::str::from_utf8(&project_bytes)
        .map_err(|_| change_model::integrity("变更声明不是合法 UTF-8。"))?;
    let declaration = Declaration::parse(text)?;
    let planner = change_model::plan(&declaration)?;
    if inputs.notes != declaration.notes.len()
        || inputs.changes != declaration.changes.len()
        || inputs.locks != declaration.locks.len()
        || inputs.refactors != declaration.refactors.len()
    {
        return Err(change_model::integrity("输入摘要里的规模与声明不符。"));
    }
    if inputs.unchanged.len() != planner.change_file.unchanged.len()
        || inputs
            .unchanged
            .iter()
            .zip(planner.change_file.unchanged.iter())
            .any(|(left, right)| {
                left.note_id != right.note_id || left.fingerprint != right.fingerprint
            })
    {
        return Err(change_model::integrity(
            "输入摘要里的未受影响笔记与重新规划的结果不符：冻结的输入已经不能复现当时的结论。",
        ));
    }
    Ok(Frozen {
        root: root.to_path_buf(),
        manifest,
        inputs,
        declaration,
    })
}

pub fn build(root: &Path, output: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let planner = frozen.plan()?;
    if output.exists() {
        return Err(change_model::argument(format!(
            "输出根必须尚不存在：{}；请选择一个新目录。",
            output.display()
        )));
    }
    let output_name = output
        .file_name()
        .ok_or_else(|| change_model::argument("输出根必须是一个目录名。"))?;
    let output_parent = output
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let parent = fs::canonicalize(&output_parent).map_err(|error| {
        change_model::integrity(format!(
            "无法访问输出根的父目录 {}：{error}",
            output_parent.display()
        ))
    })?;
    require_dir(&parent, "输出根的父目录")?;
    let output = parent.join(output_name);
    let source = fs::canonicalize(&frozen.root)
        .map_err(|error| change_model::integrity(format!("无法解析变更根：{error}")))?;
    if output.starts_with(&source) || source.starts_with(&output) {
        return Err(change_model::argument(
            "输出根不能与变更根互相包含：派生文件必须放在冻结输入之外。",
        ));
    }
    fs::create_dir(&output)
        .map_err(|error| change_model::integrity(format!("无法建立输出根：{error}")))?;
    let summary = match write_output(&output, &frozen, &planner) {
        Ok(summary) => summary,
        Err(error) => {
            let _ = fs::remove_dir_all(&output);
            return Err(error);
        }
    };
    if planner.report.verdict == "pass" {
        Ok(summary)
    } else {
        Err(ProjectError::new(
            "CHANGE_FINDINGS",
            3,
            format!(
                "变更规划完成但存在 {} 条缺口，报告已写出：{}。结论：存在缺口，已逐条列出（报告仍然完整）。",
                planner.report.counts.findings,
                output.display()
            ),
        ))
    }
}

fn write_output(output: &Path, frozen: &Frozen, planner: &Planner) -> Result<String, ProjectError> {
    let changes_bytes = format!(
        "{}\n",
        canonical(
            &serde_json::to_value(&planner.change_file).map_err(|error| {
                change_model::integrity(format!("无法编码变更清单：{error}"))
            })?
        )?
    )
    .into_bytes();
    let report_bytes = format!(
        "{}\n",
        canonical(&serde_json::to_value(&planner.report).map_err(|error| {
            change_model::integrity(format!("无法编码报告：{error}"))
        })?)?
    )
    .into_bytes();
    let markdown_bytes = change_model::render_markdown(planner).into_bytes();
    write_new(&output.join(CHANGES), &changes_bytes, "变更清单")?;
    write_new(&output.join(REPORT), &report_bytes, "报告")?;
    write_new(&output.join(MARKDOWN), &markdown_bytes, "报告正文")?;
    let manifest = OutputManifest {
        policy: change_model::POLICY.to_owned(),
        producer: OUTPUT_PRODUCER.to_owned(),
        schema_version: change_model::SCHEMA_VERSION,
        case_id: planner.report.case_id.clone(),
        source_root: frozen.root.display().to_string(),
        source_inputs_sha256: frozen.manifest.inputs_sha256.clone(),
        changes_sha256: digest(&changes_bytes),
        report_sha256: digest(&report_bytes),
        markdown_sha256: digest(&markdown_bytes),
        verdict: planner.report.verdict.clone(),
        counts: planner.report.counts.clone(),
    };
    let manifest_bytes = format!(
        "{}\n",
        canonical(&serde_json::to_value(&manifest).map_err(|error| {
            change_model::integrity(format!("无法编码输出清单：{error}"))
        })?)?
    )
    .into_bytes();
    write_new(&output.join(OUTPUT_MANIFEST), &manifest_bytes, "输出清单")?;
    let entries = names(output);
    let mut entry_names: Vec<&str> = entries.keys().map(String::as_str).collect();
    entry_names.sort_unstable();
    if entry_names != OUTPUT_ENTRIES {
        return Err(change_model::integrity(format!(
            "输出根只允许 changes.json、manifest.json、report.json、report.md，实际是 {entry_names:?}。"
        )));
    }
    let counts = &planner.report.counts;
    Ok(format!(
        "输出根 {}：已生成（policy={}，规划版本 {}）\n\
         已应用 {} 条、转候选 {} 条、重构 {} 条（引用变化 {} 处）\n\
         未受影响的笔记 {} 份；缺口 {} 条\n\
         结论：{}\n\
         文件：changes.json {}；report.json {}；report.md {}\n\
         模型调用与网络请求 0 次；输出根只含派生规划，不写项目、不写 Vault。\n",
        output.display(),
        change_model::POLICY,
        change_model::PLAN_VERSION,
        counts.applied,
        counts.candidates,
        counts.refactors,
        counts.reference_changes,
        counts.unchanged,
        counts.findings,
        verdict_text(&planner.report.verdict),
        &digest(&changes_bytes)[..16],
        &digest(&report_bytes)[..16],
        &digest(&markdown_bytes)[..16]
    ))
}

pub fn status(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let planner = frozen.plan()?;
    let counts = &planner.report.counts;
    let mut out = String::new();
    out.push_str(&format!(
        "变更根 {}：已建立（policy={}，规划版本 {}）\n",
        frozen.root.display(),
        change_model::POLICY,
        change_model::PLAN_VERSION
    ));
    out.push_str(&format!(
        "所有权：声明 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        &frozen.manifest.project_sha256[..16],
        &frozen.manifest.inputs_sha256[..16],
        frozen.manifest.created_at
    ));
    out.push_str(&format!(
        "冻结声明：{} 份笔记、{} 条变更、{} 条锁定、{} 条重构\n",
        counts.notes, counts.changes, counts.locks, counts.refactors
    ));
    out.push_str(&format!(
        "变更结果：已应用 {} 条、转候选 {} 条、重构 {} 条（引用变化 {} 处）\n",
        counts.applied, counts.candidates, counts.refactors, counts.reference_changes
    ));
    out.push_str(&format!("未受影响的笔记：{} 份\n", counts.unchanged));
    for item in &planner.change_file.unchanged {
        out.push_str(&format!(
            "  - {}（{}）：指纹 {}\n",
            item.note_id,
            item.path,
            &item.fingerprint[..16]
        ));
    }
    out.push_str(&format!(
        "结论：{}；缺口 {} 条\n",
        verdict_text(&planner.report.verdict),
        counts.findings
    ));
    out.push_str(&format!("冻结输入摘要：{} 份笔记\n", frozen.inputs.notes));
    Ok(out)
}

pub fn list(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let planner = frozen.plan()?;
    let mut value = BTreeMap::new();
    value.insert("policy".to_owned(), serde_json::json!(change_model::POLICY));
    value.insert("producer".to_owned(), serde_json::json!(APP_PRODUCER));
    value.insert(
        "schema_version".to_owned(),
        serde_json::json!(change_model::SCHEMA_VERSION),
    );
    value.insert(
        "case_id".to_owned(),
        serde_json::json!(frozen.declaration.case_id),
    );
    value.insert(
        "root".to_owned(),
        serde_json::json!(frozen.root.display().to_string()),
    );
    value.insert(
        "created_at".to_owned(),
        serde_json::json!(frozen.manifest.created_at),
    );
    value.insert(
        "project_sha256".to_owned(),
        serde_json::json!(frozen.manifest.project_sha256),
    );
    value.insert(
        "inputs_sha256".to_owned(),
        serde_json::json!(frozen.manifest.inputs_sha256),
    );
    value.insert(
        "verdict".to_owned(),
        serde_json::json!(planner.report.verdict),
    );
    value.insert(
        "counts".to_owned(),
        serde_json::json!(planner.report.counts),
    );
    value.insert(
        "applied".to_owned(),
        serde_json::json!(planner.change_file.applied),
    );
    value.insert(
        "candidates".to_owned(),
        serde_json::json!(planner.change_file.candidates),
    );
    value.insert(
        "refactors".to_owned(),
        serde_json::json!(planner.change_file.refactors),
    );
    value.insert(
        "unchanged".to_owned(),
        serde_json::json!(planner.change_file.unchanged),
    );
    value.insert(
        "findings".to_owned(),
        serde_json::json!(planner.report.findings),
    );
    serde_json::to_string_pretty(&value)
        .map_err(|error| change_model::integrity(format!("无法编码列表：{error}")))
}
