//! Writing the measurement output root.
//!
//! The output has exactly four files. `structure.json` holds everything that is
//! determined by the parameters alone; `timings.json` holds the clock readings so
//! a determinism check never has to look at them; `report.md` is the human view
//! and `manifest.json` pins the deterministic half by digest.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::project::ProjectError;
use crate::scale_model::{self, Spec, Structure, Timings};

pub(crate) const OUTPUT_PRODUCER: &str = "scale_harness_output";
pub(crate) const STRUCTURE: &str = "structure.json";
pub(crate) const TIMINGS: &str = "timings.json";
pub(crate) const REPORT: &str = "report.md";
pub(crate) const MANIFEST: &str = "manifest.json";
pub(crate) const OUTPUT_ENTRIES: [&str; 4] = [MANIFEST, REPORT, STRUCTURE, TIMINGS];

fn canonical<T: Serialize>(value: &T, label: &str) -> Result<String, ProjectError> {
    serde_json::to_string(value).map_err(|error| {
        ProjectError::new("SCALE_INTEGRITY", 3, format!("{label}无法序列化：{error}"))
    })
}

fn write_new(path: &Path, bytes: &[u8], label: &str) -> Result<(), ProjectError> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            ProjectError::new(
                "SCALE_IO",
                3,
                format!("无法创建{label} {}：{error}", path.display()),
            )
        })?;
    file.write_all(bytes).map_err(|error| {
        ProjectError::new(
            "SCALE_IO",
            3,
            format!("无法写入{label} {}：{error}", path.display()),
        )
    })?;
    file.sync_all().map_err(|error| {
        ProjectError::new(
            "SCALE_IO",
            3,
            format!("无法同步{label} {}：{error}", path.display()),
        )
    })?;
    let read_back = std::fs::read(path).map_err(|error| {
        ProjectError::new(
            "SCALE_IO",
            3,
            format!("无法回读{label} {}：{error}", path.display()),
        )
    })?;
    if read_back != bytes {
        return Err(ProjectError::new(
            "SCALE_INTEGRITY",
            3,
            format!("{label} {} 写入后回读不一致。", path.display()),
        ));
    }
    Ok(())
}

#[derive(Serialize)]
struct Manifest<'a> {
    policy: &'a str,
    producer: &'a str,
    schema_version: u32,
    case_id: &'a str,
    spec: Spec,
    structure_sha256: String,
    structure_byte_length: usize,
    counts: &'a scale_model::Counts,
    nfr10_status: &'a str,
}

/// Measure a synthetic corpus and write the four output files.
pub fn measure(
    pages: &str,
    headings: &str,
    paragraphs: &str,
    output: &Path,
) -> Result<String, ProjectError> {
    let spec = Spec::parse(pages, headings, paragraphs)?;
    if output.exists() {
        return Err(ProjectError::new(
            "SCALE_ARGUMENT",
            2,
            format!(
                "输出根必须尚不存在：{}；请选择一个新目录。",
                output.display()
            ),
        ));
    }
    let parent = output
        .parent()
        .ok_or_else(|| ProjectError::new("SCALE_ARGUMENT", 2, "输出根必须有一个父目录。"))?;
    let parent = std::fs::canonicalize(parent).map_err(|error| {
        ProjectError::new(
            "SCALE_IO",
            3,
            format!("无法访问输出根的父目录 {}：{error}", parent.display()),
        )
    })?;
    let name = output
        .file_name()
        .ok_or_else(|| ProjectError::new("SCALE_ARGUMENT", 2, "输出根必须是一个目录名。"))?;
    let target = parent.join(name);

    let (structure, timings) = scale_model::measure(&spec)?;
    let structure_text = format!("{}\n", canonical(&structure, "结构产物")?);
    let timings_text = format!("{}\n", canonical(&timings, "耗时产物")?);
    let markdown = scale_model::render_markdown(&structure);
    let manifest = Manifest {
        policy: scale_model::POLICY,
        producer: OUTPUT_PRODUCER,
        schema_version: scale_model::SCHEMA_VERSION,
        case_id: scale_model::CASE_ID,
        spec,
        structure_sha256: crate::text::sha256(structure_text.as_bytes()),
        structure_byte_length: structure_text.len(),
        counts: &structure.counts,
        nfr10_status: &structure.nfr10_status,
    };
    let manifest_text = format!("{}\n", canonical(&manifest, "输出清单")?);

    std::fs::create_dir(&target).map_err(|error| {
        ProjectError::new(
            "SCALE_IO",
            3,
            format!("无法创建输出根 {}：{error}", target.display()),
        )
    })?;
    let written = (|| -> Result<(), ProjectError> {
        write_new(
            &target.join(STRUCTURE),
            structure_text.as_bytes(),
            "结构产物",
        )?;
        write_new(&target.join(TIMINGS), timings_text.as_bytes(), "耗时产物")?;
        write_new(&target.join(REPORT), markdown.as_bytes(), "报告")?;
        write_new(&target.join(MANIFEST), manifest_text.as_bytes(), "输出清单")?;
        let mut entries: Vec<String> = std::fs::read_dir(&target)
            .map_err(|error| {
                ProjectError::new(
                    "SCALE_IO",
                    3,
                    format!("无法读取输出根 {}：{error}", target.display()),
                )
            })?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        let expected: Vec<String> = OUTPUT_ENTRIES
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        if entries != expected {
            return Err(ProjectError::new(
                "SCALE_INTEGRITY",
                3,
                format!("输出根的文件集合不符：{entries:?}。"),
            ));
        }
        Ok(())
    })();
    if let Err(error) = written {
        let _ = std::fs::remove_dir_all(&target);
        return Err(error);
    }

    let summary = format!(
        "规模实测 {}：\n\
         参数：页数 {}、每页标题 {}、每页段落 {}\n\
         结构：页面 {}、字节 {}、文本片段 {}、标题 {}、段落 {}、概念 {}、出现 {}\n\
         耗时：分析 {} ms、对齐 {} ms、合计 {} ms\n\
         规划上限（未达标）：topics={}、goals={}、concepts={}、atoms={}\n\
         输出：structure.json {}；timings.json {}\n\
         边界：合成语料，不证明真实教材抽取质量；不联网、不调用模型。\n",
        target.display(),
        structure.spec.pages,
        structure.spec.headings_per_page,
        structure.spec.paragraphs_per_page,
        structure.counts.pages,
        structure.counts.bytes,
        structure.counts.spans,
        structure.counts.headings,
        structure.counts.paragraphs,
        structure.counts.concepts,
        structure.counts.occurrences,
        timings.analyse_millis,
        timings.align_millis,
        timings.total_millis,
        structure.limits.max_topics,
        structure.limits.max_goals,
        structure.limits.max_concepts,
        structure.limits.max_atoms,
        short(&manifest.structure_sha256),
        short(&crate::text::sha256(timings_text.as_bytes())),
    );
    Ok(summary)
}

fn short(digest: &str) -> String {
    digest.chars().take(16).collect()
}

/// Re-exported so the command layer can name the type without reaching into the
/// model module.
pub(crate) type Measured = (Structure, Timings);

/// Kept for callers that want the paths without writing.
pub(crate) fn paths(output: &Path) -> [PathBuf; 4] {
    [
        output.join(MANIFEST),
        output.join(REPORT),
        output.join(STRUCTURE),
        output.join(TIMINGS),
    ]
}
