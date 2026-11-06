//! Private stage injection verifies held files and cleanup without a public mutation option.
use super::*;
use std::{fs, path::PathBuf, sync::Mutex};

struct Case {
    root: PathBuf,
    case: PathBuf,
    profile: PathBuf,
    output: PathBuf,
}
impl Case {
    fn new() -> Self {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let parent = repo.join("target/compiler-temp");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!("image-guard-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let case = root.join("case");
        fs::create_dir(&case).unwrap();
        for entry in fs::read_dir(repo.join("fixtures/image-fidelity/signed_arrows")).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), case.join(entry.file_name())).unwrap();
        }
        let profile = root.join("profile.json");
        fs::copy(
            repo.join("fixtures/image-fidelity/profile-native.json"),
            &profile,
        )
        .unwrap();
        Self {
            case,
            profile,
            output: root.join("new-Vault"),
            root,
        }
    }
    fn execute(&self, hook: impl FnMut(&str) + 'static) -> Result<ImageOutput, ProjectError> {
        TEST_HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
        let result = run(&self.case, &self.profile, &self.output);
        TEST_HOOK.with(|h| *h.borrow_mut() = None);
        result
    }
    fn no_success(&self) {
        for name in [
            "image.json",
            "image.md",
            "plan.json",
            "图像预览.md",
            "redrawn.svg",
        ] {
            assert!(!self.output.join(name).exists(), "{name}");
        }
    }
    fn edit_semantics(&self, edit: impl FnOnce(&mut serde_json::Value)) {
        let path = self.case.join("semantics.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        edit(&mut value);
        let bytes = serde_json::to_vec_pretty(&value).unwrap();
        fs::write(path, &bytes).unwrap();
        let path = self.case.join("case.json");
        let mut case: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        case["semantics"]["sha256"] = serde_json::json!(fingerprint(&bytes).sha256);
        fs::write(path, serde_json::to_vec_pretty(&case).unwrap()).unwrap();
    }
}
#[test]
fn inventory_changed_before_creation_and_after_save_preserves_unknown_source() {
    for target_stage in ["inputs_held", "results_saved"] {
        let c = Case::new();
        let source = c.case.join("human-added.txt");
        let captured = source.clone();
        let result = c.execute(move |at| {
            if at == target_stage {
                fs::write(&captured, b"human inventory change").unwrap();
            }
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&source).unwrap(), b"human inventory change");
        if target_stage == "inputs_held" {
            assert!(!c.output.exists());
        } else {
            c.no_success();
            assert!(c.output.join("original.png").exists());
        }
    }
}
#[test]
fn original_png_and_profile_changes_are_denied_or_detected_while_held() {
    for target_stage in ["inputs_held", "results_saved"] {
        for png in [true, false] {
            let c = Case::new();
            let target = if png {
                c.case.join("source.png")
            } else {
                c.profile.clone()
            };
            let capture = Arc::new(Mutex::new(None));
            let observed = capture.clone();
            let result = c.execute(move |at| {
                if at == target_stage {
                    *capture.lock().unwrap() =
                        Some(fs::write(&target, b"human input edit").is_ok());
                }
            });
            if observed.lock().unwrap().unwrap() {
                assert!(result.is_err());
                c.no_success();
            } else {
                assert!(!result.unwrap().failed);
            }
        }
    }
}
#[test]
fn changed_svg_is_preserved_and_unchanged_success_objects_are_discarded() {
    let c = Case::new();
    let svg = c.output.join("redrawn.svg");
    let captured = svg.clone();
    let capture = Arc::new(Mutex::new(None));
    let observed = capture.clone();
    let result = c.execute(move |at| {
        if at == "results_saved" {
            *capture.lock().unwrap() = Some(fs::write(&captured, b"human svg edit").is_ok());
        }
    });
    if observed.lock().unwrap().unwrap() {
        assert!(result.is_err());
        assert_eq!(fs::read(&svg).unwrap(), b"human svg edit");
        for name in ["image.json", "image.md", "plan.json", "图像预览.md"] {
            assert!(!c.output.join(name).exists(), "{name}");
        }
        assert!(c.output.join("original.png").exists());
    } else {
        assert!(!result.unwrap().failed);
    }
}
#[test]
fn unknown_output_and_added_hardlink_preserve_external_bytes_and_clear_success() {
    for hardlink in [false, true] {
        let c = Case::new();
        let target = if hardlink {
            c.root.join("external.svg")
        } else {
            c.output.join("human.txt")
        };
        let captured = target.clone();
        let output = c.output.clone();
        let capture = Arc::new(Mutex::new(None));
        let observed = capture.clone();
        let result = c.execute(move |at| {
            if at == "results_saved" {
                *capture.lock().unwrap() = Some(if hardlink {
                    fs::hard_link(output.join("redrawn.svg"), &captured).is_ok()
                } else {
                    fs::write(&captured, b"unknown human bytes").is_ok()
                });
            }
        });
        if observed.lock().unwrap().unwrap() {
            assert!(result.is_err());
            c.no_success();
            if hardlink {
                assert!(fs::read(&target).unwrap().starts_with(b"<svg "));
            } else {
                assert_eq!(fs::read(&target).unwrap(), b"unknown human bytes");
            }
        } else {
            assert!(!result.unwrap().failed);
        }
    }
}
#[cfg(unix)]
#[test]
fn replacing_case_ancestor_does_not_follow_symlink_or_touch_external_sentinel() {
    use std::os::unix::fs::symlink;
    let c = Case::new();
    let source = c.case.clone();
    let moved = c.root.join("held-case");
    let outside = c.root.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("sentinel.txt"), b"external bytes").unwrap();
    let destination = outside.clone();
    let result = c.execute(move |at| {
        if at == "inputs_held" {
            fs::rename(&source, &moved).unwrap();
            symlink(&destination, &source).unwrap();
        }
    });
    assert!(result.is_err());
    assert!(!c.output.exists());
    assert_eq!(
        fs::read(outside.join("sentinel.txt")).unwrap(),
        b"external bytes"
    );
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
}
#[test]
fn redirected_arrow_and_spatial_declaration_are_blocked_before_a_success_plan() {
    for relation in [true, false] {
        let c = Case::new();
        let path = c.case.join("semantics.json");
        let mut semantics: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        if relation {
            semantics["relations"][0]["start"] = serde_json::json!([390, 210]);
            semantics["relations"][0]["end"] = serde_json::json!([260, 210]);
        } else {
            semantics["spatial_relations"][0]["relation"] = serde_json::json!("above");
        }
        let bytes = serde_json::to_vec_pretty(&semantics).unwrap();
        fs::write(path, &bytes).unwrap();
        let path = c.case.join("case.json");
        let mut case: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        case["semantics"]["sha256"] = serde_json::json!(fingerprint(&bytes).sha256);
        fs::write(path, serde_json::to_vec_pretty(&case).unwrap()).unwrap();
        let output = c.execute(|_| {}).unwrap();
        assert!(output.failed);
        let report: serde_json::Value = serde_json::from_str(&output.json).unwrap();
        assert!(
            report["decision"]["issue_codes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "IMAGE_GEOMETRY_MISMATCH")
        );
        assert!(!c.output.join("plan.json").exists());
        assert!(!c.output.join("图像预览.md").exists());
    }
}

#[test]
fn significant_whitespace_and_long_labels_use_original_without_changing_literal_text() {
    for label in [
        "Feed\ncontinued",
        "Feed\tcontinued",
        "Feed  continued",
        "A".repeat(100).as_str(),
    ] {
        let c = Case::new();
        c.edit_semantics(|s| s["elements"][0]["label"] = serde_json::json!(label));
        let result = c.execute(|_| {}).unwrap();
        assert!(!result.failed);
        let report: serde_json::Value = serde_json::from_str(&result.json).unwrap();
        assert_eq!(report["decision"]["representation"], "original_image");
        assert!(
            report["decision"]["fallback_reasons"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "IMAGE_TEXT_LAYOUT")
        );
        assert!(
            fs::read_to_string(c.output.join("图像预览.md"))
                .unwrap()
                .contains(label)
        );
        assert!(!c.output.join("redrawn.svg").exists());
    }
}
#[test]
fn parent_label_occlusion_and_an_intermediate_shape_across_the_arrow_use_original() {
    for parent_overlap in [true, false] {
        let c = Case::new();
        c.edit_semantics(|s| {
            if parent_overlap {
                s["elements"].as_array_mut().unwrap().push(serde_json::json!({"id":"child","kind":"rect","bbox":[100,180,100,60],"label":"Child","unit":null,"legend_id":null}));
            } else {
                s["elements"].as_array_mut().unwrap().push(serde_json::json!({"id":"blocker","kind":"rect","bbox":[300,180,40,60],"label":"B","unit":null,"legend_id":null}));
            }
        });
        let result = c.execute(|_| {}).unwrap();
        assert!(!result.failed);
        let report: serde_json::Value = serde_json::from_str(&result.json).unwrap();
        assert_eq!(report["decision"]["representation"], "original_image");
        assert!(
            report["decision"]["fallback_reasons"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "IMAGE_GEOMETRY_LAYOUT")
        );
        assert!(!c.output.join("redrawn.svg").exists());
    }
}

#[test]
fn svg_has_an_explicit_white_canvas_and_black_relation_ink_with_source_mappings() {
    let c = Case::new();
    let result = c.execute(|_| {}).unwrap();
    assert!(!result.failed);
    let svg = fs::read_to_string(c.output.join("redrawn.svg")).unwrap();
    assert!(svg.lines().nth(1).unwrap().contains(
        "<rect id=\"image-canvas\" x=\"0\" y=\"0\" width=\"960\" height=\"520\" fill=\"#ffffff\"/>"
    ));
    for line in svg
        .lines()
        .filter(|line| line.starts_with("<line ") || line.starts_with("<polygon "))
    {
        assert!(line.contains("stroke=\"#000000\""));
        if line.starts_with("<polygon ") {
            assert!(line.contains("fill=\"#000000\""));
        }
    }
    assert!(svg.contains("fill=\"#c5efcb\"") && svg.contains("fill=\"#f5c1c1\""));
    let report: serde_json::Value = serde_json::from_str(&result.json).unwrap();
    let canvas = &report["files"][1]["mappings"][0];
    assert_eq!(canvas["kind"], "canvas_background");
    assert_eq!(canvas["source_field"], "case.dimensions");
    let start = canvas["start_byte"].as_u64().unwrap() as usize;
    let end = canvas["end_byte"].as_u64().unwrap() as usize;
    assert_eq!(&svg[start..end], svg.lines().nth(1).unwrap());
}

#[test]
fn an_arrowhead_corner_cannot_cover_a_third_shape_when_its_center_line_is_clear() {
    let c = Case::new();
    c.edit_semantics(|s| {
        s["relations"][0]["label"]=serde_json::json!("");s["relations"][0]["effect"]=serde_json::json!("neutral");s["relations"][0]["unit"]=serde_json::Value::Null;
        s["elements"].as_array_mut().unwrap().push(serde_json::json!({"id":"occluder","kind":"rect","bbox":[360,177,28,30],"label":"X","unit":null,"legend_id":null}));
    });
    let result = c.execute(|_| {}).unwrap();
    assert!(!result.failed);
    let report: serde_json::Value = serde_json::from_str(&result.json).unwrap();
    assert_eq!(report["decision"]["representation"], "original_image");
    assert!(
        report["decision"]["fallback_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "IMAGE_GEOMETRY_LAYOUT")
    );
    assert!(!c.output.join("redrawn.svg").exists());
}

#[test]
fn known_dark_fill_colors_use_original_without_changing_labels_units_or_palette() {
    for color in ["#000000", "#bfffff", "#ffbfff", "#ffffbf", "#c0c0c0"] {
        let c = Case::new();
        c.edit_semantics(|s| {
            s["legend"][0]["color"] = serde_json::json!(color);
            s["elements"][0]["legend_id"] = serde_json::json!("positive");
        });
        let result = c.execute(|_| {}).unwrap();
        assert!(!result.failed);
        let report: serde_json::Value = serde_json::from_str(&result.json).unwrap();
        assert_eq!(report["semantics"]["legend"][0]["color"], color);
        assert_eq!(report["semantics"]["elements"][0]["label"], "Feed");
        assert_eq!(report["semantics"]["elements"][0]["unit"], "mmol/L");
        assert_eq!(report["checks"]["author_declaration"], "passed");
        assert_eq!(report["decision"]["issue_codes"], serde_json::json!([]));
        let note = fs::read_to_string(c.output.join("图像预览.md")).unwrap();
        assert!(note.contains(color) && note.contains("Feed") && note.contains("mmol/L"));
        if color == "#c0c0c0" {
            assert_eq!(report["decision"]["representation"], "svg_redraw");
            assert_eq!(
                report["decision"]["fallback_reasons"],
                serde_json::json!([])
            );
            assert!(
                fs::read_to_string(c.output.join("redrawn.svg"))
                    .unwrap()
                    .contains("fill=\"#c0c0c0\"")
            );
        } else {
            assert_eq!(report["decision"]["representation"], "original_image");
            assert_eq!(
                report["decision"]["fallback_reasons"],
                serde_json::json!(["IMAGE_COLOR_CONTRAST"])
            );
            assert!(note.contains("来源颜色与固定黑字不能在有限重绘中保证可读，保留原图。"));
            assert!(!c.output.join("redrawn.svg").exists());
        }
        assert_eq!(
            fs::read(c.output.join("original.png")).unwrap(),
            fs::read(c.case.join("source.png")).unwrap()
        );
    }
}
#[cfg(unix)]
#[test]
fn output_ancestor_replacement_before_create_and_after_save_never_follows_external_symlink() {
    use std::os::unix::fs::symlink;
    for target_stage in ["inputs_held", "results_saved"] {
        for replace_root in [false, true] {
            if target_stage == "inputs_held" && replace_root {
                continue;
            }
            let mut c = Case::new();
            let parent = c.root.join("output-parent");
            fs::create_dir(&parent).unwrap();
            c.output = parent.join("new-Vault");
            let outside = c.root.join("external");
            fs::create_dir(&outside).unwrap();
            fs::write(outside.join("sentinel.txt"), b"untouched external").unwrap();
            let source = if replace_root {
                c.output.clone()
            } else {
                parent.clone()
            };
            let moved = c.root.join("held-output-moved");
            let captured_source = source.clone();
            let captured_moved = moved.clone();
            let destination = outside.clone();
            let result = c.execute(move |at| {
                if at == target_stage {
                    fs::rename(&captured_source, &captured_moved).unwrap();
                    symlink(&destination, &captured_source).unwrap();
                }
            });
            assert!(result.is_err());
            assert_eq!(
                fs::read(outside.join("sentinel.txt")).unwrap(),
                b"untouched external"
            );
            assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
            if target_stage == "results_saved" {
                let retained = if replace_root {
                    moved.clone()
                } else {
                    moved.join("new-Vault")
                };
                assert_eq!(
                    fs::read(retained.join("original.png")).unwrap(),
                    fs::read(c.case.join("source.png")).unwrap()
                );
                // A redirected ancestor is unknown scene: no cleanup follows it or adopts the moved directory.
                assert!(retained.join("source-case.json").exists());
            }
        }
    }
}
#[cfg(windows)]
#[test]
fn held_output_root_and_parent_deny_rename_after_save() {
    for replace_root in [false, true] {
        let mut c = Case::new();
        let parent = c.root.join("output-parent");
        fs::create_dir(&parent).unwrap();
        c.output = parent.join("new-Vault");
        let source = if replace_root {
            c.output.clone()
        } else {
            parent
        };
        let moved = c.root.join("moved-output");
        let capture = Arc::new(Mutex::new(None));
        let observed = capture.clone();
        let result = c.execute(move |at| {
            if at == "results_saved" {
                *capture.lock().unwrap() = Some(fs::rename(&source, &moved).is_ok());
            }
        });
        assert_eq!(
            *observed.lock().unwrap(),
            Some(false),
            "held Windows directory must deny rename"
        );
        assert!(!result.unwrap().failed);
    }
}
