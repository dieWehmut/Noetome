//! Freeze a diagnostics declaration, then export the selected categories.
//!
//! `adopt` evaluates the whole export first: a declaration that cannot produce an
//! export never becomes a root. `build` writes exactly the files the manifest lists
//! and refuses an output root that already exists. The output manifest carries no
//! timestamp, so two exports of the same frozen declaration are byte-identical.

use crate::compile_identity::{Identity, identity};
use crate::diagnostics_model::{self, Declaration, Export};
use crate::project::ProjectError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const APP_PRODUCER: &str = "noetome-diagnostics";
pub const OUTPUT_PRODUCER: &str = "diagnostics_export_output";

const CONTROL: &str = "control";
const MANIFEST: &str = "manifest.json";
const INPUTS: &str = "inputs.json";
const PROJECT: &str = "project.json";
const EXPORT: &str = "export.json";
const REPORT: &str = "report.json";
const MARKDOWN: &str = "report.md";
const OUTPUT_MANIFEST: &str = "manifest.json";

const ROOT_ENTRIES: [&str; 1] = [CONTROL];
const CONTROL_ENTRIES: [&str; 3] = [INPUTS, MANIFEST, PROJECT];
const OUTPUT_ENTRIES: [&str; 4] = [EXPORT, MANIFEST, MARKDOWN, REPORT];

fn canonical<T: Serialize>(value: &T, label: &str) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec(value).map_err(|error| {
        diagnostics_model::integrity(format!("{label}无法编码为规范 JSON：{error}"))
    })
}

fn strict<T: DeserializeOwned + Serialize>(bytes: &[u8], label: &str) -> Result<T, ProjectError> {
    let value: T = serde_json::from_slice(bytes)
        .map_err(|error| diagnostics_model::integrity(format!("{label}不是合法 JSON：{error}")))?;
    let expected = canonical(&value, label)?;
    let mut actual = bytes;
    if actual.last() == Some(&b'\n') {
        actual = &actual[..actual.len() - 1];
    }
    if actual != expected.as_slice() {
        return Err(diagnostics_model::integrity(format!(
            "{label}不是规范编码：拒绝把一份重新排版过的文件当成冻结内容。"
        )));
    }
    Ok(value)
}

fn now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

fn read_file(path: &Path, limit: usize, label: &str) -> Result<Vec<u8>, ProjectError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        diagnostics_model::integrity(format!("无法读取{label} {}：{error}", path.display()))
    })?;
    if !metadata.file_type().is_file() {
        return Err(diagnostics_model::integrity(format!(
            "{label} {} 不是普通文件。",
            path.display()
        )));
    }
    if metadata.len() > limit as u64 {
        return Err(diagnostics_model::integrity(format!(
            "{label} {} 有 {} 字节，超过上限 {limit} 字节。",
            path.display(),
            metadata.len()
        )));
    }
    let mut file = fs::File::open(path).map_err(|error| {
        diagnostics_model::integrity(format!("无法打开{label} {}：{error}", path.display()))
    })?;
    let mut buffer = Vec::new();
    (&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut buffer)
        .map_err(|error| {
            diagnostics_model::integrity(format!("无法读取{label} {}：{error}", path.display()))
        })?;
    Ok(buffer)
}

fn create_parent(path: &Path, label: &str) -> Result<PathBuf, ProjectError> {
    let parent = path.parent().ok_or_else(|| {
        diagnostics_model::argument(format!("{label}必须是一个目录名：{}。", path.display()))
    })?;
    fs::canonicalize(parent).map_err(|error| {
        diagnostics_model::integrity(format!(
            "无法访问{label}的父目录 {}：{error}",
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
            diagnostics_model::integrity(format!("无法写入{label} {}：{error}", path.display()))
        })?;
    file.write_all(bytes).map_err(|error| {
        diagnostics_model::integrity(format!("无法写入{label} {}：{error}", path.display()))
    })?;
    file.sync_all().map_err(|error| {
        diagnostics_model::integrity(format!("无法落盘{label} {}：{error}", path.display()))
    })?;
    let written = read_file(path, bytes.len().max(1), label)?;
    if written != bytes {
        return Err(diagnostics_model::integrity(format!(
            "{label} {} 读回的内容与写入不一致。",
            path.display()
        )));
    }
    Ok(())
}

fn require_dir(path: &Path, label: &str) -> Result<(), ProjectError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        diagnostics_model::integrity(format!("无法访问{label} {}：{error}", path.display()))
    })?;
    if !metadata.file_type().is_dir() {
        return Err(diagnostics_model::integrity(format!(
            "{label} {} 不是目录。",
            path.display()
        )));
    }
    Ok(())
}

fn names(path: &Path, label: &str) -> Result<Vec<String>, ProjectError> {
    let mut out = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| {
        diagnostics_model::integrity(format!("无法列出{label} {}：{error}", path.display()))
    })? {
        let entry = entry.map_err(|error| {
            diagnostics_model::integrity(format!("无法列出{label} {}：{error}", path.display()))
        })?;
        out.push(entry.file_name().to_string_lossy().into_owned());
    }
    out.sort();
    Ok(out)
}

fn check_identity(path: &Path, expected: &Identity, label: &str) -> Result<(), ProjectError> {
    let current = identity(path).map_err(|error| {
        diagnostics_model::integrity(format!("无法读取{label}的身份：{}。", error.code()))
    })?;
    if current != *expected {
        return Err(diagnostics_model::integrity(format!(
            "{label}的身份变了：冻结时是 {expected:?}，现在是 {current:?}。"
        )));
    }
    Ok(())
}

fn short(digest: &str) -> String {
    digest.chars().take(16).collect()
}

/// Lexically normalize a path without touching the filesystem.
///
/// The split must go through `Path::components`, not through a `/` split on the
/// string: a `/` split drops the leading empty segment and silently turns an
/// absolute path into a relative one, which then resolves against the working
/// directory instead of the root. Keeping `RootDir` intact is the whole point.
fn resolve(path: &Path) -> Result<PathBuf, ProjectError> {
    if path.as_os_str().is_empty() {
        return Err(diagnostics_model::argument("路径不能为空。"));
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err(ProjectError::unsafe_path(path));
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        return Err(ProjectError::unsafe_path(path));
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inputs {
    policy: String,
    schema_version: u32,
    case_id: String,
    project_sha256: String,
    categories: usize,
    entries: usize,
    selected_categories: usize,
    config: usize,
    fingerprints: usize,
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
    export_sha256: String,
    report_sha256: String,
    markdown_sha256: String,
    counts: diagnostics_model::Counts,
}

pub(crate) struct Frozen {
    root: PathBuf,
    manifest: Manifest,
    declaration: Declaration,
    inputs: Inputs,
}

impl Frozen {
    fn export(&self) -> Result<Export, ProjectError> {
        diagnostics_model::export(&self.declaration)
    }
}

fn inputs_of(declaration: &Declaration, digest: &str) -> Result<Inputs, ProjectError> {
    let export = diagnostics_model::export(declaration)?;
    Ok(Inputs {
        policy: diagnostics_model::POLICY.to_owned(),
        schema_version: diagnostics_model::SCHEMA_VERSION,
        case_id: declaration.case_id.clone(),
        project_sha256: digest.to_owned(),
        categories: export.report.counts.categories,
        entries: export.report.counts.entries,
        selected_categories: export.report.counts.selected_categories,
        config: export.report.counts.config,
        fingerprints: export.report.counts.fingerprints,
    })
}

fn adopt_into(
    declaration: &Declaration,
    root: &Path,
    root_identity: Identity,
    parent_identity: Identity,
    project_bytes: &[u8],
    inputs: &Inputs,
) -> Result<String, ProjectError> {
    let control = root.join(CONTROL);
    fs::create_dir(&control).map_err(|error| {
        diagnostics_model::integrity(format!("无法建立控制目录 {}：{error}", control.display()))
    })?;

    write_new(&control.join(PROJECT), project_bytes, "工程声明副本")?;
    let inputs_bytes = canonical(inputs, "冻结输入")?;
    write_new(&control.join(INPUTS), &inputs_bytes, "冻结输入")?;

    // The manifest is written last: a half-built root must not advertise itself.
    let manifest_path = control.join(MANIFEST);
    write_new(&manifest_path, b"", "诊断根清单")?;
    let control_identity = identity(&control).map_err(|error| {
        diagnostics_model::integrity(format!("无法读取控制目录身份：{}。", error.code()))
    })?;
    let manifest_identity = identity(&manifest_path).map_err(|error| {
        diagnostics_model::integrity(format!("无法读取清单身份：{}。", error.code()))
    })?;
    let inputs_identity = identity(&control.join(INPUTS)).map_err(|error| {
        diagnostics_model::integrity(format!("无法读取冻结输入身份：{}。", error.code()))
    })?;

    let manifest = Manifest {
        policy: diagnostics_model::POLICY.to_owned(),
        producer: APP_PRODUCER.to_owned(),
        schema_version: diagnostics_model::SCHEMA_VERSION,
        case_id: declaration.case_id.clone(),
        root: root.to_string_lossy().into_owned(),
        root_identity,
        parent_identity,
        control_identity,
        manifest_identity,
        inputs_identity,
        project_sha256: crate::text::sha256(project_bytes),
        inputs_sha256: crate::text::sha256(&inputs_bytes),
        created_at: now(),
    };
    let manifest_bytes = canonical(&manifest, "诊断根清单")?;
    fs::write(&manifest_path, &manifest_bytes)
        .map_err(|error| diagnostics_model::integrity(format!("无法写入诊断根清单：{error}")))?;
    let written = read_file(&manifest_path, diagnostics_model::MAX_BYTES, "诊断根清单")?;
    if written != manifest_bytes {
        return Err(diagnostics_model::integrity(
            "诊断根清单读回的内容与写入不一致。".to_owned(),
        ));
    }
    Ok(short(&manifest.project_sha256))
}

pub fn adopt(project: &Path, root: &Path) -> Result<String, ProjectError> {
    let project = resolve(project)?;
    let parent = create_parent(root, "诊断根")?;
    let root = parent.join(root.file_name().ok_or_else(|| {
        diagnostics_model::argument(format!("诊断根必须是一个目录名：{}。", root.display()))
    })?);
    if root.exists() {
        return Err(diagnostics_model::argument(format!(
            "诊断根必须尚不存在：{}；请选择一个新目录。",
            root.display()
        )));
    }
    let bytes = read_file(&project, diagnostics_model::MAX_BYTES, "诊断声明")?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| diagnostics_model::argument(format!("诊断声明不是合法 UTF-8：{error}")))?;
    let declaration = Declaration::parse(text)?;

    // Evaluate the export before building anything: a declaration that cannot be
    // exported must not leave a root behind.
    let digest = crate::text::sha256(&bytes);
    let inputs = inputs_of(&declaration, &digest)?;

    require_dir(&parent, "诊断根的父目录")?;
    let parent_identity = identity(&parent).map_err(|error| {
        diagnostics_model::integrity(format!("无法读取父目录身份：{}。", error.code()))
    })?;

    fs::create_dir(&root).map_err(|error| {
        diagnostics_model::integrity(format!("无法建立诊断根 {}：{error}", root.display()))
    })?;
    let root_identity = identity(&root).map_err(|error| {
        diagnostics_model::integrity(format!("无法读取诊断根身份：{}。", error.code()))
    })?;

    let outcome = adopt_into(
        &declaration,
        &root,
        root_identity.clone(),
        parent_identity,
        &bytes,
        &inputs,
    );
    match outcome {
        Ok(_) => {}
        Err(error) => {
            let keep = match identity(&root) {
                Ok(current) => current != root_identity,
                Err(_) => false,
            };
            if !keep {
                let _ = fs::remove_dir_all(&root);
            }
            return Err(error);
        }
    }

    let frozen = open(&root)?;
    if frozen.manifest.case_id != declaration.case_id {
        return Err(diagnostics_model::integrity(
            "冻结的案例编号与声明不一致。".to_owned(),
        ));
    }
    let export = frozen.export()?;
    let counts = &export.report.counts;
    let mut out = String::new();
    out.push_str(&format!(
        "诊断导出 {}：已建立（policy={}，导出格式版本 {}）\n",
        root.display(),
        diagnostics_model::POLICY,
        diagnostics_model::EXPORT_VERSION
    ));
    out.push_str(&format!(
        "所有权：工程声明 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        short(&frozen.manifest.project_sha256),
        short(&frozen.manifest.inputs_sha256),
        frozen.manifest.created_at
    ));
    out.push_str(&format!(
        "导出范围：范围类别 {} 个，已勾选 {} 个、未勾选 {} 个\n",
        counts.categories, counts.selected_categories, counts.excluded_categories
    ));
    out.push_str(&format!(
        "条目与字节：条目 {} 条（已导出 {} 条），已导出 {} 字节、未导出 {} 字节\n",
        counts.entries, counts.selected_entries, counts.exported_bytes, counts.excluded_bytes
    ));
    out.push_str(&format!(
        "敏感范围：{} 个类别，其中已导出 {} 个{}\n",
        counts.sensitive_categories,
        counts.sensitive_included,
        if counts.sensitive_included == 0 {
            "（默认不含原文与来源正文）"
        } else {
            "（已记录同意）"
        }
    ));
    out.push_str(&format!(
        "复现信息：配置 {} 项、输入指纹 {} 个\n",
        counts.config, counts.fingerprints
    ));
    out.push_str("检查：声明里没有绝对路径与疑似凭据字样（否则整包拒绝，不写出任何字节）\n");
    out.push_str(
        "模型调用与网络请求：0 次；本根只保存冻结声明与派生诊断包，不写项目、不写 Vault。\n",
    );
    Ok(out)
}

fn open(root: &Path) -> Result<Frozen, ProjectError> {
    if !root.exists() {
        return Err(diagnostics_model::unknown_root(format!(
            "{} 不存在，不是已建立的诊断根；未修改任何文件。",
            root.display()
        )));
    }
    require_dir(root, "诊断根")?;
    let root = fs::canonicalize(root).map_err(|error| {
        diagnostics_model::integrity(format!("无法解析诊断根 {}：{error}", root.display()))
    })?;

    let top = names(&root, "诊断根")?;
    if top != ROOT_ENTRIES {
        if top.is_empty() {
            return Err(diagnostics_model::unknown_root(format!(
                "{} 不是诊断根（缺少 control/）：陌生目录保持只读。",
                root.display()
            )));
        }
        return Err(diagnostics_model::integrity(format!(
            "诊断根顶层只允许 control，实际是 {top:?}。"
        )));
    }

    let control = root.join(CONTROL);
    let entries = names(&control, "控制目录")?;
    let mut required: Vec<String> = CONTROL_ENTRIES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    required.sort();
    if entries != required {
        return Err(diagnostics_model::integrity(format!(
            "控制目录必须恰好是 {required:?}，实际是 {entries:?}。"
        )));
    }

    let manifest_bytes = read_file(
        &control.join(MANIFEST),
        diagnostics_model::MAX_BYTES,
        "诊断根清单",
    )?;
    let manifest: Manifest = strict(&manifest_bytes, "诊断根清单")?;
    if manifest.policy != diagnostics_model::POLICY {
        return Err(diagnostics_model::integrity(format!(
            "清单策略必须是 {}：收到「{}」。",
            diagnostics_model::POLICY,
            manifest.policy
        )));
    }
    if manifest.producer != APP_PRODUCER {
        return Err(diagnostics_model::integrity(format!(
            "清单生产者必须是 {APP_PRODUCER}：收到「{}」。",
            manifest.producer
        )));
    }
    if manifest.schema_version != diagnostics_model::SCHEMA_VERSION {
        return Err(diagnostics_model::integrity(format!(
            "清单结构版本必须是 {}：收到 {}。",
            diagnostics_model::SCHEMA_VERSION,
            manifest.schema_version
        )));
    }
    if Path::new(&manifest.root) != root {
        return Err(diagnostics_model::integrity(format!(
            "清单记录的根是 {}，实际是 {}。",
            manifest.root,
            root.display()
        )));
    }
    check_identity(&root, &manifest.root_identity, "诊断根")?;
    check_identity(&control, &manifest.control_identity, "控制目录")?;
    check_identity(
        &control.join(MANIFEST),
        &manifest.manifest_identity,
        "诊断根清单",
    )?;
    check_identity(&control.join(INPUTS), &manifest.inputs_identity, "冻结输入")?;
    if let Some(parent) = root.parent() {
        check_identity(parent, &manifest.parent_identity, "诊断根的父目录")?;
    }

    let project_path = control.join(PROJECT);
    let project_bytes = read_file(&project_path, diagnostics_model::MAX_BYTES, "工程声明副本")?;
    if crate::text::sha256(&project_bytes) != manifest.project_sha256 {
        return Err(diagnostics_model::integrity(
            "工程声明副本的内容与清单记录不一致：冻结内容被改过。".to_owned(),
        ));
    }
    let text = std::str::from_utf8(&project_bytes).map_err(|error| {
        diagnostics_model::integrity(format!("工程声明副本不是合法 UTF-8：{error}"))
    })?;
    let declaration = Declaration::parse(text)?;
    if declaration.case_id != manifest.case_id {
        return Err(diagnostics_model::integrity(format!(
            "清单案例编号 {} 与声明 {} 不一致。",
            manifest.case_id, declaration.case_id
        )));
    }

    let inputs_bytes = read_file(
        &control.join(INPUTS),
        diagnostics_model::MAX_BYTES,
        "冻结输入",
    )?;
    if crate::text::sha256(&inputs_bytes) != manifest.inputs_sha256 {
        return Err(diagnostics_model::integrity(
            "冻结输入的内容与清单记录不一致。".to_owned(),
        ));
    }
    let inputs: Inputs = strict(&inputs_bytes, "冻结输入")?;
    if inputs.policy != diagnostics_model::POLICY
        || inputs.case_id != declaration.case_id
        || inputs.project_sha256 != manifest.project_sha256
    {
        return Err(diagnostics_model::integrity(
            "冻结输入里的策略、案例编号或声明摘要与声明不一致。".to_owned(),
        ));
    }
    let recomputed = inputs_of(&declaration, &manifest.project_sha256)?;
    if recomputed.categories != inputs.categories
        || recomputed.entries != inputs.entries
        || recomputed.selected_categories != inputs.selected_categories
        || recomputed.config != inputs.config
        || recomputed.fingerprints != inputs.fingerprints
    {
        return Err(diagnostics_model::integrity(
            "冻结输入的规模与重新导出的结果不一致：这份输入已经不能复现当时的结论。".to_owned(),
        ));
    }

    Ok(Frozen {
        root,
        manifest,
        declaration,
        inputs,
    })
}

fn write_output(
    frozen: &Frozen,
    output: &Path,
    export: &Export,
) -> Result<OutputManifest, ProjectError> {
    let export_bytes = canonical(&export.export, "导出内容")?;
    let report_bytes = canonical(&export.report, "导出报告")?;
    let markdown = diagnostics_model::render_markdown(export);

    write_new(&output.join(EXPORT), &export_bytes, "导出内容")?;
    write_new(&output.join(REPORT), &report_bytes, "导出报告")?;
    write_new(&output.join(MARKDOWN), markdown.as_bytes(), "导出说明")?;

    let manifest = OutputManifest {
        policy: diagnostics_model::POLICY.to_owned(),
        producer: OUTPUT_PRODUCER.to_owned(),
        schema_version: diagnostics_model::SCHEMA_VERSION,
        case_id: frozen.declaration.case_id.clone(),
        source_root: frozen.root.to_string_lossy().into_owned(),
        source_inputs_sha256: frozen.manifest.inputs_sha256.clone(),
        export_sha256: crate::text::sha256(&export_bytes),
        report_sha256: crate::text::sha256(&report_bytes),
        markdown_sha256: crate::text::sha256(markdown.as_bytes()),
        counts: export.report.counts.clone(),
    };
    let manifest_bytes = canonical(&manifest, "输出清单")?;
    write_new(&output.join(OUTPUT_MANIFEST), &manifest_bytes, "输出清单")?;

    let entries = names(output, "输出根")?;
    let mut required: Vec<String> = OUTPUT_ENTRIES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    required.sort();
    if entries != required {
        return Err(diagnostics_model::integrity(format!(
            "输出根必须恰好是 {required:?}，实际是 {entries:?}。"
        )));
    }
    Ok(manifest)
}

pub fn build(root: &Path, output: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let export = frozen.export()?;

    let parent = create_parent(output, "输出根")?;
    let output = parent.join(output.file_name().ok_or_else(|| {
        diagnostics_model::argument(format!("输出根必须是一个目录名：{}。", output.display()))
    })?);
    if output.exists() {
        return Err(diagnostics_model::argument(format!(
            "输出根必须尚不存在：{}；请选择一个新目录。",
            output.display()
        )));
    }
    let source = fs::canonicalize(&frozen.root)
        .map_err(|error| diagnostics_model::integrity(format!("无法解析诊断根：{error}")))?;
    if output.starts_with(&source) || source.starts_with(&output) {
        return Err(diagnostics_model::argument(format!(
            "输出根不能与诊断根互相包含：{} 与 {}。",
            output.display(),
            source.display()
        )));
    }

    fs::create_dir(&output).map_err(|error| {
        diagnostics_model::integrity(format!("无法建立输出根 {}：{error}", output.display()))
    })?;
    match write_output(&frozen, &output, &export) {
        Ok(manifest) => {
            let counts = &manifest.counts;
            let mut out = String::new();
            out.push_str(&format!(
                "诊断导出 {}：已生成（policy={}，导出格式版本 {}）\n",
                output.display(),
                diagnostics_model::POLICY,
                diagnostics_model::EXPORT_VERSION
            ));
            out.push_str(&format!(
                "导出范围：范围类别 {} 个，已导出 {} 个、未导出 {} 个；条目 {} 条（已导出 {} 条）\n",
                counts.categories,
                counts.selected_categories,
                counts.excluded_categories,
                counts.entries,
                counts.selected_entries
            ));
            out.push_str(&format!(
                "字节：已导出 {} 字节、未导出 {} 字节；敏感类别 {} 个（已导出 {} 个）\n",
                counts.exported_bytes,
                counts.excluded_bytes,
                counts.sensitive_categories,
                counts.sensitive_included
            ));
            out.push_str(&format!(
                "文件：export.json {}；report.json {}；report.md {}\n",
                short(&manifest.export_sha256),
                short(&manifest.report_sha256),
                short(&manifest.markdown_sha256)
            ));
            out.push_str(
                "模型调用与网络请求：0 次；输出根只含派生诊断包，不写项目、不写 Vault。\n",
            );
            Ok(out)
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&output);
            Err(error)
        }
    }
}

pub fn status(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let export = frozen.export()?;
    let counts = &export.report.counts;
    let mut out = String::new();
    out.push_str(&format!(
        "诊断导出 {}：已建立（policy={}，导出格式版本 {}）\n",
        frozen.root.display(),
        diagnostics_model::POLICY,
        diagnostics_model::EXPORT_VERSION
    ));
    out.push_str(&format!(
        "所有权：工程声明 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        short(&frozen.manifest.project_sha256),
        short(&frozen.manifest.inputs_sha256),
        frozen.manifest.created_at
    ));
    out.push_str(&format!(
        "冻结规模：范围类别 {} 个、条目 {} 条、配置 {} 项、输入指纹 {} 个\n",
        counts.categories, counts.entries, counts.config, counts.fingerprints
    ));
    out.push_str(&format!(
        "冻结输入：{} 个类别、{} 条条目、勾选 {} 个类别\n",
        frozen.inputs.categories, frozen.inputs.entries, frozen.inputs.selected_categories
    ));
    out.push_str(&format!(
        "勾选：已导出 {} 个类别、{} 条、{} 字节；未导出 {} 个类别、{} 字节\n",
        counts.selected_categories,
        counts.selected_entries,
        counts.exported_bytes,
        counts.excluded_categories,
        counts.excluded_bytes
    ));
    for category in &export.report.categories {
        out.push_str(&format!(
            "  - {}（{}）：{} 条、{} 字节，{}\n",
            category.category_id,
            category.label,
            category.entries,
            category.bytes,
            if category.included {
                "已导出"
            } else {
                "未导出"
            }
        ));
    }
    out.push_str(&format!(
        "敏感范围：{} 个类别，其中已导出 {} 个\n",
        counts.sensitive_categories, counts.sensitive_included
    ));
    out.push_str("模型调用与网络请求：0 次；只读重算，未修改任何文件。\n");
    Ok(out)
}

pub fn list(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let export = frozen.export()?;
    let mut payload: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    payload.insert(
        "policy".to_owned(),
        serde_json::Value::String(diagnostics_model::POLICY.to_owned()),
    );
    payload.insert(
        "producer".to_owned(),
        serde_json::Value::String(APP_PRODUCER.to_owned()),
    );
    payload.insert(
        "export_version".to_owned(),
        serde_json::json!(diagnostics_model::EXPORT_VERSION),
    );
    payload.insert(
        "schema_version".to_owned(),
        serde_json::json!(diagnostics_model::SCHEMA_VERSION),
    );
    payload.insert(
        "case_id".to_owned(),
        serde_json::Value::String(frozen.declaration.case_id.clone()),
    );
    payload.insert(
        "root".to_owned(),
        serde_json::Value::String(frozen.root.to_string_lossy().into_owned()),
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
    payload.insert("counts".to_owned(), serde_json::json!(export.report.counts));
    payload.insert(
        "categories".to_owned(),
        serde_json::json!(export.report.categories),
    );
    payload.insert(
        "fingerprints".to_owned(),
        serde_json::json!(frozen.declaration.fingerprints),
    );
    payload.insert(
        "configuration".to_owned(),
        serde_json::json!(frozen.inputs.categories),
    );
    payload.insert("model_calls".to_owned(), serde_json::json!(0));
    payload.insert("network_requests".to_owned(), serde_json::json!(0));
    serde_json::to_string_pretty(&payload)
        .map_err(|error| diagnostics_model::integrity(format!("无法编码 list 输出：{error}")))
}
