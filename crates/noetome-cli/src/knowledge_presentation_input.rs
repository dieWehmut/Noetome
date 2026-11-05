//! Rebuild the actual source chain before consuming a saved coordination result.
use crate::{
    knowledge_coordinate_conflicts, knowledge_coordinate_facts,
    knowledge_coordinate_input::{
        CaseInputs, CaseVersion, FILES, Held, VerifiedReview, check, encoded, fingerprint, parse,
    },
    knowledge_coordinate_variants,
    knowledge_presentation_model::{PresentationFact, Provenance},
    project::ProjectError,
    sidecar_model::Fingerprint,
    sidecar_process::Control,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

pub(crate) fn error(code: &'static str, exit: u8, msg: impl ToString) -> ProjectError {
    ProjectError::new(code, exit, msg.to_string())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    schema_version: u32,
    producer: String,
    purpose: String,
    request_id: String,
}

pub(crate) struct PresentationInputs {
    pub(crate) case: CaseInputs,
    pub(crate) reviews: Vec<VerifiedReview>,
    pub(crate) schema: u32,
    pub(crate) rebuilt: Value,
    pub(crate) ir_fingerprint: Option<Fingerprint>,
    saved: Held,
}
impl PresentationInputs {
    pub(crate) fn open(
        project: &Path,
        case: &Path,
        saved: &Path,
        c: &Control,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Self, ProjectError> {
        let inputs = CaseInputs::open(case, c)?;
        let mut reviews = Vec::new();
        let mut units = BTreeSet::new();
        let mut total = inputs.original_bytes().0.len() + inputs.original_bytes().1.len();
        for input in &inputs.config.inputs {
            check(c)?;
            let review = VerifiedReview::open(project, input, inputs.root(), c, cancel)?;
            total += review.files.values().map(Vec::len).sum::<usize>();
            if total > 96 * 1024 * 1024 {
                return Err(error(
                    "PRESENT_INPUT_LIMIT",
                    2,
                    "aggregate source inputs exceed 96 MiB",
                ));
            }
            let binding = &review.source.binding;
            let unit = &review.source.units[0];
            let key = encoded(&json!([
                binding.project_id,
                binding.source_id,
                binding.version_id,
                binding.parse_id,
                unit.unit_id,
                unit.start_byte,
                unit.end_byte
            ]))?;
            if !units.insert(key) {
                return Err(error(
                    "COORD_DUPLICATE_INPUT",
                    2,
                    "repeated actual source unit",
                ));
            }
            reviews.push(review);
        }
        let (schema, rebuilt) = match &inputs.version {
            CaseVersion::V1 => (
                1,
                serde_json::to_value(knowledge_coordinate_facts::coordinate(
                    &inputs.map,
                    &reviews,
                )?),
            ),
            CaseVersion::V2(contexts) => (
                2,
                serde_json::to_value(knowledge_coordinate_variants::coordinate(
                    &inputs.map,
                    contexts,
                    &reviews,
                )?),
            ),
            CaseVersion::V3(contexts) => (
                3,
                serde_json::to_value(knowledge_coordinate_conflicts::coordinate(
                    &inputs.map,
                    contexts,
                    &reviews,
                )?),
            ),
        };
        let rebuilt = rebuilt.map_err(|e| error("PRESENT_SCHEMA", 2, e))?;
        let ready = rebuilt["status"] == "passed";
        let mut held = Held::open(saved)?;
        let mut names = vec![
            ".owner.json".to_owned(),
            "coordination-original.json".into(),
            "identity-original.json".into(),
            "coordination.json".into(),
            "coordination.md".into(),
        ];
        if ready {
            names.push("coordination-ir.json".into());
        }
        for review in &reviews {
            for name in FILES {
                names.push(format!("input-{}--{name}", review.input.input_id));
            }
        }
        held.exact(&names)?;
        let mut saved_total = 0usize;
        for name in &names {
            held.add(name, 8 * 1024 * 1024, c)?;
            saved_total += held.bytes(name)?.len();
            if saved_total > 112 * 1024 * 1024 {
                return Err(error(
                    "PRESENT_INPUT_LIMIT",
                    2,
                    "saved coordination exceeds 112 MiB",
                ));
            }
        }
        let mismatch = || {
            error(
                "PRESENT_COORDINATION_MISMATCH",
                3,
                "saved coordination differs from independently rebuilt source chain",
            )
        };
        if held.bytes("coordination-original.json")? != inputs.original_bytes().0
            || held.bytes("identity-original.json")? != inputs.original_bytes().1
        {
            return Err(mismatch());
        }
        for review in &reviews {
            for (name, bytes) in &review.files {
                if held.bytes(&format!("input-{}--{name}", review.input.input_id))? != bytes {
                    return Err(mismatch());
                }
            }
        }
        let owner: Owner = parse(held.bytes(".owner.json")?)?;
        let id = uuid::Uuid::parse_str(&owner.request_id).map_err(|_| mismatch())?;
        if owner.schema_version != 1
            || owner.producer != "knowledge_coordination"
            || owner.purpose != "unpublished_coordination"
            || id.get_version_num() != 4
            || id.to_string() != owner.request_id
        {
            return Err(mismatch());
        }
        let artifacts = names
            .iter()
            .filter(|n| {
                !matches!(
                    n.as_str(),
                    "coordination.json" | "coordination.md" | "coordination-ir.json"
                )
            })
            .map(|n| Ok((n.clone(), fingerprint(held.bytes(n)?))))
            .collect::<Result<BTreeMap<_, _>, ProjectError>>()?;
        let ir_fingerprint = match schema {
            1 => validate_saved::<
                crate::knowledge_coordination_model::CoordinationIr,
                crate::knowledge_coordination_model::CoordinationReport,
            >(
                &held,
                &rebuilt,
                schema,
                &inputs.config.project_id,
                &owner.request_id,
                artifacts,
            )?,
            2 => validate_saved::<
                crate::knowledge_coordination_v2_model::CoordinationIr,
                crate::knowledge_coordination_v2_model::CoordinationReport,
            >(
                &held,
                &rebuilt,
                schema,
                &inputs.config.project_id,
                &owner.request_id,
                artifacts,
            )?,
            3 => validate_saved::<
                crate::knowledge_coordination_v3_model::CoordinationIr,
                crate::knowledge_coordination_v3_model::CoordinationReport,
            >(
                &held,
                &rebuilt,
                schema,
                &inputs.config.project_id,
                &owner.request_id,
                artifacts,
            )?,
            _ => return Err(mismatch()),
        };
        let value = Self {
            case: inputs,
            reviews,
            schema,
            rebuilt,
            ir_fingerprint,
            saved: held,
        };
        value.verify(c)?;
        Ok(value)
    }
    pub(crate) fn root(&self) -> &Path {
        self.saved.root()
    }
    pub(crate) fn ready(&self) -> bool {
        self.rebuilt["status"] == "passed"
    }
    pub(crate) fn verify(&self, c: &Control) -> Result<(), ProjectError> {
        self.case.verify(c)?;
        for review in &self.reviews {
            review.verify(c)?;
        }
        self.saved.verify(c)?;
        check(c)
    }
    pub(crate) fn copies(&self) -> Result<BTreeMap<String, Vec<u8>>, ProjectError> {
        let mut files = BTreeMap::from([
            (
                ".selection-original.json".into(),
                self.case.original_bytes().0.to_vec(),
            ),
            (
                ".identity-original.json".into(),
                self.case.original_bytes().1.to_vec(),
            ),
            (
                ".coordination-owner.json".into(),
                self.saved.bytes(".owner.json")?.to_vec(),
            ),
            (
                ".coordination-report.json".into(),
                self.saved.bytes("coordination.json")?.to_vec(),
            ),
            (
                ".coordination-report.md".into(),
                self.saved.bytes("coordination.md")?.to_vec(),
            ),
        ]);
        if self.ready() {
            files.insert(
                ".coordination-ir.json".into(),
                self.saved.bytes("coordination-ir.json")?.to_vec(),
            );
        }
        for review in &self.reviews {
            for (name, bytes) in &review.files {
                files.insert(
                    format!(".input-{}--{name}", review.input.input_id),
                    bytes.clone(),
                );
            }
        }
        Ok(files)
    }
    pub(crate) fn facts(&self) -> Result<Vec<PresentationFact>, ProjectError> {
        self.rebuilt["facts"].as_array().ok_or_else(||error("PRESENT_SCHEMA",2,"missing facts"))?.iter().map(|fact|{
            let occurrence_ids:Vec<String>=serde_json::from_value(fact["occurrence_ids"].clone()).map_err(|e|error("PRESENT_SCHEMA",2,e))?;
            let first=occurrence_ids.first().ok_or_else(||error("PRESENT_SCHEMA",2,"empty fact origin"))?;
            let occurrence=self.rebuilt["occurrences"].as_array().and_then(|xs|xs.iter().find(|o|o["occurrence_id"]==first.as_str())).ok_or_else(||error("PRESENT_SCHEMA",2,"unknown fact origin"))?;
            serde_json::from_value(json!({
                "fact_id":fact["fact_id"],"object_id":fact["object_id"],"property_id":fact["property_id"],"value_status":"known",
                "value":occurrence["value"],"unit":occurrence["unit_value"],"conditions":occurrence["conditions"],"exceptions":occurrence["exceptions"],
                "applicability_status":if self.schema==1{Value::Null}else{fact["applicability_status"].clone()},
                "time":if self.schema==1{Value::Null}else{fact["time"].clone()},"occurrence_ids":occurrence_ids
            })).map_err(|e|error("PRESENT_SCHEMA",2,e))
        }).collect()
    }
    pub(crate) fn provenance(&self) -> Result<Vec<Provenance>, ProjectError> {
        self.rebuilt["occurrences"]
            .as_array()
            .ok_or_else(|| error("PRESENT_SCHEMA", 2, "missing occurrences"))?
            .iter()
            .map(|o| {
                let input = o["input_id"]
                    .as_str()
                    .ok_or_else(|| error("PRESENT_SCHEMA", 2, "missing input identity"))?;
                let review = self
                    .reviews
                    .iter()
                    .find(|r| r.input.input_id == input)
                    .ok_or_else(|| error("PRESENT_SCHEMA", 2, "unknown input"))?;
                let atom = &review.candidate.payload.atoms[0];
                let mut seen = BTreeSet::new();
                let mut evidence = vec![];
                for link in &atom.atom.evidence_links {
                    if seen.insert(link.evidence_id.clone()) {
                        evidence.push(
                            review
                                .candidate
                                .payload
                                .evidence
                                .iter()
                                .find(|e| e.evidence_id == link.evidence_id)
                                .ok_or_else(|| {
                                    error("PRESENT_SCHEMA", 2, "missing original evidence")
                                })?
                                .clone(),
                        );
                    }
                }
                let contexts = match &self.case.version {
                    CaseVersion::V1 => None,
                    CaseVersion::V2(c) | CaseVersion::V3(c) => Some(c),
                };
                let context = contexts
                    .and_then(|cs| {
                        cs.iter().find(|x| {
                            x.input_id == input && x.original_atom_id == atom.atom.atom_id
                        })
                    })
                    .cloned();
                Ok(Provenance {
                    occurrence_id: o["occurrence_id"]
                        .as_str()
                        .ok_or_else(|| error("PRESENT_SCHEMA", 2, "missing occurrence"))?
                        .into(),
                    input_id: input.into(),
                    original_atom_id: atom.atom.atom_id.clone(),
                    binding: review.source.binding.clone(),
                    unit: review.source.units[0].clone(),
                    original_concepts: review.candidate.payload.concepts.clone(),
                    original_atom: atom.clone(),
                    evidence,
                    context,
                })
            })
            .collect()
    }
}
fn validate_saved<I: Serialize + DeserializeOwned, R: Serialize + DeserializeOwned>(
    held: &Held,
    facts: &Value,
    schema: u32,
    project_id: &str,
    request_id: &str,
    mut artifacts: BTreeMap<String, Fingerprint>,
) -> Result<Option<Fingerprint>, ProjectError> {
    let mismatch = || {
        error(
            "PRESENT_COORDINATION_MISMATCH",
            3,
            "saved IR/report/Markdown differ from complete reconstructed coordination",
        )
    };
    let scope = match schema {
        1 => "explicit_identity_duplicate_complement_v1",
        2 => "explicit_identity_condition_time_v2",
        3 => "explicit_identity_conflict_review_v3",
        _ => return Err(mismatch()),
    };
    let meta = json!({"schema_version":schema,"producer":"knowledge_coordination","request_id":request_id,"scope":scope,"project_id":project_id,"knowledge_status":"pending","published":false,"scientific_quality":"not_run","research_status":"not_requested"});
    let ready = facts["status"] == "passed";
    let ir_fp = if ready {
        let mut data = facts.clone();
        let map = data.as_object_mut().ok_or_else(mismatch)?;
        map.remove("status");
        map.extend(meta.as_object().ok_or_else(mismatch)?.clone());
        map.insert("revision".into(), json!(1));
        let expected: I =
            serde_json::from_value(data).map_err(|e| error("PRESENT_SCHEMA", 2, e))?;
        let bytes = encoded(&expected)?;
        let actual: I = parse(held.bytes("coordination-ir.json")?)?;
        if encoded(&actual)? != bytes || held.bytes("coordination-ir.json")? != bytes {
            return Err(mismatch());
        }
        let fp = fingerprint(&bytes);
        artifacts.insert("coordination-ir.json".into(), fp.clone());
        Some(fp)
    } else {
        None
    };
    let mut data = facts.clone();
    let map = data.as_object_mut().ok_or_else(mismatch)?;
    map.extend(meta.as_object().ok_or_else(mismatch)?.clone());
    map.extend(json!({"report_kind":"knowledge_coordination","network_calls":0,"paid_calls":0,"artifacts":artifacts}).as_object().ok_or_else(mismatch)?.clone());
    let report: R = serde_json::from_value(data).map_err(|e| error("PRESENT_SCHEMA", 2, e))?;
    let bytes = encoded(&report)?;
    let actual: R = parse(held.bytes("coordination.json")?)?;
    if encoded(&actual)? != bytes || held.bytes("coordination.json")? != bytes {
        return Err(mismatch());
    }
    let text = String::from_utf8(bytes).map_err(|e| error("PRESENT_SCHEMA", 2, e))?;
    let totals = &facts["totals"];
    let count = |n: &str| totals[n].as_u64().ok_or_else(mismatch);
    let summary=match schema {
        1=>"显式身份重复／互补范围；完整来源、原对象、证据与分母见以下事实，与 coordination.json 一致。".into(),
        2=>format!("条件已列明：{}；条件未知：{}；条件不适用：{}；时间已知：{}；时间未知：{}；条件并列关系：{}；历史关系：{}。完整来源、原对象、证据与分母与 coordination.json 一致。",count("known_condition_occurrences")?,count("unknown_condition_occurrences")?,count("not_applicable_condition_occurrences")?,count("dated_occurrences")?,count("undated_occurrences")?,count("condition_variants")?,count("historical_relations")?),
        3=>format!("条件已列明：{}；条件未知：{}；条件不适用：{}；时间已知：{}；时间未知：{}；条件并列关系：{}；历史关系：{}；真实冲突关系：{}；证据不足关系：{}；反证链接：{}。完整来源、原对象、证据与分母与 coordination.json 一致。",count("known_condition_occurrences")?,count("unknown_condition_occurrences")?,count("not_applicable_condition_occurrences")?,count("dated_occurrences")?,count("undated_occurrences")?,count("condition_variants")?,count("historical_relations")?,count("true_conflicts")?,count("insufficient_evidence_relations")?,count("counter_evidence_links")?),
        _=>return Err(mismatch()),
    };
    let status = facts["status"].as_str().ok_or_else(mismatch)?;
    let markdown = format!(
        "# 有界多来源知识协调\n\n状态：{status}；知识状态：pending；发布：false。\n\n科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n{summary}\n\n```json\n{text}\n```\n"
    );
    if held.bytes("coordination.md")? != markdown.as_bytes() {
        return Err(mismatch());
    }
    Ok(ir_fp)
}
