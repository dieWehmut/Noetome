//! The single selected Chat wire strategy; raw receipts stay unchanged.
use super::*;
use serde_json::json;

pub(super) fn response_metadata(body: &[u8]) -> Result<ResponseMetadata, ProjectError> {
    let value = strict_json(body).map_err(|_| error("EXTRACT_API_RESPONSE"))?;
    chat_metadata(&value)
}
fn chat_metadata(value: &Value) -> Result<ResponseMetadata, ProjectError> {
    let raw = value
        .get("usage")
        .filter(|v| v.is_object())
        .ok_or_else(|| error("EXTRACT_API_USAGE"))?;
    let tokens = |name: &str| raw[name].as_u64().ok_or_else(|| error("EXTRACT_API_USAGE"));
    let input = tokens("prompt_tokens")?;
    let output = tokens("completion_tokens")?;
    let total = tokens("total_tokens")?;
    let mut usage = json!({"input_tokens":input,"output_tokens":output,"total_tokens":total});
    for (from, to) in [
        ("prompt_tokens_details", "input_tokens_details"),
        ("completion_tokens_details", "output_tokens_details"),
    ] {
        if let Some(details) = raw.get(from) {
            usage[to] = details.clone();
        }
    }
    for field in ["prompt_cache_hit_tokens", "prompt_cache_miss_tokens"] {
        if raw
            .get(field)
            .is_some_and(|v| v.as_u64().is_none_or(|n| n > input))
        {
            return Err(error("EXTRACT_API_USAGE"));
        }
    }
    if let (Some(hit), Some(miss)) = (
        raw.get("prompt_cache_hit_tokens").and_then(Value::as_u64),
        raw.get("prompt_cache_miss_tokens").and_then(Value::as_u64),
    ) {
        if hit.checked_add(miss) != Some(input) {
            return Err(error("EXTRACT_API_USAGE"));
        }
    }
    let mut normalized = value.clone();
    normalized["usage"] = usage;
    let mut parsed = metadata(&normalized)?;
    parsed.raw_usage = Some(raw.clone());
    Ok(parsed)
}
pub(super) fn decode(body: &[u8]) -> Result<ParsedResponse, ProjectError> {
    let value = strict_json(body).map_err(|_| error("EXTRACT_API_RESPONSE"))?;
    if !value.is_object() || value["object"] != "chat.completion" {
        return Err(error("EXTRACT_API_RESPONSE"));
    }
    optional_types(&value, &["id", "model", "object"], &["created"], &[], &[])?;
    optional_nullable_strings(&value, &["system_fingerprint", "service_tier"])?;
    let choices = value["choices"]
        .as_array()
        .filter(|v| v.len() == 1)
        .ok_or_else(|| error("EXTRACT_API_OUTPUT"))?;
    let choice = &choices[0];
    if !choice.is_object() || choice["index"].as_u64() != Some(0) {
        return Err(error("EXTRACT_API_OUTPUT"));
    }
    if choice
        .get("logprobs")
        .is_some_and(|v| !v.is_null() && !v.is_object())
    {
        return Err(error("EXTRACT_API_RESPONSE"));
    }
    match choice["finish_reason"].as_str() {
        Some("stop") => {}
        Some("length") => return Err(error("EXTRACT_API_INCOMPLETE")),
        Some("content_filter") => return Err(error("EXTRACT_API_REFUSAL")),
        Some(_) => return Err(error("EXTRACT_API_OUTPUT")),
        None => return Err(error("EXTRACT_API_RESPONSE")),
    }
    let message = &choice["message"];
    if !message.is_object() || message["role"] != "assistant" {
        return Err(error("EXTRACT_API_OUTPUT"));
    }
    optional_nullable_strings(message, &["reasoning_content", "refusal"])?;
    if message.get("refusal").is_some_and(|v| !v.is_null()) {
        return Err(error("EXTRACT_API_REFUSAL"));
    }
    if ["tool_calls", "function_call"]
        .iter()
        .any(|f| message.get(*f).is_some())
    {
        return Err(error("EXTRACT_API_OUTPUT"));
    }
    let text = message["content"]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| error("EXTRACT_API_OUTPUT"))?;
    let metadata = chat_metadata(&value)?;
    let candidate = strict_json(text.as_bytes()).map_err(|_| error("EXTRACT_API_CANDIDATES"))?;
    let candidates =
        serde_json::from_value(candidate).map_err(|_| error("EXTRACT_API_CANDIDATES"))?;
    Ok(ParsedResponse {
        id: metadata.id,
        model: metadata.model,
        usage: metadata.usage,
        service_tier: metadata.service_tier,
        candidates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qualifier_fix_original_fourth_structure_passes_but_unit_stays_missing() {
        let bytes =
            include_bytes!("../../../../doc/validation/assets/B04-T02-live-fourth-response.bin");
        let metadata = response_metadata(bytes).unwrap();
        assert_eq!(metadata.usage["input_tokens"], 1712);
        assert_eq!(metadata.usage["output_tokens"], 3071);
        assert_eq!(metadata.usage["total_tokens"], 4783);
        let decoded = decode(bytes).unwrap();
        assert_eq!(decoded.candidates.atoms.len(), 1);
        let candidate = &decoded.candidates.atoms[0];
        let measurement = candidate.measurement.as_ref().unwrap();
        assert_eq!(measurement.unit.as_deref(), Some("m/s"));
        assert!(
            !candidate
                .atom
                .qualifiers
                .contains(measurement.unit.as_ref().unwrap())
        );
        assert!(
            measurement
                .conditions
                .iter()
                .all(|condition| candidate.atom.qualifiers.contains(condition))
        );
    }
    #[test]
    fn shape_fix_original_third_flattened_candidate_stays_rejected_with_usage() {
        let bytes =
            include_bytes!("../../../../doc/validation/assets/B04-T02-live-third-response.bin");
        let parsed = response_metadata(bytes).unwrap();
        assert_eq!(parsed.usage["input_tokens"], 1583);
        assert_eq!(parsed.usage["output_tokens"], 3329);
        assert_eq!(parsed.usage["total_tokens"], 4912);
        assert_eq!(decode(bytes).unwrap_err().code(), "EXTRACT_API_CANDIDATES");
    }
    #[test]
    fn cache_pair_requires_checked_prompt_sum_without_inventing_absent_counters() {
        let mut v = response();
        v["usage"]["prompt_cache_hit_tokens"] = json!(8);
        v["usage"]["prompt_cache_miss_tokens"] = json!(8);
        assert_eq!(
            response_metadata(v.to_string().as_bytes())
                .unwrap_err()
                .code(),
            "EXTRACT_API_USAGE"
        );
        v["usage"]["prompt_cache_miss_tokens"] = json!(4);
        let parsed = response_metadata(v.to_string().as_bytes()).unwrap();
        assert_eq!(parsed.raw_usage, Some(v["usage"].clone()));
        for absent in ["prompt_cache_hit_tokens", "prompt_cache_miss_tokens"] {
            let mut partial = v.clone();
            partial["usage"].as_object_mut().unwrap().remove(absent);
            let parsed = response_metadata(partial.to_string().as_bytes()).unwrap();
            assert!(parsed.raw_usage.unwrap().get(absent).is_none());
        }
        v["usage"]["prompt_cache_hit_tokens"] = json!("8");
        assert_eq!(
            response_metadata(v.to_string().as_bytes())
                .unwrap_err()
                .code(),
            "EXTRACT_API_USAGE"
        );
        v["usage"]["prompt_tokens"] = json!(u64::MAX);
        v["usage"]["completion_tokens"] = json!(0);
        v["usage"]["total_tokens"] = json!(u64::MAX);
        v["usage"]["completion_tokens_details"] = json!({"reasoning_tokens":0});
        v["usage"]["prompt_cache_hit_tokens"] = json!(u64::MAX);
        v["usage"]["prompt_cache_miss_tokens"] = json!(1);
        assert_eq!(
            response_metadata(v.to_string().as_bytes())
                .unwrap_err()
                .code(),
            "EXTRACT_API_USAGE"
        );
    }
    fn response() -> Value {
        let mut candidate: Value = serde_json::from_str(include_str!(
            "../../../../fixtures/extraction-gateway-v1/response.json"
        ))
        .unwrap();
        candidate.as_object_mut().unwrap().remove("producer");
        candidate["request_id"] = json!("req");
        candidate["binding"] = json!({"project_id":"p","import_id":"i","source_id":"s","version_id":"v","content_sha256":"sha","normalized_sha256":null,"parse_id":null,"manifest_sha256":null});
        json!({"id":"chat","model":"model","object":"chat.completion","created":1,
            "choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":candidate.to_string(),"reasoning_content":null}}],
            "usage":{"prompt_tokens":12,"completion_tokens":8,"total_tokens":20,"prompt_tokens_details":{"cached_tokens":2},"completion_tokens_details":{"reasoning_tokens":1}}})
    }
    #[test]
    fn mapped_metadata_keeps_raw_usage_and_survives_length_candidate_failures() {
        for finish in ["stop", "length"] {
            let mut v = response();
            v["choices"][0]["finish_reason"] = json!(finish);
            v["choices"][0]["message"]["content"] = json!("not JSON");
            let bytes = v.to_string();
            let meta = response_metadata(bytes.as_bytes()).unwrap();
            assert_eq!(
                meta.usage,
                json!({"input_tokens":12,"output_tokens":8,"total_tokens":20,"input_tokens_details":{"cached_tokens":2},"output_tokens_details":{"reasoning_tokens":1}})
            );
            assert_eq!(meta.raw_usage, Some(v["usage"].clone()));
            assert_eq!(
                decode(bytes.as_bytes()).unwrap_err().code(),
                if finish == "length" {
                    "EXTRACT_API_INCOMPLETE"
                } else {
                    "EXTRACT_API_CANDIDATES"
                }
            );
        }
    }
    #[test]
    fn only_one_stopped_assistant_plain_json_and_typed_metadata_is_accepted() {
        assert!(decode(response().to_string().as_bytes()).is_ok());
        for (pointer, bad) in [
            ("/choices/0/index", json!(1)),
            ("/choices/0/index", json!(0.0)),
            ("/choices/0/finish_reason", json!("length")),
            ("/choices/0/finish_reason", json!("content_filter")),
            ("/choices/0/finish_reason", json!("tool_calls")),
            ("/choices/0/finish_reason", Value::Null),
            ("/choices/0/message/role", json!("user")),
            ("/choices/0/message/content", json!([])),
            ("/choices/0/message/reasoning_content", json!({})),
            ("/object", json!("chat.completion.chunk")),
            ("/created", Value::Null),
            ("/model", json!(1)),
        ] {
            let mut v = response();
            *v.pointer_mut(pointer).unwrap() = bad;
            assert!(decode(v.to_string().as_bytes()).is_err(), "{pointer}");
        }
        for field in ["tool_calls", "function_call", "refusal"] {
            let mut v = response();
            v["choices"][0]["message"][field] = if field == "refusal" {
                json!("no")
            } else {
                json!([])
            };
            assert!(decode(v.to_string().as_bytes()).is_err());
        }
        let mut v = response();
        v["choices"] = json!([]);
        assert!(decode(v.to_string().as_bytes()).is_err());
        let mut v = response();
        let extra = v["choices"][0].clone();
        v["choices"].as_array_mut().unwrap().push(extra);
        assert!(decode(v.to_string().as_bytes()).is_err());
    }
    #[test]
    fn duplicates_fences_trailing_unknown_candidates_and_bad_usage_fail() {
        let good = response();
        let text = good.to_string();
        for bad in [
            text.replacen("{", "{\"future\":{\"x\":1,\"x\":2},", 1),
            format!("{text} {{}}"),
            text.replace("\"index\":0", "\"index\":0,\"index\":0"),
        ] {
            assert!(decode(bad.as_bytes()).is_err());
            assert!(response_metadata(bad.as_bytes()).is_err());
        }
        let candidate = good["choices"][0]["message"]["content"].as_str().unwrap();
        for bad in [
            format!("```json\n{candidate}\n```"),
            format!("{candidate} {{}}"),
            candidate.replacen("{", "{\"future\":{\"x\":1,\"x\":2},", 1),
            candidate.replacen("{", "{\"producer\":\"fake\",", 1),
        ] {
            let mut v = good.clone();
            v["choices"][0]["message"]["content"] = json!(bad);
            assert_eq!(
                decode(v.to_string().as_bytes()).unwrap_err().code(),
                "EXTRACT_API_CANDIDATES"
            );
        }
        for usage in [
            Value::Null,
            json!({"prompt_tokens":12,"completion_tokens":8,"total_tokens":21}),
            json!({"prompt_tokens":12.0,"completion_tokens":8,"total_tokens":20}),
            json!({"prompt_tokens":12,"completion_tokens":8,"total_tokens":20,"completion_tokens_details":{"reasoning_tokens":9}}),
        ] {
            let mut v = good.clone();
            v["usage"] = usage;
            assert_eq!(
                response_metadata(v.to_string().as_bytes())
                    .unwrap_err()
                    .code(),
                "EXTRACT_API_USAGE"
            );
        }
    }
}
