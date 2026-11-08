//! Diagnostics export: scope, content preview, and what the user did not choose.
//!
//! The declaration is a set of **categories**, each holding concrete entries with a
//! byte count and a short preview. `selection` is the user's checkbox: a category is
//! exported only if it is listed there. Everything else is reported as
//! `included: false` — an export must never look complete when it is not.
//!
//! Two shapes are refused outright, before a single byte is written: absolute paths
//! (a package that leaks the machine layout cannot be shared) and credential-shaped
//! strings (FR-40: 导出诊断不包含明文凭据).

use crate::project::ProjectError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const POLICY: &str = "diagnostics-export-v1";
pub const AUTHOR_PRODUCER: &str = "fixture_author";
pub const SCHEMA_VERSION: u32 = 1;
pub const EXPORT_VERSION: u32 = 1;

/// Upper bound on the declaration file itself.
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_CATEGORIES: usize = 8;
pub const MAX_ENTRIES: usize = 64;
pub const MAX_PREVIEW: usize = 240;
pub const MAX_ID: usize = 48;
pub const MAX_TEXT: usize = 300;
pub const MAX_LABEL: usize = 120;
pub const MAX_CONFIG: usize = 16;
pub const MAX_FINGERPRINTS: usize = 16;

/// Credential-shaped markers. Deliberately literal: a package that only *looks*
/// like it might carry a secret is still refused, because the export gets shared.
pub const SECRET_MARKERS: [&str; 8] = [
    "api_key",
    "apikey",
    "password",
    "secret",
    "bearer ",
    "authorization:",
    "token=",
    "-----begin",
];

pub(crate) fn argument(message: impl Into<String>) -> ProjectError {
    ProjectError::new("DIAGNOSTICS_ARGUMENT", 2, message)
}

pub(crate) fn integrity(message: impl Into<String>) -> ProjectError {
    ProjectError::new("DIAGNOSTICS_INTEGRITY", 3, message)
}

pub(crate) fn unknown_root(message: impl Into<String>) -> ProjectError {
    ProjectError::new("DIAGNOSTICS_UNKNOWN_ROOT", 3, message)
}

/// A sensitive category was selected without the user's consent record.
pub(crate) fn consent(message: impl Into<String>) -> ProjectError {
    ProjectError::new("DIAGNOSTICS_CONSENT", 3, message)
}

pub(crate) fn secret_marker(message: impl Into<String>) -> ProjectError {
    ProjectError::new("DIAGNOSTICS_SECRET_MARKER", 3, message)
}

pub(crate) fn absolute_path(message: impl Into<String>) -> ProjectError {
    ProjectError::new("DIAGNOSTICS_ABSOLUTE_PATH", 3, message)
}

fn single(value: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(argument(format!("{label}不能为空。")));
    }
    if trimmed.chars().count() > limit {
        return Err(argument(format!(
            "{label}最多 {limit} 个字符：收到 {} 个。",
            trimmed.chars().count()
        )));
    }
    Ok(())
}

fn identifier(value: &str, label: &str) -> Result<(), ProjectError> {
    single(value, label, MAX_ID)?;
    let ok = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !ok {
        return Err(argument(format!(
            "{label}只能包含 ASCII 字母、数字、连字符与下划线：收到「{value}」。"
        )));
    }
    Ok(())
}

/// Absolute path shapes we refuse anywhere in the declaration.
///
/// The check is not anchored at the start of the string: a path pasted into the
/// middle of a sentence (``见 /home/... 下的样例``) leaks the machine layout just
/// as well as one that starts the line, so every path-looking token counts.
fn absolute_forms(value: &str) -> bool {
    let chars: Vec<char> = value.chars().collect();
    let starts_token = |index: usize| -> bool {
        if index == 0 {
            return true;
        }
        let previous = chars[index - 1];
        previous.is_whitespace()
            || matches!(
                previous,
                '(' | '（'
                    | '「'
                    | '['
                    | '【'
                    | '：'
                    | ':'
                    | ','
                    | '，'
                    | ';'
                    | '；'
                    | '='
                    | '"'
                    | '\''
            )
    };
    for (index, current) in chars.iter().enumerate() {
        // A leading slash or backslash at a token boundary starts an absolute path.
        if (*current == '/' || *current == '\\') && starts_token(index) {
            return true;
        }
        // `C:` / `/mnt/d/` / `/mnt/c/` — a drive letter followed by a separator.
        if index + 2 < chars.len()
            && current.is_ascii_alphabetic()
            && chars[index + 1] == ':'
            && (chars[index + 2] == '/' || chars[index + 2] == '\\')
        {
            return true;
        }
    }
    false
}

/// Every label/value pair that a human might have pasted into.
fn secret_hits<'a, I>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut hits = BTreeSet::new();
    for (label, value) in values {
        let lowered = value.to_ascii_lowercase();
        for marker in SECRET_MARKERS {
            if lowered.contains(marker) {
                hits.insert(format!("{label} 里出现疑似凭据字样「{marker}」"));
            }
        }
    }
    hits.into_iter().collect()
}

/// Every string in the declaration that could carry a path or a secret.
fn declaration_strings(declaration: &Declaration) -> Vec<(String, String)> {
    let mut out = Vec::new();
    out.push(("案例编号".to_owned(), declaration.case_id.clone()));
    out.push(("说明".to_owned(), declaration.description.clone()));
    for entry in &declaration.config {
        out.push((format!("配置「{}」的键", entry.key), entry.key.clone()));
        out.push((format!("配置「{}」的值", entry.key), entry.value.clone()));
    }
    for fingerprint in &declaration.fingerprints {
        out.push((
            format!("指纹「{}」的名称", fingerprint.name),
            fingerprint.name.clone(),
        ));
        out.push((
            format!("指纹「{}」的摘要", fingerprint.name),
            fingerprint.digest.clone(),
        ));
    }
    for category in &declaration.categories {
        out.push((
            format!("范围类别「{}」的编号", category.category_id),
            category.category_id.clone(),
        ));
        out.push((
            format!("范围类别「{}」的名称", category.category_id),
            category.label.clone(),
        ));
        for entry in &category.entries {
            out.push((
                format!("条目「{}」的编号", entry.entry_id),
                entry.entry_id.clone(),
            ));
            out.push((
                format!("条目「{}」的名称", entry.entry_id),
                entry.label.clone(),
            ));
            out.push((
                format!("条目「{}」的预览", entry.entry_id),
                entry.preview.clone(),
            ));
        }
    }
    out
}

fn as_pairs(values: &[(String, String)]) -> Vec<(&str, &str)> {
    values
        .iter()
        .map(|(label, value)| (label.as_str(), value.as_str()))
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Fingerprint {
    pub name: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EntrySpec {
    pub entry_id: String,
    pub label: String,
    pub bytes: u64,
    /// Short preview written by the declaration; never the full body.
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategorySpec {
    pub category_id: String,
    pub label: String,
    /// Sensitive categories may only be exported with recorded consent.
    pub sensitive: bool,
    pub entries: Vec<EntrySpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Consent {
    pub sensitive_approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_categories: usize,
    pub max_entries: usize,
    pub max_preview: usize,
}

impl Limits {
    fn validate(&self) -> Result<(), ProjectError> {
        if self.max_categories == 0 || self.max_categories > MAX_CATEGORIES {
            return Err(argument(format!(
                "范围类别上限必须在 1 到 {MAX_CATEGORIES} 之间：收到 {}。",
                self.max_categories
            )));
        }
        if self.max_entries == 0 || self.max_entries > MAX_ENTRIES {
            return Err(argument(format!(
                "条目上限必须在 1 到 {MAX_ENTRIES} 之间：收到 {}。",
                self.max_entries
            )));
        }
        if self.max_preview == 0 || self.max_preview > MAX_PREVIEW {
            return Err(argument(format!(
                "预览上限必须在 1 到 {MAX_PREVIEW} 之间：收到 {}。",
                self.max_preview
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    pub policy: String,
    pub schema_version: u32,
    pub producer: String,
    pub case_id: String,
    pub description: String,
    /// Enough configuration to reproduce the problem, not the whole world.
    pub config: Vec<ConfigEntry>,
    /// Digests of the inputs the problem happened on.
    pub fingerprints: Vec<Fingerprint>,
    pub categories: Vec<CategorySpec>,
    /// The user's checkbox: category ids that were selected for export.
    pub selection: Vec<String>,
    pub consent: Consent,
    pub limits: Limits,
}

impl Declaration {
    pub fn parse(text: &str) -> Result<Self, ProjectError> {
        let declaration: Declaration = serde_json::from_str(text)
            .map_err(|error| argument(format!("诊断导出声明不是合法 JSON：{error}")))?;
        declaration.validate()?;
        Ok(declaration)
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!(
                "策略必须是 {POLICY}：收到「{}」。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "结构版本必须是 {SCHEMA_VERSION}：收到 {}。",
                self.schema_version
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "生产者必须是 {AUTHOR_PRODUCER}：收到「{}」。",
                self.producer
            )));
        }
        identifier(&self.case_id, "案例编号")?;
        single(&self.description, "说明", MAX_TEXT)?;
        self.limits.validate()?;

        if self.config.len() > MAX_CONFIG {
            return Err(argument(format!(
                "配置项最多 {MAX_CONFIG} 条：收到 {} 条。",
                self.config.len()
            )));
        }
        let mut seen_keys = BTreeSet::new();
        for entry in &self.config {
            identifier(&entry.key, "配置键")?;
            single(&entry.value, "配置值", MAX_TEXT)?;
            if !seen_keys.insert(entry.key.as_str()) {
                return Err(argument(format!("配置键重复：{}。", entry.key)));
            }
        }

        if self.fingerprints.len() > MAX_FINGERPRINTS {
            return Err(argument(format!(
                "指纹最多 {MAX_FINGERPRINTS} 条：收到 {} 条。",
                self.fingerprints.len()
            )));
        }
        let mut seen_names = BTreeSet::new();
        for fingerprint in &self.fingerprints {
            identifier(&fingerprint.name, "指纹名称")?;
            if fingerprint.digest.len() != 64
                || !fingerprint
                    .digest
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            {
                return Err(argument(format!(
                    "指纹「{}」的摘要必须是 64 位小写十六进制。",
                    fingerprint.name
                )));
            }
            if !seen_names.insert(fingerprint.name.as_str()) {
                return Err(argument(format!("指纹名称重复：{}。", fingerprint.name)));
            }
        }

        if self.categories.is_empty() {
            return Err(argument("至少要声明一个范围类别。".to_owned()));
        }
        if self.categories.len() > self.limits.max_categories {
            return Err(argument(format!(
                "范围类别最多 {} 个：收到 {} 个。",
                self.limits.max_categories,
                self.categories.len()
            )));
        }
        let mut seen_categories = BTreeSet::new();
        let mut seen_entries = BTreeSet::new();
        for category in &self.categories {
            identifier(&category.category_id, "范围类别编号")?;
            single(&category.label, "范围类别名称", MAX_LABEL)?;
            if !seen_categories.insert(category.category_id.as_str()) {
                return Err(argument(format!(
                    "范围类别编号重复：{}。",
                    category.category_id
                )));
            }
            if category.entries.is_empty() {
                return Err(argument(format!(
                    "范围类别 {} 没有任何条目：空类别会让预览看起来比实际更完整。",
                    category.category_id
                )));
            }
            if category.entries.len() > self.limits.max_entries {
                return Err(argument(format!(
                    "范围类别 {} 的条目最多 {} 条：收到 {} 条。",
                    category.category_id,
                    self.limits.max_entries,
                    category.entries.len()
                )));
            }
            for entry in &category.entries {
                identifier(&entry.entry_id, "条目编号")?;
                single(&entry.label, "条目名称", MAX_LABEL)?;
                let preview_chars = entry.preview.chars().count();
                if preview_chars > self.limits.max_preview {
                    return Err(argument(format!(
                        "条目 {} 的预览最多 {} 个字符：收到 {preview_chars} 个。",
                        entry.entry_id, self.limits.max_preview
                    )));
                }
                if !seen_entries.insert(entry.entry_id.as_str()) {
                    return Err(argument(format!("条目编号重复：{}。", entry.entry_id)));
                }
            }
        }

        let mut seen_selection = BTreeSet::new();
        for category_id in &self.selection {
            identifier(category_id, "勾选的范围类别")?;
            if !seen_categories.contains(category_id.as_str()) {
                return Err(argument(format!("勾选了不存在的范围类别：{category_id}。")));
            }
            if !seen_selection.insert(category_id.as_str()) {
                return Err(argument(format!("范围类别被勾选了两次：{category_id}。")));
            }
        }

        // Absolute paths anywhere: the package gets shared, so machine layout leaks.
        let strings = declaration_strings(self);
        let mut absolute = Vec::new();
        for (label, value) in &strings {
            if absolute_forms(value) {
                absolute.push(format!("{label}「{value}」"));
            }
        }
        if !absolute.is_empty() {
            return Err(absolute_path(format!(
                "诊断声明里出现绝对路径：{}。诊断包会被分享出去，只能写相对路径或对数编号。",
                absolute.join("；")
            )));
        }

        let hits = secret_hits(as_pairs(&strings));
        if !hits.is_empty() {
            return Err(secret_marker(format!(
                "诊断声明里出现疑似凭据：{}。导出会跟着被分享出去，凭据不能进声明；请改成引用凭据名。",
                hits.join("；")
            )));
        }
        Ok(())
    }

    pub fn is_selected(&self, category_id: &str) -> bool {
        self.selection.iter().any(|id| id == category_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EntryRecord {
    pub entry_id: String,
    pub category_id: String,
    pub label: String,
    pub bytes: u64,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryRecord {
    pub category_id: String,
    pub label: String,
    pub sensitive: bool,
    pub included: bool,
    pub entries: usize,
    pub selected_entries: usize,
    /// Bytes of the entries this category would contribute if selected.
    pub bytes: u64,
    /// Bytes actually carried by the export (zero when not selected).
    pub exported_bytes: u64,
    /// Preview of the first entry, so a reader sees the shape before opening it.
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub categories: usize,
    pub selected_categories: usize,
    pub excluded_categories: usize,
    pub sensitive_categories: usize,
    pub sensitive_included: usize,
    pub entries: usize,
    pub selected_entries: usize,
    pub exported_bytes: u64,
    pub excluded_bytes: u64,
    pub config: usize,
    pub fingerprints: usize,
    pub model_calls: u64,
    pub network_requests: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExportFile {
    pub policy: String,
    pub export_version: u32,
    pub case_id: String,
    pub description: String,
    pub config: Vec<ConfigEntry>,
    pub fingerprints: Vec<Fingerprint>,
    /// Only the selected entries, in declaration order.
    pub entries: Vec<EntryRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub policy: String,
    pub schema_version: u32,
    pub case_id: String,
    pub description: String,
    pub counts: Counts,
    pub categories: Vec<CategoryRecord>,
    pub model_calls: u64,
    pub network_requests: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Export {
    pub export: ExportFile,
    pub report: Report,
}

pub(crate) fn export(declaration: &Declaration) -> Result<Export, ProjectError> {
    declaration.validate()?;

    // Consent is checked before anything is counted: a sensitive category the user
    // selected without recording consent is a hard stop, not a preview note.
    let mut sensitive_selected = Vec::new();
    for category in &declaration.categories {
        if category.sensitive && declaration.is_selected(&category.category_id) {
            sensitive_selected.push(category.category_id.clone());
        }
    }
    if !sensitive_selected.is_empty() && !declaration.consent.sensitive_approved {
        return Err(consent(format!(
            "勾选了敏感范围 {}，但没有记录同意：导出前必须明确同意包含敏感材料。",
            sensitive_selected.join("、")
        )));
    }

    let mut entries = Vec::new();
    let mut records = Vec::new();
    let mut counts = Counts {
        categories: declaration.categories.len(),
        selected_categories: 0,
        excluded_categories: 0,
        sensitive_categories: 0,
        sensitive_included: 0,
        entries: 0,
        selected_entries: 0,
        exported_bytes: 0,
        excluded_bytes: 0,
        config: declaration.config.len(),
        fingerprints: declaration.fingerprints.len(),
        model_calls: 0,
        network_requests: 0,
    };

    for category in &declaration.categories {
        let included = declaration.is_selected(&category.category_id);
        let bytes: u64 = category.entries.iter().map(|entry| entry.bytes).sum();
        let mut selected_entries = 0;
        for entry in &category.entries {
            counts.entries += 1;
            if included {
                entries.push(EntryRecord {
                    entry_id: entry.entry_id.clone(),
                    category_id: category.category_id.clone(),
                    label: entry.label.clone(),
                    bytes: entry.bytes,
                    preview: entry.preview.clone(),
                });
                selected_entries += 1;
                counts.selected_entries += 1;
                counts.exported_bytes += entry.bytes;
            } else {
                counts.excluded_bytes += entry.bytes;
            }
        }
        if included {
            counts.selected_categories += 1;
            if category.sensitive {
                counts.sensitive_included += 1;
            }
        } else {
            counts.excluded_categories += 1;
        }
        if category.sensitive {
            counts.sensitive_categories += 1;
        }
        records.push(CategoryRecord {
            category_id: category.category_id.clone(),
            label: category.label.clone(),
            sensitive: category.sensitive,
            included,
            entries: category.entries.len(),
            selected_entries,
            bytes,
            exported_bytes: if included { bytes } else { 0 },
            preview: category
                .entries
                .first()
                .map(|entry| entry.preview.clone())
                .unwrap_or_default(),
        });
    }

    let export = ExportFile {
        policy: POLICY.to_owned(),
        export_version: EXPORT_VERSION,
        case_id: declaration.case_id.clone(),
        description: declaration.description.clone(),
        config: declaration.config.clone(),
        fingerprints: declaration.fingerprints.clone(),
        entries,
    };
    let report = Report {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: declaration.case_id.clone(),
        description: declaration.description.clone(),
        counts,
        categories: records,
        model_calls: 0,
        network_requests: 0,
    };
    Ok(Export { export, report })
}

/// Per-category preview block for the markdown report.
fn category_rows(report: &Report) -> String {
    let mut out = String::from("| 范围类别 | 名称 | 敏感 | 是否导出 | 条目 | 字节 | 预览 |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
    for category in &report.categories {
        let included = if category.included {
            "已导出"
        } else {
            "未导出"
        };
        let sensitive = if category.sensitive { "是" } else { "否" };
        let preview = category.preview.replace('|', "\\|").replace('\n', " ");
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            category.category_id,
            category.label,
            sensitive,
            included,
            category.entries,
            category.bytes,
            preview
        ));
    }
    out
}

pub(crate) fn render_markdown(export: &Export) -> String {
    let report = &export.report;
    let counts = &report.counts;
    let mut out = String::new();
    out.push_str(&format!("# 诊断导出（{}）\n\n", report.case_id));
    out.push_str(&format!("{}\n\n", report.description));
    out.push_str("## 导出范围\n\n");
    out.push_str(&format!(
        "- 范围类别 {} 个：已导出 {} 个、未导出 {} 个；敏感类别 {} 个，其中已导出 {} 个。\n",
        counts.categories,
        counts.selected_categories,
        counts.excluded_categories,
        counts.sensitive_categories,
        counts.sensitive_included
    ));
    out.push_str(&format!(
        "- 条目 {} 条：已导出 {} 条，共 {} 字节；未导出 {} 字节。\n",
        counts.entries, counts.selected_entries, counts.exported_bytes, counts.excluded_bytes
    ));
    out.push_str(&format!(
        "- 复现信息：配置 {} 项、输入指纹 {} 个。\n\n",
        counts.config, counts.fingerprints
    ));
    out.push_str("## 类别与预览\n\n");
    out.push_str(&category_rows(report));
    out.push('\n');
    out.push_str("## 未导出的类别\n\n");
    let excluded: Vec<&CategoryRecord> = report
        .categories
        .iter()
        .filter(|category| !category.included)
        .collect();
    if excluded.is_empty() {
        out.push_str("- 没有未导出的类别：本次勾选覆盖了全部范围。\n\n");
    } else {
        for category in excluded {
            out.push_str(&format!(
                "- {}（{}）：未勾选，如实标为未导出；这 {} 条、{} 字节不在包里。\n",
                category.category_id, category.label, category.entries, category.bytes
            ));
        }
        out.push('\n');
    }
    out.push_str("## 边界\n\n");
    out.push_str("- 只导出声明的类别与条目：没有勾选的内容不会被静默补全。\n");
    out.push_str("- 预览是声明写好的短摘要，不是原文全文；默认不含原文与来源正文。\n");
    out.push_str("- 声明里出现绝对路径或疑似凭据时整包拒绝，拒绝时不写出任何字节。\n");
    out.push_str("- 导出清单不含时间戳：同一冻结声明的两次导出字节相同。\n");
    out.push_str("- 模型调用与网络请求 0 次；不判断内容质量与科学正确性。\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    fn declaration_text() -> String {
        serde_json::json!({
            "policy": POLICY,
            "schema_version": SCHEMA_VERSION,
            "producer": AUTHOR_PRODUCER,
            "case_id": "diag-metabolism-v1",
            "description": "糖酵解笔记生成失败时的诊断导出样例。",
            "config": [
                {"key": "model", "value": "deepseek-flash"},
                {"key": "max_rounds", "value": "3"}
            ],
            "fingerprints": [
                {"name": "note-glycolysis", "digest": "a".repeat(64)},
                {"name": "source-textbook-a", "digest": "b".repeat(64)}
            ],
            "categories": [
                {
                    "category_id": "journal-events",
                    "label": "审计事件",
                    "sensitive": false,
                    "entries": [
                        {"entry_id": "ev-1", "label": "task-parse started", "bytes": 120, "preview": "在第 1 轮开始解析来源。"},
                        {"entry_id": "ev-2", "label": "task-parse failed", "bytes": 180, "preview": "第 12 页没有文本层。"}
                    ]
                },
                {
                    "category_id": "run-logs",
                    "label": "运行日志",
                    "sensitive": false,
                    "entries": [
                        {"entry_id": "log-1", "label": "第 1 轮", "bytes": 400, "preview": "读取三个来源，编译两份笔记。"}
                    ]
                },
                {
                    "category_id": "source-body",
                    "label": "来源正文",
                    "sensitive": true,
                    "entries": [
                        {"entry_id": "body-1", "label": "教材A 第三章", "bytes": 9000, "preview": "糖酵解十步反应的原文段落节选。"}
                    ]
                }
            ],
            "selection": ["journal-events", "run-logs"],
            "consent": {"sensitive_approved": false},
            "limits": {"max_categories": 8, "max_entries": 16, "max_preview": 240}
        })
        .to_string()
    }

    fn declaration() -> Declaration {
        Declaration::parse(&declaration_text()).expect("valid declaration")
    }

    fn error_of(declaration: &Declaration) -> ProjectError {
        if let Err(error) = declaration.validate() {
            return error;
        }
        super::export(declaration).expect_err("expected a refusal")
    }

    #[test]
    fn the_same_declaration_produces_the_same_export() {
        let first = super::export(&declaration()).unwrap();
        let second = super::export(&declaration()).unwrap();
        assert_eq!(
            serde_json::to_string(&first.export).unwrap(),
            serde_json::to_string(&second.export).unwrap()
        );
        assert_eq!(
            serde_json::to_string(&first.report).unwrap(),
            serde_json::to_string(&second.report).unwrap()
        );
    }

    #[test]
    fn unselected_categories_are_reported_not_silently_filled() {
        let export = super::export(&declaration()).unwrap();
        assert_eq!(export.report.counts.categories, 3);
        assert_eq!(export.report.counts.selected_categories, 2);
        assert_eq!(export.report.counts.excluded_categories, 1);
        let body = export
            .report
            .categories
            .iter()
            .find(|category| category.category_id == "source-body")
            .expect("source-body");
        assert!(!body.included);
        assert_eq!(body.exported_bytes, 0);
        assert_eq!(body.bytes, 9000);
        assert_eq!(export.report.counts.excluded_bytes, 9000);
        // The export file carries only the selected entries.
        assert_eq!(export.export.entries.len(), 3);
        assert!(
            export
                .export
                .entries
                .iter()
                .all(|entry| entry.category_id != "source-body")
        );
    }

    #[test]
    fn selecting_a_sensitive_category_needs_recorded_consent() {
        let mut declaration = declaration();
        declaration.selection.push("source-body".to_owned());
        let error = error_of(&declaration);
        assert_eq!(error.code(), "DIAGNOSTICS_CONSENT");
        assert_eq!(error.exit_code(), 3);

        declaration.consent.sensitive_approved = true;
        let export = super::export(&declaration).unwrap();
        assert_eq!(export.report.counts.sensitive_included, 1);
        assert_eq!(export.report.counts.exported_bytes, 120 + 180 + 400 + 9000);
    }

    #[test]
    fn an_absolute_path_refuses_the_whole_declaration() {
        let text = declaration_text().replace("读取三个来源", "/home/lyinn/读取三个来源");
        let error = Declaration::parse(&text).expect_err("absolute path");
        assert_eq!(error.code(), "DIAGNOSTICS_ABSOLUTE_PATH");
        assert_eq!(error.exit_code(), 3);
    }

    #[test]
    fn a_path_pasted_mid_sentence_is_still_an_absolute_path() {
        // The first version only flagged strings that *started* with a path, so a
        // path pasted into the middle of a sentence sailed through. Machine layout
        // leaks either way, so the scan must look at every token.
        let text = declaration_text().replace(
            "第 12 页没有文本层。",
            "第 12 页没有文本层，见 /home/lyinn/project/Noetome 下的样例。",
        );
        let error = Declaration::parse(&text).expect_err("mid-sentence path");
        assert_eq!(error.code(), "DIAGNOSTICS_ABSOLUTE_PATH");
    }

    #[test]
    fn a_credential_shaped_preview_refuses_the_whole_declaration() {
        let text = declaration_text().replace("第 12 页没有文本层", "password=hunter2");
        let error = Declaration::parse(&text).expect_err("secret");
        assert_eq!(error.code(), "DIAGNOSTICS_SECRET_MARKER");
        assert_eq!(error.exit_code(), 3);
    }

    #[test]
    fn selecting_an_unknown_category_is_refused() {
        let mut declaration = declaration();
        declaration.selection.push("not-a-category".to_owned());
        let error = error_of(&declaration);
        assert_eq!(error.code(), "DIAGNOSTICS_ARGUMENT");
        assert_eq!(error.exit_code(), 2);
    }

    #[test]
    fn an_empty_category_is_refused() {
        let mut declaration = declaration();
        declaration.categories[1].entries.clear();
        let error = error_of(&declaration);
        assert_eq!(error.code(), "DIAGNOSTICS_ARGUMENT");
    }

    #[test]
    fn a_preview_longer_than_the_limit_is_refused() {
        let mut declaration = declaration();
        declaration.categories[0].entries[0].preview = "糖".repeat(241);
        let error = error_of(&declaration);
        assert_eq!(error.code(), "DIAGNOSTICS_ARGUMENT");
    }

    #[test]
    fn a_fingerprint_that_is_not_a_digest_is_refused() {
        let mut declaration = declaration();
        declaration.fingerprints[0].digest = "not-a-digest".to_owned();
        let error = error_of(&declaration);
        assert_eq!(error.code(), "DIAGNOSTICS_ARGUMENT");
    }

    #[test]
    fn the_markdown_lists_the_excluded_category_and_the_boundaries() {
        let export = super::export(&declaration()).unwrap();
        let markdown = super::render_markdown(&export);
        assert!(markdown.contains("## 类别与预览"));
        assert!(markdown.contains("## 未导出的类别"));
        assert!(markdown.contains("source-body"));
        assert!(markdown.contains("未勾选，如实标为未导出"));
        assert!(markdown.contains("导出清单不含时间戳"));
        assert!(markdown.contains("不判断内容质量与科学正确性"));
    }

    #[test]
    fn every_entry_carries_its_category_bytes_and_preview() {
        let export = super::export(&declaration()).unwrap();
        let mut by_entry = BTreeMap::new();
        for entry in &export.export.entries {
            by_entry.insert(entry.entry_id.as_str(), entry);
        }
        let entry = by_entry.get("ev-2").expect("ev-2");
        assert_eq!(entry.category_id, "journal-events");
        assert_eq!(entry.bytes, 180);
        assert_eq!(entry.preview, "第 12 页没有文本层。");
    }
}
