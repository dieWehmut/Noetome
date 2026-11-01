//! Strict single-request protocol DTOs and byte-level response validation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize};

use crate::sidecar::SidecarLimits;

fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

fn deserialize_object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for ObjectVisitor<T> {
        type Value = T;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a JSON object with named fields")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            // Preserve the original MapAccess. Converting through Value here
            // would discard duplicate keys before the strict fields visitor.
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectVisitor(std::marker::PhantomData))
}

macro_rules! model {
    ($name:ident { $($(#[$attribute:meta])* $field:ident : $kind:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            $($(#[$attribute])* pub $field: $kind),*
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $($(#[$attribute])* $field: $kind),* }
                let fields: Fields = deserialize_object(deserializer)?;
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

enumeration!(Operation { InspectBytes });
enumeration!(Producer { Fake, Parser });
enumeration!(LocatorKind { InputBytes });
enumeration!(ArtifactKind { Binary, Utf8Text });
enumeration!(ResponseStatus {
    Complete,
    Partial,
    Failed
});
enumeration!(UnitStatus {
    Parsed,
    Failed,
    NeedsReview
});

model!(SidecarRequest {
    schema_version: u32,
    protocol: String,
    request_id: String,
    operation: Operation,
    input: InputFingerprint,
    workspace: WorkspacePolicy,
    artifact_policy: ArtifactPolicy,
});
model!(InputFingerprint {
    relative_path: String,
    byte_length: u64,
    sha256: String
});
model!(WorkspacePolicy {
    artifact_directory: String
});
model!(ArtifactPolicy {
    max_file_bytes: u64,
    max_total_bytes: u64,
    max_files: u32
});
model!(SidecarResponse {
    schema_version: u32,
    protocol: String,
    request_id: String,
    adapter: AdapterIdentity,
    input: InputFingerprint,
    status: ResponseStatus,
    #[serde(deserialize_with = "required_option")]
    unit_count: Option<u32>,
    units: Vec<Unit>,
    artifacts: Vec<Artifact>,
    issues: Vec<ReportedIssue>,
});
model!(AdapterIdentity {
    producer: Producer,
    adapter_id: String,
    adapter_version: String,
    capabilities: Capabilities,
});
model!(Capabilities { locator_kinds: Vec<LocatorKind>, artifact_kinds: Vec<ArtifactKind> });
model!(Unit {
    unit_id: String,
    ordinal: u32,
    locator: InputBytesLocator,
    status: UnitStatus,
    #[serde(deserialize_with = "required_option")]
    content: Option<ContentRef>,
});
model!(InputBytesLocator {
    kind: LocatorKind,
    start_byte: u64,
    end_byte: u64
});
model!(ContentRef {
    artifact_id: String,
    start_byte: u64,
    end_byte: u64
});
model!(Artifact {
    artifact_id: String,
    kind: ArtifactKind,
    relative_path: String,
    byte_length: u64,
    sha256: String,
});
model!(ReportedIssue { code: String, message: String, unit_ids: Vec<String> });
model!(ProtocolViolation { rule_id: String, object_refs: Vec<String>, message: String });

pub(crate) fn validate_response(
    request: &SidecarRequest,
    response: &SidecarResponse,
    expected: &AdapterIdentity,
    input_bytes: &[u8],
    artifact_bytes: &BTreeMap<String, Vec<u8>>,
    limits: &SidecarLimits,
) -> Vec<ProtocolViolation> {
    let mut issues = Vec::new();
    validate_header(
        request,
        response,
        expected,
        input_bytes,
        limits,
        &mut issues,
    );
    if response.units.len() as u64 > u64::from(limits.max_units)
        || response.issues.len() as u64 > u64::from(limits.max_issues)
    {
        push(
            &mut issues,
            "SIDECAR_UNIT_MANIFEST",
            &[],
            "单元或问题数量超过本轮上限。",
        );
        return issues;
    }
    if response.artifacts.len() as u64 > u64::from(limits.max_artifacts)
        || artifact_bytes.len() as u64 > u64::from(limits.max_artifacts)
    {
        push(
            &mut issues,
            "SIDECAR_ARTIFACT_MANIFEST",
            &[],
            "资源数量超过本轮上限。",
        );
        return issues;
    }
    let artifacts = validate_artifacts(response, artifact_bytes, limits, &mut issues);
    let (unit_ids, referenced) =
        validate_units(response, input_bytes, &artifacts, limits, &mut issues);
    for artifact in &response.artifacts {
        if !referenced.contains(artifact.artifact_id.as_str()) {
            push(
                &mut issues,
                "SIDECAR_ARTIFACT_MANIFEST",
                &[&artifact.artifact_id],
                "资源未被任何内容范围引用。",
            );
        }
    }
    validate_reported_issues(response, &unit_ids, limits, &mut issues);
    validate_status(response, &mut issues);
    issues
}

fn push(issues: &mut Vec<ProtocolViolation>, rule: &str, refs: &[&str], message: &str) {
    issues.push(ProtocolViolation {
        rule_id: rule.into(),
        object_refs: refs.iter().map(|value| (*value).to_owned()).collect(),
        message: message.into(),
    });
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn valid_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value)
        .is_ok_and(|id| id.get_version() == Some(uuid::Version::Random) && id.to_string() == value)
}

fn valid_capabilities(value: &Capabilities) -> bool {
    value.locator_kinds == [LocatorKind::InputBytes]
        && (1..=2).contains(&value.artifact_kinds.len())
        && value
            .artifact_kinds
            .iter()
            .enumerate()
            .all(|(index, kind)| !value.artifact_kinds[..index].contains(kind))
}

fn valid_adapter(value: &AdapterIdentity) -> bool {
    valid_id(&value.adapter_id)
        && !value.adapter_version.is_empty()
        && value.adapter_version.len() <= 128
        && !value.adapter_version.trim().is_empty()
        && value
            .adapter_version
            .bytes()
            .all(|byte| byte.is_ascii() && !byte.is_ascii_control())
        && valid_capabilities(&value.capabilities)
}

fn same_capabilities(left: &Capabilities, right: &Capabilities) -> bool {
    left.locator_kinds == right.locator_kinds
        && left.artifact_kinds.len() == right.artifact_kinds.len()
        && left
            .artifact_kinds
            .iter()
            .all(|kind| right.artifact_kinds.contains(kind))
}

fn validate_header(
    request: &SidecarRequest,
    response: &SidecarResponse,
    expected: &AdapterIdentity,
    input: &[u8],
    limits: &SidecarLimits,
    issues: &mut Vec<ProtocolViolation>,
) {
    if request.schema_version != 1
        || response.schema_version != 1
        || request.protocol != "noetome-parser-single-v1"
        || response.protocol != request.protocol
    {
        push(issues, "SIDECAR_VERSION", &[], "只接受固定单请求协议v1。");
    }
    if !valid_uuid(&request.request_id)
        || response.request_id != request.request_id
        || request.input.relative_path != "input/raw.bin"
        || request.workspace.artifact_directory != "artifacts"
        || input.len() as u64 > limits.max_input_bytes
        || request.input.byte_length != input.len() as u64
        || !valid_sha(&request.input.sha256)
        || (input.len() as u64 <= limits.max_input_bytes
            && request.input.sha256 != crate::text::sha256(input))
        || response.input != request.input
        || request.artifact_policy.max_file_bytes != limits.max_artifact_bytes
        || request.artifact_policy.max_total_bytes != limits.max_total_artifact_bytes
        || request.artifact_policy.max_files != limits.max_artifacts
    {
        push(
            issues,
            "SIDECAR_IDENTITY",
            &[],
            "请求身份、固定输入能力、真实原始指纹或资源政策不一致。",
        );
    }
    if !valid_adapter(expected)
        || !valid_adapter(&response.adapter)
        || response.adapter.producer != expected.producer
        || response.adapter.adapter_id != expected.adapter_id
        || response.adapter.adapter_version != expected.adapter_version
        || !same_capabilities(&response.adapter.capabilities, &expected.capabilities)
    {
        push(
            issues,
            "SIDECAR_PRODUCER",
            &[],
            "响应不能改变Host选定的producer、适配器或能力。",
        );
    }
}

struct CheckedArtifact<'a> {
    bytes: &'a [u8],
    text: Option<&'a str>,
}

fn artifact_path(artifact: &Artifact) -> Option<String> {
    if !valid_id(&artifact.artifact_id) {
        return None;
    }
    let extension = match artifact.kind {
        ArtifactKind::Binary => "bin",
        ArtifactKind::Utf8Text => "txt",
    };
    let path = format!("artifacts/{}.{}", artifact.artifact_id, extension);
    crate::fixture_input::valid_relative_path(&path).then_some(path)
}

fn validate_artifacts<'a>(
    response: &'a SidecarResponse,
    actual: &'a BTreeMap<String, Vec<u8>>,
    limits: &SidecarLimits,
    issues: &mut Vec<ProtocolViolation>,
) -> BTreeMap<&'a str, CheckedArtifact<'a>> {
    let mut checked = BTreeMap::new();
    let declared: BTreeSet<_> = response
        .artifacts
        .iter()
        .map(|artifact| artifact.relative_path.as_str())
        .collect();
    let present: BTreeSet<_> = actual.keys().map(String::as_str).collect();
    if declared != present {
        push(
            issues,
            "SIDECAR_ARTIFACT_MANIFEST",
            &[],
            "实际资源集合与声明不相等。",
        );
    }
    let total = actual
        .values()
        .try_fold(0u64, |sum, bytes| sum.checked_add(bytes.len() as u64));
    if total.is_none_or(|total| total > limits.max_total_artifact_bytes)
        || actual
            .values()
            .any(|bytes| bytes.len() as u64 > limits.max_artifact_bytes)
    {
        push(
            issues,
            "SIDECAR_ARTIFACT_INTEGRITY",
            &[],
            "实际单份或总资源字节超过上限。",
        );
        return checked;
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut declared_total = Some(0u64);
    for artifact in &response.artifacts {
        declared_total = declared_total.and_then(|sum| sum.checked_add(artifact.byte_length));
        if artifact_path(artifact).as_deref() != Some(artifact.relative_path.as_str())
            || !ids.insert(artifact.artifact_id.to_ascii_lowercase())
            || !paths.insert(artifact.relative_path.to_ascii_lowercase())
            || !response
                .adapter
                .capabilities
                .artifact_kinds
                .contains(&artifact.kind)
        {
            push(
                issues,
                "SIDECAR_ARTIFACT_MANIFEST",
                &[&artifact.artifact_id],
                "资源ID、平面路径、能力或唯一性不合法。",
            );
            continue;
        }
        let Some(bytes) = actual.get(&artifact.relative_path) else {
            continue;
        };
        let text = match artifact.kind {
            ArtifactKind::Binary => None,
            ArtifactKind::Utf8Text => match std::str::from_utf8(bytes) {
                Ok(text) => Some(text),
                Err(_) => {
                    push(
                        issues,
                        "SIDECAR_ARTIFACT_INTEGRITY",
                        &[&artifact.artifact_id],
                        "声明的UTF8资源不是有效UTF8。",
                    );
                    continue;
                }
            },
        };
        if artifact.byte_length != bytes.len() as u64
            || !valid_sha(&artifact.sha256)
            || artifact.sha256 != crate::text::sha256(bytes)
        {
            push(
                issues,
                "SIDECAR_ARTIFACT_INTEGRITY",
                &[&artifact.artifact_id],
                "资源长度或真实SHA不一致。",
            );
            continue;
        }
        checked.insert(
            artifact.artifact_id.as_str(),
            CheckedArtifact { bytes, text },
        );
    }
    if declared_total.is_none_or(|total| total > limits.max_total_artifact_bytes) {
        push(
            issues,
            "SIDECAR_ARTIFACT_INTEGRITY",
            &[],
            "声明的资源字节总数溢出或超限。",
        );
    }
    checked
}

fn range(bytes: &[u8], start: u64, end: u64) -> Option<&[u8]> {
    if start >= end {
        return None;
    }
    bytes.get(usize::try_from(start).ok()?..usize::try_from(end).ok()?)
}

fn valid_content(
    unit: &Unit,
    input: &[u8],
    artifacts: &BTreeMap<&str, CheckedArtifact<'_>>,
) -> bool {
    let Some(content) = &unit.content else {
        return unit.status != UnitStatus::Parsed;
    };
    if unit.status == UnitStatus::Failed || !valid_id(&content.artifact_id) {
        return false;
    }
    let Some(artifact) = artifacts.get(content.artifact_id.as_str()) else {
        return false;
    };
    let Some(body) = range(artifact.bytes, content.start_byte, content.end_byte) else {
        return false;
    };
    if let Some(text) = artifact.text {
        let (Ok(start), Ok(end)) = (
            usize::try_from(content.start_byte),
            usize::try_from(content.end_byte),
        ) else {
            return false;
        };
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return false;
        }
    }
    range(input, unit.locator.start_byte, unit.locator.end_byte) == Some(body)
}

fn validate_units<'a>(
    response: &'a SidecarResponse,
    input: &[u8],
    artifacts: &BTreeMap<&str, CheckedArtifact<'_>>,
    limits: &SidecarLimits,
    issues: &mut Vec<ProtocolViolation>,
) -> (BTreeSet<&'a str>, BTreeSet<&'a str>) {
    let mut ids = BTreeSet::new();
    let mut referenced = BTreeSet::new();
    let Some(count) = response.unit_count else {
        if response.status != ResponseStatus::Failed
            || !response.units.is_empty()
            || !response.artifacts.is_empty()
        {
            push(
                issues,
                "SIDECAR_UNIT_MANIFEST",
                &[],
                "只有未知库存的failed响应允许null计数和空清单。",
            );
        }
        return (ids, referenced);
    };
    if u64::from(count) != response.units.len() as u64 || count > limits.max_units {
        push(
            issues,
            "SIDECAR_UNIT_MANIFEST",
            &[],
            "单元计数与完整清单不一致或超限。",
        );
    }
    let mut folded = BTreeSet::new();
    let mut next_byte = 0u64;
    for (index, unit) in response.units.iter().enumerate() {
        if !valid_id(&unit.unit_id)
            || !folded.insert(unit.unit_id.to_ascii_lowercase())
            || u64::from(unit.ordinal) != index as u64
        {
            push(
                issues,
                "SIDECAR_UNIT_MANIFEST",
                &[&unit.unit_id],
                "单元ID或完整顺序不合法。",
            );
        }
        ids.insert(unit.unit_id.as_str());
        if unit.locator.start_byte != next_byte
            || range(input, unit.locator.start_byte, unit.locator.end_byte).is_none()
        {
            push(
                issues,
                "SIDECAR_UNIT_RANGE",
                &[&unit.unit_id],
                "源范围必须非空、有界、连续且不重叠。",
            );
        }
        next_byte = unit.locator.end_byte;
        if let Some(content) = &unit.content {
            referenced.insert(content.artifact_id.as_str());
        }
        if !valid_content(unit, input, artifacts) {
            push(
                issues,
                "SIDECAR_CONTENT",
                &[&unit.unit_id],
                "内容引用、状态、字符边界或原始范围字节不一致。",
            );
        }
    }
    if next_byte != input.len() as u64 {
        push(
            issues,
            "SIDECAR_UNIT_RANGE",
            &[],
            "源分区没有覆盖完整原始输入。",
        );
    }
    (ids, referenced)
}

fn validate_reported_issues(
    response: &SidecarResponse,
    units: &BTreeSet<&str>,
    limits: &SidecarLimits,
    issues: &mut Vec<ProtocolViolation>,
) {
    let mut covered = BTreeSet::new();
    for reported in &response.issues {
        if !valid_id(&reported.code)
            || reported.message.is_empty()
            || reported.message.len() > 4096
            || reported.message.trim().is_empty()
            || reported
                .message
                .chars()
                .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t'))
            || reported.unit_ids.len() as u64 > u64::from(limits.max_units)
        {
            push(
                issues,
                "SIDECAR_UNIT_MANIFEST",
                &[],
                "结构化问题的代码、文字或引用数量不合法。",
            );
            continue;
        }
        let mut refs = BTreeSet::new();
        for unit_id in &reported.unit_ids {
            if !units.contains(unit_id.as_str()) || !refs.insert(unit_id.as_str()) {
                push(
                    issues,
                    "SIDECAR_UNIT_MANIFEST",
                    &[unit_id],
                    "问题引用必须唯一指向本轮单元。",
                );
            } else {
                covered.insert(unit_id.as_str());
            }
        }
    }
    for unit in &response.units {
        if unit.status != UnitStatus::Parsed && !covered.contains(unit.unit_id.as_str()) {
            push(
                issues,
                "SIDECAR_UNIT_MANIFEST",
                &[&unit.unit_id],
                "失败或需复核单元必须有自己的问题记录。",
            );
        }
    }
}

fn validate_status(response: &SidecarResponse, issues: &mut Vec<ProtocolViolation>) {
    let unparsed = response
        .units
        .iter()
        .any(|unit| unit.status != UnitStatus::Parsed);
    let invalid_status = match response.status {
        ResponseStatus::Complete => {
            response.unit_count.is_none() || unparsed || !response.issues.is_empty()
        }
        ResponseStatus::Partial => {
            response.unit_count.is_none() || !unparsed || response.issues.is_empty()
        }
        ResponseStatus::Failed => {
            response.issues.is_empty()
                || (response.unit_count.is_some()
                    && !unparsed
                    && !response
                        .issues
                        .iter()
                        .any(|issue| issue.unit_ids.is_empty()))
        }
    };
    if invalid_status {
        push(
            issues,
            "SIDECAR_UNIT_MANIFEST",
            &[],
            "完成或失败状态与库存、单元和问题不一致。",
        );
    }
    if response.status != ResponseStatus::Complete || invalid_status {
        push(
            issues,
            "SIDECAR_REPORTED_FAILURE",
            &[],
            "partial、failed或不完整状态不能作为成功响应。",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const REQUEST: &str = r#"{
      "schema_version":1,"protocol":"noetome-parser-single-v1",
      "request_id":"11111111-1111-4111-8111-111111111111","operation":"inspect_bytes",
      "input":{"relative_path":"input/raw.bin","byte_length":3,"sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"},
      "workspace":{"artifact_directory":"artifacts"},
      "artifact_policy":{"max_file_bytes":16777216,"max_total_bytes":67108864,"max_files":64}
    }"#;
    const RESPONSE: &str = r#"{
      "schema_version":1,"protocol":"noetome-parser-single-v1",
      "request_id":"11111111-1111-4111-8111-111111111111",
      "adapter":{"producer":"fake","adapter_id":"identity-fake","adapter_version":"1",
        "capabilities":{"locator_kinds":["input_bytes"],"artifact_kinds":["binary","utf8_text"]}},
      "input":{"relative_path":"input/raw.bin","byte_length":3,"sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"},
      "status":"complete","unit_count":2,
      "units":[
        {"unit_id":"U1","ordinal":0,"locator":{"kind":"input_bytes","start_byte":0,"end_byte":1},"status":"parsed","content":{"artifact_id":"A1","start_byte":0,"end_byte":1}},
        {"unit_id":"U2","ordinal":1,"locator":{"kind":"input_bytes","start_byte":1,"end_byte":3},"status":"parsed","content":{"artifact_id":"A1","start_byte":1,"end_byte":3}}
      ],
      "artifacts":[{"artifact_id":"A1","kind":"binary","relative_path":"artifacts/A1.bin","byte_length":3,"sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}],
      "issues":[]
    }"#;
    fn request() -> SidecarRequest {
        serde_json::from_str(REQUEST).unwrap()
    }
    fn response() -> SidecarResponse {
        serde_json::from_str(RESPONSE).unwrap()
    }
    fn resources() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([("artifacts/A1.bin".into(), b"abc".to_vec())])
    }
    fn check(value: &SidecarResponse) -> Vec<ProtocolViolation> {
        validate_response(
            &request(),
            value,
            &response().adapter,
            b"abc",
            &resources(),
            &SidecarLimits::default(),
        )
    }
    fn has(issues: &[ProtocolViolation], rule: &str) {
        assert!(
            issues.iter().any(|issue| issue.rule_id == rule),
            "expected {rule}, got {issues:?}"
        );
    }
    #[test]
    fn literal_request_response_roundtrip_preserves_reviewed_fields() {
        for (actual, expected) in [
            (
                serde_json::to_value(request()).unwrap(),
                serde_json::from_str::<Value>(REQUEST).unwrap(),
            ),
            (
                serde_json::to_value(response()).unwrap(),
                serde_json::from_str::<Value>(RESPONSE).unwrap(),
            ),
        ] {
            assert_eq!(actual, expected);
        }
        assert_eq!(crate::text::sha256(b"abc"), SHA);
    }
    #[test]
    fn nullable_unit_count_is_required_even_when_null_is_supported() {
        let mut value: Value = serde_json::from_str(RESPONSE).unwrap();
        value.as_object_mut().unwrap().remove("unit_count");
        assert!(
            serde_json::from_value::<SidecarResponse>(value).is_err(),
            "missing nullable field was accepted"
        );
    }
    #[test]
    fn nullable_unit_content_is_required_even_when_null_is_supported() {
        let mut value: Value = serde_json::from_str(RESPONSE).unwrap();
        value["units"][0].as_object_mut().unwrap().remove("content");
        assert!(
            serde_json::from_value::<SidecarResponse>(value).is_err(),
            "missing nullable content was accepted"
        );
    }
    #[test]
    fn explicit_null_fields_decode_without_becoming_a_complete_result() {
        let mut value: Value = serde_json::from_str(RESPONSE).unwrap();
        value["unit_count"] = Value::Null;
        value["units"][0]["content"] = Value::Null;
        let decoded: SidecarResponse = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.unit_count, None);
        assert_eq!(decoded.units[0].content, None);
        has(&check(&decoded), "SIDECAR_UNIT_MANIFEST");
    }
    #[test]
    fn unknown_fields_are_rejected_at_every_wire_object() {
        for path in [
            "",
            "/adapter",
            "/adapter/capabilities",
            "/input",
            "/units/0",
            "/units/0/locator",
            "/units/0/content",
            "/artifacts/0",
        ] {
            let mut value: Value = serde_json::from_str(RESPONSE).unwrap();
            value
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unreviewed".into(), json!(true));
            assert!(
                serde_json::from_value::<SidecarResponse>(value).is_err(),
                "accepted unknown field at {path}"
            );
        }
        for path in ["", "/input", "/workspace", "/artifact_policy"] {
            let mut value: Value = serde_json::from_str(REQUEST).unwrap();
            value
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unreviewed".into(), json!(true));
            assert!(
                serde_json::from_value::<SidecarRequest>(value).is_err(),
                "accepted unknown request field at {path}"
            );
        }
    }
    #[test]
    fn unknown_enums_and_noninteger_ranges_are_rejected() {
        for (path, replacement) in [
            ("/adapter/producer", json!("fixture")),
            ("/status", json!("succeeded")),
            ("/units/0/status", json!("complete")),
            ("/units/0/locator/kind", json!("pdf_page")),
            ("/artifacts/0/kind", json!("image")),
            ("/unit_count", json!(-1)),
            ("/unit_count", json!(4294967296u64)),
            ("/units/0/locator/start_byte", json!(1.5)),
        ] {
            let mut value: Value = serde_json::from_str(RESPONSE).unwrap();
            *value.pointer_mut(path).unwrap() = replacement;
            assert!(
                serde_json::from_value::<SidecarResponse>(value).is_err(),
                "accepted {path}"
            );
        }
    }
    #[test]
    fn duplicate_keys_are_rejected_by_typed_deserialization() {
        let value = RESPONSE.replace("\"unit_count\":2", "\"unit_count\":2,\"unit_count\":1");
        assert!(serde_json::from_str::<SidecarResponse>(&value).is_err());
    }
    #[test]
    fn complete_literal_has_no_protocol_violations() {
        assert!(check(&response()).is_empty());
    }
    #[test]
    fn unknown_schema_or_protocol_never_inherits_v1_validation() {
        for revision in [0, 2] {
            let mut value = response();
            value.schema_version = revision;
            has(&check(&value), "SIDECAR_VERSION");
        }
        let mut value = response();
        value.protocol.push_str("-future");
        has(&check(&value), "SIDECAR_VERSION");
    }
    #[test]
    fn request_identity_and_real_input_fingerprint_must_match() {
        let mut value = response();
        value.request_id = "22222222-2222-4222-8222-222222222222".into();
        has(&check(&value), "SIDECAR_IDENTITY");
        let mut value = response();
        value.input.sha256 = "0".repeat(64);
        has(&check(&value), "SIDECAR_IDENTITY");
        let mut value = response();
        value.input.byte_length = 2;
        has(&check(&value), "SIDECAR_IDENTITY");
        let mut value = response();
        value.input.relative_path = "other-input.bin".into();
        has(&check(&value), "SIDECAR_IDENTITY");
    }
    #[test]
    fn fake_cannot_claim_parser_or_another_selected_adapter() {
        let mut value = response();
        value.adapter.producer = Producer::Parser;
        has(&check(&value), "SIDECAR_PRODUCER");
        let mut value = response();
        value.adapter.adapter_id = "different-adapter".into();
        has(&check(&value), "SIDECAR_PRODUCER");
    }
    #[test]
    fn duplicate_or_unknown_capability_claims_are_rejected() {
        let mut value = response();
        value
            .adapter
            .capabilities
            .locator_kinds
            .push(LocatorKind::InputBytes);
        has(&check(&value), "SIDECAR_PRODUCER");
        let mut value = response();
        value.adapter.capabilities.artifact_kinds.clear();
        has(&check(&value), "SIDECAR_PRODUCER");
    }
    #[test]
    fn missing_tail_cannot_be_hidden_by_a_refreshed_count() {
        let mut value = response();
        value.units.pop();
        value.unit_count = Some(1);
        has(&check(&value), "SIDECAR_UNIT_RANGE");
    }
    #[test]
    fn duplicate_missing_or_reordered_ordinals_fail_inventory() {
        for ordinal in [0, 2] {
            let mut value = response();
            value.units[1].ordinal = ordinal;
            has(&check(&value), "SIDECAR_UNIT_MANIFEST");
        }
        let mut value = response();
        value.units.swap(0, 1);
        has(&check(&value), "SIDECAR_UNIT_MANIFEST");
        let mut value = response();
        value.units[1].unit_id = "u1".into();
        has(&check(&value), "SIDECAR_UNIT_MANIFEST");
    }
    #[test]
    fn empty_nonempty_or_out_of_bounds_source_ranges_are_rejected_safely() {
        for (start, end) in [(0, 0), (2, 1), (0, 4), (0, u64::MAX)] {
            let mut value = response();
            value.units[0].locator.start_byte = start;
            value.units[0].locator.end_byte = end;
            has(&check(&value), "SIDECAR_UNIT_RANGE");
        }
        let mut value = response();
        value.units[1].locator.start_byte = 0;
        has(&check(&value), "SIDECAR_UNIT_RANGE");
    }
    #[test]
    fn parsed_requires_content_and_failed_forbids_it() {
        let mut value = response();
        value.units[0].content = None;
        has(&check(&value), "SIDECAR_CONTENT");
        let mut value = response();
        value.units[0].status = UnitStatus::Failed;
        has(&check(&value), "SIDECAR_CONTENT");
    }
    #[test]
    fn wrong_or_out_of_bounds_content_reference_is_rejected() {
        let mut value = response();
        value.units[0].content.as_mut().unwrap().artifact_id = "missing".into();
        has(&check(&value), "SIDECAR_CONTENT");
        let mut value = response();
        value.units[0].content.as_mut().unwrap().end_byte = u64::MAX;
        has(&check(&value), "SIDECAR_CONTENT");
        let mut value = response();
        let content = value.units[0].content.as_mut().unwrap();
        content.start_byte = 1;
        content.end_byte = 2;
        has(&check(&value), "SIDECAR_CONTENT");
    }
    #[test]
    fn resources_must_have_fixed_paths_exact_sets_and_honest_hashes() {
        for path in [
            "../outside",
            "C:/outside",
            "artifacts/A1.bin:stream",
            "artifacts/sub/A1.bin",
            "artifacts/A1.txt",
            "artifacts/NUL.bin",
        ] {
            let mut value = response();
            value.artifacts[0].relative_path = path.into();
            has(&check(&value), "SIDECAR_ARTIFACT_MANIFEST");
        }
        let mut value = response();
        value.artifacts[0].sha256 = "0".repeat(64);
        has(&check(&value), "SIDECAR_ARTIFACT_INTEGRITY");
        let mut value = response();
        value.artifacts[0].byte_length = 2;
        has(&check(&value), "SIDECAR_ARTIFACT_INTEGRITY");
        let issues = validate_response(
            &request(),
            &response(),
            &response().adapter,
            b"abc",
            &BTreeMap::new(),
            &SidecarLimits::default(),
        );
        has(&issues, "SIDECAR_ARTIFACT_MANIFEST");
    }
    #[test]
    fn orphan_resources_and_case_colliding_ids_are_rejected() {
        let mut value = response();
        let mut extra = value.artifacts[0].clone();
        extra.artifact_id = "A2".into();
        extra.relative_path = "artifacts/A2.bin".into();
        value.artifacts.push(extra);
        let mut bytes = resources();
        bytes.insert("artifacts/A2.bin".into(), b"abc".to_vec());
        has(
            &validate_response(
                &request(),
                &value,
                &response().adapter,
                b"abc",
                &bytes,
                &SidecarLimits::default(),
            ),
            "SIDECAR_ARTIFACT_MANIFEST",
        );
        let mut value = response();
        let mut extra = value.artifacts[0].clone();
        extra.artifact_id = "a1".into();
        extra.relative_path = "artifacts/a1.bin".into();
        value.artifacts.push(extra);
        has(&check(&value), "SIDECAR_ARTIFACT_MANIFEST");
    }
    #[test]
    fn nonempty_input_cannot_be_reported_complete_with_zero_units() {
        let mut value = response();
        value.units.clear();
        value.artifacts.clear();
        value.unit_count = Some(0);
        has(&check(&value), "SIDECAR_UNIT_RANGE");
    }
    #[test]
    fn partial_and_unknown_failed_results_remain_explicit_failures() {
        let mut value = response();
        value.status = ResponseStatus::Partial;
        value.units[1].status = UnitStatus::Failed;
        value.units[1].content = None;
        value.issues.push(ReportedIssue {
            code: "TEST_FAILURE".into(),
            message: "末尾待处理。".into(),
            unit_ids: vec!["U2".into()],
        });
        let issues = check(&value);
        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.rule_id.as_str())
                .collect::<Vec<_>>(),
            ["SIDECAR_REPORTED_FAILURE"]
        );
        let mut value = response();
        value.status = ResponseStatus::Failed;
        value.unit_count = None;
        value.units.clear();
        value.artifacts.clear();
        value.issues.push(ReportedIssue {
            code: "TEST_FAILURE".into(),
            message: "库存未知。".into(),
            unit_ids: Vec::new(),
        });
        let issues = validate_response(
            &request(),
            &value,
            &response().adapter,
            b"abc",
            &BTreeMap::new(),
            &SidecarLimits::default(),
        );
        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.rule_id.as_str())
                .collect::<Vec<_>>(),
            ["SIDECAR_REPORTED_FAILURE"]
        );
    }
    #[test]
    fn failed_or_review_units_require_valid_issue_references() {
        let mut value = response();
        value.status = ResponseStatus::Partial;
        value.units[1].status = UnitStatus::NeedsReview;
        value.issues.push(ReportedIssue {
            code: "TEST_FAILURE".into(),
            message: "错误引用。".into(),
            unit_ids: vec!["missing".into()],
        });
        has(&check(&value), "SIDECAR_UNIT_MANIFEST");
    }
    #[test]
    fn counts_and_resource_bytes_are_limited_even_for_valid_wire_values() {
        let limits = SidecarLimits {
            max_units: 1,
            ..SidecarLimits::default()
        };
        has(
            &validate_response(
                &request(),
                &response(),
                &response().adapter,
                b"abc",
                &resources(),
                &limits,
            ),
            "SIDECAR_UNIT_MANIFEST",
        );
        let limits = SidecarLimits {
            max_artifact_bytes: 2,
            ..SidecarLimits::default()
        };
        has(
            &validate_response(
                &request(),
                &response(),
                &response().adapter,
                b"abc",
                &resources(),
                &limits,
            ),
            "SIDECAR_ARTIFACT_INTEGRITY",
        );
    }
    fn unicode_case() -> (SidecarRequest, SidecarResponse, BTreeMap<String, Vec<u8>>) {
        let input = "甲乙".as_bytes();
        let mut request = request();
        request.input.byte_length = 6;
        request.input.sha256 = crate::text::sha256(input);
        let mut response = response();
        response.input = request.input.clone();
        response.artifacts[0].kind = ArtifactKind::Utf8Text;
        response.artifacts[0].relative_path = "artifacts/A1.txt".into();
        response.artifacts[0].byte_length = 6;
        response.artifacts[0].sha256 = request.input.sha256.clone();
        response.units[0].locator.end_byte = 3;
        response.units[0].content.as_mut().unwrap().end_byte = 3;
        response.units[1].locator.start_byte = 3;
        response.units[1].locator.end_byte = 6;
        response.units[1].content.as_mut().unwrap().start_byte = 3;
        response.units[1].content.as_mut().unwrap().end_byte = 6;
        (
            request,
            response,
            BTreeMap::from([("artifacts/A1.txt".into(), input.to_vec())]),
        )
    }
    #[test]
    fn unicode_text_resource_has_real_character_boundaries() {
        let (request, response, bytes) = unicode_case();
        assert!(
            validate_response(
                &request,
                &response,
                &response.adapter,
                "甲乙".as_bytes(),
                &bytes,
                &SidecarLimits::default()
            )
            .is_empty()
        );
    }
    #[test]
    fn split_utf8_character_fails_even_when_byte_partition_and_hashes_match() {
        let (request, mut response, bytes) = unicode_case();
        response.units[0].locator.end_byte = 1;
        response.units[0].content.as_mut().unwrap().end_byte = 1;
        response.units[1].locator.start_byte = 1;
        response.units[1].content.as_mut().unwrap().start_byte = 1;
        has(
            &validate_response(
                &request,
                &response,
                &response.adapter,
                "甲乙".as_bytes(),
                &bytes,
                &SidecarLimits::default(),
            ),
            "SIDECAR_CONTENT",
        );
    }
    #[test]
    fn text_resource_with_refreshed_real_hash_still_requires_valid_utf8() {
        let input = vec![0xff, 0xfe, 0xfd];
        let mut request = request();
        request.input.sha256 = crate::text::sha256(&input);
        let mut response = response();
        response.input = request.input.clone();
        response.artifacts[0].kind = ArtifactKind::Utf8Text;
        response.artifacts[0].relative_path = "artifacts/A1.txt".into();
        response.artifacts[0].sha256 = request.input.sha256.clone();
        let bytes = BTreeMap::from([("artifacts/A1.txt".into(), input.clone())]);
        has(
            &validate_response(
                &request,
                &response,
                &response.adapter,
                &input,
                &bytes,
                &SidecarLimits::default(),
            ),
            "SIDECAR_ARTIFACT_INTEGRITY",
        );
    }
    #[test]
    fn empty_input_requires_the_exact_empty_complete_manifest() {
        let mut request = request();
        request.input.byte_length = 0;
        request.input.sha256 = crate::text::sha256(b"");
        let mut response = response();
        response.input = request.input.clone();
        response.unit_count = Some(0);
        response.units.clear();
        response.artifacts.clear();
        assert!(
            validate_response(
                &request,
                &response,
                &response.adapter,
                b"",
                &BTreeMap::new(),
                &SidecarLimits::default()
            )
            .is_empty()
        );
    }

    fn positional(value: &Value, fields: &[&str]) -> Value {
        Value::Array(fields.iter().map(|name| value[*name].clone()).collect())
    }

    #[test]
    fn wire_shape_roots_and_nested_objects_reject_positional_sequences() {
        let req = serde_json::to_value(request()).unwrap();
        let seq = positional(
            &req,
            &[
                "schema_version",
                "protocol",
                "request_id",
                "operation",
                "input",
                "workspace",
                "artifact_policy",
            ],
        );
        assert!(serde_json::from_value::<SidecarRequest>(seq).is_err());
        let res = serde_json::to_value(response()).unwrap();
        let seq = positional(
            &res,
            &[
                "schema_version",
                "protocol",
                "request_id",
                "adapter",
                "input",
                "status",
                "unit_count",
                "units",
                "artifacts",
                "issues",
            ],
        );
        assert!(serde_json::from_value::<SidecarResponse>(seq).is_err());
        for (path, fields) in [
            (
                "/adapter",
                &["producer", "adapter_id", "adapter_version", "capabilities"][..],
            ),
            (
                "/adapter/capabilities",
                &["locator_kinds", "artifact_kinds"][..],
            ),
            ("/input", &["relative_path", "byte_length", "sha256"][..]),
            (
                "/units/0",
                &["unit_id", "ordinal", "locator", "status", "content"][..],
            ),
            ("/units/0/locator", &["kind", "start_byte", "end_byte"][..]),
            (
                "/units/0/content",
                &["artifact_id", "start_byte", "end_byte"][..],
            ),
            (
                "/artifacts/0",
                &[
                    "artifact_id",
                    "kind",
                    "relative_path",
                    "byte_length",
                    "sha256",
                ][..],
            ),
        ] {
            let mut changed = res.clone();
            let array = positional(changed.pointer(path).unwrap(), fields);
            *changed.pointer_mut(path).unwrap() = array;
            assert!(
                serde_json::from_value::<SidecarResponse>(changed).is_err(),
                "{path}"
            );
        }
    }

    #[test]
    fn wire_shape_enum_values_must_be_strings_not_external_tag_maps() {
        assert!(serde_json::from_str::<Operation>(r#"{"inspect_bytes":null}"#).is_err());
        for path in [
            "/status",
            "/adapter/producer",
            "/adapter/capabilities/locator_kinds/0",
            "/adapter/capabilities/artifact_kinds/0",
            "/units/0/status",
            "/units/0/locator/kind",
            "/artifacts/0/kind",
        ] {
            let mut changed = serde_json::to_value(response()).unwrap();
            let variant = changed.pointer(path).unwrap().as_str().unwrap().to_owned();
            *changed.pointer_mut(path).unwrap() =
                Value::Object(serde_json::Map::from_iter([(variant, Value::Null)]));
            assert!(
                serde_json::from_value::<SidecarResponse>(changed).is_err(),
                "{path}"
            );
        }
    }

    #[test]
    fn wire_shape_request_policies_and_issues_are_also_object_only() {
        assert!(serde_json::from_str::<WorkspacePolicy>(r#"["artifacts"]"#).is_err());
        assert!(serde_json::from_str::<ArtifactPolicy>("[1,2,3]").is_err());
        assert!(serde_json::from_str::<ReportedIssue>(r#"["FAIL","reason",[]]"#).is_err());
        assert!(serde_json::from_str::<ProtocolViolation>(r#"["FAIL",[],"reason"]"#).is_err());
    }
}
