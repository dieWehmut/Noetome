use super::*;
use std::process::Command;
use std::{
    fs,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
fn command(mode: &str) -> ParserCommand {
    let python = std::env::var_os("NOETOME_TEST_PYTHON").unwrap_or_else(|| "python".into());
    ParserCommand {
        program: python,
        args: vec![
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/support/fake_parser.py")
                .into_os_string(),
            mode.into(),
        ],
    }
}
fn setup() -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/sidecar-tests")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&root).unwrap();
    let input = root.join("selected.txt");
    fs::write(&input, b"selected input").unwrap();
    let sentinel = root.join("outside.txt");
    fs::write(&sentinel, b"outside sentinel").unwrap();
    (input, root.join("new-workspace"), sentinel)
}
fn invoke(mode: &str, limits: Limits) -> Result<ValidatedResponse, SidecarError> {
    let (input, workspace, sentinel) = setup();
    let result = run_inner(
        &command(mode),
        &input,
        &workspace,
        limits,
        Arc::new(AtomicBool::new(false)),
        true,
    );
    assert_eq!(fs::read(input).unwrap(), b"selected input");
    assert_eq!(fs::read(sentinel).unwrap(), b"outside sentinel");
    result
}
macro_rules! reject {
    ($name:ident,$mode:literal,$code:literal) => {
        #[test]
        fn $name() {
            assert_eq!(invoke($mode, Limits::default()).unwrap_err().code, $code);
        }
    };
}
reject!(
    valid_json_with_nonzero_exit_is_failure,
    "nonzero",
    "SIDECAR_EXIT"
);
reject!(crashed_process, "crash", "SIDECAR_EXIT");
reject!(empty_response, "empty", "SIDECAR_PROTOCOL");
reject!(invalid_utf8_response, "invalid_utf8", "SIDECAR_PROTOCOL");
reject!(invalid_json_response, "invalid_json", "SIDECAR_PROTOCOL");
reject!(truncated_response, "truncated", "SIDECAR_PROTOCOL");
reject!(second_stdout_json, "extra", "SIDECAR_PROTOCOL");
reject!(unknown_field, "unknown", "SIDECAR_PROTOCOL");
reject!(unknown_nested_field, "nested_unknown", "SIDECAR_PROTOCOL");
reject!(unknown_enum, "enum", "SIDECAR_PROTOCOL");
reject!(unknown_capability, "capability", "SIDECAR_PROTOCOL");
reject!(unsupported_version, "version", "SIDECAR_VERSION");
reject!(
    unsupported_capability_version,
    "capability_version",
    "SIDECAR_VERSION"
);
reject!(wrong_request_id, "request_id", "SIDECAR_IDENTITY");
reject!(wrong_sha, "sha", "SIDECAR_IDENTITY");
reject!(wrong_length, "length", "SIDECAR_IDENTITY");
reject!(wrong_workspace_identity, "workspace_id", "SIDECAR_IDENTITY");
reject!(empty_parser_version, "parser_empty", "SIDECAR_VERSION");
reject!(duplicate_units, "duplicate_unit", "SIDECAR_UNITS");
reject!(missing_units, "missing_unit", "SIDECAR_UNITS");
reject!(missing_ordinal, "ordinal", "SIDECAR_UNITS");
reject!(unknown_issue_reference, "unknown_ref", "SIDECAR_UNITS");
reject!(resource_wrong_sha, "resource_sha", "SIDECAR_RESOURCE");
reject!(resource_wrong_length, "resource_length", "SIDECAR_RESOURCE");
reject!(
    resource_unknown_file,
    "resource_unknown",
    "SIDECAR_RESOURCE"
);
reject!(
    resource_unknown_directory,
    "resource_emptydir",
    "SIDECAR_RESOURCE"
);
reject!(resource_absolute_path, "resource_abs", "SIDECAR_RESOURCE");
reject!(resource_parent_path, "resource_parent", "SIDECAR_RESOURCE");
reject!(
    resource_reserved_name,
    "resource_reserved",
    "SIDECAR_RESOURCE"
);
reject!(
    resource_trailing_dot,
    "resource_trailing",
    "SIDECAR_RESOURCE"
);
reject!(resource_case_collision, "resource_case", "SIDECAR_RESOURCE");
reject!(
    resource_prefix_collision,
    "resource_prefix",
    "SIDECAR_RESOURCE"
);
reject!(
    resource_range_splits_character,
    "resource_utf8range",
    "SIDECAR_UNITS"
);
reject!(
    resource_unknown_reference,
    "resource_reference",
    "SIDECAR_UNITS"
);
reject!(
    resource_unreferenced,
    "resource_unreferenced",
    "SIDECAR_UNITS"
);
reject!(
    resource_invalid_utf8,
    "resource_invalid_utf8",
    "SIDECAR_RESOURCE"
);
#[test]
fn stdout_byte_limit() {
    assert_eq!(
        invoke(
            "stdout_limit",
            Limits {
                response_bytes: 65536,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .code,
        "SIDECAR_LIMIT"
    );
}
reject!(stderr_byte_limit, "stderr_limit", "SIDECAR_LIMIT");
#[test]
fn trailing_whitespace_is_accepted() {
    invoke("trailing", Limits::default()).unwrap();
}
#[test]
fn diagnostics_are_separate() {
    assert_eq!(
        invoke("diagnostics", Limits::default())
            .unwrap()
            .diagnostics,
        b"synthetic diagnostic\n"
    );
}
#[test]
fn concurrent_stderr_is_drained() {
    assert_eq!(
        invoke("concurrent", Limits::default())
            .unwrap()
            .diagnostics
            .len(),
        60000
    );
}
#[test]
fn valid_resources() {
    let result = invoke("valid_resource", Limits::default()).unwrap();
    assert_eq!(result.response.units.len(), 1);
}
#[test]
fn input_write_is_denied() {
    invoke("input_write", Limits::default()).unwrap();
}
#[test]
fn input_byte_limit() {
    assert_eq!(
        invoke(
            "valid",
            Limits {
                input_bytes: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .code,
        "SIDECAR_LIMIT"
    );
}
#[test]
fn request_byte_limit() {
    assert_eq!(
        invoke(
            "valid",
            Limits {
                request_bytes: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .code,
        "SIDECAR_LIMIT"
    );
}
#[test]
fn resource_byte_limit() {
    assert_eq!(
        invoke(
            "valid_resource",
            Limits {
                resource_bytes: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .code,
        "SIDECAR_LIMIT"
    );
}
#[test]
fn total_resource_byte_limit() {
    assert_eq!(
        invoke(
            "valid_resource",
            Limits {
                total_resource_bytes: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .code,
        "SIDECAR_LIMIT"
    );
}
#[test]
fn existing_workspace_is_rejected() {
    let (input, workspace, _) = setup();
    fs::create_dir(&workspace).unwrap();
    assert_eq!(
        run_inner(
            &command("valid"),
            &input,
            &workspace,
            Limits::default(),
            Arc::new(AtomicBool::new(false)),
            true
        )
        .unwrap_err()
        .code,
        "SIDECAR_WORKSPACE"
    );
}
#[test]
fn start_failure() {
    let (input, workspace, _) = setup();
    let cmd = ParserCommand {
        program: "noetome-missing-executable".into(),
        args: vec![],
    };
    assert_eq!(
        run_inner(
            &cmd,
            &input,
            &workspace,
            Limits::default(),
            Arc::new(AtomicBool::new(false)),
            true
        )
        .unwrap_err()
        .code,
        "SIDECAR_START"
    );
}
#[test]
fn no_stdin_reader_times_out() {
    assert_eq!(
        invoke(
            "no_stdin",
            Limits {
                timeout_ms: 100,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .code,
        "SIDECAR_TIMEOUT"
    );
}
#[test]
fn public_entry_rejects_fake() {
    let (input, workspace, _) = setup();
    assert_eq!(
        run(
            &command("valid"),
            &input,
            &workspace,
            Limits::default(),
            Arc::new(AtomicBool::new(false))
        )
        .unwrap_err()
        .code,
        "SIDECAR_PRODUCER"
    );
}
#[test]
fn pre_cancel_does_not_launch() {
    let (input, workspace, _) = setup();
    assert_eq!(
        run_inner(
            &command("valid"),
            &input,
            &workspace,
            Limits::default(),
            Arc::new(AtomicBool::new(true)),
            true
        )
        .unwrap_err()
        .code,
        "SIDECAR_CANCELLED"
    );
    assert!(!workspace.exists());
}
#[test]
fn hardlink_resource_rejected_with_external_sentinel_unchanged() {
    let (input, workspace, sentinel) = setup();
    let mut cmd = command("resource_hardlink");
    cmd.args.push(sentinel.clone().into_os_string());
    let error = run_inner(
        &cmd,
        &input,
        &workspace,
        Limits::default(),
        Arc::new(AtomicBool::new(false)),
        true,
    )
    .unwrap_err();
    assert_eq!(error.code, "SIDECAR_RESOURCE", "{error:?}");
    assert_eq!(fs::read(sentinel).unwrap(), b"outside sentinel");
}
#[cfg(windows)]
#[test]
fn junction_rejected_with_external_sentinel_unchanged() {
    let (input, workspace, sentinel) = setup();
    let mut cmd = command("resource_junction");
    cmd.args
        .push(sentinel.parent().unwrap().to_path_buf().into_os_string());
    let error = run_inner(
        &cmd,
        &input,
        &workspace,
        Limits::default(),
        Arc::new(AtomicBool::new(false)),
        true,
    )
    .unwrap_err();
    assert_eq!(error.code, "SIDECAR_RESOURCE", "{error:?}");
    assert_eq!(fs::read(sentinel).unwrap(), b"outside sentinel");
}
#[cfg(windows)]
fn tree(mode: &str, want: Option<&str>) {
    let (input, workspace, sentinel) = setup();
    let handshake = sentinel.with_file_name("handshake.json");
    let mut cmd = command(mode);
    cmd.args.push(handshake.clone().into_os_string());
    let cancelled = Arc::new(AtomicBool::new(false));
    let token = cancelled.clone();
    let thread = std::thread::spawn(move || {
        run_inner(
            &cmd,
            &input,
            &workspace,
            Limits {
                timeout_ms: 2000,
                response_bytes: 4096,
                ..Limits::default()
            },
            token,
            true,
        )
    });
    let until = Instant::now() + Duration::from_secs(1);
    while !handshake.exists() {
        assert!(Instant::now() < until, "tree handshake timeout");
        std::thread::sleep(Duration::from_millis(5));
    }
    let pids: Vec<u32> = serde_json::from_slice(&fs::read(&handshake).unwrap()).unwrap();
    assert_eq!(pids.len(), 3);
    let probe = |pids: &[u32]| {
        let mut cmd = command("probe");
        cmd.args
            .extend(pids.iter().map(|v| OsString::from(v.to_string())));
        let out = Command::new(cmd.program).args(cmd.args).output().unwrap();
        assert!(out.status.success());
        serde_json::from_slice::<Vec<u32>>(&out.stdout).unwrap()
    };
    assert_eq!(
        probe(&pids),
        pids,
        "root/child/grandchild must be alive before termination"
    );
    if mode == "tree_cancel" {
        cancelled.store(true, Ordering::SeqCst);
    }
    fs::write(handshake.with_extension("json.go"), b"go").unwrap();
    let result = thread.join().unwrap();
    if let Some(code) = want {
        assert_eq!(result.unwrap_err().code, code);
    } else {
        result.unwrap();
    }
    assert!(
        probe(&pids).is_empty(),
        "all descendants must exit before host returns"
    );
    assert_eq!(fs::read(sentinel).unwrap(), b"outside sentinel");
}
#[cfg(windows)]
#[test]
fn timeout_reaps_live_tree() {
    tree("tree_timeout", Some("SIDECAR_TIMEOUT"));
}
#[cfg(windows)]
#[test]
fn cancellation_reaps_live_tree() {
    tree("tree_cancel", Some("SIDECAR_CANCELLED"));
}
#[cfg(windows)]
#[test]
fn overflow_reaps_live_tree() {
    tree("tree_output", Some("SIDECAR_LIMIT"));
}
#[cfg(windows)]
#[test]
fn early_parent_exit_reaps_noninherited_descendants() {
    tree("tree_early", None);
}

reject!(
    missing_nullable_text_field,
    "resource_missing_text",
    "SIDECAR_PROTOCOL"
);
reject!(
    failed_unit_without_issue,
    "resource_failed_no_issue",
    "SIDECAR_UNITS"
);
reject!(
    review_unit_without_issue,
    "resource_review_no_issue",
    "SIDECAR_UNITS"
);
reject!(
    unknown_unit_cannot_be_parsed,
    "resource_unknown_parsed",
    "SIDECAR_UNITS"
);
#[test]
fn review_units_preserve_status_and_issue() {
    let result = invoke("resource_review", Limits::default()).unwrap();
    assert_eq!(result.response.units[0].status, ParseStatus::NeedsReview);
    assert_eq!(result.response.issues.len(), 1);
}
#[test]
fn unit_count_limit() {
    assert_eq!(
        invoke(
            "resource_extra_units",
            Limits {
                units: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .code,
        "SIDECAR_LIMIT"
    );
}
#[test]
fn issue_count_limit() {
    assert_eq!(
        invoke(
            "resource_extra_issue",
            Limits {
                issues: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .code,
        "SIDECAR_LIMIT"
    );
}
#[test]
fn resource_count_limit() {
    assert_eq!(
        invoke(
            "resource_extra_resources",
            Limits {
                resources: 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .code,
        "SIDECAR_LIMIT"
    );
}
#[test]
fn zero_policy_rejected_without_workspace() {
    let (input, workspace, _) = setup();
    assert_eq!(
        run_inner(
            &command("valid"),
            &input,
            &workspace,
            Limits {
                timeout_ms: 0,
                ..Limits::default()
            },
            Arc::new(AtomicBool::new(false)),
            true
        )
        .unwrap_err()
        .code,
        "SIDECAR_CONFIG"
    );
    assert!(!workspace.exists());
}
#[test]
fn bounded_diagnostic_overflow_is_explicit() {
    let error = invoke(
        "stderr_limit",
        Limits {
            diagnostic_bytes: 32,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "SIDECAR_LIMIT");
    assert_eq!(error.diagnostics.len(), 32);
    assert!(error.message.contains("incomplete"));
}
#[test]
fn failed_protocol_retains_diagnostics() {
    let (input, workspace, _) = setup();
    let mut cmd = command("unknown");
    cmd.args[0] = cmd.args[0].clone();
    let error = run_inner(
        &cmd,
        &input,
        &workspace,
        Limits::default(),
        Arc::new(AtomicBool::new(false)),
        true,
    )
    .unwrap_err();
    assert_eq!(error.code, "SIDECAR_PROTOCOL");
    assert_eq!(
        error.diagnostics,
        b"synthetic protocol failure diagnostic\n"
    );
}
#[test]
fn actual_input_fingerprint_is_complete() {
    let result = invoke("valid", Limits::default()).unwrap();
    assert_eq!(
        result.response.input_fingerprint.sha256,
        "9ce76b0fd6d0b46d6e843ddf1a167d8422d6aad723cdb3fc24055aaf3c4b2e2e"
    );
    assert_eq!(result.response.input_fingerprint.byte_length, 14);
}
#[test]
fn input_hardlink_rejected() {
    let (input, workspace, sentinel) = setup();
    fs::remove_file(&input).unwrap();
    fs::hard_link(&sentinel, &input).unwrap();
    assert_eq!(
        run_inner(
            &command("valid"),
            &input,
            &workspace,
            Limits::default(),
            Arc::new(AtomicBool::new(false)),
            true
        )
        .unwrap_err()
        .code,
        "SIDECAR_INPUT"
    );
    assert!(!workspace.exists());
    assert_eq!(fs::read(sentinel).unwrap(), b"outside sentinel");
}
#[cfg(windows)]
#[test]
fn successful_result_holds_resource_and_directory_guards() {
    let (input, workspace, _) = setup();
    let result = run_inner(
        &command("valid_resource"),
        &input,
        &workspace,
        Limits::default(),
        Arc::new(AtomicBool::new(false)),
        true,
    )
    .unwrap();
    assert!(fs::write(workspace.join("text.txt"), b"tampered").is_err());
    assert!(fs::rename(&workspace, workspace.with_file_name("replaced")).is_err());
    drop(result);
    fs::write(workspace.join("text.txt"), b"after guard release").unwrap();
}
#[cfg(windows)]
#[test]
fn blocked_stdin_writer_is_joined_on_timeout() {
    let limits = Limits {
        timeout_ms: 100,
        ..Limits::default()
    };
    let control = Control {
        start: Instant::now(),
        timeout: Duration::from_millis(100),
        cancelled: Arc::new(AtomicBool::new(false)),
    };
    let before = Instant::now();
    assert_eq!(
        transport(
            &command("no_stdin"),
            vec![b'x'; 1024 * 1024],
            &limits,
            &control
        )
        .unwrap_err()
        .code,
        "SIDECAR_TIMEOUT"
    );
    assert!(before.elapsed() < Duration::from_secs(2));
}
reject!(
    resource_directory_depth_limit,
    "resource_deep",
    "SIDECAR_LIMIT"
);
reject!(
    real_parser_zero_units_rejected,
    "parser_zero",
    "SIDECAR_UNITS"
);
