//! Complete, bounded parsing of process_source_zh_v1 selected text.
//!
//! This parser receives only source text. Gold documents cannot select tokens,
//! skip statements, or supply missing fields to the parsing path.
use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;

use crate::{fixture_plan::plain_text, process_model::*};

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_TEXT_BYTES: usize = 1024;

#[derive(Debug)]
pub(crate) struct ParseResult {
    pub(crate) document: Option<ProcessDocument>,
    pub(crate) unparsed_statements: usize,
    pub(crate) issues: Vec<String>,
    pub(crate) observed_totals: BTreeMap<String, usize>,
}

fn rejected(
    unparsed_statements: usize,
    code: &str,
    observed_totals: BTreeMap<String, usize>,
) -> ParseResult {
    ParseResult {
        document: None,
        unparsed_statements,
        issues: vec![code.to_owned()],
        observed_totals,
    }
}

pub(crate) fn parse(text: &str) -> ParseResult {
    if text.len() > MAX_SOURCE_BYTES {
        return rejected(0, "PROCESS_INVALID_DOCUMENT", document_totals(None, 0));
    }
    // Exactly one terminal LF is permitted. Empty physical lines anywhere
    // else remain unconsumed statements, including a second terminal LF.
    let body = text.strip_suffix('\n').unwrap_or(text);
    let mut process = None;
    let mut steps = Vec::new();
    let mut relations = Vec::new();
    let mut evidence = Vec::new();
    let mut unparsed = 0;
    let mut invalid_stage = false;
    let mut relation_stage = false;
    let mut start_byte = 0;
    let mut source_statements = 0;
    let mut process_conditions = 0;
    let mut process_exceptions = 0;
    for (line_index, line) in body.split('\n').enumerate() {
        source_statements += 1;
        let statement = parse_line(line);
        match statement {
            Ok(Statement::Process(value)) => {
                invalid_stage |= line_index != 0 || process.is_some();
                process_conditions += value.conditions.len();
                process_exceptions += value.exceptions.len();
                process = Some(value);
                evidence.push(line_evidence("process", line, start_byte));
            }
            Ok(Statement::Step(value)) => {
                invalid_stage |= process.is_none() || relation_stage;
                evidence.push(line_evidence(&value.id, line, start_byte));
                steps.push(value);
            }
            Ok(Statement::Relation(value)) => {
                invalid_stage |= process.is_none();
                relation_stage = true;
                evidence.push(line_evidence(&value.id, line, start_byte));
                relations.push(value);
            }
            Err(()) => unparsed += 1,
        }
        start_byte += line.len() + 1;
    }
    let has_process = process.is_some();
    // A temporary record carries only independently consumed fields for the
    // observation counters. Its empty process placeholder is never returned
    // or accepted by normalization when the process statement was absent.
    let raw_document = ProcessDocument {
        process: process.unwrap_or(Process {
            title: String::new(),
            entry: String::new(),
            goals: Vec::new(),
            conditions: Vec::new(),
            exceptions: Vec::new(),
        }),
        steps,
        relations,
        evidence,
    };
    let mut observed_totals = document_totals(Some(&raw_document), unparsed);
    observed_totals.insert("source_statements".into(), source_statements);
    // Even invalid duplicate process declarations must not erase observed
    // source fields by keeping only the last declaration's counters.
    observed_totals.insert("process_conditions".into(), process_conditions);
    observed_totals.insert("process_exceptions".into(), process_exceptions);
    // Unknown source has priority: do not present a partial parsed document as
    // successful merely because its remaining records are well formed.
    if unparsed != 0 {
        return rejected(unparsed, "PROCESS_UNPARSED_TEXT", observed_totals);
    }
    if !has_process || invalid_stage {
        return rejected(0, "PROCESS_INVALID_DOCUMENT", observed_totals);
    }
    match normalize(raw_document) {
        Ok(document) => ParseResult {
            document: Some(document),
            unparsed_statements: 0,
            issues: Vec::new(),
            observed_totals,
        },
        Err(_) => rejected(0, "PROCESS_INVALID_DOCUMENT", observed_totals),
    }
}

fn line_evidence(object_id: &str, line: &str, start_byte: usize) -> Evidence {
    Evidence {
        object_id: object_id.to_owned(),
        start_byte,
        end_byte: start_byte + line.len(),
        quote: line.to_owned(),
    }
}

enum Statement {
    Process(Process),
    Step(Step),
    Relation(Relation),
}

struct Cursor<'a> {
    tail: &'a str,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, literal: &str) -> Result<(), ()> {
        self.tail = self.tail.strip_prefix(literal).ok_or(())?;
        Ok(())
    }

    fn id(&mut self) -> Result<String, ()> {
        if !self
            .tail
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_lowercase)
        {
            return Err(());
        }
        let length = self
            .tail
            .bytes()
            .take_while(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
            .count();
        let id = self.tail[..length].to_owned();
        self.tail = &self.tail[length..];
        Ok(id)
    }

    fn json<T: DeserializeOwned>(&mut self, leading: char) -> Result<T, ()> {
        if !self.tail.starts_with(leading) {
            return Err(());
        }
        // Strings and arrays are self-delimited JSON tokens; byte_offset stops
        // at their real closing delimiter, including escaped quotes correctly.
        let mut values = serde_json::Deserializer::from_str(self.tail).into_iter::<T>();
        let value = values.next().ok_or(())?.map_err(|_| ())?;
        let consumed = values.byte_offset();
        self.tail = &self.tail[consumed..];
        Ok(value)
    }

    fn string(&mut self) -> Result<String, ()> {
        self.json('"')
    }

    fn strings(&mut self) -> Result<Vec<String>, ()> {
        self.json('[')
    }

    fn criterion(&mut self) -> Result<Option<String>, ()> {
        if self.tail.starts_with('"') {
            self.string().map(Some)
        } else {
            // Unlike a quoted string, JSON null is not self-delimited. The
            // following template literal checks its exact boundary.
            self.take("null")?;
            Ok(None)
        }
    }

    fn conditions_exceptions(&mut self) -> Result<(Vec<String>, Vec<String>), ()> {
        self.take("。条件：")?;
        let conditions = self.strings()?;
        self.take("。例外：")?;
        let exceptions = self.strings()?;
        self.take("。")?;
        Ok((conditions, exceptions))
    }
}

fn parse_line(line: &str) -> Result<Statement, ()> {
    if line.is_empty() || line.contains('\r') {
        return Err(());
    }
    let mut cursor = Cursor { tail: line };
    let statement = if line.starts_with("过程：") {
        cursor.take("过程：")?;
        let title = cursor.string()?;
        cursor.take("。入口：")?;
        let entry = cursor.id()?;
        cursor.take("。目标：")?;
        let goals = cursor.strings()?;
        let (conditions, exceptions) = cursor.conditions_exceptions()?;
        Statement::Process(Process {
            title,
            entry,
            goals,
            conditions,
            exceptions,
        })
    } else if line.starts_with("步骤 ") || line.starts_with("判定 ") {
        let decision = line.starts_with("判定 ");
        cursor.take(if decision { "判定 " } else { "步骤 " })?;
        let id = cursor.id()?;
        cursor.take("：")?;
        let label = cursor.string()?;
        let criterion = if decision {
            cursor.take("。判据：")?;
            cursor.criterion()?
        } else {
            None
        };
        cursor.take("。说明：")?;
        let description = cursor.string()?;
        cursor.take("。输入：")?;
        let inputs = cursor.strings()?;
        cursor.take("。输出：")?;
        let outputs = cursor.strings()?;
        let (conditions, exceptions) = cursor.conditions_exceptions()?;
        Statement::Step(Step {
            id,
            kind: if decision {
                StepKind::Decision
            } else {
                StepKind::Step
            },
            label,
            description,
            criterion,
            inputs,
            outputs,
            conditions,
            exceptions,
        })
    } else if line.starts_with("关系 ") {
        cursor.take("关系 ")?;
        let id = cursor.id()?;
        cursor.take("：")?;
        let left = cursor.id()?;
        let (right, kind, branch_outcome) = if cursor.tail.starts_with(" 与 ") {
            cursor.take(" 与 ")?;
            let right = cursor.id()?;
            let kind = if cursor.tail.starts_with(" 相关") {
                cursor.take(" 相关")?;
                RelationKind::Correlates
            } else {
                cursor.take(" 的方向未说明")?;
                RelationKind::Unknown
            };
            (right, kind, None)
        } else {
            let (literal, kind, outcome) = [
                (" 完成后执行 ", RelationKind::Sequence, None),
                (" 的判据为真时执行 ", RelationKind::Branch, Some(true)),
                (" 的判据为假时执行 ", RelationKind::Branch, Some(false)),
                (" 导致 ", RelationKind::Causes, None),
                (" 促进 ", RelationKind::Promotes, None),
                (" 抑制 ", RelationKind::Inhibits, None),
            ]
            .into_iter()
            .find(|(literal, _, _)| cursor.tail.starts_with(literal))
            .ok_or(())?;
            cursor.take(literal)?;
            (cursor.id()?, kind, outcome)
        };
        let (conditions, exceptions) = cursor.conditions_exceptions()?;
        Statement::Relation(Relation {
            id,
            left,
            right,
            kind,
            branch_outcome,
            conditions,
            exceptions,
        })
    } else {
        return Err(());
    };
    if !cursor.tail.is_empty() {
        return Err(());
    }
    Ok(statement)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn valid_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_TEXT_BYTES && plain_text(value)
}

fn valid_texts(values: &[String]) -> bool {
    values.len() <= 8
        && values.iter().all(|value| valid_text(value))
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

/// Validate the document's own structure without interpreting external text or
/// asserting that its evidence quotes are the actual selected source. The
/// caller must still compare the complete independently parsed source result.
pub(crate) fn validate_document(document: &ProcessDocument) -> Result<(), String> {
    require(
        (2..=12).contains(&document.steps.len()),
        "process requires 2..12 steps",
    )?;
    require(
        document.relations.len() <= 24,
        "process exceeds 24 relations",
    )?;
    let process = &document.process;
    require(valid_text(&process.title), "invalid process title")?;
    require(valid_id(&process.entry), "invalid process entry")?;
    require(
        valid_texts(&process.goals) && process.goals.iter().all(|id| valid_id(id)),
        "invalid or duplicate process goals",
    )?;
    require(
        valid_texts(&process.conditions) && valid_texts(&process.exceptions),
        "invalid process conditions or exceptions",
    )?;
    let mut object_ids = BTreeSet::from(["process"]);
    let mut steps = BTreeMap::new();
    for step in &document.steps {
        require(
            valid_id(&step.id) && object_ids.insert(step.id.as_str()),
            "invalid, duplicate or reserved step id",
        )?;
        require(
            valid_text(&step.label) && valid_text(&step.description),
            "invalid step label or description",
        )?;
        require(
            step.criterion.as_deref().is_none_or(valid_text),
            "invalid decision criterion",
        )?;
        require(
            step.kind == StepKind::Decision || step.criterion.is_none(),
            "ordinary step has a decision criterion",
        )?;
        require(
            [
                &step.inputs,
                &step.outputs,
                &step.conditions,
                &step.exceptions,
            ]
            .into_iter()
            .all(|values| valid_texts(values)),
            "invalid or duplicate step text list",
        )?;
        steps.insert(step.id.as_str(), step);
    }
    require(
        steps.contains_key(process.entry.as_str())
            && process
                .goals
                .iter()
                .all(|id| steps.contains_key(id.as_str())),
        "entry or goal references an undeclared step",
    )?;
    let mut relation_ids = BTreeSet::new();
    for relation in &document.relations {
        require(
            valid_id(&relation.id) && object_ids.insert(relation.id.as_str()),
            "invalid, duplicate or reserved relation id",
        )?;
        require(
            steps.contains_key(relation.left.as_str())
                && steps.contains_key(relation.right.as_str()),
            "relation references an undeclared step",
        )?;
        require(
            valid_texts(&relation.conditions) && valid_texts(&relation.exceptions),
            "invalid or duplicate relation text list",
        )?;
        if relation.kind == RelationKind::Branch {
            require(
                relation.branch_outcome.is_some()
                    && steps[relation.left.as_str()].kind == StepKind::Decision,
                "branch requires a decision source and explicit boolean outcome",
            )?;
        } else {
            require(
                relation.branch_outcome.is_none(),
                "non-branch relation has a branch outcome",
            )?;
        }
        relation_ids.insert(relation.id.as_str());
    }
    require(
        document.evidence.len() == object_ids.len() && document.evidence.len() <= 37,
        "evidence must cover every process object exactly once",
    )?;
    let mut evidence_ids = BTreeSet::new();
    for evidence in &document.evidence {
        require(
            object_ids.contains(evidence.object_id.as_str())
                && evidence_ids.insert(evidence.object_id.as_str()),
            "unknown or duplicate evidence object",
        )?;
        require(
            evidence.start_byte < evidence.end_byte
                && evidence.end_byte <= MAX_SOURCE_BYTES
                && evidence.quote.len() == evidence.end_byte - evidence.start_byte
                && !evidence.quote.contains(['\n', '\r']),
            "invalid UTF-8 evidence length, range or physical line",
        )?;
    }
    let mut ordered: Vec<_> = document.evidence.iter().collect();
    ordered.sort_by_key(|evidence| evidence.start_byte);
    require(
        ordered[0].object_id == "process" && ordered[0].start_byte == 0,
        "process evidence must begin at the first source byte",
    )?;
    let mut relation_stage = false;
    let mut previous_end = 0;
    for evidence in ordered {
        require(
            previous_end <= evidence.start_byte,
            "overlapping evidence ranges",
        )?;
        previous_end = evidence.end_byte;
        if relation_ids.contains(evidence.object_id.as_str()) {
            relation_stage = true;
        } else if steps.contains_key(evidence.object_id.as_str()) {
            require(
                !relation_stage,
                "step evidence occurs after relation declarations",
            )?;
        }
    }
    Ok(())
}

pub(crate) fn normalize(mut document: ProcessDocument) -> Result<ProcessDocument, String> {
    validate_document(&document)?;
    document.steps.sort_by(|a, b| a.id.cmp(&b.id));
    document.relations.sort_by(|a, b| a.id.cmp(&b.id));
    document
        .evidence
        .sort_by(|a, b| a.object_id.cmp(&b.object_id));
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    fn fixture(name: &str) -> String {
        fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/process-flow-v1")
                .join(format!("{name}.txt")),
        )
        .unwrap()
    }

    fn document() -> ProcessDocument {
        parse(&fixture("linear")).document.unwrap()
    }

    #[test]
    fn frozen_authors_match_all_parsed_fields_and_real_utf8_ranges() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/process-flow-v1");
        let mut count = 0;
        for entry in fs::read_dir(&root).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|ext| ext != "txt") {
                continue;
            }
            count += 1;
            let name = path.file_stem().unwrap().to_str().unwrap();
            let source = fs::read_to_string(&path).unwrap();
            let parsed = parse(&source);
            let expected: serde_json::Value = serde_json::from_slice(
                &fs::read(root.join(format!("expected-{name}.json"))).unwrap(),
            )
            .unwrap();
            for (key, count) in &parsed.observed_totals {
                assert_eq!(
                    expected["totals"][key],
                    serde_json::json!(count),
                    "{name} observed {key}"
                );
            }
            if ["unparsed_tail", "negated_relation"].contains(&name) {
                assert!(parsed.document.is_none(), "{name}");
                assert_eq!(parsed.unparsed_statements, 1, "{name}");
                assert_eq!(parsed.issues, ["PROCESS_UNPARSED_TEXT"]);
            } else {
                let gold: serde_json::Value = serde_json::from_slice(
                    &fs::read(root.join(format!("gold-{name}.json"))).unwrap(),
                )
                .unwrap();
                let authored: ProcessDocument = serde_json::from_value(serde_json::json!({"process": gold["process"], "steps": gold["steps"], "relations": gold["relations"], "evidence": gold["evidence"]})).unwrap();
                let actual = parsed
                    .document
                    .unwrap_or_else(|| panic!("{name}: {:?}", parsed.issues));
                assert_eq!(actual, normalize(authored).unwrap(), "{name}");
                for evidence in &actual.evidence {
                    assert_eq!(
                        source.get(evidence.start_byte..evidence.end_byte),
                        Some(evidence.quote.as_str()),
                        "{name}"
                    );
                }
                assert_eq!(parsed.unparsed_statements, 0);
                assert!(parsed.issues.is_empty());
            }
        }
        assert_eq!(count, 16);
    }

    #[test]
    fn complete_consumption_rejects_unknown_fields_tails_and_physical_blank_lines() {
        let source = fixture("linear");
        for changed in [
            format!("\n{source}"),
            format!("{source}\n"),
            source.replacen('\n', "\n\n", 1),
            source.replace('\n', "\r\n"),
            source.replacen("。入口：", "。未知：[]。入口：", 1),
            format!("{source}关系 r9：s1 不导致 s2。条件：[]。例外：[]。\n"),
            source.replacen("。例外：", " 。例外：", 1),
        ] {
            let parsed = parse(&changed);
            assert!(parsed.document.is_none());
            assert!(parsed.unparsed_statements > 0);
            assert_eq!(parsed.issues, ["PROCESS_UNPARSED_TEXT"]);
        }
        assert!(parse(source.trim_end_matches('\n')).document.is_some());
        assert_eq!(parse("").unparsed_statements, 1);
    }

    #[test]
    fn json_tokens_preserve_quoted_delimiters_and_do_not_infer_relations() {
        let source = fixture("linear");
        let label = "标签。入口：s9。关系 r9：s1 不导致 s2 | \\\"\n\t保留";
        let changed = source.replacen("\"登记卡片\"", &serde_json::to_string(label).unwrap(), 1);
        let actual = parse(&changed).document.unwrap();
        assert_eq!(actual.steps[0].label, label);
        assert_eq!(actual.relations.len(), 2);
        let spaced_array =
            source.replacen("[\"演示模式开启\"]", "[ \"演示模式开启\" , \"条件二\" ]", 1);
        assert_eq!(
            parse(&spaced_array).document.unwrap().process.conditions,
            ["演示模式开启", "条件二"]
        );
        let leading_space = source.replacen("目标：[", "目标： [", 1);
        assert!(parse(&leading_space).document.is_none());
    }

    #[test]
    fn known_but_invalid_structure_cannot_create_a_document() {
        let source = fixture("linear");
        let lines: Vec<_> = source.lines().collect();
        let reordered = [lines[0], lines[1], lines[4], lines[2], lines[3], lines[5]].join("\n");
        let duplicate_process = format!("{}\n{source}", lines[0]);
        let repeated = source.replacen("[\"演示模式开启\"]", "[\"重复\",\"重复\"]", 1);
        for changed in [
            reordered,
            duplicate_process,
            repeated,
            source.replacen("入口：s1", "入口：absent", 1),
        ] {
            let parsed = parse(&changed);
            assert!(parsed.document.is_none());
            assert_eq!(parsed.unparsed_statements, 0);
            assert_eq!(parsed.issues, ["PROCESS_INVALID_DOCUMENT"]);
        }
    }

    #[test]
    fn document_validation_rejects_counterfeit_evidence_and_type_fields() {
        let mut changed = document();
        changed.evidence[0].quote.push('x');
        assert!(validate_document(&changed).is_err());
        let mut changed = document();
        changed.evidence[0].object_id = "absent".into();
        assert!(validate_document(&changed).is_err());
        let mut changed = document();
        changed.evidence[1] = changed.evidence[0].clone();
        assert!(validate_document(&changed).is_err());
        let mut changed = document();
        changed.relations[0].branch_outcome = Some(false);
        assert!(validate_document(&changed).is_err());
        let mut changed = document();
        changed.relations[0].kind = RelationKind::Branch;
        changed.relations[0].branch_outcome = Some(true);
        assert!(validate_document(&changed).is_err());
        let mut changed = document();
        changed.steps[0].criterion = Some("假设判据".into());
        assert!(validate_document(&changed).is_err());
        let mut changed = document();
        changed.steps[0].id = "process".into();
        assert!(validate_document(&changed).is_err());
    }

    #[test]
    fn limits_and_empty_arrays_are_explicit_and_normalization_preserves_author_order() {
        let mut changed = document();
        changed.process.goals.clear();
        assert!(validate_document(&changed).is_ok());
        let mut changed = document();
        changed.process.conditions = vec!["二".into(), "一".into()];
        changed.steps.reverse();
        changed.evidence.reverse();
        let normalized = normalize(changed).unwrap();
        assert_eq!(normalized.process.conditions, ["二", "一"]);
        assert_eq!(normalized.steps[0].id, "s1");
        let mut changed = document();
        changed.steps[0].label = "x".repeat(1025);
        assert!(validate_document(&changed).is_err());
        let mut changed = document();
        changed.process.conditions = (0..9).map(|n| n.to_string()).collect();
        assert!(validate_document(&changed).is_err());
        assert_eq!(
            parse(&"x".repeat(MAX_SOURCE_BYTES + 1)).issues,
            ["PROCESS_INVALID_DOCUMENT"]
        );
        let unknown = parse(&(fixture("linear") + "不认识的第一行\n不认识的第二行\n"));
        assert_eq!(unknown.unparsed_statements, 2);
        assert_eq!(unknown.observed_totals["source_statements"], 8);
        assert_eq!(unknown.observed_totals["parsed_statements"], 6);
        assert_eq!(unknown.observed_totals["steps"], 3);
        assert!(unknown.document.is_none());
    }
}
