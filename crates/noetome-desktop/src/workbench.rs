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
                crate::external_path::argument(project)?,
                crate::external_path::argument(source)?,
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
            crate::external_path::argument(project)?,
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
    pub fn compile_create(
        &self,
        project: &Path,
        source_ids: &[String],
    ) -> Result<noetome_cli::production_compile::RunSummary, String> {
        self.compile_create_scoped(project, source_ids, None)
    }
    pub fn compile_create_scoped(
        &self,
        project: &Path,
        source_ids: &[String],
        scope: Option<noetome_cli::production_compile::CompileScope>,
    ) -> Result<noetome_cli::production_compile::RunSummary, String> {
        let (_, ledger) = profile_paths()?;
        noetome_cli::production_compile::create_scoped(
            project,
            source_ids,
            compile_config(ledger),
            scope,
        )
        .map_err(|error| error.to_string())
    }
    pub fn compile_run(
        &self,
        project: &Path,
        run_id: &str,
    ) -> Result<noetome_cli::production_compile::RunSummary, String> {
        #[cfg(not(windows))]
        {
            let _ = (project, run_id);
            Err("DESKTOP_MODEL_UNAVAILABLE：生产模型调用需要Windows CurrentUser DPAPI。".into())
        }
        #[cfg(windows)]
        {
            // Validate the persisted run before starting the narrow credential bridge.
            noetome_cli::production_compile::status(project, run_id)
                .map_err(|error| error.to_string())?;
            let (credential, _) = profile_paths()?;
            let args = vec![
                "-NoProfile".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                resource(
                    "helpers/invoke_compile_credential.ps1",
                    "scripts/invoke_compile_credential.ps1",
                    false,
                )?
                .into_os_string(),
                "-Binary".into(),
                cli_binary()?.into_os_string(),
                "-Project".into(),
                crate::external_path::argument(project)?,
                "-RunId".into(),
                run_id.into(),
                "-CredentialFile".into(),
                credential.into_os_string(),
            ];
            // One book can take hours. Individual requests have Rust deadlines;
            // output remains bounded and the launcher/worker remain in native Jobs.
            let output = run_bounded(
                &powershell()?,
                &args,
                &Arc::new(AtomicBool::new(false)),
                None,
                0,
            )?;
            if let Some(failure) = output.failure {
                return Err(format!("{}：{}", failure.code, failure.message));
            }
            if output.code != Some(0) {
                return Err(compile_worker_failure(output.code, &output.stderr));
            }
            let summary: noetome_cli::production_compile::RunSummary =
                serde_json::from_slice(&output.stdout).map_err(|_| {
                    "DESKTOP_COMPILE_WORKER_INVALID：worker返回无效任务记录。".to_owned()
                })?;
            if summary.run_id != run_id {
                return Err("DESKTOP_COMPILE_BINDING：worker返回了其他任务。".into());
            }
            Ok(summary)
        }
    }
    pub fn compile_export(
        &self,
        project: &Path,
        run_id: &str,
        parent: &Path,
        name: &str,
    ) -> Result<Value, String> {
        project_entry::validate_name(name)?;
        if !parent.is_dir() {
            return Err("COMPILE_PARENT_MISSING：请选择已存在的导出父目录。".into());
        }
        noetome_cli::production_compile::export(project, run_id, &parent.join(name))
            .map_err(|error| error.to_string())
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
            ledger.clone().into_os_string(),
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
        let status = serde_json::from_slice(&output.stdout)
            .map_err(|_| "DESKTOP_MODEL_PROFILE_INVALID：状态报告无效。".to_owned())?;
        production_model_status(status, &ledger)
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
fn compile_config(ledger: PathBuf) -> noetome_cli::production_compile::CompileConfig {
    noetome_cli::production_compile::CompileConfig {
        schema_version: 1,
        endpoint: ENDPOINT.into(),
        model: MODEL.into(),
        credential_mode: None,
        // deepseek-flash otherwise spends the whole 8192-token budget on hidden
        // reasoning and never emits the candidate JSON for content pages.
        reasoning_effort: Some("none".into()),
        credential_env: "NOETOME_TANGMAO_API_KEY".into(),
        transport_mode: "live".into(),
        usage_ledger: ledger,
        max_chunk_bytes: 6000,
        max_output_tokens: 8192,
        request_timeout_ms: 60000,
        request_strategy_version: None,
    }
}
fn production_model_status(mut status: ModelStatus, ledger: &Path) -> Result<ModelStatus, String> {
    let marker = noetome_cli::production_budget::authority_marker_path(ledger);
    let database = noetome_cli::production_budget::default_database_path(ledger);
    // A partial/invalid authority never falls back to the historical JSON balance.
    let absent = |path: &Path| matches!(fs::symlink_metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound);
    if absent(&marker) && absent(&database) {
        return Ok(status);
    }
    match noetome_cli::production_budget::BudgetAuthority::open(ledger)
        .and_then(|mut authority| authority.status())
    {
        Ok(budget) => {
            status.ledger_state = "production_ready".into();
            status.accounted_tokens = Some(budget.accounted_tokens);
            status.pending_reserved_tokens = Some(budget.pending_reserved_tokens);
            status.remaining_tokens = Some(budget.remaining_tokens);
            status.model_attempts = Some(budget.model_attempts);
            status.ready = status.credential_state == "ready"
                && !budget.stopped
                && budget.remaining_tokens > 0;
            status.message = if status.ready {
                "生产整理使用当前累计预算；待核算预留已经计入余额。"
            } else {
                "生产凭据或预算不可用；待核算预留保留，请查看状态。"
            }
            .into();
        }
        Err(_) => {
            status.ready = false;
            status.ledger_state = "production_invalid".into();
            status.accounted_tokens = None;
            status.pending_reserved_tokens = None;
            status.remaining_tokens = None;
            status.model_attempts = None;
            status.message = "生产预算权威记录未通过验证，历史JSON余额不能作为当前余额。".into();
        }
    }
    Ok(status)
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
) -> thread::JoinHandle<()> {
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
    })
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
    let (kill, census) = {
        let mut limits = win32job::ExtendedLimitInfo::new();
        limits.limit_kill_on_job_close();
        let kill = Arc::new(win32job::Job::create_with_limit_info(&limits).map_err(io_error)?);
        let census = Arc::new(win32job::Job::create().map_err(io_error)?);
        let mut flags = CreationFlags(Default::default());
        flags.0.0 = 0x08000000;
        wrap.wrap(flags)
            .wrap(JobObserver {
                kill: kill.clone(),
                census: census.clone(),
            })
            .wrap(JobObject);
        (kill, census)
    };
    let mut child = wrap.spawn().map_err(io_error)?;
    #[cfg(windows)]
    let root_process = noetome_cli::native_process::NativeProcess::open(child.id());
    #[cfg(windows)]
    let mut held_processes = noetome_cli::native_process::JobProcesses::new();
    let overflow = Arc::new(AtomicBool::new(false));
    let (outsend, outrecv) = mpsc::channel();
    let (errsend, errrecv) = mpsc::channel();
    let out_thread = pipe(
        child
            .stdout()
            .take()
            .ok_or_else(|| "DESKTOP_PIPE：stdout不可用。".to_owned())?,
        16777216,
        overflow.clone(),
        outsend,
    );
    let err_thread = pipe(
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
        #[cfg(windows)]
        if let Err(error) = held_processes.capture(&census) {
            failure = Some(Failure {
                code: "DESKTOP_CLEANUP".into(),
                message: error,
            });
            break None;
        }
        match child.inner_mut().try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(error) => {
                failure = Some(Failure {
                    code: "DESKTOP_IO".into(),
                    message: error.to_string(),
                });
                break None;
            }
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
        if (timeout_ms != 0 && started.elapsed().as_millis() > u128::from(timeout_ms))
            || overflow.load(Ordering::SeqCst)
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
    // A root exit and an empty Job census do not prove that Windows has completed
    // descendant termination. Capture owned process identities before requesting it.
    let cleanup_deadline = Instant::now() + Duration::from_secs(10);
    #[cfg(windows)]
    let captured = held_processes.capture(&census);
    let kill_result = if status.is_none() || cfg!(windows) {
        child.start_kill()
    } else {
        Ok(())
    };
    // Close every kill-on-close job handle before waiting, including the wrapper's
    // independent fallback and process-wrap's child job. Keep the census and actual
    // process handles alive; they cannot later resolve to reused PIDs.
    drop(child);
    drop(wrap);
    #[cfg(windows)]
    drop(kill);
    #[cfg(windows)]
    let mut identity_error = root_process.as_ref().err().cloned().or(captured.err());
    #[cfg(windows)]
    {
        loop {
            let root_exited = match &root_process {
                Ok(root) => match root.has_exited() {
                    Ok(exited) => exited,
                    Err(error) => {
                        identity_error = Some(error);
                        false
                    }
                },
                Err(_) => false,
            };
            let all_exited = match held_processes.all_exited() {
                Ok(exited) => exited,
                Err(error) => {
                    identity_error = Some(error);
                    false
                }
            };
            let census_empty = match census.query_process_id_list() {
                Ok(pids) => pids.is_empty(),
                Err(error) => {
                    identity_error = Some(error.to_string());
                    false
                }
            };
            if root_exited && all_exited && census_empty {
                break;
            }
            if Instant::now() >= cleanup_deadline {
                return Err(format!(
                    "DESKTOP_CLEANUP：清理期限内未确认根进程和全部子进程结束。{}",
                    identity_error.unwrap_or_default()
                ));
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
    while !(out_thread.is_finished() && err_thread.is_finished()) {
        if Instant::now() >= cleanup_deadline {
            return Err("DESKTOP_CLEANUP：输出读取线程未在清理期限内结束。".into());
        }
        thread::sleep(Duration::from_millis(2));
    }
    out_thread
        .join()
        .map_err(|_| "DESKTOP_CLEANUP：stdout读取线程异常。".to_owned())?;
    err_thread
        .join()
        .map_err(|_| "DESKTOP_CLEANUP：stderr读取线程异常。".to_owned())?;
    let stdout = outrecv
        .recv_timeout(cleanup_deadline.saturating_duration_since(Instant::now()))
        .map_err(|error| format!("DESKTOP_CLEANUP：stdout未在清理期限内结束：{error}"));
    let stderr = errrecv
        .recv_timeout(cleanup_deadline.saturating_duration_since(Instant::now()))
        .map_err(|error| format!("DESKTOP_CLEANUP：stderr未在清理期限内结束：{error}"));
    #[cfg(windows)]
    if let Some(error) = identity_error {
        return Err(format!("DESKTOP_CLEANUP：进程身份确认失败：{error}"));
    }
    kill_result.map_err(|error| format!("DESKTOP_CLEANUP：终止请求失败：{error}"))?;
    let stdout = stdout??;
    let stderr = stderr??;
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
        .add_filter("文本资料", &["md", "txt"])
        .pick_file()
        .map(|path| path.display().to_string()))
}
#[cfg(not(windows))]
pub fn source_pick() -> Result<Option<String>, String> {
    Err("DESKTOP_SOURCE_DIALOG_UNAVAILABLE：当前系统没有原生文件选择器。".into())
}

#[cfg(windows)]
pub fn corpus_pick() -> Result<Option<String>, String> {
    Ok(rfd::FileDialog::new()
        .set_title("选择整本 PDF")
        .add_filter("PDF", &["pdf"])
        .pick_file()
        .map(|path| path.display().to_string()))
}
#[cfg(not(windows))]
pub fn corpus_pick() -> Result<Option<String>, String> {
    Err("DESKTOP_SOURCE_DIALOG_UNAVAILABLE：当前系统没有原生文件选择器。".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn production_balance_replaces_historical_status_and_keeps_unknown_reservations() {
        use noetome_cli::production_budget::{BudgetAttemptKey, BudgetAuthority, BudgetRequest};
        let root = TestRoot::new();
        let ledger = root.0.join("synthetic.usage.json");
        fs::write(&ledger, serde_json::to_vec(&serde_json::json!({
            "schema_version":1,"approved_total_tokens":100000,"accounted_tokens":33185,
            "pending_reserved_tokens":0,"remaining_tokens":66815,"model_attempts":1,
            "records":[{"invocation_id":"historical-request","reservation_tokens":50000,
                "confirmed_total_tokens":33185,"network_attempts":1,"status":"settled_valid_usage_text"}],
            "updated_at":"2026-11-16T00:00:00Z"
        })).unwrap()).unwrap();
        let original = fs::read(&ledger).unwrap();
        let mut authority = BudgetAuthority::migrate(&ledger).unwrap();
        authority
            .reserve(&BudgetRequest {
                key: BudgetAttemptKey {
                    run_id: "synthetic-run".into(),
                    unit_id: "synthetic-unit".into(),
                    attempt_no: 1,
                    request_id: "synthetic-request".into(),
                },
                request_sha256: noetome_cli::text::sha256(b"synthetic request"),
                reserved_tokens: 9000,
            })
            .unwrap();
        drop(authority);
        let old = ModelStatus {
            ready: true,
            endpoint: ENDPOINT.into(),
            model: MODEL.into(),
            credential_state: "ready".into(),
            ledger_state: "ready".into(),
            accounted_tokens: Some(1),
            pending_reserved_tokens: Some(0),
            remaining_tokens: Some(99999),
            model_attempts: Some(0),
            message: "historical".into(),
        };
        let current = production_model_status(old, &ledger).unwrap();
        assert!(current.ready);
        assert_eq!(current.ledger_state, "production_ready");
        assert_eq!(current.accounted_tokens, Some(33185));
        assert_eq!(current.pending_reserved_tokens, Some(9000));
        assert_eq!(current.remaining_tokens, Some(57815));
        assert_eq!(fs::read(&ledger).unwrap(), original);
        #[cfg(windows)]
        {
            let mut permission = fs::metadata(&ledger).unwrap().permissions();
            permission.set_readonly(false);
            fs::set_permissions(&ledger, permission).unwrap();
        }
    }
    #[test]
    fn partial_production_authority_hides_old_balance_instead_of_falling_back() {
        let root = TestRoot::new();
        let ledger = root.0.join("synthetic.usage.json");
        fs::write(
            noetome_cli::production_budget::authority_marker_path(&ledger),
            b"{}",
        )
        .unwrap();
        let old = ModelStatus {
            ready: true,
            endpoint: ENDPOINT.into(),
            model: MODEL.into(),
            credential_state: "ready".into(),
            ledger_state: "ready".into(),
            accounted_tokens: Some(33185),
            pending_reserved_tokens: Some(0),
            remaining_tokens: Some(9966815),
            model_attempts: Some(10),
            message: "historical".into(),
        };
        let current = production_model_status(old, &ledger).unwrap();
        assert!(!current.ready);
        assert_eq!(current.credential_state, "ready");
        assert_eq!(current.ledger_state, "production_invalid");
        assert_eq!(current.accounted_tokens, None);
        assert_eq!(current.pending_reserved_tokens, None);
        assert_eq!(current.remaining_tokens, None);
        assert_eq!(current.model_attempts, None);
    }
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
    struct HeldTreeProbe {
        child: std::process::Child,
        input: std::process::ChildStdin,
        snapshots: mpsc::Receiver<Result<String, String>>,
        reader: Option<thread::JoinHandle<()>>,
    }
    #[cfg(windows)]
    impl HeldTreeProbe {
        fn open(root: &Path, pids: &[u32]) -> (Self, Value) {
            use std::io::BufRead;
            // Independent observer uses Win32 WaitForSingleObject over handles
            // acquired while all three acknowledged processes are still alive.
            let script = root.join("held-process-observer.ps1");
            fs::write(&script, r#"
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class NoetomeHeldProcessProbe {
  [StructLayout(LayoutKind.Sequential)] public struct FT { public uint lo; public uint hi; }
  [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr OpenProcess(uint rights, bool inherit, uint pid);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern uint WaitForSingleObject(IntPtr process, uint milliseconds);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool GetProcessTimes(IntPtr process, out FT creation, out FT exit, out FT kernel, out FT user);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool CloseHandle(IntPtr handle);
  public static long[] Times(IntPtr process) {
    FT creation, exit, kernel, user;
    if (!GetProcessTimes(process, out creation, out exit, out kernel, out user)) throw new System.ComponentModel.Win32Exception();
    return new long[] { ((long)creation.hi << 32) | creation.lo, ((long)exit.hi << 32) | exit.lo };
  }
}
'@
$pids=[Console]::ReadLine() | ConvertFrom-Json
$handles=@($pids | ForEach-Object { $h=[NoetomeHeldProcessProbe]::OpenProcess(0x101000,$false,[uint32]$_); if($h -eq [IntPtr]::Zero){throw 'Owned test process handle unavailable'}; $h })
function Snapshot {
  $times=@($handles | ForEach-Object { ,([NoetomeHeldProcessProbe]::Times($_)) })
  $v=[ordered]@{pids=@($pids | ForEach-Object { [uint32]$_ });creation_times=@($times | ForEach-Object { $_[0] });exit_times=@($times | ForEach-Object { $_[1] });wait_codes=@($handles | ForEach-Object { [NoetomeHeldProcessProbe]::WaitForSingleObject($_,0) })}
  [Console]::Out.WriteLine(($v | ConvertTo-Json -Compress)); [Console]::Out.Flush()
}
try { Snapshot; while($null -ne [Console]::ReadLine()) { Snapshot } }
finally { $handles | ForEach-Object { [void][NoetomeHeldProcessProbe]::CloseHandle($_) } }
"#).unwrap();
            let mut child = std::process::Command::new(powershell().unwrap())
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(&script)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut input = child.stdin.take().unwrap();
            input
                .write_all(format!("{}\n", serde_json::to_string(pids).unwrap()).as_bytes())
                .unwrap();
            input.flush().unwrap();
            let output = child.stdout.take().unwrap();
            let (send, snapshots) = mpsc::channel();
            let reader = thread::spawn(move || {
                for line in std::io::BufReader::new(output).lines() {
                    if send.send(line.map_err(|error| error.to_string())).is_err() {
                        break;
                    }
                }
            });
            let probe = Self {
                child,
                input,
                snapshots,
                reader: Some(reader),
            };
            let first = probe.receive();
            (probe, first)
        }
        fn receive(&self) -> Value {
            let line = self
                .snapshots
                .recv_timeout(Duration::from_secs(10))
                .expect("independent held observer must respond")
                .expect("held observer stdout must be readable");
            serde_json::from_str(&line).expect("held observer snapshot must be JSON")
        }
        fn sample(&mut self) -> Value {
            self.input.write_all(b"sample\n").unwrap();
            self.input.flush().unwrap();
            self.receive()
        }
    }
    #[cfg(windows)]
    impl Drop for HeldTreeProbe {
        fn drop(&mut self) {
            // Only the independently spawned observer, which has no descendants.
            let _ = self.child.kill();
            let _ = self.child.wait();
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
        }
    }
    #[cfg(windows)]
    fn native_tree_cleanup(mode: &str) {
        let root = TestRoot::new();
        let grand_script = root.0.join("grandchild.ps1");
        let child_script = root.0.join("child.ps1");
        let root_script = root.0.join("root.ps1");
        let grand_pid = root.0.join("grandchild.pid");
        let child_pid = root.0.join("child.pid");
        let root_pid = root.0.join("root.pid");
        let gate = root.0.join("continue.signal");
        let shell = powershell().unwrap();
        let quote = |path: &Path| path.display().to_string().replace('\'', "''");
        let acknowledge = |path: &Path| {
            format!(
                "[IO.File]::WriteAllText('{}.tmp',[string]$PID);[IO.File]::Move('{}.tmp','{}');",
                quote(path),
                quote(path),
                quote(path)
            )
        };
        fs::write(
            &grand_script,
            format!("{}Start-Sleep -Seconds 180", acknowledge(&grand_pid)),
        )
        .unwrap();
        let launch = |script: &Path| {
            format!(
                "$i=New-Object Diagnostics.ProcessStartInfo;$i.FileName='{}';$i.Arguments='-NoProfile -ExecutionPolicy Bypass -File \"{}\"';$i.UseShellExecute=$false;[void][Diagnostics.Process]::Start($i);",
                quote(&shell),
                quote(script)
            )
        };
        fs::write(&child_script, format!(
            "{}while(-not [IO.File]::Exists('{}')){{Start-Sleep -Milliseconds 10}};{}Start-Sleep -Seconds 180",
            launch(&grand_script), quote(&grand_pid), acknowledge(&child_pid)
        )).unwrap();
        fs::write(&root_script, format!(
            "{}while(-not [IO.File]::Exists('{}')){{Start-Sleep -Milliseconds 10}};{}while(-not [IO.File]::Exists('{}')){{Start-Sleep -Milliseconds 10}};{}",
            launch(&child_script), quote(&child_pid), acknowledge(&root_pid), quote(&gate),
            if mode == "normal" { "exit 0" } else { "Start-Sleep -Seconds 180" }
        )).unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        let worker_flag = flag.clone();
        let script = root_script.clone();
        let program = shell.clone();
        let started = Instant::now();
        let worker = thread::spawn(move || {
            run_bounded(
                &program,
                &[
                    "-NoProfile".into(),
                    "-ExecutionPolicy".into(),
                    "Bypass".into(),
                    "-File".into(),
                    script.into_os_string(),
                ],
                &worker_flag,
                None,
                20000,
            )
        });
        while !root_pid.is_file() {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "root/child/grandchild must acknowledge"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let pids: Vec<u32> = [&root_pid, &child_pid, &grand_pid]
            .iter()
            .map(|path| fs::read_to_string(path).unwrap().parse().unwrap())
            .collect();
        let (mut probe, live) = HeldTreeProbe::open(&root.0, &pids);
        assert_eq!(live["pids"], json!(pids));
        assert_eq!(
            live["wait_codes"],
            json!([258, 258, 258]),
            "all owned identities must be alive before stop"
        );
        println!("desktop_tree_mode={mode} held_live={live}");
        if mode == "cancel" {
            flag.store(true, Ordering::SeqCst);
        }
        fs::write(&gate, b"continue").unwrap();
        let output = worker.join().unwrap().unwrap();
        let stopped = probe.sample();
        println!(
            "desktop_tree_mode={mode} held_stopped={stopped} elapsed_ms={}",
            started.elapsed().as_millis()
        );
        assert_eq!(
            stopped["creation_times"], live["creation_times"],
            "same actual held identities, never reopened PIDs"
        );
        assert_eq!(
            stopped["wait_codes"],
            json!([0, 0, 0]),
            "all owned descendants must finish before run_bounded returns"
        );
        if mode == "normal" {
            assert_eq!(output.code, Some(0));
            assert!(output.failure.is_none());
        } else {
            assert_eq!(
                output.failure.unwrap().code,
                if mode == "cancel" {
                    "DESKTOP_CANCELLED"
                } else {
                    "DESKTOP_TIMEOUT"
                }
            );
        }
        assert!(started.elapsed() < Duration::from_secs(30));
    }
    #[cfg(windows)]
    #[test]
    fn native_descendant_is_killed_after_the_root_exits_with_inherited_pipes() {
        native_tree_cleanup("normal");
    }
    #[cfg(windows)]
    #[test]
    fn native_descendants_finish_before_cancellation_returns() {
        native_tree_cleanup("cancel");
    }
    #[cfg(windows)]
    #[test]
    fn native_descendants_finish_before_timeout_returns() {
        native_tree_cleanup("timeout");
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

// Helper output has already passed its credential redactor; retain a bounded diagnostic.
fn compile_worker_failure(exit_code: Option<i32>, stderr: &[u8]) -> String {
    let stderr: String = String::from_utf8_lossy(stderr).chars().take(4096).collect();
    let code = if stderr.contains("compile-credential-invoke: scope-invalid") {
        "DESKTOP_COMPILE_CREDENTIAL_SCOPE"
    } else if stderr.contains("compile-credential-invoke: credential-missing") {
        "DESKTOP_COMPILE_CREDENTIAL_MISSING"
    } else if stderr.contains("compile-credential-invoke: credential-unavailable") {
        "DESKTOP_COMPILE_CREDENTIAL_UNAVAILABLE"
    } else {
        "DESKTOP_COMPILE_WORKER_REFUSED"
    };
    json!({"code":code,"message":"整理worker失败，请检查凭据与持久任务状态。",
        "exit_code":exit_code,"stderr":stderr})
    .to_string()
}
#[cfg(test)]
mod compile_failure_tests {
    #[test]
    fn preserves_diagnostics_and_exit_code() {
        for (diagnostic, suffix) in [
            ("scope-invalid", "SCOPE"),
            ("credential-missing", "MISSING"),
            ("credential-unavailable", "UNAVAILABLE"),
        ] {
            let raw = format!("compile-credential-invoke: {diagnostic}\n");
            let value: serde_json::Value =
                serde_json::from_str(&super::compile_worker_failure(Some(2), raw.as_bytes()))
                    .unwrap();
            assert_eq!(
                value["code"],
                format!("DESKTOP_COMPILE_CREDENTIAL_{suffix}")
            );
            assert_eq!(value["stderr"], raw);
            assert_eq!(value["exit_code"], 2);
        }
    }
}
