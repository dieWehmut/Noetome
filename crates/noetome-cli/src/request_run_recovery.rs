//! Offline validation of already-sealed transport bytes. No HTTP is issued here.
use super::{decoded, invalid, owned_stamp, write_new};
use crate::{
    extraction_api,
    extraction_api_config::SelectedConfig,
    extraction_api_schema, extraction_api_wire,
    extraction_source::SourceSession,
    project::{self, ProjectError},
    request_budget_model::{AttemptPlan, TransportReceipt},
    sidecar_process::Control,
    text::sha256,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

fn safe_json(path: &Path) -> Result<Value, ProjectError> {
    serde_json::from_slice(&owned_stamp(path)?.1).map_err(|_| invalid("原恢复JSON无效。"))
}
fn add_or_match(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    if project::exists(path)? {
        if owned_stamp(path)?.1 != bytes {
            return Err(invalid("既有恢复结果与重新原件校验不同，不覆盖。"));
        }
        Ok(())
    } else {
        write_new(path, bytes)
    }
}
/// Charging is independent of changed source/configuration. Only the already
/// sealed request and transport enter this path; no current credential, source
/// or candidate acceptance is read. BudgetStore repeats exact pin and wire
/// accounting validation in its settlement transaction.
pub(super) fn recover_accounting(
    output: &Path,
    plan: &AttemptPlan,
    diagnostic: &'static str,
) -> Result<(Value, bool), ProjectError> {
    if project::exists(&output.join("validation.json"))? {
        return Ok((safe_json(&output.join("validation.json"))?, false));
    }
    let mut originals = BTreeMap::new();
    for n in [
        ".owner.json",
        "request.json",
        "request-body.json",
        "transport-receipt.json",
        "response.bin",
    ] {
        let p = output.join(n);
        if project::exists(&p)? {
            originals.insert(n, owned_stamp(&p)?.0);
        }
    }
    let request = safe_json(&output.join("request.json"))?;
    let t: TransportReceipt = decoded(&owned_stamp(&output.join("transport-receipt.json"))?.1)?;
    if t.schema_version != 1
        || t.producer != "synthetic_api_transport_receipt"
        || t.request_id != plan.key.request_id
        || request["request_id"] != plan.key.request_id
        || request["provider"] != "openai_responses"
        || request["transport_mode"] != "local_http_test"
    {
        return Err(invalid("纯计费恢复缺原请求/传输绑定。"));
    }
    let body = owned_stamp(&output.join("request-body.json"))?;
    if body.0.sha256 != plan.request_body.sha256
        || body.0.byte_length != plan.request_body.byte_length
    {
        return Err(invalid("纯计费原正文pin变化。"));
    }
    let mut r = json!({"schema_version":1,"report_kind":"text_api_extraction","producer":"api","provider":"openai_responses","transport_mode":"local_http_test",
        "request_id":plan.key.request_id,"binding":request["binding"],"endpoint":request["endpoint"],"request_strategy":request["request_strategy"],"configured_model":request["configured_model"],
        "actual_model":null,"response_id":null,"service_tier":null,"http_request_id":t.http_request_id,"http_status":t.http_status,"response_complete":t.response_complete,
        "status":"failed","knowledge_status":"pending","published":false,"scientific_quality":"not_run","validated_atoms":0,"source_statuses":[],
        "usage":{"network_attempts":t.network_attempts,"real_model_calls":0,"paid_calls":0,"price_usd":null,"model_tokenizer":null,"model_usage":null,"model_usage_status":"not_validated",
            "input_reserved_tokens":plan.input_reserved_tokens,"reserved_tokens":plan.reserved_tokens,"reservation_policy":plan.reservation_policy,"request_utf8_bytes":plan.request_body.byte_length},
        "response_receipt":{"sha256":t.response.sha256,"byte_length":t.response.byte_length,"omission_reason":t.omission_reason},
        "errors":[{"code":t.error_code.as_deref().unwrap_or(diagnostic),"message":"当前请求校验不能完成；只核算原封存传输，不接受候选、不发送请求。"}],"artifacts":{},
        "recovery":{"mode":"sealed_transport_accounting_only","issued_http":false,"candidate_validation":"not_run"}});
    if t.error_code.is_none()
        && t.omission_reason.is_none()
        && t.network_attempts == 1
        && t.http_status == Some(200)
        && t.response_complete
    {
        let (s, raw) = owned_stamp(&output.join("response.bin"))?;
        if s.sha256 != t.response.sha256 || s.byte_length != t.response.byte_length {
            return Err(invalid("原计费用量传输pin不等。"));
        }
        if let Ok(metadata) = extraction_api_wire::response_metadata(&raw) {
            r["actual_model"] = json!(metadata.model);
            r["response_id"] = json!(metadata.id);
            r["service_tier"] = json!(metadata.service_tier);
            r["usage"]["model_usage"] = metadata.usage;
            r["usage"]["model_usage_status"] = json!("validated");
            if let Some(raw_usage) = metadata.raw_usage {
                r["usage"]["raw_model_usage"] = raw_usage;
            }
        }
    }
    for (n, s) in &originals {
        if owned_stamp(&output.join(n))?.0 != *s {
            return Err(invalid("纯计费恢复期间原件变化。"));
        }
    }
    add_or_match(
        &output.join("recovery-validation.json"),
        &serde_json::to_vec_pretty(&r).map_err(|_| invalid("计费报告序列化失败。"))?,
    )?;
    for (n, s) in &originals {
        if owned_stamp(&output.join(n))?.0 != *s {
            return Err(invalid("纯计费报告同步期间原件变化。"));
        }
    }
    Ok((r, true))
}
pub(super) fn recover(
    output: &Path,
    plan: &AttemptPlan,
    project_path: &str,
    import: &str,
    config_path: &str,
) -> Result<(Value, bool), ProjectError> {
    // A normal completed report must retain both original JSON and Markdown;
    // BudgetStore independently verifies those bytes again before settlement.
    if project::exists(&output.join("validation.json"))? {
        return Ok((safe_json(&output.join("validation.json"))?, false));
    }
    let mut originals = BTreeMap::new();
    for name in [
        ".owner.json",
        "request.json",
        "request-body.json",
        "transport-receipt.json",
        "response.bin",
    ] {
        let path = output.join(name);
        if project::exists(&path)? {
            originals.insert(name, owned_stamp(&path)?.0);
        }
    }
    let transport: TransportReceipt =
        decoded(&owned_stamp(&output.join("transport-receipt.json"))?.1)?;
    if transport.schema_version != 1
        || transport.producer != "synthetic_api_transport_receipt"
        || transport.request_id != plan.key.request_id
        || transport.network_attempts > 1
    {
        return Err(invalid("恢复传输的真实身份/framing无效。"));
    }
    let selected = SelectedConfig::open(Path::new(config_path))?;
    let cfg = &selected.config;
    if cfg.transport_mode != "local_http_test"
        || cfg.provider != "openai_responses"
        || !cfg.credential_env.starts_with("NOETOME_TEST_")
        || !cfg.model.starts_with("noetome-test-")
    {
        return Err(invalid("恢复仅数字环回人工Responses来源。"));
    }
    let bearer = selected.credential()?;
    let source = SourceSession::open(Path::new(project_path), import, &cfg.selection, &cfg.limits)?;
    source.verify()?;
    selected.verify()?;
    let strategy = cfg.request_strategy();
    let body = extraction_api_schema::request_body(
        strategy,
        &cfg.model,
        &plan.key.request_id,
        &source.binding,
        &source.units,
        cfg.limits.max_output_tokens,
    )?;
    let request = safe_json(&output.join("request.json"))?;
    let expected_request = json!({"schema_version":1,"producer":"api","provider":cfg.provider,"transport_mode":cfg.transport_mode,
        "request_id":plan.key.request_id,"binding":source.binding,"units":source.units,"configured_model":cfg.model,
        "endpoint":cfg.endpoint,"request_strategy":strategy.version(),"config":{"path":selected.path.display().to_string(),"fingerprint":selected.fingerprint()},
        "credential_env":cfg.credential_env,"limits":cfg.limits,"budget":cfg.budget,
        "reservation_policy":"request_utf8_bytes_plus_1024_v1","input_reserved_tokens":plan.input_reserved_tokens,"reserved_tokens":plan.reserved_tokens,
        "model_tokenizer":null,"request_body":{"sha256":sha256(&body),"byte_length":body.len()}});
    if request != expected_request
        || owned_stamp(&output.join("request-body.json"))?.1 != body
        || plan.request_body.sha256 != sha256(&body)
        || plan.request_body.byte_length != body.len() as u64
        || plan.request_metadata.sha256 != sha256(&owned_stamp(&output.join("request.json"))?.1)
        || plan.input_reserved_tokens != body.len() as u64 + 1024
        || plan.max_output_tokens != cfg.limits.max_output_tokens
        || plan.reserved_tokens != plan.input_reserved_tokens + plan.max_output_tokens
    {
        return Err(invalid("恢复来源/配置/策略/正文与原计划不等。"));
    }
    let response = if transport.response_file.as_deref() == Some("response.bin") {
        let (s, b) = owned_stamp(&output.join("response.bin"))?;
        if s.sha256 != transport.response.sha256
            || s.byte_length != transport.response.byte_length
            || transport.omission_reason.is_some()
        {
            return Err(invalid("原响应与framing pin不同。"));
        }
        Some(b)
    } else {
        if project::exists(&output.join("response.bin"))? {
            return Err(invalid("传输没有绑定存在的响应。"));
        }
        None
    };
    let mut report = json!({"schema_version":1,"report_kind":"text_api_extraction","producer":"api","provider":cfg.provider,
        "transport_mode":cfg.transport_mode,"request_id":plan.key.request_id,"binding":source.binding,
        "endpoint":cfg.endpoint,"request_strategy":strategy.version(),"configured_model":cfg.model,"actual_model":null,
        "response_id":null,"service_tier":null,"http_request_id":transport.http_request_id,"http_status":transport.http_status,
        "response_complete":transport.response_complete,"status":"failed","knowledge_status":"pending","published":false,"scientific_quality":"not_run",
        "source_statuses":source.units.iter().map(|u|&u.source_status).collect::<Vec<_>>(),"validated_atoms":0,
        "usage":{"network_attempts":transport.network_attempts,"real_model_calls":0,"paid_calls":0,"price_usd":null,"model_tokenizer":null,
            "model_usage":null,"model_usage_status":"not_validated","input_reserved_tokens":plan.input_reserved_tokens,"reserved_tokens":plan.reserved_tokens,
            "reservation_policy":plan.reservation_policy,"request_utf8_bytes":body.len()},
        "response_receipt":{"sha256":transport.response.sha256,"byte_length":transport.response.byte_length,"omission_reason":transport.omission_reason},
        "errors":[],"artifacts":{},"recovery":{"mode":"sealed_transport_offline","issued_http":false}});
    let cancelled = Arc::new(AtomicBool::new(false));
    let control = Control::new(60000, cancelled.clone());
    let validation = (|| {
        if transport.omission_reason.is_some() {
            return Err(ProjectError::new(
                "EXTRACT_API_CREDENTIAL_ECHO",
                3,
                "原传输安全省略凭据回显。",
            ));
        }
        if let Some(code) = &transport.error_code {
            return Err(super::err_code(code, "原传输故障保留，未重新请求。"));
        }
        if transport.network_attempts != 1
            || transport.http_status != Some(200)
            || !transport.response_complete
        {
            return Err(invalid("framing未证明完整HTTP200。"));
        }
        let raw = response.as_ref().ok_or_else(|| invalid("原响应缺失。"))?;
        if extraction_api::credential_echo(raw, &bearer) {
            return Err(ProjectError::new(
                "EXTRACT_API_CREDENTIAL_ECHO",
                3,
                "原响应含人工凭据回显，拒绝恢复。",
            ));
        }
        if let Ok(metadata) = extraction_api_wire::response_metadata_with_strategy(raw, strategy) {
            report["actual_model"] = json!(metadata.model);
            report["response_id"] = json!(metadata.id);
            report["service_tier"] = json!(metadata.service_tier);
            report["usage"]["model_usage"] = metadata.usage.clone();
            report["usage"]["model_usage_status"] = json!("validated");
            if let Some(raw_usage) = metadata.raw_usage {
                report["usage"]["raw_model_usage"] = raw_usage;
            }
            if !extraction_api::usage_within_budget(
                &metadata.usage,
                plan.input_reserved_tokens,
                plan.max_output_tokens,
                cfg.budget.max_total_tokens,
            ) {
                return Err(ProjectError::new(
                    "EXTRACT_BUDGET_STOP",
                    3,
                    "已验证实际用量超过原预留/上限。",
                ));
            }
        }
        let parsed = extraction_api_wire::decode_with_strategy(raw, strategy)?;
        if report["response_id"] != json!(parsed.id)
            || report["actual_model"] != json!(parsed.model)
            || report["service_tier"] != json!(parsed.service_tier)
            || report["usage"]["model_usage"] != parsed.usage
        {
            return Err(invalid("原wire元数据与候选不一致。"));
        }
        let candidate = parsed.candidates;
        if candidate.request_id != plan.key.request_id || candidate.binding != source.binding {
            return Err(ProjectError::new(
                "EXTRACT_SOURCE_BINDING",
                3,
                "原候选请求/来源不等。",
            ));
        }
        crate::extraction::validate_candidates(
            &source.units,
            &candidate.concepts,
            &candidate.evidence,
            &candidate.atoms,
            &control,
            &cancelled,
            None,
        )?;
        Ok(candidate)
    })();
    match validation {
        Ok(candidate) => {
            let value = json!({"schema_version":1,"producer":"api","transport_mode":cfg.transport_mode,"request_id":plan.key.request_id,"binding":source.binding,"payload":candidate});
            add_or_match(
                &output.join("candidates.json"),
                &serde_json::to_vec_pretty(&value).map_err(|_| invalid("候选序列化错误。"))?,
            )?;
            report["status"] = json!("candidates_validated");
            report["validated_atoms"] = json!(candidate.atoms.len());
        }
        Err(e) => {
            report["errors"] = json!([{"code":e.code(),"message":"原封存传输经离线校验未形成候选；保留实际用量或未知承诺。"}]);
        }
    }
    source.verify()?;
    selected.verify()?;
    for (n, s) in &originals {
        if owned_stamp(&output.join(n))?.0 != *s {
            return Err(invalid("原响应恢复期间变化。"));
        }
    }
    let bytes = serde_json::to_vec_pretty(&report).map_err(|_| invalid("恢复报告序列化失败。"))?;
    add_or_match(&output.join("recovery-validation.json"), &bytes)?;
    source.verify()?;
    selected.verify()?;
    for (n, s) in &originals {
        if owned_stamp(&output.join(n))?.0 != *s {
            return Err(invalid("恢复报告同步期间原件变化。"));
        }
    }
    Ok((report, true))
}
