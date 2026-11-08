//! Writing the library-scale measurement output root.
//!
//! Exactly four files. `structure.json`, `report.md` and `manifest.json` depend
//! only on the parameters; `timings.json` holds every clock reading so that a
//! determinism check never has to look at one.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::bench_model::{self, Spec};
use crate::project::ProjectError;

pub(crate) const OUTPUT_PRODUCER: &str = "library_bench_output";
pub(crate) const STRUCTURE: &str = "structure.json";
pub(crate) const TIMINGS: &str = "timings.json";
pub(crate) const REPORT: &str = "report.md";
pub(crate) const MANIFEST: &str = "manifest.json";
pub(crate) const OUTPUT_ENTRIES: [&str; 4] = [MANIFEST, REPORT, STRUCTURE, TIMINGS];

fn canonical<T: Serialize>(value: &T, label: &str) -> Result<String, ProjectError> {
    serde_json::to_string(value).map_err(|error| {
        ProjectError::new("BENCH_INTEGRITY", 3, format!("{label}无法序列化：{error}"))
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
                "BENCH_IO",
                3,
                format!("无法创建{label} {}：{error}", path.display()),
            )
        })?;
    file.write_all(bytes).map_err(|error| {
        ProjectError::new(
            "BENCH_IO",
            3,
            format!("无法写入{label} {}：{error}", path.display()),
        )
    })?;
    file.sync_all().map_err(|error| {
        ProjectError::new(
            "BENCH_IO",
            3,
            format!("无法同步{label} {}：{error}", path.display()),
        )
    })?;
    let read_back = std::fs::read(path).map_err(|error| {
        ProjectError::new(
            "BENCH_IO",
            3,
            format!("无法回读{label} {}：{error}", path.display()),
        )
    })?;
    if read_back != bytes {
        return Err(ProjectError::new(
            "BENCH_INTEGRITY",
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
    counts: &'a bench_model::Counts,
    coverage: &'a [bench_model::Coverage],
}

/// Measure a synthetic library and write the four output files.
pub fn measure(
    notes: &str,
    edits: &str,
    samples: &str,
    warmup: &str,
    output: &Path,
) -> Result<String, ProjectError> {
    let spec = Spec::parse(notes, edits, samples, warmup)?;
    if output.exists() {
        return Err(ProjectError::new(
            "BENCH_ARGUMENT",
            2,
            format!(
                "输出根必须尚不存在：{}；请选择一个新目录。",
                output.display()
            ),
        ));
    }
    let parent = output
        .parent()
        .ok_or_else(|| ProjectError::new("BENCH_ARGUMENT", 2, "输出根必须有一个父目录。"))?;
    let parent = std::fs::canonicalize(parent).map_err(|error| {
        ProjectError::new(
            "BENCH_IO",
            3,
            format!("无法访问输出根的父目录 {}：{error}", parent.display()),
        )
    })?;
    let name = output
        .file_name()
        .ok_or_else(|| ProjectError::new("BENCH_ARGUMENT", 2, "输出根必须是一个目录名。"))?;
    let target = parent.join(name);

    let (structure, timings) = bench_model::measure(&spec)?;
    let structure_text = format!("{}\n", canonical(&structure, "结构产物")?);
    let timings_text = format!("{}\n", canonical(&timings, "耗时产物")?);
    let markdown = bench_model::render_markdown(&structure);
    let manifest = Manifest {
        policy: bench_model::POLICY,
        producer: OUTPUT_PRODUCER,
        schema_version: bench_model::SCHEMA_VERSION,
        case_id: bench_model::CASE_ID,
        spec,
        structure_sha256: crate::text::sha256(structure_text.as_bytes()),
        structure_byte_length: structure_text.len(),
        counts: &structure.counts,
        coverage: &structure.coverage,
    };
    let manifest_text = format!("{}\n", canonical(&manifest, "输出清单")?);

    std::fs::create_dir(&target).map_err(|error| {
        ProjectError::new(
            "BENCH_IO",
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
                    "BENCH_IO",
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
                "BENCH_INTEGRITY",
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

    let stage_lines: String = timings
        .stages
        .iter()
        .map(|stage| {
            format!(
                "{} P95 {} ms（中位 {}、最小 {}、最大 {}）",
                stage.stage,
                stage.p95_millis,
                stage.median_millis,
                stage.min_millis,
                stage.max_millis
            )
        })
        .collect::<Vec<_>>()
        .join("；");
    let verdict_lines: String = timings
        .verdicts
        .iter()
        .map(|verdict| format!("{}={}", verdict.stage, verdict.status))
        .collect::<Vec<_>>()
        .join("、");
    let summary = format!(
        "大库实测 {}：\n\
         参数：笔记 {}、人工改动 {}、样本 {}、预热 {}\n\
         语料：主题 {}、概念 {}、文本片段 {}；改动 {} 处、变更清单 {} 页、影响范围 {} 篇\n\
         阶段 P95：{}\n\
         门槛：{} ms；判定：{}\n\
         覆盖：NFR-11 {}、NFR-12 {}\n\
         输出：structure.json {}；timings.json {}\n\
         边界：合成语料，不代表真实笔记库；不联网、不调用模型、不写项目与 Vault。\n",
        target.display(),
        structure.spec.notes,
        structure.spec.edits,
        structure.spec.samples,
        structure.spec.warmup,
        structure.counts.topics,
        structure.counts.concepts,
        structure.counts.spans,
        structure.counts.changed,
        structure.counts.pages,
        structure.counts.impacts,
        stage_lines,
        timings.threshold_millis,
        verdict_lines,
        structure.coverage[0].status,
        structure.coverage[1].status,
        short(&manifest.structure_sha256),
        short(&crate::text::sha256(timings_text.as_bytes())),
    );
    Ok(summary)
}

fn short(digest: &str) -> String {
    digest.chars().take(16).collect()
}

/// Kept for callers that want the paths without writing.
pub(crate) fn paths(output: &Path) -> [PathBuf; 4] {
    [
        output.join(MANIFEST),
        output.join(REPORT),
        output.join(STRUCTURE),
        output.join(TIMINGS),
    ]
}
