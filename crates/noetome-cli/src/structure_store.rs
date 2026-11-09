//! 同类结构一致性与导航复习的冻结根与确定性输出（B10-T03）。
//!
//! 冻结根只允许 `control/`，里面是作者包、概念表、每份笔记的原始字节与清单；
//! 输出根固定六个文件，同一冻结输入的两次构建必须字节相同。

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
use crate::structure_model::*;
use crate::text;

pub(crate) const APP_PRODUCER: &str = "noetome-structure";
pub(crate) const OUTPUT_PRODUCER: &str = "structure_navigation_output";
pub(crate) const CONTROL: &str = "control";
pub(crate) const MANIFEST: &str = "control/manifest.json";
pub(crate) const INPUTS: &str = "control/inputs.json";
pub(crate) const FAMILY: &str = "control/family.json";
pub(crate) const CONCEPTS: &str = "control/concepts.json";
pub(crate) const TEXT_DIR: &str = "control/text";
pub(crate) const TEXT_SUFFIX: &str = ".md";
pub(crate) const STRUCTURE: &str = "structure.json";
pub(crate) const LINKS: &str = "links.json";
pub(crate) const REPORT: &str = "report.json";
pub(crate) const NAVIGATION: &str = "navigation.md";
pub(crate) const MARKDOWN: &str = "report.md";
pub(crate) const OUTPUT_MANIFEST: &str = "manifest.json";
pub(crate) const ROOT_ENTRIES: [&str; 1] = ["control"];
pub(crate) const CONTROL_ENTRIES: [&str; 5] = [
    "manifest.json",
    "inputs.json",
    "family.json",
    "concepts.json",
    "text",
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
            "{label} 不是规范编码：请用紧凑 JSON、无重复键、无未知字段。"
        )));
    }
    Ok(value)
}

fn now() -> Result<String, ProjectError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| integrity(format!("无法记录时间：{error}。")))
}

fn read_file(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>, ProjectError> {
    let metadata = fs::metadata(path)
        .map_err(|error| integrity(format!("无法读取{label} {}：{error}。", path.display())))?;
    if !metadata.is_file() {
        return Err(integrity(format!(
            "{label} 不是普通文件：{}。",
            path.display()
        )));
    }
    if metadata.len() > limit {
        return Err(integrity(format!(
            "{label} 超过 {limit} 字节上限：{}。",
            path.display()
        )));
    }
    let guard = project::read_guard(path)?;
    let mut bytes = Vec::new();
    guard
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| integrity(format!("读取{label}失败：{error}。")))?;
    if bytes.len() as u64 > limit {
        return Err(integrity(format!("{label} 超过 {limit} 字节上限。")));
    }
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8], label: &str) -> Result<(), ProjectError> {
    project::check_components(path)?;
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
pub(crate) struct DocumentDigest {
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) normalized_sha256: String,
    pub(crate) byte_length: u64,
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
    pub(crate) family_sha256: String,
    pub(crate) concepts_sha256: String,
    pub(crate) inputs_sha256: String,
    pub(crate) documents: Vec<DocumentDigest>,
    pub(crate) created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Inputs {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) family: String,
    pub(crate) notes: Vec<NoteDigest>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoteDigest {
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) title: String,
    pub(crate) raw_sha256: String,
    pub(crate) byte_length: u64,
    pub(crate) normalized_sha256: String,
    pub(crate) span_count: usize,
    pub(crate) heading_count: usize,
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
    pub(crate) structure_sha256: String,
    pub(crate) links_sha256: String,
    pub(crate) report_sha256: String,
    pub(crate) navigation_sha256: String,
    pub(crate) markdown_sha256: String,
    pub(crate) counts: Counts,
}

pub(crate) struct Frozen {
    pub(crate) root: PathBuf,
    pub(crate) manifest: Manifest,
    pub(crate) inputs: Inputs,
    pub(crate) family: Family,
    pub(crate) concepts: ConceptFile,
    pub(crate) bodies: BTreeMap<String, String>,
}

impl Frozen {
    fn views(&self) -> Vec<NoteView<'_>> {
        self.family
            .notes
            .iter()
            .filter_map(|spec| {
                self.bodies.get(&spec.note_id).map(|text| NoteView {
                    spec,
                    text: text.as_str(),
                })
            })
            .collect()
    }

    pub(crate) fn decide(&self) -> Result<Decision, ProjectError> {
        let views = self.views();
        if views.len() != self.family.notes.len() {
            return Err(integrity(
                "冻结文本与作者包声明的笔记数量不一致。".to_owned(),
            ));
        }
        decide(&self.family, &self.concepts, &views)
    }
}

fn read_package_file(package: &Path, relative: &str, label: &str) -> Result<Vec<u8>, ProjectError> {
    let path = resolve(&package.join(relative))?;
    if !path.starts_with(package) {
        return Err(argument(format!(
            "{label}的路径 {relative} 越出了作者包目录；本阶段只读作者包内的声明。"
        )));
    }
    read_file(&path, MAX_BYTES, label)
}

fn digest_of(package: &Path, spec: &NoteSpec) -> Result<(NoteDigest, String), ProjectError> {
    let relative = spec.path.as_str();
    let bytes = read_package_file(package, relative, "笔记正文")?;
    let parsed = text::analyze(&bytes).map_err(|error| {
        argument(format!(
            "笔记 {} 不是合法的 UTF-8 文本（首个非法字节在 {}）：{relative}。",
            spec.note_id,
            error.valid_up_to()
        ))
    })?;
    let heading_count = headings(&parsed.text).len();
    if heading_count == 0 {
        return Err(argument(format!(
            "笔记 {} 没有任何标题，无法判断章节骨架：{relative}。",
            spec.note_id
        )));
    }
    let digest = NoteDigest {
        note_id: spec.note_id.clone(),
        path: spec.path.clone(),
        title: spec.title.clone(),
        raw_sha256: text::sha256(&bytes),
        byte_length: bytes.len() as u64,
        normalized_sha256: parsed.normalized_sha256.clone(),
        span_count: parsed.spans.len(),
        heading_count,
    };
    Ok((digest, parsed.text))
}

pub fn adopt(family_path: &Path, root: &Path) -> Result<String, ProjectError> {
    let package = family_path
        .parent()
        .ok_or_else(|| argument("作者包必须位于某个目录里。".to_owned()))?;
    let package = fs::canonicalize(package)
        .map_err(|error| argument(format!("无法定位作者包目录：{error}。")))?;
    let family_bytes = read_file(family_path, MAX_BYTES, "作者包")?;
    let family_text = std::str::from_utf8(&family_bytes)
        .map_err(|error| argument(format!("作者包必须是UTF-8文本：{error}。")))?;
    let family = Family::parse(family_text)?;
    family.validate()?;
    let concepts_bytes = read_package_file(&package, &family.concepts, "概念表")?;
    let concepts_text = std::str::from_utf8(&concepts_bytes)
        .map_err(|error| argument(format!("概念表必须是UTF-8文本：{error}。")))?;
    let concepts = ConceptFile::parse(concepts_text)?;
    concepts.validate(family.limits.max_concepts)?;

    let mut digests = Vec::new();
    let mut bodies = BTreeMap::new();
    for spec in &family.notes {
        let (digest, body) = digest_of(&package, spec)?;
        digests.push(digest);
        bodies.insert(spec.note_id.clone(), body);
    }

    // 先在内存里跑一遍判定：语义拒绝不允许留下半个根。
    let views: Vec<NoteView<'_>> = family
        .notes
        .iter()
        .filter_map(|spec| {
            bodies.get(&spec.note_id).map(|text| NoteView {
                spec,
                text: text.as_str(),
            })
        })
        .collect();
    decide(&family, &concepts, &views)?;

    let target = resolve(root)?;
    let parent = target
        .parent()
        .ok_or_else(|| argument("结构根必须有父目录。".to_owned()))?
        .to_path_buf();
    if fs::symlink_metadata(&target).is_ok() {
        return Err(argument(format!(
            "结构根必须尚不存在：{}；请选择一个新目录。",
            target.display()
        )));
    }
    require_dir(&parent, "结构根的父目录")?;
    let parent_identity = identity(&parent)?;

    fs::create_dir(&target)
        .map_err(|error| integrity(format!("无法创建结构根 {}：{error}。", target.display())))?;
    let root_identity = identity(&target)?;
    let outcome = adopt_into(AdoptInput {
        target: &target,
        family: &family,
        digests: &digests,
        bodies: &bodies,
        root_identity: &root_identity,
        parent_identity: &parent_identity,
        family_sha256: text::sha256(&family_bytes),
        concepts_sha256: text::sha256(&concepts_bytes),
        family_bytes: &family_bytes,
        concepts_bytes: &concepts_bytes,
    });
    match outcome {
        Ok(manifest) => Ok(adopt_summary(&manifest, &family)),
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
    family: &'a Family,
    digests: &'a [NoteDigest],
    bodies: &'a BTreeMap<String, String>,
    root_identity: &'a Identity,
    parent_identity: &'a Identity,
    family_sha256: String,
    concepts_sha256: String,
    family_bytes: &'a [u8],
    concepts_bytes: &'a [u8],
}

fn adopt_into(input: AdoptInput<'_>) -> Result<Manifest, ProjectError> {
    let root = fs::canonicalize(input.target)
        .map_err(|error| integrity(format!("无法定位结构根：{error}。")))?;
    let control = root.join(CONTROL);
    fs::create_dir(&control).map_err(|error| integrity(format!("无法创建控制目录：{error}。")))?;
    let control_identity = identity(&control)?;
    let text_dir = control.join("text");
    fs::create_dir(&text_dir)
        .map_err(|error| integrity(format!("无法创建冻结文本目录：{error}。")))?;
    for spec in &input.family.notes {
        let body = input
            .bodies
            .get(&spec.note_id)
            .ok_or_else(|| integrity(format!("缺少笔记 {} 的正文。", spec.note_id)))?;
        write_new(
            &text_dir.join(format!("{}{}", spec.note_id, TEXT_SUFFIX)),
            body.as_bytes(),
            "冻结笔记文本",
        )?;
    }
    write_new(
        &control.join("family.json"),
        input.family_bytes,
        "冻结作者包",
    )?;
    write_new(
        &control.join("concepts.json"),
        input.concepts_bytes,
        "冻结概念表",
    )?;
    let inputs = Inputs {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: input.family.case_id.clone(),
        family: input.family.family.clone(),
        notes: input.digests.to_vec(),
    };
    let inputs_bytes = canonical(&inputs)?.into_bytes();
    write_new(&control.join("inputs.json"), &inputs_bytes, "冻结输入清单")?;
    write_new(&control.join("manifest.json"), b"", "冻结清单")?;
    let manifest_path = control.join("manifest.json");
    let manifest_identity = identity(&manifest_path)?;
    let manifest = Manifest {
        policy: POLICY.to_owned(),
        producer: APP_PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: input.family.case_id.clone(),
        root: root.display().to_string(),
        root_identity: input.root_identity.clone(),
        parent_identity: input.parent_identity.clone(),
        control_identity,
        manifest_identity,
        inputs_identity: identity(&control.join("inputs.json"))?,
        family_sha256: input.family_sha256,
        concepts_sha256: input.concepts_sha256,
        inputs_sha256: text::sha256(&inputs_bytes),
        documents: input
            .digests
            .iter()
            .map(|digest| DocumentDigest {
                note_id: digest.note_id.clone(),
                path: digest.path.clone(),
                normalized_sha256: digest.normalized_sha256.clone(),
                byte_length: digest.byte_length,
            })
            .collect(),
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
    let readback = read_file(&manifest_path, MAX_BYTES, "冻结清单")?;
    if readback != bytes {
        return Err(integrity(
            "写入冻结清单后读回不一致；保留现场并拒绝继续。".to_owned(),
        ));
    }
    Ok(manifest)
}

fn adopt_summary(manifest: &Manifest, family: &Family) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "结构根 {}：已建立（policy={}，结构版本 {}）\n",
        manifest.root, manifest.policy, manifest.schema_version
    ));
    out.push_str(&format!(
        "所有权：作者包 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        short(&manifest.family_sha256),
        short(&manifest.inputs_sha256),
        manifest.created_at
    ));
    let concepts = family
        .notes
        .iter()
        .map(|note| note.explains.len() + note.references.len())
        .sum::<usize>();
    out.push_str(&format!(
        "冻结来源：{} 份笔记、{} 个骨架节、{} 处概念声明，共 {} 字节\n",
        manifest.documents.len(),
        family.skeleton.len(),
        concepts,
        manifest
            .documents
            .iter()
            .map(|digest| digest.byte_length)
            .sum::<u64>()
    ));
    out.push_str(&format!(
        "主题族：{}；作者包里的概念引用声明 {} 条、复习提示 {} 条\n",
        family.family,
        family
            .notes
            .iter()
            .map(|note| note.references.len())
            .sum::<usize>(),
        family
            .notes
            .iter()
            .map(|note| note.reviews.len())
            .sum::<usize>()
    ));
    out.push_str("模型调用与网络请求：0 次；本根只保存冻结输入，不生成知识。\n");
    out
}

pub(crate) fn open(root: &Path) -> Result<Frozen, ProjectError> {
    let target = resolve(root)?;
    if fs::symlink_metadata(&target).is_err() {
        return Err(unknown_root(format!(
            "{} 不存在，不是已建立的结构根；未修改任何文件。",
            target.display()
        )));
    }
    let control = target.join(CONTROL);
    if fs::symlink_metadata(&control).is_err() {
        return Err(unknown_root(format!(
            "{} 不是结构根（缺少 control/）：陌生目录保持只读，未接管、未修改任何文件。",
            target.display()
        )));
    }
    let root_entries = names(&target)?;
    for name in root_entries.keys() {
        if !ROOT_ENTRIES.contains(&name.as_str()) {
            return Err(integrity(format!(
                "结构根顶层只允许 control/；发现多余的 {name}：未修改任何文件。"
            )));
        }
    }
    let control_entries = names(&control)?;
    for name in control_entries.keys() {
        if !CONTROL_ENTRIES.contains(&name.as_str()) {
            return Err(integrity(format!(
                "控制目录只允许 manifest.json、inputs.json、family.json、concepts.json、text；发现多余的 {name}。"
            )));
        }
    }
    for required in CONTROL_ENTRIES {
        if !control_entries.contains_key(required) {
            return Err(integrity(format!("控制目录缺少 {required}。")));
        }
    }
    let manifest_bytes = read_file(&target.join(MANIFEST), MAX_BYTES, "冻结清单")?;
    let manifest: Manifest = strict(&manifest_bytes, "冻结清单")?;
    if manifest.policy != POLICY
        || manifest.producer != APP_PRODUCER
        || manifest.schema_version != SCHEMA_VERSION
    {
        return Err(integrity(
            "冻结清单的 policy、producer 或 schema_version 与本版本不符。".to_owned(),
        ));
    }
    let canonical_root = fs::canonicalize(&target)
        .map_err(|error| integrity(format!("无法定位结构根：{error}。")))?;
    if manifest.root != canonical_root.display().to_string() {
        return Err(integrity(format!(
            "冻结清单记录的根是 {}，与当前路径 {} 不符。",
            manifest.root,
            canonical_root.display()
        )));
    }
    check_identity(&target, &manifest.root_identity, "结构根")?;
    check_identity(&control, &manifest.control_identity, "控制目录")?;
    let parent = target
        .parent()
        .ok_or_else(|| integrity("结构根没有父目录。".to_owned()))?;
    check_identity(parent, &manifest.parent_identity, "结构根的父目录")?;
    check_identity(
        &target.join(MANIFEST),
        &manifest.manifest_identity,
        "冻结清单",
    )?;
    check_identity(
        &target.join(INPUTS),
        &manifest.inputs_identity,
        "冻结输入清单",
    )?;

    let family_bytes = read_file(&target.join(FAMILY), MAX_BYTES, "冻结作者包")?;
    if text::sha256(&family_bytes) != manifest.family_sha256 {
        return Err(integrity("冻结作者包的摘要与清单不符。".to_owned()));
    }
    let family_text = std::str::from_utf8(&family_bytes)
        .map_err(|error| integrity(format!("冻结作者包必须是UTF-8文本：{error}。")))?;
    let family = Family::parse(family_text)?;
    family.validate()?;
    let concepts_bytes = read_file(&target.join(CONCEPTS), MAX_BYTES, "冻结概念表")?;
    if text::sha256(&concepts_bytes) != manifest.concepts_sha256 {
        return Err(integrity("冻结概念表的摘要与清单不符。".to_owned()));
    }
    let concepts_text = std::str::from_utf8(&concepts_bytes)
        .map_err(|error| integrity(format!("冻结概念表必须是UTF-8文本：{error}。")))?;
    let concepts = ConceptFile::parse(concepts_text)?;
    concepts.validate(family.limits.max_concepts)?;

    let inputs_bytes = read_file(&target.join(INPUTS), MAX_BYTES, "冻结输入清单")?;
    if text::sha256(&inputs_bytes) != manifest.inputs_sha256 {
        return Err(integrity("冻结输入清单的摘要与清单不符。".to_owned()));
    }
    let inputs: Inputs = strict(&inputs_bytes, "冻结输入清单")?;
    if inputs.policy != POLICY
        || inputs.schema_version != SCHEMA_VERSION
        || inputs.case_id != family.case_id
    {
        return Err(integrity(
            "冻结输入清单的 policy、结构版本或 case_id 与作者包不符。".to_owned(),
        ));
    }
    if inputs.notes.len() != family.notes.len() || manifest.documents.len() != family.notes.len() {
        return Err(integrity(
            "冻结输入清单、冻结清单与作者包声明的笔记数量不一致。".to_owned(),
        ));
    }

    let text_dir = target.join(TEXT_DIR);
    let text_entries = names(&text_dir)?;
    if text_entries.len() != family.notes.len() {
        return Err(integrity(format!(
            "冻结文本目录必须恰好有 {} 份笔记，实际 {} 项。",
            family.notes.len(),
            text_entries.len()
        )));
    }
    let mut bodies = BTreeMap::new();
    for spec in &family.notes {
        let wanted = format!("{}{}", spec.note_id, TEXT_SUFFIX);
        let path = text_entries
            .get(&wanted)
            .ok_or_else(|| integrity(format!("冻结文本目录缺少笔记 {}。", spec.note_id)))?;
        let bytes = read_file(path, MAX_BYTES, "冻结笔记文本")?;
        let digest = inputs
            .notes
            .iter()
            .find(|digest| digest.note_id == spec.note_id)
            .ok_or_else(|| integrity(format!("冻结输入清单缺少笔记 {}。", spec.note_id)))?;
        if text::sha256(&bytes) != digest.raw_sha256 {
            return Err(integrity(format!(
                "冻结笔记 {} 的正文摘要与输入清单不符。",
                spec.note_id
            )));
        }
        let body = String::from_utf8(bytes)
            .map_err(|error| integrity(format!("冻结笔记不是UTF-8文本：{error}。")))?;
        bodies.insert(spec.note_id.clone(), body);
    }

    Ok(Frozen {
        root: canonical_root,
        manifest,
        inputs,
        family,
        concepts,
        bodies,
    })
}

pub fn build(root: &Path, output: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
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
    let decision = frozen.decide()?;
    let parent = target
        .parent()
        .ok_or_else(|| argument("输出根必须有父目录。".to_owned()))?
        .to_path_buf();
    require_dir(&parent, "输出根的父目录")?;
    // 语义上不允许的情形先拒绝，再建目录：不留半成品。
    if decision.links.counts.generated_broken > 0 {
        return Err(integrity(format!(
            "生成链接里有 {} 条断链；断链必须为 0 才能写出报告。",
            decision.links.counts.generated_broken
        )));
    }
    fs::create_dir(&target).map_err(|error| integrity(format!("无法创建输出根：{error}。")))?;
    let outcome = write_output(&frozen, &decision, &target);
    match outcome {
        Ok(manifest) => Ok(build_summary(&manifest, &target)),
        Err(error) => {
            let _ = fs::remove_dir_all(&target);
            Err(error)
        }
    }
}

fn write_output(
    frozen: &Frozen,
    decision: &Decision,
    target: &Path,
) -> Result<OutputManifest, ProjectError> {
    let structure = format!("{}\n", canonical(&decision.structure)?);
    let links = format!("{}\n", canonical(&decision.links)?);
    let report = format!("{}\n", canonical(&decision.report)?);
    let navigation = decision.navigation.as_str();
    let markdown = decision.markdown.as_str();
    write_new(&target.join(STRUCTURE), structure.as_bytes(), "结构报告")?;
    write_new(&target.join(LINKS), links.as_bytes(), "链接报告")?;
    write_new(&target.join(REPORT), report.as_bytes(), "汇总报告")?;
    write_new(&target.join(NAVIGATION), navigation.as_bytes(), "导航文件")?;
    write_new(&target.join(MARKDOWN), markdown.as_bytes(), "说明文件")?;
    let manifest = OutputManifest {
        policy: POLICY.to_owned(),
        producer: OUTPUT_PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: decision.report.case_id.clone(),
        source_root: frozen.root.display().to_string(),
        source_inputs_sha256: frozen.manifest.inputs_sha256.clone(),
        structure_sha256: text::sha256(structure.as_bytes()),
        links_sha256: text::sha256(links.as_bytes()),
        report_sha256: text::sha256(report.as_bytes()),
        navigation_sha256: text::sha256(navigation.as_bytes()),
        markdown_sha256: text::sha256(markdown.as_bytes()),
        counts: decision.report.counts.clone(),
    };
    let bytes = format!("{}\n", canonical(&manifest)?);
    write_new(&target.join(OUTPUT_MANIFEST), bytes.as_bytes(), "输出清单")?;
    Ok(manifest)
}

fn build_summary(manifest: &OutputManifest, output: &Path) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "输出根 {}：已生成（policy={}，结构版本 {}）\n",
        output.display(),
        manifest.policy,
        manifest.schema_version
    ));
    out.push_str(&format!("冻结来源：{}\n", manifest.source_root));
    out.push_str(&format!(
        "结构一致性：{} 份笔记；就位 {}、错位 {}、多余 {}、缺必填 {}、省可选 {}、空节 {}；一致 {} 份\n",
        manifest.counts.sections.notes,
        manifest.counts.sections.matched,
        manifest.counts.sections.misordered,
        manifest.counts.sections.extra,
        manifest.counts.sections.missing_required,
        manifest.counts.sections.omitted_optional,
        manifest.counts.sections.empty,
        manifest.counts.sections.consistent_notes
    ));
    out.push_str(&format!(
        "链接：正文 {} 条（可解析 {}、目标缺失 {}、标题缺失 {}、有歧义 {}）；生成 {} 条、断链 {}\n",
        manifest.counts.links.source_links,
        manifest.counts.links.source_resolved,
        manifest.counts.links.source_target_missing,
        manifest.counts.links.source_heading_missing,
        manifest.counts.links.source_ambiguous,
        manifest.counts.links.generated_links,
        manifest.counts.links.generated_broken
    ));
    out.push_str(&format!(
        "文件：structure.json {}；links.json {}；report.json {}；navigation.md {}；report.md {}\n",
        short(&manifest.structure_sha256),
        short(&manifest.links_sha256),
        short(&manifest.report_sha256),
        short(&manifest.navigation_sha256),
        short(&manifest.markdown_sha256)
    ));
    out.push_str("模型调用与网络请求：0 次；输出根只含派生报告，不写项目、不写 Vault。\n");
    out
}

pub fn status(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let decision = frozen.decide()?;
    let mut out = String::new();
    out.push_str(&format!(
        "结构根 {}：已建立（policy={}，结构版本 {}）\n",
        frozen.root.display(),
        frozen.manifest.policy,
        frozen.manifest.schema_version
    ));
    out.push_str(&format!(
        "所有权：作者包 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        short(&frozen.manifest.family_sha256),
        short(&frozen.manifest.inputs_sha256),
        frozen.manifest.created_at
    ));
    out.push_str(&format!(
        "冻结来源：{} 份笔记、{} 个骨架节、{} 个概念\n",
        frozen.manifest.documents.len(),
        frozen.family.skeleton.len(),
        frozen.concepts.concepts.len()
    ));
    for note in &decision.structure.notes {
        out.push_str(&format!(
            "  - {} {}：{} 个章节（就位 {}、错位 {}、多余 {}、缺必填 {}、省可选 {}、空节 {}）{}\n",
            note.note_id,
            note.title,
            note.sections.len(),
            note.matched,
            note.misordered,
            note.extra,
            note.missing_required,
            note.omitted_optional,
            note.empty,
            if note.consistent {
                ""
            } else {
                "（结构偏离）"
            }
        ));
    }
    out.push_str(&format!(
        "结构一致性：一致 {} / {} 份\n",
        decision.report.counts.sections.consistent_notes, decision.report.counts.sections.notes
    ));
    out.push_str(&format!(
        "链接：正文 {} 条（可解析 {}、目标缺失 {}、标题缺失 {}、有歧义 {}）；生成 {} 条、断链 {}；外部引用 {} 个\n",
        decision.report.counts.links.source_links,
        decision.report.counts.links.source_resolved,
        decision.report.counts.links.source_target_missing,
        decision.report.counts.links.source_heading_missing,
        decision.report.counts.links.source_ambiguous,
        decision.report.counts.links.generated_links,
        decision.report.counts.links.generated_broken,
        decision.report.counts.links.external_references
    ));
    out.push_str("模型调用与网络请求：0 次；本根只保存冻结输入，不生成知识。\n");
    Ok(out)
}

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned())
}

pub fn list(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let decision = frozen.decide()?;
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!(
        "  \"case_id\": {},\n",
        json(&frozen.manifest.case_id)
    ));
    out.push_str(&format!(
        "  \"policy\": {},\n",
        json(&frozen.manifest.policy)
    ));
    out.push_str(&format!(
        "  \"producer\": {},\n",
        json(&frozen.manifest.producer)
    ));
    out.push_str(&format!(
        "  \"schema_version\": {},\n",
        frozen.manifest.schema_version
    ));
    out.push_str(&format!(
        "  \"root\": {},\n",
        json(&frozen.root.display().to_string())
    ));
    out.push_str(&format!(
        "  \"family_sha256\": {},\n",
        json(&frozen.manifest.family_sha256)
    ));
    out.push_str(&format!(
        "  \"inputs_sha256\": {},\n",
        json(&frozen.manifest.inputs_sha256)
    ));
    out.push_str(&format!(
        "  \"created_at\": {},\n",
        json(&frozen.manifest.created_at)
    ));
    out.push_str(&format!("  \"notes\": {},\n", json(&frozen.inputs.notes)));
    out.push_str(&format!(
        "  \"skeleton\": {},\n",
        json(&frozen.family.skeleton)
    ));
    out.push_str(&format!(
        "  \"structure\": {},\n",
        json(&decision.structure)
    ));
    out.push_str(&format!("  \"links\": {},\n", json(&decision.links)));
    out.push_str(&format!("  \"report\": {},\n", json(&decision.report)));
    out.push_str("  \"model_calls\": 0,\n  \"network_requests\": 0\n}\n");
    Ok(out)
}
