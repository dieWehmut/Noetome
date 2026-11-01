//! Strict PDF v2 wire models and primitive-to-derived Core verification.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use crate::sidecar::{
    Artifact, ArtifactKind, ArtifactPolicy, InputFingerprint, Producer, ProtocolViolation,
    ResponseStatus, SidecarLimits, UnitStatus, WorkspacePolicy,
};

pub const PROTOCOL: &str = "noetome-pdf-single-v2";
pub const OPTIONS_VERSION: &str = "pdfplumber-glyph-lines-v1";

pub(crate) fn required_option<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

pub(crate) fn deserialize_object<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    struct ObjectVisitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for ObjectVisitor<T> {
        type Value = T;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a JSON object with named fields")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectVisitor(std::marker::PhantomData))
}

macro_rules! model {
    ($name:ident { $($(#[$attribute:meta])* $field:ident : $kind:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Serialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $($(#[$attribute])* pub $field: $kind),* }
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

enumeration!(PdfOperation { ParsePdf });
enumeration!(PdfLocatorKind { PdfPageRegion });

model!(PdfPolicy {
    max_pages: u32,
    max_glyphs: u32,
    max_regions: u32,
    max_objects: u32
});
impl Copy for PdfPolicy {}
impl Default for PdfPolicy {
    fn default() -> Self {
        Self {
            max_pages: 63,
            max_glyphs: 32768,
            max_regions: 4096,
            max_objects: 4096,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PdfLimits {
    pub transport: SidecarLimits,
    pub policy: PdfPolicy,
}

model!(PdfRequest {
    schema_version: u32,
    protocol: String,
    request_id: String,
    operation: PdfOperation,
    input: InputFingerprint,
    workspace: WorkspacePolicy,
    artifact_policy: ArtifactPolicy,
    pdf_policy: PdfPolicy,
    options_version: String,
});
model!(PdfResponse {
    schema_version: u32, protocol: String, request_id: String, adapter: PdfAdapter,
    input: InputFingerprint, status: ResponseStatus,
    #[serde(deserialize_with = "required_option")] page_count: Option<u32>,
    #[serde(deserialize_with = "required_option")] inventory: Option<ArtifactRef>,
    pages: Vec<PdfPage>, artifacts: Vec<Artifact>, issues: Vec<PdfIssue>,
});
model!(PdfAdapter {
    producer: Producer,
    adapter_id: String,
    adapter_version: String,
    options_version: String,
    runtime: PdfRuntime,
});
model!(PdfRuntime {
    python: String,
    pdfplumber: String,
    pdfminer: String,
    pypdfium2: String,
    pillow: String,
    cryptography: String,
    cffi: String,
    pycparser: String,
    charset_normalizer: String,
});
pub fn expected_runtime() -> PdfRuntime {
    PdfRuntime {
        python: "3.12.14".into(),
        pdfplumber: "0.11.9".into(),
        pdfminer: "20251230".into(),
        pypdfium2: "5.14.0".into(),
        pillow: "12.3.0".into(),
        cryptography: "50.0.2".into(),
        cffi: "2.1.1".into(),
        pycparser: "3.11".into(),
        charset_normalizer: "3.5.2".into(),
    }
}
model!(ArtifactRef {
    artifact_id: String,
    sha256: String
});
model!(PdfPage {
    physical_page_index: u32,
    #[serde(deserialize_with = "required_option")] printed_page_label: Option<String>,
    status: UnitStatus,
    #[serde(deserialize_with = "required_option")] geometry: Option<PdfPageGeometry>,
    #[serde(deserialize_with = "required_option")] glyph_count: Option<u32>,
    excluded_glyph_ordinals: Vec<u32>,
    #[serde(deserialize_with = "required_option")] text_artifact: Option<ArtifactRef>,
    regions: Vec<PdfRegion>,
});
model!(PdfPageGeometry {
    media_box: [f64; 4],
    crop_box: [f64; 4],
    rotation: u32,
    user_unit: f64,
    display_width_pt: f64,
    display_height_pt: f64,
});
model!(PdfRegion {
    region_id: String, ordinal: u32, locator: PdfLocator, content: PdfContent,
    glyph_ordinals: Vec<u32>,
});
model!(PdfLocator {
    kind: PdfLocatorKind,
    physical_page_index: u32,
    bbox_pt: [f64; 4]
});
model!(PdfContent {
    artifact_id: String,
    sha256: String,
    start_byte: u64,
    end_byte: u64
});
model!(PdfIssue { code: String, message: String, page_indices: Vec<u32> });
model!(PdfInventory {
    schema_version: u32, input: InputFingerprint, page_count: u32, pages: Vec<PdfSourcePage>,
});
model!(PdfSourcePage {
    physical_page_index: u32,
    #[serde(deserialize_with = "required_option")] geometry: Option<PdfSourceGeometry>,
    #[serde(deserialize_with = "required_option")] glyphs: Option<Vec<PdfGlyph>>,
    #[serde(deserialize_with = "required_option")] visual_objects: Option<PdfVisualObjects>,
    #[serde(deserialize_with = "required_option")] decode_error: Option<String>,
});
model!(PdfSourceGeometry {
    media_box: [f64; 4],
    crop_box: [f64; 4],
    rotation: u32,
    user_unit: f64
});
model!(PdfGlyph {
    ordinal: u32,
    text: String,
    media_bbox: [f64; 4],
    upright: bool,
    font_size: f64,
    font_name: String,
});
model!(PdfVisualObjects {
    images: u32,
    lines: u32,
    rectangles: u32,
    curves: u32,
    annotations: u32
});

pub fn validate_pdf(
    request: &PdfRequest,
    response: &PdfResponse,
    artifact_bytes: &BTreeMap<String, Vec<u8>>,
    limits: &PdfLimits,
    cancel: &AtomicBool,
) -> Vec<ProtocolViolation> {
    let mut violations = validate_pdf_inner(request, response, artifact_bytes, limits, cancel);
    // Check after every inner exit, including malformed inventory JSON, so a
    // cancellation observed during synchronous work retains its stable rule.
    cancelled(cancel, &mut violations);
    violations
}

fn validate_pdf_inner(
    request: &PdfRequest,
    response: &PdfResponse,
    artifact_bytes: &BTreeMap<String, Vec<u8>>,
    limits: &PdfLimits,
    cancel: &AtomicBool,
) -> Vec<ProtocolViolation> {
    let mut violations = Vec::new();
    if cancelled(cancel, &mut violations) {
        return violations;
    }
    validate_header(request, response, limits, &mut violations);
    if response.pages.len() as u64 > u64::from(limits.policy.max_pages)
        || response.issues.len() as u64 > u64::from(limits.transport.max_issues)
        || response.artifacts.len() as u64 > u64::from(limits.transport.max_artifacts)
        || artifact_bytes.len() as u64 > u64::from(limits.transport.max_artifacts)
    {
        push(
            &mut violations,
            "PDF_POLICY",
            &[],
            "页面、问题或资源数量超过本轮政策。",
        );
        return violations;
    }
    let artifacts = validate_artifacts(response, artifact_bytes, limits, cancel, &mut violations);
    if cancelled(cancel, &mut violations) {
        return violations;
    }
    validate_issues(response, &mut violations);
    let Some(count) = response.page_count else {
        if response.inventory.is_some()
            || !response.pages.is_empty()
            || !response.artifacts.is_empty()
            || !artifact_bytes.is_empty()
            || response.status != ResponseStatus::Failed
        {
            push(
                &mut violations,
                "PDF_PAGE_MANIFEST",
                &[],
                "没有完整库存时只能返回无页面、无资源的failed文档。",
            );
        }
        if response.status != ResponseStatus::Complete {
            push(
                &mut violations,
                "PDF_REPORTED_INCOMPLETE",
                &[],
                "文档没有完成可复核的完整页面库存。",
            );
        }
        return violations;
    };
    if count > limits.policy.max_pages
        || response.pages.len() as u64 != u64::from(count)
        || response.pages.iter().enumerate().any(|(index, page)| {
            page.physical_page_index as usize != index || page.printed_page_label.is_some()
        })
    {
        push(
            &mut violations,
            "PDF_PAGE_MANIFEST",
            &[],
            "页面清单必须完整、按真实物理索引排序且印刷页标签为空。",
        );
        return violations;
    }
    let Some(reference) = &response.inventory else {
        push(
            &mut violations,
            "PDF_INVENTORY",
            &[],
            "完整页面清单必须引用独立字形库存。",
        );
        return violations;
    };
    let mut referenced = BTreeSet::new();
    referenced.insert(reference.artifact_id.as_str());
    let Some(inventory_artifact) = checked_reference(reference, ArtifactKind::Binary, &artifacts)
    else {
        push(
            &mut violations,
            "PDF_INVENTORY",
            &[],
            "字形库存必须引用同一实际二进制资源及真实SHA。",
        );
        return violations;
    };
    if reference.artifact_id != "pdf_inventory" {
        push(
            &mut violations,
            "PDF_INVENTORY",
            &[],
            "字形库存只能使用固定pdf_inventory资源。",
        );
    }
    if cancelled(cancel, &mut violations) {
        return violations;
    }
    let inventory = match serde_json::from_slice::<PdfInventory>(inventory_artifact.bytes) {
        Ok(value) => value,
        Err(error) => {
            // The serde error text may include a document token. Only expose
            // diagnostic category and line/column, never the payload itself.
            push(
                &mut violations,
                "PDF_INVENTORY_JSON",
                &[],
                &format!(
                    "字形库存JSON不符合严格对象：{:?}，行{}，列{}。",
                    error.classify(),
                    error.line(),
                    error.column()
                ),
            );
            return violations;
        }
    };
    if cancelled(cancel, &mut violations) {
        return violations;
    }
    if inventory.schema_version != 1
        || inventory.input != request.input
        || inventory.page_count != count
        || inventory.pages.len() as u64 != u64::from(count)
        || inventory
            .pages
            .iter()
            .enumerate()
            .any(|(index, page)| page.physical_page_index as usize != index)
    {
        push(
            &mut violations,
            "PDF_INVENTORY",
            &[],
            "原始字形库存版本、输入指纹和完整页面清单不一致。",
        );
        return violations;
    }
    let glyph_count: u64 = inventory
        .pages
        .iter()
        .map(|page| page.glyphs.as_ref().map_or(0, |glyphs| glyphs.len() as u64))
        .sum();
    let region_count: u64 = response
        .pages
        .iter()
        .map(|page| page.regions.len() as u64)
        .sum();
    let object_count: u64 = inventory
        .pages
        .iter()
        .filter_map(|page| page.visual_objects.as_ref())
        .map(object_count)
        .sum();
    if glyph_count > u64::from(limits.policy.max_glyphs)
        || region_count > u64::from(limits.policy.max_regions)
        || object_count > u64::from(limits.policy.max_objects)
    {
        push(
            &mut violations,
            "PDF_POLICY",
            &[],
            "字形、区域或视觉对象总数超过本轮政策。",
        );
        return violations;
    }
    let mut region_ids = BTreeSet::new();
    let mut derived_regions = 0u64;
    for (source, page) in inventory.pages.iter().zip(&response.pages) {
        if cancelled(cancel, &mut violations) {
            return violations;
        }
        derived_regions += validate_page(
            source,
            page,
            &artifacts,
            &mut referenced,
            &mut region_ids,
            limits,
            cancel,
            &mut violations,
        );
    }
    if derived_regions > u64::from(limits.policy.max_regions) {
        push(
            &mut violations,
            "PDF_POLICY",
            &[],
            "重算区域总数超过本轮政策。",
        );
    }
    for artifact in &response.artifacts {
        if !referenced.contains(artifact.artifact_id.as_str()) {
            push(
                &mut violations,
                "PDF_ARTIFACT_MANIFEST",
                &[],
                "资源未被库存或其所属页面正文引用。",
            );
        }
    }
    let expected = if response
        .pages
        .iter()
        .all(|page| page.status == UnitStatus::Parsed)
    {
        ResponseStatus::Complete
    } else {
        ResponseStatus::Partial
    };
    if response.status != expected {
        push(
            &mut violations,
            "PDF_PAGE_STATE",
            &[],
            "整体状态必须与全部页面的已解析或未完成状态一致。",
        );
    }
    if response.status != ResponseStatus::Complete {
        push(
            &mut violations,
            "PDF_REPORTED_INCOMPLETE",
            &[],
            "解析保留了已确认范围，但文档仍需复核或存在失败页面。",
        );
    }
    cancelled(cancel, &mut violations);
    violations
}

fn push(violations: &mut Vec<ProtocolViolation>, rule: &str, refs: &[&str], message: &str) {
    violations.push(ProtocolViolation {
        rule_id: rule.into(),
        object_refs: refs.iter().map(|value| (*value).into()).collect(),
        message: message.into(),
    });
}

fn cancelled(cancel: &AtomicBool, violations: &mut Vec<ProtocolViolation>) -> bool {
    if !cancel.load(Ordering::Relaxed) {
        return false;
    }
    if !violations
        .iter()
        .any(|value| value.rule_id == "SIDECAR_CANCELLED")
    {
        push(violations, "SIDECAR_CANCELLED", &[], "PDF复核被取消。");
    }
    true
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
fn validate_header(
    request: &PdfRequest,
    response: &PdfResponse,
    limits: &PdfLimits,
    violations: &mut Vec<ProtocolViolation>,
) {
    if request.schema_version != 2
        || response.schema_version != 2
        || request.protocol != PROTOCOL
        || response.protocol != PROTOCOL
        || request.options_version != OPTIONS_VERSION
        || request.operation != PdfOperation::ParsePdf
    {
        push(
            violations,
            "PDF_VERSION",
            &[],
            "只接受固定PDF v2协议与字形行选项。",
        );
    }
    let valid_uuid = uuid::Uuid::parse_str(&request.request_id).is_ok_and(|value| {
        value.get_version() == Some(uuid::Version::Random)
            && value.to_string() == request.request_id
    });
    if !valid_uuid
        || response.request_id != request.request_id
        || response.input != request.input
        || request.input.relative_path != "input/raw.bin"
        || !valid_sha(&request.input.sha256)
        || request.input.byte_length > limits.transport.max_input_bytes
        || request.workspace.artifact_directory != "artifacts"
    {
        push(
            violations,
            "PDF_IDENTITY",
            &[],
            "Host身份、原始输入指纹或固定工作区不一致。",
        );
    }
    let policy = PdfPolicy::default();
    let transport = SidecarLimits::default();
    if request.pdf_policy != limits.policy
        || limits.policy.max_pages > policy.max_pages
        || limits.policy.max_glyphs > policy.max_glyphs
        || limits.policy.max_regions > policy.max_regions
        || limits.policy.max_objects > policy.max_objects
        || request.artifact_policy.max_file_bytes != limits.transport.max_artifact_bytes
        || request.artifact_policy.max_total_bytes != limits.transport.max_total_artifact_bytes
        || request.artifact_policy.max_files != limits.transport.max_artifacts
        || limits.transport.max_input_bytes > transport.max_input_bytes
        || limits.transport.max_request_bytes > transport.max_request_bytes
        || limits.transport.max_stdout_bytes > transport.max_stdout_bytes
        || limits.transport.max_stderr_bytes > transport.max_stderr_bytes
        || limits.transport.max_artifact_bytes > transport.max_artifact_bytes
        || limits.transport.max_total_artifact_bytes > transport.max_total_artifact_bytes
        || limits.transport.max_artifacts > transport.max_artifacts
        || limits.transport.max_units > transport.max_units
        || limits.transport.max_issues > transport.max_issues
        || limits.transport.timeout > transport.timeout
        || limits.transport.cleanup_timeout > transport.cleanup_timeout
    {
        push(
            violations,
            "PDF_POLICY",
            &[],
            "请求政策必须与Host本轮收紧的资源和PDF上限一致。",
        );
    }
    if response.adapter.producer != Producer::Parser
        || response.adapter.adapter_id != "pdfplumber-text"
        || response.adapter.adapter_version != "1.0.0"
        || response.adapter.options_version != OPTIONS_VERSION
        || response.adapter.runtime != expected_runtime()
    {
        push(
            violations,
            "PDF_ADAPTER",
            &[],
            "只接受Host选定的真实解析器、选项和逐项锁定运行时。",
        );
    }
}

struct CheckedArtifact<'a> {
    artifact: &'a Artifact,
    bytes: &'a [u8],
}
fn checked_reference<'a>(
    reference: &ArtifactRef,
    kind: ArtifactKind,
    artifacts: &'a BTreeMap<&str, CheckedArtifact<'a>>,
) -> Option<&'a CheckedArtifact<'a>> {
    let value = artifacts.get(reference.artifact_id.as_str())?;
    (valid_id(&reference.artifact_id)
        && valid_sha(&reference.sha256)
        && value.artifact.kind == kind
        && value.artifact.sha256 == reference.sha256)
        .then_some(value)
}
fn validate_artifacts<'a>(
    response: &'a PdfResponse,
    actual: &'a BTreeMap<String, Vec<u8>>,
    limits: &PdfLimits,
    cancel: &AtomicBool,
    violations: &mut Vec<ProtocolViolation>,
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
            violations,
            "PDF_ARTIFACT_MANIFEST",
            &[],
            "实际资源集合必须与完整声明精确相等。",
        );
    }
    let total = actual
        .values()
        .try_fold(0u64, |sum, bytes| sum.checked_add(bytes.len() as u64));
    let declared_total = response
        .artifacts
        .iter()
        .try_fold(0u64, |sum, artifact| sum.checked_add(artifact.byte_length));
    if total.is_none_or(|total| total > limits.transport.max_total_artifact_bytes)
        || declared_total.is_none_or(|total| total > limits.transport.max_total_artifact_bytes)
        || actual
            .values()
            .any(|bytes| bytes.len() as u64 > limits.transport.max_artifact_bytes)
    {
        push(
            violations,
            "PDF_ARTIFACT_INTEGRITY",
            &[],
            "实际或声明资源字节超过本轮上限。",
        );
        return checked;
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for artifact in &response.artifacts {
        if cancelled(cancel, violations) {
            return checked;
        }
        let extension = if artifact.kind == ArtifactKind::Binary {
            "bin"
        } else {
            "txt"
        };
        let path = format!("artifacts/{}.{}", artifact.artifact_id, extension);
        if !valid_id(&artifact.artifact_id)
            || artifact.relative_path != path
            || !crate::fixture_input::valid_relative_path(&path)
            || !ids.insert(artifact.artifact_id.to_ascii_lowercase())
            || !paths.insert(path.to_ascii_lowercase())
        {
            push(
                violations,
                "PDF_ARTIFACT_MANIFEST",
                &[],
                "资源ID、固定平面路径或大小写唯一性不合法。",
            );
            continue;
        }
        let Some(bytes) = actual.get(&artifact.relative_path) else {
            continue;
        };
        let mut hash = Sha256::new();
        for chunk in bytes.chunks(64 * 1024) {
            if cancelled(cancel, violations) {
                return checked;
            }
            hash.update(chunk);
        }
        let sha = format!("{:x}", hash.finalize());
        if artifact.byte_length != bytes.len() as u64
            || artifact.byte_length > limits.transport.max_artifact_bytes
            || !valid_sha(&artifact.sha256)
            || sha != artifact.sha256
            || (artifact.kind == ArtifactKind::Utf8Text && std::str::from_utf8(bytes).is_err())
        {
            push(
                violations,
                "PDF_ARTIFACT_INTEGRITY",
                &[],
                "实际资源长度、SHA或UTF8内容不符合声明。",
            );
            continue;
        }
        checked.insert(
            artifact.artifact_id.as_str(),
            CheckedArtifact { artifact, bytes },
        );
    }
    checked
}

fn validate_issues(response: &PdfResponse, violations: &mut Vec<ProtocolViolation>) {
    let mut cited = BTreeSet::new();
    for issue in &response.issues {
        let unique: BTreeSet<_> = issue.page_indices.iter().copied().collect();
        if !valid_id(&issue.code)
            || issue.message.trim().is_empty()
            || issue.message.len() > 4096
            || issue.message.chars().any(|value| value.is_control())
            || unique.len() != issue.page_indices.len()
            || (response.page_count.is_some() && issue.page_indices.is_empty())
            || issue.page_indices.iter().any(|index| {
                response.page_count.is_none_or(|count| *index >= count)
                    || response
                        .pages
                        .get(*index as usize)
                        .is_none_or(|page| page.status == UnitStatus::Parsed)
            })
        {
            push(
                violations,
                "PDF_ISSUE",
                &[],
                "问题必须有受限消息和本轮未完成页面的唯一真实引用。",
            );
        }
        cited.extend(issue.page_indices.iter().copied());
    }
    if response.page_count.is_none() && response.issues.is_empty() {
        push(
            violations,
            "PDF_ISSUE",
            &[],
            "未知页面库存的失败文档必须保留明确问题。",
        );
    }
    for page in &response.pages {
        if page.status != UnitStatus::Parsed && !cited.contains(&page.physical_page_index) {
            push(
                violations,
                "PDF_ISSUE",
                &[],
                "每个失败或需复核页面必须有可定位问题。",
            );
        }
    }
}

fn object_count(value: &PdfVisualObjects) -> u64 {
    u64::from(value.images)
        + u64::from(value.lines)
        + u64::from(value.rectangles)
        + u64::from(value.curves)
        + u64::from(value.annotations)
}
fn finite_box(value: &[f64; 4]) -> bool {
    value.iter().all(|value| value.is_finite()) && value[0] <= value[2] && value[1] <= value[3]
}
fn close(left: f64, right: f64) -> bool {
    left.is_finite() && right.is_finite() && (left - right).abs() <= 0.000001
}
fn same_box(left: &[f64; 4], right: &[f64; 4]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| close(*left, *right))
}

struct Geometry {
    page: PdfPageGeometry,
    crop: [f64; 4],
}
fn geometry(raw: &PdfSourceGeometry) -> Option<Geometry> {
    let m = raw.media_box;
    let c = raw.crop_box;
    if !finite_box(&m)
        || !finite_box(&c)
        || m[0] >= m[2]
        || m[1] >= m[3]
        || c[0] >= c[2]
        || c[1] >= c[3]
        || c[0] < m[0]
        || c[1] < m[1]
        || c[2] > m[2]
        || c[3] > m[3]
        || !matches!(raw.rotation, 0 | 90 | 180 | 270)
        || !raw.user_unit.is_finite()
        || raw.user_unit <= 0.
        || raw.user_unit > 64.
    {
        return None;
    }
    let w = m[2] - m[0];
    let h = m[3] - m[1];
    let mut crop = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for (x, y) in [(c[0], c[1]), (c[0], c[3]), (c[2], c[1]), (c[2], c[3])] {
        let u = x - m[0];
        let v = y - m[1];
        let (x, top) = match raw.rotation {
            0 => (u, h - v),
            90 => (v, u),
            180 => (w - u, v),
            270 => (h - v, w - u),
            _ => return None,
        };
        crop[0] = crop[0].min(x);
        crop[1] = crop[1].min(top);
        crop[2] = crop[2].max(x);
        crop[3] = crop[3].max(top);
    }
    let width = (crop[2] - crop[0]) * raw.user_unit;
    let height = (crop[3] - crop[1]) * raw.user_unit;
    if !finite_box(&crop)
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.
        || height <= 0.
    {
        return None;
    }
    Some(Geometry {
        page: PdfPageGeometry {
            media_box: m,
            crop_box: c,
            rotation: raw.rotation,
            user_unit: raw.user_unit,
            display_width_pt: width,
            display_height_pt: height,
        },
        crop,
    })
}
fn geometry_matches(actual: &Option<PdfPageGeometry>, expected: Option<&Geometry>) -> bool {
    match (actual, expected) {
        (None, None) => true,
        (Some(actual), Some(expected)) => {
            same_box(&actual.media_box, &expected.page.media_box)
                && same_box(&actual.crop_box, &expected.page.crop_box)
                && actual.rotation == expected.page.rotation
                && close(actual.user_unit, expected.page.user_unit)
                && close(actual.display_width_pt, expected.page.display_width_pt)
                && close(actual.display_height_pt, expected.page.display_height_pt)
        }
        _ => false,
    }
}

struct VisibleGlyph<'a> {
    glyph: &'a PdfGlyph,
    bbox: [f64; 4],
}
struct Line {
    ordinals: Vec<u32>,
    text: String,
    bbox: [f64; 4],
}
fn has_formula(glyph: &PdfGlyph) -> bool {
    let name = glyph.font_name.to_ascii_lowercase();
    name.contains("math")
        || name.contains("symbol")
        || glyph
            .text
            .chars()
            .any(|value| "=^_∑∫√±²³×÷≤≥≠".contains(value))
}
fn nonblank(value: &str) -> bool {
    value.chars().any(|value| !value.is_whitespace())
}
fn lines(
    visible: &mut [VisibleGlyph<'_>],
    review: &mut bool,
    cancel: &AtomicBool,
) -> Option<Vec<Line>> {
    visible.sort_by(|left, right| {
        left.glyph.media_bbox[1]
            .total_cmp(&right.glyph.media_bbox[1])
            .then(left.glyph.media_bbox[0].total_cmp(&right.glyph.media_bbox[0]))
            .then(left.glyph.ordinal.cmp(&right.glyph.ordinal))
    });
    let mut result = Vec::new();
    let mut start = 0;
    while start < visible.len() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let top = visible[start].glyph.media_bbox[1];
        let mut end = start + 1;
        while end < visible.len() && visible[end].glyph.media_bbox[1] - top <= 3. {
            end += 1;
        }
        let group = &mut visible[start..end];
        group.sort_by(|left, right| {
            left.glyph.media_bbox[0]
                .total_cmp(&right.glyph.media_bbox[0])
                .then(left.glyph.ordinal.cmp(&right.glyph.ordinal))
        });
        let mut text = String::new();
        let mut bbox = group[0].bbox;
        let mut ordinals = Vec::new();
        let mut min_size = f64::INFINITY;
        let mut max_size = f64::NEG_INFINITY;
        for (index, value) in group.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let glyph = value.glyph;
            if index > 0 && nonblank(&group[index - 1].glyph.text) && nonblank(&glyph.text) {
                let gap = glyph.media_bbox[0] - group[index - 1].glyph.media_bbox[2];
                if gap > 3. {
                    text.push(' ');
                }
                if gap > 72. {
                    *review = true;
                }
            }
            if nonblank(&glyph.text) {
                min_size = min_size.min(glyph.font_size);
                max_size = max_size.max(glyph.font_size);
            }
            text.push_str(&glyph.text);
            ordinals.push(glyph.ordinal);
            bbox[0] = bbox[0].min(value.bbox[0]);
            bbox[1] = bbox[1].min(value.bbox[1]);
            bbox[2] = bbox[2].max(value.bbox[2]);
            bbox[3] = bbox[3].max(value.bbox[3]);
        }
        if max_size - min_size > 1. {
            *review = true;
        }
        result.push(Line {
            ordinals,
            text,
            bbox,
        });
        start = end;
    }
    Some(result)
}

#[allow(clippy::too_many_arguments)]
fn validate_page<'a>(
    source: &PdfSourcePage,
    page: &'a PdfPage,
    artifacts: &'a BTreeMap<&str, CheckedArtifact<'a>>,
    referenced: &mut BTreeSet<&'a str>,
    region_ids: &mut BTreeSet<String>,
    limits: &PdfLimits,
    cancel: &AtomicBool,
    violations: &mut Vec<ProtocolViolation>,
) -> u64 {
    let page_ref = format!("page:{}", page.physical_page_index);
    let refs = &[page_ref.as_str()];
    let geometry = source.geometry.as_ref().and_then(geometry);
    if !geometry_matches(&page.geometry, geometry.as_ref()) {
        push(
            violations,
            "PDF_GEOMETRY",
            refs,
            "页面几何必须等于原始盒四角旋转、Crop局部平移和UserUnit缩放结果。",
        );
    }
    if source.decode_error.is_some() || geometry.is_none() {
        if source.decode_error.as_ref().is_none_or(|value| {
            value.trim().is_empty()
                || value.len() > 4096
                || value.chars().any(|value| value.is_control())
        }) || source.glyphs.is_some()
            || source.visual_objects.is_some()
        {
            push(
                violations,
                "PDF_INVENTORY",
                refs,
                "无法解码或几何无效页面必须保留受限原因而不伪造字形和视觉清单。",
            );
        }
        if page.status != UnitStatus::Failed
            || page.glyph_count.is_some()
            || page.text_artifact.is_some()
            || !page.regions.is_empty()
            || !page.excluded_glyph_ordinals.is_empty()
        {
            push(
                violations,
                "PDF_PAGE_STATE",
                refs,
                "解码或几何失败页必须failed且没有假正文、字形数或区域。",
            );
        }
        return 0;
    }
    let geometry = geometry.unwrap();
    let (Some(glyphs), Some(objects)) = (&source.glyphs, &source.visual_objects) else {
        push(
            violations,
            "PDF_INVENTORY",
            refs,
            "已解码页面必须提供完整字形和视觉对象证据。",
        );
        return 0;
    };
    if page.glyph_count.map(u64::from) != Some(glyphs.len() as u64)
        || glyphs
            .iter()
            .enumerate()
            .any(|(index, glyph)| glyph.ordinal as usize != index)
    {
        push(
            violations,
            "PDF_GLYPH_MANIFEST",
            refs,
            "原始字形ordinal必须连续且页字形数必须完整匹配。",
        );
    }
    if glyphs.iter().any(|glyph| {
        !finite_box(&glyph.media_bbox) || !glyph.font_size.is_finite() || glyph.font_size < 0.
    }) {
        push(
            violations,
            "PDF_GLYPH_MANIFEST",
            refs,
            "原始字形坐标和字号必须有限且范围有序。",
        );
        return 0;
    }
    let mut excluded = Vec::new();
    let mut visible = Vec::new();
    let mut review = object_count(objects) > 0;
    for glyph in glyphs {
        if cancelled(cancel, violations) {
            return 0;
        }
        let b = glyph.media_bbox;
        let c = geometry.crop;
        let clipped = [
            b[0].max(c[0]),
            b[1].max(c[1]),
            b[2].min(c[2]),
            b[3].min(c[3]),
        ];
        if clipped[0] >= clipped[2] || clipped[1] >= clipped[3] {
            excluded.push(glyph.ordinal);
            continue;
        }
        // Output float comparison permits arithmetic rounding only. Review
        // gates use the exact primitive intersection so tiny clipping stays
        // visible as incomplete extraction.
        review |= b != clipped
            || !glyph.upright
            || glyph.text.contains("(cid:")
            || glyph.text.contains('\u{fffd}')
            || has_formula(glyph);
        let unit = geometry.page.user_unit;
        let bbox = [
            (clipped[0] - c[0]) * unit,
            (clipped[1] - c[1]) * unit,
            (clipped[2] - c[0]) * unit,
            (clipped[3] - c[1]) * unit,
        ];
        if !finite_box(&bbox) {
            push(
                violations,
                "PDF_GEOMETRY",
                refs,
                "字形显示坐标缩放必须有限。",
            );
            return 0;
        }
        visible.push(VisibleGlyph { glyph, bbox });
    }
    if excluded != page.excluded_glyph_ordinals {
        push(
            violations,
            "PDF_GLYPH_MANIFEST",
            refs,
            "完全裁掉的字形必须精确列在排除清单，不能漏字形。",
        );
    }
    review |= !visible.iter().any(|value| nonblank(&value.glyph.text));
    let Some(lines) = lines(&mut visible, &mut review, cancel) else {
        cancelled(cancel, violations);
        return 0;
    };
    if lines.len() as u64 > u64::from(limits.policy.max_regions) {
        push(violations, "PDF_POLICY", refs, "本页重算行数超过区域政策。");
        return lines.len() as u64;
    }
    let expected_status = if review {
        UnitStatus::NeedsReview
    } else {
        UnitStatus::Parsed
    };
    if page.status != expected_status {
        push(
            violations,
            "PDF_PAGE_STATE",
            refs,
            "页面状态必须诚实反映空文本、视觉对象、裁剪、公式和不可靠顺序门槛。",
        );
    }
    let Some(reference) = &page.text_artifact else {
        push(
            violations,
            "PDF_TEXT",
            refs,
            "已解码页面必须保留实际提取正文资源。",
        );
        return lines.len() as u64;
    };
    referenced.insert(reference.artifact_id.as_str());
    let Some(text) = checked_reference(reference, ArtifactKind::Utf8Text, artifacts) else {
        push(
            violations,
            "PDF_TEXT",
            refs,
            "页正文必须引用真实UTF8资源及相同SHA。",
        );
        return lines.len() as u64;
    };
    if reference.artifact_id != format!("P{}_text", page.physical_page_index) {
        push(
            violations,
            "PDF_TEXT",
            refs,
            "正文资源必须绑定该物理页的固定ID。",
        );
    }
    let mut expected_text = String::new();
    let mut ranges = Vec::new();
    for line in &lines {
        let start = expected_text.len() as u64;
        expected_text.push_str(&line.text);
        ranges.push((start, expected_text.len() as u64));
        expected_text.push('\n');
    }
    if expected_text.as_bytes() != text.bytes {
        push(
            violations,
            "PDF_TEXT",
            refs,
            "页正文必须逐字等于完整字形行清单与固定LF分隔。",
        );
    }
    if page.regions.len() != lines.len() {
        push(
            violations,
            "PDF_REGION",
            refs,
            "每一重算行必须恰有一个区域。",
        );
    }
    for (index, (region, line)) in page.regions.iter().zip(&lines).enumerate() {
        if cancelled(cancel, violations) {
            return lines.len() as u64;
        }
        let (start, end) = ranges[index];
        if !valid_id(&region.region_id)
            || !region_ids.insert(region.region_id.to_ascii_lowercase())
            || region.ordinal as usize != index
            || region.locator.kind != PdfLocatorKind::PdfPageRegion
            || region.locator.physical_page_index != page.physical_page_index
            || !same_box(&region.locator.bbox_pt, &line.bbox)
            || region.glyph_ordinals != line.ordinals
            || region.content.artifact_id != reference.artifact_id
            || region.content.sha256 != reference.sha256
            || region.content.start_byte != start
            || region.content.end_byte != end
            || std::str::from_utf8(text.bytes).ok().is_none_or(|value| {
                !value.is_char_boundary(start as usize) || !value.is_char_boundary(end as usize)
            })
        {
            push(
                violations,
                "PDF_REGION",
                refs,
                "区域必须匹配真实页、完整字形顺序、可见框并集和对应正文UTF8半开范围。",
            );
        }
    }
    lines.len() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    // Author literal primitives and output: expectations never call the validator's recipe.
    fn literal() -> (PdfRequest, PdfResponse, BTreeMap<String, Vec<u8>>) {
        let input = InputFingerprint {
            relative_path: "input/raw.bin".into(),
            byte_length: 3,
            sha256: crate::text::sha256(b"pdf"),
        };
        let request = PdfRequest {
            schema_version: 2,
            protocol: PROTOCOL.into(),
            request_id: "11111111-1111-4111-8111-111111111111".into(),
            operation: PdfOperation::ParsePdf,
            input: input.clone(),
            workspace: WorkspacePolicy {
                artifact_directory: "artifacts".into(),
            },
            artifact_policy: ArtifactPolicy {
                max_file_bytes: 16777216,
                max_total_bytes: 67108864,
                max_files: 64,
            },
            pdf_policy: PdfPolicy::default(),
            options_version: OPTIONS_VERSION.into(),
        };
        let raw = PdfSourceGeometry {
            media_box: [10., 20., 210., 220.],
            crop_box: [20., 30., 200., 210.],
            rotation: 0,
            user_unit: 2.,
        };
        let inventory = PdfInventory {
            schema_version: 1,
            input: input.clone(),
            page_count: 1,
            pages: vec![PdfSourcePage {
                physical_page_index: 0,
                geometry: Some(raw),
                glyphs: Some(vec![
                    PdfGlyph {
                        ordinal: 0,
                        text: "A".into(),
                        media_bbox: [20., 20., 26., 30.],
                        upright: true,
                        font_size: 10.,
                        font_name: "Courier".into(),
                    },
                    PdfGlyph {
                        ordinal: 1,
                        text: "B".into(),
                        media_bbox: [26., 20., 32., 30.],
                        upright: true,
                        font_size: 10.,
                        font_name: "Courier".into(),
                    },
                    PdfGlyph {
                        ordinal: 2,
                        text: "甲".into(),
                        media_bbox: [20., 50., 30., 60.],
                        upright: true,
                        font_size: 10.,
                        font_name: "Courier".into(),
                    },
                ]),
                visual_objects: Some(PdfVisualObjects {
                    images: 0,
                    lines: 0,
                    rectangles: 0,
                    curves: 0,
                    annotations: 0,
                }),
                decode_error: None,
            }],
        };
        let inventory_bytes = serde_json::to_vec(&inventory).unwrap();
        let text = b"AB\n\xe7\x94\xb2\n".to_vec();
        let inventory_sha = crate::text::sha256(&inventory_bytes);
        let text_sha = crate::text::sha256(&text);
        let response = PdfResponse {
            schema_version: 2,
            protocol: PROTOCOL.into(),
            request_id: request.request_id.clone(),
            adapter: PdfAdapter {
                producer: Producer::Parser,
                adapter_id: "pdfplumber-text".into(),
                adapter_version: "1.0.0".into(),
                options_version: OPTIONS_VERSION.into(),
                runtime: expected_runtime(),
            },
            input,
            status: ResponseStatus::Complete,
            page_count: Some(1),
            inventory: Some(ArtifactRef {
                artifact_id: "pdf_inventory".into(),
                sha256: inventory_sha.clone(),
            }),
            pages: vec![PdfPage {
                physical_page_index: 0,
                printed_page_label: None,
                status: UnitStatus::Parsed,
                geometry: Some(PdfPageGeometry {
                    media_box: [10., 20., 210., 220.],
                    crop_box: [20., 30., 200., 210.],
                    rotation: 0,
                    user_unit: 2.,
                    display_width_pt: 360.,
                    display_height_pt: 360.,
                }),
                glyph_count: Some(3),
                excluded_glyph_ordinals: vec![],
                text_artifact: Some(ArtifactRef {
                    artifact_id: "P0_text".into(),
                    sha256: text_sha.clone(),
                }),
                regions: vec![
                    PdfRegion {
                        region_id: "P0_R0".into(),
                        ordinal: 0,
                        locator: PdfLocator {
                            kind: PdfLocatorKind::PdfPageRegion,
                            physical_page_index: 0,
                            bbox_pt: [20., 20., 44., 40.],
                        },
                        content: PdfContent {
                            artifact_id: "P0_text".into(),
                            sha256: text_sha.clone(),
                            start_byte: 0,
                            end_byte: 2,
                        },
                        glyph_ordinals: vec![0, 1],
                    },
                    PdfRegion {
                        region_id: "P0_R1".into(),
                        ordinal: 1,
                        locator: PdfLocator {
                            kind: PdfLocatorKind::PdfPageRegion,
                            physical_page_index: 0,
                            bbox_pt: [20., 80., 40., 100.],
                        },
                        content: PdfContent {
                            artifact_id: "P0_text".into(),
                            sha256: text_sha.clone(),
                            start_byte: 3,
                            end_byte: 6,
                        },
                        glyph_ordinals: vec![2],
                    },
                ],
            }],
            artifacts: vec![
                Artifact {
                    artifact_id: "pdf_inventory".into(),
                    kind: ArtifactKind::Binary,
                    relative_path: "artifacts/pdf_inventory.bin".into(),
                    byte_length: inventory_bytes.len() as u64,
                    sha256: inventory_sha,
                },
                Artifact {
                    artifact_id: "P0_text".into(),
                    kind: ArtifactKind::Utf8Text,
                    relative_path: "artifacts/P0_text.txt".into(),
                    byte_length: 7,
                    sha256: text_sha,
                },
            ],
            issues: vec![],
        };
        (
            request,
            response,
            BTreeMap::from([
                ("artifacts/pdf_inventory.bin".into(), inventory_bytes),
                ("artifacts/P0_text.txt".into(), text),
            ]),
        )
    }
    fn check(response: &PdfResponse) -> Vec<ProtocolViolation> {
        let (request, _, bytes) = literal();
        validate_pdf(
            &request,
            response,
            &bytes,
            &PdfLimits::default(),
            &AtomicBool::new(false),
        )
    }
    fn has(issues: &[ProtocolViolation], rule: &str) {
        assert!(
            issues.iter().any(|issue| issue.rule_id == rule),
            "missing {rule}: {issues:?}"
        );
    }
    fn update_inventory(
        response: &mut PdfResponse,
        bytes: &mut BTreeMap<String, Vec<u8>>,
        change: impl FnOnce(&mut PdfInventory),
    ) {
        let mut inventory: PdfInventory =
            serde_json::from_slice(&bytes["artifacts/pdf_inventory.bin"]).unwrap();
        change(&mut inventory);
        let actual = serde_json::to_vec(&inventory).unwrap();
        let sha = crate::text::sha256(&actual);
        response.inventory.as_mut().unwrap().sha256 = sha.clone();
        response.artifacts[0].sha256 = sha;
        response.artifacts[0].byte_length = actual.len() as u64;
        bytes.insert("artifacts/pdf_inventory.bin".into(), actual);
    }
    fn run(
        request: &PdfRequest,
        response: &PdfResponse,
        bytes: &BTreeMap<String, Vec<u8>>,
    ) -> Vec<ProtocolViolation> {
        validate_pdf(
            request,
            response,
            bytes,
            &PdfLimits::default(),
            &AtomicBool::new(false),
        )
    }
    #[test]
    fn literal_crop_userunit_unicode_requires_exact_independent_output() {
        let (_, response, _) = literal();
        assert!(check(&response).is_empty());
    }
    #[test]
    fn changed_bbox_inside_page_is_still_rejected() {
        let (_, mut response, _) = literal();
        response.pages[0].regions[0].locator.bbox_pt = [21., 20., 45., 40.];
        has(&check(&response), "PDF_REGION");
    }
    #[test]
    fn wire_rejects_unknown_fields_duplicate_keys_objects_as_arrays_and_enum_maps() {
        let (request, response, _) = literal();
        let req = serde_json::to_value(request).unwrap();
        for path in [
            "",
            "/input",
            "/workspace",
            "/artifact_policy",
            "/pdf_policy",
        ] {
            let mut changed = req.clone();
            changed
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unknown".into(), json!(true));
            assert!(
                serde_json::from_value::<PdfRequest>(changed).is_err(),
                "{path}"
            );
        }
        let res = serde_json::to_value(response).unwrap();
        for path in [
            "",
            "/adapter",
            "/adapter/runtime",
            "/input",
            "/inventory",
            "/pages/0",
            "/pages/0/geometry",
            "/pages/0/text_artifact",
            "/pages/0/regions/0",
            "/pages/0/regions/0/locator",
            "/pages/0/regions/0/content",
            "/artifacts/0",
        ] {
            let mut changed = res.clone();
            changed
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unknown".into(), json!(true));
            assert!(
                serde_json::from_value::<PdfResponse>(changed).is_err(),
                "unknown {path}"
            );
            let mut changed = res.clone();
            let array: Vec<Value> = changed
                .pointer(path)
                .unwrap()
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect();
            *changed.pointer_mut(path).unwrap() = Value::Array(array);
            assert!(
                serde_json::from_value::<PdfResponse>(changed).is_err(),
                "array {path}"
            );
        }
        let raw = serde_json::to_string(&res).unwrap();
        assert!(
            serde_json::from_str::<PdfResponse>(&raw.replacen(
                "\"schema_version\":2",
                "\"schema_version\":2,\"schema_version\":2",
                1
            ))
            .is_err()
        );
        for path in [
            "/status",
            "/adapter/producer",
            "/pages/0/status",
            "/pages/0/regions/0/locator/kind",
            "/artifacts/0/kind",
        ] {
            let mut changed = res.clone();
            *changed.pointer_mut(path).unwrap() = json!({"parsed": null});
            assert!(
                serde_json::from_value::<PdfResponse>(changed).is_err(),
                "enum {path}"
            );
        }
    }
    #[test]
    fn every_nullable_field_is_required_and_explicit_null_is_accepted() {
        let (_, response, bytes) = literal();
        let res = serde_json::to_value(response).unwrap();
        for (path, field) in [
            ("", "page_count"),
            ("", "inventory"),
            ("/pages/0", "printed_page_label"),
            ("/pages/0", "geometry"),
            ("/pages/0", "glyph_count"),
            ("/pages/0", "text_artifact"),
        ] {
            let mut changed = res.clone();
            changed
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                serde_json::from_value::<PdfResponse>(changed).is_err(),
                "missing {path}/{field}"
            );
            let mut changed = res.clone();
            changed.pointer_mut(path).unwrap()[field] = Value::Null;
            assert!(
                serde_json::from_value::<PdfResponse>(changed).is_ok(),
                "null {path}/{field}"
            );
        }
        let inventory: Value =
            serde_json::from_slice(&bytes["artifacts/pdf_inventory.bin"]).unwrap();
        for field in ["geometry", "glyphs", "visual_objects", "decode_error"] {
            let mut changed = inventory.clone();
            changed["pages"][0].as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<PdfInventory>(changed).is_err(),
                "{field}"
            );
        }
    }
    #[test]
    fn every_locked_runtime_component_and_parser_identity_are_required() {
        let (_, response, _) = literal();
        for component in [
            "python",
            "pdfplumber",
            "pdfminer",
            "pypdfium2",
            "pillow",
            "cryptography",
            "cffi",
            "pycparser",
            "charset_normalizer",
        ] {
            let mut changed = serde_json::to_value(&response).unwrap();
            changed["adapter"]["runtime"][component] = json!("future");
            has(
                &check(&serde_json::from_value(changed).unwrap()),
                "PDF_ADAPTER",
            );
        }
        let mut changed = response;
        changed.adapter.producer = Producer::Fake;
        has(&check(&changed), "PDF_ADAPTER");
    }
    #[test]
    fn missing_pages_wrong_physical_order_and_printed_labels_are_rejected() {
        let (_, mut response, _) = literal();
        response.pages[0].physical_page_index = 1;
        has(&check(&response), "PDF_PAGE_MANIFEST");
        response.pages[0].physical_page_index = 0;
        response.pages[0].printed_page_label = Some("1".into());
        has(&check(&response), "PDF_PAGE_MANIFEST");
        response.pages.clear();
        has(&check(&response), "PDF_PAGE_MANIFEST");
    }
    #[test]
    fn line_order_coverage_and_utf8_ranges_cannot_be_refreshed_away() {
        for mutate in 0..5 {
            let (_, mut response, _) = literal();
            match mutate {
                0 => response.pages[0].regions.swap(0, 1),
                1 => response.pages[0].regions[0].glyph_ordinals = vec![1, 0],
                2 => response.pages[0].regions[0].glyph_ordinals = vec![0, 0],
                3 => response.pages[0].regions[1].content.end_byte = 4,
                _ => response.pages[0].regions[0].locator.physical_page_index = 1,
            }
            has(&check(&response), "PDF_REGION");
        }
    }
    #[test]
    fn actual_text_must_match_the_complete_recomputed_bytes() {
        let (request, mut response, mut bytes) = literal();
        let text = b"A\n\xe7\x94\xb2\n".to_vec();
        let sha = crate::text::sha256(&text);
        response.artifacts[1].byte_length = 6;
        response.artifacts[1].sha256 = sha.clone();
        response.pages[0].text_artifact.as_mut().unwrap().sha256 = sha.clone();
        for region in &mut response.pages[0].regions {
            region.content.sha256 = sha.clone();
        }
        bytes.insert("artifacts/P0_text.txt".into(), text);
        has(&run(&request, &response, &bytes), "PDF_TEXT");
    }
    #[test]
    fn all_four_rotations_use_raw_nonzero_origins_asymmetric_crop_and_userunit() {
        for (rotation, width, height, first, second) in [
            (
                0,
                320.,
                460.,
                [80., 40., 104., 60.],
                [80., 100., 100., 120.],
            ),
            (
                90,
                460.,
                320.,
                [60., 80., 84., 100.],
                [60., 140., 80., 160.],
            ),
            (
                180,
                320.,
                460.,
                [80., 60., 104., 80.],
                [80., 120., 100., 140.],
            ),
            (
                270,
                460.,
                320.,
                [40., 80., 64., 100.],
                [40., 140., 60., 160.],
            ),
        ] {
            let (request, mut response, mut bytes) = literal();
            update_inventory(&mut response, &mut bytes, |inventory| {
                inventory.pages[0].geometry = Some(PdfSourceGeometry {
                    media_box: [10., 20., 210., 320.],
                    crop_box: [30., 50., 190., 280.],
                    rotation,
                    user_unit: 2.,
                });
                let glyphs = inventory.pages[0].glyphs.as_mut().unwrap();
                glyphs[0].media_bbox = [60., 60., 66., 70.];
                glyphs[1].media_bbox = [66., 60., 72., 70.];
                glyphs[2].media_bbox = [60., 90., 70., 100.];
            });
            response.pages[0].geometry = Some(PdfPageGeometry {
                media_box: [10., 20., 210., 320.],
                crop_box: [30., 50., 190., 280.],
                rotation,
                user_unit: 2.,
                display_width_pt: width,
                display_height_pt: height,
            });
            response.pages[0].regions[0].locator.bbox_pt = first;
            response.pages[0].regions[1].locator.bbox_pt = second;
            assert!(
                run(&request, &response, &bytes).is_empty(),
                "rotation {rotation}"
            );
        }
    }
    #[test]
    fn excluded_glyphs_are_preserved_and_exactly_accounted() {
        let (request, mut response, mut bytes) = literal();
        update_inventory(&mut response, &mut bytes, |inventory| {
            inventory.pages[0].glyphs.as_mut().unwrap().push(PdfGlyph {
                ordinal: 3,
                text: "hidden".into(),
                media_bbox: [0., 0., 5., 5.],
                upright: false,
                font_size: 30.,
                font_name: "Math".into(),
            })
        });
        response.pages[0].glyph_count = Some(4);
        response.pages[0].excluded_glyph_ordinals = vec![3];
        assert!(run(&request, &response, &bytes).is_empty());
        response.pages[0].excluded_glyph_ordinals.clear();
        has(&run(&request, &response, &bytes), "PDF_GLYPH_MANIFEST");
    }
    #[test]
    fn partial_crop_and_visual_objects_require_review_and_a_local_issue() {
        for visual in [false, true] {
            let (request, mut response, mut bytes) = literal();
            update_inventory(&mut response, &mut bytes, |inventory| {
                if visual {
                    inventory.pages[0].visual_objects.as_mut().unwrap().images = 1;
                } else {
                    inventory.pages[0].glyphs.as_mut().unwrap()[0].media_bbox[0] = 8.;
                }
            });
            if !visual {
                response.pages[0].regions[0].locator.bbox_pt[0] = 0.;
            }
            has(&run(&request, &response, &bytes), "PDF_PAGE_STATE");
            response.status = ResponseStatus::Partial;
            response.pages[0].status = UnitStatus::NeedsReview;
            response.issues.push(PdfIssue {
                code: "PDF_GLYPH_REVIEW".into(),
                message: "Visible page needs review.".into(),
                page_indices: vec![0],
            });
            let issues = run(&request, &response, &bytes);
            assert_eq!(
                issues
                    .iter()
                    .map(|issue| issue.rule_id.as_str())
                    .collect::<Vec<_>>(),
                ["PDF_REPORTED_INCOMPLETE"],
                "{issues:?}"
            );
        }
    }
    #[test]
    fn sub_tolerance_partial_clipping_still_requires_review() {
        let (request, mut response, mut bytes) = literal();
        update_inventory(&mut response, &mut bytes, |inventory| {
            inventory.pages[0].glyphs.as_mut().unwrap()[0].media_bbox[0] = 9.9999999
        });
        response.pages[0].regions[0].locator.bbox_pt[0] = 0.;
        has(&run(&request, &response, &bytes), "PDF_PAGE_STATE");
        response.status = ResponseStatus::Partial;
        response.pages[0].status = UnitStatus::NeedsReview;
        response.issues.push(PdfIssue {
            code: "PDF_GLYPH_REVIEW".into(),
            message: "A visible glyph was clipped.".into(),
            page_indices: vec![0],
        });
        let issues = run(&request, &response, &bytes);
        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.rule_id.as_str())
                .collect::<Vec<_>>(),
            ["PDF_REPORTED_INCOMPLETE"],
            "{issues:?}"
        );
    }
    #[test]
    fn decode_failure_retains_the_physical_page_without_fabricated_content() {
        let (request, mut response, mut bytes) = literal();
        update_inventory(&mut response, &mut bytes, |inventory| {
            let page = &mut inventory.pages[0];
            page.glyphs = None;
            page.visual_objects = None;
            page.decode_error = Some("Unsupported content filter".into());
        });
        response.status = ResponseStatus::Partial;
        response.pages[0].status = UnitStatus::Failed;
        response.pages[0].glyph_count = None;
        response.pages[0].text_artifact = None;
        response.pages[0].regions.clear();
        response.artifacts.pop();
        bytes.remove("artifacts/P0_text.txt");
        response.issues.push(PdfIssue {
            code: "PDF_PAGE_DECODE".into(),
            message: "Page decode failed.".into(),
            page_indices: vec![0],
        });
        let issues = run(&request, &response, &bytes);
        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.rule_id.as_str())
                .collect::<Vec<_>>(),
            ["PDF_REPORTED_INCOMPLETE"],
            "{issues:?}"
        );
    }
    #[test]
    fn no_inventory_failed_document_is_legal_but_explicitly_incomplete() {
        let (request, mut response, _) = literal();
        response.status = ResponseStatus::Failed;
        response.page_count = None;
        response.inventory = None;
        response.pages.clear();
        response.artifacts.clear();
        response.issues.push(PdfIssue {
            code: "PDF_DOCUMENT_DECODE".into(),
            message: "Document could not be enumerated.".into(),
            page_indices: vec![],
        });
        let issues = run(&request, &response, &BTreeMap::new());
        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.rule_id.as_str())
                .collect::<Vec<_>>(),
            ["PDF_REPORTED_INCOMPLETE"]
        );
        response.issues[0].page_indices.push(0);
        has(&run(&request, &response, &BTreeMap::new()), "PDF_ISSUE");
    }
    #[test]
    fn resource_manifest_integrity_inventory_shape_and_cancellation_are_preserved() {
        let (request, response, bytes) = literal();
        let mut changed = response.clone();
        changed.artifacts[0].sha256 = "0".repeat(64);
        has(&run(&request, &changed, &bytes), "PDF_ARTIFACT_INTEGRITY");
        let mut changed = bytes.clone();
        changed.insert("artifacts/unknown.bin".into(), vec![]);
        has(&run(&request, &response, &changed), "PDF_ARTIFACT_MANIFEST");
        let issues = validate_pdf(
            &request,
            &response,
            &bytes,
            &PdfLimits::default(),
            &AtomicBool::new(true),
        );
        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.rule_id.as_str())
                .collect::<Vec<_>>(),
            ["SIDECAR_CANCELLED"]
        );
        let mut changed = response.clone();
        let mut changed_bytes = bytes;
        let malformed = br#"{"schema_version":1,"schema_version":1,"private":"sensitive payload must not leak"}"#.to_vec();
        let sha = crate::text::sha256(&malformed);
        changed.artifacts[0].sha256 = sha.clone();
        changed.artifacts[0].byte_length = malformed.len() as u64;
        changed.inventory.as_mut().unwrap().sha256 = sha;
        changed_bytes.insert("artifacts/pdf_inventory.bin".into(), malformed);
        let issues = run(&request, &changed, &changed_bytes);
        has(&issues, "PDF_INVENTORY_JSON");
        assert!(
            !issues
                .iter()
                .any(|issue| issue.message.contains("sensitive")
                    || issue.message.contains("private"))
        );
    }
    #[test]
    fn inventory_wire_objects_and_issue_objects_are_strict_at_every_depth() {
        let (_, response, bytes) = literal();
        let inventory: Value =
            serde_json::from_slice(&bytes["artifacts/pdf_inventory.bin"]).unwrap();
        for path in [
            "",
            "/input",
            "/pages/0",
            "/pages/0/geometry",
            "/pages/0/glyphs/0",
            "/pages/0/visual_objects",
        ] {
            let mut changed = inventory.clone();
            changed
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unknown".into(), json!(1));
            assert!(
                serde_json::from_value::<PdfInventory>(changed).is_err(),
                "unknown inventory {path}"
            );
            let mut changed = inventory.clone();
            let fields = changed
                .pointer(path)
                .unwrap()
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect();
            *changed.pointer_mut(path).unwrap() = Value::Array(fields);
            assert!(
                serde_json::from_value::<PdfInventory>(changed).is_err(),
                "array inventory {path}"
            );
        }
        for (path, invalid) in [
            ("/schema_version", json!(1.0)),
            ("/page_count", json!(-1)),
            ("/pages/0/glyphs/0/ordinal", json!(0.0)),
            ("/pages/0/visual_objects/images", json!(-1)),
            ("/pages/0/geometry/rotation", json!(90.0)),
        ] {
            let mut changed = inventory.clone();
            *changed.pointer_mut(path).unwrap() = invalid;
            assert!(
                serde_json::from_value::<PdfInventory>(changed).is_err(),
                "integer {path}"
            );
        }
        let raw = serde_json::to_string(&inventory).unwrap().replacen(
            "\"ordinal\":0",
            "\"ordinal\":0,\"ordinal\":0",
            1,
        );
        assert!(serde_json::from_str::<PdfInventory>(&raw).is_err());
        let mut response = serde_json::to_value(response).unwrap();
        response["issues"] = json!([{ "code": "PDF_REVIEW", "message": "Review required", "page_indices": [0], "unknown": true }]);
        assert!(serde_json::from_value::<PdfResponse>(response).is_err());
        assert!(
            serde_json::from_str::<PdfIssue>(r#"["PDF_REVIEW","Review required",[0]]"#).is_err()
        );
    }
    #[test]
    fn a_real_empty_page_keeps_empty_utf8_artifact_and_is_only_incomplete() {
        let (request, mut response, mut bytes) = literal();
        update_inventory(&mut response, &mut bytes, |inventory| {
            inventory.pages[0].glyphs = Some(vec![])
        });
        let sha = crate::text::sha256(b"");
        response.status = ResponseStatus::Partial;
        response.pages[0].status = UnitStatus::NeedsReview;
        response.pages[0].glyph_count = Some(0);
        response.pages[0].regions.clear();
        response.pages[0].text_artifact.as_mut().unwrap().sha256 = sha.clone();
        response.artifacts[1].sha256 = sha;
        response.artifacts[1].byte_length = 0;
        bytes.insert("artifacts/P0_text.txt".into(), vec![]);
        response.issues.push(PdfIssue {
            code: "PDF_EMPTY_TEXT".into(),
            message: "No verified text layer.".into(),
            page_indices: vec![0],
        });
        let issues = run(&request, &response, &bytes);
        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.rule_id.as_str())
                .collect::<Vec<_>>(),
            ["PDF_REPORTED_INCOMPLETE"],
            "{issues:?}"
        );
        response.pages[0].status = UnitStatus::Parsed;
        response.status = ResponseStatus::Complete;
        response.issues.clear();
        has(&run(&request, &response, &bytes), "PDF_PAGE_STATE");
    }
    #[test]
    fn visible_upright_font_and_formula_gates_cannot_be_claimed_parsed() {
        for mode in 0..4 {
            let (request, mut response, mut bytes) = literal();
            update_inventory(&mut response, &mut bytes, |inventory| {
                let glyphs = inventory.pages[0].glyphs.as_mut().unwrap();
                match mode {
                    0 => glyphs[0].upright = false,
                    1 => glyphs[1].font_size = 11.01,
                    2 => glyphs[1].font_name = "Symbol".into(),
                    _ => glyphs[1].font_name = "OpenMath".into(),
                }
            });
            has(&run(&request, &response, &bytes), "PDF_PAGE_STATE");
            response.status = ResponseStatus::Partial;
            response.pages[0].status = UnitStatus::NeedsReview;
            response.issues.push(PdfIssue {
                code: "PDF_REVIEW".into(),
                message: "Visible text needs review.".into(),
                page_indices: vec![0],
            });
            let issues = run(&request, &response, &bytes);
            assert_eq!(
                issues
                    .iter()
                    .map(|issue| issue.rule_id.as_str())
                    .collect::<Vec<_>>(),
                ["PDF_REPORTED_INCOMPLETE"],
                "{issues:?}"
            );
        }
        let (request, mut response, mut bytes) = literal();
        update_inventory(&mut response, &mut bytes, |inventory| {
            inventory.pages[0].glyphs.as_mut().unwrap()[2].font_size = 30.
        });
        assert!(
            run(&request, &response, &bytes).is_empty(),
            "different heading size across lines must be parsed"
        );
    }
    #[test]
    fn spacing_keeps_wide_gap_glyphs_and_line_grouping_uses_first_top() {
        let (request, mut response, mut bytes) = literal();
        update_inventory(&mut response, &mut bytes, |inventory| {
            let glyphs = inventory.pages[0].glyphs.as_mut().unwrap();
            glyphs[1].media_bbox = [105., 23., 111., 33.];
            glyphs[2].media_bbox = [20., 26., 30., 36.];
        });
        // Top 20 and 23 form the first line; top26 does not chain into it.
        // Gap79 inserts one space and requests review without losing B.
        let text = b"A B\n\xe7\x94\xb2\n".to_vec();
        let sha = crate::text::sha256(&text);
        response.pages[0].regions[0].locator.bbox_pt = [20., 20., 202., 46.];
        response.pages[0].regions[0].content.end_byte = 3;
        response.pages[0].regions[1].locator.bbox_pt = [20., 32., 40., 52.];
        response.pages[0].regions[1].content.start_byte = 4;
        response.pages[0].regions[1].content.end_byte = 7;
        response.pages[0].text_artifact.as_mut().unwrap().sha256 = sha.clone();
        for region in &mut response.pages[0].regions {
            region.content.sha256 = sha.clone();
        }
        response.artifacts[1].sha256 = sha;
        response.artifacts[1].byte_length = 8;
        bytes.insert("artifacts/P0_text.txt".into(), text);
        response.status = ResponseStatus::Partial;
        response.pages[0].status = UnitStatus::NeedsReview;
        response.issues.push(PdfIssue {
            code: "PDF_READING_ORDER_REVIEW".into(),
            message: "Wide gap needs review.".into(),
            page_indices: vec![0],
        });
        let issues = run(&request, &response, &bytes);
        assert_eq!(
            issues
                .iter()
                .map(|issue| issue.rule_id.as_str())
                .collect::<Vec<_>>(),
            ["PDF_REPORTED_INCOMPLETE"],
            "{issues:?}"
        );
    }
    #[test]
    fn bad_geometry_is_failed_without_clamping_or_inventing_a_location() {
        for mode in 0..4 {
            let (request, mut response, mut bytes) = literal();
            update_inventory(&mut response, &mut bytes, |inventory| {
                let source = &mut inventory.pages[0];
                let raw = source.geometry.as_mut().unwrap();
                match mode {
                    0 => raw.crop_box[0] = -1.,
                    1 => raw.rotation = 45,
                    2 => raw.user_unit = 0.,
                    _ => raw.user_unit = 65.,
                }
                source.glyphs = None;
                source.visual_objects = None;
                source.decode_error = Some("Invalid page geometry".into());
            });
            response.status = ResponseStatus::Partial;
            response.pages[0].status = UnitStatus::Failed;
            response.pages[0].geometry = None;
            response.pages[0].glyph_count = None;
            response.pages[0].text_artifact = None;
            response.pages[0].regions.clear();
            response.artifacts.pop();
            bytes.remove("artifacts/P0_text.txt");
            response.issues.push(PdfIssue {
                code: "PDF_PAGE_GEOMETRY".into(),
                message: "Invalid page geometry.".into(),
                page_indices: vec![0],
            });
            let issues = run(&request, &response, &bytes);
            assert_eq!(
                issues
                    .iter()
                    .map(|issue| issue.rule_id.as_str())
                    .collect::<Vec<_>>(),
                ["PDF_REPORTED_INCOMPLETE"],
                "mode {mode}: {issues:?}"
            );
        }
    }
    #[test]
    fn issue_page_state_and_each_pdf_budget_are_checked_in_both_directions() {
        let (request, mut response, bytes) = literal();
        response.issues.push(PdfIssue {
            code: "PDF_REVIEW".into(),
            message: "Cannot cite a parsed page.".into(),
            page_indices: vec![0],
        });
        has(&run(&request, &response, &bytes), "PDF_ISSUE");
        response.issues.clear();
        response.pages[0].status = UnitStatus::NeedsReview;
        response.status = ResponseStatus::Partial;
        has(&run(&request, &response, &bytes), "PDF_ISSUE");
        for mode in 0..4 {
            let mut limits = PdfLimits::default();
            let mut request = request.clone();
            match mode {
                0 => limits.policy.max_pages = 0,
                1 => limits.policy.max_glyphs = 2,
                2 => limits.policy.max_regions = 1,
                _ => limits.policy.max_objects = 0,
            }
            request.pdf_policy = limits.policy;
            let (_, mut response, mut bytes) = literal();
            if mode == 3 {
                update_inventory(&mut response, &mut bytes, |inventory| {
                    inventory.pages[0].visual_objects.as_mut().unwrap().lines = 1
                });
            }
            has(
                &validate_pdf(
                    &request,
                    &response,
                    &bytes,
                    &limits,
                    &AtomicBool::new(false),
                ),
                "PDF_POLICY",
            );
        }
    }
    #[test]
    fn public_core_policy_only_accepts_transport_tightening() {
        let (mut request, response, bytes) = literal();
        let mut limits = PdfLimits::default();
        limits.transport.max_artifact_bytes += 1;
        request.artifact_policy.max_file_bytes = limits.transport.max_artifact_bytes;
        has(
            &validate_pdf(
                &request,
                &response,
                &bytes,
                &limits,
                &AtomicBool::new(false),
            ),
            "PDF_POLICY",
        );
    }
}
