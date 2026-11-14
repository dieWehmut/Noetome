use super::*;
use process_wrap::std::*;
use std::{
    path::PathBuf,
    process::Stdio,
    thread,
    time::{Duration, Instant},
};

const WORKER: &[u8] = include_bytes!("../../../../sidecars/corpus_pdf/worker.py");
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
const BATCH_TIMEOUT: Duration = Duration::from_secs(180);

fn runtime_path() -> Result<PathBuf, ProjectError> {
    if let Some(selected) = std::env::var_os("NOETOME_PDF_PYTHON") {
        let path = PathBuf::from(selected);
        if !path.is_absolute() {
            return Err(error(
                "PDF_RUNTIME",
                "Python解释器必须是明确选择的绝对路径。",
            ));
        }
        project::check_components(
            path.parent()
                .ok_or_else(|| error("PDF_RUNTIME", "解释器路径没有父目录。"))?,
        )?;
        // A venv's interpreter is commonly a symlink. Validate its selected
        // canonical executable while launching through the venv to retain its
        // pinned packages. Document paths retain the stricter no-link rule.
        let target = path.canonicalize().map_err(|e| io(&path, e))?;
        project::check_components(&target)?;
        project::checked_file(&target)?;
        return Ok(path);
    }
    crate::pdf_parse::runtime_path().map_err(|e| error("PDF_RUNTIME", e.to_string()))
}

struct Workspace {
    root: PathBuf,
}
impl Workspace {
    fn new() -> Result<Self, ProjectError> {
        let root = std::env::temp_dir().join(format!("noetome-corpus-{}", Uuid::new_v4()));
        project::check_components(&root)?;
        fs::create_dir(&root).map_err(|e| io(&root, e))?;
        fs::write(root.join("worker.py"), WORKER).map_err(|e| io(&root, e))?;
        Ok(Self { root })
    }
    fn read(&self, name: &str) -> Result<Value, ProjectError> {
        let path = self.root.join(name);
        let file = project::read_guard(&path)?;
        let mut bytes = vec![];
        file.take(MAX_PAGE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| io(&path, e))?;
        if bytes.len() as u64 > MAX_PAGE_BYTES {
            return Err(error(
                "CORPUS_PAGE_LIMIT",
                "解析器单页响应超过16MiB，没有截断保存。",
            ));
        }
        serde_json::from_slice(&bytes)
            .map_err(|e| error("CORPUS_WORKER_JSON", format!("解析器响应格式无效：{e}")))
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        // Private random scratch contains no user input, only embedded code and
        // worker outputs; never remove selected source or project directories.
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Exit {
    cancelled: bool,
    failure: Option<Failure>,
}
pub(super) struct PageRun {
    pub pages: Vec<Value>,
    pub cancelled: bool,
    pub failure: Option<Failure>,
}

fn run(
    work: &Workspace,
    input: &Path,
    verb: &str,
    range: Option<(u32, u32)>,
    mut cancelled: impl FnMut() -> Result<bool, ProjectError>,
) -> Result<Exit, ProjectError> {
    let runtime = runtime_path()?;
    let stdout_path = work.root.join("stdout.jsonl");
    let stderr_path = work.root.join("stderr.log");
    let stdout = fs::File::create(&stdout_path).map_err(|e| io(&stdout_path, e))?;
    let stderr = fs::File::create(&stderr_path).map_err(|e| io(&stderr_path, e))?;
    let mut command = CommandWrap::with_new(runtime, |c| {
        c.args(["-I", "-B", "-X", "utf8"])
            .arg(work.root.join("worker.py"))
            .arg(verb)
            .arg("--input")
            .arg(input)
            .arg("--output-dir")
            .arg(&work.root);
        if let Some((start, end)) = range {
            c.arg("--start-page")
                .arg(start.to_string())
                .arg("--end-page")
                .arg(end.to_string());
        }
        c.stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
    });
    #[cfg(windows)]
    let _host_job = super::process_host::guard(&mut command)
        .map_err(|e| error("CORPUS_WORKER_START", format!("无法保护PDF解析进程：{e}")))?;
    let mut child = command
        .spawn()
        .map_err(|e| error("CORPUS_WORKER_START", format!("无法启动PDF解析环境：{e}")))?;
    let begin = Instant::now();
    loop {
        let stop = cancelled();
        if !matches!(stop, Ok(false)) {
            let _ = child.kill();
            let _ = child.wait();
            stop?;
            return Ok(Exit {
                cancelled: true,
                failure: None,
            });
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|e| error("CORPUS_WORKER_WAIT", e.to_string()))?
        {
            if status.success() {
                return Ok(Exit {
                    cancelled: false,
                    failure: None,
                });
            }
            let diagnostic = read_log(&stderr_path);
            if let Some(failure) = structured_failure(&stdout_path) {
                return Ok(Exit {
                    cancelled: false,
                    failure: Some(failure),
                });
            }
            return Ok(Exit {
                cancelled: false,
                failure: Some(Failure {
                    code: "CORPUS_WORKER_EXIT".into(),
                    message: format!("PDF解析器退出{status}。{diagnostic}"),
                }),
            });
        }
        let overflow = [&stdout_path, &stderr_path]
            .iter()
            .any(|path| fs::metadata(path).is_ok_and(|m| m.len() > MAX_LOG_BYTES));
        if begin.elapsed() > BATCH_TIMEOUT || overflow {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(Exit {
                cancelled: false,
                failure: Some(Failure {
                    code: if overflow {
                        "CORPUS_WORKER_LOG_LIMIT"
                    } else {
                        "CORPUS_WORKER_TIMEOUT"
                    }
                    .into(),
                    message: if overflow {
                        "解析器诊断输出超过2MiB；已保留确认页。"
                    } else {
                        "本批解析超过180秒；已保留确认页，可以继续重试。"
                    }
                    .into(),
                }),
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
}
fn read_log(path: &Path) -> String {
    let mut bytes = vec![];
    if let Ok(file) = fs::File::open(path) {
        let _ = file.take(8192).read_to_end(&mut bytes);
    }
    String::from_utf8_lossy(&bytes).trim().to_owned()
}

fn structured_failure(path: &Path) -> Option<Failure> {
    let mut bytes = vec![];
    fs::File::open(path)
        .ok()?
        .take(MAX_LOG_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    String::from_utf8_lossy(&bytes)
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find_map(|event| {
            if event["event"] != "error" {
                return None;
            }
            Some(Failure {
                code: event["code"].as_str()?.to_owned(),
                message: event["message"].as_str()?.to_owned(),
            })
        })
}

pub(super) fn preflight(input: &Path) -> Result<Value, ProjectError> {
    let work = Workspace::new()?;
    let exit = run(&work, input, "preflight", None, || Ok(false))?;
    // Expected document errors are structured preflight records, even if the
    // command exits nonzero; a host failure still preserves its diagnostic.
    if work.root.join("preflight.json").exists() {
        return work.read("preflight.json").map(|mut value| {
            value["parser"]["adapter_sha256"] = json!(crate::text::sha256(WORKER));
            value
        });
    }
    let failure = exit.failure.unwrap_or(Failure {
        code: "CORPUS_WORKER_PROTOCOL".into(),
        message: "解析器未返回预检结果。".into(),
    });
    Err(error(
        "CORPUS_PREFLIGHT",
        format!("{}：{}", failure.code, failure.message),
    ))
}
pub(super) fn parse_pages(
    input: &Path,
    start: u32,
    end: u32,
    cancelled: impl FnMut() -> Result<bool, ProjectError>,
) -> Result<PageRun, ProjectError> {
    let work = Workspace::new()?;
    let exit = run(&work, input, "pages", Some((start, end)), cancelled)?;
    collect_pages(&work, start, end, exit)
}

fn collect_pages(
    work: &Workspace,
    start: u32,
    end: u32,
    exit: Exit,
) -> Result<PageRun, ProjectError> {
    let mut pages = vec![];
    let mut failure = exit.failure;
    for number in start..=end {
        let name = format!("pages/{number:06}.json");
        if work.root.join(&name).exists() {
            let mut page = match work.read(&name) {
                Ok(page) => page,
                Err(e) => {
                    failure.get_or_insert_with(|| Failure {
                        code: e.code().into(),
                        message: format!("物理页{number}：{e}；其他已完成页保留。"),
                    });
                    continue;
                }
            };
            page["parser"]["adapter_sha256"] = json!(crate::text::sha256(WORKER));
            if page["page_number"].as_u64() != Some(u64::from(number)) {
                failure.get_or_insert_with(|| Failure {
                    code: "CORPUS_WORKER_PROTOCOL".into(),
                    message: format!(
                        "解析器物理页{number}的文件名和页号不一致；其他已完成页保留。"
                    ),
                });
                continue;
            }
            pages.push(page);
        }
    }
    if !exit.cancelled && failure.is_none() && pages.len() != (end - start + 1) as usize {
        failure = Some(Failure {
            code: "CORPUS_WORKER_INCOMPLETE".into(),
            message: "解析器没有返回本批的全部页；缺页保持待处理。".into(),
        });
    }
    Ok(PageRun {
        pages,
        cancelled: exit.cancelled,
        failure,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn damaged_worker_page_keeps_completed_neighbours() {
        let work = Workspace::new().unwrap();
        fs::create_dir(work.root.join("pages")).unwrap();
        fs::write(
            work.root.join("pages/000001.json"),
            br#"{"page_number":1,"parser":{}}"#,
        )
        .unwrap();
        fs::write(work.root.join("pages/000002.json"), b"{incomplete").unwrap();
        fs::write(
            work.root.join("pages/000003.json"),
            br#"{"page_number":3,"parser":{}}"#,
        )
        .unwrap();
        let result = collect_pages(
            &work,
            1,
            3,
            Exit {
                cancelled: false,
                failure: None,
            },
        )
        .unwrap();
        assert_eq!(
            result
                .pages
                .iter()
                .map(|p| p["page_number"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(result.failure.unwrap().code, "CORPUS_WORKER_JSON");
    }
}
