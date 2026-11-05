//! Private stage injection proves source/output capabilities, never a public CLI option.
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
        let root = parent.join(format!("formula-guard-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let case = root.join("case");
        fs::create_dir(&case).unwrap();
        let fixture = repo.join("fixtures/formula-fidelity");
        for e in fs::read_dir(fixture.join("native")).unwrap() {
            let e = e.unwrap();
            fs::copy(e.path(), case.join(e.file_name())).unwrap();
        }
        let profile = root.join("profile.json");
        fs::copy(fixture.join("profile-native.json"), &profile).unwrap();
        Self {
            output: root.join("new-Vault"),
            root,
            case,
            profile,
        }
    }
    fn execute(&self, hook: impl FnMut(&str) + 'static) -> Result<FormulaOutput, ProjectError> {
        TEST_HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
        let result = run(&self.case, &self.profile, &self.output);
        TEST_HOOK.with(|h| *h.borrow_mut() = None);
        result
    }
    fn assert_no_success(&self) {
        for name in [
            "formula.json",
            "formula.md",
            "plan.json",
            "compiled.tex",
            "公式预览.md",
        ] {
            assert!(!self.output.join(name).exists(), "{name}");
        }
    }
}
#[test]
fn inventory_mutation_before_creation_and_after_save_is_detected_without_deleting_it() {
    for stage_name in ["inputs_held", "results_saved"] {
        let c = Case::new();
        let changed = c.case.join("human-added.txt");
        let target = changed.clone();
        let result = c.execute(move |at| {
            if at == stage_name {
                fs::write(&target, b"human source inventory change").unwrap();
            }
        });
        assert!(result.is_err());
        assert_eq!(
            fs::read(&changed).unwrap(),
            b"human source inventory change"
        );
        if stage_name == "inputs_held" {
            assert!(!c.output.exists());
        } else {
            c.assert_no_success();
            assert_eq!(
                fs::read(c.output.join("original.tex")).unwrap(),
                fs::read(c.case.join("original.tex")).unwrap()
            );
            assert!(c.output.join("original.png").exists());
        }
    }
}
#[test]
fn original_attachment_and_profile_changes_are_denied_or_detected_while_held() {
    for stage_name in ["inputs_held", "results_saved"] {
        for attachment in [true, false] {
            let c = Case::new();
            let target = if attachment {
                c.case.join("region.png")
            } else {
                c.profile.clone()
            };
            let capture = Arc::new(Mutex::new(None));
            let observed = capture.clone();
            let result = c.execute(move |at| {
                if at == stage_name {
                    *capture.lock().unwrap() =
                        Some(fs::write(&target, b"human original edit").is_ok());
                }
            });
            if observed.lock().unwrap().unwrap() {
                assert!(result.is_err());
                c.assert_no_success();
            } else {
                assert!(result.is_ok());
            }
        }
    }
}
#[test]
fn generated_note_edit_survives_but_unchanged_success_reports_are_removed() {
    let c = Case::new();
    let target = c.output.join("公式预览.md");
    let capture = Arc::new(Mutex::new(None));
    let observed = capture.clone();
    let result = c.execute(move |at| {
        if at == "results_saved" {
            *capture.lock().unwrap() = Some(fs::write(&target, b"human formula edit").is_ok());
        }
    });
    if observed.lock().unwrap().unwrap() {
        assert!(result.is_err());
        assert_eq!(
            fs::read(c.output.join("公式预览.md")).unwrap(),
            b"human formula edit"
        );
        for name in ["formula.json", "formula.md", "plan.json", "compiled.tex"] {
            assert!(!c.output.join(name).exists());
        }
        assert!(c.output.join("original.tex").exists());
    } else {
        assert!(result.is_ok());
    }
}
#[test]
fn new_unknown_output_and_hardlink_alias_preserve_external_bytes_and_clear_success() {
    for hardlink in [false, true] {
        let c = Case::new();
        let target = if hardlink {
            c.root.join("outside-alias.txt")
        } else {
            c.output.join("unknown-human-file.txt")
        };
        let captured_target = target.clone();
        let output = c.output.clone();
        let capture = Arc::new(Mutex::new(None));
        let observed = capture.clone();
        let result = c.execute(move |at| {
            if at == "results_saved" {
                let mutation = if hardlink {
                    fs::hard_link(output.join("公式预览.md"), &captured_target)
                } else {
                    fs::write(&captured_target, b"unowned output sentinel")
                };
                *capture.lock().unwrap() = Some(mutation.is_ok());
            }
        });
        if observed.lock().unwrap().unwrap() {
            assert!(result.is_err());
            c.assert_no_success();
            if hardlink {
                assert!(
                    fs::read(&target)
                        .unwrap()
                        .starts_with("# 公式预览".as_bytes())
                );
            } else {
                assert_eq!(fs::read(&target).unwrap(), b"unowned output sentinel");
            }
        } else {
            assert!(result.is_ok());
        }
    }
}
#[cfg(unix)]
#[test]
fn replaced_case_ancestor_never_follows_the_link_or_writes_a_success_vault() {
    use std::os::unix::fs::symlink;
    let c = Case::new();
    let original = c.case.clone();
    let moved = c.root.join("case-held-moved");
    let outside = c.root.join("external-directory");
    fs::create_dir(&outside).unwrap();
    fs::write(
        outside.join("external-sentinel.txt"),
        b"untouched external bytes",
    )
    .unwrap();
    let destination = outside.clone();
    let result = c.execute(move |at| {
        if at == "inputs_held" {
            fs::rename(&original, &moved).unwrap();
            symlink(&destination, &original).unwrap();
        }
    });
    assert!(result.is_err());
    assert!(!c.output.exists());
    assert_eq!(
        fs::read(outside.join("external-sentinel.txt")).unwrap(),
        b"untouched external bytes"
    );
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
}
