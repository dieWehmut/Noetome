//! Frozen input root and deterministic output for chapter alignment.
//!
//! The root holds only frozen inputs: the author declaration, the normalized
//! text of every declared document, the span list derived from that text, and
//! the concept lexicon. There is no ledger and no state machine — nothing in
//! this capability mutates a root after adopt. `build` re-reads the frozen
//! inputs, re-verifies every digest, and writes a brand new output root.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::compile_identity::{Identity, identity};
use crate::outline_model::*;
use crate::project::{self, ProjectError};
use crate::text;

pub(crate) const APP_PRODUCER: &str = "noetome-outline";
pub(crate) const OUTPUT_PRODUCER: &str = "outline_alignment_output";
pub(crate) const CONTROL: &str = "control";
pub(crate) const MANIFEST: &str = "control/manifest.json";
pub(crate) const INPUTS: &str = "control/inputs.json";
pub(crate) const TEXT_DIR: &str = "control/text";
pub(crate) const LEXICON_SUFFIX: &str = ".lexicon.json";
pub(crate) const TEXT_SUFFIX: &str = ".md";
pub(crate) const TREE: &str = "tree.json";
pub(crate) const CONCEPTS: &str = "concepts.json";
pub(crate) const REPORT: &str = "report.json";
pub(crate) const MARKDOWN: &str = "report.md";
pub(crate) const OUTPUT_MANIFEST: &str = "manifest.json";
pub(crate) const ROOT_ENTRIES: [&str; 1] = [CONTROL];
pub(crate) const CONTROL_ENTRIES: [&str; 3] = ["manifest.json", "inputs.json", "text"];

/// Frozen-input manifest written at adopt and re-verified on every read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub(crate) author_manifest_sha256: String,
    pub(crate) lexicon_sha256: String,
    pub(crate) inputs_sha256: String,
    pub(crate) documents: Vec<DocumentDigest>,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentDigest {
    pub(crate) source_id: String,
    pub(crate) normalized_sha256: String,
    pub(crate) span_count: usize,
    pub(crate) byte_length: u64,
}

/// Self-describing record of one derived output root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OutputManifest {
    pub(crate) policy: String,
    pub(crate) producer: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) source_root: String,
    pub(crate) source_inputs_sha256: String,
    pub(crate) tree_sha256: String,
    pub(crate) concepts_sha256: String,
    pub(crate) report_sha256: String,
    pub(crate) markdown_sha256: String,
    pub(crate) counts: Counts,
}

fn canonical<T: Serialize>(value: &T) -> Result<String, ProjectError> {
    serde_json::to_string(value).map_err(|error| integrity(format!("无法序列化记录：{error}。")))
}

fn strict<T>(bytes: &[u8], label: &str) -> Result<T, ProjectError>
where
    T: serde::de::DeserializeOwned + Serialize,
{
    let text = std::str::from_utf8(bytes)
        .map_err(|error| integrity(format!("{label}必须是UTF-8文本：{error}。")))?;
    let value: T = serde_json::from_str(text)
        .map_err(|error| integrity(format!("{label}不是严格JSON：{error}。")))?;
    if canonical(&value)? != text {
        return Err(integrity(format!("{label}非规范编码、重复键或未知字段。")));
    }
    Ok(value)
}

fn now() -> Result<String, ProjectError> {
    Ok(OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| integrity(format!("无法生成时间戳：{error}。")))?
        .to_string())
}

fn read_file(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>, ProjectError> {
    let mut file = project::read_guard(path)?;
    let metadata = file
        .metadata()
        .map_err(|error| ProjectError::io(path, error))?;
    if !metadata.is_file() {
        return Err(integrity(format!(
            "{label}必须是普通文件：{}。",
            path.display()
        )));
    }
    if metadata.len() > limit {
        return Err(integrity(format!(
            "{label}超过 {limit} 字节上限：{}。",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| ProjectError::io(path, error))?;
    if bytes.len() as u64 > limit {
        return Err(integrity(format!(
            "{label}超过 {limit} 字节上限：{}。",
            path.display()
        )));
    }
    Ok(bytes)
}

fn create_parent(path: &Path) -> Result<(), ProjectError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| ProjectError::io(parent, error))?;
    }
    Ok(())
}

/// Create a file that must not exist yet, then read it back byte for byte.
fn write_new(path: &Path, bytes: &[u8], label: &str) -> Result<(), ProjectError> {
    let target = project::check_components(path)?;
    if project::exists(&target)? {
        return Err(argument(format!(
            "{label}已存在，不会覆盖：{}。",
            target.display()
        )));
    }
    create_parent(&target)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x00200000);
    }
    let mut file = options
        .open(&target)
        .map_err(|error| ProjectError::io(&target, error))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| ProjectError::io(&target, error))?;
    drop(file);
    let readback = read_file(&target, bytes.len() as u64 + 1, label)?;
    if readback != bytes {
        return Err(integrity(format!(
            "写入后读回不一致：{}；保留现场并拒绝继续。",
            target.display()
        )));
    }
    Ok(())
}

fn require_dir(path: &Path, label: &str) -> Result<(), ProjectError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| ProjectError::io(path, error))?;
    if project::is_link(&metadata) || !metadata.is_dir() {
        return Err(integrity(format!(
            "{label}必须是普通目录、不能是链接：{}。",
            path.display()
        )));
    }
    Ok(())
}

fn names(dir: &Path) -> Result<BTreeMap<String, PathBuf>, ProjectError> {
    let mut entries = BTreeMap::new();
    let read = fs::read_dir(dir).map_err(|error| ProjectError::io(dir, error))?;
    for entry in read {
        let entry = entry.map_err(|error| ProjectError::io(dir, error))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| integrity(format!("目录 {} 里有非UTF-8文件名。", dir.display())))?;
        entries.insert(name, entry.path());
    }
    Ok(entries)
}

fn check_identity(path: &Path, expected: &Identity, label: &str) -> Result<(), ProjectError> {
    let current = identity(path)?;
    if &current != expected {
        return Err(integrity(format!(
            "{label}的文件身份与冻结记录不一致：{}。",
            path.display()
        )));
    }
    Ok(())
}

/// Lexically normalise a caller path, refusing to climb past its anchor.
fn resolve(path: &Path) -> Result<PathBuf, ProjectError> {
    let mut resolved = PathBuf::new();
    let mut depth = 0i32;
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => resolved.push(component),
            Component::CurDir => {}
            Component::ParentDir => {
                if depth == 0 {
                    return Err(ProjectError::unsafe_path(path));
                }
                resolved.pop();
                depth -= 1;
            }
            Component::Normal(part) => {
                resolved.push(part);
                depth += 1;
            }
        }
    }
    if path.is_relative() {
        let cwd = std::env::current_dir().map_err(|error| ProjectError::io(path, error))?;
        resolved = cwd.join(resolved);
    }
    project::check_components(&resolved)
}

/// Turn one declared document into the frozen record used by every later step.
fn read_document(
    package: &Path,
    spec: &DocumentSpec,
    limit: u64,
) -> Result<(DocumentRecord, String), ProjectError> {
    let absolute = resolve(&package.join(&spec.path))?;
    if !absolute.starts_with(package) {
        return Err(argument(format!(
            "文档 {} 的路径 {} 越出了作者包目录；本阶段只读作者包内的来源。",
            spec.source_id, spec.path
        )));
    }
    let raw = read_file(&absolute, limit, "来源文档")?;
    let parsed = text::analyze(&raw).map_err(|error| {
        argument(format!(
            "文档 {} 不是有效UTF-8（原始字节位置{}）：本阶段只对齐已登记的Markdown来源。",
            spec.source_id,
            error.valid_up_to()
        ))
    })?;
    let mut spans = Vec::new();
    for (index, span) in parsed.spans.iter().enumerate() {
        spans.push(SpanRecord {
            span_id: format!("{}-{:05}", spec.source_id, index + 1),
            kind: span.kind.as_str().to_owned(),
            start_byte: span.start_byte,
            end_byte: span.end_byte,
            start_line: span.start_line,
            end_line: span.end_line,
            text_sha256: span.text_sha256.clone(),
        });
    }
    let record = DocumentRecord {
        source_id: spec.source_id.clone(),
        title: spec.title.clone(),
        path: spec.path.clone(),
        raw_sha256: text::sha256(&raw),
        byte_length: raw.len() as u64,
        normalized_sha256: parsed.normalized_sha256.clone(),
        span_count: spans.len(),
        spans,
    };
    Ok((record, parsed.text))
}

fn views<'a>(inputs: &'a Inputs, bodies: &'a BTreeMap<String, String>) -> Vec<DocumentView<'a>> {
    inputs
        .documents
        .iter()
        .map(|record| DocumentView {
            record,
            text: bodies
                .get(&record.source_id)
                .map(String::as_str)
                .unwrap_or(""),
        })
        .collect()
}

fn derive(
    inputs: &Inputs,
    bodies: &BTreeMap<String, String>,
    concepts: &ConceptFile,
) -> Result<(Tree, ConceptInventory, Report), ProjectError> {
    let documents = views(inputs, bodies);
    let alignment = align(&documents, concepts, MAX_OCCURRENCES)?;
    let tree = build_tree(&inputs.case_id, &alignment);
    let inventory = build_inventory(&inputs.case_id, &alignment);
    let report = build_report(&inputs.case_id, &tree, &inventory, alignment.warnings);
    Ok((tree, inventory, report))
}

fn outside_body(inventory: &ConceptInventory) -> usize {
    inventory
        .concepts
        .iter()
        .filter(|concept| {
            concept.total > 0
                && concept.contexts.get(CONTEXT_BODY).copied().unwrap_or(0) == 0
                && concept.contexts.get(CONTEXT_HEADING).copied().unwrap_or(0) == 0
        })
        .count()
}

fn warning_list(report: &Report) -> String {
    if report.warning_counts.is_empty() {
        return "无".to_string();
    }
    report
        .warning_counts
        .iter()
        .map(|(kind, count)| format!("{kind} {count}"))
        .collect::<Vec<_>>()
        .join("、")
}

fn short(digest: &str) -> &str {
    if digest.len() >= 16 {
        &digest[..16]
    } else {
        digest
    }
}

pub fn adopt(author_path: &Path, root: &Path) -> Result<String, ProjectError> {
    let author_file = resolve(author_path)?;
    if !project::checked_file(&author_file)?.is_file() {
        return Err(argument(format!(
            "作者包必须是普通文件：{}。",
            author_file.display()
        )));
    }
    let author_bytes = read_file(&author_file, MAX_BYTES, "作者包")?;
    let author = Author::parse(&author_bytes)?;
    let package = author_file
        .parent()
        .ok_or_else(|| argument("作者包必须位于作者包目录内。"))?
        .to_path_buf();
    let concept_bytes = read_file(
        &resolve(&package.join(&author.concepts))?,
        MAX_BYTES,
        "概念词表",
    )?;
    let concepts = ConceptFile::parse(&concept_bytes)?;
    if concepts.concepts.len() > author.limits.max_concepts {
        return Err(argument(format!(
            "概念词表有 {} 条，超过 limits.max_concepts={}。",
            concepts.concepts.len(),
            author.limits.max_concepts
        )));
    }

    let target = resolve(root)?;
    if project::exists(&target)? {
        return Err(argument(format!(
            "对齐根必须尚不存在：{}；请选择一个新目录。",
            target.display()
        )));
    }
    let parent = target
        .parent()
        .ok_or_else(|| argument("对齐根必须有父目录。"))?
        .to_path_buf();
    require_dir(&parent, "对齐根的父目录")?;
    let parent_identity = identity(&parent)?;

    // Read and validate every document, then run a dry alignment, BEFORE
    // creating anything on disk: a malformed author package must leave no根.
    let mut documents = Vec::new();
    let mut bodies = BTreeMap::new();
    for spec in &author.documents {
        let (record, body) = read_document(&package, spec, MAX_BYTES)?;
        documents.push(record);
        bodies.insert(spec.source_id.clone(), body);
    }
    let inputs = Inputs {
        policy: POLICY.into(),
        schema_version: SCHEMA_VERSION,
        case_id: author.case_id.clone(),
        documents,
    };
    let inputs_json = canonical(&inputs)?;
    let (tree, inventory, report) = derive(&inputs, &bodies, &concepts)?;

    fs::create_dir(&target).map_err(|error| ProjectError::io(&target, error))?;
    let created = identity(&target)?;
    let result = adopt_into(AdoptInput {
        target: &target,
        parent_identity,
        author: &author,
        author_sha256: &text::sha256(&author_bytes),
        lexicon_sha256: &text::sha256(&concept_bytes),
        concept_bytes: &concept_bytes,
        inputs_json: &inputs_json,
        bodies: &bodies,
        document_digests: &inputs
            .documents
            .iter()
            .map(|record| DocumentDigest {
                source_id: record.source_id.clone(),
                normalized_sha256: record.normalized_sha256.clone(),
                span_count: record.span_count,
                byte_length: record.byte_length,
            })
            .collect::<Vec<_>>(),
        created: &created,
    });
    if result.is_err() && identity(&target).ok().as_ref() == Some(&created) {
        let _ = fs::remove_dir_all(&target);
    }
    let manifest = result?;
    Ok(adopt_summary(
        &manifest, &inputs, &tree, &inventory, &report,
    ))
}

struct AdoptInput<'a> {
    target: &'a Path,
    parent_identity: Identity,
    author: &'a Author,
    author_sha256: &'a str,
    lexicon_sha256: &'a str,
    concept_bytes: &'a [u8],
    inputs_json: &'a str,
    bodies: &'a BTreeMap<String, String>,
    document_digests: &'a [DocumentDigest],
    created: &'a Identity,
}

fn adopt_into(input: AdoptInput<'_>) -> Result<Manifest, ProjectError> {
    let root =
        fs::canonicalize(input.target).map_err(|error| ProjectError::io(input.target, error))?;
    let control = root.join(CONTROL);
    fs::create_dir(&control).map_err(|error| ProjectError::io(&control, error))?;
    let text_dir = root.join(TEXT_DIR);
    fs::create_dir(&text_dir).map_err(|error| ProjectError::io(&text_dir, error))?;

    for digest in input.document_digests {
        let body = input
            .bodies
            .get(&digest.source_id)
            .ok_or_else(|| integrity(format!("缺少文档 {} 的冻结文本。", digest.source_id)))?;
        write_new(
            &text_dir.join(format!("{}{TEXT_SUFFIX}", digest.source_id)),
            body.as_bytes(),
            "冻结文本",
        )?;
    }
    write_new(
        &text_dir.join(format!("{}{LEXICON_SUFFIX}", input.author.case_id)),
        input.concept_bytes,
        "冻结概念词表",
    )?;
    write_new(&root.join(INPUTS), input.inputs_json.as_bytes(), "冻结输入")?;

    // A placeholder gives the manifest a stable identity before its final
    // canonical bytes are written.
    let manifest_path = root.join(MANIFEST);
    write_new(&manifest_path, b"", "清单")?;
    let manifest = Manifest {
        policy: POLICY.into(),
        producer: APP_PRODUCER.into(),
        schema_version: SCHEMA_VERSION,
        case_id: input.author.case_id.clone(),
        root: root
            .to_str()
            .ok_or_else(|| ProjectError::unsafe_path(&root))?
            .to_owned(),
        root_identity: input.created.clone(),
        parent_identity: input.parent_identity.clone(),
        control_identity: identity(&control)?,
        manifest_identity: identity(&manifest_path)?,
        inputs_identity: identity(&root.join(INPUTS))?,
        author_manifest_sha256: input.author_sha256.to_owned(),
        lexicon_sha256: input.lexicon_sha256.to_owned(),
        inputs_sha256: text::sha256(input.inputs_json.as_bytes()),
        documents: input.document_digests.to_vec(),
        created_at: now()?,
    };
    let bytes = canonical(&manifest)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).truncate(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x00200000);
    }
    let mut file = options
        .open(&manifest_path)
        .map_err(|error| ProjectError::io(&manifest_path, error))?;
    file.write_all(bytes.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|error| ProjectError::io(&manifest_path, error))?;
    drop(file);
    if read_file(&manifest_path, MAX_BYTES, "清单")? != bytes.as_bytes() {
        return Err(integrity(
            "写入清单后读回不一致；保留现场并拒绝继续。".to_string(),
        ));
    }
    Ok(manifest)
}

fn adopt_summary(
    manifest: &Manifest,
    inputs: &Inputs,
    tree: &Tree,
    inventory: &ConceptInventory,
    report: &Report,
) -> String {
    let spans: usize = inputs
        .documents
        .iter()
        .map(|record| record.span_count)
        .sum();
    let bytes: u64 = inputs
        .documents
        .iter()
        .map(|record| record.byte_length)
        .sum();
    let mut out = String::new();
    out.push_str(&format!(
        "对齐根 {}：已建立（policy={}，结构版本 {}）\n",
        manifest.root, manifest.policy, manifest.schema_version
    ));
    out.push_str(&format!(
        "所有权：作者包 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        short(&manifest.author_manifest_sha256),
        short(&manifest.inputs_sha256),
        manifest.created_at
    ));
    out.push_str(&format!(
        "冻结来源：{} 份文档、{} 个文本片段、共 {} 字节\n",
        inputs.documents.len(),
        spans,
        bytes
    ));
    out.push_str(&format!(
        "章节对齐：{} 个章节（已对齐 {}、未对齐 {}）\n",
        report.counts.sections, report.counts.sections_aligned, report.counts.sections_unaligned
    ));
    out.push_str(&format!(
        "概念清单：{} 个概念、{} 次出现；只出现在图注或附录的 {} 个\n",
        report.counts.concepts,
        report.counts.occurrences,
        outside_body(inventory)
    ));
    out.push_str(&format!(
        "对齐警告：{} 条（{}）\n",
        report.counts.warnings,
        warning_list(report)
    ));
    let _ = tree;
    out.push_str("模型调用与网络请求：0 次；本根只保存冻结输入，不生成知识。");
    out
}

struct Frozen {
    root: PathBuf,
    manifest: Manifest,
    inputs: Inputs,
    bodies: BTreeMap<String, String>,
    concepts: ConceptFile,
}

fn open(root: &Path) -> Result<Frozen, ProjectError> {
    let target = resolve(root)?;
    if !project::exists(&target)? {
        return Err(unknown_root(format!(
            "{} 不存在，不是已建立的对齐根；未修改任何文件。",
            target.display()
        )));
    }
    require_dir(&target, "对齐根")?;
    for name in names(&target)?.keys() {
        if !ROOT_ENTRIES.contains(&name.as_str()) {
            return Err(integrity(format!(
                "对齐根顶层只允许 control/；发现多余的 {name}：陌生目录保持只读，未接管、未修改任何文件。"
            )));
        }
    }
    let control = target.join(CONTROL);
    if !project::exists(&control)? {
        return Err(unknown_root(format!(
            "{} 不是对齐根（缺少 control/）：陌生目录保持只读，未接管、未修改任何文件。",
            target.display()
        )));
    }
    require_dir(&control, "控制目录")?;
    for name in names(&control)?.keys() {
        if !CONTROL_ENTRIES.contains(&name.as_str()) {
            return Err(integrity(format!(
                "控制目录只允许 manifest.json、inputs.json 与 text/；发现多余的 {name}。"
            )));
        }
    }

    let manifest_path = target.join(MANIFEST);
    let manifest: Manifest = strict(&read_file(&manifest_path, MAX_BYTES, "清单")?, "清单")?;
    if manifest.policy != POLICY {
        return Err(integrity(format!(
            "清单 policy 必须是 {POLICY}；收到 {}。",
            manifest.policy
        )));
    }
    if manifest.producer != APP_PRODUCER {
        return Err(integrity(format!(
            "清单 producer 必须是 {APP_PRODUCER}；收到 {}。",
            manifest.producer
        )));
    }
    if manifest.schema_version != SCHEMA_VERSION {
        return Err(integrity(format!(
            "清单 schema_version 必须是 {SCHEMA_VERSION}；收到 {}。",
            manifest.schema_version
        )));
    }
    let canonical_root =
        fs::canonicalize(&target).map_err(|error| ProjectError::io(&target, error))?;
    if manifest.root != canonical_root.to_str().unwrap_or("") {
        return Err(integrity(format!(
            "清单记录的根是 {}，与当前路径 {} 不一致；不接管、不修改。",
            manifest.root,
            canonical_root.display()
        )));
    }
    check_identity(&target, &manifest.root_identity, "对齐根")?;
    check_identity(&control, &manifest.control_identity, "控制目录")?;
    check_identity(&manifest_path, &manifest.manifest_identity, "清单")?;

    let inputs_path = target.join(INPUTS);
    check_identity(&inputs_path, &manifest.inputs_identity, "冻结输入")?;
    let inputs_bytes = read_file(&inputs_path, MAX_BYTES, "冻结输入")?;
    if text::sha256(&inputs_bytes) != manifest.inputs_sha256 {
        return Err(integrity(
            "冻结输入的 SHA256 与清单不一致；保留现场并拒绝继续。".to_string(),
        ));
    }
    let inputs: Inputs = strict(&inputs_bytes, "冻结输入")?;
    if inputs.case_id != manifest.case_id {
        return Err(integrity(format!(
            "冻结输入的 case_id {} 与清单的 {} 不一致。",
            inputs.case_id, manifest.case_id
        )));
    }
    if inputs.policy != POLICY || inputs.schema_version != SCHEMA_VERSION {
        return Err(integrity(format!(
            "冻结输入的 policy 或结构版本与 {POLICY} v{SCHEMA_VERSION} 不一致。"
        )));
    }
    if inputs.documents.len() != manifest.documents.len() {
        return Err(integrity(
            "冻结输入的文档数与清单摘要数不一致；保留现场并拒绝继续。".to_string(),
        ));
    }

    let text_dir = target.join(TEXT_DIR);
    require_dir(&text_dir, "文本目录")?;
    let listed = names(&text_dir)?;
    let lexicon_name = format!("{}{LEXICON_SUFFIX}", manifest.case_id);
    let concept_bytes = read_file(&text_dir.join(&lexicon_name), MAX_BYTES, "冻结概念词表")?;
    if text::sha256(&concept_bytes) != manifest.lexicon_sha256 {
        return Err(integrity(
            "冻结概念词表的 SHA256 与清单不一致；保留现场并拒绝继续。".to_string(),
        ));
    }
    let concepts = ConceptFile::parse(&concept_bytes)?;

    let mut bodies = BTreeMap::new();
    for document in &inputs.documents {
        let name = format!("{}{TEXT_SUFFIX}", document.source_id);
        let path = text_dir.join(&name);
        if !listed.contains_key(&name) {
            return Err(integrity(format!("缺少冻结文本 {name}。")));
        }
        let body = String::from_utf8(read_file(&path, MAX_BYTES, "冻结文本")?)
            .map_err(|error| integrity(format!("冻结文本 {name} 不是UTF-8：{error}。")))?;
        if text::sha256(body.as_bytes()) != document.normalized_sha256 {
            return Err(integrity(format!(
                "冻结文本 {name} 的 SHA256 与冻结输入不一致；保留现场并拒绝继续。"
            )));
        }
        let recorded = manifest
            .documents
            .iter()
            .find(|entry| entry.source_id == document.source_id)
            .ok_or_else(|| integrity(format!("清单缺少文档 {} 的摘要。", document.source_id)))?;
        if recorded.normalized_sha256 != document.normalized_sha256
            || recorded.span_count != document.span_count
            || recorded.byte_length != document.byte_length
        {
            return Err(integrity(format!(
                "清单里文档 {} 的摘要与冻结输入不一致。",
                document.source_id
            )));
        }
        bodies.insert(document.source_id.clone(), body);
    }
    if listed.len() != inputs.documents.len() + 1 {
        return Err(integrity(
            "文本目录的文件数必须是冻结文档数加一份概念词表；保留现场并拒绝继续。".to_string(),
        ));
    }

    Ok(Frozen {
        root: target,
        manifest,
        inputs,
        bodies,
        concepts,
    })
}

pub fn build(root: &Path, output: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let target = resolve(output)?;
    if project::exists(&target)? {
        return Err(argument(format!(
            "输出根必须尚不存在：{}；请选择一个新目录。",
            target.display()
        )));
    }
    if target.starts_with(&frozen.root) || frozen.root.starts_with(&target) {
        return Err(argument(
            "输出根不能与对齐根互相包含：本命令只写全新目录。".to_string(),
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| argument("输出根必须有父目录。"))?
        .to_path_buf();
    require_dir(&parent, "输出根的父目录")?;

    let (tree, inventory, report) = derive(&frozen.inputs, &frozen.bodies, &frozen.concepts)?;
    let tree_json = format!("{}\n", canonical(&tree)?);
    let concept_json = format!("{}\n", canonical(&inventory)?);
    let report_json = format!("{}\n", canonical(&report)?);
    let markdown = render_markdown(&tree, &inventory, &report);

    fs::create_dir(&target).map_err(|error| ProjectError::io(&target, error))?;
    let created = identity(&target)?;
    let result = write_output(
        &target,
        &frozen,
        &tree_json,
        &concept_json,
        &report_json,
        &markdown,
    );
    if result.is_err() && identity(&target).ok().as_ref() == Some(&created) {
        let _ = fs::remove_dir_all(&target);
    }
    let output_manifest = result?;
    Ok(build_summary(&output_manifest, &inventory))
}

fn write_output(
    target: &Path,
    frozen: &Frozen,
    tree_json: &str,
    concept_json: &str,
    report_json: &str,
    markdown: &str,
) -> Result<OutputManifest, ProjectError> {
    let root = fs::canonicalize(target).map_err(|error| ProjectError::io(target, error))?;
    write_new(&root.join(TREE), tree_json.as_bytes(), "章节树")?;
    write_new(&root.join(CONCEPTS), concept_json.as_bytes(), "概念清单")?;
    write_new(&root.join(REPORT), report_json.as_bytes(), "对齐报告")?;
    write_new(
        &root.join(MARKDOWN),
        markdown.as_bytes(),
        "对齐报告Markdown",
    )?;
    let counts: Counts = serde_json::from_str::<Report>(report_json)
        .map_err(|error| integrity(format!("无法回读对齐报告：{error}。")))?
        .counts;
    let manifest = OutputManifest {
        policy: POLICY.into(),
        producer: OUTPUT_PRODUCER.into(),
        schema_version: SCHEMA_VERSION,
        case_id: frozen.manifest.case_id.clone(),
        source_root: frozen
            .root
            .to_str()
            .ok_or_else(|| ProjectError::unsafe_path(&frozen.root))?
            .to_owned(),
        source_inputs_sha256: frozen.manifest.inputs_sha256.clone(),
        tree_sha256: text::sha256(tree_json.as_bytes()),
        concepts_sha256: text::sha256(concept_json.as_bytes()),
        report_sha256: text::sha256(report_json.as_bytes()),
        markdown_sha256: text::sha256(markdown.as_bytes()),
        counts,
    };
    write_new(
        &root.join(OUTPUT_MANIFEST),
        format!("{}\n", canonical(&manifest)?).as_bytes(),
        "输出清单",
    )?;
    Ok(manifest)
}

fn build_summary(manifest: &OutputManifest, inventory: &ConceptInventory) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "输出根 {}：已生成（policy={}，结构版本 {}）\n",
        manifest.source_root, manifest.policy, manifest.schema_version
    ));
    out.push_str(&format!(
        "章节对齐：{} 个章节（已对齐 {}、未对齐 {}），来自 {} 份冻结文档\n",
        manifest.counts.sections,
        manifest.counts.sections_aligned,
        manifest.counts.sections_unaligned,
        manifest.counts.documents
    ));
    out.push_str(&format!(
        "概念清单：{} 个概念、{} 次出现；只出现在图注或附录的 {} 个\n",
        manifest.counts.concepts,
        manifest.counts.occurrences,
        outside_body(inventory)
    ));
    out.push_str(&format!("对齐警告：{} 条\n", manifest.counts.warnings));
    out.push_str(&format!(
        "文件：tree.json {}；concepts.json {}；report.json {}；report.md {}\n",
        short(&manifest.tree_sha256),
        short(&manifest.concepts_sha256),
        short(&manifest.report_sha256),
        short(&manifest.markdown_sha256)
    ));
    out.push_str("模型调用与网络请求：0 次；输出根只含派生报告，不写项目、不写 Vault。");
    out
}

pub fn status(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let (tree, inventory, report) = derive(&frozen.inputs, &frozen.bodies, &frozen.concepts)?;
    let spans: usize = frozen
        .inputs
        .documents
        .iter()
        .map(|record| record.span_count)
        .sum();
    let mut out = String::new();
    out.push_str(&format!(
        "对齐根 {}：已建立（policy={}，结构版本 {}）\n",
        frozen.manifest.root, frozen.manifest.policy, frozen.manifest.schema_version
    ));
    out.push_str(&format!(
        "所有权：作者包 SHA256 {}；冻结输入 SHA256 {}；建立于 {}\n",
        short(&frozen.manifest.author_manifest_sha256),
        short(&frozen.manifest.inputs_sha256),
        frozen.manifest.created_at
    ));
    out.push_str(&format!(
        "冻结来源：{} 份文档、{} 个文本片段、{} 个概念\n",
        frozen.inputs.documents.len(),
        spans,
        frozen.concepts.concepts.len()
    ));
    out.push_str(&format!(
        "章节对齐：{} 个章节（已对齐 {}、未对齐 {}）\n",
        report.counts.sections, report.counts.sections_aligned, report.counts.sections_unaligned
    ));
    for document in &tree.documents {
        out.push_str(&format!(
            "  - {} {}：{} 个章节（已对齐 {}、未对齐 {}）\n",
            document.source_id,
            document.title,
            document.sections.len(),
            document.aligned,
            document.unaligned
        ));
    }
    out.push_str(&format!(
        "概念清单：{} 个概念、{} 次出现；只出现在图注或附录的 {} 个\n",
        report.counts.concepts,
        report.counts.occurrences,
        outside_body(&inventory)
    ));
    out.push_str(&format!(
        "对齐警告：{} 条（{}）\n",
        report.counts.warnings,
        warning_list(&report)
    ));
    out.push_str("模型调用与网络请求：0 次；本根只保存冻结输入，不生成知识。");
    Ok(out)
}

pub fn list(root: &Path) -> Result<String, ProjectError> {
    let frozen = open(root)?;
    let (tree, inventory, report) = derive(&frozen.inputs, &frozen.bodies, &frozen.concepts)?;
    let value = serde_json::json!({
        "case_id": frozen.inputs.case_id,
        "policy": frozen.manifest.policy,
        "producer": frozen.manifest.producer,
        "schema_version": frozen.manifest.schema_version,
        "inputs_sha256": frozen.manifest.inputs_sha256,
        "author_manifest_sha256": frozen.manifest.author_manifest_sha256,
        "lexicon_sha256": frozen.manifest.lexicon_sha256,
        "created_at": frozen.manifest.created_at,
        "documents": frozen.inputs.documents.iter().map(|record| serde_json::json!({
            "source_id": record.source_id,
            "title": record.title,
            "path": record.path,
            "raw_sha256": record.raw_sha256,
            "byte_length": record.byte_length,
            "normalized_sha256": record.normalized_sha256,
            "span_count": record.span_count,
        })).collect::<Vec<_>>(),
        "tree": tree,
        "concepts": inventory,
        "report": report,
        "model_calls": 0,
        "network_requests": 0,
    });
    Ok(format!(
        "{}\n",
        serde_json::to_string_pretty(&value)
            .map_err(|error| integrity(format!("无法序列化结果：{error}。")))?
    ))
}
