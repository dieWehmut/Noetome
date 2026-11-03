//! Strict structured-output schema and the exact request body used for reservation.
use crate::extraction_api_config::RequestStrategy;
use crate::extraction_model::{RequestUnit, SourceBinding};
use crate::project::ProjectError;
use serde_json::{Value, json};
pub(crate) fn request_body(
    strategy: RequestStrategy,
    model: &str,
    request_id: &str,
    binding: &SourceBinding,
    units: &[RequestUnit],
    max_output_tokens: u64,
) -> Result<Vec<u8>, ProjectError> {
    let evidence_anchors: Vec<Value> = units
        .iter()
        .map(|unit| {
            json!({
                "unit_id":unit.unit_id,"excerpt_start_byte":unit.start_byte,"excerpt_end_byte":unit.end_byte,
                "excerpt_sha256":crate::text::sha256(unit.text.as_bytes()),"quote":unit.text,
            })
        })
        .collect();
    let request = json!({"request_id":request_id,"binding":binding,"units":units,"evidence_anchors":evidence_anchors});
    let instructions = "Extract only evidence-supported concepts and atomic claims from the selected text units. Return one JSON object matching the supplied strict schema, copying request_id and binding exactly. Full-unit evidence_anchors provide exact UTF8 byte offsets in the referenced source resource, quote and SHA256. Copy the resource-absolute start/end offsets from the supplied anchors; preserve the quote and its SHA256. Preserve conditions, exceptions and units. Every atom must have required=false and verification=unverified. Use null for unknown measurement/value/unit and explicit value_status. Do not invent scientific verification or producer provenance.";
    let schema = candidate_schema();
    let input_value = match strategy {
        RequestStrategy::OpenAiStrict => request,
        RequestStrategy::TangmaoDeepseek => json!({
            "task":format!("{instructions} Return exactly one plain JSON object; no Markdown, code fences, explanations or additional text."),
            "response_schema":schema,"request":request,
        }),
    };
    let input = serde_json::to_string(&input_value).map_err(|_| request_error())?;
    let mut body = json!({
        "model":model,"instructions":instructions,
        "input":input,"text":{"format":{"type":"json_schema","name":"noetome_candidates_v1","schema":schema}},
        "max_output_tokens":max_output_tokens,"store":false,"stream":false,
        "tools":[],"tool_choice":"none",
    });
    match strategy {
        RequestStrategy::OpenAiStrict => {
            body["text"]["format"]["strict"] = json!(true);
            body["background"] = json!(false);
            body["service_tier"] = json!("default");
            body["truncation"] = json!("disabled");
        }
        RequestStrategy::TangmaoDeepseek => {
            body["reasoning"] = json!({"effort":"none"});
        }
    }
    let bytes = serde_json::to_vec(&body).map_err(|_| request_error())?;
    if bytes.len() > 256 * 1024 {
        return Err(request_error());
    }
    Ok(bytes)
}
fn request_error() -> ProjectError {
    ProjectError::new(
        "EXTRACT_API_REQUEST",
        3,
        "文本 API 请求正文无法序列化或超过 256 KiB。",
    )
}
fn object(properties: Value) -> Value {
    let required: Vec<String> = properties
        .as_object()
        .expect("schema properties")
        .keys()
        .cloned()
        .collect();
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn array(items: Value) -> Value {
    json!({"type":"array","items":items})
}
fn nullable(value: Value) -> Value {
    json!({"anyOf":[value,{"type":"null"}]})
}
fn candidate_schema() -> Value {
    let string = json!({"type":"string"});
    let integer = json!({"type":"integer"});
    let boolean = json!({"type":"boolean"});
    let strings = array(string.clone());
    let optional_string = nullable(string.clone());
    let value_status = json!({"type":"string","enum":["known","unknown","not_applicable"]});
    let binding = object(
        json!({"project_id":string,"import_id":string,"source_id":string,"version_id":string,"content_sha256":string,"normalized_sha256":optional_string,"parse_id":optional_string,"manifest_sha256":optional_string}),
    );
    let concept = object(
        json!({"concept_id":string,"revision":integer,"canonical_name":string,"aliases":strings,"needs_full_explanation":boolean}),
    );
    let evidence = object(
        json!({"evidence_id":string,"unit_id":string,"excerpt_start_byte":integer,"excerpt_end_byte":integer,"excerpt_sha256":string,"quote":string}),
    );
    let link = object(
        json!({"evidence_id":string,"relation":{"type":"string","enum":["supports","refutes","qualifies"]}}),
    );
    let atom = object(
        json!({"atom_id":string,"revision":integer,"kind":{"type":"string","enum":["definition","statement","comparison_value"]},"concept_ids":strings,"content":string,"qualifiers":strings,"evidence_links":array(link),"required":{"type":"boolean","enum":[false]},"verification":{"type":"string","enum":["unverified"]},"value_status":value_status}),
    );
    let measurement = object(
        json!({"value_status":value_status,"value":optional_string,"unit":optional_string,"conditions":strings}),
    );
    let candidate_atom = object(json!({"atom":atom,"measurement":nullable(measurement)}));
    object(
        json!({"schema_version":{"type":"integer","enum":[1]},"request_id":string,"binding":binding,"concepts":array(concept),"evidence":array(evidence),"atoms":array(candidate_atom)}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction_model::Locator;
    #[test]
    fn inline_fix_selected_input_contains_task_schema_and_original_absolute_request() {
        let binding: SourceBinding = serde_json::from_value(json!({"project_id":"p","import_id":"i","source_id":"s","version_id":"v","content_sha256":"sha","normalized_sha256":null,"parse_id":null,"manifest_sha256":null})).unwrap();
        let units = [RequestUnit {
            unit_id: "u".into(),
            locator: Locator::Text {
                span_id: "s".into(),
                ordinal: 1,
                resource_sha256: "resource".into(),
            },
            source_status: "parsed".into(),
            start_byte: 123,
            end_byte: 129,
            text: "中文".into(),
        }];
        let selected: Value = serde_json::from_slice(
            &request_body(
                RequestStrategy::TangmaoDeepseek,
                "deepseek-flash",
                "req",
                &binding,
                &units,
                8192,
            )
            .unwrap(),
        )
        .unwrap();
        let official: Value = serde_json::from_slice(
            &request_body(
                RequestStrategy::OpenAiStrict,
                "deepseek-flash",
                "req",
                &binding,
                &units,
                8192,
            )
            .unwrap(),
        )
        .unwrap();
        let keys: Vec<&str> = selected
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "input",
                "instructions",
                "max_output_tokens",
                "model",
                "reasoning",
                "store",
                "stream",
                "text",
                "tool_choice",
                "tools"
            ]
        );
        assert_eq!(selected["model"], "deepseek-flash");
        assert_eq!(selected["reasoning"], json!({"effort":"none"}));
        assert_eq!(selected["max_output_tokens"], 8192);
        assert_eq!(selected["store"], false);
        assert_eq!(selected["stream"], false);
        assert_eq!(selected["tools"], json!([]));
        assert_eq!(selected["tool_choice"], "none");
        let format = &selected["text"]["format"];
        assert_eq!(format["type"], "json_schema");
        assert_eq!(format["name"], "noetome_candidates_v1");
        assert!(format.get("strict").is_none());
        assert_eq!(format["schema"]["additionalProperties"], false);
        assert_eq!(format["schema"], official["text"]["format"]["schema"]);
        assert_eq!(selected["instructions"], official["instructions"]);
        let input: Value = serde_json::from_str(selected["input"].as_str().unwrap()).unwrap();
        assert_eq!(
            input
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["request", "response_schema", "task"]
        );
        assert!(
            input["task"]
                .as_str()
                .unwrap()
                .starts_with(selected["instructions"].as_str().unwrap())
        );
        assert!(
            input["task"]
                .as_str()
                .unwrap()
                .contains("one plain JSON object")
        );
        assert!(input["task"].as_str().unwrap().contains("no Markdown"));
        assert_eq!(input["response_schema"], format["schema"]);
        assert_eq!(
            input["request"],
            serde_json::from_str::<Value>(official["input"].as_str().unwrap()).unwrap()
        );
        let request = &input["request"];
        assert_eq!(request["request_id"], "req");
        assert_eq!(request["binding"], serde_json::to_value(binding).unwrap());
        assert_eq!(
            request["evidence_anchors"][0],
            json!({"unit_id":"u","excerpt_start_byte":123,"excerpt_end_byte":129,"excerpt_sha256":"72726d8818f693066ceb69afa364218b692e62ea92b385782363780f47529c21","quote":"中文"})
        );
        // A model name alone never changes the official endpoint's strategy.
        assert_eq!(official["text"]["format"]["strict"], true);
        assert_eq!(official["background"], false);
        assert_eq!(official["service_tier"], "default");
        assert_eq!(official["truncation"], "disabled");
        assert!(official.get("reasoning").is_none());
    }
    #[test]
    fn request_has_explicit_policy_strict_nullable_schema_and_full_utf8_anchor() {
        let binding:SourceBinding=serde_json::from_value(json!({"project_id":"p","import_id":"i","source_id":"s","version_id":"v","content_sha256":"sha","normalized_sha256":null,"parse_id":null,"manifest_sha256":null})).unwrap();
        let unit = RequestUnit {
            unit_id: "u".into(),
            locator: Locator::Text {
                span_id: "s".into(),
                ordinal: 1,
                resource_sha256: crate::text::sha256(format!("{}中文", " ".repeat(123)).as_bytes()),
            },
            source_status: "parsed".into(),
            start_byte: 123,
            end_byte: 129,
            text: "中文".into(),
        };
        let bytes = request_body(
            RequestStrategy::OpenAiStrict,
            "model",
            "req",
            &binding,
            &[unit],
            2048,
        )
        .unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        for field in ["store", "stream", "background"] {
            assert_eq!(body[field], false);
        }
        assert_eq!(body["tools"], json!([]));
        assert_eq!(body["tool_choice"], "none");
        assert_eq!(body["service_tier"], "default");
        assert_eq!(body["truncation"], "disabled");
        assert_eq!(body["max_output_tokens"], 2048);
        let input: Value = serde_json::from_str(body["input"].as_str().unwrap()).unwrap();
        assert_eq!(input["request_id"], "req");
        assert_eq!(input["binding"], serde_json::to_value(binding).unwrap());
        let anchor = &input["evidence_anchors"][0];
        assert_eq!(anchor["excerpt_start_byte"], 123);
        assert_eq!(anchor["excerpt_end_byte"], 129);
        assert_eq!(anchor["quote"], "中文");
        assert_eq!(
            anchor["excerpt_sha256"],
            crate::text::sha256("中文".as_bytes())
        );
        fn strict(v: &Value) {
            if v["type"] == "object" {
                let properties = v["properties"].as_object().unwrap();
                assert_eq!(v["additionalProperties"], false);
                let required = v["required"].as_array().unwrap();
                assert_eq!(properties.len(), required.len());
                for key in properties.keys() {
                    assert!(required.contains(&json!(key)));
                }
            }
            if let Some(map) = v.as_object() {
                for value in map.values() {
                    strict(value);
                }
            }
            if let Some(array) = v.as_array() {
                for value in array {
                    strict(value);
                }
            }
        }
        let schema = &body["text"]["format"]["schema"];
        strict(schema);
        assert!(schema["properties"].get("producer").is_none());
        assert_eq!(
            schema["properties"]["atoms"]["items"]["properties"]["measurement"]["anyOf"][1]["type"],
            "null"
        );
    }
}
