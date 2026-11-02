//! One selected PDF -> an exclusive parse sandbox; project schemas are untouched.
use crate::{
    pdf_model::{MANIFEST_ID, Manifest, Report},
    pdf_validate::validate_manifest,
    pdf_workspace::Workspace,
    project,
    sidecar_model::{Limits, ParseStatus, SidecarError},
    sidecar_process::{self, Control, ParserCommand, ValidatedResponse},
};
use std::{
    ffi::OsString,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};

pub struct PdfOutput {
    pub json: String,
    pub failure: Option<(String, String)>,
}
// Generated from readable adapter.py and checked by build_pdf_sidecar.py.
// Windows CreateProcess has a 32,767-character command line bound; the trusted
// compressed payload keeps native argv short without temporary driver files.
const ADAPTER_PAYLOAD: &[u8] = include_bytes!("../../../sidecars/pdf_parser/adapter.zlib");

fn configured_runtime() -> Result<std::path::PathBuf, SidecarError> {
    std::env::var_os("NOETOME_PDF_PYTHON").filter(|value|!value.is_empty()).map(Into::into)
        .ok_or_else(||SidecarError::new("PDF_RUNTIME","set NOETOME_PDF_PYTHON to the absolute locked Python interpreter; see sidecars/pdf_parser/README.md"))
}
fn command(runtime: &Path) -> Result<ParserCommand, SidecarError> {
    if !runtime.is_absolute() {
        return Err(SidecarError::new(
            "PDF_RUNTIME",
            "Python interpreter path must be absolute",
        ));
    }
    let runtime =
        project::check_components(runtime).map_err(|e| SidecarError::new("PDF_RUNTIME", e))?;
    project::checked_file(&runtime).map_err(|e| SidecarError::new("PDF_RUNTIME", e))?;
    let hex: String = ADAPTER_PAYLOAD
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let code = format!(
        "import zlib;exec(compile(zlib.decompress(bytes.fromhex('{hex}')),'<noetome-pdf-sidecar-v1>','exec'))"
    );
    let mut args: Vec<OsString> = ["-I", "-B", "-X", "utf8", "-c"]
        .into_iter()
        .map(Into::into)
        .collect();
    args.push(code.into());
    Ok(ParserCommand {
        program: runtime.into_os_string(),
        args,
    })
}
pub fn parse(input: &Path, output: &Path) -> Result<PdfOutput, SidecarError> {
    parse_with(
        input,
        output,
        &configured_runtime()?,
        Limits::default(),
        Arc::new(AtomicBool::new(false)),
    )
}
/// Trusted callers may select an interpreter and a tighter policy/cancellation.
/// Documents cannot supply any of these values. No fake/parser switch exists.
pub fn parse_with(
    input: &Path,
    output: &Path,
    runtime: &Path,
    limits: Limits,
    cancelled: Arc<AtomicBool>,
) -> Result<PdfOutput, SidecarError> {
    parse_command(input, output, &command(runtime)?, limits, cancelled)
}
fn parse_command(
    input: &Path,
    output: &Path,
    command: &ParserCommand,
    limits: Limits,
    cancelled: Arc<AtomicBool>,
) -> Result<PdfOutput, SidecarError> {
    if [
        limits.request_bytes,
        limits.response_bytes,
        limits.diagnostic_bytes,
        limits.input_bytes,
        limits.resource_bytes,
        limits.total_resource_bytes,
        limits.timeout_ms,
    ]
    .contains(&0)
        || [limits.resources, limits.units, limits.issues].contains(&0)
    {
        return Err(SidecarError::new(
            "SIDECAR_CONFIG",
            "PDF limits must be positive",
        ));
    }
    let start = Instant::now();
    let control = Control::new(limits.timeout_ms, Arc::clone(&cancelled));
    control.check()?;
    let workspace = Workspace::create(output, &control)?;
    let result = (|| {
        let remaining = limits
            .timeout_ms
            .saturating_sub(start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        control.check()?;
        let child_limits = Limits {
            timeout_ms: remaining.max(1),
            ..limits.clone()
        };
        let validated = sidecar_process::run(
            command,
            input,
            &workspace.root.join("parser"),
            child_limits,
            cancelled,
        )?;
        control.check()?;
        let manifest = load_manifest(&validated, &control)?;
        let report = report(
            &manifest,
            &validated,
            start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        )?;
        let json = serde_json::to_string_pretty(&report)
            .map_err(|e| SidecarError::new("PDF_REPORT", e))?
            + "\n";
        workspace.write("parse-result.json", json.as_bytes(), &control)?;
        workspace.write("parse-result.md", markdown(&report).as_bytes(), &control)?;
        workspace.complete_layout(&control)?;
        let (json_guard, json_bytes) = workspace.read("parse-result.json", &control)?;
        let (md_guard, md_bytes) = workspace.read("parse-result.md", &control)?;
        let saved: Report =
            serde_json::from_slice(&json_bytes).map_err(|e| SidecarError::new("PDF_REPORT", e))?;
        verify_report(&saved, &manifest, &validated, &md_bytes)?;
        json_guard.verify(&control, &"PDF_REPORT")?;
        md_guard.verify(&control, &"PDF_REPORT")?;
        validated.recheck(&control)?;
        workspace.complete_layout(&control)?;
        control.check()?;
        Ok(output_report(json, &saved))
    })();
    result.map_err(|mut error: SidecarError| {
        // A failed/expired operation has no trusted full PDF result. Keep a
        // bounded diagnostic snapshot using a separate small cleanup deadline.
        let diagnostic_control = Control::new(5000, Arc::new(AtomicBool::new(false)));
        if let Err(persist) = workspace.write(
            "failure.json",
            failure_json(&error).as_bytes(),
            &diagnostic_control,
        ) {
            error
                .message
                .push_str(&format!("; failure report not persisted: {persist}"));
        }
        error
    })
}

pub fn inspect(output: &Path) -> Result<PdfOutput, SidecarError> {
    let control = Control::new(30000, Arc::new(AtomicBool::new(false)));
    let workspace = Workspace::inspect(output, &control)?;
    workspace.complete_layout(&control)?;
    let (json_guard, bytes) = workspace.read("parse-result.json", &control)?;
    let (md_guard, md_bytes) = workspace.read("parse-result.md", &control)?;
    let report: Report =
        serde_json::from_slice(&bytes).map_err(|e| SidecarError::new("PDF_REPORT", e))?;
    control.check()?;
    let validated = sidecar_process::reopen_pdf_resources(
        report.sidecar.clone(),
        &workspace.root.join("parser"),
        Limits::default(),
        &control,
    )?;
    // Semantic checks precede report wording/summary so stale Markdown cannot
    // disguise a missing body or an incorrect native locator.
    let manifest = load_manifest(&validated, &control)?;
    verify_report(&report, &manifest, &validated, &md_bytes)?;
    json_guard.verify(&control, &"PDF_REPORT")?;
    md_guard.verify(&control, &"PDF_REPORT")?;
    validated.recheck(&control)?;
    workspace.complete_layout(&control)?;
    control.check()?;
    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| SidecarError::new("PDF_REPORT", e))?
        + "\n";
    Ok(output_report(json, &report))
}
fn load_manifest(
    validated: &ValidatedResponse,
    control: &Control,
) -> Result<Manifest, SidecarError> {
    control.check()?;
    let text = validated
        .resource_data()
        .get(MANIFEST_ID)
        .and_then(crate::sidecar_workspace::ResourceData::text)
        .ok_or_else(|| SidecarError::new("PDF_MANIFEST", "missing typed PDF manifest resource"))?;
    let manifest: Manifest =
        serde_json::from_str(text).map_err(|e| SidecarError::new("PDF_MANIFEST", e))?;
    control.check()?;
    validate_manifest(
        &manifest,
        &validated.response,
        validated.resource_data(),
        control,
    )?;
    Ok(manifest)
}
fn report(
    manifest: &Manifest,
    validated: &ValidatedResponse,
    elapsed_ms: u64,
) -> Result<Report, SidecarError> {
    let manifest_sha = validated
        .response
        .resources
        .iter()
        .find(|r| r.resource_id == MANIFEST_ID)
        .ok_or_else(|| SidecarError::new("PDF_MANIFEST", "manifest not fingerprinted"))?
        .fingerprint
        .sha256
        .clone();
    Ok(Report {
        schema_version: 1,
        report_kind: "pdf_parse_sandbox".into(),
        input_fingerprint: manifest.input_fingerprint.clone(),
        parser: manifest.parser.clone(),
        status: manifest.status.clone(),
        page_count: manifest.page_count,
        parsed_page_count: manifest
            .pages
            .iter()
            .filter(|p| p.status == ParseStatus::Parsed)
            .count() as u32,
        needs_review_page_count: manifest
            .pages
            .iter()
            .filter(|p| p.status == ParseStatus::NeedsReview)
            .count() as u32,
        failed_page_count: manifest
            .pages
            .iter()
            .filter(|p| p.status == ParseStatus::Failed)
            .count() as u32,
        manifest_path: "parser/pdf-manifest.json".into(),
        manifest_sha256: manifest_sha,
        elapsed_ms,
        resource_bytes: validated
            .response
            .resources
            .iter()
            .map(|r| r.fingerprint.byte_length)
            .sum(),
        sidecar: validated.response.clone(),
    })
}
fn verify_report(
    saved: &Report,
    manifest: &Manifest,
    validated: &ValidatedResponse,
    md_bytes: &[u8],
) -> Result<(), SidecarError> {
    let recomputed = report(manifest, validated, saved.elapsed_ms)?;
    if serde_json::to_value(saved).map_err(|e| SidecarError::new("PDF_REPORT", e))?
        != serde_json::to_value(&recomputed).map_err(|e| SidecarError::new("PDF_REPORT", e))?
    {
        return Err(SidecarError::new(
            "PDF_REPORT",
            "persisted report counts/identity/resources differ from verified PDF",
        ));
    }
    if md_bytes != markdown(saved).as_bytes() {
        return Err(SidecarError::new(
            "PDF_REPORT",
            "JSON/Markdown PDF report facts differ",
        ));
    }
    Ok(())
}
fn status(status: &ParseStatus) -> &'static str {
    match status {
        ParseStatus::Parsed => "parsed",
        ParseStatus::Failed => "failed",
        ParseStatus::NeedsReview => "needs_review",
    }
}
fn cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\r', " ")
        .replace('\n', " ")
}
fn markdown(report: &Report) -> String {
    let mut md = format!(
        "# PDF 单文件解析沙盒\n\n| 字段 | 值 |\n| --- | --- |\n| schema_version | {} |\n| report_kind | {} |\n| status | {} |\n| input_sha256 | {} |\n| input_byte_length | {} |\n| parser | {} {} / {} {} / {} / {} / Python {} |\n| page_count | {} |\n| parsed_page_count | {} |\n| needs_review_page_count | {} |\n| failed_page_count | {} |\n| manifest_path | {} |\n| manifest_sha256 | {} |\n| elapsed_ms | {} |\n| resource_bytes | {} |\n\n物理索引从0开始；显示物理页为索引+1；印刷页码未确认。知识抽取、OCR与科学验证未运行。\n\n| 物理索引 | 显示物理页 | 状态 |\n| --- | --- | --- |\n",
        report.schema_version,
        report.report_kind,
        status(&report.status),
        report.input_fingerprint.sha256,
        report.input_fingerprint.byte_length,
        report.parser.engine,
        report.parser.engine_version,
        report.parser.engine_dependency,
        report.parser.engine_dependency_version,
        report.parser.adapter_version,
        report.parser.options_version,
        report.parser.python_version,
        report
            .page_count
            .map_or_else(|| "null".into(), |n| n.to_string()),
        report.parsed_page_count,
        report.needs_review_page_count,
        report.failed_page_count,
        report.manifest_path,
        report.manifest_sha256,
        report.elapsed_ms,
        report.resource_bytes
    );
    for unit in &report.sidecar.units {
        if unit.unit_id != "pdf-document" {
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                unit.ordinal,
                unit.ordinal + 1,
                status(&unit.status)
            ));
        }
    }
    md.push_str("\n| 问题 | 来源单元 | 原因 |\n| --- | --- | --- |\n");
    for issue in &report.sidecar.issues {
        md.push_str(&format!(
            "| {} | {} | {} |\n",
            cell(&issue.code),
            cell(&issue.unit_ids.join(",")),
            cell(&issue.message)
        ));
    }
    md
}
fn output_report(json: String, report: &Report) -> PdfOutput {
    let failure = if report.status == ParseStatus::Parsed {
        None
    } else {
        Some((
            "PDF_INCOMPLETE".into(),
            format!(
                "PDF status {}: {} parsed, {} needs_review, {} failed pages; confirmed total {}. See manifest/report for reasons.",
                status(&report.status),
                report.parsed_page_count,
                report.needs_review_page_count,
                report.failed_page_count,
                report
                    .page_count
                    .map_or_else(|| "unknown".into(), |n| n.to_string())
            ),
        ))
    };
    PdfOutput { json, failure }
}
pub fn failure_json(error: &SidecarError) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "schema_version":1,"report_kind":"pdf_parse_failure","status":"failed",
        "raw_acquisition":"not_confirmed","input_fingerprint":null,"page_count":null,"confirmed_pages":[],
        "error":{"code":error.code,"message":error.message,"diagnostics":String::from_utf8_lossy(&error.diagnostics)}
    })).unwrap()+"\n"
}

#[cfg(test)]
#[path = "../tests/support/pdf_parse_tests.rs"]
mod tests;
