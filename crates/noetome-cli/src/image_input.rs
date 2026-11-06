//! Selected three-file inventory, source fingerprints, bounded complete PNG decoding.
use crate::{
    extraction_verify,
    image_model::*,
    knowledge_coordinate_input::{Held, fingerprint},
    project::ProjectError,
    sidecar_process::Control,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::BTreeSet, io::Cursor, path::Path};

pub(crate) fn error(code: &'static str, exit: u8, message: impl ToString) -> ProjectError {
    ProjectError::new(code, exit, message.to_string())
}
pub(crate) fn parse<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProjectError> {
    extraction_verify::recover_unambiguous_json(bytes).map_err(|e| error("IMAGE_SCHEMA", 2, e))?;
    serde_json::from_slice(bytes).map_err(|e| error("IMAGE_SCHEMA", 2, e))
}
pub(crate) fn encoded<T: Serialize>(value: &T) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec_pretty(value).map_err(|e| error("IMAGE_SCHEMA", 2, e))
}
pub(crate) fn check(control: &Control) -> Result<(), ProjectError> {
    control.check().map_err(|e| error("IMAGE_TIMEOUT", 3, e))
}
fn held_error(e: ProjectError) -> ProjectError {
    error(
        if e.exit_code() == 2 {
            "IMAGE_INPUT"
        } else {
            "IMAGE_INPUT_CHANGED"
        },
        e.exit_code(),
        e,
    )
}
pub(crate) fn text(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 1024
        && !value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}
fn identity(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
}
fn filename(value: &str) -> bool {
    !value.starts_with('.')
        && !value.contains(['/', '\\'])
        && crate::fixture_input::valid_relative_path(value)
}
fn pin(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn list(values: &[String]) -> bool {
    values.len() <= 32
        && values.iter().all(|s| text(s))
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}
pub(crate) fn bbox([x, y, w, h]: [u32; 4], [canvas_w, canvas_h]: [u32; 2]) -> bool {
    w > 0
        && h > 0
        && x.checked_add(w).is_some_and(|right| right <= canvas_w)
        && y.checked_add(h).is_some_and(|bottom| bottom <= canvas_h)
}
fn nullable_text(value: &Option<String>) -> bool {
    value.as_ref().is_none_or(|s| text(s))
}
pub(crate) struct Inputs {
    pub(crate) held: Held,
    pub(crate) case: ImageCase,
    pub(crate) semantics: Semantics,
}
impl Inputs {
    pub(crate) fn open(path: &Path, control: &Control) -> Result<Self, ProjectError> {
        check(control)?;
        let mut held = Held::open(path).map_err(held_error)?;
        held.add("case.json", 16384, control).map_err(held_error)?;
        let case: ImageCase = parse(held.bytes("case.json").map_err(held_error)?)?;
        if !identity(&case.selection_id)
            || !text(&case.title)
            || case.dimensions.iter().any(|n| *n == 0 || *n > 2048)
        {
            return Err(error(
                "IMAGE_SCHEMA",
                2,
                "invalid selection, title or image dimensions",
            ));
        }
        let selected = [
            (&case.image, 8 * 1024 * 1024),
            (&case.semantics, 256 * 1024),
        ];
        let mut names = vec!["case.json".to_owned()];
        for (file, _) in &selected {
            if !filename(&file.file) || !pin(&file.sha256) || names.contains(&file.file) {
                return Err(error(
                    "IMAGE_SCHEMA",
                    2,
                    "distinct pinned direct attachments are required",
                ));
            }
            names.push(file.file.clone());
        }
        held.exact(&names).map_err(held_error)?;
        for (file, limit) in selected {
            held.add(&file.file, limit, control).map_err(held_error)?;
            if fingerprint(held.bytes(&file.file).map_err(held_error)?).sha256 != file.sha256 {
                return Err(error(
                    "IMAGE_SOURCE_PIN",
                    2,
                    "selected source bytes differ from their fingerprint",
                ));
            }
        }
        validate_png(
            held.bytes(&case.image.file).map_err(held_error)?,
            case.dimensions,
        )?;
        let semantics: Semantics = parse(held.bytes(&case.semantics.file).map_err(held_error)?)?;
        validate_semantics(&semantics, case.dimensions)?;
        let value = Self {
            held,
            case,
            semantics,
        };
        value.verify(control)?;
        Ok(value)
    }
    pub(crate) fn verify(&self, control: &Control) -> Result<(), ProjectError> {
        check(control)?;
        self.held.verify(control).map_err(held_error)
    }
    pub(crate) fn bytes(&self, name: &str) -> Result<&[u8], ProjectError> {
        self.held.bytes(name).map_err(held_error)
    }
}
fn validate_semantics(value: &Semantics, dimensions: [u32; 2]) -> Result<(), ProjectError> {
    let invalid = || {
        error(
            "IMAGE_SCHEMA",
            2,
            "invalid bounded author declaration or geometry reference",
        )
    };
    if !text(&value.author)
        || !text(&value.caption)
        || !list(&value.conditions)
        || !list(&value.exceptions)
        || value.elements.len() > 64
        || value.relations.len() > 64
        || value.spatial_relations.len() > 64
        || value.legend.len() > 32
    {
        return Err(invalid());
    }
    let elements = value
        .elements
        .iter()
        .map(|e| e.id.as_str())
        .collect::<BTreeSet<_>>();
    let legends = value
        .legend
        .iter()
        .map(|e| e.id.as_str())
        .collect::<BTreeSet<_>>();
    let relations = value
        .relations
        .iter()
        .map(|e| e.id.as_str())
        .collect::<BTreeSet<_>>();
    let spatial = value
        .spatial_relations
        .iter()
        .map(|e| e.id.as_str())
        .collect::<BTreeSet<_>>();
    if elements.len() != value.elements.len()
        || legends.len() != value.legend.len()
        || relations.len() != value.relations.len()
        || spatial.len() != value.spatial_relations.len()
    {
        return Err(invalid());
    }
    let legend_reference = |id: &Option<String>| {
        id.as_ref()
            .is_none_or(|id| identity(id) && legends.contains(id.as_str()))
    };
    for element in &value.elements {
        if !identity(&element.id)
            || !bbox(element.bbox, dimensions)
            || !nullable_text(&element.label)
            || !nullable_text(&element.unit)
            || !legend_reference(&element.legend_id)
            || (element.kind == Shape::Circle && element.bbox[2] != element.bbox[3])
        {
            return Err(invalid());
        }
    }
    for relation in &value.relations {
        if !identity(&relation.id)
            || relation.from == relation.to
            || !elements.contains(relation.from.as_str())
            || !elements.contains(relation.to.as_str())
            || relation.start == relation.end
            || [relation.start, relation.end]
                .iter()
                .any(|p| p[0] > dimensions[0] || p[1] > dimensions[1])
            || !relation
                .label
                .as_ref()
                .is_none_or(|s| s.is_empty() || text(s))
            || !nullable_text(&relation.unit)
            || !legend_reference(&relation.legend_id)
        {
            return Err(invalid());
        }
    }
    for relation in &value.spatial_relations {
        if !identity(&relation.id)
            || relation.a == relation.b
            || !elements.contains(relation.a.as_str())
            || !elements.contains(relation.b.as_str())
        {
            return Err(invalid());
        }
    }
    for legend in &value.legend {
        if !identity(&legend.id)
            || !nullable_text(&legend.label)
            || !nullable_text(&legend.meaning)
            || !bbox(legend.bbox, dimensions)
            || legend.color.len() != 7
            || !legend.color.starts_with('#')
            || !legend
                .color
                .bytes()
                .skip(1)
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
    }
    Ok(())
}
pub(crate) fn validate_profile(value: &Profile) -> Result<(), ProjectError> {
    if text(&value.app) && text(&value.app_version) {
        Ok(())
    } else {
        Err(error(
            "IMAGE_PROFILE_SCHEMA",
            2,
            "profile labels must be bounded text",
        ))
    }
}
fn validate_png(bytes: &[u8], dimensions: [u32; 2]) -> Result<(), ProjectError> {
    let invalid = || {
        error(
            "IMAGE_PNG_INVALID",
            2,
            "selected PNG is incomplete, animated, over limit or has mismatched dimensions",
        )
    };
    if bytes.len() > 8 * 1024 * 1024 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(invalid());
    }
    let mut offset = 8usize;
    let mut ended = false;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset.checked_add(8).ok_or_else(invalid)?)
            .ok_or_else(invalid)?;
        let length = u32::from_be_bytes(header[..4].try_into().map_err(|_| invalid())?) as usize;
        let kind = &header[4..8];
        let end = offset
            .checked_add(12)
            .and_then(|n| n.checked_add(length))
            .ok_or_else(invalid)?;
        if end > bytes.len() || matches!(kind, b"acTL" | b"fcTL" | b"fdAT") {
            return Err(invalid());
        }
        if kind == b"IEND" {
            if length != 0 || end != bytes.len() {
                return Err(invalid());
            }
            ended = true;
        }
        offset = end;
    }
    if !ended {
        return Err(invalid());
    }
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: 32 * 1024 * 1024,
        },
    );
    decoder.ignore_checksums(false);
    let mut reader = decoder.read_info().map_err(|_| invalid())?;
    if [reader.info().width, reader.info().height] != dimensions
        || reader.info().animation_control.is_some()
    {
        return Err(invalid());
    }
    let size = reader
        .output_buffer_size()
        .filter(|size| *size <= 32 * 1024 * 1024)
        .ok_or_else(invalid)?;
    let mut decoded = vec![0; size];
    let frame = reader.next_frame(&mut decoded).map_err(|_| invalid())?;
    if [frame.width, frame.height] != dimensions {
        return Err(invalid());
    }
    reader.finish().map_err(|_| invalid())?;
    Ok(())
}
