//! 小节提升为独立笔记与链接批量更新的冻结根与确定性输出（B11-T01）。
//!
//! 冻结根只允许 `control/`，里面是迁移计划、知识对象表、提升前后两份笔记的原始字节与清单；
//! 输出根固定五个文件，同一冻结输入的两次构建必须字节相同。

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::compile_identity::{Identity, identity};
use crate::migrate_model::*;
use crate::project::{self, ProjectError};
use crate::text;

pub(crate) const APP_PRODUCER: &str = "noetome-migrate";
pub(crate) const OUTPUT_PRODUCER: &str = "link_migration_output";
pub(crate) const CONTROL: &str = "control";
pub(crate) const MANIFEST: &str = "control/manifest.json";
pub(crate) const INPUTS: &str = "control/inputs.json";
pub(crate) const PLAN: &str = "control/plan.json";
pub(crate) const OBJECTS: &str = "control/objects.json";
pub(crate) const TEXT_SUFFIX: &str = ".md";
pub(crate) const LINKS: &str = "links.json";
pub(crate) const PLACEMENTS: &str = "objects.json";
pub(crate) const REPORT: &str = "report.json";
pub(crate) const MARKDOWN: &str = "report.md";
pub(crate) const OUTPUT_MANIFEST: &str = "manifest.json";
pub(crate) const ROOT_ENTRIES: [&str; 1] = ["control"];
pub(crate) const CONTROL_ENTRIES: [&str; 6] = [
    "manifest.json",
    "inputs.json",
    "plan.json",
    "objects.json",
    "before",
    "after",
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
    pub(crate) side: String,
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) normalized_sha256: String,
    pub(crate) byte_length: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoteDigest {
    pub(crate) side: String,
    pub(crate) note_id: String,
    pub(crate) path: String,
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) counterpart: Option<String>,
    #[serde(default)]
    pub(crate) change: Option<String>,
    pub(crate) raw_sha256: String,
    pub(crate) byte_length: u64,
    pub(crate) normalized_sha256: String,
    pub(crate) span_count: usize,
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
    pub(crate) plan_sha256: String,
    pub(crate) objects_sha256: String,
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
    pub(crate) before_dir: String,
    pub(crate) after_dir: String,
    pub(crate) notes: Vec<NoteDigest>,
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
    pub(crate) placements_sha256: String,
    pub(crate) report_sha256: String,
    pub(crate) markdown_sha256: String,
    pub(crate) counts: Counts,
}

pub(crate) struct Frozen {
    pub(crate) root: PathBuf,
    pub(crate) manifest: Manifest,
    pub(crate) inputs: Inputs,
    pub(crate) plan: Plan,
    pub(crate) objects: ObjectFile,
    pub(crate) before: BTreeMap<String, String>,
    pub(crate) after: BTreeMap<String, String>,
}

impl Frozen {
    fn views<'a>(specs: &'a [NoteSpec], bodies: &'a BTreeMap<String, String>) -> Vec<NoteView<'a>> {
        specs
            .iter()
            .filter_map(|spec| {
                bodies.get(&spec.note_id).map(|text| NoteView {
                    spec,
                    text: text.as_str(),
                })
            })
            .collect()
    }

    pub(crate) fn decide(&self) -> Result<Decision, ProjectError> {
        let before = Self::views(&self.plan.before, &self.before);
        let after = Self::views(&self.plan.after, &self.after);
        if before.len() != self.plan.before.len() || after.len() != self.plan.after.len() {
            return Err(integrity(
                "冻结文本与迁移计划声明的笔记数量不一致。".to_owned(),
            ));
        }
        decide(&self.plan, &self.objects, &before, &after)
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

fn digest_of(
    package: &Path,
    side: &str,
    dir: &str,
    spec: &NoteSpec,
) -> Result<(NoteDigest, String), ProjectError> {
    let relative = format!("{dir}/{}", spec.path);
    let bytes = read_package_file(package, &relative, "笔记正文")?;
    let parsed = text::analyze(&bytes).map_err(|error| {
        argument(format!(
            "{side} 笔记 {} 不是合法的 UTF-8 文本（首个非法字节在 {}）：{relative}。",
            spec.note_id,
            error.valid_up_to()
        ))
    })?;
    let digest = NoteDigest {
        side: side.to_owned(),
        note_id: spec.note_id.clone(),
        path: spec.path.clone(),
        title: spec.title.clone(),
        counterpart: spec.counterpart.clone(),
        change: spec.change.clone(),
        raw_sha256: text::sha256(&bytes),
        byte_length: bytes.len() as u64,
        normalized_sha256: parsed.normalized_sha256.clone(),
        span_count: parsed.spans.len(),
    };
    Ok((digest, parsed.text))
}

pub fn adopt(plan_path: &Path, root: &Path) -> Result<String, ProjectError> {
    let package = plan_path
        .parent()
        .ok_or_else(|| argument("迁移计划必须位于某个目录里。".to_owned()))?;
    let package = fs::canonicalize(package)
        .map_err(|error| argument(format!("无法定位计划包目录：{error}。")))?;
    let plan_bytes = read_file(plan_path, MAX_BYTES, "迁移计划")?;
    let plan_text = std::str::from_utf8(&plan_bytes)
        .map_err(|error| argument(format!("迁移计划必须是UTF-8文本：{error}。")))?;
    let plan = Plan::parse(plan_text)?;
    plan.validate()?;
    let objects_bytes = read_package_file(&package, &plan.objects, "知识对象表")?;
    let objects_text = std::str::from_utf8(&objects_bytes)
        .map_err(|error| argument(format!("知识对象表必须是UTF-8文本：{error}。")))?;
    let objects = ObjectFile::parse(objects_text)?;
    objects.validate(&plan.limits)?;
    if objects.case_id != plan.case_id {
        return Err(argument(format!(
            "知识对象表的 case_id 是 {}，与迁移计划的 {} 不一致。",
            objects.case_id, plan.case_id
        )));
    }

    let mut digests = Vec::new();
    let mut before = BTreeMap::new();
    for spec in &plan.before {
        let (digest, body) = digest_of(&package, SIDE_BEFORE, &plan.before_dir, spec)?;
        digests.push(digest);
        before.insert(spec.note_id.clone(), body);
    }
    let mut after = BTreeMap::new();
    for spec in &plan.after {
        let (digest, body) = digest_of(&package, SIDE_AFTER, &plan.after_dir, spec)?;
        digests.push(digest);
        after.insert(spec.note_id.clone(), body);
    }

    // 先在内存里跑一遍判定：语义拒绝不允许留下半个根。
    let before_views = Frozen::views(&plan.before, &before);
    let after_views = Frozen::views(&plan.after, &after);
    let decision = decide(&plan, &objects, &before_views, &after_views)?;

    let target = resolve(root)?;
    let parent = target
        .parent()
        .ok_or_else(|| argument("迁移根必须有父目录。".to_owned()))?
        .to_path_buf();
    if fs::symlink_metadata(&target).is_ok() {
        return Err(argument(format!(
            "迁移根必须尚不存在：{}；请选择一个新目录。",
            target.display()
        )));
    }
    require_dir(&parent, "迁移根的父目录")?;
    let parent_identity = identity(&parent)?;

    fs::create_dir(&target)
        .map_err(|error| integrity(format!("无法创建迁移根 {}：{error}。", target.display())))?;
    let root_identity = identity(&target)?;
    let outcome = adopt_into(AdoptInput {
        target: &target,
        plan: &plan,
        digests: &digests,
        before: &before,
        after: &after,
        root_identity: &root_identity,
        parent_identity: &parent_identity,
        plan_sha256: text::sha256(&plan_bytes),
        objects_sha256: text::sha256(&objects_bytes),
        plan_bytes: &plan_bytes,
        objects_bytes: &objects_bytes,
    });
    match outcome {
        Ok(manifest) => Ok(adopt_summary(&manifest, &plan, &decision)),
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
    plan: &'a Plan,
    digests: &'a [NoteDigest],
    before: &'a BTreeMap<String, String>,
    after: &'a BTreeMap<String, String>,
    root_identity: &'a Identity,
    parent_identity: &'a Identity,
    plan_sha256: String,
    objects_sha256: String,
    plan_bytes: &'a [u8],
    objects_bytes: &'a [u8],
}

fn adopt_into(input: AdoptInput<'_>) -> Result<Manifest, ProjectError> {
    let root = fs::canonicalize(input.target)
        .map_err(|error| integrity(format!("无法定位迁移根：{error}。")))?;
    let control = root.join(CONTROL);
    fs::create_dir(&control).map_err(|error| integrity(format!("无法创建控制目录：{error}。")))?;
    let control_identity = identity(&control)?;
    let before_dir = control.join("before");
    fs::create_dir(&before_dir)
        .map_err(|error| integrity(format!("无法创建提升前文本目录：{error}。")))?;
    let after_dir = control.join("after");
    fs::create_dir(&after_dir)
        .map_err(|error| integrity(format!("无法创建提升后文本目录：{error}。")))?;
    for spec in &input.plan.before {
        let body = input
            .before
            .get(&spec.note_id)
            .ok_or_else(|| integrity(format!("缺少提升前笔记 {} 的正文。", spec.note_id)))?;
        write_new(
            &before_dir.join(format!("{}{}", spec.note_id, TEXT_SUFFIX)),
            body.as_bytes(),
            "冻结提升前文本",
        )?;
    }
    for spec in &input.plan.after {
        let body = input
            .after
            .get(&spec.note_id)
            .ok_or_else(|| integrity(format!("缺少提升后笔记 {} 的正文。", spec.note_id)))?;
        write_new(
            &after_dir.join(format!("{}{}", spec.note_id, TEXT_SUFFIX)),
            body.as_bytes(),
            "冻结提升后文本",
        )?;
    }
    write_new(&control.join("plan.json"), input.plan_bytes, "冻结迁移计划")?;
    write_new(
        &control.join("objects.json"),
        input.objects_bytes,
        "冻结知识对象表",
    )?;
    let inputs = Inputs {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: input.plan.case_id.clone(),
        before_dir: input.plan.before_dir.clone(),
        after_dir: input.plan.after_dir.clone(),
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
        case_id: input.plan.case_id.clone(),
        root: root.display().to_string(),
        root_identity: input.root_identity.clone(),
        parent_identity: input.parent_identity.clone(),
        control_identity,
        manifest_identity,
        inputs_identity: identity(&control.join("inputs.json"))?,
        plan_sha256: input.plan_sha256,
        objects_sha256: input.objects_sha256,
        inputs_sha256: text::sha256(&inputs_bytes),
        documents: input
            .digests
            .iter()
            .map(|digest| DocumentDigest {
                side: digest.side.clone(),
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

fn adopt_summary(manifest: &Manifest, plan: &Plan, decision: &Decision) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "迁移根 {}：已建立（policy={}，结构版本 {}）\n",
        manifest.root, manifest.policy, manifest.schema_version
    ));
    out.push_str(&format!(
        "所有权：计划包 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        short(&manifest.plan_sha256),
        short(&manifest.inputs_sha256),
        manifest.created_at
    ));
    out.push_str(&format!(
        "冻结来源：{} 份提升前笔记、{} 份提升后笔记、{} 个知识对象，共 {} 字节\n",
        plan.before.len(),
        plan.after.len(),
        decision.objects.counts.objects,
        manifest
            .documents
            .iter()
            .map(|digest| digest.byte_length)
            .sum::<u64>()
    ));
    out.push_str(&format!(
        "迁移计划：{} 条提升；核对后重写 {} 条引用\n",
        plan.promotions.len(),
        decision.links.counts.rewritten_targets
    ));
    out.push_str("模型调用与网络请求：0 次；本根只保存冻结输入，不生成知识。\n");
    out
}

pub(crate) fn open(root: &Path) -> Result<Frozen, ProjectError> {
    let target = resolve(root)?;
    if fs::symlink_metadata(&target).is_err() {
        return Err(unknown_root(format!(
            "{} 不存在，不是已建立的迁移根；未修改任何文件。",
            target.display()
        )));
    }
    let control = target.join(CONTROL);
    if fs::symlink_metadata(&control).is_err() {
        return Err(unknown_root(format!(
            "{} 不是迁移根（缺少 control/）：陌生目录保持只读，未接管、未修改任何文件。",
            target.display()
        )));
    }
    let root_entries = names(&target)?;
    for name in root_entries.keys() {
        if !ROOT_ENTRIES.contains(&name.as_str()) {
            return Err(integrity(format!(
                "迁移根顶层只允许 control/；发现多余的 {name}：未修改任何文件。"
            )));
        }
    }
    let control_entries = names(&control)?;
    for name in control_entries.keys() {
        if !CONTROL_ENTRIES.contains(&name.as_str()) {
            return Err(integrity(format!(
                "控制目录只允许 manifest.json、inputs.json、plan.json、objects.json、before、after；发现多余的 {name}。"
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
        .map_err(|error| integrity(format!("无法定位迁移根：{error}。")))?;
    if manifest.root != canonical_root.display().to_string() {
        return Err(integrity(format!(
            "冻结清单记录的根是 {}，与当前路径 {} 不符。",
            manifest.root,
            canonical_root.display()
        )));
    }
    check_identity(&target, &manifest.root_identity, "迁移根")?;
    check_identity(&control, &manifest.control_identity, "控制目录")?;
    let parent = target
        .parent()
        .ok_or_else(|| integrity("迁移根没有父目录。".to_owned()))?;
    check_identity(parent, &manifest.parent_identity, "迁移根的父目录")?;
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

    let plan_bytes = read_file(&target.join(PLAN), MAX_BYTES, "冻结迁移计划")?;
    if text::sha256(&plan_bytes) != manifest.plan_sha256 {
        return Err(integrity("冻结迁移计划的摘要与清单不符。".to_owned()));
    }
    let plan_text = std::str::from_utf8(&plan_bytes)
        .map_err(|error| integrity(format!("冻结迁移计划必须是UTF-8文本：{error}。")))?;
    let plan = Plan::parse(plan_text)?;
    plan.validate()?;
    let objects_bytes = read_file(&target.join(OBJECTS), MAX_BYTES, "冻结知识对象表")?;
    if text::sha256(&objects_bytes) != manifest.objects_sha256 {
        return Err(integrity("冻结知识对象表的摘要与清单不符。".to_owned()));
    }
    let objects_text = std::str::from_utf8(&objects_bytes)
        .map_err(|error| integrity(format!("冻结知识对象表必须是UTF-8文本：{error}。")))?;
    let objects = ObjectFile::parse(objects_text)?;
    objects.validate(&plan.limits)?;
    if objects.case_id != plan.case_id {
        return Err(integrity(format!(
            "冻结知识对象表的 case_id 是 {}，与迁移计划的 {} 不一致。",
            objects.case_id, plan.case_id
        )));
    }

    let inputs_bytes = read_file(&target.join(INPUTS), MAX_BYTES, "冻结输入清单")?;
    if text::sha256(&inputs_bytes) != manifest.inputs_sha256 {
        return Err(integrity("冻结输入清单的摘要与清单不符。".to_owned()));
    }
    let inputs: Inputs = strict(&inputs_bytes, "冻结输入清单")?;
    if inputs.policy != POLICY || inputs.schema_version != SCHEMA_VERSION {
        return Err(integrity(
            "冻结输入清单的 policy 或结构版本与作者包不符。".to_owned(),
        ));
    }
    if inputs.case_id != plan.case_id
        || inputs.before_dir != plan.before_dir
        || inputs.after_dir != plan.after_dir
    {
        return Err(integrity(
            "冻结输入清单记录的 case_id 或两侧目录与迁移计划不符。".to_owned(),
        ));
    }
    let expected_notes = plan.before.len() + plan.after.len();
    if inputs.notes.len() != expected_notes || manifest.documents.len() != expected_notes {
        return Err(integrity(
            "冻结输入清单、冻结清单与迁移计划声明的笔记数量不一致。".to_owned(),
        ));
    }

    let before = read_side(&target, "before", &plan.before, &inputs, SIDE_BEFORE)?;
    let after = read_side(&target, "after", &plan.after, &inputs, SIDE_AFTER)?;

    Ok(Frozen {
        root: canonical_root,
        manifest,
        inputs,
        plan,
        objects,
        before,
        after,
    })
}

fn read_side(
    target: &Path,
    dir_name: &str,
    specs: &[NoteSpec],
    inputs: &Inputs,
    side: &str,
) -> Result<BTreeMap<String, String>, ProjectError> {
    let dir = target.join(CONTROL).join(dir_name);
    require_dir(&dir, "冻结文本目录")?;
    let entries = names(&dir)?;
    if entries.len() != specs.len() {
        return Err(integrity(format!(
            "冻结文本目录 {dir_name} 必须恰好有 {} 份笔记，实际 {} 项。",
            specs.len(),
            entries.len()
        )));
    }
    let mut bodies = BTreeMap::new();
    for spec in specs {
        let wanted = format!("{}{}", spec.note_id, TEXT_SUFFIX);
        let path = entries.get(&wanted).ok_or_else(|| {
            integrity(format!(
                "冻结文本目录 {dir_name} 缺少笔记 {}。",
                spec.note_id
            ))
        })?;
        let bytes = read_file(path, MAX_BYTES, "冻结笔记文本")?;
        let digest = inputs
            .notes
            .iter()
            .find(|digest| digest.side == side && digest.note_id == spec.note_id)
            .ok_or_else(|| {
                integrity(format!("冻结输入清单缺少 {side} 侧笔记 {}。", spec.note_id))
            })?;
        if text::sha256(&bytes) != digest.raw_sha256 {
            return Err(integrity(format!(
                "冻结笔记 {}（{side} 侧）的正文摘要与输入清单不符。",
                spec.note_id
            )));
        }
        let body = String::from_utf8(bytes)
            .map_err(|error| integrity(format!("冻结笔记不是UTF-8文本：{error}。")))?;
        bodies.insert(spec.note_id.clone(), body);
    }
    Ok(bodies)
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
            "输出根不能与迁移根互相包含：派生报告必须写在迁移根之外。".to_owned(),
        ));
    }
    let decision = frozen.decide()?;
    let parent = target
        .parent()
        .ok_or_else(|| argument("输出根必须有父目录。".to_owned()))?
        .to_path_buf();
    require_dir(&parent, "输出根的父目录")?;
    // 语义上不允许的情形先拒绝，再建目录：不留半成品。
    if decision.links.counts.broken_links > 0 {
        return Err(integrity(format!(
            "迁移后仍有 {} 条断链；断链必须为 0 才能写出迁移报告。",
            decision.links.counts.broken_links
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
    let links = format!("{}\n", canonical(&decision.links)?);
    let placements = format!("{}\n", canonical(&decision.objects)?);
    let report = format!("{}\n", canonical(&decision.report)?);
    let markdown = decision.markdown.as_str();
    write_new(&target.join(LINKS), links.as_bytes(), "链接报告")?;
    write_new(
        &target.join(PLACEMENTS),
        placements.as_bytes(),
        "对象位置报告",
    )?;
    write_new(&target.join(REPORT), report.as_bytes(), "汇总报告")?;
    write_new(&target.join(MARKDOWN), markdown.as_bytes(), "说明文件")?;
    let manifest = OutputManifest {
        policy: POLICY.to_owned(),
        producer: OUTPUT_PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: decision.report.case_id.clone(),
        source_root: frozen.root.display().to_string(),
        source_inputs_sha256: frozen.manifest.inputs_sha256.clone(),
        links_sha256: text::sha256(links.as_bytes()),
        placements_sha256: text::sha256(placements.as_bytes()),
        report_sha256: text::sha256(report.as_bytes()),
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
        "迁移核对：{} 份提升前笔记 → {} 份提升后笔记；知识对象 {} 个（被引用 {}、未被引用 {}）\n",
        manifest.counts.notes_before,
        manifest.counts.notes_after,
        manifest.counts.objects,
        manifest.counts.objects_referenced,
        manifest.counts.objects_unreferenced
    ));
    out.push_str(&format!(
        "链接：迁移前 {} 条、迁移后 {} 条；规范位置 {} 条、重定向位置 {} 条；重写目标 {} 条、断链 {} 条\n",
        manifest.counts.links_before,
        manifest.counts.links_after,
        manifest.counts.canonical_links_after,
        manifest.counts.redirect_links_after,
        manifest.counts.rewritten_targets,
        manifest.counts.broken_links
    ));
    out.push_str(&format!(
        "文件：links.json {}；objects.json {}；report.json {}；report.md {}\n",
        short(&manifest.links_sha256),
        short(&manifest.placements_sha256),
        short(&manifest.report_sha256),
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
        "迁移根 {}：已建立（policy={}，结构版本 {}）\n",
        frozen.root.display(),
        frozen.manifest.policy,
        frozen.manifest.schema_version
    ));
    out.push_str(&format!(
        "所有权：计划包 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        short(&frozen.manifest.plan_sha256),
        short(&frozen.manifest.inputs_sha256),
        frozen.manifest.created_at
    ));
    out.push_str(&format!(
        "冻结来源：{} 份提升前笔记、{} 份提升后笔记、{} 个知识对象\n",
        frozen.plan.before.len(),
        frozen.plan.after.len(),
        frozen.objects.objects.len()
    ));
    for note in decision
        .report
        .notes
        .iter()
        .filter(|note| note.side == SIDE_AFTER)
    {
        out.push_str(&format!(
            "  - {} {}（{}）：字节相等 {}、剥链接后相等 {}、提升满足 {}\n",
            note.note_id,
            note.path,
            note.change,
            yes_no(note.bytes_equal),
            yes_no(note.stripped_equal),
            yes_no(note.promotion_satisfied)
        ));
    }
    out.push_str(&format!(
        "引用身份：{} 个知识对象被引用 {} 个、未被引用 {} 个；身份违规 {} 个\n",
        decision.objects.counts.objects,
        decision.objects.counts.objects_referenced,
        decision.objects.counts.objects_unreferenced,
        decision.objects.counts.identity_violations
    ));
    out.push_str(&format!(
        "链接：迁移前 {} 条（可解析 {}）、迁移后 {} 条（可解析 {}）；规范位置 {} 条、重定向位置 {} 条；重写目标 {} 条\n",
        decision.links.counts.links_before,
        decision.links.counts.resolved_before,
        decision.links.counts.links_after,
        decision.links.counts.resolved_after,
        decision.links.counts.canonical_links_after,
        decision.links.counts.redirect_links_after,
        decision.links.counts.rewritten_targets
    ));
    out.push_str(&format!(
        "断链：{} 条\n",
        decision.links.counts.broken_links
    ));
    out.push_str("模型调用与网络请求：0 次；本根只保存冻结输入，不生成知识。\n");
    Ok(out)
}

fn yes_no(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "是",
        Some(false) => "否",
        None => "不适用",
    }
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
        "  \"plan_sha256\": {},\n",
        json(&frozen.manifest.plan_sha256)
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
        "  \"objects\": {},\n",
        json(&frozen.objects.objects)
    ));
    out.push_str(&format!(
        "  \"promotions\": {},\n",
        json(&frozen.plan.promotions)
    ));
    out.push_str(&format!("  \"links\": {},\n", json(&decision.links)));
    out.push_str(&format!("  \"placements\": {},\n", json(&decision.objects)));
    out.push_str(&format!("  \"report\": {},\n", json(&decision.report)));
    out.push_str("  \"model_calls\": 0,\n  \"network_requests\": 0\n}\n");
    Ok(out)
}
