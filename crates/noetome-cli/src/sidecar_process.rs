//! Bounded concurrent transport; protocol and resource validation stay in Core.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use noetome_sidecar_platform::{JobProcess, SpawnSpec, Spawned};

use crate::sidecar::{AdapterSpec, SidecarError, SidecarLimits};

#[derive(Debug)]
pub(crate) struct ProcessOutput {
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) pid: u32,
    pub(crate) elapsed: Duration,
}
pub(crate) fn execute(
    adapter: &AdapterSpec,
    request: &[u8],
    workspace: &Path,
    limits: &SidecarLimits,
    cancel: &AtomicBool,
) -> Result<ProcessOutput, SidecarError> {
    if request.len() > limits.max_request_bytes {
        return Err(SidecarError::new(
            "SIDECAR_REQUEST_LIMIT",
            "Request exceeds the byte limit; no child was started.",
        ));
    }
    if cancel.load(Ordering::Acquire) {
        return Err(SidecarError::new(
            "SIDECAR_CANCELLED",
            "Cancelled before process creation.",
        ));
    }
    let started = Instant::now();
    let deadline = started
        .checked_add(limits.timeout)
        .ok_or_else(|| SidecarError::new("SIDECAR_TIMEOUT", "Invalid finite process deadline."))?;
    // Trusted program/argv do not imply inheriting arbitrary development,
    // terminal or parser configuration. T01 needs only the existing Windows
    // system directory context; future adapters must define additions explicitly.
    let system_root = std::env::vars_os()
        .find(|(key, _)| key.to_string_lossy().eq_ignore_ascii_case("SystemRoot"))
        .map(|(_, value)| value)
        .ok_or_else(|| {
            SidecarError::new(
                "SIDECAR_SPAWN_FAILED",
                "Windows SystemRoot is unavailable; no child was started.",
            )
        })?;
    let environment = [(std::ffi::OsString::from("SystemRoot"), system_root)];
    let Spawned {
        mut process,
        stdin,
        stdout,
        stderr,
    } = JobProcess::spawn_with_cleanup_timeout(
        &SpawnSpec {
            executable: &adapter.program,
            args: &adapter.args,
            cwd: workspace,
            environment: &environment,
        },
        limits.cleanup_timeout,
    )
    .map_err(|error| {
        let code = if noetome_sidecar_platform::is_cleanup_failure(&error) {
            "SIDECAR_CLEANUP_FAILED"
        } else {
            "SIDECAR_SPAWN_FAILED"
        };
        SidecarError::new(
            code,
            format!("Contained native process creation failed: {error}"),
        )
    })?;
    let pid = process.pid();
    let stop = Arc::new(AtomicBool::new(false));
    let exceeded = Arc::new(AtomicU8::new(0));
    let mut workers = Vec::new();
    // Readers start independently of the writer: either output pipe may fill
    // before the child accepts input. Each File moves to exactly one worker.
    let spawn_result = (|| {
        for (role, file, limit, signal) in [
            (Role::Stdout, stdout, limits.max_stdout_bytes, 1),
            (Role::Stderr, stderr, limits.max_stderr_bytes, 2),
        ] {
            let stopped = stop.clone();
            let over = exceeded.clone();
            workers.push(Worker::spawn(role, move || {
                read_pipe(role, file, limit, signal, &stopped, &over)
            })?);
        }
        let stopped = stop.clone();
        let owned_request = request.to_vec();
        workers.push(Worker::spawn(Role::Stdin, move || {
            write_pipe(stdin, &owned_request, &stopped)
        })?);
        Ok::<(), io::Error>(())
    })();
    if let Err(error) = spawn_result {
        let cause = SidecarError::new(
            "SIDECAR_PIPE_FAILED",
            format!("Could not start all concurrent pipe workers: {error}"),
        );
        return Err(cleanup(
            &mut process,
            &stop,
            &mut workers,
            limits.cleanup_timeout,
            cause,
            pid,
        ));
    }
    loop {
        let join_issue = drain_finished(&mut workers);
        // A real byte limit takes precedence over a secondary child exit caused
        // by closing its pipe. No truncated bytes can become a response.
        let over = exceeded.load(Ordering::Acquire);
        let mut failure = if over & 1 != 0 {
            Some(SidecarError::new(
                "SIDECAR_STDOUT_LIMIT",
                "Child stdout exceeded the byte limit.",
            ))
        } else if over & 2 != 0 {
            Some(SidecarError::new(
                "SIDECAR_STDERR_LIMIT",
                "Child stderr exceeded the byte limit.",
            ))
        } else if cancel.load(Ordering::Acquire) {
            Some(SidecarError::new(
                "SIDECAR_CANCELLED",
                "Cancelled while the contained process was running.",
            ))
        } else if Instant::now() >= deadline {
            Some(SidecarError::new(
                "SIDECAR_TIMEOUT",
                "Contained process and its pipes did not finish before the deadline.",
            ))
        } else {
            None
        };
        let root = match process.try_wait_root() {
            Ok(status) => status,
            Err(error) => {
                failure.get_or_insert_with(|| {
                    SidecarError::new(
                        "SIDECAR_PIPE_FAILED",
                        format!("Root process status could not be observed: {error}"),
                    )
                });
                None
            }
        };
        if failure.is_none() {
            if let Some(exit) = root.filter(|exit| exit.code != 0) {
                failure = Some(SidecarError::new(
                    "SIDECAR_EXIT_NONZERO",
                    format!("Child exited with native code {}.", exit.code),
                ));
            } else if let Some(error) = worker_error(&workers) {
                failure = Some(SidecarError::new("SIDECAR_PIPE_FAILED", error));
            } else if let Some(error) = join_issue {
                failure = Some(SidecarError::new("SIDECAR_PIPE_FAILED", error));
            }
        }
        if let Some(cause) = failure {
            return Err(cleanup(
                &mut process,
                &stop,
                &mut workers,
                limits.cleanup_timeout,
                cause,
                pid,
            ));
        }
        if root.is_some() && workers.iter().all(Worker::done) {
            match process.tree_state() {
                Ok(state) if state.active_processes == 0 => {
                    let stdout = take_bytes(&mut workers, Role::Stdout);
                    let stderr = take_bytes(&mut workers, Role::Stderr);
                    return Ok(ProcessOutput {
                        stdout,
                        stderr,
                        pid,
                        elapsed: started.elapsed(),
                    });
                }
                Ok(_) => {} // A descendant is still in the Job, even after EOF.
                Err(error) => {
                    let cause = SidecarError::new(
                        "SIDECAR_PIPE_FAILED",
                        format!("Job process count could not be verified: {error}"),
                    );
                    return Err(cleanup(
                        &mut process,
                        &stop,
                        &mut workers,
                        limits.cleanup_timeout,
                        cause,
                        pid,
                    ));
                }
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Stdin,
    Stdout,
    Stderr,
}
struct PipeResult {
    bytes: Vec<u8>,
    error: Option<String>,
    complete: bool,
}
struct Worker {
    role: Role,
    handle: Option<JoinHandle<PipeResult>>,
    result: Option<PipeResult>,
}
impl Worker {
    fn spawn(role: Role, task: impl FnOnce() -> PipeResult + Send + 'static) -> io::Result<Self> {
        let name = match role {
            Role::Stdin => "sidecar-stdin",
            Role::Stdout => "sidecar-stdout",
            Role::Stderr => "sidecar-stderr",
        };
        Ok(Self {
            role,
            handle: Some(thread::Builder::new().name(name.into()).spawn(task)?),
            result: None,
        })
    }
    fn done(&self) -> bool {
        self.handle.is_none()
    }
}
fn drain_finished(workers: &mut [Worker]) -> Option<String> {
    let mut issue = None;
    for worker in workers {
        if worker.handle.as_ref().is_some_and(JoinHandle::is_finished) {
            match noetome_sidecar_platform::io_thread_exited(
                worker
                    .handle
                    .as_ref()
                    .expect("worker retains thread ownership"),
            ) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    issue.get_or_insert_with(|| {
                        format!("Native pipe thread termination could not be observed: {error}")
                    });
                    continue;
                }
            }
            let handle = worker
                .handle
                .take()
                .expect("finished worker retains its join ownership");
            worker.result = Some(handle.join().unwrap_or_else(|_| PipeResult {
                bytes: Vec::new(),
                error: Some("A concurrent pipe worker panicked.".into()),
                complete: false,
            }));
        }
    }
    issue
}
fn worker_error(workers: &[Worker]) -> Option<String> {
    workers
        .iter()
        .filter_map(|worker| worker.result.as_ref())
        .find_map(|result| {
            result.error.clone().or_else(|| {
                (!result.complete)
                    .then(|| "A pipe worker stopped before completing the request/response.".into())
            })
        })
}
fn take_bytes(workers: &mut [Worker], role: Role) -> Vec<u8> {
    workers
        .iter_mut()
        .find(|worker| worker.role == role)
        .and_then(|worker| worker.result.as_mut())
        .map(|result| std::mem::take(&mut result.bytes))
        .unwrap_or_default()
}
fn read_pipe(
    _role: Role,
    mut pipe: File,
    limit: usize,
    signal: u8,
    stop: &AtomicBool,
    exceeded: &AtomicU8,
) -> PipeResult {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        if stop.load(Ordering::Acquire) {
            return PipeResult {
                bytes,
                error: None,
                complete: false,
            };
        }
        // Read one byte beyond the remaining capacity to distinguish exact EOF
        // from overflow, retaining at most `limit` diagnostic/output bytes.
        let room = limit - bytes.len();
        let count = chunk.len().min(room.saturating_add(1));
        match pipe.read(&mut chunk[..count]) {
            Ok(0) => {
                return PipeResult {
                    bytes,
                    error: None,
                    complete: true,
                };
            }
            Ok(read) => {
                bytes.extend_from_slice(&chunk[..read.min(room)]);
                if read > room {
                    exceeded.fetch_or(signal, Ordering::Release);
                    stop.store(true, Ordering::Release);
                    return PipeResult {
                        bytes,
                        error: None,
                        complete: false,
                    };
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                return PipeResult {
                    bytes,
                    error: (!stop.load(Ordering::Acquire))
                        .then(|| format!("Pipe read failed: {error}")),
                    complete: false,
                };
            }
        }
    }
}
fn write_pipe(mut pipe: File, request: &[u8], stop: &AtomicBool) -> PipeResult {
    for chunk in request.chunks(8192) {
        if stop.load(Ordering::Acquire) {
            return PipeResult {
                bytes: Vec::new(),
                error: None,
                complete: false,
            };
        }
        if let Err(error) = pipe.write_all(chunk) {
            return PipeResult {
                bytes: Vec::new(),
                error: (!stop.load(Ordering::Acquire))
                    .then(|| format!("Request pipe write failed: {error}")),
                complete: false,
            };
        }
    }
    // Dropping the unique stdin File marks the single request's end.
    PipeResult {
        bytes: Vec::new(),
        error: None,
        complete: true,
    }
}
fn cleanup(
    process: &mut JobProcess,
    stop: &AtomicBool,
    workers: &mut [Worker],
    timeout: Duration,
    mut cause: SidecarError,
    pid: u32,
) -> SidecarError {
    stop.store(true, Ordering::Release);
    let mut problems = Vec::new();
    if let Err(error) = process.terminate_tree(1) {
        problems.push(format!("Job termination: {error}"));
    }
    let deadline = Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(Instant::now);
    loop {
        if let Some(error) = drain_finished(workers) {
            problems.push(error);
        }
        // Cancel against each live thread identity; a cancellation can race with
        // its next I/O, so repeat while bounded waiting. Job termination also
        // closes every contained child end, waking blocked reads and writes.
        for worker in workers.iter().filter_map(|worker| worker.handle.as_ref()) {
            if let Err(error) = noetome_sidecar_platform::cancel_io_thread(worker) {
                problems.push(format!("Pipe I/O cancellation: {error}"));
            }
        }
        if let Err(error) = process.try_wait_root() {
            problems.push(format!("Root exit observation: {error}"));
        }
        let empty = match process.tree_state() {
            Ok(state) => state.active_processes == 0,
            Err(error) => {
                problems.push(format!("Job emptiness observation: {error}"));
                false
            }
        };
        if empty && workers.iter().all(Worker::done) {
            break;
        }
        if Instant::now() >= deadline {
            problems.push(
                "Process tree or pipe workers did not finish before the cleanup deadline.".into(),
            );
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    // No unbounded JoinHandle::join: unfinished handles are detached only on an
    // explicit cleanup failure, and closing the Job is a final kill safeguard.
    if !problems.is_empty() {
        cause = SidecarError::new(
            "SIDECAR_CLEANUP_FAILED",
            format!(
                "{}: {}; cleanup could not be confirmed: {}",
                cause.code,
                cause.message,
                problems.join("; ")
            ),
        );
    }
    cause.pid = Some(pid);
    cause.diagnostics = take_bytes(workers, Role::Stderr);
    cause
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::sidecar::{AdapterIdentity, ArtifactKind, Capabilities, LocatorKind, Producer};
    use std::ffi::OsString;
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};
    fn sandbox() -> PathBuf {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.artifacts/process-tests");
        std::fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        root.canonicalize().unwrap()
    }
    fn adapter(helper: &str) -> AdapterSpec {
        AdapterSpec {
            program: std::env::current_exe().unwrap(),
            args: vec![
                "--exact".into(),
                format!("sidecar_process::tests::{helper}").into(),
                "--nocapture".into(),
            ],
            expected: AdapterIdentity {
                producer: Producer::Fake,
                adapter_id: "transport-test".into(),
                adapter_version: "1".into(),
                capabilities: Capabilities {
                    locator_kinds: vec![LocatorKind::InputBytes],
                    artifact_kinds: vec![ArtifactKind::Binary],
                },
            },
        }
    }
    fn helper_enabled(name: &str) -> bool {
        let args: Vec<OsString> = std::env::args_os().collect();
        let selected = OsString::from(format!("sidecar_process::tests::{name}"));
        args.iter().any(|arg| arg == "--exact") && args.iter().any(|arg| arg == &selected)
    }
    #[test]
    fn helper_exchange() {
        if !helper_enabled("helper_exchange") {
            return;
        }
        assert!(std::env::var_os("SystemRoot").is_some());
        assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_none());
        assert!(std::env::var_os("TERM").is_none());
        let mut request = Vec::new();
        std::io::stdin().read_to_end(&mut request).unwrap();
        std::io::stdout().write_all(b"TRANSPORT_ECHO=").unwrap();
        std::io::stdout().write_all(&request).unwrap();
        std::io::stdout().flush().unwrap();
        std::io::stderr()
            .write_all(b"transport diagnostic")
            .unwrap();
    }
    #[test]
    fn helper_nonzero() {
        if !helper_enabled("helper_nonzero") {
            return;
        }
        std::io::stdout().write_all(b"{}").unwrap();
        std::io::stdout().flush().unwrap();
        std::io::stderr().write_all(b"nonzero diagnostic").unwrap();
        std::process::exit(7);
    }
    #[test]
    fn helper_blocked_stdin() {
        if !helper_enabled("helper_blocked_stdin") {
            return;
        }
        std::fs::write("transport-blocked.ready", std::process::id().to_string()).unwrap();
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    #[test]
    fn helper_flood_stdout() {
        if !helper_enabled("helper_flood_stdout") {
            return;
        }
        std::io::stdout().write_all(&vec![b'x'; 16384]).unwrap();
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    #[test]
    fn helper_flood_stderr() {
        if !helper_enabled("helper_flood_stderr") {
            return;
        }
        std::io::stderr().write_all(&vec![b'e'; 16384]).unwrap();
        std::io::stderr().flush().unwrap();
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    #[test]
    fn actual_transport_returns_complete_stdout_stderr_and_pid() {
        let root = sandbox();
        let output = execute(
            &adapter("helper_exchange"),
            b"request bytes",
            &root,
            &SidecarLimits::default(),
            &AtomicBool::new(false),
        )
        .expect("real child transport");
        // These helpers intentionally use libtest: this assertion proves native
        // transport only. Full protocol tests use the separate banner-free fake.
        assert!(
            output
                .stdout
                .windows(b"TRANSPORT_ECHO=request bytes".len())
                .any(|part| part == b"TRANSPORT_ECHO=request bytes")
        );
        assert_eq!(output.stderr, b"transport diagnostic");
        assert_ne!(output.pid, 0);
        assert!(output.elapsed < Duration::from_secs(2));
    }
    #[test]
    fn nonzero_process_is_rejected_before_protocol_can_use_its_stdout() {
        let root = sandbox();
        let error = execute(
            &adapter("helper_nonzero"),
            b"request",
            &root,
            &SidecarLimits::default(),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(error.code, "SIDECAR_EXIT_NONZERO");
        assert!(error.pid.is_some());
        assert_eq!(error.diagnostics, b"nonzero diagnostic");
    }
    #[test]
    fn blocked_stdin_larger_than_pipe_capacity_times_out_and_joins() {
        let root = sandbox();
        let limits = SidecarLimits {
            max_request_bytes: 2 * 1024 * 1024,
            timeout: Duration::from_millis(120),
            ..SidecarLimits::default()
        };
        let start = Instant::now();
        let error = execute(
            &adapter("helper_blocked_stdin"),
            &vec![b'p'; 1024 * 1024],
            &root,
            &limits,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(error.code, "SIDECAR_TIMEOUT");
        assert!(error.pid.is_some());
        assert_eq!(
            std::fs::read_to_string(root.join("transport-blocked.ready")).unwrap(),
            error.pid.unwrap().to_string()
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn each_output_pipe_limit_rejects_after_actual_concurrent_drain() {
        for (helper, code) in [
            ("helper_flood_stdout", "SIDECAR_STDOUT_LIMIT"),
            ("helper_flood_stderr", "SIDECAR_STDERR_LIMIT"),
        ] {
            let root = sandbox();
            let limits = SidecarLimits {
                max_stdout_bytes: 1024,
                max_stderr_bytes: 1024,
                timeout: Duration::from_secs(1),
                ..SidecarLimits::default()
            };
            let error = execute(
                &adapter(helper),
                b"request",
                &root,
                &limits,
                &AtomicBool::new(false),
            )
            .unwrap_err();
            assert_eq!(error.code, code);
            assert!(error.pid.is_some());
            assert!(error.diagnostics.len() <= 1024);
        }
    }
    #[test]
    fn cancellation_during_execution_terminates_and_joins() {
        let root = sandbox();
        let cancel = Arc::new(AtomicBool::new(false));
        let changed = cancel.clone();
        let ready = root.join("transport-blocked.ready");
        let signal = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !ready.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            let live = ready.exists();
            std::thread::sleep(Duration::from_millis(45));
            changed.store(true, Ordering::Release);
            live
        });
        let error = execute(
            &adapter("helper_blocked_stdin"),
            b"request",
            &root,
            &SidecarLimits::default(),
            &cancel,
        )
        .unwrap_err();
        assert!(
            signal.join().unwrap(),
            "cancel must follow a live helper handshake"
        );
        assert_eq!(error.code, "SIDECAR_CANCELLED");
        assert!(error.pid.is_some());
    }
    #[test]
    fn request_limit_and_pre_cancel_do_not_start_a_process() {
        let root = sandbox();
        let limits = SidecarLimits {
            max_request_bytes: 2,
            ..SidecarLimits::default()
        };
        let error = execute(
            &adapter("helper_exchange"),
            b"123",
            &root,
            &limits,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(error.code, "SIDECAR_REQUEST_LIMIT");
        assert!(error.pid.is_none());
        let error = execute(
            &adapter("helper_exchange"),
            b"",
            &root,
            &SidecarLimits::default(),
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert_eq!(error.code, "SIDECAR_CANCELLED");
        assert!(error.pid.is_none());
    }
}
