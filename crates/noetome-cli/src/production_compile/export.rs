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
// Preserve visible source text without activating comments, entities, math,
// tables or Markdown blocks. Legacy exports retain their original encoder.
fn precise_literal(text: &str) -> String {
    let mut result = String::new();
    let mut characters = text.chars().peekable();
    let mut line_start = true;
    let mut ordered_prefix = false;
    while let Some(c) = characters.next() {
        if c == '\r' && characters.peek() == Some(&'\n') {
            continue;
        }
        if matches!(c, '\r' | '\n') {
            result.push_str("  \n");
            line_start = true;
            ordered_prefix = false;
            continue;
        }
        if line_start && matches!(c, ' ' | '\t') {
            // An entity at the physical line start prevents an indented block.
            result.push_str(if c == ' ' { "&#32;" } else { "&#9;" });
            line_start = false;
            ordered_prefix = false;
            continue;
        }
        let block_marker = line_start && matches!(c, '-' | '+' | '=');
        let ordered_marker = ordered_prefix
            && c == '.'
            && characters
                .peek()
                .is_none_or(|next| next.is_ascii_whitespace());
        if "\\`*_{}[]<>!#$%&|~()".contains(c) || block_marker || ordered_marker {
            result.push('\\');
        }
        result.push(c);
        ordered_prefix = c.is_ascii_digit() && (line_start || ordered_prefix);
        line_start = false;
    }
    result
}
fn write_precise_details(
    file: &mut File,
    path: &Path,
    store: &store::Store,
    run_id: &str,
    atom: &Value,
) -> Result<(), ProjectError> {
    let measurement = &atom["measurement"];
    if !measurement.is_null() {
        let unit = measurement["unit"].as_str().unwrap_or("");
        let description = match measurement["value_status"].as_str() {
            Some("known") => format!(
                "数值：{} {}。",
                precise_literal(
                    measurement["value"]
                        .as_str()
                        .ok_or_else(|| error("COMPILE_EXPORT", "已知数值缺少值。"))?
                ),
                precise_literal(unit)
            ),
            Some("unknown") => format!(
                "数值：未知；单位：{}。",
                if unit.is_empty() {
                    "未知".to_string()
                } else {
                    precise_literal(unit)
                }
            ),
            Some("not_applicable") => "数值：不适用。".to_string(),
            _ => return Err(error("COMPILE_EXPORT", "数值状态不可读。")),
        };
        writeln!(file, "{description}\n").map_err(|e| ProjectError::io(path, e))?;
        let conditions = measurement["conditions"]
            .as_array()
            .ok_or_else(|| error("COMPILE_EXPORT", "测量条件不可读。"))?;
        if !conditions.is_empty() {
            let conditions = conditions
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(precise_literal)
                        .ok_or_else(|| error("COMPILE_EXPORT", "测量条件不是文本。"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            writeln!(file, "测量条件：{}。\n", conditions.join("；"))
                .map_err(|e| ProjectError::io(path, e))?;
        }
    }
    for link in atom["evidence_links"]
        .as_array()
        .ok_or_else(|| error("COMPILE_EXPORT", "原子证据关系不可读。"))?
    {
        let id = link["evidence_id"]
            .as_str()
            .ok_or_else(|| error("COMPILE_EXPORT", "证据身份缺失。"))?;
        let evidence = store.evidence(run_id, id)?;
        let relation = match link["relation"].as_str() {
            Some("supports") => "来源支持",
            Some("qualifies") => "限定依据",
            Some("refutes") => "反证依据，尚待核对",
            _ => return Err(error("COMPILE_EXPORT", "证据关系不受支持。")),
        };
        let number = evidence["page_number"]
            .as_u64()
            .ok_or_else(|| error("COMPILE_EXPORT", "证据物理页缺失。"))?;
        let quote = evidence["quote"]
            .as_str()
            .ok_or_else(|| error("COMPILE_EXPORT", "证据引文缺失。"))?;
        writeln!(
            file,
            "{relation}（物理页 {number}）：{}\n",
            precise_literal(quote)
        )
        .map_err(|e| ProjectError::io(path, e))?;
    }
    Ok(())
}
fn write_visible_issues(
    file: &mut File,
    path: &Path,
    store: &store::Store,
    project: &Path,
    run_id: &str,
) -> Result<Vec<Value>, ProjectError> {
    let mut locations = Vec::new();
    writeln!(file, "\n## 尚未解决的页与原图定位\n\n以下位置只固定原件与区域，不代表图、表或公式已经解释。坐标使用保存页的 PDF 显示坐标；原件保留在项目中。\n")
        .map_err(|e| ProjectError::io(path, e))?;
    let mut offset = 0;
    loop {
        let pages = store.query(run_id, "pages", offset, 50, "")?;
        let pages = pages["items"]
            .as_array()
            .ok_or_else(|| error("COMPILE_EXPORT", "处理页分页不可读。"))?;
        if pages.is_empty() {
            break;
        }
        for page in pages {
            if page["in_scope"] != true {
                continue;
            }
            let number = page["page_number"]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| error("COMPILE_EXPORT", "处理物理页不可读。"))?;
            let source = page["source_id"]
                .as_str()
                .ok_or_else(|| error("COMPILE_EXPORT", "处理来源不可读。"))?;
            let Some(pin) = page["page_sha256"].as_str() else {
                writeln!(
                    file,
                    "- 物理页 {number}（来源 {}）：尚未准备，原图区域未判断。",
                    precise_literal(source)
                )
                .map_err(|e| ProjectError::io(path, e))?;
                continue;
            };
            let retained = corpus::page_revision(project, source, number, pin)?;
            let issues = retained["issues"]
                .as_array()
                .ok_or_else(|| error("COMPILE_EXPORT", "原页问题清单不可读。"))?;
            let failed = page["units_failed"].as_u64().unwrap_or(0);
            let pending = page["units_total"]
                .as_u64()
                .unwrap_or(0)
                .saturating_sub(page["units_confirmed"].as_u64().unwrap_or(0))
                .saturating_sub(failed);
            let unresolved_status = matches!(
                page["status"].as_str(),
                Some("needs_review" | "needs_ocr" | "failed")
            );
            if !unresolved_status && issues.is_empty() && failed == 0 && pending == 0 {
                continue;
            }
            writeln!(file, "\n### 物理页 {number}\n\n来源：{}；原件 SHA-256：{}；页 SHA-256：{}；页状态：{}。失败片段 {failed} 个，尚待处理 {pending} 个。\n", precise_literal(source), page["source_sha256"].as_str().unwrap_or("未知"), pin, precise_literal(page["status"].as_str().unwrap_or("未知"))).map_err(|e| ProjectError::io(path, e))?;
            let geometry = json!({"width_pt":retained["width_pt"],"height_pt":retained["height_pt"],"rotation":retained["rotation"],"geometry":retained["geometry"]});
            writeln!(
                file,
                "原页尺寸与坐标系：{}。坐标直接沿用保存页，不再次旋转。\n",
                precise_literal(&geometry.to_string())
            )
            .map_err(|e| ProjectError::io(path, e))?;
            if unresolved_status && issues.is_empty() {
                writeln!(
                    file,
                    "原页状态仍待核对，未提供逐项问题代码；下列位置仅保留原页与可用候选区域。\n"
                )
                .map_err(|e| ProjectError::io(path, e))?;
            }
            for issue in issues {
                writeln!(
                    file,
                    "- {}：{}",
                    precise_literal(issue["code"].as_str().unwrap_or("SOURCE_REVIEW")),
                    precise_literal(issue["message"].as_str().unwrap_or("原页保留待核对问题。"))
                )
                .map_err(|e| ProjectError::io(path, e))?;
            }
            let regions = retained["regions"]
                .as_array()
                .ok_or_else(|| error("COMPILE_EXPORT", "原页保留区域不可读。"))?;
            writeln!(
                file,
                "以下为同页保留的候选图、表或公式区域；页级问题尚未逐项对应到区域。\n"
            )
            .map_err(|e| ProjectError::io(path, e))?;
            let mut region_count = 0;
            let mut retained_regions = Vec::new();
            for region in regions {
                if !matches!(
                    region["kind"].as_str(),
                    Some(
                        "image"
                            | "vector"
                            | "image_body"
                            | "chart_body"
                            | "table_body"
                            | "equation"
                    )
                ) {
                    continue;
                }
                if !region["bbox"]
                    .as_array()
                    .is_some_and(|bbox| bbox.len() == 4 && bbox.iter().all(Value::is_number))
                {
                    continue;
                }
                let legacy_id = format!("{pin}:r{}", region["ordinal"]);
                let region_id = region["region_id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .unwrap_or(&legacy_id);
                writeln!(
                    file,
                    "- 原图区域 {}（{}；区域标识 {}）：{}；坐标范围：{}；资产描述：{}",
                    region["ordinal"],
                    precise_literal(region["kind"].as_str().unwrap_or("未知")),
                    precise_literal(region_id),
                    precise_literal(&region["bbox"].to_string()),
                    precise_literal(region["bbox_scope"].as_str().unwrap_or("原区域")),
                    precise_literal(&region["assets"].to_string())
                )
                .map_err(|e| ProjectError::io(path, e))?;
                region_count += 1;
                retained_regions.push(region.clone());
            }
            if region_count == 0 {
                writeln!(file, "该问题目前只固定到整页，较小的图或公式区域尚未知。\n")
                    .map_err(|e| ProjectError::io(path, e))?;
            }
            locations.push(json!({
                "schema_version":1,"run_id":run_id,"source_id":source,
                "source_sha256":page["source_sha256"],"page_number":number,
                "page_sha256":pin,"status":page["status"],
                "units_failed":failed,"units_pending":pending,"geometry":geometry,
                "source_issues":issues,"regions":retained_regions,
                "localization":if region_count == 0 { "page_only" } else { "page_with_candidate_regions" },
                "issue_region_mapping":"unresolved","note_path":"处理范围.md"
            }));
        }
        offset += pages.len() as u64;
    }
    writeln!(file, "\n## 其他待核对事项\n").map_err(|e| ProjectError::io(path, e))?;
    let mut offset = 0;
    loop {
        let result = store.query(run_id, "issues", offset, 50, "")?;
        let items = result["items"]
            .as_array()
            .ok_or_else(|| error("COMPILE_EXPORT", "待核对事项不可读。"))?;
        if items.is_empty() {
            break;
        }
        for issue in items {
            if issue["scope"]
                .as_str()
                .is_some_and(|scope| scope.starts_with("page:"))
            {
                continue;
            }
            writeln!(
                file,
                "- {}（{}）：{}",
                precise_literal(issue["code"].as_str().unwrap_or("SOURCE_REVIEW")),
                precise_literal(issue["scope"].as_str().unwrap_or("未知范围")),
                precise_literal(&issue["detail"].to_string())
            )
            .map_err(|e| ProjectError::io(path, e))?;
        }
        offset += items.len() as u64;
    }
    Ok(locations)
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
    let precise = matches!(
        summary.request_strategy_version,
        model::PRECISE_REQUEST_STRATEGY
            | model::ANCHOR_REQUEST_STRATEGY
            | model::COMPACT_REQUEST_STRATEGY
    );
    let text_literal = if precise { precise_literal } else { literal };
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
            writeln!(file, "# {}\n", text_literal(title))
                .map_err(|e| ProjectError::io(&path, e))?;
            let aliases = concept["aliases"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(text_literal)
                .collect::<Vec<_>>();
            if !aliases.is_empty() {
                writeln!(file, "相关称谓：{}。\n", aliases.join("、"))
                    .map_err(|e| ProjectError::io(&path, e))?;
            }
            let mut ordinal = 0u64;
            let sections: &[(&str, &str)] = if precise {
                &[("", "知识与过程")]
            } else {
                &[
                    ("definition", "概念与定义"),
                    ("statement", "机制与事实"),
                    ("comparison_value", "条件与数值"),
                ]
            };
            for &(kind, heading) in sections {
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
                        if (!precise && atom["kind"] != kind)
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
                        writeln!(file, "{}\n", text_literal(content))
                            .map_err(|e| ProjectError::io(&path, e))?;
                        let qualifiers = atom["qualifiers"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(text_literal)
                            .collect::<Vec<_>>();
                        if !qualifiers.is_empty() {
                            writeln!(file, "适用条件、单位与例外：{}。\n", qualifiers.join("；"))
                                .map_err(|e| ProjectError::io(&path, e))?;
                        }
                        if precise {
                            write_precise_details(&mut file, &path, &store, run_id, atom)?;
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
            writeln!(overview, "- [{}](知识/{id}.md)", text_literal(title))
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
    let scope_text = format!(
        "# 处理范围\n\n原书共 {} 个物理页；本次选择 {} 页，范围外 {} 页。范围内已尝试准备 {} 页，尚未准备 {} 页，失败 {} 页（失败页可同时计入已尝试准备）。确认 {} 个片段，失败 {} 个片段，尚待处理 {} 个片段，待OCR {} 页。\n\n已保存候选原子 {} 个，均未独立核验。累计预算及所有未处理范围以处理范围.json和独立索引为准。原始PDF未随笔记打包。\n",
        summary.physical_pages,
        summary.selected_pages,
        summary.out_of_scope_pages,
        summary.prepared_pages,
        summary.unprepared_pages,
        summary.failed_pages,
        summary.units_confirmed,
        summary.units_failed,
        pending,
        summary.pending_ocr_pages,
        summary.atoms
    );
    let mut scope_file = new_file(&scope_note)?;
    scope_file
        .write_all(scope_text.as_bytes())
        .map_err(|e| ProjectError::io(&scope_note, e))?;
    if precise {
        let locations =
            write_visible_issues(&mut scope_file, &scope_note, &store, project, run_id)?;
        let path = index.join("source-locations.ndjson");
        let mut file = new_file(&path)?;
        for location in locations {
            line(&mut file, &location)?;
        }
        manifest.push(
            json!({"path":relative_path(&path,&output),"fingerprint":finish_file(&path,file)?}),
        );
    }
    manifest
        .push(json!({"path":"处理范围.md","fingerprint":finish_file(&scope_note, scope_file)?}));
    let scope_kind = if summary.out_of_scope_pages == 0 {
        "all_physical_pages"
    } else {
        "selected_physical_page_ranges"
    };
    let report = json!({"schema_version":1,"run_id":run_id,"scope":scope_kind,"scope_schema_version":1,"summary":summary,
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
