//! 写入 soak 的输出根。
//!
//! 恰四个文件。`structure.json`、`report.md` 与 `manifest.json` 只由参数决定；
//! `timings.json` 装下本次运行的全部读数，确定性比较就不必看它一眼。

use std::path::Path;

use serde::Serialize;

use crate::project::ProjectError;
use crate::soak_model::{self, Spec};

pub(crate) const OUTPUT_PRODUCER: &str = "soak_harness_output";
pub(crate) const STRUCTURE: &str = "structure.json";
pub(crate) const TIMINGS: &str = "timings.json";
pub(crate) const REPORT: &str = "report.md";
pub(crate) const MANIFEST: &str = "manifest.json";
pub(crate) const OUTPUT_ENTRIES: [&str; 4] = [MANIFEST, REPORT, STRUCTURE, TIMINGS];

fn canonical<T: Serialize>(value: &T, label: &str) -> Result<String, ProjectError> {
    serde_json::to_string(value).map_err(|error| {
        ProjectError::new("SOAK_INTEGRITY", 3, format!("{label}无法序列化：{error}"))
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
                "SOAK_IO",
                3,
                format!("无法创建{label} {}：{error}", path.display()),
            )
        })?;
    file.write_all(bytes).map_err(|error| {
        ProjectError::new(
            "SOAK_IO",
            3,
            format!("无法写入{label} {}：{error}", path.display()),
        )
    })?;
    file.sync_all().map_err(|error| {
        ProjectError::new(
            "SOAK_IO",
            3,
            format!("无法同步{label} {}：{error}", path.display()),
        )
    })?;
    let read_back = std::fs::read(path).map_err(|error| {
        ProjectError::new(
            "SOAK_IO",
            3,
            format!("无法回读{label} {}：{error}", path.display()),
        )
    })?;
    if read_back != bytes {
        return Err(ProjectError::new(
            "SOAK_INTEGRITY",
            3,
            format!("{label} {} 写入后回读不一致。", path.display()),
        ));
    }
    Ok(())
}

/// 输出清单只记参数与规则结论。
///
/// 这里**不能**放 `summary` 或 `pause`：那两个结构里有毫秒与 RSS 读数，
/// 放进来 `manifest.json` 就会随机器快慢变化，"同参数两次运行字节相同"立刻不成立。
/// 读数留在 `timings.json`，清单只回答"这份产物出自什么参数、结论是什么"。
#[derive(Serialize)]
struct Manifest<'a> {
    policy: &'a str,
    producer: &'a str,
    schema_version: u32,
    case_id: &'a str,
    spec: Spec,
    structure_sha256: String,
    structure_byte_length: usize,
    coverage: &'a [soak_model::Coverage],
}

fn bytes_text(value: Option<u64>) -> String {
    match value {
        Some(bytes) => format!("{bytes} 字节"),
        None => "不可得".to_owned(),
    }
}

fn short(digest: &str) -> String {
    digest.chars().take(16).collect()
}

/// 跑一次 soak 并把四个文件写进输出根。
pub fn run(
    rounds: &str,
    interval_ms: &str,
    pause_after_ms: Option<&str>,
    max_rss_growth_bytes: &str,
    output: &Path,
) -> Result<String, ProjectError> {
    let spec = Spec::parse(rounds, interval_ms, pause_after_ms, max_rss_growth_bytes)?;
    if output.exists() {
        return Err(ProjectError::new(
            "SOAK_ARGUMENT",
            2,
            format!(
                "输出根必须尚不存在：{}；请选择一个新目录。",
                output.display()
            ),
        ));
    }
    let parent = output
        .parent()
        .ok_or_else(|| ProjectError::new("SOAK_ARGUMENT", 2, "输出根必须有一个父目录。"))?;
    let parent = std::fs::canonicalize(parent).map_err(|error| {
        ProjectError::new(
            "SOAK_IO",
            3,
            format!("无法访问输出根的父目录 {}：{error}", parent.display()),
        )
    })?;
    let name = output
        .file_name()
        .ok_or_else(|| ProjectError::new("SOAK_ARGUMENT", 2, "输出根必须是一个目录名。"))?;
    let target = parent.join(name);

    let (structure, timings) = soak_model::run(&spec, soak_model::unit)?;
    let structure_text = format!("{}\n", canonical(&structure, "结构产物")?);
    let timings_text = format!("{}\n", canonical(&timings, "耗时产物")?);
    let markdown = soak_model::render_markdown(&structure);
    let manifest = Manifest {
        policy: soak_model::POLICY,
        producer: OUTPUT_PRODUCER,
        schema_version: soak_model::SCHEMA_VERSION,
        case_id: soak_model::CASE_ID,
        spec: spec.clone(),
        structure_sha256: crate::text::sha256(structure_text.as_bytes()),
        structure_byte_length: structure_text.len(),
        coverage: &structure.coverage,
    };
    let manifest_text = format!("{}\n", canonical(&manifest, "输出清单")?);

    std::fs::create_dir(&target).map_err(|error| {
        ProjectError::new(
            "SOAK_IO",
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
                    "SOAK_IO",
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
                "SOAK_INTEGRITY",
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

    let pause_line = match (&timings.pause.after_ms, timings.pause.effective_round) {
        (Some(after), Some(round)) => format!(
            "暂停：{} 毫秒处关闭闸门（第 {} 轮生效），延迟 {} 毫秒，时限 {} 毫秒；关闭后调度 {} 轮",
            after,
            round,
            timings.pause.latency_millis.unwrap_or(0),
            timings.pause.bound_millis,
            timings.pause.scheduled_after_pause
        ),
        _ => "暂停：本次不暂停，所有轮次都被调度".to_owned(),
    };
    Ok(format!(
        "长时运行 soak {}：\n参数：轮次 {}、轮间隔 {} 毫秒、暂停 {}、内存增长上限 {} 字节\n执行：已调度 {} 轮、拒绝 {} 轮、成功单元 {} 个、失败 {} 个\n{}\n内存：起始 {}、结束 {}、峰值 {}、增长 {}\n覆盖：{}；{}\n输出：structure.json {}；timings.json {}\n边界：合成负载，不写项目与 Vault；模型与网络 0 次；NFR-09 未达标（见报告）\n",
        target.display(),
        spec.rounds,
        spec.interval_ms,
        match spec.pause_after_ms {
            Some(after) => format!("{after} 毫秒"),
            None => "不暂停".to_owned(),
        },
        spec.max_rss_growth_bytes,
        timings.summary.scheduled,
        timings.summary.refused,
        timings.summary.units,
        timings.summary.errors,
        pause_line,
        bytes_text(timings.summary.first_rss_bytes),
        bytes_text(timings.summary.last_rss_bytes),
        bytes_text(timings.summary.peak_rss_bytes),
        timings
            .summary
            .rss_growth_bytes
            .map(|value| format!("{value} 字节"))
            .unwrap_or_else(|| "未知".to_owned()),
        structure.coverage[0].requirement,
        structure.coverage[0].status,
        short(&crate::text::sha256(structure_text.as_bytes())),
        short(&crate::text::sha256(timings_text.as_bytes())),
    ))
}
