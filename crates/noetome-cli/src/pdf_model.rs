//! PDF native page/region locators. They are never legacy text SourceSpans.
use crate::sidecar_model::{Fingerprint, Issue, ParseStatus, Resource, Response};
use serde::{Deserialize, Serialize};

pub(crate) const MANIFEST_ID: &str = "pdf-manifest";
pub(crate) const RAW_ID: &str = "raw-pdf";
pub(crate) const MAX_PAGES: usize = 128;
pub(crate) const MAX_PAGE_GLYPHS: usize = 50_000;
pub(crate) const MAX_GLYPHS: usize = 200_000;

macro_rules! object {
    ($name:ident { $($(#[$attr:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct $name { $($(#[$attr])* pub $field: $ty),* }
    };
}
pub(crate) fn nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    de: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(de)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ParserIdentity {
    pub engine: String,
    pub engine_version: String,
    pub engine_dependency: String,
    pub engine_dependency_version: String,
    pub adapter_version: String,
    pub options_version: String,
    pub python_version: String,
}
impl ParserIdentity {
    pub(crate) fn pinned() -> Self {
        Self {
            engine: "pdfplumber".into(),
            engine_version: "0.11.9".into(),
            engine_dependency: "pdfminer.six".into(),
            engine_dependency_version: "20251230".into(),
            adapter_version: "noetome-pdf-text-v1".into(),
            options_version: "crop-pt-lines-v1".into(),
            python_version: "3.12.14".into(),
        }
    }
}
object!(Geometry {
    unit: String,
    origin: String,
    y_axis: String,
    media_box: [f64; 4],
    crop_box: [f64; 4],
    rotation_degrees: u32,
    user_unit: f64,
    width_pt: f64,
    height_pt: f64,
    pdf_to_display: [f64; 6]
});
object!(Glyph {
    ordinal: u32,
    text: String,
    bbox: [f64; 4],
    font_name: String,
    font_size: f64,
    upright: bool
});
object!(TextArtifact {
    resource_id: String,
    sha256: String,
    byte_length: u64
});
object!(RegionText {
    resource_id: String,
    resource_sha256: String,
    start_byte: u64,
    end_byte: u64
});
object!(Region {
    region_id: String, locator_kind: String, physical_page_index: u32,
    bbox: [f64; 4], text: RegionText, glyph_indices: Vec<u32>
});
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VisualKind {
    Image,
    Vector,
    Annotation,
}
object!(Visual {
    visual_id: String,
    kind: VisualKind,
    physical_page_index: u32,
    bbox: [f64; 4],
    evidence_resource_id: String
});
object!(Page {
    physical_page_index: u32, display_page_number: u32,
    #[serde(deserialize_with = "nullable")]
    printed_page_label: Option<String>, status: ParseStatus,
    #[serde(deserialize_with = "nullable")]
    geometry: Option<Geometry>,
    #[serde(deserialize_with = "nullable")]
    text: Option<TextArtifact>,
    glyphs: Vec<Glyph>, regions: Vec<Region>, visuals: Vec<Visual>
});
object!(Manifest {
    schema_version: u32, format: String, input_fingerprint: Fingerprint,
    parser: ParserIdentity, raw_resource_id: String,
    #[serde(deserialize_with = "nullable")]
    page_count: Option<u32>,
    status: ParseStatus, pages: Vec<Page>, resources: Vec<Resource>, issues: Vec<Issue>
});
object!(Report {
    schema_version: u32, report_kind: String, input_fingerprint: Fingerprint,
    parser: ParserIdentity, status: ParseStatus,
    #[serde(deserialize_with = "nullable")]
    page_count: Option<u32>,
    parsed_page_count: u32, needs_review_page_count: u32, failed_page_count: u32,
    manifest_path: String, manifest_sha256: String, elapsed_ms: u64,
    resource_bytes: u64, sidecar: Response
});

pub(crate) fn page_id(index: usize) -> String {
    format!("page-{index:06}")
}
pub(crate) fn text_id(index: usize) -> String {
    format!("page-text-{index:06}")
}
