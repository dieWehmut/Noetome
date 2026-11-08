//! Library-scale measurement: a deterministic synthetic library, the four stages
//! that a large update actually runs (scan, detect, plan, page) and the sampling
//! protocol that turns repeated runs into a P95.
//!
//! The deterministic half of the output (`structure.json`, `report.md`,
//! `manifest.json`) depends only on the parameters. Every clock reading lives in
//! `timings.json`, so "same parameters, same bytes" stays checkable.

use std::collections::BTreeSet;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::project::ProjectError;
use crate::text;

pub(crate) const POLICY: &str = "library-bench-v1";
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const CASE_ID: &str = "bench-library-v1";
pub(crate) const MAX_NOTES: usize = 20_000;
pub(crate) const MAX_EDITS: usize = 10_000;
pub(crate) const MAX_SAMPLES: usize = 200;
pub(crate) const MAX_WARMUP: usize = 50;
pub(crate) const PAGE_SIZE: usize = 100;
pub(crate) const THRESHOLD_MILLIS: u128 = 2_000;
/// Fan-out of the synthetic library, derived from the note count so that a small
/// fixture and a large run share one formula: roughly ten notes per topic and two
/// concepts per note. Deriving it keeps `counts.topics/concepts` reproducible by an
/// independent reader instead of hiding a magic number.
pub(crate) fn topic_count(notes: usize) -> usize {
    (notes / 10).max(1)
}

pub(crate) fn concept_count(notes: usize) -> usize {
    (notes * 2).max(3)
}
pub(crate) const QUANTILE: usize = 95;

pub(crate) const STAGE_SCAN: &str = "scan";
pub(crate) const STAGE_DETECT: &str = "detect";
pub(crate) const STAGE_PLAN: &str = "plan";
pub(crate) const STAGE_PAGE: &str = "page";

pub(crate) const STAGES: [&str; 4] = [STAGE_SCAN, STAGE_DETECT, STAGE_PLAN, STAGE_PAGE];

/// NFR-11 的覆盖率说明按本次参数生成：写死「3000 篇、200 处」会在跑小样本时变成谎话。
pub(crate) fn nfr11_detail(spec: &Spec) -> String {
    format!(
        "合成库确实建到 {} 篇笔记、{} 处人工改动，扫描、检测、影响规划与分页都在这个规模上真实跑过；但「不因全量渲染冻结界面」属于 GUI 行为，本 harness 没有界面，只证明数据结构与分页产物能算出来、算得快。",
        spec.notes, spec.edits
    )
}
pub(crate) const NFR12_DETAIL: &str = "本轮只测 scan/detect/plan/page 四个阶段在 2 秒门槛下的 P95；项目页、暂停命令、搜索与打开问题的端到端反馈没有界面可测，暂停语义属于 B13-T03。";

pub(crate) fn argument(message: impl Into<String>) -> ProjectError {
    ProjectError::new("BENCH_ARGUMENT", 2, message)
}

pub(crate) fn integrity(message: impl Into<String>) -> ProjectError {
    ProjectError::new("BENCH_INTEGRITY", 3, message)
}

/// Parameters of one measurement run.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub notes: usize,
    pub edits: usize,
    pub samples: usize,
    pub warmup: usize,
}

impl Spec {
    pub fn new(
        notes: usize,
        edits: usize,
        samples: usize,
        warmup: usize,
    ) -> Result<Self, ProjectError> {
        let spec = Self {
            notes,
            edits,
            samples,
            warmup,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn parse(
        notes: &str,
        edits: &str,
        samples: &str,
        warmup: &str,
    ) -> Result<Self, ProjectError> {
        fn number(value: &str, label: &str) -> Result<usize, ProjectError> {
            value
                .parse::<usize>()
                .map_err(|_| argument(format!("{label}必须是非负整数：收到「{value}」。")))
        }
        Self::new(
            number(notes, "笔记数")?,
            number(edits, "人工改动数")?,
            number(samples, "样本数")?,
            number(warmup, "预热次数")?,
        )
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if !(1..=MAX_NOTES).contains(&self.notes) {
            return Err(argument(format!(
                "笔记数必须在 1 到 {MAX_NOTES} 之间：收到 {}。",
                self.notes
            )));
        }
        if self.edits > self.notes.min(MAX_EDITS) {
            return Err(argument(format!(
                "人工改动数必须在 0 到 {} 之间：收到 {}。",
                self.notes.min(MAX_EDITS),
                self.edits
            )));
        }
        if !(1..=MAX_SAMPLES).contains(&self.samples) {
            return Err(argument(format!(
                "样本数必须在 1 到 {MAX_SAMPLES} 之间：收到 {}。",
                self.samples
            )));
        }
        if self.warmup > MAX_WARMUP {
            return Err(argument(format!(
                "预热次数必须在 0 到 {MAX_WARMUP} 之间：收到 {}。",
                self.warmup
            )));
        }
        Ok(())
    }
}

/// One synthetic note, before and after its manual edit.
#[derive(Clone, Debug)]
pub(crate) struct Note {
    pub index: usize,
    pub path: String,
    pub topic: String,
    pub concepts: Vec<String>,
    pub body: String,
    pub baseline: String,
    pub edited: bool,
}

fn concept_name(index: usize, concepts: usize) -> String {
    format!("concept-{:04}", index % concepts)
}

fn topic_name(index: usize, topics: usize) -> String {
    format!("topic-{:03}", index % topics)
}

fn body_of(index: usize, edited: bool, concepts_available: usize) -> String {
    let concepts = concepts_of(index, concepts_available);
    let mut text = format!(
        "# 第 {index} 篇\n\n## 定位\n\n{} 与 {} 的关系讨论。\n\n## 反应\n\n第 {index} 篇的反应描述。\n\n",
        concepts[0], concepts[1]
    );
    if edited {
        text.push_str("## 人工改动\n\n这一节是人工追加的内容。\n\n");
    }
    text
}

pub(crate) fn concepts_of(index: usize, concepts: usize) -> [String; 3] {
    [
        concept_name(index, concepts),
        concept_name(index + 7919, concepts),
        concept_name(index + 104_729, concepts),
    ]
}

/// Indices of the manually edited notes. Deterministic and distinct as long as
/// `edits <= notes`.
pub(crate) fn edit_indices(notes: usize, edits: usize) -> Vec<usize> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut cursor = 3usize % notes.max(1);
    while out.len() < edits {
        if seen.insert(cursor) {
            out.push(cursor);
        }
        cursor = (cursor + 7) % notes.max(1);
    }
    out
}

pub(crate) fn generate(spec: &Spec) -> Vec<Note> {
    let edits = edit_indices(spec.notes, spec.edits);
    let edited: BTreeSet<usize> = edits.iter().copied().collect();
    let topics = topic_count(spec.notes);
    let concepts = concept_count(spec.notes);
    (0..spec.notes)
        .map(|index| {
            let is_edited = edited.contains(&index);
            Note {
                index,
                path: format!("合成/第{index:05}篇.md"),
                topic: topic_name(index, topics),
                concepts: concepts_of(index, concepts).to_vec(),
                body: body_of(index, is_edited, concepts),
                baseline: body_of(index, false, concepts),
                edited: is_edited,
            }
        })
        .collect()
}

/// A note fingerprint after the text pipeline has looked at it.
#[derive(Clone, Debug)]
pub(crate) struct Scan {
    pub index: usize,
    pub sha256: String,
    pub spans: usize,
}

pub(crate) fn scan(notes: &[Note]) -> Result<Vec<Scan>, ProjectError> {
    let mut out = Vec::with_capacity(notes.len());
    for note in notes {
        let parsed = text::analyze(note.body.as_bytes())
            .map_err(|_| integrity(format!("第 {} 篇不是合法 UTF-8。", note.index)))?;
        out.push(Scan {
            index: note.index,
            sha256: parsed.normalized_sha256.clone(),
            spans: parsed.spans.len(),
        });
    }
    Ok(out)
}

pub(crate) fn baseline_digests(notes: &[Note]) -> Result<Vec<String>, ProjectError> {
    notes
        .iter()
        .map(|note| {
            text::analyze(note.baseline.as_bytes())
                .map(|parsed| parsed.normalized_sha256.clone())
                .map_err(|_| integrity(format!("第 {} 篇的基线不是合法 UTF-8。", note.index)))
        })
        .collect()
}

/// Notes whose digest differs from the frozen baseline.
pub(crate) fn detect(scans: &[Scan], baseline: &[String]) -> Vec<usize> {
    scans
        .iter()
        .filter(|scan| {
            baseline
                .get(scan.index)
                .is_some_and(|digest| *digest != scan.sha256)
        })
        .map(|scan| scan.index)
        .collect()
}

/// Every note that shares a concept or a topic with a changed note, excluding the
/// changed notes themselves.
pub(crate) fn impact(notes: &[Note], changed: &[usize]) -> Vec<usize> {
    let changed_set: BTreeSet<usize> = changed.iter().copied().collect();
    let mut concepts = BTreeSet::new();
    let mut topics = BTreeSet::new();
    for index in changed {
        if let Some(note) = notes.get(*index) {
            topics.insert(note.topic.clone());
            for concept in &note.concepts {
                concepts.insert(concept.clone());
            }
        }
    }
    notes
        .iter()
        .filter(|note| !changed_set.contains(&note.index))
        .filter(|note| {
            topics.contains(&note.topic)
                || note
                    .concepts
                    .iter()
                    .any(|concept| concepts.contains(concept))
        })
        .map(|note| note.index)
        .collect()
}

/// One page of the change list.
pub(crate) fn page_of(notes: &[Note], changed: &[usize], page: usize) -> String {
    let start = page.saturating_mul(PAGE_SIZE);
    let mut out = format!("# 变更清单 第 {} 页（每页 {PAGE_SIZE} 条）\n\n", page + 1);
    out.push_str("| 序号 | 路径 | 主题 |\n| --- | --- | --- |\n");
    for index in changed.iter().skip(start).take(PAGE_SIZE) {
        if let Some(note) = notes.get(*index) {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                note.index, note.path, note.topic
            ));
        }
    }
    out
}

pub(crate) fn page_count(changed: usize) -> usize {
    changed.div_ceil(PAGE_SIZE)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub notes: usize,
    pub edits: usize,
    pub changed: usize,
    pub topics: usize,
    pub concepts: usize,
    pub spans: usize,
    pub pages: usize,
    pub page_size: usize,
    pub impacts: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangedNote {
    pub note_id: String,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    pub requirement: String,
    pub status: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Protocol {
    pub samples: usize,
    pub warmup: usize,
    pub process_start_included: bool,
    pub percentile: usize,
    pub quantile_method: String,
    pub sample_unit: String,
    pub note: String,
}

impl Protocol {
    pub fn of(spec: &Spec) -> Self {
        Self {
            samples: spec.samples,
            warmup: spec.warmup,
            process_start_included: false,
            percentile: QUANTILE,
            quantile_method: "nearest_rank".to_owned(),
            sample_unit: "同一进程内对已建语料重复执行该阶段".to_owned(),
            note: "样本从预热之后的重复执行里取；进程启动、参数解析与语料生成都不计入阶段耗时。"
                .to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    pub policy: String,
    pub schema_version: u32,
    pub case_id: String,
    pub spec: Spec,
    pub counts: Counts,
    pub corpus_sha256: String,
    pub baseline_sha256: String,
    pub changed_notes: Vec<ChangedNote>,
    pub first_page_sha256: String,
    pub protocol: Protocol,
    pub threshold_millis: u128,
    pub coverage: Vec<Coverage>,
    pub model_calls: u32,
    pub network_requests: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub stage: String,
    pub samples_millis: Vec<u128>,
    pub p95_millis: u128,
    pub median_millis: u128,
    pub min_millis: u128,
    pub max_millis: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    pub requirement: String,
    pub stage: String,
    pub p95_millis: u128,
    pub threshold_millis: u128,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub rss_before_bytes: u64,
    pub rss_after_scan_bytes: u64,
    pub rss_after_plan_bytes: u64,
    pub peak_rss_bytes: Option<u64>,
    pub note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timings {
    pub policy: String,
    pub case_id: String,
    pub protocol: Protocol,
    pub stages: Vec<Stage>,
    pub threshold_millis: u128,
    pub verdicts: Vec<Verdict>,
    pub memory: Memory,
    pub note: String,
}

/// Nearest-rank percentile: sort the samples and take the `ceil(p/100 * n)`-th.
pub(crate) fn nearest_rank(samples: &[u128], percentile: usize) -> u128 {
    if samples.is_empty() {
        return 0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = ((percentile * sorted.len()).div_ceil(100)).max(1);
    sorted[rank.min(sorted.len()) - 1]
}

pub(crate) fn median(samples: &[u128]) -> u128 {
    if samples.is_empty() {
        return 0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn stage_of(name: &str, samples: Vec<u128>) -> Stage {
    Stage {
        stage: name.to_owned(),
        p95_millis: nearest_rank(&samples, QUANTILE),
        median_millis: median(&samples),
        min_millis: samples.iter().copied().min().unwrap_or(0),
        max_millis: samples.iter().copied().max().unwrap_or(0),
        samples_millis: samples,
    }
}

fn rss_of(field: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix(field) {
            let value = rest.trim().split_whitespace().next()?;
            let kib: u64 = value.parse().ok()?;
            return Some(kib * 1024);
        }
    }
    None
}

fn corpus_digest(notes: &[Note]) -> String {
    let mut joined = String::new();
    for note in notes {
        joined.push_str(&note.path);
        joined.push('\u{0}');
        joined.push_str(&note.body);
        joined.push('\u{0}');
    }
    text::sha256(joined.as_bytes())
}

fn baseline_digest(digests: &[String]) -> String {
    let mut joined = String::new();
    for digest in digests {
        joined.push_str(digest);
        joined.push('\n');
    }
    text::sha256(joined.as_bytes())
}

/// Run the four stages `samples + warmup` times and keep the clock readings.
pub(crate) fn measure(spec: &Spec) -> Result<(Structure, Timings), ProjectError> {
    let notes = generate(spec);
    let baseline = baseline_digests(&notes)?;
    let mut scan_samples = Vec::with_capacity(spec.samples);
    let mut detect_samples = Vec::with_capacity(spec.samples);
    let mut plan_samples = Vec::with_capacity(spec.samples);
    let mut page_samples = Vec::with_capacity(spec.samples);
    let mut memory: Option<Memory> = None;

    for round in 0..(spec.samples + spec.warmup) {
        let keep = round >= spec.warmup;
        let rss_before = rss_of("VmRSS:").unwrap_or(0);
        let started = Instant::now();
        let scans = scan(&notes)?;
        let scan_millis = started.elapsed().as_millis();
        let rss_after_scan = rss_of("VmRSS:").unwrap_or(0);

        let started = Instant::now();
        let changed = detect(&scans, &baseline);
        let detect_millis = started.elapsed().as_millis();

        let started = Instant::now();
        let impacted = impact(&notes, &changed);
        let plan_millis = started.elapsed().as_millis();
        let rss_after_plan = rss_of("VmRSS:").unwrap_or(0);

        let started = Instant::now();
        let page = page_of(&notes, &changed, 0);
        let page_millis = started.elapsed().as_millis();

        if keep {
            scan_samples.push(scan_millis);
            detect_samples.push(detect_millis);
            plan_samples.push(plan_millis);
            page_samples.push(page_millis);
        }
        if memory.is_none() && keep {
            memory = Some(Memory {
                rss_before_bytes: rss_before,
                rss_after_scan_bytes: rss_after_scan,
                rss_after_plan_bytes: rss_after_plan,
                peak_rss_bytes: rss_of("VmHWM:"),
                note: "内存读数是整个进程的常驻集与峰值（/proc/self/status 的 VmRSS/VmHWM），包含已建好的语料，不是单个阶段的分配量；平台不支持时峰值写 null。".to_owned(),
            });
        }
        if round + 1 == spec.samples + spec.warmup {
            let spans = scans.iter().map(|scan| scan.spans).sum();
            let counts = Counts {
                notes: notes.len(),
                edits: spec.edits,
                changed: changed.len(),
                topics: notes
                    .iter()
                    .map(|note| note.topic.clone())
                    .collect::<BTreeSet<_>>()
                    .len(),
                concepts: notes
                    .iter()
                    .flat_map(|note| note.concepts.iter().cloned())
                    .collect::<BTreeSet<_>>()
                    .len(),
                spans,
                pages: page_count(changed.len()),
                page_size: PAGE_SIZE,
                impacts: impacted.len(),
            };
            if counts.changed != spec.edits {
                return Err(integrity(format!(
                    "检测到 {} 处改动，但声明的是 {} 处：语料生成与检测不一致。",
                    counts.changed, spec.edits
                )));
            }
            let changed_notes: Vec<ChangedNote> = changed
                .iter()
                .filter_map(|index| notes.get(*index))
                .map(|note| ChangedNote {
                    note_id: format!("note-{:05}", note.index),
                    path: note.path.clone(),
                })
                .collect();
            let structure = Structure {
                policy: POLICY.to_owned(),
                schema_version: SCHEMA_VERSION,
                case_id: CASE_ID.to_owned(),
                spec: *spec,
                counts,
                corpus_sha256: corpus_digest(&notes),
                baseline_sha256: baseline_digest(&baseline),
                changed_notes,
                first_page_sha256: text::sha256(page.as_bytes()),
                protocol: Protocol::of(spec),
                threshold_millis: THRESHOLD_MILLIS,
                coverage: vec![
                    Coverage {
                        requirement: "NFR-11".to_owned(),
                        status: "partially_measured".to_owned(),
                        detail: nfr11_detail(spec),
                    },
                    Coverage {
                        requirement: "NFR-12".to_owned(),
                        status: "partially_measured".to_owned(),
                        detail: NFR12_DETAIL.to_owned(),
                    },
                ],
                model_calls: 0,
                network_requests: 0,
            };
            let stages = vec![
                stage_of(STAGE_SCAN, scan_samples),
                stage_of(STAGE_DETECT, detect_samples),
                stage_of(STAGE_PLAN, plan_samples),
                stage_of(STAGE_PAGE, page_samples),
            ];
            let verdicts: Vec<Verdict> = stages
                .iter()
                .map(|stage| Verdict {
                    requirement: "NFR-12".to_owned(),
                    stage: stage.stage.clone(),
                    p95_millis: stage.p95_millis,
                    threshold_millis: THRESHOLD_MILLIS,
                    status: if stage.p95_millis <= THRESHOLD_MILLIS {
                        "met".to_owned()
                    } else {
                        "not_met".to_owned()
                    },
                })
                .collect();
            let timings = Timings {
                policy: POLICY.to_owned(),
                case_id: CASE_ID.to_owned(),
                protocol: Protocol::of(spec),
                stages,
                threshold_millis: THRESHOLD_MILLIS,
                verdicts,
                memory: memory.take().unwrap_or(Memory {
                    rss_before_bytes: 0,
                    rss_after_scan_bytes: 0,
                    rss_after_plan_bytes: 0,
                    peak_rss_bytes: None,
                    note: "内存读数不可用。".to_owned(),
                }),
                note: "所有毫秒数来自本次真实运行，不参与确定性比较；结构产物只由参数决定。"
                    .to_owned(),
            };
            return Ok((structure, timings));
        }
    }
    Err(integrity("测量循环没有产生结果。"))
}

pub(crate) fn render_markdown(structure: &Structure) -> String {
    let counts = &structure.counts;
    let mut out = String::new();
    out.push_str(&format!("# 大库规模实测（{}）\n\n", structure.case_id));
    out.push_str("语料是**合成的**：笔记由参数生成，人工改动也是生成的。它证明这个规模上的结构与分页能算出来，");
    out.push_str("不证明真实笔记库的语言质量，也不证明界面不会冻结。\n\n");

    out.push_str("## 参数\n\n");
    out.push_str(&format!(
        "- 笔记数 {}、人工改动 {}、样本数 {}、预热 {}\n",
        structure.spec.notes, structure.spec.edits, structure.spec.samples, structure.spec.warmup
    ));
    out.push_str(&format!("- 分页大小 {PAGE_SIZE} 条/页\n\n"));

    out.push_str("## 语料与阶段\n\n");
    out.push_str(&format!(
        "- 主题 {}、概念 {}、文本片段 {}\n",
        counts.topics, counts.concepts, counts.spans
    ));
    out.push_str(&format!(
        "- 检测到改动 {} 处，落在 {} 页变更清单里\n",
        counts.changed, counts.pages
    ));
    out.push_str(&format!(
        "- 影响范围 {} 篇（与改动笔记共享概念或主题的笔记）\n\n",
        counts.impacts
    ));

    out.push_str("## 测量协议\n\n");
    out.push_str(&format!(
        "- 每个阶段重复 {} 次，前 {} 次作为预热丢弃\n",
        structure.protocol.samples, structure.protocol.warmup
    ));
    out.push_str("- 不含进程启动、参数解析与语料生成：计时只包住阶段本身\n");
    out.push_str(&format!(
        "- 分位数用 {} 的 P{}：样本排序后取第 ceil(P/100×n) 个\n",
        structure.protocol.quantile_method, structure.protocol.percentile
    ));
    out.push_str("- 阶段定义：scan 逐篇跑文本分析并取摘要；detect 与冻结基线比摘要；plan 汇总共享概念与主题的笔记；page 渲染变更清单第一页\n\n");

    out.push_str("## 门槛与判定规则\n\n");
    out.push_str(&format!(
        "- NFR-12 门槛 {} ms，逐个阶段比较 P95 与门槛；判定结果写在 timings.json，不写进本文件\n",
        structure.threshold_millis
    ));
    out.push_str("- 本文件不含本次运行的任何耗时读数：一旦写进来，同一参数两次运行就不再相同；上面那个门槛是常量，不是读数\n\n");

    out.push_str("## 覆盖范围与未达标项\n\n");
    for coverage in &structure.coverage {
        out.push_str(&format!(
            "- {}（{}）：{}\n",
            coverage.requirement, coverage.status, coverage.detail
        ));
    }
    out.push('\n');

    out.push_str("## 边界\n\n");
    out.push_str("- 不联网、不调用模型、不写项目与 Vault；只写这一个输出根。\n");
    out.push_str("- 合成语料不代表真实笔记库；没有真实 PDF 抽取，也没有真实人工编辑。\n");
    out.push_str("- 没有界面，因此「不因全量渲染冻结界面」没有被验证。\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(notes: usize, edits: usize, samples: usize, warmup: usize) -> Spec {
        Spec::new(notes, edits, samples, warmup).expect("spec")
    }

    #[test]
    fn the_same_parameters_produce_the_same_corpus() {
        let first = generate(&spec(40, 5, 2, 0));
        let second = generate(&spec(40, 5, 2, 0));
        assert_eq!(corpus_digest(&first), corpus_digest(&second));
    }

    #[test]
    fn edited_notes_differ_from_their_baseline() {
        let notes = generate(&spec(40, 5, 1, 0));
        let edited: Vec<usize> = notes
            .iter()
            .filter(|note| note.edited)
            .map(|note| note.index)
            .collect();
        assert_eq!(edited.len(), 5);
        for note in notes.iter().filter(|note| note.edited) {
            assert_ne!(note.body, note.baseline);
        }
        for note in notes.iter().filter(|note| !note.edited) {
            assert_eq!(note.body, note.baseline);
        }
    }

    #[test]
    fn every_edit_is_detected_and_nothing_else_is() {
        let declared = spec(40, 5, 1, 0);
        let notes = generate(&declared);
        let baseline = baseline_digests(&notes).expect("baseline");
        let scans = scan(&notes).expect("scan");
        let changed = detect(&scans, &baseline);
        assert_eq!(changed.len(), 5);
        assert_eq!(changed, edit_indices(40, 5));
    }

    #[test]
    fn impact_never_contains_the_changed_notes_themselves() {
        let declared = spec(40, 5, 1, 0);
        let notes = generate(&declared);
        let changed = edit_indices(40, 5);
        let impacted = impact(&notes, &changed);
        assert!(impacted.iter().all(|index| !changed.contains(index)));
        assert!(!impacted.is_empty());
    }

    #[test]
    fn the_page_holds_at_most_one_page_of_changes() {
        let declared = spec(4000, 250, 1, 0);
        let notes = generate(&declared);
        let changed: Vec<usize> = (0..250).collect();
        let page = page_of(&notes, &changed, 0);
        let rows = page.lines().filter(|line| line.starts_with("| ")).count();
        // Header row plus the separator row are not records.
        assert_eq!(rows - 2, PAGE_SIZE);
        assert_eq!(page_count(250), 3);
    }

    #[test]
    fn nearest_rank_picks_the_documented_sample() {
        assert_eq!(nearest_rank(&[5, 1, 3, 4, 2], 95), 5);
        assert_eq!(nearest_rank(&[10, 20, 30, 40], 50), 20);
        assert_eq!(median(&[9, 1, 5]), 5);
    }
}
