//! Structure templates for one topic with several reading goals.
//!
//! The plan answers three questions and refuses to guess when it cannot:
//! which note carries the full explanation of a concept, which sections each
//! note actually fills, and which structural differences from the template are
//! declared rather than accidental. A template is a structure specification, not
//! a form to fill: inapplicable sections may be omitted, but a section that is
//! declared must hold at least one atom, and a change of order or an extra
//! section needs a recorded reason.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::project::ProjectError;

pub(crate) const POLICY: &str = "structure-template-v1";
pub(crate) const AUTHOR_PRODUCER: &str = "fixture_author";
pub(crate) const PLAN_VERSION: u32 = 1;
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_TEMPLATES: usize = 4;
pub(crate) const MAX_TOPICS: usize = 4;
pub(crate) const MAX_GOALS: usize = 12;
pub(crate) const MAX_CONCEPTS: usize = 64;
pub(crate) const MAX_ATOMS: usize = 256;
pub(crate) const MAX_ID: usize = 48;
pub(crate) const MAX_TEXT: usize = 600;
pub(crate) const MAX_TITLE: usize = 200;
pub(crate) const MAX_SECTIONS: usize = 12;
pub(crate) const MAX_DEPTH: usize = 4;

pub(crate) const KIND_OVERVIEW: &str = "overview";
pub(crate) const KIND_REACTION: &str = "reaction";
pub(crate) const KIND_REGULATION: &str = "regulation";
pub(crate) const KIND_ENERGY: &str = "energy";
pub(crate) const KIND_COMPARISON: &str = "comparison";

pub(crate) const ROLE_EXPLANATION: &str = "explanation";
pub(crate) const ROLE_REFERENCE: &str = "reference";
pub(crate) const ROLE_EMBED: &str = "embed";

pub(crate) const DEVIATION_OMITTED: &str = "section_omitted";
pub(crate) const DEVIATION_SPECIAL: &str = "special_section";
pub(crate) const DEVIATION_TITLE: &str = "title_deviation";
pub(crate) const DEVIATION_DEPTH: &str = "depth_deviation";

pub(crate) const FINDING_EMPTY: &str = "empty_section";
pub(crate) const FINDING_UNREASONED: &str = "unreasoned_structure";
pub(crate) const FINDING_INCONSISTENT: &str = "inconsistent_structure";
pub(crate) const FINDING_UNEXPLAINED: &str = "concept_unexplained";
pub(crate) const FINDING_DUPLICATE: &str = "duplicate_explanation";

pub(crate) fn argument(message: impl Into<String>) -> ProjectError {
    ProjectError::new("TEMPLATE_ARGUMENT", 2, message.into())
}

pub(crate) fn integrity(message: impl Into<String>) -> ProjectError {
    ProjectError::new("TEMPLATE_INTEGRITY", 3, message.into())
}

pub(crate) fn unknown_root(message: impl Into<String>) -> ProjectError {
    ProjectError::new("TEMPLATE_UNKNOWN_ROOT", 3, message.into())
}

pub(crate) fn section_order(message: impl Into<String>) -> ProjectError {
    ProjectError::new("TEMPLATE_SECTION_ORDER", 3, message.into())
}

pub(crate) fn empty_section(message: impl Into<String>) -> ProjectError {
    ProjectError::new("TEMPLATE_EMPTY_SECTION", 3, message.into())
}

pub(crate) fn unreasoned(message: impl Into<String>) -> ProjectError {
    ProjectError::new("TEMPLATE_UNREASONED", 3, message.into())
}

pub(crate) fn inconsistent(message: impl Into<String>) -> ProjectError {
    ProjectError::new("TEMPLATE_INCONSISTENT", 3, message.into())
}

pub(crate) fn unexplained(message: impl Into<String>) -> ProjectError {
    ProjectError::new("TEMPLATE_UNEXPLAINED", 3, message.into())
}

pub(crate) fn duplicate(message: impl Into<String>) -> ProjectError {
    ProjectError::new("TEMPLATE_DUPLICATE_EXPLANATION", 3, message.into())
}

fn single(text: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    if text.trim().is_empty() || text.trim() != text {
        return Err(argument(format!("{label} 必须是非空且前后无空白的文本。")));
    }
    if text.chars().count() > limit {
        return Err(argument(format!("{label} 超过 {limit} 个字符。")));
    }
    if text.chars().any(char::is_control) {
        return Err(argument(format!("{label} 不能包含控制字符。")));
    }
    Ok(())
}

fn identifier(text: &str, label: &str) -> Result<(), ProjectError> {
    single(text, label, MAX_ID)?;
    if !text
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || value == '-' || value == '_')
    {
        return Err(argument(format!(
            "{label} 只能包含 ASCII 字母、数字、连字符与下划线：{text}"
        )));
    }
    Ok(())
}

fn relative(text: &str, label: &str) -> Result<(), ProjectError> {
    single(text, label, MAX_TEXT)?;
    if text.starts_with('/') || text.contains('\\') || text.contains(':') {
        return Err(argument(format!(
            "{label} 必须是相对路径且不能含盘符或反斜杠：{text}"
        )));
    }
    if text
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(argument(format!(
            "{label} 不能包含空路径段、. 或 ..：{text}"
        )));
    }
    Ok(())
}

pub(crate) fn kind_label(kind: &str) -> &'static str {
    match kind {
        KIND_OVERVIEW => "总览",
        KIND_REACTION => "反应",
        KIND_REGULATION => "调控",
        KIND_ENERGY => "能量",
        KIND_COMPARISON => "比较",
        _ => "未知",
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TemplateSpec {
    pub(crate) template_id: String,
    pub(crate) title: String,
    pub(crate) sections: Vec<String>,
    pub(crate) navigation_depth: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GoalSpec {
    pub(crate) goal_id: String,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) sections: Vec<String>,
    pub(crate) navigation_depth: usize,
    #[serde(default)]
    pub(crate) special_reason: Option<String>,
    #[serde(default)]
    pub(crate) title_reason: Option<String>,
    #[serde(default)]
    pub(crate) depth_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TopicSpec {
    pub(crate) topic_id: String,
    pub(crate) title: String,
    pub(crate) folder: String,
    pub(crate) template_id: String,
    pub(crate) level: usize,
    pub(crate) goals: Vec<GoalSpec>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConceptSpec {
    pub(crate) concept_id: String,
    pub(crate) canonical: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AtomSpec {
    pub(crate) atom_id: String,
    pub(crate) concept_id: String,
    pub(crate) goal_id: String,
    pub(crate) section: String,
    pub(crate) role: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Limits {
    pub(crate) max_templates: usize,
    pub(crate) max_topics: usize,
    pub(crate) max_goals: usize,
    pub(crate) max_concepts: usize,
    pub(crate) max_atoms: usize,
}

impl Limits {
    fn validate(&self) -> Result<(), ProjectError> {
        if self.max_templates == 0 || self.max_templates > MAX_TEMPLATES {
            return Err(argument("max_templates 必须在 1..=4 之间。"));
        }
        if self.max_topics == 0 || self.max_topics > MAX_TOPICS {
            return Err(argument("max_topics 必须在 1..=4 之间。"));
        }
        if self.max_goals == 0 || self.max_goals > MAX_GOALS {
            return Err(argument("max_goals 必须在 1..=12 之间。"));
        }
        if self.max_concepts == 0 || self.max_concepts > MAX_CONCEPTS {
            return Err(argument("max_concepts 必须在 1..=64 之间。"));
        }
        if self.max_atoms == 0 || self.max_atoms > MAX_ATOMS {
            return Err(argument("max_atoms 必须在 1..=256 之间。"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Plan {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) folder_root: String,
    pub(crate) templates: Vec<TemplateSpec>,
    pub(crate) topics: Vec<TopicSpec>,
    pub(crate) concepts: Vec<ConceptSpec>,
    pub(crate) atoms: Vec<AtomSpec>,
    pub(crate) limits: Limits,
}

impl Plan {
    pub(crate) fn parse(text: &str) -> Result<Self, ProjectError> {
        let plan: Plan = serde_json::from_str(text)
            .map_err(|error| argument(format!("规划声明不是合法 JSON：{error}。")))?;
        plan.validate()?;
        Ok(plan)
    }

    pub(crate) fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!("policy 必须是 {POLICY}。")));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "schema_version 必须是 {SCHEMA_VERSION}。"
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!("producer 必须是 {AUTHOR_PRODUCER}。")));
        }
        identifier(&self.case_id, "case_id")?;
        single(&self.description, "description", MAX_TEXT)?;
        relative(&self.folder_root, "folder_root")?;
        self.limits.validate()?;

        if self.templates.is_empty() || self.templates.len() > self.limits.max_templates {
            return Err(argument("模板数量必须在 1..=max_templates 之间。"));
        }
        let mut template_ids: BTreeSet<&str> = BTreeSet::new();
        for template in &self.templates {
            identifier(&template.template_id, "template_id")?;
            if !template_ids.insert(template.template_id.as_str()) {
                return Err(argument(format!(
                    "模板 {template_id} 重复。",
                    template_id = template.template_id
                )));
            }
            single(&template.title, "模板标题", MAX_TITLE)?;
            if template.sections.is_empty() || template.sections.len() > MAX_SECTIONS {
                return Err(argument(format!(
                    "模板 {} 的章节数量必须在 1..=12 之间。",
                    template.template_id
                )));
            }
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            for section in &template.sections {
                single(section, "章节名", MAX_TITLE)?;
                if !seen.insert(section.as_str()) {
                    return Err(argument(format!(
                        "模板 {} 里章节「{section}」重复。",
                        template.template_id
                    )));
                }
            }
            if template.navigation_depth == 0 || template.navigation_depth > MAX_DEPTH {
                return Err(argument("模板的 navigation_depth 必须在 1..=4 之间。"));
            }
        }

        if self.topics.is_empty() || self.topics.len() > self.limits.max_topics {
            return Err(argument("主题数量必须在 1..=max_topics 之间。"));
        }
        let mut topic_ids: BTreeSet<&str> = BTreeSet::new();
        let mut folders: BTreeSet<&str> = BTreeSet::new();
        let mut goal_ids: BTreeSet<&str> = BTreeSet::new();
        let mut goal_count = 0usize;
        for topic in &self.topics {
            identifier(&topic.topic_id, "topic_id")?;
            if !topic_ids.insert(topic.topic_id.as_str()) {
                return Err(argument(format!("主题 {} 重复。", topic.topic_id)));
            }
            single(&topic.title, "主题标题", MAX_TITLE)?;
            relative(&topic.folder, "主题目录")?;
            if !folders.insert(topic.folder.as_str()) {
                return Err(argument(format!(
                    "主题目录 {} 被多个主题占用。",
                    topic.folder
                )));
            }
            if !template_ids.contains(topic.template_id.as_str()) {
                return Err(argument(format!(
                    "主题 {} 引用了不存在的模板 {}。",
                    topic.topic_id, topic.template_id
                )));
            }
            if topic.level == 0 || topic.level > MAX_DEPTH {
                return Err(argument("主题的 level 必须在 1..=4 之间。"));
            }
            if topic.goals.is_empty() {
                return Err(argument(format!(
                    "主题 {} 至少需要一个阅读目标。",
                    topic.topic_id
                )));
            }
            for goal in &topic.goals {
                goal_count += 1;
                identifier(&goal.goal_id, "goal_id")?;
                if !goal_ids.insert(goal.goal_id.as_str()) {
                    return Err(argument(format!("阅读目标 {} 重复。", goal.goal_id)));
                }
                if !matches!(
                    goal.kind.as_str(),
                    KIND_OVERVIEW | KIND_REACTION | KIND_REGULATION | KIND_ENERGY | KIND_COMPARISON
                ) {
                    return Err(argument(format!(
                        "阅读目标 {} 的 kind 不受支持：{}。",
                        goal.goal_id, goal.kind
                    )));
                }
                single(&goal.title, "阅读目标标题", MAX_TITLE)?;
                if goal.sections.is_empty() || goal.sections.len() > MAX_SECTIONS {
                    return Err(argument(format!(
                        "阅读目标 {} 的章节数量必须在 1..=12 之间。",
                        goal.goal_id
                    )));
                }
                let mut seen: BTreeSet<&str> = BTreeSet::new();
                for section in &goal.sections {
                    single(section, "章节名", MAX_TITLE)?;
                    if !seen.insert(section.as_str()) {
                        return Err(argument(format!(
                            "阅读目标 {} 里章节「{section}」重复。",
                            goal.goal_id
                        )));
                    }
                }
                if goal.navigation_depth == 0 || goal.navigation_depth > MAX_DEPTH {
                    return Err(argument("阅读目标的 navigation_depth 必须在 1..=4 之间。"));
                }
            }
        }
        if goal_count > self.limits.max_goals {
            return Err(argument("阅读目标数量超过 max_goals。"));
        }

        if self.concepts.is_empty() || self.concepts.len() > self.limits.max_concepts {
            return Err(argument("概念数量必须在 1..=max_concepts 之间。"));
        }
        let mut concept_ids: BTreeSet<&str> = BTreeSet::new();
        for concept in &self.concepts {
            identifier(&concept.concept_id, "concept_id")?;
            if !concept_ids.insert(concept.concept_id.as_str()) {
                return Err(argument(format!("概念 {} 重复。", concept.concept_id)));
            }
            single(&concept.canonical, "概念正名", MAX_TITLE)?;
        }

        if self.atoms.is_empty() || self.atoms.len() > self.limits.max_atoms {
            return Err(argument("原子数量必须在 1..=max_atoms 之间。"));
        }
        let mut atom_ids: BTreeSet<&str> = BTreeSet::new();
        for atom in &self.atoms {
            identifier(&atom.atom_id, "atom_id")?;
            if !atom_ids.insert(atom.atom_id.as_str()) {
                return Err(argument(format!("原子 {} 重复。", atom.atom_id)));
            }
            if !concept_ids.contains(atom.concept_id.as_str()) {
                return Err(argument(format!(
                    "原子 {} 引用了不存在的概念 {}。",
                    atom.atom_id, atom.concept_id
                )));
            }
            if !goal_ids.contains(atom.goal_id.as_str()) {
                return Err(argument(format!(
                    "原子 {} 引用了不存在的阅读目标 {}。",
                    atom.atom_id, atom.goal_id
                )));
            }
            single(&atom.section, "原子所在章节", MAX_TITLE)?;
            if !matches!(
                atom.role.as_str(),
                ROLE_EXPLANATION | ROLE_REFERENCE | ROLE_EMBED
            ) {
                return Err(argument(format!(
                    "原子 {} 的 role 不受支持：{}。",
                    atom.atom_id, atom.role
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn goal(&self, goal_id: &str) -> Option<(&TopicSpec, &GoalSpec)> {
        self.topics.iter().find_map(|topic| {
            topic
                .goals
                .iter()
                .find(|goal| goal.goal_id == goal_id)
                .map(|goal| (topic, goal))
        })
    }

    pub(crate) fn template(&self, template_id: &str) -> Option<&TemplateSpec> {
        self.templates
            .iter()
            .find(|template| template.template_id == template_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SectionRecord {
    pub(crate) section: String,
    pub(crate) atoms: Vec<String>,
    pub(crate) special: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReferenceRecord {
    pub(crate) concept_id: String,
    pub(crate) target_note: String,
    pub(crate) role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GoalRecord {
    pub(crate) goal_id: String,
    pub(crate) topic_id: String,
    pub(crate) template_id: String,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) path: String,
    pub(crate) navigation_depth: usize,
    pub(crate) sections: Vec<SectionRecord>,
    pub(crate) atoms: Vec<String>,
    pub(crate) references: Vec<ReferenceRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TopicRecord {
    pub(crate) topic_id: String,
    pub(crate) title: String,
    pub(crate) folder: String,
    pub(crate) template_id: String,
    pub(crate) level: usize,
    pub(crate) goals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExplanationRecord {
    pub(crate) concept_id: String,
    pub(crate) canonical: String,
    pub(crate) note: String,
    pub(crate) references: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanFile {
    pub(crate) policy: String,
    pub(crate) plan_version: u32,
    pub(crate) case_id: String,
    pub(crate) folder_root: String,
    pub(crate) topics: Vec<TopicRecord>,
    pub(crate) goals: Vec<GoalRecord>,
    pub(crate) explanations: Vec<ExplanationRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Deviation {
    pub(crate) kind: String,
    pub(crate) subject: String,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Counts {
    pub(crate) templates: usize,
    pub(crate) topics: usize,
    pub(crate) goals: usize,
    pub(crate) notes: usize,
    pub(crate) concepts: usize,
    pub(crate) atoms: usize,
    pub(crate) explanations: usize,
    pub(crate) references: usize,
    pub(crate) embeds: usize,
    pub(crate) sections: usize,
    pub(crate) special_sections: usize,
    pub(crate) omitted_sections: usize,
    pub(crate) deviations: usize,
    pub(crate) findings: usize,
    pub(crate) model_calls: usize,
    pub(crate) network_requests: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Report {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) verdict: String,
    pub(crate) counts: Counts,
    pub(crate) deviations: Vec<Deviation>,
    pub(crate) findings: Vec<Deviation>,
}

#[derive(Debug, Clone)]
pub(crate) struct Planner {
    pub(crate) plan_file: PlanFile,
    pub(crate) report: Report,
}

fn note_path(folder_root: &str, topic: &TopicSpec, index: usize, goal: &GoalSpec) -> String {
    format!(
        "{folder_root}/{folder}/{index:02}-{goal_id}.md",
        folder = topic.folder,
        index = index + 1,
        goal_id = goal.goal_id
    )
}

/// Plan one topic's reading goals against its template.
///
/// Omission is an affordance: a section may be left out when it does not apply,
/// and every omission is reported. What is *not* free is changing the relative
/// order of the sections that remain, or adding a section the template does not
/// know — both need a recorded reason, because they break the promise that two
/// topics of the same type read the same way.
pub(crate) fn plan(declaration: &Plan) -> Result<Planner, ProjectError> {
    let mut topics = Vec::new();
    let mut goals = Vec::new();
    let mut deviations = Vec::new();
    let mut findings = Vec::new();
    let mut sections_total = 0usize;
    let mut special_total = 0usize;
    let mut omitted_total = 0usize;

    let mut atoms_by_goal: BTreeMap<&str, Vec<&AtomSpec>> = BTreeMap::new();
    for atom in &declaration.atoms {
        atoms_by_goal
            .entry(atom.goal_id.as_str())
            .or_default()
            .push(atom);
    }

    for topic in &declaration.topics {
        let template = declaration
            .template(&topic.template_id)
            .ok_or_else(|| argument(format!("主题 {} 引用了不存在的模板。", topic.topic_id)))?;
        let mut goal_ids = Vec::new();
        for (index, goal) in topic.goals.iter().enumerate() {
            goal_ids.push(goal.goal_id.clone());
            let path = note_path(&declaration.folder_root, topic, index, goal);

            // Sections known to the template must keep the template's relative order.
            let mut positions = Vec::new();
            let mut unknown = Vec::new();
            for section in &goal.sections {
                match template.sections.iter().position(|value| value == section) {
                    Some(position) => positions.push((position, section)),
                    None => unknown.push(section),
                }
            }
            if positions.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
                return Err(section_order(format!(
                    "阅读目标 {} 的章节顺序与模板 {} 不一致：模板顺序是 {}，声明顺序是 {}。省略章节可以，改动顺序必须换模板或先改模板。",
                    goal.goal_id,
                    template.template_id,
                    template.sections.join(" → "),
                    goal.sections.join(" → ")
                )));
            }
            if !unknown.is_empty() {
                match &goal.special_reason {
                    Some(reason) => {
                        single(reason, "special_reason", MAX_TEXT)?;
                        for section in &unknown {
                            special_total += 1;
                            deviations.push(Deviation {
                                kind: DEVIATION_SPECIAL.to_owned(),
                                subject: format!("{}#{section}", goal.goal_id),
                                detail: format!("模板外章节「{section}」：{reason}"),
                            });
                        }
                    }
                    None => {
                        return Err(unreasoned(format!(
                            "阅读目标 {} 声明了模板 {} 里没有的章节：{}。特殊结构必须记录特殊理由（special_reason），否则无法区分设计选择与笔误。",
                            goal.goal_id,
                            template.template_id,
                            unknown
                                .iter()
                                .map(|value| value.as_str())
                                .collect::<Vec<_>>()
                                .join("、")
                        )));
                    }
                }
            }

            // Declared sections must actually hold atoms: a template is a
            // structure specification, not a form to fill with empty headings.
            let goal_atoms = atoms_by_goal
                .get(goal.goal_id.as_str())
                .cloned()
                .unwrap_or_default();
            let mut records = Vec::new();
            for section in &goal.sections {
                let atoms: Vec<String> = goal_atoms
                    .iter()
                    .filter(|atom| &atom.section == section)
                    .map(|atom| atom.atom_id.clone())
                    .collect();
                if atoms.is_empty() {
                    return Err(empty_section(format!(
                        "阅读目标 {} 的章节「{section}」没有任何原子：不适用的章节应当省略，而不是留一个空标题凑齐模板。",
                        goal.goal_id
                    )));
                }
                sections_total += 1;
                records.push(SectionRecord {
                    section: section.clone(),
                    atoms,
                    special: unknown.contains(&section),
                });
            }
            for atom in &goal_atoms {
                if !goal.sections.iter().any(|section| section == &atom.section) {
                    return Err(argument(format!(
                        "原子 {} 声明在章节「{}」，但阅读目标 {} 没有列出这个章节。",
                        atom.atom_id, atom.section, goal.goal_id
                    )));
                }
            }

            let expected_title = format!("{}：{}", topic.title, kind_label(&goal.kind));
            if goal.title != expected_title {
                match &goal.title_reason {
                    Some(reason) => {
                        single(reason, "title_reason", MAX_TEXT)?;
                        deviations.push(Deviation {
                            kind: DEVIATION_TITLE.to_owned(),
                            subject: goal.goal_id.clone(),
                            detail: format!(
                                "标题「{}」而非「{expected_title}」：{reason}",
                                goal.title
                            ),
                        });
                    }
                    None => {
                        return Err(unreasoned(format!(
                            "阅读目标 {} 的标题「{}」与同类同层模板的命名「{expected_title}」不一致，且没有给出 title_reason。",
                            goal.goal_id, goal.title
                        )));
                    }
                }
            }
            if goal.navigation_depth != template.navigation_depth {
                match &goal.depth_reason {
                    Some(reason) => {
                        single(reason, "depth_reason", MAX_TEXT)?;
                        deviations.push(Deviation {
                            kind: DEVIATION_DEPTH.to_owned(),
                            subject: goal.goal_id.clone(),
                            detail: format!(
                                "导航深度 {} 而非模板的 {}：{reason}",
                                goal.navigation_depth, template.navigation_depth
                            ),
                        });
                    }
                    None => {
                        return Err(unreasoned(format!(
                            "阅读目标 {} 的导航深度 {} 与模板 {} 的 {} 不一致，且没有给出 depth_reason。",
                            goal.goal_id,
                            goal.navigation_depth,
                            template.template_id,
                            template.navigation_depth
                        )));
                    }
                }
            }

            for section in &template.sections {
                if !goal.sections.iter().any(|value| value == section) {
                    omitted_total += 1;
                    deviations.push(Deviation {
                        kind: DEVIATION_OMITTED.to_owned(),
                        subject: format!("{}#{section}", goal.goal_id),
                        detail: format!(
                            "省略模板章节「{section}」：该章节不适用于这个阅读目标，省略是允许的。"
                        ),
                    });
                }
            }

            goals.push(GoalRecord {
                goal_id: goal.goal_id.clone(),
                topic_id: topic.topic_id.clone(),
                template_id: template.template_id.clone(),
                kind: goal.kind.clone(),
                title: goal.title.clone(),
                path,
                navigation_depth: goal.navigation_depth,
                sections: records,
                atoms: goal_atoms.iter().map(|atom| atom.atom_id.clone()).collect(),
                references: Vec::new(),
            });
        }
        topics.push(TopicRecord {
            topic_id: topic.topic_id.clone(),
            title: topic.title.clone(),
            folder: topic.folder.clone(),
            template_id: topic.template_id.clone(),
            level: topic.level,
            goals: goal_ids,
        });
    }

    // Same template and level must read the same: the sequence of template sections
    // that survived must be identical for the same goal kind.
    let mut shapes: BTreeMap<(String, usize, String), Vec<String>> = BTreeMap::new();
    for topic in &declaration.topics {
        for goal in &topic.goals {
            let template = declaration
                .template(&topic.template_id)
                .ok_or_else(|| argument("主题引用了不存在的模板。"))?;
            let shape: Vec<String> = goal
                .sections
                .iter()
                .filter(|section| template.sections.iter().any(|value| value == *section))
                .cloned()
                .collect();
            let key = (topic.template_id.clone(), topic.level, goal.kind.clone());
            match shapes.get(&key) {
                None => {
                    shapes.insert(key, shape);
                }
                Some(first) => {
                    if first != &shape {
                        let has_reason = goal.special_reason.is_some();
                        let detail = format!(
                            "同类同层的「{}」结构是 {}，这里是 {}",
                            goal.kind,
                            first.join(" → "),
                            shape.join(" → ")
                        );
                        if has_reason {
                            deviations.push(Deviation {
                                kind: DEVIATION_SPECIAL.to_owned(),
                                subject: goal.goal_id.clone(),
                                detail: format!("{detail}（已声明理由）。"),
                            });
                        } else {
                            findings.push(Deviation {
                                kind: FINDING_INCONSISTENT.to_owned(),
                                subject: goal.goal_id.clone(),
                                detail: format!("{detail}，但没有 special_reason。"),
                            });
                        }
                    }
                }
            }
        }
    }

    // Every concept must have exactly one full explanation; references point at it.
    let mut explanations = Vec::new();
    for concept in &declaration.concepts {
        let owners: Vec<&AtomSpec> = declaration
            .atoms
            .iter()
            .filter(|atom| atom.concept_id == concept.concept_id && atom.role == ROLE_EXPLANATION)
            .collect();
        if owners.is_empty() {
            findings.push(Deviation {
                kind: FINDING_UNEXPLAINED.to_owned(),
                subject: concept.concept_id.clone(),
                detail: format!(
                    "概念「{}」没有任何 explanation 原子：规划里必须先决定它在哪份笔记里被完整解释。",
                    concept.canonical
                ),
            });
            continue;
        }
        if owners.len() > 1 {
            let places: Vec<String> = owners
                .iter()
                .map(|atom| {
                    goals
                        .iter()
                        .find(|goal| goal.goal_id == atom.goal_id)
                        .map(|goal| goal.path.clone())
                        .unwrap_or_else(|| atom.goal_id.clone())
                })
                .collect();
            findings.push(Deviation {
                kind: FINDING_DUPLICATE.to_owned(),
                subject: concept.concept_id.clone(),
                detail: format!(
                    "概念「{}」在 {} 处声明了完整解释：{}。其余位置应当写成 reference 或 embed。",
                    concept.canonical,
                    owners.len(),
                    places.join("、")
                ),
            });
            continue;
        }
        let owner = owners[0];
        let goal = goals
            .iter()
            .find(|goal| goal.goal_id == owner.goal_id)
            .ok_or_else(|| argument("原子引用了不存在的阅读目标。"))?;
        let canonical_note = goal.path.clone();
        let mut references = 0usize;
        for atom in declaration
            .atoms
            .iter()
            .filter(|atom| atom.concept_id == concept.concept_id && atom.role != ROLE_EXPLANATION)
        {
            if atom.goal_id == owner.goal_id {
                return Err(argument(format!(
                    "原子 {} 在完整解释所在笔记 {} 里又声明为 {}：同一份笔记里引用自己会把链接变成噪音。",
                    atom.atom_id, goal.goal_id, atom.role
                )));
            }
            references += 1;
        }
        explanations.push(ExplanationRecord {
            concept_id: concept.concept_id.clone(),
            canonical: concept.canonical.clone(),
            note: canonical_note,
            references,
        });
    }

    // Fold the references into the owning goal records.
    for explanation in &explanations {
        for atom in declaration.atoms.iter().filter(|atom| {
            atom.concept_id == explanation.concept_id && atom.role != ROLE_EXPLANATION
        }) {
            if let Some(goal) = goals.iter_mut().find(|goal| goal.goal_id == atom.goal_id) {
                goal.references.push(ReferenceRecord {
                    concept_id: explanation.concept_id.clone(),
                    target_note: explanation.note.clone(),
                    role: atom.role.clone(),
                });
            }
        }
    }

    let references_total = declaration
        .atoms
        .iter()
        .filter(|atom| atom.role == ROLE_REFERENCE)
        .count();
    let embeds_total = declaration
        .atoms
        .iter()
        .filter(|atom| atom.role == ROLE_EMBED)
        .count();

    let counts = Counts {
        templates: declaration.templates.len(),
        topics: declaration.topics.len(),
        goals: declaration
            .topics
            .iter()
            .map(|topic| topic.goals.len())
            .sum(),
        notes: goals.len(),
        concepts: declaration.concepts.len(),
        atoms: declaration.atoms.len(),
        explanations: explanations.len(),
        references: references_total,
        embeds: embeds_total,
        sections: sections_total,
        special_sections: special_total,
        omitted_sections: omitted_total,
        deviations: deviations.len(),
        findings: findings.len(),
        model_calls: 0,
        network_requests: 0,
    };

    let report = Report {
        policy: POLICY.to_owned(),
        schema_version: SCHEMA_VERSION,
        case_id: declaration.case_id.clone(),
        description: declaration.description.clone(),
        verdict: if findings.is_empty() {
            "pass".to_owned()
        } else {
            "findings".to_owned()
        },
        counts,
        deviations,
        findings,
    };

    let plan_file = PlanFile {
        policy: POLICY.to_owned(),
        plan_version: PLAN_VERSION,
        case_id: declaration.case_id.clone(),
        folder_root: declaration.folder_root.clone(),
        topics,
        goals,
        explanations,
    };

    Ok(Planner { plan_file, report })
}

pub(crate) fn render_markdown(planner: &Planner) -> String {
    let report = &planner.report;
    let counts = &report.counts;
    let mut text = String::new();
    text.push_str(&format!(
        "# 主题阅读目标与结构模板（{}）\n\n",
        report.case_id
    ));
    text.push_str(&format!("{}\n\n", report.description));
    text.push_str("## 规划规模\n\n");
    text.push_str(&format!(
        "- 模板 {} 个，主题 {} 个，阅读目标 {} 个（每个目标对应一份笔记）\n",
        counts.templates, counts.topics, counts.goals
    ));
    text.push_str(&format!(
        "- 概念 {} 个，原子 {} 个；完整解释 {} 处，引用 {} 处、同源嵌入 {} 处\n",
        counts.concepts, counts.atoms, counts.explanations, counts.references, counts.embeds
    ));
    text.push_str(&format!(
        "- 章节 {} 个（模板外章节 {} 个、省略章节 {} 个）；偏差 {} 条、缺口 {} 条\n\n",
        counts.sections,
        counts.special_sections,
        counts.omitted_sections,
        counts.deviations,
        counts.findings
    ));
    text.push_str("## 阅读目标与结构\n\n");
    text.push_str("| 阅读目标 | 路径 | 章节 | 原子 | 引用 |\n| --- | --- | --- | --- | --- |\n");
    for goal in &planner.plan_file.goals {
        let sections = goal
            .sections
            .iter()
            .map(|record| {
                if record.special {
                    format!("{}（模板外）", record.section)
                } else {
                    record.section.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" → ");
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            goal.title,
            goal.path,
            sections,
            goal.atoms.len(),
            goal.references.len()
        ));
    }
    text.push('\n');
    text.push_str("## 唯一解释位置\n\n");
    text.push_str("| 概念 | 正名 | 完整解释笔记 | 引用数 |\n| --- | --- | --- | --- |\n");
    for explanation in &planner.plan_file.explanations {
        text.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            explanation.concept_id, explanation.canonical, explanation.note, explanation.references
        ));
    }
    text.push('\n');
    text.push_str("## 偏差与理由\n\n");
    if report.deviations.is_empty() {
        text.push_str("- 没有结构偏差。\n");
    } else {
        for deviation in &report.deviations {
            text.push_str(&format!(
                "- {}（{}）：{}\n",
                deviation.kind, deviation.subject, deviation.detail
            ));
        }
    }
    text.push('\n');
    text.push_str("## 缺口\n\n");
    if report.findings.is_empty() {
        text.push_str("- 缺口 0 条。\n");
    } else {
        for finding in &report.findings {
            text.push_str(&format!(
                "- {}（{}）：{}\n",
                finding.kind, finding.subject, finding.detail
            ));
        }
    }
    text.push('\n');
    text.push_str("## 边界\n\n");
    text.push_str("- 模板是结构规范，不是填空表：不适用的章节应当省略，声明了就必须有原子。\n");
    text.push_str("- 同一类型、同一层级的主题采用同一命名与章节顺序；改动顺序或加入模板外章节必须写明理由。\n");
    text.push_str("- 每个概念在整份规划里恰有一处完整解释，其余笔记只写引用或同源嵌入。\n");
    text.push_str("- 模型调用与网络请求 0 次；不生成正文、不判断内容质量与科学正确性。\n");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> Plan {
        serde_json::from_str(
            r#"{
              "policy": "structure-template-v1",
              "schema_version": 1,
              "producer": "fixture_author",
              "case_id": "tpl-test",
              "description": "单元测试声明。",
              "folder_root": "代谢",
              "templates": [
                {"template_id": "pathway", "title": "途径模板", "sections": ["定位", "反应", "调控"], "navigation_depth": 2}
              ],
              "topics": [
                {"topic_id": "t1", "title": "糖酵解", "folder": "糖酵解", "template_id": "pathway", "level": 1,
                 "goals": [
                   {"goal_id": "g1", "kind": "overview", "title": "糖酵解：总览", "sections": ["定位"], "navigation_depth": 2},
                   {"goal_id": "g2", "kind": "energy", "title": "糖酵解：能量", "sections": ["反应"], "navigation_depth": 2}
                 ]}
              ],
              "concepts": [{"concept_id": "c1", "canonical": "糖酵解"}],
              "atoms": [
                {"atom_id": "a1", "concept_id": "c1", "goal_id": "g1", "section": "定位", "role": "explanation"},
                {"atom_id": "a3", "concept_id": "c1", "goal_id": "g2", "section": "反应", "role": "embed"}
              ],
              "limits": {"max_templates": 2, "max_topics": 2, "max_goals": 4, "max_concepts": 4, "max_atoms": 8}
            }"#,
        )
        .expect("declaration")
    }

    #[test]
    fn omission_is_allowed_and_reported() {
        let plan = declaration();
        let planner = plan_declaration(&plan);
        assert!(
            planner
                .report
                .deviations
                .iter()
                .any(|item| item.kind == DEVIATION_OMITTED)
        );
    }

    #[test]
    fn an_empty_declared_section_is_refused() {
        let mut plan = declaration();
        plan.topics[0].goals[0].sections = vec!["定位".to_owned(), "调控".to_owned()];
        let failure = super::plan(&plan).err().expect("must refuse");
        assert_eq!(failure.code(), "TEMPLATE_EMPTY_SECTION");
    }

    #[test]
    fn a_section_out_of_order_is_refused() {
        let mut plan = declaration();
        plan.topics[0].goals[0].sections = vec!["反应".to_owned(), "定位".to_owned()];
        plan.atoms.push(AtomSpec {
            atom_id: "a4".to_owned(),
            concept_id: "c1".to_owned(),
            goal_id: "g1".to_owned(),
            section: "反应".to_owned(),
            role: "embed".to_owned(),
        });
        let failure = super::plan(&plan).err().expect("must refuse");
        assert_eq!(failure.code(), "TEMPLATE_SECTION_ORDER");
    }

    #[test]
    fn a_special_section_needs_a_reason() {
        let mut plan = declaration();
        plan.topics[0].goals[0].sections.push("补充".to_owned());
        plan.atoms.push(AtomSpec {
            atom_id: "a9".to_owned(),
            concept_id: "c1".to_owned(),
            goal_id: "g1".to_owned(),
            section: "补充".to_owned(),
            role: "reference".to_owned(),
        });
        let failure = super::plan(&plan).err().expect("must refuse");
        assert_eq!(failure.code(), "TEMPLATE_UNREASONED");
    }

    #[test]
    fn two_explanations_are_a_finding_not_a_pass() {
        let mut plan = declaration();
        plan.atoms[1].role = ROLE_EXPLANATION.to_owned();
        let planner = plan_declaration(&plan);
        assert_eq!(planner.report.verdict, "findings");
        assert!(
            planner
                .report
                .findings
                .iter()
                .any(|item| item.kind == FINDING_DUPLICATE)
        );
    }

    #[test]
    fn references_point_at_the_explanation_note() {
        let planner = plan_declaration(&declaration());
        let goal = planner
            .plan_file
            .goals
            .iter()
            .find(|goal| goal.goal_id == "g2")
            .expect("goal");
        assert_eq!(goal.references.len(), 1);
        assert_eq!(goal.references[0].target_note, "代谢/糖酵解/01-g1.md");
    }

    #[test]
    fn a_reference_inside_the_explanation_note_is_refused() {
        let mut plan = declaration();
        plan.topics[0].goals[0].sections = vec!["定位".to_owned(), "反应".to_owned()];
        plan.atoms.push(AtomSpec {
            atom_id: "a5".to_owned(),
            concept_id: "c1".to_owned(),
            goal_id: "g1".to_owned(),
            section: "反应".to_owned(),
            role: ROLE_REFERENCE.to_owned(),
        });
        let failure = super::plan(&plan).err().expect("must refuse");
        assert_eq!(failure.code(), "TEMPLATE_ARGUMENT");
    }

    #[test]
    fn inconsistent_shapes_need_a_reason() {
        let mut plan = declaration();
        plan.topics.push(TopicSpec {
            topic_id: "t2".to_owned(),
            title: "糖异生".to_owned(),
            folder: "糖异生".to_owned(),
            template_id: "pathway".to_owned(),
            level: 1,
            goals: vec![GoalSpec {
                goal_id: "g3".to_owned(),
                kind: "overview".to_owned(),
                title: "糖异生：总览".to_owned(),
                sections: vec!["调控".to_owned()],
                navigation_depth: 2,
                special_reason: None,
                title_reason: None,
                depth_reason: None,
            }],
        });
        plan.atoms.push(AtomSpec {
            atom_id: "a10".to_owned(),
            concept_id: "c1".to_owned(),
            goal_id: "g3".to_owned(),
            section: "调控".to_owned(),
            role: "reference".to_owned(),
        });
        let planner = plan_declaration(&plan);
        assert!(
            planner
                .report
                .findings
                .iter()
                .any(|item| item.kind == FINDING_INCONSISTENT)
        );
    }

    #[test]
    fn a_title_that_does_not_follow_the_pattern_needs_a_reason() {
        let mut plan = declaration();
        plan.topics[0].goals[1].title = "糖酵解：能量账".to_owned();
        let failure = super::plan(&plan).err().expect("must refuse");
        assert_eq!(failure.code(), "TEMPLATE_UNREASONED");

        let mut plan = declaration();
        plan.topics[0].goals[1].title = "糖酵解：能量账".to_owned();
        plan.topics[0].goals[1].title_reason = Some("用户要求保留更自然的口语标题。".to_owned());
        let planner = plan_declaration(&plan);
        assert!(
            planner
                .report
                .deviations
                .iter()
                .any(|item| item.kind == DEVIATION_TITLE)
        );
    }

    #[test]
    fn a_concept_without_an_explanation_is_a_finding() {
        let mut plan = declaration();
        for atom in &mut plan.atoms {
            if atom.role == ROLE_EXPLANATION {
                atom.role = ROLE_REFERENCE.to_owned();
            }
        }
        let planner = plan_declaration(&plan);
        assert!(
            planner
                .report
                .findings
                .iter()
                .any(|item| item.kind == FINDING_UNEXPLAINED)
        );
    }

    fn plan_declaration(plan: &Plan) -> Planner {
        match super::plan(plan) {
            Ok(planner) => planner,
            Err(error) => panic!("unexpected rejection: {}", error.code()),
        }
    }
}
