//! Independent model candidate v1 contracts. Candidate validity grants no review or release.

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

pub const PROTOCOL: &str = "noetome-model-single-v1";

macro_rules! model {
    ($name:ident { $($(#[$attribute:meta])* $field:ident : $kind:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $($(#[$attribute])* pub $field: $kind),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $($(#[$attribute])* $field: $kind),* }
                let fields: Fields = crate::pdf_model::deserialize_object(deserializer)?;
                Ok(Self { $($field: fields.$field),* })
            }
        }
    };
}
macro_rules! enumeration {
    ($name:ident { $($variant:ident),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(rename_all = "snake_case")]
                enum Wire { $($variant),* }
                let name = String::deserialize(deserializer)?;
                let wire = Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(name))?;
                Ok(match wire { $(Wire::$variant => Self::$variant),* })
            }
        }
    };
}

enumeration!(Producer { Fake, Model });
enumeration!(SourceUnitKind { TextSpan });
enumeration!(OutcomeStatus { Complete, Failed });
enumeration!(UsageProvenance {
    Simulated,
    ProviderReported
});
enumeration!(CostStatus { Known, Unknown });
enumeration!(CostBasis {
    ProviderReported,
    PriceEstimate,
    NotAvailable
});
enumeration!(Failure {
    Authentication,
    RateLimit,
    Timeout,
    CapabilityInsufficient,
    RemoteUnknown
});
enumeration!(Verification { Unverified });

model!(ProviderIdentity {
    producer: Producer,
    provider_id: String,
    adapter_version: String,
    requested_model: String
});
model!(SourceSlice {
    project_id: String, source_id: String, version_id: String, revision: u64,
    raw_sha256: String, normalized_sha256: String, normalization_version: String,
    parser_version: String, units: Vec<SourceUnit>
});
model!(SourceUnit {
    kind: SourceUnitKind,
    span_id: String,
    ordinal: u64,
    start_byte: u64,
    end_byte: u64,
    text_sha256: String,
    text: String
});
model!(Policy {
    max_input_bytes: u64,
    max_response_bytes: u64,
    max_output_tokens: u64,
    timeout_ms: u64,
    max_calls: u64,
    retries: u64,
    search_enabled: bool,
    tools_enabled: bool
});
impl Default for Policy {
    fn default() -> Self {
        Self {
            max_input_bytes: 8192,
            max_response_bytes: 524288,
            max_output_tokens: 1024,
            timeout_ms: 30000,
            max_calls: 1,
            retries: 0,
            search_enabled: false,
            tools_enabled: false,
        }
    }
}
model!(ExtractionRequest {
    schema_version: u32,
    protocol: String,
    request_id: String,
    provider: ProviderIdentity,
    source: SourceSlice,
    policy: Policy
});
model!(TextEvidence {
    span_id: String,
    excerpt_start_byte: u64,
    excerpt_end_byte: u64,
    excerpt_sha256: String
});
model!(Candidate {
    ordinal: u64, statement: String, qualifiers: Vec<String>, evidence: Vec<TextEvidence>
});
model!(Cost {
    status: CostStatus,
    #[serde(deserialize_with = "crate::pdf_model::required_option")] amount_microunits: Option<u64>,
    #[serde(deserialize_with = "crate::pdf_model::required_option")] currency: Option<String>,
    basis: CostBasis
});
model!(Usage {
    provenance: UsageProvenance,
    #[serde(deserialize_with = "crate::pdf_model::required_option")] input_tokens: Option<u64>,
    #[serde(deserialize_with = "crate::pdf_model::required_option")] output_tokens: Option<u64>,
    cost: Cost
});
model!(Outcome {
    schema_version: u32, protocol: String, request_id: String, request_sha256: String,
    provider: ProviderIdentity, actual_model: String, status: OutcomeStatus,
    candidates: Vec<Candidate>, usage: Usage,
    #[serde(deserialize_with = "crate::pdf_model::required_option")] failure: Option<Failure>
});
model!(CandidateAtom {
    atom_id: String,
    verification: Verification,
    proposal: Candidate
});
model!(CandidateIr {
    schema_version: u32, producer: Producer, candidate_id: String, revision: u64,
    verification: Verification, request_id: String, request_sha256: String,
    response_sha256: String, source: SourceSlice, atoms: Vec<CandidateAtom>
});

fn uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| {
        id.get_version_num() == 4
            && id.get_variant() == uuid::Variant::RFC4122
            && id.to_string() == value
    })
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn nonempty(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum && !value.contains('\0')
}
fn identity(value: &str, maximum: usize) -> bool {
    nonempty(value, maximum) && !value.chars().any(char::is_control)
}

/// Validate the bounded request before any provider invocation or output creation.
/// Registry and full-span provenance are separately checked by the source loader.
pub fn validate_request(request: &ExtractionRequest) -> Result<(), &'static str> {
    let source = &request.source;
    let policy = &request.policy;
    let maximum = Policy::default();
    if request.schema_version != 1
        || request.protocol != PROTOCOL
        || !uuid(&request.request_id)
        || !identity(&request.provider.provider_id, 128)
        || !identity(&request.provider.adapter_version, 128)
        || !identity(&request.provider.requested_model, 256)
        || !uuid(&source.project_id)
        || !uuid(&source.source_id)
        || !uuid(&source.version_id)
        || source.revision == 0
        || !hash(&source.raw_sha256)
        || !hash(&source.normalized_sha256)
        || source.normalization_version != crate::text::NORMALIZATION_VERSION
        || source.parser_version != crate::text::PARSER_VERSION
        || source.units.is_empty()
        || policy.max_input_bytes == 0
        || policy.max_input_bytes > maximum.max_input_bytes
        || policy.max_response_bytes == 0
        || policy.max_response_bytes > maximum.max_response_bytes
        || policy.max_output_tokens == 0
        || policy.max_output_tokens > maximum.max_output_tokens
        || policy.timeout_ms == 0
        || policy.timeout_ms > maximum.timeout_ms
        || policy.max_calls != 1
        || policy.retries != 0
        || policy.search_enabled
        || policy.tools_enabled
    {
        return Err("MODEL_REQUEST_INVALID");
    }
    let mut spans = BTreeSet::new();
    let mut previous: Option<(u64, u64)> = None;
    let mut total = 0u64;
    for unit in &source.units {
        let length = u64::try_from(unit.text.len()).map_err(|_| "MODEL_REQUEST_INVALID")?;
        if !uuid(&unit.span_id)
            || !spans.insert(&unit.span_id)
            || unit.text.is_empty()
            || unit.end_byte.checked_sub(unit.start_byte) != Some(length)
            || !hash(&unit.text_sha256)
            || crate::text::sha256(unit.text.as_bytes()) != unit.text_sha256
            || previous
                .is_some_and(|(ordinal, end)| unit.ordinal <= ordinal || unit.start_byte < end)
        {
            return Err("MODEL_REQUEST_INVALID");
        }
        total = total.checked_add(length).ok_or("MODEL_REQUEST_INVALID")?;
        if total > policy.max_input_bytes {
            return Err("MODEL_REQUEST_INVALID");
        }
        previous = Some((unit.ordinal, unit.end_byte));
    }
    // This fixed request wire ceiling also bounds metadata and unit inventories.
    let wire = serde_json::to_vec(request).map_err(|_| "MODEL_REQUEST_INVALID")?;
    if wire.len() > 65536 {
        return Err("MODEL_REQUEST_INVALID");
    }
    Ok(())
}

fn validate_usage(usage: &Usage, request: &ExtractionRequest) -> Result<(), &'static str> {
    let provenance = match request.provider.producer {
        Producer::Fake => UsageProvenance::Simulated,
        Producer::Model => UsageProvenance::ProviderReported,
    };
    if usage.provenance != provenance {
        return Err("MODEL_USAGE_INVALID");
    }
    if !valid_cost(&usage.cost) {
        return Err("MODEL_USAGE_INVALID");
    }
    Ok(())
}
pub(crate) fn valid_cost(cost: &Cost) -> bool {
    match cost.status {
        CostStatus::Known => {
            cost.amount_microunits.is_some()
                && cost.currency.as_deref().is_some_and(|currency| {
                    currency.len() == 3 && currency.bytes().all(|byte| byte.is_ascii_uppercase())
                })
                && cost.basis != CostBasis::NotAvailable
        }
        CostStatus::Unknown => {
            cost.amount_microunits.is_none()
                && cost.currency.is_none()
                && cost.basis == CostBasis::NotAvailable
        }
    }
}

fn validate_envelope(
    outcome: &Outcome,
    request: &ExtractionRequest,
    request_sha: &str,
) -> Result<(), &'static str> {
    validate_request(request)?;
    if !hash(request_sha)
        || outcome.schema_version != 1
        || outcome.protocol != PROTOCOL
        || outcome.request_id != request.request_id
        || outcome.request_sha256 != request_sha
        || outcome.provider != request.provider
        || !identity(&outcome.actual_model, 256)
    {
        return Err("MODEL_RESPONSE_INVALID");
    }
    validate_usage(&outcome.usage, request)
}

fn validate_outcome(
    outcome: &Outcome,
    request: &ExtractionRequest,
    request_sha: &str,
) -> Result<(), &'static str> {
    validate_envelope(outcome, request, request_sha)?;
    if outcome
        .usage
        .output_tokens
        .is_some_and(|tokens| tokens > request.policy.max_output_tokens)
    {
        return Err("MODEL_USAGE_INVALID");
    }
    match outcome.status {
        OutcomeStatus::Failed => {
            if outcome.failure.is_none() || !outcome.candidates.is_empty() {
                return Err("MODEL_RESPONSE_INVALID");
            }
        }
        OutcomeStatus::Complete => {
            if outcome.failure.is_some()
                || outcome.candidates.is_empty()
                || outcome.candidates.len() > 256
            {
                return Err("MODEL_RESPONSE_INVALID");
            }
            for (ordinal, candidate) in outcome.candidates.iter().enumerate() {
                if candidate.ordinal != ordinal as u64
                    || !nonempty(&candidate.statement, 4096)
                    || candidate.qualifiers.len() > 32
                    || candidate
                        .qualifiers
                        .iter()
                        .any(|qualifier| !nonempty(qualifier, 256))
                    || candidate.evidence.is_empty()
                    || candidate.evidence.len() > 8
                {
                    return Err("MODEL_RESPONSE_INVALID");
                }
                let mut excerpts = BTreeSet::new();
                for evidence in &candidate.evidence {
                    let unit = request
                        .source
                        .units
                        .iter()
                        .find(|unit| unit.span_id == evidence.span_id)
                        .ok_or("MODEL_EVIDENCE_INVALID")?;
                    let start = usize::try_from(evidence.excerpt_start_byte)
                        .map_err(|_| "MODEL_EVIDENCE_INVALID")?;
                    let end = usize::try_from(evidence.excerpt_end_byte)
                        .map_err(|_| "MODEL_EVIDENCE_INVALID")?;
                    if start >= end
                        || !hash(&evidence.excerpt_sha256)
                        || !excerpts.insert((&evidence.span_id, start, end))
                    {
                        return Err("MODEL_EVIDENCE_INVALID");
                    }
                    let excerpt = unit.text.get(start..end).ok_or("MODEL_EVIDENCE_INVALID")?;
                    if crate::text::sha256(excerpt.as_bytes()) != evidence.excerpt_sha256 {
                        return Err("MODEL_EVIDENCE_INVALID");
                    }
                }
            }
        }
    }
    Ok(())
}

/// Decode exactly one bounded named-object response, retaining no remote text in errors.
/// `request_sha` is the SHA of the actual serialized request bytes sent by Core.
pub fn decode_outcome(
    bytes: &[u8],
    request: &ExtractionRequest,
    request_sha: &str,
) -> Result<Outcome, &'static str> {
    let outcome = decode_auditable_outcome(bytes, request, request_sha)?;
    validate_outcome(&outcome, request, request_sha)?;
    Ok(outcome)
}

/// Recover a strictly identified, bounded response for audit and accounting.
/// Candidate validity and the returned token count's policy compliance are separate;
/// a rejected proposal cannot erase already reported provider usage or known cost.
pub fn decode_auditable_outcome(
    bytes: &[u8],
    request: &ExtractionRequest,
    request_sha: &str,
) -> Result<Outcome, &'static str> {
    validate_request(request)?;
    if bytes.len() as u64 > request.policy.max_response_bytes {
        return Err("MODEL_RESPONSE_LIMIT");
    }
    let outcome: Outcome = serde_json::from_slice(bytes).map_err(|_| "MODEL_RESPONSE_INVALID")?;
    validate_envelope(&outcome, request, request_sha)?;
    if outcome.candidates.len() > 256
        || outcome.candidates.iter().any(|candidate| {
            candidate.statement.len() > 4096
                || candidate.qualifiers.len() > 32
                || candidate
                    .qualifiers
                    .iter()
                    .any(|qualifier| qualifier.len() > 256)
                || candidate.evidence.len() > 8
        })
    {
        return Err("MODEL_RESPONSE_INVALID");
    }
    Ok(outcome)
}

/// Extract the same auditable usage without settling or repeating a provider call.
pub fn decode_confirmed_usage(
    bytes: &[u8],
    request: &ExtractionRequest,
    request_sha: &str,
) -> Result<Usage, &'static str> {
    decode_auditable_outcome(bytes, request, request_sha).map(|outcome| outcome.usage)
}

/// Assign fresh Core identities only after complete response and evidence validation.
pub fn make_candidate_ir(
    outcome: &Outcome,
    request: &ExtractionRequest,
    request_sha: &str,
    response_sha: &str,
) -> Result<CandidateIr, &'static str> {
    validate_outcome(outcome, request, request_sha)?;
    if outcome.status != OutcomeStatus::Complete {
        return Err("MODEL_NO_CANDIDATE");
    }
    if !hash(response_sha) {
        return Err("MODEL_IR_INVALID");
    }
    let ir = CandidateIr {
        schema_version: 1,
        producer: request.provider.producer,
        candidate_id: Uuid::new_v4().to_string(),
        revision: 1,
        verification: Verification::Unverified,
        request_id: request.request_id.clone(),
        request_sha256: request_sha.into(),
        response_sha256: response_sha.into(),
        source: request.source.clone(),
        atoms: outcome
            .candidates
            .iter()
            .map(|proposal| CandidateAtom {
                atom_id: Uuid::new_v4().to_string(),
                verification: Verification::Unverified,
                proposal: proposal.clone(),
            })
            .collect(),
    };
    validate_saved_ir(&ir, outcome, request, request_sha, response_sha)?;
    Ok(ir)
}

/// Recheck persisted candidate content against the actual validated response and source.
/// This does not authenticate self-consistent hashes or perform scientific review.
pub fn validate_saved_ir(
    ir: &CandidateIr,
    outcome: &Outcome,
    request: &ExtractionRequest,
    request_sha: &str,
    response_sha: &str,
) -> Result<(), &'static str> {
    validate_outcome(outcome, request, request_sha)?;
    if outcome.status != OutcomeStatus::Complete
        || !hash(response_sha)
        || ir.schema_version != 1
        || ir.producer != request.provider.producer
        || !uuid(&ir.candidate_id)
        || ir.revision != 1
        || ir.verification != Verification::Unverified
        || ir.request_id != request.request_id
        || ir.request_sha256 != request_sha
        || ir.response_sha256 != response_sha
        || ir.source != request.source
        || ir.atoms.len() != outcome.candidates.len()
    {
        return Err("MODEL_IR_INVALID");
    }
    let mut ids = BTreeSet::from([ir.candidate_id.as_str(), ir.request_id.as_str()]);
    if ids.len() != 2 {
        return Err("MODEL_IR_INVALID");
    }
    for (atom, proposal) in ir.atoms.iter().zip(&outcome.candidates) {
        if !uuid(&atom.atom_id)
            || !ids.insert(&atom.atom_id)
            || atom.verification != Verification::Unverified
            || &atom.proposal != proposal
        {
            return Err("MODEL_IR_INVALID");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn request() -> ExtractionRequest {
        serde_json::from_value(json!({
            "schema_version":1,"protocol":PROTOCOL,
            "request_id":"10000000-0000-4000-8000-000000000001",
            "provider":{"producer":"fake","provider_id":"offline-fake","adapter_version":"1",
                "requested_model":"reviewed-candidate-v1"},
            "source":{"project_id":"20000000-0000-4000-8000-000000000001",
                "source_id":"30000000-0000-4000-8000-000000000001",
                "version_id":"40000000-0000-4000-8000-000000000001","revision":1,
                "raw_sha256":"a".repeat(64),"normalized_sha256":"b".repeat(64),
                "normalization_version":"utf8-newlines-v1","parser_version":"blocks-v1",
                "units":[{"kind":"text_span","span_id":"50000000-0000-4000-8000-000000000001",
                    "ordinal":2,"start_byte":20,"end_byte":29,
                    "text_sha256":crate::text::sha256("alpha β\n".as_bytes()),"text":"alpha β\n"}]},
            "policy":{"max_input_bytes":8192,"max_response_bytes":524288,"max_output_tokens":1024,
                "timeout_ms":30000,"max_calls":1,"retries":0,"search_enabled":false,"tools_enabled":false}
        })).unwrap()
    }
    fn request_sha(request: &ExtractionRequest) -> String {
        crate::text::sha256(&serde_json::to_vec(request).unwrap())
    }
    #[test]
    fn currency_policy_is_shared_with_settlement() {
        let request = request();
        for currency in ["usd", "USDT", "credits"] {
            let mut wire = outcome(&request);
            wire["usage"]["cost"] = json!({"status":"known","amount_microunits":25,"currency":currency,"basis":"provider_reported"});
            let bytes = serde_json::to_vec(&wire).unwrap();
            assert_eq!(
                decode_auditable_outcome(&bytes, &request, &request_sha(&request)),
                Err("MODEL_USAGE_INVALID"),
                "{currency}"
            );
        }
    }
    fn outcome(request: &ExtractionRequest) -> Value {
        json!({"schema_version":1,"protocol":PROTOCOL,"request_id":request.request_id,
            "request_sha256":request_sha(request),"provider":request.provider,
            "actual_model":"reviewed-candidate-v1","status":"complete",
            "candidates":[{"ordinal":0,"statement":"A conditional claim.","qualifiers":["under condition β"],
                "evidence":[{"span_id":request.source.units[0].span_id,"excerpt_start_byte":6,
                    "excerpt_end_byte":8,"excerpt_sha256":crate::text::sha256("β".as_bytes())}]}],
            "usage":{"provenance":"simulated","input_tokens":null,"output_tokens":null,
                "cost":{"status":"unknown","amount_microunits":null,"currency":null,"basis":"not_available"}},
            "failure":null})
    }
    fn decode(value: &Value, request: &ExtractionRequest) -> Result<Outcome, &'static str> {
        decode_outcome(
            &serde_json::to_vec(value).unwrap(),
            request,
            &request_sha(request),
        )
    }

    #[test]
    fn valid_request_and_complete_utf8_evidence_are_accepted() {
        let req = request();
        assert_eq!(validate_request(&req), Ok(()));
        let actual = decode(&outcome(&req), &req).unwrap();
        assert_eq!(actual.candidates[0].statement, "A conditional claim.");
        assert_eq!(actual.usage.cost.amount_microunits, None);
    }

    #[test]
    fn complete_outcomes_create_distinct_core_ids_and_stay_unverified() {
        let req = request();
        let bytes = serde_json::to_vec(&outcome(&req)).unwrap();
        let result = decode_outcome(&bytes, &req, &request_sha(&req)).unwrap();
        let sha = crate::text::sha256(&bytes);
        let ir = make_candidate_ir(&result, &req, &request_sha(&req), &sha).unwrap();
        assert_eq!(ir.verification, Verification::Unverified);
        assert_eq!(ir.atoms[0].verification, Verification::Unverified);
        assert_ne!(ir.candidate_id, ir.atoms[0].atom_id);
        assert_eq!(
            validate_saved_ir(&ir, &result, &req, &request_sha(&req), &sha),
            Ok(())
        );
    }

    #[test]
    fn classified_failures_decode_without_success_candidates() {
        let req = request();
        for classification in [
            "authentication",
            "rate_limit",
            "timeout",
            "capability_insufficient",
            "remote_unknown",
        ] {
            let mut wire = outcome(&req);
            wire["status"] = json!("failed");
            wire["candidates"] = json!([]);
            wire["failure"] = json!(classification);
            let parsed = decode(&wire, &req).unwrap();
            assert_eq!(parsed.status, OutcomeStatus::Failed);
            assert!(make_candidate_ir(&parsed, &req, &request_sha(&req), &"c".repeat(64)).is_err());
        }
    }

    #[test]
    fn named_objects_and_explicit_null_are_required_at_wire_boundary() {
        let req = request();
        let mut wire = outcome(&req);
        wire.as_object_mut().unwrap().remove("failure");
        assert!(serde_json::from_slice::<Outcome>(&serde_json::to_vec(&wire).unwrap()).is_err());
        let array = json!([
            1,
            PROTOCOL,
            req.request_id,
            request_sha(&req),
            req.provider,
            "reviewed-candidate-v1",
            "complete",
            [],
            outcome(&req)["usage"],
            null
        ]);
        assert!(serde_json::from_slice::<Outcome>(&serde_json::to_vec(&array).unwrap()).is_err());
    }

    #[test]
    fn request_policy_rejects_retry_tools_excess_and_invalid_source_binding() {
        let base = request();
        for mutation in [
            "retry", "calls", "search", "tools", "input", "response", "output", "timeout", "sha",
            "range", "version", "span",
        ] {
            let mut req = base.clone();
            match mutation {
                "retry" => req.policy.retries = 1,
                "calls" => req.policy.max_calls = 2,
                "search" => req.policy.search_enabled = true,
                "tools" => req.policy.tools_enabled = true,
                "input" => req.policy.max_input_bytes = 8193,
                "response" => req.policy.max_response_bytes = 524289,
                "output" => req.policy.max_output_tokens = 1025,
                "timeout" => req.policy.timeout_ms = 30001,
                "sha" => req.source.units[0].text_sha256 = "c".repeat(64),
                "range" => req.source.units[0].end_byte = 28,
                "version" => req.source.normalization_version = "future".into(),
                "span" => req.source.units[0].span_id = "named-span".into(),
                _ => unreachable!(),
            }
            assert!(validate_request(&req).is_err(), "{mutation}");
        }
    }

    #[test]
    fn response_identity_and_evidence_fail_closed() {
        let req = request();
        for mutation in [
            "request",
            "sha",
            "model",
            "version",
            "provider",
            "empty",
            "ordinal",
            "evidence",
            "unknown-span",
            "empty-range",
            "utf8",
            "range",
            "excerpt",
        ] {
            let mut wire = outcome(&req);
            match mutation {
                "request" => wire["request_id"] = json!("10000000-0000-4000-8000-000000000002"),
                "sha" => wire["request_sha256"] = json!("c".repeat(64)),
                "model" => wire["actual_model"] = json!(""),
                "version" => wire["schema_version"] = json!(2),
                "provider" => wire["provider"]["producer"] = json!("model"),
                "empty" => wire["candidates"] = json!([]),
                "ordinal" => wire["candidates"][0]["ordinal"] = json!(1),
                "evidence" => wire["candidates"][0]["evidence"] = json!([]),
                "unknown-span" => {
                    wire["candidates"][0]["evidence"][0]["span_id"] =
                        json!("60000000-0000-4000-8000-000000000001")
                }
                "empty-range" => {
                    wire["candidates"][0]["evidence"][0]["excerpt_end_byte"] = json!(6)
                }
                "utf8" => wire["candidates"][0]["evidence"][0]["excerpt_start_byte"] = json!(7),
                "range" => wire["candidates"][0]["evidence"][0]["excerpt_end_byte"] = json!(10),
                "excerpt" => {
                    wire["candidates"][0]["evidence"][0]["excerpt_sha256"] = json!("c".repeat(64))
                }
                _ => unreachable!(),
            }
            assert!(decode(&wire, &req).is_err(), "{mutation}");
        }
    }

    #[test]
    fn model_reported_usage_and_known_cost_are_accepted() {
        let mut req = request();
        req.provider.producer = Producer::Model;
        let mut wire = outcome(&req);
        wire["usage"] = json!({"provenance":"provider_reported","input_tokens":25,"output_tokens":1024,
            "cost":{"status":"known","amount_microunits":120,"currency":"USD","basis":"provider_reported"}});
        let parsed = decode(&wire, &req).unwrap();
        assert_eq!(parsed.usage.output_tokens, Some(1024));
        assert_eq!(parsed.usage.cost.amount_microunits, Some(120));
    }

    #[test]
    fn invalid_cost_provenance_and_over_limit_output_are_rejected() {
        let req = request();
        for mutation in [
            "provenance",
            "output",
            "unknown-amount",
            "unknown-currency",
            "unknown-basis",
            "known-null",
            "known-currency",
            "known-basis",
        ] {
            let mut wire = outcome(&req);
            match mutation {
                "provenance" => wire["usage"]["provenance"] = json!("provider_reported"),
                "output" => wire["usage"]["output_tokens"] = json!(1025),
                "unknown-amount" => wire["usage"]["cost"]["amount_microunits"] = json!(0),
                "unknown-currency" => wire["usage"]["cost"]["currency"] = json!("USD"),
                "unknown-basis" => wire["usage"]["cost"]["basis"] = json!("price_estimate"),
                "known-null" => wire["usage"]["cost"]["status"] = json!("known"),
                "known-currency" => {
                    wire["usage"]["cost"] = json!({"status":"known","amount_microunits":1,"currency":" ","basis":"provider_reported"})
                }
                "known-basis" => {
                    wire["usage"]["cost"] = json!({"status":"known","amount_microunits":1,"currency":"USD","basis":"not_available"})
                }
                _ => unreachable!(),
            }
            assert!(decode(&wire, &req).is_err(), "{mutation}");
        }
    }

    #[test]
    fn bounded_response_accepts_exact_limit_but_rejects_one_more_byte() {
        let req = request();
        let mut bytes = serde_json::to_vec(&outcome(&req)).unwrap();
        bytes.resize(req.policy.max_response_bytes as usize, b' ');
        assert!(decode_outcome(&bytes, &req, &request_sha(&req)).is_ok());
        bytes.push(b' ');
        assert!(decode_outcome(&bytes, &req, &request_sha(&req)).is_err());
    }

    #[test]
    fn nested_named_objects_reject_unknown_duplicate_missing_and_array_fields() {
        let req = request();
        let base = outcome(&req);
        for pointer in [
            "",
            "/provider",
            "/candidates/0",
            "/candidates/0/evidence/0",
            "/usage",
            "/usage/cost",
        ] {
            let mut extra = base.clone();
            extra
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), json!(true));
            assert!(decode(&extra, &req).is_err(), "unknown at {pointer}");
            let node = base.pointer(pointer).unwrap();
            let original = serde_json::to_string(node).unwrap();
            let fields = node.as_object().unwrap();
            let (key, value) = fields.iter().next().unwrap();
            let pair = format!(
                "{}:{}",
                serde_json::to_string(key).unwrap(),
                serde_json::to_string(value).unwrap()
            );
            let duplicate = format!("{{{pair},{}", &original[1..]);
            let raw = serde_json::to_string(&base)
                .unwrap()
                .replacen(&original, &duplicate, 1);
            assert!(
                decode_outcome(raw.as_bytes(), &req, &request_sha(&req)).is_err(),
                "duplicate at {pointer}"
            );
            let mut array = base.clone();
            *array.pointer_mut(pointer).unwrap() = json!(fields.values().collect::<Vec<_>>());
            assert!(decode(&array, &req).is_err(), "array at {pointer}");
            for field in fields.keys() {
                let mut missing = base.clone();
                missing
                    .pointer_mut(pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                assert!(
                    serde_json::from_slice::<Outcome>(&serde_json::to_vec(&missing).unwrap())
                        .is_err(),
                    "missing {pointer}/{field}"
                );
            }
        }
    }

    #[test]
    fn malformed_empty_and_truncated_responses_never_decode() {
        let req = request();
        for raw in [
            b"".as_slice(),
            b" ",
            b"[]",
            b"{}",
            b"\xff",
            b"{\"schema_version\":1",
        ] {
            assert!(decode_outcome(raw, &req, &request_sha(&req)).is_err());
        }
        let mut raw = serde_json::to_vec(&outcome(&req)).unwrap();
        raw.pop();
        assert!(decode_outcome(&raw, &req, &request_sha(&req)).is_err());
        raw.extend_from_slice(b"} {}");
        assert!(decode_outcome(&raw, &req, &request_sha(&req)).is_err());
    }

    #[test]
    fn complete_and_failed_states_cannot_mix_candidates_and_failures() {
        let req = request();
        for (status, candidates, failure) in [
            ("complete", json!([]), json!(null)),
            (
                "complete",
                outcome(&req)["candidates"].clone(),
                json!("timeout"),
            ),
            (
                "failed",
                outcome(&req)["candidates"].clone(),
                json!("timeout"),
            ),
            ("failed", json!([]), json!(null)),
        ] {
            let mut wire = outcome(&req);
            wire["status"] = json!(status);
            wire["candidates"] = candidates;
            wire["failure"] = failure;
            assert!(decode(&wire, &req).is_err());
        }
    }

    #[test]
    fn candidate_count_statement_qualifier_and_evidence_limits_are_enforced() {
        let req = request();
        let mut wire = outcome(&req);
        wire["candidates"][0]["statement"] = json!("x".repeat(4096));
        wire["candidates"][0]["qualifiers"] = json!(vec!["q".repeat(256); 32]);
        let segments = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 4),
            (4, 5),
            (5, 6),
            (6, 8),
            (8, 9),
        ];
        wire["candidates"][0]["evidence"] = json!(segments.map(|(start,end)| {
            json!({"span_id":req.source.units[0].span_id,"excerpt_start_byte":start,"excerpt_end_byte":end,
                "excerpt_sha256":crate::text::sha256(&req.source.units[0].text.as_bytes()[start..end])})
        }));
        assert!(decode(&wire, &req).is_ok());
        for mutation in [
            "statement",
            "qualifier-bytes",
            "qualifier-count",
            "evidence-count",
        ] {
            let mut bad = wire.clone();
            match mutation {
                "statement" => bad["candidates"][0]["statement"] = json!("x".repeat(4097)),
                "qualifier-bytes" => bad["candidates"][0]["qualifiers"][0] = json!("q".repeat(257)),
                "qualifier-count" => bad["candidates"][0]["qualifiers"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("q")),
                "evidence-count" => {
                    let extra = bad["candidates"][0]["evidence"][0].clone();
                    bad["candidates"][0]["evidence"]
                        .as_array_mut()
                        .unwrap()
                        .push(extra);
                }
                _ => unreachable!(),
            }
            assert!(decode(&bad, &req).is_err(), "{mutation}");
        }
        let template = outcome(&req)["candidates"][0].clone();
        let mut list: Vec<_> = (0..256)
            .map(|ordinal| {
                let mut row = template.clone();
                row["ordinal"] = json!(ordinal);
                row
            })
            .collect();
        let mut many = outcome(&req);
        many["candidates"] = json!(list);
        assert!(decode(&many, &req).is_ok());
        let mut extra = template;
        extra["ordinal"] = json!(256);
        list.push(extra);
        many["candidates"] = json!(list);
        assert!(decode(&many, &req).is_err());
    }

    #[test]
    fn exact_input_byte_limit_and_multiple_ordered_units_are_accepted() {
        let mut req = request();
        req.source.units[0].text = "x".repeat(8192);
        req.source.units[0].end_byte = 8212;
        req.source.units[0].text_sha256 = crate::text::sha256(req.source.units[0].text.as_bytes());
        assert_eq!(validate_request(&req), Ok(()));
        req.source.units[0].text.push('x');
        req.source.units[0].end_byte += 1;
        req.source.units[0].text_sha256 = crate::text::sha256(req.source.units[0].text.as_bytes());
        assert!(validate_request(&req).is_err());
        let mut req = request();
        let mut next = req.source.units[0].clone();
        next.span_id = "50000000-0000-4000-8000-000000000002".into();
        next.ordinal = 3;
        next.start_byte = 29;
        next.end_byte = 38;
        req.source.units.push(next);
        assert_eq!(validate_request(&req), Ok(()));
        let mut reversed = req.clone();
        reversed.source.units.reverse();
        assert!(validate_request(&reversed).is_err());
        let mut duplicate = req.clone();
        duplicate.source.units[1].span_id = duplicate.source.units[0].span_id.clone();
        assert!(validate_request(&duplicate).is_err());
        req.source.units[1].start_byte = 28;
        req.source.units[1].end_byte = 37;
        assert!(validate_request(&req).is_err());
    }

    #[test]
    fn saved_ir_rejects_identity_fingerprint_source_and_proposal_mutation() {
        let req = request();
        let bytes = serde_json::to_vec(&outcome(&req)).unwrap();
        let result = decode_outcome(&bytes, &req, &request_sha(&req)).unwrap();
        let sha = crate::text::sha256(&bytes);
        let ir = make_candidate_ir(&result, &req, &request_sha(&req), &sha).unwrap();
        for mutation in [
            "candidate",
            "atom",
            "revision",
            "source",
            "request-sha",
            "response-sha",
            "proposal",
            "empty",
        ] {
            let mut bad = ir.clone();
            match mutation {
                "candidate" => bad.candidate_id = "model-generated-name".into(),
                "atom" => bad.atoms[0].atom_id = "model-generated-name".into(),
                "revision" => bad.revision = 2,
                "source" => bad.source.raw_sha256 = "c".repeat(64),
                "request-sha" => bad.request_sha256 = "c".repeat(64),
                "response-sha" => bad.response_sha256 = "c".repeat(64),
                "proposal" => bad.atoms[0].proposal.qualifiers.clear(),
                "empty" => bad.atoms.clear(),
                _ => unreachable!(),
            }
            assert!(
                validate_saved_ir(&bad, &result, &req, &request_sha(&req), &sha).is_err(),
                "{mutation}"
            );
        }
    }

    #[test]
    fn audit_retains_confirmed_model_usage_for_rejected_evidence_and_output_limit() {
        let mut req = request();
        req.provider.producer = Producer::Model;
        let mut wire = outcome(&req);
        wire["usage"] = json!({"provenance":"provider_reported","input_tokens":31,"output_tokens":1025,
            "cost":{"status":"known","amount_microunits":29,"currency":"USD","basis":"provider_reported"}});
        wire["candidates"][0]["evidence"][0]["excerpt_start_byte"] = json!(7);
        let bytes = serde_json::to_vec(&wire).unwrap();
        assert!(decode_outcome(&bytes, &req, &request_sha(&req)).is_err());
        let audited = decode_auditable_outcome(&bytes, &req, &request_sha(&req)).unwrap();
        assert_eq!(audited.usage.output_tokens, Some(1025));
        assert_eq!(audited.usage.cost.amount_microunits, Some(29));
        assert_eq!(
            decode_confirmed_usage(&bytes, &req, &request_sha(&req)).unwrap(),
            audited.usage
        );
    }

    #[test]
    fn audit_requires_strict_fields_identity_cost_and_bounded_payload() {
        let req = request();
        let wire = outcome(&req);
        let bytes = serde_json::to_vec(&wire).unwrap();
        assert!(decode_auditable_outcome(&bytes, &req, &request_sha(&req)).is_ok());
        for mutation in [
            "identity",
            "provider",
            "cost",
            "provenance",
            "statement",
            "qualifier",
            "qualifiers",
            "evidence",
            "candidates",
            "unknown",
        ] {
            let mut bad = wire.clone();
            match mutation {
                "identity" => bad["request_id"] = json!("10000000-0000-4000-8000-000000000002"),
                "provider" => bad["provider"]["provider_id"] = json!("another-provider"),
                "cost" => bad["usage"]["cost"]["amount_microunits"] = json!(0),
                "provenance" => bad["usage"]["provenance"] = json!("provider_reported"),
                "statement" => bad["candidates"][0]["statement"] = json!("x".repeat(4097)),
                "qualifier" => bad["candidates"][0]["qualifiers"][0] = json!("x".repeat(257)),
                "qualifiers" => bad["candidates"][0]["qualifiers"] = json!(vec!["x"; 33]),
                "evidence" => {
                    bad["candidates"][0]["evidence"] =
                        json!(vec![wire["candidates"][0]["evidence"][0].clone(); 9])
                }
                "candidates" => bad["candidates"] = json!(vec![wire["candidates"][0].clone(); 257]),
                "unknown" => bad["unexpected"] = json!(1),
                _ => unreachable!(),
            }
            assert!(
                decode_auditable_outcome(
                    &serde_json::to_vec(&bad).unwrap(),
                    &req,
                    &request_sha(&req)
                )
                .is_err(),
                "{mutation}"
            );
        }
        for pointer in [
            "/usage/input_tokens",
            "/usage/output_tokens",
            "/usage/cost/amount_microunits",
            "/usage/cost/currency",
            "/failure",
        ] {
            let mut bad = wire.clone();
            let (parent, field) = pointer.rsplit_once('/').unwrap();
            bad.pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                decode_auditable_outcome(
                    &serde_json::to_vec(&bad).unwrap(),
                    &req,
                    &request_sha(&req)
                )
                .is_err()
            );
        }
    }
}
