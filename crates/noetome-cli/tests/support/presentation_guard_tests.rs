//! Stage guards exercise the normal presentation entry with owned test roots.
//! Test hooks exist only in cfg(test); ordinary CLI receives no fault switches.
use super::test_case::PresentationCase;
use super::*;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

type FileBytes = BTreeMap<PathBuf, Vec<u8>>;

fn byte_pin(bytes: &[u8]) -> Value {
    json!({"sha256":test_case::hash(bytes),"byte_length":bytes.len()})
}

fn snapshot(root: &Path) -> FileBytes {
    let mut files = BTreeMap::new();
    fn gather(path: &Path, files: &mut FileBytes) {
        assert!(!path.is_symlink());
        if path.is_file() {
            files.insert(path.to_owned(), fs::read(path).unwrap());
        } else {
            for entry in fs::read_dir(path).unwrap() {
                gather(&entry.unwrap().path(), files);
            }
        }
    }
    gather(root, &mut files);
    files
}

fn pins(files: &FileBytes) -> Value {
    Value::Object(
        files
            .iter()
            .map(|(path, bytes)| (path.to_string_lossy().into_owned(), byte_pin(bytes)))
            .collect(),
    )
}

fn current_pins(files: &FileBytes) -> Value {
    Value::Object(
        files
            .keys()
            .map(|path| {
                let actual = fs::read(path)
                    .map(|bytes| byte_pin(&bytes))
                    .unwrap_or_else(|error| json!({"read_error":error.to_string()}));
                (path.to_string_lossy().into_owned(), actual)
            })
            .collect(),
    )
}

fn originals(f: &PresentationCase) -> FileBytes {
    let mut original = snapshot(&f.project);
    original.extend(snapshot(&f.case));
    original.extend(snapshot(&f.output));
    original
}

fn assert_originals(original: &FileBytes) {
    for (path, bytes) in original {
        assert_eq!(
            fs::read(path).unwrap(),
            *bytes,
            "original changed: {}",
            path.display()
        );
    }
}

fn ready(version: u64) -> PresentationCase {
    let mut f = match version {
        1 => PresentationCase::prepare_legacy("legacy_v1_two_objects"),
        2 => PresentationCase::prepare_legacy("legacy_v2_two_objects"),
        3 => PresentationCase::prepare("two_objects"),
        _ => unreachable!(),
    };
    let coordinated = f.coordinate();
    assert_eq!(
        coordinated.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&coordinated.stderr)
    );
    f.freeze_coordination();
    println!(
        "presentation_guard_root={} coordination_schema={version}",
        f.root.display()
    );
    f
}

struct ClearHook;
impl Drop for ClearHook {
    fn drop(&mut self) {
        TEST_HOOK.with(|hook| *hook.borrow_mut() = None);
    }
}

fn execute(
    f: &PresentationCase,
    action: impl FnMut(&str) + 'static,
) -> Result<PresentationOutput, ProjectError> {
    TEST_HOOK.with(|hook| {
        assert!(hook.borrow().is_none(), "a previous test hook leaked");
        *hook.borrow_mut() = Some(Box::new(action));
    });
    let _reset = ClearHook;
    run(&f.project, &f.case, &f.output, &f.preview)
}

fn result_receipt(result: &Result<PresentationOutput, ProjectError>) -> Value {
    match result {
        Ok(value) => {
            json!({"returned":"success","failed":value.failed,"report":serde_json::from_str::<Value>(&value.json).unwrap()})
        }
        Err(error) => {
            json!({"returned":"error","code":error.code(),"exit_code":error.exit_code(),"message":error.to_string()})
        }
    }
}

fn receipt(f: &PresentationCase, value: Value) {
    fs::write(
        f.root.join("guard-receipt.json"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
}

fn failure(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_owned()))
        .unwrap_or_else(|| "unexpected panic payload".to_owned())
}

fn assert_captured_cleanup(captured: &FileBytes, preserved_plan: bool) {
    assert!(
        captured
            .keys()
            .any(|path| path.file_name().unwrap() == "plan.json")
    );
    assert!(
        captured
            .keys()
            .any(|path| path.file_name().unwrap() == "presentation.json")
    );
    assert!(
        captured
            .keys()
            .any(|path| path.file_name().unwrap() == "presentation.md")
    );
    for (path, bytes) in captured {
        let name = path.file_name().unwrap().to_str().unwrap();
        if name.starts_with('.') {
            assert_eq!(
                fs::read(path).unwrap(),
                *bytes,
                "hidden original must survive: {}",
                path.display()
            );
        } else if name == "plan.json" && preserved_plan {
            assert!(path.is_file(), "changed plan must survive cleanup");
        } else {
            assert!(
                !path.exists(),
                "unchanged captured success result remains: {}",
                path.display()
            );
        }
    }
}

#[test]
fn inputs_held_inventory_changes_reject_before_preview_creation() {
    let mut failures = vec![];
    for scope in ["case", "source"] {
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let f = ready(3);
            let original = originals(&f);
            let path = if scope == "case" {
                f.case.join("human-before-output.txt")
            } else {
                f.project.join("human-before-output.txt")
            };
            let injected = path.clone();
            let observations = Arc::new(Mutex::new(vec![]));
            let observed = observations.clone();
            let result = execute(&f, move |stage| {
                if stage == "inputs_held" {
                    let path = injected.clone();
                    let outcome = std::thread::spawn(move || {
                        let wrote = fs::write(&path, b"human input addition before output\n");
                        json!({"stage":"inputs_held","path":path,"operation":"create unknown file","write_succeeded":wrote.is_ok(),"error":wrote.err().map(|error|error.to_string())})
                    }).join().unwrap();
                    observed.lock().unwrap().push(outcome);
                }
            });
            let attempts = observations.lock().unwrap().clone();
            receipt(
                &f,
                json!({"entrypoint":"knowledge_presentation::run","scenario":"pre_output_inventory_change","scope":scope,"stage":"inputs_held","observations":attempts,"result":result_receipt(&result),"preview_created":f.preview.exists(),"original_before":pins(&original),"original_after":current_pins(&original)}),
            );
            assert_eq!(attempts.len(), 1);
            assert_eq!(attempts[0]["write_succeeded"], true);
            assert_eq!(
                fs::read(&path).unwrap(),
                b"human input addition before output\n"
            );
            assert!(!f.preview.exists());
            let error = result.err().expect("changed inventory accepted");
            assert_eq!(
                error.code(),
                if scope == "case" {
                    "COORD_INPUT_CHANGED"
                } else {
                    "EXTRACT_SOURCE_CHANGED"
                }
            );
            assert_eq!(error.exit_code(), 3);
            assert_originals(&original);
            f.assert_protected();
        }));
        if let Err(panic) = checked {
            failures.push(format!("{scope}: {}", failure(panic)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn results_saved_inventory_changes_preserve_originals_and_unknown_bytes() {
    let mut failures = vec![];
    for scope in ["case", "source", "coordination", "output"] {
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let f = ready(3);
            let original = originals(&f);
            let path = match scope {
                "case" => f.case.join("human-after-save.txt"),
                "source" => f.project.join("human-after-save.txt"),
                "coordination" => f.output.join("human-after-save.txt"),
                _ => f.preview.join("human-after-save.txt"),
            };
            let injected = path.clone();
            let preview = f.preview.clone();
            let observations = Arc::new(Mutex::new(vec![]));
            let observed = observations.clone();
            let captured = Arc::new(Mutex::new(FileBytes::new()));
            let saved = captured.clone();
            let result = execute(&f, move |stage| {
                if stage == "results_saved" {
                    *saved.lock().unwrap() = snapshot(&preview);
                    let path = injected.clone();
                    let outcome = std::thread::spawn(move || {
                        let wrote = fs::write(&path, b"human bytes added after results saved\n");
                        json!({"stage":"results_saved","path":path,"operation":"create unknown file","write_succeeded":wrote.is_ok(),"error":wrote.err().map(|error|error.to_string())})
                    }).join().unwrap();
                    observed.lock().unwrap().push(outcome);
                }
            });
            let attempts = observations.lock().unwrap().clone();
            let captured = captured.lock().unwrap().clone();
            receipt(
                &f,
                json!({"entrypoint":"knowledge_presentation::run","scenario":"final_inventory_change","scope":scope,"stage":"results_saved","observations":attempts,"result":result_receipt(&result),"original_before":pins(&original),"original_after":current_pins(&original),"captured_before":pins(&captured),"captured_after":current_pins(&captured)}),
            );
            assert_eq!(attempts.len(), 1);
            assert_eq!(attempts[0]["write_succeeded"], true);
            assert_eq!(
                fs::read(&path).unwrap(),
                b"human bytes added after results saved\n"
            );
            let error = result.err().expect("changed inventory accepted");
            assert_eq!(
                error.code(),
                match scope {
                    "source" => "EXTRACT_SOURCE_CHANGED",
                    "output" => "EXTRACT_OUTPUT_CHANGED",
                    _ => "COORD_INPUT_CHANGED",
                }
            );
            assert_eq!(error.exit_code(), 3);
            assert_captured_cleanup(&captured, false);
            assert_originals(&original);
            f.assert_protected();
        }));
        if let Err(panic) = checked {
            failures.push(format!("{scope}: {}", failure(panic)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[cfg(unix)]
#[test]
fn changed_plan_and_unknown_file_survive_late_cleanup_on_unix() {
    let f = ready(3);
    let original = originals(&f);
    let preview = f.preview.clone();
    let captured = Arc::new(Mutex::new(FileBytes::new()));
    let saved = captured.clone();
    let observations = Arc::new(Mutex::new(vec![]));
    let observed = observations.clone();
    let changed_plan = b"{\"human_changed_plan\":true}\n";
    let result = execute(&f, move |stage| {
        if stage == "results_saved" {
            *saved.lock().unwrap() = snapshot(&preview);
            let preview = preview.clone();
            let outcome = std::thread::spawn(move || {
                let plan = preview.join("plan.json");
                let unknown = preview.join("human-late-output.txt");
                let changed = fs::write(&plan, changed_plan);
                let added = fs::write(&unknown, b"preserve human late output\n");
                json!({"stage":"results_saved","changed_plan_path":plan,"unknown_path":unknown,"plan_write_succeeded":changed.is_ok(),"unknown_write_succeeded":added.is_ok()})
            }).join().unwrap();
            observed.lock().unwrap().push(outcome);
        }
    });
    let captured = captured.lock().unwrap().clone();
    let attempts = observations.lock().unwrap().clone();
    receipt(
        &f,
        json!({"entrypoint":"knowledge_presentation::run","scenario":"changed_captured_plan_and_unknown","stage":"results_saved","observations":attempts,"result":result_receipt(&result),"original_before":pins(&original),"original_after":current_pins(&original),"captured_before":pins(&captured),"captured_after":current_pins(&captured)}),
    );
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0]["plan_write_succeeded"], true);
    assert_eq!(attempts[0]["unknown_write_succeeded"], true);
    assert_eq!(result.err().unwrap().code(), "EXTRACT_OUTPUT_CHANGED");
    assert_eq!(fs::read(f.preview.join("plan.json")).unwrap(), changed_plan);
    assert_eq!(
        fs::read(f.preview.join("human-late-output.txt")).unwrap(),
        b"preserve human late output\n"
    );
    assert_captured_cleanup(&captured, true);
    assert_originals(&original);
    f.assert_protected();
}

#[cfg(windows)]
#[test]
fn windows_v1_v2_v3_held_inputs_and_saved_results_deny_actual_write_and_delete() {
    let mut failures = vec![];
    for version in [1, 2, 3] {
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let f = ready(version);
            let original = originals(&f);
            let mut held_paths: BTreeSet<PathBuf> = snapshot(&f.case).into_keys().collect();
            held_paths.extend(snapshot(&f.output).into_keys());
            held_paths.insert(f.project.join("project.sqlite"));
            for input in ["a", "b"] {
                let imported: Value = serde_json::from_slice(
                    &fs::read(f.root.join(format!("import-{input}-stdout.txt"))).unwrap(),
                )
                .unwrap();
                for field in ["raw_snapshot", "normalized_snapshot"] {
                    held_paths.insert(f.project.join(imported["version"][field].as_str().unwrap()));
                }
            }
            let preview = f.preview.clone();
            let observations = Arc::new(Mutex::new(vec![]));
            let observed = observations.clone();
            let captured = Arc::new(Mutex::new(FileBytes::new()));
            let saved = captured.clone();
            let result = execute(&f, move |stage| {
                if !matches!(stage, "inputs_held" | "results_saved") {
                    return;
                }
                let mut paths = held_paths.clone();
                if stage == "results_saved" {
                    let results = snapshot(&preview);
                    paths.extend(results.keys().cloned());
                    *saved.lock().unwrap() = results;
                }
                for path in paths {
                    let phase = stage.to_owned();
                    let outcome = std::thread::spawn(move || {
                        let before = fs::read(&path).unwrap();
                        let wrote = fs::write(&path, b"guard test forbidden replacement\n");
                        let removed = fs::remove_file(&path);
                        let after = fs::read(&path);
                        let write_denied = wrote.is_err();
                        let delete_denied = removed.is_err();
                        json!({"stage":phase,"path":path,"operation":"actual fs::write and fs::remove_file from another thread","write_denied":write_denied,"delete_denied":delete_denied,
                            "write_error":wrote.err().map(|error|json!({"kind":format!("{:?}",error.kind()),"raw_os_error":error.raw_os_error(),"message":error.to_string()})),
                            "delete_error":removed.err().map(|error|json!({"kind":format!("{:?}",error.kind()),"raw_os_error":error.raw_os_error(),"message":error.to_string()})),
                            "before":byte_pin(&before),"after":after.as_ref().map(|bytes|byte_pin(bytes)).unwrap_or_else(|error|json!({"read_error":error.to_string()})),"bytes_unchanged":after.as_ref().is_ok_and(|bytes|*bytes==before)})
                    }).join().unwrap();
                    observed.lock().unwrap().push(outcome);
                }
            });
            let attempts = observations.lock().unwrap().clone();
            let captured = captured.lock().unwrap().clone();
            receipt(
                &f,
                json!({"entrypoint":"knowledge_presentation::run","scenario":"Windows_native_held_file_operations","coordination_schema_version":version,"observations":attempts,"result":result_receipt(&result),"original_before":pins(&original),"original_after":current_pins(&original),"saved_result_before":pins(&captured),"saved_result_after":current_pins(&captured)}),
            );
            for stage in ["inputs_held", "results_saved"] {
                assert!(
                    attempts.iter().any(|attempt| attempt["stage"] == stage),
                    "stage never observed: {stage}"
                );
            }
            for attempt in attempts {
                assert_eq!(attempt["write_denied"], true, "{attempt}");
                assert_eq!(attempt["delete_denied"], true, "{attempt}");
                assert_eq!(attempt["bytes_unchanged"], true, "{attempt}");
            }
            assert!(!result.unwrap().failed);
            assert_originals(&original);
            assert_originals(&captured);
            f.assert_protected();
        }));
        if let Err(panic) = checked {
            failures.push(format!("v{version}: {}", failure(panic)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
