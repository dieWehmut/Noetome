//! Explicit desktop operations over the existing Core and trusted CLI resources.
use crate::project_entry::{self, ProjectSummary};
use process_wrap::std::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const ENDPOINT: &str = "https://ai.1343263.xyz/v1/chat/completions";
const MODEL: &str = "deepseek-flash";
static NEXT_JOB: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failure {
    pub code: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub record: Value,
    pub failure: Option<Failure>,
    pub project: Option<ProjectSummary>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ExtractResult {
    pub record: Value,
    pub failure: Option<Failure>,
    pub output_root: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct SelfTestResult {
    pub record: Value,
    pub failure: Option<Failure>,
    pub output_root: String,
    pub vault_path: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelStatus {
    pub ready: bool,
    pub endpoint: String,
    pub model: String,
    pub credential_state: String,
    pub ledger_state: String,
    pub accounted_tokens: Option<u64>,
    pub pending_reserved_tokens: Option<u64>,
    pub remaining_tokens: Option<u64>,
    pub model_attempts: Option<u64>,
    pub message: String,
}
impl ModelStatus {
    fn unavailable(message: String) -> Self {
        Self {
            ready: false,
            endpoint: ENDPOINT.into(),
            model: MODEL.into(),
            credential_state: "unavailable".into(),
            ledger_state: "unavailable".into(),
            accounted_tokens: None,
            pending_reserved_tokens: None,
            remaining_tokens: None,
            model_attempts: None,
            message,
        }
    }
}
#[derive(Clone)]
pub struct WorkbenchState {
    inner: Arc<Inner>,
}
struct Inner {
    data: Result<PathBuf, String>,
    operation: Mutex<Option<Arc<AtomicBool>>>,
}
struct Operation {
    inner: Arc<Inner>,
    cancel: Arc<AtomicBool>,
}
impl Drop for Operation {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.inner.operation.lock() {
            if slot
                .as_ref()
                .is_some_and(|flag| Arc::ptr_eq(flag, &self.cancel))
            {
                *slot = None;
            }
        }
    }
}
impl WorkbenchState {
    pub fn new(data: Result<PathBuf, String>) -> Self {
        Self {
            inner: Arc::new(Inner {
                data,
                operation: Mutex::new(None),
            }),
        }
    }
    fn begin(&self) -> Result<Operation, String> {
        let mut slot = self
            .inner
            .operation
            .lock()
            .map_err(|_| "DESKTOP_OPERATION_UNAVAILABLE：操作锁不可用。".to_owned())?;
        if slot.is_some() {
            return Err("DESKTOP_OPERATION_BUSY：已有操作正在运行，请等待或取消。".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *slot = Some(cancel.clone());
        Ok(Operation {
            inner: self.inner.clone(),
            cancel,
        })
    }
    pub fn cancel(&self) -> bool {
        if let Ok(slot) = self.inner.operation.lock() {
            if let Some(flag) = slot.as_ref() {
                flag.store(true, Ordering::SeqCst);
                return true;
            }
        }
        false
    }
    fn new_job(&self) -> Result<PathBuf, String> {
        let parent = self
            .inner
            .data
            .as_ref()
            .map_err(Clone::clone)?
            .join("workbench-jobs");
        noetome_cli::extraction_preview::ensure_outside_vault(&parent)
            .map_err(|error| error.to_string())?;
        ensure_plain_ancestors(&parent)?;
        fs::create_dir_all(&parent).map_err(io_error)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io_error)?
            .as_nanos();
        let root = parent.join(format!(
            "job-{}-{nonce}-{}",
            std::process::id(),
            NEXT_JOB.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&root).map_err(io_error)?;
        Ok(root)
    }
    pub fn create_project(&self, parent: &Path, name: &str) -> Result<ProjectSummary, String> {
        let _operation = self.begin()?;
        project_entry::create_in_parent(parent, name)
    }
    pub fn import(&self, project: &Path, source: &Path) -> Result<ImportResult, String> {
        let operation = self.begin()?;
        noetome_cli::extraction_preview::ensure_outside_vault(project)
            .map_err(|error| error.to_string())?;
        let output = run_cli(
            &[
                "source".into(),
                "import".into(),
                project.as_os_str().into(),
                source.as_os_str().into(),
            ],
            &operation.cancel,
            None,
        )?;
        let (record, failure) = command_record(output);
        Ok(ImportResult {
            record,
            failure,
            project: project_entry::open(project).ok(),
        })
    }
    pub fn extract(
        &self,
        project: &Path,
        import_id: &str,
        selection: Value,
        confirmed_units: Value,
    ) -> Result<ExtractResult, String> {
        let operation = self.begin()?;
        // SourceSession revalidates real units before reading credentials or reserving quota.
        let source =
            noetome_cli::extraction_preview::hold_selected_units(project, import_id, &selection)
                .map_err(|error| error.to_string())?;
        if source.value() != confirmed_units {
            return Err("DESKTOP_SOURCE_CHANGED：当前来源与已确认原文不同；未读取凭据、预留预算或发送请求，请重新查看来源。".into());
        }
        let status = self.model_status()?;
        if !status.ready {
            return Err(format!("DESKTOP_MODEL_NOT_READY：{}", status.message));
        }
        if operation.cancel.load(Ordering::SeqCst) {
            return Err("DESKTOP_CANCELLED：已取消，未发起请求。".into());
        }
        let job = self.new_job()?;
        let output_root = job.join("api-output");
        let config = job.join("text-api-config.json");
        let signal = job.join("cancel.signal");
        let cap = status.remaining_tokens.unwrap_or(0).min(524288);
        let value = json!({"schema_version":1,"provider":"openai_chat_completions","transport_mode":"live","endpoint":ENDPOINT,"model":MODEL,"credential_env":"NOETOME_TANGMAO_API_KEY","required_input":"text","capabilities":{"text":true,"structured_output":true,"vision":false},"selection":selection,"limits":{"max_units":64,"max_input_bytes":65536,"max_response_bytes":1048576,"max_output_tokens":8192,"timeout_ms":60000},"budget":{"max_requests":1,"max_total_tokens":cap,"max_cost_usd":null}});
        write_new_json(&config, &value)?;
        let (credential, ledger) = profile_paths()?;
        let args = vec![
            "-NoProfile".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-File".into(),
            resource(
                "helpers/invoke_text_api_credential.ps1",
                "scripts/invoke_text_api_credential.ps1",
                false,
            )?
            .into_os_string(),
            "-Binary".into(),
            cli_binary()?.into_os_string(),
            "-Project".into(),
            project.as_os_str().into(),
            "-ImportId".into(),
            import_id.into(),
            "-Config".into(),
            config.into_os_string(),
            "-OutputRoot".into(),
            output_root.as_os_str().into(),
            "-CredentialFile".into(),
            credential.into_os_string(),
            "-UsageLedgerFile".into(),
            ledger.into_os_string(),
            "-CancelFile".into(),
            signal.as_os_str().into(),
        ];
        let output = run_bounded(
            &powershell()?,
            &args,
            &operation.cancel,
            Some(&signal),
            75000,
        );
        let (record, mut failure) = match output {
            Ok(output) => command_record(output),
            Err(message) => (
                Value::Null,
                Some(Failure {
                    code: "DESKTOP_PROCESS_FAILED".into(),
                    message,
                }),
            ),
        };
        if let Err(error) = source.verify() {
            failure = Some(Failure {
                code: "DESKTOP_SOURCE_CHANGED".into(),
                message: format!("调用结束时来源重验失败；已有请求及用量原件保留：{error}"),
            });
        }
        Ok(ExtractResult {
            record,
            failure,
            output_root: output_root.display().to_string(),
        })
    }
    pub fn export(
        &self,
        project: &Path,
        import_id: &str,
        result: &Path,
        parent: &Path,
        name: &str,
    ) -> Result<Value, String> {
        let _operation = self.begin()?;
        project_entry::validate_name(name)?;
        if !parent.is_dir() {
            return Err("PREVIEW_PARENT_MISSING：请选择已存在的导出父目录。".into());
        }
        noetome_cli::extraction_preview::export(project, import_id, result, &parent.join(name))
            .map_err(|error| error.to_string())
    }
    pub fn model_status(&self) -> Result<ModelStatus, String> {
        #[cfg(not(windows))]
        {
            return Ok(ModelStatus::unavailable(
                "当前系统没有Windows CurrentUser DPAPI；未发送模型请求。".into(),
            ));
        }
        #[cfg(windows)]
        {
            let cancel = Arc::new(AtomicBool::new(false));
            self.profile_operation("Status", false, &cancel)
        }
    }
    pub fn store_credential(&self) -> Result<ModelStatus, String> {
        let operation = self.begin()?;
        self.profile_operation("Store", false, &operation.cancel)
    }
    pub fn restore_ledger(&self, confirmed: bool) -> Result<ModelStatus, String> {
        if !confirmed {
            return Err("TEXT_LEDGER_CONFIRMATION_REQUIRED：请明确确认恢复最后有效的33185 Token、十条记录；不能初始化零用量。".into());
        }
        let operation = self.begin()?;
        self.profile_operation("RestoreLedger", true, &operation.cancel)
    }
    fn profile_operation(
        &self,
        mode: &str,
        confirmed: bool,
        cancel: &Arc<AtomicBool>,
    ) -> Result<ModelStatus, String> {
        let (credential, ledger) = profile_paths()?;
        let mut args = vec![
            "-NoProfile".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-STA".into(),
            "-File".into(),
            resource(
                "helpers/text_model_profile.ps1",
                "scripts/text_model_profile.ps1",
                false,
            )?
            .into_os_string(),
            "-Mode".into(),
            mode.into(),
            "-CredentialFile".into(),
            credential.into_os_string(),
            "-UsageLedgerFile".into(),
            ledger.into_os_string(),
        ];
        if mode == "Store" {
            args.extend([
                "-CredentialSetter".into(),
                resource(
                    "helpers/set_text_api_credential.ps1",
                    "scripts/set_text_api_credential.ps1",
                    false,
                )?
                .into_os_string(),
            ]);
        }
        if confirmed {
            args.extend(["-Confirmed".into(), "-RecoveryLedger".into(), resource("fixtures/model-ledger/actual-final-ledger.json", "doc/validation/assets/B06V-T02/recognition/vision-live-final/actual-final-ledger.json", false)?.into_os_string(), "-RecoveryProof".into(), resource("fixtures/model-ledger/continuation-current-ledger.json", "doc/validation/assets/B06V-T02/review/continuation-current-ledger.json", false)?.into_os_string()]);
        }
        let output = run_bounded(
            &powershell()?,
            &args,
            cancel,
            None,
            if mode == "Store" { 120000 } else { 15000 },
        )?;
        if let Some(failure) = output.failure {
            return Err(format!("{}：{}", failure.code, failure.message));
        }
        if output.code != Some(0) {
            return Err(
                "DESKTOP_MODEL_PROFILE_REFUSED：凭据或账本操作被拒绝，既有文件保持。".into(),
            );
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|_| "DESKTOP_MODEL_PROFILE_INVALID：状态报告无效。".into())
    }
    pub fn self_test(&self) -> Result<SelfTestResult, String> {
        let operation = self.begin()?;
        let job = self.new_job()?;
        let output_root = job.join("offline-run");
        let fixture = resource(
            "fixtures/run-checkpoint-v1/fixture",
            "fixtures/run-checkpoint-v1/fixture",
            true,
        )?;
        let output = run_cli(
            &[
                "run".into(),
                "start".into(),
                fixture.into_os_string(),
                output_root.as_os_str().into(),
            ],
            &operation.cancel,
            None,
        )?;
        let (mut record, failure) = command_record(output);
        if let Some(object) = record.as_object_mut() {
            object.insert(
                "desktop_self_test".into(),
                json!({"kind":"fixture_authored_offline","model_calls":0,"network_requests":0}),
            );
        }
        let vault = output_root.join("output/vault");
        let vault_path = if failure.is_none() && vault.is_dir() {
            Some(vault.display().to_string())
        } else {
            None
        };
        Ok(SelfTestResult {
            record,
            failure,
            output_root: output_root.display().to_string(),
            vault_path,
        })
    }
}
fn io_error(error: impl std::fmt::Display) -> String {
    format!("DESKTOP_IO：{error}")
}
fn ensure_plain_ancestors(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if is_link(&metadata) => {
                return Err("DESKTOP_UNSAFE_PATH：路径不能经过链接。".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(error)),
        }
    }
    Ok(())
}
fn is_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}
fn resource(installed: &str, development: &str, directory: bool) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(io_error)?;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "DESKTOP_RESOURCE_UNAVAILABLE：开发路径不可用。".to_owned())?;
    resource_from(
        &exe,
        repo,
        cfg!(debug_assertions),
        installed,
        development,
        directory,
    )
}
fn resource_from(
    exe: &Path,
    repo: &Path,
    allow_development: bool,
    installed: &str,
    development: &str,
    directory: bool,
) -> Result<PathBuf, String> {
    let mut candidates = vec![
        exe.parent()
            .ok_or_else(|| "DESKTOP_RESOURCE_UNAVAILABLE：可执行文件目录不可用。".to_owned())?
            .join(installed),
    ];
    if allow_development && repo.join("Cargo.toml").is_file() && repo.join("AGENTS.md").is_file() {
        candidates.push(repo.join(development));
    }
    for candidate in candidates {
        ensure_plain_ancestors(&candidate)?;
        if fs::symlink_metadata(&candidate)
            .is_ok_and(|m| if directory { m.is_dir() } else { m.is_file() })
        {
            return Ok(candidate);
        }
    }
    Err(format!(
        "DESKTOP_RESOURCE_UNAVAILABLE：缺少安装或已确认开发资源：{installed}"
    ))
}
fn cli_binary() -> Result<PathBuf, String> {
    let filename = if cfg!(windows) {
        "noetome.exe"
    } else {
        "noetome"
    };
    resource(filename, &format!("target/debug/{filename}"), false)
}
fn profile_paths() -> Result<(PathBuf, PathBuf), String> {
    if let Some(root) = crate::desktop_data::model_root_override()? {
        noetome_cli::extraction_preview::ensure_outside_vault(&root)
            .map_err(|error| error.to_string())?;
        return Ok((
            root.join("credentials/tangmao-deepseek.dpapi"),
            root.join("model-services/tangmao-deepseek.usage.json"),
        ));
    }
    let root = std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "DESKTOP_MODEL_PROFILE_UNAVAILABLE：LOCALAPPDATA不可用。".to_owned())?
        .join("Noetome");
    if !root.is_absolute() {
        return Err("DESKTOP_MODEL_PROFILE_UNAVAILABLE：LOCALAPPDATA须为绝对路径。".into());
    }
    noetome_cli::extraction_preview::ensure_outside_vault(&root)
        .map_err(|error| error.to_string())?;
    Ok((
        root.join("credentials/tangmao-deepseek.dpapi"),
        root.join("model-services/tangmao-deepseek.usage.json"),
    ))
}
fn powershell() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        Ok(PathBuf::from(
            std::env::var_os("SystemRoot")
                .ok_or_else(|| "DESKTOP_POWERSHELL_UNAVAILABLE：SystemRoot不可用。".to_owned())?,
        )
        .join("System32/WindowsPowerShell/v1.0/powershell.exe"))
    }
    #[cfg(not(windows))]
    {
        Err("DESKTOP_MODEL_UNAVAILABLE：凭据调用需要Windows CurrentUser DPAPI。".into())
    }
}
fn write_new_json(path: &Path, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(io_error)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(io_error)
}
struct ChildOutput {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    failure: Option<Failure>,
}
fn command_record(output: ChildOutput) -> (Value, Option<Failure>) {
    let record = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    let failure = output
        .failure
        .or_else(|| {
            if output.code == Some(0) {
                None
            } else {
                let text = String::from_utf8_lossy(&output.stderr);
                let line = text
                    .lines()
                    .next()
                    .unwrap_or("操作失败，请查看已保存的报告。");
                let (code, message) = line
                    .split_once(['：', ':'])
                    .unwrap_or(("DESKTOP_COMMAND_FAILED", line));
                Some(Failure {
                    code: code.chars().take(80).collect(),
                    message: message.chars().take(2048).collect(),
                })
            }
        })
        .or_else(|| {
            (!record.is_object()).then(|| Failure {
                code: "DESKTOP_REPORT_INVALID".into(),
                message: "操作未返回有效的结构化报告。".into(),
            })
        });
    (record, failure)
}
fn run_cli(
    args: &[OsString],
    cancel: &Arc<AtomicBool>,
    signal: Option<&Path>,
) -> Result<ChildOutput, String> {
    run_bounded(&cli_binary()?, args, cancel, signal, 75000)
}
fn pipe(
    mut input: impl Read + Send + 'static,
    maximum: usize,
    overflow: Arc<AtomicBool>,
    send: mpsc::Sender<Result<Vec<u8>, String>>,
) {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buffer = [0; 8192];
        let result = loop {
            match input.read(&mut buffer) {
                Ok(0) => break Ok(bytes),
                Ok(count) => {
                    if bytes.len() + count > maximum {
                        overflow.store(true, Ordering::SeqCst);
                        break Err("DESKTOP_OUTPUT_LIMIT：子进程输出超出上限。".into());
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                }
                Err(error) => break Err(io_error(error)),
            }
        };
        let _ = send.send(result);
    });
}
#[cfg(windows)]
#[derive(Debug)]
struct JobObserver {
    kill: Arc<win32job::Job>,
    census: Arc<win32job::Job>,
}
#[cfg(windows)]
impl CommandWrapper for JobObserver {
    fn wrap_child(
        &mut self,
        mut child: Box<dyn ChildWrapper>,
        _: &CommandWrap,
    ) -> std::io::Result<Box<dyn ChildWrapper>> {
        use std::os::windows::io::AsRawHandle;
        let handle = child
            .process_handle()
            .ok_or_else(|| std::io::Error::other("no process handle"))?;
        for job in [&self.kill, &self.census] {
            if let Err(error) = job.assign_process(handle.as_raw_handle() as isize) {
                let _ = child.start_kill();
                return Err(std::io::Error::other(error.to_string()));
            }
        }
        Ok(child)
    }
}
fn run_bounded(
    program: &Path,
    args: &[OsString],
    cancel: &Arc<AtomicBool>,
    signal: Option<&Path>,
    timeout_ms: u64,
) -> Result<ChildOutput, String> {
    if cancel.load(Ordering::SeqCst) {
        return Err("DESKTOP_CANCELLED：已取消，未启动子进程。".into());
    }
    let mut wrap = CommandWrap::with_new(program, |command| {
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    });
    #[cfg(windows)]
    {
        let mut limits = win32job::ExtendedLimitInfo::new();
        limits.limit_kill_on_job_close();
        let kill = Arc::new(win32job::Job::create_with_limit_info(&limits).map_err(io_error)?);
        let census = Arc::new(win32job::Job::create().map_err(io_error)?);
        let mut flags = CreationFlags(Default::default());
        flags.0.0 = 0x08000000;
        wrap.wrap(flags)
            .wrap(JobObserver { kill, census })
            .wrap(JobObject);
    }
    let mut child = wrap.spawn().map_err(io_error)?;
    let overflow = Arc::new(AtomicBool::new(false));
    let (outsend, outrecv) = mpsc::channel();
    let (errsend, errrecv) = mpsc::channel();
    pipe(
        child
            .stdout()
            .take()
            .ok_or_else(|| "DESKTOP_PIPE：stdout不可用。".to_owned())?,
        16777216,
        overflow.clone(),
        outsend,
    );
    pipe(
        child
            .stderr()
            .take()
            .ok_or_else(|| "DESKTOP_PIPE：stderr不可用。".to_owned())?,
        1048576,
        overflow.clone(),
        errsend,
    );
    let started = Instant::now();
    let mut cancelling = None;
    let mut failure = None;
    let status = loop {
        if let Some(status) = child.inner_mut().try_wait().map_err(io_error)? {
            break Some(status);
        }
        if cancel.load(Ordering::SeqCst) {
            let cancelled_at = *cancelling.get_or_insert_with(|| {
                if let Some(path) = signal {
                    let _ = OpenOptions::new().write(true).create_new(true).open(path);
                }
                Instant::now()
            });
            if signal.is_none() || cancelled_at.elapsed() > Duration::from_secs(2) {
                failure = Some(Failure {
                    code: "DESKTOP_CANCELLED".into(),
                    message: "已取消；可能发出的请求保留未知用量预留。".into(),
                });
                break None;
            }
        }
        if started.elapsed().as_millis() > u128::from(timeout_ms) || overflow.load(Ordering::SeqCst)
        {
            failure = Some(Failure {
                code: if overflow.load(Ordering::SeqCst) {
                    "DESKTOP_OUTPUT_LIMIT"
                } else {
                    "DESKTOP_TIMEOUT"
                }
                .into(),
                message: "操作超出有界等待或输出上限，子进程已终止。".into(),
            });
            break None;
        }
        thread::sleep(Duration::from_millis(10));
    };
    // JobObject started its root suspended and assigned before resuming. Terminate
    // all descendants even after a normal root exit; dropping closes its kill job.
    if status.is_none() || cfg!(windows) {
        child.start_kill().map_err(io_error)?;
    }
    drop(child);
    drop(wrap);
    let stdout = outrecv
        .recv_timeout(Duration::from_secs(3))
        .map_err(io_error)??;
    let stderr = errrecv
        .recv_timeout(Duration::from_secs(3))
        .map_err(io_error)??;
    Ok(ChildOutput {
        code: status.and_then(|status| status.code()),
        stdout,
        stderr,
        failure,
    })
}

#[cfg(windows)]
pub fn source_pick() -> Result<Option<String>, String> {
    Ok(rfd::FileDialog::new()
        .set_title("选择一份资料")
        .add_filter("资料", &["md", "txt", "pdf"])
        .pick_file()
        .map(|path| path.display().to_string()))
}
#[cfg(not(windows))]
pub fn source_pick() -> Result<Option<String>, String> {
    Err("DESKTOP_SOURCE_DIALOG_UNAVAILABLE：当前系统没有原生文件选择器。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestRoot(PathBuf);
    impl TestRoot {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "noetome-workbench-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_JOB.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir(&root).unwrap();
            Self(root.canonicalize().unwrap())
        }
        fn state(&self) -> WorkbenchState {
            WorkbenchState::new(Ok(self.0.join("new app data")))
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn simultaneous_operations_are_rejected_and_cancel_is_bound_to_its_operation() {
        let root = TestRoot::new();
        let state = root.state();
        assert!(!state.cancel());
        let first = state.begin().unwrap();
        let other = state.clone();
        assert!(
            thread::spawn(move || other.begin().err().unwrap())
                .join()
                .unwrap()
                .starts_with("DESKTOP_OPERATION_BUSY")
        );
        assert!(state.cancel());
        assert!(first.cancel.load(Ordering::SeqCst));
        drop(first);
        assert!(!state.cancel());
        let second = state.begin().unwrap();
        assert!(!second.cancel.load(Ordering::SeqCst));
        assert!(!root.0.join("new app data").exists());
    }
    #[test]
    fn changed_confirmed_units_stop_before_profile_or_job_creation() {
        let root = TestRoot::new();
        let project = root.0.join("资料 project");
        project_entry::create(&project).unwrap();
        let original = root.0.join("所选 text.txt");
        fs::write(&original, "Only selected original words.\n").unwrap();
        let imported = noetome_cli::source::import(&project, &original, None).unwrap();
        assert!(imported.failure.is_none());
        let imported: Value = serde_json::from_str(&imported.json).unwrap();
        let selection = json!([{"kind":"text","ordinal":0}]);
        let actual = noetome_cli::extraction_preview::selected_units(
            &project,
            imported["import_id"].as_str().unwrap(),
            &selection,
        )
        .unwrap();
        let db = fs::read(project.join("project.sqlite")).unwrap();
        let mut changed = actual;
        changed["units"][0]["text"] = json!("Words the user never confirmed.");
        let result = root.state().extract(
            &project,
            imported["import_id"].as_str().unwrap(),
            selection,
            changed,
        );
        assert!(result.err().unwrap().starts_with("DESKTOP_SOURCE_CHANGED"));
        assert_eq!(db, fs::read(project.join("project.sqlite")).unwrap());
        assert!(!root.0.join("new app data").exists());
    }
    #[test]
    fn invalid_selection_stops_before_model_profile_and_jobs() {
        let root = TestRoot::new();
        let project = root.0.join("project");
        project_entry::create(&project).unwrap();
        let result = root
            .state()
            .extract(&project, "not-a-real-import-id", json!([]), json!({}));
        assert!(result.is_err());
        assert!(!root.0.join("new app data").exists());
    }
    #[test]
    fn restoring_without_explicit_confirmation_does_not_touch_disk() {
        let root = TestRoot::new();
        assert!(
            root.state()
                .restore_ledger(false)
                .err()
                .unwrap()
                .starts_with("TEXT_LEDGER_CONFIRMATION_REQUIRED")
        );
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
    }
    #[test]
    fn project_parent_and_single_name_preserve_chinese_spaces_and_refuse_existing() {
        let root = TestRoot::new();
        let state = root.state();
        let created = state
            .create_project(&root.0, "我的资料 with spaces")
            .unwrap();
        assert_eq!(
            created,
            project_entry::open(&root.0.join("我的资料 with spaces")).unwrap()
        );
        let before = fs::read(root.0.join("我的资料 with spaces/project.json")).unwrap();
        assert!(
            state
                .create_project(&root.0, "我的资料 with spaces")
                .is_err()
        );
        assert_eq!(
            before,
            fs::read(root.0.join("我的资料 with spaces/project.json")).unwrap()
        );
        for name in [
            "../other",
            "a/b",
            "a\\b",
            "CON.txt",
            "LPT9",
            "ends.",
            "ends ",
            "invalid?name",
            "invalid*name",
            "line\nfeed",
        ] {
            assert!(state.create_project(&root.0, name).is_err(), "{name}");
        }
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
    }
    #[test]
    fn project_creation_and_jobs_refuse_a_selected_vault_before_writing() {
        let root = TestRoot::new();
        fs::create_dir(root.0.join(".obsidian")).unwrap();
        assert!(root.state().create_project(&root.0, "project").is_err());
        assert!(root.state().new_job().is_err());
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), 1);
    }
    #[test]
    fn release_missing_resource_never_uses_existing_compile_repository() {
        let root = TestRoot::new();
        let repo = root.0.join("repo");
        let install = root.0.join("install");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&install).unwrap();
        fs::write(repo.join("Cargo.toml"), "public owned test").unwrap();
        fs::write(repo.join("AGENTS.md"), "public owned test").unwrap();
        fs::write(repo.join("helper.ps1"), "public owned helper").unwrap();
        let exe = install.join("desktop.exe");
        assert!(resource_from(&exe, &repo, false, "helper.ps1", "helper.ps1", false).is_err());
        assert_eq!(
            resource_from(&exe, &repo, true, "helper.ps1", "helper.ps1", false).unwrap(),
            repo.join("helper.ps1")
        );
        fs::write(install.join("helper.ps1"), "installed helper").unwrap();
        assert_eq!(
            resource_from(&exe, &repo, false, "helper.ps1", "helper.ps1", false).unwrap(),
            install.join("helper.ps1")
        );
        fs::rename(install.join("helper.ps1"), install.join("renamed.ps1")).unwrap();
        assert!(resource_from(&exe, &repo, false, "helper.ps1", "helper.ps1", false).is_err());
    }
    #[test]
    fn successful_exit_without_an_object_report_is_a_failure_and_failed_report_is_retained() {
        for stdout in [b"null".as_slice(), b"[]", b"not-json"] {
            let (_, failure) = command_record(ChildOutput {
                code: Some(0),
                stdout: stdout.to_vec(),
                stderr: vec![],
                failure: None,
            });
            assert_eq!(failure.unwrap().code, "DESKTOP_REPORT_INVALID");
        }
        let (record, failure) = command_record(ChildOutput {
            code: Some(2),
            stdout: br#"{"status":"failed","usage":{"network_attempts":1}}"#.to_vec(),
            stderr: "API_FAILED：saved response\n".as_bytes().to_vec(),
            failure: None,
        });
        assert_eq!(record["usage"]["network_attempts"], 1);
        assert_eq!(failure.unwrap().code, "API_FAILED");
    }
    #[test]
    fn existing_job_json_is_never_overwritten() {
        let root = TestRoot::new();
        let file = root.0.join("config.json");
        write_new_json(&file, &json!({"first":true})).unwrap();
        let bytes = fs::read(&file).unwrap();
        assert!(write_new_json(&file, &json!({"replacement":true})).is_err());
        assert_eq!(bytes, fs::read(&file).unwrap());
    }
    #[test]
    fn offline_fixture_runs_real_cli_in_a_new_owned_root() {
        let root = TestRoot::new();
        let result = root.state().self_test().unwrap();
        assert!(result.failure.is_none(), "{:?}", result.failure);
        assert_eq!(
            result.record["desktop_self_test"],
            json!({"kind":"fixture_authored_offline","model_calls":0,"network_requests":0})
        );
        let vault = PathBuf::from(result.vault_path.unwrap());
        assert!(vault.is_dir());
        assert!(vault.starts_with(root.0.join("new app data/workbench-jobs")));
        assert_eq!(
            fs::read(vault.join("笔记/甲容器.md")).unwrap(),
            fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../fixtures/run-checkpoint-v1/expected-markdown/笔记/甲容器.md")
            )
            .unwrap()
        );
    }
    #[cfg(windows)]
    #[test]
    fn native_descendant_is_killed_after_the_root_exits_with_inherited_pipes() {
        let root = TestRoot::new();
        let marker = root.0.join("descendant.pid");
        let child_script = root.0.join("child.ps1");
        fs::write(
            &child_script,
            format!(
                "[IO.File]::WriteAllText('{}',[string]$PID); Start-Sleep -Seconds 180",
                marker.display().to_string().replace('\'', "''")
            ),
        )
        .unwrap();
        let powershell = powershell().unwrap();
        let launcher = format!(
            "$i=New-Object Diagnostics.ProcessStartInfo;$i.FileName='{}';$i.Arguments='-NoProfile -ExecutionPolicy Bypass -File \"{}\"';$i.UseShellExecute=$false;$p=[Diagnostics.Process]::Start($i);while(-not [IO.File]::Exists('{}')){{Start-Sleep -Milliseconds 10}};exit 0",
            powershell.display().to_string().replace('\'', "''"),
            child_script.display().to_string().replace('\'', "''"),
            marker.display().to_string().replace('\'', "''")
        );
        let flag = Arc::new(AtomicBool::new(false));
        let started = Instant::now();
        let output = run_bounded(
            &powershell,
            &["-NoProfile".into(), "-Command".into(), launcher.into()],
            &flag,
            None,
            15000,
        )
        .unwrap();
        assert_eq!(output.code, Some(0));
        assert!(started.elapsed() < Duration::from_secs(10));
        let pid = fs::read_to_string(&marker).unwrap();
        let probe = std::process::Command::new(&powershell).args(["-NoProfile", "-Command", &format!("try{{$p=[Diagnostics.Process]::GetProcessById({pid});if($p.WaitForExit(5000)){{exit 0}}else{{exit 1}}}}catch [ArgumentException]{{exit 0}}")]).status().unwrap();
        assert!(
            probe.success(),
            "the acknowledged native descendant must have exited"
        );
    }
    #[test]
    fn prestart_cancellation_does_not_spawn_even_a_missing_binary() {
        let flag = Arc::new(AtomicBool::new(true));
        let result = run_bounded(Path::new("missing-noetome-child"), &[], &flag, None, 1000);
        assert!(result.err().unwrap().starts_with("DESKTOP_CANCELLED"));
    }
    #[test]
    fn cancellation_interrupts_an_acknowledged_real_child_within_the_bound() {
        let root = TestRoot::new();
        let marker = root.0.join("child-started");
        #[cfg(windows)]
        let (program, args) = (
            powershell().unwrap(),
            vec![
                "-NoProfile".into(),
                "-Command".into(),
                format!(
                    "[IO.File]::WriteAllText('{}',[string]$PID);Start-Sleep -Seconds 180",
                    marker.display().to_string().replace('\'', "''")
                )
                .into(),
            ],
        );
        #[cfg(not(windows))]
        let (program, args) = (
            PathBuf::from("/bin/sh"),
            vec![
                "-c".into(),
                "printf ready > \"$1\"; exec /bin/sleep 180".into(),
                "noetome-test".into(),
                marker.as_os_str().into(),
            ],
        );
        let flag = Arc::new(AtomicBool::new(false));
        let cancel_flag = flag.clone();
        let observed = marker.clone();
        let canceller = thread::spawn(move || {
            let deadline = Instant::now();
            while !observed.exists() && deadline.elapsed() < Duration::from_secs(10) {
                thread::sleep(Duration::from_millis(10));
            }
            assert!(
                observed.is_file(),
                "real child must acknowledge before cancellation"
            );
            cancel_flag.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let output = run_bounded(&program, &args, &flag, None, 20000).unwrap();
        canceller.join().unwrap();
        assert_eq!(output.failure.unwrap().code, "DESKTOP_CANCELLED");
        assert!(started.elapsed() < Duration::from_secs(10));
        #[cfg(windows)]
        {
            let pid = fs::read_to_string(&marker).unwrap();
            let probe = std::process::Command::new(&program).args(["-NoProfile", "-Command", &format!("try{{$p=[Diagnostics.Process]::GetProcessById({pid});if($p.WaitForExit(5000)){{exit 0}}else{{exit 1}}}}catch [ArgumentException]{{exit 0}}")]).status().unwrap();
            assert!(probe.success(), "acknowledged native root must have exited");
        }
    }
}
