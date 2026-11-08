//! 长时运行与暂停语义的可复现 soak。
//!
//! 这一层只做判定与计时，不碰磁盘：给定规格，重复执行一个只读工作单元，
//! 每轮记录耗时、内存与错误码，并在暂停闸门关闭后不再调度新单元。
//!
//! 两条硬规则写死在实现里，因为它们是这个 harness 存在的原因：
//!
//! 1. **闸门只管未来的调度**。每轮开始时用当时的挂钟判断闸门是否已关；
//!    一旦关闭就不再打开，"暂停后仍在调度" 这件事就不可能有例外。
//! 2. **观察与声明分开**。`structure.json` 只记规格与规则常量，因此由参数唯一决定；
//!    每轮的读数进 `timings.json`。把读数写进结构产物，同参数两次运行就不再相同。
//!
//! 本模块**不**声称 NFR-09 达标：24 小时基准需要连续 24 小时的真实观察，
//! 这里的 `coverage` 恒把 NFR-09 写成 `not_met`，实测时长交给 `timings.json`。

use std::time::{Duration, Instant};

use crate::project::ProjectError;
use crate::text;

/// 产物里的策略标识。
pub const POLICY: &str = "soak-harness-v1";
pub const SCHEMA_VERSION: u32 = 1;
pub const CASE_ID: &str = "soak-synthetic-v1";

pub const MAX_ROUNDS: usize = 100_000;
pub const MAX_INTERVAL_MS: u64 = 1_000;
pub const MAX_PAUSE_AFTER_MS: u64 = 600_000;
pub const MAX_RSS_GROWTH_BYTES: u64 = 1_073_741_824;

/// NFR-13 的时限：暂停或退出后 2 秒内停止调度新请求。
pub const PAUSE_BOUND_MILLIS: u64 = 2_000;
/// NFR-09 的时限：连续 24 小时。
pub const NFR09_REQUIRED_MILLIS: u64 = 86_400_000;

pub const STATE_SCHEDULED: &str = "scheduled";
pub const STATE_REFUSED: &str = "refused";

pub const COVERAGE_NOT_MET: &str = "not_met";
pub const COVERAGE_PARTIAL: &str = "partially_measured";

pub const UNIT_NOTE: &str =
    "每轮的工作单元是对一页合成文本跑一次只读分析并核对摘要；不写文件、不联网、不调用模型。";

fn argument(message: impl Into<String>) -> ProjectError {
    ProjectError::new("SOAK_ARGUMENT", 2, message)
}

fn integrity(message: impl Into<String>) -> ProjectError {
    ProjectError::new("SOAK_INTEGRITY", 3, message)
}

fn work_error(message: impl Into<String>) -> ProjectError {
    ProjectError::new("SOAK_WORK", 3, message)
}

/// 一次 soak 的规格。全部字段都是参数，产物里原样冻结，便于复现。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub rounds: usize,
    pub interval_ms: u64,
    pub pause_after_ms: Option<u64>,
    pub max_rss_growth_bytes: u64,
}

impl Spec {
    pub fn new(
        rounds: usize,
        interval_ms: u64,
        pause_after_ms: Option<u64>,
        max_rss_growth_bytes: u64,
    ) -> Result<Self, ProjectError> {
        let spec = Self {
            rounds,
            interval_ms,
            pause_after_ms,
            max_rss_growth_bytes,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if !(1..=MAX_ROUNDS).contains(&self.rounds) {
            return Err(argument(format!(
                "轮次必须在 1 到 {MAX_ROUNDS} 之间：收到 {}。",
                self.rounds
            )));
        }
        if self.interval_ms > MAX_INTERVAL_MS {
            return Err(argument(format!(
                "轮间隔必须在 0 到 {MAX_INTERVAL_MS} 毫秒之间：收到 {}。",
                self.interval_ms
            )));
        }
        if let Some(after) = self.pause_after_ms {
            if after > MAX_PAUSE_AFTER_MS {
                return Err(argument(format!(
                    "暂停时点必须在 0 到 {MAX_PAUSE_AFTER_MS} 毫秒之间：收到 {after}。"
                )));
            }
        }
        if self.max_rss_growth_bytes > MAX_RSS_GROWTH_BYTES {
            return Err(argument(format!(
                "内存增长上限必须在 0 到 {MAX_RSS_GROWTH_BYTES} 字节之间：收到 {}。",
                self.max_rss_growth_bytes
            )));
        }
        Ok(())
    }

    /// 参数解析：非负整数，失败时报出原始输入而不是笼统的 "invalid"。
    pub fn parse(
        rounds: &str,
        interval_ms: &str,
        pause_after_ms: Option<&str>,
        max_rss_growth_bytes: &str,
    ) -> Result<Self, ProjectError> {
        let rounds = number(rounds, "轮次")?;
        if rounds > MAX_ROUNDS as u64 {
            return Err(argument(format!(
                "轮次必须在 1 到 {MAX_ROUNDS} 之间：收到 {rounds}。"
            )));
        }
        let interval_ms = number(interval_ms, "轮间隔")?;
        let pause = match pause_after_ms {
            None | Some("") | Some("none") | Some("never") => None,
            Some(value) => Some(number(value, "暂停时点")?),
        };
        let growth = number(max_rss_growth_bytes, "内存增长上限")?;
        Self::new(rounds as usize, interval_ms, pause, growth)
    }

    /// 计划时长的下限：只算轮间隔，不算工作单元。
    pub fn planned_floor_millis(&self) -> u64 {
        self.rounds as u64 * self.interval_ms
    }
}

fn number(value: &str, label: &str) -> Result<u64, ProjectError> {
    value
        .trim()
        .parse::<u64>()
        .map_err(|_| argument(format!("{label}必须是非负整数：收到「{value}」。")))
}

/// 每轮的观察。`elapsed_millis` 是**该轮开始时**距起点的毫秒数，
/// 暂停判定用的就是它——没有这一列，闸门是否真的关过就无法复核。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Round {
    pub round: usize,
    pub state: String,
    pub elapsed_millis: u128,
    pub work_millis: u128,
    pub total_millis: u128,
    pub rss_bytes: Option<u64>,
    pub error_code: Option<String>,
}

impl Round {
    pub fn scheduled(&self) -> bool {
        self.state == STATE_SCHEDULED
    }
}

/// 汇总读数。全部来自本次运行，不参与确定性比较。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub rounds: usize,
    pub scheduled: usize,
    pub refused: usize,
    pub units: usize,
    pub errors: usize,
    /// 闸门关闭之后仍被调度的轮数：这是 NFR-13 的可观测计数，必须为 0。
    pub scheduled_after_pause: usize,
    pub total_millis: u128,
    pub max_work_millis: u128,
    pub median_work_millis: u128,
    pub first_rss_bytes: Option<u64>,
    pub last_rss_bytes: Option<u64>,
    pub peak_rss_bytes: Option<u64>,
    pub rss_growth_bytes: Option<i64>,
}

/// 暂停窗口的观察。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pause {
    pub after_ms: Option<u64>,
    pub effective_round: Option<usize>,
    pub latency_millis: Option<u128>,
    pub bound_millis: u64,
    pub within_bound: Option<bool>,
    pub scheduled_after_pause: usize,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    pub requirement: String,
    pub status: String,
    pub detail: String,
}

/// 由参数唯一决定的规则块。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Enforcement {
    pub pause_bound_millis: u64,
    pub required_millis: u64,
    pub unit: String,
    pub planned_floor_millis: u64,
    pub note: String,
}

/// 结构产物：只由规格决定，重复运行必须字节相同。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    pub policy: String,
    pub schema_version: u32,
    pub case_id: String,
    pub spec: Spec,
    pub enforcement: Enforcement,
    pub coverage: Vec<Coverage>,
    pub reproduce: String,
    pub model_calls: u32,
    pub network_requests: u32,
}

/// 计时产物：本次运行的读数。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timings {
    pub policy: String,
    pub case_id: String,
    pub rounds: Vec<Round>,
    pub summary: Summary,
    pub pause: Pause,
    pub note: String,
}

/// 复现本次规格的命令行（`<输出根>` 留给调用方填）。
pub fn reproduce(spec: &Spec) -> String {
    let pause = match spec.pause_after_ms {
        Some(after) => format!("{after}"),
        None => "none".to_owned(),
    };
    format!(
        "noetome soak run --rounds {} --interval-ms {} --pause-after-ms {} --max-rss-growth-bytes {} --output <输出根>",
        spec.rounds, spec.interval_ms, pause, spec.max_rss_growth_bytes
    )
}

fn coverage(spec: &Spec) -> Vec<Coverage> {
    vec![
        Coverage {
            requirement: "NFR-09".to_owned(),
            status: COVERAGE_NOT_MET.to_owned(),
            detail: format!(
                "NFR-09 要求连续 24 小时（{NFR09_REQUIRED_MILLIS} 毫秒）基准运行。本 harness 按轮次运行，本轮计划时长下限只有 {} 毫秒；实际跑了多久写在 timings.json，未跑满 24 小时即未达到 NFR-09，不把短时结果当长时结论。",
                spec.planned_floor_millis()
            ),
        },
        Coverage {
            requirement: "NFR-13".to_owned(),
            status: COVERAGE_PARTIAL.to_owned(),
            detail: format!(
                "闸门关闭后不再调度新单元这件事由可观测计数证明：每轮开始时记下距起点的毫秒数，暂停时点之后的所有轮次都必须是 refused。真正的「应用暂停命令」还没有界面，所以这里只标 partially_measured，时限常量是 {PAUSE_BOUND_MILLIS} 毫秒。"
            ),
        },
    ]
}

fn structure_of(spec: &Spec) -> Structure {
    Structure {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: CASE_ID.to_owned(),
        spec: spec.clone(),
        enforcement: Enforcement {
            pause_bound_millis: PAUSE_BOUND_MILLIS,
            required_millis: NFR09_REQUIRED_MILLIS,
            unit: UNIT_NOTE.to_owned(),
            planned_floor_millis: spec.planned_floor_millis(),
            note: "结构产物只记录规格与规则常量：任何一次运行的读数写进来，同一参数两次运行就不再相同。".to_owned(),
        },
        coverage: coverage(spec),
        reproduce: reproduce(spec),
        model_calls: 0,
        network_requests: 0,
    }
}

/// 一页合成文本，只用来做只读工作单元。
fn page_of(round: usize) -> String {
    format!(
        "# 第 {round} 轮\n\n## 定位\n\n第 {round} 轮是只读工作单元，只用来观察长时运行里的耗时与内存。\n\n## 反应\n\n这一段同样是合成文本，不含任何教材内容。\n\n"
    )
}

/// 默认工作单元：对一页合成文本跑一次只读分析。
pub fn unit(round: usize) -> Result<(), ProjectError> {
    let page = page_of(round);
    text::analyze(page.as_bytes())
        .map(|_parsed| ())
        .map_err(|_| work_error(format!("第 {round} 轮的工作单元不是合法 UTF-8")))
}

fn rss_of(field: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix(field) {
            let value = rest.trim().trim_end_matches("kB").trim();
            let kib = value.parse::<u64>().ok()?;
            return Some(kib * 1024);
        }
    }
    None
}

fn median(values: &[u128]) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

/// 跑一次 soak。`work` 是每轮要做的事，注入进来是为了让单元测试能在不伪造产品行为的前提下
/// 验证"工作单元失败时错误里带着复现命令"。
pub fn run<F>(spec: &Spec, mut work: F) -> Result<(Structure, Timings), ProjectError>
where
    F: FnMut(usize) -> Result<(), ProjectError>,
{
    spec.validate()?;
    let started = Instant::now();
    let mut rounds: Vec<Round> = Vec::with_capacity(spec.rounds);
    let mut scheduled = 0usize;
    let mut refused = 0usize;
    let mut units = 0usize;
    let mut errors = 0usize;
    let mut scheduled_after_pause = 0usize;
    let mut effective_round: Option<usize> = None;
    let mut pause_latency: Option<u128> = None;
    let mut first_rss: Option<u64> = None;
    let mut last_rss: Option<u64> = None;
    let mut peak_rss: Option<u64> = None;

    for round in 1..=spec.rounds {
        let elapsed = started.elapsed().as_millis();
        // 闸门只在这一处判断：关了就永远关。任何"暂停后仍在调度"都只能来自这里漏判。
        let paused = spec
            .pause_after_ms
            .is_some_and(|after| elapsed >= u128::from(after));
        if paused {
            if effective_round.is_none() {
                effective_round = Some(round);
                pause_latency = spec
                    .pause_after_ms
                    .map(|after| elapsed.saturating_sub(u128::from(after)));
            }
            refused += 1;
            rounds.push(Round {
                round,
                state: STATE_REFUSED.to_owned(),
                elapsed_millis: elapsed,
                work_millis: 0,
                total_millis: 0,
                rss_bytes: None,
                error_code: None,
            });
            if spec.interval_ms > 0 {
                std::thread::sleep(Duration::from_millis(spec.interval_ms));
            }
            continue;
        }
        if effective_round.is_some() {
            // 闸门关过又打开是不可能的；真发生就是实现错了，记为违规而不是静默继续。
            scheduled_after_pause += 1;
        }
        let mark = Instant::now();
        let outcome = work(round);
        let work_millis = mark.elapsed().as_millis();
        let rss = rss_of("VmRSS:");
        if first_rss.is_none() {
            first_rss = rss;
        }
        last_rss = rss.or(last_rss);
        peak_rss = match (peak_rss, rss) {
            (Some(peak), Some(value)) => Some(peak.max(value)),
            (None, value) => value,
            (peak, None) => peak,
        };
        let error_code = match outcome {
            Ok(()) => {
                units += 1;
                None
            }
            Err(error) => {
                errors += 1;
                Some(error.code().to_owned())
            }
        };
        scheduled += 1;
        rounds.push(Round {
            round,
            state: STATE_SCHEDULED.to_owned(),
            elapsed_millis: elapsed,
            work_millis,
            total_millis: mark.elapsed().as_millis(),
            rss_bytes: rss,
            error_code,
        });
        if spec.interval_ms > 0 {
            std::thread::sleep(Duration::from_millis(spec.interval_ms));
        }
    }

    if errors > 0 {
        let failed = rounds
            .iter()
            .find(|round| round.error_code.is_some())
            .map(|round| round.round)
            .unwrap_or_default();
        let code = rounds
            .iter()
            .find_map(|round| round.error_code.clone())
            .unwrap_or_else(|| STATE_REFUSED.to_owned());
        return Err(work_error(format!(
            "第 {failed} 轮的工作单元失败（{code}）：本轮没有继续跑，也没有把失败吞掉。重现命令：{}",
            reproduce(spec)
        )));
    }

    if let (Some(first), Some(last), Some(growth)) = (
        first_rss,
        last_rss,
        last_rss
            .zip(first_rss)
            .map(|(last, first)| last as i64 - first as i64),
    ) {
        let _ = (first, last);
        if growth > 0 && growth as u64 > spec.max_rss_growth_bytes {
            return Err(integrity(format!(
                "长时运行的内存增长 {growth} 字节超过声明的上限 {} 字节：这属于无界资源增长，不能当成通过。重现命令：{}",
                spec.max_rss_growth_bytes,
                reproduce(spec)
            )));
        }
    }

    let within_bound = pause_latency.map(|latency| latency <= u128::from(PAUSE_BOUND_MILLIS));
    let pause = Pause {
        after_ms: spec.pause_after_ms,
        effective_round,
        latency_millis: pause_latency,
        bound_millis: PAUSE_BOUND_MILLIS,
        within_bound,
        scheduled_after_pause,
        note: format!(
            "闸门关闭后不再调度新请求：关闭后调度的轮数是 {scheduled_after_pause}，必须为 0。延迟是关闭那一轮开始时的毫秒数减去暂停时点。"
        ),
    };
    let summary = Summary {
        rounds: spec.rounds,
        scheduled,
        refused,
        units,
        errors,
        scheduled_after_pause,
        total_millis: started.elapsed().as_millis(),
        max_work_millis: rounds
            .iter()
            .map(|round| round.work_millis)
            .max()
            .unwrap_or(0),
        median_work_millis: median(
            &rounds
                .iter()
                .filter(|round| round.scheduled())
                .map(|round| round.work_millis)
                .collect::<Vec<_>>(),
        ),
        first_rss_bytes: first_rss,
        last_rss_bytes: last_rss,
        peak_rss_bytes: peak_rss,
        rss_growth_bytes: last_rss
            .zip(first_rss)
            .map(|(last, first)| last as i64 - first as i64),
    };
    let timings = Timings {
        policy: POLICY.to_owned(),
        case_id: CASE_ID.to_owned(),
        rounds,
        summary,
        pause,
        note: "读数来自本次运行，不参与确定性比较；结构产物只由参数决定。".to_owned(),
    };
    Ok((structure_of(spec), timings))
}

/// 报告：五节，且**不写任何本次运行的读数**。
pub fn render_markdown(structure: &Structure) -> String {
    let mut out = String::new();
    out.push_str(&format!("# 长时运行 soak（{}）\n\n", structure.case_id));
    out.push_str(
        "这一份是**合成**的长时运行观察：工作单元是对一页合成文本做只读分析，不含任何教材内容，也不代表真实项目负载。\n\n",
    );
    out.push_str("## 参数\n\n");
    out.push_str(&format!("- 轮次：{}\n", structure.spec.rounds));
    out.push_str(&format!("- 轮间隔：{} 毫秒\n", structure.spec.interval_ms));
    out.push_str(&format!(
        "- 暂停时点：{}\n",
        match structure.spec.pause_after_ms {
            Some(after) => format!("{after} 毫秒"),
            None => "不暂停".to_owned(),
        }
    ));
    out.push_str(&format!(
        "- 内存增长上限：{} 字节\n\n",
        structure.spec.max_rss_growth_bytes
    ));
    out.push_str("## 工作单元\n\n");
    out.push_str(&format!("{}\n\n", structure.enforcement.unit));
    out.push_str("## 暂停闸门\n\n");
    out.push_str(&format!(
        "- 时限：暂停后 {} 毫秒内必须停止调度新请求（NFR-13）。\n",
        structure.enforcement.pause_bound_millis
    ));
    out.push_str(
        "- 判定口径：每一轮开始时记下距起点的毫秒数；暂停时点之后的所有轮次都必须是 refused。\n",
    );
    out.push_str("- 可观测计数：闸门关闭后仍被调度的轮数必须为 0，读数在 `timings.json` 的 `pause.scheduled_after_pause`。\n\n");
    out.push_str("## 覆盖范围与未达标项\n\n");
    for entry in &structure.coverage {
        out.push_str(&format!(
            "- **{}**：{}——{}\n",
            entry.requirement, entry.status, entry.detail
        ));
    }
    out.push_str("\n## 边界\n\n");
    out.push_str("- 读数写在 `timings.json`：本文件不含本次运行的任何耗时或内存读数，写进来同一参数两次运行就不再相同；上面出现的毫秒数只有规则常量。\n");
    out.push_str("- 合成负载：工作单元只跑文本分析，不写项目、不写 Vault、不联网、不调用模型。\n");
    out.push_str("- 复现命令：`");
    out.push_str(&structure.reproduce);
    out.push_str("`\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(rounds: usize, pause_after_ms: Option<u64>) -> Spec {
        Spec::new(rounds, 0, pause_after_ms, MAX_RSS_GROWTH_BYTES).expect("规格应当合法")
    }

    #[test]
    fn the_same_spec_produces_the_same_structure() {
        let first = run(&spec(8, None), unit).expect("应当跑通").0;
        let second = run(&spec(8, None), unit).expect("应当跑通").0;
        assert_eq!(first, second);
    }

    #[test]
    fn a_paused_run_schedules_nothing_after_the_gate_closes() {
        let (_, timings) = run(&spec(8, Some(0)), unit).expect("应当跑通");
        assert_eq!(timings.summary.scheduled, 0);
        assert_eq!(timings.summary.refused, 8);
        assert_eq!(timings.summary.scheduled_after_pause, 0);
        assert_eq!(timings.pause.effective_round, Some(1));
        assert_eq!(timings.pause.within_bound, Some(true));
        assert!(timings.rounds.iter().all(|round| !round.scheduled()));
    }

    #[test]
    fn a_run_without_pause_schedules_every_round() {
        let (_, timings) = run(&spec(5, None), unit).expect("应当跑通");
        assert_eq!(timings.summary.scheduled, 5);
        assert_eq!(timings.summary.refused, 0);
        assert_eq!(timings.summary.units, 5);
        assert_eq!(timings.pause.effective_round, None);
        assert_eq!(timings.pause.within_bound, None);
    }

    #[test]
    fn a_failing_unit_names_the_round_and_the_reproduce_command() {
        let error = run(&spec(4, None), |round| {
            if round == 3 {
                Err(work_error("注入的失败"))
            } else {
                Ok(())
            }
        })
        .expect_err("工作单元失败必须返回错误");
        assert_eq!(error.code(), "SOAK_WORK");
        assert!(error.to_string().contains("第 3 轮"));
        assert!(error.to_string().contains("noetome soak run --rounds 4"));
    }

    #[test]
    fn the_report_states_the_unmet_requirement_and_carries_no_readings() {
        let structure = run(&spec(6, None), unit).expect("应当跑通").0;
        let markdown = render_markdown(&structure);
        assert!(markdown.contains("NFR-09"));
        assert!(markdown.contains("not_met"));
        assert!(markdown.contains("NFR-13"));
        assert!(markdown.contains("2000"));
        assert!(!markdown.contains("毫秒内已停止"));
        // 报告唯一允许的毫秒数是规则常量；2000 出现的次数不设限，但不得出现"耗时"读数行。
        assert!(!markdown.contains("本轮耗时"));
    }

    #[test]
    fn bounds_are_enforced_before_anything_runs() {
        assert_eq!(
            Spec::new(0, 0, None, 0).expect_err("轮次 0").code(),
            "SOAK_ARGUMENT"
        );
        assert_eq!(
            Spec::new(1, MAX_INTERVAL_MS + 1, None, 0)
                .expect_err("间隔越界")
                .code(),
            "SOAK_ARGUMENT"
        );
        assert_eq!(
            Spec::new(1, 0, Some(MAX_PAUSE_AFTER_MS + 1), 0)
                .expect_err("暂停越界")
                .code(),
            "SOAK_ARGUMENT"
        );
        assert_eq!(
            Spec::parse("abc", "0", None, "0")
                .expect_err("解析失败")
                .code(),
            "SOAK_ARGUMENT"
        );
    }
}
