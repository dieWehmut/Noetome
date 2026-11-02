//! Native reconstruction. No saved path is executed or acquired by this reader.

use super::{
    ENDPOINT, ExtractionRequest, HttpObservation, ModelError, Outcome, Usage, UsageProvenance,
};
use crate::model_candidate::{Candidate, OutcomeStatus, TextEvidence};
use crate::sidecar;
use serde_json::{Map, Value, json};

fn require(value: bool) -> Result<(), ModelError> {
    if value {
        Ok(())
    } else {
        Err("MODEL_API_RESPONSE_INVALID".into())
    }
}
pub(super) fn strict_value(bytes: &[u8]) -> Result<Value, ModelError> {
    crate::credential_handoff::strict_json_value(bytes)
        .map_err(|_| "MODEL_API_RESPONSE_INVALID".into())
}
fn object<'a>(
    value: &'a Value,
    required: &[&str],
    optional: &[&str],
) -> Result<&'a Map<String, Value>, ModelError> {
    let object = value
        .as_object()
        .ok_or(ModelError::from("MODEL_API_RESPONSE_INVALID"))?;
    require(
        required.iter().all(|name| object.contains_key(*name))
            && object
                .keys()
                .all(|key| required.contains(&key.as_str()) || optional.contains(&key.as_str())),
    )?;
    Ok(object)
}
fn string(value: &Value, max: usize, nonempty: bool, identity: bool) -> Result<&str, ModelError> {
    let text = value
        .as_str()
        .ok_or(ModelError::from("MODEL_API_RESPONSE_INVALID"))?;
    require(
        text.len() <= max
            && !text.contains('\0')
            && (!nonempty || !text.trim().is_empty())
            && (!identity || !text.chars().any(char::is_control)),
    )?;
    Ok(text)
}
fn optional_string(value: &Value, max: usize) -> Result<Option<String>, ModelError> {
    if value.is_null() {
        Ok(None)
    } else {
        Ok(Some(string(value, max, true, true)?.into()))
    }
}
fn optional_u64(value: &Value) -> Result<Option<u64>, ModelError> {
    if value.is_null() {
        Ok(None)
    } else {
        Ok(Some(
            value
                .as_u64()
                .ok_or(ModelError::from("MODEL_API_RESPONSE_INVALID"))?,
        ))
    }
}
fn array(value: &Value) -> Result<&Vec<Value>, ModelError> {
    value.as_array().ok_or("MODEL_API_RESPONSE_INVALID".into())
}
fn usage(value: &Value) -> Result<(Option<u64>, Option<u64>, Option<u64>), ModelError> {
    if value.is_null() {
        return Ok((None, None, None));
    }
    let v = object(
        value,
        &["input_tokens", "output_tokens", "total_tokens"],
        &["input_tokens_details", "output_tokens_details"],
    )?;
    let input = optional_u64(&v["input_tokens"])?;
    let output = optional_u64(&v["output_tokens"])?;
    let total = optional_u64(&v["total_tokens"])?;
    if let (Some(i), Some(o), Some(t)) = (input, output, total) {
        require(i.checked_add(o) == Some(t))?;
    }
    for (field, counter, upper) in [
        ("input_tokens_details", "cached_tokens", input),
        ("output_tokens_details", "reasoning_tokens", output),
    ] {
        if let Some(details) = v.get(field).filter(|value| !value.is_null()) {
            let d = object(details, &[counter], &[])?;
            let count = d[counter]
                .as_u64()
                .ok_or(ModelError::from("MODEL_API_RESPONSE_INVALID"))?;
            require(upper.is_none_or(|upper| count <= upper))?;
        }
    }
    Ok((input, output, total))
}
pub(super) fn failure_code(code: &str) -> &'static str {
    match code {
        "authentication" => "MODEL_API_AUTHENTICATION",
        "rate_limit" => "MODEL_API_RATE_LIMIT",
        "timeout" => "MODEL_API_TIMEOUT",
        "capability_insufficient" => "MODEL_API_CAPABILITY_INSUFFICIENT",
        "invalid_response" => "MODEL_API_RESPONSE_INVALID",
        "response_limit" => "MODEL_API_RESPONSE_LIMIT",
        _ => "MODEL_API_REMOTE_UNKNOWN",
    }
}
pub(super) fn allowed_failure(code: &str) -> bool {
    matches!(
        code,
        "MODEL_API_AUTHENTICATION"
            | "MODEL_API_RATE_LIMIT"
            | "MODEL_API_TIMEOUT"
            | "MODEL_API_CAPABILITY_INSUFFICIENT"
            | "MODEL_API_RESPONSE_INVALID"
            | "MODEL_API_RESPONSE_LIMIT"
            | "MODEL_API_REMOTE_UNKNOWN"
            | "MODEL_API_OBSERVATION_INVALID"
            | "MODEL_API_HTTP_BODY_INVALID"
            | "MODEL_API_CREDENTIAL_ECHO"
            | "MODEL_API_CREDENTIAL_INVALID"
            | "MODEL_API_EVIDENCE_INVALID"
            | "MODEL_API_USAGE_BUDGET_EXCEEDED"
    )
}
fn status_failure(status: u32) -> &'static str {
    match status {
        401 | 403 => "authentication",
        429 => "rate_limit",
        408 | 504 => "timeout",
        400 | 404 | 405 | 415 | 422 => "capability_insufficient",
        _ => "remote_unknown",
    }
}

pub(super) fn validate_body(bytes: &[u8], request: &ExtractionRequest) -> Result<(), ModelError> {
    require(!bytes.is_empty() && bytes.len() <= 65536)?;
    let body = strict_value(bytes)?;
    let input=serde_json::to_string(&json!({"units":request.source.units.iter().map(|unit|json!({"ordinal":unit.ordinal,"text":unit.text})).collect::<Vec<_>>()})).map_err(|_|ModelError::from("MODEL_API_HTTP_BODY_INVALID"))?;
    let expected = json!({"model":super::MODEL,"instructions":include_str!("../../../sidecars/responses_api/instructions.txt"),
        "input":input,"max_output_tokens":request.policy.max_output_tokens,"store":false,"background":false,"stream":false,"truncation":"disabled",
        "parallel_tool_calls":false,"tools":[],"tool_choice":"none","text":{"format":{"type":"json_schema","name":"noetome_candidates","strict":true,"schema":strict_value(include_bytes!("../../../sidecars/responses_api/schema.json"))?}}});
    // The nested input is JSON text. Match its parsed value independently of
    // object member order while requiring the outer body to have exact fields.
    let mut actual = body;
    let actual_input = actual
        .get("input")
        .and_then(Value::as_str)
        .ok_or(ModelError::from("MODEL_API_HTTP_BODY_INVALID"))?;
    require(strict_value(actual_input.as_bytes())? == strict_value(input.as_bytes())?)?;
    actual["input"] = expected["input"].clone();
    super::require(actual == expected, "MODEL_API_HTTP_BODY_INVALID")
}
pub(super) fn validate_observation(
    obs: &HttpObservation,
    request: &ExtractionRequest,
    request_sha: &str,
    body_sha: &str,
) -> Result<(), ModelError> {
    super::require(
        obs.schema_version == 1
            && obs.protocol == "noetome-responses-observation-v1"
            && obs.request_sha256 == request_sha
            && obs.http_request_sha256 == body_sha
            && obs.endpoint == ENDPOINT
            && matches!(
                obs.http_state.as_str(),
                "not_sent" | "sent_once" | "remote_unknown"
            )
            && obs
                .http_status
                .is_none_or(|status| (100..=599).contains(&status))
            && obs.failure.as_deref().is_none_or(|failure| {
                matches!(
                    failure,
                    "authentication"
                        | "rate_limit"
                        | "timeout"
                        | "capability_insufficient"
                        | "remote_unknown"
                        | "invalid_response"
                        | "response_limit"
                )
            })
            && obs.response_id.as_ref().is_none_or(|text| {
                !text.trim().is_empty() && text.len() <= 256 && !text.chars().any(char::is_control)
            })
            && obs.actual_model.as_ref().is_none_or(|text| {
                !text.trim().is_empty() && text.len() <= 256 && !text.chars().any(char::is_control)
            }),
        "MODEL_API_OBSERVATION_INVALID",
    )?;
    if let (Some(i), Some(o), Some(t)) = (
        obs.usage_input_tokens,
        obs.usage_output_tokens,
        obs.usage_total_tokens,
    ) {
        super::require(i.checked_add(o) == Some(t), "MODEL_API_OBSERVATION_INVALID")?;
    }
    if obs.http_state == "not_sent" {
        super::require(
            obs.http_status.is_none()
                && obs.response_id.is_none()
                && obs.actual_model.is_none()
                && !obs.raw_response_available
                && !obs.canonical_response_available
                && obs.usage_input_tokens.is_none()
                && obs.usage_output_tokens.is_none()
                && obs.usage_total_tokens.is_none()
                && obs.failure.is_some(),
            "MODEL_API_OBSERVATION_INVALID",
        )?;
    }
    if obs.http_state == "remote_unknown" {
        super::require(
            !obs.raw_response_available
                && !obs.canonical_response_available
                && obs.response_id.is_none()
                && obs.actual_model.is_none()
                && obs.usage_input_tokens.is_none()
                && obs.usage_output_tokens.is_none()
                && obs.usage_total_tokens.is_none()
                && obs.failure.is_some(),
            "MODEL_API_OBSERVATION_INVALID",
        )?;
    }
    if obs.raw_response_available {
        super::require(
            obs.http_state == "sent_once" && obs.http_status.is_some(),
            "MODEL_API_OBSERVATION_INVALID",
        )?;
    } else {
        super::require(
            obs.response_id.is_none()
                && obs.actual_model.is_none()
                && obs.usage_input_tokens.is_none()
                && obs.usage_output_tokens.is_none()
                && obs.usage_total_tokens.is_none()
                && !obs.canonical_response_available,
            "MODEL_API_OBSERVATION_INVALID",
        )?;
    }
    if obs.canonical_response_available {
        super::require(
            obs.raw_response_available
                && obs.failure.is_none()
                && obs.response_id.is_some()
                && obs.actual_model.is_some()
                && obs
                    .http_status
                    .is_some_and(|status| (200..300).contains(&status))
                && obs
                    .usage_output_tokens
                    .is_none_or(|tokens| tokens <= request.policy.max_output_tokens),
            "MODEL_API_OBSERVATION_INVALID",
        )?;
    }
    if obs.http_state == "sent_once" {
        super::require(
            obs.http_status.is_some()
                && (obs.canonical_response_available || obs.failure.is_some()),
            "MODEL_API_OBSERVATION_INVALID",
        )?;
    }
    Ok(())
}

fn validate_response(value: &Value) -> Result<(), ModelError> {
    let v = object(
        value,
        &[
            "id",
            "object",
            "created_at",
            "status",
            "model",
            "output",
            "usage",
            "error",
            "incomplete_details",
        ],
        &[
            "background",
            "billing",
            "completed_at",
            "conversation",
            "instructions",
            "max_output_tokens",
            "max_tool_calls",
            "metadata",
            "parallel_tool_calls",
            "previous_response_id",
            "prompt",
            "prompt_cache_key",
            "prompt_cache_retention",
            "reasoning",
            "safety_identifier",
            "service_tier",
            "store",
            "temperature",
            "text",
            "tool_choice",
            "tools",
            "top_logprobs",
            "top_p",
            "truncation",
            "user",
        ],
    )?;
    require(
        v["object"] == "response"
            && matches!(
                v["status"].as_str(),
                Some(
                    "completed" | "failed" | "incomplete" | "in_progress" | "queued" | "cancelled"
                )
            ),
    )?;
    string(&v["id"], 256, true, true)?;
    string(&v["model"], 256, true, true)?;
    require(v["created_at"].as_u64().is_some())?;
    for field in ["background", "parallel_tool_calls", "store"] {
        if let Some(value) = v.get(field) {
            require(value.is_boolean())?;
        }
    }
    for field in [
        "completed_at",
        "max_output_tokens",
        "max_tool_calls",
        "top_logprobs",
    ] {
        if let Some(value) = v.get(field) {
            optional_u64(value)?;
        }
    }
    for field in [
        "previous_response_id",
        "prompt_cache_key",
        "prompt_cache_retention",
        "safety_identifier",
        "service_tier",
        "truncation",
        "user",
    ] {
        if let Some(value) = v.get(field) {
            optional_string(value, 256)?;
        }
    }
    for field in ["temperature", "top_p"] {
        if let Some(value) = v.get(field).filter(|v| !v.is_null()) {
            require(
                value
                    .as_f64()
                    .is_some_and(|n| n.is_finite() && (0.0..=2.0).contains(&n)),
            )?;
        }
    }
    if let Some(value) = v.get("instructions").filter(|v| !v.is_null()) {
        string(value, 65536, false, false)?;
    }
    if let Some(value) = v.get("metadata") {
        let meta = value
            .as_object()
            .ok_or(ModelError::from("MODEL_API_RESPONSE_INVALID"))?;
        require(meta.len() <= 16)?;
        for (k, val) in meta {
            string(&Value::String(k.clone()), 64, true, false)?;
            string(val, 512, false, false)?;
        }
    }
    if let Some(value) = v.get("billing") {
        let billing = object(value, &["payer"], &[])?;
        require(matches!(
            billing["payer"].as_str(),
            Some("developer" | "openai")
        ))?;
    }
    if let Some(value) = v.get("conversation").filter(|v| !v.is_null()) {
        let conversation = object(value, &["id"], &[])?;
        string(&conversation["id"], 256, true, true)?;
    }
    require(v.get("prompt").is_none_or(Value::is_null))?;
    if let Some(value) = v.get("reasoning").filter(|v| !v.is_null()) {
        let reasoning = object(value, &[], &["effort", "summary", "generate_summary"])?;
        for value in reasoning.values() {
            optional_string(value, 256)?;
        }
    }
    if let Some(value) = v.get("text") {
        let text = object(value, &["format"], &["verbosity"])?;
        let form = object(
            &text["format"],
            &["type"],
            &["name", "strict", "schema", "description"],
        )?;
        require(matches!(
            form["type"].as_str(),
            Some("text" | "json_object" | "json_schema")
        ))?;
        for field in ["name", "description"] {
            if let Some(value) = form.get(field) {
                string(value, 4096, false, false)?;
            }
        }
        if let Some(value) = form.get("strict") {
            require(value.is_boolean())?;
        }
        if let Some(value) = form.get("schema") {
            require(value.is_object())?;
        }
        if let Some(value) = text.get("verbosity") {
            optional_string(value, 256)?;
        }
    }
    require(
        v.get("tools").is_none_or(|value| *value == json!([]))
            && v.get("tool_choice")
                .is_none_or(|value| matches!(value.as_str(), Some("none" | "auto"))),
    )?;
    if !v["error"].is_null() {
        let error = object(&v["error"], &["code", "message"], &[])?;
        string(&error["code"], 256, true, true)?;
        string(&error["message"], 65536, false, false)?;
    }
    if !v["incomplete_details"].is_null() {
        let details = object(&v["incomplete_details"], &["reason"], &[])?;
        string(&details["reason"], 256, true, true)?;
    }
    usage(&v["usage"])?;
    for item in array(&v["output"])? {
        if item.get("type") == Some(&json!("reasoning")) {
            let item = object(
                item,
                &["id", "type", "summary"],
                &["content", "encrypted_content", "status"],
            )?;
            string(&item["id"], 256, true, true)?;
            for summary in array(&item["summary"])? {
                let summary = object(summary, &["type", "text"], &[])?;
                require(summary["type"] == "summary_text")?;
                string(&summary["text"], 524288, false, false)?;
            }
            if let Some(content) = item.get("content").filter(|v| !v.is_null()) {
                for part in array(content)? {
                    let part = object(part, &["type", "text"], &[])?;
                    require(part["type"] == "reasoning_text")?;
                    string(&part["text"], 524288, false, false)?;
                }
            }
            if let Some(content) = item.get("encrypted_content") {
                optional_string(content, 524288)?;
            }
            if let Some(status) = item.get("status") {
                require(matches!(
                    status.as_str(),
                    Some("completed" | "incomplete" | "in_progress")
                ))?;
            }
            continue;
        }
        let item = object(item, &["id", "type", "role", "status", "content"], &[])?;
        string(&item["id"], 256, true, true)?;
        require(
            item["type"] == "message"
                && item["role"] == "assistant"
                && matches!(
                    item["status"].as_str(),
                    Some("completed" | "incomplete" | "in_progress")
                ),
        )?;
        for part in array(&item["content"])? {
            let part = object(part, &["type", "text", "annotations"], &["logprobs"])?;
            require(
                part["type"] == "output_text"
                    && part["annotations"] == json!([])
                    && part.get("logprobs").is_none_or(|value| *value == json!([])),
            )?;
            string(&part["text"], 524288, false, false)?;
        }
    }
    Ok(())
}
fn canonical(
    request: &ExtractionRequest,
    wire: &[u8],
    response: &Value,
) -> Result<Outcome, ModelError> {
    require(
        response["status"] == "completed"
            && response["error"].is_null()
            && response["incomplete_details"].is_null(),
    )?;
    let messages: Vec<_> = array(&response["output"])?
        .iter()
        .filter(|item| item["type"] == "message")
        .collect();
    require(messages.len() == 1)?;
    let message = messages[0];
    require(message["status"] == "completed" && array(&message["content"])?.len() == 1)?;
    let inner =
        strict_value(string(&message["content"][0]["text"], 524288, false, false)?.as_bytes())?;
    let inner = object(&inner, &["candidates"], &[])?;
    let proposals = array(&inner["candidates"])?;
    require((1..=256).contains(&proposals.len()))?;
    let mut candidates = Vec::new();
    for (index, proposal) in proposals.iter().enumerate() {
        let proposal = object(proposal, &["unit_ordinal", "statement", "qualifiers"], &[])?;
        let ordinal = proposal["unit_ordinal"]
            .as_u64()
            .ok_or(ModelError::from("MODEL_API_RESPONSE_INVALID"))?;
        let unit = request
            .source
            .units
            .iter()
            .find(|unit| unit.ordinal == ordinal)
            .ok_or(ModelError::from("MODEL_API_RESPONSE_INVALID"))?;
        let statement = string(&proposal["statement"], 4096, true, false)?;
        let qualifiers = array(&proposal["qualifiers"])?;
        require(qualifiers.len() <= 32)?;
        let qualifiers = qualifiers
            .iter()
            .map(|value| string(value, 256, true, false).map(str::to_owned))
            .collect::<Result<Vec<_>, _>>()?;
        let start = unit
            .text
            .find(statement)
            .ok_or(ModelError::from("MODEL_API_RESPONSE_INVALID"))?;
        let next = start
            + unit.text[start..]
                .chars()
                .next()
                .ok_or(ModelError::from("MODEL_API_RESPONSE_INVALID"))?
                .len_utf8();
        require(!unit.text[next..].contains(statement))?;
        candidates.push(Candidate {
            ordinal: index as u64,
            statement: statement.into(),
            qualifiers,
            evidence: vec![TextEvidence {
                span_id: unit.span_id.clone(),
                excerpt_start_byte: start as u64,
                excerpt_end_byte: (start + statement.len()) as u64,
                excerpt_sha256: sidecar::digest(statement.as_bytes()),
            }],
        });
    }
    let (input_tokens, output_tokens, _) = usage(&response["usage"])?;
    require(output_tokens.is_none_or(|count| count <= request.policy.max_output_tokens))?;
    let outcome = Outcome {
        schema_version: 1,
        protocol: crate::model_candidate::PROTOCOL.into(),
        request_id: request.request_id.clone(),
        request_sha256: sidecar::digest(wire),
        provider: request.provider.clone(),
        actual_model: string(&response["model"], 256, true, true)?.into(),
        status: OutcomeStatus::Complete,
        candidates,
        usage: Usage {
            provenance: UsageProvenance::ProviderReported,
            input_tokens,
            output_tokens,
            cost: super::unknown_usage().cost,
        },
        failure: None,
    };
    super::require(
        serde_json::to_vec(&outcome)
            .map_err(|_| ModelError::from("MODEL_API_RESPONSE_INVALID"))?
            .len() as u64
            <= request.policy.max_response_bytes,
        "MODEL_API_RESPONSE_LIMIT",
    )?;
    Ok(outcome)
}
pub(super) fn derive(
    request: &ExtractionRequest,
    wire: &[u8],
    body: &[u8],
    raw: &[u8],
    status: u32,
) -> Result<(Option<Outcome>, HttpObservation), ModelError> {
    require(
        (100..=599).contains(&status)
            && !raw.is_empty()
            && raw.len() as u64 <= request.policy.max_response_bytes,
    )?;
    validate_body(body, request)?;
    let value = strict_value(raw)?;
    let mut obs = HttpObservation {
        schema_version: 1,
        protocol: "noetome-responses-observation-v1".into(),
        request_sha256: sidecar::digest(wire),
        http_request_sha256: sidecar::digest(body),
        endpoint: ENDPOINT.into(),
        http_state: "sent_once".into(),
        http_status: Some(status),
        response_id: None,
        actual_model: None,
        failure: None,
        raw_response_available: true,
        canonical_response_available: false,
        usage_input_tokens: None,
        usage_output_tokens: None,
        usage_total_tokens: None,
    };
    if !(200..300).contains(&status)
        && value
            .as_object()
            .is_some_and(|v| v.len() == 1 && v.contains_key("error"))
    {
        let error = object(&value["error"], &["message"], &["type", "param", "code"])?;
        string(&error["message"], 65536, false, false)?;
        for field in ["type", "param", "code"] {
            if let Some(v) = error.get(field) {
                optional_string(v, 256)?;
            }
        }
        obs.failure = Some(status_failure(status).into());
        return Ok((None, obs));
    }
    validate_response(&value)?;
    obs.response_id = Some(string(&value["id"], 256, true, true)?.into());
    obs.actual_model = Some(string(&value["model"], 256, true, true)?.into());
    let (i, o, t) = usage(&value["usage"])?;
    obs.usage_input_tokens = i;
    obs.usage_output_tokens = o;
    obs.usage_total_tokens = t;
    if !(200..300).contains(&status) {
        obs.failure = Some(status_failure(status).into());
        return Ok((None, obs));
    }
    // Preserve the adapter's distinction between a valid vendor envelope that
    // cannot supply one completed assistant text and malformed candidate JSON.
    let messages: Vec<_> = array(&value["output"])?
        .iter()
        .filter(|item| item["type"] == "message")
        .collect();
    let candidate_capability = value["status"] == "completed"
        && value["error"].is_null()
        && value["incomplete_details"].is_null()
        && messages.len() == 1
        && messages[0]["status"] == "completed"
        && array(&messages[0]["content"])?.len() == 1
        && o.is_none_or(|count| count <= request.policy.max_output_tokens);
    if !candidate_capability {
        obs.failure = Some("capability_insufficient".into());
        return Ok((None, obs));
    }
    match canonical(request, wire, &value) {
        Ok(outcome) => {
            obs.canonical_response_available = true;
            Ok((Some(outcome), obs))
        }
        Err(error) => {
            obs.failure = Some(
                if error.code == "MODEL_API_RESPONSE_LIMIT" {
                    "response_limit"
                } else {
                    "invalid_response"
                }
                .into(),
            );
            Ok((None, obs))
        }
    }
}
