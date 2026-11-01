//! B04-T01 offline process boundary; fake entry exists only in this test binary.
#[path = "support/model_checks.rs"]
mod checks;

type TestCase = (&'static str, fn());

fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|value| value == "--fake-child")
    {
        std::process::exit(checks::fake_child(&arguments[1..]));
    }
    let tests: &[TestCase] = &[
        (
            "frozen_author_and_response_template_are_independently_bound",
            checks::frozen_author_and_response_template_are_independently_bound,
        ),
        (
            "real_process_delivers_twelve_source_bound_candidates",
            checks::real_process_delivers_twelve_source_bound_candidates,
        ),
        (
            "saved_candidate_survives_original_move_and_real_cli_inspect",
            checks::saved_candidate_survives_original_move_and_real_cli_inspect,
        ),
        (
            "budget_and_invalid_source_selection_start_zero_processes",
            checks::budget_and_invalid_source_selection_start_zero_processes,
        ),
        (
            "classified_provider_failures_save_facts_without_candidate",
            checks::classified_provider_failures_save_facts_without_candidate,
        ),
        (
            "malformed_real_responses_and_credentials_never_create_candidate",
            checks::malformed_real_responses_and_credentials_never_create_candidate,
        ),
        (
            "native_timeout_and_cancellation_wait_for_observed_exit",
            checks::native_timeout_and_cancellation_wait_for_observed_exit,
        ),
        (
            "saved_facts_reject_unknown_inventory_and_synchronized_ir_tamper",
            checks::saved_facts_reject_unknown_inventory_and_synchronized_ir_tamper,
        ),
        (
            "existing_and_windows_protected_paths_never_start_process",
            checks::existing_and_windows_protected_paths_never_start_process,
        ),
        (
            "windows_saved_hardlink_and_sharelock_are_rejected",
            checks::windows_saved_hardlink_and_sharelock_are_rejected,
        ),
        (
            "registered_real_pdf_is_refused_before_provider_invocation",
            checks::registered_real_pdf_is_refused_before_provider_invocation,
        ),
        (
            "live_source_and_output_handles_reject_same_byte_replacement",
            checks::live_source_and_output_handles_reject_same_byte_replacement,
        ),
        (
            "unknown_file_inserted_during_invocation_is_preserved",
            checks::unknown_file_inserted_during_invocation_is_preserved,
        ),
        (
            "known_currency_policy_rejects_usdt_and_preserves_usd_settlement",
            checks::known_currency_policy_rejects_usdt_and_preserves_usd_settlement,
        ),
        (
            "empty_response_report_cross_fields_reject_synchronized_tamper",
            checks::empty_response_report_cross_fields_reject_synchronized_tamper,
        ),
    ];
    let mut exact = None;
    let mut list = false;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].to_str() {
            Some("--exact") if index + 1 < arguments.len() => {
                index += 1;
                exact = arguments[index].to_str();
            }
            Some("--list") => list = true,
            Some("--nocapture") => {}
            _ => {
                eprintln!("unknown model test argument: {:?}", arguments[index]);
                std::process::exit(2);
            }
        }
        index += 1;
    }
    if list {
        for (name, _) in tests {
            println!("{name}: test");
        }
        return;
    }
    let selected: Vec<_> = tests
        .iter()
        .filter(|(name, _)| exact.is_none_or(|filter| *name == filter))
        .collect();
    if selected.is_empty() {
        eprintln!("no model test matched the exact selector");
        std::process::exit(2);
    }
    let mut failed = 0;
    for (name, test) in &selected {
        let result = std::panic::catch_unwind(*test);
        println!(
            "test {name} ... {}",
            if result.is_ok() { "ok" } else { "FAILED" }
        );
        failed += usize::from(result.is_err());
    }
    println!(
        "model process tests: {} passed; {failed} failed",
        selected.len() - failed
    );
    if failed != 0 {
        std::process::exit(1);
    }
}
