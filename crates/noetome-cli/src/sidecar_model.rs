//! Strict single-request parser protocol. Synthetic producers are test-only.
use serde::{Deserialize, Serialize};

macro_rules! object {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $(pub $field: $ty),* }
    };
}
object!(Fingerprint {
    sha256: String,
    byte_length: u64
});
object!(Input {
    path: String,
    access: InputAccess,
    fingerprint: Fingerprint
});
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InputAccess {
    ReadOnlySingleFile,
}
object!(Workspace {
    path: String,
    capability_id: String,
    access: WorkspaceAccess
});
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceAccess {
    ExclusiveNewDirectory,
}
object!(Limits {
    request_bytes: u64,
    response_bytes: u64,
    diagnostic_bytes: u64,
    input_bytes: u64,
    resource_bytes: u64,
    total_resource_bytes: u64,
    resources: u32,
    units: u32,
    issues: u32,
    timeout_ms: u64,
});
impl Default for Limits {
    fn default() -> Self {
        Self {
            request_bytes: 64 * 1024,
            response_bytes: 4 * 1024 * 1024,
            diagnostic_bytes: 64 * 1024,
            input_bytes: 64 * 1024 * 1024,
            resource_bytes: 16 * 1024 * 1024,
            total_resource_bytes: 64 * 1024 * 1024,
            resources: 256,
            units: 10000,
            issues: 1000,
            timeout_ms: 30000,
        }
    }
}
object!(Request {
    protocol_version: u32,
    request_id: String,
    input: Input,
    workspace: Workspace,
    limits: Limits
});
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Producer {
    Parser,
    Fake,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    SourceUnitsV1,
}
object!(Parser { name: String, version: String, capability_version: u32, capabilities: Vec<Capability> });
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ParseStatus {
    Parsed,
    Failed,
    NeedsReview,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UnitKind {
    Text,
    Formatting,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Utf8Text,
    Binary,
}
object!(Resource {
    resource_id: String,
    relative_path: String,
    kind: ResourceKind,
    fingerprint: Fingerprint
});
object!(TextRange {
    resource_id: String,
    start_byte: u64,
    end_byte: u64
});
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unit {
    pub unit_id: String,
    pub ordinal: u32,
    pub kind: UnitKind,
    pub status: ParseStatus,
    #[serde(deserialize_with = "required_nullable")]
    pub text: Option<TextRange>,
    pub resource_ids: Vec<String>,
}
fn required_nullable<'de, D: serde::Deserializer<'de>>(
    de: D,
) -> Result<Option<TextRange>, D::Error> {
    Option::deserialize(de)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Warning,
    Blocking,
}
object!(Issue { code: String, severity: Severity, message: String, unit_ids: Vec<String>, resource_ids: Vec<String> });
object!(Response {
    protocol_version: u32, request_id: String, input_fingerprint: Fingerprint,
    workspace_capability_id: String, producer: Producer, parser: Parser,
    unit_count: u32, units: Vec<Unit>, resources: Vec<Resource>, issues: Vec<Issue>,
});
#[derive(Debug, Clone)]
pub struct SidecarError {
    pub code: &'static str,
    pub message: String,
    pub diagnostics: Vec<u8>,
}
impl SidecarError {
    pub(crate) fn new(code: &'static str, message: impl ToString) -> Self {
        Self {
            code,
            message: message.to_string(),
            diagnostics: Vec::new(),
        }
    }
}
impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for SidecarError {}
