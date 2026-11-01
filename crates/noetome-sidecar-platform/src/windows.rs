//! The only unsafe module: Win32 buffer/handle ownership is checked at each call.

use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io;
use std::mem::{offset_of, size_of};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr::{null, null_mut};

use crate::{JobProcess, RootExit, SpawnSpec, Spawned, TreeState};
use windows_sys::Win32::Foundation::{
    ERROR_INSUFFICIENT_BUFFER, FILETIME, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
    SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_RENAME_INFO, FileRenameInfo,
    GetFileInformationByHandle, GetFinalPathNameByHandleW, SetFileInformationByHandle,
};
use windows_sys::Win32::System::IO::CancelSynchronousIo;
use windows_sys::Win32::System::JobObjects::{
    CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Memory::{GetProcessHeap, HEAP_ZERO_MEMORY, HeapAlloc, HeapFree};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
    DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
    GetProcessTimes, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST, PROCESS_INFORMATION,
    ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, UpdateProcThreadAttribute,
    WaitForSingleObject,
};

fn os_error() -> io::Error {
    io::Error::last_os_error()
}
fn checked(value: i32) -> io::Result<()> {
    if value == 0 { Err(os_error()) } else { Ok(()) }
}
fn own(raw: HANDLE) -> io::Result<OwnedHandle> {
    if raw.is_null() || raw == INVALID_HANDLE_VALUE {
        return Err(os_error());
    }
    // SAFETY: successful Win32 creation returned a valid uniquely owned handle;
    // this is the single transfer and OwnedHandle closes it exactly once.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}
fn native(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle()
}
fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut result: Vec<_> = value.encode_wide().collect();
    if result.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "NUL in native path/argument/environment",
        ));
    }
    result.push(0);
    Ok(result)
}
fn argument(value: &OsStr) -> io::Result<Vec<u16>> {
    let input = wide(value)?;
    let mut result = vec![b'"' as u16];
    let mut slashes = 0;
    for &ch in &input[..input.len() - 1] {
        if ch == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            b'\\' as u16,
            if ch == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        slashes = 0;
        result.push(ch);
    }
    result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    result.push(b'"' as u16);
    Ok(result)
}
fn command_line(spec: &SpawnSpec<'_>) -> io::Result<Vec<u16>> {
    let mut result = argument(spec.executable.as_os_str())?;
    for arg in spec.args {
        result.push(b' ' as u16);
        result.extend(argument(arg)?);
        if result.len() >= 32767 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "native command line exceeds Windows limit",
            ));
        }
    }
    result.push(0);
    if result.len() > 32767 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native command line exceeds Windows limit",
        ));
    }
    Ok(result)
}
fn environment(spec: &SpawnSpec<'_>) -> io::Result<Vec<u16>> {
    let mut entries = spec.environment.iter().collect::<Vec<_>>();
    entries.sort_by_key(|(key, _)| key.to_string_lossy().to_uppercase());
    let mut names = std::collections::BTreeSet::new();
    let mut result = Vec::new();
    for (key, value) in entries {
        let key_wide = wide(key)?;
        let value_wide = wide(value)?;
        let name = &key_wide[..key_wide.len() - 1];
        // CreateProcess explicitly permits the inherited hidden drive-current-
        // directory entries `=C:`. They are distinct from ordinary variables;
        // no other name containing '=' is accepted.
        let drive = name.len() == 3
            && name[0] == b'=' as u16
            && name[2] == b':' as u16
            && ((b'A' as u16..=b'Z' as u16).contains(&name[1])
                || (b'a' as u16..=b'z' as u16).contains(&name[1]));
        if name.is_empty()
            || (name.contains(&(b'=' as u16)) && !drive)
            || !names.insert(key.to_string_lossy().to_uppercase())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid or duplicate native environment name: {key:?}"),
            ));
        }
        result.extend_from_slice(&key_wide[..key_wide.len() - 1]);
        result.push(b'=' as u16);
        result.extend(value_wide);
    }
    if result.is_empty() {
        result.push(0);
    }
    result.push(0);
    Ok(result)
}
fn pipe(parent_reads: bool) -> io::Result<(OwnedHandle, OwnedHandle)> {
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let (mut read, mut write) = (null_mut(), null_mut());
    // SAFETY: writable out-pointers and a correctly sized live security record.
    checked(unsafe { CreatePipe(&mut read, &mut write, &security, 0) })?;
    let read = own(read)?;
    let write = own(write)?;
    let parent = if parent_reads { &read } else { &write };
    // SAFETY: owned valid pipe handle; removes inheritance on the parent end.
    checked(unsafe { SetHandleInformation(native(parent), HANDLE_FLAG_INHERIT, 0) })?;
    Ok((read, write))
}
struct Attributes {
    heap: HANDLE,
    list: LPPROC_THREAD_ATTRIBUTE_LIST,
    initialized: bool,
}
impl Attributes {
    fn new() -> io::Result<Self> {
        let mut bytes = 0;
        // SAFETY: documented sizing query, no attribute storage supplied yet.
        let query = unsafe { InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut bytes) };
        if query != 0
            || os_error().raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
            || bytes == 0
        {
            return Err(os_error());
        }
        // SAFETY: process heap is live; HeapAlloc supplies native aligned storage.
        let heap = unsafe { GetProcessHeap() };
        if heap.is_null() {
            return Err(os_error());
        }
        // SAFETY: live process heap and queried positive allocation size.
        let allocation = unsafe { HeapAlloc(heap, HEAP_ZERO_MEMORY, bytes) };
        if allocation.is_null() {
            return Err(io::Error::other("attribute-list allocation failed"));
        }
        let mut result = Self {
            heap,
            list: allocation.cast(),
            initialized: false,
        };
        // SAFETY: allocation is aligned and at least the queried size; same count.
        checked(unsafe { InitializeProcThreadAttributeList(result.list, 2, 0, &mut bytes) })?;
        result.initialized = true;
        Ok(result)
    }
    fn set(&self, key: usize, values: &[HANDLE]) -> io::Result<()> {
        // SAFETY: initialized list; pointer/byte size describe the live handle
        // slice, retained in spawn until this list is deleted after CreateProcess.
        checked(unsafe {
            UpdateProcThreadAttribute(
                self.list,
                0,
                key,
                values.as_ptr().cast_mut().cast(),
                std::mem::size_of_val(values),
                null_mut(),
                null_mut(),
            )
        })
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: only initialized lists are deleted; exact heap allocation is
        // freed once, after no Win32 process-creation call can reference it.
        unsafe {
            if self.initialized {
                DeleteProcThreadAttributeList(self.list);
            }
            HeapFree(self.heap, 0, self.list.cast());
        }
    }
}

pub(crate) struct Job {
    job: OwnedHandle,
    process: Option<OwnedHandle>,
    pid: u32,
    creation_time: u64,
    status: Option<RootExit>,
}
impl Job {
    pub(crate) fn pid(&self) -> u32 {
        self.pid
    }
    pub(crate) fn creation_time(&self) -> u64 {
        self.creation_time
    }
    pub(crate) fn try_wait_root(&mut self) -> io::Result<Option<RootExit>> {
        if self.status.is_some() {
            return Ok(self.status);
        }
        let process = self
            .process
            .as_ref()
            .ok_or_else(|| io::Error::other("missing root process ownership"))?;
        // SAFETY: valid retained process handle, zero-duration status observation.
        match unsafe { WaitForSingleObject(native(process), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                // SAFETY: retained signaled process and exact writable code.
                checked(unsafe { GetExitCodeProcess(native(process), &mut code) })?;
                self.status = Some(RootExit { code });
                // Cache status and release the creation handle before checking
                // Job accounting, without relying on handle-reference behavior.
                self.process.take();
                Ok(self.status)
            }
            _ => Err(os_error()),
        }
    }
    pub(crate) fn tree_state(&self) -> io::Result<TreeState> {
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: owned Job, exact initialized typed output buffer and byte size.
        checked(unsafe {
            QueryInformationJobObject(
                native(&self.job),
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                null_mut(),
            )
        })?;
        Ok(TreeState {
            active_processes: info.ActiveProcesses,
            total_processes: info.TotalProcesses,
        })
    }
    pub(crate) fn terminate_tree(&self, code: u32) -> io::Result<()> {
        // SAFETY: owned private Job has terminate rights; this affects its exact
        // associated hierarchy. Callers still wait and verify tree emptiness.
        checked(unsafe { TerminateJobObject(native(&self.job), code) })
    }
}
pub(crate) fn spawn(
    spec: &SpawnSpec<'_>,
    cleanup_timeout: std::time::Duration,
) -> io::Result<Spawned> {
    spawn_with_probe(spec, cleanup_timeout, |_| Ok(()))
}
pub(crate) fn spawn_with_probe(
    spec: &SpawnSpec<'_>,
    cleanup_timeout: std::time::Duration,
    probe: impl FnOnce(&Job) -> io::Result<()>,
) -> io::Result<Spawned> {
    if cleanup_timeout.is_zero() || cleanup_timeout > std::time::Duration::from_secs(2) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native cleanup timeout must be positive and at most two seconds",
        ));
    }
    if !spec.executable.is_absolute() || !spec.cwd.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native executable and cwd must be complete absolute paths",
        ));
    }
    let application = wide(spec.executable.as_os_str())?;
    let cwd = wide(spec.cwd.as_os_str())?;
    let mut command = command_line(spec)?;
    let env = environment(spec)?;
    let _executable_guard = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(spec.executable)?;
    use std::os::windows::fs::MetadataExt;
    let metadata = _executable_guard.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "executable is not an ordinary file",
        ));
    }
    // SAFETY: null name/security creates a fresh unnamed non-inheritable Job.
    let job = own(unsafe { CreateJobObjectW(null(), null()) })?;
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: valid Job and exact initialized extended-limit record. No breakaway.
    checked(unsafe {
        SetInformationJobObject(
            native(&job),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    })?;
    let (stdin_read, stdin_write) = pipe(false)?;
    let (stdout_read, stdout_write) = pipe(true)?;
    let (stderr_read, stderr_write) = pipe(true)?;
    let child_handles = [
        native(&stdin_read),
        native(&stdout_write),
        native(&stderr_write),
    ];
    let jobs = [native(&job)];
    let attributes = Attributes::new()?;
    attributes.set(PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize, &child_handles)?;
    attributes.set(PROC_THREAD_ATTRIBUTE_JOB_LIST as usize, &jobs)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = child_handles[0];
    startup.StartupInfo.hStdOutput = child_handles[1];
    startup.StartupInfo.hStdError = child_handles[2];
    startup.lpAttributeList = attributes.list;
    let mut info = PROCESS_INFORMATION::default();
    // SAFETY: mutable terminated command line; terminated executable/cwd and
    // double-terminated environment. All exact native structure/attribute buffers
    // and inherited three-pipe handles live through this synchronous call. The
    // Job list atomically assigns while CREATE_SUSPENDED prevents execution.
    checked(unsafe {
        CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_SUSPENDED
                | CREATE_NO_WINDOW
                | CREATE_UNICODE_ENVIRONMENT
                | EXTENDED_STARTUPINFO_PRESENT,
            env.as_ptr().cast(),
            cwd.as_ptr(),
            &startup.StartupInfo,
            &mut info,
        )
    })?;
    let process = own(info.hProcess)?;
    let thread = own(info.hThread)?;
    let mut inner = Job {
        job,
        process: Some(process),
        pid: info.dwProcessId,
        creation_time: 0,
        status: None,
    };
    let prepared = (|| {
        let process = native(
            inner
                .process
                .as_ref()
                .ok_or_else(|| io::Error::other("missing newly created process handle"))?,
        );
        let mut assigned = 0;
        // SAFETY: newly created retained process, exact owned Job and writable
        // BOOL; primary thread is still suspended and no handle is released.
        checked(unsafe { IsProcessInJob(process, native(&inner.job), &mut assigned) })?;
        if assigned == 0 {
            return Err(io::Error::other(
                "atomic Job assignment was not established",
            ));
        }
        let (mut creation, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        // SAFETY: same retained process and four exact writable FILETIME buffers.
        checked(unsafe {
            GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user)
        })?;
        inner.creation_time =
            ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64;
        probe(&inner)?;
        drop(attributes);
        drop(stdin_read);
        drop(stdout_write);
        drop(stderr_write);
        // SAFETY: exact uniquely owned primary thread returned by CreateProcess,
        // proven associated Job, still suspended. No thread enumeration/resume race.
        let previous = unsafe { ResumeThread(native(&thread)) };
        if previous == u32::MAX {
            return Err(os_error());
        }
        if previous != 1 {
            return Err(io::Error::other(
                "unexpected primary-thread suspension count",
            ));
        }
        Ok(())
    })();
    drop(thread);
    if let Err(error) = prepared {
        inner.terminate_tree(1).map_err(|cleanup| {
            crate::cleanup_failure(format!(
                "process preparation failed ({error}); Job termination failed ({cleanup})"
            ))
        })?;
        let deadline = std::time::Instant::now() + cleanup_timeout;
        loop {
            inner.try_wait_root().map_err(|cleanup| {
                crate::cleanup_failure(format!(
                    "process preparation failed ({error}); root status failed ({cleanup})"
                ))
            })?;
            if inner
                .tree_state()
                .map_err(|cleanup| {
                    crate::cleanup_failure(format!(
                        "process preparation failed ({error}); Job accounting failed ({cleanup})"
                    ))
                })?
                .active_processes
                == 0
            {
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err(crate::cleanup_failure(format!(
                    "process preparation failed ({error}); Job cleanup did not finish"
                )));
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        return Err(error);
    }
    Ok(Spawned {
        process: JobProcess { inner },
        stdin: File::from(stdin_write),
        stdout: File::from(stdout_read),
        stderr: File::from(stderr_read),
    })
}
pub(crate) fn file_link_count(file: &File) -> io::Result<u32> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: borrowed live File handle and exact native output record. No handle
    // ownership transfer; caller keeps its existing identity/read guard.
    checked(unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) })?;
    Ok(info.nNumberOfLinks)
}

pub(crate) fn file_identity(file: &File) -> io::Result<(u32, u32, u32)> {
    let mut info: BY_HANDLE_FILE_INFORMATION = Default::default();
    // SAFETY: the borrowed live File owns its handle and info has the exact
    // writable Win32 output type for the duration of this synchronous call.
    checked(unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) })?;
    Ok((
        info.dwVolumeSerialNumber,
        info.nFileIndexHigh,
        info.nFileIndexLow,
    ))
}

fn final_file_path(file: &File) -> io::Result<std::path::PathBuf> {
    // A finite buffer includes the terminating NUL at the Win32 maximum path
    // size. The returned count excludes NUL on success and may not exceed it.
    let mut buffer = vec![0u16; 32768];
    // SAFETY: File remains live and buffer is writable for exactly its reported
    // u16 capacity. Zero flags request the normalized DOS-style final path.
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    };
    if length == 0 {
        return Err(os_error());
    }
    if length as usize >= buffer.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "native file path exceeds bounded buffer",
        ));
    }
    Ok(OsString::from_wide(&buffer[..length as usize]).into())
}

fn ordinary_directory(path: &std::path::Path) -> io::Result<File> {
    let directory = OpenOptions::new()
        .access_mode(0xa0) // FILE_TRAVERSE | FILE_READ_ATTRIBUTES; child entries remain writable.
        .share_mode(3) // Readers/writers allowed; deletion/renaming is denied.
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let mut info: BY_HANDLE_FILE_INFORMATION = Default::default();
    // SAFETY: the owned directory handle and exact native output remain live.
    checked(unsafe { GetFileInformationByHandle(directory.as_raw_handle(), &mut info) })?;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT)
        != FILE_ATTRIBUTE_DIRECTORY
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication parent must be an ordinary directory",
        ));
    }
    Ok(directory)
}

fn publication_directory_chain(path: &std::path::Path) -> io::Result<Vec<File>> {
    let components: Vec<_> = path
        .ancestors()
        .filter(|entry| entry.is_absolute())
        .collect();
    // Walk from the volume root so each parent is already held against rename
    // before resolving its child. Reject junctions at every resolved component.
    components
        .into_iter()
        .rev()
        .map(ordinary_directory)
        .collect()
}

pub(crate) fn rename_file_no_replace(file: &File, target: &std::path::Path) -> io::Result<()> {
    if !target.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication target must be absolute",
        ));
    }
    let name = target.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication target needs one file name",
        )
    })?;
    let name = wide(name)?;
    let plain = &name[..name.len() - 1];
    if plain.is_empty()
        || plain
            .iter()
            .any(|&ch| matches!(ch, 34 | 42 | 47 | 58 | 60 | 62 | 63 | 92 | 124))
        || plain.last().is_some_and(|&ch| ch == 32 || ch == 46)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication target must be a plain file name without streams",
        ));
    }
    let parent = target.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication target needs a parent",
        )
    })?;
    let original_path = final_file_path(file)?;
    let original_parent = original_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "opened file has no ordinary parent",
        )
    })?;
    let source_directories = publication_directory_chain(original_parent)?;
    let target_directories = publication_directory_chain(parent)?;
    let source_directory = source_directories.last().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication source has no parent guard",
        )
    })?;
    let target_directory = target_directories.last().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication target has no parent guard",
        )
    })?;
    if file_identity(source_directory)? != file_identity(target_directory)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication cannot move a file outside its current directory",
        ));
    }
    let mut info: BY_HANDLE_FILE_INFORMATION = Default::default();
    // SAFETY: the source File owns a live handle and the output has exact layout.
    checked(unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) })?;
    if info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
        || info.nNumberOfLinks != 1
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication requires one ordinary singly linked file",
        ));
    }
    let name = wide(target.as_os_str())?;
    if name.len() > 32768 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication path exceeds native bounded buffer",
        ));
    }
    let name_bytes = (name.len() - 1)
        .checked_mul(size_of::<u16>())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "publication name length overflow",
            )
        })?;
    let buffer_bytes = offset_of!(FILE_RENAME_INFO, FileName)
        .checked_add(name.len() * size_of::<u16>())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "publication buffer length overflow",
            )
        })?
        .max(size_of::<FILE_RENAME_INFO>());
    let buffer_length = u32::try_from(buffer_bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication buffer exceeds Win32 byte limit",
        )
    })?;
    // Vec<u64> provides the native pointer alignment on supported Windows x64;
    // retain enough zeroed bytes for the header, variable name and final NUL.
    let mut storage = vec![0u64; buffer_bytes.div_ceil(size_of::<u64>())];
    let rename = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // SAFETY: aligned, fully initialized storage is large enough for the exact
    // native header and the flexible WCHAR tail. The source File, all guarded
    // ancestors, and the buffer remain live through the synchronous call. The
    // false ReplaceIfExists flag makes collision rejection atomic in the OS.
    unsafe {
        (*rename).Anonymous.ReplaceIfExists = false;
        (*rename).RootDirectory = null_mut();
        (*rename).FileNameLength = u32::try_from(name_bytes).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "publication name exceeds Win32 byte limit",
            )
        })?;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            std::ptr::addr_of_mut!((*rename).FileName).cast::<u16>(),
            name.len(),
        );
        checked(SetFileInformationByHandle(
            file.as_raw_handle(),
            FileRenameInfo,
            rename.cast(),
            buffer_length,
        ))
    }
}
pub(crate) fn crash_test_flags() -> (u32, Result<u32, i32>) {
    use windows_sys::Win32::System::Diagnostics::Debug::GetErrorMode;
    use windows_sys::Win32::System::ErrorReporting::WerGetFlags;
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: GetErrorMode has no buffers/handles and reads only current process
    // policy; the pseudo handle returned here is borrowed and must not be closed.
    let mode = unsafe { GetErrorMode() };
    let current = unsafe { GetCurrentProcess() };
    let mut flags = 0;
    // SAFETY: current-process pseudo handle and exact writable flag output.
    let status = unsafe { WerGetFlags(current, &mut flags) };
    (mode, if status < 0 { Err(status) } else { Ok(flags) })
}
pub(crate) fn configure_crash_test_process() -> io::Result<()> {
    use windows_sys::Win32::System::Diagnostics::Debug::{
        GetErrorMode, SEM_NOGPFAULTERRORBOX, SetErrorMode,
    };
    use windows_sys::Win32::System::ErrorReporting::{WER_FAULT_REPORTING_NO_UI, WerSetFlags};
    // SAFETY: these calls only read/write the current process error policy and
    // have no caller buffers. Preserve inherited error-mode bits and add only
    // opt-out of WER invocation, before this private helper induces a crash.
    let mode = unsafe { GetErrorMode() };
    unsafe {
        SetErrorMode(mode | SEM_NOGPFAULTERRORBOX);
    }
    // SAFETY: sets documented process-local WER flags for this dedicated crash
    // helper. It does not edit registry/user policy or configure the Host.
    let status = unsafe { WerSetFlags(WER_FAULT_REPORTING_NO_UI) };
    if status < 0 {
        return Err(io::Error::other(format!(
            "WerSetFlags HRESULT 0x{:08x}",
            status as u32
        )));
    }
    let (actual_mode, flags) = crash_test_flags();
    let flags = flags.map_err(|status| {
        io::Error::other(format!(
            "Configured WerGetFlags HRESULT 0x{:08x}",
            status as u32
        ))
    })?;
    if actual_mode & SEM_NOGPFAULTERRORBOX == 0 || flags & WER_FAULT_REPORTING_NO_UI == 0 {
        return Err(io::Error::other(
            "current process crash test policy did not persist",
        ));
    }
    Ok(())
}
pub(crate) fn cancel_io_thread<T>(thread: &std::thread::JoinHandle<T>) -> io::Result<()> {
    // SAFETY: borrowing the live JoinHandle retains its exact thread identity
    // and required native rights while this synchronous call completes.
    let result = unsafe { CancelSynchronousIo(thread.as_raw_handle()) };
    if result != 0 {
        return Ok(());
    }
    let error = os_error();
    if error.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_NOT_FOUND as i32) {
        Ok(())
    } else {
        Err(error)
    }
}
pub(crate) fn io_thread_exited<T>(thread: &std::thread::JoinHandle<T>) -> io::Result<bool> {
    // SAFETY: borrowing the retained native thread handle and observing with a
    // zero timeout. Signaled confirms native TLS teardown, not just Rust's task.
    match unsafe { WaitForSingleObject(thread.as_raw_handle(), 0) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        _ => Err(os_error()),
    }
}
pub(crate) struct Observer {
    process: OwnedHandle,
    creation_time: u64,
}
impl Observer {
    pub(crate) fn open(pid: u32) -> io::Result<Self> {
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        };
        // SAFETY: this opens a new noninheritable wait/query handle for the PID.
        // The unique handle, rather than the numeric PID, is retained thereafter.
        let process = own(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        })?;
        let (mut creation, mut exit, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        // SAFETY: retained process handle and exact writable FILETIME buffers.
        checked(unsafe {
            GetProcessTimes(
                native(&process),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        })?;
        Ok(Self {
            process,
            creation_time: ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64,
        })
    }
    pub(crate) fn creation_time(&self) -> u64 {
        self.creation_time
    }
    pub(crate) fn wait_exit(&self, timeout: std::time::Duration) -> io::Result<Option<u32>> {
        let milliseconds =
            timeout.as_millis() + u128::from(timeout.subsec_nanos() % 1_000_000 != 0);
        if milliseconds >= u32::MAX as u128 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "observer wait exceeds finite Windows timeout",
            ));
        }
        // SAFETY: retained process identity and finite native timeout. Querying
        // exit code only after signaled avoids confusing STILL_ACTIVE with exit.
        match unsafe { WaitForSingleObject(native(&self.process), milliseconds as u32) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                // SAFETY: signaled retained process and exact writable code.
                checked(unsafe { GetExitCodeProcess(native(&self.process), &mut code) })?;
                Ok(Some(code))
            }
            _ => Err(os_error()),
        }
    }
}
