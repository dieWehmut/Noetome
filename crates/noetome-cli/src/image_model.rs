//! Strict source-author image declarations, never an image-recognition result.
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
        _ => Err(serde::de::Error::custom("unsupported image version")),
    }
}
fn purpose<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == "image_fidelity" {
        Ok(value)
    } else {
        Err(serde::de::Error::custom("unsupported image purpose"))
    }
}
fn author_producer<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value == "author_image_declaration" {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(
            "image semantics require an explicit author declaration",
        ))
    }
}
macro_rules! dto {
    ($name:ident {$($(#[$attr:meta])* $field:ident:$ty:ty),* $(,)?}) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct $name { $($(#[$attr])* pub(crate) $field:$ty),* }
    };
}
macro_rules! enumeration {
    ($name:ident {$($variant:ident),* $(,)?}) => {
        #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
        #[serde(rename_all = "snake_case")]
        pub(crate) enum $name { $($variant),* }
    };
}
enumeration!(SourceKind {
    Diagram,
    Spatial,
    ExperimentalCurve,
    Observation
});
enumeration!(Readability { Clear, Unreadable });
enumeration!(Shape {
    Rect,
    Circle,
    Unsupported
});
enumeration!(Direction {
    Forward,
    Reverse,
    None,
    Unknown
});
enumeration!(Effect {
    Positive,
    Negative,
    Neutral,
    Unknown
});
enumeration!(SpatialKind {
    LeftOf,
    Above,
    Inside,
    Overlaps,
    Unknown
});
enumeration!(Representation {
    SvgRedraw,
    OriginalImage
});
dto!(SourceFile {
    file: String,
    sha256: String
});
dto!(ImageCase {
    #[serde(deserialize_with = "one")]
    schema_version: u32,
    #[serde(deserialize_with = "purpose")]
    purpose: String,
    selection_id: String,
    title: String,
    source_kind: SourceKind,
    image: SourceFile,
    semantics: SourceFile,
    dimensions: [u32; 2],
    readability: Readability
});
dto!(Element {
    id:String, kind:Shape, bbox:[u32;4],
    #[serde(deserialize_with="nullable")] label:Option<String>,
    #[serde(deserialize_with="nullable")] unit:Option<String>,
    #[serde(deserialize_with="nullable")] legend_id:Option<String>
});
dto!(Relation {
    id:String, from:String, to:String, start:[u32;2], end:[u32;2], direction:Direction, effect:Effect,
    #[serde(deserialize_with="nullable")] label:Option<String>,
    #[serde(deserialize_with="nullable")] unit:Option<String>,
    #[serde(deserialize_with="nullable")] legend_id:Option<String>
});
dto!(SpatialRelation {
    id: String,
    a: String,
    b: String,
    relation: SpatialKind
});
dto!(Legend {
    id:String,
    #[serde(deserialize_with="nullable")] label:Option<String>,
    #[serde(deserialize_with="nullable")] meaning:Option<String>,
    color:String, bbox:[u32;4]
});
dto!(Semantics {
    #[serde(deserialize_with="one")] schema_version:u32,
    #[serde(deserialize_with="author_producer")] producer:String,
    author:String, complete:bool, data_available:bool, caption:String,
    conditions:Vec<String>, exceptions:Vec<String>, elements:Vec<Element>, relations:Vec<Relation>,
    spatial_relations:Vec<SpatialRelation>, legend:Vec<Legend>
});
dto!(Profile {
    #[serde(deserialize_with = "one")]
    schema_version: u32,
    app: String,
    app_version: String,
    svg_supported: bool,
    local_images: bool,
    verified: bool
});
dto!(Decision {
    #[serde(deserialize_with="nullable")] representation:Option<Representation>,
    fallback_reasons:Vec<String>, issue_codes:Vec<String>
});
dto!(ImagePlan {
    #[serde(deserialize_with = "one")]
    schema_version: u32,
    producer: String,
    scope: String,
    request_id: String,
    case: ImageCase,
    semantics: Semantics,
    decision: Decision
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
    author_declaration: String,
    geometry_review: String,
    semantic_review: String,
    note_plan: String,
    rendering: String,
    saved_output: String
});
dto!(ImageReport {
    #[serde(deserialize_with="one")] schema_version:u32,
    producer:String, scope:String, request_id:String, status:String, preview_status:String,
    knowledge_status:String, scientific_quality:String, published:bool, network_calls:usize, paid_calls:usize,
    case:ImageCase, semantics:Semantics, profile:Profile, checks:Checks, decision:Decision,
    #[serde(deserialize_with="nullable")] plan_fingerprint:Option<Fingerprint>,
    inputs:BTreeMap<String,Fingerprint>, files:Vec<RenderedFile>, artifacts:BTreeMap<String,Fingerprint>
});
