#[cfg(test)]
mod tests {
    #[test]
    fn credential_scan_catches_nested_escaped_and_malformed_strings() {
        let key = "noetome-test-secret";
        let escaped = key
            .chars()
            .map(|c| format!("\\u{:04x}", c as u32))
            .collect::<String>();
        let nested =
            serde_json::to_vec(&serde_json::json!({"text":format!("{{\"name\":\"{escaped}\"}}") }))
                .unwrap();
        for bytes in [
            nested,
            format!("{{\"error\":\"{escaped}\" ]").into_bytes(),
            key.as_bytes().to_vec(),
        ] {
            assert!(super::credential_echo(&bytes, key));
        }
        assert!(!super::credential_echo(b"{\"text\":\"ordinary\"}", key));
    }
    #[test]
    fn credential_scan_catches_literal_escape_keys_with_mixed_encoding_depths() {
        for prefix in [
            r"noetome-test-\n-",
            r"noetome-test-\u0065-",
            r#"noetome-test-\"-"#,
            r#"noetome-test-\n-\u0065-\"-"#,
        ] {
            let key = format!("{prefix}abcdef");
            let mut tail = "abcdef"
                .chars()
                .map(|c| format!("\\u{:04x}", c as u32))
                .collect::<String>();
            for _ in 0..4 {
                tail = tail.replace('\\', "\\\\");
            }
            let mixed = format!("{prefix}{tail}");
            let nested = serde_json::to_vec(&serde_json::json!({"model":mixed})).unwrap();
            assert!(super::credential_echo(&nested, &key));
            assert!(super::credential_echo(
                format!("{{\"model\":\"{mixed}\" ]").as_bytes(),
                &key
            ));
            assert!(super::credential_echo(key.as_bytes(), &key));
            assert!(!super::credential_echo(
                b"{\"model\":\"ordinary-returned-model\"}",
                &key
            ));
        }
    }
    #[test]
    fn credential_scan_fails_closed_when_the_total_work_limit_is_exceeded() {
        assert!(super::credential_echo(
            &vec![b'x'; 64 * 1024 * 1024 + 1],
            "noetome-test-secret"
        ));
    }
    #[test]
    fn actual_usage_rejects_each_limit_and_overflow_without_zero_defaults() {
        let valid = serde_json::json!({"input_tokens":100,"output_tokens":600,"total_tokens":700});
        assert!(super::usage_within_budget(&valid, 200, 600, 700));
        for (input, output, total) in [(99, 600, 700), (200, 599, 700), (200, 600, 699)] {
            assert!(!super::usage_within_budget(&valid, input, output, total));
        }
        assert!(!super::usage_within_budget(
            &serde_json::json!({"input_tokens":u64::MAX,"output_tokens":1,"total_tokens":0}),
            u64::MAX,
            u64::MAX,
            u64::MAX
        ));
        assert!(!super::usage_within_budget(
            &serde_json::json!({"input_tokens":100,"output_tokens":null,"total_tokens":100}),
            200,
            600,
            700
        ));
    }
}
use crate::{
    extraction_api_config::SelectedConfig,
    extraction_api_schema, extraction_api_transport, extraction_api_wire, extraction_fake,
    extraction_output::{self, OutputRoot},
    extraction_source::SourceSession,
    project::{self, ProjectError},
    sidecar_process::Control,
    text::sha256,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

/// Scheduler gates around the existing single-request adapter. The ordinary
/// adapter keeps the no-op path and its original artifact contract.
pub(crate) trait ApiHooks {
    fn enabled(&self) -> bool {
        false
    }
    fn timeout_ms(&self, default_timeout_ms: u64) -> u64 {
        default_timeout_ms
    }
    fn output_directories(
        &self,
    ) -> std::collections::BTreeMap<std::path::PathBuf, crate::compile_identity::Identity> {
        std::collections::BTreeMap::new()
    }
    fn selected_config(
        &self,
        _path: &Path,
        _fingerprint: &crate::sidecar_model::Fingerprint,
    ) -> Result<(), ProjectError> {
        Ok(())
    }
    fn prepared(&mut self, _output: &Path, _request: &Value) -> Result<(), ProjectError> {
        Ok(())
    }
    fn before_send(&mut self, _output: &Path, _request: &Value) -> Result<(), ProjectError> {
        Ok(())
    }
    fn transport_saved(
        &mut self,
        _output: &Path,
        _receipt: &crate::request_budget_model::TransportReceipt,
    ) -> Result<(), ProjectError> {
        Ok(())
    }
    fn finished(&mut self, _output: &Path, _report: &Value) -> Result<(), ProjectError> {
        Ok(())
    }
}
struct NoopHooks;
impl ApiHooks for NoopHooks {}

fn selected_credential(
    selected: &SelectedConfig,
    hooks: &dyn ApiHooks,
) -> Result<String, ProjectError> {
    if hooks.enabled() {
        let cfg = &selected.config;
        if cfg.transport_mode != "local_http_test"
            || cfg.provider != "openai_responses"
            || !cfg.model.starts_with("noetome-test-")
            || !cfg.credential_env.starts_with("NOETOME_TEST_")
        {
            return Err(ProjectError::new(
                "REQUEST_RUN_CONFIG",
                2,
                "有界运行器仅使用人工环回配置与NOETOME_TEST_凭据引用。",
            ));
        }
        hooks.selected_config(&selected.path, &selected.fingerprint())?;
        selected.verify()?;
    }
    selected.credential()
}

fn save_transport(
    output: &mut OutputRoot,
    receipt: &crate::request_budget_model::TransportReceipt,
) -> Result<(), ProjectError> {
    let bytes = serde_json::to_vec_pretty(receipt)
        .map_err(|_| error("EXTRACT_ENCODING", "传输原件编码失败。"))?;
    output.add("transport-receipt.json", &bytes)?;
    if output.read("transport-receipt.json")? != bytes {
        return Err(error("EXTRACT_REPORT", "传输原件持久化验证失败。"));
    }
    output.verify()
}

fn error(code: &'static str, message: &str) -> ProjectError {
    ProjectError::new(code, 3, message)
}

/// Decode escape tokens repeatedly, including within nested serialized JSON and
/// malformed outer JSON. Decoding always shrinks the buffer. Bound total work
/// and fail closed for adversarial depths; no unsafe original bytes are saved.
pub(crate) fn credential_echo(bytes: &[u8], key: &str) -> bool {
    if key.is_empty() {
        return true;
    }
    let mut work = 0usize;
    // A literal escape within the credential may decode before a differently
    // escaped suffix does. Match both forms at every bounded response layer.
    let mut normalized_key = key.as_bytes().to_vec();
    loop {
        work = work.saturating_add(normalized_key.len());
        if work > 64 * 1024 * 1024 {
            return true;
        }
        let next = decode_escape_tokens(&normalized_key);
        if next == normalized_key {
            break;
        }
        normalized_key = next;
    }
    let mut current = bytes.to_vec();
    loop {
        work = work.saturating_add(current.len());
        if work > 64 * 1024 * 1024 {
            return true;
        }
        if current.windows(key.len()).any(|w| w == key.as_bytes())
            || current
                .windows(normalized_key.len())
                .any(|w| w == normalized_key)
        {
            return true;
        }
        let next = decode_escape_tokens(&current);
        if next == current {
            return false;
        }
        current = next;
    }
}
fn decode_escape_tokens(current: &[u8]) -> Vec<u8> {
    let mut next = Vec::with_capacity(current.len());
    let mut i = 0;
    while i < current.len() {
        if current[i] == b'\\' && i + 1 < current.len() {
            let width = if current[i + 1] == b'u' { 6 } else { 2 };
            if i + width <= current.len() {
                let mut token = Vec::with_capacity(width + 2);
                token.push(b'"');
                token.extend_from_slice(&current[i..i + width]);
                token.push(b'"');
                if let Ok(decoded) = serde_json::from_slice::<String>(&token) {
                    next.extend_from_slice(decoded.as_bytes());
                    i += width;
                    continue;
                }
            }
        }
        next.push(current[i]);
        i += 1;
    }
    next
}
fn encoded(value: &Value) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec_pretty(value).map_err(|_| error("EXTRACT_ENCODING", "API审计编码失败。"))
}
fn safe_error(e: ProjectError, bearer: &str) -> ProjectError {
    if credential_echo(e.to_string().as_bytes(), bearer) {
        ProjectError::new(e.code(), e.exit_code(), "诊断含所选外部凭据，原文已省略。")
    } else {
        e
    }
}
fn failure(e: ProjectError, bearer: &str) -> Value {
    let e = safe_error(e, bearer);
    json!({"code":e.code(),"message":e.to_string().chars().take(2048).collect::<String>()})
}
fn echo_error() -> ProjectError {
    error(
        "EXTRACT_API_CREDENTIAL_ECHO",
        "检测到所选外部凭据回显；未保存泄密原文或候选。",
    )
}
pub(crate) fn usage_within_budget(
    usage: &Value,
    input_limit: u64,
    output_limit: u64,
    total_limit: u64,
) -> bool {
    let (Some(input), Some(output), Some(total)) = (
        usage["input_tokens"].as_u64(),
        usage["output_tokens"].as_u64(),
        usage["total_tokens"].as_u64(),
    ) else {
        return false;
    };
    input <= input_limit
        && output <= output_limit
        && total <= total_limit
        && input.checked_add(output) == Some(total)
}

fn save_report(output: &mut OutputRoot, report: &Value) -> Result<String, ProjectError> {
    let bytes = encoded(report)?;
    let snapshot =
        String::from_utf8(bytes.clone()).map_err(|_| error("EXTRACT_REPORT", "报告不是UTF8。"))?;
    let title = if report["transport_mode"] == "local_http_test" {
        "本地HTTP假服务 API 候选验证"
    } else {
        "单请求文本 API 候选验证"
    };
    let md = format!(
        "# {title}\n\n状态：{}；候选原子：{}。知识：pending；发布：false；科学质量：not_run。\n\n配置模型：{}；实际返回模型：{}；HTTP状态：{}；完整响应：{}。\n\n实际网络尝试：{}。input_reserved_tokens 为实际请求UTF8字节加1024的工程预留；实际Token用量只记录已验证的原响应。价格、tokenizer及账单未知。\n\n来源绑定、请求策略、用量、失败诊断及实际文件指纹如下，逐字段对应 validation.json。\n\n```json\n{snapshot}\n```\n",
        report["status"],
        report["validated_atoms"],
        report["configured_model"],
        report["actual_model"],
        report["http_status"],
        report["response_complete"],
        report["usage"]["network_attempts"]
    );
    output.add("validation.json", &bytes)?;
    output.add("validation.md", md.as_bytes())?;
    if output.read("validation.json")? != bytes || output.read("validation.md")? != md.as_bytes() {
        return Err(error("EXTRACT_REPORT", "双报告持久化验证失败。"));
    }
    output.verify()?;
    Ok(format!("{snapshot}\n"))
}

pub fn run(
    project_path: &Path,
    import_id: &str,
    config_path: &Path,
    output_path: &Path,
    cancel_path: Option<&Path>,
) -> Result<crate::source::CommandOutput, ProjectError> {
    run_with_hooks(
        project_path,
        import_id,
        config_path,
        output_path,
        cancel_path,
        &mut NoopHooks,
    )
}
pub(crate) fn run_with_hooks(
    project_path: &Path,
    import_id: &str,
    config_path: &Path,
    output_path: &Path,
    cancel_path: Option<&Path>,
    hooks: &mut dyn ApiHooks,
) -> Result<crate::source::CommandOutput, ProjectError> {
    let selected = SelectedConfig::open(config_path)?;
    let bearer = selected_credential(&selected, hooks)?;
    run_selected(
        project_path,
        import_id,
        &selected,
        output_path,
        cancel_path,
        &bearer,
        hooks,
        None,
    )
    .map_err(|e| safe_error(e, &bearer))
}
pub(crate) fn resume_prepared_with_hooks(
    project_path: &Path,
    import_id: &str,
    config_path: &Path,
    output_path: &Path,
    cancel_path: Option<&Path>,
    attempt: &crate::request_budget_model::AttemptView,
    run: &crate::request_budget_model::RunView,
    hooks: &mut dyn ApiHooks,
) -> Result<crate::source::CommandOutput, ProjectError> {
    let selected = SelectedConfig::open(config_path)?;
    let bearer = selected_credential(&selected, hooks)?;
    run_selected(
        project_path,
        import_id,
        &selected,
        output_path,
        cancel_path,
        &bearer,
        hooks,
        Some((attempt, run)),
    )
    .map_err(|e| safe_error(e, &bearer))
}
fn run_selected(
    project_path: &Path,
    import_id: &str,
    selected: &SelectedConfig,
    output_path: &Path,
    cancel_path: Option<&Path>,
    bearer: &str,
    hooks: &mut dyn ApiHooks,
    resume: Option<(
        &crate::request_budget_model::AttemptView,
        &crate::request_budget_model::RunView,
    )>,
) -> Result<crate::source::CommandOutput, ProjectError> {
    let cfg = &selected.config;
    let source = SourceSession::open(project_path, import_id, &cfg.selection, &cfg.limits)?;
    let destination = project::check_components(output_path)?;
    if extraction_output::inside(&selected.path, source.root())? {
        return Err(ProjectError::new(
            "EXTRACT_API_CONFIG",
            2,
            "配置必须位于来源项目之外。",
        ));
    }
    if destination == source.root()
        || extraction_output::inside(&destination, source.root())?
        || (project::exists(&destination)?
            && extraction_output::inside(source.root(), &destination)?)
        || selected.path == destination
        || (project::exists(&destination)?
            && extraction_output::inside(&selected.path, &destination)?)
    {
        return Err(ProjectError::new(
            "EXTRACT_OUTPUT_OVERLAP",
            2,
            "API输出须与来源项目和所选配置分离。",
        ));
    }
    let signal = cancel_path.map(project::check_components).transpose()?;
    if let Some(path) = &signal {
        if path == &selected.path
            || path == &destination
            || extraction_output::inside(path, source.root())?
            || (project::exists(&destination)? && extraction_output::inside(path, &destination)?)
            || path.parent().is_none_or(|p| !p.is_dir())
        {
            return Err(ProjectError::new(
                "EXTRACT_CANCEL_PATH",
                2,
                "取消信号须为来源/输出之外且不替代配置的普通文件。",
            ));
        }
        if project::exists(path)? {
            project::checked_file(path).map_err(|_| {
                ProjectError::new("EXTRACT_CANCEL_PATH", 2, "取消信号必须为普通文件。")
            })?;
        }
    }
    let request_id = match resume {
        Some((attempt, _)) => {
            let id = attempt.plan.key.request_id.as_str();
            let parsed = uuid::Uuid::parse_str(id)
                .map_err(|_| error("EXTRACT_SOURCE_BINDING", "原请求ID无效。"))?;
            if parsed.to_string() != id {
                return Err(error("EXTRACT_SOURCE_BINDING", "原请求ID非规范UUID。"));
            }
            parsed.to_string()
        }
        None => uuid::Uuid::new_v4().to_string(),
    };
    let strategy = cfg.request_strategy();
    let body = extraction_api_schema::request_body(
        strategy,
        &cfg.model,
        &request_id,
        &source.binding,
        &source.units,
        cfg.limits.max_output_tokens,
    )?;
    let input_reserved = (body.len() as u64)
        .checked_add(1024)
        .ok_or_else(|| error("EXTRACT_BUDGET_STOP", "输入预留溢出。"))?;
    let reserved = input_reserved
        .checked_add(cfg.limits.max_output_tokens)
        .ok_or_else(|| error("EXTRACT_BUDGET_STOP", "总预留溢出。"))?;
    // Neither the selected credential value nor authorization headers enter DTOs.
    let request = json!({"schema_version":1,"producer":"api","provider":cfg.provider,"transport_mode":cfg.transport_mode,
        "request_id":request_id,"binding":source.binding,"units":source.units,"configured_model":cfg.model,
        "endpoint":cfg.endpoint,"request_strategy":strategy.version(),"config":{"path":selected.path.display().to_string(),"fingerprint":selected.fingerprint()},
        "credential_env":cfg.credential_env,"limits":cfg.limits,"budget":cfg.budget,
        "reservation_policy":"request_utf8_bytes_plus_1024_v1","input_reserved_tokens":input_reserved,"reserved_tokens":reserved,
        "model_tokenizer":null,"request_body":{"sha256":sha256(&body),"byte_length":body.len()}});
    let request_bytes = encoded(&request)?;
    if credential_echo(&body, &bearer) || credential_echo(&request_bytes, &bearer) {
        return Err(echo_error());
    }
    source.verify()?;
    selected.verify()?;
    let mut output = if let Some((attempt, run)) = resume {
        let output =
            OutputRoot::resume_prepared_api(&destination, &cfg.transport_mode, attempt, run)?;
        if output.read("request.json")? != request_bytes
            || output.read("request-body.json")? != body
        {
            return Err(error(
                "EXTRACT_SOURCE_BINDING",
                "来源、配置或原请求正文发生变化；不发送。",
            ));
        }
        output
    } else {
        let mut output = if hooks.enabled() {
            OutputRoot::create_api_guarded(
                &destination,
                &request_id,
                &cfg.transport_mode,
                &hooks.output_directories(),
            )?
        } else {
            OutputRoot::create_api(&destination, &request_id, &cfg.transport_mode)?
        };
        output.add("request.json", &request_bytes)?;
        output.add("request-body.json", &body)?;
        output
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    // Durable admission and candidate validation consume the original Unit/Run
    // deadline. Only the HTTP operation consumes the adapter's request timeout.
    let preflight_timeout_ms = if hooks.enabled() {
        hooks.timeout_ms(u64::MAX)
    } else {
        cfg.limits.timeout_ms
    };
    let control = RefCell::new(Control::new(preflight_timeout_ms, cancelled.clone()));
    let check = || extraction_fake::check(&control.borrow(), &cancelled, signal.as_deref());
    let mut report = json!({"schema_version":1,"report_kind":"text_api_extraction","producer":"api","provider":cfg.provider,
        "transport_mode":cfg.transport_mode,"request_id":request_id,"binding":source.binding,
        "endpoint":cfg.endpoint,"request_strategy":strategy.version(),
        "configured_model":cfg.model,"actual_model":null,"response_id":null,"service_tier":null,"http_request_id":null,
        "http_status":null,"response_complete":false,"status":"failed","knowledge_status":"pending","published":false,
        "scientific_quality":"not_run","source_statuses":source.units.iter().map(|u|&u.source_status).collect::<Vec<_>>(),"validated_atoms":0,
        "usage":{"network_attempts":0,"real_model_calls":0,
            "paid_calls":0,"price_usd":null,"model_tokenizer":null,
            "model_usage":null,"model_usage_status":"not_validated","input_reserved_tokens":input_reserved,"reserved_tokens":reserved,
            "reservation_policy":"request_utf8_bytes_plus_1024_v1","request_utf8_bytes":body.len()},"response_receipt":null,
        "errors":[],"artifacts":{}});
    let execution = (|| {
        hooks.prepared(&destination, &request)?;
        check()?;
        if cfg.required_input == "vision"
            || !cfg.capabilities.text
            || !cfg.capabilities.structured_output
        {
            return Err(error(
                "EXTRACT_API_CAPABILITY",
                "文本或结构化输出能力不足，或请求需要视觉输入；未调用。",
            ));
        }
        if cfg.budget.max_requests == 0 || reserved > cfg.budget.max_total_tokens {
            return Err(error(
                "EXTRACT_BUDGET_STOP",
                "单次请求次数或预留总量超过明确预算；未调用。",
            ));
        }
        if cfg.budget.max_cost_usd.is_some() {
            return Err(error(
                "EXTRACT_PRICE_UNKNOWN",
                "价格未知，无法证明金额预算；未调用。",
            ));
        }
        source.verify()?;
        selected.verify()?;
        output.verify()?;
        hooks.before_send(&destination, &request)?;
        let http_timeout_ms = if hooks.enabled() {
            let remaining = hooks
                .timeout_ms(cfg.limits.timeout_ms)
                .min(cfg.limits.timeout_ms);
            control.replace(Control::new(remaining, cancelled.clone()));
            remaining
        } else {
            cfg.limits.timeout_ms
        };
        let transport_start = Instant::now();
        let sent = extraction_api_transport::send(
            &cfg.endpoint,
            &bearer,
            &body,
            Duration::from_millis(http_timeout_ms),
            cfg.limits.max_response_bytes as usize,
            &check,
        );
        let elapsed_ms = u64::try_from(transport_start.elapsed().as_millis()).unwrap_or(u64::MAX);
        if hooks.enabled() {
            control.replace(Control::new(hooks.timeout_ms(u64::MAX), cancelled.clone()));
        }
        let receipt = match sent {
            Ok(receipt) => receipt,
            Err(e) => {
                // An outer transport error happens before its network counter
                // can become one. Persist that evidence before exposing a
                // final zero-attempt report to the scheduler.
                if hooks.enabled() {
                    report["response_receipt"] =
                        json!({"sha256":sha256(&[]),"byte_length":0,"omission_reason":null});
                    let receipt = crate::request_budget_model::TransportReceipt {
                        schema_version: 1,
                        producer: "synthetic_api_transport_receipt".into(),
                        request_id: request_id.clone(),
                        http_status: None,
                        http_request_id: None,
                        response_complete: false,
                        network_attempts: 0,
                        response: crate::request_budget_model::FilePin {
                            sha256: sha256(&[]),
                            byte_length: 0,
                        },
                        response_file: None,
                        error_code: Some(e.code().into()),
                        omission_reason: None,
                        elapsed_ms,
                    };
                    save_transport(&mut output, &receipt)?;
                    output.verify()?;
                    hooks.transport_saved(&destination, &receipt)?;
                }
                return Err(e);
            }
        };
        report["usage"]["network_attempts"] = json!(receipt.attempts);
        if cfg.transport_mode == "live" && receipt.attempts > 0 {
            report["usage"]["real_model_calls"] = Value::Null;
            report["usage"]["paid_calls"] = Value::Null;
        }
        report["http_status"] = json!(receipt.status);
        report["response_complete"] = json!(receipt.complete);
        let unsafe_header = receipt
            .request_id
            .as_ref()
            .is_some_and(|id| credential_echo(id.as_bytes(), &bearer));
        let unsafe_body = credential_echo(&receipt.body, &bearer);
        report["response_receipt"] = json!({"sha256":sha256(&receipt.body),"byte_length":receipt.body.len(),
            "omission_reason":if unsafe_header || unsafe_body {Some("omitted_credential_echo")} else {None}});
        if !unsafe_header && !unsafe_body {
            report["http_request_id"] = json!(receipt.request_id);
        }
        if !unsafe_header && !unsafe_body && receipt.attempts > 0 {
            output.add("response.bin", &receipt.body)?;
        }
        if hooks.enabled() {
            let transport = crate::request_budget_model::TransportReceipt {
                schema_version: 1,
                producer: "synthetic_api_transport_receipt".into(),
                request_id: request_id.clone(),
                http_status: receipt.status,
                http_request_id: if unsafe_header || unsafe_body {
                    None
                } else {
                    receipt.request_id.clone()
                },
                response_complete: receipt.complete,
                network_attempts: receipt.attempts,
                response: crate::request_budget_model::FilePin {
                    sha256: sha256(&receipt.body),
                    byte_length: receipt.body.len() as u64,
                },
                response_file: if !unsafe_header && !unsafe_body && receipt.attempts > 0 {
                    Some("response.bin".into())
                } else {
                    None
                },
                error_code: if unsafe_header || unsafe_body {
                    Some("EXTRACT_API_CREDENTIAL_ECHO".into())
                } else {
                    receipt.failure.as_ref().map(|e| e.code().into())
                },
                omission_reason: if unsafe_header || unsafe_body {
                    Some("omitted_credential_echo".into())
                } else {
                    None
                },
                elapsed_ms,
            };
            save_transport(&mut output, &transport)?;
            output.verify()?;
            hooks.transport_saved(&destination, &transport)?;
        }
        if unsafe_header || unsafe_body {
            return Err(echo_error());
        }
        if let Some(fail) = receipt.failure {
            return Err(fail);
        }
        if receipt.complete && receipt.status == Some(200) {
            if let Ok(metadata) =
                extraction_api_wire::response_metadata_with_strategy(&receipt.body, strategy)
            {
                report["actual_model"] = json!(metadata.model);
                report["response_id"] = json!(metadata.id);
                report["service_tier"] = json!(metadata.service_tier);
                report["usage"]["model_usage"] = metadata.usage.clone();
                if let Some(raw_usage) = metadata.raw_usage {
                    report["usage"]["raw_model_usage"] = raw_usage;
                }
                report["usage"]["model_usage_status"] = json!("validated");
                if !usage_within_budget(
                    &metadata.usage,
                    input_reserved,
                    cfg.limits.max_output_tokens,
                    cfg.budget.max_total_tokens,
                ) {
                    return Err(error(
                        "EXTRACT_BUDGET_STOP",
                        "已验证的实际模型用量超过输入预留、输出限制或总预算；不再调用。",
                    ));
                }
            }
        }
        let parsed = extraction_api_wire::decode_with_strategy(&receipt.body, strategy)?;
        if cfg.transport_mode == "live" {
            report["usage"]["real_model_calls"] = json!(1);
        }
        // Reconcile independently decoded metadata before accepting candidates.
        if report["response_id"] != json!(parsed.id)
            || report["actual_model"] != json!(parsed.model)
            || report["service_tier"] != json!(parsed.service_tier)
            || report["usage"]["model_usage"] != parsed.usage
        {
            return Err(error(
                "EXTRACT_API_RESPONSE",
                "候选与独立响应元数据验证不一致。",
            ));
        }
        let candidate = parsed.candidates;
        if candidate.request_id != request_id || candidate.binding != source.binding {
            return Err(error(
                "EXTRACT_SOURCE_BINDING",
                "API候选请求ID或实际来源绑定不一致。",
            ));
        }
        crate::extraction::validate_candidates(
            &source.units,
            &candidate.concepts,
            &candidate.evidence,
            &candidate.atoms,
            &control.borrow(),
            &cancelled,
            signal.as_deref(),
        )?;
        Ok(candidate)
    })();
    let mut errors = Vec::new();
    let mut candidate = match execution {
        Ok(c) => Some(c),
        Err(e) => {
            errors.push(failure(e, bearer));
            None
        }
    };
    for verification in [source.verify(), selected.verify(), output.verify()] {
        if let Err(e) = verification {
            errors.push(failure(e, bearer));
            candidate = None;
        }
    }
    if let Some(candidate) = candidate {
        let publish = (|| {
            output.add("candidates.json",&encoded(&json!({"schema_version":1,"producer":"api","transport_mode":cfg.transport_mode,"request_id":request_id,"binding":source.binding,"payload":candidate}))?)?;
            source.verify()?;
            selected.verify()?;
            output.verify()?;
            report["status"] = json!("candidates_validated");
            report["validated_atoms"] = json!(candidate.atoms.len());
            report["artifacts"] = json!(output.fingerprints());
            let json = save_report(&mut output, &report)?;
            source.verify()?;
            selected.verify()?;
            output.verify()?;
            Ok(json)
        })();
        match publish {
            Ok(json) => {
                hooks.finished(&destination, &report)?;
                return Ok(crate::source::CommandOutput {
                    json,
                    failure: None,
                });
            }
            Err(e) => errors.push(failure(e, bearer)),
        }
    }
    output.discard_result()?;
    output.verify()?;
    report["status"] = json!("failed");
    report["validated_atoms"] = json!(0);
    report["errors"] = json!(errors);
    report["artifacts"] = json!(output.fingerprints());
    let json = save_report(&mut output, &report)?;
    // On a late change, discard only our captured candidate/report capabilities.
    for verification in [source.verify(), selected.verify(), output.verify()] {
        if let Err(e) = verification {
            output.discard_result()?;
            return Err(e);
        }
    }
    hooks.finished(&destination, &report)?;
    let first = &report["errors"][0];
    Ok(crate::source::CommandOutput {
        json,
        failure: Some((
            first["code"].as_str().unwrap_or("EXTRACT_STATE").into(),
            first["message"].as_str().unwrap_or("API抽取失败。").into(),
        )),
    })
}
