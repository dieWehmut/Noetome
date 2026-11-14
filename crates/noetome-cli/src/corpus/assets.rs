//! Only declared OCR files inside this source's managed output are readable.
use super::*;
use std::{
    collections::BTreeMap,
    path::{Component, PathBuf},
};

pub(super) struct Asset {
    pub path: PathBuf,
    pub relative: PathBuf,
    pub sha256: String,
}

impl Repository {
    fn artifact_root(&self, id: &str, page: &Value) -> Result<PathBuf, ProjectError> {
        let selected = page["artifact_root"]
            .as_str()
            .ok_or_else(|| error("CORPUS_ASSET_PATH", "OCR声明资产缺少受控目录。"))?;
        let path = Path::new(selected);
        let project = self
            .root
            .parent()
            .ok_or_else(|| error("CORPUS_ASSET_PATH", "项目路径无效。"))?;
        let absolute = if path.is_absolute() {
            path.to_owned()
        } else {
            safe_relative(path)?;
            project.join(path)
        };
        let absolute = project::check_components(&absolute)?;
        let allowed = self.source_directory(id)?.join("ocr");
        if !absolute.starts_with(allowed) {
            return Err(error(
                "CORPUS_ASSET_PATH",
                "OCR资产目录不在当前来源的受控OCR目录内。",
            ));
        }
        Ok(absolute)
    }
    pub fn artifact_relative_root(&self, id: &str, page: &Value) -> Result<PathBuf, ProjectError> {
        self.artifact_root(id, page)?
            .strip_prefix(self.root.parent().unwrap())
            .map(Path::to_owned)
            .map_err(|_| error("CORPUS_ASSET_PATH", "OCR资产目录不是项目子目录。"))
    }
    pub fn assets(&self, id: &str, page: &Value) -> Result<Vec<Asset>, ProjectError> {
        let mut declarations = Vec::new();
        if let Some(top) = page.get("assets") {
            declarations.extend(
                top.as_array()
                    .ok_or_else(|| error("CORPUS_ASSET_INVALID", "资产清单必须是数组。"))?
                    .iter(),
            );
        }
        if let Some(regions) = page["regions"].as_array() {
            for region in regions {
                if let Some(assets) = region.get("assets") {
                    declarations.extend(
                        assets
                            .as_array()
                            .ok_or_else(|| {
                                error("CORPUS_ASSET_INVALID", "区域资产清单必须是数组。")
                            })?
                            .iter(),
                    );
                }
            }
        }
        if declarations.is_empty() {
            return Ok(Vec::new());
        }
        let root = self.artifact_root(id, page)?;
        let mut unique = BTreeMap::<PathBuf, Asset>::new();
        for entry in declarations {
            let relative = PathBuf::from(
                entry["path"]
                    .as_str()
                    .ok_or_else(|| error("CORPUS_ASSET_INVALID", "资产缺少相对路径。"))?,
            );
            safe_relative(&relative)?;
            let sha = entry["sha256"]
                .as_str()
                .filter(|s| {
                    s.len() == 64
                        && s.bytes()
                            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                })
                .ok_or_else(|| error("CORPUS_ASSET_INVALID", "资产缺少SHA256。"))?;
            let path = project::check_components(&root.join(&relative))?;
            let pin = export::fingerprint(&path).map_err(|e| {
                error(
                    "CORPUS_ASSET_CHANGED",
                    format!("OCR声明资产缺失、不可读或路径不安全：{e}"),
                )
            })?;
            if pin["sha256"] != sha
                || entry
                    .get("byte_length")
                    .is_some_and(|length| length.as_u64() != pin["byte_length"].as_u64())
            {
                return Err(error(
                    "CORPUS_ASSET_CHANGED",
                    "OCR声明资产的字节数或SHA256不匹配。",
                ));
            }
            if let Some(prior) = unique.get(&relative) {
                if prior.sha256 != sha {
                    return Err(error("CORPUS_ASSET_CHANGED", "重复资产声明不一致。"));
                }
            }
            unique.insert(
                relative.clone(),
                Asset {
                    path,
                    relative,
                    sha256: sha.into(),
                },
            );
        }
        Ok(unique.into_values().collect())
    }
    pub fn prepare_assets(&self, id: &str, page: &mut Value) -> Result<(), ProjectError> {
        if page.get("artifact_root").is_none() {
            return self.assets(id, page).map(|_| ());
        }
        let relative = self.artifact_relative_root(id, page)?;
        let root = self.artifact_root(id, page)?;
        page["artifact_root"] = json!(relative.to_string_lossy().replace('\\', "/"));
        if let Some(raw) = page["raw_output"].as_str() {
            let raw = PathBuf::from(raw);
            safe_relative(&raw)?;
            let pin = export::fingerprint(&root.join(&raw))?;
            let assets = page
                .as_object_mut()
                .ok_or_else(|| error("CORPUS_ASSET_INVALID", "页面不是对象。"))?
                .entry("assets")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or_else(|| error("CORPUS_ASSET_INVALID", "资产清单必须是数组。"))?;
            let path = raw.to_string_lossy().replace('\\', "/");
            if !assets.iter().any(|a| a["path"] == path) {
                assets.push(
                    json!({"path":path,"sha256":pin["sha256"],"byte_length":pin["byte_length"]}),
                );
            }
        }
        for entry in page
            .get_mut("assets")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            fill_length(&root, entry)?;
        }
        for region in page["regions"].as_array_mut().into_iter().flatten() {
            for entry in region
                .get_mut("assets")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                fill_length(&root, entry)?;
            }
        }
        self.assets(id, page).map(|_| ())
    }
}

fn fill_length(root: &Path, entry: &mut Value) -> Result<(), ProjectError> {
    let relative = PathBuf::from(
        entry["path"]
            .as_str()
            .ok_or_else(|| error("CORPUS_ASSET_INVALID", "资产缺少路径。"))?,
    );
    safe_relative(&relative)?;
    let pin = export::fingerprint(&root.join(&relative))?;
    if entry.get("byte_length").is_none() {
        entry["byte_length"] = pin["byte_length"].clone();
    }
    Ok(())
}
fn safe_relative(path: &Path) -> Result<(), ProjectError> {
    if path.as_os_str().is_empty()
        || path.to_string_lossy().contains(['\\', ':'])
        || !path.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(error(
            "CORPUS_ASSET_PATH",
            "资产路径必须是安全的相对文件路径。",
        ));
    }
    Ok(())
}
