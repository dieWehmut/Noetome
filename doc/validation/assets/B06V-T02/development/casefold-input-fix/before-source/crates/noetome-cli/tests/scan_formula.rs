//! Ordinary four-path CLI through artificial HTTP; live service is never contacted by these tests.
#[path = "support/scan_api_support.rs"]
mod support;
use serde_json::{Value, json};
use std::{
    fs,
    process::{Command, Output},
    time::Duration,
};
use support::{Case, KEY, Server, read, repo, response, write};
fn run(c: &Case) -> Output {
    Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(["knowledge", "scan-formula"])
        .arg(&c.case)
        .arg(&c.api)
        .arg(&c.profile)
        .arg(&c.output)
        .env("NOETOME_TEST_SCAN_CREDENTIAL", KEY)
        .env_remove("NOETOME_TANGMAO_API_KEY")
        .output()
        .unwrap()
}
fn execute(c: &Case, s: Server) -> (Output, Vec<u8>) {
    c.activate(&s);
    let out = run(c);
    let body = s.finish();
    (out, body)
}
fn no_success(c: &Case) {
    for n in ["plan.json", "compiled.tex", "扫描公式预览.md"] {
        assert!(!c.output.join(n).exists(), "{n}");
    }
}
fn decode_base64(value: &str) -> Vec<u8> {
    // An independent decoder validates the encoder through actual received HTTP bytes.
    assert_eq!(value.len() % 4, 0);
    let mut decoded = vec![];
    for (index, block) in value.as_bytes().chunks_exact(4).enumerate() {
        let mut bits = 0u32;
        let mut padding = 0;
        for byte in block {
            let digit = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' => {
                    padding += 1;
                    0
                }
                _ => panic!("invalid base64 symbol"),
            };
            if *byte != b'=' {
                assert_eq!(padding, 0);
            }
            bits = (bits << 6) | u32::from(digit);
        }
        assert!(padding <= 2);
        if padding > 0 {
            assert_eq!(index + 1, value.len() / 4);
            assert_eq!(bits & ((1 << (padding * 8)) - 1), 0);
        }
        decoded.push((bits >> 16) as u8);
        if padding < 2 {
            decoded.push((bits >> 8) as u8);
        }
        if padding == 0 {
            decoded.push(bits as u8);
        }
    }
    decoded
}
#[test]
fn strict_four_path_help_and_argument_rules() {
    let o = Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(["knowledge", "scan-formula", "--help"])
        .output()
        .unwrap();
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("api-config.json"));
    for a in [
        vec!["knowledge", "scan-formula"],
        vec!["knowledge", "scan-formula", "a", "b", "c"],
        vec!["knowledge", "scan-formula", "a", "b", "c", "d", "e"],
        vec!["knowledge", "scan-formula", "--fake", "b", "c", "d"],
    ] {
        let o = Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(a)
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&o.stderr).contains("CLI_INVALID_ARGUMENT"));
    }
}
#[test]
fn authorized_zero_budget_has_no_key_read_or_network_and_keeps_source_audit() {
    let c = Case::new("clear");
    fs::copy(
        repo().join("fixtures/scan-formula/api-profiles/authorized-stop.json"),
        &c.api,
    )
    .unwrap();
    let o = run(&c);
    assert_eq!(o.status.code(), Some(3));
    let r = read(c.output.join("scan.json"));
    assert_eq!(r["network_calls"], 0);
    assert!(
        r["decision"]["issue_codes"]
            .as_array()
            .unwrap()
            .contains(&json!("SCAN_BUDGET_STOP"))
    );
    assert!(!c.output.join("response.bin").exists());
    no_success(&c);
    assert!(c.output.join("request-body.json").exists());
    assert_eq!(
        fs::read(c.output.join("original.png")).unwrap(),
        fs::read(c.case.join("region.png")).unwrap()
    );
}
#[test]
fn artificial_post_contains_only_exact_inline_source_and_preserves_raw_tex() {
    let c = Case::new("clear");
    let (o, body) = execute(
        &c,
        Server::new(response(Some("x_{i}=-\\frac{a_{i}}{b_{i}}"))),
    );
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(body, fs::read(c.output.join("request-body.json")).unwrap());
    let request: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(request["messages"].as_array().unwrap().len(), 2);
    assert_eq!(
        request["messages"][1]["content"][0]["image_url"]["detail"],
        "original"
    );
    let inline = request["messages"][1]["content"][0]["image_url"]["url"]
        .as_str()
        .unwrap()
        .strip_prefix("data:image/png;base64,")
        .unwrap();
    assert_eq!(
        decode_base64(inline),
        fs::read(c.case.join("region.png")).unwrap()
    );
    assert_eq!(
        request["messages"][1]["content"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        request["messages"][1]["content"][0]
            .as_object()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(request["messages"][1]["content"][0]["type"], "image_url");
    let system = request["messages"][0]["content"].as_str().unwrap();
    assert!(!system.contains("x_i"));
    assert!(!system.contains("Reported displacement"));
    let r = read(c.output.join("scan.json"));
    assert_eq!(r["network_calls"], 1);
    assert_eq!(r["paid_calls"], 0);
    assert_eq!(r["api"]["usage"]["real_model_calls"], 0);
    assert_eq!(r["api"]["usage"]["model_usage"]["total_tokens"], 30);
    assert_eq!(
        fs::read_to_string(c.output.join("recognized.tex")).unwrap(),
        "x_{i}=-\\frac{a_{i}}{b_{i}}"
    );
    assert_eq!(
        fs::read_to_string(c.output.join("compiled.tex")).unwrap(),
        "x_i=-\\frac{a_i}{b_i}"
    );
    assert_eq!(
        fs::read(c.root.join("outside-sentinel.txt")).unwrap(),
        b"human boundary bytes"
    );
}
#[test]
fn unknown_source_states_and_actual_model_uncertainty_block_success() {
    for (name, guess) in [
        ("unreadable_sign", true),
        ("unreadable_subscript", true),
        ("clear", false),
    ] {
        let c = Case::new(name);
        let o = execute(
            &c,
            Server::new(response(if guess {
                Some("x_i=-\\frac{a_i}{b_i}")
            } else {
                None
            })),
        )
        .0;
        assert_eq!(o.status.code(), Some(3));
        no_success(&c);
        assert!(c.output.join("response.bin").exists());
    }
}
#[test]
fn wrong_symbol_sign_and_extra_bar_block_without_text_repair() {
    for text in [
        "x_j=-\\frac{a_i}{b_i}",
        "x_i=\\frac{a_i}{b_i}",
        "x_i=-\\frac{\\bar{a_i}}{b_i}",
    ] {
        let c = Case::new("clear");
        let o = execute(&c, Server::new(response(Some(text)))).0;
        assert_eq!(o.status.code(), Some(3));
        no_success(&c);
        assert_eq!(
            fs::read_to_string(c.output.join("recognized.tex")).unwrap(),
            text
        );
    }
}
#[test]
fn metadata_usage_survives_length_refusal_and_invalid_transcription() {
    for mode in 0..5 {
        let c = Case::new("clear");
        let mut v = response(Some("x_i=-\\frac{a_i}{b_i}"));
        match mode{0=>v["choices"][0]["finish_reason"]=json!("length"),1=>v["choices"][0]["message"]["refusal"]=json!("refused"),2=>v["choices"][0]["message"]["content"]=json!("not JSON"),3=>v["choices"][0]["message"]["content"]=json!(json!({"schema_version":1,"recognized_tex":"x_i","uncertainties":[],"invented":true}).to_string()),_=>{let extra=v["choices"][0].clone();v["choices"].as_array_mut().unwrap().push(extra);}}
        let o = execute(&c, Server::new(v)).0;
        assert_eq!(o.status.code(), Some(3));
        no_success(&c);
        let r = read(c.output.join("scan.json"));
        assert_eq!(r["api"]["usage"]["model_usage_status"], "validated");
        assert_eq!(r["api"]["usage"]["model_usage"]["total_tokens"], 30);
        assert!(!c.output.join("recognition.json").exists());
    }
}
#[test]
fn invalid_or_over_budget_usage_never_accepts_recognition() {
    for mode in 0..4 {
        let c = Case::new("clear");
        let mut v = response(Some("x_i=-\\frac{a_i}{b_i}"));
        match mode {
            0 => {
                v.as_object_mut().unwrap().remove("usage");
            }
            1 => v["usage"]["total_tokens"] = json!(31),
            2 => {
                v["usage"]["completion_tokens"] = json!(2049);
                v["usage"]["total_tokens"] = json!(2059);
            }
            _ => v["usage"]["prompt_tokens"] = json!(10.0),
        }
        let o = execute(&c, Server::new(v)).0;
        assert_eq!(o.status.code(), Some(3));
        no_success(&c);
        assert!(c.output.join("response.bin").exists());
        assert!(!c.output.join("recognition.json").exists());
    }
}
#[test]
fn http_errors_partial_framing_and_timeouts_keep_bounded_receipts_without_retry() {
    for mode in 0..6 {
        let c = Case::new("clear");
        let (status, declared, delay) = match mode {
            0 => (401, None, Duration::ZERO),
            1 => (429, None, Duration::ZERO),
            2 => (503, None, Duration::ZERO),
            3 => (302, None, Duration::ZERO),
            4 => (200, Some(999), Duration::ZERO),
            _ => (200, Some(999), Duration::from_millis(250)),
        };
        let s = Server::http(status, b"partial".to_vec(), declared, delay);
        c.activate(&s);
        if mode == 5 {
            let mut cfg = read(&c.api);
            cfg["limits"]["timeout_ms"] = json!(100);
            write(&c.api, &cfg);
        }
        let o = run(&c);
        s.finish();
        assert_eq!(o.status.code(), Some(3));
        no_success(&c);
        let r = read(c.output.join("scan.json"));
        assert_eq!(r["network_calls"], 1);
        assert_eq!(fs::read(c.output.join("response.bin")).unwrap(), b"partial");
        assert!(r["api"]["usage"]["model_usage"].is_null());
    }
}
#[test]
fn credential_echo_nested_or_malformed_is_omitted_before_persistence() {
    for mode in 0..3 {
        let c = Case::new("clear");
        let escaped = KEY
            .chars()
            .map(|ch| format!("\\u{:04x}", ch as u32))
            .collect::<String>();
        let body = match mode {
            0 => KEY.as_bytes().to_vec(),
            1 => json!({"error":format!("{{\"nested\":\"{escaped}\"}}")})
                .to_string()
                .into_bytes(),
            _ => format!("{{\"error\":\"{escaped}\" ]").into_bytes(),
        };
        let o = execute(&c, Server::http(200, body, None, Duration::ZERO)).0;
        assert_eq!(o.status.code(), Some(3));
        let r = read(c.output.join("scan.json"));
        assert_eq!(
            r["api"]["response_receipt"]["omission_reason"],
            "omitted_credential_echo"
        );
        assert!(!c.output.join("response.bin").exists());
        assert!(!c.output.join("recognition.json").exists());
        assert!(!String::from_utf8_lossy(&o.stdout).contains(KEY));
        assert!(!String::from_utf8_lossy(&o.stderr).contains(KEY));
    }
}
#[test]
fn resource_config_inventory_collision_and_hardlink_protections_precede_network() {
    for mode in 0..6 {
        let c = Case::new("clear");
        match mode {
            0 => {
                fs::copy(
                    repo().join("fixtures/scan-formula/cases/unreadable_sign/region.png"),
                    c.case.join("region.png"),
                )
                .unwrap();
                c.repin();
            }
            1 => {
                fs::write(c.case.join("human.txt"), b"human").unwrap();
            }
            2 => {
                let mut cfg = read(&c.api);
                cfg["endpoint"] = json!("http://localhost:1234/v1/chat/completions");
                write(&c.api, &cfg);
            }
            3 => {
                let mut cfg = read(&c.api);
                cfg["budget"]
                    .as_object_mut()
                    .unwrap()
                    .remove("max_cost_usd");
                write(&c.api, &cfg);
            }
            4 => {
                fs::create_dir(&c.output).unwrap();
                fs::write(c.output.join("manual.txt"), b"human").unwrap();
            }
            _ => {
                fs::hard_link(c.case.join("region.png"), c.root.join("linked.png")).unwrap();
            }
        }
        let o = run(&c);
        assert_eq!(o.status.code(), Some(2));
        if mode == 4 {
            assert_eq!(fs::read(c.output.join("manual.txt")).unwrap(), b"human");
        } else {
            assert!(!c.output.exists());
        }
        assert_eq!(
            fs::read(c.root.join("outside-sentinel.txt")).unwrap(),
            b"human boundary bytes"
        );
    }
}
#[test]
fn capability_byte_and_money_limits_stop_before_any_post() {
    for mode in 0..4 {
        let c = Case::new("clear");
        let mut cfg = read(&c.api);
        cfg["budget"]["max_requests"] = json!(1);
        match mode {
            0 => cfg["capabilities"]["vision"] = json!(false),
            1 => cfg["limits"]["max_input_bytes"] = json!(1),
            2 => cfg["budget"]["max_cost_usd"] = json!("1.00"),
            _ => cfg["budget"]["max_total_tokens"] = json!(1),
        };
        write(&c.api, &cfg);
        let o = run(&c);
        assert_eq!(o.status.code(), Some(3));
        assert_eq!(read(c.output.join("scan.json"))["network_calls"], 0);
        no_success(&c);
    }
}
#[test]
fn strict_transcription_and_envelope_reject_missing_duplicate_and_invalid_fields() {
    for mode in 0..10 {
        let c = Case::new("clear");
        let mut v = response(Some("x_i=-\\frac{a_i}{b_i}"));
        match mode {
            0 => v["choices"][0]["message"]["content"] = json!("{\"schema_version\":1,\"recognized_tex\":\"x_i\",\"recognized_tex\":\"x_j\",\"uncertainties\":[]}"),
            1 => v["choices"][0]["message"]["content"] = json!("{\"schema_version\":1,\"uncertainties\":[]}"),
            2 => v["choices"][0]["message"]["content"] = json!("{\"schema_version\":1,\"recognized_tex\":null,\"uncertainties\":[]}"),
            3 => v["choices"][0]["message"]["tool_calls"] = json!([]),
            4 => v["choices"][0]["message"]["reasoning_content"] = json!(1),
            5 => v["choices"][0]["index"] = json!(0.0),
            6 => v["object"] = json!("invented.completion"),
            7 => v["choices"][0]["message"]["content"] = json!(json!({"schema_version":1,"recognized_tex":null,"uncertainties":[{"kind":"confidence","description":"uncalibrated"}]}).to_string()),
            8 => v["choices"][0]["message"]["content"] = json!(json!({"schema_version":1,"recognized_tex":null,"uncertainties":[{"kind":"sign","description":"a".repeat(1025)}]}).to_string()),
            _ => v["choices"][0]["message"]["content"] = json!(json!({"schema_version":1,"recognized_tex":"x".repeat(65537),"uncertainties":[]}).to_string()),
        }
        let s = Server::new(v);
        c.activate(&s);
        if mode == 9 {
            let mut cfg = read(&c.api);
            cfg["limits"]["max_response_bytes"] = json!(131072);
            write(&c.api, &cfg);
        }
        let o = run(&c);
        s.finish();
        assert_eq!(o.status.code(), Some(3));
        no_success(&c);
        assert!(!c.output.join("recognition.json").exists());
        assert!(c.output.join("response.bin").exists());
    }
}
#[test]
fn actual_response_byte_limit_retains_only_bounded_received_bytes() {
    let c = Case::new("clear");
    let s = Server::http(200, vec![b'x'; 4096], None, Duration::ZERO);
    c.activate(&s);
    let mut cfg = read(&c.api);
    cfg["limits"]["max_response_bytes"] = json!(128);
    write(&c.api, &cfg);
    let o = run(&c);
    s.finish();
    assert_eq!(o.status.code(), Some(3));
    no_success(&c);
    let r = read(c.output.join("scan.json"));
    assert_eq!(r["network_calls"], 1);
    assert_eq!(r["api"]["response_complete"], false);
    assert!(fs::read(c.output.join("response.bin")).unwrap().len() <= 128);
    assert!(!c.output.join("recognition.json").exists());
}
