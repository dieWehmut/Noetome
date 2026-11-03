//! Versioned extraction DTOs. Fake provenance is separate from fixture review.
use crate::fixture_model::{Concept, KnowledgeAtom, ValueStatus};
use serde::{Deserialize, Serialize};

fn nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    de: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(de)
}
fn schema_one<'de, D: serde::Deserializer<'de>>(de: D) -> Result<u32, D::Error> {
    let version = u32::deserialize(de)?;
    if version != 1 {
        return Err(serde::de::Error::custom(
            "unknown extraction schema_version",
        ));
    }
    Ok(version)
}
macro_rules! object {
    ($name:ident { $($(#[$attr:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct $name { $($(#[$attr])* pub(crate) $field: $ty),* }
    };
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Producer {
    Fake,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceBinding {
    pub(crate) project_id: String,
    pub(crate) import_id: String,
    pub(crate) source_id: String,
    pub(crate) version_id: String,
    pub(crate) content_sha256: String,
    #[serde(deserialize_with = "nullable")]
    pub(crate) normalized_sha256: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) parse_id: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) manifest_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Selection {
    Text { ordinal: usize },
    Pdf { page_index: u32, region_id: String },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Locator {
    Text {
        span_id: String,
        ordinal: usize,
        resource_sha256: String,
    },
    Pdf {
        parse_id: String,
        page_index: u32,
        region_id: String,
        resource_id: String,
        resource_sha256: String,
        bbox: [f64; 4],
    },
}
object!(FakeCapabilities {
    text: bool,
    structured_output: bool,
    vision: bool
});
object!(ExtractionLimits {
    max_units: usize,
    max_input_bytes: u64,
    max_response_bytes: u64,
    max_output_tokens: u64,
    timeout_ms: u64
});
object!(Budget { max_requests: u32, max_total_tokens: u64, #[serde(deserialize_with = "nullable")] max_cost_usd: Option<String> });
object!(FakeConfig {
    #[serde(deserialize_with = "schema_one")] schema_version: u32,
    producer: Producer, case_id: String, selection: Vec<Selection>, capabilities: FakeCapabilities,
    limits: ExtractionLimits, budget: Budget, delay_ms: u64, response_file: String,
    #[serde(deserialize_with = "nullable")] binding_override: Option<SourceBinding>,
    #[serde(deserialize_with = "nullable")] request_id_override: Option<String>
});
object!(RequestUnit {
    unit_id: String,
    locator: Locator,
    source_status: String,
    start_byte: usize,
    end_byte: usize,
    text: String
});
object!(ExtractionRequest {
    #[serde(deserialize_with = "schema_one")] schema_version: u32,
    producer: Producer, request_id: String, binding: SourceBinding, units: Vec<RequestUnit>, limits: ExtractionLimits, budget: Budget
});
object!(ResponseEnvelope {
    #[serde(deserialize_with = "schema_one")]
    schema_version: u32,
    producer: Producer,
    request_id: String,
    binding: SourceBinding,
    payload: CandidatePayload
});
object!(CandidatePayload {
    #[serde(deserialize_with = "schema_one")] schema_version: u32,
    producer: Producer, concepts: Vec<Concept>, evidence: Vec<CandidateEvidence>, atoms: Vec<CandidateAtom>
});
object!(CandidateEvidence {
    evidence_id: String,
    unit_id: String,
    excerpt_start_byte: usize,
    excerpt_end_byte: usize,
    excerpt_sha256: String,
    quote: String
});
object!(CandidateAtom { atom: KnowledgeAtom, #[serde(deserialize_with = "nullable")] measurement: Option<Measurement> });
object!(Measurement {
    value_status: ValueStatus,
    #[serde(deserialize_with = "nullable")] value: Option<String>,
    #[serde(deserialize_with = "nullable")] unit: Option<String>,
    conditions: Vec<String>
});

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn binding() -> serde_json::Value {
        json!({"project_id":"p", "import_id":"i", "source_id":"s", "version_id":"v",
            "content_sha256":"sha", "normalized_sha256":null,"parse_id":null,"manifest_sha256":null})
    }
    #[test]
    fn binding_requires_nullable_fields_and_rejects_unknown_fields() {
        let value = binding();
        assert!(serde_json::from_value::<SourceBinding>(value.clone()).is_ok());
        for field in ["normalized_sha256", "parse_id", "manifest_sha256"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<SourceBinding>(missing).is_err(),
                "missing {field}"
            );
        }
        let mut extra = value;
        extra["secret"] = json!("unused");
        assert!(serde_json::from_value::<SourceBinding>(extra).is_err());
    }
    #[test]
    fn payload_rejects_unknown_versions_producers_fields_duplicate_and_trailing_json() {
        let valid =
            r#"{"schema_version":1,"producer":"fake","concepts":[],"evidence":[],"atoms":[]}"#;
        assert!(serde_json::from_str::<CandidatePayload>(valid).is_ok());
        for bad in [
            valid.replace("\"schema_version\":1", "\"schema_version\":2"),
            valid.replace("\"fake\"", "\"fixture\""),
            valid.replace("\"concepts\":[]", "\"concepts\":[],\"hidden\":1"),
            valid.replace(
                "\"producer\":\"fake\"",
                "\"producer\":\"fake\",\"producer\":\"fake\"",
            ),
            format!("{valid} {{}}"),
            valid.replace(",\"evidence\":[]", ""),
        ] {
            assert!(
                serde_json::from_str::<CandidatePayload>(&bad).is_err(),
                "{bad}"
            );
        }
    }
    #[test]
    fn fake_config_and_measurement_require_explicit_nullable_fields() {
        let source = include_str!("../../../fixtures/extraction-gateway-v1/fake.json");
        let value: serde_json::Value = serde_json::from_str(source).unwrap();
        assert!(serde_json::from_value::<FakeConfig>(value.clone()).is_ok());
        for field in ["binding_override", "request_id_override"] {
            let mut v = value.clone();
            v.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<FakeConfig>(v).is_err());
        }
        let mut missing_cost = value;
        missing_cost["budget"]
            .as_object_mut()
            .unwrap()
            .remove("max_cost_usd");
        assert!(serde_json::from_value::<FakeConfig>(missing_cost).is_err());
        let m = json!({"value_status":"unknown","value":null,"unit":"ms","conditions":[]});
        assert!(serde_json::from_value::<Measurement>(m.clone()).is_ok());
        for field in ["value", "unit"] {
            let mut v = m.clone();
            v.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<Measurement>(v).is_err());
        }
        let response = include_str!("../../../fixtures/extraction-gateway-v1/response.json");
        let payload: CandidatePayload = serde_json::from_str(response).unwrap();
        let mut atom = serde_json::to_value(&payload.atoms[0]).unwrap();
        atom.as_object_mut().unwrap().remove("measurement");
        assert!(serde_json::from_value::<CandidateAtom>(atom).is_err());
    }
    #[test]
    fn selectors_and_locators_reject_mixed_or_unknown_fields() {
        for v in [
            json!({"kind":"text","ordinal":0,"page_index":0}),
            json!({"kind":"pdf","page_index":0,"region_id":"r","span_id":"s"}),
            json!({"kind":"ocr","ordinal":0}),
            json!({"kind":"pdf","page_index":0}),
        ] {
            assert!(serde_json::from_value::<Selection>(v).is_err());
        }
        assert!(serde_json::from_value::<Locator>(json!({"kind":"text","span_id":"s","ordinal":0,"resource_sha256":"h","bbox":[0,0,1,1]})).is_err());
    }
}
