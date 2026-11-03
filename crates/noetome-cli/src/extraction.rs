//! Bounded fake extraction: validate candidates without publishing knowledge.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, atomic::AtomicBool};

use serde::{Deserialize, Serialize};

use crate::extraction_fake::{self, FakeInput};
use crate::extraction_model::{
    CandidateAtom, CandidateEvidence, ExtractionRequest, Producer, RequestUnit, ResponseEnvelope,
    SourceBinding,
};
use crate::extraction_output::{self, OutputRoot};
use crate::extraction_source::SourceSession;
use crate::fixture_model::{AtomKind, EvidenceRelation, ValueStatus, Verification};
use crate::project::{self, ProjectError};
use crate::sidecar_model::Fingerprint;
use crate::sidecar_process::Control;
use crate::text::sha256;

fn error(code: &'static str, message: &str) -> ProjectError {
    ProjectError::new(code, 3, message)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Failure {
    code: String,
    message: String,
}
impl From<&ProjectError> for Failure {
    fn from(error: &ProjectError) -> Self {
        Self {
            code: error.code().into(),
            message: error.to_string().chars().take(2048).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Usage {
    meter: String,
    input_tokens: u64,
    input_bytes: u64,
    #[serde(deserialize_with = "crate::pdf_model::nullable")]
    output_tokens: Option<u64>,
    #[serde(deserialize_with = "crate::pdf_model::nullable")]
    output_bytes: Option<u64>,
    reserved_tokens: u64,
    provider_calls: u32,
    network_calls: u32,
    paid_calls: u32,
    #[serde(deserialize_with = "crate::pdf_model::nullable")]
    price_usd: Option<String>,
    #[serde(deserialize_with = "crate::pdf_model::nullable")]
    model_tokenizer: Option<String>,
    #[serde(deserialize_with = "crate::pdf_model::nullable")]
    model_usage: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationReport {
    schema_version: u32,
    producer: Producer,
    request_id: String,
    binding: SourceBinding,
    status: String,
    knowledge_status: String,
    published: bool,
    scientific_quality: String,
    source_statuses: Vec<String>,
    validated_atoms: usize,
    usage: Usage,
    errors: Vec<Failure>,
    artifacts: BTreeMap<String, Fingerprint>,
}

fn encoded<T: Serialize>(value: &T) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec_pretty(value)
        .map_err(|_| error("EXTRACT_ENCODING", "无法编码有界抽取审计。"))
}

fn markdown(report: &ValidationReport) -> Result<String, ProjectError> {
    let errors = report
        .errors
        .iter()
        .map(|e| format!("- {}: {}\n", e.code, e.message))
        .collect::<String>();
    let facts = String::from_utf8(encoded(report)?)
        .map_err(|_| error("EXTRACT_REPORT", "报告不是UTF-8。"))?;
    let output_tokens = report
        .usage
        .output_tokens
        .map(|n| n.to_string())
        .unwrap_or_else(|| "unknown".into());
    Ok(format!(
        "# 离线 fake 候选验证\n\n状态：{}\n\n请求：{}\n来源版本：{}\n来源状态：{}\n候选原子：{}\n\n知识状态：{}；发布：{}；科学质量：{}。\n\nfake 计量：{}；输入 {} 字节；输出 {} 计量单位；预留 {} 计量单位；实际 provider 调用 {} 次。价格、真实模型使用量与 tokenizer 未知；网络调用 {} 次，付费调用 {} 次。\n\n{}\n## 实际验证快照\n\n下列来源绑定、状态、计量、错误与文件指纹逐字段对应 validation.json。\n\n```json\n{}\n```\n",
        report.status,
        report.request_id,
        report.binding.version_id,
        report.source_statuses.join(", "),
        report.validated_atoms,
        report.knowledge_status,
        report.published,
        report.scientific_quality,
        report.usage.meter,
        report.usage.input_bytes,
        output_tokens,
        report.usage.reserved_tokens,
        report.usage.provider_calls,
        report.usage.network_calls,
        report.usage.paid_calls,
        errors,
        facts
    ))
}

fn save_report(output: &mut OutputRoot, report: &ValidationReport) -> Result<String, ProjectError> {
    let bytes = encoded(report)?;
    output.add("validation.json", &bytes)?;
    output.add("validation.md", markdown(report)?.as_bytes())?;
    let actual: ValidationReport = serde_json::from_slice(&output.read("validation.json")?)
        .map_err(|_| error("EXTRACT_REPORT", "保存的严格报告无法重读。"))?;
    if encoded(&actual)? != bytes || output.read("validation.md")? != markdown(&actual)?.as_bytes()
    {
        return Err(error(
            "EXTRACT_REPORT",
            "双报告与实际保存的验证事实不一致。",
        ));
    }
    output.verify()?;
    String::from_utf8(bytes)
        .map(|json| format!("{json}\n"))
        .map_err(|_| error("EXTRACT_REPORT", "报告不是UTF-8。"))
}

fn report(
    request: &ExtractionRequest,
    usage: &Usage,
    errors: Vec<Failure>,
    count: usize,
    output: &OutputRoot,
) -> ValidationReport {
    ValidationReport {
        schema_version: 1,
        producer: Producer::Fake,
        request_id: request.request_id.clone(),
        binding: request.binding.clone(),
        status: if errors.is_empty() {
            "candidates_validated"
        } else {
            "failed"
        }
        .into(),
        knowledge_status: "pending".into(),
        published: false,
        scientific_quality: "not_run".into(),
        source_statuses: request
            .units
            .iter()
            .map(|u| u.source_status.clone())
            .collect(),
        validated_atoms: count,
        usage: usage.clone(),
        errors,
        artifacts: output.fingerprints(),
    }
}

fn validate(
    request: &ExtractionRequest,
    response: &ResponseEnvelope,
    control: &Control,
    cancelled: &AtomicBool,
    signal: Option<&Path>,
) -> Result<(), ProjectError> {
    if response.request_id != request.request_id || response.binding != request.binding {
        return Err(error(
            "EXTRACT_SOURCE_BINDING",
            "响应的请求、项目、导入、来源版本、解析或资源SHA与实际读取会话不一致。",
        ));
    }
    let payload = &response.payload;
    validate_candidates(
        &request.units,
        &payload.concepts,
        &payload.evidence,
        &payload.atoms,
        control,
        cancelled,
        signal,
    )
}

pub(crate) fn validate_candidates(
    request_units: &[RequestUnit],
    concepts: &[crate::fixture_model::Concept],
    evidence: &[CandidateEvidence],
    candidates: &[CandidateAtom],
    control: &Control,
    cancelled: &AtomicBool,
    signal: Option<&Path>,
) -> Result<(), ProjectError> {
    if concepts.len() > 1024 || evidence.len() > 1024 || candidates.len() > 1024 {
        return Err(error("EXTRACT_CANDIDATE_LIMIT", "每类候选实体最多1024项。"));
    }
    if candidates.is_empty() {
        return Err(error(
            "EXTRACT_EMPTY_CANDIDATES",
            "响应没有任何可验证候选原子。",
        ));
    }
    let atoms = candidates.iter().map(|c| &c.atom).collect::<Vec<_>>();
    let issues = crate::fixture_knowledge::validate_basics(
        concepts,
        evidence.iter().map(|e| e.evidence_id.as_str()),
        &atoms,
    );
    if !issues.is_empty() {
        return Err(error(
            "EXTRACT_KNOWLEDGE",
            "候选概念、原子、明文或唯一引用不符合共用严格IR规则。",
        ));
    }
    let units = request_units
        .iter()
        .map(|u| (u.unit_id.as_str(), u))
        .collect::<BTreeMap<_, _>>();
    let mut evidence_ids = BTreeSet::new();
    for entry in evidence {
        extraction_fake::check(control, cancelled, signal)?;
        let unit = units.get(entry.unit_id.as_str()).ok_or_else(|| {
            error(
                "EXTRACT_EVIDENCE_UNIT",
                "证据没有引用请求内的真实来源单位。",
            )
        })?;
        if entry.excerpt_start_byte >= entry.excerpt_end_byte
            || entry.excerpt_start_byte < unit.start_byte
            || entry.excerpt_end_byte > unit.end_byte
        {
            return Err(error(
                "EXTRACT_EVIDENCE_RANGE",
                "证据必须是实际来源单位内的非空UTF-8字节半开区间。",
            ));
        }
        let text = unit
            .text
            .get(
                entry.excerpt_start_byte - unit.start_byte
                    ..entry.excerpt_end_byte - unit.start_byte,
            )
            .ok_or_else(|| error("EXTRACT_EVIDENCE_RANGE", "证据范围不在UTF-8字符边界。"))?;
        if entry.quote != text {
            return Err(error(
                "EXTRACT_EVIDENCE_QUOTE",
                "证据quote与实际原文切片不同。",
            ));
        }
        if !crate::fixture_model::is_sha(&entry.excerpt_sha256)
            || sha256(text.as_bytes()) != entry.excerpt_sha256
        {
            return Err(error(
                "EXTRACT_EVIDENCE_SHA",
                "证据SHA与实际原文UTF-8切片不同。",
            ));
        }
        evidence_ids.insert(entry.evidence_id.as_str());
    }
    validate_atoms(candidates, &evidence_ids, control, cancelled, signal)?;
    extraction_fake::check(control, cancelled, signal)
}

fn validate_atoms(
    candidates: &[CandidateAtom],
    evidence_ids: &BTreeSet<&str>,
    control: &Control,
    cancelled: &AtomicBool,
    signal: Option<&Path>,
) -> Result<(), ProjectError> {
    let plain = |s: &str| crate::fixture_model::nonempty(s) && crate::fixture_plan::plain_text(s);
    for candidate in candidates {
        extraction_fake::check(control, cancelled, signal)?;
        let atom = &candidate.atom;
        if atom.required || atom.verification != Verification::Unverified {
            return Err(error(
                "EXTRACT_VERIFICATION",
                "候选必须保持unverified且required=false。",
            ));
        }
        if !atom.evidence_links.iter().any(|link| {
            link.relation == EvidenceRelation::Supports
                && evidence_ids.contains(link.evidence_id.as_str())
        }) {
            return Err(error(
                "EXTRACT_SUPPORT",
                "每个候选原子至少需要一条有效supports证据。",
            ));
        }
        if atom.kind == AtomKind::ComparisonValue && candidate.measurement.is_none() {
            return Err(error(
                "EXTRACT_MEASUREMENT",
                "数值候选必须明确声明measurement及未知值。",
            ));
        }
        if let Some(measurement) = &candidate.measurement {
            if measurement.value_status != atom.value_status {
                return Err(error("EXTRACT_MEASUREMENT", "数值状态与原子状态不一致。"));
            }
            match measurement.value_status {
                ValueStatus::Known
                    if !measurement.value.as_deref().is_some_and(&plain)
                        || !measurement.unit.as_deref().is_some_and(&plain) =>
                {
                    return Err(error(
                        "EXTRACT_MEASUREMENT",
                        "known数值和单位必须为非空明文。",
                    ));
                }
                ValueStatus::Unknown if measurement.value.is_some() => {
                    return Err(error(
                        "EXTRACT_MEASUREMENT",
                        "unknown数值必须为null，不能伪装成0。",
                    ));
                }
                ValueStatus::NotApplicable
                    if measurement.value.is_some() || measurement.unit.is_some() =>
                {
                    return Err(error(
                        "EXTRACT_MEASUREMENT",
                        "not_applicable数值和单位必须为null。",
                    ));
                }
                _ => {}
            }
            if measurement
                .unit
                .as_ref()
                .is_some_and(|unit| !plain(unit) || !atom.qualifiers.contains(unit))
                || measurement
                    .conditions
                    .iter()
                    .any(|condition| !plain(condition) || !atom.qualifiers.contains(condition))
            {
                return Err(error(
                    "EXTRACT_QUALIFIERS",
                    "已知单位和每个条件必须保留在原子限定信息中。",
                ));
            }
        }
    }
    Ok(())
}

pub fn fake(
    project_path: &Path,
    import_id: &str,
    case_path: &Path,
    output_path: &Path,
    cancel_path: Option<&Path>,
) -> Result<crate::source::CommandOutput, ProjectError> {
    let input = FakeInput::open(case_path)?;
    let source = SourceSession::open(
        project_path,
        import_id,
        &input.config.selection,
        &input.config.limits,
    )?;
    let destination = project::check_components(output_path)?;
    if extraction_output::inside(&destination, source.root())?
        || extraction_output::inside(&destination, input.root())?
    {
        return Err(ProjectError::new(
            "EXTRACT_OUTPUT_OVERLAP",
            2,
            "输出不能与来源项目或fake输入目录相交。",
        ));
    }
    let signal = cancel_path.map(project::check_components).transpose()?;
    if let Some(path) = &signal {
        if path == &destination
            || extraction_output::inside(path, source.root())?
            || extraction_output::inside(path, input.root())?
            || path.parent().is_none_or(|parent| !parent.is_dir())
        {
            return Err(ProjectError::new(
                "EXTRACT_CANCEL_PATH",
                2,
                "取消信号须位于输入/输出根外的已有普通父目录。",
            ));
        }
        if project::exists(path)? && !project::checked_file(path)?.is_file() {
            return Err(ProjectError::new(
                "EXTRACT_CANCEL_PATH",
                2,
                "取消信号须为普通文件。",
            ));
        }
    }
    let request = ExtractionRequest {
        schema_version: 1,
        producer: Producer::Fake,
        request_id: uuid::Uuid::new_v4().to_string(),
        binding: source.binding.clone(),
        units: source.units.clone(),
        limits: input.config.limits.clone(),
        budget: input.config.budget.clone(),
    };
    let input_bytes = request
        .units
        .iter()
        .map(|unit| unit.text.len() as u64)
        .sum::<u64>();
    let reserved = input_bytes
        .checked_add(request.limits.max_output_tokens)
        .ok_or_else(|| error("EXTRACT_BUDGET", "fake计量预留溢出。"))?;
    let mut usage = Usage {
        meter: "fake_utf8_bytes_v1".into(),
        input_tokens: input_bytes,
        input_bytes,
        output_tokens: None,
        output_bytes: None,
        reserved_tokens: reserved,
        provider_calls: 0,
        network_calls: 0,
        paid_calls: 0,
        price_usd: None,
        model_tokenizer: None,
        model_usage: None,
    };
    let mut output = OutputRoot::create(&destination, &request.request_id)?;
    output.add("request.json", &encoded(&request)?)?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let control = Control::new(request.limits.timeout_ms, cancelled.clone());
    let execution = (|| {
        extraction_fake::check(&control, &cancelled, signal.as_deref())?;
        if !input.config.capabilities.text || !input.config.capabilities.structured_output {
            return Err(error(
                "EXTRACT_CAPABILITY",
                "fake provider缺少文本或结构化输出能力。",
            ));
        }
        if request.budget.max_requests == 0 || reserved > request.budget.max_total_tokens {
            return Err(error(
                "EXTRACT_BUDGET",
                "单请求次数或fake token预留超过预算，未调用provider。",
            ));
        }
        if request.budget.max_cost_usd.is_some() {
            return Err(error(
                "EXTRACT_PRICE_UNKNOWN",
                "价格未知，无法证明金额上限；未调用provider，未知费用没有记为0。",
            ));
        }
        usage.provider_calls += 1;
        let bytes = input.replay(&request, &control, &cancelled, signal.as_deref())?;
        usage.output_bytes = Some(bytes.len() as u64);
        usage.output_tokens = usage.output_bytes;
        output.add("response.bin", &bytes)?;
        if bytes.len() as u64 > request.limits.max_output_tokens {
            return Err(error(
                "EXTRACT_OUTPUT_TOKENS",
                "完整fake响应超过fake UTF-8字节token上限。",
            ));
        }
        let response: ResponseEnvelope = serde_json::from_slice(&bytes).map_err(|_| {
            error(
                "EXTRACT_RESPONSE_JSON",
                "响应不符合严格JSON/schema：重复、额外、缺失字段及尾随数据均拒绝。",
            )
        })?;
        validate(&request, &response, &control, &cancelled, signal.as_deref())?;
        Ok(response)
    })();
    let mut errors = Vec::new();
    let mut candidate = match execution {
        Ok(candidate) => Some(candidate),
        Err(e) => {
            errors.push(Failure::from(&e));
            None
        }
    };
    for result in [source.verify(), input.verify(), output.verify()] {
        if let Err(e) = result {
            errors.push(Failure::from(&e));
            candidate = None;
        }
    }
    if let Some(candidate) = candidate {
        let publication = (|| {
            output.add("candidates.json", &encoded(&candidate)?)?;
            source.verify()?;
            input.verify()?;
            output.verify()?;
            let result = report(
                &request,
                &usage,
                Vec::new(),
                candidate.payload.atoms.len(),
                &output,
            );
            let json = save_report(&mut output, &result)?;
            source.verify()?;
            input.verify()?;
            output.verify()?;
            Ok(json)
        })();
        match publication {
            Ok(json) => {
                return Ok(crate::source::CommandOutput {
                    json,
                    failure: None,
                });
            }
            Err(e) => errors.push(Failure::from(&e)),
        }
    }
    output.discard_result()?;
    // A changed output cannot receive a fabricated new receipt. Preserve its
    // unknown files and return a nonzero diagnostic instead.
    output.verify()?;
    let result = report(&request, &usage, errors, 0, &output);
    let json = save_report(&mut output, &result)?;
    let first = result
        .errors
        .first()
        .ok_or_else(|| error("EXTRACT_STATE", "失败审计缺少实际错误。"))?;
    Ok(crate::source::CommandOutput {
        json,
        failure: Some((first.code.clone(), first.message.clone())),
    })
}
