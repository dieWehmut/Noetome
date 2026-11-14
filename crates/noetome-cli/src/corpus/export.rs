//! Source packages preserve incomplete coverage as source evidence, not notes.
use super::*;
use std::collections::BTreeMap;

pub(super) fn fingerprint(path: &Path) -> Result<Value, ProjectError> {
    let path = project::check_components(path)?;
    let mut input = project::read_guard(&path)?;
    let before = input.metadata().map_err(|e| io(&path, e))?;
    let identity = same_file::Handle::from_file(input.try_clone().map_err(|e| io(&path, e))?)
        .map_err(|e| io(&path, e))?;
    let mut hash = Sha256::new();
    let mut length = 0u64;
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = input.read(&mut buffer).map_err(|e| io(&path, e))?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        length += n as u64;
    }
    let after = input.metadata().map_err(|e| io(&path, e))?;
    if before.len() != length
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || identity != same_file::Handle::from_path(&path).map_err(|e| io(&path, e))?
    {
        return Err(error(
            "CORPUS_SOURCE_CHANGED",
            "读取期间文件身份、大小或修改时间发生变化。",
        ));
    }
    Ok(json!({"sha256":format!("{:x}",hash.finalize()),"byte_length":length}))
}

/// Stream to a new file and re-read it. This also serves future note exports;
/// callers supply a source fingerprint and never overwrite an existing file.
pub fn copy_source(
    source: &Path,
    destination: &Path,
    expected_sha: &str,
) -> Result<Value, ProjectError> {
    let source = project::check_components(source)?;
    let destination = project::check_components(destination)?;
    let mut input = project::read_guard(&source)?;
    let before = input.metadata().map_err(|e| io(&source, e))?;
    let identity = same_file::Handle::from_file(input.try_clone().map_err(|e| io(&source, e))?)
        .map_err(|e| io(&source, e))?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|e| io(&destination, e))?;
    let mut hash = Sha256::new();
    let mut length = 0u64;
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = input.read(&mut buffer).map_err(|e| io(&source, e))?;
        if n == 0 {
            break;
        }
        output
            .write_all(&buffer[..n])
            .map_err(|e| io(&destination, e))?;
        hash.update(&buffer[..n]);
        length += n as u64;
    }
    output.sync_all().map_err(|e| io(&destination, e))?;
    drop(output);
    let after = input.metadata().map_err(|e| io(&source, e))?;
    let actual = format!("{:x}", hash.finalize());
    if actual != expected_sha
        || before.len() != length
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || identity != same_file::Handle::from_path(&source).map_err(|e| io(&source, e))?
    {
        return Err(error(
            "CORPUS_SOURCE_CHANGED",
            "导出期间原件摘要或身份发生变化；导出尚未完成。",
        ));
    }
    let copied = fingerprint(&destination)?;
    if copied["sha256"] != actual || copied["byte_length"] != length {
        return Err(error(
            "CORPUS_EXPORT_CHANGED",
            "导出文件重新读取的指纹不匹配。",
        ));
    }
    Ok(copied)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<Value, ProjectError> {
    project::check_components(path)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| io(path, e))?;
    output.write_all(bytes).map_err(|e| io(path, e))?;
    output.sync_all().map_err(|e| io(path, e))?;
    drop(output);
    let pin = fingerprint(path)?;
    if pin["sha256"] != crate::text::sha256(bytes) {
        return Err(error("CORPUS_EXPORT_CHANGED", "导出文件读取核验失败。"));
    }
    Ok(pin)
}

// A new destination has no identity yet. Its existing ancestors still identify
// the containing directories, including Windows casing and short-name aliases.
pub(super) fn has_ancestor_identity(path: &Path, parent: &Path) -> Result<bool, ProjectError> {
    match fs::symlink_metadata(parent) {
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(io(parent, e)),
    }
    project::check_components(parent)?;
    let identity = same_file::Handle::from_path(parent).map_err(|e| io(parent, e))?;
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(io(ancestor, e)),
        }
        project::check_components(ancestor)?;
        if same_file::Handle::from_path(ancestor).map_err(|e| io(ancestor, e))? == identity {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn export_source(
    project_path: &Path,
    id: &str,
    destination: &Path,
) -> Result<Value, ProjectError> {
    let project_path = project::check_components(project_path)?;
    let destination = project::check_components(destination)?;
    if destination.starts_with(&project_path)
        || project_path.starts_with(&destination)
        || has_ancestor_identity(&destination, &project_path)?
        || has_ancestor_identity(&project_path, &destination)?
    {
        return Err(error("CORPUS_EXPORT_PATH", "来源包目的地必须在项目之外。"));
    }
    if destination.exists() {
        return Err(error(
            "CORPUS_EXPORT_EXISTS",
            "目的地已存在；请选择全新的空目录。",
        ));
    }
    crate::extraction_preview::ensure_outside_vault(&destination)?;
    let repo = Repository::required(&project_path)?;
    let _lease = repo.lease(id)?;
    let summary = repo.summary(id)?;
    let expected = summary
        .content_sha256
        .as_deref()
        .ok_or_else(|| error("CORPUS_NOT_ACQUIRED", "原件尚未确认，不能导出来源包。"))?;
    let raw = repo.source_directory(id)?.join("raw.pdf");
    if fingerprint(&raw)?["sha256"] != expected {
        return Err(error(
            "CORPUS_SOURCE_CHANGED",
            "保存原件摘要不匹配，尚未创建目的地。",
        ));
    }
    fs::create_dir(&destination).map_err(|e| io(&destination, e))?;
    let incomplete = destination.join("source-package.incomplete.json");
    write_new(
        &incomplete,
        json!({"schema_version":1,"kind":"source_package","status":"incomplete","source_id":id})
            .to_string()
            .as_bytes(),
    )?;
    let result = (|| {
        let mut files = BTreeMap::<String, Value>::new();
        files.insert(
            "raw.pdf".into(),
            copy_source(&raw, &destination.join("raw.pdf"), expected)?,
        );
        let page_dir = destination.join("pages");
        fs::create_dir(&page_dir).map_err(|e| io(&page_dir, e))?;
        let count = summary.page_count.unwrap_or(0);
        let mut statuses = BTreeMap::<String, u32>::new();
        let mut exported = 0u32;
        for offset in (0..count).step_by(50) {
            for mut page in repo.pages(id, offset, 50)? {
                let number = page["page_number"]
                    .as_u64()
                    .ok_or_else(|| error("CORPUS_PAGE_INVALID", "导出页缺少页号。"))?;
                let state = page["status"]
                    .as_str()
                    .ok_or_else(|| error("CORPUS_PAGE_INVALID", "导出页缺少状态。"))?
                    .to_owned();
                *statuses.entry(state).or_default() += 1;
                exported += 1;
                let assets = repo.assets(id, &page)?;
                if let Some(raw_output) = page.get("raw_output") {
                    let pinned = raw_output.as_str().is_some_and(|name| {
                        assets.iter().any(|asset| asset.relative == Path::new(name))
                    });
                    if !pinned {
                        return Err(error(
                            "CORPUS_ASSET_UNPINNED",
                            format!(
                                "物理页{number}的原始OCR结构没有已固定SHA256的资产声明，来源包尚未完成；旧页仍可读取。"
                            ),
                        ));
                    }
                }
                if !assets.is_empty() {
                    let root = repo.artifact_relative_root(id, &page)?;
                    page["artifact_root"] = json!(root.to_string_lossy().replace('\\', "/"));
                    for asset in assets {
                        let relative = root.join(&asset.relative);
                        let key = relative.to_string_lossy().replace('\\', "/");
                        if let Some(pin) = files.get(&key) {
                            if pin["sha256"] != asset.sha256 {
                                return Err(error(
                                    "CORPUS_ASSET_CHANGED",
                                    "同名声明资产的摘要冲突。",
                                ));
                            }
                            continue;
                        }
                        let target = destination.join(&relative);
                        let parent = target.parent().unwrap();
                        project::check_components(parent)?;
                        fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
                        files.insert(key, copy_source(&asset.path, &target, &asset.sha256)?);
                    }
                }
                let name = format!("pages/{number:06}.json");
                files.insert(
                    name.clone(),
                    write_new(&destination.join(&name), page.to_string().as_bytes())?,
                );
            }
        }
        if exported != count
            || serde_json::to_value(repo.summary(id)?)
                .map_err(|e| error("CORPUS_JSON", e.to_string()))?
                != serde_json::to_value(&summary)
                    .map_err(|e| error("CORPUS_JSON", e.to_string()))?
            || fingerprint(&raw)?["sha256"] != expected
        {
            return Err(error(
                "CORPUS_EXPORT_CHANGED",
                "导出期间来源记录或原件发生变化，来源包未确认完成。",
            ));
        }
        for (name, pin) in &files {
            if fingerprint(&destination.join(name))? != *pin {
                return Err(error(
                    "CORPUS_EXPORT_CHANGED",
                    format!("导出文件{name}发生变化。"),
                ));
            }
        }
        let manifest = json!({"schema_version":1,"kind":"source_package","status":"exported","source":summary,"knowledge_status":"pending","page_count":summary.page_count,"exported_page_records":exported,"page_status_counts":statuses,"files":files,"includes":"immutable original and current page evidence; no compiled knowledge or notes"});
        write_new(
            &destination.join("source-package.json"),
            manifest.to_string().as_bytes(),
        )?;
        fs::remove_file(&incomplete).map_err(|e| io(&incomplete, e))?;
        Ok(manifest)
    })();
    result
}
