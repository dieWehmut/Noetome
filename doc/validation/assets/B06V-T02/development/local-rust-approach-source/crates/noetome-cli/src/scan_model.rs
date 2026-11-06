//! Explicit selected scan provenance and author declarations, never an OCR answer oracle.
use crate::{
    formula_model::{Profile, Symbol},
    sidecar_model::Fingerprint,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
fn one<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    match u32::deserialize(d)? {
        1 => Ok(1),
        _ => Err(serde::de::Error::custom("unsupported scan version")),
    }
}
fn purpose<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == "scan_formula" {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("unsupported scan purpose"))
    }
}
fn author<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == "author_scan_declaration" {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(
            "unsupported scan declaration producer",
        ))
    }
}
macro_rules! dto {
    ($name:ident { $($(#[$attr:meta])* $field:ident:$ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct $name { $($(#[$attr])* pub(crate) $field:$ty),* }
    };
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceKind {
    ScannedPage,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Legibility {
    Clear,
    Unreadable,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UncertaintyKind {
    Sign,
    Subscript,
    Symbol,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Representation {
    NativeMath,
    FormulaImage,
}
dto!(SourceFile {
    file: String,
    sha256: String
});
dto!(Page {
    file: String,
    sha256: String,
    page: u32,
    canvas: [u32; 2]
});
dto!(Region {
    file: String,
    sha256: String,
    bounds: [u32; 4],
    legibility: Legibility
});
dto!(ScanCase {
    #[serde(deserialize_with="one")] schema_version:u32,
    #[serde(deserialize_with="purpose")] purpose:String,
    selection_id:String, title:String, source_kind:SourceKind,
    #[serde(deserialize_with="nullable")] pdf:Option<SourceFile>,
    page:Page, region:Region, semantics:SourceFile
});
dto!(Uncertainty {
    kind: UncertaintyKind,
    description: String
});
dto!(Semantics {
    #[serde(deserialize_with="one")] schema_version:u32,
    #[serde(deserialize_with="author")] producer:String,
    #[serde(deserialize_with="nullable")] expression:Option<String>,
    symbols:Vec<Symbol>, conditions:Vec<String>, ranges:Vec<String>, exceptions:Vec<String>, uncertainties:Vec<Uncertainty>
});
dto!(Decision {
    #[serde(deserialize_with="nullable")] representation:Option<Representation>,
    fallback_reasons:Vec<String>, compatibility_rewrites:Vec<String>, issue_codes:Vec<String>
});
dto!(ScanPlan {
    #[serde(deserialize_with="one")] schema_version:u32,
    producer:String, scope:String, request_id:String,
    case:ScanCase, semantics:Semantics, decision:Decision, tokens:Vec<String>, recognized_expression:String,
    #[serde(deserialize_with="nullable")] compiled_expression:Option<String>
});
dto!(Mapping {
    kind: String,
    source_field: String,
    start_byte: u64,
    end_byte: u64
});
dto!(RenderedFile { relative_path:String, content_sha256:String, byte_length:u64, mappings:Vec<Mapping> });
dto!(Checks {
    source_preservation: String,
    region_match: String,
    recognition: String,
    expression_match: String,
    symbol_definitions: String,
    semantic_review: String,
    note_plan: String,
    rendering: String,
    saved_output: String
});
dto!(RecognitionIssue {
    code: String,
    message: String
});
dto!(RecognitionLimits {
    response_bytes: u64,
    diagnostic_bytes: u64,
    timeout_ms: u64,
    max_tex_bytes: u64
});
dto!(RecognitionInput {
    path: String,
    sha256: String,
    byte_length: u64
});
dto!(RecognitionRequest {
    #[serde(deserialize_with = "one")]
    schema_version: u32,
    request_id: String,
    input: RecognitionInput,
    model_directory: String,
    limits: RecognitionLimits
});
dto!(RecognitionModel { release:String, files:BTreeMap<String,Fingerprint> });
dto!(RecognitionRuntime { python:String, packages:BTreeMap<String,String> });
dto!(RecognitionPolicy {
    device: String,
    seed: u64,
    temperature: f64,
    resize: bool,
    max_seq_len: u64,
    decoding: String,
    network_calls: u64,
    paid_calls: u64,
    automatic_downloads: bool,
    clipboard: bool
});
dto!(RecognitionResponse {
    #[serde(deserialize_with="one")] schema_version:u32,
    producer:String, package_version:String, request_id:String, input:Fingerprint,
    model:RecognitionModel,
    #[serde(deserialize_with="nullable")] recognized_tex:Option<String>,
    issues:Vec<RecognitionIssue>, elapsed_ms:u64, status:String,
    #[serde(deserialize_with="nullable")] confidence:Option<f64>,
    runtime:RecognitionRuntime, policy:RecognitionPolicy
});
dto!(ScanReport {
    #[serde(deserialize_with="one")] schema_version:u32,
    producer:String, scope:String, request_id:String, status:String, preview_status:String,
    knowledge_status:String, scientific_quality:String, published:bool, network_calls:usize, paid_calls:usize,
    case:ScanCase, semantics:Semantics, profile:Profile, checks:Checks, decision:Decision,
    #[serde(deserialize_with="nullable")] recognition:Option<RecognitionResponse>,
    recognition_invocations:usize, recognition_request_fingerprint:Fingerprint,
    #[serde(deserialize_with="nullable")] recognition_response_fingerprint:Option<Fingerprint>,
    runtime_inputs:BTreeMap<String,Fingerprint>,
    #[serde(deserialize_with="nullable")] tokens:Option<Vec<String>>,
    #[serde(deserialize_with="nullable")] plan_fingerprint:Option<Fingerprint>,
    inputs:BTreeMap<String,Fingerprint>, files:Vec<RenderedFile>, artifacts:BTreeMap<String,Fingerprint>
});
