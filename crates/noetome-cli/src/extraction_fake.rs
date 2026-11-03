//! One offline replay provider. No model, network, script or subprocess runs here.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use crate::extraction_model::{ExtractionRequest, FakeConfig};
use crate::fixture_input::valid_relative_path;
use crate::project::{self, ProjectError};
use crate::sidecar_process::Control;
use crate::sidecar_workspace::{Directory, GuardedFile};

const CASE_FILES: usize = 64;
const CASE_BYTES: u64 = 4 * 1024 * 1024;
const FILE_BYTES: u64 = 1024 * 1024;

fn error(code: &'static str, exit: u8, message: &str) -> ProjectError {
    ProjectError::new(code, exit, message)
}

fn changed() -> ProjectError {
    error(
        "EXTRACT_INPUT_CHANGED",
        3,
        "fake 输入的身份、字节、mtime 或目录清单发生变化。",
    )
}

#[derive(Clone, PartialEq, Eq)]
struct Entry {
    directory: bool,
    bytes: u64,
    modified: SystemTime,
}

fn tree(
    root: &Path,
    path: &Path,
    entries: &mut BTreeMap<PathBuf, Entry>,
) -> Result<(), ProjectError> {
    if entries.len() > CASE_FILES + 16 {
        return Err(error(
            "EXTRACT_CONFIG_LIMIT",
            2,
            "fake 输入目录超过文件/目录数量上限。",
        ));
    }
    project::check_components(path)?;
    let meta = fs::symlink_metadata(path).map_err(|e| ProjectError::io(path, e))?;
    if project::is_link(&meta) || (!meta.is_file() && !meta.is_dir()) {
        return Err(ProjectError::unsafe_path(path));
    }
    if path != root {
        entries.insert(
            path.strip_prefix(root)
                .map_err(|_| changed())?
                .to_path_buf(),
            Entry {
                directory: meta.is_dir(),
                bytes: if meta.is_file() { meta.len() } else { 0 },
                modified: meta.modified().map_err(|e| ProjectError::io(path, e))?,
            },
        );
    }
    if meta.is_dir() {
        for child in fs::read_dir(path).map_err(|e| ProjectError::io(path, e))? {
            tree(
                root,
                &child.map_err(|e| ProjectError::io(path, e))?.path(),
                entries,
            )?;
        }
    }
    Ok(())
}

pub(crate) struct FakeInput {
    root: PathBuf,
    directories: Vec<Directory>,
    guards: Vec<GuardedFile>,
    entries: BTreeMap<PathBuf, Entry>,
    response: Vec<u8>,
    pub(crate) config: FakeConfig,
}

impl FakeInput {
    pub(crate) fn open(path: &Path) -> Result<Self, ProjectError> {
        let root = project::check_components(path)?;
        let first = Directory::capture(&root, "EXTRACT_INPUT_CHANGED").map_err(|_| changed())?;
        let mut entries = BTreeMap::new();
        tree(&root, &root, &mut entries)?;
        let files: Vec<_> = entries
            .iter()
            .filter(|(_, entry)| !entry.directory)
            .collect();
        if entries.values().filter(|entry| entry.directory).count() > 16
            || files.len() > CASE_FILES
            || files.iter().any(|(_, e)| e.bytes > FILE_BYTES)
            || files
                .iter()
                .try_fold(0u64, |sum, (_, e)| sum.checked_add(e.bytes))
                .is_none_or(|sum| sum > CASE_BYTES)
        {
            return Err(error(
                "EXTRACT_CONFIG_LIMIT",
                2,
                "fake 输入最多64个文件、每文件1 MiB、总计4 MiB。",
            ));
        }
        let control = Control::new(30000, Default::default());
        let mut directories = vec![first];
        let mut guards = Vec::new();
        let mut bytes = BTreeMap::new();
        for (relative, entry) in &entries {
            let path = root.join(relative);
            if entry.directory {
                directories.push(
                    Directory::capture(&path, "EXTRACT_INPUT_CHANGED").map_err(|_| changed())?,
                );
            } else {
                let (guard, data) =
                    GuardedFile::open(&path, FILE_BYTES, &control, "EXTRACT_INPUT_CHANGED")
                        .map_err(|_| changed())?;
                guards.push(guard);
                bytes.insert(relative.clone(), data);
            }
        }
        let config_bytes = bytes
            .get(Path::new("fake.json"))
            .ok_or_else(|| error("EXTRACT_CONFIG", 2, "fake.json 缺失。"))?;
        if config_bytes.len() > 65536 {
            return Err(error("EXTRACT_CONFIG_LIMIT", 2, "fake.json 超过64 KiB。"));
        }
        let config: FakeConfig = serde_json::from_slice(config_bytes).map_err(|_| {
            error(
                "EXTRACT_CONFIG",
                2,
                "fake.json 不符合严格可版本化配置，未知/重复/缺失字段均拒绝。",
            )
        })?;
        validate_config(&config)?;
        let response = bytes
            .remove(Path::new(&config.response_file))
            .ok_or_else(|| {
                error(
                    "EXTRACT_CONFIG",
                    2,
                    "声明的 response_file 不存在于 fake 根内。",
                )
            })?;
        let value = Self {
            root,
            directories,
            guards,
            entries,
            response,
            config,
        };
        value.verify()?;
        Ok(value)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn verify(&self) -> Result<(), ProjectError> {
        let control = Control::new(30000, Default::default());
        for directory in &self.directories {
            directory
                .verify("EXTRACT_INPUT_CHANGED")
                .map_err(|_| changed())?;
        }
        for guard in &self.guards {
            guard
                .verify(&control, "EXTRACT_INPUT_CHANGED")
                .map_err(|_| changed())?;
        }
        let mut current = BTreeMap::new();
        tree(&self.root, &self.root, &mut current).map_err(|_| changed())?;
        if current != self.entries {
            return Err(changed());
        }
        for directory in &self.directories {
            directory
                .verify("EXTRACT_INPUT_CHANGED")
                .map_err(|_| changed())?;
        }
        Ok(())
    }

    pub(crate) fn replay(
        &self,
        request: &ExtractionRequest,
        control: &Control,
        cancelled: &AtomicBool,
        signal: Option<&Path>,
    ) -> Result<Vec<u8>, ProjectError> {
        let started = Instant::now();
        loop {
            check(control, cancelled, signal)?;
            let elapsed = started.elapsed();
            let delay = Duration::from_millis(self.config.delay_ms);
            if elapsed >= delay {
                break;
            }
            std::thread::sleep((delay - elapsed).min(Duration::from_millis(5)));
        }
        check(control, cancelled, signal)?;
        if self.response.iter().all(u8::is_ascii_whitespace) {
            return Err(error("EXTRACT_EMPTY_RESPONSE", 3, "fake 返回空响应。"));
        }
        let binding = self
            .config
            .binding_override
            .as_ref()
            .unwrap_or(&request.binding);
        let request_id = self
            .config
            .request_id_override
            .as_ref()
            .unwrap_or(&request.request_id);
        let prefix = format!(
            "{{\"schema_version\":1,\"producer\":\"fake\",\"request_id\":{},\"binding\":{},\"payload\":",
            serde_json::to_string(request_id).map_err(|_| error(
                "EXTRACT_ENCODING",
                3,
                "请求路由无法编码。"
            ))?,
            serde_json::to_string(binding).map_err(|_| error(
                "EXTRACT_ENCODING",
                3,
                "来源路由无法编码。"
            ))?
        );
        let length = prefix
            .len()
            .checked_add(self.response.len())
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| error("EXTRACT_RESPONSE_LIMIT", 3, "响应长度溢出。"))?;
        if length as u64 > request.limits.max_response_bytes {
            return Err(error(
                "EXTRACT_RESPONSE_LIMIT",
                3,
                "完整 fake 响应超过字节上限。",
            ));
        }
        let mut response = Vec::with_capacity(length);
        response.extend_from_slice(prefix.as_bytes());
        response.extend_from_slice(&self.response);
        response.push(b'}');
        check(control, cancelled, signal)?;
        Ok(response)
    }
}

fn validate_config(config: &FakeConfig) -> Result<(), ProjectError> {
    let limits = &config.limits;
    if config.schema_version != 1
        || !crate::fixture_model::is_id(&config.case_id)
        || config.selection.is_empty()
        || config.selection.len() > 64
        || !(1..=64).contains(&limits.max_units)
        || !(1..=1024 * 1024).contains(&limits.max_input_bytes)
        || !(1..=1024 * 1024).contains(&limits.max_response_bytes)
        || !(1..=65536).contains(&limits.max_output_tokens)
        || !(1..=30000).contains(&limits.timeout_ms)
        || config.delay_ms > 30000
        || config.budget.max_requests > 1
        || !valid_relative_path(&config.response_file)
        || config.budget.max_cost_usd.as_ref().is_some_and(|cost| {
            cost.len() > 64
                || cost.trim() != cost
                || !cost.parse::<f64>().is_ok_and(|n| n.is_finite() && n > 0.0)
        })
    {
        return Err(error(
            "EXTRACT_CONFIG",
            2,
            "fake 配置版本、选择、路径、单请求限制或金额格式无效。",
        ));
    }
    Ok(())
}

pub(crate) fn check(
    control: &Control,
    cancelled: &AtomicBool,
    signal: Option<&Path>,
) -> Result<(), ProjectError> {
    if let Some(path) = signal {
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.is_file() && !project::is_link(&meta) => {
                cancelled.store(true, Ordering::SeqCst)
            }
            Ok(_) => return Err(error("EXTRACT_CANCEL_PATH", 3, "取消信号必须为普通文件。")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(ProjectError::io(path, e)),
        }
    }
    control.check().map_err(|e| {
        error(
            if e.code == "SIDECAR_CANCELLED" {
                "EXTRACT_CANCELLED"
            } else {
                "EXTRACT_TIMEOUT"
            },
            3,
            if e.code == "SIDECAR_CANCELLED" {
                "请求已由取消信号停止。"
            } else {
                "请求已超过时间上限。"
            },
        )
    })
}
