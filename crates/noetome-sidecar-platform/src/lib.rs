//! Safe interface to the small reviewed Windows process boundary.

#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::path::Path;
use std::time::Instant;

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

pub struct SpawnSpec<'a> {
    pub executable: &'a Path,
    pub args: &'a [OsString],
    pub cwd: &'a Path,
    pub environment: &'a [(OsString, OsString)],
}
pub struct Spawned {
    pub process: JobProcess,
    pub stdin: File,
    pub stdout: File,
    pub stderr: File,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootExit {
    pub code: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeState {
    pub active_processes: u32,
    pub total_processes: u32,
}
pub struct JobProcess {
    #[cfg(windows)]
    inner: windows::Job,
}
#[cfg(not(windows))]
fn unavailable() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "Windows contained process adapter not implemented",
    )
}
impl JobProcess {
    pub fn spawn_with_cleanup_timeout(
        spec: &SpawnSpec<'_>,
        cleanup_timeout: std::time::Duration,
    ) -> io::Result<Spawned> {
        #[cfg(windows)]
        {
            windows::spawn(spec, cleanup_timeout)
        }
        #[cfg(not(windows))]
        {
            let _ = (spec, cleanup_timeout);
            Err(unavailable())
        }
    }
    pub fn spawn(spec: &SpawnSpec<'_>) -> io::Result<Spawned> {
        #[cfg(windows)]
        {
            windows::spawn(spec, std::time::Duration::from_secs(2))
        }
        #[cfg(not(windows))]
        {
            let _ = spec;
            Err(unavailable())
        }
    }
    pub fn pid(&self) -> u32 {
        #[cfg(windows)]
        {
            self.inner.pid()
        }
        #[cfg(not(windows))]
        {
            0
        }
    }
    pub fn creation_time(&self) -> u64 {
        #[cfg(windows)]
        {
            self.inner.creation_time()
        }
        #[cfg(not(windows))]
        {
            0
        }
    }
    pub fn try_wait_root(&mut self) -> io::Result<Option<RootExit>> {
        #[cfg(windows)]
        {
            self.inner.try_wait_root()
        }
        #[cfg(not(windows))]
        {
            Err(unavailable())
        }
    }
    pub fn tree_state(&self) -> io::Result<TreeState> {
        #[cfg(windows)]
        {
            self.inner.tree_state()
        }
        #[cfg(not(windows))]
        {
            Err(unavailable())
        }
    }
    pub fn terminate_tree(&self, exit_code: u32) -> io::Result<()> {
        #[cfg(windows)]
        {
            self.inner.terminate_tree(exit_code)
        }
        #[cfg(not(windows))]
        {
            let _ = exit_code;
            Err(unavailable())
        }
    }
    pub fn wait_tree_empty(&mut self, deadline: Instant) -> io::Result<()> {
        loop {
            self.try_wait_root()?;
            if self.tree_state()?.active_processes == 0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "contained process tree did not become empty",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
pub fn file_link_count(file: &File) -> io::Result<u32> {
    #[cfg(windows)]
    {
        windows::file_link_count(file)
    }
    #[cfg(not(windows))]
    {
        let _ = file;
        Err(unavailable())
    }
}

/// Configures only this process for an intentional headless crash test.
/// Call inside the private helper before inducing the crash; Host startup does
/// not invoke this function. It does not modify machine or user WER settings.
pub fn configure_crash_test_process() -> io::Result<()> {
    #[cfg(windows)]
    {
        windows::configure_crash_test_process()
    }
    #[cfg(not(windows))]
    {
        Err(unavailable())
    }
}

/// Cancels synchronous pipe I/O issued by this exact owned worker thread.
/// The caller must still wait and join; cancellation alone does not prove exit.
pub fn cancel_io_thread<T>(thread: &std::thread::JoinHandle<T>) -> io::Result<()> {
    #[cfg(windows)]
    {
        windows::cancel_io_thread(thread)
    }
    #[cfg(not(windows))]
    {
        let _ = thread;
        Err(unavailable())
    }
}

/// Checks native thread termination before a caller performs a bounded join.
pub fn io_thread_exited<T>(thread: &std::thread::JoinHandle<T>) -> io::Result<bool> {
    #[cfg(windows)]
    {
        windows::io_thread_exited(thread)
    }
    #[cfg(not(windows))]
    {
        let _ = thread;
        Err(unavailable())
    }
}

/// Distinguishes a failed native spawn whose child cleanup was not confirmed.
pub fn is_cleanup_failure(error: &io::Error) -> bool {
    #[cfg(windows)]
    {
        error
            .get_ref()
            .is_some_and(|inner| inner.is::<CleanupFailure>())
    }
    #[cfg(not(windows))]
    {
        let _ = error;
        false
    }
}
#[cfg(windows)]
#[derive(Debug)]
struct CleanupFailure(String);
#[cfg(windows)]
impl std::fmt::Display for CleanupFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
#[cfg(windows)]
impl std::error::Error for CleanupFailure {}
#[cfg(windows)]
fn cleanup_failure(message: String) -> io::Error {
    io::Error::other(CleanupFailure(message))
}

/// A process identity and wait guard used by independent lifecycle observers.
/// Release the guard promptly after observing exit, before verifying Job accounting.
pub struct ProcessObserver {
    #[cfg(windows)]
    inner: windows::Observer,
}
impl ProcessObserver {
    pub fn open(pid: u32) -> io::Result<Self> {
        #[cfg(windows)]
        {
            Ok(Self {
                inner: windows::Observer::open(pid)?,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            Err(unavailable())
        }
    }
    pub fn creation_time(&self) -> u64 {
        #[cfg(windows)]
        {
            self.inner.creation_time()
        }
        #[cfg(not(windows))]
        {
            0
        }
    }
    pub fn wait_exit(&self, timeout: std::time::Duration) -> io::Result<Option<u32>> {
        #[cfg(windows)]
        {
            self.inner.wait_exit(timeout)
        }
        #[cfg(not(windows))]
        {
            let _ = timeout;
            Err(unavailable())
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    fn sandbox() -> PathBuf {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.artifacts/platform-tests");
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
    #[test]
    fn helper_echo() {
        if std::env::var_os("NOETOME_TEST_PLATFORM_ECHO").is_none() {
            return;
        }
        let mut bytes = Vec::new();
        std::io::stdin().read_to_end(&mut bytes).unwrap();
        std::io::stdout().write_all(b"PLATFORM_ECHO=").unwrap();
        std::io::stdout().write_all(&bytes).unwrap();
        std::io::stderr().write_all(b"platform diagnostic").unwrap();
    }
    #[test]
    fn helper_headless_crash_test_policy() {
        if std::env::var_os("NOETOME_TEST_PLATFORM_HEADLESS").is_none() {
            return;
        }
        configure_crash_test_process().expect("native test process policy configures");
        let (mode, flags) = windows::crash_test_flags();
        let flags = flags.expect("configured helper flags are readable");
        assert_ne!(mode & 2, 0, "this helper must opt out of WER invocation");
        assert_ne!(flags & 32, 0, "this helper must disable WER UI");
        std::io::stdout()
            .write_all(b"HEADLESS_CRASH_POLICY_CONFIGURED")
            .unwrap();
    }
    #[test]
    fn headless_crash_test_policy_is_verified_in_child_only() {
        let parent_flags = windows::crash_test_flags();
        let root = sandbox();
        let executable = std::env::current_exe().unwrap();
        let args = [
            OsString::from("--exact"),
            OsString::from("tests::helper_headless_crash_test_policy"),
            OsString::from("--nocapture"),
        ];
        let environment = [(
            OsString::from("NOETOME_TEST_PLATFORM_HEADLESS"),
            OsString::from("1"),
        )];
        let Spawned {
            mut process,
            stdin,
            mut stdout,
            mut stderr,
        } = JobProcess::spawn(&SpawnSpec {
            executable: &executable,
            args: &args,
            cwd: &root,
            environment: &environment,
        })
        .unwrap();
        drop(stdin);
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).unwrap();
        let mut diagnostic = Vec::new();
        stderr.read_to_end(&mut diagnostic).unwrap();
        process
            .wait_tree_empty(Instant::now() + Duration::from_secs(2))
            .unwrap();
        assert_eq!(
            process.try_wait_root().unwrap(),
            Some(RootExit { code: 0 }),
            "native child policy must configure rather than fail: {}",
            String::from_utf8_lossy(&diagnostic)
        );
        assert!(
            output
                .windows(b"HEADLESS_CRASH_POLICY_CONFIGURED".len())
                .any(|part| part == b"HEADLESS_CRASH_POLICY_CONFIGURED")
        );
        assert_eq!(
            windows::crash_test_flags(),
            parent_flags,
            "child policy must not change Host flags"
        );
    }
    #[test]
    fn actual_suspended_job_process_exchanges_pipe_bytes_and_reaps() {
        let root = sandbox();
        let executable = std::env::current_exe().unwrap();
        let args = [
            OsString::from("--exact"),
            OsString::from("tests::helper_echo"),
            OsString::from("--nocapture"),
        ];
        // A controlled native test environment includes the documented hidden
        // drive entry. Core's process tests exercise its minimal Host context.
        let environment = [
            (
                OsString::from("NOETOME_TEST_PLATFORM_ECHO"),
                OsString::from("1"),
            ),
            (OsString::from("=D:"), OsString::from("D:\\")),
        ];
        let mut spawned = JobProcess::spawn(&SpawnSpec {
            executable: &executable,
            args: &args,
            cwd: &root,
            environment: &environment,
        })
        .expect("real contained process starts");
        assert_ne!(spawned.process.pid(), 0);
        assert_ne!(spawned.process.creation_time(), 0);
        let observer =
            ProcessObserver::open(spawned.process.pid()).expect("independent process handle opens");
        assert_eq!(observer.creation_time(), spawned.process.creation_time());
        assert_eq!(observer.wait_exit(Duration::ZERO).unwrap(), None);
        let observation = std::thread::spawn(move || {
            let result = observer.wait_exit(Duration::from_secs(2));
            drop(observer);
            result
        });
        spawned.stdin.write_all(b"request bytes").unwrap();
        drop(spawned.stdin);
        let mut stdout = Vec::new();
        spawned.stdout.read_to_end(&mut stdout).unwrap();
        let mut stderr = Vec::new();
        spawned.stderr.read_to_end(&mut stderr).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        spawned.process.wait_tree_empty(deadline).unwrap();
        assert_eq!(
            spawned.process.try_wait_root().unwrap(),
            Some(RootExit { code: 0 })
        );
        assert_eq!(spawned.process.tree_state().unwrap().active_processes, 0);
        assert_eq!(observation.join().unwrap().unwrap(), Some(0));
        assert!(
            stdout
                .windows(b"PLATFORM_ECHO=request bytes".len())
                .any(|v| v == b"PLATFORM_ECHO=request bytes")
        );
        assert_eq!(stderr, b"platform diagnostic");
    }
    #[test]
    fn normal_file_has_one_native_link() {
        let root = sandbox();
        let path = root.join("regular.txt");
        std::fs::write(&path, b"bytes").unwrap();
        assert_eq!(
            file_link_count(&File::open(path).unwrap()).expect("native link count"),
            1
        );
    }
    #[test]
    fn failed_suspended_preparation_reaps_actual_root_before_resume() {
        let root = sandbox();
        let executable = std::env::current_exe().unwrap();
        let args = [
            OsString::from("--exact"),
            OsString::from("tests::helper_echo"),
            OsString::from("--nocapture"),
        ];
        let environment = [(
            OsString::from("NOETOME_TEST_PLATFORM_ECHO"),
            OsString::from("1"),
        )];
        let spec = SpawnSpec {
            executable: &executable,
            args: &args,
            cwd: &root,
            environment: &environment,
        };
        let mut observer = None;
        let started = Instant::now();
        let failure = windows::spawn_with_probe(&spec, Duration::from_millis(20), |job| {
            observer = Some(ProcessObserver::open(job.pid())?);
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "injected failure before primary-thread resume",
            ))
        })
        .err()
        .expect("preparation must fail");
        let elapsed = started.elapsed();
        let observer = observer.expect("independent guard captures actual suspended process");
        assert_ne!(observer.creation_time(), 0);
        assert_eq!(
            observer.wait_exit(Duration::from_millis(100)).unwrap(),
            Some(1)
        );
        drop(observer);
        assert_eq!(failure.kind(), io::ErrorKind::InvalidData);
        assert!(
            !is_cleanup_failure(&failure),
            "actual root exited and Job accounting converged: {failure}"
        );
        assert!(
            elapsed < Duration::from_millis(200),
            "cleanup must honor 20ms rather than its default2s: {elapsed:?}"
        );
    }
}
