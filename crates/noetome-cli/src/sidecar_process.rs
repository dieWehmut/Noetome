//! One trusted process invocation; no shell or commands derived from protocol content.
use crate::{
    sidecar_model::*,
    sidecar_workspace::{Prepared, ResourceGuards, valid_id},
};
use process_wrap::std::*;
use std::{
    collections::BTreeSet,
    ffi::OsString,
    io::{Read, Write},
    path::Path,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Debug, Clone)]
pub struct ParserCommand {
    pub program: OsString,
    pub args: Vec<OsString>,
}
#[derive(Debug)]
pub struct ValidatedResponse {
    pub response: Response,
    pub diagnostics: Vec<u8>,
    _prepared: Prepared,
    _resources: ResourceGuards,
}
pub(crate) struct Control {
    start: Instant,
    timeout: Duration,
    cancelled: Arc<AtomicBool>,
}
impl Control {
    pub(crate) fn check(&self) -> Result<(), SidecarError> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Err(SidecarError::new("SIDECAR_CANCELLED", "request cancelled"));
        }
        if self.start.elapsed() >= self.timeout {
            return Err(SidecarError::new("SIDECAR_TIMEOUT", "runtime limit"));
        }
        Ok(())
    }
}
pub fn run(
    command: &ParserCommand,
    input: &Path,
    workspace: &Path,
    limits: Limits,
    cancelled: Arc<AtomicBool>,
) -> Result<ValidatedResponse, SidecarError> {
    run_inner(command, input, workspace, limits, cancelled, false)
}
fn run_inner(
    command: &ParserCommand,
    input: &Path,
    workspace: &Path,
    limits: Limits,
    cancelled: Arc<AtomicBool>,
    allow_fake: bool,
) -> Result<ValidatedResponse, SidecarError> {
    if [
        limits.request_bytes,
        limits.response_bytes,
        limits.diagnostic_bytes,
        limits.input_bytes,
        limits.resource_bytes,
        limits.total_resource_bytes,
        limits.timeout_ms,
    ]
    .contains(&0)
        || [limits.resources, limits.units, limits.issues].contains(&0)
    {
        return Err(SidecarError::new(
            "SIDECAR_CONFIG",
            "limits must be positive",
        ));
    }
    let control = Control {
        start: Instant::now(),
        timeout: Duration::from_millis(limits.timeout_ms),
        cancelled,
    };
    control.check()?;
    let prepared = Prepared::new(input, workspace, limits.clone(), &control)?;
    let bytes = serde_json::to_vec(&prepared.request)
        .map_err(|e| SidecarError::new("SIDECAR_PROTOCOL", e))?;
    if bytes.len() as u64 > limits.request_bytes {
        return Err(SidecarError::new("SIDECAR_LIMIT", "request byte limit"));
    }
    let (stdout, diagnostics) = transport(command, bytes, &limits, &control)?;
    let result = (|| {
        control.check()?;
        prepared.verify(&control)?;
        let response: Response = serde_json::from_slice(&stdout)
            .map_err(|e| SidecarError::new("SIDECAR_PROTOCOL", e))?;
        if response.protocol_version != 1
            || response.parser.capability_version != 1
            || response.parser.capabilities != [Capability::SourceUnitsV1]
            || response.parser.name.trim().is_empty()
            || response.parser.name.len() > 128
            || response.parser.version.trim().is_empty()
            || response.parser.version.len() > 128
        {
            return Err(SidecarError::new(
                "SIDECAR_VERSION",
                "unsupported protocol/parser/capability",
            ));
        }
        if response.request_id != prepared.request.request_id
            || response.workspace_capability_id != prepared.request.workspace.capability_id
            || response.input_fingerprint.sha256 != prepared.request.input.fingerprint.sha256
            || response.input_fingerprint.byte_length
                != prepared.request.input.fingerprint.byte_length
        {
            return Err(SidecarError::new(
                "SIDECAR_IDENTITY",
                "request/input/workspace identity mismatch",
            ));
        }
        if response.producer == Producer::Fake && !(cfg!(test) && allow_fake) {
            return Err(SidecarError::new(
                "SIDECAR_PRODUCER",
                "synthetic producer is restricted to tests",
            ));
        }
        let resources = prepared.resources(&response, &control)?;
        #[cfg(test)]
        VALIDATION_CANCEL.with(|flag| {
            if flag.get() {
                control.cancelled.store(true, Ordering::SeqCst);
                CANCELLED_AT.with(|time| time.set(Some(Instant::now())));
            }
        });
        validate_units(&response, &resources, &limits, &control)?;
        control.check()?;
        prepared.verify(&control)?;
        Ok(ValidatedResponse {
            response,
            diagnostics: diagnostics.clone(),
            _prepared: prepared,
            _resources: resources,
        })
    })();
    result.map_err(|mut e| {
        e.diagnostics = diagnostics;
        e
    })
}
#[cfg(windows)]
#[derive(Debug)]
struct Observer(Arc<win32job::Job>);
#[cfg(windows)]
impl CommandWrapper for Observer {
    fn wrap_child(
        &mut self,
        mut child: Box<dyn ChildWrapper>,
        _: &CommandWrap,
    ) -> std::io::Result<Box<dyn ChildWrapper>> {
        use std::os::windows::io::AsRawHandle;
        let handle = child
            .process_handle()
            .ok_or_else(|| std::io::Error::other("no process handle"))?;
        if let Err(e) = self.0.assign_process(handle.as_raw_handle() as isize) {
            let _ = child.start_kill();
            let _ = child.wait();
            return Err(std::io::Error::other(e.to_string()));
        }
        Ok(child)
    }
}
fn read_bounded(
    mut pipe: impl Read,
    max: u64,
    overflow: Arc<AtomicBool>,
) -> (Vec<u8>, Option<SidecarError>) {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match pipe.read(&mut buf) {
            Ok(0) => return (bytes, None),
            Ok(n) => {
                if bytes.len() as u64 + n as u64 > max {
                    let keep = (max - bytes.len() as u64).min(n as u64) as usize;
                    bytes.extend_from_slice(&buf[..keep]);
                    overflow.store(true, Ordering::SeqCst);
                    return (
                        bytes,
                        Some(SidecarError::new(
                            "SIDECAR_LIMIT",
                            "pipe byte limit; captured diagnostics may be incomplete",
                        )),
                    );
                }
                bytes.extend_from_slice(&buf[..n]);
            }
            Err(e) => return (bytes, Some(SidecarError::new("SIDECAR_IO", e))),
        }
    }
}
fn transport(
    command: &ParserCommand,
    bytes: Vec<u8>,
    limits: &Limits,
    control: &Control,
) -> Result<(Vec<u8>, Vec<u8>), SidecarError> {
    control.check()?;
    #[cfg(not(windows))]
    {
        let _ = (command, bytes, limits);
        return Err(SidecarError::new(
            "SIDECAR_CONFIG",
            "process tree host currently requires Windows",
        ));
    }
    #[cfg(windows)]
    {
        let mut info = win32job::ExtendedLimitInfo::new();
        info.limit_kill_on_job_close();
        let observer = Arc::new(
            win32job::Job::create_with_limit_info(&info)
                .map_err(|e| SidecarError::new("SIDECAR_START", e))?,
        );
        let mut wrap = CommandWrap::with_new(&command.program, |c| {
            c.args(&command.args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
        });
        // All pre_spawn hooks run before spawn. Observer's wrap hook therefore runs
        // on a suspended root, before JobObject assigns its own job and resumes it.
        let mut flags = CreationFlags(Default::default());
        flags.0.0 = 0x08000000; // CREATE_NO_WINDOW
        wrap.wrap(flags)
            .wrap(Observer(observer.clone()))
            .wrap(JobObject);
        let mut child = wrap
            .spawn()
            .map_err(|e| SidecarError::new("SIDECAR_START", e))?;
        let mut stdin = child.stdin().take().expect("piped stdin");
        let stdout = child.stdout().take().expect("piped stdout");
        let stderr = child.stderr().take().expect("piped stderr");
        let overflow = Arc::new(AtomicBool::new(false));
        let outmax = limits.response_bytes;
        let errmax = limits.diagnostic_bytes;
        let outoverflow = overflow.clone();
        let erroverflow = overflow.clone();
        let out_thread = thread::spawn(move || read_bounded(stdout, outmax, outoverflow));
        let err_thread = thread::spawn(move || read_bounded(stderr, errmax, erroverflow));
        let writer = thread::spawn(move || stdin.write_all(&bytes));
        let mut failure = None;
        let mut status = None;
        loop {
            if overflow.load(Ordering::SeqCst) {
                failure = Some(SidecarError::new(
                    "SIDECAR_LIMIT",
                    "stdout/stderr byte limit; captured diagnostics may be incomplete",
                ));
                break;
            }
            if let Err(e) = control.check() {
                failure = Some(e);
                break;
            }
            // Never call process-wrap wait/try_wait: its completion queue helper can
            // consume unrelated job notifications. Root wait and Job census are distinct.
            match child.inner_mut().try_wait() {
                Ok(Some(s)) => {
                    status = Some(s);
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    failure = Some(SidecarError::new("SIDECAR_IO", e));
                    break;
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        // Also terminate leftovers after a successful root exit. No descendants
        // from a single-request parser may outlive the operation.
        if let Err(e) = child.start_kill() {
            failure = Some(SidecarError::new("SIDECAR_CLEANUP", e));
        }
        match child.inner_mut().wait() {
            Ok(s) => {
                if status.is_none() {
                    status = Some(s);
                }
            }
            Err(e) => failure = Some(SidecarError::new("SIDECAR_CLEANUP", e)),
        }
        let cleanup_start = Instant::now();
        loop {
            match observer.query_process_id_list() {
                Ok(pids) if pids.is_empty() => break,
                Ok(_) => {
                    if cleanup_start.elapsed() > Duration::from_secs(10) {
                        failure = Some(SidecarError::new(
                            "SIDECAR_CLEANUP",
                            "job still has active processes",
                        ));
                        break;
                    }
                    thread::sleep(Duration::from_millis(2));
                }
                Err(e) => {
                    failure = Some(SidecarError::new("SIDECAR_CLEANUP", e));
                    break;
                }
            }
        }
        // Closing both kill-on-close jobs is a final safety net before joins.
        drop(child);
        drop(wrap);
        drop(observer);
        let write_result = writer.join();
        let out_result = out_thread.join();
        let err_result = err_thread.join();
        let (diagnostics, diag_error) =
            err_result.map_err(|_| SidecarError::new("SIDECAR_CLEANUP", "stderr thread panic"))?;
        let (stdout, out_error) =
            out_result.map_err(|_| SidecarError::new("SIDECAR_CLEANUP", "stdout thread panic"))?;
        if failure.is_none() {
            failure = diag_error.or(out_error);
        }
        if failure.is_none() && !status.is_some_and(|s| s.success()) {
            failure = Some(SidecarError::new(
                "SIDECAR_EXIT",
                format!("unsuccessful exit: {status:?}"),
            ));
        }
        if failure.is_none() {
            match write_result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => failure = Some(SidecarError::new("SIDECAR_IO", e)),
                Err(_) => {
                    failure = Some(SidecarError::new("SIDECAR_CLEANUP", "stdin thread panic"))
                }
            }
        }
        if let Some(mut error) = failure {
            error.diagnostics = diagnostics;
            return Err(error);
        }
        control.check().map_err(|mut e| {
            e.diagnostics = diagnostics.clone();
            e
        })?;
        Ok((stdout, diagnostics))
    }
}
fn validate_units(
    response: &Response,
    resources: &ResourceGuards,
    limits: &Limits,
    control: &Control,
) -> Result<(), SidecarError> {
    control.check()?;
    let fail = |msg| SidecarError::new("SIDECAR_UNITS", msg);
    if response.units.len() > limits.units as usize
        || response.issues.len() > limits.issues as usize
    {
        return Err(SidecarError::new("SIDECAR_LIMIT", "unit/issue count limit"));
    }
    if response.unit_count as usize != response.units.len()
        || (response.producer == Producer::Parser && response.units.is_empty())
    {
        return Err(fail("incomplete unit count"));
    }
    let mut ids = BTreeSet::new();
    let mut used = BTreeSet::new();
    for (ordinal, u) in response.units.iter().enumerate() {
        control.check()?;
        if u.ordinal as usize != ordinal || !valid_id(&u.unit_id) || !ids.insert(u.unit_id.clone())
        {
            return Err(fail("invalid/duplicate unit or ordinal"));
        }
        let mut refs = BTreeSet::new();
        for r in &u.resource_ids {
            control.check()?;
            if !resources.bytes.contains_key(r) || !refs.insert(r.clone()) {
                return Err(fail("invalid/duplicate resource reference"));
            }
            used.insert(r.clone());
        }
        if u.kind == UnitKind::Text && u.text.is_none() {
            return Err(fail("text unit missing range"));
        }
        if u.kind == UnitKind::Unknown && u.status == ParseStatus::Parsed {
            return Err(fail("unknown unit cannot be parsed"));
        }
        if let Some(t) = &u.text {
            let data = resources
                .bytes
                .get(&t.resource_id)
                .ok_or_else(|| fail("unknown text resource"))?;
            if !refs.contains(&t.resource_id) {
                return Err(fail("text reference must name UTF8 resource"));
            }
            let text = data
                .text()
                .ok_or_else(|| fail("text reference must name UTF8 resource"))?;
            if t.start_byte >= t.end_byte
                || t.end_byte > data.byte_length() as u64
                || !text.is_char_boundary(t.start_byte as usize)
                || !text.is_char_boundary(t.end_byte as usize)
            {
                return Err(fail("invalid UTF8 text range"));
            }
        }
    }
    if used.len() != resources.bytes.len() {
        return Err(fail("unreferenced resource"));
    }
    let mut issue_units = BTreeSet::new();
    for i in &response.issues {
        control.check()?;
        if !valid_id(&i.code) || i.message.trim().is_empty() || i.message.len() > 4096 {
            return Err(fail("invalid structured issue"));
        }
        let mut units = BTreeSet::new();
        let mut refs = BTreeSet::new();
        for id in &i.unit_ids {
            control.check()?;
            if !ids.contains(id) || !units.insert(id) {
                return Err(fail("invalid issue unit reference"));
            }
            issue_units.insert(id.clone());
        }
        for id in &i.resource_ids {
            control.check()?;
            if !resources.bytes.contains_key(id) || !refs.insert(id) {
                return Err(fail("invalid issue resource reference"));
            }
        }
    }
    for u in &response.units {
        control.check()?;
        if u.status != ParseStatus::Parsed && !issue_units.contains(&u.unit_id) {
            return Err(fail("failed/review unit requires issue"));
        }
    }
    Ok(())
}
#[cfg(test)]
#[path = "../tests/support/sidecar_tests.rs"]
mod tests;

#[cfg(test)]
thread_local! {
    static VALIDATION_CANCEL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static CANCELLED_AT: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}
