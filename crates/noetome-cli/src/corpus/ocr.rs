//! A pinned, independently installed native OCR runtime. The application owns
//! durable page progress; the third-party worker owns recognition only.
use super::*;
use process_wrap::std::*;
use std::{
    path::PathBuf,
    process::Stdio,
    thread,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Profile {
    schema_version: u32,
    engine: String,
    engine_version: String,
    adapter_version: String,
    python: PathBuf,
    worker: PathBuf,
    runtime_root: PathBuf,
    model_receipt: PathBuf,
    model_receipt_sha256: String,
    worker_sha256: String,
    render_sha256: String,
    render_dpi: u32,
    tier: String,
    timeout_seconds: u64,
    inference_verified: bool,
}
pub(super) struct Runtime {
    profile: Profile,
    profile_sha: String,
}

fn profile_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("NOETOME_CORPUS_OCR_PROFILE") {
        return Some(PathBuf::from(path));
    }
    #[cfg(windows)]
    {
        return std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("Noetome/runtimes/corpus-ocr/mineru-4.2.1/runtime.json"));
    }
    #[cfg(not(windows))]
    None
}
fn checked_bytes(path: &Path, max: u64) -> Result<Vec<u8>, ProjectError> {
    project::check_components(path)?;
    project::checked_file(path)?;
    let mut bytes = vec![];
    project::read_guard(path)?
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io(path, e))?;
    if bytes.len() as u64 > max {
        return Err(error("OCR_PROFILE_LIMIT", "OCR配置或适配器超出读取上限。"));
    }
    Ok(bytes)
}
pub(super) fn runtime() -> Result<Option<Runtime>, ProjectError> {
    let Some(path) = profile_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    };
    let raw = checked_bytes(&path, 64 * 1024)?;
    let profile: Profile =
        serde_json::from_slice(raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&raw))
            .map_err(|e| error("OCR_PROFILE_INVALID", e.to_string()))?;
    if profile.schema_version != 1
        || profile.engine != "mineru"
        || profile.engine_version != "4.2.1"
        || profile.adapter_version != "noetome-corpus-ocr-v1"
        || profile.tier != "standard"
        || !(72..=300).contains(&profile.render_dpi)
        || !(60..=1800).contains(&profile.timeout_seconds)
    {
        return Err(error(
            "OCR_PROFILE_INVALID",
            "OCR运行时版本、策略或限制不受支持。",
        ));
    }
    if !profile.inference_verified {
        return Ok(None);
    };
    for (file, hash) in [
        (&profile.worker, &profile.worker_sha256),
        (
            &profile.worker.with_file_name("render.py"),
            &profile.render_sha256,
        ),
        (&profile.model_receipt, &profile.model_receipt_sha256),
    ] {
        if crate::text::sha256(&checked_bytes(file, 1024 * 1024)?) != *hash {
            return Err(error(
                "OCR_RUNTIME_CHANGED",
                "OCR程序或模型清单摘要不匹配，请重新核对运行时。",
            ));
        }
    }
    project::check_components(&profile.python)?;
    project::checked_file(&profile.python)?;
    project::check_components(&profile.runtime_root)?;
    Ok(Some(Runtime {
        profile,
        profile_sha: crate::text::sha256(&raw),
    }))
}
pub(super) fn capabilities() -> Value {
    match runtime() {
        Ok(Some(runtime)) => {
            json!({"available":true,"engine":"mineru","engine_version":"4.2.1","page_timeout_seconds":runtime.profile.timeout_seconds,"render_dpi":runtime.profile.render_dpi,"quality_status":"needs_independent_review"})
        }
        Ok(None) => {
            json!({"available":false,"message":"尚无已实测可运行的本机OCR配置；扫描页保留待处理。"})
        }
        Err(e) => json!({"available":false,"error":{"code":e.code(),"message":e.to_string()}}),
    }
}
impl Runtime {
    pub fn page(
        &self,
        raw: &Path,
        source_sha: &str,
        page_number: u32,
        geometry: &Value,
        parent: &Path,
        mut cancelled: impl FnMut() -> Result<bool, ProjectError>,
    ) -> Result<Option<Value>, ProjectError> {
        let job = parent.join(format!("page-{page_number:06}-{}", Uuid::new_v4()));
        project::check_components(&job)?;
        fs::create_dir(&job).map_err(|e| io(&job, e))?;
        let output = job.join("output");
        let request = job.join("request.json");
        let document = json!({"schema_version":1,"pdf_path":raw,"source_sha256":source_sha,"page_number":page_number,"geometry":geometry,"render_dpi":self.profile.render_dpi});
        fs::write(&request, document.to_string()).map_err(|e| io(&request, e))?;
        let outpath = job.join("stdout.log");
        let errpath = job.join("stderr.log");
        let stdout = fs::File::create(&outpath).map_err(|e| io(&outpath, e))?;
        let stderr = fs::File::create(&errpath).map_err(|e| io(&errpath, e))?;
        let mut command = CommandWrap::with_new(&self.profile.python, |c| {
            c.args(["-I", "-B", "-X", "utf8"])
                .arg(&self.profile.worker)
                .arg("--request")
                .arg(&request)
                .arg("--output")
                .arg(&output)
                .arg("--runtime")
                .arg(&self.profile.runtime_root)
                .stdin(Stdio::null())
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr));
        });
        #[cfg(windows)]
        let _host_job = super::process_host::guard(&mut command)
            .map_err(|e| error("OCR_WORKER_START", format!("无法保护OCR进程：{e}")))?;
        let mut child = command
            .spawn()
            .map_err(|e| error("OCR_WORKER_START", e.to_string()))?;
        let begin = Instant::now();
        let mut failure = None;
        loop {
            let stop = cancelled();
            if !matches!(stop, Ok(false)) {
                let _ = child.kill();
                let _ = child.wait();
                stop?;
                return Ok(None);
            }
            if let Some(status) = child
                .try_wait()
                .map_err(|e| error("OCR_WORKER_WAIT", e.to_string()))?
            {
                if !status.success() {
                    failure = Some(format!("OCR进程退出：{status}"));
                }
                break;
            }
            if begin.elapsed() > Duration::from_secs(self.profile.timeout_seconds) {
                let _ = child.kill();
                let _ = child.wait();
                failure = Some(format!(
                    "本页OCR超过{}秒；可以单页重试。",
                    self.profile.timeout_seconds
                ));
                break;
            }
            if [&outpath, &errpath]
                .iter()
                .any(|p| fs::metadata(p).is_ok_and(|m| m.len() > 2 * 1024 * 1024))
            {
                let _ = child.kill();
                let _ = child.wait();
                failure = Some("OCR诊断超过2MiB，已停止本页。".into());
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        let result = output.join("result.json");
        let mut page = if result.exists() {
            serde_json::from_slice::<Value>(&checked_bytes(&result, MAX_PAGE_BYTES)?)
                .map_err(|e| error("OCR_PAGE_JSON", e.to_string()))?
        } else {
            json!({"schema_version":1,"page_number":page_number,"source_sha256":source_sha,"status":"failed","text":"","regions":[],"width_pt":geometry["width_pt"],"height_pt":geometry["height_pt"],"rotation":geometry["rotation"],"geometry":geometry,"issues":[{"code":"OCR_WORKER_FAILED","message":"识别器没有返回页面结果。"}],"parser":{"engine":"mineru","engine_version":"4.2.1","adapter_version":"noetome-corpus-ocr-v1"}})
        };
        validate_result(&mut page, source_sha, page_number, geometry, failure)?;
        page["ocr_profile_sha256"] = json!(self.profile_sha);
        page["artifact_root"] = json!(output.to_string_lossy());
        Ok(Some(page))
    }
}

fn validate_result(
    page: &mut Value,
    source_sha: &str,
    page_number: u32,
    geometry: &Value,
    failure: Option<String>,
) -> Result<(), ProjectError> {
    if page["page_number"].as_u64() != Some(u64::from(page_number))
        || page["geometry"] != *geometry
        || page["source_sha256"].as_str() != Some(source_sha)
    {
        return Err(error(
            "OCR_PAGE_SOURCE",
            "OCR结果的原件、页号或坐标变换不匹配。",
        ));
    }
    if let Some(message) = failure {
        page["status"] = json!("failed");
        page["issues"]
            .as_array_mut()
            .ok_or_else(|| error("OCR_PAGE_JSON", "OCR结果缺少问题清单。"))?
            .push(json!({"code":"OCR_WORKER_FAILED","message":message}));
    }
    if !matches!(
        page["status"].as_str(),
        Some("ocr_ready" | "needs_review" | "failed")
    ) {
        return Err(error(
            "OCR_PAGE_STATUS",
            "识别器返回了非终止页状态，已停止本次识别。",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recognized() -> Value {
        json!({"page_number":1,"source_sha256":"source","geometry":{"width_pt":200,"height_pt":300},"status":"ocr_ready","issues":[]})
    }
    #[test]
    fn missing_source_or_wrong_geometry_cannot_confirm_recognition() {
        let good = recognized();
        for field in ["source_sha256", "page_number", "geometry"] {
            let mut changed = good.clone();
            changed.as_object_mut().unwrap().remove(field);
            assert_eq!(
                validate_result(&mut changed, "source", 1, &good["geometry"], None)
                    .unwrap_err()
                    .code(),
                "OCR_PAGE_SOURCE"
            );
        }
    }
    #[test]
    fn unsuccessful_worker_cannot_hide_behind_a_ready_result() {
        let mut page = recognized();
        let geometry = page["geometry"].clone();
        validate_result(
            &mut page,
            "source",
            1,
            &geometry,
            Some("Synthetic process exit".into()),
        )
        .unwrap();
        assert_eq!(page["status"], "failed");
        assert_eq!(page["issues"][0]["code"], "OCR_WORKER_FAILED");
    }
    #[test]
    fn pending_ocr_output_cannot_repeat_forever_in_one_process() {
        let mut page = recognized();
        let geometry = page["geometry"].clone();
        page["status"] = json!("needs_ocr");
        assert_eq!(
            validate_result(&mut page, "source", 1, &geometry, None)
                .unwrap_err()
                .code(),
            "OCR_PAGE_STATUS"
        );
    }
}
