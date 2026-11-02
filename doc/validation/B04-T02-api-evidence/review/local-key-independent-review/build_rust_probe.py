"""Build an isolated test of the current exact credential scan source prefix."""
import json
from pathlib import Path

root = Path(__file__).resolve().parents[2]
source_path = root / "crates/noetome-cli/src/credential_handoff.rs"
source = source_path.read_text(encoding="utf-8")
prefix = source.split("#[cfg(test)]", 1)[0]
prefix = prefix.replace("use crate::model_candidate::{ExtractionRequest, Outcome};", "")
prefix = prefix.replace("use crate::model_provider::AttemptStatus;", "")
start = source.index("fn raw_contains_secret(")
end = source.index("\n}", start) + 2
raw_match = source[start:end]
probe = Path(__file__).resolve().parent / "rust-probe"
(probe / "src").mkdir(parents=True, exist_ok=True)
(probe / "Cargo.toml").write_text('''[package]
name = "independent-credential-probe"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
serde = "=1.0.229"
serde_json = "=1.0.154"
''', encoding="utf-8")
tests = r'''
#[cfg(test)]
mod independent_review {
    use super::*;
    const SECRET: &str = "SYNTHETIC_REVIEW_KEY_20261102";
    fn probe(text: &str) -> Result<(), CredentialError> {
        let key = TransientCredential::new(SECRET.as_bytes()).unwrap();
        let raw = serde_json::to_vec(&serde_json::json!({"text": text})).unwrap();
        key.validate_no_echo(&raw)
    }
    fn escaped() -> String {
        SECRET.chars().map(|c| format!("\\u{:04x}",c as u32)).collect()
    }
    #[test]
    fn quoted_scalar_and_multiple_layers_must_suppress() {
        let scalar=format!("\"{}\"",escaped());
        let nested=serde_json::to_string(&scalar).unwrap();
        assert!(probe(&scalar).is_err());
        assert!(probe(&nested).is_err());
    }
    #[test]
    fn full_escaped_secret_in_unterminated_token_must_suppress() {
        let object=format!("{{\"value\":\"{}",escaped());
        assert!(probe(&object).is_err(), "unfinished token contains fully decodable synthetic secret");
    }
    #[test]
    fn code_fence_and_prefix_must_not_hide_closed_escaped_secret() {
        let fenced=format!("```json\n{{\"value\":\"{}\"}}\n```",escaped());
        assert!(probe(&fenced).is_err(), "code fence can carry a fully decoded synthetic secret");
    }
    #[test]
    fn natural_quoted_windows_path_without_secret_must_remain_accepted() {
        assert!(probe(r#""C:\vault\notes" is a quoted local path, not a JSON document."#).is_ok(),
                "valid outer artifact should not be rejected for natural backslash text");
    }
    #[test]
    fn ordinary_inner_quotes_without_secret_must_remain_accepted() {
        assert!(probe(r#"He said "hi" and then left."#).is_ok());
        assert!(probe(r#""He said "hi" and then left.""#).is_ok());
    }
    #[test]
    fn strict_outer_duplicate_and_nonfinite_json_must_be_rejected() {
        for raw in [br#"{"a":1,"a":2}"#.as_slice(),br#"{"a":1e999}"#,b"NaN",b"true null",br#"{"a":"x""#] {
            assert!(strict_json_value(raw).is_err());
        }
    }
    #[test]
    fn utf16_surrogate_pair_escape_with_prefix_and_no_closing_quote_must_suppress() {
        let secret="SYNTHETIC_REVIEW_KEY_😀_20261102";
        let encoded:String=secret.encode_utf16().map(|n|format!("\\u{n:04X}")).collect();
        let text=format!("prose {{\"key\":\"{encoded}");
        let raw=serde_json::to_vec(&serde_json::json!({"text":text})).unwrap();
        let key=TransientCredential::new(secret.as_bytes()).unwrap();
        assert_eq!(key.validate_no_echo(&raw).unwrap_err().code,"MODEL_CREDENTIAL_ECHO");
    }
    #[test]
    fn malformed_escape_tail_does_not_hide_already_decodable_secret() {
        let text=format!("{}\\u12",escaped());
        assert_eq!(probe(&text).unwrap_err().code,"MODEL_CREDENTIAL_ECHO");
    }
}
'''
(probe / "src/lib.rs").write_text(prefix + "\n" + raw_match + tests, encoding="utf-8")
print(json.dumps({"probe_manifest": str(probe / "Cargo.toml"), "source": str(source_path),
                  "extraction": "exact credential scan prefix and raw_contains_secret; unrelated imports omitted"}))
