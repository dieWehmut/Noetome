use super::*;
use std::{fs::File, path::PathBuf};

fn literal(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        if "\\`*_{}[]<>!#".contains(c) {
            result.push('\\');
        }
        result.push(c);
    }
    result
}
fn new_file(path: &Path) -> Result<File, ProjectError> {
    store::guarded(path, true, false)
}
fn relative_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .expect("owned export path")
        .to_string_lossy()
        .replace('\\', "/")
}
fn line(file: &mut File, value: &Value) -> Result<(), ProjectError> {
    serde_json::to_writer(&mut *file, value)
        .map_err(|_| error("COMPILE_EXPORT", "索引写入失败。"))?;
    file.write_all(b"\n")
        .map_err(|_| error("COMPILE_EXPORT", "索引写入失败。"))
}
fn within(candidate: &Path, parent: &Path) -> Result<bool, ProjectError> {
    let parent = project::check_components(parent)?;
    for ancestor in candidate.ancestors() {
        if ancestor.exists()
            && same_file::is_same_file(ancestor, &parent)
                .map_err(|e| ProjectError::io(ancestor, e))?
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn finish_file(path: &Path, file: File) -> Result<Value, ProjectError> {
    file.sync_all().map_err(|e| ProjectError::io(path, e))?;
    fingerprint_guard(path, crate::compile_identity::read_after_write(path, file)?)
}
fn fingerprint(path: &Path) -> Result<Value, ProjectError> {
    fingerprint_guard(path, project::read_guard(path)?)
}
fn fingerprint_guard(path: &Path, mut input: File) -> Result<Value, ProjectError> {
    let before = crate::compile_identity::identity(path)?;
    crate::sidecar_workspace::check_links(&input, "COMPILE_INTEGRITY").map_err(|_| {
        error(
            "COMPILE_INTEGRITY",
            "导出文件拥有多个硬链接或不能核对链接数。",
        )
    })?;
    let metadata = input.metadata().map_err(|e| ProjectError::io(path, e))?;
    let opened =
        same_file::Handle::from_file(input.try_clone().map_err(|e| ProjectError::io(path, e))?)
            .map_err(|e| ProjectError::io(path, e))?;
    let mut hash = sha2::Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut bytes = 0u64;
    use sha2::Digest;
    let mut bounded = (&mut input).take(metadata.len().saturating_add(1));
    loop {
        let n = bounded
            .read(&mut buffer)
            .map_err(|e| ProjectError::io(path, e))?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        bytes += n as u64;
    }
    let after = input.metadata().map_err(|e| ProjectError::io(path, e))?;
    if bytes != metadata.len()
        || metadata.len() != after.len()
        || metadata.modified().ok() != after.modified().ok()
        || before != crate::compile_identity::identity(path)?
        || opened != same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?
    {
        return Err(error("COMPILE_EXPORT_CHANGED", "导出文件在读回期间变化。"));
    }
    Ok(json!({"bytes":bytes,"sha256":format!("{:x}",hash.finalize())}))
}

pub fn export(project: &Path, run_id: &str, output: &Path) -> Result<Value, ProjectError> {
    let store = required(project)?;
    let _lease = store.lease(run_id)?;
    let summary = status(project, run_id)?;
    let output = project::check_components(output)?;
    crate::extraction_preview::ensure_outside_vault(&output)?;
    if output.exists() || within(&output, project)? {
        return Err(error(
            "COMPILE_EXPORT_PATH",
            "知识Vault必须是项目外尚不存在的新目录。",
        ));
    }
    let parent = output
        .parent()
        .ok_or_else(|| error("COMPILE_EXPORT_PATH", "输出父目录无效。"))?;
    let parent_meta = fs::symlink_metadata(parent).map_err(|e| ProjectError::io(parent, e))?;
    if !parent_meta.is_dir() || project::is_link(&parent_meta) {
        return Err(error(
            "COMPILE_EXPORT_PATH",
            "输出父目录必须是已有真实目录。",
        ));
    }
    let _parent_guard = store::guarded(parent, false, true)?;
    for source in &summary.sources {
        corpus::verify_source(project, &source.source_id)?;
    }
    fs::create_dir(&output).map_err(|e| ProjectError::io(&output, e))?;
    let _output_guard = store::guarded(&output, false, true)?;
    let incomplete = output.join("compile-export.incomplete.json");
    save_artifact(
        &incomplete,
        &serde_json::to_vec(&json!({"schema_version":1,"run_id":run_id,"status":"incomplete"}))
            .unwrap(),
    )?;
    let notes = output.join("知识");
    let index = output.join(".noetome-index");
    fs::create_dir(&notes).map_err(|e| ProjectError::io(&notes, e))?;
    fs::create_dir(&index).map_err(|e| ProjectError::io(&index, e))?;
    let _notes_guard = store::guarded(&notes, false, true)?;
    let _index_guard = store::guarded(&index, false, true)?;
    let mut manifest = Vec::<Value>::new();
    let mut blocks = new_file(&index.join("note-blocks.ndjson"))?;
    let mut notes_index = new_file(&index.join("notes.ndjson"))?;
    let mut overview = new_file(&output.join("学习笔记.md"))?;
    overview.write_all("# 学习笔记\n\n以下笔记来自所选资料的已保存知识候选，仍待核验。未识别页、失败片段和未结清请求保留在处理范围报告中。\n\n".as_bytes()).map_err(|e|ProjectError::io(&output,e))?;
    let mut offset = 0;
    loop {
        let result = store.query(run_id, "concepts", offset, 50, "")?;
        let items = result["items"]
            .as_array()
            .ok_or_else(|| error("COMPILE_EXPORT", "概念分页不可读。"))?;
        if items.is_empty() {
            break;
        }
        for concept in items {
            let id = concept["concept_id"]
                .as_str()
                .ok_or_else(|| error("COMPILE_EXPORT", "概念身份缺失。"))?;
            if uuid::Uuid::parse_str(id).is_err() {
                return Err(error("COMPILE_EXPORT", "概念身份不是持久UUID。"));
            }
            let title = concept["canonical_name"]
                .as_str()
                .ok_or_else(|| error("COMPILE_EXPORT", "概念名称缺失。"))?;
            let relative = PathBuf::from("知识").join(format!("{id}.md"));
            let path = output.join(&relative);
            let mut file = new_file(&path)?;
            writeln!(file, "# {}\n", literal(title)).map_err(|e| ProjectError::io(&path, e))?;
            let aliases = concept["aliases"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(literal)
                .collect::<Vec<_>>();
            if !aliases.is_empty() {
                writeln!(file, "相关称谓：{}。\n", aliases.join("、"))
                    .map_err(|e| ProjectError::io(&path, e))?;
            }
            let mut ordinal = 0u64;
            for (kind, heading) in [
                ("definition", "概念与定义"),
                ("statement", "机制与事实"),
                ("comparison_value", "条件与数值"),
            ] {
                let mut atom_offset = 0;
                let mut heading_written = false;
                loop {
                    let atoms = store.query(run_id, "atoms", atom_offset, 50, id)?;
                    let batch = atoms["items"]
                        .as_array()
                        .ok_or_else(|| error("COMPILE_EXPORT", "原子分页不可读。"))?;
                    if batch.is_empty() {
                        break;
                    }
                    for atom in batch {
                        if atom["kind"] != kind
                            || !atom["concept_ids"]
                                .as_array()
                                .is_some_and(|ids| ids.iter().any(|c| c == id))
                        {
                            continue;
                        }
                        if !heading_written {
                            writeln!(file, "## {heading}\n")
                                .map_err(|e| ProjectError::io(&path, e))?;
                            heading_written = true;
                        }
                        let content = atom["content"]
                            .as_str()
                            .ok_or_else(|| error("COMPILE_EXPORT", "原子正文缺失。"))?;
                        writeln!(file, "{}\n", literal(content))
                            .map_err(|e| ProjectError::io(&path, e))?;
                        let qualifiers = atom["qualifiers"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(literal)
                            .collect::<Vec<_>>();
                        if !qualifiers.is_empty() {
                            writeln!(file, "适用条件、单位与例外：{}。\n", qualifiers.join("；"))
                                .map_err(|e| ProjectError::io(&path, e))?;
                        }
                        line(
                            &mut blocks,
                            &json!({"note_id":id,"path":relative_path(&path,&output),"section":heading,"block_ordinal":ordinal,
                            "atom_id":atom["atom_id"],"atom_revision":atom["revision"],"evidence_links":atom["evidence_links"]}),
                        )?;
                        ordinal += 1;
                    }
                    atom_offset += batch.len() as u64;
                }
            }
            let pin = finish_file(&path, file)?;
            manifest.push(json!({"path":relative_path(&path,&output),"fingerprint":pin}));
            line(
                &mut notes_index,
                &json!({"note_id":id,"revision":concept["revision"],"title":title,"path":relative_path(&path,&output),"blocks":ordinal}),
            )?;
            writeln!(overview, "- [{}](知识/{id}.md)", literal(title))
                .map_err(|e| ProjectError::io(&output, e))?;
        }
        offset += items.len() as u64;
    }
    for (path, file) in [
        (index.join("note-blocks.ndjson"), blocks),
        (index.join("notes.ndjson"), notes_index),
        (output.join("学习笔记.md"), overview),
    ] {
        manifest.push(
            json!({"path":relative_path(&path,&output),"fingerprint":finish_file(&path,file)?}),
        );
    }
    for kind in ["concepts", "atoms", "evidence", "issues", "pages", "units"] {
        let path = index.join(format!("{kind}.ndjson"));
        let mut file = new_file(&path)?;
        let mut offset = 0;
        loop {
            let page = store.query(run_id, kind, offset, 50, "")?;
            let items = page["items"]
                .as_array()
                .ok_or_else(|| error("COMPILE_EXPORT", "索引分页不可读。"))?;
            if items.is_empty() {
                break;
            }
            for item in items {
                if kind == "evidence" {
                    let _ = super::evidence(
                        project,
                        run_id,
                        item["evidence_id"]
                            .as_str()
                            .ok_or_else(|| error("COMPILE_EXPORT", "证据ID缺失。"))?,
                    )?;
                }
                line(&mut file, item)?;
            }
            offset += items.len() as u64;
        }
        manifest.push(
            json!({"path":relative_path(&path,&output),"fingerprint":finish_file(&path,file)?}),
        );
    }
    let source_index = output.join("source-index.json");
    save_artifact(
        &source_index,
        &serde_json::to_vec_pretty(
            &json!({"schema_version":1,"run_id":run_id,"sources":summary.sources,
        "mapping_directory":".noetome-index","originals_included":false}),
        )
        .unwrap(),
    )?;
    manifest.push(json!({"path":"source-index.json","fingerprint":fingerprint(&source_index)?}));
    let scope_note = output.join("处理范围.md");
    let pending = summary
        .units_total
        .saturating_sub(summary.units_confirmed)
        .saturating_sub(summary.units_failed);
    save_artifact(&scope_note,format!("# 处理范围\n\n已登记 {} 个物理页，准备 {} 页；确认 {} 个片段，失败 {} 个片段，尚待处理 {} 个片段，待OCR {} 页。\n\n已保存候选原子 {} 个，均未独立核验。累计预算及所有未处理范围以处理范围.json和独立索引为准。原始PDF未随笔记打包。\n",summary.physical_pages,summary.prepared_pages,summary.units_confirmed,summary.units_failed,pending,summary.pending_ocr_pages,summary.atoms).as_bytes())?;
    manifest.push(json!({"path":"处理范围.md","fingerprint":fingerprint(&scope_note)?}));
    let report = json!({"schema_version":1,"run_id":run_id,"scope":"all_physical_pages","summary":summary,
        "knowledge_status":summary.knowledge_status,"scientific_quality":summary.scientific_quality,"raw_pdf_included":false,
        "notes_style":"detailed_supported_candidates","manifest":"export-manifest.json","files":manifest});
    let scope_json = output.join("处理范围.json");
    save_artifact(&scope_json, &serde_json::to_vec_pretty(&report).unwrap())?;
    manifest.push(json!({"path":"处理范围.json","fingerprint":fingerprint(&scope_json)?}));
    save_artifact(
        &output.join("export-manifest.json"),
        &serde_json::to_vec_pretty(&json!({"schema_version":1,"run_id":run_id,"files":manifest,
        "self_included":false,"status":"complete"}))
        .unwrap(),
    )?;
    fs::remove_file(&incomplete).map_err(|e| ProjectError::io(&incomplete, e))?;
    Ok(report)
}
