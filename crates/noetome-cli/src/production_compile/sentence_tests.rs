//! Authored, in-memory strategy-v5 fixtures; no model, network, budget or scientific proof.
use super::*;
use crate::{
    domain::{EvidenceRelation, ValueStatus, Verification},
    extraction_api_wire::ApiCandidates,
};

const REQUEST: &str = "authored-sentence-request";
const MODEL: &str = "noetome-test-sentence-evidence";
const SOURCE_ISSUE: &str = "AUTHORED_SOURCE_LAYOUT";

fn source() -> SourceScope {
    SourceScope {
        source_id: "authored-sentence-source".into(),
        source_sha256: sha256(b"Authored sentence fixture; not a PDF."),
        display_name: "Authored sentence fixture".into(),
        byte_length: 512,
        physical_pages: 7,
        scope_schema_version: Some(1),
        page_ranges: Some(vec![PageRange {
            start_page: 7,
            end_page: 7,
        }]),
    }
}

fn page(text: &str) -> Value {
    json!({
        "source_id":source().source_id, "page_number":7, "width_pt":612.0,
        "height_pt":792.0, "status":"text_ready", "text":text,
        "issues":[{"code":SOURCE_ISSUE,"message":"Authored layout sentinel.","original":true}]
    })
}

fn assert_coverage(text: &str, max: usize, units: &[Unit], page_sha: &str) {
    assert!(!units.is_empty());
    let scope = source();
    let mut cursor = 0;
    let mut identities = std::collections::BTreeSet::new();
    for unit in units {
        assert_eq!(unit.start_byte, cursor);
        assert!(unit.start_byte < unit.end_byte);
        assert!(unit.end_byte - unit.start_byte <= max);
        assert!(text.is_char_boundary(unit.start_byte));
        assert!(text.is_char_boundary(unit.end_byte));
        assert_eq!(unit.text, text[unit.start_byte..unit.end_byte]);
        assert_eq!(unit.source_id, scope.source_id);
        assert_eq!(unit.source_sha256, scope.source_sha256);
        assert_eq!(unit.page_sha256, page_sha);
        assert_eq!(unit.page_number, 7);
        assert_eq!(unit.bbox, [0.0, 0.0, 612.0, 792.0]);
        assert_eq!(unit.source_status, "text_ready");
        assert!(unit.issue_codes.iter().any(|code| code == SOURCE_ISSUE));
        assert!(identities.insert(&unit.unit_id));
        cursor = unit.end_byte;
    }
    assert_eq!(cursor, text.len());
    assert_eq!(
        units
            .iter()
            .map(|unit| unit.text.as_str())
            .collect::<String>(),
        text
    );
}

fn split(text: &str, max: usize, version: u32) -> (Vec<Unit>, Vec<Value>) {
    let page = page(text);
    let page_sha = sha256(&serde_json::to_vec(&page).unwrap());
    let result = chunks(&source(), &page, &page_sha, max, version).unwrap();
    assert_coverage(text, max, &result.0, &page_sha);
    result
}

fn assert_forced_cuts(units: &[Unit], issues: &[Value], expected: &[usize]) {
    assert_eq!(issues.len(), expected.len());
    for (issue, &cut) in issues.iter().zip(expected) {
        assert_eq!(issue["code"], "COMPILE_FRAGMENT_BOUNDARY");
        assert!(!issue["message"].as_str().unwrap().is_empty());
        assert_eq!(issue["source_id"], source().source_id);
        assert_eq!(issue["page_number"], 7);
        assert_eq!(issue["page_sha256"], units[0].page_sha256);
        assert_eq!(issue["cut_byte"], cut);
        assert!(units.iter().any(|unit| unit.end_byte == cut));
        assert!(cut < units.last().unwrap().end_byte);
    }
}

fn config(version: Option<u32>) -> CompileConfig {
    CompileConfig {
        schema_version: 1,
        endpoint: "http://127.0.0.1:1/v1/chat/completions".into(),
        model: MODEL.into(),
        credential_env: "NOETOME_TEST_SENTENCE_KEY".into(),
        credential_mode: None,
        reasoning_effort: None,
        transport_mode: "local_http_test".into(),
        usage_ledger: "authored-sentence-unused.usage.json".into(),
        max_chunk_bytes: 2048,
        max_output_tokens: 1024,
        request_timeout_ms: 1000,
        request_strategy_version: version,
    }
}

fn prepare(unit: &Unit, version: u32) -> provider::Prepared {
    provider::prepare(
        "authored-sentence-project",
        "authored-sentence-run",
        REQUEST,
        unit,
        &config(Some(version)),
        &[],
    )
    .unwrap()
}

fn body_and_input(prepared: &provider::Prepared) -> (Value, Value) {
    let body: Value = serde_json::from_slice(&prepared.body).unwrap();
    let input = serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
    (body, input)
}

fn atom(quote: &str) -> Value {
    json!({
        "concept":"自造句末概念", "kind":"statement", "content":"自造、尚未科学核验的解释。",
        "qualifiers":[], "evidence":[{"relation":"supports","quote":quote}], "measurement":null
    })
}

fn decode(prepared: &provider::Prepared, value: Value) -> Result<ApiCandidates, ProjectError> {
    let response = json!({
        "id":"authored-sentence-response", "object":"chat.completion", "model":MODEL,
        "choices":[{"index":0,"finish_reason":"stop","message":{
            "role":"assistant","content":serde_json::to_string(&value).unwrap()}}],
        "usage":{"prompt_tokens":30,"completion_tokens":20,"total_tokens":50}
    });
    provider::decode(&serde_json::to_vec(&response).unwrap(), prepared, REQUEST)
}

#[test]
fn sentence_v5_preserves_subject_year_and_condition_across_visual_newlines() {
    let premise = "A prior complete statement fixes the premise.";
    for remainder in [
        " Researcher Ada\nLovelace recorded an authored result only within the explicitly stated experimental assumptions.",
        " The observation in the year\n2042 fixes an authored context for interpreting the reported value under explicitly stated assumptions.",
        " The result holds if pressure is fixed\nand temperature remains constant throughout every authored measurement of the carefully defined comparison.",
    ] {
        let text = format!("{premise}{remainder}");
        let max = remainder.len();
        let newline = text.find('\n').unwrap();
        assert!(newline > max / 2 && newline < max);
        let (units, issues) = split(&text, max, SENTENCE_REQUEST_STRATEGY);
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].text, premise);
        assert_eq!(units[1].text, remainder);
        assert_eq!(units[1].start_byte, premise.len());
        assert!(issues.is_empty());
        let (legacy, legacy_issues) = split(&text, max, LEGACY_REQUEST_STRATEGY);
        assert_eq!(legacy[0].text, text[..newline + 1]);
        assert!(legacy_issues.is_empty());
    }
}

#[test]
fn sentence_v5_chooses_the_latest_complete_sentence_before_the_limit() {
    let first = "One condition holds.";
    let second = " Another full\nsentence finishes!";
    let tail = " A later sentence remains intact.";
    let complete = format!("{first}{second}");
    let text = format!("{complete}{tail}");
    let (units, issues) = split(&text, complete.len() + 7, SENTENCE_REQUEST_STRATEGY);
    assert_eq!(units.len(), 2);
    assert_eq!(units[0].text, complete);
    assert_eq!(units[1].text, tail);
    assert!(issues.is_empty());
}

#[test]
fn sentence_v5_distinguishes_section_decimal_and_abbreviation_from_sentence_end() {
    let complete = "Section 1.1 uses 3.14 while Dr. A. North considers e.g. fixed controls without changing the condition.";
    let text = format!("{complete} A separate complete sentence follows.");
    let (units, issues) = split(&text, complete.len() + 8, SENTENCE_REQUEST_STRATEGY);
    assert_eq!(units[0].text, complete);
    assert!(issues.is_empty());

    // This window includes the numbers and abbreviations but no complete sentence.
    let hard_end = complete.find(" fixed controls").unwrap() + 6;
    let (units, issues) = split(&text, hard_end, SENTENCE_REQUEST_STRATEGY);
    assert_eq!(units[0].end_byte, hard_end);
    assert_eq!(units[0].text, text[..hard_end]);
    assert_eq!(issues[0]["cut_byte"], hard_end);
    assert_eq!(issues[0]["code"], "COMPILE_FRAGMENT_BOUNDARY");
}

#[test]
fn sentence_v5_long_fragment_covers_all_utf8_bytes_and_reports_every_forced_cut() {
    let text = "无句末🙂αβ gamma delta\n\tcontinues without punctuation ".repeat(7);
    for max in [4, 7, 13, 32, 79] {
        let (units, issues) = split(&text, max, SENTENCE_REQUEST_STRATEGY);
        let expected = units[..units.len() - 1]
            .iter()
            .map(|unit| unit.end_byte)
            .collect::<Vec<_>>();
        assert!(!expected.is_empty());
        assert_forced_cuts(&units, &issues, &expected);
    }
}

#[test]
fn sentence_v5_fallback_reports_actual_newline_cuts_instead_of_hard_limits() {
    let (units, issues) = split(
        "abcdefghij\nklmnopqrstu\nvwxyz",
        16,
        SENTENCE_REQUEST_STRATEGY,
    );
    assert_eq!(
        units.iter().map(|unit| unit.end_byte).collect::<Vec<_>>(),
        [11, 23, 28]
    );
    assert_forced_cuts(&units, &issues, &[11, 23]);
    assert_ne!(issues[0]["cut_byte"], 16);
    assert_eq!(units[0].issue_codes, [SOURCE_ISSUE]);
}

#[test]
fn sentence_v5_keeps_preexisting_page_issues_separate_from_new_cut_diagnostics() {
    let mut page = page("abcdefghij\nklmnopqrstu\nvwxyz");
    page["issues"].as_array_mut().unwrap().push(json!({
        "code":"COMPILE_FRAGMENT_BOUNDARY", "message":"Preexisting source issue.",
        "source_id":"authored-original-sentinel", "cut_byte":999
    }));
    let original = page.clone();
    let page_sha = sha256(&serde_json::to_vec(&page).unwrap());
    let (units, issues) =
        chunks(&source(), &page, &page_sha, 16, SENTENCE_REQUEST_STRATEGY).unwrap();
    assert_coverage(page["text"].as_str().unwrap(), 16, &units, &page_sha);
    assert_forced_cuts(&units, &issues, &[11, 23]);
    assert_eq!(page, original);
    for unit in units {
        assert_eq!(
            unit.issue_codes,
            [SOURCE_ISSUE, "COMPILE_FRAGMENT_BOUNDARY"]
        );
    }
}

#[test]
fn legacy_v1_to_v4_keep_golden_chunk_bytes_offsets_metadata_and_empty_diagnostics() {
    let fixtures: &[(&str, usize, &[(usize, usize, &str)])] = &[
        (
            "abcdefghij\nklmnopqrstu\nvwxyz",
            16,
            &[
                (0, 11, "abcdefghij\n"),
                (11, 23, "klmnopqrstu\n"),
                (23, 28, "vwxyz"),
            ],
        ),
        (
            "abcdefgh\nijklmnop",
            16,
            &[(0, 16, "abcdefgh\nijklmno"), (16, 17, "p")],
        ),
        (
            "甲乙🙂\n丙丁戊己",
            16,
            &[(0, 11, "甲乙🙂\n"), (11, 23, "丙丁戊己")],
        ),
    ];
    for &(text, max, expected) in fixtures {
        let page_sha = sha256(&serde_json::to_vec(&page(text)).unwrap());
        let scope = source();
        let golden = expected
            .iter()
            .map(|&(start, end, text)| {
                json!({
                    "source_id":scope.source_id,"source_sha256":scope.source_sha256,
                    "page_number":7,"page_sha256":page_sha,"bbox":[0.0,0.0,612.0,792.0],
                    "source_status":"text_ready","start_byte":start,"end_byte":end,
                    "text":text,"issue_codes":[SOURCE_ISSUE]
                })
            })
            .collect::<Vec<_>>();
        let golden_bytes = serde_json::to_vec(&golden).unwrap();
        for version in 1..=4 {
            let (units, issues) = split(text, max, version);
            let actual = units
                .iter()
                .map(|unit| {
                    let mut value = serde_json::to_value(unit).unwrap();
                    value.as_object_mut().unwrap().remove("unit_id");
                    value
                })
                .collect::<Vec<_>>();
            assert_eq!(serde_json::to_vec(&actual).unwrap(), golden_bytes);
            assert!(issues.is_empty());
        }
    }
}

#[test]
fn sentence_and_legacy_keep_empty_page_and_too_small_utf8_limit_contracts() {
    for version in 1..=5 {
        for text in ["", " \n\t\r\n"] {
            let (units, issues) =
                chunks(&source(), &json!({"text":text}), "unused", 0, version).unwrap();
            assert!(units.is_empty());
            assert!(issues.is_empty());
        }
        for max in 0..4 {
            let error = chunks(&source(), &page("🙂"), "authored-page", max, version).unwrap_err();
            assert_eq!(error.code(), "COMPILE_UTF8");
        }
        let (units, issues) = split("🙂", 4, version);
        assert_eq!(units.len(), 1);
        assert!(issues.is_empty());
    }
}

#[test]
fn sentence_v5_is_explicitly_pinned_while_historical_missing_strategy_stays_v1() {
    assert_eq!(SENTENCE_REQUEST_STRATEGY, 5);
    assert_eq!(ANCHOR_REQUEST_STRATEGY, 3);
    for version in [None, Some(1), Some(2), Some(3), Some(4), Some(5)] {
        let config = config(version);
        let bytes = serde_json::to_vec(&config).unwrap();
        let restored: CompileConfig = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.request_strategy_version, version);
        assert_eq!(restored.strategy_version().unwrap(), version.unwrap_or(1));
        assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
    }
    for version in [0, 6, 99, u32::MAX] {
        assert_eq!(
            config(Some(version)).strategy_version().unwrap_err().code(),
            "COMPILE_REQUEST_STRATEGY"
        );
    }
}

#[test]
fn sentence_v5_compact_request_differs_from_v4_only_in_fixed_system_instruction() {
    let text = "\n A complete authored sentence.  \nFragment without its subject";
    let (units, issues) = split(text, text.len(), SENTENCE_REQUEST_STRATEGY);
    assert!(issues.is_empty());
    let old = prepare(&units[0], COMPACT_REQUEST_STRATEGY);
    let new = prepare(&units[0], SENTENCE_REQUEST_STRATEGY);
    let (mut old_body, old_input) = body_and_input(&old);
    let (mut new_body, new_input) = body_and_input(&new);
    assert_eq!(old.strategy_version, 4);
    assert_eq!(new.strategy_version, 5);
    assert_eq!(new_input, json!({"request_id":REQUEST,"source":text}));
    assert_eq!(old_input, new_input);
    assert_eq!(new_body["messages"].as_array().unwrap().len(), 2);
    let instruction = new_body["messages"][0]["content"].as_str().unwrap();
    assert!(instruction.contains("片段残句缺主语、条件或比较对象时略过该主张"));
    assert!(instruction.contains("不把片段截断写成原文或科学上的未知"));
    assert!(!instruction.contains("response_schema"));
    assert_ne!(old.body, new.body);
    assert_ne!(sha256(&old.body), sha256(&new.body));
    old_body["messages"][0]["content"] = Value::Null;
    new_body["messages"][0]["content"] = Value::Null;
    assert_eq!(
        serde_json::to_vec(&old_body).unwrap(),
        serde_json::to_vec(&new_body).unwrap()
    );
    assert_eq!(old.binding, new.binding);
    assert_eq!(
        serde_json::to_value(&old.units).unwrap(),
        serde_json::to_value(&new.units).unwrap()
    );
}

#[test]
fn sentence_v5_chunks_prepare_and_decode_keep_visual_newline_quote_and_absolute_utf8_anchor() {
    let prefix = "前置完整句。";
    let sentence =
        " Authored investigator Ada\nLovelace measured 12 m·s⁻¹ only at 37 °C using v = Δx/Δt.";
    let text = format!("{prefix}{sentence} A trailing fragment without a subject");
    let (units, issues) = split(&text, sentence.len(), SENTENCE_REQUEST_STRATEGY);
    assert!(issues.is_empty());
    assert_eq!(units[0].text, prefix);
    assert_eq!(units[1].text, sentence);
    let prepared = prepare(&units[1], SENTENCE_REQUEST_STRATEGY);
    assert_eq!(body_and_input(&prepared).1["source"], sentence);
    let quote = "Ada\nLovelace measured 12 m·s⁻¹ only at 37 °C";
    let mut claim = atom(quote);
    claim["kind"] = json!("comparison_value");
    claim["content"] = json!(
        "自造研究者 Ada Lovelace 仅在 37 °C 条件下报告 12 m·s⁻¹，采用 v = Δx/Δt；此内容尚未科学核验。"
    );
    claim["qualifiers"] = json!(["37 °C", "m·s⁻¹"]);
    claim["measurement"] =
        json!({"value_status":"known","value":"12","unit":"m·s⁻¹","conditions":["37 °C"]});
    claim["evidence"]
        .as_array_mut()
        .unwrap()
        .push(json!({"relation":"supports","quote":"v = Δx/Δt"}));
    let decoded = decode(&prepared, json!({"request_id":REQUEST,"atoms":[claim]})).unwrap();
    assert_eq!(decoded.binding, prepared.binding);
    assert_eq!(decoded.atoms.len(), 1);
    assert_eq!(decoded.evidence.len(), 2);
    assert_eq!(decoded.atoms[0].atom.verification, Verification::Unverified);
    assert_eq!(decoded.atoms[0].atom.value_status, ValueStatus::Known);
    assert!(!decoded.atoms[0].atom.required);
    assert_eq!(
        decoded.atoms[0].atom.evidence_links[0].relation,
        EvidenceRelation::Supports
    );
    assert_eq!(
        decoded.atoms[0].measurement.as_ref().unwrap().conditions,
        ["37 °C"]
    );
    let evidence = &decoded.evidence[0];
    let start = prefix.len() + sentence.find(quote).unwrap();
    assert_eq!(evidence.quote, quote);
    assert_eq!(evidence.excerpt_start_byte, start);
    assert_eq!(evidence.excerpt_end_byte, start + quote.len());
    assert_eq!(
        &text.as_bytes()[start..start + quote.len()],
        quote.as_bytes()
    );
    assert_eq!(evidence.excerpt_sha256, sha256(quote.as_bytes()));
}

#[test]
fn sentence_v5_unique_quote_gate_rejects_ambiguity_overlap_and_newline_normalization() {
    for (text, quote) in [
        ("ab / ab / x", "ab"),
        ("aaaa", "aaa"),
        ("中中中", "中中"),
        ("🙂🙂🙂", "🙂🙂"),
        ("Ada\nLovelace authored a statement.", "Ada Lovelace"),
        ("The reported value is 12 m·s⁻¹.", "12 m/s"),
    ] {
        let (units, _) = split(text, text.len(), SENTENCE_REQUEST_STRATEGY);
        let prepared = prepare(&units[0], SENTENCE_REQUEST_STRATEGY);
        let error = decode(
            &prepared,
            json!({"request_id":REQUEST,"atoms":[atom(quote)]}),
        )
        .unwrap_err();
        assert_eq!(error.code(), "COMPILE_EVIDENCE");
    }
}

#[test]
fn sentence_v5_page_tail_empty_candidates_do_not_synthesize_scientific_unknowns() {
    let tail = "under the incompletely supplied condition";
    let (units, issues) = split(tail, tail.len(), SENTENCE_REQUEST_STRATEGY);
    assert!(issues.is_empty());
    let prepared = prepare(&units[0], SENTENCE_REQUEST_STRATEGY);
    let decoded = decode(&prepared, json!({"request_id":REQUEST,"atoms":[]})).unwrap();
    assert_eq!(decoded.binding, prepared.binding);
    assert!(decoded.concepts.is_empty());
    assert!(decoded.evidence.is_empty());
    assert!(decoded.atoms.is_empty());
}

#[test]
fn sentence_v5_serialized_config_and_units_rebuild_identical_request_and_candidate_bytes() {
    // This pins pure reconstruction, not Store crash recovery or budget settlement.
    let text = "原始前提。 Authored Ada\nLovelace retains a unique claim at 37 °C.";
    let (units, _) = split(text, text.len(), SENTENCE_REQUEST_STRATEGY);
    let saved_config = serde_json::to_vec(&config(Some(5))).unwrap();
    let saved_unit = serde_json::to_vec(&units[0]).unwrap();
    let restored_config: CompileConfig = serde_json::from_slice(&saved_config).unwrap();
    let restored_unit: Unit = serde_json::from_slice(&saved_unit).unwrap();
    let original = prepare(&units[0], SENTENCE_REQUEST_STRATEGY);
    let restored = provider::prepare(
        "authored-sentence-project",
        "authored-sentence-run",
        REQUEST,
        &restored_unit,
        &restored_config,
        &[],
    )
    .unwrap();
    assert_eq!(restored.strategy_version, 5);
    assert_eq!(restored.body, original.body);
    assert_eq!(sha256(&restored.body), sha256(&original.body));
    assert_eq!(restored.binding, original.binding);
    assert_eq!(serde_json::to_vec(&restored_unit).unwrap(), saved_unit);
    let value =
        json!({"request_id":REQUEST,"atoms":[atom("Ada\nLovelace retains a unique claim")]});
    assert_eq!(
        serde_json::to_vec(&decode(&original, value.clone()).unwrap()).unwrap(),
        serde_json::to_vec(&decode(&restored, value).unwrap()).unwrap()
    );
    let old = prepare(&restored_unit, COMPACT_REQUEST_STRATEGY);
    assert_ne!(restored.body, old.body);
}
