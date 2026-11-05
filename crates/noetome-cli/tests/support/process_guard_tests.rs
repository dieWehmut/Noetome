//! Real entrypoint stage tests: mutation is never exposed as a CLI option.
#[cfg(windows)]
use super::test_case::snapshot;
use super::test_case::{ProcessCase, assert_snapshot, write};
use super::*;
use serde_json::json;
use std::{fs, sync::Mutex};

fn execute(
    case: &ProcessCase,
    review: bool,
    hook: impl FnMut(&str) + 'static,
) -> Result<ProcessOutput, ProjectError> {
    if review {
        crate::process_review::TEST_HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
        let result = crate::process_review::run(&case.project, &case.case, &case.review);
        crate::process_review::TEST_HOOK.with(|h| *h.borrow_mut() = None);
        result
    } else {
        TEST_HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
        let result = run(
            &case.project,
            &case.case,
            &case.review,
            &case.profile,
            &case.output,
        );
        TEST_HOOK.with(|h| *h.borrow_mut() = None);
        result
    }
}

#[test]
fn input_inventory_changes_before_output_and_after_save_never_keep_success_results() {
    for review in [true, false] {
        for stage_name in ["inputs_held", "results_saved"] {
            let scopes = if review {
                vec!["case", "project"]
            } else {
                vec!["case", "project", "review"]
            };
            for scope in scopes {
                let case = if review {
                    ProcessCase::prepare("linear")
                } else {
                    ProcessCase::ready("linear")
                };
                let originals = case.protected();
                let parent = match scope {
                    "case" => &case.case,
                    "project" => &case.project,
                    _ => &case.review,
                };
                let added = parent.join("human-inventory-change.txt");
                let target = added.clone();
                let result = execute(&case, review, move |at| {
                    if at == stage_name {
                        fs::write(&target, b"human bytes must remain").unwrap();
                    }
                });
                let result_root = if review { &case.review } else { &case.output };
                write(
                    case.root.join("guard-result.json"),
                    &json!({"review":review,"stage":stage_name,"scope":scope,"error":result.as_ref().err().map(|e|e.code()),"output_exists":result_root.exists(),"added_path":added}),
                );
                assert!(result.is_err(), "{review}/{stage_name}/{scope}");
                assert_eq!(fs::read(&added).unwrap(), b"human bytes must remain");
                assert_snapshot(&originals);
                if stage_name == "inputs_held" {
                    assert!(!result_root.exists());
                } else {
                    for item in fs::read_dir(result_root).unwrap() {
                        assert!(item.unwrap().file_name().to_str().unwrap().starts_with('.'));
                    }
                }
            }
        }
    }
}

#[test]
fn unknown_output_addition_survives_and_captured_generated_results_are_removed() {
    for review in [true, false] {
        let case = if review {
            ProcessCase::prepare("linear")
        } else {
            ProcessCase::ready("linear")
        };
        let originals = case.protected();
        let result_root = if review {
            case.review.clone()
        } else {
            case.output.clone()
        };
        let unknown = result_root.join("human-after-save.txt");
        let target = unknown.clone();
        let result = execute(&case, review, move |at| {
            if at == "results_saved" {
                fs::write(&target, b"unowned sentinel").unwrap();
            }
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&unknown).unwrap(), b"unowned sentinel");
        assert_snapshot(&originals);
        for item in fs::read_dir(&result_root).unwrap() {
            let n = item.unwrap().file_name();
            let n = n.to_str().unwrap();
            assert!(n.starts_with('.') || n == "human-after-save.txt", "{n}");
        }
    }
}

#[test]
fn changed_profile_is_detected_or_denied_while_held() {
    for stage_name in ["inputs_held", "results_saved"] {
        let case = ProcessCase::ready("linear");
        let target = case.profile.clone();
        let observed = Arc::new(Mutex::new(None));
        let capture = observed.clone();
        let result = execute(&case, false, move |at| {
            if at == stage_name {
                let changed = fs::write(&target, b"human profile edit");
                *capture.lock().unwrap() = Some(changed.is_ok());
            }
        });
        let changed = observed.lock().unwrap().unwrap();
        write(
            case.root.join("profile-guard.json"),
            &json!({"stage":stage_name,"write_succeeded":changed,"run_succeeded":result.is_ok(),"error":result.as_ref().err().map(|e|e.code())}),
        );
        if changed {
            assert!(result.is_err());
            assert_eq!(fs::read(&case.profile).unwrap(), b"human profile edit");
            assert!(!case.output.join("flow.json").exists());
            assert!(!case.output.join("plan.json").exists());
        } else {
            assert!(result.is_ok());
        }
    }
}

#[test]
fn changed_generated_note_is_preserved_but_unchanged_success_report_is_removed() {
    let case = ProcessCase::ready("linear");
    let target = case.output.join("流程总览.md");
    let observed = Arc::new(Mutex::new(None));
    let capture = observed.clone();
    let result = execute(&case, false, move |at| {
        if at == "results_saved" {
            *capture.lock().unwrap() = Some(fs::write(&target, b"human note edit").is_ok());
        }
    });
    let changed = observed.lock().unwrap().unwrap();
    if changed {
        assert!(result.is_err());
        assert_eq!(
            fs::read(case.output.join("流程总览.md")).unwrap(),
            b"human note edit"
        );
        for name in ["flow.json", "flow.md", "plan.json", "过程明细.md"] {
            assert!(!case.output.join(name).exists(), "{name}");
        }
    } else {
        assert!(result.is_ok());
    }
}

#[test]
fn late_output_hardlink_does_not_delete_external_alias_bytes() {
    let case = ProcessCase::ready("linear");
    let output = case.output.clone();
    let alias = case.root.join("external-alias.txt");
    let target = alias.clone();
    let observed = Arc::new(Mutex::new(None));
    let capture = observed.clone();
    let result = execute(&case, false, move |at| {
        if at == "results_saved" {
            *capture.lock().unwrap() =
                Some(fs::hard_link(output.join("流程总览.md"), &target).is_ok());
        }
    });
    let linked = observed.lock().unwrap().unwrap();
    if linked {
        assert!(result.is_err());
        assert!(
            fs::read(&alias)
                .unwrap()
                .starts_with("# 流程总览".as_bytes())
        );
        assert!(!case.output.join("flow.json").exists());
    } else {
        assert!(result.is_ok());
    }
}

#[cfg(windows)]
#[test]
fn native_held_inputs_and_generated_files_deny_write_and_delete() {
    for review in [true, false] {
        let case = if review {
            ProcessCase::prepare("linear")
        } else {
            ProcessCase::ready("linear")
        };
        let originals = case.protected();
        let before = originals.clone();
        let output = if review {
            case.review.clone()
        } else {
            case.output.clone()
        };
        let observations = Arc::new(Mutex::new(vec![]));
        let captured = observations.clone();
        let result = execute(&case, review, move |stage| {
            let files = if stage == "inputs_held" {
                before.clone()
            } else {
                snapshot(&output)
            };
            for (path, bytes) in files {
                // Profile is not consumed by process-review, so it has no held guard there.
                if review && path.file_name().is_some_and(|n| n == "profile.json") {
                    continue;
                }
                let wrote = fs::write(&path, &bytes);
                let deleted = fs::remove_file(&path);
                captured.lock().unwrap().push(json!({"stage":stage,"path":path,"write_succeeded":wrote.is_ok(),"write_os_error":wrote.err().and_then(|e|e.raw_os_error()),"delete_succeeded":deleted.is_ok(),"delete_os_error":deleted.err().and_then(|e|e.raw_os_error())}));
            }
        });
        let rows = observations.lock().unwrap().clone();
        write(
            case.root.join("native-held-denials.json"),
            &json!({"review":review,"rows":rows,"run_succeeded":result.is_ok()}),
        );
        assert!(result.is_ok());
        assert_snapshot(&originals);
        assert!(!rows.is_empty());
        for row in rows {
            assert_eq!(row["write_succeeded"], false, "{row}");
            assert_eq!(row["delete_succeeded"], false, "{row}");
            assert_eq!(row["write_os_error"], 32);
            assert_eq!(row["delete_os_error"], 32);
        }
    }
}
