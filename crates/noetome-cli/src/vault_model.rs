//! 托管测试集合：冻结作者包、候选操作、影响清单与链接解析。
//!
//! 本模块只做纯计算，不读写磁盘。它把作者声明、当前 `vault/` 字节与候选字节
//! 归一化成「最终状态 + 影响清单 + 逐项落盘条目」，供存储层与报告层使用。
//! 拒绝一律返回明确代码，调用方负责持久化与报告，不能静默降级。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// 作者包与候选操作共用的策略标识。
pub const POLICY: &str = "managed-vault-update-v1";
/// 计划原文、批准文件与操作清单的上限。
pub const MAX_PLAN_BYTES: u64 = 8 * 1024 * 1024;
/// 单个托管文件的上限，同时约束基准、当前与候选。
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// 影响类别。六类分别列出，不合并成总数。
pub const IMPACT_BODY: &str = "body";
pub const IMPACT_STRUCTURE: &str = "structure";
pub const IMPACT_LOCATION: &str = "location";
pub const IMPACT_LINK: &str = "link";
pub const IMPACT_ATTACHMENT: &str = "attachment";
pub const IMPACT_DELETION: &str = "deletion";
pub const IMPACT_CATEGORIES: [&str; 6] = [
    IMPACT_BODY,
    IMPACT_STRUCTURE,
    IMPACT_LOCATION,
    IMPACT_LINK,
    IMPACT_ATTACHMENT,
    IMPACT_DELETION,
];

/// 拒绝码。报告与账本都用这些原样字符串。
pub const PATH_OUTSIDE: &str = "path-outside-managed-set";
pub const LINK_WOULD_BREAK: &str = "link-would-break";
pub const DELETE_NEEDS_FLAG: &str = "delete-requires-explicit-flag";
pub const APPROVAL_STALE: &str = "approval-stale";
pub const ROLLBACK_CONFLICT: &str = "rollback-conflict";
pub const CANDIDATE_MISSING: &str = "candidate-missing";
pub const CANDIDATE_UNLISTED: &str = "candidate-unlisted";
pub const OPERATION_LIMIT: &str = "operation-limit";
pub const MANAGED_CONFLICT: &str = "managed-set-conflict";
pub const CANDIDATE_INVALID: &str = "candidate-invalid";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    pub message: String,
}

impl Refusal {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Member {
    pub path: String,
    pub title: String,
    pub baseline: String,
    pub sha256: String,
    pub byte_length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub path: String,
    pub baseline: String,
    pub media_type: String,
    pub sha256: String,
    pub byte_length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredLink {
    pub from: String,
    pub raw: String,
    pub resolves_to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_notes: u64,
    pub max_resources: u64,
    pub max_operations_per_plan: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Author {
    pub policy: String,
    pub description: String,
    pub notes: Vec<Member>,
    pub resources: Vec<Resource>,
    pub links: Vec<DeclaredLink>,
    pub operations: Vec<String>,
    pub limits: Limits,
}

impl Author {
    pub fn parse(bytes: &[u8]) -> Result<Self, Refusal> {
        let author: Self = serde_json::from_slice(bytes).map_err(|error| {
            Refusal::new(CANDIDATE_INVALID, format!("作者声明不是严格JSON：{error}"))
        })?;
        author.validate()?;
        Ok(author)
    }

    pub fn validate(&self) -> Result<(), Refusal> {
        if self.policy != POLICY {
            return Err(Refusal::new(
                CANDIDATE_INVALID,
                format!("作者声明策略必须是 {POLICY}。"),
            ));
        }
        if self.notes.is_empty() || self.notes.len() as u64 > self.limits.max_notes {
            return Err(Refusal::new(
                CANDIDATE_INVALID,
                "托管笔记数量超出作者声明的上限。",
            ));
        }
        if self.resources.len() as u64 > self.limits.max_resources {
            return Err(Refusal::new(
                CANDIDATE_INVALID,
                "托管资源数量超出作者声明的上限。",
            ));
        }
        let allowed: BTreeSet<&str> = ["edit", "link", "attach", "rename", "delete"]
            .into_iter()
            .collect();
        if self.operations.is_empty()
            || self
                .operations
                .iter()
                .any(|operation| !allowed.contains(operation.as_str()))
        {
            return Err(Refusal::new(
                CANDIDATE_INVALID,
                "作者声明的操作集合必须是 edit/link/attach/rename/delete 的非空子集。",
            ));
        }
        let mut seen = BTreeSet::new();
        for member in &self.notes {
            check_relative(&member.path)?;
            check_relative(&member.baseline)?;
            check_digest(&member.sha256)?;
            if !member.path.ends_with(".md") {
                return Err(Refusal::new(
                    CANDIDATE_INVALID,
                    format!("托管笔记必须是Markdown：{}。", member.path),
                ));
            }
            if !seen.insert(member.path.clone()) {
                return Err(Refusal::new(
                    CANDIDATE_INVALID,
                    format!("托管路径重复声明：{}。", member.path),
                ));
            }
        }
        for resource in &self.resources {
            check_relative(&resource.path)?;
            check_relative(&resource.baseline)?;
            check_digest(&resource.sha256)?;
            if resource.media_type != "image/png" {
                return Err(Refusal::new(
                    CANDIDATE_INVALID,
                    "当前只支持已确认的 image/png 附件。",
                ));
            }
            if !seen.insert(resource.path.clone()) {
                return Err(Refusal::new(
                    CANDIDATE_INVALID,
                    format!("托管路径重复声明：{}。", resource.path),
                ));
            }
        }
        for link in &self.links {
            check_relative(&link.from)?;
            check_relative(&link.resolves_to)?;
            if !self.contains(&link.from) || !self.contains(&link.resolves_to) {
                return Err(Refusal::new(
                    CANDIDATE_INVALID,
                    "声明的链接两端都必须在托管集合内。",
                ));
            }
        }
        Ok(())
    }

    /// 托管集合的全部路径（笔记与资源），排序去重。
    pub fn managed(&self) -> BTreeSet<String> {
        self.notes
            .iter()
            .map(|member| member.path.clone())
            .chain(self.resources.iter().map(|resource| resource.path.clone()))
            .collect()
    }

    /// 托管笔记路径。
    pub fn notes_set(&self) -> BTreeSet<String> {
        self.notes
            .iter()
            .map(|member| member.path.clone())
            .collect()
    }

    pub fn contains(&self, path: &str) -> bool {
        self.managed().contains(path)
    }

    /// 基准路径声明，键是托管路径，值是作者包内相对路径。
    pub fn baselines(&self) -> BTreeMap<String, String> {
        self.notes
            .iter()
            .map(|member| (member.path.clone(), member.baseline.clone()))
            .chain(
                self.resources
                    .iter()
                    .map(|resource| (resource.path.clone(), resource.baseline.clone())),
            )
            .collect()
    }

    /// 作者声明的基准指纹，键是托管路径。
    pub fn baseline_digests(&self) -> BTreeMap<String, (String, u64)> {
        self.notes
            .iter()
            .map(|member| {
                (
                    member.path.clone(),
                    (member.sha256.clone(), member.byte_length),
                )
            })
            .chain(self.resources.iter().map(|resource| {
                (
                    resource.path.clone(),
                    (resource.sha256.clone(), resource.byte_length),
                )
            }))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Operation {
    Edit { path: String },
    Link { path: String, target: String },
    Attach { path: String },
    Rename { from: String, to: String },
    Delete { path: String },
}

impl Operation {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Edit { .. } => "edit",
            Self::Link { .. } => "link",
            Self::Attach { .. } => "attach",
            Self::Rename { .. } => "rename",
            Self::Delete { .. } => "delete",
        }
    }

    /// 该操作声明要在候选目录里出现的路径。
    pub fn candidate_path(&self) -> Option<&str> {
        match self {
            Self::Edit { path } | Self::Link { path, .. } | Self::Attach { path } => Some(path),
            Self::Rename { to, .. } => Some(to),
            Self::Delete { .. } => None,
        }
    }

    /// 该操作声明的全部受管路径，用于集合内外判定。
    pub fn touched(&self) -> Vec<String> {
        match self {
            Self::Edit { path } | Self::Link { path, .. } | Self::Attach { path } => {
                vec![path.clone()]
            }
            Self::Rename { from, to } => vec![from.clone(), to.clone()],
            Self::Delete { path } => vec![path.clone()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Impact {
    pub category: String,
    pub path: String,
    pub detail: String,
}

impl Impact {
    fn new(category: &str, path: &str, detail: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            path: path.into(),
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryAction {
    Write,
    Delete,
}

impl EntryAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub action: EntryAction,
    pub path: String,
    /// 落盘后该路径应有的字节指纹；删除为全零占位以外的空串表示不存在。
    pub sha256: String,
    pub byte_length: u64,
    /// 落盘前该路径的字节指纹；原本不存在为空串。
    pub previous_sha256: String,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub impacts: Vec<Impact>,
    pub entries: Vec<Entry>,
    /// 最终托管集合（重命名与删除之后）。
    pub final_paths: BTreeSet<String>,
    /// 每个受影响路径的最终字节；`None` 表示该路径最终不存在。
    pub final_bytes: BTreeMap<String, Option<Vec<u8>>>,
    /// 最终内容里实际存在的链接，用于暂存后逐项对账。
    pub final_links: Vec<ResolvedLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedLink {
    pub from: String,
    pub raw: String,
    pub resolves_to: String,
    pub present: bool,
}

/// 候选目录：`operations.json` 与它显式列出的字节。
#[derive(Debug, Clone)]
pub struct Candidate {
    pub case_id: String,
    pub summary: String,
    pub operations: Vec<Operation>,
    pub files: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationsFile {
    policy: String,
    case_id: String,
    summary: String,
    operations: Vec<Operation>,
}

impl Candidate {
    pub fn parse(
        operations_bytes: &[u8],
        files: BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, Refusal> {
        let raw: OperationsFile = serde_json::from_slice(operations_bytes).map_err(|error| {
            Refusal::new(
                CANDIDATE_INVALID,
                format!("operations.json不是严格记录：{error}"),
            )
        })?;
        if raw.policy != POLICY {
            return Err(Refusal::new(
                CANDIDATE_INVALID,
                format!("operations.json策略必须是 {POLICY}。"),
            ));
        }
        if raw.case_id.trim().is_empty() || raw.summary.trim().is_empty() {
            return Err(Refusal::new(
                CANDIDATE_INVALID,
                "operations.json必须声明 case_id 与 summary。",
            ));
        }
        Ok(Self {
            case_id: raw.case_id,
            summary: raw.summary,
            operations: raw.operations,
            files,
        })
    }
}

fn check_relative(path: &str) -> Result<(), Refusal> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(Refusal::new(
            PATH_OUTSIDE,
            format!("路径必须是根内相对路径：{path}。"),
        ));
    }
    Ok(())
}

fn check_digest(digest: &str) -> Result<(), Refusal> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(Refusal::new(
            CANDIDATE_INVALID,
            "作者声明的SHA256必须是64位小写十六进制。",
        ));
    }
    Ok(())
}

/// 从Markdown正文里抽取站内链接目标（原样，未解析）。
///
/// 只认 `[文本](目标)`；外部URL、纯锚点与空目标不算托管集合内的链接。
pub fn link_targets(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find("](") {
        let open = cursor + offset + 2;
        let Some(close) = text[open..].find(')') else {
            break;
        };
        let raw = text[open..open + close].trim();
        cursor = open + close + 1;
        if raw.is_empty() || raw.starts_with('#') || raw.contains("://") {
            continue;
        }
        let target = raw.split('#').next().unwrap_or("").trim();
        if target.is_empty() {
            continue;
        }
        out.push(target.to_owned());
    }
    out.sort();
    out.dedup();
    out
}

/// 把链接目标解析成托管根内的相对路径。
pub fn resolve_link(from: &str, raw: &str) -> Option<String> {
    if raw.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for part in raw.split('/') {
        match part {
            "" | "." => continue,
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        return None;
    }
    let mut path = parts.join("/");
    if std::path::Path::new(&path).extension().is_none() {
        path.push_str(".md");
    }
    Some(path)
}

/// 校验并归一化一份候选，返回影响清单与逐项落盘条目。
///
/// `current` 是此刻 `vault/` 中的托管字节；`baseline` 是 adopt 冻结的字节。
/// 两者都由调用方从磁盘重新读取，模型层不假定任何缓存。
pub fn analyze(
    author: &Author,
    candidate: &Candidate,
    current: &BTreeMap<String, Vec<u8>>,
    baseline: &BTreeMap<String, Vec<u8>>,
) -> Result<Analysis, Refusal> {
    if candidate.operations.is_empty() {
        return Err(Refusal::new(
            CANDIDATE_INVALID,
            "候选必须声明至少一个操作。",
        ));
    }
    if candidate.operations.len() as u64 > author.limits.max_operations_per_plan {
        return Err(Refusal::new(
            OPERATION_LIMIT,
            format!(
                "单个计划最多 {} 个操作，候选声明了 {} 个。",
                author.limits.max_operations_per_plan,
                candidate.operations.len()
            ),
        ));
    }
    let managed = author.managed();
    let notes = author.notes_set();
    let allowed_ops: BTreeSet<&str> = author.operations.iter().map(String::as_str).collect();

    let mut introduced = BTreeSet::new();
    for operation in &candidate.operations {
        if !allowed_ops.contains(operation.kind()) {
            return Err(Refusal::new(
                PATH_OUTSIDE,
                format!("作者包未声明 {} 操作。", operation.kind()),
            ));
        }
        for path in operation.touched() {
            check_relative(&path)?;
            let is_target = matches!(operation, Operation::Rename { to, .. } if &path == to);
            if is_target {
                introduced.insert(path);
                continue;
            }
            if !managed.contains(&path) {
                return Err(Refusal::new(
                    PATH_OUTSIDE,
                    format!("{path} 不在作者声明的托管集合内。"),
                ));
            }
        }
        if matches!(operation, Operation::Delete { .. }) {
            return Err(Refusal::new(
                DELETE_NEEDS_FLAG,
                "删除必须由批准文件显式携带该操作；计划阶段一律拒绝。",
            ));
        }
        if let Operation::Link { target, .. } = operation
            && !managed.contains(target)
        {
            return Err(Refusal::new(
                PATH_OUTSIDE,
                format!("链接目标 {target} 不在托管集合内。"),
            ));
        }
    }

    // 一个路径最多被一个操作声明为目标，且重命名不得与既有托管路径或另一次重命名冲突。
    for path in &introduced {
        if managed.contains(path) {
            return Err(Refusal::new(
                MANAGED_CONFLICT,
                format!("重命名目标 {path} 已属于托管集合。"),
            ));
        }
    }

    // 候选字节必须恰好覆盖操作声明的写入目标。
    let mut declared: BTreeMap<String, &Operation> = BTreeMap::new();
    for operation in &candidate.operations {
        if let Some(path) = operation.candidate_path() {
            declared.insert(path.to_owned(), operation);
        }
    }
    for path in declared.keys() {
        if !candidate.files.contains_key(path) {
            return Err(Refusal::new(
                CANDIDATE_MISSING,
                format!("候选目录缺少操作的写入目标：{path}。"),
            ));
        }
    }
    for path in candidate.files.keys() {
        if !declared.contains_key(path) {
            return Err(Refusal::new(
                CANDIDATE_UNLISTED,
                format!("候选目录含未声明的文件：{path}。"),
            ));
        }
        if candidate.files[path].len() as u64 > MAX_FILE_BYTES {
            return Err(Refusal::new(
                CANDIDATE_INVALID,
                format!("候选文件超过16MiB：{path}。"),
            ));
        }
    }

    // 应用操作，得到最终状态。
    let mut final_bytes: BTreeMap<String, Option<Vec<u8>>> = managed
        .iter()
        .map(|path| (path.clone(), current.get(path).cloned()))
        .collect();
    for operation in &candidate.operations {
        match operation {
            Operation::Edit { path }
            | Operation::Link { path, .. }
            | Operation::Attach { path } => {
                final_bytes.insert(path.clone(), Some(candidate.files[path].clone()));
            }
            Operation::Rename { from, to } => {
                let moved = final_bytes.remove(from).flatten().ok_or_else(|| {
                    Refusal::new(
                        CANDIDATE_MISSING,
                        format!("重命名来源 {from} 在当前磁盘上不存在。"),
                    )
                })?;
                let candidate_bytes = &candidate.files[to];
                if crate::text::sha256(&moved) == crate::text::sha256(candidate_bytes)
                    && moved != *candidate_bytes
                {
                    return Err(Refusal::new(
                        CANDIDATE_INVALID,
                        format!("重命名的候选字节无法区分：{to}。"),
                    ));
                }
                // 新位置写入，旧路径仍留在最终状态里记为不存在，落盘时逐项删除。
                final_bytes.insert(to.clone(), Some(candidate_bytes.clone()));
                final_bytes.insert(from.clone(), None);
            }
            Operation::Delete { path } => {
                final_bytes.insert(path.clone(), None);
            }
        }
    }
    let final_paths: BTreeSet<String> = final_bytes
        .iter()
        .filter(|(_, bytes)| bytes.is_some())
        .map(|(path, _)| path.clone())
        .collect();

    // 链接：只拒绝本次新造成的断链，既有断链保持原样并记录。
    let mut final_links = Vec::new();
    let mut broken: BTreeSet<String> = BTreeSet::new();
    for path in final_paths.iter().filter(|path| notes.contains(*path)) {
        let Some(Some(bytes)) = final_bytes.get(path) else {
            continue;
        };
        let text = String::from_utf8_lossy(bytes).to_string();
        for raw in link_targets(&text) {
            let Some(resolved) = resolve_link(path, &raw) else {
                continue;
            };
            let present = final_paths.contains(&resolved);
            if !present {
                broken.insert(format!("{path}→{raw}"));
            }
            final_links.push(ResolvedLink {
                from: path.clone(),
                raw,
                resolves_to: resolved,
                present,
            });
        }
    }
    // 当前磁盘上已经断链的引用不算本次回归。
    let mut pre_broken: BTreeSet<String> = BTreeSet::new();
    for path in managed.iter().filter(|path| notes.contains(*path)) {
        let Some(bytes) = current.get(path) else {
            continue;
        };
        let text = String::from_utf8_lossy(bytes).to_string();
        for raw in link_targets(&text) {
            if resolve_link(path, &raw).is_some_and(|resolved| !current.contains_key(&resolved)) {
                pre_broken.insert(format!("{path}→{raw}"));
            }
        }
    }
    let newly_broken: Vec<&String> = broken.difference(&pre_broken).collect();
    if !newly_broken.is_empty() {
        let mut detail: Vec<&str> = newly_broken.iter().map(|value| value.as_str()).collect();
        detail.sort();
        return Err(Refusal::new(
            LINK_WOULD_BREAK,
            format!("候选会使这些引用失去目标：{}。", detail.join("、")),
        ));
    }
    // 作者显式声明的链接必须仍然指向同一个逻辑目标。
    //
    // 目标可能被本计划重命名，所以「同一个目标」指重命名之后的路径；把
    // 声明路径按重命名映射前移，再看候选正文是否仍然解析到它。
    let mut renamed: BTreeMap<String, String> = BTreeMap::new();
    for operation in &candidate.operations {
        if let Operation::Rename { from, to } = operation {
            renamed.insert(from.clone(), to.clone());
        }
    }
    let moved = |path: &str| -> String {
        renamed
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.to_owned())
    };
    for declared in &author.links {
        let from = moved(&declared.from);
        if !final_paths.contains(&from) {
            continue;
        }
        let Some(Some(bytes)) = final_bytes.get(&from) else {
            continue;
        };
        let expected = moved(&declared.resolves_to);
        let text = String::from_utf8_lossy(bytes).to_string();
        let still_declared = link_targets(&text)
            .into_iter()
            .any(|raw| resolve_link(&from, &raw).as_deref() == Some(expected.as_str()));
        if still_declared {
            continue;
        }
        let was_declared = current
            .get(&declared.from)
            .map(|bytes| String::from_utf8_lossy(bytes).to_string())
            .unwrap_or_default();
        let had = link_targets(&was_declared).into_iter().any(|raw| {
            resolve_link(&declared.from, &raw).as_deref() == Some(declared.resolves_to.as_str())
        });
        if had {
            return Err(Refusal::new(
                LINK_WOULD_BREAK,
                format!(
                    "声明链接 {}→{} 在候选中失去目标 {expected}。",
                    declared.from, declared.resolves_to
                ),
            ));
        }
    }
    final_links.sort();

    // 影响清单：逐操作、逐类别累加，最后排序去重。
    let mut impacts: BTreeSet<Impact> = BTreeSet::new();
    for operation in &candidate.operations {
        match operation {
            Operation::Edit { path } => {
                impacts.insert(Impact::new(IMPACT_BODY, path, "正文改写"));
            }
            Operation::Link { path, target } => {
                impacts.insert(Impact::new(IMPACT_LINK, path, format!("新增链接→{target}")));
                impacts.insert(Impact::new(IMPACT_BODY, path, "正文改写（新增链接）"));
            }
            Operation::Attach { path } => {
                impacts.insert(Impact::new(IMPACT_ATTACHMENT, path, "附件字节替换"));
            }
            Operation::Rename { from, to } => {
                impacts.insert(Impact::new(IMPACT_LOCATION, from, format!("重命名→{to}")));
                impacts.insert(Impact::new(
                    IMPACT_LOCATION,
                    to,
                    format!("新位置（来自{from}）"),
                ));
                impacts.insert(Impact::new(IMPACT_STRUCTURE, from, "目录成员减少"));
                impacts.insert(Impact::new(IMPACT_STRUCTURE, to, "目录成员增加"));
            }
            Operation::Delete { path } => {
                impacts.insert(Impact::new(IMPACT_DELETION, path, "删除托管笔记"));
            }
        }
    }
    // 任何正文变化都单独记一条，避免只靠操作名推断影响。
    for path in &final_paths {
        if !notes.contains(path) {
            continue;
        }
        let before = current.get(path);
        let after = final_bytes.get(path).and_then(|bytes| bytes.as_ref());
        if before != after {
            impacts.insert(Impact::new(IMPACT_BODY, path, "内容字节变化"));
        }
        let before_links: BTreeSet<String> = before
            .map(|bytes| {
                link_targets(&String::from_utf8_lossy(bytes))
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default();
        let after_links: BTreeSet<String> = after
            .map(|bytes| {
                link_targets(&String::from_utf8_lossy(bytes))
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default();
        if before_links != after_links {
            impacts.insert(Impact::new(IMPACT_LINK, path, "链接集合变化"));
        }
    }
    for path in &final_paths {
        if !notes.contains(path) && !baseline.contains_key(path) {
            impacts.insert(Impact::new(IMPACT_STRUCTURE, path, "托管集合新增路径"));
        }
    }
    if impacts.is_empty() {
        return Err(Refusal::new(
            CANDIDATE_INVALID,
            "候选没有产生任何可描述的托管影响。",
        ));
    }

    // 逐项条目：先写后删，路径序固定。
    let mut entries = Vec::new();
    let mut writes: Vec<(&String, &Vec<u8>)> = Vec::new();
    let mut deletes: Vec<&String> = Vec::new();
    for (path, state) in &final_bytes {
        match state {
            Some(bytes) => {
                if current.get(path) != Some(bytes) {
                    writes.push((path, bytes));
                }
            }
            None => {
                if current.contains_key(path) {
                    deletes.push(path);
                }
            }
        }
    }
    writes.sort_by(|a, b| a.0.cmp(b.0));
    deletes.sort();
    for (path, bytes) in writes {
        entries.push(Entry {
            action: EntryAction::Write,
            path: path.clone(),
            sha256: crate::text::sha256(bytes),
            byte_length: bytes.len() as u64,
            previous_sha256: current
                .get(path)
                .map(|b| crate::text::sha256(b))
                .unwrap_or_default(),
        });
    }
    for path in deletes {
        entries.push(Entry {
            action: EntryAction::Delete,
            path: path.clone(),
            sha256: String::new(),
            byte_length: 0,
            previous_sha256: current
                .get(path)
                .map(|b| crate::text::sha256(b))
                .unwrap_or_default(),
        });
    }
    if entries.is_empty() {
        return Err(Refusal::new(
            CANDIDATE_INVALID,
            "候选字节与当前磁盘一致，没有可落盘的条目。",
        ));
    }

    Ok(Analysis {
        impacts: impacts.into_iter().collect(),
        entries,
        final_paths,
        final_bytes,
        final_links,
    })
}

/// 影响清单的规范序列化，用于审批绑定。
pub fn canonical_impacts(impacts: &[Impact]) -> String {
    serde_json::to_string(impacts).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn package() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/managed-vault-update-v1")
    }

    fn author() -> Author {
        Author::parse(&std::fs::read(package().join("author.json")).unwrap()).unwrap()
    }

    fn candidate(id: &str) -> Candidate {
        let root = package().join("candidates").join(id);
        let operations = std::fs::read(root.join("operations.json")).unwrap();
        let mut files = BTreeMap::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if entry.file_name() != "operations.json" {
                    let relative = path
                        .strip_prefix(&root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned();
                    files.insert(relative, std::fs::read(&path).unwrap());
                }
            }
        }
        Candidate::parse(&operations, files).unwrap()
    }

    fn baseline(author: &Author) -> BTreeMap<String, Vec<u8>> {
        author
            .baselines()
            .into_iter()
            .map(|(path, relative)| (path, std::fs::read(package().join(relative)).unwrap()))
            .collect()
    }

    fn decisions() -> BTreeMap<String, (bool, Option<&'static str>)> {
        let cases = std::fs::read_to_string(package().join("cases.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&cases).unwrap();
        value["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| {
                let id = case["id"].as_str().unwrap().to_owned();
                let refuse = case["expect"]["decision"].as_str() == Some("refuse");
                let code = case["expect"]["refusal"]
                    .as_str()
                    .map(|code| Box::leak(code.to_owned().into_boxed_str()) as &'static str);
                (id, (refuse, code))
            })
            .collect()
    }

    #[test]
    fn frozen_author_package_declares_two_notes_one_png_and_one_link() {
        let author = author();
        assert_eq!(author.notes.len(), 2);
        assert_eq!(author.resources.len(), 1);
        assert_eq!(author.resources[0].media_type, "image/png");
        assert_eq!(author.links.len(), 1);
        assert_eq!(author.managed().len(), 3);
        let baseline = baseline(&author);
        for (path, (digest, length)) in author.baseline_digests() {
            let bytes = baseline.get(&path).expect("declared baseline exists");
            assert_eq!(bytes.len() as u64, length);
            assert_eq!(crate::text::sha256(bytes), digest);
        }
    }

    #[test]
    fn every_frozen_case_matches_its_declared_decision() {
        let author = author();
        let current = baseline(&author);
        for (id, (refuse, code)) in decisions() {
            let result = analyze(&author, &candidate(&id), &current, &current);
            match (refuse, result) {
                (true, Err(refusal)) => assert_eq!(Some(refusal.code), code, "case {id}"),
                (true, Ok(analysis)) => {
                    panic!("case {id} should refuse, got {:?}", analysis.impacts)
                }
                (false, Err(refusal)) => panic!("case {id} should pass, refused {}", refusal.code),
                (false, Ok(analysis)) => {
                    assert!(
                        !analysis.entries.is_empty(),
                        "case {id} produced no entries"
                    );
                    assert!(
                        !analysis.impacts.is_empty(),
                        "case {id} produced no impacts"
                    );
                }
            }
        }
    }

    #[test]
    fn rename_that_keeps_the_reference_is_accepted_and_rewrites_both_files() {
        let author = author();
        let current = baseline(&author);
        let analysis = analyze(&author, &candidate("rename-linked"), &current, &current).unwrap();
        assert!(analysis.final_paths.contains("笔记/几何光学.md"));
        assert!(!analysis.final_paths.contains("笔记/光学.md"));
        assert!(analysis.final_paths.contains("笔记/力学.md"));
        let actions: Vec<(&str, &str)> = analysis
            .entries
            .iter()
            .map(|entry| (entry.action.as_str(), entry.path.as_str()))
            .collect();
        assert_eq!(
            actions,
            vec![
                ("write", "笔记/几何光学.md"),
                ("write", "笔记/力学.md"),
                ("delete", "笔记/光学.md"),
            ],
            "先写新字节再删旧路径，顺序由路径字节序固定"
        );
        assert!(analysis.final_links.iter().all(|link| link.present));
    }

    #[test]
    fn rename_without_rewriting_the_reference_is_refused() {
        let author = author();
        let current = baseline(&author);
        let refusal =
            analyze(&author, &candidate("rename-dangling"), &current, &current).unwrap_err();
        assert_eq!(refusal.code, LINK_WOULD_BREAK);
    }

    #[test]
    fn delete_and_outside_paths_are_refused_before_any_byte_is_written() {
        let author = author();
        let current = baseline(&author);
        assert_eq!(
            analyze(&author, &candidate("delete-note"), &current, &current)
                .unwrap_err()
                .code,
            DELETE_NEEDS_FLAG
        );
        assert_eq!(
            analyze(&author, &candidate("foreign-touch"), &current, &current)
                .unwrap_err()
                .code,
            PATH_OUTSIDE
        );
    }

    #[test]
    fn impact_categories_are_listed_separately() {
        let author = author();
        let current = baseline(&author);
        let analysis = analyze(
            &author,
            &candidate("replace-attachment"),
            &current,
            &current,
        )
        .unwrap();
        let categories: BTreeSet<&str> = analysis
            .impacts
            .iter()
            .map(|impact| impact.category.as_str())
            .collect();
        assert!(categories.contains(IMPACT_ATTACHMENT));
        assert_eq!(analysis.entries.len(), 1, "only the PNG byte changes");
        assert_eq!(analysis.entries[0].path, "附件/光谱.png");
    }

    #[test]
    fn candidate_bytes_must_be_exactly_the_declared_targets() {
        let author = author();
        let current = baseline(&author);
        let mut extra = candidate("edit-body");
        extra.files.insert("笔记/陌生.md".into(), b"# x\n".to_vec());
        assert_eq!(
            analyze(&author, &extra, &current, &current)
                .unwrap_err()
                .code,
            CANDIDATE_UNLISTED
        );
        let mut missing = candidate("edit-body");
        missing.files.clear();
        assert_eq!(
            analyze(&author, &missing, &current, &current)
                .unwrap_err()
                .code,
            CANDIDATE_MISSING
        );
    }

    #[test]
    fn link_parsing_ignores_external_and_anchor_targets() {
        assert_eq!(
            link_targets("[甲](甲.md)\n[乙](https://example.test)\n[丙](#标题)\n"),
            vec!["甲.md".to_owned()]
        );
        assert_eq!(
            resolve_link("笔记/力学.md", "光学.md").as_deref(),
            Some("笔记/光学.md")
        );
        assert_eq!(
            resolve_link("笔记/力学.md", "../x.md").as_deref(),
            Some("x.md")
        );
        assert_eq!(resolve_link("笔记/力学.md", "../../x.md"), None);
        assert_eq!(
            resolve_link("笔记/力学.md", "光学"),
            Some("笔记/光学.md".into())
        );
    }
}
