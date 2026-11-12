//! Read-only process identities retained while an owned Windows Job is cleaned up.
use std::collections::BTreeMap;
use winsafe::{HPROCESS, co, guard::CloseHandleGuard};

/// An original process object, never a later process found by reusing its PID.
pub struct NativeProcess {
    handle: CloseHandleGuard<HPROCESS>,
    creation_time: u64,
}
impl NativeProcess {
    /// Open only query/synchronization rights. This type cannot terminate a process.
    pub fn open(pid: u32) -> Result<Self, String> {
        Self::try_open(pid).map_err(|error| format!("open owned process {pid}: {error}"))
    }

    fn try_open(pid: u32) -> Result<Self, co::ERROR> {
        let handle = HPROCESS::OpenProcess(
            co::PROCESS::QUERY_LIMITED_INFORMATION | co::PROCESS::SYNCHRONIZE,
            false,
            pid,
        )?;
        if handle.GetProcessId()? != pid {
            return Err(co::ERROR::INVALID_DATA);
        }
        // Verify that this is a queryable process object before accepting the guard.
        let (creation, _, _, _) = handle.GetProcessTimes()?;
        Ok(Self {
            handle,
            creation_time: u64::from(creation),
        })
    }

    /// Only the signaled process object proves exit; exit code 259 is legitimate.
    pub fn has_exited(&self) -> Result<bool, String> {
        match self
            .handle
            .WaitForSingleObject(Some(0))
            .map_err(|error| format!("wait owned process: {error}"))?
        {
            co::WAIT::OBJECT_0 => Ok(true),
            co::WAIT::TIMEOUT => Ok(false),
            _ => Err("unexpected process wait state".into()),
        }
    }
}

/// Holds process objects discovered in one owned census Job across termination.
#[derive(Default)]
pub struct JobProcesses {
    processes: BTreeMap<(u32, u64), NativeProcess>,
}
impl JobProcesses {
    pub fn new() -> Self {
        Self::default()
    }

    /// Refresh during operation and immediately before terminating the Job.
    /// Newly opened PIDs must still belong to the same Job. No process is killed
    /// or assigned by PID, and identities already held are never reopened.
    pub fn capture(&mut self, job: &win32job::Job) -> Result<(), String> {
        let pids = job
            .query_process_id_list()
            .map_err(|error| error.to_string())?;
        for pid in pids {
            let pid = u32::try_from(pid).map_err(|_| "invalid census PID")?;
            if self
                .processes
                .range((pid, 0)..=(pid, u64::MAX))
                .next()
                .is_some()
            {
                continue;
            }
            // Matches the existing census maximum and bounds retained history too.
            if self.processes.len() >= 1024 {
                return Err("owned process identity limit".into());
            }
            let process = match NativeProcess::try_open(pid) {
                Ok(process) => process,
                Err(co::ERROR::INVALID_PARAMETER) => {
                    // The object can have disappeared between the two queries.
                    // Other OpenProcess errors are never treated as proof of exit.
                    if !job
                        .query_process_id_list()
                        .map_err(|error| error.to_string())?
                        .contains(&(pid as usize))
                    {
                        continue;
                    }
                    return Err("owned process disappeared without census confirmation".into());
                }
                Err(error) => return Err(format!("open owned process {pid}: {error}")),
            };
            let confirmed = job
                .query_process_id_list()
                .map_err(|error| error.to_string())?;
            if !confirmed.contains(&(pid as usize)) {
                // A disappearing PID cannot establish Job membership. A completed
                // object needs no further wait; a live/ambiguous object fails closed.
                if process.has_exited()? {
                    continue;
                }
                return Err("process left owned census before identity confirmation".into());
            }
            self.processes.insert((pid, process.creation_time), process);
        }
        Ok(())
    }

    /// Census zero and native root exit are separate conditions at the call site.
    pub fn all_exited(&self) -> Result<bool, String> {
        let mut exited = true;
        for process in self.processes.values() {
            exited &= process.has_exited()?;
        }
        Ok(exited)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    #[test]
    fn current_process_is_not_terminal_and_invalid_pid_is_rejected() {
        let process = NativeProcess::open(std::process::id()).unwrap();
        assert!(!process.has_exited().unwrap());
        assert!(NativeProcess::open(0).is_err());
    }

    #[test]
    fn terminal_exit_code_259_is_not_mistaken_for_running() {
        let mut child = Command::new(std::env::var_os("NOETOME_TEST_PYTHON").unwrap())
            .args([
                "-I",
                "-B",
                "-c",
                "import sys; sys.stdin.buffer.read(1); sys.exit(259)",
            ])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let process = NativeProcess::open(child.id()).unwrap();
        assert!(!process.has_exited().unwrap());
        child.stdin.take().unwrap().write_all(b"x").unwrap();
        assert_eq!(child.wait().unwrap().code(), Some(259));
        assert!(process.has_exited().unwrap());
    }

    #[test]
    fn census_retains_original_child_handle_through_job_exit() {
        use std::os::windows::io::AsRawHandle;
        let mut child = Command::new(std::env::var_os("NOETOME_TEST_PYTHON").unwrap())
            .args(["-I", "-B", "-c", "import sys; sys.stdin.buffer.read(1)"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let job = win32job::Job::create().unwrap();
        job.assign_process(child.as_raw_handle() as isize).unwrap();
        let mut processes = JobProcesses::new();
        processes.capture(&job).unwrap();
        assert_eq!(processes.processes.len(), 1);
        assert!(!processes.all_exited().unwrap());
        child.kill().unwrap();
        child.wait().unwrap();
        // Process signaling and Job accounting are separate observations. Keep
        // the original handles while waiting for both, as the API does not
        // promise that the Job list is updated when child.wait() returns.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let remaining = job.query_process_id_list().unwrap();
            let handles_exited = processes.all_exited().unwrap();
            if remaining.is_empty() && handles_exited {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Job cleanup did not converge: remaining={remaining:?}, handles_exited={handles_exited}"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
