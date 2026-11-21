//! Authored, in-memory v4 fixtures only; no model, network, budget, PDF or scientific proof.
use super::*;
use crate::domain::{AtomKind, EvidenceRelation, ValueStatus, Verification};
use crate::extraction_api_wire::ApiCandidates;
use crate::project::ProjectError;

const REQUEST: &str = "authored-compact-request";
const MODEL: &str = "noetome-test-compact-evidence";

fn config(version: Option<u32>) -> CompileConfig {
    CompileConfig {
        schema_version: 1,
        endpoint: "http://127.0.0.1:1/v1/chat/completions".into(),
        model: MODEL.into(),
        credential_env: "NOETOME_TEST_COMPACT_KEY".into(),
        credential_mode: None,
        reasoning_effort: None,
        transport_mode: "local_http_test".into(),
        usage_ledger: "authored-compact-unused.usage.json".into(),
        max_chunk_bytes: 2048,
        max_output_tokens: 1024,
        request_timeout_ms: 1000,
        request_strategy_version: version,
    }
}

fn unit(text: &str, start: usize) -> Unit {
    Unit {
        unit_id: "authored-compact-unit".into(),
        source_id: "authored-compact-source".into(),
        source_sha256: sha256(b"Authored compact original; not a PDF."),
        page_number: 7,
        page_sha256: sha256(text.as_bytes()),
        bbox: [0.0, 0.0, 612.0, 792.0],
        source_status: "text_ready".into(),
        start_byte: start,
        end_byte: start + text.len(),
        text: text.into(),
        issue_codes: vec![],
    }
}

fn prepare_version(text: &str, start: usize, version: Option<u32>) -> provider::Prepared {
    provider::prepare(
        "authored-compact-project",
        "authored-compact-run",
        REQUEST,
        &unit(text, start),
        &config(version),
        &[],
    )
    .unwrap()
}

fn prepared(text: &str, start: usize) -> provider::Prepared {
    prepare_version(text, start, Some(4))
}

fn atom(quote: &str) -> Value {
    json!({
        "concept":"自造概念", "kind":"statement", "content":"自造、尚未科学核验的解释。",
        "qualifiers":[], "evidence":[{"relation":"supports","quote":quote}],
        "measurement":null
    })
}

fn payload(quote: &str) -> Value {
    json!({"request_id":REQUEST,"atoms":[atom(quote)]})
}

fn envelope(content: &str) -> Value {
    json!({
        "object":"chat.completion", "id":"authored-compact-response", "model":MODEL,
        "choices":[{"index":0,"finish_reason":"stop","message":{
            "role":"assistant","content":content}}],
        "usage":{"prompt_tokens":30,"completion_tokens":20,"total_tokens":50}
    })
}

fn decode_text(
    prepared: &provider::Prepared,
    content: &str,
) -> Result<ApiCandidates, ProjectError> {
    provider::decode(
        &serde_json::to_vec(&envelope(content)).unwrap(),
        prepared,
        REQUEST,
    )
}

fn decode(prepared: &provider::Prepared, value: Value) -> Result<ApiCandidates, ProjectError> {
    decode_text(prepared, &serde_json::to_string(&value).unwrap())
}

fn reject(prepared: &provider::Prepared, value: Value, code: &str) {
    assert_eq!(decode(prepared, value).unwrap_err().code(), code);
}

fn body_and_input(prepared: &provider::Prepared) -> (Value, Value) {
    let body: Value = serde_json::from_slice(&prepared.body).unwrap();
    let input = serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
    (body, input)
}

#[test]
fn compact_v4_request_sends_only_request_id_and_source_without_context_or_location_metadata() {
    let text = "前缀🙂; authored compact source; 后缀";
    let context = [ConceptContext {
        concept: crate::domain::Concept {
            concept_id: "existing-context-sentinel".into(),
            revision: 9,
            canonical_name: "Authored context excluded from v4".into(),
            aliases: vec!["context-alias-sentinel".into()],
            needs_full_explanation: true,
        },
        sample_atoms: vec![],
    }];
    let unit = unit(text, 1025);
    let prepared = provider::prepare(
        "authored-compact-project",
        "authored-compact-run",
        REQUEST,
        &unit,
        &config(Some(4)),
        &context,
    )
    .unwrap();
    let (body, input) = body_and_input(&prepared);
    assert_eq!(prepared.strategy_version, 4);
    assert_eq!(input, json!({"request_id":REQUEST,"source":text}));
    assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["response_format"], json!({"type":"json_object"}));
    assert_eq!(body["thinking"]["type"], "disabled");
    assert_eq!(body["stream"], false);
    assert_eq!(body["max_tokens"], 1024);
    assert!(!body.to_string().contains("existing-context-sentinel"));
    assert!(!body.to_string().contains("context-alias-sentinel"));
    let instruction = body["messages"][0]["content"].as_str().unwrap();
    assert!(
        instruction.chars().count() < 2000,
        "v4 instruction must stay compact"
    );
    assert!(!instruction.contains("response_schema"));
    assert!(!instruction.contains("existing_concepts"));
    assert!(!instruction.contains("additionalProperties"));
    assert_eq!(prepared.binding.source_id, unit.source_id);
    assert_eq!(prepared.binding.content_sha256, unit.source_sha256);
    assert_eq!(prepared.units[0].start_byte, 1025);
    assert_eq!(prepared.units[0].end_byte, 1025 + text.len());
}

#[test]
fn compact_v4_is_explicitly_pinned_and_does_not_reinterpret_historical_or_new_default_versions() {
    assert_eq!(LEGACY_REQUEST_STRATEGY, 1);
    // The existing scope_tests create/reopen fixture exercises the actual new-run default.
    assert_eq!(ANCHOR_REQUEST_STRATEGY, 3);
    assert_eq!(COMPACT_REQUEST_STRATEGY, 4);
    for version in [None, Some(1), Some(2), Some(3), Some(4)] {
        let config = config(version);
        let bytes = serde_json::to_vec(&config).unwrap();
        let restored: CompileConfig = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.request_strategy_version, version);
        assert_eq!(restored.strategy_version().unwrap(), version.unwrap_or(1));
        assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
    }
    let historical = prepare_version("prefix; exact; suffix", 257, None);
    let explicit_v1 = prepare_version("prefix; exact; suffix", 257, Some(1));
    assert_eq!(historical.strategy_version, 1);
    assert_eq!(historical.body, explicit_v1.body);
    let v4 = prepared("prefix; exact; suffix", 257);
    assert_eq!(v4.strategy_version, 4);
    assert_ne!(historical.body, v4.body);
}

#[test]
fn compact_v4_keeps_v1_to_v3_request_branches_in_the_existing_full_contract() {
    let text = "prefix; exact; suffix";
    let mut old_inputs = Vec::new();
    for version in [1, 2, 3] {
        let old = prepare_version(text, 257, Some(version));
        let (body, mut input) = body_and_input(&old);
        assert_eq!(old.strategy_version, version);
        assert_eq!(input.as_object().unwrap().len(), 2);
        assert!(input["response_schema"].is_object());
        assert_eq!(input["request"]["schema_version"], 1);
        assert_eq!(input["request"]["request_id"], REQUEST);
        assert_eq!(
            input["request"]["binding"],
            serde_json::to_value(&old.binding).unwrap()
        );
        assert_eq!(input["request"]["unit"]["text"], text);
        assert_eq!(input["request"]["unit"]["unit_id"], "authored-compact-unit");
        assert_eq!(input["request"]["existing_concepts"], json!([]));
        assert_eq!(input["request"]["source_issues"], json!([]));
        assert_eq!(
            input["request"]["unit"]["evidence_anchor"]["excerpt_start_byte"],
            257
        );
        assert_eq!(
            input["request"]["unit"]["evidence_anchor"]["excerpt_end_byte"],
            257 + text.len()
        );
        let expected_sha = if version == 3 {
            String::new()
        } else {
            sha256(text.as_bytes())
        };
        assert_eq!(
            input["request"]["unit"]["evidence_anchor"]["excerpt_sha256"],
            expected_sha
        );
        assert_eq!(body["response_format"], json!({"type":"json_object"}));
        input["request"]["unit"]["evidence_anchor"]["excerpt_sha256"] = json!("");
        old_inputs.push(input);
    }
    assert_eq!(old_inputs[0], old_inputs[1]);
    assert_eq!(old_inputs[1], old_inputs[2]);
}

#[test]
fn compact_quotes_use_independent_utf8_byte_offsets_at_both_unit_edges_and_compute_sha() {
    let text = "中🙂café é 尾";
    let base = 511;
    let prepared = prepared(text, base);
    // Relative offsets are authored byte expectations, not decoded locations.
    for (quote, offset, length) in [
        ("中", 0, 3),
        ("🙂", 3, 4),
        ("café", 7, 5),
        ("é", 13, 3),
        ("尾", 17, 3),
        (text, 0, 20),
    ] {
        let decoded = decode(&prepared, payload(quote)).unwrap();
        assert_eq!(decoded.schema_version, 1);
        assert_eq!(decoded.request_id, REQUEST);
        assert_eq!(decoded.binding, prepared.binding);
        assert_eq!(decoded.evidence.len(), 1);
        let evidence = &decoded.evidence[0];
        assert_eq!(evidence.unit_id, "authored-compact-unit");
        assert_eq!(evidence.quote, quote);
        assert_eq!(evidence.excerpt_start_byte, base + offset);
        assert_eq!(evidence.excerpt_end_byte, base + offset + length);
        assert_eq!(evidence.excerpt_sha256, sha256(quote.as_bytes()));
        assert_eq!(&text.as_bytes()[offset..offset + length], quote.as_bytes());
        assert_eq!(decoded.atoms[0].atom.verification, Verification::Unverified);
        assert!(!decoded.atoms[0].atom.required);
    }
}

#[test]
fn compact_preserves_explanatory_content_relations_formula_units_conditions_and_qualifier_order() {
    let text = "前缀🙂; 12 m·s⁻¹ at 37 °C; only under pH 7.4; not 0 m·s⁻¹; v = Δx/Δt; 后缀";
    let prepared = prepared(text, 1025);
    let content = "先固定条件 37 °C 和 pH 7.4，再按 v = Δx/Δt 解释自造速度 12 m·s⁻¹；not 0 m·s⁻¹ 是原文明示的反证。";
    let relations = [
        (EvidenceRelation::Supports, "12 m·s⁻¹ at 37 °C"),
        (EvidenceRelation::Qualifies, "only under pH 7.4"),
        (EvidenceRelation::Refutes, "not 0 m·s⁻¹"),
        (EvidenceRelation::Supports, "v = Δx/Δt"),
    ];
    let measurement =
        json!({"value_status":"known","value":"12","unit":"m·s⁻¹","conditions":["37 °C","pH 7.4"]});
    let mut input = payload("12 m·s⁻¹ at 37 °C");
    input["atoms"][0]["kind"] = json!("comparison_value");
    input["atoms"][0]["content"] = json!(content);
    input["atoms"][0]["qualifiers"] = json!(["authored exception", "37 °C"]);
    input["atoms"][0]["measurement"] = measurement.clone();
    input["atoms"][0]["evidence"] = json!(
        relations
            .iter()
            .map(|(relation, quote)| { json!({"relation":relation,"quote":quote}) })
            .collect::<Vec<_>>()
    );
    let decoded = decode(&prepared, input).unwrap();
    let candidate = &decoded.atoms[0];
    assert_eq!(candidate.atom.kind, AtomKind::ComparisonValue);
    assert_eq!(candidate.atom.content, content);
    assert_eq!(
        candidate.atom.qualifiers,
        ["authored exception", "37 °C", "m·s⁻¹", "pH 7.4"]
    );
    assert_eq!(
        serde_json::to_value(&candidate.measurement).unwrap(),
        measurement
    );
    assert_eq!(candidate.atom.value_status, ValueStatus::Known);
    assert_eq!(candidate.atom.verification, Verification::Unverified);
    assert!(!candidate.atom.required);
    assert_eq!(decoded.evidence.len(), relations.len());
    for ((link, evidence), (relation, quote)) in candidate
        .atom
        .evidence_links
        .iter()
        .zip(&decoded.evidence)
        .zip(relations)
    {
        assert_eq!(link.relation, relation);
        assert_eq!(link.evidence_id, evidence.evidence_id);
        assert_eq!(evidence.quote, quote);
        assert_eq!(
            evidence.excerpt_start_byte,
            1025 + text.find(quote).unwrap()
        );
        assert_eq!(
            evidence.excerpt_end_byte,
            evidence.excerpt_start_byte + quote.len()
        );
        assert_eq!(evidence.excerpt_sha256, sha256(quote.as_bytes()));
        assert_ne!(evidence.quote, text);
    }
}

#[test]
fn compact_generates_distinct_valid_identities_for_same_name_claims_without_reordering_or_merging()
{
    let prepared = prepared("first authored premise; second authored consequence", 257);
    let mut first = atom("first authored premise");
    first["kind"] = json!("definition");
    first["content"] = json!("先定义自造前提。 ");
    let mut second = atom("second authored consequence");
    second["content"] = json!("再解释自造结果。");
    let input = json!({"request_id":REQUEST,"atoms":[first,second]});
    let decoded = decode(&prepared, input.clone()).unwrap();
    assert_eq!(decoded.concepts.len(), 2);
    assert_eq!(decoded.atoms.len(), 2);
    assert_eq!(decoded.evidence.len(), 2);
    assert_eq!(decoded.atoms[0].atom.content, "先定义自造前提。 ");
    assert_eq!(decoded.atoms[1].atom.content, "再解释自造结果。");
    assert_eq!(decoded.atoms[0].atom.kind, AtomKind::Definition);
    assert_eq!(decoded.atoms[1].atom.kind, AtomKind::Statement);
    assert_eq!(decoded.concepts[0].canonical_name, "自造概念");
    assert_eq!(decoded.concepts[1].canonical_name, "自造概念");
    assert_ne!(
        decoded.concepts[0].concept_id,
        decoded.concepts[1].concept_id
    );
    assert_ne!(decoded.atoms[0].atom.atom_id, decoded.atoms[1].atom.atom_id);
    assert_ne!(
        decoded.evidence[0].evidence_id,
        decoded.evidence[1].evidence_id
    );
    for index in 0..2 {
        let concept = &decoded.concepts[index];
        let atom = &decoded.atoms[index].atom;
        assert!(crate::domain::is_id(&concept.concept_id));
        assert!(crate::domain::is_id(&atom.atom_id));
        assert!(crate::domain::is_id(&decoded.evidence[index].evidence_id));
        assert_eq!(concept.revision, 1);
        assert!(concept.needs_full_explanation);
        assert!(concept.aliases.is_empty());
        assert_eq!(atom.revision, 1);
        assert_eq!(atom.concept_ids, [concept.concept_id.clone()]);
        assert_eq!(
            atom.evidence_links[0].evidence_id,
            decoded.evidence[index].evidence_id
        );
        assert_eq!(atom.verification, Verification::Unverified);
        assert_eq!(atom.value_status, ValueStatus::NotApplicable);
        assert!(!atom.required);
    }
    assert_eq!(
        serde_json::to_value(&decoded).unwrap(),
        serde_json::to_value(decode(&prepared, input).unwrap()).unwrap()
    );
}

#[test]
fn compact_correct_quote_and_wrong_scientific_claim_remain_unverified() {
    let prepared = prepared("Authored temperature is 37 °C.", 257);
    let mut input = payload("temperature is 37 °C");
    // An exact source slice verifies provenance bytes, not this deliberately false assertion.
    input["atoms"][0]["content"] = json!("自造温度是 900 °C，且对所有条件都成立。");
    let decoded = decode(&prepared, input).unwrap();
    assert_eq!(
        decoded.atoms[0].atom.content,
        "自造温度是 900 °C，且对所有条件都成立。"
    );
    assert_eq!(decoded.evidence[0].quote, "temperature is 37 °C");
    assert_eq!(decoded.atoms[0].atom.verification, Verification::Unverified);
    assert!(!decoded.atoms[0].atom.required);
}

#[test]
fn compact_empty_atoms_remain_empty_candidates_with_the_original_binding() {
    let prepared = prepared("Authored registration metadata only.", 257);
    let decoded = decode(&prepared, json!({"request_id":REQUEST,"atoms":[]})).unwrap();
    assert_eq!(decoded.request_id, REQUEST);
    assert_eq!(decoded.binding, prepared.binding);
    assert!(decoded.concepts.is_empty());
    assert!(decoded.evidence.is_empty());
    assert!(decoded.atoms.is_empty());
}

#[test]
fn compact_repeated_and_overlapping_quotes_are_rejected_without_guessing_an_occurrence() {
    for (text, quote) in [
        ("ab / ab / x", "ab"),
        ("aaaa!", "aaa"),
        ("中中中!", "中中"),
        ("🙂🙂🙂!", "🙂🙂"),
    ] {
        let prepared = prepared(text, 257);
        reject(&prepared, payload(quote), "COMPILE_EVIDENCE");
    }
    let prepared = prepared("ab / ab / x", 257);
    let decoded = decode(&prepared, payload("ab / ab")).unwrap();
    assert_eq!(decoded.evidence[0].quote, "ab / ab");
    assert_eq!(decoded.evidence[0].excerpt_start_byte, 257);
    assert_eq!(decoded.evidence[0].excerpt_end_byte, 264);
}

#[test]
fn compact_blank_absent_paraphrased_normalized_or_cross_unit_quotes_cannot_gain_support() {
    let prepared = prepared("prefix café CASE; x = −2 m·s⁻¹; suffix", 257);
    for quote in [
        "",
        " ",
        "\t",
        "\n",
        " ",
        "absent",
        "咖啡",
        "CAFÉ",
        "café",
        "case",
        "café  CASE",
        "x = -2 m·s⁻¹",
        "x = −2 m/s",
        "prefix suffix",
        "a quote from another authored source",
    ] {
        reject(&prepared, payload(quote), "COMPILE_EVIDENCE");
    }
}

#[test]
fn compact_refutes_qualifies_and_empty_evidence_never_substitute_for_supports() {
    let prepared = prepared("exact authored evidence", 257);
    for evidence in [
        json!([]),
        json!([{"relation":"refutes","quote":"exact"}]),
        json!([{"relation":"qualifies","quote":"exact"}]),
        json!([{"relation":"refutes","quote":"exact"},{"relation":"qualifies","quote":"authored evidence"}]),
    ] {
        let mut input = payload("exact");
        input["atoms"][0]["evidence"] = evidence;
        reject(&prepared, input, "EXTRACT_SUPPORT");
    }
}

#[test]
fn compact_forged_request_id_is_rejected_even_when_the_quote_matches() {
    let prepared = prepared("exact authored evidence", 257);
    for request_id in ["", "another-request", "authored-compact-request "] {
        let mut input = payload("exact");
        input["request_id"] = json!(request_id);
        reject(&prepared, input, "COMPILE_BINDING");
    }
}

#[test]
fn compact_unknown_fields_cannot_override_source_locations_or_generated_review_and_identity() {
    let prepared = prepared("exact authored evidence", 257);
    let valid = payload("exact");
    for field in [
        "schema_version",
        "binding",
        "concepts",
        "evidence",
        "source",
    ] {
        let mut input = valid.clone();
        input[field] = json!({"forged":true});
        reject(&prepared, input, "COMPILE_COMPACT");
    }
    for field in [
        "atom",
        "atom_id",
        "concept_id",
        "revision",
        "required",
        "verification",
        "value_status",
    ] {
        let mut input = valid.clone();
        input["atoms"][0][field] = json!("forged");
        reject(&prepared, input, "COMPILE_COMPACT");
    }
    for field in [
        "evidence_id",
        "unit_id",
        "excerpt_start_byte",
        "excerpt_end_byte",
        "excerpt_sha256",
        "source_id",
    ] {
        let mut input = valid.clone();
        input["atoms"][0]["evidence"][0][field] = json!("forged");
        reject(&prepared, input, "COMPILE_COMPACT");
    }
    let mut input = valid;
    input["atoms"][0]["measurement"] =
        json!({"value_status":"known","value":"12","unit":"m/s","conditions":[],"hidden":true});
    reject(&prepared, input, "COMPILE_COMPACT");
}

#[test]
fn compact_required_fields_types_and_enumerations_are_strict_at_each_level() {
    let prepared = prepared("exact authored evidence", 257);
    let valid = payload("exact");
    for field in ["request_id", "atoms"] {
        let mut input = valid.clone();
        input.as_object_mut().unwrap().remove(field);
        reject(&prepared, input, "COMPILE_COMPACT");
    }
    for field in ["concept", "kind", "content", "qualifiers", "evidence"] {
        let mut input = valid.clone();
        input["atoms"][0].as_object_mut().unwrap().remove(field);
        reject(&prepared, input, "COMPILE_COMPACT");
    }
    for field in ["relation", "quote"] {
        let mut input = valid.clone();
        input["atoms"][0]["evidence"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        reject(&prepared, input, "COMPILE_COMPACT");
    }
    for (field, value) in [
        ("concept", json!(12)),
        ("kind", json!("summary")),
        ("content", Value::Null),
        ("qualifiers", json!("not an array")),
        ("evidence", json!({})),
        ("measurement", json!(false)),
    ] {
        let mut input = valid.clone();
        input["atoms"][0][field] = value;
        reject(&prepared, input, "COMPILE_COMPACT");
    }
    let mut invalid_relation = valid;
    invalid_relation["atoms"][0]["evidence"][0]["relation"] = json!("contradicts");
    reject(&prepared, invalid_relation, "COMPILE_COMPACT");
    for input in [
        Value::Null,
        json!([]),
        json!({"request_id":REQUEST,"atoms":{}}),
    ] {
        reject(&prepared, input, "COMPILE_COMPACT");
    }
}

#[test]
fn compact_duplicate_json_keys_are_rejected_before_compact_expansion() {
    let prepared = prepared("exact authored evidence", 257);
    let mut valid = payload("exact");
    valid["atoms"][0]["measurement"] =
        json!({"value_status":"known","value":"12","unit":"m/s","conditions":[]});
    let raw = valid.to_string();
    for (needle, replacement) in [
        (
            r#""request_id":"authored-compact-request""#,
            r#""request_id":"authored-compact-request","request_id":"authored-compact-request""#,
        ),
        (
            r#""concept":"自造概念""#,
            r#""concept":"自造概念","concept":"自造概念""#,
        ),
        (
            r#""relation":"supports""#,
            r#""relation":"supports","relation":"supports""#,
        ),
        (r#""quote":"exact""#, r#""quote":"exact","quote":"exact""#),
        (r#""value":"12""#, r#""value":"12","value":"12""#),
        (r#""unit":"m/s""#, r#""unit":"m/s","unit":"m/s""#),
    ] {
        assert!(raw.contains(needle));
        let duplicate = raw.replace(needle, replacement);
        assert_eq!(
            decode_text(&prepared, &duplicate).unwrap_err().code(),
            "EXTRACT_API_CANDIDATES"
        );
    }
    let mut response = envelope(&raw).to_string();
    let needle = r#""object":"chat.completion""#;
    assert!(response.contains(needle));
    response = response.replace(
        needle,
        r#""object":"chat.completion","object":"chat.completion""#,
    );
    assert_eq!(
        provider::decode(response.as_bytes(), &prepared, REQUEST)
            .unwrap_err()
            .code(),
        "EXTRACT_API_RESPONSE"
    );
}

#[test]
fn compact_truncated_trailing_or_fenced_candidate_json_is_rejected() {
    let prepared = prepared("exact authored evidence", 257);
    let raw = payload("exact").to_string();
    for invalid in [
        raw[..raw.len() - 1].to_owned(),
        format!("{raw} {{}}"),
        format!("~~~json\n{raw}\n~~~"),
        "{".to_owned(),
    ] {
        assert_eq!(
            decode_text(&prepared, &invalid).unwrap_err().code(),
            "EXTRACT_API_CANDIDATES"
        );
    }
}

#[test]
fn compact_truncation_refusal_and_tool_response_envelopes_fail_before_acceptance() {
    let prepared = prepared("exact authored evidence", 257);
    let valid = envelope(&payload("exact").to_string());
    for (finish, code) in [
        ("length", "EXTRACT_API_INCOMPLETE"),
        ("content_filter", "EXTRACT_API_REFUSAL"),
        ("tool_calls", "EXTRACT_API_OUTPUT"),
    ] {
        let mut response = valid.clone();
        response["choices"][0]["finish_reason"] = json!(finish);
        assert_eq!(
            provider::decode(&serde_json::to_vec(&response).unwrap(), &prepared, REQUEST)
                .unwrap_err()
                .code(),
            code
        );
    }
    let mut refusal = valid.clone();
    refusal["choices"][0]["message"]["refusal"] = json!("Authored refusal");
    assert_eq!(
        provider::decode(&serde_json::to_vec(&refusal).unwrap(), &prepared, REQUEST)
            .unwrap_err()
            .code(),
        "EXTRACT_API_REFUSAL"
    );
    let mut tool = valid.clone();
    tool["choices"][0]["message"]["tool_calls"] = json!([]);
    assert_eq!(
        provider::decode(&serde_json::to_vec(&tool).unwrap(), &prepared, REQUEST)
            .unwrap_err()
            .code(),
        "EXTRACT_API_OUTPUT"
    );
    let mut blank = valid;
    blank["choices"][0]["message"]["content"] = json!(" ");
    assert_eq!(
        provider::decode(&serde_json::to_vec(&blank).unwrap(), &prepared, REQUEST)
            .unwrap_err()
            .code(),
        "EXTRACT_API_OUTPUT"
    );
}

#[test]
fn compact_absent_measurement_and_explicit_null_both_preserve_not_applicable() {
    let prepared = prepared("exact authored evidence", 257);
    let mut omitted = payload("exact");
    omitted["atoms"][0]
        .as_object_mut()
        .unwrap()
        .remove("measurement");
    let implicit = decode(&prepared, omitted).unwrap();
    let explicit = decode(&prepared, payload("exact")).unwrap();
    assert_eq!(
        serde_json::to_value(&implicit).unwrap(),
        serde_json::to_value(&explicit).unwrap()
    );
    assert!(implicit.atoms[0].measurement.is_none());
    assert_eq!(
        implicit.atoms[0].atom.value_status,
        ValueStatus::NotApplicable
    );
}

#[test]
fn compact_unknown_measurement_stays_null_and_invalid_values_do_not_become_zero() {
    let prepared = prepared("Authored value unknown at 37 °C.", 257);
    let mut input = payload("value unknown at 37 °C");
    input["atoms"][0]["kind"] = json!("comparison_value");
    input["atoms"][0]["measurement"] =
        json!({"value_status":"unknown","value":null,"unit":null,"conditions":["37 °C"]});
    let decoded = decode(&prepared, input.clone()).unwrap();
    let measurement = decoded.atoms[0].measurement.as_ref().unwrap();
    assert_eq!(measurement.value_status, ValueStatus::Unknown);
    assert!(measurement.value.is_none());
    assert!(measurement.unit.is_none());
    assert_eq!(decoded.atoms[0].atom.value_status, ValueStatus::Unknown);
    assert_eq!(decoded.atoms[0].atom.qualifiers, ["37 °C"]);
    assert_eq!(decoded.atoms[0].atom.verification, Verification::Unverified);
    let mut zero = input.clone();
    zero["atoms"][0]["measurement"]["value"] = json!("0");
    reject(&prepared, zero, "EXTRACT_MEASUREMENT");
    let mut missing = input.clone();
    missing["atoms"][0]["measurement"] = Value::Null;
    reject(&prepared, missing, "EXTRACT_MEASUREMENT");
    for measurement in [
        json!({"value_status":"known","value":null,"unit":"m/s","conditions":[]}),
        json!({"value_status":"known","value":"12","unit":null,"conditions":[]}),
        json!({"value_status":"known","value":"12","unit":"","conditions":[]}),
        json!({"value_status":"not_applicable","value":"12","unit":null,"conditions":[]}),
    ] {
        let mut invalid = input.clone();
        invalid["atoms"][0]["measurement"] = measurement;
        reject(&prepared, invalid, "EXTRACT_MEASUREMENT");
    }
    for field in ["value_status", "value", "unit", "conditions"] {
        let mut invalid = input.clone();
        invalid["atoms"][0]["measurement"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        reject(&prepared, invalid, "COMPILE_COMPACT");
    }
}
