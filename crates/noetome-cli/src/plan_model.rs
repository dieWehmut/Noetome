//! Deterministic textbook-style planning over already verified knowledge.
//!
//! Planning answers one question: given concepts, their prerequisite relations,
//! the required atoms and a declared set of reading goals, where does every atom
//! live in the file tree?  The knowledge graph allows a concept to be reached
//! from several goals, but the tree picks exactly one primary location; every
//! other goal only links to it.  A continuous reaction chain is never scattered
//! into one-line files without a reason that is itself part of the declaration.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::project::ProjectError;

pub(crate) const POLICY: &str = "adaptive-plan-v1";
pub(crate) const AUTHOR_PRODUCER: &str = "fixture_author";
pub(crate) const PLAN_VERSION: u32 = 1;
pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_TOPICS: usize = 4;
pub(crate) const MAX_GOALS: usize = 8;
pub(crate) const MAX_CONCEPTS: usize = 64;
pub(crate) const MAX_ATOMS: usize = 256;
pub(crate) const MAX_CHAINS: usize = 32;
pub(crate) const MAX_ID: usize = 48;
pub(crate) const MAX_TEXT: usize = 600;
pub(crate) const MAX_TITLE: usize = 200;

pub(crate) const RELATION_PREREQUISITE: &str = "prerequisite";
pub(crate) const RELATION_RELATED: &str = "related";
pub(crate) const RELATIONS: [&str; 2] = [RELATION_PREREQUISITE, RELATION_RELATED];

pub(crate) const GOAL_OVERVIEW: &str = "overview";
pub(crate) const GOAL_REACTION: &str = "reaction";
pub(crate) const GOAL_REGULATION: &str = "regulation";
pub(crate) const GOAL_ENERGY: &str = "energy";
pub(crate) const GOAL_COMPARISON: &str = "comparison";
pub(crate) const GOAL_KINDS: [&str; 5] = [
    GOAL_OVERVIEW,
    GOAL_REACTION,
    GOAL_REGULATION,
    GOAL_ENERGY,
    GOAL_COMPARISON,
];

/// Reasons that make splitting one chain across notes acceptable.  Anything else
/// is refused: "拆开更方便排版" is not a reason a reader can check.
pub(crate) const SPLIT_CHAIN_TOO_LONG: &str = "chain_too_long";
pub(crate) const SPLIT_TOPIC_BOUNDARY: &str = "topic_boundary";
pub(crate) const SPLIT_USER_LOCK: &str = "user_lock";
pub(crate) const SPLIT_REASONS: [&str; 3] =
    [SPLIT_CHAIN_TOO_LONG, SPLIT_TOPIC_BOUNDARY, SPLIT_USER_LOCK];

pub(crate) fn argument(message: impl Into<String>) -> ProjectError {
    ProjectError::new("PLAN_ARGUMENT", 2, message.into())
}

pub(crate) fn integrity(message: impl Into<String>) -> ProjectError {
    ProjectError::new("PLAN_INTEGRITY", 3, message.into())
}

pub(crate) fn unknown_root(message: impl Into<String>) -> ProjectError {
    ProjectError::new("PLAN_UNKNOWN_ROOT", 3, message.into())
}

pub(crate) fn chain_split(message: impl Into<String>) -> ProjectError {
    ProjectError::new("PLAN_CHAIN_SPLIT", 3, message.into())
}

pub(crate) fn cycle(message: impl Into<String>) -> ProjectError {
    ProjectError::new("PLAN_PREREQUISITE_CYCLE", 3, message.into())
}

pub(crate) fn unplaced(message: impl Into<String>) -> ProjectError {
    ProjectError::new("PLAN_UNPLACED", 3, message.into())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConceptSpec {
    pub(crate) concept_id: String,
    pub(crate) canonical: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RelationSpec {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChainSpec {
    pub(crate) chain_id: String,
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) split_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AtomSpec {
    pub(crate) atom_id: String,
    pub(crate) concept_id: String,
    pub(crate) label: String,
    pub(crate) goal_id: String,
    #[serde(default)]
    pub(crate) chain_id: Option<String>,
    #[serde(default)]
    pub(crate) step: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GoalSpec {
    pub(crate) goal_id: String,
    pub(crate) topic_id: String,
    pub(crate) title: String,
    pub(crate) kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TopicSpec {
    pub(crate) topic_id: String,
    pub(crate) title: String,
    pub(crate) folder: String,
    pub(crate) concepts: Vec<String>,
    pub(crate) goals: Vec<String>,
    pub(crate) primary_goal: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Limits {
    pub(crate) max_topics: usize,
    pub(crate) max_goals: usize,
    pub(crate) max_concepts: usize,
    pub(crate) max_atoms: usize,
    pub(crate) max_chains: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Plan {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) folder_root: String,
    pub(crate) topics: Vec<TopicSpec>,
    pub(crate) goals: Vec<GoalSpec>,
    pub(crate) concepts: Vec<ConceptSpec>,
    pub(crate) relations: Vec<RelationSpec>,
    pub(crate) chains: Vec<ChainSpec>,
    pub(crate) atoms: Vec<AtomSpec>,
    pub(crate) limits: Limits,
}

/// One planned note: the file tree picks exactly one goal of one topic per note.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct NoteRecord {
    pub(crate) note_id: String,
    pub(crate) topic_id: String,
    pub(crate) goal_id: String,
    pub(crate) path: String,
    pub(crate) title: String,
    pub(crate) primary: bool,
    pub(crate) atom_ids: Vec<String>,
    pub(crate) links: Vec<LinkRecord>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct LinkRecord {
    pub(crate) concept_id: String,
    pub(crate) target_note: String,
    pub(crate) display_text: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct FolderRecord {
    pub(crate) topic_id: String,
    pub(crate) path: String,
    pub(crate) title: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AtomRecord {
    pub(crate) atom_id: String,
    pub(crate) concept_id: String,
    pub(crate) chain_id: Option<String>,
    pub(crate) note_id: String,
    pub(crate) position: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct OrderRecord {
    pub(crate) concept_id: String,
    pub(crate) canonical: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct PlanFile {
    pub(crate) policy: String,
    pub(crate) plan_version: u32,
    pub(crate) case_id: String,
    pub(crate) folder_root: String,
    pub(crate) teaching_order: Vec<OrderRecord>,
    pub(crate) folders: Vec<FolderRecord>,
    pub(crate) notes: Vec<NoteRecord>,
    pub(crate) atoms: Vec<AtomRecord>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ChainRecord {
    pub(crate) chain_id: String,
    pub(crate) label: String,
    pub(crate) notes: Vec<String>,
    pub(crate) split_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Counts {
    pub(crate) topics: usize,
    pub(crate) goals: usize,
    pub(crate) concepts: usize,
    pub(crate) atoms: usize,
    pub(crate) atoms_planned: usize,
    pub(crate) notes: usize,
    pub(crate) folders: usize,
    pub(crate) chains: usize,
    pub(crate) chains_split: usize,
    pub(crate) cross_refs: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Warning {
    pub(crate) kind: String,
    pub(crate) subject: String,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Report {
    pub(crate) policy: String,
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) description: String,
    pub(crate) counts: Counts,
    pub(crate) chains: Vec<ChainRecord>,
    pub(crate) warnings: Vec<Warning>,
    pub(crate) model_calls: u32,
    pub(crate) network_requests: u32,
}

pub(crate) struct Planner {
    pub(crate) plan_file: PlanFile,
    pub(crate) report: Report,
}

pub(crate) fn single(text: &str, label: &str, limit: usize) -> Result<(), ProjectError> {
    if text.is_empty() {
        return Err(argument(format!("{label}不能为空。")));
    }
    if text != text.trim() {
        return Err(argument(format!("{label}不能有首尾空白：{text}")));
    }
    if text.chars().count() > limit {
        return Err(argument(format!(
            "{label}最多 {limit} 个字符，实际 {} 个。",
            text.chars().count()
        )));
    }
    if text.chars().any(char::is_control) {
        return Err(argument(format!("{label}不能包含控制字符：{text}")));
    }
    Ok(())
}

pub(crate) fn identifier(text: &str, label: &str) -> Result<(), ProjectError> {
    single(text, label, MAX_ID)?;
    if !text
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
    {
        return Err(argument(format!(
            "{label}只能包含ASCII字母、数字、连字符、下划线和点：{text}"
        )));
    }
    Ok(())
}

pub(crate) fn relative(text: &str, label: &str) -> Result<(), ProjectError> {
    single(text, label, MAX_TEXT)?;
    if text.starts_with('/') || text.contains('\\') {
        return Err(argument(format!(
            "{label}必须是相对路径，并以 / 分隔：{text}"
        )));
    }
    if text
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(argument(format!("{label}不能包含空段、. 或 ..：{text}")));
    }
    Ok(())
}

impl Limits {
    fn validate(&self) -> Result<(), ProjectError> {
        for (value, minimum, maximum, label) in [
            (self.max_topics, 1, MAX_TOPICS, "max_topics"),
            (self.max_goals, 1, MAX_GOALS, "max_goals"),
            (self.max_concepts, 1, MAX_CONCEPTS, "max_concepts"),
            (self.max_atoms, 1, MAX_ATOMS, "max_atoms"),
            (self.max_chains, 0, MAX_CHAINS, "max_chains"),
        ] {
            if value < minimum || value > maximum {
                return Err(argument(format!(
                    "{label} 必须在 {minimum} 到 {maximum} 之间，实际 {value}。"
                )));
            }
        }
        Ok(())
    }
}

impl Plan {
    pub(crate) fn parse(text: &str) -> Result<Self, ProjectError> {
        let plan: Self = serde_json::from_str(text)
            .map_err(|error| argument(format!("规划声明不是合法 JSON：{error}")))?;
        plan.validate()?;
        Ok(plan)
    }

    fn validate(&self) -> Result<(), ProjectError> {
        if self.policy != POLICY {
            return Err(argument(format!(
                "规划声明 policy 必须是 {POLICY}，实际 {}。",
                self.policy
            )));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(argument(format!(
                "规划声明结构版本必须是 {SCHEMA_VERSION}，实际 {}。",
                self.schema_version
            )));
        }
        if self.producer != AUTHOR_PRODUCER {
            return Err(argument(format!(
                "规划声明 producer 必须是 {AUTHOR_PRODUCER}，实际 {}。",
                self.producer
            )));
        }
        identifier(&self.case_id, "case_id")?;
        single(&self.description, "description", MAX_TEXT)?;
        relative(&self.folder_root, "folder_root")?;
        self.limits.validate()?;

        if self.topics.is_empty() || self.topics.len() > self.limits.max_topics {
            return Err(argument(format!(
                "主题数量必须在 1 到 {} 之间，实际 {}。",
                self.limits.max_topics,
                self.topics.len()
            )));
        }
        if self.concepts.is_empty() || self.concepts.len() > self.limits.max_concepts {
            return Err(argument(format!(
                "概念数量必须在 1 到 {} 之间，实际 {}。",
                self.limits.max_concepts,
                self.concepts.len()
            )));
        }
        if self.atoms.is_empty() || self.atoms.len() > self.limits.max_atoms {
            return Err(argument(format!(
                "必要原子数量必须在 1 到 {} 之间，实际 {}。",
                self.limits.max_atoms,
                self.atoms.len()
            )));
        }
        if self.chains.len() > self.limits.max_chains {
            return Err(argument(format!(
                "反应链数量不能超过 {}，实际 {}。",
                self.limits.max_chains,
                self.chains.len()
            )));
        }
        if self.goals.len() > self.limits.max_goals {
            return Err(argument(format!(
                "阅读目标数量不能超过 {}，实际 {}。",
                self.limits.max_goals,
                self.goals.len()
            )));
        }

        let mut concept_ids = BTreeSet::new();
        for concept in &self.concepts {
            identifier(&concept.concept_id, "concept_id")?;
            single(&concept.canonical, "概念正名", MAX_TITLE)?;
            if !concept_ids.insert(concept.concept_id.as_str()) {
                return Err(argument(format!("概念编号重复：{}。", concept.concept_id)));
            }
        }

        let mut goal_ids = BTreeSet::new();
        for goal in &self.goals {
            identifier(&goal.goal_id, "goal_id")?;
            identifier(&goal.topic_id, "goal.topic_id")?;
            single(&goal.title, "阅读目标标题", MAX_TITLE)?;
            if !GOAL_KINDS.contains(&goal.kind.as_str()) {
                return Err(argument(format!(
                    "阅读目标 {} 的类型必须是 {} 之一，实际 {}。",
                    goal.goal_id,
                    GOAL_KINDS.join("、"),
                    goal.kind
                )));
            }
            if !goal_ids.insert(goal.goal_id.as_str()) {
                return Err(argument(format!("阅读目标编号重复：{}。", goal.goal_id)));
            }
        }

        let mut topic_ids = BTreeSet::new();
        let mut folders = BTreeSet::new();
        let mut assigned_concepts: BTreeSet<&str> = BTreeSet::new();
        let mut topic_of_goal: BTreeMap<&str, &str> = BTreeMap::new();
        let mut primary_of_topic: BTreeMap<&str, &str> = BTreeMap::new();
        for topic in &self.topics {
            identifier(&topic.topic_id, "topic_id")?;
            single(&topic.title, "主题标题", MAX_TITLE)?;
            relative(&topic.folder, "topic.folder")?;
            if !topic_ids.insert(topic.topic_id.as_str()) {
                return Err(argument(format!("主题编号重复：{}。", topic.topic_id)));
            }
            if !folders.insert(topic.folder.as_str()) {
                return Err(argument(format!(
                    "两个主题不能落在同一个文件夹：{}。",
                    topic.folder
                )));
            }
            if topic.concepts.is_empty() {
                return Err(argument(format!(
                    "主题 {} 没有声明任何概念，文件树里会出现空文件夹。",
                    topic.topic_id
                )));
            }
            for concept_id in &topic.concepts {
                if !concept_ids.contains(concept_id.as_str()) {
                    return Err(argument(format!(
                        "主题 {} 声明了未登记的概念 {concept_id}。",
                        topic.topic_id
                    )));
                }
                if !assigned_concepts.insert(concept_id.as_str()) {
                    return Err(argument(format!(
                        "概念 {concept_id} 不能同时作为两个主题的主要位置：知识图谱可以多重归属，但文件树只能有一个主要位置，其余联系用链接表达。"
                    )));
                }
            }
            if topic.goals.is_empty() {
                return Err(argument(format!(
                    "主题 {} 没有声明任何阅读目标。",
                    topic.topic_id
                )));
            }
            for goal_id in &topic.goals {
                let Some(goal) = self.goals.iter().find(|goal| &goal.goal_id == goal_id) else {
                    return Err(argument(format!(
                        "主题 {} 声明了未登记的阅读目标 {goal_id}。",
                        topic.topic_id
                    )));
                };
                if goal.topic_id != topic.topic_id {
                    return Err(argument(format!(
                        "阅读目标 {goal_id} 登记的主题是 {}，但被主题 {} 引用。",
                        goal.topic_id, topic.topic_id
                    )));
                }
                if topic_of_goal
                    .insert(&goal.goal_id, &topic.topic_id)
                    .is_some()
                {
                    return Err(argument(format!("阅读目标 {goal_id} 被重复引用。")));
                }
            }
            if !topic.goals.iter().any(|goal| goal == &topic.primary_goal) {
                return Err(argument(format!(
                    "主题 {} 的主阅读目标 {} 不在它声明的目标列表里。",
                    topic.topic_id, topic.primary_goal
                )));
            }
            primary_of_topic.insert(topic.topic_id.as_str(), topic.primary_goal.as_str());
        }
        for goal in &self.goals {
            if !topic_of_goal.contains_key(goal.goal_id.as_str()) {
                return Err(argument(format!(
                    "阅读目标 {} 没有被任何主题引用，会变成无人引用的笔记。",
                    goal.goal_id
                )));
            }
        }

        let mut chain_ids = BTreeSet::new();
        for chain in &self.chains {
            identifier(&chain.chain_id, "chain_id")?;
            single(&chain.label, "反应链标题", MAX_TITLE)?;
            if !chain_ids.insert(chain.chain_id.as_str()) {
                return Err(argument(format!("反应链编号重复：{}。", chain.chain_id)));
            }
            if let Some(reason) = &chain.split_reason {
                if !SPLIT_REASONS.contains(&reason.as_str()) {
                    return Err(argument(format!(
                        "反应链 {} 的拆分理由必须是 {} 之一，实际 {reason}。",
                        chain.chain_id,
                        SPLIT_REASONS.join("、")
                    )));
                }
            }
        }

        let mut atom_ids = BTreeSet::new();
        let mut steps: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
        for atom in &self.atoms {
            identifier(&atom.atom_id, "atom_id")?;
            single(&atom.label, "原子标题", MAX_TITLE)?;
            if !concept_ids.contains(atom.concept_id.as_str()) {
                return Err(argument(format!(
                    "原子 {} 声明了未登记的概念 {}。",
                    atom.atom_id, atom.concept_id
                )));
            }
            if !atom_ids.insert(atom.atom_id.as_str()) {
                return Err(argument(format!("原子编号重复：{}。", atom.atom_id)));
            }
            let Some(topic) = self
                .topics
                .iter()
                .find(|topic| topic.concepts.iter().any(|id| id == &atom.concept_id))
            else {
                return Err(unplaced(format!(
                    "原子 {} 的概念 {} 没有主要位置，规划里无处安放。",
                    atom.atom_id, atom.concept_id
                )));
            };
            let Some(goal_topic) = topic_of_goal.get(atom.goal_id.as_str()) else {
                return Err(argument(format!(
                    "原子 {} 声明了未登记的阅读目标 {}。",
                    atom.atom_id, atom.goal_id
                )));
            };
            if *goal_topic != topic.topic_id {
                return Err(argument(format!(
                    "原子 {} 的概念 {} 属于主题 {}，却要放进主题 {} 的阅读目标 {}：跨主题只能建立链接，不能搬走原子。",
                    atom.atom_id, atom.concept_id, topic.topic_id, goal_topic, atom.goal_id
                )));
            }
            match (&atom.chain_id, atom.step) {
                (Some(chain_id), Some(step)) => {
                    if !chain_ids.contains(chain_id.as_str()) {
                        return Err(argument(format!(
                            "原子 {} 声明了未登记的反应链 {chain_id}。",
                            atom.atom_id
                        )));
                    }
                    if step == 0 {
                        return Err(argument(format!(
                            "原子 {} 的链内步骤必须从 1 开始。",
                            atom.atom_id
                        )));
                    }
                    steps.entry(chain_id.as_str()).or_default().push(step);
                }
                (None, None) => {}
                _ => {
                    return Err(argument(format!(
                        "原子 {} 的 chain_id 与 step 必须同时给出或同时省略。",
                        atom.atom_id
                    )));
                }
            }
        }
        for (chain_id, mut values) in steps {
            values.sort_unstable();
            let expected: Vec<u32> = (1..=values.len() as u32).collect();
            if values != expected {
                return Err(argument(format!(
                    "反应链 {chain_id} 的步骤必须从 1 连续编号且不重复，实际 {:?}。",
                    values
                )));
            }
        }

        for relation in &self.relations {
            if !concept_ids.contains(relation.from.as_str())
                || !concept_ids.contains(relation.to.as_str())
            {
                return Err(argument(format!(
                    "概念关系 {0} → {1} 引用了未登记的概念。",
                    relation.from, relation.to
                )));
            }
            if !RELATIONS.contains(&relation.kind.as_str()) {
                return Err(argument(format!(
                    "概念关系类型必须是 {} 之一，实际 {}。",
                    RELATIONS.join("、"),
                    relation.kind
                )));
            }
            if relation.from == relation.to {
                return Err(argument(format!(
                    "概念 {} 不能与自己建立关系。",
                    relation.from
                )));
            }
        }

        for concept in &self.concepts {
            let Some(topic) = self
                .topics
                .iter()
                .find(|topic| topic.concepts.iter().any(|id| id == &concept.concept_id))
            else {
                return Err(unplaced(format!(
                    "概念 {} 没有主要位置，规划里无处安放。",
                    concept.concept_id
                )));
            };
            let has_explanation = self.atoms.iter().any(|atom| {
                atom.concept_id == concept.concept_id && atom.goal_id == topic.primary_goal
            });
            if !has_explanation {
                return Err(unplaced(format!(
                    "概念 {} 的完整解释不在主题 {} 的主阅读目标 {} 里：每个概念必须恰有一个完整解释位置。",
                    concept.concept_id, topic.topic_id, topic.primary_goal
                )));
            }
        }

        Ok(())
    }

    /// Teaching order over concepts.  Prerequisite edges point from the concept
    /// that must be taught first; ties fall back to declaration order so the
    /// order never depends on hash iteration.
    pub(crate) fn teaching_order(&self) -> Result<Vec<String>, ProjectError> {
        let mut indegree: BTreeMap<&str, usize> = self
            .concepts
            .iter()
            .map(|concept| (concept.concept_id.as_str(), 0))
            .collect();
        let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for relation in &self.relations {
            if relation.kind != RELATION_PREREQUISITE {
                continue;
            }
            edges
                .entry(relation.from.as_str())
                .or_default()
                .push(relation.to.as_str());
            if let Some(value) = indegree.get_mut(relation.to.as_str()) {
                *value += 1;
            }
        }
        let mut ready: Vec<&str> = self
            .concepts
            .iter()
            .map(|concept| concept.concept_id.as_str())
            .filter(|concept| indegree.get(concept) == Some(&0))
            .collect();
        let mut order = Vec::new();
        while !ready.is_empty() {
            ready.sort_by_key(|concept| {
                self.concepts
                    .iter()
                    .position(|entry| entry.concept_id == *concept)
                    .unwrap_or(usize::MAX)
            });
            let next = ready.remove(0);
            order.push(next.to_string());
            for target in edges.get(next).cloned().unwrap_or_default() {
                if let Some(value) = indegree.get_mut(target) {
                    *value -= 1;
                    if *value == 0 {
                        ready.push(target);
                    }
                }
            }
        }
        if order.len() != self.concepts.len() {
            let mut left: Vec<&str> = self
                .concepts
                .iter()
                .map(|concept| concept.concept_id.as_str())
                .filter(|concept| !order.iter().any(|entry| entry == concept))
                .collect();
            left.sort_unstable();
            return Err(cycle(format!(
                "概念的前置关系成环，无法排出教学顺序：{}。",
                left.join("、")
            )));
        }
        Ok(order)
    }
}

pub(crate) fn plan(declaration: &Plan) -> Result<Planner, ProjectError> {
    let order = declaration.teaching_order()?;
    let rank: BTreeMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(index, concept)| (concept.as_str(), index))
        .collect();

    let mut folders = Vec::new();
    let mut notes = Vec::new();
    let mut atoms = Vec::new();
    let mut links = 0usize;

    for topic in &declaration.topics {
        let mut note_order: Vec<(&GoalSpec, usize, usize)> = Vec::new();
        for (goal_index, goal_id) in topic.goals.iter().enumerate() {
            let goal = declaration
                .goals
                .iter()
                .find(|goal| &goal.goal_id == goal_id)
                .expect("validated goal");
            let best = declaration
                .atoms
                .iter()
                .filter(|atom| atom.goal_id == *goal_id)
                .filter_map(|atom| rank.get(atom.concept_id.as_str()).copied())
                .min()
                .unwrap_or(usize::MAX);
            note_order.push((goal, best, goal_index));
        }
        note_order.sort_by_key(|(_, best, goal_index)| (*best, *goal_index));

        folders.push(FolderRecord {
            topic_id: topic.topic_id.clone(),
            path: format!("{}/{}", declaration.folder_root, topic.folder),
            title: topic.title.clone(),
        });

        let mut note_paths: BTreeMap<&str, String> = BTreeMap::new();
        for (position, (goal, _, _)) in note_order.iter().enumerate() {
            let path = format!(
                "{}/{}/{:02}-{}.md",
                declaration.folder_root,
                topic.folder,
                position + 1,
                goal.goal_id
            );
            note_paths.insert(goal.goal_id.as_str(), path.clone());
            notes.push(NoteRecord {
                note_id: format!("{}-{}", topic.topic_id, goal.goal_id),
                topic_id: topic.topic_id.clone(),
                goal_id: goal.goal_id.clone(),
                path,
                title: goal.title.clone(),
                primary: goal.goal_id == topic.primary_goal,
                atom_ids: Vec::new(),
                links: Vec::new(),
            });
        }

        // Atoms always live in the topic that owns their concept; the primary
        // goal holds the explanation, other goals hold their own atoms plus a
        // link back to the explanation.
        let mut buckets: BTreeMap<&str, Vec<&AtomSpec>> = BTreeMap::new();
        for atom in &declaration.atoms {
            if !topic.concepts.iter().any(|id| id == &atom.concept_id) {
                continue;
            }
            buckets.entry(atom.goal_id.as_str()).or_default().push(atom);
        }
        for (goal_id, mut bucket) in buckets {
            bucket.sort_by_key(|atom| {
                (
                    atom.chain_id.as_ref().map(|_| 0usize).unwrap_or(1),
                    atom.step.unwrap_or(0),
                    rank.get(atom.concept_id.as_str())
                        .copied()
                        .unwrap_or(usize::MAX),
                    declaration
                        .atoms
                        .iter()
                        .position(|entry| entry.atom_id == atom.atom_id)
                        .unwrap_or(usize::MAX),
                )
            });
            let note_index = notes
                .iter()
                .position(|note| note.topic_id == topic.topic_id && note.goal_id == goal_id)
                .expect("planned note");
            let note_id = notes[note_index].note_id.clone();
            for (position, atom) in bucket.iter().enumerate() {
                notes[note_index].atom_ids.push(atom.atom_id.clone());
                atoms.push(AtomRecord {
                    atom_id: atom.atom_id.clone(),
                    concept_id: atom.concept_id.clone(),
                    chain_id: atom.chain_id.clone(),
                    note_id: note_id.clone(),
                    position: position + 1,
                });
            }
        }

        // Cross-note links: every goal that carries atoms of a concept but is not
        // the explanation location links to the primary note instead of copying it.
        let primary_path = note_paths
            .get(topic.primary_goal.as_str())
            .cloned()
            .expect("primary note");
        let mut seen: BTreeSet<(usize, String)> = BTreeSet::new();
        for atom in &declaration.atoms {
            if !topic.concepts.iter().any(|id| id == &atom.concept_id) {
                continue;
            }
            if atom.goal_id == topic.primary_goal {
                continue;
            }
            let Some(note_index) = notes
                .iter()
                .position(|note| note.topic_id == topic.topic_id && note.goal_id == atom.goal_id)
            else {
                continue;
            };
            if !seen.insert((note_index, atom.concept_id.clone())) {
                continue;
            }
            let canonical = declaration
                .concepts
                .iter()
                .find(|concept| concept.concept_id == atom.concept_id)
                .map(|concept| concept.canonical.clone())
                .unwrap_or_else(|| atom.concept_id.clone());
            notes[note_index].links.push(LinkRecord {
                concept_id: atom.concept_id.clone(),
                target_note: primary_path.clone(),
                display_text: canonical,
            });
            links += 1;
        }
    }

    // Atoms are reported in reading order (note order, then position inside the
    // note) rather than in declaration order, so plan.json reads like the plan.
    atoms.sort_by_key(|atom| {
        let note_index = notes
            .iter()
            .position(|note| note.note_id == atom.note_id)
            .unwrap_or(usize::MAX);
        (note_index, atom.position)
    });

    // Chain cohesion: a chain that ends up in more than one note needs a reason
    // that the declaration itself states; otherwise the whole package is refused.
    let mut chains = Vec::new();
    let mut split_count = 0usize;
    for chain in &declaration.chains {
        let members: Vec<&AtomRecord> = atoms
            .iter()
            .filter(|atom| atom.chain_id.as_deref() == Some(chain.chain_id.as_str()))
            .collect();
        let mut note_ids: Vec<String> = members.iter().map(|atom| atom.note_id.clone()).collect();
        note_ids.sort();
        note_ids.dedup();
        if note_ids.len() > 1 {
            match &chain.split_reason {
                Some(_) => split_count += 1,
                None => {
                    let mut paths: Vec<String> = note_ids
                        .iter()
                        .filter_map(|id| {
                            notes
                                .iter()
                                .find(|note| &note.note_id == id)
                                .map(|note| note.path.clone())
                        })
                        .collect();
                    paths.sort();
                    return Err(chain_split(format!(
                        "反应链 {}（{}）被拆到 {} 份笔记里，却没有给出可检查的拆分理由：{}。请合并为一份笔记，或在链上声明拆分理由（{}）。",
                        chain.chain_id,
                        chain.label,
                        paths.len(),
                        paths.join("、"),
                        SPLIT_REASONS.join("、")
                    )));
                }
            }
        }
        chains.push(ChainRecord {
            chain_id: chain.chain_id.clone(),
            label: chain.label.clone(),
            notes: note_ids,
            split_reason: chain.split_reason.clone(),
        });
    }

    let mut warnings = Vec::new();
    for note in &notes {
        if note.atom_ids.is_empty() {
            warnings.push(Warning {
                kind: "goal_without_atoms".to_string(),
                subject: note.note_id.clone(),
                detail: format!(
                    "阅读目标 {} 没有自己的原子，只保留指向完整解释位置的链接：{}。",
                    note.goal_id, note.path
                ),
            });
        }
    }

    let counts = Counts {
        topics: declaration.topics.len(),
        goals: declaration.goals.len(),
        concepts: declaration.concepts.len(),
        atoms: declaration.atoms.len(),
        atoms_planned: atoms.len(),
        notes: notes.len(),
        folders: folders.len(),
        chains: declaration.chains.len(),
        chains_split: split_count,
        cross_refs: links,
    };
    if counts.atoms_planned != counts.atoms {
        return Err(unplaced(format!(
            "规划只安置了 {} 个必要原子，声明里有 {} 个：每个必要原子都必须有规划位置。",
            counts.atoms_planned, counts.atoms
        )));
    }

    Ok(Planner {
        plan_file: PlanFile {
            policy: POLICY.to_string(),
            plan_version: PLAN_VERSION,
            case_id: declaration.case_id.clone(),
            folder_root: declaration.folder_root.clone(),
            teaching_order: order
                .iter()
                .map(|concept_id| OrderRecord {
                    concept_id: concept_id.clone(),
                    canonical: declaration
                        .concepts
                        .iter()
                        .find(|concept| &concept.concept_id == concept_id)
                        .map(|concept| concept.canonical.clone())
                        .unwrap_or_else(|| concept_id.clone()),
                })
                .collect(),
            folders,
            notes,
            atoms,
        },
        report: Report {
            policy: POLICY.to_string(),
            schema_version: SCHEMA_VERSION,
            case_id: declaration.case_id.clone(),
            description: declaration.description.clone(),
            counts,
            chains,
            warnings,
            model_calls: 0,
            network_requests: 0,
        },
    })
}

pub(crate) fn render_markdown(planner: &Planner) -> String {
    let counts = &planner.report.counts;
    let mut text = String::new();
    text.push_str(&format!(
        "# 教材式自适应规划（{}）\n\n",
        planner.report.case_id
    ));
    text.push_str(&format!("{}\n\n", planner.report.description));
    text.push_str("## 规划规模\n\n");
    text.push_str(&format!(
        "- 主题 {} 个、阅读目标 {} 个、概念 {} 个、必要原子 {} 个。\n",
        counts.topics, counts.goals, counts.concepts, counts.atoms
    ));
    text.push_str(&format!(
        "- 笔记 {} 份、文件夹 {} 个；每个必要原子都有唯一规划位置（{} / {}）。\n",
        counts.notes, counts.folders, counts.atoms_planned, counts.atoms
    ));
    text.push_str(&format!(
        "- 反应链 {} 条，其中跨笔记拆分 {} 条；跨笔记引用 {} 处。\n\n",
        counts.chains, counts.chains_split, counts.cross_refs
    ));
    text.push_str("## 教学顺序\n\n");
    for (index, entry) in planner.plan_file.teaching_order.iter().enumerate() {
        text.push_str(&format!("{}. {}\n", index + 1, entry.canonical));
    }
    text.push('\n');

    text.push_str("## 阅读目标与位置\n\n");
    text.push_str("| 笔记 | 路径 | 主位置 | 原子 | 链接 |\n");
    text.push_str("| --- | --- | --- | --- | --- |\n");
    for note in &planner.plan_file.notes {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            note.title,
            note.path,
            if note.primary { "是" } else { "否" },
            note.atom_ids.len(),
            note.links.len()
        ));
    }
    text.push_str("\n## 反应链\n\n");
    if planner.report.chains.is_empty() {
        text.push_str("- 声明里没有反应链。\n");
    } else {
        text.push_str("| 反应链 | 笔记 | 拆分理由 |\n| --- | --- | --- |\n");
        for chain in &planner.report.chains {
            text.push_str(&format!(
                "| {} | {} | {} |\n",
                chain.label,
                chain.notes.join("、"),
                chain
                    .split_reason
                    .clone()
                    .unwrap_or_else(|| "未拆分".to_string())
            ));
        }
    }
    text.push_str("\n## 提醒\n\n");
    if planner.report.warnings.is_empty() {
        text.push_str("- 没有提醒。\n");
    } else {
        for warning in &planner.report.warnings {
            text.push_str(&format!(
                "- {}（{}）：{}\n",
                warning.kind, warning.subject, warning.detail
            ));
        }
    }
    text.push_str("\n## 边界\n\n");
    text.push_str("- 规划只读已核对的概念关系与必要原子，不生成知识、不写正文、不判断内容质量。\n");
    text.push_str("- 知识图谱允许多重归属，但文件树只选一个主要位置，其余联系用链接表达。\n");
    text.push_str("- 模型调用与网络请求 0 次；本阶段只做结构规划，科学正确性未验收。\n");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> Plan {
        Plan::parse(
            r#"{
              "policy": "adaptive-plan-v1",
              "schema_version": 1,
              "producer": "fixture_author",
              "case_id": "plan-demo-v1",
              "description": "演示用规划声明。",
              "folder_root": "代谢",
              "topics": [
                {
                  "topic_id": "topic-glycolysis",
                  "title": "糖酵解",
                  "folder": "糖酵解",
                  "concepts": ["concept-glycolysis", "concept-pfk"],
                  "goals": ["goal-glycolysis-main", "goal-glycolysis-energy"],
                  "primary_goal": "goal-glycolysis-main"
                }
              ],
              "goals": [
                {
                  "goal_id": "goal-glycolysis-main",
                  "topic_id": "topic-glycolysis",
                  "title": "糖酵解：反应与调控",
                  "kind": "reaction"
                },
                {
                  "goal_id": "goal-glycolysis-energy",
                  "topic_id": "topic-glycolysis",
                  "title": "糖酵解的能量账",
                  "kind": "energy"
                }
              ],
              "concepts": [
                {"concept_id": "concept-glycolysis", "canonical": "糖酵解"},
                {"concept_id": "concept-pfk", "canonical": "磷酸果糖激酶"}
              ],
              "relations": [
                {"from": "concept-glycolysis", "to": "concept-pfk", "kind": "prerequisite"}
              ],
              "chains": [
                {"chain_id": "chain-glycolysis", "label": "糖酵解十步反应"}
              ],
              "atoms": [
                {"atom_id": "atom-glycolysis-definition", "concept_id": "concept-glycolysis", "label": "糖酵解的定义", "goal_id": "goal-glycolysis-main", "chain_id": "chain-glycolysis", "step": 1},
                {"atom_id": "atom-glycolysis-steps", "concept_id": "concept-glycolysis", "label": "十步反应", "goal_id": "goal-glycolysis-main", "chain_id": "chain-glycolysis", "step": 2},
                {"atom_id": "atom-pfk-regulation", "concept_id": "concept-pfk", "label": "别构调节", "goal_id": "goal-glycolysis-main"},
                {"atom_id": "atom-glycolysis-atp", "concept_id": "concept-glycolysis", "label": "净能量收支", "goal_id": "goal-glycolysis-energy"}
              ],
              "limits": {"max_topics": 4, "max_goals": 8, "max_concepts": 64, "max_atoms": 256, "max_chains": 32}
            }"#,
        )
        .expect("valid declaration")
    }

    #[test]
    fn every_atom_gets_one_position_and_the_primary_goal_keeps_the_chain() {
        let planner = plan(&declaration()).expect("plans");
        assert_eq!(planner.report.counts.atoms, 4);
        assert_eq!(planner.report.counts.atoms_planned, 4);
        assert_eq!(planner.report.counts.notes, 2);
        assert_eq!(planner.report.counts.folders, 1);
        assert_eq!(planner.report.counts.chains_split, 0);
        let mut positions: Vec<&str> = planner
            .plan_file
            .atoms
            .iter()
            .map(|atom| atom.atom_id.as_str())
            .collect();
        positions.sort_unstable();
        assert_eq!(positions.len(), 4);
        let chain_note = planner
            .plan_file
            .atoms
            .iter()
            .filter(|atom| atom.chain_id.is_some())
            .map(|atom| atom.note_id.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(chain_note.len(), 1);
    }

    #[test]
    fn the_secondary_goal_links_to_the_primary_note_instead_of_copying_it() {
        let planner = plan(&declaration()).expect("plans");
        let secondary = planner
            .plan_file
            .notes
            .iter()
            .find(|note| !note.primary)
            .expect("secondary note");
        assert_eq!(secondary.links.len(), 1);
        assert_eq!(secondary.links[0].concept_id, "concept-glycolysis");
        assert_eq!(planner.report.counts.cross_refs, 1);
    }

    #[test]
    fn splitting_a_chain_without_a_reason_is_refused() {
        let mut plan = declaration();
        plan.atoms[3].chain_id = Some("chain-glycolysis".to_string());
        plan.atoms[3].step = Some(3);
        let error = report_error(plan);
        assert_eq!(error.code(), "PLAN_CHAIN_SPLIT");
    }

    #[test]
    fn a_declared_reason_allows_the_split_and_is_recorded() {
        let mut plan = declaration();
        plan.atoms[3].chain_id = Some("chain-glycolysis".to_string());
        plan.atoms[3].step = Some(3);
        plan.chains[0].split_reason = Some(SPLIT_CHAIN_TOO_LONG.to_string());
        let planner = super::plan(&plan).expect("plans");
        assert_eq!(planner.report.counts.chains_split, 1);
        assert_eq!(planner.report.chains[0].notes.len(), 2);
    }

    #[test]
    fn a_concept_without_an_atom_in_the_primary_goal_is_refused() {
        let mut plan = declaration();
        // Move every atom of one concept to the secondary goal: the concept then
        // has no complete explanation in the topic's primary goal.
        for atom in &mut plan.atoms {
            if atom.concept_id == "concept-glycolysis" {
                atom.goal_id = "goal-glycolysis-energy".to_string();
            }
            // Drop the chain entirely: a chain split across notes would be
            // refused first and would hide the rule this test is about.
            atom.chain_id = None;
            atom.step = None;
        }
        let error = report_error(plan);
        assert_eq!(error.code(), "PLAN_UNPLACED");
    }

    #[test]
    fn a_prerequisite_cycle_is_refused() {
        let mut plan = declaration();
        plan.relations.push(RelationSpec {
            from: "concept-pfk".to_string(),
            to: "concept-glycolysis".to_string(),
            kind: RELATION_PREREQUISITE.to_string(),
        });
        let error = report_error(plan);
        assert_eq!(error.code(), "PLAN_PREREQUISITE_CYCLE");
    }

    #[test]
    fn teaching_order_puts_prerequisites_first() {
        let plan = declaration();
        let order = plan.teaching_order().expect("order");
        let glycolysis = order
            .iter()
            .position(|concept| concept == "concept-glycolysis")
            .expect("concept");
        let pfk = order
            .iter()
            .position(|concept| concept == "concept-pfk")
            .expect("concept");
        assert!(glycolysis < pfk);
    }

    #[test]
    fn everything_in_one_goal_needs_no_cross_reference() {
        let mut plan = declaration();
        plan.topics[0].goals = vec!["goal-glycolysis-main".to_string()];
        plan.goals
            .retain(|goal| goal.goal_id == "goal-glycolysis-main");
        plan.atoms[3].goal_id = "goal-glycolysis-main".to_string();
        plan.atoms[3].chain_id = Some("chain-glycolysis".to_string());
        plan.atoms[3].step = Some(3);
        let planner = super::plan(&plan).expect("plans");
        assert_eq!(planner.report.counts.notes, 1);
        assert_eq!(planner.report.counts.cross_refs, 0);
    }

    #[test]
    fn a_chain_with_non_contiguous_steps_is_refused() {
        let mut plan = declaration();
        plan.atoms[1].step = Some(5);
        let error = report_error(plan);
        assert_eq!(error.code(), "PLAN_ARGUMENT");
    }

    #[test]
    fn unknown_goal_and_duplicate_concept_are_refused() {
        let mut plan = declaration();
        plan.atoms[0].goal_id = "goal-missing".to_string();
        let error = report_error(plan);
        assert_eq!(error.code(), "PLAN_ARGUMENT");

        let mut plan = declaration();
        plan.topics[0]
            .concepts
            .push("concept-glycolysis".to_string());
        let error = report_error(plan);
        assert_eq!(error.code(), "PLAN_ARGUMENT");
    }

    fn report_error(plan: Plan) -> ProjectError {
        // Mirrors `Plan::parse`: validation runs before planning, so a malformed
        // declaration never reaches the planner and never masks a rule violation.
        if let Err(error) = plan.validate() {
            return error;
        }
        match super::plan(&plan) {
            Ok(_) => panic!("expected the declaration to be refused"),
            Err(error) => error,
        }
    }
}
