//! Real process/file boundary tests for the PDF wrapper. Fault commands are
//! test-only trusted programs; ordinary CLI never accepts a command override.
use super::*;
use serde_json::Value;
use std::{fs, path::PathBuf, sync::atomic::Ordering, thread, time::Duration};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("noetome-pdf-boundary-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("sentinel.txt"), b"outside parse root: unchanged").unwrap();
        Self(root)
    }
    fn input(&self) -> PathBuf {
        let input = self.0.join("selected.pdf");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/pdf-text-v1/single-page.pdf"),
            &input,
        )
        .unwrap();
        input
    }
    fn sentinel(&self) {
        assert_eq!(
            fs::read(self.0.join("sentinel.txt")).unwrap(),
            b"outside parse root: unchanged"
        );
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if let Ok(path) = fs::canonicalize(&self.0) {
            assert!(path.starts_with(fs::canonicalize(std::env::temp_dir()).unwrap()));
            assert!(
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("noetome-pdf-boundary-")
            );
            let _ = fs::remove_dir_all(path);
        }
    }
}
fn runtime() -> PathBuf {
    configured_runtime().expect("real PDF boundary tests need absolute NOETOME_PDF_PYTHON")
}
fn helper(code: &str) -> ParserCommand {
    ParserCommand {
        program: runtime().into_os_string(),
        args: ["-I", "-B", "-X", "utf8", "-c", code]
            .into_iter()
            .map(OsString::from)
            .collect(),
    }
}
fn cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[test]
fn input_byte_limit_never_invents_confirmed_sha_or_pages() {
    let scratch = Scratch::new();
    let input = scratch.input();
    let original = fs::read(&input).unwrap();
    let output = scratch.0.join("limit");
    let error = parse_with(
        &input,
        &output,
        &runtime(),
        Limits {
            input_bytes: 1,
            ..Limits::default()
        },
        cancel(),
    )
    .err()
    .unwrap();
    assert_eq!(error.code, "SIDECAR_LIMIT");
    let failure: Value =
        serde_json::from_slice(&fs::read(output.join("failure.json")).unwrap()).unwrap();
    assert_eq!(failure["input_fingerprint"], Value::Null);
    assert_eq!(failure["page_count"], Value::Null);
    assert_eq!(fs::read(&input).unwrap(), original);
    scratch.sentinel();
}
#[test]
fn cancellation_before_launch_does_not_create_an_output_root() {
    let scratch = Scratch::new();
    let input = scratch.input();
    let output = scratch.0.join("cancel-before");
    let flag = Arc::new(AtomicBool::new(true));
    assert_eq!(
        parse_with(&input, &output, &runtime(), Limits::default(), flag)
            .err()
            .unwrap()
            .code,
        "SIDECAR_CANCELLED"
    );
    assert!(!output.exists());
    scratch.sentinel();
}
#[test]
fn unsuccessful_real_child_preserves_bounded_diagnostics_and_unconfirmed_failure() {
    let scratch = Scratch::new();
    let input = scratch.input();
    let output = scratch.0.join("child-exit");
    let command = helper(
        "import sys\nsys.stdin.buffer.read()\nsys.stderr.write('PDF_TEST_CHILD_FAILURE')\nsys.stderr.flush()\nsys.exit(7)",
    );
    let error = parse_command(&input, &output, &command, Limits::default(), cancel())
        .err()
        .unwrap();
    assert_eq!(error.code, "SIDECAR_EXIT");
    assert_eq!(error.diagnostics, b"PDF_TEST_CHILD_FAILURE");
    assert!(
        String::from_utf8_lossy(&fs::read(output.join("failure.json")).unwrap())
            .contains("PDF_TEST_CHILD_FAILURE")
    );
    assert!(!output.join("parse-result.json").exists());
    scratch.sentinel();
}
#[test]
fn stdout_overflow_is_not_truncated_into_a_successful_pdf_report() {
    let scratch = Scratch::new();
    let input = scratch.input();
    let output = scratch.0.join("stdout-limit");
    let command = helper(
        "import sys\nsys.stdin.buffer.read()\nsys.stdout.buffer.write(b'x'*1048576)\nsys.stdout.buffer.flush()",
    );
    let limits = Limits {
        response_bytes: 128,
        ..Limits::default()
    };
    let error = parse_command(&input, &output, &command, limits, cancel())
        .err()
        .unwrap();
    assert_eq!(error.code, "SIDECAR_LIMIT");
    assert!(!output.join("parse-result.json").exists());
    scratch.sentinel();
}
#[test]
fn synthetic_producer_is_refused_by_the_pdf_wrapper() {
    let scratch = Scratch::new();
    let input = scratch.input();
    let output = scratch.0.join("fake-refused");
    let command = helper(
        "import json,sys\nr=json.load(sys.stdin)\nprint(json.dumps({'protocol_version':1,'request_id':r['request_id'],'input_fingerprint':r['input']['fingerprint'],'workspace_capability_id':r['workspace']['capability_id'],'producer':'fake','parser':{'name':'pdfplumber','version':'0.11.9','capability_version':1,'capabilities':['source_units_v1']},'unit_count':0,'units':[],'resources':[],'issues':[]}))",
    );
    assert_eq!(
        parse_command(&input, &output, &command, Limits::default(), cancel())
            .err()
            .unwrap()
            .code,
        "SIDECAR_PRODUCER"
    );
    scratch.sentinel();
}
#[test]
fn real_parser_resource_limit_returns_nonzero_with_a_locatable_reason() {
    let scratch = Scratch::new();
    let input = scratch.input();
    let output = scratch.0.join("resource-limit");
    let error = parse_with(
        &input,
        &output,
        &runtime(),
        Limits {
            resource_bytes: 32,
            ..Limits::default()
        },
        cancel(),
    )
    .err()
    .unwrap();
    assert!(matches!(error.code, "SIDECAR_EXIT" | "SIDECAR_LIMIT"));
    if error.code == "SIDECAR_EXIT" {
        assert!(
            String::from_utf8_lossy(&error.diagnostics).contains("LIMIT"),
            "{}",
            String::from_utf8_lossy(&error.diagnostics)
        );
    }
    assert!(!output.join("parse-result.json").exists());
    scratch.sentinel();
}

#[cfg(windows)]
fn alive(pid: u32) -> bool {
    // Independent real Windows PID check through the stdlib test interpreter;
    // the Rust workspace continues to forbid unsafe code. Access denial is an
    // error, never evidence that the process exited.
    let code = r#"import ctypes,sys
from ctypes import wintypes as w
k=ctypes.WinDLL('kernel32',use_last_error=True)
k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD]
k.OpenProcess.restype=w.HANDLE
k.GetExitCodeProcess.argtypes=[w.HANDLE,ctypes.POINTER(w.DWORD)]
k.CloseHandle.argtypes=[w.HANDLE]
h=k.OpenProcess(0x1000,False,int(sys.argv[1]))
if not h:
    error=ctypes.get_last_error()
    if error!=87: raise ctypes.WinError(error)
    print('false')
else:
    status=w.DWORD()
    if not k.GetExitCodeProcess(h,ctypes.byref(status)): raise ctypes.WinError(ctypes.get_last_error())
    k.CloseHandle(h)
    print('true' if status.value==259 else 'false')
"#;
    let output = std::process::Command::new(runtime())
        .args(["-I", "-B", "-X", "utf8", "-c", code])
        .arg(pid.to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "PID query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    match output.stdout.as_slice() {
        b"true\r\n" | b"true\n" => true,
        b"false\r\n" | b"false\n" => false,
        _ => panic!("invalid PID query response"),
    }
}
#[cfg(windows)]
fn live_helper(cancel_during: bool) {
    let scratch = Scratch::new();
    let input = scratch.input();
    let output = scratch.0.join("live-child");
    // Deliberately expose a partially written staging file to exercise the
    // publication boundary. The parent observes only the closed, renamed PID
    // file, after diagnostics are flushed; existence alone is not a write ACK.
    let command = helper(
        "import json,os,pathlib,sys,time\nr=json.load(sys.stdin)\np=pathlib.Path(r['workspace']['path'],'handshake.pending')\nwith p.open('x',encoding='ascii') as f:\n    f.flush()\n    time.sleep(0.2)\n    f.write(str(os.getpid()))\nsys.stderr.write('PDF_TEST_CHILD_LIVE')\nsys.stderr.flush()\np.rename(p.with_name('handshake.pid'))\ntime.sleep(60)",
    );
    let flag = cancel();
    let worker_flag = Arc::clone(&flag);
    let worker_input = input.clone();
    let worker_output = output.clone();
    let timeout = if cancel_during { 30000 } else { 5000 };
    let worker = thread::spawn(move || {
        parse_command(
            &worker_input,
            &worker_output,
            &command,
            Limits {
                timeout_ms: timeout,
                ..Limits::default()
            },
            worker_flag,
        )
    });
    let handshake = output.join("parser/handshake.pid");
    let start = Instant::now();
    while !handshake.exists() && start.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(10));
    }
    let pid: u32 = fs::read_to_string(handshake)
        .expect("real child handshake before deadline")
        .parse()
        .unwrap();
    assert!(
        alive(pid),
        "must observe live child before timeout/cancellation"
    );
    if cancel_during {
        flag.store(true, Ordering::SeqCst);
    }
    let error = worker.join().unwrap().err().unwrap();
    assert_eq!(
        error.code,
        if cancel_during {
            "SIDECAR_CANCELLED"
        } else {
            "SIDECAR_TIMEOUT"
        }
    );
    assert!(!alive(pid), "real Python child must have exited");
    assert!(
        error
            .diagnostics
            .windows(b"PDF_TEST_CHILD_LIVE".len())
            .any(|window| window == b"PDF_TEST_CHILD_LIVE")
    );
    assert!(!output.join("parse-result.json").exists());
    scratch.sentinel();
}
#[test]
#[cfg(windows)]
fn real_live_pdf_helper_times_out_and_exits() {
    live_helper(false);
}
#[test]
#[cfg(windows)]
fn real_live_pdf_helper_cancels_and_exits() {
    live_helper(true);
}

#[test]
#[cfg(windows)]
fn input_change_attempt_is_refused_and_preserves_original_bytes() {
    let scratch = Scratch::new();
    let input = scratch.input();
    let original = fs::read(&input).unwrap();
    let output = scratch.0.join("input-change");
    let command = helper(
        "import json,sys\nr=json.load(sys.stdin)\nwith open(r['input']['path'],'wb') as f: f.write(b'changed')",
    );
    let error = parse_command(&input, &output, &command, Limits::default(), cancel())
        .err()
        .unwrap();
    assert_eq!(error.code, "SIDECAR_EXIT");
    assert!(String::from_utf8_lossy(&error.diagnostics).contains("PermissionError"));
    assert_eq!(fs::read(input).unwrap(), original);
    scratch.sentinel();
}
#[test]
#[cfg(windows)]
fn locked_input_and_multiple_hard_links_are_not_confirmed_as_complete() {
    use std::os::windows::fs::OpenOptionsExt;
    let scratch = Scratch::new();
    let input = scratch.input();
    let original = fs::read(&input).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&input)
        .unwrap();
    assert_eq!(
        parse_with(
            &input,
            &scratch.0.join("locked"),
            &runtime(),
            Limits::default(),
            cancel()
        )
        .err()
        .unwrap()
        .code,
        "SIDECAR_INPUT"
    );
    drop(lock);
    let link = scratch.0.join("second-link.pdf");
    fs::hard_link(&input, &link).unwrap();
    assert_eq!(
        parse_with(
            &input,
            &scratch.0.join("hard-linked"),
            &runtime(),
            Limits::default(),
            cancel()
        )
        .err()
        .unwrap()
        .code,
        "SIDECAR_INPUT"
    );
    fs::remove_file(link).unwrap();
    assert_eq!(fs::read(input).unwrap(), original);
    scratch.sentinel();
}
#[test]
#[cfg(windows)]
fn junction_output_parent_cannot_cross_the_selected_sandbox() {
    use std::process::Command;
    let scratch = Scratch::new();
    let input = scratch.input();
    let outside = scratch.0.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("human.txt"), b"preserve human bytes").unwrap();
    let link = scratch.0.join("junction");
    assert!(
        Command::new("cmd.exe")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap()
            .status
            .success()
    );
    let error = parse_with(
        &input,
        &link.join("new-root"),
        &runtime(),
        Limits::default(),
        cancel(),
    )
    .err()
    .unwrap();
    assert_eq!(error.code, "SIDECAR_WORKSPACE");
    assert!(!outside.join("new-root").exists());
    assert_eq!(
        fs::read(outside.join("human.txt")).unwrap(),
        b"preserve human bytes"
    );
    fs::remove_dir(link).unwrap();
    scratch.sentinel();
}

#[test]
#[cfg(windows)]
fn outer_sandbox_replacement_attempt_is_refused_by_live_directory_guards() {
    let scratch = Scratch::new();
    let input = scratch.input();
    let original = fs::read(&input).unwrap();
    let output = scratch.0.join("guarded-root");
    let command = helper(
        "import json,os,pathlib,sys\nr=json.load(sys.stdin)\nroot=pathlib.Path(r['workspace']['path']).parent\nos.rename(root,root.with_name('replacement-root'))",
    );
    let error = parse_command(&input, &output, &command, Limits::default(), cancel())
        .err()
        .unwrap();
    assert_eq!(error.code, "SIDECAR_EXIT");
    assert!(String::from_utf8_lossy(&error.diagnostics).contains("PermissionError"));
    assert!(output.is_dir());
    assert!(!scratch.0.join("replacement-root").exists());
    assert!(!output.join("parse-result.json").exists());
    assert_eq!(fs::read(input).unwrap(), original);
    scratch.sentinel();
}

#[test]
fn real_parser_report_collision_preserves_existing_report_bytes_and_cannot_inspect_as_complete() {
    let scratch = Scratch::new();
    let input = scratch.input();
    let output = scratch.0.join("report-collision");
    let mut command = command(&runtime()).unwrap();
    let adapter_bootstrap = command.args.last().unwrap().to_str().unwrap().to_owned();
    *command.args.last_mut().unwrap()=format!("import pathlib,sys\npathlib.Path(sys.argv[1],'parse-result.md').write_bytes(b'Human report bytes: preserve')\n{adapter_bootstrap}").into();
    command.args.push(output.clone().into_os_string());
    let error = parse_command(&input, &output, &command, Limits::default(), cancel())
        .err()
        .unwrap();
    assert_eq!(error.code, "PDF_WRITE_INCOMPLETE");
    assert_eq!(
        fs::read(output.join("parse-result.md")).unwrap(),
        b"Human report bytes: preserve"
    );
    assert!(output.join("failure.json").is_file());
    assert!(inspect(&output).is_err());
    assert_eq!(
        fs::read(output.join("parse-result.md")).unwrap(),
        b"Human report bytes: preserve"
    );
    scratch.sentinel();
}
