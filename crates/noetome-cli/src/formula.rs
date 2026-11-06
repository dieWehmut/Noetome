//! Deterministic fidelity of explicitly selected formula attachments into a fresh test Vault.
use crate::{
    extraction_output::{self, OutputRoot},
    formula_input::{self, Inputs, check, encoded, error, parse},
    formula_model::*,
    formula_parser, formula_render,
    knowledge_coordinate_input::fingerprint,
    process_input::SingleInput,
    project::{self, ProjectError},
    sidecar_process::Control,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

pub struct FormulaOutput {
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
#[path = "../tests/support/formula_guard_tests.rs"]
mod guard_tests;

struct Evaluation {
    decision: Decision,
    tokens: Option<Vec<String>>,
    compiled_expression: Option<String>,
    expression_match: bool,
    symbol_definitions: bool,
}
fn evaluate(inputs: &Inputs, profile: &Profile) -> Result<Evaluation, ProjectError> {
    let mut decision = Decision {
        representation: None,
        fallback_reasons: vec![],
        compatibility_rewrites: vec![],
        issue_codes: vec![],
    };
    let mut evaluation = Evaluation {
        decision: decision.clone(),
        tokens: None,
        compiled_expression: None,
        expression_match: false,
        symbol_definitions: false,
    };
    let mut source_uses_dfrac = false;
    if inputs.case.source_kind == SourceKind::ExperimentalCurve {
        decision.issue_codes.push(
            if inputs.case.data_available {
                "FORMULA_CURVE_UNSUPPORTED"
            } else {
                "FORMULA_CURVE_NO_DATA"
            }
            .into(),
        );
        evaluation.decision = decision;
        return Ok(evaluation);
    }
    let parsed = match &inputs.semantics.expression {
        None => {
            decision
                .issue_codes
                .push("FORMULA_SEMANTICS_UNKNOWN".into());
            None
        }
        Some(expression) => match formula_parser::parse(expression) {
            Ok(parsed) => Some(parsed),
            Err(_) => {
                decision
                    .issue_codes
                    .push("FORMULA_UNSUPPORTED_SYNTAX".into());
                None
            }
        },
    };
    if let Some(parsed) = &parsed {
        evaluation.tokens = Some(parsed.tokens.clone());
        let actual_names: BTreeSet<String> = inputs
            .semantics
            .symbols
            .iter()
            .map(|s| s.symbol.clone())
            .collect();
        let names_match =
            actual_names == parsed.symbols && actual_names.len() == inputs.semantics.symbols.len();
        if !names_match {
            decision.issue_codes.push("FORMULA_SYMBOLS_MISMATCH".into());
        }
        let confirmed = inputs
            .semantics
            .symbols
            .iter()
            .all(|s| s.definition.is_some());
        if !confirmed {
            decision.issue_codes.push("FORMULA_UNKNOWN_SYMBOL".into());
        }
        evaluation.symbol_definitions = names_match && confirmed;
        if let Some(tex) = &inputs.case.tex {
            match std::str::from_utf8(inputs.bytes(&tex.file)?)
                .ok()
                .and_then(|s| formula_parser::parse(s).ok())
            {
                None => decision
                    .issue_codes
                    .push("FORMULA_UNSUPPORTED_SYNTAX".into()),
                Some(source) if source.tokens != parsed.tokens => decision
                    .issue_codes
                    .push("FORMULA_TEX_SEMANTIC_MISMATCH".into()),
                Some(source) => {
                    evaluation.expression_match = true;
                    source_uses_dfrac = source.uses_dfrac;
                }
            }
        } else {
            // A selected region has explicit source-author annotation, never recovered author TeX.
            evaluation.expression_match = true;
        }
    }
    if decision.issue_codes.is_empty() {
        let native = profile.native_math
            && profile.verified
            && crate::visual_capabilities::math(
                &profile.app,
                &profile.app_version,
                &profile.mathjax_version,
            );
        let fallback = if inputs.case.source_kind == SourceKind::FormulaRegion {
            Some("FORMULA_REGION_ONLY")
        } else if !native {
            Some("FORMULA_NATIVE_UNAVAILABLE")
        } else if source_uses_dfrac && !profile.compatibility_dfrac {
            // This profile did not authorize our controlled normalization.
            // It does not assert that MathJax itself cannot render dfrac.
            Some("FORMULA_DFRAC_UNAVAILABLE")
        } else {
            None
        };
        if let Some(reason) = fallback {
            if !profile.local_images || inputs.case.region.is_none() {
                decision
                    .issue_codes
                    .push("FORMULA_IMAGE_UNAVAILABLE".into());
            } else if inputs
                .case
                .region
                .as_ref()
                .is_some_and(|r| r.legibility != Legibility::Clear)
            {
                decision
                    .issue_codes
                    .push("FORMULA_REGION_UNREADABLE".into());
            } else {
                decision.representation = Some(Representation::FormulaImage);
                decision.fallback_reasons.push(reason.into());
            }
        } else {
            let source = inputs
                .case
                .tex
                .as_ref()
                .expect("native source is original TeX");
            let original = std::str::from_utf8(inputs.bytes(&source.file)?)
                .map_err(|e| error("FORMULA_UNSUPPORTED_SYNTAX", 3, e))?;
            let mut expression = original.trim_matches([' ', '\t', '\r', '\n']).to_owned();
            if source_uses_dfrac {
                expression = expression.replace("\\dfrac", "\\frac");
                decision
                    .compatibility_rewrites
                    .push("FORMULA_DFRAC_COMPATIBILITY".into());
            }
            if formula_parser::parse(&expression).map(|p| p.tokens).ok() != evaluation.tokens {
                return Err(error(
                    "FORMULA_COMPILED_SEMANTICS",
                    3,
                    "compiled expression changed source semantics",
                ));
            }
            decision.representation = Some(Representation::NativeMath);
            evaluation.compiled_expression = Some(expression);
        }
    }
    evaluation.decision = decision;
    Ok(evaluation)
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
            "FORMULA_OUTPUT_CHANGED",
            3,
            "saved value differs from validated bytes",
        ));
    }
    Ok(bytes)
}
pub fn run(case: &Path, profile: &Path, output: &Path) -> Result<FormulaOutput, ProjectError> {
    let control = Control::new(30000, Arc::new(AtomicBool::new(false)));
    let path = project::check_components(output)?;
    let inputs = Inputs::open(case, &control)?;
    let profile_input = SingleInput::open(profile, &control)?;
    let profile: Profile = parse(&profile_input.bytes)?;
    formula_input::validate_profile(&profile)?;
    if extraction_output::inside(&path, inputs.held.root())?
        || path == inputs.held.root()
        || profile_input.path.starts_with(&path)
    {
        return Err(error(
            "FORMULA_OUTPUT_OVERLAP",
            2,
            "output overlaps selected source attachments or profile",
        ));
    }
    #[cfg(test)]
    stage("inputs_held");
    inputs.verify(&control)?;
    profile_input.verify(&control)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut out = OutputRoot::create_formula(&path, &request_id)?;
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
        ]);
        if let Some(tex) = &inputs.case.tex {
            originals.insert("original.tex".into(), inputs.bytes(&tex.file)?.to_vec());
        }
        if let Some(region) = &inputs.case.region {
            originals.insert("original.png".into(), inputs.bytes(&region.file)?.to_vec());
        }
        let input_fingerprints = originals
            .iter()
            .map(|(name, bytes)| (name.clone(), fingerprint(bytes)))
            .collect();
        for (name, bytes) in originals {
            check(&control)?;
            out.add(&name, &bytes)?;
        }
        let evaluation = evaluate(&inputs, &profile)?;
        let ready = evaluation.decision.representation.is_some();
        let mut files = vec![];
        let mut plan_fingerprint = None;
        if ready {
            let plan = FormulaPlan {
                schema_version: 1,
                producer: "knowledge_formula".into(),
                scope: "selected_formula_fidelity_v1".into(),
                request_id: request_id.clone(),
                case: inputs.case.clone(),
                semantics: inputs.semantics.clone(),
                decision: evaluation.decision.clone(),
                tokens: evaluation
                    .tokens
                    .clone()
                    .expect("ready formula has validated tokens"),
                compiled_expression: evaluation.compiled_expression.clone(),
            };
            let plan_bytes = save(&mut out, "plan.json", &plan)?;
            let plan: FormulaPlan = parse(&plan_bytes)?;
            plan_fingerprint = Some(fingerprint(&plan_bytes));
            if let Some(expression) = &plan.compiled_expression {
                out.add("compiled.tex", expression.as_bytes())?;
            }
            let (bytes, file) = formula_render::render(&plan);
            out.add(&file.relative_path, &bytes)?;
            let actual = out.read(&file.relative_path)?;
            if actual != bytes
                || fingerprint(&actual).sha256 != file.content_sha256
                || fingerprint(&actual).byte_length != file.byte_length
            {
                return Err(error("FORMULA_RENDER_CHANGED", 3, "rendered note changed"));
            }
            for mapping in &file.mappings {
                let slice = actual
                    .get(mapping.start_byte as usize..mapping.end_byte as usize)
                    .ok_or_else(|| {
                        error("FORMULA_RENDER_MAPPING", 3, "rendered mapping outside note")
                    })?;
                std::str::from_utf8(slice).map_err(|e| error("FORMULA_RENDER_MAPPING", 3, e))?;
            }
            files.push(file);
        }
        let report = FormulaReport {
            schema_version: 1,
            producer: "knowledge_formula".into(),
            scope: "selected_formula_fidelity_v1".into(),
            request_id: request_id.clone(),
            status: if ready { "passed" } else { "needs_review" }.into(),
            preview_status: if ready { "rendered" } else { "not_run" }.into(),
            knowledge_status: "pending".into(),
            scientific_quality: "not_run".into(),
            published: false,
            network_calls: 0,
            paid_calls: 0,
            case: inputs.case.clone(),
            semantics: inputs.semantics.clone(),
            profile: profile.clone(),
            checks: Checks {
                source_preservation: "passed".into(),
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
            tokens: evaluation.tokens,
            plan_fingerprint,
            inputs: input_fingerprints,
            files,
            artifacts: out.fingerprints(),
        };
        let report_bytes = save(&mut out, "formula.json", &report)?;
        let json = String::from_utf8(report_bytes).map_err(|e| error("FORMULA_SCHEMA", 3, e))?;
        let markdown = format!(
            "# 选定公式保真预览报告\n\n状态：{}；预览：{}；知识：pending；科学质量：not_run；发布：false。\n\n仅选定公式/原区域工程核对；网络与付费调用：0。\n\n```json\n{json}\n```\n",
            report.status, report.preview_status
        );
        out.add("formula.md", markdown.as_bytes())?;
        if out.read("formula.md")? != markdown.as_bytes() {
            return Err(error(
                "FORMULA_OUTPUT_CHANGED",
                3,
                "saved Markdown report differs",
            ));
        }
        #[cfg(test)]
        stage("results_saved");
        inputs.verify(&control)?;
        profile_input.verify(&control)?;
        out.verify()?;
        check(&control)?;
        Ok(FormulaOutput {
            json: format!("{json}\n"),
            failed: !ready,
        })
    })();
    if result.is_err() {
        out.discard_formula_result()?;
    }
    result
}
