//! Authored, in-memory evidence fixtures only: no model, network, PDF or scientific proof.
use super::*;
use crate::extraction_api_wire::ApiCandidates;
use crate::project::ProjectError;

const REQUEST: &str = "authored-precise-request";

fn make_prepared(text: &str, start: usize, version: Option<u32>) -> provider::Prepared {
    let config = CompileConfig {
        schema_version: 1,
        endpoint: "http://127.0.0.1:1/v1/chat/completions".into(),
        model: "noetome-test-precise-evidence".into(),
        credential_env: "NOETOME_TEST_PRECISE_KEY".into(),
        credential_mode: None,
        reasoning_effort: None,
        transport_mode: "local_http_test".into(),
        usage_ledger: "authored-unused.usage.json".into(),
        max_chunk_bytes: 1024,
        max_output_tokens: 512,
        request_timeout_ms: 1000,
        request_strategy_version: version,
    };
    let unit = Unit {
        unit_id: "authored-unit".into(),
        source_id: "authored-source".into(),
        source_sha256: sha256(b"Authored original; not a PDF."),
        page_number: 7,
        page_sha256: sha256(text.as_bytes()),
        bbox: [0.0, 0.0, 612.0, 792.0],
        source_status: "text_ready".into(),
        start_byte: start,
        end_byte: start + text.len(),
        text: text.into(),
        issue_codes: vec![],
    };
    provider::prepare(
        "authored-project",
        "authored-run",
        REQUEST,
        &unit,
        &config,
        &[],
    )
    .unwrap()
}

fn payload(prepared: &provider::Prepared, start: usize, end: usize, quote: &str) -> Value {
    json!({
        "schema_version":1,"request_id":REQUEST,"binding":prepared.binding,
        "concepts":[{"concept_id":"concept-authored","revision":1,
            "canonical_name":"Authored concept","aliases":[],"needs_full_explanation":true}],
        "evidence":[{"evidence_id":"evidence-authored","unit_id":"authored-unit",
            "excerpt_start_byte":start,"excerpt_end_byte":end,"excerpt_sha256":"", "quote":quote}],
        "atoms":[{"atom":{"atom_id":"atom-authored","revision":1,"kind":"statement",
            "concept_ids":["concept-authored"],"content":"Authored candidate claim.","qualifiers":[],
            "evidence_links":[{"evidence_id":"evidence-authored","relation":"supports"}],
            "required":false,"verification":"unverified","value_status":"known"},"measurement":null}]
    })
}

fn decode(prepared: &provider::Prepared, payload: Value) -> Result<ApiCandidates, ProjectError> {
    let body = serde_json::to_vec(&json!({
        "object":"chat.completion","id":"authored-response","model":"noetome-test-precise-evidence",
        "choices":[{"index":0,"finish_reason":"stop","message":{
            "role":"assistant","content":serde_json::to_string(&payload).unwrap()}}],
        "usage":{"prompt_tokens":30,"completion_tokens":20,"total_tokens":50}
    }))
    .unwrap();
    provider::decode(&body, prepared, REQUEST)
}

fn reject(prepared: &provider::Prepared, payload: Value, code: &str) {
    assert_eq!(decode(prepared, payload).unwrap_err().code(), code);
}

#[test]
fn precise_subquote_uses_resource_absolute_half_open_utf8_bytes_and_computed_sha() {
    let text = "前缀🙂; café 中的精确证据; 后缀";
    let prepared = make_prepared(text, 257, Some(2));
    let quote = "café 中的精确证据";
    let start = 257 + text.find(quote).unwrap();
    let mut input = payload(&prepared, start, start + quote.len(), quote);
    input["evidence"][0]["excerpt_sha256"] = json!("untrusted model hash");
    let decoded = decode(&prepared, input).unwrap();
    let entry = &decoded.evidence[0];
    assert_eq!(entry.quote, quote);
    assert_ne!(entry.quote, text);
    assert_eq!(
        (entry.excerpt_start_byte, entry.excerpt_end_byte),
        (start, start + quote.len())
    );
    assert_eq!(entry.excerpt_sha256, sha256(quote.as_bytes()));
    assert_eq!(entry.unit_id, "authored-unit");
    assert_eq!(prepared.strategy_version, 2);
    assert_eq!(decoded.schema_version, 1);
    assert_eq!(decoded.binding, prepared.binding);
    assert_eq!(
        decoded.atoms[0].atom.verification,
        crate::domain::Verification::Unverified
    );
    assert!(!decoded.atoms[0].atom.required);
}

#[test]
fn precise_quotes_accept_both_unit_edges_and_complete_multibyte_scalars() {
    let text = "中🙂café é 尾";
    let prepared = make_prepared(text, 511, Some(2));
    for quote in ["中", "🙂", "café", "é", "尾", text] {
        let start = 511 + text.find(quote).unwrap();
        let decoded = decode(
            &prepared,
            payload(&prepared, start, start + quote.len(), quote),
        )
        .unwrap();
        assert_eq!(decoded.evidence[0].excerpt_start_byte, start);
        assert_eq!(decoded.evidence[0].excerpt_end_byte, start + quote.len());
        assert_eq!(decoded.evidence[0].quote, quote);
    }
}

#[test]
fn precise_invalid_ranges_fail_before_relocation_even_for_a_unique_existing_quote() {
    let text = "中🙂 unique 尾";
    let base = 257;
    let prepared = make_prepared(text, base, Some(2));
    for (start, end) in [
        (base - 1, base + 3),
        (base, base + text.len() + 1),
        (base, base),
        (base + 3, base),
        (0, 3),
        (base + 1, base + 3),
        (base, base + 1),
        (base + 4, base + 7),
        (base + 3, base + 4),
        (usize::MAX - 1, usize::MAX),
    ] {
        reject(
            &prepared,
            payload(&prepared, start, end, "unique"),
            "COMPILE_EVIDENCE",
        );
    }
}

#[test]
fn precise_blank_missing_or_normalized_quotes_do_not_fall_back_to_full_unit() {
    let text = "prefix café CASE; suffix";
    let prepared = make_prepared(text, 257, Some(2));
    for quote in [
        "",
        " ",
        "\t",
        "\n",
        "\u{2003}",
        "absent",
        "CAFÉ",
        "cafe\u{301}",
        "case",
    ] {
        let mut input = payload(&prepared, 257, 263, quote);
        input["evidence"][0]["excerpt_sha256"] = json!(sha256(text.as_bytes()));
        reject(&prepared, input, "COMPILE_EVIDENCE");
    }
}

#[test]
fn precise_unique_verbatim_quote_relocates_with_absolute_offsets_without_expansion() {
    let text = "prefix; 唯一🙂证据; suffix";
    let prepared = make_prepared(text, 1025, Some(2));
    let quote = "唯一🙂证据";
    let expected = 1025 + text.find(quote).unwrap();
    let decoded = decode(&prepared, payload(&prepared, 1025, 1026, quote)).unwrap();
    let entry = &decoded.evidence[0];
    assert_eq!(entry.quote, quote);
    assert_eq!(
        (entry.excerpt_start_byte, entry.excerpt_end_byte),
        (expected, expected + quote.len())
    );
    assert_eq!(entry.excerpt_sha256, sha256(quote.as_bytes()));
    assert_ne!(entry.excerpt_sha256, sha256(text.as_bytes()));
}

#[test]
fn precise_repeated_quote_is_valid_when_the_supplied_slice_matches() {
    let prepared = make_prepared("aba / aba", 257, Some(2));
    for relative in [0, 6] {
        let start = 257 + relative;
        let decoded = decode(&prepared, payload(&prepared, start, start + 3, "aba")).unwrap();
        assert_eq!(decoded.evidence[0].excerpt_start_byte, start);
        assert_eq!(decoded.evidence[0].excerpt_end_byte, start + 3);
    }
    let prepared = make_prepared("aaaa!", 511, Some(2));
    for relative in [0, 1] {
        let start = 511 + relative;
        let decoded = decode(&prepared, payload(&prepared, start, start + 3, "aaa")).unwrap();
        assert_eq!(decoded.evidence[0].excerpt_start_byte, start);
    }
}

#[test]
fn precise_misaligned_repeated_and_overlapping_occurrences_are_ambiguous() {
    for (text, quote, relative) in [
        ("ab / ab / x", "ab", 10),
        ("aaaa!", "aaa", 4),
        ("中中中!", "中中", 9),
    ] {
        let prepared = make_prepared(text, 257, Some(2));
        reject(
            &prepared,
            payload(&prepared, 257 + relative, 258 + relative, quote),
            "COMPILE_EVIDENCE",
        );
    }
}

#[test]
fn precise_hashes_are_recomputed_for_empty_wrong_full_unit_and_uppercase_hashes() {
    let text = "prefix; exact; suffix";
    let prepared = make_prepared(text, 257, Some(2));
    for supplied in [
        String::new(),
        "not a hash".into(),
        sha256(text.as_bytes()),
        sha256(b"exact").to_uppercase(),
    ] {
        let mut input = payload(&prepared, 265, 270, "exact");
        input["evidence"][0]["excerpt_sha256"] = json!(supplied);
        let decoded = decode(&prepared, input).unwrap();
        assert_eq!(decoded.evidence[0].excerpt_sha256, sha256(b"exact"));
        assert_eq!(decoded.evidence[0].quote, "exact");
    }
}

#[test]
fn precise_wrong_request_binding_or_unit_cannot_borrow_a_matching_quote() {
    let prepared = make_prepared("exact", 257, Some(2));
    let valid = payload(&prepared, 257, 262, "exact");
    let mut wrong_request = valid.clone();
    wrong_request["request_id"] = json!("another-request");
    reject(&prepared, wrong_request, "COMPILE_BINDING");
    for field in [
        "project_id",
        "import_id",
        "source_id",
        "version_id",
        "content_sha256",
        "normalized_sha256",
        "parse_id",
        "manifest_sha256",
    ] {
        let mut input = valid.clone();
        input["binding"][field] = json!("another-source-or-version");
        reject(&prepared, input, "COMPILE_BINDING");
    }
    for unit in ["another-source-unit", "authored-source", ""] {
        let mut input = valid.clone();
        input["evidence"][0]["unit_id"] = json!(unit);
        reject(&prepared, input, "COMPILE_EVIDENCE");
    }
}

#[test]
fn precise_support_refute_and_qualify_relations_survive_without_relabeling() {
    let prepared = make_prepared("exact", 257, Some(2));
    let mut input = payload(&prepared, 257, 262, "exact");
    let original = input["evidence"][0].clone();
    let relations = ["supports", "refutes", "qualifies"];
    input["evidence"] = json!(
        relations
            .iter()
            .map(|relation| {
                let mut evidence = original.clone();
                evidence["evidence_id"] = json!(format!("evidence-{relation}"));
                evidence
            })
            .collect::<Vec<_>>()
    );
    let links = json!(
        relations
            .iter()
            .map(|relation| json!({
                "evidence_id":format!("evidence-{relation}"),"relation":relation
            }))
            .collect::<Vec<_>>()
    );
    input["atoms"][0]["atom"]["evidence_links"] = links.clone();
    let decoded = decode(&prepared, input).unwrap();
    assert_eq!(decoded.evidence.len(), 3);
    assert_eq!(
        serde_json::to_value(&decoded.atoms[0].atom.evidence_links).unwrap(),
        links
    );
    assert_eq!(
        decoded.atoms[0].atom.verification,
        crate::domain::Verification::Unverified
    );
}

#[test]
fn precise_refutes_or_qualifies_alone_never_substitute_for_supports() {
    let prepared = make_prepared("exact", 257, Some(2));
    for relation in ["refutes", "qualifies"] {
        let mut input = payload(&prepared, 257, 262, "exact");
        input["atoms"][0]["atom"]["evidence_links"][0]["relation"] = json!(relation);
        reject(&prepared, input, "EXTRACT_SUPPORT");
    }
}

#[test]
fn precise_measurement_units_conditions_and_existing_qualifiers_remain_verbatim() {
    let text = "Authored comparison: 12 m·s⁻¹ at 37 °C; pH 7.4.";
    let prepared = make_prepared(text, 257, Some(2));
    let mut input = payload(&prepared, 257, 257 + text.len(), text);
    input["atoms"][0]["atom"]["kind"] = json!("comparison_value");
    input["atoms"][0]["atom"]["qualifiers"] = json!(["authored exception", "37 °C"]);
    let measurement = json!({"value_status":"known","value":"12","unit":"m·s⁻¹", "conditions":["37 °C", "pH 7.4"]});
    input["atoms"][0]["measurement"] = measurement.clone();
    let decoded = decode(&prepared, input).unwrap();
    let candidate = &decoded.atoms[0];
    assert_eq!(
        serde_json::to_value(candidate.measurement.as_ref().unwrap()).unwrap(),
        measurement
    );
    assert_eq!(
        candidate.atom.qualifiers,
        ["authored exception", "37 °C", "m·s⁻¹", "pH 7.4"]
    );
    assert_eq!(
        candidate.atom.verification,
        crate::domain::Verification::Unverified
    );
}

#[test]
fn precise_unknown_measurement_stays_null_and_malformed_measurements_are_rejected() {
    let text = "Authored value unknown at 37 °C.";
    let prepared = make_prepared(text, 257, Some(2));
    let mut input = payload(&prepared, 257, 257 + text.len(), text);
    input["atoms"][0]["atom"]["kind"] = json!("comparison_value");
    input["atoms"][0]["atom"]["value_status"] = json!("unknown");
    input["atoms"][0]["measurement"] =
        json!({"value_status":"unknown","value":null,"unit":null,"conditions":["37 °C"]});
    let decoded = decode(&prepared, input.clone()).unwrap();
    let measurement = decoded.atoms[0].measurement.as_ref().unwrap();
    assert!(measurement.value.is_none());
    assert!(measurement.unit.is_none());
    assert_eq!(
        measurement.value_status,
        crate::domain::ValueStatus::Unknown
    );
    assert_eq!(decoded.atoms[0].atom.qualifiers, ["37 °C"]);
    assert_eq!(
        decoded.atoms[0].atom.verification,
        crate::domain::Verification::Unverified
    );
    let mut zero = input.clone();
    zero["atoms"][0]["measurement"]["value"] = json!("0");
    reject(&prepared, zero, "EXTRACT_MEASUREMENT");
    let mut missing = input.clone();
    missing["atoms"][0]["measurement"] = Value::Null;
    reject(&prepared, missing, "EXTRACT_MEASUREMENT");
    input["atoms"][0]["atom"]["value_status"] = json!("known");
    input["atoms"][0]["measurement"]["value_status"] = json!("known");
    input["atoms"][0]["measurement"]["value"] = json!("12");
    reject(&prepared, input, "EXTRACT_MEASUREMENT");
}

#[test]
fn precise_valid_bytes_do_not_grant_scientific_review_or_required_status() {
    let text = "Authored original claim.";
    let prepared = make_prepared(text, 257, Some(2));
    let mut input = payload(&prepared, 257, 257 + text.len(), text);
    // Deliberately unsupported content: byte validation is not semantic verification.
    input["atoms"][0]["atom"]["content"] =
        json!("A deliberately unsupported scientific assertion.");
    let decoded = decode(&prepared, input.clone()).unwrap();
    assert_eq!(
        decoded.atoms[0].atom.content,
        "A deliberately unsupported scientific assertion."
    );
    assert_eq!(
        decoded.atoms[0].atom.verification,
        crate::domain::Verification::Unverified
    );
    assert!(!decoded.atoms[0].atom.required);
    input["atoms"][0]["atom"]["required"] = json!(true);
    reject(&prepared, input.clone(), "EXTRACT_VERIFICATION");
    input["atoms"][0]["atom"]["required"] = json!(false);
    input["atoms"][0]["atom"]["verification"] = json!("fixture_reviewed");
    reject(&prepared, input, "EXTRACT_VERIFICATION");
}

#[test]
fn legacy_missing_strategy_and_explicit_v1_keep_identical_request_bytes_and_full_unit_echo() {
    let text = "prefix; exact; suffix";
    let historical = make_prepared(text, 257, None);
    let explicit_v1 = make_prepared(text, 257, Some(1));
    let precise = make_prepared(text, 257, Some(2));
    assert_eq!(historical.strategy_version, 1);
    assert_eq!(historical.body, explicit_v1.body);
    assert_eq!(sha256(&historical.body), sha256(&explicit_v1.body));
    assert_ne!(historical.body, precise.body);
    let old_body: Value = serde_json::from_slice(&historical.body).unwrap();
    let input: Value =
        serde_json::from_str(old_body["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(input["request"]["schema_version"], 1);
    assert_eq!(input["request"]["unit"]["text"], text);
    assert_eq!(
        input["request"]["unit"]["evidence_anchor"]["excerpt_start_byte"],
        257
    );
    assert_eq!(
        input["request"]["unit"]["evidence_anchor"]["excerpt_end_byte"],
        257 + text.len()
    );
    assert_eq!(
        input["request"]["unit"]["evidence_anchor"]["excerpt_sha256"],
        sha256(text.as_bytes())
    );
    assert_eq!(old_body["thinking"]["type"], "disabled");
    assert_eq!(old_body["response_format"]["type"], "json_object");
    let candidate = payload(&historical, 265, 270, "exact");
    let old_decoded = decode(&historical, candidate.clone()).unwrap();
    let explicit_decoded = decode(&explicit_v1, candidate.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&old_decoded).unwrap(),
        serde_json::to_value(&explicit_decoded).unwrap()
    );
    assert_eq!(old_decoded.evidence[0].quote, text);
    assert_eq!(
        (
            old_decoded.evidence[0].excerpt_start_byte,
            old_decoded.evidence[0].excerpt_end_byte
        ),
        (257, 257 + text.len())
    );
    assert_eq!(
        old_decoded.evidence[0].excerpt_sha256,
        sha256(text.as_bytes())
    );
    assert_eq!(
        decode(&precise, candidate).unwrap().evidence[0].quote,
        "exact"
    );
}

fn full_anchor_payload(prepared: &provider::Prepared, quote: &str) -> Value {
    let body: Value = serde_json::from_slice(&prepared.body).unwrap();
    let input: Value =
        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
    let anchor = &input["request"]["unit"]["evidence_anchor"];
    let mut candidate = payload(
        prepared,
        anchor["excerpt_start_byte"].as_u64().unwrap() as usize,
        anchor["excerpt_end_byte"].as_u64().unwrap() as usize,
        quote,
    );
    candidate["evidence"][0]["excerpt_sha256"] = anchor["excerpt_sha256"].clone();
    candidate
}

#[test]
fn anchor_v3_request_copies_full_legal_anchor_without_manual_offsets_or_hashes() {
    let text = "前缀🙂; café 中的唯一证据; 后缀";
    let prepared = make_prepared(text, 257, Some(3));
    assert_eq!(prepared.strategy_version, 3);
    let body: Value = serde_json::from_slice(&prepared.body).unwrap();
    let input: Value =
        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
    let anchor = &input["request"]["unit"]["evidence_anchor"];
    assert_eq!(
        anchor,
        &json!({
            "excerpt_start_byte":257,"excerpt_end_byte":257 + text.len(),"excerpt_sha256":""
        })
    );
    let instruction = body["messages"][0]["content"].as_str().unwrap();
    assert!(instruction.contains("禁止手算字符或 UTF-8 字节偏移，不计算 SHA256"));
    assert!(instruction.contains("直接输出最终 JSON，不输出推理过程或中间计算"));
    for field in [
        "excerpt_start_byte",
        "excerpt_end_byte",
        "excerpt_sha256",
        "evidence_anchor",
    ] {
        assert!(instruction.contains(field), "missing anchor field {field}");
    }
    assert!(instruction.contains("唯一"));
    for version in [1, 2] {
        let historical = make_prepared(text, 257, Some(version));
        let old_body: Value = serde_json::from_slice(&historical.body).unwrap();
        let mut old_input: Value =
            serde_json::from_str(old_body["messages"][1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(
            old_input["request"]["unit"]["evidence_anchor"]["excerpt_sha256"],
            sha256(text.as_bytes())
        );
        old_input["request"]["unit"]["evidence_anchor"]["excerpt_sha256"] = json!("");
        assert_eq!(input, old_input);
        assert_eq!(body["response_format"], old_body["response_format"]);
        assert_ne!(
            body["messages"][0]["content"],
            old_body["messages"][0]["content"]
        );
    }
}

#[test]
fn anchor_v3_unique_subquote_relocates_from_full_legal_anchor_with_computed_sha() {
    let text = "前缀🙂; café 中的唯一证据; 后缀";
    let quote = "café 中的唯一证据";
    let base = 1025;
    let prepared = make_prepared(text, base, Some(3));
    let candidate = full_anchor_payload(&prepared, quote);
    assert_eq!(candidate["evidence"][0]["excerpt_start_byte"], base);
    assert_eq!(
        candidate["evidence"][0]["excerpt_end_byte"],
        base + text.len()
    );
    assert_eq!(candidate["evidence"][0]["excerpt_sha256"], "");
    let decoded = decode(&prepared, candidate).unwrap();
    let evidence = &decoded.evidence[0];
    let expected = base + text.find(quote).unwrap();
    assert_eq!(
        (evidence.excerpt_start_byte, evidence.excerpt_end_byte),
        (expected, expected + quote.len())
    );
    assert_eq!(evidence.quote, quote);
    assert_ne!(evidence.quote, text);
    assert_eq!(evidence.excerpt_sha256, sha256(quote.as_bytes()));
    assert_ne!(evidence.excerpt_sha256, sha256(text.as_bytes()));
    assert_eq!(decoded.binding, prepared.binding);
    assert_eq!(
        decoded.atoms[0].atom.verification,
        crate::domain::Verification::Unverified
    );
    assert!(!decoded.atoms[0].atom.required);
}

#[test]
fn anchor_v3_full_anchor_accepts_unit_edges_and_complete_multibyte_scalars() {
    let text = "中🙂café é 尾";
    let base = 511;
    let prepared = make_prepared(text, base, Some(3));
    for quote in ["中", "🙂", "café", "é", "尾", text] {
        let decoded = decode(&prepared, full_anchor_payload(&prepared, quote)).unwrap();
        let evidence = &decoded.evidence[0];
        let expected = base + text.find(quote).unwrap();
        assert_eq!(
            (evidence.excerpt_start_byte, evidence.excerpt_end_byte),
            (expected, expected + quote.len())
        );
        assert_eq!(evidence.quote, quote);
        assert_eq!(evidence.excerpt_sha256, sha256(quote.as_bytes()));
    }
}

#[test]
fn anchor_v3_invalid_ranges_fail_before_unique_quote_relocation() {
    let text = "中🙂 unique 尾";
    let base = 257;
    let prepared = make_prepared(text, base, Some(3));
    for (start, end) in [
        (base - 1, base + 3),
        (base, base + text.len() + 1),
        (base, base),
        (base + 3, base),
        (0, 3),
        (base + 1, base + 3),
        (base, base + 1),
        (base + 4, base + 7),
        (base + 3, base + 4),
        (usize::MAX - 1, usize::MAX),
    ] {
        let mut candidate = full_anchor_payload(&prepared, "unique");
        candidate["evidence"][0]["excerpt_start_byte"] = json!(start);
        candidate["evidence"][0]["excerpt_end_byte"] = json!(end);
        reject(&prepared, candidate, "COMPILE_EVIDENCE");
    }
}

#[test]
fn anchor_v3_full_anchor_rejects_repeated_and_overlapping_quotes() {
    for (text, quote) in [("ab / ab / x", "ab"), ("aaaa!", "aaa"), ("中中中!", "中中")] {
        let prepared = make_prepared(text, 257, Some(3));
        reject(
            &prepared,
            full_anchor_payload(&prepared, quote),
            "COMPILE_EVIDENCE",
        );
    }
}

#[test]
fn anchor_v3_exact_duplicate_slice_remains_valid_without_relocation() {
    for (text, quote, offsets) in [("aba / aba", "aba", [0, 6]), ("aaaa!", "aaa", [0, 1])] {
        let prepared = make_prepared(text, 257, Some(3));
        for relative in offsets {
            let start = 257 + relative;
            let mut candidate = full_anchor_payload(&prepared, quote);
            candidate["evidence"][0]["excerpt_start_byte"] = json!(start);
            candidate["evidence"][0]["excerpt_end_byte"] = json!(start + quote.len());
            let decoded = decode(&prepared, candidate).unwrap();
            assert_eq!(decoded.evidence[0].excerpt_start_byte, start);
            assert_eq!(decoded.evidence[0].excerpt_end_byte, start + quote.len());
            assert_eq!(decoded.evidence[0].excerpt_sha256, sha256(quote.as_bytes()));
        }
    }
}

#[test]
fn anchor_v3_blank_absent_or_normalized_quotes_never_fall_back_to_full_unit() {
    let prepared = make_prepared("prefix café CASE; suffix", 257, Some(3));
    for quote in [
        "",
        " ",
        "\t",
        "\n",
        "\u{2003}",
        "absent",
        "CAFÉ",
        "cafe\u{301}",
        "case",
    ] {
        let mut candidate = full_anchor_payload(&prepared, quote);
        candidate["evidence"][0]["excerpt_sha256"] =
            json!(sha256(prepared.units[0].text.as_bytes()));
        reject(&prepared, candidate, "COMPILE_EVIDENCE");
    }
}

#[test]
fn anchor_v3_wrong_request_source_or_unit_cannot_borrow_unique_quote() {
    let prepared = make_prepared("prefix 唯一🙂证据 suffix", 257, Some(3));
    let valid = full_anchor_payload(&prepared, "唯一🙂证据");
    let mut wrong_request = valid.clone();
    wrong_request["request_id"] = json!("another-request");
    reject(&prepared, wrong_request, "COMPILE_BINDING");
    for field in [
        "project_id",
        "import_id",
        "source_id",
        "version_id",
        "content_sha256",
        "normalized_sha256",
        "parse_id",
        "manifest_sha256",
    ] {
        let mut candidate = valid.clone();
        candidate["binding"][field] = json!("another-source-or-version");
        reject(&prepared, candidate, "COMPILE_BINDING");
    }
    for unit in ["another-source-unit", "authored-source", ""] {
        let mut candidate = valid.clone();
        candidate["evidence"][0]["unit_id"] = json!(unit);
        reject(&prepared, candidate, "COMPILE_EVIDENCE");
    }
}

#[test]
fn anchor_v3_hashes_are_computed_instead_of_copying_anchor_or_candidate_hashes() {
    let text = "prefix; 唯一🙂证据; suffix";
    let quote = "唯一🙂证据";
    let prepared = make_prepared(text, 257, Some(3));
    for supplied in [
        String::new(),
        "not a hash".into(),
        sha256(text.as_bytes()),
        sha256(quote.as_bytes()).to_uppercase(),
    ] {
        let mut candidate = full_anchor_payload(&prepared, quote);
        candidate["evidence"][0]["excerpt_sha256"] = json!(supplied);
        let decoded = decode(&prepared, candidate).unwrap();
        assert_eq!(decoded.evidence[0].quote, quote);
        assert_eq!(decoded.evidence[0].excerpt_sha256, sha256(quote.as_bytes()));
        assert_ne!(decoded.evidence[0].excerpt_sha256, sha256(text.as_bytes()));
    }
}

#[test]
fn anchor_v3_relations_measurement_and_unverified_contracts_are_unchanged() {
    let text = "Authored comparison: 12 m·s⁻¹ at 37 °C; pH 7.4.";
    let prepared = make_prepared(text, 257, Some(3));
    let mut candidate = full_anchor_payload(&prepared, "12 m·s⁻¹ at 37 °C; pH 7.4");
    let original = candidate["evidence"][0].clone();
    let relations = ["supports", "refutes", "qualifies"];
    candidate["evidence"] = json!(
        relations
            .iter()
            .map(|relation| {
                let mut evidence = original.clone();
                evidence["evidence_id"] = json!(format!("evidence-{relation}"));
                evidence
            })
            .collect::<Vec<_>>()
    );
    let links = json!(
        relations
            .iter()
            .map(|relation| json!({
                "evidence_id":format!("evidence-{relation}"),"relation":relation
            }))
            .collect::<Vec<_>>()
    );
    candidate["atoms"][0]["atom"]["evidence_links"] = links.clone();
    candidate["atoms"][0]["atom"]["kind"] = json!("comparison_value");
    candidate["atoms"][0]["atom"]["qualifiers"] = json!(["authored exception", "37 °C"]);
    // Deliberately incorrect semantics must not become scientifically reviewed.
    candidate["atoms"][0]["atom"]["content"] =
        json!("A deliberately unsupported scientific assertion.");
    let measurement =
        json!({"value_status":"known","value":"12","unit":"m·s⁻¹","conditions":["37 °C","pH 7.4"]});
    candidate["atoms"][0]["measurement"] = measurement.clone();
    let decoded = decode(&prepared, candidate.clone()).unwrap();
    assert_eq!(decoded.evidence.len(), 3);
    assert!(
        decoded
            .evidence
            .iter()
            .all(|evidence| evidence.quote == "12 m·s⁻¹ at 37 °C; pH 7.4")
    );
    assert_eq!(
        serde_json::to_value(&decoded.atoms[0].atom.evidence_links).unwrap(),
        links
    );
    assert_eq!(
        serde_json::to_value(decoded.atoms[0].measurement.as_ref().unwrap()).unwrap(),
        measurement
    );
    assert_eq!(
        decoded.atoms[0].atom.qualifiers,
        ["authored exception", "37 °C", "m·s⁻¹", "pH 7.4"]
    );
    assert_eq!(
        decoded.atoms[0].atom.content,
        "A deliberately unsupported scientific assertion."
    );
    assert_eq!(
        decoded.atoms[0].atom.verification,
        crate::domain::Verification::Unverified
    );
    assert!(!decoded.atoms[0].atom.required);
    candidate["atoms"][0]["atom"]["required"] = json!(true);
    reject(&prepared, candidate.clone(), "EXTRACT_VERIFICATION");
    candidate["atoms"][0]["atom"]["required"] = json!(false);
    candidate["atoms"][0]["atom"]["verification"] = json!("fixture_reviewed");
    reject(&prepared, candidate, "EXTRACT_VERIFICATION");
}

#[test]
fn anchor_v3_refutes_or_qualifies_without_supports_are_rejected() {
    let prepared = make_prepared("prefix 唯一🙂证据 suffix", 257, Some(3));
    for relation in ["refutes", "qualifies"] {
        let mut candidate = full_anchor_payload(&prepared, "唯一🙂证据");
        candidate["atoms"][0]["atom"]["evidence_links"][0]["relation"] = json!(relation);
        reject(&prepared, candidate, "EXTRACT_SUPPORT");
    }
}

#[test]
fn anchor_v3_unknown_measurement_stays_null_and_never_becomes_zero() {
    let prepared = make_prepared("Authored value unknown at 37 °C.", 257, Some(3));
    let mut candidate = full_anchor_payload(&prepared, "value unknown at 37 °C");
    candidate["atoms"][0]["atom"]["kind"] = json!("comparison_value");
    candidate["atoms"][0]["atom"]["value_status"] = json!("unknown");
    candidate["atoms"][0]["measurement"] =
        json!({"value_status":"unknown","value":null,"unit":null,"conditions":["37 °C"]});
    let decoded = decode(&prepared, candidate.clone()).unwrap();
    let measurement = decoded.atoms[0].measurement.as_ref().unwrap();
    assert_eq!(
        measurement.value_status,
        crate::domain::ValueStatus::Unknown
    );
    assert!(measurement.value.is_none());
    assert!(measurement.unit.is_none());
    assert_eq!(decoded.atoms[0].atom.qualifiers, ["37 °C"]);
    let mut zero = candidate.clone();
    zero["atoms"][0]["measurement"]["value"] = json!("0");
    reject(&prepared, zero, "EXTRACT_MEASUREMENT");
    candidate["atoms"][0]["measurement"] = Value::Null;
    reject(&prepared, candidate, "EXTRACT_MEASUREMENT");
}

#[test]
fn legacy_v1_v2_request_body_byte_hashes_remain_frozen() {
    // Independent pre-v3 contract snapshot: complete system prompt, schema,
    // binding, anchor, user-message serialization and outer request bytes.
    // This is authored local fixture data, not a provider/model observation.
    let text = "prefix; exact; suffix";
    for (version, length, body_sha, prompt_sha) in [
        (
            1,
            6656,
            "82b3788427d5a48db4f707f33dd8bc56d4186fe400e1535d4e85cf53b50c5429",
            "52cf9d3d75cdb352b54f6e6dd9c1c0d6e4aa76bad0c1950b732911a3d17fbf41",
        ),
        (
            2,
            7682,
            "9c64b5e7e446d0e99344222e7c8e71bec7782739e1d7df980468ebf2e063c97f",
            "24b4dbe732e2ee545b3ffd2cb02c50e868cbf0f50142cea1745af9ebbbdbae69",
        ),
    ] {
        let prepared = make_prepared(text, 257, Some(version));
        assert_eq!(prepared.body.len(), length, "v{version} body byte length");
        assert_eq!(
            sha256(&prepared.body),
            body_sha,
            "v{version} body bytes changed"
        );
        let body: Value = serde_json::from_slice(&prepared.body).unwrap();
        let instruction = body["messages"][0]["content"].as_str().unwrap();
        assert_eq!(sha256(instruction.as_bytes()), prompt_sha);
    }
    let historical = make_prepared(text, 257, None);
    assert_eq!(historical.strategy_version, 1);
    assert_eq!(historical.body, make_prepared(text, 257, Some(1)).body);
}
