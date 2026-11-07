//! One bounded synthetic Responses workflow. Accounting remains in BudgetStore.
use crate::{
    compile_identity::{Identity, identity},
    extraction_api::{self, ApiHooks},
    extraction_api_config::SelectedConfig,
    extraction_source::SourceSession,
    project::{self, ProjectError},
    request_budget_model::*,
    request_budget_store::{BudgetStore, ReceiptRetryDecision, registered_identity_matches},
    run_model::FileStamp,
    source::CommandOutput,
    text::sha256,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, UNIX_EPOCH},
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
#[path = "request_run_recovery.rs"]
mod recovery;

const OWNER: &str = "control/owner.json";
const LOCK: &str = "control/worker.lock";
fn err(code: &'static str, text: impl Into<String>) -> ProjectError {
    ProjectError::new(code, 3, text)
}
fn err_code(code: &str, text: &str) -> ProjectError {
    // Transport errors are a closed existing protocol, never dynamically
    // converted into an invented retry classification.
    let code = match code {
        "EXTRACT_API_TRANSPORT" => "EXTRACT_API_TRANSPORT",
        "EXTRACT_API_TRUNCATED" => "EXTRACT_API_TRUNCATED",
        "EXTRACT_TIMEOUT" => "EXTRACT_TIMEOUT",
        "EXTRACT_API_HTTP_AUTH" => "EXTRACT_API_HTTP_AUTH",
        "EXTRACT_API_HTTP_RATE_LIMIT" => "EXTRACT_API_HTTP_RATE_LIMIT",
        "EXTRACT_API_HTTP_SERVER" => "EXTRACT_API_HTTP_SERVER",
        "EXTRACT_API_HTTP_STATUS" => "EXTRACT_API_HTTP_STATUS",
        "EXTRACT_RESPONSE_LIMIT" => "EXTRACT_RESPONSE_LIMIT",
        "EXTRACT_CANCELLED" => "EXTRACT_CANCELLED",
        _ => "REQUEST_RUN_INTEGRITY",
    };
    err(code, text)
}
fn invalid(text: impl Into<String>) -> ProjectError {
    err("REQUEST_RUN_INTEGRITY", text)
}
fn encoded<T: Serialize>(value: &T) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec(value).map_err(|e| invalid(e.to_string()))
}
fn decoded<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, ProjectError> {
    serde_json::from_slice(bytes).map_err(|e| err("REQUEST_RUN_CONFIG", e.to_string()))
}
fn utc(value: &str) -> Result<OffsetDateTime, ProjectError> {
    let t = OffsetDateTime::parse(value, &Rfc3339).map_err(|_| invalid("必须保存真实UTC时间。"))?;
    if t.offset() != time::UtcOffset::UTC {
        return Err(invalid("不是UTC时间。"));
    }
    Ok(t)
}
fn deadline(at: OffsetDateTime, ms: u64) -> Result<String, ProjectError> {
    at.checked_add(time::Duration::milliseconds(ms as i64))
        .ok_or_else(|| invalid("截止时间溢出。"))?
        .format(&Rfc3339)
        .map_err(|e| invalid(e.to_string()))
}
fn name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
fn canonical(path: &Path) -> Result<PathBuf, ProjectError> {
    let p = project::check_components(path)?;
    fs::canonicalize(&p).map_err(|e| ProjectError::io(&p, e))
}
fn new_path(path: &Path) -> Result<PathBuf, ProjectError> {
    let p = project::check_components(path)?;
    if project::exists(&p)? {
        return Err(err("REQUEST_RUN_EXISTS", "目标须是明确不存在的新根。"));
    }
    let parent = canonical(p.parent().ok_or_else(|| invalid("目标没有父目录。"))?)?;
    Ok(parent.join(p.file_name().ok_or_else(|| invalid("目标名称缺失。"))?))
}
fn overlap(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
pub(super) fn stamp(path: &Path) -> Result<(FileStamp, Vec<u8>), ProjectError> {
    stamp_with(path, false)
}
// The original API owns a read/write handle with ShareRead. Our read-only
// observation must share that already-owned writer, while external inputs
// retain the ordinary deny-write read guard.
pub(super) fn owned_stamp(path: &Path) -> Result<(FileStamp, Vec<u8>), ProjectError> {
    stamp_with(path, true)
}
fn stamp_with(path: &Path, owned: bool) -> Result<(FileStamp, Vec<u8>), ProjectError> {
    // Input and owner checks use the same actual-read-handle proof as the
    // authority. Its owned/external sharing and bounded full-byte checks are
    // identical; keep this caller's Run integrity error namespace.
    crate::request_budget_store::stamp_mode(path, owned).map_err(|e| {
        if e.code() == "REQUEST_BUDGET_INTEGRITY" {
            invalid(format!("明确选取原件校验失败：{e}"))
        } else {
            e
        }
    })
}
pub(super) fn write_new(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    project::check_components(path)?;
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| ProjectError::io(path, e))?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| ProjectError::io(path, e))?;
    drop(f);
    if stamp(path)?.1 != bytes {
        return Err(invalid("原件同步后读回不等。"));
    }
    Ok(())
}
fn files(
    root: &Path,
    pins: &mut BTreeMap<String, FileStamp>,
    dirs: &mut BTreeMap<String, Identity>,
) -> Result<(), ProjectError> {
    project_inventory_names(root)?;
    files_known(root, pins, dirs)
}
fn project_inventory_names(root: &Path) -> Result<(), ProjectError> {
    // This new workflow accepts only the explicit minimal synthetic project.
    // Reject unknown top-level entries from metadata without opening contents
    // or descending an unselected tree. SourceSession must have proved the
    // registered cache before callers reach this capture.
    for entry in fs::read_dir(root).map_err(|e| ProjectError::io(root, e))? {
        let entry = entry.map_err(|e| ProjectError::io(root, e))?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| invalid("测试项目有非UTF8库存。"))?;
        let meta = fs::symlink_metadata(entry.path()).map_err(|e| ProjectError::io(root, e))?;
        let allowed = match name {
            "project.json" | "project.sqlite" => meta.is_file(),
            ".noetome-sources" | ".noetome-pdf-parses" => meta.is_dir(),
            _ => false,
        };
        if !allowed || project::is_link(&meta) {
            return Err(err(
                "REQUEST_RUN_SOURCE_INVENTORY",
                "仅接受已经证明登记的最小测试项目库存；未知内容未读取。",
            ));
        }
    }
    Ok(())
}
fn files_known(
    root: &Path,
    pins: &mut BTreeMap<String, FileStamp>,
    dirs: &mut BTreeMap<String, Identity>,
) -> Result<(), ProjectError> {
    if dirs.len() + pins.len() > 128 {
        return Err(invalid("本有限流程选取库存超过128项。"));
    }
    dirs.insert(root.to_string_lossy().into(), identity(root)?);
    for e in fs::read_dir(root).map_err(|e| ProjectError::io(root, e))? {
        let p = e.map_err(|e| ProjectError::io(root, e))?.path();
        let m = fs::symlink_metadata(&p).map_err(|e| ProjectError::io(&p, e))?;
        if project::is_link(&m) {
            return Err(ProjectError::unsafe_path(&p));
        }
        if m.is_dir() {
            files_known(&p, pins, dirs)?;
        } else {
            pins.insert(p.to_string_lossy().into(), stamp(&p)?.0);
        }
    }
    Ok(())
}
fn directory_handle(path: &Path) -> Result<File, ProjectError> {
    let mut o = OpenOptions::new();
    o.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        o.share_mode(3).custom_flags(0x02200000);
    }
    o.open(path).map_err(|e| ProjectError::io(path, e))
}
fn worker(path: &Path, create: bool) -> Result<File, ProjectError> {
    project::check_components(path)?;
    let mut o = OpenOptions::new();
    o.read(true).write(true).create_new(create);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        o.share_mode(3).custom_flags(0x00200000);
    }
    let f = o.open(path).map_err(|e| ProjectError::io(path, e))?;
    if f.metadata().map_err(|e| ProjectError::io(path, e))?.len() != 0 {
        return Err(invalid("worker锁须是普通单link空文件。"));
    }
    identity(path)?;
    Ok(f)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorityConfig {
    schema_version: u32,
    producer: String,
    global_budget: BudgetLimits,
    opening: OpeningBalance,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetryConfig {
    base_delay_ms: u64,
    max_delay_ms: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UnitConfig {
    unit_id: String,
    project: String,
    import_id: String,
    api_config: String,
    max_attempts: u32,
    max_retry_tokens: u64,
    max_elapsed_ms: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunConfig {
    schema_version: u32,
    producer: String,
    run_budget: BudgetLimits,
    concurrency: usize,
    max_elapsed_ms: u64,
    retry: RetryConfig,
    units: Vec<UnitConfig>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunOwner {
    schema_version: u32,
    producer: String,
    run_id: String,
    root: String,
    budget_root: String,
    budget_identity: Identity,
    root_identity: Identity,
    parent_identity: Identity,
    control_identity: Identity,
    attempts_identity: Identity,
    owner_identity: Identity,
    lock_identity: Identity,
    config_path: String,
    config_stamp: FileStamp,
    compiler_fingerprint: String,
    config: RunConfig,
    spec: RunSpec,
    input_directories: BTreeMap<String, Identity>,
    unit_directories: BTreeMap<String, Identity>,
}
struct RunGuard {
    root: PathBuf,
    owner: RunOwner,
    owner_bytes: Vec<u8>,
    _worker: File,
    _directories: Vec<File>,
    _owner_file: File,
    active: Mutex<BTreeSet<PathBuf>>,
}
struct ActiveAttempt {
    guard: Arc<RunGuard>,
    path: PathBuf,
}
impl Drop for ActiveAttempt {
    fn drop(&mut self) {
        if let Ok(mut active) = self.guard.active.lock() {
            active.remove(&self.path);
        }
    }
}
impl RunGuard {
    fn trusted_owner(&self, view: &BudgetView) -> Result<(), ProjectError> {
        // Only fixed caller-selected root/control paths are read before this
        // external authority record authenticates any owner-derived path.
        let r = find_run(view, &self.owner.run_id)?;
        let o = &self.owner;
        let units: BTreeMap<String, Identity> = r
            .identity
            .units
            .iter()
            .map(|(unit, id)| {
                (
                    self.root
                        .join("attempts")
                        .join(unit)
                        .to_string_lossy()
                        .into_owned(),
                    id.clone(),
                )
            })
            .collect();
        if r.spec != o.spec
            || r.identity.root != o.root_identity
            || r.identity.parent != o.parent_identity
            || r.identity.control != o.control_identity
            || r.identity.attempts != o.attempts_identity
            || r.identity.owner.identity != o.owner_identity
            || r.identity.worker.identity != o.lock_identity
            || r.identity.owner != stamp(&self.root.join(OWNER))?.0
            || r.identity.owner.sha256 != sha256(&self.owner_bytes)
            || units != o.unit_directories
        {
            return Err(invalid(
                "原authority中的Run/owner/control/worker身份锚不匹配。",
            ));
        }
        Ok(())
    }
    fn inventory(&self, view: &BudgetView) -> Result<(), ProjectError> {
        let run = find_run(view, &self.owner.run_id)?;
        let active = self
            .active
            .lock()
            .map_err(|_| invalid("worker库存锁损坏。"))?;
        for unit in &run.units {
            let known: BTreeSet<PathBuf> = unit
                .attempts
                .iter()
                .map(|a| self.root.join(&a.plan.attempt_root))
                .collect();
            let base = self.root.join("attempts").join(&unit.spec.unit_id);
            for entry in fs::read_dir(&base).map_err(|e| ProjectError::io(&base, e))? {
                let path = entry.map_err(|e| ProjectError::io(&base, e))?.path();
                if !known.contains(&path) && !active.contains(&path) {
                    return Err(invalid("未登记attempt目录现场保留；不能按名称收养。"));
                }
                let meta = fs::symlink_metadata(&path).map_err(|e| ProjectError::io(&path, e))?;
                if !meta.is_dir() || project::is_link(&meta) {
                    return Err(ProjectError::unsafe_path(&path));
                }
            }
        }
        Ok(())
    }
    fn claim(self: &Arc<Self>, path: &Path) -> Result<ActiveAttempt, ProjectError> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| invalid("worker库存锁损坏。"))?;
        if !active.insert(path.to_path_buf()) {
            return Err(invalid("同attempt有两个实际worker。"));
        }
        Ok(ActiveAttempt {
            guard: self.clone(),
            path: path.to_path_buf(),
        })
    }
    fn authority_binding(&self, view: &BudgetView) -> Result<(), ProjectError> {
        self.trusted_owner(view)?;
        let r = find_run(view, &self.owner.run_id)?;
        let o = &self.owner;
        if r.spec != o.spec
            || r.identity.root != o.root_identity
            || r.identity.parent != o.parent_identity
            || o.spec.case_sha256 != o.config_stamp.sha256
            || o.spec.input_pins.get(&o.config_path) != Some(&o.config_stamp)
        {
            return Err(invalid("Run owner未绑定原authority RunSpec/config。"));
        }
        let (pin, bytes) = stamp(Path::new(&o.config_path))?;
        if pin != o.config_stamp {
            return Err(err("REQUEST_RUN_INPUT_CHANGED", "原Run配置身份/字节变化。"));
        }
        let mut c: RunConfig = decoded(&bytes)?;
        for u in &mut c.units {
            if !Path::new(&u.project).is_absolute() || !Path::new(&u.api_config).is_absolute() {
                return Err(invalid("原配置路径不是绝对路径。"));
            }
            u.project = canonical(Path::new(&u.project))?.to_string_lossy().into();
            u.api_config = canonical(Path::new(&u.api_config))?
                .to_string_lossy()
                .into();
        }
        if encoded(&c)? != encoded(&o.config)? || c.units.len() != o.spec.units.len() {
            return Err(invalid("owner内配置不能绕过原输入配置。"));
        }
        Ok(())
    }
    fn verify(&self) -> Result<(), ProjectError> {
        let o = &self.owner;
        if !registered_identity_matches(&self.root, &o.root_identity)?
            || !registered_identity_matches(
                self.root
                    .parent()
                    .ok_or_else(|| invalid("Run父目录缺失。"))?,
                &o.parent_identity,
            )?
            || !registered_identity_matches(&self.root.join("control"), &o.control_identity)?
            || !registered_identity_matches(&self.root.join("attempts"), &o.attempts_identity)?
            || !registered_identity_matches(&self.root.join(LOCK), &o.lock_identity)?
            || fs::metadata(self.root.join(LOCK))
                .map_err(|e| ProjectError::io(&self.root, e))?
                .len()
                != 0
            || !registered_identity_matches(Path::new(&o.budget_root), &o.budget_identity)?
        {
            return Err(invalid("Run/authority/原owner身份或字节变化。"));
        }
        let (owner, bytes) = stamp(&self.root.join(OWNER))?;
        if owner.identity != o.owner_identity || bytes != self.owner_bytes {
            return Err(invalid("Run原owner身份或字节变化。"));
        }
        let names = |p: &Path| -> Result<BTreeSet<String>, ProjectError> {
            fs::read_dir(p)
                .map_err(|e| ProjectError::io(p, e))?
                .map(|e| {
                    e.map_err(|e| ProjectError::io(p, e)).and_then(|e| {
                        e.file_name()
                            .into_string()
                            .map_err(|_| invalid("非UTF8库存。"))
                    })
                })
                .collect()
        };
        if names(&self.root)? != BTreeSet::from(["control".into(), "attempts".into()])
            || names(&self.root.join("control"))?
                != BTreeSet::from(["owner.json".into(), "worker.lock".into()])
        {
            return Err(invalid("Run出现未知库存。"));
        }
        let expected = self
            .owner
            .config
            .units
            .iter()
            .map(|u| u.unit_id.clone())
            .collect();
        if names(&self.root.join("attempts"))? != expected {
            return Err(invalid("单元目录清单变化。"));
        }
        for (path, id) in &o.unit_directories {
            if identity(Path::new(path))? != *id {
                return Err(invalid("单元目录被替换。"));
            }
        }
        Ok(())
    }
    fn inputs(&self) -> Result<(), ProjectError> {
        self.input_inventory()?;
        for (p, s) in &self.owner.spec.input_pins {
            if stamp(Path::new(p))?.0 != *s {
                return Err(err(
                    "REQUEST_RUN_INPUT_CHANGED",
                    "原输入身份/mtime/字节变化。",
                ));
            }
        }
        self.input_inventory()?;
        if self.owner.compiler_fingerprint != env!("NOETOME_RUN_COMPILER_FINGERPRINT") {
            return Err(err(
                "REQUEST_RUN_VERSION",
                "编译指纹变化，旧累计消费与承诺保留。",
            ));
        }
        Ok(())
    }
    fn input_inventory(&self) -> Result<(), ProjectError> {
        // The initial SourceSession already proved these exact database/cache
        // bytes. Check the entire metadata inventory before hashing any known
        // file, and again afterwards. Unknown branches are never descended.
        let mut directories = BTreeSet::new();
        let mut files = BTreeSet::new();
        for unit in &self.owner.config.units {
            let root = Path::new(&unit.project);
            project_inventory_names(root)?;
            let mut todo = vec![root.to_path_buf()];
            while let Some(path) = todo.pop() {
                let key = path.to_string_lossy().into_owned();
                let expected = self
                    .owner
                    .input_directories
                    .get(&key)
                    .ok_or_else(|| err("REQUEST_RUN_INPUT_CHANGED", "未知输入目录未下降。"))?;
                if identity(&path)? != *expected {
                    return Err(err("REQUEST_RUN_INPUT_CHANGED", "原输入目录身份变化。"));
                }
                directories.insert(key);
                for entry in fs::read_dir(&path).map_err(|e| ProjectError::io(&path, e))? {
                    let p = entry.map_err(|e| ProjectError::io(&path, e))?.path();
                    let key = p.to_string_lossy().into_owned();
                    let m = fs::symlink_metadata(&p).map_err(|e| ProjectError::io(&p, e))?;
                    if project::is_link(&m) {
                        return Err(ProjectError::unsafe_path(&p));
                    }
                    if m.is_dir() {
                        if !self.owner.input_directories.contains_key(&key) {
                            return Err(err("REQUEST_RUN_INPUT_CHANGED", "未知缓存目录未下降。"));
                        }
                        todo.push(p);
                    } else {
                        let s = self.owner.spec.input_pins.get(&key).ok_or_else(|| {
                            err("REQUEST_RUN_INPUT_CHANGED", "未知输入文件未读取。")
                        })?;
                        let modified = m
                            .modified()
                            .map_err(|e| ProjectError::io(&p, e))?
                            .duration_since(UNIX_EPOCH)
                            .map_err(|_| invalid("输入时间早于epoch。"))?
                            .as_nanos()
                            .to_string();
                        if !m.is_file()
                            || m.len() != s.byte_length
                            || modified != s.modified
                            || identity(&p)? != s.identity
                        {
                            return Err(err("REQUEST_RUN_INPUT_CHANGED", "原输入metadata变化。"));
                        }
                        files.insert(key);
                    }
                }
            }
        }
        if directories
            != self
                .owner
                .input_directories
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>()
        {
            return Err(err("REQUEST_RUN_INPUT_CHANGED", "输入目录库存缺失。"));
        }
        let expected: BTreeSet<String> = self
            .owner
            .spec
            .input_pins
            .keys()
            .filter(|p| {
                self.owner
                    .config
                    .units
                    .iter()
                    .any(|u| Path::new(p).starts_with(&u.project))
            })
            .cloned()
            .collect();
        if files != expected {
            return Err(err("REQUEST_RUN_INPUT_CHANGED", "原项目文件库存缺失。"));
        }
        Ok(())
    }
}
fn config(
    path: &Path,
) -> Result<
    (
        PathBuf,
        FileStamp,
        RunConfig,
        BTreeMap<String, FileStamp>,
        BTreeMap<String, Identity>,
    ),
    ProjectError,
> {
    let path = canonical(path)?;
    let (pin, bytes) = stamp(&path)?;
    if bytes.len() > 65536 {
        return Err(err("REQUEST_RUN_CONFIG", "Run配置超过64KiB。"));
    }
    let mut c: RunConfig = decoded(&bytes)?;
    if c.schema_version != 1
        || c.producer != "synthetic_api_run"
        || !(1..=2).contains(&c.concurrency)
        || c.units.is_empty()
        || c.units.len() > 4
        || !(1..=60000).contains(&c.max_elapsed_ms)
        || !(1..=5000).contains(&c.retry.base_delay_ms)
        || c.retry.max_delay_ms < c.retry.base_delay_ms
        || c.retry.max_delay_ms > 10000
        || c.run_budget.max_cost_usd.is_some()
    {
        return Err(err("REQUEST_RUN_CONFIG", "有限Run配置、期限或价格不合法。"));
    }
    let mut ids = BTreeSet::new();
    let mut pins = BTreeMap::from([(path.to_string_lossy().into(), pin.clone())]);
    let mut dirs = BTreeMap::new();
    for u in &mut c.units {
        if !name(&u.unit_id)
            || !ids.insert(u.unit_id.clone())
            || !(1..=4).contains(&u.max_attempts)
            || u.max_elapsed_ms == 0
            || u.max_elapsed_ms > c.max_elapsed_ms
        {
            return Err(err("REQUEST_RUN_CONFIG", "单元身份、次数或期限不合法。"));
        }
        if !Path::new(&u.project).is_absolute() || !Path::new(&u.api_config).is_absolute() {
            return Err(err(
                "REQUEST_RUN_CONFIG",
                "单元project与api_config必须是明确绝对路径。",
            ));
        }
        let project = canonical(Path::new(&u.project))?;
        let api = canonical(Path::new(&u.api_config))?;
        let selected = SelectedConfig::open(&api)?;
        let cfg = &selected.config;
        if cfg.transport_mode != "local_http_test"
            || cfg.provider != "openai_responses"
            || !cfg.credential_env.starts_with("NOETOME_TEST_")
            || !cfg.model.starts_with("noetome-test-")
            || cfg.required_input != "text"
            || !cfg.capabilities.text
            || !cfg.capabilities.structured_output
        {
            return Err(err(
                "REQUEST_RUN_CONFIG",
                "仅一个人工数字环回文本Responses适配器。",
            ));
        }
        // Credential validation reads only the explicitly synthetic variable.
        selected.credential()?;
        project_inventory_names(&project)?;
        let source = SourceSession::open(&project, &u.import_id, &cfg.selection, &cfg.limits)?;
        source.verify()?;
        selected.verify()?;
        files(&project, &mut pins, &mut dirs)?;
        pins.insert(api.to_string_lossy().into(), stamp(&api)?.0);
        u.project = project.to_string_lossy().into();
        u.api_config = api.to_string_lossy().into();
    }
    if pins.len() > 128 {
        return Err(invalid("选取输入库存超过128。"));
    }
    Ok((path, pin, c, pins, dirs))
}
fn load(root: &Path, budget: &Path, exclusive: bool) -> Result<RunGuard, ProjectError> {
    let root = canonical(root)?;
    let budget = canonical(budget)?;
    let bytes = stamp(&root.join(OWNER))?.1;
    let o: RunOwner = decoded(&bytes)?;
    if encoded(&o)? != bytes
        || o.schema_version != 1
        || o.producer != "synthetic_api_run_owner"
        || o.root != root.to_string_lossy()
        || o.budget_root != budget.to_string_lossy()
        || o.run_id != o.spec.run_id
        || o.spec.run_root != o.root
    {
        return Err(invalid("不是原拥有的Run。"));
    }
    let lock = worker(&root.join(LOCK), false)?;
    if exclusive {
        lock.try_lock().map_err(|e| {
            err(
                "REQUEST_RUN_BUSY",
                format!("旧worker尚有能力或锁不可取得：{e}"),
            )
        })?;
    }
    let mut handles = Vec::new();
    let mut p = Some(root.as_path());
    while let Some(path) = p {
        handles.push(directory_handle(path)?);
        p = path.parent();
    }
    let owner_file = project::read_guard(&root.join(OWNER))?;
    let g = RunGuard {
        root,
        owner: o,
        owner_bytes: bytes,
        _worker: lock,
        _directories: handles,
        _owner_file: owner_file,
        active: Mutex::new(BTreeSet::new()),
    };
    let authority = BudgetStore::open_readonly(&budget)?.view()?;
    g.trusted_owner(&authority)?;
    g.verify()?;
    Ok(g)
}
pub fn init(config_path: &Path, root: &Path) -> Result<CommandOutput, ProjectError> {
    let p = canonical(config_path)?;
    let (pin, bytes) = stamp(&p)?;
    if bytes.len() > 65536 {
        return Err(err("REQUEST_RUN_CONFIG", "配置超过64KiB。"));
    }
    let c: AuthorityConfig = decoded(&bytes)?;
    if c.schema_version != 1 || c.producer != "synthetic_api_budget" {
        return Err(err("REQUEST_RUN_CONFIG", "未知authority配置。"));
    }
    let root = new_path(root)?;
    if overlap(&p, &root) {
        return Err(err("REQUEST_RUN_OVERLAP", "authority与配置须分离。"));
    }
    let spec = AuthoritySpec {
        schema_version: 1,
        authority_id: uuid::Uuid::new_v4().to_string(),
        global_limits: c.global_budget,
        opening: c.opening,
        policy_sha256: pin.sha256.clone(),
    };
    let store = BudgetStore::create(&root, &spec, &bytes)?;
    if stamp(&p)?.0 != pin {
        return Err(invalid("初始化期间配置变化。"));
    }
    boundary("global_authority_created_commit", "", 0, "", &root)?;
    Ok(CommandOutput{json:format!("{}\n",serde_json::to_string_pretty(&json!({"schema_version":1,"producer":"synthetic_api_run","operation":"budget_init","budget":store.view()?,"real_model_calls":0,"paid_calls":0,"price_usd":null,"published":false,"knowledge_status":"pending","scientific_quality":"not_run"})).map_err(|e|invalid(e.to_string()))?),failure:None})
}
pub fn start(
    config_path: &Path,
    budget: &Path,
    root: &Path,
    paused: bool,
) -> Result<CommandOutput, ProjectError> {
    let (config_path, config_stamp, c, input_pins, input_directories) = config(config_path)?;
    let budget = canonical(budget)?;
    let root = new_path(root)?;
    for p in input_pins.keys().chain(input_directories.keys()) {
        if overlap(Path::new(p), &root) || overlap(Path::new(p), &budget) {
            return Err(err(
                "REQUEST_RUN_OVERLAP",
                "输入/配置/authority/Run须分离。",
            ));
        }
    }
    if overlap(&root, &budget) {
        return Err(err("REQUEST_RUN_OVERLAP", "Run与authority须分离。"));
    }
    let mut store = BudgetStore::open(&budget)?;
    store.verify_version()?;
    let created = OffsetDateTime::now_utc();
    let run_id = uuid::Uuid::new_v4().to_string();
    fs::create_dir(&root).map_err(|e| ProjectError::io(&root, e))?;
    let root = canonical(&root)?;
    fs::create_dir(root.join("control")).map_err(|e| ProjectError::io(&root, e))?;
    fs::create_dir(root.join("attempts")).map_err(|e| ProjectError::io(&root, e))?;
    let lock = worker(&root.join(LOCK), true)?;
    lock.try_lock()
        .map_err(|e| err("REQUEST_RUN_BUSY", e.to_string()))?;
    let mut unit_directories = BTreeMap::new();
    for u in &c.units {
        let p = root.join("attempts").join(&u.unit_id);
        fs::create_dir(&p).map_err(|e| ProjectError::io(&p, e))?;
        unit_directories.insert(p.to_string_lossy().into(), identity(&p)?);
    }
    let spec = RunSpec {
        run_id: run_id.clone(),
        run_root: root.to_string_lossy().into(),
        case_sha256: config_stamp.sha256.clone(),
        input_pins,
        run_limits: c.run_budget.clone(),
        deadline_utc: deadline(created, c.max_elapsed_ms)?,
        retry_base_delay_ms: c.retry.base_delay_ms,
        retry_max_delay_ms: c.retry.max_delay_ms,
        units: c
            .units
            .iter()
            .map(|u| {
                Ok(UnitSpec {
                    unit_id: u.unit_id.clone(),
                    max_attempts: u.max_attempts,
                    max_retry_tokens: u.max_retry_tokens,
                    deadline_utc: deadline(created, u.max_elapsed_ms)?,
                })
            })
            .collect::<Result<_, ProjectError>>()?,
    };
    let owner_path = root.join(OWNER);
    let mut owner_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&owner_path)
        .map_err(|e| ProjectError::io(&owner_path, e))?;
    let owner = RunOwner {
        schema_version: 1,
        producer: "synthetic_api_run_owner".into(),
        run_id,
        root: root.to_string_lossy().into(),
        budget_root: budget.to_string_lossy().into(),
        budget_identity: identity(&budget)?,
        root_identity: identity(&root)?,
        parent_identity: identity(root.parent().unwrap())?,
        control_identity: identity(&root.join("control"))?,
        attempts_identity: identity(&root.join("attempts"))?,
        owner_identity: identity(&owner_path)?,
        lock_identity: identity(&root.join(LOCK))?,
        config_path: config_path.to_string_lossy().into(),
        config_stamp,
        compiler_fingerprint: env!("NOETOME_RUN_COMPILER_FINGERPRINT").into(),
        config: c,
        spec,
        input_directories,
        unit_directories,
    };
    let bytes = encoded(&owner)?;
    owner_file
        .write_all(&bytes)
        .and_then(|_| owner_file.sync_all())
        .map_err(|e| ProjectError::io(&owner_path, e))?;
    drop(owner_file);
    let mut handles = Vec::new();
    let mut p = Some(root.as_path());
    while let Some(path) = p {
        handles.push(directory_handle(path)?);
        p = path.parent();
    }
    let g = Arc::new(RunGuard {
        root: root.clone(),
        owner,
        owner_bytes: bytes,
        _worker: lock,
        _directories: handles,
        _owner_file: project::read_guard(&owner_path)?,
        active: Mutex::new(BTreeSet::new()),
    });
    g.verify()?;
    g.inputs()?;
    store.register_run(&g.owner.spec)?;
    boundary("api_run_created_commit", "", 0, &g.owner.run_id, &root)?;
    if !paused {
        execute(g.clone(), false)?;
    }
    envelope(&g, &store, "api_run_start")
}
pub fn resume(
    config_path: &Path,
    budget: &Path,
    root: &Path,
) -> Result<CommandOutput, ProjectError> {
    let g = Arc::new(load(root, budget, true)?);
    // Wrong caller arguments are a read-only refusal, even if a separate
    // original input has changed and would otherwise require invalidation.
    if canonical(config_path)?.to_string_lossy() != g.owner.config_path {
        return Err(err(
            "REQUEST_RUN_CONFIG_CHANGED",
            "恢复必须选同一原Run配置。",
        ));
    }
    let mut store = BudgetStore::open(Path::new(&g.owner.budget_root))?;
    let view = store.view()?;
    g.inventory(&view)?;
    if let Err(e) = g.authority_binding(&view) {
        if e.code() == "REQUEST_RUN_INPUT_CHANGED" {
            accounting_only(&g, &mut store, e.code())?;
            release_all_unsent(&g, &mut store, e.code())?;
            store.invalidate_run(&g.owner.run_id, e.code())?;
        }
        return Err(e);
    }
    if let Err(e) = g.inputs() {
        accounting_only(&g, &mut store, e.code())?;
        release_all_unsent(&g, &mut store, e.code())?;
        store.invalidate_run(&g.owner.run_id, e.code())?;
        return Err(e);
    }
    g.verify()?;
    store.verify_version()?;
    let v = store.view()?;
    let r = find_run(&v, &g.owner.run_id)?;
    if matches!(r.outcome, Some(RunOutcome::Invalidated)) {
        return Err(err("REQUEST_RUN_INVALIDATED", "旧Run已经失效。"));
    }
    if r.outcome.is_none() {
        execute(g.clone(), true)?;
    }
    envelope(&g, &store, "api_run_resume")
}
fn accounting_only(
    g: &RunGuard,
    store: &mut BudgetStore,
    diagnostic: &'static str,
) -> Result<(), ProjectError> {
    let view = store.view()?;
    g.trusted_owner(&view)?;
    g.verify()?;
    g.inventory(&view)?;
    let run = find_run(&view, &g.owner.run_id)?;
    if run.spec != g.owner.spec {
        return Err(invalid("纯计费必须绑定原RunSpec。"));
    }
    for a in run.units.iter().flat_map(|u| &u.attempts).filter(|a| {
        a.receipt.is_none()
            && a.transport_sealed
            && matches!(a.state, AttemptState::MaySend | AttemptState::Unknown)
    }) {
        let output = g.root.join(&a.plan.attempt_root);
        let (report, recovered) = recovery::recover_accounting(&output, &a.plan, diagnostic)?;
        let receipt = receipt(&output, &report, recovered)?;
        store.settle(&a.plan.key, &receipt)?;
        boundary(
            "attempt_settled_commit",
            &a.plan.key.unit_id,
            a.plan.key.attempt_no,
            &g.owner.run_id,
            &g.root,
        )?;
    }
    Ok(())
}
fn release_unsent(
    store: &mut BudgetStore,
    g: &RunGuard,
    index: usize,
    reason: &str,
) -> Result<(), ProjectError> {
    let view = store.view()?;
    g.trusted_owner(&view)?;
    g.verify()?;
    g.inventory(&view)?;
    let unit = unit_view(&view, g, index)?;
    if let Some(a) = unit.attempts.last().filter(|a| {
        a.receipt.is_none() && matches!(a.state, AttemptState::Prepared | AttemptState::Reserved)
    }) {
        if store.release_reserved(&a.plan.key, reason)? == SettlementDecision::Applied {
            boundary(
                "attempt_unsent_release_commit",
                &a.plan.key.unit_id,
                a.plan.key.attempt_no,
                &g.owner.run_id,
                &g.root,
            )?;
        }
    }
    Ok(())
}
fn release_all_unsent(
    g: &RunGuard,
    store: &mut BudgetStore,
    reason: &str,
) -> Result<(), ProjectError> {
    for index in 0..g.owner.config.units.len() {
        release_unsent(store, g, index, reason)?;
    }
    Ok(())
}
pub fn status(budget: &Path, root: &Path) -> Result<CommandOutput, ProjectError> {
    let g = load(root, budget, false)?;
    let store = BudgetStore::open_readonly(Path::new(&g.owner.budget_root))?;
    store.verify_version()?;
    g.authority_binding(&store.view()?)?;
    g.inputs()?;
    envelope(&g, &store, "api_run_status")
}
fn find_run<'a>(v: &'a BudgetView, id: &str) -> Result<&'a RunView, ProjectError> {
    v.runs
        .iter()
        .find(|r| r.spec.run_id == id)
        .ok_or_else(|| invalid("原Run尚未在authority登记，保留现场。"))
}
fn envelope(
    g: &RunGuard,
    store: &BudgetStore,
    operation: &str,
) -> Result<CommandOutput, ProjectError> {
    g.verify()?;
    let v = store.view()?;
    g.inventory(&v)?;
    g.authority_binding(&v)?;
    let r = find_run(&v, &g.owner.run_id)?;
    if r.spec != g.owner.spec {
        return Err(invalid("Run owner与原authority RunSpec不同。"));
    }
    let status = match r.outcome {
        Some(RunOutcome::Succeeded) => "succeeded",
        Some(RunOutcome::Failed) => "failed",
        Some(RunOutcome::BudgetStopped) => "budget_stopped",
        Some(RunOutcome::Invalidated) => "invalidated",
        None => "paused_or_interrupted",
    };
    let value = json!({"schema_version":1,"producer":"synthetic_api_run","operation":operation,"run_id":g.owner.run_id,"run_status":status,"budget_root":g.owner.budget_root,"run_root":g.owner.root,"compiler_fingerprint":g.owner.compiler_fingerprint,"run":r,"budget":v,"real_model_calls":0,"paid_calls":0,"price_usd":null,"published":false,"knowledge_status":"pending","scientific_quality":"not_run"});
    let failure = match r.outcome {
        Some(RunOutcome::Failed) => Some((
            "REQUEST_RUN_FAILED".into(),
            "有界单元失败；已确认用量及未知承诺保留。".into(),
        )),
        Some(RunOutcome::BudgetStopped) => Some((
            "REQUEST_RUN_BUDGET_STOP".into(),
            "预算停止新请求；知识保持pending。".into(),
        )),
        Some(RunOutcome::Invalidated) => Some((
            "REQUEST_RUN_INVALIDATED".into(),
            "原输入/依赖失效；累计承诺保留。".into(),
        )),
        _ => None,
    };
    Ok(CommandOutput {
        json: format!(
            "{}\n",
            serde_json::to_string_pretty(&value).map_err(|e| invalid(e.to_string()))?
        ),
        failure,
    })
}

fn execute(g: Arc<RunGuard>, resuming: bool) -> Result<(), ProjectError> {
    let queue = AtomicUsize::new(0);
    let stop = Arc::new(AtomicBool::new(false));
    let results = thread::scope(|scope| {
        let mut threads = Vec::new();
        for _ in 0..g.owner.config.concurrency.min(g.owner.config.units.len()) {
            let guard = g.clone();
            let stop = stop.clone();
            let queue = &queue;
            threads.push(scope.spawn(move || {
                loop {
                    let index = queue.fetch_add(1, Ordering::SeqCst);
                    if index >= guard.owner.config.units.len() {
                        return Ok(());
                    }
                    if stop.load(Ordering::SeqCst) {
                        return Err(err(
                            "REQUEST_RUN_STOPPED",
                            "其他worker发现原件/输入错误，停止新调度。",
                        ));
                    }
                    if let Err(e) = unit_loop(guard.clone(), index, resuming, stop.clone()) {
                        stop.store(true, Ordering::SeqCst);
                        return Err(e);
                    }
                }
            }));
        }
        threads
            .into_iter()
            .map(|h| {
                h.join()
                    .map_err(|_| err("REQUEST_RUN_WORKER", "worker异常退出，原承诺保留。"))?
            })
            .collect::<Vec<Result<(), ProjectError>>>()
    });
    for result in results {
        if let Err(e) = result {
            let mut store = BudgetStore::open(Path::new(&g.owner.budget_root))?;
            accounting_only(&g, &mut store, e.code())?;
            release_all_unsent(&g, &mut store, e.code())?;
            if let Err(changed) = g.inputs() {
                store.invalidate_run(&g.owner.run_id, changed.code())?;
            }
            return Err(e);
        }
    }
    g.verify()?;
    g.inputs()?;
    let mut store = BudgetStore::open(Path::new(&g.owner.budget_root))?;
    let view = store.view()?;
    let run = find_run(&view, &g.owner.run_id)?;
    if run.outcome.is_none() {
        let outcome = if run
            .units
            .iter()
            .all(|u| u.outcome == Some(UnitOutcome::Succeeded))
        {
            RunOutcome::Succeeded
        } else if run
            .units
            .iter()
            .any(|u| u.outcome == Some(UnitOutcome::BudgetStopped))
        {
            RunOutcome::BudgetStopped
        } else {
            RunOutcome::Failed
        };
        store.finish_run(
            &g.owner.run_id,
            outcome,
            "All bounded unit workers exited; known usage and outstanding commitments retained",
        )?;
        boundary("run_terminal_commit", "", 0, &g.owner.run_id, &g.root)?;
    }
    Ok(())
}
fn unit_view<'a>(
    v: &'a BudgetView,
    g: &RunGuard,
    index: usize,
) -> Result<&'a UnitView, ProjectError> {
    let id = &g.owner.config.units[index].unit_id;
    find_run(v, &g.owner.run_id)?
        .units
        .iter()
        .find(|u| &u.spec.unit_id == id)
        .ok_or_else(|| invalid("单元未登记。"))
}
fn retryable(code: &str) -> bool {
    matches!(
        code,
        "EXTRACT_API_HTTP_RATE_LIMIT"
            | "EXTRACT_API_HTTP_SERVER"
            | "EXTRACT_TIMEOUT"
            | "EXTRACT_API_TRANSPORT"
            | "EXTRACT_API_TRUNCATED"
    )
}
fn retryable_receipt(receipt: &AttemptReceipt) -> bool {
    receipt.usage.is_none()
        && !(receipt.response_complete && receipt.http_status == Some(200))
        && receipt.error_code.as_deref().is_some_and(retryable)
}
fn finish_unit(
    store: &mut BudgetStore,
    g: &RunGuard,
    index: usize,
    outcome: UnitOutcome,
    reason: &str,
    attempt: u32,
) -> Result<(), ProjectError> {
    if outcome != UnitOutcome::Succeeded {
        release_unsent(store, g, index, reason)?;
    }
    let id = &g.owner.config.units[index].unit_id;
    store.finish_unit(&g.owner.run_id, id, outcome, reason)?;
    boundary(
        "unit_terminal_commit",
        id,
        attempt,
        &g.owner.run_id,
        &g.root,
    )
}
fn unit_loop(
    g: Arc<RunGuard>,
    index: usize,
    resuming: bool,
    stop: Arc<AtomicBool>,
) -> Result<(), ProjectError> {
    let cfg = &g.owner.config.units[index];
    let mut store = BudgetStore::open(Path::new(&g.owner.budget_root))?;
    loop {
        g.verify()?;
        g.inputs()?;
        let view = store.view()?;
        g.inventory(&view)?;
        let u = unit_view(&view, &g, index)?;
        if u.outcome.is_some() {
            return Ok(());
        }
        let last = u.attempts.last().cloned();
        if let Some(a) = &last {
            let attempt_root = g.root.join(&a.plan.attempt_root);
            if a.receipt.is_none()
                && matches!(a.state, AttemptState::MaySend | AttemptState::Unknown)
            {
                if !resuming {
                    return Err(invalid("活跃API未产生最终结算，不能假称完成。"));
                }
                if a.transport_sealed {
                    let (report, recovered) = recovery::recover(
                        &attempt_root,
                        &a.plan,
                        &cfg.project,
                        &cfg.import_id,
                        &cfg.api_config,
                    )?;
                    let receipt = receipt(&attempt_root, &report, recovered)?;
                    store.settle(&a.plan.key, &receipt)?;
                    boundary(
                        "attempt_settled_commit",
                        &cfg.unit_id,
                        a.plan.key.attempt_no,
                        &g.owner.run_id,
                        &g.root,
                    )?;
                    continue;
                }
                // An unsealed external report cannot refund or confirm a request.
                if a.state == AttemptState::MaySend {
                    store.retain_unknown(&a.plan.key, "REQUEST_INTERRUPTED")?;
                }
                if a.unknown_reason
                    .as_deref()
                    .is_some_and(|s| s != "REQUEST_INTERRUPTED")
                {
                    return Err(invalid("原未知诊断不能在恢复中改写。"));
                }
                if a.plan.key.attempt_no >= u.spec.max_attempts
                    || OffsetDateTime::now_utc() >= utc(&u.spec.deadline_utc)?
                {
                    finish_unit(
                        &mut store,
                        &g,
                        index,
                        UnitOutcome::Failed,
                        "Interrupted request remains unknown; retry attempts or deadline exhausted",
                        a.plan.key.attempt_no,
                    )?;
                    return Ok(());
                }
                if a.retry.is_none() {
                    let delay = g
                        .owner
                        .spec
                        .retry_base_delay_ms
                        .saturating_mul(1u64 << a.plan.key.attempt_no.saturating_sub(1))
                        .min(g.owner.spec.retry_max_delay_ms);
                    if OffsetDateTime::now_utc() + time::Duration::milliseconds(delay as i64)
                        >= utc(&u.spec.deadline_utc)?
                        || view.global_stopped
                        || find_run(&view, &g.owner.run_id)?.budget_stopped
                    {
                        finish_unit(
                            &mut store,
                            &g,
                            index,
                            UnitOutcome::Failed,
                            "Interrupted request remains held; no retry fits original limits",
                            a.plan.key.attempt_no,
                        )?;
                        return Ok(());
                    }
                    store.schedule_retry_after(&a.plan.key, "REQUEST_INTERRUPTED")?;
                    boundary(
                        "retry_schedule_commit",
                        &cfg.unit_id,
                        a.plan.key.attempt_no,
                        &g.owner.run_id,
                        &g.root,
                    )?;
                    continue;
                }
            }
            if let Some(r) = &a.receipt {
                if r.candidate_status == CandidateStatus::Validated {
                    finish_unit(
                        &mut store,
                        &g,
                        index,
                        UnitOutcome::Succeeded,
                        "Strict source-bound candidates confirmed; knowledge remains pending",
                        a.plan.key.attempt_no,
                    )?;
                    return Ok(());
                }
                let code = r
                    .error_code
                    .as_deref()
                    .ok_or_else(|| invalid("失败receipt缺分类。"))?;
                if code.starts_with("REQUEST_BUDGET") || code == "REQUEST_RUN_BUDGET_STOP" {
                    finish_unit(
                        &mut store,
                        &g,
                        index,
                        UnitOutcome::BudgetStopped,
                        code,
                        a.plan.key.attempt_no,
                    )?;
                    return Ok(());
                }
                if !retryable_receipt(r) || a.plan.key.attempt_no >= u.spec.max_attempts {
                    finish_unit(
                        &mut store,
                        &g,
                        index,
                        UnitOutcome::Failed,
                        code,
                        a.plan.key.attempt_no,
                    )?;
                    return Ok(());
                }
                if a.retry.is_none() {
                    if view.global_stopped || find_run(&view, &g.owner.run_id)?.budget_stopped {
                        finish_unit(
                            &mut store,
                            &g,
                            index,
                            UnitOutcome::BudgetStopped,
                            "Confirmed overrun stops all new sends",
                            a.plan.key.attempt_no,
                        )?;
                        return Ok(());
                    }
                    if OffsetDateTime::now_utc() >= utc(&u.spec.deadline_utc)? {
                        finish_unit(
                            &mut store,
                            &g,
                            index,
                            UnitOutcome::Failed,
                            "Unit absolute deadline expired",
                            a.plan.key.attempt_no,
                        )?;
                        return Ok(());
                    }
                    let delay = g
                        .owner
                        .spec
                        .retry_base_delay_ms
                        .saturating_mul(1u64 << a.plan.key.attempt_no.saturating_sub(1))
                        .min(g.owner.spec.retry_max_delay_ms);
                    if OffsetDateTime::now_utc() + time::Duration::milliseconds(delay as i64)
                        >= utc(&u.spec.deadline_utc)?
                    {
                        finish_unit(
                            &mut store,
                            &g,
                            index,
                            UnitOutcome::Failed,
                            "No retry fits the original absolute deadline",
                            a.plan.key.attempt_no,
                        )?;
                        return Ok(());
                    }
                    store.schedule_retry_after(&a.plan.key, code)?;
                    boundary(
                        "retry_schedule_commit",
                        &cfg.unit_id,
                        a.plan.key.attempt_no,
                        &g.owner.run_id,
                        &g.root,
                    )?;
                    continue;
                }
            }
            if let Some(schedule) = &a.retry {
                if OffsetDateTime::now_utc() >= utc(&u.spec.deadline_utc)? {
                    finish_unit(
                        &mut store,
                        &g,
                        index,
                        UnitOutcome::Failed,
                        "Unit absolute deadline expired",
                        a.plan.key.attempt_no,
                    )?;
                    return Ok(());
                }
                if OffsetDateTime::now_utc() < utc(&schedule.retry_at_utc)? {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
            }
        }
        if OffsetDateTime::now_utc() >= utc(&u.spec.deadline_utc)?
            || OffsetDateTime::now_utc() >= utc(&g.owner.spec.deadline_utc)?
        {
            finish_unit(
                &mut store,
                &g,
                index,
                UnitOutcome::Failed,
                "Original absolute deadline expired; no new request",
                last.as_ref().map_or(0, |a| a.plan.key.attempt_no),
            )?;
            return Ok(());
        }
        let prepared = last
            .as_ref()
            .filter(|a| matches!(a.state, AttemptState::Prepared | AttemptState::Reserved));
        let attempt_no = prepared.map_or(
            last.as_ref().map_or(1, |a| a.plan.key.attempt_no + 1),
            |a| a.plan.key.attempt_no,
        );
        if attempt_no > u.spec.max_attempts {
            finish_unit(
                &mut store,
                &g,
                index,
                UnitOutcome::Failed,
                "Unit attempt cap exhausted",
                attempt_no - 1,
            )?;
            return Ok(());
        }
        let output = prepared.map_or_else(
            || {
                g.root
                    .join("attempts")
                    .join(&cfg.unit_id)
                    .join(format!("attempt-{attempt_no:04}"))
            },
            |a| g.root.join(&a.plan.attempt_root),
        );
        let claimed = g.claim(&output)?;
        let mut hooks = Hooks {
            guard: g.clone(),
            index,
            attempt_no,
            store: &mut store,
            plan: prepared.map(|a| a.plan.clone()),
            stop: stop.clone(),
            denial: None,
        };
        let execution = if let Some(a) = prepared {
            extraction_api::resume_prepared_with_hooks(
                Path::new(&cfg.project),
                &cfg.import_id,
                Path::new(&cfg.api_config),
                &output,
                None,
                a,
                find_run(&view, &g.owner.run_id)?,
                &mut hooks,
            )
        } else {
            if project::exists(&output)? {
                return Err(invalid("未登记attempt目录不能按名称收养。"));
            }
            extraction_api::run_with_hooks(
                Path::new(&cfg.project),
                &cfg.import_id,
                Path::new(&cfg.api_config),
                &output,
                None,
                &mut hooks,
            )
        };
        let denial = hooks.denial.take();
        let finished_key = hooks.plan.as_ref().map(|p| p.key.clone());
        drop(hooks);
        drop(claimed);
        // A guard/output failure is not a pure budget denial. The outer joined
        // worker path performs only safe accounting, releases never-sent plans
        // and invalidates changed inputs before returning this real error.
        if let Err(e) = execution {
            return Err(e);
        }
        if let Some(scope) = denial {
            let outcome = if matches!(
                scope,
                DenialScope::UnitAttempts | DenialScope::UnitRetryTokens | DenialScope::Deadline
            ) {
                UnitOutcome::Failed
            } else {
                UnitOutcome::BudgetStopped
            };
            finish_unit(
                &mut store,
                &g,
                index,
                outcome,
                "Bounded request admission denied",
                attempt_no,
            )?;
            return Ok(());
        }
        // Revalidate the authoritative receipt in a new complete transaction.
        // Its guard and input checks precede the transaction's UTC sample;
        // the next loop's read-only view is not a retry admission snapshot.
        let key =
            finished_key.ok_or_else(|| invalid("API exited without a Prepared identity。"))?;
        let decision = store.schedule_receipt_retry_checked(
            &key,
            |view| {
                g.verify()?;
                g.inventory(view)?;
                g.inputs()
            },
            || stop.load(Ordering::SeqCst),
        )?;
        match decision {
            ReceiptRetryDecision::Unchanged => {}
            ReceiptRetryDecision::Scheduled(_) => boundary(
                "retry_schedule_commit",
                &cfg.unit_id,
                key.attempt_no,
                &g.owner.run_id,
                &g.root,
            )?,
            ReceiptRetryDecision::Finish {
                outcome,
                diagnostic,
            } => {
                finish_unit(&mut store, &g, index, outcome, &diagnostic, key.attempt_no)?;
                return Ok(());
            }
        }
    }
}
struct Hooks<'a> {
    guard: Arc<RunGuard>,
    index: usize,
    attempt_no: u32,
    store: &'a mut BudgetStore,
    plan: Option<AttemptPlan>,
    stop: Arc<AtomicBool>,
    denial: Option<DenialScope>,
}
impl Hooks<'_> {
    fn guard(&self) -> Result<(), ProjectError> {
        Self::guard_view(&self.guard, &self.stop, &self.store.view()?)
    }
    fn guard_view(
        guard: &RunGuard,
        stop: &AtomicBool,
        view: &BudgetView,
    ) -> Result<(), ProjectError> {
        guard.verify()?;
        guard.inventory(view)?;
        guard.inputs()?;
        if stop.load(Ordering::SeqCst) {
            return Err(err("REQUEST_RUN_STOPPED", "其他worker要求停止新请求。"));
        }
        Ok(())
    }
    fn key(&self) -> Result<&AttemptKey, ProjectError> {
        self.plan
            .as_ref()
            .map(|p| &p.key)
            .ok_or_else(|| invalid("请求未Prepared。"))
    }
    fn event(&self, name: &str) -> Result<(), ProjectError> {
        boundary(
            name,
            &self.guard.owner.config.units[self.index].unit_id,
            self.attempt_no,
            &self.guard.owner.run_id,
            &self.guard.root,
        )
    }
}
impl ApiHooks for Hooks<'_> {
    fn enabled(&self) -> bool {
        true
    }
    fn selected_config(
        &self,
        path: &Path,
        fingerprint: &crate::sidecar_model::Fingerprint,
    ) -> Result<(), ProjectError> {
        let expected = self
            .guard
            .owner
            .spec
            .input_pins
            .get(&path.to_string_lossy().into_owned())
            .ok_or_else(|| invalid("API配置未在原输入pins中。"))?;
        if expected.sha256 != fingerprint.sha256
            || expected.byte_length != fingerprint.byte_length
            || stamp(path)?.0 != *expected
        {
            return Err(err(
                "REQUEST_RUN_INPUT_CHANGED",
                "读取人工凭据前原API配置变化。",
            ));
        }
        self.guard()
    }
    fn output_directories(&self) -> BTreeMap<PathBuf, Identity> {
        let root = &self.guard.root;
        let o = &self.guard.owner;
        let mut ids = BTreeMap::from([
            (root.clone(), o.root_identity.clone()),
            (root.join("attempts"), o.attempts_identity.clone()),
        ]);
        if let Some(parent) = root.parent() {
            ids.insert(parent.to_path_buf(), o.parent_identity.clone());
        }
        let unit = root
            .join("attempts")
            .join(&o.config.units[self.index].unit_id);
        if let Some(id) = o.unit_directories.get(&unit.to_string_lossy().into_owned()) {
            ids.insert(unit, id.clone());
        }
        ids
    }
    fn timeout_ms(&self, default: u64) -> u64 {
        let at = OffsetDateTime::now_utc();
        let end = utc(&self.guard.owner.spec.deadline_utc)
            .ok()
            .zip(utc(&self.guard.owner.spec.units[self.index].deadline_utc).ok())
            .map(|(a, b)| a.min(b));
        end.map_or(0, |e| {
            if e <= at {
                0
            } else {
                u64::try_from((e - at).whole_milliseconds())
                    .unwrap_or(0)
                    .min(default)
            }
        })
    }
    fn prepared(&mut self, output: &Path, request: &Value) -> Result<(), ProjectError> {
        self.guard()?;
        let body = owned_stamp(&output.join("request-body.json"))?.0;
        let metadata = owned_stamp(&output.join("request.json"))?.0;
        let relative = output
            .strip_prefix(&self.guard.root)
            .map_err(|_| invalid("请求输出越界。"))?
            .to_string_lossy()
            .replace('\\', "/");
        let plan = AttemptPlan {
            key: AttemptKey {
                run_id: self.guard.owner.run_id.clone(),
                unit_id: self.guard.owner.config.units[self.index].unit_id.clone(),
                attempt_no: self.attempt_no,
                request_id: request["request_id"]
                    .as_str()
                    .ok_or_else(|| invalid("API请求UUID缺失。"))?
                    .into(),
            },
            attempt_root: relative,
            request_body: FilePin {
                sha256: body.sha256,
                byte_length: body.byte_length,
            },
            request_metadata: FilePin {
                sha256: metadata.sha256,
                byte_length: metadata.byte_length,
            },
            input_reserved_tokens: request["input_reserved_tokens"]
                .as_u64()
                .ok_or_else(|| invalid("输入预留缺失。"))?,
            max_output_tokens: request["limits"]["max_output_tokens"]
                .as_u64()
                .ok_or_else(|| invalid("输出上限缺失。"))?,
            reserved_tokens: request["reserved_tokens"]
                .as_u64()
                .ok_or_else(|| invalid("R缺失。"))?,
            reservation_policy: RESERVATION_POLICY.into(),
        };
        if self.plan.as_ref().is_some_and(|old| old != &plan) {
            return Err(invalid("原Prepared计划被替换。"));
        }
        self.store.prepare(&plan)?;
        self.plan = Some(plan.clone());
        self.event("request_body_original_sync")?;
        loop {
            let guard = &self.guard;
            let stop = &self.stop;
            match self
                .store
                .reserve_prepared_checked(&plan, |view| Self::guard_view(guard, stop, view))?
            {
                ReservationDecision::Reserved => {
                    self.event("attempt_reserved_commit")?;
                    return Ok(());
                }
                ReservationDecision::AlreadyReserved => {
                    let v = self.store.view()?;
                    let a = unit_view(&v, &self.guard, self.index)?
                        .attempts
                        .iter()
                        .find(|a| a.plan.key == plan.key)
                        .ok_or_else(|| invalid("Prepared消失。"))?;
                    if a.state != AttemptState::Reserved {
                        return Err(invalid("只有原Reserved可继续发送检查。"));
                    }
                    return Ok(());
                }
                ReservationDecision::Denied { scope, .. } => {
                    let v = self.store.view()?;
                    let own = find_run(&v, &self.guard.owner.run_id)?;
                    let expired = OffsetDateTime::now_utc() >= utc(&own.spec.deadline_utc)?
                        || OffsetDateTime::now_utc()
                            >= utc(&own.units[self.index].spec.deadline_utc)?;
                    let refundable = v
                        .runs
                        .iter()
                        .flat_map(|r| &r.units)
                        .flat_map(|u| &u.attempts)
                        .any(|a| {
                            a.plan.key != plan.key
                                && matches!(a.state, AttemptState::Reserved | AttemptState::MaySend)
                        });
                    if matches!(scope, DenialScope::Global | DenialScope::Run)
                        && refundable
                        && !expired
                    {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    self.denial = Some(if expired {
                        DenialScope::Deadline
                    } else {
                        scope
                    });
                    return Err(err(
                        "REQUEST_RUN_BUDGET_STOP",
                        "准入拒绝；未发送并保留Prepared审计。",
                    ));
                }
            }
        }
    }
    fn before_send(&mut self, _output: &Path, _request: &Value) -> Result<(), ProjectError> {
        let key = self.key()?.clone();
        let guard = &self.guard;
        let stop = &self.stop;
        match self
            .store
            .mark_may_send_checked(&key, |view| Self::guard_view(guard, stop, view))?
        {
            DispatchDecision::Permitted => self.event("dispatch_intent_commit"),
            DispatchDecision::Denied => {
                let v = self.store.view()?;
                let r = find_run(&v, &self.guard.owner.run_id)?;
                self.denial = Some(if v.global_stopped || r.budget_stopped {
                    DenialScope::Terminal
                } else {
                    DenialScope::Deadline
                });
                Err(err(
                    "REQUEST_RUN_BUDGET_STOP",
                    "原期限/预算已停止，未取得发送许可。",
                ))
            }
            DispatchDecision::AlreadyMaySend => {
                Err(invalid("发送ticket已使用或状态不允许，不再次HTTP。"))
            }
        }
    }
    fn transport_saved(
        &mut self,
        _output: &Path,
        _receipt: &TransportReceipt,
    ) -> Result<(), ProjectError> {
        let key = self.key()?.clone();
        self.store.seal_transport(&key)?;
        self.event("response_original_sync")
    }
    fn finished(&mut self, output: &Path, report: &Value) -> Result<(), ProjectError> {
        self.guard.verify()?;
        let key = self.key()?.clone();
        let r = receipt(output, report, false)?;
        self.store.settle(&key, &r)?;
        self.event("attempt_settled_commit")
    }
}
fn receipt(output: &Path, report: &Value, recovered: bool) -> Result<AttemptReceipt, ProjectError> {
    let report_path = output.join(if recovered {
        "recovery-validation.json"
    } else {
        "validation.json"
    });
    let report_pin = owned_stamp(&report_path)?.0;
    let response = if project::exists(&output.join("response.bin"))? {
        let s = owned_stamp(&output.join("response.bin"))?.0;
        Some(FilePin {
            sha256: s.sha256,
            byte_length: s.byte_length,
        })
    } else {
        None
    };
    let usage = &report["usage"]["model_usage"];
    let usage = if usage.is_null() {
        None
    } else {
        Some(TokenUsage {
            input_tokens: usage["input_tokens"]
                .as_u64()
                .ok_or_else(|| invalid("实际input usage无效。"))?,
            output_tokens: usage["output_tokens"]
                .as_u64()
                .ok_or_else(|| invalid("实际output usage无效。"))?,
            total_tokens: usage["total_tokens"]
                .as_u64()
                .ok_or_else(|| invalid("实际total usage无效。"))?,
        })
    };
    let transport = if project::exists(&output.join("transport-receipt.json"))? {
        Some(decoded::<TransportReceipt>(
            &owned_stamp(&output.join("transport-receipt.json"))?.1,
        )?)
    } else {
        None
    };
    Ok(AttemptReceipt {
        report: FilePin {
            sha256: report_pin.sha256,
            byte_length: report_pin.byte_length,
        },
        response,
        network_attempts: u32::try_from(
            report["usage"]["network_attempts"]
                .as_u64()
                .ok_or_else(|| invalid("网络次数缺失。"))?,
        )
        .map_err(|_| invalid("网络次数超限。"))?,
        http_status: report["http_status"]
            .as_u64()
            .map(u16::try_from)
            .transpose()
            .map_err(|_| invalid("HTTP超限。"))?,
        response_complete: report["response_complete"]
            .as_bool()
            .ok_or_else(|| invalid("framing状态缺失。"))?,
        response_id: report["response_id"].as_str().map(str::to_owned),
        http_request_id: report["http_request_id"].as_str().map(str::to_owned),
        usage,
        candidate_status: if report["status"] == "candidates_validated" {
            CandidateStatus::Validated
        } else {
            CandidateStatus::Failed
        },
        error_code: report["errors"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v["code"].as_str())
            .map(str::to_owned),
        elapsed_ms: transport.map_or(0, |t| t.elapsed_ms),
        price_usd: None,
    })
}
fn boundary(
    event: &str,
    unit: &str,
    attempt: u32,
    run_id: &str,
    root: &Path,
) -> Result<(), ProjectError> {
    #[cfg(test)]
    {
        return tests::boundary(event, unit, attempt, run_id, root);
    }
    #[cfg(not(test))]
    {
        let _ = (event, unit, attempt, run_id, root);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env,
        io::{self, BufRead},
        sync::{Mutex, OnceLock},
    };
    static HANDSHAKE: OnceLock<Mutex<()>> = OnceLock::new();
    pub(super) fn boundary(
        event: &str,
        unit: &str,
        attempt: u32,
        run_id: &str,
        root: &Path,
    ) -> Result<(), ProjectError> {
        if env::var_os("NOETOME_REQUEST_TEST_MODE").is_none() {
            return Ok(());
        }
        let selected = env::var("NOETOME_REQUEST_TEST_EVENT").unwrap_or_else(|_| "trace".into());
        let selected_unit = env::var("NOETOME_REQUEST_TEST_UNIT").unwrap_or_default();
        let selected_attempt = env::var("NOETOME_REQUEST_TEST_ATTEMPT")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        let matched = selected == "trace"
            || (selected == event
                && (selected_unit.is_empty() || selected_unit == unit)
                && (selected_attempt == 0 || selected_attempt == attempt));
        if !matched && selected != "observe" {
            return Ok(());
        }
        let _held = HANDSHAKE
            .get_or_init(|| Mutex::new(()))
            .lock()
            .map_err(|_| invalid("测试握手锁损坏。"))?;
        let budget = if event == "global_authority_created_commit" {
            root.to_path_buf()
        } else {
            PathBuf::from(
                env::var_os("NOETOME_REQUEST_TEST_BUDGET_ROOT")
                    .ok_or_else(|| invalid("测试预算根缺失。"))?,
            )
        };
        let view = BudgetStore::open_readonly(&budget)?.view()?;
        let line = json!({"producer":"request_run_checkpoint_test_handshake","event":event,"unit":unit,"attempt":attempt,
            "run_id":run_id,"pid":std::process::id(),"compiler_fingerprint":env!("NOETOME_RUN_COMPILER_FINGERPRINT"),
            "thread_id":format!("{:?}",thread::current().id()),"sequence":view.sequence,"authority_id":view.authority.authority_id,
            "root":root,"will_wait":matched});
        let bytes = serde_json::to_vec(&line).map_err(|_| invalid("握手JSON失败。"))?;
        {
            let mut out = io::stdout().lock();
            out.write_all(&bytes)
                .and_then(|_| out.write_all(b"\n"))
                .and_then(|_| out.flush())
                .map_err(|e| invalid(e.to_string()))?;
        }
        if selected != "trace" {
            drop(_held);
        }
        if matched {
            if let Some(path) = env::var_os("NOETOME_REQUEST_TEST_READY_PATH") {
                let path = PathBuf::from(path);
                if !path.is_absolute() || overlap(&path, root) || overlap(&path, &budget) {
                    return Err(invalid("测试ready必须位于两个根之外的明确新文件。"));
                }
                if !project::exists(&path)? {
                    write_new(&path, &bytes)?;
                }
            }
            let mut release = String::new();
            io::stdin()
                .lock()
                .read_line(&mut release)
                .map_err(|e| invalid(e.to_string()))?;
            if release.trim_end_matches(['\r', '\n']) != "release" {
                return Err(err(
                    "REQUEST_RUN_TEST_HANDSHAKE",
                    "只有外部精确release才继续真实边界。",
                ));
            }
        }
        Ok(())
    }
    #[test]
    fn checkpoint_driver() {
        let Some(mode) = env::var_os("NOETOME_REQUEST_TEST_MODE") else {
            return;
        };
        let path =
            |name: &str| PathBuf::from(env::var_os(name).expect("explicit selected test path"));
        let budget = path("NOETOME_REQUEST_TEST_BUDGET_ROOT");
        let result = match mode.to_str().expect("UTF8 mode") {
            "init" => super::init(&path("NOETOME_REQUEST_TEST_CONFIG"), &budget),
            "start" | "start_paused" => super::start(
                &path("NOETOME_REQUEST_TEST_CONFIG"),
                &budget,
                &path("NOETOME_REQUEST_TEST_RUN_ROOT"),
                mode == "start_paused",
            ),
            "resume" => super::resume(
                &path("NOETOME_REQUEST_TEST_CONFIG"),
                &budget,
                &path("NOETOME_REQUEST_TEST_RUN_ROOT"),
            ),
            "status" => super::status(&budget, &path("NOETOME_REQUEST_TEST_RUN_ROOT")),
            _ => Err(err("REQUEST_RUN_TEST_MODE", "未知显式driver模式。")),
        };
        let value = match result {
            Ok(out) => {
                json!({"producer":"request_run_test_driver_result","pid":std::process::id(),"compiler_fingerprint":env!("NOETOME_RUN_COMPILER_FINGERPRINT"),"result":serde_json::from_str::<Value>(&out.json).expect("actual report JSON"),"failure":out.failure})
            }
            Err(e) => {
                json!({"producer":"request_run_test_driver_result","pid":std::process::id(),"compiler_fingerprint":env!("NOETOME_RUN_COMPILER_FINGERPRINT"),"error_code":e.code(),"exit_code":e.exit_code()})
            }
        };
        let mut out = io::stdout().lock();
        writeln!(out, "{value}")
            .and_then(|_| out.flush())
            .expect("actual driver stdout");
    }
    #[test]
    fn request_run_retry_classes_do_not_expand_to_wire_or_candidates() {
        for code in [
            "EXTRACT_API_HTTP_RATE_LIMIT",
            "EXTRACT_API_HTTP_SERVER",
            "EXTRACT_TIMEOUT",
            "EXTRACT_API_TRANSPORT",
            "EXTRACT_API_TRUNCATED",
        ] {
            assert!(retryable(code));
        }
        for code in [
            "EXTRACT_API_CANDIDATES",
            "EXTRACT_API_OUTPUT",
            "EXTRACT_API_HTTP_AUTH",
            "REQUEST_INTERRUPTED",
            "EXTRACT_API_RESPONSE",
        ] {
            assert!(!retryable(code));
        }
    }
    #[test]
    fn request_run_unknown_project_entry_is_rejected_before_read() {
        let base = std::env::temp_dir().join(format!(
            "noetome-request-inventory-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&base).unwrap();
        let unknown = base.join("unselected-private.bin");
        fs::write(&unknown, b"not source material").unwrap();
        #[cfg(windows)]
        let held = {
            use std::os::windows::fs::OpenOptionsExt;
            OpenOptions::new()
                .write(true)
                .share_mode(0)
                .open(&unknown)
                .unwrap()
        };
        let result = files(&base, &mut BTreeMap::new(), &mut BTreeMap::new());
        assert_eq!(result.unwrap_err().code(), "REQUEST_RUN_SOURCE_INVENTORY");
        #[cfg(windows)]
        drop(held);
        fs::remove_dir_all(base).unwrap();
    }
}
