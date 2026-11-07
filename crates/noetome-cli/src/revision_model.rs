//! Strict revision contract: writing feedback raises gap problems, verified revisions
//! produce a new IR version, and every declared dependent representation is recompiled.
use crate::project::ProjectError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const POLICY: &str = "revision-feedback-v1";
pub(crate) const CORPUS_POLICY: &str = "revision-corpus-v1";
pub(crate) const PRODUCER: &str = "revision-feedback";
pub(crate) const AUTHOR_PRODUCER: &str = "fixture_author";
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_TEXT: usize = 600;
pub(crate) const MAX_ID: usize = 48;
pub(crate) const MAX_ATOMS: usize = 128;
pub(crate) const MAX_REPRESENTATIONS: usize = 16;
pub(crate) const MAX_EVIDENCE: usize = 8;
pub(crate) const MAX_UNIT: usize = 32;
pub(crate) const MAX_REVISIONS_LIMIT: u32 = 8;

pub(crate) const KIND_TABLE: &str = "table";
pub(crate) const KIND_PARAGRAPH: &str = "paragraph";
pub(crate) const KIND_TRACEABILITY: &str = "traceability";
pub(crate) const KINDS: [&str; 3] = [KIND_TABLE, KIND_PARAGRAPH, KIND_TRACEABILITY];

pub(crate) const VERIFIED: &str = "verified";
pub(crate) const PENDING: &str = "pending_verification";

pub(crate) const GAP_OPEN: &str = "open";
pub(crate) const GAP_RESOLVED: &str = "resolved";
pub(crate) const GAP_PENDING: &str = "pending_verification";
pub(crate) const GAP_ESCALATED: &str = "escalated";

pub(crate) const SEVERITY_NORMAL: &str = "normal";
pub(crate) const SEVERITY_MANUAL: &str = "manual_judgment_required";

pub(crate) fn argument(text: impl Into<String>) -> ProjectError {
    ProjectError::new("REVISION_ARGUMENT", 2, text)
}

pub(crate) fn integrity(text: impl Into<String>) -> ProjectError {
    ProjectError::new("REVISION_INTEGRITY", 3, text)
}

pub(crate) fn unknown_root(text: impl Into<String>) -> ProjectError {
    ProjectError::new("REVISION_UNKNOWN_ROOT", 3, text)
}

pub(crate) fn busy(text: impl Into<String>) -> ProjectError {
    ProjectError::new("REVISION_BUSY", 3, text)
}

pub(crate) fn escalated(text: impl Into<String>) -> ProjectError {
    ProjectError::new("REVISION_LIMIT_ESCALATED", 3, text)
}

pub(crate) fn no_new_evidence(text: impl Into<String>) -> ProjectError {
    ProjectError::new("REVISION_NO_NEW_EVIDENCE", 3, text)
}

pub(crate) fn corpus_mismatch(text: impl Into<String>) -> ProjectError {
    ProjectError::new("REVISION_CORPUS_MISMATCH", 3, text)
}

fn single(text: &str, label: &str, limit: usize) -> Result<String, ProjectError> {
    let trimmed = text.trim();
    if trimmed != text {
        return Err(argument(format!(
            "{label} 首尾不能有空白（实际 {text:?}）。"
        )));
    }
    if trimmed.is_empty() {
        return Err(argument(format!("{label} 不能为空。")));
    }
    if trimmed.chars().count() > limit {
        return Err(argument(format!(
            "{label} 超过 {limit} 字符（实际 {}）。",
            trimmed.chars().count()
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(argument(format!("{label} 不能包含控制字符。")));
    }
    Ok(trimmed.to_owned())
}

fn identifier(value: &str, label: &str) -> Result<String, ProjectError> {
    let text = single(value, label, MAX_ID)?;
    if !text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(argument(format!(
            "{label} 只能使用 ASCII 字母、数字与 - _ .（实际 {text}）。"
        )));
    }
    Ok(text)
}

fn relative(value: &str, label: &str) -> Result<String, ProjectError> {
    let text = single(value, label, MAX_TEXT)?;
    if text.starts_with('/') || text.contains('\\') {
        return Err(argument(format!(
            "{label} 必须是相对路径且不能包含反斜杠（实际 {text}）。"
        )));
    }
    if text.split('/').any(|part| part.is_empty() || part == "..") {
        return Err(argument(format!(
            "{label} 不能包含空段或 ..（实际 {text}）。"
        )));
    }
    Ok(text)
}

fn sha256_text(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EvidenceRef {
    pub(crate) source: String,
    pub(crate) locator: String,
}

impl EvidenceRef {
    fn validate(&self) -> Result<(), ProjectError> {
        relative(&self.source, "证据来源")?;
        let locator = single(&self.locator, "证据定位", MAX_TEXT)?;
        if !locator.starts_with('#') {
            return Err(argument(format!(
                "证据定位必须以 # 开头（实际 {locator}）。"
            )));
        }
        Ok(())
    }

    pub(crate) fn key(&self) -> String {
        format!("{}{}", self.source, self.locator)
    }

    pub(crate) fn text(&self) -> String {
        format!("{}{}", self.source, self.locator)
    }
}

fn evidence_list(items: &[EvidenceRef], label: &str) -> Result<(), ProjectError> {
    if items.len() > MAX_EVIDENCE {
        return Err(argument(format!(
            "{label} 最多 {MAX_EVIDENCE} 条（实际 {}）。",
            items.len()
        )));
    }
    let mut seen = BTreeSet::new();
    for item in items {
        item.validate()?;
        if !seen.insert(item.key()) {
            return Err(argument(format!(
                "{label} 出现重复来源 {item_key}。",
                item_key = item.key()
            )));
        }
    }
    Ok(())
}

pub(crate) fn evidence_keys(items: &[EvidenceRef]) -> BTreeSet<String> {
    items.iter().map(EvidenceRef::key).collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Atom {
    pub(crate) atom_id: String,
    pub(crate) subject: String,
    pub(crate) property: String,
    pub(crate) value: String,
    pub(crate) unit: String,
    pub(crate) evidence: Vec<EvidenceRef>,
}

impl Atom {
    fn validate(&self) -> Result<(), ProjectError> {
        identifier(&self.atom_id, "原子编号")?;
        single(&self.subject, "对象", MAX_TEXT)?;
        single(&self.property, "属性", MAX_TEXT)?;
        single(&self.value, "值", MAX_TEXT)?;
        if !self.unit.is_empty() {
            single(&self.unit, "单位", MAX_UNIT)?;
        }
        evidence_list(&self.evidence, &format!("原子 {} 的证据", self.atom_id))
    }

    pub(crate) fn verification(&self) -> &'static str {
        if self.evidence.is_empty() {
            PENDING
        } else {
            VERIFIED
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Representation {
    pub(crate) representation_id: String,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) atom_ids: Vec<String>,
}

impl Representation {
    fn validate(&self) -> Result<(), ProjectError> {
        identifier(&self.representation_id, "表示编号")?;
        if !KINDS.contains(&self.kind.as_str()) {
            return Err(argument(format!(
                "表示类型必须是 {} 之一（实际 {}）。",
                KINDS.join("、"),
                self.kind
            )));
        }
        single(&self.title, "表示标题", MAX_TEXT)?;
        if self.atom_ids.is_empty() {
            return Err(argument(format!(
                "表示 {} 至少要引用一个原子。",
                self.representation_id
            )));
        }
        let mut seen = BTreeSet::new();
        for atom_id in &self.atom_ids {
            identifier(atom_id, "表示引用的原子编号")?;
            if !seen.insert(atom_id.clone()) {
                return Err(argument(format!(
                    "表示 {} 重复引用原子 {atom_id}。",
                    self.representation_id
                )));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorpusSpec {
    pub(crate) root: String,
    pub(crate) manifest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorpusDocument {
    pub(crate) path: String,
    pub(crate) sha256: String,
    pub(crate) byte_length: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorpusManifest {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) documents: Vec<CorpusDocument>,
}

impl CorpusManifest {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ProjectError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|error| argument(format!("语料清单必须是 UTF-8 文本：{error}。")))?;
        let manifest: CorpusManifest = serde_json::from_str(text)
            .map_err(|error| argument(format!("语料清单不是严格 JSON：{error}。")))?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != CORPUS_POLICY {
            return Err(argument(format!(
                "语料清单 policy 必须是 {CORPUS_POLICY}（实际 {}）。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "语料清单 schema_version 必须是 {SCHEMA_VERSION}（实际 {}）。",
                self.schema_version
            )));
        }
        identifier(&self.case_id, "语料清单 case_id")?;
        if self.documents.is_empty() || self.documents.len() > MAX_REPRESENTATIONS * 4 {
            return Err(argument(format!(
                "语料清单文档数必须在 1 到 {} 之间（实际 {}）。",
                MAX_REPRESENTATIONS * 4,
                self.documents.len()
            )));
        }
        let mut seen = BTreeSet::new();
        for document in &self.documents {
            relative(&document.path, "语料文档路径")?;
            if !document.path.ends_with(".md") {
                return Err(argument(format!(
                    "语料文档必须以 .md 结尾（实际 {}）。",
                    document.path
                )));
            }
            if !seen.insert(document.path.clone()) {
                return Err(argument(format!("语料清单重复登记 {}。", document.path)));
            }
            if !sha256_text(&document.sha256) {
                return Err(argument(format!(
                    "语料文档 {} 的 sha256 必须是 64 位小写十六进制。",
                    document.path
                )));
            }
            if document.byte_length == 0 || document.byte_length > MAX_BYTES {
                return Err(argument(format!(
                    "语料文档 {} 的字节数必须在 1 到 {MAX_BYTES} 之间。",
                    document.path
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn total_bytes(&self) -> u64 {
        self.documents.iter().map(|item| item.byte_length).sum()
    }

    pub(crate) fn paths(&self) -> BTreeSet<String> {
        self.documents
            .iter()
            .map(|item| item.path.clone())
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Limits {
    pub(crate) max_revisions: u32,
}

impl Limits {
    fn validate(&self) -> Result<(), ProjectError> {
        if self.max_revisions == 0 || self.max_revisions > MAX_REVISIONS_LIMIT {
            return Err(argument(format!(
                "max_revisions 必须在 1 到 {MAX_REVISIONS_LIMIT} 之间（实际 {}）。",
                self.max_revisions
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Author {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) case_id: String,
    pub(crate) corpus: CorpusSpec,
    pub(crate) limits: Limits,
    pub(crate) atoms: Vec<Atom>,
    pub(crate) representations: Vec<Representation>,
}

impl Author {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ProjectError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|error| argument(format!("作者清单必须是 UTF-8 文本：{error}。")))?;
        let author: Author = serde_json::from_str(text)
            .map_err(|error| argument(format!("作者清单不是严格 JSON：{error}。")))?;
        author.validate()?;
        Ok(author)
    }

    fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!(
                "作者清单 policy 必须是 {POLICY}（实际 {}）。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "作者清单 schema_version 必须是 {SCHEMA_VERSION}（实际 {}）。",
                self.schema_version
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "作者清单 producer 必须是 {AUTHOR_PRODUCER}（实际 {}）。",
                self.producer
            )));
        }
        identifier(&self.case_id, "作者清单 case_id")?;
        relative(&self.corpus.root, "语料目录")?;
        relative(&self.corpus.manifest, "语料清单路径")?;
        self.limits.validate()?;
        if self.atoms.is_empty() || self.atoms.len() > MAX_ATOMS {
            return Err(argument(format!(
                "原子数必须在 1 到 {MAX_ATOMS} 之间（实际 {}）。",
                self.atoms.len()
            )));
        }
        let mut ids = BTreeSet::new();
        for atom in &self.atoms {
            atom.validate()?;
            if !ids.insert(atom.atom_id.clone()) {
                return Err(argument(format!("原子编号重复：{}。", atom.atom_id)));
            }
        }
        let mut pairs = BTreeSet::new();
        for atom in &self.atoms {
            let pair = (atom.subject.clone(), atom.property.clone());
            if !pairs.insert(pair) {
                return Err(argument(format!(
                    "同一对象与属性出现两次：{} 的 {}。",
                    atom.subject, atom.property
                )));
            }
        }
        if self.representations.is_empty() || self.representations.len() > MAX_REPRESENTATIONS {
            return Err(argument(format!(
                "表示数必须在 1 到 {MAX_REPRESENTATIONS} 之间（实际 {}）。",
                self.representations.len()
            )));
        }
        let mut reps = BTreeSet::new();
        for representation in &self.representations {
            representation.validate()?;
            if !reps.insert(representation.representation_id.clone()) {
                return Err(argument(format!(
                    "表示编号重复：{}。",
                    representation.representation_id
                )));
            }
            for atom_id in &representation.atom_ids {
                if !ids.contains(atom_id) {
                    return Err(argument(format!(
                        "表示 {} 引用了不存在的原子 {atom_id}。",
                        representation.representation_id
                    )));
                }
            }
        }
        for atom_id in &ids {
            if !self
                .representations
                .iter()
                .any(|representation| representation.atom_ids.contains(atom_id))
            {
                return Err(argument(format!(
                    "原子 {atom_id} 没有被任何表示引用：写作反馈不会覆盖它。"
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn atom_ids(&self) -> BTreeSet<String> {
        self.atoms.iter().map(|atom| atom.atom_id.clone()).collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GapInput {
    pub(crate) gap_id: String,
    pub(crate) representation_id: String,
    pub(crate) atom_id: String,
    pub(crate) reason: String,
    pub(crate) source: Vec<EvidenceRef>,
    pub(crate) impact: Vec<String>,
    pub(crate) note: String,
}

impl GapInput {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ProjectError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|error| argument(format!("缺口文件必须是 UTF-8 文本：{error}。")))?;
        let gap: GapInput = serde_json::from_str(text)
            .map_err(|error| argument(format!("缺口文件不是严格 JSON：{error}。")))?;
        gap.validate()?;
        Ok(gap)
    }

    fn validate(&self) -> Result<(), ProjectError> {
        identifier(&self.gap_id, "缺口编号")?;
        identifier(&self.representation_id, "缺口发现位置")?;
        identifier(&self.atom_id, "缺口涉及的原子编号")?;
        single(&self.reason, "缺口原因", MAX_TEXT)?;
        single(&self.note, "缺口说明", MAX_TEXT)?;
        if self.source.is_empty() {
            return Err(argument(
                "缺口必须带来源：写作反馈发现的缺口要能定位到原始资料。",
            ));
        }
        evidence_list(&self.source, "缺口来源")?;
        if self.impact.is_empty() {
            return Err(argument("缺口必须声明影响范围（至少一个表示）。"));
        }
        if self.impact.len() > MAX_REPRESENTATIONS {
            return Err(argument(format!(
                "影响范围最多 {MAX_REPRESENTATIONS} 项（实际 {}）。",
                self.impact.len()
            )));
        }
        let mut seen = BTreeSet::new();
        for item in &self.impact {
            identifier(item, "影响范围里的表示编号")?;
            if !seen.insert(item.clone()) {
                return Err(argument(format!("影响范围重复列出 {item}。")));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RevisionInput {
    pub(crate) revision_id: String,
    pub(crate) gap_id: String,
    pub(crate) atom_id: String,
    pub(crate) value: String,
    pub(crate) unit: String,
    pub(crate) evidence: Vec<EvidenceRef>,
    pub(crate) reason: String,
}

impl RevisionInput {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ProjectError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|error| argument(format!("修订文件必须是 UTF-8 文本：{error}。")))?;
        let revision: RevisionInput = serde_json::from_str(text)
            .map_err(|error| argument(format!("修订文件不是严格 JSON：{error}。")))?;
        revision.validate()?;
        Ok(revision)
    }

    fn validate(&self) -> Result<(), ProjectError> {
        identifier(&self.revision_id, "修订编号")?;
        identifier(&self.gap_id, "修订对应的缺口编号")?;
        identifier(&self.atom_id, "修订涉及的原子编号")?;
        single(&self.value, "修订后的值", MAX_TEXT)?;
        if !self.unit.is_empty() {
            single(&self.unit, "修订后的单位", MAX_UNIT)?;
        }
        single(&self.reason, "修订原因", MAX_TEXT)?;
        evidence_list(&self.evidence, "修订证据")?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GapRecord {
    pub(crate) gap_id: String,
    pub(crate) representation_id: String,
    pub(crate) atom_id: String,
    pub(crate) reason: String,
    pub(crate) source: Vec<EvidenceRef>,
    pub(crate) impact: Vec<String>,
    pub(crate) note: String,
    pub(crate) status: String,
    pub(crate) severity: String,
    pub(crate) revisions: u32,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

impl GapRecord {
    pub(crate) fn from_input(input: GapInput, at: String) -> Self {
        GapRecord {
            gap_id: input.gap_id,
            representation_id: input.representation_id,
            atom_id: input.atom_id,
            reason: input.reason,
            source: input.source,
            impact: input.impact,
            note: input.note,
            status: GAP_OPEN.into(),
            severity: SEVERITY_NORMAL.into(),
            revisions: 0,
            created_at: at.clone(),
            updated_at: at,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RevisionRecord {
    pub(crate) revision_id: String,
    pub(crate) gap_id: String,
    pub(crate) atom_id: String,
    pub(crate) value: String,
    pub(crate) unit: String,
    pub(crate) evidence: Vec<EvidenceRef>,
    pub(crate) reason: String,
    pub(crate) ir_version: u32,
    pub(crate) recompiled: Vec<String>,
    pub(crate) unchanged: Vec<String>,
    pub(crate) at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IrVersion {
    pub(crate) version: u32,
    pub(crate) atoms_sha256: String,
    pub(crate) atoms: Vec<Atom>,
    pub(crate) at: String,
}

impl IrVersion {
    pub(crate) fn verification(&self) -> BTreeMap<String, u64> {
        let mut counts = BTreeMap::new();
        counts.insert(VERIFIED.to_owned(), 0u64);
        counts.insert(PENDING.to_owned(), 0u64);
        for atom in &self.atoms {
            *counts.entry(atom.verification().to_owned()).or_insert(0) += 1;
        }
        counts
    }
}

/// Deterministic rendering of one representation from the current atoms.
pub(crate) fn render(
    representation: &Representation,
    atoms: &BTreeMap<String, Atom>,
) -> Result<String, ProjectError> {
    let mut ordered = Vec::new();
    for atom_id in &representation.atom_ids {
        let atom = atoms.get(atom_id).ok_or_else(|| {
            integrity(format!(
                "表示 {} 引用的原子 {atom_id} 不在当前 IR 里。",
                representation.representation_id
            ))
        })?;
        ordered.push(atom);
    }
    let marker = |atom: &Atom| -> &'static str {
        if atom.verification() == PENDING {
            "【待核验】"
        } else {
            ""
        }
    };
    let mut lines = Vec::new();
    match representation.kind.as_str() {
        KIND_TABLE => {
            lines.push(format!("## {}", representation.title));
            lines.push(String::new());
            lines.push("| 对象 | 属性 | 值 | 单位 | 证据 | 状态 |".to_owned());
            lines.push("| --- | --- | --- | --- | --- | --- |".to_owned());
            for atom in ordered {
                lines.push(format!(
                    "| {} | {} | {}{} | {} | {} | {} |",
                    atom.subject,
                    atom.property,
                    marker(atom),
                    atom.value,
                    if atom.unit.is_empty() {
                        "-"
                    } else {
                        &atom.unit
                    },
                    atom.evidence.len(),
                    atom.verification()
                ));
            }
        }
        KIND_PARAGRAPH => {
            lines.push(format!("# {}", representation.title));
            lines.push(String::new());
            for atom in ordered {
                lines.push(format!(
                    "- {} 的 {} 是 {}{}{}（证据 {} 条；状态 {}）",
                    atom.subject,
                    atom.property,
                    marker(atom),
                    atom.value,
                    if atom.unit.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", atom.unit)
                    },
                    atom.evidence.len(),
                    atom.verification()
                ));
            }
        }
        KIND_TRACEABILITY => {
            lines.push(format!("# {}", representation.title));
            lines.push(String::new());
            for atom in ordered {
                if atom.evidence.is_empty() {
                    lines.push(format!("- {} ← 无来源，待核验", atom.atom_id));
                } else {
                    for evidence in &atom.evidence {
                        lines.push(format!("- {} ← {}", atom.atom_id, evidence.text()));
                    }
                }
            }
        }
        other => {
            return Err(argument(format!(
                "表示类型必须是 {} 之一（实际 {other}）。",
                KINDS.join("、")
            )));
        }
    }
    let mut text = lines.join("\n");
    text.push('\n');
    Ok(text)
}

pub(crate) fn atoms_map(atoms: &[Atom]) -> BTreeMap<String, Atom> {
    atoms
        .iter()
        .map(|atom| (atom.atom_id.clone(), atom.clone()))
        .collect()
}

/// Which representations declare a dependency on the given atom.
pub(crate) fn dependents(representations: &[Representation], atom_id: &str) -> BTreeSet<String> {
    representations
        .iter()
        .filter(|representation| representation.atom_ids.iter().any(|item| item == atom_id))
        .map(|representation| representation.representation_id.clone())
        .collect()
}

pub(crate) fn reason_text(status: &str) -> &'static str {
    match status {
        GAP_OPEN => "待处理",
        GAP_RESOLVED => "已解决",
        GAP_PENDING => "新增内容待核验",
        GAP_ESCALATED => "已升级为人工判断",
        _ => "未知",
    }
}

pub(crate) fn kind_text(kind: &str) -> &'static str {
    match kind {
        KIND_TABLE => "表格",
        KIND_PARAGRAPH => "正文段落",
        KIND_TRACEABILITY => "追溯关系",
        _ => "未知",
    }
}
