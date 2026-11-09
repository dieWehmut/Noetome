//! Recover validated, unpublished API candidates into a new test Vault.
//! No network calls, credentials, scientific adjudication or production publishing.
use crate::{
    extraction_api_config::RequestStrategy,
    extraction_api_wire::{self, ApiCandidates},
    extraction_model::{ExtractionLimits, Locator, RequestUnit, Selection, SourceBinding},
    extraction_output::{OutputRoot, inside},
    extraction_review_model::SavedCandidate,
    extraction_source::SourceSession,
    project::{self, ProjectError},
    sidecar_process::Control,
    sidecar_workspace::{Directory, GuardedFile},
    source_model::SourceVersion,
    text::sha256,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};
const INPUT_CHANGED: &str = "PREVIEW_INPUT_CHANGED";
const API_FILE_BYTES: u64 = 8 * 1024 * 1024;
fn error(code: &'static str, message: impl Into<String>) -> ProjectError {
    ProjectError::new(code, 2, message)
}
fn invalid() -> ProjectError {
    error(
        "PREVIEW_INVALID_API",
        "保存的 API 请求、原始响应、候选或成功报告不一致；没有导出笔记。",
    )
}
fn control() -> Control {
    Control::new(30000, Arc::new(AtomicBool::new(false)))
}
fn capture_error(e: impl ToString) -> ProjectError {
    error(INPUT_CHANGED, e.to_string())
}
fn limits() -> ExtractionLimits {
    ExtractionLimits {
        max_units: 64,
        max_input_bytes: 65536,
        max_response_bytes: 1024 * 1024,
        max_output_tokens: 8192,
        timeout_ms: 60000,
    }
}
fn encoded(v: &Value) -> Result<Vec<u8>, ProjectError> {
    let mut bytes = serde_json::to_vec_pretty(v).map_err(|_| invalid())?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn strict(bytes: &[u8]) -> Result<Value, ProjectError> {
    crate::extraction_verify::recover_unambiguous_json(bytes).map_err(|_| invalid())
}

/// Check only the selected path and ancestor marker metadata; never scan a Vault.
/// Missing markers are permitted. Access errors, links and reparse points are errors.
pub fn ensure_outside_vault(path: &Path) -> Result<(), ProjectError> {
    let selected = project::check_components(path)?;
    for ancestor in selected.ancestors() {
        let marker = ancestor.join(".obsidian");
        match fs::symlink_metadata(&marker) {
            Ok(_) => {
                return Err(error(
                    "PREVIEW_EXISTING_VAULT",
                    "选定目录位于已有 Obsidian Vault 内；请选择 Vault 外的新测试目录。",
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(ProjectError::io(&marker, e)),
        }
    }
    Ok(())
}

fn binding(
    project_id: &str,
    import_id: &str,
    v: &SourceVersion,
    parse: Option<(&str, Option<String>)>,
) -> SourceBinding {
    SourceBinding {
        project_id: project_id.into(),
        import_id: import_id.into(),
        source_id: v.source_id.clone(),
        version_id: v.version_id.clone(),
        content_sha256: v.content_sha256.clone(),
        normalized_sha256: if parse.is_some() {
            None
        } else {
            v.normalized_sha256.clone()
        },
        parse_id: parse.as_ref().map(|(id, _)| (*id).into()),
        manifest_sha256: parse.and_then(|(_, sha)| sha),
    }
}

/// Metadata choices retain actual Span ordinals and physical-page region IDs.
pub fn choices(project: &Path, import_id: &str) -> Result<Value, ProjectError> {
    crate::source::invalid_id(import_id)?;
    let root = project::check_components(project)?;
    let (info, store) = crate::source::current_store(&root)?;
    if let Some(report) = store.reports.iter().find(|r| r.import_id == import_id) {
        let Some(v) = &report.version else {
            return Ok(
                json!({"schema_version":1,"binding":null,"source_status":report.status,"filename":Path::new(&report.selected_path).file_name().and_then(|s|s.to_str()).unwrap_or(""),"choices":[]}),
            );
        };
        let available = v.ingest_status == "ready"
            && report.status != "failed"
            && v.normalized_snapshot.is_some()
            && v.normalized_sha256.is_some();
        let c = control();
        let normalized = if available {
            let path = root.join(v.normalized_snapshot.as_ref().ok_or_else(invalid)?);
            let (guard, bytes) = GuardedFile::open(&path, 64 * 1024 * 1024, &c, INPUT_CHANGED)
                .map_err(capture_error)?;
            if Some(sha256(&bytes)) != v.normalized_sha256 {
                return Err(capture_error("来源正文指纹变化。"));
            }
            std::str::from_utf8(&bytes).map_err(|_| invalid())?;
            Some((guard, bytes))
        } else {
            None
        };
        let items: Vec<Value> = store.spans.get(&v.version_id).into_iter().flatten().map(|span| {
            let selectable = available && span.parse_status != crate::text::ParseStatus::Failed && normalized.as_ref().and_then(|(_,bytes)|std::str::from_utf8(bytes).ok()).and_then(|text|text.get(span.start_byte..span.end_byte)).is_some_and(|text|!text.trim().is_empty());
            let mut item = json!({"selection":{"kind":"text","ordinal":span.ordinal},"label":format!("片段 {} · 第 {}–{} 行",span.ordinal+1,span.start_line,span.end_line),"byte_length":span.end_byte.saturating_sub(span.start_byte),"source_status":span.parse_status.as_str(),"selectable":selectable});
            if !selectable {item["reason"] = json!("来源失败、正文为空或没有已确认文本。");} item
        }).collect();
        if let Some((guard, _)) = &normalized {
            guard.verify(&c, INPUT_CHANGED).map_err(capture_error)?;
        }
        return Ok(
            json!({"schema_version":1,"binding":binding(&info.project_id,import_id,v,None),"source_status":report.status,"filename":v.display_name,"choices":items}),
        );
    }
    if let Some(report) = store.pdf.reports.iter().find(|r| r.import_id == import_id) {
        let Some(v) = &report.version else {
            return Ok(
                json!({"schema_version":1,"binding":null,"source_status":report.pdf.status,"filename":Path::new(&report.selected_path).file_name().and_then(|s|s.to_str()).unwrap_or(""),"choices":[]}),
            );
        };
        let parse_id = report.pdf.parse_id.as_deref();
        let attempt = parse_id.and_then(|id| store.pdf.attempts.get(id));
        let available = report.acquisition_status == "acquired"
            && matches!(report.pdf.status.as_str(), "parsed" | "needs_review")
            && attempt.is_some_and(|a| a.manifest_sha256.is_some());
        let c = control();
        let mut resource_bytes: BTreeMap<String, (GuardedFile, Vec<u8>)> = BTreeMap::new();
        let mut items = Vec::new();
        if let Some(id) = parse_id {
            let mut regions: Vec<_> = store
                .pdf
                .regions
                .get(id)
                .into_iter()
                .flat_map(|r| r.values())
                .collect();
            regions.sort_by(|a, b| {
                (a.physical_page_index, &a.region_id).cmp(&(b.physical_page_index, &b.region_id))
            });
            for region in regions {
                let page = store
                    .pdf
                    .pages
                    .get(id)
                    .and_then(|p| p.get(&region.physical_page_index));
                let resource = store
                    .pdf
                    .resources
                    .get(id)
                    .and_then(|r| r.get(&region.text.resource_id));
                let mut nonempty = false;
                if available
                    && let (Some(attempt), Some(resource)) = (attempt, resource)
                    && resource.confidence == "confirmed"
                    && resource.kind == "utf8"
                {
                    if !resource_bytes.contains_key(&resource.resource_id) {
                        let path = root
                            .join(&attempt.snapshot_path)
                            .join(&resource.relative_path);
                        let (guard, bytes) =
                            GuardedFile::open(&path, 96 * 1024 * 1024, &c, INPUT_CHANGED)
                                .map_err(capture_error)?;
                        if sha256(&bytes) != resource.sha256
                            || bytes.len() as u64 != resource.byte_length
                        {
                            return Err(capture_error("PDF区域正文指纹变化。"));
                        }
                        resource_bytes.insert(resource.resource_id.clone(), (guard, bytes));
                    }
                    let (_, bytes) = &resource_bytes[&resource.resource_id];
                    if let (Ok(start), Ok(end)) = (
                        usize::try_from(region.text.start_byte),
                        usize::try_from(region.text.end_byte),
                    ) {
                        nonempty = std::str::from_utf8(bytes)
                            .ok()
                            .and_then(|text| text.get(start..end))
                            .is_some_and(|text| !text.trim().is_empty());
                    }
                }
                let selectable = available
                    && page.is_some_and(|p| {
                        p.status != crate::sidecar_model::ParseStatus::Failed
                            && p.geometry.is_some()
                            && p.text.is_some()
                    })
                    && resource.is_some_and(|r| r.confidence == "confirmed" && r.kind == "utf8")
                    && nonempty;
                let mut item = json!({"selection":{"kind":"pdf","page_index":region.physical_page_index,"region_id":region.region_id},"label":format!("第 {} 页 · 区域 {}",region.physical_page_index+1,region.region_id),"byte_length":region.text.end_byte.saturating_sub(region.text.start_byte),"source_status":page.map(|p|crate::pdf_source_model::status_name(&p.status)).unwrap_or("failed"),"selectable":selectable});
                if !selectable {
                    item["reason"] = json!(
                        "页面或区域没有已确认的 UTF-8 正文；不能把图像或失败页面作为文本请求。"
                    );
                }
                items.push(item);
            }
        }
        for (guard, _) in resource_bytes.values() {
            guard.verify(&c, INPUT_CHANGED).map_err(capture_error)?;
        }
        return Ok(
            json!({"schema_version":1,"binding":binding(&info.project_id,import_id,v,parse_id.map(|id|(id,attempt.and_then(|a|a.manifest_sha256.clone())))),"source_status":report.pdf.status,"filename":v.display_name,"choices":items}),
        );
    }
    Err(error(
        "EXTRACT_SOURCE_UNAVAILABLE",
        "没有找到选定导入的来源记录。",
    ))
}

/// An immutable selected-source capability held across desktop consent and CLI dispatch.
/// The private session denies replacement on Windows and verifies bytes on other hosts.
pub struct SelectedSource {
    session: SourceSession,
}
impl SelectedSource {
    pub fn value(&self) -> Value {
        json!({"binding":self.session.binding,"units":self.session.units})
    }
    pub fn verify(&self) -> Result<(), ProjectError> {
        self.session.verify()
    }
}

/// Acquire before credentials or quota reservation; retain until the child returns.
pub fn hold_selected_units(
    project: &Path,
    import_id: &str,
    selection: &Value,
) -> Result<SelectedSource, ProjectError> {
    crate::source::invalid_id(import_id)?;
    let selection: Vec<Selection> = serde_json::from_value(selection.clone()).map_err(|_| {
        error(
            "EXTRACT_SELECTION",
            "选择须为严格的文本片段或 PDF 物理页区域数组。",
        )
    })?;
    let selected = SelectedSource {
        session: SourceSession::open(project, import_id, &selection, &limits())?,
    };
    selected.verify()?;
    Ok(selected)
}

/// Strict, bounded source preflight. Call before credentials or quota reservation.
pub fn selected_units(
    project: &Path,
    import_id: &str,
    selection: &Value,
) -> Result<Value, ProjectError> {
    let selected = hold_selected_units(project, import_id, selection)?;
    selected.verify()?;
    Ok(selected.value())
}

/// CLI selection files have duplicate-key, size and identity protection too.
pub fn selected_units_from_file(
    project: &Path,
    import_id: &str,
    path: &Path,
) -> Result<Value, ProjectError> {
    let c = control();
    let (guard, bytes) =
        GuardedFile::open(path, 65536, &c, INPUT_CHANGED).map_err(capture_error)?;
    let selection = strict(&bytes)?;
    let result = selected_units(project, import_id, &selection)?;
    guard.verify(&c, INPUT_CHANGED).map_err(capture_error)?;
    Ok(result)
}

struct SavedApi {
    root: PathBuf,
    directory: Directory,
    files: BTreeMap<String, (GuardedFile, Vec<u8>)>,
    names: BTreeSet<std::ffi::OsString>,
}
impl SavedApi {
    fn open(path: &Path, c: &Control) -> Result<Self, ProjectError> {
        let root = project::check_components(path)?;
        let directory = Directory::capture(&root, INPUT_CHANGED).map_err(capture_error)?;
        let names = fs::read_dir(&root)
            .map_err(|e| ProjectError::io(&root, e))?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<Result<BTreeSet<_>, _>>()
            .map_err(|e| ProjectError::io(&root, e))?;
        let required = BTreeSet::from(
            [
                ".owner.json",
                "request.json",
                "request-body.json",
                "response.bin",
                "candidates.json",
                "validation.json",
                "validation.md",
            ]
            .map(std::ffi::OsString::from),
        );
        // A successful durable request can also retain its transport receipt.
        let optional = std::ffi::OsString::from("transport-receipt.json");
        if !required.is_subset(&names)
            || names
                .iter()
                .any(|n| !required.contains(n) && n != &optional)
        {
            return Err(invalid());
        }
        let mut files = BTreeMap::new();
        for name in &names {
            let key = name.to_str().ok_or_else(invalid)?.to_string();
            let (guard, bytes) =
                GuardedFile::open(&root.join(name), API_FILE_BYTES, c, INPUT_CHANGED)
                    .map_err(capture_error)?;
            files.insert(key, (guard, bytes));
        }
        let value = Self {
            root,
            directory,
            files,
            names,
        };
        value.verify(c)?;
        Ok(value)
    }
    fn bytes(&self, name: &str) -> Result<&[u8], ProjectError> {
        self.files
            .get(name)
            .map(|(_, b)| b.as_slice())
            .ok_or_else(invalid)
    }
    fn json(&self, name: &str) -> Result<Value, ProjectError> {
        strict(self.bytes(name)?)
    }
    fn verify(&self, c: &Control) -> Result<(), ProjectError> {
        self.directory
            .verify(INPUT_CHANGED)
            .map_err(capture_error)?;
        let names = fs::read_dir(&self.root)
            .map_err(capture_error)?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<Result<BTreeSet<_>, _>>()
            .map_err(capture_error)?;
        if names != self.names {
            return Err(capture_error("保存结果目录的成员发生变化。"));
        }
        for (guard, _) in self.files.values() {
            guard.verify(c, INPUT_CHANGED).map_err(capture_error)?;
        }
        Ok(())
    }
    fn validate(
        &self,
        source: &SourceSession,
        request: &Value,
        request_units: &[RequestUnit],
    ) -> Result<ApiCandidates, ProjectError> {
        let saved: SavedCandidate =
            serde_json::from_value(self.json("candidates.json")?).map_err(|_| invalid())?;
        let report = self.json("validation.json")?;
        let owner = self.json(".owner.json")?;
        let id = request["request_id"].as_str().ok_or_else(invalid)?;
        crate::source::invalid_id(id).map_err(|_| invalid())?;
        let request_binding: SourceBinding =
            serde_json::from_value(request["binding"].clone()).map_err(|_| invalid())?;
        let transport = match saved.transport_mode {
            crate::extraction_review_model::Transport::Live => "live",
            crate::extraction_review_model::Transport::LocalHttpTest => "local_http_test",
        };
        let candidate = saved.payload;
        if request["schema_version"] != 1
            || request["producer"] != "api"
            || request["transport_mode"] != transport
            || owner
                != json!({"schema_version":1,"producer":"api","request_id":id,"purpose":"unpublished_extraction_test","transport_mode":transport})
            || saved.request_id != id
            || candidate.request_id != id
            || saved.binding != source.binding
            || candidate.binding != source.binding
            || request_binding != source.binding
            || request_units != source.units
            || report["schema_version"] != 1
            || report["report_kind"] != "text_api_extraction"
            || report["producer"] != "api"
            || report["status"] != "candidates_validated"
            || report["knowledge_status"] != "pending"
            || report["published"] != false
            || report["scientific_quality"] != "not_run"
            || report["errors"] != json!([])
            || report["transport_mode"] != transport
            || report["request_id"] != id
            || report["binding"] != request["binding"]
            || report["validated_atoms"] != candidate.atoms.len()
            || report["http_status"] != 200
            || report["response_complete"] != true
            || report["source_statuses"]
                != json!(
                    source
                        .units
                        .iter()
                        .map(|u| &u.source_status)
                        .collect::<Vec<_>>()
                )
        {
            return Err(invalid());
        }
        for field in [
            "provider",
            "endpoint",
            "request_strategy",
            "configured_model",
        ] {
            if request.get(field).is_none() || report[field] != request[field] {
                return Err(invalid());
            }
        }
        let md = std::str::from_utf8(self.bytes("validation.md")?).map_err(|_| invalid())?;
        let embedded = md
            .rsplit_once("```json\n")
            .and_then(|(_, j)| j.strip_suffix("\n```\n"))
            .ok_or_else(invalid)?;
        if strict(embedded.as_bytes())? != report {
            return Err(invalid());
        }
        let artifacts = report["artifacts"].as_object().ok_or_else(invalid)?;
        let expected: BTreeSet<_> = self
            .files
            .keys()
            .filter(|n| n.as_str() != "validation.json" && n.as_str() != "validation.md")
            .cloned()
            .collect();
        if artifacts.keys().cloned().collect::<BTreeSet<_>>() != expected {
            return Err(invalid());
        }
        for (name, pin) in artifacts {
            let bytes = self.bytes(name)?;
            if *pin != json!({"sha256":sha256(bytes),"byte_length":bytes.len()}) {
                return Err(invalid());
            }
        }
        let saved_limits: ExtractionLimits =
            serde_json::from_value(request["limits"].clone()).map_err(|_| invalid())?;
        if !(1..=64).contains(&saved_limits.max_units)
            || source.units.len() > saved_limits.max_units
            || !(1..=65536).contains(&saved_limits.max_input_bytes)
            || source
                .units
                .iter()
                .map(|u| u.text.len() as u64)
                .sum::<u64>()
                > saved_limits.max_input_bytes
            || !(1..=1048576).contains(&saved_limits.max_response_bytes)
            || !(16..=8192).contains(&saved_limits.max_output_tokens)
            || !(1..=60000).contains(&saved_limits.timeout_ms)
        {
            return Err(invalid());
        }
        let body = self.bytes("request-body.json")?;
        if request["request_body"] != json!({"sha256":sha256(body),"byte_length":body.len()}) {
            return Err(invalid());
        }
        let body_value = strict(body)?;
        let input = if let Some(input) = body_value["input"].as_str() {
            strict(input.as_bytes())?
        } else {
            let content = body_value["messages"]
                .as_array()
                .and_then(|messages| messages.iter().find(|m| m["role"] == "user"))
                .and_then(|m| m["content"].as_str())
                .ok_or_else(invalid)?;
            strict(content.as_bytes())?
        };
        if input["request_id"] != id
            || input["binding"] != request["binding"]
            || input["units"] != json!(source.units)
            || body_value["model"] != request["configured_model"]
        {
            return Err(invalid());
        }
        let response = self.bytes("response.bin")?;
        if response.len() > limits().max_response_bytes as usize
            || report["response_receipt"]
                != json!({"sha256":sha256(response),"byte_length":response.len(),"omission_reason":null})
        {
            return Err(invalid());
        }
        let strategy = [
            RequestStrategy::OpenAiStrict,
            RequestStrategy::TangmaoDeepseek,
            RequestStrategy::TangmaoChat,
        ]
        .into_iter()
        .find(|s| request["request_strategy"] == s.version())
        .ok_or_else(invalid)?;
        let expected_body = crate::extraction_api_schema::request_body(
            strategy,
            request["configured_model"].as_str().ok_or_else(invalid)?,
            id,
            &source.binding,
            &source.units,
            saved_limits.max_output_tokens,
        )?;
        if strict(&expected_body)? != body_value
            || response.len() as u64 > saved_limits.max_response_bytes
        {
            return Err(invalid());
        }
        let parsed =
            extraction_api_wire::decode_with_strategy(response, strategy).map_err(|_| invalid())?;
        if serde_json::to_value(&parsed.candidates).map_err(|_| invalid())?
            != serde_json::to_value(&candidate).map_err(|_| invalid())?
            || report["actual_model"] != parsed.model
            || report["response_id"] != parsed.id
            || report["service_tier"] != json!(parsed.service_tier)
            || report["usage"]["model_usage"] != parsed.usage
            || report["usage"]["model_usage_status"] != "validated"
        {
            return Err(invalid());
        }
        Ok(candidate)
    }
}

fn selection(units: &[RequestUnit]) -> Vec<Selection> {
    units
        .iter()
        .map(|u| match &u.locator {
            Locator::Text { ordinal, .. } => Selection::Text { ordinal: *ordinal },
            Locator::Pdf {
                page_index,
                region_id,
                ..
            } => Selection::Pdf {
                page_index: *page_index,
                region_id: region_id.clone(),
            },
        })
        .collect()
}
fn safe_text(text: &str) -> String {
    let mut escaped = String::new();
    let mut leading = true;
    for ch in text.chars() {
        match ch {
            '\n' | '\r' => {
                escaped.push(ch);
                leading = true;
                continue;
            }
            ' ' if leading => {
                escaped.push_str("&#32;");
                continue;
            }
            '\t' if leading => {
                escaped.push_str("&#9;");
                continue;
            }
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '[' => escaped.push_str("&#91;"),
            ']' => escaped.push_str("&#93;"),
            '|' => escaped.push_str("&#124;"),
            '*' | '_' | '`' | '#' | '\\' | '$' | '%' | '~' | '=' | '-' | '+' | '.' | '!' | '('
            | ')' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            _ => escaped.push(ch),
        }
        leading = false;
    }
    escaped
}
fn safe_inline(text: &str) -> String {
    safe_text(
        &text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\n', " ↵ ")
            .replace('\t', " ⇥ "),
    )
}
fn quoted(text: &str) -> String {
    text.lines()
        .map(|line| format!("> {}\n", safe_text(line)))
        .collect()
}
fn provenance(local: bool) -> &'static str {
    if local {
        "状态：pending（待核验）。本地 HTTP 假服务结果，仅用于测试链路。verification：unverified；required：false；published：false；scientific_quality：not_run。\n\n"
    } else {
        "状态：pending（待核验）。模型候选尚未科学核验。verification：unverified；required：false；published：false；scientific_quality：not_run。\n\n"
    }
}
fn locator(unit: &RequestUnit, attachment: &str) -> String {
    match &unit.locator {
        Locator::Text {
            span_id,
            ordinal,
            resource_sha256,
        } => format!(
            "来源：[[attachments/{attachment}]]；片段 {}（span_id：{}）；UTF-8 半开字节范围 [{}, {})；正文 SHA-256：{}；source_status：{}。\n\n",
            ordinal, span_id, unit.start_byte, unit.end_byte, resource_sha256, unit.source_status
        ),
        Locator::Pdf {
            parse_id,
            page_index,
            region_id,
            resource_id,
            resource_sha256,
            bbox,
        } => format!(
            "来源：[[attachments/{attachment}#page={}]]；物理页 {}（page_index：{}）；区域 {}；bbox：{:?}；UTF-8 半开字节范围 [{}, {})；parse_id：{}；resource_id：{}；正文 SHA-256：{}；source_status：{}。\n\n",
            page_index + 1,
            page_index + 1,
            page_index,
            safe_text(region_id),
            bbox,
            unit.start_byte,
            unit.end_byte,
            parse_id,
            safe_text(resource_id),
            resource_sha256,
            unit.source_status
        ),
    }
}

/// Reopen and independently validate a successful saved API run, then export.
/// The destination must be a brand-new directory outside existing Vaults.
pub fn export(
    project: &Path,
    import_id: &str,
    api_root: &Path,
    destination: &Path,
) -> Result<Value, ProjectError> {
    crate::source::invalid_id(import_id)?;
    ensure_outside_vault(destination)?;
    let destination = project::check_components(destination)?;
    if project::exists(&destination)? {
        return Err(error(
            "EXTRACT_OUTPUT_EXISTS",
            "输出根已存在；请选择新的测试目录。",
        ));
    }
    let c = control();
    let api = SavedApi::open(api_root, &c)?;
    let request = api.json("request.json")?;
    let request_units: Vec<RequestUnit> =
        serde_json::from_value(request["units"].clone()).map_err(|_| invalid())?;
    let source = SourceSession::open(project, import_id, &selection(&request_units), &limits())?;
    if inside(&destination, source.root())? || inside(&destination, &api.root)? {
        return Err(error(
            "PREVIEW_OUTPUT_OVERLAP",
            "输出根必须位于来源项目和 API 审计根之外。",
        ));
    }
    let candidate = api.validate(&source, &request, &request_units)?;
    crate::extraction::validate_candidates(
        &source.units,
        &candidate.concepts,
        &candidate.evidence,
        &candidate.atoms,
        &c,
        &AtomicBool::new(false),
        None,
    )?;
    let (_, store) = crate::source::current_store(source.root())?;
    let v = store
        .versions
        .values()
        .find(|v| v.version_id == source.binding.version_id)
        .ok_or_else(invalid)?;
    let extension = match v.media_type.as_str() {
        "application/pdf" => "pdf",
        "text/markdown" => "md",
        "text/plain" => "txt",
        _ => {
            return Err(error(
                "PREVIEW_SOURCE_TYPE",
                "只导出已登记的 PDF、Markdown 或 TXT 原件。",
            ));
        }
    };
    let attachment = format!("source-0001.{extension}");
    let raw = &v.raw_snapshot;
    let (raw_guard, raw_bytes) = GuardedFile::open(
        &source.root().join(raw),
        if extension == "pdf" {
            16 * 1024 * 1024
        } else {
            64 * 1024 * 1024
        },
        &c,
        INPUT_CHANGED,
    )
    .map_err(capture_error)?;
    if raw_bytes.len() as u64 != v.byte_length
        || sha256(&raw_bytes) != source.binding.content_sha256
    {
        return Err(invalid());
    }
    source.verify()?;
    api.verify(&c)?;
    // Check ancestor markers again immediately before creating our own root.
    ensure_outside_vault(&destination)?;
    let mut output = OutputRoot::create_preview(&destination, &candidate.request_id)?;
    let result = (|| {
        let local = request["transport_mode"] == "local_http_test";
        let concepts: BTreeMap<_, _> = candidate
            .concepts
            .iter()
            .enumerate()
            .map(|(i, concept)| {
                (
                    concept.concept_id.as_str(),
                    format!("concept-{:04}.md", i + 1),
                )
            })
            .collect();
        let evidence: BTreeMap<_, _> = candidate
            .evidence
            .iter()
            .enumerate()
            .map(|(i, e)| (e.evidence_id.as_str(), format!("evidence-{:04}.md", i + 1)))
            .collect();
        let mut overview = format!(
            "# 待核验模型笔记\n\n{}来源文件：{}。\n\nrequest_id：{}。\n\n[[attachments/{attachment}|查看原件]]\n\n## 概念索引\n\n",
            provenance(local),
            safe_text(&v.display_name),
            candidate.request_id
        );
        for concept in &candidate.concepts {
            let filename = &concepts[concept.concept_id.as_str()];
            overview.push_str(&format!(
                "- [[{}|{}]]\n",
                filename.as_str(),
                safe_inline(&concept.canonical_name)
            ));
            let mut note = format!(
                "# {}\n\n{}concept_id：{}；revision：{}。\n\n",
                safe_inline(&concept.canonical_name),
                provenance(local),
                concept.concept_id,
                concept.revision
            );
            if !concept.aliases.is_empty() {
                note.push_str(&format!(
                    "别名：{}。\n\n",
                    concept
                        .aliases
                        .iter()
                        .map(|a| safe_inline(a))
                        .collect::<Vec<_>>()
                        .join("、")
                ));
            }
            for item in candidate
                .atoms
                .iter()
                .filter(|a| a.atom.concept_ids.contains(&concept.concept_id))
            {
                let atom = &item.atom;
                note.push_str(&format!(
                    "## 候选 {}\n\n{}\n\nkind：{}；revision：{}；value_status：{}。\n\n",
                    atom.atom_id,
                    safe_text(&atom.content),
                    serde_json::to_value(&atom.kind)
                        .map_err(|_| invalid())?
                        .as_str()
                        .unwrap_or(""),
                    atom.revision,
                    serde_json::to_value(&atom.value_status)
                        .map_err(|_| invalid())?
                        .as_str()
                        .unwrap_or("")
                ));
                if !atom.qualifiers.is_empty() {
                    note.push_str("条件 / 限定：\n\n");
                    for qualifier in &atom.qualifiers {
                        note.push_str(&format!("- {}\n", safe_text(qualifier)));
                    }
                    note.push('\n');
                }
                if let Some(measurement) = &item.measurement {
                    note.push_str(&format!(
                        "数值状态：{}；数值：{}；单位：{}。\n\n",
                        serde_json::to_value(&measurement.value_status)
                            .map_err(|_| invalid())?
                            .as_str()
                            .unwrap_or(""),
                        measurement
                            .value
                            .as_deref()
                            .map(safe_text)
                            .unwrap_or_else(|| "未知 / 不适用（null）".into()),
                        measurement
                            .unit
                            .as_deref()
                            .map(safe_text)
                            .unwrap_or_else(|| "未知 / 不适用（null）".into())
                    ));
                    for condition in &measurement.conditions {
                        note.push_str(&format!("- 测量条件：{}\n", safe_text(condition)));
                    }
                    note.push('\n');
                }
                note.push_str("证据：\n\n");
                for link in &atom.evidence_links {
                    let filename = &evidence[link.evidence_id.as_str()];
                    note.push_str(&format!(
                        "- [[{}]] · relation：{}\n",
                        filename.as_str(),
                        serde_json::to_value(&link.relation)
                            .map_err(|_| invalid())?
                            .as_str()
                            .unwrap_or("")
                    ));
                }
                note.push('\n');
            }
            output.add(&format!("vault/{filename}"), note.as_bytes())?;
        }
        for e in &candidate.evidence {
            let unit = source
                .units
                .iter()
                .find(|u| u.unit_id == e.unit_id)
                .ok_or_else(invalid)?;
            let note = format!(
                "# 证据 {}\n\n{}{}摘录：UTF-8 半开字节范围 [{}, {})；excerpt_sha256：{}。\n\n{}\n## 所选来源单位全文\n\n{}\n",
                e.evidence_id,
                provenance(local),
                locator(unit, &attachment),
                e.excerpt_start_byte,
                e.excerpt_end_byte,
                e.excerpt_sha256,
                quoted(&e.quote),
                quoted(&unit.text)
            );
            output.add(
                &format!("vault/{}", evidence[e.evidence_id.as_str()]),
                note.as_bytes(),
            )?;
        }
        overview.push_str("\n## 来源单位全文\n\n");
        for unit in &source.units {
            overview.push_str(&format!(
                "### {}\n\n{}{}\n",
                unit.unit_id,
                locator(unit, &attachment),
                quoted(&unit.text)
            ));
        }
        output.add_preview_attachment(&format!("vault/attachments/{attachment}"), &raw_bytes)?;
        output.add("vault/待核验笔记.md", overview.as_bytes())?;
        for (name, (_, bytes)) in &api.files {
            if name != ".owner.json" {
                output.add(&format!("reports/{name}"), bytes)?;
            }
        }
        let value = json!({"producer":"extraction_preview","status":"pending","output_root":destination,"vault_path":destination.join("vault"),"counts":{"concepts":candidate.concepts.len(),"atoms":candidate.atoms.len(),"evidence":candidate.evidence.len()},"markdown":overview});
        let audit = json!({"schema_version":1,"producer":"extraction_preview","status":"pending","binding":source.binding,"request_id":candidate.request_id,"transport_mode":request["transport_mode"],"published":false,"scientific_quality":"not_run","counts":value["counts"],"units":source.units,"files":output.fingerprints(),"preview":value});
        output.add("reports/preview.json", &encoded(&audit)?)?;
        source.verify()?;
        api.verify(&c)?;
        raw_guard.verify(&c, INPUT_CHANGED).map_err(capture_error)?;
        output.verify()?;
        ensure_outside_vault(&destination)?;
        Ok(value)
    })();
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            let _ = output.discard_preview_result();
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arbitrary_source_text_cannot_create_active_markdown_links_or_html() {
        let text = "[[outside/path]] <script>bad</script> # title `code`";
        let rendered = quoted(text);
        assert!(!rendered.contains("[["));
        assert!(!rendered.contains("<script>"));
        assert!(rendered.contains("&#91;&#91;outside/path&#93;&#93;"));
        assert!(rendered.contains("&lt;script&gt;"));
    }
    #[test]
    fn source_comments_fences_math_lists_and_indentation_remain_literal() {
        let rendered = safe_text(
            "%%hidden%%\n~~~\n$math$ ~~strike~~ ==mark==\n---\n- item\n1. item\n    indented\n\ttab",
        );
        for token in [
            "\\%\\%hidden\\%\\%",
            "\\~\\~\\~",
            "\\$math\\$",
            "\\~\\~strike\\~\\~",
            "\\=\\=mark\\=\\=",
            "\\-\\-\\-",
            "\\- item",
            "1\\. item",
            "&#32;&#32;&#32;&#32;indented",
            "&#9;tab",
        ] {
            assert!(rendered.contains(token), "{token}: {rendered}");
        }
        assert!(!rendered.contains("%%"));
        assert!(!rendered.contains("~~~"));
    }
    #[test]
    fn multiline_titles_and_wiki_aliases_cannot_break_the_generated_line() {
        let label = safe_inline("first\r\nsecond\n\t[[outside]]");
        assert!(!label.contains(['\n', '\r', '\t']));
        assert!(!label.contains("[["));
        assert_eq!(label, "first ↵ second ↵  ⇥ &#91;&#91;outside&#93;&#93;");
    }
    #[test]
    fn pdf_locator_keeps_real_page_region_bbox_and_resource_offsets() {
        let unit = RequestUnit {
            unit_id: "unit-0".into(),
            locator: Locator::Pdf {
                parse_id: "parse".into(),
                page_index: 6,
                region_id: "actual-line".into(),
                resource_id: "page-text".into(),
                resource_sha256: "sha".into(),
                bbox: [10., 20., 30., 40.],
            },
            source_status: "needs_review".into(),
            start_byte: 123,
            end_byte: 456,
            text: "actual selected text".into(),
        };
        let rendered = locator(&unit, "source-0001.pdf");
        for text in [
            "attachments/source-0001.pdf#page=7",
            "page_index：6",
            "actual\\-line",
            "[10.0, 20.0, 30.0, 40.0]",
            "[123, 456)",
            "needs_review",
            "page\\-text",
        ] {
            assert!(rendered.contains(text), "{text}: {rendered}");
        }
    }
}
