//! Strict structured-output schema and the exact request body used for reservation.
use crate::extraction_model::{RequestUnit, SourceBinding};
use crate::project::ProjectError;
use serde_json::{Value, json};
pub(crate) fn request_body(
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
    let input = serde_json::to_string(&json!({"request_id":request_id,"binding":binding,"units":units,"evidence_anchors":evidence_anchors}))
        .map_err(|_| request_error())?;
    let body = json!({
        "model":model,"instructions":"Extract only evidence-supported concepts and atomic claims from the selected text units. Return one JSON object matching the supplied strict schema, copying request_id and binding exactly. Full-unit evidence_anchors provide exact UTF8 byte offsets in the referenced source resource, quote and SHA256. Copy the resource-absolute start/end offsets from the supplied anchors; preserve the quote and its SHA256. Preserve conditions, exceptions and units. Every atom must have required=false and verification=unverified. Use null for unknown measurement/value/unit and explicit value_status. Do not invent scientific verification or producer provenance.",
        "input":input,"text":{"format":{"type":"json_schema","name":"noetome_candidates_v1","strict":true,"schema":candidate_schema()}},
        "max_output_tokens":max_output_tokens,"store":false,"stream":false,"background":false,
        "tools":[],"tool_choice":"none","service_tier":"default","truncation":"disabled",
    });
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
        let bytes = request_body("model", "req", &binding, &[unit], 2048).unwrap();
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
