//! 全库链接与唯一解释位置审计的冻结根与确定性输出（B11-T03）。
//!
//! 冻结根只允许 `control/`，里面是工程声明、金标准、冻结输入清单与每份笔记正文的副本。
//! 输出根固定五个文件，同一冻结输入的两次构建必须字节相同。
//!
//! 这里的 `build` 有一条与其他模块不同的约定：**审计有缺口时仍然写出完整报告**，
//! 只在退出码上区分「输入非法」与「输入合法但库里有缺口」。审计的产出是结论本身，
//! 不是合法性的前提；把缺口藏进拒绝里，缺口就永远不会被逐条列名。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::audit_model::*;
use crate::compile_identity::{Identity, identity};
use crate::project::{self, ProjectError};

pub(crate) const APP_PRODUCER: &str = "noetome-audit";
pub(crate) const OUTPUT_PRODUCER: &str = "library_audit_output";
pub(crate) const CONTROL: &str = "control";
pub(crate) const MANIFEST: &str = "manifest.json";
pub(crate) const INPUTS: &str = "inputs.json";
pub(crate) const PROJECT: &str = "project.json";
pub(crate) const GOLD: &str = "gold.json";
pub(crate) const NOTES: &str = "notes";
pub(crate) const LINKS: &str = "links.json";
pub(crate) const EXPLANATIONS: &str = "explanations.json";
pub(crate) const REPORT: &str = "report.json";
pub(crate) const MARKDOWN: &str = "report.md";
pub(crate) const OUTPUT_MANIFEST: &str = "manifest.json";
pub(crate) const ROOT_ENTRIES: [&str; 1] = ["control"];
pub(crate) const CONTROL_ENTRIES: [&str; 5] = [
    "gold.json",
    "inputs.json",
    "manifest.json",
    "notes",
    "project.json",
];
pub(crate) const OUTPUT_ENTRIES: [&str; 5] = [
    "explanations.json",
    "links.json",
    "manifest.json",
    "report.json",
    "report.md",
];

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
    let guard = project::read_guard(path)
        .map_err(|error| integrity(format!("无法读取{label} {}：{error}。", path.display())))?;
    let mut file = guard;
    let mut bytes = Vec::new();
    let mut limited = (&mut file).take(limit + 1);
    limited
        .read_to_end(&mut bytes)
        .map_err(|error| integrity(format!("读取{label}失败：{error}。")))?;
    if bytes.len() as u64 > limit {
        return Err(argument(format!(
            "{label}超过 {limit} 字节上限：{}。",
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

fn digest_of(bytes: &[u8]) -> String {
    crate::text::sha256(bytes)
}

// ---------------------------------------------------------------------------
// records
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoteDigest {
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) title: String,
    pub(crate) sha256: String,
    pub(crate) byte_length: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Inputs {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) gold_sha256: String,
    pub(crate) notes: Vec<NoteDigest>,
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
    pub(crate) notes_identity: Identity,
    pub(crate) inputs_identity: Identity,
    pub(crate) project_sha256: String,
    pub(crate) gold_sha256: String,
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
    pub(crate) links_sha256: String,
    pub(crate) explanations_sha256: String,
    pub(crate) report_sha256: String,
    pub(crate) markdown_sha256: String,
    pub(crate) verdict: String,
    pub(crate) counts: Counts,
}

pub(crate) struct Frozen {
    pub(crate) root: PathBuf,
    pub(crate) manifest: Manifest,
    pub(crate) project: Project,
    pub(crate) gold: Gold,
    pub(crate) inputs: Inputs,
    pub(crate) bodies: BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------
// adopt
// ---------------------------------------------------------------------------

pub fn adopt(project_path: &Path, root: &Path) -> Result<String, ProjectError> {
    let package = project_path
        .parent()
        .ok_or_else(|| argument("工程声明必须位于某个目录里。".to_owned()))?;
    let package = fs::canonicalize(package)
        .map_err(|error| argument(format!("无法定位工程目录：{error}。")))?;
    let project_bytes = read_file(project_path, MAX_BYTES as u64, "工程声明")?;
    let project = Project::parse(&project_bytes)?;

    let gold_path = package.join(&project.gold);
    let gold_bytes = read_file(&gold_path, MAX_BYTES as u64, "金标准")?;
    let gold = Gold::parse(&gold_bytes)?;
    if gold.case_id != project.case_id {
        return Err(argument(format!(
            "金标准的 case_id ({}) 与工程声明 ({}) 不一致。",
            gold.case_id, project.case_id
        )));
    }

    let mut notes = Vec::new();
    let mut bodies = BTreeMap::new();
    for spec in &project.notes {
        let path = package.join(&spec.path);
        let bytes = read_file(&path, MAX_BYTES as u64, "笔记正文")?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| {
                argument(format!(
                    "笔记 {} 不是合法的 UTF-8 文本（首个非法字节在 {}）：{}。",
                    spec.note_id,
                    error.valid_up_to(),
                    spec.path
                ))
            })?
            .to_owned();
        notes.push(NoteDigest {
            note_id: spec.note_id.clone(),
            path: spec.path.clone(),
            title: spec.title.clone(),
            sha256: digest_of(&bytes),
            byte_length: bytes.len() as u64,
        });
        bodies.insert(spec.note_id.clone(), text);
    }

    let views: Vec<NoteView<'_>> = project
        .notes
        .iter()
        .map(|spec| NoteView {
            spec,
            text: bodies
                .get(&spec.note_id)
                .map(|value| value.as_str())
                .unwrap_or(""),
        })
        .collect();
    // Declaration errors (unknown marker concept, unescaped table alias, too
    // many links) must fail before any byte is written. Gaps are not errors:
    // they are what the audit is for.
    let audit = evaluate(&project, &views, &gold)?;

    let inputs = Inputs {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: project.case_id.clone(),
        gold_sha256: digest_of(&gold_bytes),
        notes,
    };

    if root.exists() {
        return Err(argument(format!(
            "审计根必须尚不存在：{}；请选择一个新目录。",
            root.display()
        )));
    }
    let root = resolve(root)?;
    let parent = root
        .parent()
        .ok_or_else(|| argument("审计根必须有父目录。".to_owned()))?;
    require_dir(parent, "审计根的父目录")?;
    let parent_identity = identity(parent)?;
    fs::create_dir(&root).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            argument(format!("审计根必须尚不存在：{}。", root.display()))
        } else {
            integrity(format!("无法建立审计根 {}：{error}。", root.display()))
        }
    })?;
    let root_identity = identity(&root)?;

    let result = adopt_into(
        &root,
        &package,
        &project,
        &gold,
        &gold_bytes,
        &project_bytes,
        inputs,
        &bodies,
        parent_identity,
        root_identity.clone(),
    );
    match result {
        Ok(manifest) => Ok(adopt_summary(&manifest, &audit)),
        Err(error) => {
            if identity(&root)
                .map(|value| value == root_identity)
                .unwrap_or(false)
            {
                let _ = fs::remove_dir_all(&root);
            }
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn adopt_into(
    root: &Path,
    package: &Path,
    project: &Project,
    gold: &Gold,
    gold_bytes: &[u8],
    project_bytes: &[u8],
    inputs: Inputs,
    bodies: &BTreeMap<String, String>,
    parent_identity: Identity,
    root_identity: Identity,
) -> Result<Manifest, ProjectError> {
    let control = root.join(CONTROL);
    fs::create_dir(&control).map_err(|error| integrity(format!("无法建立控制目录：{error}。")))?;
    let control_identity = identity(&control)?;
    let notes_dir = control.join(NOTES);
    fs::create_dir(&notes_dir)
        .map_err(|error| integrity(format!("无法建立笔记目录：{error}。")))?;
    let notes_identity = identity(&notes_dir)?;

    for spec in &project.notes {
        let body = bodies
            .get(&spec.note_id)
            .ok_or_else(|| integrity(format!("笔记 {} 的正文缺失。", spec.note_id)))?;
        write_new(&notes_dir.join(&spec.path), body.as_bytes(), "冻结笔记")?;
    }
    write_new(&control.join(GOLD), gold_bytes, "冻结金标准")?;
    write_new(&control.join(PROJECT), project_bytes, "冻结工程声明")?;

    let inputs_text = canonical(&inputs)?;
    write_new(
        &control.join(INPUTS),
        inputs_text.as_bytes(),
        "冻结输入清单",
    )?;
    let inputs_sha256 = digest_of(inputs_text.as_bytes());

    write_new(&control.join(MANIFEST), b"", "冻结清单占位")?;
    let manifest_path = control.join(MANIFEST);
    let manifest = Manifest {
        policy: POLICY.to_owned(),
        producer: APP_PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: project.case_id.clone(),
        root: root
            .to_str()
            .ok_or_else(|| integrity("审计根路径不是 UTF-8。".to_owned()))?
            .to_owned(),
        root_identity,
        parent_identity,
        control_identity,
        manifest_identity: identity(&manifest_path)?,
        notes_identity,
        inputs_identity: identity(&control.join(INPUTS))?,
        project_sha256: digest_of(project_bytes),
        gold_sha256: digest_of(gold_bytes),
        inputs_sha256,
        created_at: now()?,
    };
    let manifest_text = canonical(&manifest)?;
    fs::write(&manifest_path, manifest_text.as_bytes())
        .map_err(|error| integrity(format!("无法写入冻结清单：{error}。")))?;
    let readback = read_file(&manifest_path, MAX_BYTES as u64, "冻结清单")?;
    if readback != manifest_text.as_bytes() {
        return Err(integrity("写入冻结清单后读回不一致。".to_owned()));
    }
    let loaded: Manifest = strict(&readback, "冻结清单")?;
    if loaded.inputs_sha256 != manifest.inputs_sha256 {
        return Err(integrity("冻结清单读回后输入摘要不一致。".to_owned()));
    }
    let _ = package;
    let _ = gold;
    Ok(manifest)
}

fn adopt_summary(manifest: &Manifest, audit: &Audit) -> String {
    format!(
        "审计根 {}：已建立（policy={}，结构版本 {}）\n所有权：工程声明 SHA256 {}；金标准 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n冻结来源：{} 份笔记、{} 个标题、{} 条链接\n唯一解释位置：{} 个概念、{} 个标记，多处声明 {} 个\n链接可用：{}/{}（{}），断链 {}、歧义 {}\n原子召回：关键 {}/{}（{}）、一般 {}/{}（{}）\n审计结论：{}\n模型调用与网络请求：0 次；本根只保存冻结输入，不写项目、不写 Vault。",
        manifest.root,
        manifest.policy,
        manifest.schema_version,
        short(&manifest.project_sha256),
        short(&manifest.gold_sha256),
        short(&manifest.inputs_sha256),
        manifest.created_at,
        audit.counts.notes,
        audit.counts.headings,
        audit.counts.links,
        audit.counts.concepts,
        audit.counts.markers,
        audit.counts.duplicate_concepts,
        audit.availability.numerator,
        audit.availability.denominator,
        audit.availability.display,
        audit.counts.links_broken,
        audit.counts.links_ambiguous,
        audit.key_recall.numerator,
        audit.key_recall.denominator,
        audit.key_recall.display,
        audit.general_recall.numerator,
        audit.general_recall.denominator,
        audit.general_recall.display,
        verdict_text(&audit.verdict),
    )
}

// ---------------------------------------------------------------------------
// open
// ---------------------------------------------------------------------------

pub(crate) fn open(root: &Path) -> Result<Frozen, ProjectError> {
    let root = resolve(root)?;
    if !root.exists() {
        return Err(unknown_root(format!(
            "{} 不存在，不是已建立的审计根；未修改任何文件。",
            root.display()
        )));
    }
    require_dir(&root, "审计根")?;
    let top = names(&root)?;
    if !top.contains_key(CONTROL) {
        // 陌生目录（没有 control/）不能当成损坏的审计根：错误码要说明"这不是本工具
        // 建立的根"，否则用户会以为自己的笔记库被改坏了。
        return Err(unknown_root(format!(
            "{} 不是已建立的审计根（缺少 {}）；陌生目录保持只读，未接管、未修改任何文件。",
            root.display(),
            CONTROL
        )));
    }
    if top.keys().cloned().collect::<Vec<_>>() != ROOT_ENTRIES.to_vec() {
        return Err(integrity(format!(
            "审计根顶层只允许 {}，实际是 {:?}。",
            ROOT_ENTRIES.join("、"),
            top.keys().cloned().collect::<Vec<_>>()
        )));
    }
    let control = root.join(CONTROL);
    require_dir(&control, "控制目录")?;
    let entries = names(&control)?;
    if entries.keys().cloned().collect::<Vec<_>>() != CONTROL_ENTRIES.to_vec() {
        return Err(integrity(format!(
            "控制目录只允许 {}，实际是 {:?}。",
            CONTROL_ENTRIES.join("、"),
            entries.keys().cloned().collect::<Vec<_>>()
        )));
    }
    let manifest_bytes = read_file(&control.join(MANIFEST), MAX_BYTES as u64, "冻结清单")?;
    let manifest: Manifest = strict(&manifest_bytes, "冻结清单")?;
    if manifest.policy != POLICY
        || manifest.producer != APP_PRODUCER
        || manifest.schema_version != SCHEMA_VERSION
    {
        return Err(integrity(
            "冻结清单的 policy/producer/schema_version 不符。".to_owned(),
        ));
    }
    let canonical_root =
        fs::canonicalize(&root).map_err(|error| integrity(format!("无法定位审计根：{error}。")))?;
    if manifest.root != canonical_root.to_string_lossy() {
        return Err(integrity(format!(
            "冻结清单记录的根是 {}，实际是 {}。",
            manifest.root,
            canonical_root.display()
        )));
    }
    check_identity(&root, &manifest.root_identity, "审计根")?;
    if let Some(parent) = canonical_root.parent() {
        check_identity(parent, &manifest.parent_identity, "审计根的父目录")?;
    }
    check_identity(&control, &manifest.control_identity, "控制目录")?;
    check_identity(
        &control.join(MANIFEST),
        &manifest.manifest_identity,
        "冻结清单",
    )?;
    check_identity(&control.join(NOTES), &manifest.notes_identity, "笔记目录")?;
    check_identity(&control.join(INPUTS), &manifest.inputs_identity, "输入清单")?;

    let project_bytes = read_file(&control.join(PROJECT), MAX_BYTES as u64, "工程声明")?;
    if digest_of(&project_bytes) != manifest.project_sha256 {
        return Err(integrity("工程声明的摘要与冻结清单不符。".to_owned()));
    }
    let project = Project::parse(&project_bytes)?;
    let gold_bytes = read_file(&control.join(GOLD), MAX_BYTES as u64, "金标准")?;
    if digest_of(&gold_bytes) != manifest.gold_sha256 {
        return Err(integrity("金标准的摘要与冻结清单不符。".to_owned()));
    }
    let gold = Gold::parse(&gold_bytes)?;

    let inputs_bytes = read_file(&control.join(INPUTS), MAX_BYTES as u64, "输入清单")?;
    if digest_of(&inputs_bytes) != manifest.inputs_sha256 {
        return Err(integrity("输入清单的摘要与冻结清单不符。".to_owned()));
    }
    let inputs: Inputs = strict(&inputs_bytes, "输入清单")?;
    if inputs.policy != POLICY
        || inputs.schema_version != SCHEMA_VERSION
        || inputs.case_id != project.case_id
    {
        return Err(integrity(
            "输入清单的 policy/schema/case_id 不符。".to_owned(),
        ));
    }
    if inputs.gold_sha256 != manifest.gold_sha256 {
        return Err(integrity("输入清单里的金标准摘要不符。".to_owned()));
    }

    let notes_dir = control.join(NOTES);
    let mut bodies = BTreeMap::new();
    let mut expected: BTreeSet<String> = BTreeSet::new();
    for digest in &inputs.notes {
        expected.insert(digest.path.clone());
        let path = notes_dir.join(&digest.path);
        let bytes = read_file(&path, MAX_BYTES as u64, "冻结笔记")?;
        if digest_of(&bytes) != digest.sha256 {
            return Err(integrity(format!(
                "冻结笔记 {} 的摘要与输入清单不符。",
                digest.path
            )));
        }
        if bytes.len() as u64 != digest.byte_length {
            return Err(integrity(format!(
                "冻结笔记 {} 的字节长度与输入清单不符。",
                digest.path
            )));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| integrity(format!("冻结笔记不是 UTF-8：{error}。")))?
            .to_owned();
        if project.note(&digest.note_id).is_none() {
            return Err(integrity(format!(
                "输入清单里的笔记 {} 不在工程声明里。",
                digest.note_id
            )));
        }
        bodies.insert(digest.note_id.clone(), text);
    }
    let mut actual = BTreeSet::new();
    for (name, path) in walk(&notes_dir)? {
        actual.insert(name);
        let _ = path;
    }
    if actual != expected {
        return Err(integrity(format!(
            "冻结笔记集合与输入清单不符：清单 {:?}，实际 {:?}。",
            expected, actual
        )));
    }
    if bodies.len() != project.notes.len() {
        return Err(integrity("冻结笔记数量与工程声明不符。".to_owned()));
    }

    Ok(Frozen {
        root: canonical_root,
        manifest,
        project,
        gold,
        inputs,
        bodies,
    })
}

fn walk(dir: &Path) -> Result<Vec<(String, PathBuf)>, ProjectError> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = fs::read_dir(&current)
            .map_err(|error| integrity(format!("无法列出目录 {}：{error}。", current.display())))?;
        for entry in entries {
            let entry = entry.map_err(|error| integrity(format!("无法读取目录项：{error}。")))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| integrity(format!("无法读取 {}：{error}。", path.display())))?;
            if project::is_link(&metadata) {
                return Err(argument(format!(
                    "冻结目录里不允许符号链接：{}。",
                    path.display()
                )));
            }
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(dir)
                .map_err(|_| integrity("无法计算相对路径。".to_owned()))?
                .to_str()
                .ok_or_else(|| integrity("冻结笔记路径不是 UTF-8。".to_owned()))?
                .replace('\\', "/");
            out.push((relative, path));
        }
    }
    out.sort();
    Ok(out)
}

fn views<'a>(frozen: &'a Frozen) -> Vec<NoteView<'a>> {
    frozen
        .project
        .notes
        .iter()
        .map(|spec| NoteView {
            spec,
            text: frozen
                .bodies
                .get(&spec.note_id)
                .map(|value| value.as_str())
                .unwrap_or(""),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// build
// ---------------------------------------------------------------------------

pub fn build(root: &Path, output: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    if output.exists() {
        return Err(argument(format!(
            "输出根必须尚不存在：{}；请选择一个新目录。",
            output.display()
        )));
    }
    let output = resolve(output)?;
    let parent = output
        .parent()
        .ok_or_else(|| argument("输出根必须有父目录。".to_owned()))?;
    require_dir(parent, "输出根的父目录")?;
    let frozen_canonical = fs::canonicalize(&frozen.root)
        .map_err(|error| integrity(format!("无法定位审计根：{error}。")))?;
    let output_canonical = parent.join(output.file_name().unwrap_or_default());
    if output_canonical.starts_with(&frozen_canonical)
        || frozen_canonical.starts_with(&output_canonical)
    {
        return Err(argument(
            "输出根不能与审计根互相包含：冻结输入必须保持只读。".to_owned(),
        ));
    }

    let views = views(&frozen);
    let audit = evaluate(&frozen.project, &views, &frozen.gold)?;
    fs::create_dir(&output)
        .map_err(|error| integrity(format!("无法建立输出根 {}：{error}。", output.display())))?;
    let result = write_output(&frozen, &audit, &output);
    match result {
        Ok(manifest) => {
            let summary = build_summary(&manifest, &output, &audit);
            if audit.verdict == "pass" {
                Ok(summary)
            } else {
                Err(findings(format!(
                    "审计完成但存在 {} 条缺口，报告已写出：{}\n结论：{}\n链接可用 {}/{}；关键原子召回 {}/{}；一般原子召回 {}/{}；唯一解释位置多处声明 {} 个概念。",
                    audit.findings.len(),
                    output.display(),
                    verdict_text(&audit.verdict),
                    audit.availability.numerator,
                    audit.availability.denominator,
                    audit.key_recall.numerator,
                    audit.key_recall.denominator,
                    audit.general_recall.numerator,
                    audit.general_recall.denominator,
                    audit.counts.duplicate_concepts,
                )))
            }
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&output);
            Err(error)
        }
    }
}

fn write_output(
    frozen: &Frozen,
    audit: &Audit,
    output: &Path,
) -> Result<OutputManifest, ProjectError> {
    let links_text = format!("{}\n", canonical(&links_file(&frozen.project, audit))?);
    let explanations_text = format!(
        "{}\n",
        canonical(&explanations_file(&frozen.project, audit))?
    );
    let summary = summarize(&frozen.project, audit);
    let report_text = format!("{}\n", canonical(&summary)?);
    let markdown = render_markdown(&frozen.project, audit);

    write_new(&output.join(LINKS), links_text.as_bytes(), "链接明细")?;
    write_new(
        &output.join(EXPLANATIONS),
        explanations_text.as_bytes(),
        "解释位置",
    )?;
    write_new(&output.join(REPORT), report_text.as_bytes(), "审计摘要")?;
    write_new(&output.join(MARKDOWN), markdown.as_bytes(), "审计报告")?;

    let manifest = OutputManifest {
        policy: POLICY.to_owned(),
        producer: OUTPUT_PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: frozen.project.case_id.clone(),
        source_root: frozen
            .root
            .to_str()
            .ok_or_else(|| integrity("审计根路径不是 UTF-8。".to_owned()))?
            .to_owned(),
        source_inputs_sha256: frozen.manifest.inputs_sha256.clone(),
        links_sha256: digest_of(links_text.as_bytes()),
        explanations_sha256: digest_of(explanations_text.as_bytes()),
        report_sha256: digest_of(report_text.as_bytes()),
        markdown_sha256: digest_of(markdown.as_bytes()),
        verdict: audit.verdict.clone(),
        counts: audit.counts.clone(),
    };
    let manifest_text = format!("{}\n", canonical(&manifest)?);
    write_new(
        &output.join(OUTPUT_MANIFEST),
        manifest_text.as_bytes(),
        "输出清单",
    )?;

    let entries = names(output)?;
    let mut actual: Vec<String> = entries.keys().cloned().collect();
    actual.sort();
    let mut expected: Vec<String> = OUTPUT_ENTRIES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    expected.sort();
    if actual != expected {
        return Err(integrity(format!(
            "输出根的文件集合不符：期望 {expected:?}，实际 {actual:?}。"
        )));
    }
    Ok(manifest)
}

fn build_summary(manifest: &OutputManifest, output: &Path, audit: &Audit) -> String {
    let counts = &audit.counts;
    format!(
        "输出根 {}：已生成（policy={}，结构版本 {}）\n审计结论：{}\n链接：{} 条（可用 {}、断链 {}、歧义 {}；表格内 {}、转义别名 {}、嵌入 {}）\n唯一解释位置：{} 个概念、{} 个标记，多处声明 {} 个\n原子召回：关键 {}/{}、一般 {}/{}，缺口 {} 个\n文件：links.json {}；explanations.json {}；report.json {}；report.md {}\n模型调用与网络请求：0 次；输出根只含派生报告，不写项目、不写 Vault。",
        output.display(),
        manifest.policy,
        manifest.schema_version,
        verdict_text(&manifest.verdict),
        counts.links,
        counts.links_resolved,
        counts.links_broken,
        counts.links_ambiguous,
        counts.links_in_table,
        counts.links_escaped_alias,
        counts.links_embedded,
        counts.concepts,
        counts.markers,
        counts.duplicate_concepts,
        audit.key_recall.numerator,
        audit.key_recall.denominator,
        audit.general_recall.numerator,
        audit.general_recall.denominator,
        counts.atoms_missing,
        short(&manifest.links_sha256),
        short(&manifest.explanations_sha256),
        short(&manifest.report_sha256),
        short(&manifest.markdown_sha256),
    )
}

// ---------------------------------------------------------------------------
// status / list
// ---------------------------------------------------------------------------

pub fn status(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let views = views(&frozen);
    let audit = evaluate(&frozen.project, &views, &frozen.gold)?;
    let mut out = status_text(&audit, &frozen.project.case_id);
    out.push_str(&format!(
        "审计根：{}（冻结于 {}；冻结输入 {} 份笔记）\n",
        frozen.root.display(),
        frozen.manifest.created_at,
        frozen.inputs.notes.len()
    ));
    Ok(out)
}

fn json<T: Serialize>(value: &T) -> Result<String, ProjectError> {
    serde_json::to_string_pretty(value)
        .map_err(|error| integrity(format!("无法序列化输出：{error}。")))
}

pub fn list(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let views = views(&frozen);
    let audit = evaluate(&frozen.project, &views, &frozen.gold)?;
    let summary = summarize(&frozen.project, &audit);
    #[derive(Serialize)]
    struct Listing<'a> {
        policy: &'a str,
        producer: &'a str,
        schema_version: u32,
        case_id: &'a str,
        inputs_sha256: &'a str,
        project_sha256: &'a str,
        gold_sha256: &'a str,
        created_at: &'a str,
        verdict: &'a str,
        counts: &'a Counts,
        availability: &'a Ratio,
        key_recall: &'a Ratio,
        general_recall: &'a Ratio,
        findings: &'a [Finding],
        model_calls: usize,
        network_requests: usize,
    }
    let listing = Listing {
        policy: POLICY,
        producer: APP_PRODUCER,
        schema_version: SCHEMA_VERSION,
        case_id: &frozen.project.case_id,
        inputs_sha256: &frozen.manifest.inputs_sha256,
        project_sha256: &frozen.manifest.project_sha256,
        gold_sha256: &frozen.manifest.gold_sha256,
        created_at: &frozen.manifest.created_at,
        verdict: &summary.verdict,
        counts: &summary.counts,
        availability: &summary.availability,
        key_recall: &summary.key_recall,
        general_recall: &summary.general_recall,
        findings: &summary.findings,
        model_calls: 0,
        network_requests: 0,
    };
    json(&listing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_entries_are_the_five_documented_files() {
        assert_eq!(OUTPUT_ENTRIES.len(), 5);
        assert!(OUTPUT_ENTRIES.contains(&LINKS));
        assert!(OUTPUT_ENTRIES.contains(&EXPLANATIONS));
        assert!(OUTPUT_ENTRIES.contains(&REPORT));
        assert!(OUTPUT_ENTRIES.contains(&MARKDOWN));
        assert!(OUTPUT_ENTRIES.contains(&OUTPUT_MANIFEST));
    }

    #[test]
    fn control_entries_match_the_documented_layout() {
        assert_eq!(
            CONTROL_ENTRIES.to_vec(),
            vec![
                "gold.json",
                "inputs.json",
                "manifest.json",
                "notes",
                "project.json"
            ]
        );
    }
}
