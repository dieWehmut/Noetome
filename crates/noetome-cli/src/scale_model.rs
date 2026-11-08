//! Deterministic synthetic corpora and the measurement rules for page scale.
//!
//! The harness answers one question honestly: at a requested number of pages,
//! what does the real text pipeline actually do — how many spans, headings and
//! concept occurrences come out, how long does it take, and how much memory does
//! it hold? Everything here is generated from the parameters alone, so two runs
//! with the same parameters produce the same bytes.
//!
//! The harness deliberately does **not** pretend the planner scales: the plan
//! verb keeps hard declaration caps, and the report prints them as unmet rather
//! than working around them.

use serde::{Deserialize, Serialize};

use crate::outline_model::{self, Concept, ConceptFile, DocumentRecord, DocumentView, SpanRecord};
use crate::project::ProjectError;
use crate::text::{self, SpanKind};

pub(crate) const POLICY: &str = "scale-harness-v1";
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const CASE_ID: &str = "scale-synthetic-v1";
pub(crate) const MAX_PAGES: usize = 5000;
pub(crate) const MAX_HEADINGS: usize = 12;
pub(crate) const MAX_PARAGRAPHS: usize = 12;
pub(crate) const MAX_OCCURRENCES: usize = 4096;

/// The planner caps this harness reports against. They are read from the plan
/// verb so the report cannot drift away from what that verb really accepts.
pub(crate) const PLAN_TOPICS: usize = crate::plan_model::MAX_TOPICS;
pub(crate) const PLAN_GOALS: usize = crate::plan_model::MAX_GOALS;
pub(crate) const PLAN_CONCEPTS: usize = crate::plan_model::MAX_CONCEPTS;
pub(crate) const PLAN_ATOMS: usize = crate::plan_model::MAX_ATOMS;

pub(crate) const NFR10_DETAIL: &str = "规划动词的声明上限是 topics=4、goals=8、concepts=64、atoms=256；\
千笔记、万原子的规划在本轮无法达成。本 harness 只证明文本分析与章节对齐在千页规模可用，\
不把页面处理当成规划已经达标。";

fn argument(message: impl Into<String>) -> ProjectError {
    ProjectError::new("SCALE_ARGUMENT", 2, message.into())
}

fn integrity(message: impl Into<String>) -> ProjectError {
    ProjectError::new("SCALE_INTEGRITY", 3, message.into())
}

pub(crate) fn unsafe_path(message: impl Into<String>) -> ProjectError {
    ProjectError::new("SCALE_UNSAFE_PATH", 3, message.into())
}

/// The three knobs that define a synthetic corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub pages: usize,
    pub headings_per_page: usize,
    pub paragraphs_per_page: usize,
}

impl Spec {
    pub(crate) fn new(
        pages: usize,
        headings_per_page: usize,
        paragraphs_per_page: usize,
    ) -> Result<Self, ProjectError> {
        if pages == 0 || pages > MAX_PAGES {
            return Err(argument(format!(
                "页数必须在 1 到 {MAX_PAGES} 之间：收到 {pages}。"
            )));
        }
        if headings_per_page == 0 || headings_per_page > MAX_HEADINGS {
            return Err(argument(format!(
                "每页标题数必须在 1 到 {MAX_HEADINGS} 之间：收到 {headings_per_page}。"
            )));
        }
        if paragraphs_per_page == 0 || paragraphs_per_page > MAX_PARAGRAPHS {
            return Err(argument(format!(
                "每页段落数必须在 1 到 {MAX_PARAGRAPHS} 之间：收到 {paragraphs_per_page}。"
            )));
        }
        Ok(Self {
            pages,
            headings_per_page,
            paragraphs_per_page,
        })
    }

    pub(crate) fn parse(
        pages: &str,
        headings: &str,
        paragraphs: &str,
    ) -> Result<Self, ProjectError> {
        let read = |value: &str, label: &str| -> Result<usize, ProjectError> {
            value
                .parse::<usize>()
                .map_err(|_| argument(format!("{label}必须是正整数：收到「{value}」。")))
        };
        Self::new(
            read(pages, "页数")?,
            read(headings, "每页标题数")?,
            read(paragraphs, "每页段落数")?,
        )
    }
}

/// A page ready to be analysed. `body` is generated from the index alone.
pub(crate) struct Page {
    pub(crate) index: usize,
    pub(crate) path: String,
    pub(crate) body: String,
}

const CONCEPTS: [(&str, &str); 8] = [
    ("concept-glycolysis", "糖酵解"),
    ("concept-gluconeogenesis", "糖异生"),
    ("concept-tca", "三羧酸循环"),
    ("concept-pfk", "磷酸果糖激酶"),
    ("concept-pyruvate", "丙酮酸"),
    ("concept-nadph", "NADPH"),
    ("concept-atp", "ATP"),
    ("concept-lactate", "乳酸"),
];

fn concept_file() -> ConceptFile {
    ConceptFile {
        policy: "scale-harness-concepts-v1".to_owned(),
        schema_version: SCHEMA_VERSION,
        concepts: CONCEPTS
            .iter()
            .map(|(concept_id, canonical)| Concept {
                concept_id: (*concept_id).to_owned(),
                canonical: (*canonical).to_owned(),
                aliases: Vec::new(),
            })
            .collect(),
    }
}

pub(crate) fn generate(spec: &Spec) -> Vec<Page> {
    let mut pages = Vec::with_capacity(spec.pages);
    for index in 1..=spec.pages {
        let mut body = String::new();
        for heading in 0..spec.headings_per_page {
            let concept = CONCEPTS[(index + heading) % CONCEPTS.len()].1;
            body.push_str(&format!(
                "## {}.{heading} {concept} 的第 {heading} 节\n\n",
                index
            ));
            for paragraph in 0..spec.paragraphs_per_page {
                let first = CONCEPTS[(index + heading + paragraph) % CONCEPTS.len()].1;
                let second = CONCEPTS[(index + heading + paragraph + 3) % CONCEPTS.len()].1;
                body.push_str(&format!(
                    "第 {paragraph} 段讨论{first}与{second}的关系，这一段是合成文本，\
                     只用来测量规模，不代表任何教材内容。\n\n"
                ));
            }
        }
        pages.push(Page {
            index,
            path: format!("合成/第{index:04}页.md"),
            body,
        });
    }
    pages
}

/// One page after the real text pipeline has looked at it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageRecord {
    pub index: usize,
    pub path: String,
    pub sha256: String,
    pub byte_length: usize,
    pub span_count: usize,
    pub headings: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub pages: usize,
    pub bytes: usize,
    pub spans: usize,
    pub headings: usize,
    pub paragraphs: usize,
    pub concepts: usize,
    pub occurrences: usize,
    pub warnings: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_topics: usize,
    pub max_goals: usize,
    pub max_concepts: usize,
    pub max_atoms: usize,
    pub detail: String,
}

/// The deterministic half of a measurement: everything here depends only on the
/// parameters, never on the clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    pub policy: String,
    pub schema_version: u32,
    pub case_id: String,
    pub spec: Spec,
    pub counts: Counts,
    pub pages: Vec<PageRecord>,
    pub warning_counts: std::collections::BTreeMap<String, usize>,
    pub limits: Limits,
    pub nfr10_status: String,
    pub model_calls: usize,
    pub network_requests: usize,
}

/// The non-deterministic half. It is written to its own file so a determinism
/// check never has to explain away a millisecond.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timings {
    pub analyse_millis: u128,
    pub align_millis: u128,
    pub total_millis: u128,
    pub peak_rss_bytes: Option<u64>,
    pub note: String,
}

/// Run the real pipeline over a generated corpus and keep both halves apart.
pub(crate) fn measure(spec: &Spec) -> Result<(Structure, Timings), ProjectError> {
    let generated = generate(spec);

    let analyse_start = std::time::Instant::now();
    let mut documents = Vec::with_capacity(generated.len());
    let mut pages = Vec::with_capacity(generated.len());
    let mut bodies: Vec<String> = Vec::with_capacity(generated.len());
    let mut bytes = 0usize;
    let mut spans = 0usize;
    let mut headings = 0usize;
    let mut paragraphs = 0usize;
    for page in &generated {
        let parsed = text::analyze(page.body.as_bytes())
            .map_err(|error| integrity(format!("第 {} 页不是合法 UTF-8：{error}", page.index)))?;
        let page_headings = parsed
            .spans
            .iter()
            .filter(|span| span.kind == SpanKind::Heading)
            .count();
        paragraphs += parsed
            .spans
            .iter()
            .filter(|span| span.kind == SpanKind::Text)
            .count();
        spans += parsed.spans.len();
        headings += page_headings;
        bytes += page.body.len();
        let spans_records = parsed
            .spans
            .iter()
            .enumerate()
            .map(|(index, span)| SpanRecord {
                span_id: format!("{:04}-{index:04}", page.index),
                kind: span.kind.as_str().to_owned(),
                start_byte: span.start_byte,
                end_byte: span.end_byte,
                start_line: span.start_line,
                end_line: span.end_line,
                text_sha256: span.text_sha256.clone(),
            })
            .collect();
        pages.push(PageRecord {
            index: page.index,
            path: page.path.clone(),
            sha256: text::sha256(page.body.as_bytes()),
            byte_length: page.body.len(),
            span_count: parsed.spans.len(),
            headings: page_headings,
        });
        documents.push(DocumentRecord {
            source_id: format!("page-{:04}", page.index),
            title: format!("第{:04}页", page.index),
            path: page.path.clone(),
            raw_sha256: text::sha256(page.body.as_bytes()),
            byte_length: page.body.len() as u64,
            normalized_sha256: parsed.normalized_sha256,
            span_count: parsed.spans.len(),
            spans: spans_records,
        });
        bodies.push(parsed.text);
    }
    let analyse_millis = analyse_start.elapsed().as_millis();

    let concepts = concept_file();
    let align_start = std::time::Instant::now();
    let views: Vec<DocumentView<'_>> = documents
        .iter()
        .zip(bodies.iter())
        .map(|(record, body)| DocumentView {
            record,
            text: body.as_str(),
        })
        .collect();
    let alignment = outline_model::align(&views, &concepts, MAX_OCCURRENCES)?;
    let align_millis = align_start.elapsed().as_millis();

    let occurrences: usize = alignment.inventory.iter().map(|entry| entry.total).sum();
    let mut warning_counts = std::collections::BTreeMap::new();
    for warning in &alignment.warnings {
        *warning_counts.entry(warning.kind.clone()).or_insert(0) += 1;
    }
    let counts = Counts {
        pages: generated.len(),
        bytes,
        spans,
        headings,
        paragraphs,
        concepts: concepts.concepts.len(),
        occurrences,
        warnings: alignment.warnings.len(),
    };
    let structure = Structure {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: CASE_ID.to_owned(),
        spec: *spec,
        counts,
        pages,
        warning_counts,
        limits: Limits {
            max_topics: PLAN_TOPICS,
            max_goals: PLAN_GOALS,
            max_concepts: PLAN_CONCEPTS,
            max_atoms: PLAN_ATOMS,
            detail: NFR10_DETAIL.to_owned(),
        },
        nfr10_status: "not_met".to_owned(),
        model_calls: 0,
        network_requests: 0,
    };
    let timings = Timings {
        analyse_millis,
        align_millis,
        total_millis: analyse_millis + align_millis,
        peak_rss_bytes: peak_rss_bytes(),
        note: "耗时与峰值内存来自本次真实运行，不参与确定性比较；结构产物只由参数决定。".to_owned(),
    };
    Ok((structure, timings))
}

fn peak_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kilobytes = rest.split_whitespace().next()?.parse::<u64>().ok()?;
            return Some(kilobytes.saturating_mul(1024));
        }
    }
    None
}

pub(crate) fn render_markdown(structure: &Structure) -> String {
    let mut out = String::new();
    out.push_str(&format!("# 规模实测（{}）\n\n", structure.case_id));
    out.push_str(&format!(
        "这是**合成语料**的实测：页面由参数生成，不含任何真实教材内容，也不证明真实教材的抽取质量。\n\n"
    ));
    out.push_str("## 参数\n\n");
    out.push_str(&format!(
        "- 页数 {}；每页标题 {}；每页段落 {}\n\n",
        structure.spec.pages, structure.spec.headings_per_page, structure.spec.paragraphs_per_page
    ));
    out.push_str("## 结构产物\n\n");
    out.push_str(&format!(
        "- 页面 {}、字节 {}、文本片段 {}、标题 {}、段落 {}、概念 {}、出现 {}、对齐警告 {}\n\n",
        structure.counts.pages,
        structure.counts.bytes,
        structure.counts.spans,
        structure.counts.headings,
        structure.counts.paragraphs,
        structure.counts.concepts,
        structure.counts.occurrences,
        structure.counts.warnings
    ));
    out.push_str("## 实测\n\n");
    out.push_str(concat!(
        "- 耗时与峰值内存写在 `timings.json`，本文件不复制它们：报告是摘要产物，\n",
        "  必须与结构产物一起保持逐字节确定，任何挂钟读数都会让同一参数两次运行不再相同。\n\n",
    ));
    out.push_str("## 规划上限（未达标项）\n\n");
    out.push_str(&format!(
        "- 结论：{}\n- 上限：topics={}、goals={}、concepts={}、atoms={}\n- {}\n\n",
        structure.nfr10_status,
        structure.limits.max_topics,
        structure.limits.max_goals,
        structure.limits.max_concepts,
        structure.limits.max_atoms,
        structure.limits.detail
    ));
    out.push_str("## 边界\n\n");
    out.push_str(
        "- 结构产物只由参数决定，两次同参数运行的字节相同；耗时单独成文件，不参与确定性比较。\n\
         - 页面是合成的，不证明真实教材的抽取质量，也不代表科学正确性。\n\
         - 不联网、不调用模型、不写项目与 Vault；本阶段不生成知识。\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Spec {
        Spec::new(4, 2, 2).expect("spec")
    }

    #[test]
    fn the_same_parameters_produce_the_same_pages() {
        let first = generate(&spec());
        let second = generate(&spec());
        assert_eq!(first.len(), 4);
        for (left, right) in first.iter().zip(second.iter()) {
            assert_eq!(left.body, right.body);
            assert_eq!(left.path, right.path);
        }
    }

    #[test]
    fn a_different_parameter_changes_the_bytes() {
        let first = generate(&Spec::new(4, 2, 2).expect("spec"));
        let second = generate(&Spec::new(4, 2, 3).expect("spec"));
        assert_ne!(first[0].body, second[0].body);
    }

    #[test]
    fn page_bounds_are_enforced() {
        assert_eq!(
            Spec::new(0, 1, 1).expect_err("zero pages").code(),
            "SCALE_ARGUMENT"
        );
        assert_eq!(
            Spec::new(MAX_PAGES + 1, 1, 1)
                .expect_err("too many pages")
                .code(),
            "SCALE_ARGUMENT"
        );
        assert_eq!(
            Spec::new(1, 0, 1).expect_err("no headings").code(),
            "SCALE_ARGUMENT"
        );
        assert_eq!(
            Spec::new(1, 1, 13).expect_err("too many paragraphs").code(),
            "SCALE_ARGUMENT"
        );
    }

    #[test]
    fn every_page_gets_a_heading_and_its_own_digest() {
        let (structure, timings) = measure(&spec()).expect("measure");
        assert_eq!(structure.counts.pages, 4);
        assert_eq!(structure.pages.len(), 4);
        assert!(structure.counts.headings >= 8);
        assert!(structure.counts.spans > 0);
        assert_eq!(structure.nfr10_status, "not_met");
        assert_eq!(structure.limits.max_atoms, 256);
        assert!(timings.total_millis <= timings.analyse_millis + timings.align_millis + 1);
        let mut digests: Vec<_> = structure
            .pages
            .iter()
            .map(|page| page.sha256.clone())
            .collect();
        digests.sort();
        digests.dedup();
        assert_eq!(digests.len(), 4);
    }

    #[test]
    fn the_markdown_states_the_synthetic_boundary_and_the_unmet_limit() {
        let (structure, timings) = measure(&spec()).expect("measure");
        let markdown = render_markdown(&structure);
        assert!(markdown.contains("合成语料"));
        assert!(markdown.contains("未达标项"));
        assert!(markdown.contains("atoms=256"));
        assert!(markdown.contains("timings.json"));
    }
}
