//! Explicitly selected formula sources; no PDF recovery or scientific verification.
use crate::sidecar_model::Fingerprint;
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
        _ => Err(serde::de::Error::custom("unsupported formula version")),
    }
}
fn purpose<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = String::deserialize(d)?;
    if v == "formula_fidelity" {
        Ok(v)
    } else {
        Err(serde::de::Error::custom("unsupported formula purpose"))
    }
}
macro_rules! dto {
    ($name:ident {$($(#[$attr:meta])* $field:ident:$ty:ty),* $(,)?}) => {
        #[derive(Debug,Clone,Serialize,Deserialize,PartialEq)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct $name { $($(#[$attr])* pub(crate) $field:$ty),* }
    };
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceKind {
    Tex,
    FormulaRegion,
    ExperimentalCurve,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Legibility {
    Clear,
    Unreadable,
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
dto!(Region {
    file: String,
    sha256: String,
    page: u32,
    canvas: [u32; 2],
    bounds: [u32; 4],
    legibility: Legibility
});
dto!(FormulaCase {
    #[serde(deserialize_with="one")] schema_version:u32,
    #[serde(deserialize_with="purpose")] purpose:String,
    selection_id:String,title:String,source_kind:SourceKind,
    #[serde(deserialize_with="nullable")] tex:Option<SourceFile>,
    semantics:SourceFile,
    #[serde(deserialize_with="nullable")] region:Option<Region>,
    data_available:bool
});
dto!(Symbol {
    symbol:String,
    #[serde(deserialize_with="nullable")] definition:Option<String>,
    #[serde(deserialize_with="nullable")] unit:Option<String>
});
dto!(Semantics {
    #[serde(deserialize_with="one")] schema_version:u32,
    #[serde(deserialize_with="nullable")] expression:Option<String>,
    symbols:Vec<Symbol>,conditions:Vec<String>,ranges:Vec<String>,exceptions:Vec<String>
});
dto!(Profile {
    #[serde(deserialize_with = "one")]
    schema_version: u32,
    app: String,
    app_version: String,
    mathjax_version: String,
    native_math: bool,
    compatibility_dfrac: bool,
    local_images: bool,
    verified: bool
});
dto!(Decision {
    #[serde(deserialize_with="nullable")] representation:Option<Representation>,
    fallback_reasons:Vec<String>,compatibility_rewrites:Vec<String>,issue_codes:Vec<String>
});
dto!(FormulaPlan {
    #[serde(deserialize_with="one")] schema_version:u32,
    producer:String,scope:String,request_id:String,
    case:FormulaCase,semantics:Semantics,decision:Decision,tokens:Vec<String>,
    #[serde(deserialize_with="nullable")] compiled_expression:Option<String>
});
dto!(Mapping {
    kind: String,
    source_field: String,
    start_byte: u64,
    end_byte: u64
});
dto!(RenderedFile {relative_path:String,content_sha256:String,byte_length:u64,mappings:Vec<Mapping>});
dto!(Checks {
    source_preservation: String,
    expression_match: String,
    symbol_definitions: String,
    semantic_review: String,
    note_plan: String,
    rendering: String,
    saved_output: String
});
dto!(FormulaReport {
    #[serde(deserialize_with="one")] schema_version:u32,
    producer:String,scope:String,request_id:String,status:String,preview_status:String,
    knowledge_status:String,scientific_quality:String,published:bool,network_calls:usize,paid_calls:usize,
    case:FormulaCase,semantics:Semantics,profile:Profile,checks:Checks,decision:Decision,
    #[serde(deserialize_with="nullable")] tokens:Option<Vec<String>>,
    #[serde(deserialize_with="nullable")] plan_fingerprint:Option<Fingerprint>,
    inputs:BTreeMap<String,Fingerprint>,files:Vec<RenderedFile>,artifacts:BTreeMap<String,Fingerprint>
});
