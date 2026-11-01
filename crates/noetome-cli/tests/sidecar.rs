//! Native-process tests with a private argv entry and no libtest stdout banner.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant, SystemTime};

use noetome_cli::sidecar::{
    AdapterIdentity, AdapterSpec, SidecarError, SidecarLimits, ValidationScope, VerifiedResponse,
    run_sidecar,
};
use serde_json::{Value, json};
use uuid::Uuid;

#[path = "support/sidecar_fake.rs"]
mod fake;

type TestCase = (&'static str, fn());
static HOST_CALLS: AtomicUsize = AtomicUsize::new(0);
static NATIVE_ROOTS: AtomicUsize = AtomicUsize::new(0);
static DIRECT_SMOKES: AtomicUsize = AtomicUsize::new(0);
static OBSERVED_GRANDCHILDREN: AtomicUsize = AtomicUsize::new(0);

fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|value| value == fake::ENTRY) {
        std::process::exit(fake::main(&arguments[1..]));
    }
    let tests: &[TestCase] = &[
        (
            "valid_response_from_real_test_process_preserves_reviewed_resources",
            valid_response_from_real_test_process_preserves_reviewed_resources,
        ),
        (
            "missing_executable_is_a_real_spawn_failure",
            missing_executable_is_a_real_spawn_failure,
        ),
        (
            "strict_wire_json_is_rejected_from_real_processes",
            strict_wire_json_is_rejected_from_real_processes,
        ),
        (
            "strict_object_and_enum_shapes_are_rejected_from_real_processes",
            strict_object_and_enum_shapes_are_rejected_from_real_processes,
        ),
        (
            "real_identity_and_capability_mutations_are_rejected",
            real_identity_and_capability_mutations_are_rejected,
        ),
        (
            "source_inventory_cannot_hide_missing_or_repeated_units",
            source_inventory_cannot_hide_missing_or_repeated_units,
        ),
        (
            "real_range_and_content_mutations_are_rejected",
            real_range_and_content_mutations_are_rejected,
        ),
        (
            "reported_partial_and_failure_preserve_verified_diagnostics",
            reported_partial_and_failure_preserve_verified_diagnostics,
        ),
        (
            "nonzero_exit_and_crash_cannot_use_complete_json",
            nonzero_exit_and_crash_cannot_use_complete_json,
        ),
        (
            "stdout_exact_boundary_and_one_excess_byte",
            stdout_exact_boundary_and_one_excess_byte,
        ),
        (
            "stderr_boundaries_zero_and_binary_are_explicit",
            stderr_boundaries_zero_and_binary_are_explicit,
        ),
        (
            "stderr_is_drained_before_child_reads_request",
            stderr_is_drained_before_child_reads_request,
        ),
        (
            "physical_resources_and_inventory_are_rechecked",
            physical_resources_and_inventory_are_rechecked,
        ),
        (
            "response_resource_paths_do_not_expand_capabilities",
            response_resource_paths_do_not_expand_capabilities,
        ),
        (
            "malformed_partial_is_not_misclassified_as_verified_failure",
            malformed_partial_is_not_misclassified_as_verified_failure,
        ),
        (
            "real_tree_timeout_covers_root_and_pipe_holding_descendants",
            real_tree_timeout_covers_root_and_pipe_holding_descendants,
        ),
        (
            "real_tree_cancellation_is_observed_after_live_handshake",
            real_tree_cancellation_is_observed_after_live_handshake,
        ),
        (
            "real_tree_limits_end_descendants_and_both_pipes",
            real_tree_limits_end_descendants_and_both_pipes,
        ),
        (
            "utf8_artifact_characters_are_checked_independently",
            utf8_artifact_characters_are_checked_independently,
        ),
        (
            "artifact_limits_are_checked_at_exact_boundaries",
            artifact_limits_are_checked_at_exact_boundaries,
        ),
        (
            "request_and_manifest_counts_have_exact_limits",
            request_and_manifest_counts_have_exact_limits,
        ),
        (
            "preflight_limits_and_existing_roots_never_start_fake",
            preflight_limits_and_existing_roots_never_start_fake,
        ),
        (
            "real_windows_junctions_and_hardlinks_are_rejected",
            real_windows_junctions_and_hardlinks_are_rejected,
        ),
        (
            "late_resource_inventory_addition_is_rejected",
            late_resource_inventory_addition_is_rejected,
        ),
        (
            "cancellation_during_host_resource_verification_is_rejected",
            cancellation_during_host_resource_verification_is_rejected,
        ),
    ];
    let mut exact: Option<&str> = None;
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
                eprintln!("unknown sidecar test argument: {:?}", arguments[index]);
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
        eprintln!("no sidecar test matched the exact selector");
        std::process::exit(2);
    }
    let mut failed = 0;
    for (name, test) in &selected {
        let result = std::panic::catch_unwind(*test);
        println!(
            "test {name} ... {}",
            if result.is_ok() { "ok" } else { "FAILED" }
        );
        if result.is_err() {
            failed += 1;
        }
    }
    println!(
        "sidecar process tests: {} passed; {failed} failed",
        selected.len() - failed
    );
    println!(
        "observed counts: {} host calls; {} native roots; {} direct fake smokes; {} handle-observed grandchildren",
        HOST_CALLS.load(Ordering::Relaxed),
        NATIVE_ROOTS.load(Ordering::Relaxed),
        DIRECT_SMOKES.load(Ordering::Relaxed),
        OBSERVED_GRANDCHILDREN.load(Ordering::Relaxed)
    );
    if failed != 0 {
        std::process::exit(1);
    }
}

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let parent =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/sidecar-process-tests");
        fs::create_dir_all(&parent).unwrap();
        let root = parent
            .canonicalize()
            .unwrap()
            .join(Uuid::new_v4().to_string());
        fs::create_dir(&root).unwrap();
        fs::write(root.join("selected-input.bin"), b"abc").unwrap();
        fs::write(root.join("external-sentinel.md"), "作者人工哨兵保留。\n").unwrap();
        Self { root }
    }
    fn input(&self) -> PathBuf {
        self.root.join("selected-input.bin")
    }
    fn control(&self, name: &str) -> PathBuf {
        let control = self.root.join(name);
        fs::create_dir(&control).unwrap();
        control
    }
    fn adapter(&self, control: &Path) -> AdapterSpec {
        AdapterSpec {
            program: std::env::current_exe().unwrap(),
            args: vec![
                fake::ENTRY.into(),
                "valid".into(),
                control.as_os_str().to_owned(),
                Uuid::new_v4().to_string().into(),
            ],
            expected: serde_json::from_value::<AdapterIdentity>(
                fake::response(fake::UUID)["adapter"].clone(),
            )
            .unwrap(),
        }
    }
    fn scenario(&self, control: &Path, name: &str) -> AdapterSpec {
        let mut adapter = self.adapter(control);
        adapter.args[1] = name.into();
        adapter
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/sidecar-process-tests")
            .canonicalize()
            .unwrap();
        let resolved = self.root.canonicalize().unwrap();
        assert!(resolved != parent && resolved.starts_with(&parent));
        unlink_owned_links(&self.root);
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn unlink_owned_links(path: &Path) {
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let metadata = fs::symlink_metadata(entry.path()).unwrap();
        let mut link = metadata.file_type().is_symlink();
        let mut directory = metadata.is_dir();
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            link |= metadata.file_attributes() & 0x400 != 0;
            directory |= metadata.file_attributes() & 0x10 != 0;
        }
        if link {
            if directory {
                fs::remove_dir(entry.path()).unwrap();
            } else {
                fs::remove_file(entry.path()).unwrap();
            }
        } else if directory {
            unlink_owned_links(&entry.path());
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Fingerprint {
    bytes: Vec<u8>,
    modified: SystemTime,
    readonly: bool,
    identity: same_file::Handle,
}

fn fingerprint(path: &Path) -> Fingerprint {
    let metadata = fs::metadata(path).unwrap();
    Fingerprint {
        bytes: fs::read(path).unwrap(),
        modified: metadata.modified().unwrap(),
        readonly: metadata.permissions().readonly(),
        identity: same_file::Handle::from_path(path).unwrap(),
    }
}

fn files(root: &Path) -> BTreeSet<PathBuf> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeSet<PathBuf>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            assert!(!kind.is_symlink(), "unexpected link in normal workspace");
            if kind.is_dir() {
                visit(root, &entry.path(), result);
            } else {
                result.insert(entry.path().strip_prefix(root).unwrap().to_owned());
            }
        }
    }
    let mut result = BTreeSet::new();
    visit(root, root, &mut result);
    result
}

fn direct_helper_smoke(sandbox: &Sandbox) {
    // This smoke really launches the fake even while the facade is a non-spawning stub.
    let root = sandbox.root.join("direct-helper-smoke");
    fs::create_dir_all(root.join("input")).unwrap();
    fs::create_dir(root.join("artifacts")).unwrap();
    fs::write(root.join("input/raw.bin"), b"abc").unwrap();
    let control = sandbox.control("smoke-control");
    let adapter = sandbox.adapter(&control);
    let mut child = Command::new(&adapter.program)
        .args(&adapter.args)
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    DIRECT_SMOKES.fetch_add(1, Ordering::Relaxed);
    child
        .stdin
        .take()
        .unwrap()
        .write_all(fake::REQUEST.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "direct helper: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        fake::response(fake::UUID)
    );
    assert_eq!(output.stderr, fake::DIAGNOSTIC);
    assert_eq!(fs::read(root.join("artifacts/A1.bin")).unwrap(), b"abc");
    let marker: Value =
        serde_json::from_slice(&fs::read(control.join("root.json")).unwrap()).unwrap();
    assert_eq!(marker["pid"], pid);
    assert_eq!(marker["nonce"], adapter.args[3].to_str().unwrap());
    println!("direct fake smoke pid {pid}: unique reviewed JSON and abc resource confirmed");
}

fn valid_response_from_real_test_process_preserves_reviewed_resources() {
    let sandbox = Sandbox::new();
    let input_before = fingerprint(&sandbox.input());
    let sentinel = sandbox.root.join("external-sentinel.md");
    let sentinel_before = fingerprint(&sentinel);
    direct_helper_smoke(&sandbox);
    let control = sandbox.control("host-control");
    let adapter = sandbox.adapter(&control);
    let root = sandbox.root.join("host-workspace");
    let limits = SidecarLimits {
        timeout: Duration::from_secs(5),
        ..SidecarLimits::default()
    };
    let result = counted_run(
        &adapter,
        &sandbox.input(),
        &root,
        limits,
        &AtomicBool::new(false),
    )
    .expect("valid real child response must pass the production facade");
    assert_eq!(result.validation_scope, ValidationScope::ProtocolOnly);
    let request = serde_json::to_value(&result.request).unwrap();
    let request_id = request["request_id"].as_str().unwrap();
    let id = Uuid::parse_str(request_id).unwrap();
    assert_eq!(id.get_version_num(), 4);
    assert_eq!(id.to_string(), request_id);
    assert_eq!(request["schema_version"], 1);
    assert_eq!(request["operation"], "inspect_bytes");
    assert_eq!(
        request["input"],
        json!({"relative_path":"input/raw.bin","byte_length":3,"sha256":fake::SHA})
    );
    assert_eq!(
        request["workspace"],
        json!({"artifact_directory":"artifacts"})
    );
    assert_eq!(
        request["artifact_policy"],
        json!({"max_file_bytes":limits.max_artifact_bytes,"max_total_bytes":limits.max_total_artifact_bytes,"max_files":limits.max_artifacts})
    );
    assert_eq!(
        serde_json::to_value(result.response).unwrap(),
        fake::response(request_id)
    );
    assert_eq!(result.diagnostics, fake::DIAGNOSTIC);
    assert!(result.pid > 0);
    assert!(result.elapsed <= limits.timeout + limits.cleanup_timeout);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(root.join("request.json")).unwrap()).unwrap(),
        request
    );
    assert_eq!(fs::read(root.join("input/raw.bin")).unwrap(), b"abc");
    assert_eq!(fs::read(root.join("artifacts/A1.bin")).unwrap(), b"abc");
    assert_eq!(
        files(&root),
        BTreeSet::from(
            [
                ".owner.json",
                "request.json",
                "input/raw.bin",
                "artifacts/A1.bin"
            ]
            .map(PathBuf::from)
        )
    );
    assert_eq!(fingerprint(&sandbox.input()), input_before);
    assert_eq!(fingerprint(&sentinel), sentinel_before);
}

fn missing_executable_is_a_real_spawn_failure() {
    let sandbox = Sandbox::new();
    let control = sandbox.control("never-started-control");
    let mut adapter = sandbox.adapter(&control);
    adapter.program = sandbox.root.join("nonexistent-sidecar.exe");
    let input_before = fingerprint(&sandbox.input());
    let sentinel = sandbox.root.join("external-sentinel.md");
    let sentinel_before = fingerprint(&sentinel);
    let root = sandbox.root.join("spawn-failure-workspace");
    let error = counted_run(
        &adapter,
        &sandbox.input(),
        &root,
        SidecarLimits::default(),
        &AtomicBool::new(false),
    )
    .expect_err("a nonexistent native executable cannot start");
    assert_eq!(error.code, "SIDECAR_SPAWN_FAILED");
    assert!(error.pid.is_none());
    assert!(error.response.is_none());
    assert!(!control.join("root.json").exists());
    assert_eq!(fingerprint(&sandbox.input()), input_before);
    assert_eq!(fingerprint(&sentinel), sentinel_before);
}

fn bounded_run(
    adapter: AdapterSpec,
    input: PathBuf,
    root: PathBuf,
    limits: SidecarLimits,
    cancel: Arc<AtomicBool>,
) -> Result<VerifiedResponse, SidecarError> {
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = counted_run(&adapter, &input, &root, limits, &cancel);
        let _ = sender.send(result);
    });
    match receiver.recv_timeout(Duration::from_secs(15)) {
        Ok(result) => {
            worker.join().unwrap();
            result
        }
        Err(error) => {
            // Exit closes non-inheritable host Job handles if broken code never returns.
            // This is an explicit failed watchdog, never a claimed successful cleanup.
            eprintln!("SIDECAR_TEST_WATCHDOG: host did not return within 15 seconds: {error}");
            std::process::exit(1);
        }
    }
}

fn counted_run(
    adapter: &AdapterSpec,
    input: &Path,
    root: &Path,
    limits: SidecarLimits,
    cancel: &AtomicBool,
) -> Result<VerifiedResponse, SidecarError> {
    HOST_CALLS.fetch_add(1, Ordering::Relaxed);
    let result = run_sidecar(adapter, input, root, limits, cancel);
    if result
        .as_ref()
        .map(|value| value.pid > 0)
        .unwrap_or_else(|error| error.pid.is_some())
    {
        NATIVE_ROOTS.fetch_add(1, Ordering::Relaxed);
    }
    result
}

fn scenario(name: &str, limits: SidecarLimits) -> Result<VerifiedResponse, SidecarError> {
    scenario_input(name, b"abc", limits)
}

fn scenario_input(
    name: &str,
    input: &[u8],
    limits: SidecarLimits,
) -> Result<VerifiedResponse, SidecarError> {
    let sandbox = Sandbox::new();
    fs::write(sandbox.input(), input).unwrap();
    let before = fingerprint(&sandbox.input());
    let sentinel = sandbox.root.join("external-sentinel.md");
    let sentinel_before = fingerprint(&sentinel);
    let control = sandbox.control("scenario-control");
    let external = control.join("external");
    if name == "resource-junction" {
        fs::create_dir(&external).unwrap();
        fs::write(
            external.join("sentinel.bin"),
            b"junction external sentinel\n",
        )
        .unwrap();
    }
    let external_before =
        (name == "resource-junction").then(|| fingerprint(&external.join("sentinel.bin")));
    let adapter = sandbox.scenario(&control, name);
    let root = sandbox.root.join("workspace");
    let result = bounded_run(
        adapter,
        sandbox.input(),
        root.clone(),
        limits,
        Arc::new(AtomicBool::new(false)),
    );
    assert!(
        control.join("root.json").is_file(),
        "{name}: helper did not actually start: {result:?}"
    );
    if let Some(before) = external_before {
        assert_eq!(fingerprint(&external.join("sentinel.bin")), before);
    }
    if name == "utf8-valid" && result.is_ok() {
        assert_eq!(
            fs::read(root.join("artifacts/A1.txt")).unwrap(),
            "é".as_bytes()
        );
    }
    assert_eq!(
        fingerprint(&sandbox.input()),
        before,
        "{name}: selected input changed"
    );
    assert_eq!(
        fingerprint(&sentinel),
        sentinel_before,
        "{name}: external sentinel changed"
    );
    assert!(
        root.is_dir(),
        "{name}: confirmed process workspace was removed"
    );
    result
}

fn small_limits() -> SidecarLimits {
    SidecarLimits {
        timeout: Duration::from_secs(5),
        ..SidecarLimits::default()
    }
}

fn rejected(name: &str, code: &str, rule: Option<&str>) -> SidecarError {
    let error = scenario(name, small_limits()).expect_err(&format!(
        "{name}: malicious real process response must fail"
    ));
    assert_eq!(error.code, code, "{name}: {error:?}");
    assert!(
        error.pid.is_some(),
        "{name}: no native process was observed"
    );
    if let Some(rule) = rule {
        assert!(
            error
                .violations
                .iter()
                .any(|violation| violation.rule_id == rule),
            "{name} lacks {rule}: {error:?}"
        );
    }
    error
}

fn strict_wire_json_is_rejected_from_real_processes() {
    for name in [
        "empty",
        "invalid-utf8",
        "invalid-json",
        "truncated",
        "bom",
        "extra-json",
        "banner",
        "extra-text",
        "duplicate-key",
        "duplicate-nested-key",
        "unknown-field",
        "unknown-nested-field",
        "missing-nullable",
        "unknown-status",
    ] {
        rejected(name, "SIDECAR_PROTOCOL_INVALID", None);
    }
}

fn strict_object_and_enum_shapes_are_rejected_from_real_processes() {
    for name in [
        "root-array",
        "adapter-array",
        "capabilities-array",
        "input-array",
        "unit-array",
        "locator-array",
        "content-array",
        "artifact-array",
        "issue-array",
        "status-tagged-map",
        "producer-tagged-map",
        "unit-status-tagged-map",
        "locator-kind-tagged-map",
        "artifact-kind-tagged-map",
        "capability-locator-tagged-map",
        "capability-artifact-tagged-map",
    ] {
        let error = rejected(name, "SIDECAR_PROTOCOL_INVALID", None);
        assert!(error.response.is_none(), "{name} retained malformed DTO");
    }
}

fn real_identity_and_capability_mutations_are_rejected() {
    for (name, rule) in [
        ("future-version", "SIDECAR_VERSION"),
        ("wrong-protocol", "SIDECAR_VERSION"),
        ("wrong-request-id", "SIDECAR_IDENTITY"),
        ("wrong-input-sha", "SIDECAR_IDENTITY"),
        ("wrong-input-length", "SIDECAR_IDENTITY"),
        ("wrong-producer", "SIDECAR_PRODUCER"),
        ("wrong-adapter-version", "SIDECAR_PRODUCER"),
        ("wrong-capabilities", "SIDECAR_PRODUCER"),
    ] {
        rejected(name, "SIDECAR_PROTOCOL_INVALID", Some(rule));
    }
}

fn source_inventory_cannot_hide_missing_or_repeated_units() {
    for (name, rule) in [
        ("duplicate-ordinal", "SIDECAR_UNIT_MANIFEST"),
        ("missing-ordinal", "SIDECAR_UNIT_MANIFEST"),
        ("missing-unit-refreshed-count", "SIDECAR_UNIT_RANGE"),
        ("empty-complete", "SIDECAR_UNIT_RANGE"),
    ] {
        rejected(name, "SIDECAR_PROTOCOL_INVALID", Some(rule));
    }
}

fn real_range_and_content_mutations_are_rejected() {
    for (name, rule) in [
        ("empty-range", "SIDECAR_UNIT_RANGE"),
        ("overlap-range", "SIDECAR_UNIT_RANGE"),
        ("outside-range", "SIDECAR_UNIT_RANGE"),
        ("missing-content", "SIDECAR_CONTENT"),
    ] {
        rejected(name, "SIDECAR_PROTOCOL_INVALID", Some(rule));
    }
}

fn reported_partial_and_failure_preserve_verified_diagnostics() {
    for name in ["partial", "reported-failure"] {
        let error = rejected(name, "SIDECAR_INCOMPLETE", None);
        let response = serde_json::to_value(
            error
                .response
                .expect("valid reported failure must remain inspectable"),
        )
        .unwrap();
        assert_eq!(
            response["status"],
            if name == "partial" {
                "partial"
            } else {
                "failed"
            }
        );
        assert!(!response["issues"].as_array().unwrap().is_empty());
    }
}

fn nonzero_exit_and_crash_cannot_use_complete_json() {
    let error = rejected("exit-7", "SIDECAR_EXIT_NONZERO", None);
    assert!(
        error.response.is_none(),
        "nonzero wire claims are not verified facts"
    );
    observed_crash();
}

fn observed_crash() {
    use noetome_sidecar_platform::ProcessObserver;
    let sandbox = Sandbox::new();
    let input_before = fingerprint(&sandbox.input());
    let sentinel = sandbox.root.join("external-sentinel.md");
    let sentinel_before = fingerprint(&sentinel);
    let control = sandbox.control("crash-control");
    let adapter = sandbox.scenario(&control, "crash");
    let nonce = adapter.args[3].to_str().unwrap().to_owned();
    let root = sandbox.root.join("crash-workspace");
    let input = sandbox.input();
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = counted_run(
            &adapter,
            &input,
            &root,
            small_limits(),
            &AtomicBool::new(false),
        );
        let _ = sender.send(result);
    });
    let marker = read_marker(&control.join("before-abort.json"), &worker);
    assert_eq!(marker["nonce"], nonce);
    let pid = marker["pid"].as_u64().unwrap() as u32;
    let process = ProcessObserver::open(pid).expect("capture live helper before real abort");
    assert_eq!(
        process.creation_time(),
        marker["creation_time"].as_u64().unwrap()
    );
    assert_eq!(process.wait_exit(Duration::ZERO).unwrap(), None);
    let observer = std::thread::spawn(move || {
        let started = Instant::now();
        let exit = process.wait_exit(Duration::from_secs(8)).unwrap();
        drop(process); // Release the observation before Host waits for Job convergence.
        (exit, started.elapsed())
    });
    fs::write(control.join("abort-release"), nonce).unwrap();
    let result = receiver
        .recv_timeout(Duration::from_secs(15))
        .unwrap_or_else(|error| {
            eprintln!("SIDECAR_TEST_WATCHDOG: real abort Host stalled: {error}");
            std::process::exit(1);
        });
    worker.join().unwrap();
    let (exit, elapsed) = observer.join().unwrap();
    println!("real abort root {pid}: exit {exit:?}; observed wait {elapsed:?}");
    assert_eq!(
        exit,
        Some(0xC000_0409),
        "real Rust fast-fail must be a natural abnormal exit, not Host kill code1"
    );
    assert_eq!(fingerprint(&sandbox.input()), input_before);
    assert_eq!(fingerprint(&sentinel), sentinel_before);
    let error = result.expect_err("real process abort cannot verify complete facts");
    assert_eq!(
        error.code, "SIDECAR_EXIT_NONZERO",
        "observed abort root {exit:?} after {elapsed:?}: {error:?}"
    );
    assert_eq!(error.pid, Some(pid));
    assert!(error.response.is_none());
    assert_eq!(error.diagnostics, fake::DIAGNOSTIC);
}

fn stdout_exact_boundary_and_one_excess_byte() {
    let limits = SidecarLimits {
        max_stdout_bytes: 4096,
        ..small_limits()
    };
    let result = scenario("stdout-4096", limits)
        .expect("exact stdout cap with trailing whitespace is valid");
    assert_eq!(
        serde_json::to_value(&result.response).unwrap(),
        fake::response(&result.request.request_id)
    );
    let error = scenario("stdout-4097", limits).expect_err("one excess stdout byte must fail");
    assert_eq!(error.code, "SIDECAR_STDOUT_LIMIT");
    assert!(error.response.is_none());
}

fn stderr_boundaries_zero_and_binary_are_explicit() {
    let limits = SidecarLimits {
        max_stderr_bytes: 4096,
        ..small_limits()
    };
    assert_eq!(
        scenario("stderr-4096", limits).unwrap().diagnostics,
        vec![b'd'; 4096]
    );
    let error = scenario("stderr-4097", limits).expect_err("one excess diagnostic byte must fail");
    assert_eq!(error.code, "SIDECAR_STDERR_LIMIT");
    assert!(error.diagnostics.len() <= 4096);
    assert_eq!(
        scenario("stderr-binary", limits).unwrap().diagnostics,
        [0xff, 0x00, 0x80]
    );
    let silent = SidecarLimits {
        max_stderr_bytes: 0,
        ..small_limits()
    };
    assert!(scenario("silent", silent).unwrap().diagnostics.is_empty());
}

fn stderr_is_drained_before_child_reads_request() {
    let result = scenario("stderr-first", small_limits())
        .expect("stderr larger than OS pipe must drain concurrently with request");
    assert_eq!(result.diagnostics, vec![b'd'; 200 * 1024]);
}

fn physical_resources_and_inventory_are_rechecked() {
    for name in [
        "resource-sha",
        "resource-length",
        "resource-bytes",
        "resource-missing",
        "resource-extra-file",
        "resource-extra-directory",
    ] {
        rejected(name, "SIDECAR_RESOURCE_INVALID", None);
    }
    rejected("root-extra-file", "SIDECAR_WORKSPACE_CONFLICT", None);
}

fn response_resource_paths_do_not_expand_capabilities() {
    for name in [
        "resource-parent",
        "resource-absolute",
        "resource-backslash",
        "resource-subdirectory",
    ] {
        rejected(name, "SIDECAR_RESOURCE_INVALID", None);
    }
    for name in ["resource-tail-dot", "resource-stream"] {
        rejected(name, "SIDECAR_RESOURCE_INVALID", None);
    }
    rejected("resource-reserved-name", "SIDECAR_UNSAFE_PATH", None);
}

fn malformed_partial_is_not_misclassified_as_verified_failure() {
    rejected("malformed-partial", "SIDECAR_PROTOCOL_INVALID", None);
}

fn read_marker(path: &Path, worker: &std::thread::JoinHandle<()>) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(bytes) = fs::read(path) {
            if let Ok(marker) = serde_json::from_slice(&bytes) {
                return marker;
            }
        }
        assert!(
            !worker.is_finished(),
            "host ended before real process handshake {path:?}"
        );
        assert!(
            Instant::now() < deadline,
            "no real process handshake {path:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn observed_tree(name: &str, expected_code: &str, cancel_after_handshake: bool) {
    use noetome_sidecar_platform::ProcessObserver;
    let sandbox = Sandbox::new();
    let input_before = fingerprint(&sandbox.input());
    let sentinel = sandbox.root.join("external-sentinel.md");
    let sentinel_before = fingerprint(&sentinel);
    let control = sandbox.control("tree-control");
    let adapter = sandbox.scenario(&control, name);
    let nonce = adapter.args[3].to_str().unwrap().to_owned();
    let root = sandbox.root.join("tree-workspace");
    let input = sandbox.input();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel);
    let limits = SidecarLimits {
        timeout: Duration::from_secs(2),
        max_stdout_bytes: 4096,
        max_stderr_bytes: 4096,
        ..SidecarLimits::default()
    };
    let (sender, receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = counted_run(&adapter, &input, &root, limits, &worker_cancel);
        let _ = sender.send(result);
    });
    let root_marker = read_marker(&control.join("root.json"), &worker);
    let grand_marker = read_marker(&control.join("grandchild.json"), &worker);
    assert_eq!(root_marker["nonce"], nonce);
    assert_eq!(grand_marker["nonce"], nonce);
    let root_pid = root_marker["pid"].as_u64().unwrap() as u32;
    let grand_pid = grand_marker["pid"].as_u64().unwrap() as u32;
    assert_ne!(root_pid, grand_pid);
    OBSERVED_GRANDCHILDREN.fetch_add(1, Ordering::Relaxed);
    let root_observer = ProcessObserver::open(root_pid).expect("capture live real root process");
    let grand_observer =
        ProcessObserver::open(grand_pid).expect("capture live real grandchild process");
    assert_eq!(
        root_observer.creation_time(),
        root_marker["creation_time"].as_u64().unwrap()
    );
    assert_eq!(
        grand_observer.creation_time(),
        grand_marker["creation_time"].as_u64().unwrap()
    );
    assert_eq!(root_observer.wait_exit(Duration::ZERO).unwrap(), None);
    assert_eq!(grand_observer.wait_exit(Duration::ZERO).unwrap(), None);
    let observer = std::thread::spawn(move || {
        let root_exit = root_observer.wait_exit(Duration::from_secs(8)).unwrap();
        drop(root_observer);
        let grand_exit = grand_observer.wait_exit(Duration::from_secs(8)).unwrap();
        drop(grand_observer); // Release references before waiting for host Job accounting to settle.
        (root_exit, grand_exit)
    });
    if cancel_after_handshake {
        cancel.store(true, Ordering::SeqCst);
    } else {
        fs::write(control.join("release"), nonce).unwrap();
    }
    let result = match receiver.recv_timeout(Duration::from_secs(15)) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("SIDECAR_TEST_WATCHDOG: real tree host stalled: {name}: {error}");
            std::process::exit(1);
        }
    };
    worker.join().unwrap();
    let (root_exit, grand_exit) = observer.join().unwrap();
    assert!(root_exit.is_some(), "real root did not terminate");
    assert!(grand_exit.is_some(), "real grandchild did not terminate");
    if matches!(name, "grandchild-root-exit" | "grandchild-no-pipe") {
        assert_eq!(root_exit, Some(0));
    }
    let error = result
        .expect_err("a live descendant or forced tree termination cannot yield verified success");
    assert_eq!(error.code, expected_code, "{name}: {error:?}");
    assert_eq!(error.pid, Some(root_pid));
    assert!(error.response.is_none());
    assert_eq!(fingerprint(&sandbox.input()), input_before);
    assert_eq!(fingerprint(&sentinel), sentinel_before);
    println!(
        "real tree {name}: root {root_pid} exit {root_exit:?}; grandchild {grand_pid} exit {grand_exit:?}"
    );
}

fn real_tree_timeout_covers_root_and_pipe_holding_descendants() {
    for name in [
        "grandchild-hang",
        "grandchild-root-exit",
        "grandchild-no-pipe",
    ] {
        observed_tree(name, "SIDECAR_TIMEOUT", false);
    }
}

fn real_tree_cancellation_is_observed_after_live_handshake() {
    observed_tree("grandchild-hang", "SIDECAR_CANCELLED", true);
}

fn real_tree_limits_end_descendants_and_both_pipes() {
    observed_tree("grandchild-stdout-limit", "SIDECAR_STDOUT_LIMIT", false);
    observed_tree("grandchild-stderr-limit", "SIDECAR_STDERR_LIMIT", false);
}

fn utf8_artifact_characters_are_checked_independently() {
    let result = scenario_input("utf8-valid", "é".as_bytes(), small_limits())
        .expect("author-reviewed UTF8 resource is valid");
    assert_eq!(result.request.input.sha256, fake::UTF8_SHA);
    let response = serde_json::to_value(result.response).unwrap();
    assert_eq!(response["unit_count"], 1);
    assert_eq!(
        response["units"][0]["content"],
        json!({"artifact_id":"A1","start_byte":0,"end_byte":2})
    );
    let error = scenario_input("utf8-boundary", "é".as_bytes(), small_limits())
        .expect_err("binary input partitions cannot make invalid UTF8 resource boundaries valid");
    assert_eq!(error.code, "SIDECAR_PROTOCOL_INVALID");
    assert!(
        error
            .violations
            .iter()
            .any(|value| value.rule_id == "SIDECAR_CONTENT")
    );
}

fn artifact_limits_are_checked_at_exact_boundaries() {
    let exact = SidecarLimits {
        max_artifact_bytes: 3,
        max_total_artifact_bytes: 6,
        max_artifacts: 2,
        ..small_limits()
    };
    let result = scenario("two-resources", exact)
        .expect("two author-reviewed 3-byte resources at all exact limits");
    let value = serde_json::to_value(result.response).unwrap();
    assert_eq!(value["artifacts"].as_array().unwrap().len(), 2);
    assert_eq!(value["units"][1]["content"]["artifact_id"], "A2");
    for limits in [
        SidecarLimits {
            max_artifact_bytes: 2,
            ..exact
        },
        SidecarLimits {
            max_total_artifact_bytes: 5,
            ..exact
        },
        SidecarLimits {
            max_artifacts: 1,
            ..exact
        },
    ] {
        let error = scenario("two-resources", limits)
            .expect_err("one byte or file beyond resource limits must fail");
        assert_eq!(error.code, "SIDECAR_RESOURCE_INVALID");
    }
}

fn request_and_manifest_counts_have_exact_limits() {
    // UUID v4 strings have the same fixed width. The wire budget comes from the
    // independently authored request, never Host output or an observed failure.
    let author_request: Value = serde_json::from_str(fake::REQUEST).unwrap();
    let request_length = serde_json::to_vec(&author_request).unwrap().len();
    assert_eq!(
        request_length, 407,
        "reviewed compact request byte length changed"
    );
    let exact = SidecarLimits {
        max_request_bytes: request_length,
        max_units: 2,
        ..small_limits()
    };
    let result = scenario("valid", exact).expect("author request and two units at exact limits");
    assert_eq!(result.request.request_id.len(), fake::UUID.len());
    assert_eq!(result.response.units.len(), 2);

    let sandbox = Sandbox::new();
    let source_before = fingerprint(&sandbox.input());
    let sentinel = sandbox.root.join("external-sentinel.md");
    let sentinel_before = fingerprint(&sentinel);
    let control = sandbox.control("request-boundary-control");
    let error = bounded_run(
        sandbox.adapter(&control),
        sandbox.input(),
        sandbox.root.join("workspace"),
        SidecarLimits {
            max_request_bytes: request_length - 1,
            ..exact
        },
        Arc::new(AtomicBool::new(false)),
    )
    .expect_err("one extra request byte is rejected before native spawn");
    assert_eq!(error.code, "SIDECAR_REQUEST_LIMIT");
    assert!(error.pid.is_none());
    assert!(!control.join("root.json").exists());
    assert_eq!(fingerprint(&sandbox.input()), source_before);
    assert_eq!(fingerprint(&sentinel), sentinel_before);

    let error = scenario(
        "valid",
        SidecarLimits {
            max_units: 1,
            ..small_limits()
        },
    )
    .expect_err("second complete unit exceeds a one-unit policy");
    assert_eq!(error.code, "SIDECAR_PROTOCOL_INVALID");
    assert!(
        error
            .violations
            .iter()
            .any(|value| value.rule_id == "SIDECAR_UNIT_MANIFEST")
    );

    let issue_limit = SidecarLimits {
        max_issues: 1,
        ..small_limits()
    };
    let error = scenario("partial", issue_limit).expect_err("verified partial remains incomplete");
    assert_eq!(error.code, "SIDECAR_INCOMPLETE");
    assert_eq!(error.response.unwrap().issues.len(), 1);
    let error = scenario("partial-two-issues", issue_limit)
        .expect_err("second reported issue exceeds the explicit issue policy");
    assert_eq!(error.code, "SIDECAR_PROTOCOL_INVALID");
    assert!(
        error
            .violations
            .iter()
            .any(|value| value.rule_id == "SIDECAR_UNIT_MANIFEST")
    );
}

fn preflight_limits_and_existing_roots_never_start_fake() {
    for (name, code) in [
        ("empty-dir", "SIDECAR_WORKSPACE_CONFLICT"),
        ("file", "SIDECAR_WORKSPACE_CONFLICT"),
        ("human-dir", "SIDECAR_WORKSPACE_CONFLICT"),
        ("input-limit", "SIDECAR_INPUT_LIMIT"),
        ("request-limit", "SIDECAR_REQUEST_LIMIT"),
        ("cancel-before", "SIDECAR_CANCELLED"),
    ] {
        let sandbox = Sandbox::new();
        let control = sandbox.control("preflight-control");
        let adapter = sandbox.adapter(&control);
        let root = sandbox.root.join("selected-workspace");
        if name == "file" {
            fs::write(&root, b"human target\n").unwrap();
        }
        if name == "empty-dir" || name == "human-dir" {
            fs::create_dir(&root).unwrap();
        }
        if name == "human-dir" {
            fs::write(root.join("human.md"), b"human target\n").unwrap();
        }
        let source_before = fingerprint(&sandbox.input());
        let sentinel = sandbox.root.join("external-sentinel.md");
        let sentinel_before = fingerprint(&sentinel);
        let mut limits = small_limits();
        if name == "input-limit" {
            limits.max_input_bytes = 2;
        }
        if name == "request-limit" {
            limits.max_request_bytes = 1;
        }
        let error = bounded_run(
            adapter,
            sandbox.input(),
            root.clone(),
            limits,
            Arc::new(AtomicBool::new(name == "cancel-before")),
        )
        .unwrap_err();
        assert_eq!(error.code, code, "{name}: {error:?}");
        assert!(error.pid.is_none());
        assert!(!control.join("root.json").exists());
        assert_eq!(fingerprint(&sandbox.input()), source_before);
        assert_eq!(fingerprint(&sentinel), sentinel_before);
        if name == "file" {
            assert_eq!(fs::read(&root).unwrap(), b"human target\n");
        }
        if name == "human-dir" {
            assert_eq!(fs::read(root.join("human.md")).unwrap(), b"human target\n");
        }
    }
    let result = scenario(
        "valid",
        SidecarLimits {
            max_input_bytes: 3,
            ..small_limits()
        },
    )
    .expect("raw input at exact byte limit is valid");
    assert_eq!(result.request.input.byte_length, 3);
}

fn real_windows_junctions_and_hardlinks_are_rejected() {
    rejected("resource-hardlink", "SIDECAR_RESOURCE_INVALID", None);
    rejected("resource-case-collision", "SIDECAR_RESOURCE_INVALID", None);
    rejected("resource-junction", "SIDECAR_UNSAFE_PATH", None);
    let sandbox = Sandbox::new();
    for name in [
        "ordinary\\..\\escaped-target",
        "CON",
        "NUL",
        "target:stream",
        "tail.",
        "tail ",
    ] {
        let control = sandbox.control(&format!("preflight-{}", Uuid::new_v4()));
        let adapter = sandbox.adapter(&control);
        let mut raw = sandbox.root.as_os_str().to_owned();
        raw.push("\\");
        raw.push(name);
        let root = PathBuf::from(raw);
        if name.contains("\\..\\") {
            assert!(root.as_os_str().to_string_lossy().contains("\\..\\"));
        }
        let error = bounded_run(
            adapter,
            sandbox.input(),
            root,
            small_limits(),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap_err();
        assert_eq!(error.code, "SIDECAR_UNSAFE_PATH", "{name}: {error:?}");
        assert!(!control.join("root.json").exists());
    }
    let external = sandbox.root.join("external-directory");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("selected.bin"), b"abc").unwrap();
    let before = fingerprint(&external.join("selected.bin"));
    let parent_link = sandbox.root.join("junction-parent");
    make_junction(&parent_link, &external);
    for (name, input, root) in [
        (
            "linked-input",
            parent_link.join("selected.bin"),
            sandbox.root.join("linked-input-output"),
        ),
        (
            "linked-output-parent",
            sandbox.input(),
            parent_link.join("new-workspace"),
        ),
        ("linked-output-root", sandbox.input(), parent_link.clone()),
    ] {
        let control = sandbox.control(name);
        let error = bounded_run(
            sandbox.adapter(&control),
            input,
            root,
            small_limits(),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap_err();
        assert_eq!(error.code, "SIDECAR_UNSAFE_PATH", "{name}: {error:?}");
        assert!(error.pid.is_none());
        assert!(!control.join("root.json").exists());
        assert_eq!(fingerprint(&external.join("selected.bin")), before);
    }
    use std::os::windows::fs::OpenOptionsExt;
    let control = sandbox.control("locked-input");
    let before_bytes = fs::read(sandbox.input()).unwrap();
    let guard = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(sandbox.input())
        .unwrap();
    let error = counted_run(
        &sandbox.adapter(&control),
        &sandbox.input(),
        &sandbox.root.join("locked-workspace"),
        small_limits(),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    drop(guard);
    assert_eq!(error.code, "SIDECAR_INPUT_INVALID");
    assert!(error.pid.is_none());
    assert!(!control.join("root.json").exists());
    assert_eq!(fs::read(sandbox.input()).unwrap(), before_bytes);
}

fn make_junction(link: &Path, target: &Path) {
    use std::os::windows::process::CommandExt;
    let dos = |path: &Path| {
        path.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_owned()
    };
    let result = Command::new("cmd")
        .creation_flags(0x0800_0000)
        .args(["/c", "mklink", "/J"])
        .arg(dos(link))
        .arg(dos(target))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "owned junction failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn late_resource_inventory_addition_is_rejected() {
    for hardlink in [false, true] {
        let sandbox = Sandbox::new();
        fs::write(sandbox.input(), vec![b'x'; fake::LARGE_LEN]).unwrap();
        let source_before = fingerprint(&sandbox.input());
        let sentinel = sandbox.root.join("external-sentinel.md");
        let sentinel_before = fingerprint(&sentinel);
        let control = sandbox.control("large-probe-control");
        let adapter = sandbox.scenario(&control, "large_inventory_probe");
        let root = sandbox.root.join("large-workspace");
        let resource = root.join("artifacts/A1.bin");
        let alias = sandbox.root.join("external_alias.bin");
        let alias_for_watcher = alias.clone();
        let done = Arc::new(AtomicBool::new(false));
        let watcher_done = Arc::clone(&done);
        let watcher = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(12);
            while Instant::now() < deadline && !watcher_done.load(Ordering::Acquire) {
                match fs::OpenOptions::new().write(true).open(&resource) {
                    Ok(handle) => drop(handle), // Never keep a write handle and manufacture a host open failure.
                    Err(error) if error.raw_os_error() == Some(32) => {
                        if hardlink {
                            fs::hard_link(&resource, &alias_for_watcher).unwrap();
                        } else {
                            fs::write(
                                resource.parent().unwrap().join("unregistered.bin"),
                                b"late unlisted bytes",
                            )
                            .unwrap();
                        }
                        return true;
                    }
                    Err(_) => {}
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            false
        });
        let result = bounded_run(
            adapter,
            sandbox.input(),
            root.clone(),
            SidecarLimits::default(),
            Arc::new(AtomicBool::new(false)),
        );
        done.store(true, Ordering::Release);
        let observed = watcher.join().unwrap();
        assert!(
            observed,
            "the real resource guard was never observed; cannot claim the late fault ran"
        );
        let error = result.expect_err("late inventory/link-count mutation during a real read guard cannot return verified success");
        assert_eq!(
            error.code, "SIDECAR_RESOURCE_INVALID",
            "late hardlink={hardlink}: {error:?}"
        );
        assert!(error.pid.is_some());
        assert_eq!(fingerprint(&sandbox.input()), source_before);
        assert_eq!(fingerprint(&sentinel), sentinel_before);
        if hardlink {
            assert_eq!(fs::read(alias).unwrap(), vec![b'x'; fake::LARGE_LEN]);
        } else {
            assert_eq!(
                fs::read(root.join("artifacts/unregistered.bin")).unwrap(),
                b"late unlisted bytes"
            );
        }
        println!("late resource probe: hardlink={hardlink}, guard_observed={observed}");
    }
}

fn cancellation_during_host_resource_verification_is_rejected() {
    let sandbox = Sandbox::new();
    fs::write(sandbox.input(), vec![b'x'; fake::LARGE_LEN]).unwrap();
    let source_before = fingerprint(&sandbox.input());
    let sentinel = sandbox.root.join("external-sentinel.md");
    let sentinel_before = fingerprint(&sentinel);
    let control = sandbox.control("late-cancel-control");
    let root = sandbox.root.join("late-cancel-workspace");
    let resource = root.join("artifacts/A1.bin");
    let done = Arc::new(AtomicBool::new(false));
    let watcher_done = Arc::clone(&done);
    let cancel = Arc::new(AtomicBool::new(false));
    let watcher_cancel = Arc::clone(&cancel);
    let watcher = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(12);
        while Instant::now() < deadline && !watcher_done.load(Ordering::Acquire) {
            match fs::OpenOptions::new().write(true).open(&resource) {
                // Before Host verification, the helper permits write sharing.
                // Drop every probe handle immediately without changing bytes.
                Ok(handle) => drop(handle),
                Err(error) if error.raw_os_error() == Some(32) => {
                    watcher_cancel.store(true, Ordering::Release);
                    return true;
                }
                Err(_) => {}
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        false
    });
    let result = bounded_run(
        sandbox.scenario(&control, "large_inventory_probe"),
        sandbox.input(),
        root.clone(),
        SidecarLimits::default(),
        Arc::clone(&cancel),
    );
    done.store(true, Ordering::Release);
    let observed = watcher.join().unwrap();
    assert!(
        observed,
        "Host resource guard was not observed; cancellation did not run at the required phase"
    );
    assert!(cancel.load(Ordering::Acquire));
    println!("late Host cancellation probe: guard_observed={observed}");
    let error = result.expect_err("cancellation during Host verification cannot return Complete");
    assert_eq!(error.code, "SIDECAR_CANCELLED", "{error:?}");
    assert!(error.pid.is_some());
    assert!(error.response.is_none());
    assert_eq!(error.diagnostics, fake::DIAGNOSTIC);
    assert_eq!(fingerprint(&sandbox.input()), source_before);
    assert_eq!(fingerprint(&sentinel), sentinel_before);
    assert_eq!(
        fs::read(root.join("artifacts/A1.bin")).unwrap(),
        vec![b'x'; fake::LARGE_LEN]
    );
}
