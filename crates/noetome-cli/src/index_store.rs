//! 可移植来源索引的冻结根与确定性输出（B11-T02）。
//!
//! 冻结根只允许 `control/`，里面是工程声明、冻结输入清单与每份输出区文件、原始资料的副本；
//! 运行库、缓存、临时文件**不复制**进冻结根，只在输入清单里留下「被排除」的证据。
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
use crate::index_model::*;
use crate::project::{self, ProjectError};
use crate::text;

pub(crate) const APP_PRODUCER: &str = "noetome-index";
pub(crate) const OUTPUT_PRODUCER: &str = "source_index_output";
pub(crate) const CONTROL: &str = "control";
pub(crate) const MANIFEST: &str = "manifest.json";
pub(crate) const INPUTS: &str = "inputs.json";
pub(crate) const PROJECT: &str = "project.json";
pub(crate) const FILES: &str = "files";
pub(crate) const INDEX: &str = "index.json";
pub(crate) const RELOCATION: &str = "relocation.json";
pub(crate) const REPORT: &str = "report.json";
pub(crate) const MARKDOWN: &str = "report.md";
pub(crate) const OUTPUT_MANIFEST: &str = "manifest.json";
pub(crate) const ROOT_ENTRIES: [&str; 1] = ["control"];
pub(crate) const CONTROL_ENTRIES: [&str; 4] =
    ["manifest.json", "inputs.json", "project.json", "files"];
pub(crate) const OUTPUT_ENTRIES: [&str; 5] = [
    "index.json",
    "manifest.json",
    "relocation.json",
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

/// Walk `dir` and return every regular file as a relative `/`-separated path.
/// Symlinks are skipped by the caller's guard: a relocation scan must never
/// follow a link out of the directory it was pointed at.
fn scan(
    dir: &Path,
    prefix: &str,
    out: &mut Vec<(String, String, u64)>,
) -> Result<(), ProjectError> {
    for (name, path) in names(dir)? {
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| integrity(format!("无法访问 {}：{error}。", path.display())))?;
        if project::is_link(&metadata) {
            continue;
        }
        if metadata.is_dir() {
            scan(&path, &relative, out)?;
        } else if metadata.is_file() {
            let bytes = read_file(&path, MAX_BYTES, "重定位候选")?;
            out.push((relative, text::sha256(&bytes), bytes.len() as u64));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileDigest {
    pub(crate) path: String,
    pub(crate) role: String,
    pub(crate) sha256: String,
    pub(crate) byte_length: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MaterialDigest {
    pub(crate) source_id: String,
    pub(crate) path: String,
    pub(crate) sha256: String,
    pub(crate) byte_length: u64,
}

/// An entry that exists in the runtime library or the temporary area.  Its
/// digest is recorded so that "we saw it and deliberately left it out" is
/// provable; its bytes are deliberately not copied, because copying them is the
/// leak this task exists to prevent.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExcludedDigest {
    pub(crate) path: String,
    pub(crate) sha256: String,
    pub(crate) byte_length: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Inputs {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) output_area: String,
    pub(crate) runtime_area: String,
    pub(crate) temp_area: String,
    pub(crate) relocate_area: String,
    pub(crate) files: Vec<FileDigest>,
    pub(crate) materials: Vec<MaterialDigest>,
    pub(crate) runtime_entries: Vec<ExcludedDigest>,
    pub(crate) temp_entries: Vec<ExcludedDigest>,
    pub(crate) candidates: Vec<ExcludedDigest>,
}

impl Inputs {
    fn runtime_paths(&self) -> Vec<String> {
        self.runtime_entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect()
    }

    fn temp_paths(&self) -> Vec<String> {
        self.temp_entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect()
    }

    fn candidates(&self) -> Vec<Candidate> {
        self.candidates
            .iter()
            .map(|entry| Candidate {
                path: entry.path.clone(),
                sha256: entry.sha256.clone(),
            })
            .collect()
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
    pub(crate) files_identity: Identity,
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
    pub(crate) index_sha256: String,
    pub(crate) relocation_sha256: String,
    pub(crate) report_sha256: String,
    pub(crate) markdown_sha256: String,
    pub(crate) counts: Counts,
}

pub(crate) struct Frozen {
    pub(crate) root: PathBuf,
    pub(crate) manifest: Manifest,
    pub(crate) inputs: Inputs,
    pub(crate) project: Project,
    pub(crate) bodies: BTreeMap<String, String>,
}

impl Frozen {
    pub(crate) fn decide(&self) -> Result<Decision, ProjectError> {
        let notes = self
            .project
            .notes
            .iter()
            .filter_map(|spec| {
                let digest = self
                    .inputs
                    .files
                    .iter()
                    .find(|file| file.path == spec.path)?;
                let text = self.bodies.get(&spec.path)?;
                Some(NoteView {
                    spec,
                    text: text.as_str(),
                    sha256: digest.sha256.as_str(),
                    byte_length: digest.byte_length,
                })
            })
            .collect::<Vec<_>>();
        let resources = self
            .project
            .resources
            .iter()
            .filter_map(|spec| {
                let digest = self
                    .inputs
                    .files
                    .iter()
                    .find(|file| file.path == spec.path)?;
                let text = self.bodies.get(&spec.path)?;
                Some(ResourceView {
                    spec,
                    text: text.as_str(),
                    sha256: digest.sha256.as_str(),
                    byte_length: digest.byte_length,
                })
            })
            .collect::<Vec<_>>();
        let materials = self
            .project
            .sources
            .iter()
            .filter_map(|spec| {
                let digest = self
                    .inputs
                    .materials
                    .iter()
                    .find(|material| material.source_id == spec.source_id)?;
                Some(MaterialView {
                    spec,
                    sha256: digest.sha256.as_str(),
                    byte_length: digest.byte_length,
                })
            })
            .collect::<Vec<_>>();
        if notes.len() != self.project.notes.len()
            || resources.len() != self.project.resources.len()
            || materials.len() != self.project.sources.len()
        {
            return Err(integrity(
                "冻结输入清单与工程声明的文件数量不一致。".to_owned(),
            ));
        }
        decide(
            &self.project,
            &notes,
            &resources,
            &materials,
            &self.inputs.runtime_paths(),
            &self.inputs.temp_paths(),
            &self.inputs.candidates(),
        )
    }
}

fn digest_file(
    package: &Path,
    relative: &str,
    label: &str,
) -> Result<(ExcludedDigest, Vec<u8>), ProjectError> {
    let path = resolve(&package.join(relative))?;
    if !path.starts_with(package) {
        return Err(argument(format!(
            "{label}的路径 {relative} 越出了工程目录；本阶段只读工程内的声明。"
        )));
    }
    let bytes = read_file(&path, MAX_BYTES, label)?;
    Ok((
        ExcludedDigest {
            path: relative.to_owned(),
            sha256: text::sha256(&bytes),
            byte_length: bytes.len() as u64,
        },
        bytes,
    ))
}

fn collect_area(
    package: &Path,
    area: &str,
    label: &str,
) -> Result<Vec<ExcludedDigest>, ProjectError> {
    let dir = resolve(&package.join(area))?;
    if !dir.starts_with(package) {
        return Err(argument(format!("{label}的路径 {area} 越出了工程目录。")));
    }
    if fs::symlink_metadata(&dir).is_err() {
        return Ok(Vec::new());
    }
    require_dir(&dir, label)?;
    let mut found = Vec::new();
    scan(&dir, area, &mut found)?;
    found.sort();
    Ok(found
        .into_iter()
        .map(|(path, sha256, byte_length)| ExcludedDigest {
            path,
            sha256,
            byte_length,
        })
        .collect())
}

pub fn adopt(project_path: &Path, root: &Path) -> Result<String, ProjectError> {
    let package = project_path
        .parent()
        .ok_or_else(|| argument("工程声明必须位于某个目录里。".to_owned()))?;
    let package = fs::canonicalize(package)
        .map_err(|error| argument(format!("无法定位工程目录：{error}。")))?;
    let project_bytes = read_file(project_path, MAX_BYTES, "工程声明")?;
    let project_text = std::str::from_utf8(&project_bytes)
        .map_err(|error| argument(format!("工程声明必须是UTF-8文本：{error}。")))?;
    let project = Project::parse(project_text)?;
    project.validate()?;

    let mut files = Vec::new();
    let mut bodies = BTreeMap::new();
    for spec in &project.notes {
        let (digest, bytes) = digest_file(&package, &spec.path, "笔记正文")?;
        let parsed = text::analyze(&bytes).map_err(|error| {
            argument(format!(
                "笔记 {} 不是合法的 UTF-8 文本（首个非法字节在 {}）：{}。",
                spec.note_id,
                error.valid_up_to(),
                spec.path
            ))
        })?;
        files.push(FileDigest {
            path: spec.path.clone(),
            role: ROLE_NOTE.to_owned(),
            sha256: digest.sha256,
            byte_length: digest.byte_length,
        });
        bodies.insert(spec.path.clone(), parsed.text);
    }
    for spec in &project.resources {
        let (digest, bytes) = digest_file(&package, &spec.path, "资源文件")?;
        let parsed = text::analyze(&bytes).map_err(|error| {
            argument(format!(
                "资源 {} 不是合法的 UTF-8 文本（首个非法字节在 {}）：{}。",
                spec.resource_id,
                error.valid_up_to(),
                spec.path
            ))
        })?;
        files.push(FileDigest {
            path: spec.path.clone(),
            role: ROLE_RESOURCE.to_owned(),
            sha256: digest.sha256,
            byte_length: digest.byte_length,
        });
        bodies.insert(spec.path.clone(), parsed.text);
    }
    let mut materials = Vec::new();
    for spec in &project.sources {
        let (digest, _) = digest_file(&package, &spec.path, "原始资料")?;
        materials.push(MaterialDigest {
            source_id: spec.source_id.clone(),
            path: spec.path.clone(),
            sha256: digest.sha256,
            byte_length: digest.byte_length,
        });
    }
    let runtime_entries = collect_area(&package, &project.runtime_area, "运行库")?;
    let temp_entries = collect_area(&package, &project.temp_area, "临时区")?;
    let candidates = collect_area(&package, &project.relocate_area, "重定位目录")?;

    let inputs = Inputs {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: project.case_id.clone(),
        output_area: project.output_area.clone(),
        runtime_area: project.runtime_area.clone(),
        temp_area: project.temp_area.clone(),
        relocate_area: project.relocate_area.clone(),
        files,
        materials,
        runtime_entries,
        temp_entries,
        candidates,
    };

    // 先在内存里跑一遍判定：语义拒绝不允许留下半个根。
    let frozen = Frozen {
        root: package.clone(),
        manifest: Manifest {
            policy: POLICY.to_owned(),
            producer: APP_PRODUCER.to_owned(),
            schema_version: SCHEMA_VERSION,
            case_id: project.case_id.clone(),
            root: String::new(),
            root_identity: identity(&package)?,
            parent_identity: identity(&package)?,
            control_identity: identity(&package)?,
            manifest_identity: identity(&package)?,
            files_identity: identity(&package)?,
            inputs_identity: identity(&package)?,
            project_sha256: text::sha256(&project_bytes),
            inputs_sha256: text::sha256(canonical(&inputs)?.as_bytes()),
            created_at: String::new(),
        },
        inputs,
        project,
        bodies,
    };
    let decision = frozen.decide()?;

    let target = resolve(root)?;
    let parent = target
        .parent()
        .ok_or_else(|| argument("索引根必须有父目录。".to_owned()))?
        .to_path_buf();
    if fs::symlink_metadata(&target).is_ok() {
        return Err(argument(format!(
            "索引根必须尚不存在：{}；请选择一个新目录。",
            target.display()
        )));
    }
    require_dir(&parent, "索引根的父目录")?;
    let parent_identity = identity(&parent)?;

    fs::create_dir(&target)
        .map_err(|error| integrity(format!("无法创建索引根 {}：{error}。", target.display())))?;
    let root_identity = identity(&target)?;
    let outcome = adopt_into(AdoptInput {
        target: &target,
        project: &frozen.project,
        inputs: &frozen.inputs,
        bodies: &frozen.bodies,
        root_identity: &root_identity,
        parent_identity: &parent_identity,
        project_sha256: text::sha256(&project_bytes),
        project_bytes: &project_bytes,
    });
    match outcome {
        Ok(manifest) => Ok(adopt_summary(&manifest, &decision)),
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
    project: &'a Project,
    inputs: &'a Inputs,
    bodies: &'a BTreeMap<String, String>,
    root_identity: &'a Identity,
    parent_identity: &'a Identity,
    project_sha256: String,
    project_bytes: &'a [u8],
}

fn adopt_into(input: AdoptInput<'_>) -> Result<Manifest, ProjectError> {
    let root = fs::canonicalize(input.target)
        .map_err(|error| integrity(format!("无法定位索引根：{error}。")))?;
    let control = root.join(CONTROL);
    fs::create_dir(&control).map_err(|error| integrity(format!("无法创建控制目录：{error}。")))?;
    let control_identity = identity(&control)?;
    let files_dir = control.join("files");
    fs::create_dir(&files_dir)
        .map_err(|error| integrity(format!("无法创建冻结文件目录：{error}。")))?;
    let files_identity = identity(&files_dir)?;

    // 只复制输出区与原始资料：运行库、缓存与临时文件的字节留在原地，
    // 它们的摘要已经进了输入清单，复制只会把不该跟着输出的东西带出来。
    for file in &input.inputs.files {
        let body = input
            .bodies
            .get(&file.path)
            .ok_or_else(|| integrity(format!("缺少冻结正文：{}。", file.path)))?;
        write_new(
            &files_dir.join(&file.path),
            body.as_bytes(),
            "冻结输出区文件",
        )?;
    }
    write_new(&control.join(PROJECT), input.project_bytes, "冻结工程声明")?;
    let inputs_bytes = canonical(input.inputs)?.into_bytes();
    write_new(&control.join(INPUTS), &inputs_bytes, "冻结输入清单")?;
    write_new(&control.join(MANIFEST), b"", "冻结清单")?;
    let manifest_path = control.join(MANIFEST);
    let manifest_identity = identity(&manifest_path)?;
    let manifest = Manifest {
        policy: POLICY.to_owned(),
        producer: APP_PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: input.project.case_id.clone(),
        root: root.display().to_string(),
        root_identity: input.root_identity.clone(),
        parent_identity: input.parent_identity.clone(),
        control_identity,
        manifest_identity,
        files_identity,
        inputs_identity: identity(&control.join(INPUTS))?,
        project_sha256: input.project_sha256,
        inputs_sha256: text::sha256(&inputs_bytes),
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
    let readback = read_file(&manifest_path, MAX_BYTES, "冻结清单")?;
    if readback != bytes {
        return Err(integrity(
            "写入冻结清单后读回不一致；保留现场并拒绝继续。".to_owned(),
        ));
    }
    Ok(manifest)
}

fn adopt_summary(manifest: &Manifest, decision: &Decision) -> String {
    let counts = &decision.report.counts;
    let mut out = String::new();
    out.push_str(&format!(
        "索引根 {}：已建立（policy={}，索引版本 {}）\n",
        manifest.root, manifest.policy, decision.index.index_version
    ));
    out.push_str(&format!(
        "所有权：工程声明 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        short(&manifest.project_sha256),
        short(&manifest.inputs_sha256),
        manifest.created_at
    ));
    out.push_str(&format!(
        "冻结来源：{} 份笔记、{} 份资源、{} 条来源，输出区 {} 个文件\n",
        counts.notes, counts.resources, counts.sources, counts.output_files
    ));
    out.push_str(&format!(
        "区域分离：输出区 {}；未纳入索引的运行库条目 {} 个、临时区条目 {} 个\n",
        decision.report.areas.output, counts.excluded_runtime_entries, counts.excluded_temp_entries
    ));
    out.push_str(&format!(
        "重定位：在 {} 里核对 {} 条来源，已找到 {}、缺失 {}、摘要不符 {}\n",
        decision.relocation.directory,
        counts.sources,
        counts.relocated,
        counts.missing,
        counts.digest_mismatch
    ));
    out.push_str("模型调用与网络请求：0 次；本根只保存冻结输入，不生成知识。\n");
    out
}

pub(crate) fn open(root: &Path) -> Result<Frozen, ProjectError> {
    let target = resolve(root)?;
    if fs::symlink_metadata(&target).is_err() {
        return Err(unknown_root(format!(
            "{} 不存在，不是已建立的索引根；未修改任何文件。",
            target.display()
        )));
    }
    let control = target.join(CONTROL);
    if fs::symlink_metadata(&control).is_err() {
        return Err(unknown_root(format!(
            "{} 不是索引根（缺少 control/）：陌生目录保持只读，未接管、未修改任何文件。",
            target.display()
        )));
    }
    for name in names(&target)?.keys() {
        if !ROOT_ENTRIES.contains(&name.as_str()) {
            return Err(integrity(format!(
                "索引根顶层只允许 control/；发现多余的 {name}：未修改任何文件。"
            )));
        }
    }
    let control_entries = names(&control)?;
    for name in control_entries.keys() {
        if !CONTROL_ENTRIES.contains(&name.as_str()) {
            return Err(integrity(format!(
                "控制目录只允许 manifest.json、inputs.json、project.json、files；发现多余的 {name}。"
            )));
        }
    }
    for required in CONTROL_ENTRIES {
        if !control_entries.contains_key(required) {
            return Err(integrity(format!("控制目录缺少 {required}。")));
        }
    }
    let manifest_bytes = read_file(&control.join(MANIFEST), MAX_BYTES, "冻结清单")?;
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
        .map_err(|error| integrity(format!("无法定位索引根：{error}。")))?;
    if manifest.root != canonical_root.display().to_string() {
        return Err(integrity(format!(
            "冻结清单记录的根是 {}，与当前路径 {} 不符。",
            manifest.root,
            canonical_root.display()
        )));
    }
    check_identity(&target, &manifest.root_identity, "索引根")?;
    check_identity(&control, &manifest.control_identity, "控制目录")?;
    check_identity(
        &control.join(FILES),
        &manifest.files_identity,
        "冻结文件目录",
    )?;
    check_identity(
        &control.join(MANIFEST),
        &manifest.manifest_identity,
        "冻结清单",
    )?;
    check_identity(
        &control.join(INPUTS),
        &manifest.inputs_identity,
        "冻结输入清单",
    )?;
    if let Some(parent) = target.parent() {
        check_identity(parent, &manifest.parent_identity, "索引根的父目录")?;
    }

    let project_bytes = read_file(&control.join(PROJECT), MAX_BYTES, "冻结工程声明")?;
    if text::sha256(&project_bytes) != manifest.project_sha256 {
        return Err(integrity("冻结工程声明的摘要与清单记录不符。".to_owned()));
    }
    let project_text = std::str::from_utf8(&project_bytes)
        .map_err(|error| integrity(format!("冻结工程声明必须是UTF-8文本：{error}。")))?;
    let project = Project::parse(project_text)?;
    project.validate()?;

    let inputs_bytes = read_file(&control.join(INPUTS), MAX_BYTES, "冻结输入清单")?;
    if text::sha256(&inputs_bytes) != manifest.inputs_sha256 {
        return Err(integrity("冻结输入清单的摘要与清单记录不符。".to_owned()));
    }
    let inputs: Inputs = strict(&inputs_bytes, "冻结输入清单")?;
    if inputs.policy != POLICY || inputs.schema_version != SCHEMA_VERSION {
        return Err(integrity(
            "冻结输入清单的 policy 或 schema_version 不符。".to_owned(),
        ));
    }
    if inputs.case_id != project.case_id {
        return Err(integrity(format!(
            "冻结输入清单的 case_id 是 {}，与工程声明的 {} 不一致。",
            inputs.case_id, project.case_id
        )));
    }
    if inputs.output_area != project.output_area
        || inputs.runtime_area != project.runtime_area
        || inputs.temp_area != project.temp_area
        || inputs.relocate_area != project.relocate_area
    {
        return Err(integrity(
            "冻结输入清单的区域声明与工程声明不符。".to_owned(),
        ));
    }

    // 每一份被复制的文件都必须与清单摘要一致，且不能有一份落在运行库或临时区里。
    let files_dir = control.join("files");
    let mut bodies = BTreeMap::new();
    for file in &inputs.files {
        let path = resolve(&files_dir.join(&file.path))?;
        if !path.starts_with(&files_dir) {
            return Err(integrity(format!(
                "冻结文件的路径 {} 越出了 control/files。",
                file.path
            )));
        }
        let bytes = read_file(&path, MAX_BYTES, "冻结输出区文件")?;
        if text::sha256(&bytes) != file.sha256 {
            return Err(integrity(format!(
                "冻结文件 {} 的摘要与清单记录不符。",
                file.path
            )));
        }
        let parsed = text::analyze(&bytes)
            .map_err(|error| integrity(format!("冻结文件 {} 不是 UTF-8：{error}。", file.path)))?;
        bodies.insert(file.path.clone(), parsed.text);
    }
    let mut copied = Vec::new();
    scan(&files_dir, "", &mut copied)?;
    let declared: Vec<&str> = inputs.files.iter().map(|file| file.path.as_str()).collect();
    if copied.len() != declared.len() {
        return Err(integrity(format!(
            "冻结文件目录里有 {} 个文件，但清单只声明了 {} 个。",
            copied.len(),
            declared.len()
        )));
    }
    for (path, _, _) in &copied {
        if !declared.contains(&path.as_str()) {
            return Err(integrity(format!(
                "冻结文件目录里出现了未声明的文件：{path}。"
            )));
        }
        if under(path, &inputs.runtime_area) || under(path, &inputs.temp_area) {
            return Err(area_leak(format!(
                "冻结文件 {path} 落在运行库或临时区里；输出区与运行库必须分离。"
            )));
        }
    }

    let frozen = Frozen {
        root: canonical_root,
        manifest,
        inputs,
        project,
        bodies,
    };
    // 打开即判定一次：损坏的冻结根不该等到 build 才暴露。
    let _ = frozen.decide()?;
    Ok(frozen)
}

pub fn build(root: &Path, output: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let decision = frozen.decide()?;
    let target = resolve(output)?;
    if fs::symlink_metadata(&target).is_ok() {
        return Err(argument(format!(
            "输出根必须尚不存在：{}；请选择一个新目录。",
            target.display()
        )));
    }
    if target.starts_with(&frozen.root) || frozen.root.starts_with(&target) {
        return Err(argument(
            "输出根不能与索引根互相包含：派生报告必须写在索引根之外。".to_owned(),
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| argument("输出根必须有父目录。".to_owned()))?
        .to_path_buf();
    require_dir(&parent, "输出根的父目录")?;
    fs::create_dir(&target)
        .map_err(|error| integrity(format!("无法创建输出根 {}：{error}。", target.display())))?;
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
    let index_bytes = format!("{}\n", canonical(&decision.index)?).into_bytes();
    let relocation_bytes = format!("{}\n", canonical(&decision.relocation)?).into_bytes();
    let report_bytes = format!("{}\n", canonical(&decision.report)?).into_bytes();
    let markdown_bytes = decision.markdown.clone().into_bytes();
    write_new(&target.join(INDEX), &index_bytes, "索引导出")?;
    write_new(&target.join(RELOCATION), &relocation_bytes, "重定位结果")?;
    write_new(&target.join(REPORT), &report_bytes, "索引报告")?;
    write_new(&target.join(MARKDOWN), &markdown_bytes, "索引说明")?;
    let manifest = OutputManifest {
        policy: POLICY.to_owned(),
        producer: OUTPUT_PRODUCER.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: frozen.project.case_id.clone(),
        source_root: frozen.root.display().to_string(),
        source_inputs_sha256: frozen.manifest.inputs_sha256.clone(),
        index_sha256: text::sha256(&index_bytes),
        relocation_sha256: text::sha256(&relocation_bytes),
        report_sha256: text::sha256(&report_bytes),
        markdown_sha256: text::sha256(&markdown_bytes),
        counts: decision.report.counts.clone(),
    };
    let manifest_bytes = format!("{}\n", canonical(&manifest)?).into_bytes();
    write_new(&target.join(OUTPUT_MANIFEST), &manifest_bytes, "输出清单")?;
    // 输出根的文件集合本身也是合同的一部分：多一个少一个都算布局漂移。
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
    let mut out = String::new();
    out.push_str(&format!(
        "输出根 {}：已生成（policy={}，索引版本 {}）\n",
        output.display(),
        manifest.policy,
        INDEX_VERSION
    ));
    out.push_str(&format!("冻结来源：{}\n", manifest.source_root));
    out.push_str(&format!(
        "索引：{} 个文件（笔记 {}、资源 {}）、来源 {} 条\n",
        manifest.counts.output_files,
        manifest.counts.notes,
        manifest.counts.resources,
        manifest.counts.sources
    ));
    out.push_str(&format!(
        "重定位：已找到 {} 条、缺失 {} 条、摘要不符 {} 条\n",
        manifest.counts.relocated, manifest.counts.missing, manifest.counts.digest_mismatch
    ));
    out.push_str(&format!(
        "文件：index.json {}；relocation.json {}；report.json {}；report.md {}\n",
        short(&manifest.index_sha256),
        short(&manifest.relocation_sha256),
        short(&manifest.report_sha256),
        short(&manifest.markdown_sha256)
    ));
    out.push_str("模型调用与网络请求：0 次；输出根只含派生索引，不写项目、不写 Vault。\n");
    out
}

pub fn status(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let decision = frozen.decide()?;
    let counts = &decision.report.counts;
    let mut out = String::new();
    out.push_str(&format!(
        "索引根 {}：已建立（policy={}，索引版本 {}）\n",
        frozen.root.display(),
        frozen.manifest.policy,
        decision.index.index_version
    ));
    out.push_str(&format!(
        "所有权：工程声明 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        short(&frozen.manifest.project_sha256),
        short(&frozen.manifest.inputs_sha256),
        frozen.manifest.created_at
    ));
    out.push_str(&format!(
        "区域：输出区 {}；运行库 {}；临时区 {}；重定位目录 {}\n",
        decision.report.areas.output,
        decision.report.areas.runtime,
        decision.report.areas.temp,
        decision.report.areas.relocate
    ));
    out.push_str(&format!(
        "冻结来源：{} 份笔记、{} 份资源、{} 条来源、{} 个标题\n",
        counts.notes, counts.resources, counts.sources, counts.headings
    ));
    for note in &decision.index.notes {
        out.push_str(&format!(
            "  - {} {}：{} 个标题\n",
            note.note_id,
            note.title,
            note.headings.len()
        ));
    }
    out.push_str(&format!(
        "排除项：运行库 {} 个条目、临时区 {} 个条目未进入索引\n",
        counts.excluded_runtime_entries, counts.excluded_temp_entries
    ));
    out.push_str(&format!(
        "正文负担：文献编号 {} 处、绝对路径 {} 处、疑似凭据 {} 处\n",
        counts.citation_marks, counts.absolute_paths, counts.secret_markers
    ));
    out.push_str(&format!(
        "重定位：在 {} 里已找到 {}、缺失 {}、摘要不符 {}\n",
        decision.relocation.directory, counts.relocated, counts.missing, counts.digest_mismatch
    ));
    out.push_str("模型调用与网络请求：0 次；本根只保存冻结输入，不生成知识。\n");
    Ok(out)
}

fn json<T: Serialize>(value: &T) -> Result<String, ProjectError> {
    serde_json::to_string(value).map_err(|error| integrity(format!("无法序列化记录：{error}。")))
}

pub fn list(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let decision = frozen.decide()?;
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!(
        "  \"case_id\": {},\n",
        json(&frozen.project.case_id)?
    ));
    out.push_str(&format!("  \"policy\": {},\n", json(&POLICY)?));
    out.push_str(&format!("  \"producer\": {},\n", json(&APP_PRODUCER)?));
    out.push_str(&format!("  \"schema_version\": {},\n", SCHEMA_VERSION));
    out.push_str(&format!("  \"index_version\": {},\n", INDEX_VERSION));
    out.push_str(&format!(
        "  \"root\": {},\n",
        json(&frozen.root.display().to_string())?
    ));
    out.push_str(&format!(
        "  \"project_sha256\": {},\n",
        json(&frozen.manifest.project_sha256)?
    ));
    out.push_str(&format!(
        "  \"inputs_sha256\": {},\n",
        json(&frozen.manifest.inputs_sha256)?
    ));
    out.push_str(&format!(
        "  \"created_at\": {},\n",
        json(&frozen.manifest.created_at)?
    ));
    out.push_str(&format!(
        "  \"areas\": {},\n",
        json(&decision.report.areas)?
    ));
    out.push_str(&format!("  \"files\": {},\n", json(&decision.index.files)?));
    out.push_str(&format!("  \"notes\": {},\n", json(&decision.index.notes)?));
    out.push_str(&format!(
        "  \"sources\": {},\n",
        json(&decision.index.sources)?
    ));
    out.push_str(&format!(
        "  \"relocation\": {},\n",
        json(&decision.relocation)?
    ));
    out.push_str(&format!(
        "  \"counts\": {},\n",
        json(&decision.report.counts)?
    ));
    out.push_str("  \"model_calls\": 0,\n");
    out.push_str("  \"network_requests\": 0\n");
    out.push_str("}\n");
    Ok(out)
}

/// Scan a live directory and report where each frozen source now lives.
/// Nothing is written: relocation is a read-only lookup, and a missing or
/// edited file produces "needs a rebuilt mapping" instead of a guessed target.
pub fn relocate(root: &Path, directory: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let decision = frozen.decide()?;
    let target = resolve(directory)?;
    require_dir(&target, "重定位目录")?;
    let mut found = Vec::new();
    scan(&target, "", &mut found)?;
    let candidates = found
        .into_iter()
        .map(|(path, sha256, _)| Candidate { path, sha256 })
        .collect::<Vec<_>>();
    let relocation = relocation_of(
        &frozen.project.case_id,
        &target.display().to_string(),
        &decision.index.sources,
        &candidates,
    );
    let mut out = String::new();
    out.push_str(&format!(
        "重定位目录 {}：{} 个候选文件\n",
        target.display(),
        candidates.len()
    ));
    for entry in &relocation.sources {
        out.push_str(&format!(
            "  - {} {}：{}\n",
            entry.source_id, entry.status, entry.detail
        ));
    }
    out.push_str(&format!(
        "合计：已找到 {}、缺失 {}、摘要不符 {}；模型调用与网络请求 0 次。\n",
        relocation.relocated, relocation.missing, relocation.digest_mismatch
    ));
    Ok(out)
}
