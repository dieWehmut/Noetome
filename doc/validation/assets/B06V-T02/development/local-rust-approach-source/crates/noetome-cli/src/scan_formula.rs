//! A single selected scan recognition and independent source-declaration comparison into a fresh Vault.
use crate::{
    extraction_output::{self, OutputRoot},
    formula_model::Profile,
    formula_parser,
    knowledge_coordinate_input::fingerprint,
    process_input::SingleInput,
    project::{self, ProjectError},
    scan_input::{Inputs, check, encoded, error, parse},
    scan_model::*,
    scan_recognition::{self, Capture, Runtime},
    scan_render,
    sidecar_process::Control,
};
use serde::{Serialize, de::DeserializeOwned};
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
            && profile.app == "Obsidian"
            && profile.app_version == "1.6.7"
            && profile.mathjax_version == "3.2.2";
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
pub fn run(case: &Path, profile: &Path, output: &Path) -> Result<ScanOutput, ProjectError> {
    run_inner(case, profile, output, None)
}
type Recognizer = fn(&RecognitionRequest) -> Capture;
fn run_inner(
    case: &Path,
    profile: &Path,
    output: &Path,
    test_recognizer: Option<Recognizer>,
) -> Result<ScanOutput, ProjectError> {
    if test_recognizer.is_some() && !cfg!(test) {
        return Err(error(
            "SCAN_RUNTIME_CONFIG",
            2,
            "synthetic recognition is restricted to internal tests",
        ));
    }
    let control = Control::new(
        scan_recognition::TIMEOUT_MS,
        Arc::new(AtomicBool::new(false)),
    );
    let path = project::check_components(output)?;
    let inputs = Inputs::open(case, &control)?;
    let profile_input = SingleInput::open(profile, &control)?;
    let profile: Profile = parse(&profile_input.bytes)?;
    crate::formula_input::validate_profile(&profile)?;
    if extraction_output::inside(&path, inputs.held.root())?
        || path == inputs.held.root()
        || profile_input.path.starts_with(&path)
    {
        return Err(error(
            "SCAN_OUTPUT_OVERLAP",
            2,
            "output overlaps selected scan attachments or profile",
        ));
    }
    #[cfg(test)]
    stage("inputs_held");
    inputs.verify(&control)?;
    profile_input.verify(&control)?;
    let runtime = if test_recognizer.is_none() {
        Some(Runtime::open(&control)?)
    } else {
        None
    };
    if runtime.is_some() && !cfg!(windows) {
        return Err(error(
            "SCAN_RUNTIME_HOST",
            2,
            "the single-request OCR process tree host currently requires Windows",
        ));
    }
    if let Some(runtime) = &runtime {
        if path.starts_with(runtime.models.root()) {
            return Err(error(
                "SCAN_OUTPUT_OVERLAP",
                2,
                "output overlaps selected OCR model directory",
            ));
        }
    }
    let runtime_inputs = runtime
        .as_ref()
        .map(Runtime::fingerprints)
        .unwrap_or_else(scan_recognition::model_fingerprints);
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut out = OutputRoot::create_scan(&path, &request_id)?;
    let result = (|| {
        let mut originals = BTreeMap::from([
            (
                "source-case.json".to_owned(),
                inputs.bytes("case.json")?.to_vec(),
            ),
            (
                "source-semantics.json".into(),
                inputs.bytes(&inputs.case.semantics.file)?.to_vec(),
            ),
            ("profile.json".into(), profile_input.bytes.clone()),
            (
                "page.png".into(),
                inputs.bytes(&inputs.case.page.file)?.to_vec(),
            ),
            (
                "original.png".into(),
                inputs.bytes(&inputs.case.region.file)?.to_vec(),
            ),
        ]);
        if let Some(pdf) = &inputs.case.pdf {
            originals.insert("original.pdf".into(), inputs.bytes(&pdf.file)?.to_vec());
        }
        let input_fingerprints = originals
            .iter()
            .map(|(name, bytes)| (name.clone(), fingerprint(bytes)))
            .collect();
        for (name, bytes) in originals {
            check(&control)?;
            out.add(&name, &bytes)?;
        }
        let image = fingerprint(inputs.bytes(&inputs.case.region.file)?);
        let request = RecognitionRequest {
            schema_version: 1,
            request_id: request_id.clone(),
            input: RecognitionInput {
                path: inputs
                    .held
                    .root()
                    .join(&inputs.case.region.file)
                    .to_str()
                    .ok_or_else(|| error("SCAN_INPUT", 2, "OCR selected path must be Unicode"))?
                    .into(),
                sha256: image.sha256,
                byte_length: image.byte_length,
            },
            model_directory: runtime
                .as_ref()
                .map(|r| r.models.root().to_string_lossy().into_owned())
                .unwrap_or_else(|| "internal-test-model".into()),
            limits: RecognitionLimits {
                response_bytes: scan_recognition::RESPONSE_BYTES,
                diagnostic_bytes: scan_recognition::DIAGNOSTIC_BYTES,
                timeout_ms: scan_recognition::TIMEOUT_MS,
                max_tex_bytes: 65536,
            },
        };
        let request_bytes = save(&mut out, "recognition-request.json", &request)?;
        #[cfg(test)]
        stage("recognition_request_saved");
        inputs.verify(&control)?;
        profile_input.verify(&control)?;
        out.verify()?;
        let capture = if let Some(recognizer) = test_recognizer {
            recognizer(&request)
        } else {
            runtime
                .as_ref()
                .expect("ordinary runtime prepared")
                .invoke(request_bytes.clone(), &control)?
        };
        out.add("recognition-stderr.bin", &capture.diagnostics)?;
        let response_fingerprint = if let Some(raw) = &capture.raw {
            out.add("raw-response.json", raw)?;
            Some(fingerprint(raw))
        } else {
            None
        };
        let (recognition, recognition_issue) = match capture.raw.as_deref() {
            Some(raw) => match scan_recognition::response(raw, &request) {
                Ok(r) => (Some(r), None),
                Err(e) => (None, Some(e.code().to_string())),
            },
            None => (
                None,
                Some(
                    capture
                        .transport_issue
                        .unwrap_or_else(|| "SCAN_RECOGNITION_TRANSPORT".into()),
                ),
            ),
        };
        if let Some(raw_tex) = recognition
            .as_ref()
            .and_then(|r| r.recognized_tex.as_deref())
        {
            out.add("recognized.tex", raw_tex.as_bytes())?;
        }
        #[cfg(test)]
        stage("recognition_saved");
        inputs.verify(&control)?;
        profile_input.verify(&control)?;
        if let Some(runtime) = &runtime {
            runtime.verify(&control)?;
        }
        let evaluation = evaluate(
            &inputs,
            &profile,
            recognition.as_ref(),
            recognition_issue.as_deref(),
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
            .is_some_and(|r| r.status == "recognized");
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
            network_calls: 0,
            paid_calls: 0,
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
            recognition_invocations: 1,
            recognition_request_fingerprint: fingerprint(&request_bytes),
            recognition_response_fingerprint: response_fingerprint,
            runtime_inputs,
            tokens: evaluation.tokens,
            plan_fingerprint,
            inputs: input_fingerprints,
            files,
            artifacts: out.fingerprints(),
        };
        let report_bytes = save(&mut out, "scan.json", &report)?;
        let json = String::from_utf8(report_bytes).map_err(|e| error("SCAN_SCHEMA", 3, e))?;
        let markdown = format!(
            "# 选定扫描公式重建预览报告\n\n状态：{}；预览：{}；知识：pending；科学质量：not_run；发布：false。\n\n仅明确选定扫描区域的本地识别与语义核对；原表达并非作者 TeX；网络与付费调用：0。\n\n```json\n{json}\n```\n",
            report.status, report.preview_status
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
        if let Some(runtime) = &runtime {
            runtime.verify(&control)?;
        }
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
