//! Offline credential handoff; private child exists only in this test binary.
#[path = "support/credential_checks.rs"]
mod checks;

fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|arg| arg == "--credential-fake-child")
    {
        std::process::exit(checks::fake_child(&arguments[1..]));
    }
    let tests: &[(&str, fn())] = &[
        (
            "valid_frame_is_one_request_and_only_systemroot_is_inherited",
            checks::valid_frame_is_one_request_and_only_systemroot_is_inherited,
        ),
        (
            "credential_shapes_and_missing_explicit_reference_are_fixed_errors",
            checks::credential_shapes_and_missing_explicit_reference_are_fixed_errors,
        ),
        (
            "invalid_request_and_secret_in_request_or_argv_start_zero_children",
            checks::invalid_request_and_secret_in_request_or_argv_start_zero_children,
        ),
        (
            "all_sensitive_echo_and_malformed_streams_are_suppressed",
            checks::all_sensitive_echo_and_malformed_streams_are_suppressed,
        ),
        (
            "clean_bad_candidate_preserves_auditable_usage",
            checks::clean_bad_candidate_preserves_auditable_usage,
        ),
        (
            "native_timeout_and_cancel_have_observed_exit_and_no_secret_facts",
            checks::native_timeout_and_cancel_have_observed_exit_and_no_secret_facts,
        ),
    ];
    let exact = if arguments.len() == 2 && arguments[0] == "--exact" {
        arguments[1].to_str()
    } else if arguments.is_empty() {
        None
    } else {
        eprintln!("CREDENTIAL_TEST_ARGUMENT_INVALID");
        std::process::exit(2);
    };
    let selected: Vec<_> = tests
        .iter()
        .filter(|(name, _)| exact.is_none_or(|chosen| chosen == *name))
        .collect();
    if selected.is_empty() {
        eprintln!("CREDENTIAL_TEST_NOT_FOUND");
        std::process::exit(2);
    }
    let mut failures = 0;
    for (name, test) in &selected {
        let passed = std::panic::catch_unwind(*test).is_ok();
        println!("test {name} ... {}", if passed { "ok" } else { "FAILED" });
        failures += usize::from(!passed);
    }
    println!(
        "credential process tests: {} passed; {failures} failed",
        selected.len() - failures
    );
    if failures > 0 {
        std::process::exit(1);
    }
}
