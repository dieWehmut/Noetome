//! Actual artificial HTTP and capability probes; these are not real model measurements.
use super::*;
#[path = "scan_api_support.rs"]
mod support;
use serde_json::{Value, json};
use std::{fs, sync::Mutex};
use support::{Case, KEY, Server, read, repo, response, write};
impl Case {
    fn run(&self) -> Result<ScanOutput, ProjectError> {
        run_inner(
            &self.case,
            &self.api,
            &self.profile,
            &self.output,
            Some(KEY),
        )
    }
    fn execute(&self, value: Value) -> Result<ScanOutput, ProjectError> {
        let s = Server::new(value);
        self.activate(&s);
        let result = self.run();
        s.finish();
        result
    }
    fn with_hook(&self, hook: impl FnMut(&str) + 'static) -> Result<ScanOutput, ProjectError> {
        let s = Server::new(response(Some("x_{i}=-\\frac{a_{i}}{b_{i}}")));
        self.activate(&s);
        TEST_HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
        let result = self.run();
        TEST_HOOK.with(|h| *h.borrow_mut() = None);
        if s.accepted.load(std::sync::atomic::Ordering::SeqCst) {
            s.finish();
        }
        result
    }
    fn no_success(&self) {
        for n in [
            "scan.json",
            "scan.md",
            "plan.json",
            "compiled.tex",
            "扫描公式预览.md",
        ] {
            assert!(!self.output.join(n).exists(), "{n}");
        }
    }
}
#[test]
fn canonical_scripts_keep_every_sign_variable_and_fraction_role() {
    let want = canonical("x_i=-\\frac{a_i}{b_i}").unwrap().0;
    assert_eq!(want, canonical("x_{i}=-\\dfrac{a_{i}}{b_{i}}").unwrap().0);
    for wrong in [
        "x_j=-\\frac{a_i}{b_i}",
        "x_i=\\frac{a_i}{b_i}",
        "x_i=-\\frac{b_i}{a_i}",
        "x_i=-\\frac{a^i}{b_i}",
    ] {
        assert_ne!(want, canonical(wrong).unwrap().0);
    }
    assert!(canonical("x_i=-\\frac{\\bar{a_i}}{b_i}").is_err());
}
#[test]
fn artificial_http_success_preserves_raw_tex_and_single_script_rewrite() {
    let c = Case::new("clear");
    assert!(
        !c.execute(response(Some("x_{i}=-\\frac{a_{i}}{b_{i}}")))
            .unwrap()
            .failed
    );
    let r = read(c.output.join("scan.json"));
    assert_eq!(r["network_calls"], 1);
    assert_eq!(r["api"]["usage"]["real_model_calls"], 0);
    assert_eq!(r["decision"]["representation"], "native_math");
    assert_eq!(
        fs::read_to_string(c.output.join("recognized.tex")).unwrap(),
        "x_{i}=-\\frac{a_{i}}{b_{i}}"
    );
    assert_eq!(
        fs::read_to_string(c.output.join("compiled.tex")).unwrap(),
        "x_i=-\\frac{a_i}{b_i}"
    );
    assert_eq!(
        r["decision"]["compatibility_rewrites"],
        json!(["SCAN_SINGLE_SCRIPT_GROUP_COMPATIBILITY"])
    );
}
#[test]
fn guessed_clear_answer_does_not_resolve_unknown_source_sign_or_subscript() {
    for name in ["unreadable_sign", "unreadable_subscript"] {
        let c = Case::new(name);
        assert!(
            c.execute(response(Some("x_i=-\\frac{a_i}{b_i}")))
                .unwrap()
                .failed
        );
        let r = read(c.output.join("scan.json"));
        assert!(
            r["decision"]["issue_codes"]
                .as_array()
                .unwrap()
                .contains(&json!("SCAN_CRITICAL_SYMBOL_UNKNOWN"))
        );
        assert!(!c.output.join("plan.json").exists());
        assert!(!c.output.join("compiled.tex").exists());
        assert!(!c.output.join("扫描公式预览.md").exists());
        assert!(c.output.join("response.bin").exists());
    }
}
#[test]
fn wrong_sign_and_extra_bar_are_not_repaired() {
    for text in ["x_i=\\frac{a_i}{b_i}", "x_i=-\\frac{\\bar{a_i}}{b_i}"] {
        let c = Case::new("clear");
        assert!(c.execute(response(Some(text))).unwrap().failed);
        assert!(!c.output.join("compiled.tex").exists());
        assert_eq!(
            fs::read_to_string(c.output.join("recognized.tex")).unwrap(),
            text
        );
    }
}
#[test]
fn recognized_uncertainty_blocks_even_matching_expression() {
    let c = Case::new("clear");
    let mut v = response(Some("x_i=-\\frac{a_i}{b_i}"));
    v["choices"][0]["message"]["content"]=json!(json!({"schema_version":1,"recognized_tex":"x_i=-\\frac{a_i}{b_i}","uncertainties":[{"kind":"sign","description":"Uncertain sign."}]}).to_string());
    assert!(c.execute(v).unwrap().failed);
    assert!(
        read(c.output.join("scan.json"))["decision"]["issue_codes"]
            .as_array()
            .unwrap()
            .contains(&json!("SCAN_RECOGNITION_UNCERTAIN"))
    );
    assert!(!c.output.join("plan.json").exists());
}
#[test]
fn target_without_native_math_uses_original_only_after_semantic_match() {
    let c = Case::new("clear");
    let mut p = read(&c.profile);
    p["app_version"] = json!("unknown");
    write(&c.profile, &p);
    assert!(
        !c.execute(response(Some("x_i=-\\frac{a_i}{b_i}")))
            .unwrap()
            .failed
    );
    assert_eq!(
        read(c.output.join("scan.json"))["decision"]["representation"],
        "formula_image"
    );
    assert!(!c.output.join("compiled.tex").exists());
}
#[test]
fn cancellation_after_actual_post_preserves_attempt_and_token_reservation() {
    let c = Case::new("clear");
    let s = Server::http(
        200,
        b"partial".to_vec(),
        Some(99),
        std::time::Duration::from_millis(200),
    );
    c.activate(&s);
    let config: ScanApiConfig = parse(&fs::read(&c.api).unwrap()).unwrap();
    let body =
        scan_api::request_body(&config, &fs::read(c.case.join("region.png")).unwrap()).unwrap();
    let exchange = scan_api::invoke(&config, Some(KEY), &body, &|| {
        if s.accepted.load(std::sync::atomic::Ordering::SeqCst) {
            Err(error(
                "SCAN_TEST_CANCELLED",
                3,
                "artificial cancellation after request acceptance",
            ))
        } else {
            Ok(())
        }
    })
    .unwrap();
    s.finish();
    assert_eq!(exchange.api["usage"]["network_attempts"], 1);
    assert_eq!(
        exchange.api["usage"]["input_reserved_tokens"],
        body.len() as u64 + 1024
    );
    assert_eq!(
        exchange.api["usage"]["reserved_tokens"],
        body.len() as u64 + 1024 + config.limits.max_output_tokens
    );
    assert_eq!(exchange.api["usage"]["real_model_calls"], 0);
    assert_eq!(exchange.api["usage"]["paid_calls"], 0);
    assert_eq!(exchange.issue.as_deref(), Some("SCAN_TEST_CANCELLED"));
    assert!(exchange.recognition.is_none());
}
#[test]
fn original_crop_pixel_mismatch_and_png_trailing_data_reject_before_http() {
    for trailing in [false, true] {
        let c = Case::new("clear");
        if trailing {
            let mut b = fs::read(c.case.join("region.png")).unwrap();
            b.extend_from_slice(b"junk");
            fs::write(c.case.join("region.png"), b).unwrap();
        } else {
            fs::copy(
                repo().join("fixtures/scan-formula/cases/unreadable_sign/region.png"),
                c.case.join("region.png"),
            )
            .unwrap();
        }
        c.repin();
        assert!(c.run().is_err());
        assert!(!c.output.exists());
    }
}
#[test]
fn casefold_colliding_source_references_fail_at_the_schema_gate() {
    let c = Case::new("clear");
    c.casefold_collision();
    let e = c.run().err().expect("casefold aliases must be rejected");
    assert_eq!(e.code(), "SCAN_SCHEMA");
    assert_eq!(e.exit_code(), 2);
    assert!(!c.output.exists());
}
#[test]
fn greek_casefold_aliases_fail_at_the_schema_gate() {
    let c = Case::new("clear");
    c.casefold_collision_names("Σ.png", "ς.png");
    let e = c
        .run()
        .err()
        .expect("Greek casefold aliases must be rejected");
    assert_eq!(e.code(), "SCAN_SCHEMA");
    assert_eq!(e.exit_code(), 2);
    assert!(!c.output.exists());
}
#[test]
fn source_inventory_changes_before_or_after_response_are_detected_and_preserved() {
    for phase in ["inputs_held", "recognition_saved", "results_saved"] {
        let c = Case::new("clear");
        let p = c.case.join("human.txt");
        let target = p.clone();
        assert!(
            c.with_hook(move |at| {
                if at == phase {
                    fs::write(&target, b"human source addition").unwrap();
                }
            })
            .is_err()
        );
        assert_eq!(fs::read(p).unwrap(), b"human source addition");
        c.no_success();
        if phase != "inputs_held" {
            assert!(c.output.join("response.bin").exists());
            assert!(c.output.join("original.png").exists());
        }
    }
}
#[test]
fn held_config_profile_and_source_edits_are_denied_or_detected() {
    for field in ["api", "profile", "region"] {
        let c = Case::new("clear");
        let p = match field {
            "api" => c.api.clone(),
            "profile" => c.profile.clone(),
            _ => c.case.join("region.png"),
        };
        let observed = Arc::new(Mutex::new(None));
        let found = observed.clone();
        let result = c.with_hook(move |at| {
            if at == "results_saved" {
                *found.lock().unwrap() = Some(fs::write(&p, b"human selected edit").is_ok());
            }
        });
        if observed.lock().unwrap().unwrap() {
            assert!(result.is_err());
            c.no_success();
        } else {
            assert!(result.is_ok());
        }
    }
}
#[test]
fn manual_note_change_and_unknown_output_survive_cleanup() {
    for note in [false, true] {
        let c = Case::new("clear");
        let p = c.output.join(if note {
            "扫描公式预览.md"
        } else {
            "human.txt"
        });
        let target = p.clone();
        let observed = Arc::new(Mutex::new(None));
        let found = observed.clone();
        let result = c.with_hook(move |at| {
            if at == "results_saved" {
                *found.lock().unwrap() = Some(fs::write(&target, b"human edit").is_ok());
            }
        });
        if observed.lock().unwrap().unwrap() {
            assert!(result.is_err());
            assert_eq!(fs::read(p).unwrap(), b"human edit");
            for n in ["scan.json", "scan.md", "plan.json", "compiled.tex"] {
                assert!(!c.output.join(n).exists());
            }
            assert!(c.output.join("response.bin").exists());
            assert!(c.output.join("original.png").exists());
        } else {
            assert!(result.is_ok());
        }
    }
}
#[cfg(unix)]
#[test]
fn selected_ancestor_symlink_rejects_before_read() {
    let mut c = Case::new("clear");
    let alias = c.root.join("alias");
    std::os::unix::fs::symlink(&c.case, &alias).unwrap();
    c.case = alias;
    assert!(c.run().is_err());
    assert!(!c.output.exists());
}
#[cfg(windows)]
#[test]
fn selected_ancestor_junction_rejects_before_read() {
    let mut c = Case::new("clear");
    let alias = c.root.join("alias");
    let result = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&alias)
        .arg(&c.case)
        .output()
        .unwrap();
    assert!(result.status.success());
    c.case = alias;
    assert!(c.run().is_err());
    assert!(!c.output.exists());
}
