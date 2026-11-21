//! Responses envelope validation and strict candidate DTOs.
use crate::extraction_api_config::RequestStrategy;
use crate::extraction_model::{CandidateAtom, CandidateEvidence, SourceBinding};
use crate::fixture_model::Concept;
use crate::project::ProjectError;
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
mod chat;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApiCandidates {
    #[serde(deserialize_with = "schema_one")]
    pub(crate) schema_version: u32,
    pub(crate) request_id: String,
    pub(crate) binding: SourceBinding,
    pub(crate) concepts: Vec<Concept>,
    pub(crate) evidence: Vec<CandidateEvidence>,
    pub(crate) atoms: Vec<CandidateAtom>,
}
#[derive(Debug)]
pub(crate) struct ParsedResponse {
    pub(crate) id: String,
    pub(crate) model: String,
    pub(crate) usage: Value,
    pub(crate) service_tier: Option<String>,
    pub(crate) candidates: ApiCandidates,
}
#[derive(Debug)]
pub(crate) struct ResponseMetadata {
    pub(crate) id: String,
    pub(crate) model: String,
    pub(crate) usage: Value,
    pub(crate) service_tier: Option<String>,
    pub(crate) raw_usage: Option<Value>,
}
pub(crate) fn response_metadata_with_strategy(
    body: &[u8],
    strategy: RequestStrategy,
) -> Result<ResponseMetadata, ProjectError> {
    if matches!(strategy, RequestStrategy::TangmaoChat) {
        chat::response_metadata(body)
    } else {
        response_metadata(body)
    }
}
pub(crate) fn response_metadata(body: &[u8]) -> Result<ResponseMetadata, ProjectError> {
    // Parse all nesting levels strictly without allowing output/status failures
    // to discard independently valid model and usage facts.
    let value = strict_json(body).map_err(|_| error("EXTRACT_API_RESPONSE"))?;
    metadata(&value)
}
pub(crate) fn chat_candidate_value(body: &[u8]) -> Result<Value, ProjectError> {
    chat::candidate_value(body).map(|(_, candidate)| candidate)
}

#[cfg(test)]
pub(crate) fn decode(body: &[u8]) -> Result<ParsedResponse, ProjectError> {
    decode_with_strategy(body, RequestStrategy::OpenAiStrict)
}
pub(crate) fn decode_with_strategy(
    body: &[u8],
    strategy: RequestStrategy,
) -> Result<ParsedResponse, ProjectError> {
    if matches!(strategy, RequestStrategy::TangmaoChat) {
        return chat::decode(body);
    }
    let value = envelope(body, strategy)?;
    let status = value["status"]
        .as_str()
        .ok_or_else(|| error("EXTRACT_API_RESPONSE"))?;
    if status == "incomplete" || !value["incomplete_details"].is_null() {
        return Err(error("EXTRACT_API_INCOMPLETE"));
    }
    if status != "completed" || !value["error"].is_null() {
        return Err(error("EXTRACT_API_STATUS"));
    }
    let mut text = None;
    let output = value["output"]
        .as_array()
        .ok_or_else(|| error("EXTRACT_API_OUTPUT"))?;
    for item in output {
        match item["type"].as_str() {
            Some("reasoning") => {
                if item["status"] == "incomplete" {
                    return Err(error("EXTRACT_API_INCOMPLETE"));
                }
                if item.get("status").is_some_and(|v| v != "completed") {
                    return Err(error("EXTRACT_API_OUTPUT"));
                }
                for (field, kind) in [("summary", "summary_text"), ("content", "reasoning_text")] {
                    if let Some(parts) = item.get(field).and_then(Value::as_array) {
                        if parts
                            .iter()
                            .any(|part| part["type"] != kind || !part["text"].is_string())
                        {
                            return Err(error("EXTRACT_API_OUTPUT"));
                        }
                    }
                }
            }
            Some("message") => {
                if text.is_some() || item["role"] != "assistant" || item["status"] != "completed" {
                    return Err(error("EXTRACT_API_OUTPUT"));
                }
                let contents = item["content"]
                    .as_array()
                    .ok_or_else(|| error("EXTRACT_API_OUTPUT"))?;
                if contents.iter().any(|part| part["type"] == "refusal") {
                    return Err(error("EXTRACT_API_REFUSAL"));
                }
                if contents.len() != 1 || contents[0]["type"] != "output_text" {
                    return Err(error("EXTRACT_API_OUTPUT"));
                }
                text = contents[0]["text"].as_str();
                if text.is_none_or(|v| v.trim().is_empty()) {
                    return Err(error("EXTRACT_API_OUTPUT"));
                }
            }
            _ => return Err(error("EXTRACT_API_OUTPUT")),
        }
    }
    let metadata = metadata(&value)?;
    let candidate_value = strict_json(text.ok_or_else(|| error("EXTRACT_API_OUTPUT"))?.as_bytes())
        .map_err(|_| error("EXTRACT_API_CANDIDATES"))?;
    let candidates: ApiCandidates =
        serde_json::from_value(candidate_value).map_err(|_| error("EXTRACT_API_CANDIDATES"))?;
    Ok(ParsedResponse {
        id: metadata.id,
        model: metadata.model,
        usage: metadata.usage,
        service_tier: metadata.service_tier,
        candidates,
    })
}
fn schema_one<'de, D: serde::Deserializer<'de>>(de: D) -> Result<u32, D::Error> {
    let value = u32::deserialize(de)?;
    if value != 1 {
        return Err(serde::de::Error::custom("unknown candidate schema"));
    }
    Ok(value)
}
fn error(code: &'static str) -> ProjectError {
    ProjectError::new(code, 3, "文本 API 响应未通过严格契约校验。")
}

// Decode into Value without serde_json's usual last-key-wins behavior, including
// unknown future metadata. Candidate JSON embedded in output_text uses this too.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        de.deserialize_any(UniqueVisitor)
    }
}
struct UniqueVisitor;
impl<'de> Visitor<'de> for UniqueVisitor {
    type Value = Unique;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("JSON with unique object keys")
    }
    fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Unique, E> {
        Ok(Unique(Value::Bool(v)))
    }
    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Unique, E> {
        Ok(Unique(Value::Number(v.into())))
    }
    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Unique, E> {
        Ok(Unique(Value::Number(v.into())))
    }
    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Unique, E> {
        serde_json::Number::from_f64(v)
            .map(|n| Unique(Value::Number(n)))
            .ok_or_else(|| E::custom("nonfinite JSON number"))
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Unique, E> {
        Ok(Unique(Value::String(v.into())))
    }
    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Unique, E> {
        Ok(Unique(Value::String(v)))
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Unique, E> {
        Ok(Unique(Value::Null))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Unique, A::Error> {
        let mut array = Vec::new();
        while let Some(Unique(value)) = access.next_element()? {
            array.push(value);
        }
        Ok(Unique(Value::Array(array)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Unique, A::Error> {
        let mut map = serde_json::Map::new();
        while let Some(key) = access.next_key::<String>()? {
            if map.contains_key(&key) {
                return Err(serde::de::Error::custom("duplicate JSON key"));
            }
            let Unique(value) = access.next_value()?;
            map.insert(key, value);
        }
        Ok(Unique(Value::Object(map)))
    }
}
fn strict_json(bytes: &[u8]) -> Result<Value, serde_json::Error> {
    serde_json::from_slice::<Unique>(bytes).map(|v| v.0)
}
fn envelope(body: &[u8], strategy: RequestStrategy) -> Result<Value, ProjectError> {
    let value = strict_json(body).map_err(|_| error("EXTRACT_API_RESPONSE"))?;
    if !value.is_object() {
        return Err(error("EXTRACT_API_RESPONSE"));
    }
    for field in ["id", "model", "status"] {
        if !value[field].is_string() {
            return Err(error("EXTRACT_API_RESPONSE"));
        }
    }
    for field in ["error", "incomplete_details"] {
        let Some(v) = value.get(field) else {
            if matches!(strategy, RequestStrategy::TangmaoDeepseek)
                && value["status"] == "completed"
            {
                continue;
            }
            return Err(error("EXTRACT_API_RESPONSE"));
        };
        if !v.is_null() && !v.is_object() {
            return Err(error("EXTRACT_API_RESPONSE"));
        }
    }
    optional_types(
        &value,
        &["object"],
        &["created_at"],
        &["store", "stream", "background"],
        &[],
    )?;
    optional_nullable_strings(&value, &["service_tier"])?;
    optional_types(&value["error"], &["code", "message", "type"], &[], &[], &[])?;
    optional_nullable_strings(&value["error"], &["param"])?;
    optional_types(&value["incomplete_details"], &["reason"], &[], &[], &[])?;
    let output = value["output"]
        .as_array()
        .ok_or_else(|| error("EXTRACT_API_RESPONSE"))?;
    for item in output {
        if !item.is_object() || !item["type"].is_string() {
            return Err(error("EXTRACT_API_RESPONSE"));
        }
        optional_types(
            item,
            &["id", "role", "status"],
            &[],
            &[],
            &["content", "summary"],
        )?;
        optional_nullable_strings(item, &["encrypted_content"])?;
        for field in ["content", "summary"] {
            if let Some(parts) = item.get(field).and_then(Value::as_array) {
                for part in parts {
                    if !part.is_object() || !part["type"].is_string() {
                        return Err(error("EXTRACT_API_RESPONSE"));
                    }
                    optional_types(
                        part,
                        &["text", "refusal"],
                        &[],
                        &[],
                        &["annotations", "logprobs"],
                    )?;
                    if part["type"] == "output_text" && !part["text"].is_string() {
                        return Err(error("EXTRACT_API_RESPONSE"));
                    }
                    if part["type"] == "refusal" && !part["refusal"].is_string() {
                        return Err(error("EXTRACT_API_RESPONSE"));
                    }
                }
            }
        }
    }
    Ok(value)
}
fn optional_types(
    value: &Value,
    strings: &[&str],
    integers: &[&str],
    booleans: &[&str],
    arrays: &[&str],
) -> Result<(), ProjectError> {
    for key in strings {
        if let Some(v) = value.get(*key) {
            if !v.is_string() {
                return Err(error("EXTRACT_API_RESPONSE"));
            }
        }
    }
    for key in integers {
        if let Some(v) = value.get(*key) {
            if v.as_u64().is_none() {
                return Err(error("EXTRACT_API_RESPONSE"));
            }
        }
    }
    for key in booleans {
        if let Some(v) = value.get(*key) {
            if !v.is_boolean() {
                return Err(error("EXTRACT_API_RESPONSE"));
            }
        }
    }
    for key in arrays {
        if let Some(v) = value.get(*key) {
            if !v.is_array() {
                return Err(error("EXTRACT_API_RESPONSE"));
            }
        }
    }
    Ok(())
}
fn optional_nullable_strings(value: &Value, keys: &[&str]) -> Result<(), ProjectError> {
    for key in keys {
        if let Some(v) = value.get(*key) {
            if !v.is_null() && !v.is_string() {
                return Err(error("EXTRACT_API_RESPONSE"));
            }
        }
    }
    Ok(())
}
fn metadata(value: &Value) -> Result<ResponseMetadata, ProjectError> {
    if !value.is_object() {
        return Err(error("EXTRACT_API_RESPONSE"));
    }
    optional_nullable_strings(value, &["service_tier"])?;
    let usage = value
        .get("usage")
        .filter(|v| v.is_object())
        .ok_or_else(|| error("EXTRACT_API_USAGE"))?;
    let tokens = |field: &str| {
        usage[field]
            .as_u64()
            .ok_or_else(|| error("EXTRACT_API_USAGE"))
    };
    let input = tokens("input_tokens")?;
    let output = tokens("output_tokens")?;
    let total = tokens("total_tokens")?;
    if input.checked_add(output) != Some(total) {
        return Err(error("EXTRACT_API_USAGE"));
    }
    for (field, parent, known) in [
        (
            "input_tokens_details",
            input,
            &["cached_tokens", "cache_write_tokens"][..],
        ),
        ("output_tokens_details", output, &["reasoning_tokens"][..]),
    ] {
        if let Some(details) = usage.get(field) {
            if !details.is_object() {
                return Err(error("EXTRACT_API_USAGE"));
            }
            for counter in known {
                if let Some(v) = details.get(*counter) {
                    if v.as_u64().is_none_or(|n| n > parent) {
                        return Err(error("EXTRACT_API_USAGE"));
                    }
                }
            }
        }
    }
    let id = value["id"]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| error("EXTRACT_API_RESPONSE"))?
        .to_owned();
    let model = value["model"]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| error("EXTRACT_API_RESPONSE"))?
        .to_owned();
    Ok(ResponseMetadata {
        id,
        model,
        usage: usage.clone(),
        raw_usage: None,
        service_tier: value
            .get("service_tier")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn chat_fix_valid_single_chat_candidate_requires_chat_decoder() {
        let value = json!({"id":"chat","model":"returned","object":"chat.completion","created":1,
            "choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":candidate().to_string(),"reasoning_content":null}}],
            "usage":{"prompt_tokens":12,"completion_tokens":8,"total_tokens":20}});
        let parsed =
            super::decode_with_strategy(value.to_string().as_bytes(), RequestStrategy::TangmaoChat)
                .unwrap();
        assert_eq!(parsed.candidates.request_id, "req");
        assert_eq!(parsed.usage["input_tokens"], 12);
        for strategy in [
            RequestStrategy::OpenAiStrict,
            RequestStrategy::TangmaoDeepseek,
        ] {
            assert!(super::decode_with_strategy(value.to_string().as_bytes(), strategy).is_err());
        }
    }
    fn selected_decode(body: &[u8]) -> Result<ParsedResponse, ProjectError> {
        super::decode_with_strategy(body, RequestStrategy::TangmaoDeepseek)
    }
    fn sparse_response() -> Value {
        let mut v = response();
        v.as_object_mut().unwrap().remove("error");
        v.as_object_mut().unwrap().remove("incomplete_details");
        v
    }
    #[test]
    fn inline_fix_selected_sparse_completed_accepts_candidates_but_default_stays_strict() {
        let bytes = sparse_response().to_string().into_bytes();
        let parsed = selected_decode(&bytes).unwrap();
        assert_eq!(parsed.id, "resp");
        assert_eq!(parsed.usage["total_tokens"], 20);
        assert_eq!(parsed.candidates.request_id, "req");
        assert_eq!(decode(&bytes).unwrap_err().code(), "EXTRACT_API_RESPONSE");
        let mut full = response();
        full["model"] = json!("deepseek-flash");
        // Model aliases cannot opt official/local wire into sparse tolerance.
        full.as_object_mut().unwrap().remove("error");
        assert_eq!(
            decode(full.to_string().as_bytes()).unwrap_err().code(),
            "EXTRACT_API_RESPONSE"
        );
    }
    #[test]
    fn inline_fix_first_actual_plaintext_is_candidate_failure_with_real_usage_preserved() {
        let bytes =
            include_bytes!("../../../doc/validation/assets/B04-T02-live-first-response.bin");
        let metadata = response_metadata(bytes).unwrap();
        assert_eq!(metadata.model, "deepseek-flash");
        assert_eq!(metadata.usage["input_tokens"], 551);
        assert_eq!(metadata.usage["output_tokens"], 385);
        assert_eq!(metadata.usage["total_tokens"], 936);
        assert_eq!(
            selected_decode(bytes).unwrap_err().code(),
            "EXTRACT_API_CANDIDATES"
        );
        assert_eq!(decode(bytes).unwrap_err().code(), "EXTRACT_API_RESPONSE");
    }
    #[test]
    fn inline_fix_selected_sparse_status_errors_types_and_usage_remain_strict() {
        let mut missing_status = sparse_response();
        missing_status.as_object_mut().unwrap().remove("status");
        assert!(selected_decode(missing_status.to_string().as_bytes()).is_err());
        for (field, bad, expected) in [
            ("status", Value::Null, "EXTRACT_API_RESPONSE"),
            ("status", json!(true), "EXTRACT_API_RESPONSE"),
            ("status", json!("incomplete"), "EXTRACT_API_RESPONSE"),
            ("status", json!("failed"), "EXTRACT_API_RESPONSE"),
            ("status", json!("unknown"), "EXTRACT_API_RESPONSE"),
            (
                "error",
                json!({"code":"bad","message":"failed"}),
                "EXTRACT_API_STATUS",
            ),
            ("error", json!({"code":null}), "EXTRACT_API_RESPONSE"),
            ("error", json!(false), "EXTRACT_API_RESPONSE"),
            ("error", json!([]), "EXTRACT_API_RESPONSE"),
            (
                "incomplete_details",
                json!({"reason":"max_output_tokens"}),
                "EXTRACT_API_INCOMPLETE",
            ),
            (
                "incomplete_details",
                json!({"reason":7}),
                "EXTRACT_API_RESPONSE",
            ),
            ("incomplete_details", json!("none"), "EXTRACT_API_RESPONSE"),
            ("usage", Value::Null, "EXTRACT_API_USAGE"),
            (
                "usage",
                json!({"input_tokens":12,"output_tokens":8,"total_tokens":21}),
                "EXTRACT_API_USAGE",
            ),
            ("model", json!(7), "EXTRACT_API_RESPONSE"),
        ] {
            let mut value = sparse_response();
            value[field] = bad;
            assert_eq!(
                selected_decode(value.to_string().as_bytes())
                    .unwrap_err()
                    .code(),
                expected,
                "{field}"
            );
        }
        for omitted in ["error", "incomplete_details"] {
            let mut value = response();
            value.as_object_mut().unwrap().remove(omitted);
            assert!(selected_decode(value.to_string().as_bytes()).is_ok());
            assert_eq!(
                decode(value.to_string().as_bytes()).unwrap_err().code(),
                "EXTRACT_API_RESPONSE"
            );
        }
        assert!(selected_decode(response().to_string().as_bytes()).is_ok());
        let mut incomplete = response();
        incomplete["status"] = json!("incomplete");
        assert_eq!(
            selected_decode(incomplete.to_string().as_bytes())
                .unwrap_err()
                .code(),
            "EXTRACT_API_INCOMPLETE"
        );
    }
    #[test]
    fn inline_fix_selected_sparse_duplicates_tools_extra_text_and_fences_stay_rejected() {
        let good = sparse_response().to_string();
        for bad in [
            good.replacen("{", "{\"future\":{\"x\":1,\"x\":2},", 1),
            good.replace(
                "\"total_tokens\":20",
                "\"total_tokens\":20,\"total_tokens\":20",
            ),
            good.replace(
                "\"status\":\"completed\"",
                "\"status\":\"completed\",\"status\":\"completed\"",
            ),
            format!("{good} {{}}"),
        ] {
            assert_eq!(
                selected_decode(bad.as_bytes()).unwrap_err().code(),
                "EXTRACT_API_RESPONSE"
            );
        }
        for item in [
            json!({"type":"function_call"}),
            response()["output"][1].clone(),
        ] {
            let mut v = sparse_response();
            v["output"].as_array_mut().unwrap().push(item);
            assert_eq!(
                selected_decode(v.to_string().as_bytes())
                    .unwrap_err()
                    .code(),
                "EXTRACT_API_OUTPUT"
            );
        }
        for content in [
            json!([{"type":"output_text","text":"{}"},{"type":"output_text","text":"extra"}]),
            json!([{"type":"refusal","refusal":"no"}]),
        ] {
            let mut v = sparse_response();
            v["output"][1]["content"] = content;
            assert!(selected_decode(v.to_string().as_bytes()).is_err());
        }
        for text in [
            "Please give me a task".into(),
            format!("```json\n{}\n```", candidate()),
            candidate()
                .to_string()
                .replacen("{", "{\"future\":{\"x\":1,\"x\":2},", 1),
        ] {
            let mut v = sparse_response();
            v["output"][1]["content"][0]["text"] = json!(text);
            assert_eq!(
                selected_decode(v.to_string().as_bytes())
                    .unwrap_err()
                    .code(),
                "EXTRACT_API_CANDIDATES"
            );
            assert_eq!(
                response_metadata(v.to_string().as_bytes()).unwrap().usage["total_tokens"],
                20
            );
        }
    }
    fn candidate() -> Value {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../../fixtures/extraction-gateway-v1/response.json"
        ))
        .unwrap();
        value.as_object_mut().unwrap().remove("producer");
        value["request_id"] = json!("req");
        value["binding"] = json!({"project_id":"p","import_id":"i","source_id":"s","version_id":"v","content_sha256":"sha","normalized_sha256":null,"parse_id":null,"manifest_sha256":null});
        value
    }
    fn response() -> Value {
        json!({"id":"resp","object":"response","model":"returned-model","status":"completed","error":null,"incomplete_details":null,
            "output":[{"type":"reasoning","id":"r","summary":[]},{"type":"message","role":"assistant","status":"completed","id":"m","content":[{"type":"output_text","text":candidate().to_string(),"annotations":[],"logprobs":[]}]}],
            "usage":{"input_tokens":12,"output_tokens":8,"total_tokens":20}})
    }
    fn code(v: &Value) -> &'static str {
        decode(v.to_string().as_bytes()).unwrap_err().code()
    }
    #[test]
    fn metadata_preserves_real_usage_independent_of_candidate_approval() {
        let mut v = response();
        v["status"] = json!("incomplete");
        v["output"][1]["content"][0]["text"] = json!("malformed candidates");
        let metadata = response_metadata(v.to_string().as_bytes()).unwrap();
        assert_eq!(metadata.usage["total_tokens"], 20);
        assert_eq!(metadata.model, "returned-model");
        v["usage"]["total_tokens"] = json!(21);
        assert_eq!(
            response_metadata(v.to_string().as_bytes())
                .unwrap_err()
                .code(),
            "EXTRACT_API_USAGE"
        );
    }
    #[test]
    fn completed_assistant_and_reasoning_allow_evolving_metadata() {
        let mut value = response();
        value["future"] = json!({"unknown":{"list":[1,true,null]}});
        let parsed = decode(value.to_string().as_bytes()).unwrap();
        assert_eq!(parsed.model, "returned-model");
        assert_eq!(parsed.id, "resp");
        assert_eq!(parsed.candidates.request_id, "req");
        assert!(parsed.service_tier.is_none());
        assert!(parsed.usage.get("input_tokens_details").is_none());
    }
    #[test]
    fn actual_metadata_fields_reject_null_or_wrong_types() {
        for (field, bad) in [
            ("id", Value::Null),
            ("model", Value::Null),
            ("service_tier", json!(7)),
        ] {
            let mut v = response();
            v[field] = bad;
            assert_eq!(
                response_metadata(v.to_string().as_bytes())
                    .unwrap_err()
                    .code(),
                "EXTRACT_API_RESPONSE"
            );
        }
    }
    #[test]
    fn valid_metadata_survives_invalid_output_fields_and_decode_rejects_them() {
        for (pointer, bad) in [
            ("/object", Value::Null),
            ("/created_at", Value::Null),
            ("/output/0/id", Value::Null),
            ("/output/1/content/0/annotations", json!({})),
            ("/output/0/summary", json!("bad")),
        ] {
            let mut v = response();
            if pointer == "/created_at" {
                v["created_at"] = bad;
            } else {
                *v.pointer_mut(pointer).unwrap() = bad;
            }
            let metadata = response_metadata(v.to_string().as_bytes()).unwrap();
            assert_eq!(metadata.id, "resp");
            assert_eq!(metadata.model, "returned-model");
            assert_eq!(
                metadata.usage,
                json!({"input_tokens":12,"output_tokens":8,"total_tokens":20})
            );
            assert_eq!(code(&v), "EXTRACT_API_RESPONSE");
        }
    }
    #[test]
    fn rejects_duplicate_keys_at_unknown_nesting_and_trailing_json() {
        let good = response().to_string();
        for bad in [
            good.replacen("{", "{\"future\":{\"x\":1,\"x\":2},", 1),
            format!("{good} {{}}"),
            good.replace(
                "\"total_tokens\":20",
                "\"total_tokens\":20,\"total_tokens\":20",
            ),
        ] {
            assert_eq!(
                decode(bad.as_bytes()).unwrap_err().code(),
                "EXTRACT_API_RESPONSE"
            );
            assert_eq!(
                response_metadata(bad.as_bytes()).unwrap_err().code(),
                "EXTRACT_API_RESPONSE"
            );
        }
    }
    #[test]
    fn refusal_incomplete_tools_and_extra_text_are_rejected() {
        let mut v = response();
        v["status"] = json!("incomplete");
        assert_eq!(code(&v), "EXTRACT_API_INCOMPLETE");
        let mut v = response();
        v["incomplete_details"] = json!({"reason":"max_output_tokens"});
        assert_eq!(code(&v), "EXTRACT_API_INCOMPLETE");
        let mut v = response();
        v["output"][1]["content"][0] = json!({"type":"refusal","refusal":"no"});
        assert_eq!(code(&v), "EXTRACT_API_REFUSAL");
        for item in [
            json!({"type":"function_call"}),
            response()["output"][1].clone(),
        ] {
            let mut v = response();
            v["output"].as_array_mut().unwrap().push(item);
            assert_eq!(code(&v), "EXTRACT_API_OUTPUT");
        }
        let mut v = response();
        v["output"][1]["content"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"output_text","text":"{}"}));
        assert_eq!(code(&v), "EXTRACT_API_OUTPUT");
    }
    #[test]
    fn reasoning_cannot_hide_extra_output_text_refusal_or_incomplete_status() {
        for content in [
            json!([{"type":"output_text","text":"extra"}]),
            json!([{"type":"refusal","refusal":"no"}]),
        ] {
            let mut v = response();
            v["output"][0]["content"] = content;
            assert_eq!(code(&v), "EXTRACT_API_OUTPUT");
        }
        let mut v = response();
        v["output"][0]["status"] = json!("incomplete");
        assert_eq!(code(&v), "EXTRACT_API_INCOMPLETE");
    }
    #[test]
    fn usage_missing_fractional_negative_wrong_sum_and_details_fail() {
        for usage in [
            Value::Null,
            json!({"input_tokens":12,"output_tokens":8}),
            json!({"input_tokens":12.0,"output_tokens":8,"total_tokens":20}),
            json!({"input_tokens":-1,"output_tokens":8,"total_tokens":7}),
            json!({"input_tokens":12,"output_tokens":8,"total_tokens":21}),
            json!({"input_tokens":12,"output_tokens":8,"total_tokens":20,"output_tokens_details":{"reasoning_tokens":"0"}}),
        ] {
            let mut v = response();
            v["usage"] = usage;
            assert_eq!(code(&v), "EXTRACT_API_USAGE");
            assert_eq!(
                response_metadata(v.to_string().as_bytes())
                    .unwrap_err()
                    .code(),
                "EXTRACT_API_USAGE"
            );
        }
        let mut missing = response();
        missing.as_object_mut().unwrap().remove("usage");
        assert_eq!(
            response_metadata(missing.to_string().as_bytes())
                .unwrap_err()
                .code(),
            "EXTRACT_API_USAGE"
        );
    }
    #[test]
    fn candidate_missing_nullable_extra_version_types_and_duplicates_fail() {
        let good = candidate();
        let mut missing = good.clone();
        missing["binding"]
            .as_object_mut()
            .unwrap()
            .remove("parse_id");
        let mut extra = good.clone();
        extra["producer"] = json!("fake");
        let mut version = good.clone();
        version["schema_version"] = json!(2);
        let mut wrong = good.clone();
        wrong["atoms"][0]["measurement"]["unit"] = json!(1);
        let mut missing_m = good.clone();
        missing_m["atoms"][0]
            .as_object_mut()
            .unwrap()
            .remove("measurement");
        for candidate in [missing, extra, version, wrong, missing_m] {
            let mut v = response();
            v["output"][1]["content"][0]["text"] = json!(candidate.to_string());
            assert_eq!(code(&v), "EXTRACT_API_CANDIDATES");
        }
        let mut v = response();
        v["output"][1]["content"][0]["text"] = json!(good.to_string().replacen(
            "{",
            "{\"future\":{\"x\":1,\"x\":2},",
            1
        ));
        assert_eq!(code(&v), "EXTRACT_API_CANDIDATES");
    }
}
