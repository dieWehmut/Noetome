use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
#[path = "support/presentation_case.rs"]
mod support;
use support::*;

fn ready_case(name: &str) -> PresentationCase {
    let mut f = PresentationCase::prepare(name);
    let run = f.coordinate();
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    f.freeze_coordination();
    f
}

fn read_json(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

fn pin(path: impl AsRef<Path>) -> Value {
    let bytes = fs::read(path).unwrap();
    json!({"sha256":hash(&bytes),"byte_length":bytes.len()})
}

fn assert_cli_error(run: &std::process::Output, exit: i32, family: &str) {
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(exit), "{stderr}");
    assert!(stderr.starts_with(family), "expected {family}: {stderr}");
}

// Decode the documented finite plain-text escaping. This reads saved content
// ranges independently; links and their aliases cannot satisfy body assertions.
fn decode_plain(text: &str) -> String {
    let mut out = String::new();
    let mut at = 0;
    while at < text.len() {
        let tail = &text[at..];
        if tail.starts_with("<br>") {
            out.push('\n');
            at += 4;
        } else if tail.starts_with("&#9;") {
            out.push('\t');
            at += 4;
        } else if tail.starts_with('\\') {
            let ch = tail[1..].chars().next().expect("truncated plain escape");
            assert!(ch.is_ascii_punctuation(), "invalid plain escape {tail}");
            out.push(ch);
            at += 1 + ch.len_utf8();
        } else {
            let ch = tail.chars().next().unwrap();
            assert!(
                !ch.is_ascii_punctuation(),
                "unescaped source punctuation {tail}"
            );
            out.push(ch);
            at += ch.len_utf8();
        }
    }
    out
}

fn assert_fact_text(text: &str, fact: &Value) {
    for key in ["value", "unit"] {
        let required = fact[key].as_str().unwrap();
        assert!(
            text.contains(required),
            "body omitted {key}={required:?}: {text}"
        );
    }
    for key in ["conditions", "exceptions"] {
        for required in fact[key].as_array().unwrap() {
            let required = required.as_str().unwrap();
            assert!(
                text.contains(required),
                "body omitted complete {key}={required:?}: {text}"
            );
        }
    }
    if fact["time"]["status"] == "known" {
        let required = fact["time"]["value"].as_str().unwrap();
        assert!(
            text.contains(required),
            "body omitted known time={required}: {text}"
        );
    }
}

fn scope_target<'a>(note: &'a Value, scope: &Value) -> &'a Value {
    assert_eq!(scope["section_id"], note["section_id"]);
    let node = note["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["node_id"] == scope["node_id"])
        .unwrap();
    if node["kind"] == "paragraph" {
        assert_eq!(scope["kind"], "paragraph");
        return node;
    }
    if scope["kind"] == "table_header" {
        assert!(scope["row_id"].is_null());
        return node["columns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["column_id"] == scope["column_id"])
            .unwrap();
    }
    let row = node["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["row_id"] == scope["row_id"])
        .unwrap();
    let cell = row["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["column_id"] == scope["column_id"])
        .unwrap();
    assert_eq!(scope["kind"], cell["kind"]);
    cell
}

fn assert_preview_semantics(f: &PresentationCase, author: &Value, expected: &Value) {
    let plan = read_json(f.preview.join("plan.json"));
    let report = read_json(f.preview.join("presentation.json"));
    for value in [&plan, &report] {
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["producer"], "knowledge_presentation");
        assert_eq!(value["scope"], "active_presentation_v1");
        assert_eq!(value["knowledge_status"], "pending");
        assert_eq!(value["scientific_quality"], "not_run");
        assert_eq!(value["research_status"], "not_requested");
        assert_eq!(value["published"], false);
        assert_eq!(
            value["coordination_schema_version"],
            author["coordination_expected"]["schema_version"]
        );
    }
    assert_eq!(plan["representation_origin"], "deterministic_planner");
    assert_eq!(plan["revision"], 1);
    assert_eq!(plan["representation"], expected["representation"]);
    assert_eq!(report["request_id"], plan["request_id"]);
    assert_eq!(report["project_id"], plan["project_id"]);
    assert_eq!(report["status"], "passed");
    assert_eq!(report["network_calls"], 0);
    assert_eq!(report["paid_calls"], 0);
    for value in report["checks"].as_object().unwrap().values() {
        assert_eq!(value, "passed");
    }
    assert_eq!(report["plan_fingerprint"], pin(f.preview.join("plan.json")));
    assert_eq!(
        plan["coordination_ir_fingerprint"],
        pin(f.output.join("coordination-ir.json"))
    );
    assert_eq!(
        report["coordination_ir_fingerprint"],
        plan["coordination_ir_fingerprint"]
    );

    for (actual_key, expected_key, id_key) in [
        ("objects", "objects", "object_id"),
        ("properties", "properties", "property_id"),
    ] {
        let actual: BTreeMap<_, _> = plan[actual_key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v["concept_id"].as_str().unwrap(),
                    v["label"].as_str().unwrap(),
                )
            })
            .collect();
        let declared: BTreeMap<_, _> = expected[expected_key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v[id_key].as_str().unwrap(), v["label"].as_str().unwrap()))
            .collect();
        assert_eq!(
            actual.len(),
            plan[actual_key].as_array().unwrap().len(),
            "duplicate label IDs"
        );
        assert_eq!(actual, declared, "independently authored {actual_key}");
    }
    let provenance: BTreeMap<_, _> = plan["provenance"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["occurrence_id"].as_str().unwrap().to_owned(), v))
        .collect();
    assert_eq!(
        provenance.len(),
        plan["provenance"].as_array().unwrap().len()
    );
    let actual_refs: BTreeSet<_> = provenance
        .values()
        .map(|v| {
            (
                v["input_id"].as_str().unwrap(),
                v["original_atom_id"].as_str().unwrap(),
            )
        })
        .collect();
    let expected_refs: BTreeSet<_> = expected["required_atom_refs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["input_id"].as_str().unwrap(),
                v["original_atom_id"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(actual_refs, expected_refs);
    let selection = read_json(f.case.join("coordination.json"));
    for occurrence in provenance.values() {
        let input_id = occurrence["input_id"].as_str().unwrap();
        let input = selection["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["input_id"] == input_id)
            .unwrap();
        let authored = author["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["input_id"] == input_id)
            .unwrap();
        let bundle = f.case.join(input["review_bundle"].as_str().unwrap());
        let candidate = read_json(bundle.join("candidates-original.json"));
        let snapshot = read_json(bundle.join("source-snapshot.json"));
        assert_eq!(occurrence["binding"], input["binding"]);
        assert_eq!(occurrence["unit"], snapshot["units"][0]);
        assert_eq!(
            occurrence["unit"]["text"],
            fs::read_to_string(f.root.join(format!("selected-{input_id}.txt"))).unwrap()
        );
        assert_eq!(
            occurrence["original_concepts"],
            candidate["payload"]["concepts"]
        );
        assert_eq!(
            occurrence["original_atom"],
            candidate["payload"]["atoms"][0]
        );
        assert_eq!(
            occurrence["original_atom"]["atom"]["content"],
            authored["gold"]["atom_content"]
        );
        assert_eq!(occurrence["context"], authored["context"]);
        assert_eq!(occurrence["evidence"], candidate["payload"]["evidence"]);
        for evidence in occurrence["evidence"].as_array().unwrap() {
            let source = occurrence["unit"]["text"].as_str().unwrap();
            let start = evidence["excerpt_start_byte"].as_u64().unwrap() as usize;
            let end = evidence["excerpt_end_byte"].as_u64().unwrap() as usize;
            assert_eq!(&source[start..end], evidence["quote"].as_str().unwrap());
            assert_eq!(
                hash(source[start..end].as_bytes()),
                evidence["excerpt_sha256"]
            );
        }
    }

    let facts = plan["facts"].as_array().unwrap();
    let fact_by_id: BTreeMap<_, _> = facts
        .iter()
        .map(|v| (v["fact_id"].as_str().unwrap().to_owned(), v))
        .collect();
    assert_eq!(fact_by_id.len(), facts.len());
    let mut matched_ids = BTreeSet::new();
    let mut expected_cell_ids: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for cell in expected["cells"].as_array().unwrap() {
        let mut ids = BTreeSet::new();
        for wanted in cell["facts"].as_array().unwrap() {
            let matching: Vec<_> = facts
                .iter()
                .filter(|fact| {
                    fact["object_id"] == cell["object_id"]
                        && fact["property_id"] == cell["property_id"]
                        && [
                            "value_status",
                            "value",
                            "unit",
                            "conditions",
                            "exceptions",
                            "applicability_status",
                            "time",
                        ]
                        .iter()
                        .all(|key| fact[*key] == wanted[*key])
                })
                .collect();
            assert_eq!(matching.len(), 1, "missing/excess authored fact: {wanted}");
            let actual = matching[0];
            let id = actual["fact_id"].as_str().unwrap().to_owned();
            assert!(
                matched_ids.insert(id.clone()),
                "fact reused between authored cells"
            );
            ids.insert(id);
            let refs: BTreeSet<_> = strings(&actual["occurrence_ids"])
                .iter()
                .map(|id| {
                    let occurrence = provenance[id];
                    (
                        occurrence["input_id"].as_str().unwrap().to_owned(),
                        occurrence["original_atom_id"].as_str().unwrap().to_owned(),
                    )
                })
                .collect();
            let wanted_refs: BTreeSet<_> = wanted["input_ids"]
                .as_array()
                .unwrap()
                .iter()
                .zip(wanted["original_atom_ids"].as_array().unwrap())
                .map(|(input, atom)| {
                    (
                        input.as_str().unwrap().to_owned(),
                        atom.as_str().unwrap().to_owned(),
                    )
                })
                .collect();
            assert_eq!(refs, wanted_refs, "full authored source occurrences");
        }
        expected_cell_ids.insert(
            (
                cell["object_id"].as_str().unwrap().to_owned(),
                cell["property_id"].as_str().unwrap().to_owned(),
            ),
            ids,
        );
    }
    assert_eq!(
        matched_ids.len(),
        facts.len(),
        "unrequested or dropped facts"
    );
    for (labels, role) in [
        (&plan["objects"], "object_id"),
        (&plan["properties"], "property_id"),
    ] {
        for label in labels.as_array().unwrap() {
            let expected_ids: BTreeSet<_> = facts
                .iter()
                .filter(|v| v[role] == label["concept_id"])
                .flat_map(|v| strings(&v["occurrence_ids"]))
                .collect();
            assert_eq!(strings(&label["occurrence_ids"]), expected_ids);
        }
    }
    let shared: BTreeSet<_> = expected["properties"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|property| {
            let objects: BTreeSet<_> = expected["cells"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|c| c["property_id"] == property["property_id"] && c["state"] == "facts")
                .map(|c| c["object_id"].as_str().unwrap())
                .collect();
            (objects.len() >= 2).then(|| property["property_id"].as_str().unwrap().to_owned())
        })
        .collect();
    assert_eq!(strings(&plan["decision"]["shared_property_ids"]), shared);
    assert!(!plan["decision"]["reason"].as_str().unwrap().is_empty());

    let notes = plan["notes"].as_array().unwrap();
    let comparison = notes
        .iter()
        .find(|n| n["output_path"] == "知识比较.md")
        .unwrap();
    let mut seen_cells = BTreeSet::new();
    let mut main_facts = BTreeSet::new();
    for node in comparison["nodes"].as_array().unwrap() {
        if node["kind"] == "paragraph" {
            assert_eq!(expected["representation"], "paragraph");
            for id in strings(&node["fact_ids"]) {
                main_facts.insert(id.clone());
                assert_fact_text(node["text"].as_str().unwrap(), fact_by_id[&id]);
            }
            continue;
        }
        assert_eq!(expected["representation"], "table");
        for column in node["columns"].as_array().unwrap() {
            if !column["property_id"].is_null() {
                let label = expected["properties"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|property| property["property_id"] == column["property_id"])
                    .unwrap();
                assert_eq!(
                    column["label"], label["label"],
                    "independent property column label"
                );
                assert_eq!(
                    strings(&column["concept_ids"]),
                    BTreeSet::from([column["property_id"].as_str().unwrap().to_owned()])
                );
            }
        }
        for row in node["rows"].as_array().unwrap() {
            for cell in row["cells"].as_array().unwrap() {
                let column = node["columns"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| c["column_id"] == cell["column_id"])
                    .unwrap();
                if column["property_id"].is_null() {
                    assert_eq!(cell["kind"], "label");
                    let label = expected["objects"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|l| l["object_id"] == row["object_id"])
                        .unwrap();
                    assert_eq!(cell["text"], label["label"]);
                    assert_eq!(
                        strings(&cell["concept_ids"]),
                        BTreeSet::from([row["object_id"].as_str().unwrap().to_owned()])
                    );
                    let object_fact_ids: BTreeSet<_> = facts
                        .iter()
                        .filter(|fact| fact["object_id"] == row["object_id"])
                        .map(|fact| fact["fact_id"].as_str().unwrap().to_owned())
                        .collect();
                    let object_occurrence_ids: BTreeSet<_> = object_fact_ids
                        .iter()
                        .flat_map(|id| strings(&fact_by_id[id]["occurrence_ids"]))
                        .collect();
                    assert!(
                        strings(&cell["fact_ids"]).is_empty(),
                        "identity label does not assert fact quantities or body"
                    );
                    assert_eq!(
                        strings(&cell["occurrence_ids"]),
                        object_occurrence_ids,
                        "object label complete source references"
                    );
                    continue;
                }
                let key = (
                    row["object_id"].as_str().unwrap().to_owned(),
                    column["property_id"].as_str().unwrap().to_owned(),
                );
                assert!(
                    seen_cells.insert(key.clone()),
                    "duplicate object/property cell"
                );
                let ids = &expected_cell_ids[&key];
                assert_eq!(&strings(&cell["fact_ids"]), ids, "cell facts at {key:?}");
                if ids.is_empty() {
                    assert_eq!(cell["kind"], "missing");
                    assert_eq!(cell["text"], "选定事实未给出");
                    assert!(strings(&cell["occurrence_ids"]).is_empty());
                    assert!(strings(&cell["concept_ids"]).is_empty());
                } else {
                    assert_eq!(cell["kind"], "facts");
                    assert_eq!(
                        strings(&cell["concept_ids"]),
                        BTreeSet::from([key.0.clone(), key.1.clone()])
                    );
                    let occurrence_ids: BTreeSet<_> = ids
                        .iter()
                        .flat_map(|id| strings(&fact_by_id[id]["occurrence_ids"]))
                        .collect();
                    assert_eq!(strings(&cell["occurrence_ids"]), occurrence_ids);
                    for id in ids {
                        main_facts.insert(id.clone());
                        assert_fact_text(cell["text"].as_str().unwrap(), fact_by_id[id]);
                    }
                }
            }
        }
    }
    assert_eq!(main_facts, matched_ids);
    if expected["representation"] == "table" {
        assert_eq!(seen_cells, expected_cell_ids.keys().cloned().collect());
    }
    for object in expected["objects"].as_array().unwrap() {
        let path = format!("对象-{}.md", object["object_id"].as_str().unwrap());
        let detail = notes.iter().find(|n| n["output_path"] == path).unwrap();
        assert_eq!(detail["heading"], "知识明细");
        let ids: BTreeSet<_> = detail["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|n| strings(&n["fact_ids"]))
            .collect();
        let wanted: BTreeSet<_> = facts
            .iter()
            .filter(|v| v["object_id"] == object["object_id"])
            .map(|v| v["fact_id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(ids, wanted, "object detail fact coverage");
        if expected["representation"] == "table" {
            assert!(
                comparison["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|node| node["rows"].as_array().unwrap())
                    .flat_map(|row| row["cells"].as_array().unwrap())
                    .flat_map(|cell| cell["links"].as_array().unwrap())
                    .any(|link| link["target_concept_id"] == object["object_id"]
                        && link["target_path"] == path
                        && link["target_heading"] == "知识明细"),
                "table has no structured link to object detail"
            );
        }
    }

    let mut content_count = 0;
    let mut link_count = 0;
    let mut mapped_scopes = BTreeSet::new();
    let mut planned_scopes = BTreeSet::new();
    let mut planned_link_count = 0;
    for note in notes {
        for node in note["nodes"].as_array().unwrap() {
            let scope = |kind: &str, row: Value, column: Value| {
                serde_json::to_string(&json!([
                    note["note_id"],
                    note["section_id"],
                    node["node_id"],
                    kind,
                    row,
                    column
                ]))
                .unwrap()
            };
            if node["kind"] == "paragraph" {
                planned_scopes.insert(scope("paragraph", Value::Null, Value::Null));
                planned_link_count += node["links"].as_array().unwrap().len();
            } else {
                for column in node["columns"].as_array().unwrap() {
                    planned_scopes.insert(scope(
                        "table_header",
                        Value::Null,
                        column["column_id"].clone(),
                    ));
                }
                for row in node["rows"].as_array().unwrap() {
                    for cell in row["cells"].as_array().unwrap() {
                        planned_scopes.insert(scope(
                            cell["kind"].as_str().unwrap(),
                            row["row_id"].clone(),
                            cell["column_id"].clone(),
                        ));
                        planned_link_count += cell["links"].as_array().unwrap().len();
                    }
                }
            }
        }
    }
    for file in report["files"].as_array().unwrap() {
        let bytes = fs::read(f.preview.join(file["relative_path"].as_str().unwrap())).unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert_eq!(file["content_sha256"], hash(&bytes));
        assert_eq!(file["byte_length"], bytes.len());
        assert_eq!(file["plan_revision"], 1);
        let note = notes
            .iter()
            .find(|n| n["note_id"] == file["note_id"])
            .unwrap();
        assert_eq!(file["relative_path"], note["output_path"]);
        for mapping in file["content_mappings"].as_array().unwrap() {
            content_count += 1;
            let scope = &mapping["scope"];
            assert!(
                mapped_scopes.insert(
                    serde_json::to_string(&json!([
                        file["note_id"],
                        scope["section_id"],
                        scope["node_id"],
                        scope["kind"],
                        scope["row_id"],
                        scope["column_id"]
                    ]))
                    .unwrap()
                ),
                "duplicate content scope"
            );
            let start = mapping["start_byte"].as_u64().unwrap() as usize;
            let end = mapping["end_byte"].as_u64().unwrap() as usize;
            let body = decode_plain(&text[start..end]);
            let target = scope_target(note, &mapping["scope"]);
            assert_eq!(
                body,
                if mapping["scope"]["kind"] == "table_header" {
                    target["label"].as_str().unwrap()
                } else {
                    target["text"].as_str().unwrap()
                }
            );
            for key in ["occurrence_ids", "concept_ids"] {
                assert_eq!(strings(&mapping[key]), strings(&target[key]));
            }
            if mapping["scope"]["kind"] != "table_header" {
                assert_eq!(strings(&mapping["fact_ids"]), strings(&target["fact_ids"]));
            } else {
                assert!(strings(&mapping["fact_ids"]).is_empty());
            }
            if matches!(
                mapping["scope"]["kind"].as_str(),
                Some("facts" | "paragraph")
            ) {
                for id in strings(&mapping["fact_ids"]) {
                    assert_fact_text(&body, fact_by_id[&id]);
                }
            }
        }
        for link in file["links"].as_array().unwrap() {
            link_count += 1;
            let start = link["start_byte"].as_u64().unwrap() as usize;
            let end = link["end_byte"].as_u64().unwrap() as usize;
            let token = &text[start..end];
            assert!(token.starts_with("[[") && token.ends_with("]]"));
            for mapping in file["content_mappings"].as_array().unwrap() {
                let a = mapping["start_byte"].as_u64().unwrap() as usize;
                let b = mapping["end_byte"].as_u64().unwrap() as usize;
                assert!(end <= a || start >= b, "link aliases overlap factual body");
            }
            let target = scope_target(note, &link["scope"]);
            assert!(
                strings(&target["concept_ids"])
                    .contains(link["target_concept_id"].as_str().unwrap()),
                "link target belongs to actual body/row concepts"
            );
            assert!(target["links"].as_array().unwrap().iter().any(|v| {
                [
                    "target_concept_id",
                    "target_path",
                    "target_heading",
                    "display_text",
                ]
                .iter()
                .all(|key| v[*key] == link[*key])
            }));
            let target_note = notes
                .iter()
                .find(|n| n["output_path"] == link["target_path"])
                .unwrap();
            assert_eq!(link["target_heading"], target_note["heading"]);
            assert!(
                fs::read_to_string(f.preview.join(link["target_path"].as_str().unwrap()))
                    .unwrap()
                    .contains("## 知识明细")
            );
            let separator = if link["scope"]["kind"] == "paragraph" {
                '|'
            } else {
                assert!(token.contains("\\|"));
                '|'
            };
            let split = token.find(separator).unwrap();
            let mut destination = &token[2..split];
            if destination.ends_with('\\') {
                destination = &destination[..destination.len() - 1];
            }
            assert_eq!(
                destination,
                format!(
                    "{}#{}",
                    link["target_path"]
                        .as_str()
                        .unwrap()
                        .trim_end_matches(".md"),
                    link["target_heading"].as_str().unwrap()
                )
            );
            assert_eq!(
                decode_plain(&token[split + 1..token.len() - 2]),
                link["display_text"].as_str().unwrap()
            );
        }
    }
    assert_eq!(report["totals"]["facts"], facts.len());
    assert_eq!(
        report["totals"]["input_bundles"],
        author["inputs"].as_array().unwrap().len()
    );
    assert_eq!(report["totals"]["source_occurrences"], provenance.len());
    assert_eq!(
        report["totals"]["objects"],
        expected["objects"].as_array().unwrap().len()
    );
    assert_eq!(
        report["totals"]["properties"],
        expected["properties"].as_array().unwrap().len()
    );
    assert_eq!(report["totals"]["rendered_notes"], notes.len());
    assert_eq!(report["totals"]["content_mappings"], content_count);
    assert_eq!(report["totals"]["links"], link_count);
    assert_eq!(
        mapped_scopes, planned_scopes,
        "every substantive label/body has a saved byte range"
    );
    assert_eq!(
        link_count, planned_link_count,
        "every structured link has a saved token range"
    );
    assert_eq!(report["files"].as_array().unwrap().len(), notes.len());
    let table = expected["representation"] == "table";
    let object_count = expected["objects"].as_array().unwrap().len();
    let property_count = expected["properties"].as_array().unwrap().len();
    assert_eq!(
        report["totals"]["table_cells"],
        if table {
            object_count * (property_count + 1)
        } else {
            0
        }
    );
    assert_eq!(
        report["totals"]["missing_cells"],
        expected["cells"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|cell| cell["state"] == "missing")
            .count()
    );
    let mut hidden = BTreeMap::from([
        (
            ".selection-original.json".to_owned(),
            f.case.join("coordination.json"),
        ),
        (
            ".identity-original.json".to_owned(),
            f.case.join("identity.json"),
        ),
        (
            ".coordination-owner.json".to_owned(),
            f.output.join(".owner.json"),
        ),
        (
            ".coordination-report.json".to_owned(),
            f.output.join("coordination.json"),
        ),
        (
            ".coordination-report.md".to_owned(),
            f.output.join("coordination.md"),
        ),
        (
            ".coordination-ir.json".to_owned(),
            f.output.join("coordination-ir.json"),
        ),
    ]);
    for input in selection["inputs"].as_array().unwrap() {
        for name in FILES {
            hidden.insert(
                format!(".input-{}--{name}", input["input_id"].as_str().unwrap()),
                f.case
                    .join(input["review_bundle"].as_str().unwrap())
                    .join(name),
            );
        }
    }
    let input_names: BTreeSet<_> = report["inputs"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(input_names, hidden.keys().cloned().collect());
    for (name, original) in &hidden {
        assert_eq!(
            fs::read(f.preview.join(name)).unwrap(),
            fs::read(original).unwrap(),
            "hidden original {name}"
        );
    }
    let mut artifact_names = BTreeSet::from([".owner.json".to_owned(), "plan.json".to_owned()]);
    artifact_names.extend(input_names.iter().cloned());
    for note in notes {
        artifact_names.insert(note["output_path"].as_str().unwrap().to_owned());
    }
    assert_eq!(
        report["artifacts"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
        artifact_names
    );
    for (name, fingerprint) in report["inputs"]
        .as_object()
        .unwrap()
        .iter()
        .chain(report["artifacts"].as_object().unwrap())
    {
        assert_eq!(*fingerprint, pin(f.preview.join(name)));
    }
    let mut inventory = input_names;
    inventory.extend(artifact_names);
    inventory.extend(["presentation.json".to_owned(), "presentation.md".to_owned()]);
    let actual_inventory: BTreeSet<_> = fs::read_dir(&f.preview)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            entry.file_name().to_str().unwrap().to_owned()
        })
        .collect();
    assert_eq!(actual_inventory, inventory, "flat preview exact inventory");
    let markdown = fs::read_to_string(f.preview.join("presentation.md")).unwrap();
    assert!(markdown.contains("not_run") && markdown.contains("pending"));
}

// This catches a missing ordinary planner route after successful source review
// and coordination, rather than checking author-supplied rendered NotePlan.
#[test]
fn ordinary_present_chooses_table_after_source_review_and_successful_coordination() {
    let mut f = PresentationCase::prepare("two_objects");
    let coordinated = f.coordinate();
    assert_eq!(
        coordinated.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&coordinated.stderr)
    );
    assert!(f.output.join("coordination-ir.json").is_file());
    f.freeze_coordination();
    let presented = f.present();
    f.assert_protected();
    assert_eq!(
        presented.status.code(),
        Some(0),
        "ordinary knowledge present should generate a preview; root={} stderr={}",
        f.root.display(),
        String::from_utf8_lossy(&presented.stderr)
    );
    assert!(f.preview.join("plan.json").is_file());
    assert!(f.preview.join("知识比较.md").is_file());
    let plan: serde_json::Value =
        serde_json::from_slice(&fs::read(f.preview.join("plan.json")).unwrap()).unwrap();
    assert_eq!(plan["representation"], "table");
}

#[test]
fn every_frozen_author_case_obeys_table_paragraph_or_unresolved_preview_rules() {
    let fixture = repo().join("fixtures/active-presentation-v1");
    let authors: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.join("authors.json")).unwrap()).unwrap();
    let matrix = repo()
        .join("target/compiler-temp")
        .join(format!("presentation-matrix-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&matrix).unwrap();
    println!("presentation_matrix_root={}", matrix.display());
    let mut rows = vec![];
    let mut failures = vec![];
    for author in authors["cases"].as_array().unwrap() {
        let name = author["name"].as_str().unwrap();
        let expected: serde_json::Value = serde_json::from_slice(
            &fs::read(fixture.join(author["presentation_expected_file"].as_str().unwrap()))
                .unwrap(),
        )
        .unwrap();
        let mut prepared_root = None;
        // Retain evidence for every declared case, even while the new route is
        // absent. One observed failure must not suppress the remaining cases.
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut f = PresentationCase::prepare(name);
            prepared_root = Some(f.root.clone());
            let coordinated = f.coordinate();
            let ready = expected["expect_successful_preview"].as_bool().unwrap();
            assert_eq!(
                coordinated.status.code(),
                Some(if ready { 0 } else { 3 }),
                "source-backed coordination for {name}: {}",
                String::from_utf8_lossy(&coordinated.stderr)
            );
            let report: serde_json::Value =
                serde_json::from_slice(&fs::read(f.output.join("coordination.json")).unwrap())
                    .unwrap();
            assert_eq!(report["totals"], author["coordination_expected"]["totals"]);
            assert_eq!(report["knowledge_status"], "pending");
            assert_eq!(report["scientific_quality"], "not_run");
            assert_eq!(report["published"], false);
            assert_eq!(f.output.join("coordination-ir.json").is_file(), ready);
            f.freeze_coordination();
            let presented = f.present();
            f.assert_protected();
            if ready {
                assert_eq!(
                    presented.status.code(),
                    Some(0),
                    "present {name}: {}",
                    String::from_utf8_lossy(&presented.stderr)
                );
                let plan: serde_json::Value =
                    serde_json::from_slice(&fs::read(f.preview.join("plan.json")).unwrap())
                        .unwrap();
                assert_eq!(plan["representation"], expected["representation"]);
                assert!(f.preview.join("知识比较.md").is_file());
                assert_preview_semantics(&f, author, &expected);
            } else {
                assert_eq!(
                    presented.status.code(),
                    Some(3),
                    "unresolved {name} must remain blocked: {}",
                    String::from_utf8_lossy(&presented.stderr)
                );
                assert!(!f.preview.join("plan.json").exists());
                assert!(!f.preview.join("知识比较.md").exists());
            }
        }));
        let failure = checked.err().map(|p| {
            p.downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_else(|| "unexpected panic payload".to_owned())
        });
        if let Some(reason) = &failure {
            failures.push(format!("{name}: {reason}"));
        }
        rows.push(serde_json::json!({
            "name":name,
            "expected_representation":expected["representation"],
            "expected_successful_preview":expected["expect_successful_preview"],
            "prepared_root":prepared_root,
            "test_passed":failure.is_none(),
            "failure":failure
        }));
        fs::write(matrix.join("cases.json"), encode(&serde_json::json!(rows))).unwrap();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn legacy_source_reviews_coordinate_and_present_without_context_upgrade_or_lost_qualifiers() {
    let fixture = repo().join("fixtures/active-presentation-legacy-v1");
    let authors = read_json(fixture.join("authors.json"));
    let matrix = repo()
        .join("target/compiler-temp")
        .join(format!("presentation-legacy-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&matrix).unwrap();
    println!("presentation_legacy_root={}", matrix.display());
    let mut rows = vec![];
    let mut failures = vec![];
    for author in authors["cases"].as_array().unwrap() {
        let name = author["name"].as_str().unwrap();
        let version = author["coordination_expected"]["schema_version"]
            .as_u64()
            .unwrap();
        let expected =
            read_json(fixture.join(author["presentation_expected_file"].as_str().unwrap()));
        let mut prepared_root = None;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut f = PresentationCase::prepare_legacy(name);
            prepared_root = Some(f.root.clone());
            let run = f.coordinate();
            assert_eq!(
                run.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&run.stderr)
            );
            let report = read_json(f.output.join("coordination.json"));
            assert_eq!(report["schema_version"], version);
            assert_eq!(report["status"], "passed");
            if version == 1 {
                for (total, authored) in [
                    ("input_bundles", "expected_input_bundles"),
                    ("retained_fact_groups", "expected_fact_groups"),
                    ("canonical_facts", "expected_fact_groups"),
                    ("source_occurrences", "expected_source_occurrences"),
                    ("independent_sources", "expected_independent_sources"),
                    ("evidence_links", "expected_evidence_links"),
                    ("duplicate_occurrences", "expected_duplicate_occurrences"),
                ] {
                    assert_eq!(
                        report["totals"][total],
                        author["coordination_expected"][authored]
                    );
                }
                let ir = read_json(f.output.join("coordination-ir.json"));
                for fact in ir["facts"].as_array().unwrap() {
                    assert!(
                        !fact
                            .as_object()
                            .unwrap()
                            .contains_key("applicability_status")
                    );
                    assert!(!fact.as_object().unwrap().contains_key("time"));
                }
            } else {
                assert_eq!(report["totals"], author["coordination_expected"]["totals"]);
            }
            f.freeze_coordination();
            let presented = f.present();
            f.assert_protected();
            assert_eq!(
                presented.status.code(),
                Some(0),
                "{name}: {}",
                String::from_utf8_lossy(&presented.stderr)
            );
            assert_preview_semantics(&f, author, &expected);
            let plan = read_json(f.preview.join("plan.json"));
            let occurrence = plan["provenance"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["input_id"] == "a")
                .unwrap();
            assert!(
                occurrence["original_atom"]["atom"]["evidence_links"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|link| link["evidence_id"] == "a-exception-qualifies"
                        && link["relation"] == "qualifies")
            );
            assert!(occurrence["evidence"].as_array().unwrap().iter().any(
                |evidence| evidence["evidence_id"] == "a-exception-qualifies"
                    && evidence["quote"] == "结冰时不适用"
            ));
        }));
        let failure = result.err().map(|p| {
            p.downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_else(|| "unexpected panic payload".to_owned())
        });
        if let Some(reason) = &failure {
            failures.push(format!("{name}: {reason}"));
        }
        rows.push(json!({"name":name,"coordination_schema_version":version,"prepared_root":prepared_root,"passed":failure.is_none(),"failure":failure}));
        fs::write(matrix.join("cases.json"), encode(&json!(rows))).unwrap();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn cross_atom_concept_role_reuse_preserves_source_backed_paragraph() {
    let mut f = PresentationCase::prepare_package(
        "active-presentation-cross-role-v1",
        "cross_role_disjoint_paragraph",
    );
    let author = read_json(f.root.join("frozen-author.json"));
    let expected = read_json(f.root.join("expected.json"));
    let run = f.coordinate();
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report = read_json(f.output.join("coordination.json"));
    assert_eq!(report["totals"], author["coordination_expected"]["totals"]);
    assert!(f.output.join("coordination-ir.json").is_file());
    let object_ids: BTreeSet<_> = expected["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|label| label["object_id"].as_str().unwrap())
        .collect();
    let property_ids: BTreeSet<_> = expected["properties"]
        .as_array()
        .unwrap()
        .iter()
        .map(|label| label["property_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        object_ids.intersection(&property_ids).count(),
        1,
        "frozen legal reuse across independent atom roles"
    );
    f.freeze_coordination();
    let presented = f.present();
    f.assert_protected();
    assert_eq!(
        presented.status.code(),
        Some(0),
        "legal cross-atom role reuse must produce paragraph; root={} stderr={}",
        f.root.display(),
        String::from_utf8_lossy(&presented.stderr)
    );
    assert_preview_semantics(&f, &author, &expected);
}

#[test]
fn present_preserves_existing_output_and_rejects_input_descendant_targets() {
    let f = ready_case("two_objects");
    fs::create_dir(&f.preview).unwrap();
    let sentinel = f.preview.join("人工保留.txt");
    fs::write(&sentinel, "保留现有输出，禁止接管").unwrap();
    let run = f.present();
    f.assert_protected();
    assert_eq!(
        fs::read_to_string(&sentinel).unwrap(),
        "保留现有输出，禁止接管"
    );
    assert_eq!(fs::read_dir(&f.preview).unwrap().count(), 1);
    assert_cli_error(&run, 2, "EXTRACT_OUTPUT_EXISTS");
    for input in [&f.project, &f.case, &f.output] {
        let target = input.join("新的知识预览");
        let run = f.present_to(&target);
        f.assert_protected();
        assert!(
            !target.exists(),
            "input descendant created: {}",
            target.display()
        );
        assert_cli_error(&run, 2, "EXTRACT_OUTPUT_OVERLAP");
    }
}

#[test]
fn present_accepts_delimited_four_paths_and_new_unicode_vault_path() {
    let mut f = ready_case("two_objects");
    f.preview = f.root.join("新的知识 Vault 预览");
    let run = f.command(
        "present-delimited",
        &[
            "knowledge",
            "present",
            "--",
            f.project.to_str().unwrap(),
            f.case.to_str().unwrap(),
            f.output.to_str().unwrap(),
            f.preview.to_str().unwrap(),
        ],
    );
    f.assert_protected();
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_preview_semantics(
        &f,
        &read_json(f.root.join("frozen-author.json")),
        &read_json(f.root.join("expected.json")),
    );
}

#[test]
fn present_preserves_multiline_display_name_and_stable_detail_link() {
    let mut f = PresentationCase::prepare("two_objects");
    let label = "样例\nA\t（两行标签）";
    let mut identity = read_json(f.case.join("identity.json"));
    let object_id = identity["concepts"][0]["canonical_id"].clone();
    identity["concepts"][0]["display_name"] = json!(label);
    let mut expected = read_json(f.root.join("expected.json"));
    let object = expected["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["object_id"] == object_id)
        .unwrap();
    object["label"] = json!(label);
    // Freeze this explicit relabeling and its expected display before either
    // ordinary coordination or presentation runs; source/gold stay unchanged.
    fs::write(f.case.join("identity.json"), encode(&identity)).unwrap();
    fs::write(f.root.join("expected.json"), encode(&expected)).unwrap();
    fs::write(f.root.join("frozen-label-identity.json"), encode(&identity)).unwrap();
    f.freeze();
    let coordinated = f.coordinate();
    assert_eq!(coordinated.status.code(), Some(0));
    f.freeze_coordination();
    let presented = f.present();
    f.assert_protected();
    assert_eq!(
        presented.status.code(),
        Some(0),
        "valid coordinated multiline label rejected: {}",
        String::from_utf8_lossy(&presented.stderr)
    );
    assert_preview_semantics(&f, &read_json(f.root.join("frozen-author.json")), &expected);
    let path = format!("对象-{}.md", object_id.as_str().unwrap());
    let detail = fs::read_to_string(f.preview.join(path)).unwrap();
    assert!(detail.starts_with("# 样例<br>A&#9;（两行标签）\n\n## 知识明细\n"));
    let report = read_json(f.preview.join("presentation.json"));
    let links = report["files"][0]["links"].as_array().unwrap();
    assert!(links.iter().any(|link| {
        link["target_concept_id"] == object_id && link["target_heading"] == "知识明细"
    }));
}

#[cfg(unix)]
#[test]
fn present_rejects_symlink_ancestor_without_touching_external_content() {
    let f = ready_case("two_objects");
    let outside = f.root.join("外部保留目录");
    fs::create_dir(&outside).unwrap();
    let sentinel = outside.join("sentinel.txt");
    fs::write(&sentinel, "保留链接外的内容").unwrap();
    let alias = f.root.join("链接入口");
    std::os::unix::fs::symlink(&outside, &alias).unwrap();
    let run = f.present_to(&alias.join("新预览"));
    f.assert_protected();
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "保留链接外的内容");
    assert!(alias.is_symlink());
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
    assert_cli_error(&run, 2, "PROJECT_UNSAFE_PATH");
}

#[cfg(unix)]
#[test]
fn present_rejects_hardlinked_source_review_original_without_unlinking_alias() {
    let f = ready_case("two_objects");
    let original = f.case.join("bundle-a/gold-original.json");
    let bytes = fs::read(&original).unwrap();
    let alias = f.root.join("shared-gold-original.json");
    fs::hard_link(&original, &alias).unwrap();
    let run = f.present();
    f.assert_protected();
    assert_eq!(fs::read(&alias).unwrap(), bytes);
    assert!(!f.preview.exists());
    assert_cli_error(&run, 2, "COORD_INPUT_PATH");
}

#[test]
fn present_reopens_saved_coordination_and_original_source_review_materials() {
    let matrix = repo()
        .join("target/compiler-temp")
        .join(format!("presentation-tamper-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&matrix).unwrap();
    println!("presentation_tamper_root={}", matrix.display());
    let mut rows = vec![];
    let mut failures = vec![];
    for material in [
        "coordination_report",
        "coordination_ir",
        "coordination_markdown",
        "source_review_candidate",
    ] {
        let mut prepared_root = None;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut f = ready_case("two_objects");
            prepared_root = Some(f.root.clone());
            let path = match material {
                "coordination_report" => f.output.join("coordination.json"),
                "coordination_ir" => f.output.join("coordination-ir.json"),
                "coordination_markdown" => f.output.join("coordination.md"),
                _ => f.case.join("bundle-a/candidates-original.json"),
            };
            let original = fs::read(&path).unwrap();
            fs::write(
                f.root.join(format!("before-tamper-{material}.bin")),
                &original,
            )
            .unwrap();
            if material == "coordination_markdown" {
                let mut changed = original.clone();
                changed.extend_from_slice("\n未经来源核对的结论\n".as_bytes());
                fs::write(&path, changed).unwrap();
            } else {
                let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
                if material == "coordination_report" {
                    value["totals"]["source_occurrences"] = serde_json::json!(999);
                } else if material == "source_review_candidate" {
                    let content = value["payload"]["atoms"][0]["atom"]["content"]
                        .as_str()
                        .unwrap();
                    value["payload"]["atoms"][0]["atom"]["content"] =
                        serde_json::json!(content.replacen("10", "999", 1));
                    value["payload"]["atoms"][0]["measurement"]["value"] = serde_json::json!("999");
                } else {
                    value["facts"][0]["value"] = serde_json::json!("999");
                }
                fs::write(&path, encode(&value)).unwrap();
            }
            if material == "source_review_candidate" {
                // Refresh the test caller's pin so the rejection must reach
                // actual source-review reconstruction, rather than stop at a
                // stale caller fingerprint. Gold and source bytes stay frozen.
                let selection_path = f.case.join("coordination.json");
                fs::write(
                    f.root.join("before-tamper-selection.bin"),
                    fs::read(&selection_path).unwrap(),
                )
                .unwrap();
                let mut selection = read_json(&selection_path);
                let input = selection["inputs"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|input| input["input_id"] == "a")
                    .unwrap();
                input["artifacts"]["candidates-original.json"] = pin(&path);
                fs::write(selection_path, encode(&selection)).unwrap();
            }
            // Freeze the deliberately altered input, proving rejection neither
            // repairs it silently nor changes the other held original files.
            f.freeze();
            f.freeze_coordination();
            let changed = fs::read(&path).unwrap();
            let run = f.present();
            f.assert_protected();
            assert_eq!(fs::read(&path).unwrap(), changed);
            assert!(!f.preview.exists());
            assert_eq!(
                run.status.code(),
                Some(3),
                "{material}: {}",
                String::from_utf8_lossy(&run.stderr)
            );
            let family = if material == "source_review_candidate" {
                "COORD_SOURCE_REVIEW"
            } else {
                "PRESENT_COORDINATION_MISMATCH"
            };
            assert_cli_error(&run, 3, family);
        }));
        let failure = result.err().map(|p| {
            p.downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_else(|| "unexpected panic payload".to_owned())
        });
        if let Some(reason) = &failure {
            failures.push(format!("{material}: {reason}"));
        }
        rows.push(serde_json::json!({"material":material,"prepared_root":prepared_root,"passed":failure.is_none(),"failure":failure}));
        fs::write(matrix.join("cases.json"), encode(&serde_json::json!(rows))).unwrap();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
