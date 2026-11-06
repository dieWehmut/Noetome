//! Internal synthetic wire probes; these do not prove mathematical OCR success.
use super::*;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn read(p: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
}
fn write(p: impl AsRef<Path>, v: &Value) {
    fs::write(p, serde_json::to_vec_pretty(v).unwrap()).unwrap();
}
fn test_response(request: &RecognitionRequest, tex: &str) -> Value {
    json!({"schema_version":1,"producer":"pix2tex","package_version":"0.1.4","request_id":request.request_id,"input":{"sha256":request.input.sha256,"byte_length":request.input.byte_length},"model":{"release":"v0.0.1","files":scan_recognition::model_fingerprints()},"recognized_tex":tex,"issues":[],"elapsed_ms":1,"status":"recognized","confidence":null,"runtime":{"python":"3.12.14","packages":{"pix2tex":"0.1.4","torch":"2.5.1+cpu","torchvision":"0.20.1+cpu","numpy":"1.26.4","albumentations":"1.3.1","opencv-python-headless":"4.10.0.84","pandas":"2.2.3","transformers":"4.49.0","timm":"0.5.4","x-transformers":"0.15.0","tokenizers":"0.21.4","Pillow":"13.0.0"}},"policy":{"device":"cpu","seed":42,"temperature":0.2,"resize":true,"max_seq_len":512,"decoding":"greedy_argmax_v1","network_calls":0,"paid_calls":0,"automatic_downloads":false,"clipboard":false}})
}
fn synthetic(request: &RecognitionRequest) -> Capture {
    Capture {
        raw: Some(
            serde_json::to_vec(&test_response(request, "x_{i}=-\\frac{a_{i}}{b_{i}}")).unwrap(),
        ),
        diagnostics: vec![],
        transport_issue: None,
    }
}
fn missing_sign(request: &RecognitionRequest) -> Capture {
    Capture {
        raw: Some(
            serde_json::to_vec(&test_response(request, "x_{i}=\\frac{a_{i}}{b_{i}}")).unwrap(),
        ),
        diagnostics: vec![],
        transport_issue: None,
    }
}
fn extra_bar(request: &RecognitionRequest) -> Capture {
    Capture {
        raw: Some(
            serde_json::to_vec(&test_response(
                request,
                "x_{i}=-\\frac{\\bar{a_{i}}}{b_{i}}",
            ))
            .unwrap(),
        ),
        diagnostics: vec![],
        transport_issue: None,
    }
}
struct Case {
    root: PathBuf,
    case: PathBuf,
    profile: PathBuf,
    output: PathBuf,
}
impl Case {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("noetome-scan-guard-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let case = root.join("selected");
        fs::create_dir(&case).unwrap();
        for e in fs::read_dir(repo().join("fixtures/scan-formula/cases").join(name)).unwrap() {
            let p = e.unwrap().path();
            fs::copy(&p, case.join(p.file_name().unwrap())).unwrap();
        }
        let profile = root.join("profile.json");
        write(
            &profile,
            &json!({"schema_version":1,"app":"Obsidian","app_version":"1.6.7","mathjax_version":"3.2.2","native_math":true,"compatibility_dfrac":true,"local_images":true,"verified":true}),
        );
        let output = root.join("new-vault");
        Self {
            root,
            case,
            profile,
            output,
        }
    }
    fn run(&self, r: Recognizer) -> Result<ScanOutput, ProjectError> {
        run_inner(&self.case, &self.profile, &self.output, Some(r))
    }
    fn with_hook(&self, hook: impl FnMut(&str) + 'static) -> Result<ScanOutput, ProjectError> {
        TEST_HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
        let result = self.run(synthetic);
        TEST_HOOK.with(|h| *h.borrow_mut() = None);
        result
    }
    fn no_success(&self) {
        for name in [
            "scan.json",
            "scan.md",
            "plan.json",
            "compiled.tex",
            "扫描公式预览.md",
        ] {
            assert!(!self.output.join(name).exists(), "{name}");
        }
    }
    fn repin(&self) {
        let mut v = read(self.case.join("case.json"));
        for field in ["pdf", "page", "region", "semantics"] {
            if !v[field].is_null() {
                let p = self.case.join(v[field]["file"].as_str().unwrap());
                v[field]["sha256"] = json!(fingerprint(&fs::read(p).unwrap()).sha256);
            }
        }
        write(self.case.join("case.json"), &v);
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.root.is_symlink() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
#[test]
fn single_script_equivalence_preserves_signs_names_and_fraction_roles() {
    let want = canonical("x_i=-\\frac{a_i}{b_i}").unwrap().0;
    assert_eq!(canonical("x_{i}=-\\dfrac{a_{i}}{b_{i}}").unwrap().0, want);
    for wrong in [
        "x_j=-\\frac{a_i}{b_i}",
        "x_i=\\frac{a_i}{b_i}",
        "x_i=-\\frac{b_i}{a_i}",
        "x_i=-\\frac{a^i}{b_i}",
    ] {
        assert_ne!(canonical(wrong).unwrap().0, want);
    }
    for bad in [
        "x_i=-\\frac{\\bar{a_i}}{b_i}",
        "\\href{evil}{x}",
        "x_i=-\\mystery{a_i}",
    ] {
        assert!(canonical(bad).is_err());
    }
}
#[test]
fn synthetic_success_retains_raw_recognition_and_provenance() {
    let c = Case::new("clear");
    let result = c.run(synthetic).unwrap();
    assert!(!result.failed);
    let r = read(c.output.join("scan.json"));
    assert_eq!(r["checks"]["region_match"], "passed");
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
    assert!(c.output.join("扫描公式预览.md").exists());
    let q = read(c.output.join("recognition-request.json"));
    assert_eq!(q.as_object().unwrap().len(), 5);
    assert!(
        !fs::read_to_string(c.output.join("recognition-request.json"))
            .unwrap()
            .contains("expression")
    );
}
#[test]
fn model_guess_cannot_resolve_the_two_unknown_scan_states() {
    for name in ["unreadable_sign", "unreadable_subscript"] {
        let c = Case::new(name);
        let result = c.run(synthetic).unwrap();
        assert!(result.failed);
        let r = read(c.output.join("scan.json"));
        assert!(r["decision"]["representation"].is_null());
        assert!(
            r["decision"]["issue_codes"]
                .as_array()
                .unwrap()
                .contains(&json!("SCAN_CRITICAL_SYMBOL_UNKNOWN"))
        );
        for file in ["plan.json", "compiled.tex", "扫描公式预览.md"] {
            assert!(!c.output.join(file).exists());
        }
        assert!(c.output.join("raw-response.json").exists());
        assert_eq!(
            fs::read(c.output.join("original.png")).unwrap(),
            fs::read(c.case.join("region.png")).unwrap()
        );
    }
}
#[test]
fn wrong_sign_and_extra_bar_are_blocked_without_repairing_model_output() {
    for recognizer in [missing_sign as Recognizer, extra_bar] {
        let c = Case::new("clear");
        assert!(c.run(recognizer).unwrap().failed);
        let r = read(c.output.join("scan.json"));
        assert!(r["decision"]["representation"].is_null());
        assert!(!c.output.join("compiled.tex").exists());
        let wire = read(c.output.join("raw-response.json"));
        assert_eq!(
            fs::read_to_string(c.output.join("recognized.tex")).unwrap(),
            wire["recognized_tex"].as_str().unwrap()
        );
    }
}
#[test]
fn unverified_target_keeps_original_region_after_semantic_match() {
    let c = Case::new("clear");
    let mut p = read(&c.profile);
    p["app_version"] = json!("unknown");
    write(&c.profile, &p);
    assert!(!c.run(synthetic).unwrap().failed);
    let r = read(c.output.join("scan.json"));
    assert_eq!(r["decision"]["representation"], "formula_image");
    assert!(!c.output.join("compiled.tex").exists());
    assert!(
        fs::read_to_string(c.output.join("扫描公式预览.md"))
            .unwrap()
            .contains("![[original.png]]")
    );
}
#[test]
fn actual_crop_pixels_reject_a_consistent_fingerprint_for_another_region() {
    let c = Case::new("clear");
    let other = repo().join("fixtures/scan-formula/cases/unreadable_sign/region.png");
    fs::copy(other, c.case.join("region.png")).unwrap();
    c.repin();
    let e = c.run(synthetic).err().unwrap();
    assert_eq!(e.code(), "SCAN_REGION_MISMATCH");
    assert!(!c.output.exists());
}
#[test]
fn png_trailing_bytes_and_out_of_page_bounds_are_rejected_before_ocr() {
    for mutate in [0, 1] {
        let c = Case::new("clear");
        if mutate == 0 {
            let mut bytes = fs::read(c.case.join("region.png")).unwrap();
            bytes.extend_from_slice(b"trailing");
            fs::write(c.case.join("region.png"), bytes).unwrap();
            c.repin();
        } else {
            let mut v = read(c.case.join("case.json"));
            v["region"]["bounds"] = json!([799, 999, 560, 200]);
            write(c.case.join("case.json"), &v);
        }
        assert!(c.run(synthetic).is_err());
        assert!(!c.output.exists());
    }
}
#[test]
fn unknown_file_and_nullable_omission_reject_before_ocr() {
    for mutate in [0, 1] {
        let c = Case::new("clear");
        if mutate == 0 {
            fs::write(c.case.join("manual.txt"), b"human").unwrap();
        } else {
            let mut v = read(c.case.join("semantics.json"));
            v["symbols"][0].as_object_mut().unwrap().remove("unit");
            write(c.case.join("semantics.json"), &v);
            c.repin();
        }
        assert!(c.run(synthetic).is_err());
        assert!(!c.output.exists());
    }
}
#[test]
fn output_collision_and_selected_source_overlap_preserve_human_content() {
    for overlap in [false, true] {
        let mut c = Case::new("clear");
        if overlap {
            c.output = c.case.join("new-vault");
        } else {
            fs::create_dir(&c.output).unwrap();
            fs::write(c.output.join("manual.txt"), b"human").unwrap();
        }
        assert!(c.run(synthetic).is_err());
        if !overlap {
            assert_eq!(fs::read(c.output.join("manual.txt")).unwrap(), b"human");
        } else {
            assert!(!c.output.exists());
        }
    }
}
#[test]
fn model_wire_unknown_confidence_policy_and_changed_identity_never_pass() {
    let c = Case::new("clear");
    let request = RecognitionRequest {
        schema_version: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        input: RecognitionInput {
            path: c.case.join("region.png").to_string_lossy().into_owned(),
            sha256: fingerprint(&fs::read(c.case.join("region.png")).unwrap()).sha256,
            byte_length: fs::metadata(c.case.join("region.png")).unwrap().len(),
        },
        model_directory: "internal-test".into(),
        limits: RecognitionLimits {
            response_bytes: 131072,
            diagnostic_bytes: 65536,
            timeout_ms: 120000,
            max_tex_bytes: 65536,
        },
    };
    for mode in 0..5 {
        let mut v = test_response(&request, "x_i=-\\frac{a_i}{b_i}");
        match mode {
            0 => v["confidence"] = json!(1.0),
            1 => v["input"]["sha256"] = json!("0".repeat(64)),
            2 => v["policy"]["network_calls"] = json!(1),
            3 => v["producer"] = json!("fake"),
            _ => v["runtime"]["packages"]["torch"] = json!("unknown"),
        };
        assert!(scan_recognition::response(&serde_json::to_vec(&v).unwrap(), &request).is_err());
    }
}
#[test]
fn source_inventory_change_at_each_phase_is_detected_and_preserved() {
    for phase in ["inputs_held", "recognition_saved", "results_saved"] {
        let c = Case::new("clear");
        let target = c.case.join("human-added.txt");
        let p = target.clone();
        let result = c.with_hook(move |at| {
            if at == phase {
                fs::write(&p, b"human source addition").unwrap();
            }
        });
        assert!(result.is_err());
        assert_eq!(fs::read(target).unwrap(), b"human source addition");
        c.no_success();
        if phase == "inputs_held" {
            assert!(!c.output.exists());
        } else {
            assert!(c.output.join("original.png").exists());
            assert!(c.output.join("raw-response.json").exists());
        }
    }
}
#[test]
fn held_source_and_profile_edits_are_denied_or_detected() {
    use std::sync::Mutex;
    for phase in ["inputs_held", "recognition_saved", "results_saved"] {
        for profile in [false, true] {
            let c = Case::new("clear");
            let p = if profile {
                c.profile.clone()
            } else {
                c.case.join("region.png")
            };
            let observed = Arc::new(Mutex::new(None));
            let found = observed.clone();
            let result = c.with_hook(move |at| {
                if at == phase {
                    *found.lock().unwrap() = Some(fs::write(&p, b"human edit").is_ok());
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
}
#[test]
fn human_note_edit_is_preserved_while_unchanged_success_objects_are_removed() {
    use std::sync::Mutex;
    let c = Case::new("clear");
    let p = c.output.join("扫描公式预览.md");
    let observed = Arc::new(Mutex::new(None));
    let found = observed.clone();
    let result = c.with_hook(move |at| {
        if at == "results_saved" {
            *found.lock().unwrap() = Some(fs::write(&p, b"human note edit").is_ok());
        }
    });
    if observed.lock().unwrap().unwrap() {
        assert!(result.is_err());
        assert_eq!(
            fs::read(c.output.join("扫描公式预览.md")).unwrap(),
            b"human note edit"
        );
        for name in ["scan.json", "scan.md", "plan.json", "compiled.tex"] {
            assert!(!c.output.join(name).exists());
        }
        assert!(c.output.join("original.png").exists());
        assert!(c.output.join("raw-response.json").exists());
    } else {
        assert!(result.is_ok());
    }
}
#[test]
fn unknown_output_file_is_not_deleted_by_failure_cleanup() {
    let c = Case::new("clear");
    let p = c.output.join("human.txt");
    let target = p.clone();
    let result = c.with_hook(move |at| {
        if at == "results_saved" {
            fs::write(&target, b"unmanaged human bytes").unwrap();
        }
    });
    assert!(result.is_err());
    c.no_success();
    assert_eq!(fs::read(p).unwrap(), b"unmanaged human bytes");
    assert!(c.output.join("original.png").exists());
    assert!(c.output.join("raw-response.json").exists());
}
#[cfg(unix)]
#[test]
fn selected_ancestor_symlink_is_rejected_before_semantic_read() {
    let mut c = Case::new("clear");
    let alias = c.root.join("alias");
    std::os::unix::fs::symlink(&c.case, &alias).unwrap();
    c.case = alias;
    assert!(c.run(synthetic).is_err());
    assert!(!c.output.exists());
}
#[cfg(windows)]
#[test]
fn selected_ancestor_junction_is_rejected_before_semantic_read() {
    let mut c = Case::new("clear");
    let alias = c.root.join("junction");
    let created = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&alias)
        .arg(&c.case)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    c.case = alias;
    assert!(c.run(synthetic).is_err());
    assert!(!c.output.exists());
}
