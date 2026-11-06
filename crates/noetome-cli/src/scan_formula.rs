//! A single selected scan recognition and independent source-declaration comparison into a fresh Vault.
use crate::{
    extraction_output::{self, OutputRoot},
    formula_model::Profile,
    formula_parser,
    knowledge_coordinate_input::fingerprint,
    process_input::SingleInput,
    project::{self, ProjectError},
    scan_api::{self, SelectedConfig},
    scan_input::{Inputs, check, encoded, error, parse},
    scan_model::*,
    scan_render,
    sidecar_process::Control,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};
pub struct ScanOutput {
    pub json: String,
    pub failed: bool,
}
#[cfg(test)]
thread_local! { pub(crate) static TEST_HOOK:std::cell::RefCell<Option<Box<dyn FnMut(&str)>>> = std::cell::RefCell::new(None); }
#[cfg(test)]
fn stage(at: &str) {
    TEST_HOOK.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() {
            hook(at);
        }
    });
}
#[cfg(test)]
#[path = "../tests/support/scan_guard_tests.rs"]
mod guard_tests;

/// A script consisting of one scalar token has the same meaning with or without its TeX grouping braces.
/// Other grouping, operand order, signs and fraction argument delimiters remain unchanged.
pub(crate) fn canonical(
    expression: &str,
) -> Result<(Vec<String>, BTreeSet<String>, bool), &'static str> {
    let parsed = formula_parser::parse(expression)?;
    let mut tokens = vec![];
    let mut at = 0;
    let mut script_braces = false;
    while at < parsed.tokens.len() {
        if matches!(parsed.tokens[at].as_str(), "_" | "^")
            && parsed.tokens.get(at + 1).is_some_and(|s| s == "{")
            && parsed.tokens.get(at + 3).is_some_and(|s| s == "}")
        {
            let scalar = &parsed.tokens[at + 2];
            if scalar.len() == 1 && scalar.as_bytes()[0].is_ascii_alphanumeric()
                || matches!(scalar.as_str(), "\\alpha" | "\\beta" | "\\gamma")
            {
                tokens.extend([parsed.tokens[at].clone(), scalar.clone()]);
                at += 4;
                script_braces = true;
                continue;
            }
        }
        tokens.push(parsed.tokens[at].clone());
        at += 1;
    }
    Ok((tokens, parsed.symbols, script_braces))
}
struct Evaluation {
    decision: Decision,
    tokens: Option<Vec<String>>,
    compiled_expression: Option<String>,
    expression_match: bool,
    symbol_definitions: bool,
}
fn evaluate(
    inputs: &Inputs,
    profile: &Profile,
    recognized: Option<&RecognitionResponse>,
    recognition_issue: Option<&str>,
) -> Evaluation {
    let mut result = Evaluation {
        decision: Decision {
            representation: None,
            fallback_reasons: vec![],
            compatibility_rewrites: vec![],
            issue_codes: vec![],
        },
        tokens: None,
        compiled_expression: None,
        expression_match: false,
        symbol_definitions: false,
    };
    if let Some(issue) = recognition_issue {
        result.decision.issue_codes.push(issue.into());
    }
    if inputs.case.region.legibility != Legibility::Clear {
        result
            .decision
            .issue_codes
            .push("SCAN_REGION_UNREADABLE".into());
    }
    if !inputs.semantics.uncertainties.is_empty() {
        result
            .decision
            .issue_codes
            .push("SCAN_CRITICAL_SYMBOL_UNKNOWN".into());
    }
    let source = match inputs.semantics.expression.as_deref() {
        None => {
            result
                .decision
                .issue_codes
                .push("SCAN_SEMANTICS_UNKNOWN".into());
            None
        }
        Some(e) => match canonical(e) {
            Ok(s) => Some(s),
            Err(_) => {
                result
                    .decision
                    .issue_codes
                    .push("SCAN_UNSUPPORTED_SYNTAX".into());
                None
            }
        },
    };
    if let Some((source_tokens, source_symbols, _)) = &source {
        result.tokens = Some(source_tokens.clone());
        let names = inputs
            .semantics
            .symbols
            .iter()
            .map(|s| s.symbol.clone())
            .collect::<BTreeSet<_>>();
        if names != *source_symbols || names.len() != inputs.semantics.symbols.len() {
            result
                .decision
                .issue_codes
                .push("SCAN_SYMBOLS_MISMATCH".into());
        }
        if inputs
            .semantics
            .symbols
            .iter()
            .any(|s| s.definition.is_none())
        {
            result
                .decision
                .issue_codes
                .push("SCAN_UNKNOWN_SYMBOL".into());
        }
        result.symbol_definitions = names == *source_symbols
            && names.len() == inputs.semantics.symbols.len()
            && inputs
                .semantics
                .symbols
                .iter()
                .all(|s| s.definition.is_some());
    }
    if recognized.is_some_and(|r| !r.uncertainties.is_empty()) {
        result
            .decision
            .issue_codes
            .push("SCAN_RECOGNITION_UNCERTAIN".into());
    }
    let raw_tex = recognized.and_then(|r| r.recognized_tex.as_deref());
    let recognized_tokens = match raw_tex {
        None => {
            if recognition_issue.is_none() {
                result
                    .decision
                    .issue_codes
                    .push("SCAN_RECOGNITION_FAILED".into());
            }
            None
        }
        Some(e) => match canonical(e) {
            Ok(s) => Some(s),
            Err(_) => {
                result
                    .decision
                    .issue_codes
                    .push("SCAN_UNSUPPORTED_RECOGNITION".into());
                None
            }
        },
    };
    if let (Some((source_tokens, _, _)), Some((ocr_tokens, _, _))) = (&source, &recognized_tokens) {
        if source_tokens == ocr_tokens {
            result.expression_match = true;
        } else {
            result
                .decision
                .issue_codes
                .push("SCAN_RECOGNITION_SEMANTIC_MISMATCH".into());
        }
    }
    if result.decision.issue_codes.is_empty() {
        let raw = raw_tex.expect("recognized expression checked");
        let uses_dfrac = formula_parser::parse(raw).is_ok_and(|p| p.uses_dfrac);
        let native = profile.native_math
            && profile.verified
            && crate::visual_capabilities::math(
                &profile.app,
                &profile.app_version,
                &profile.mathjax_version,
            );
        let fallback = if !native {
            Some("SCAN_NATIVE_UNAVAILABLE")
        } else if uses_dfrac && !profile.compatibility_dfrac {
            Some("SCAN_DFRAC_UNAVAILABLE")
        } else {
            None
        };
        if let Some(reason) = fallback {
            if !profile.local_images {
                result
                    .decision
                    .issue_codes
                    .push("SCAN_IMAGE_UNAVAILABLE".into());
            } else {
                result.decision.representation = Some(Representation::FormulaImage);
                result.decision.fallback_reasons.push(reason.into());
            }
        } else {
            let (tokens, _, script_braces) =
                recognized_tokens.expect("recognized expression checked");
            if uses_dfrac {
                result
                    .decision
                    .compatibility_rewrites
                    .push("SCAN_DFRAC_COMPATIBILITY".into());
            }
            if script_braces {
                result
                    .decision
                    .compatibility_rewrites
                    .push("SCAN_SINGLE_SCRIPT_GROUP_COMPATIBILITY".into());
            }
            let compiled = tokens.concat();
            if canonical(&compiled).map(|p| p.0).ok() != result.tokens {
                result
                    .decision
                    .issue_codes
                    .push("SCAN_COMPILED_SEMANTICS".into());
            } else {
                result.decision.representation = Some(Representation::NativeMath);
                result.compiled_expression = Some(compiled);
            }
        }
    }
    result
}
fn save<T: Serialize + DeserializeOwned>(
    out: &mut OutputRoot,
    name: &str,
    value: &T,
) -> Result<Vec<u8>, ProjectError> {
    let bytes = encoded(value)?;
    out.add(name, &bytes)?;
    let actual = out.read(name)?;
    let decoded: T = parse(&actual)?;
    if actual != bytes || encoded(&decoded)? != bytes {
        return Err(error(
            "SCAN_OUTPUT_CHANGED",
            3,
            "saved object differs from validated bytes",
        ));
    }
    Ok(bytes)
}
pub fn run(
    case: &Path,
    api_config: &Path,
    profile: &Path,
    output: &Path,
) -> Result<ScanOutput, ProjectError> {
    run_inner(case, api_config, profile, output, None)
}
fn run_inner(
    case: &Path,
    api_config: &Path,
    profile: &Path,
    output: &Path,
    test_bearer: Option<&str>,
) -> Result<ScanOutput, ProjectError> {
    let control = Control::new(120000, Arc::new(AtomicBool::new(false)));
    let path = project::check_components(output)?;
    let inputs = Inputs::open(case, &control)?;
    let selected = SelectedConfig::open(api_config, &control)?;
    let cfg = &selected.config;
    if test_bearer.is_some()
        && (!cfg!(test)
            || cfg.transport_mode != "local_http_test"
            || test_bearer.is_none_or(|s| !s.starts_with("noetome-test-")))
    {
        return Err(error(
            "SCAN_API_CREDENTIAL",
            2,
            "internal bearer override is restricted to artificial loopback tests",
        ));
    }
    let profile_input = SingleInput::open(profile, &control)?;
    let profile: Profile = parse(&profile_input.bytes)?;
    crate::formula_input::validate_profile(&profile)?;
    if extraction_output::inside(&path, inputs.held.root())?
        || path == inputs.held.root()
        || profile_input.path.starts_with(&path)
        || selected.input.path.starts_with(&path)
    {
        return Err(error(
            "SCAN_OUTPUT_OVERLAP",
            2,
            "output overlaps selected scan attachments, profile or API config",
        ));
    }
    #[cfg(test)]
    stage("inputs_held");
    inputs.verify(&control)?;
    profile_input.verify(&control)?;
    selected.verify(&control)?;
    let image = inputs.bytes(&inputs.case.region.file)?;
    let body = scan_api::request_body(cfg, image)?;
    let input_reserved = body.len() as u64 + 1024;
    let reserved = input_reserved
        .checked_add(cfg.limits.max_output_tokens)
        .ok_or_else(|| error("SCAN_BUDGET_STOP", 3, "reservation overflow"))?;
    let permitted = cfg.budget.max_requests == 1
        && cfg.capabilities.vision
        && cfg.capabilities.structured_output
        && cfg.budget.max_cost_usd.is_none()
        && body.len() as u64 <= cfg.limits.max_input_bytes
        && reserved <= cfg.budget.max_total_tokens;
    let bearer = if permitted {
        Some(if let Some(value) = test_bearer {
            value.to_owned()
        } else {
            selected.credential()?
        })
    } else {
        None
    };
    let request_id = uuid::Uuid::new_v4().to_string();
    let request = json!({"schema_version":1,"producer":"vision_scan_request","request_id":request_id,"request_strategy":scan_api::STRATEGY,"provider":cfg.provider,"transport_mode":cfg.transport_mode,"endpoint":cfg.endpoint,"configured_model":cfg.model,"credential_env":cfg.credential_env,"input":{"path":inputs.held.root().join(&inputs.case.region.file).to_string_lossy(),"sha256":fingerprint(image).sha256,"byte_length":image.len(),"dimensions":[inputs.case.region.bounds[2],inputs.case.region.bounds[3]]},"config":{"path":selected.input.path.to_string_lossy(),"fingerprint":fingerprint(&selected.input.bytes)},"limits":cfg.limits,"budget":cfg.budget,"reservation_policy":"request_utf8_bytes_plus_1024_v1","input_reserved_tokens":input_reserved,"reserved_tokens":reserved,"model_tokenizer":null,"request_body":fingerprint(&body)});
    let request_bytes = encoded(&request)?;
    if bearer.as_ref().is_some_and(|key| {
        crate::extraction_api::credential_echo(&body, key)
            || crate::extraction_api::credential_echo(&request_bytes, key)
    }) {
        return Err(error(
            "SCAN_API_CREDENTIAL_ECHO",
            3,
            "request unexpectedly contains selected credential; no output or request",
        ));
    }
    let mut originals = BTreeMap::from([
        (
            "source-case.json".to_owned(),
            inputs.bytes("case.json")?.to_vec(),
        ),
        (
            "source-semantics.json".into(),
            inputs.bytes(&inputs.case.semantics.file)?.to_vec(),
        ),
        (
            "source-api-config.json".into(),
            selected.input.bytes.clone(),
        ),
        ("profile.json".into(), profile_input.bytes.clone()),
        (
            "page.png".into(),
            inputs.bytes(&inputs.case.page.file)?.to_vec(),
        ),
        ("original.png".into(), image.to_vec()),
    ]);
    if let Some(pdf) = &inputs.case.pdf {
        originals.insert("original.pdf".into(), inputs.bytes(&pdf.file)?.to_vec());
    }
    if bearer.as_ref().is_some_and(|key| {
        originals
            .values()
            .any(|b| crate::extraction_api::credential_echo(b, key))
    }) {
        return Err(error(
            "SCAN_API_CREDENTIAL_ECHO",
            3,
            "selected source or config contains the credential; no persistent output or request",
        ));
    }
    let mut out = OutputRoot::create_scan(&path, &request_id)?;
    let result = (|| {
        let input_fingerprints = originals
            .iter()
            .map(|(name, bytes)| (name.clone(), fingerprint(bytes)))
            .collect();
        for (name, bytes) in originals {
            check(&control)?;
            out.add(&name, &bytes)?;
        }
        out.add("request.json", &request_bytes)?;
        out.add("request-body.json", &body)?;
        #[cfg(test)]
        stage("recognition_request_saved");
        let held_check = || {
            inputs.verify(&control)?;
            profile_input.verify(&control)?;
            selected.verify(&control)?;
            out.verify()?;
            Ok(())
        };
        let exchange = scan_api::invoke(cfg, bearer.as_deref(), &body, &held_check)?;
        let response_fingerprint=exchange.api["response_receipt"].as_object().and_then(|_|serde_json::from_value::<crate::sidecar_model::Fingerprint>(json!({"sha256":exchange.api["response_receipt"]["sha256"],"byte_length":exchange.api["response_receipt"]["byte_length"]})).ok());
        if let Some(raw) = &exchange.raw {
            out.add("response.bin", raw)?;
        }
        let recognition = exchange.recognition;
        if let Some(r) = &recognition {
            save(&mut out, "recognition.json", r)?;
            if let Some(raw_tex) = &r.recognized_tex {
                out.add("recognized.tex", raw_tex.as_bytes())?;
            }
        }
        #[cfg(test)]
        stage("recognition_saved");
        inputs.verify(&control)?;
        profile_input.verify(&control)?;
        selected.verify(&control)?;
        let evaluation = evaluate(
            &inputs,
            &profile,
            recognition.as_ref(),
            exchange.issue.as_deref(),
        );
        let ready = evaluation.decision.representation.is_some()
            && evaluation.decision.issue_codes.is_empty();
        let mut files = vec![];
        let plan_fingerprint = if ready {
            let plan = ScanPlan {
                schema_version: 1,
                producer: "knowledge_scan_formula".into(),
                scope: "selected_scan_formula_v1".into(),
                request_id: request_id.clone(),
                case: inputs.case.clone(),
                semantics: inputs.semantics.clone(),
                decision: evaluation.decision.clone(),
                tokens: evaluation.tokens.clone().expect("source semantics checked"),
                recognized_expression: recognition
                    .as_ref()
                    .and_then(|r| r.recognized_tex.clone())
                    .expect("raw recognition checked"),
                compiled_expression: evaluation.compiled_expression.clone(),
            };
            let bytes = save(&mut out, "plan.json", &plan)?;
            if let Some(expression) = &plan.compiled_expression {
                out.add("compiled.tex", expression.as_bytes())?;
            }
            let (note, file) = scan_render::render(&plan);
            out.add(&file.relative_path, &note)?;
            if out.read(&file.relative_path)? != note {
                return Err(error("SCAN_OUTPUT_CHANGED", 3, "saved note differs"));
            }
            files.push(file);
            Some(fingerprint(&bytes))
        } else {
            None
        };
        let recognition_ok = recognition
            .as_ref()
            .is_some_and(|r| r.recognized_tex.is_some() && r.uncertainties.is_empty());
        let network_calls = exchange.api["usage"]["network_attempts"]
            .as_u64()
            .unwrap_or(0) as u32;
        let paid_calls = exchange.api["usage"]["paid_calls"]
            .as_u64()
            .map(|n| n as u32);
        let report = ScanReport {
            schema_version: 1,
            producer: "knowledge_scan_formula".into(),
            scope: "selected_scan_formula_v1".into(),
            request_id,
            status: if ready {
                "preview_ready"
            } else {
                "needs_review"
            }
            .into(),
            preview_status: if ready { "saved" } else { "blocked" }.into(),
            knowledge_status: "pending".into(),
            scientific_quality: "not_run".into(),
            published: false,
            network_calls,
            paid_calls,
            case: inputs.case.clone(),
            semantics: inputs.semantics.clone(),
            profile,
            checks: Checks {
                source_preservation: "passed".into(),
                region_match: "passed".into(),
                recognition: if recognition_ok {
                    "passed"
                } else {
                    "needs_review"
                }
                .into(),
                expression_match: if evaluation.expression_match {
                    "passed"
                } else {
                    "needs_review"
                }
                .into(),
                symbol_definitions: if evaluation.symbol_definitions {
                    "passed"
                } else {
                    "needs_review"
                }
                .into(),
                semantic_review: if ready { "passed" } else { "needs_review" }.into(),
                note_plan: if ready { "passed" } else { "not_run" }.into(),
                rendering: if ready { "passed" } else { "not_run" }.into(),
                saved_output: "passed".into(),
            },
            decision: evaluation.decision,
            recognition,
            api: exchange.api,
            recognition_request_fingerprint: fingerprint(&body),
            recognition_response_fingerprint: response_fingerprint,
            tokens: evaluation.tokens,
            plan_fingerprint,
            inputs: input_fingerprints,
            files,
            artifacts: out.fingerprints(),
        };
        let report_bytes = save(&mut out, "scan.json", &report)?;
        let json = String::from_utf8(report_bytes).map_err(|e| error("SCAN_SCHEMA", 3, e))?;
        let markdown = format!(
            "# 选定扫描公式重建预览报告\n\n状态：{}；预览：{}；知识：pending；科学质量：not_run；发布：false。\n\n识别模式：{}；实际网络尝试：{}。价格、Token tokenizer 和账单未知；原始 TeX 未知。\n\n选定原扫描区域、单请求正文、实际 HTTP 原响应、已核实用量与语义核对分开保存；本地人工 HTTP 服务只验证工程契约。\n\n```json\n{json}\n```\n",
            report.status,
            report.preview_status,
            report.api["transport_mode"].as_str().unwrap_or("unknown"),
            network_calls
        );
        out.add("scan.md", markdown.as_bytes())?;
        if out.read("scan.md")? != markdown.as_bytes() {
            return Err(error(
                "SCAN_OUTPUT_CHANGED",
                3,
                "saved Markdown report differs",
            ));
        }
        #[cfg(test)]
        stage("results_saved");
        inputs.verify(&control)?;
        profile_input.verify(&control)?;
        selected.verify(&control)?;
        out.verify()?;
        check(&control)?;
        Ok(ScanOutput {
            json: format!("{json}\n"),
            failed: !ready,
        })
    })();
    if result.is_err() {
        out.discard_scan_result()?;
    }
    result
}
