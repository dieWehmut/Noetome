//! Windows lifetime guard shared by PDF and OCR workers.
use process_wrap::std::*;
use std::{io, os::windows::io::AsRawHandle, sync::Arc};

#[derive(Debug)]
struct KillOnClose {
    job: Arc<win32job::Job>,
}

impl CommandWrapper for KillOnClose {
    fn wrap_child(
        &mut self,
        mut child: Box<dyn ChildWrapper>,
        _: &CommandWrap,
    ) -> io::Result<Box<dyn ChildWrapper>> {
        let Some(handle) = child.process_handle() else {
            let _ = child.start_kill();
            return Err(io::Error::other("worker has no process handle"));
        };
        if let Err(error) = self.job.assign_process(handle.as_raw_handle() as isize) {
            let _ = child.start_kill();
            return Err(io::Error::other(error.to_string()));
        }
        Ok(child)
    }
}

pub(super) fn guard(command: &mut CommandWrap) -> io::Result<Arc<win32job::Job>> {
    let mut limits = win32job::ExtendedLimitInfo::new();
    limits.limit_kill_on_job_close();
    let job = Arc::new(
        win32job::Job::create_with_limit_info(&limits)
            .map_err(|error| io::Error::other(error.to_string()))?,
    );
    let mut flags = CreationFlags(Default::default());
    flags.0.0 = 0x08000000; // CREATE_NO_WINDOW
    // JobObject suspends the root before all wrap_child hooks, then resumes it
    // after this guard has assigned it. Descendants inherit the guard Job too.
    // std JobObject itself omits KILL_ON_JOB_CLOSE; this additional owned Job
    // closes when the host is terminated, including a hard-killed CLI.
    command
        .wrap(flags)
        .wrap(KillOnClose { job: job.clone() })
        .wrap(JobObject);
    Ok(job)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        thread,
        time::{Duration, Instant},
    };

    #[test]
    fn closing_guard_terminates_worker_and_descendant() {
        let mut command = CommandWrap::with_new("cmd.exe", |command| {
            command.args(["/D", "/C", "ping -n 30 127.0.0.1 >nul"]);
        });
        let job = guard(&mut command).unwrap();
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let pids = loop {
            let pids = job.query_process_id_list().unwrap();
            if pids.len() >= 2 {
                break pids;
            }
            assert!(
                Instant::now() < deadline,
                "worker descendant was not observed"
            );
            thread::sleep(Duration::from_millis(20));
        };
        let processes: Vec<_> = pids
            .into_iter()
            .map(|pid| {
                crate::native_process::NativeProcess::open(u32::try_from(pid).unwrap()).unwrap()
            })
            .collect();
        drop(command);
        drop(job);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            // Check native process objects directly: the wrapper's completion
            // port notifications must not be consumed by a preceding try_wait.
            let root_exited = child.inner_mut().try_wait().unwrap().is_some();
            let tree_exited = processes
                .iter()
                .all(|process| process.has_exited().unwrap());
            if root_exited && tree_exited {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "guard left a live process object"
            );
            thread::sleep(Duration::from_millis(20));
        }
        child.inner_mut().wait().unwrap();
    }
}
