//! Held scan attachments and a pixel-level proof that the selected crop comes from the selected PNG page.
use crate::{
    extraction_verify,
    knowledge_coordinate_input::{Held, fingerprint, plain},
    project::ProjectError,
    scan_model::*,
    sidecar_process::Control,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::BTreeSet, io::Cursor, path::Path};
pub(crate) fn error(code: &'static str, exit: u8, msg: impl ToString) -> ProjectError {
    ProjectError::new(code, exit, msg.to_string())
}
pub(crate) fn parse<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProjectError> {
    extraction_verify::recover_unambiguous_json(bytes).map_err(|e| error("SCAN_SCHEMA", 2, e))?;
    serde_json::from_slice(bytes).map_err(|e| error("SCAN_SCHEMA", 2, e))
}
pub(crate) fn encoded<T: Serialize>(value: &T) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec_pretty(value).map_err(|e| error("SCAN_SCHEMA", 2, e))
}
pub(crate) fn check(c: &Control) -> Result<(), ProjectError> {
    c.check().map_err(|e| error("SCAN_TIMEOUT", 3, e))
}
pub(crate) fn pin(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn filename(s: &str) -> bool {
    !s.starts_with('.') && !s.contains(['/', '\\']) && crate::fixture_input::valid_relative_path(s)
}
fn identity(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
}
fn list(v: &[String]) -> bool {
    v.len() <= 16
        && v.iter().all(|s| plain(s))
        && v.iter().collect::<BTreeSet<_>>().len() == v.len()
}
fn held_error(e: ProjectError) -> ProjectError {
    error(
        if e.exit_code() == 2 {
            "SCAN_INPUT"
        } else {
            "SCAN_INPUT_CHANGED"
        },
        e.exit_code(),
        e,
    )
}
pub(crate) struct Inputs {
    pub(crate) held: Held,
    pub(crate) case: ScanCase,
    pub(crate) semantics: Semantics,
}
impl Inputs {
    pub(crate) fn open(path: &Path, c: &Control) -> Result<Self, ProjectError> {
        check(c)?;
        let mut held = Held::open(path).map_err(held_error)?;
        held.add("case.json", 16384, c).map_err(held_error)?;
        let case: ScanCase = parse(held.bytes("case.json").map_err(held_error)?)?;
        let [cw, ch] = case.page.canvas;
        let [x, y, w, h] = case.region.bounds;
        if !identity(&case.selection_id)
            || !plain(&case.title)
            || case.page.page == 0
            || case.page.page > 1_000_000
            || cw == 0
            || ch == 0
            || cw > 2048
            || ch > 2048
            || w == 0
            || h == 0
            || x.checked_add(w).is_none_or(|n| n > cw)
            || y.checked_add(h).is_none_or(|n| n > ch)
        {
            return Err(error(
                "SCAN_SCHEMA",
                2,
                "invalid selected identity, page or bounded region",
            ));
        }
        let mut selected = vec![
            (&case.semantics.file, &case.semantics.sha256, 65536u64),
            (&case.page.file, &case.page.sha256, 8 * 1024 * 1024),
            (&case.region.file, &case.region.sha256, 8 * 1024 * 1024),
        ];
        if let Some(pdf) = &case.pdf {
            selected.push((&pdf.file, &pdf.sha256, 8 * 1024 * 1024));
        }
        let mut names = vec!["case.json".to_owned()];
        for (file, sha, _) in &selected {
            if !filename(file) || !pin(sha) || names.contains(file) {
                return Err(error(
                    "SCAN_SCHEMA",
                    2,
                    "distinct pinned direct attachments required",
                ));
            }
            names.push((*file).clone());
        }
        held.exact(&names).map_err(held_error)?;
        for (file, sha, max) in selected {
            held.add(file, max, c).map_err(held_error)?;
            if fingerprint(held.bytes(file).map_err(held_error)?).sha256 != *sha {
                return Err(error(
                    "SCAN_SOURCE_PIN",
                    2,
                    "selected original bytes differ from their fingerprint",
                ));
            }
        }
        if let Some(pdf) = &case.pdf {
            if !held
                .bytes(&pdf.file)
                .map_err(held_error)?
                .starts_with(b"%PDF-")
            {
                return Err(error(
                    "SCAN_PDF_ATTACHMENT",
                    2,
                    "archive attachment is not a PDF byte stream",
                ));
            }
        }
        let semantics: Semantics = parse(held.bytes(&case.semantics.file).map_err(held_error)?)?;
        if semantics
            .expression
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 65536)
            || semantics.symbols.len() > 64
            || semantics.symbols.iter().any(|s| {
                !plain(&s.symbol)
                    || s.definition.as_ref().is_some_and(|s| !plain(s))
                    || s.unit.as_ref().is_some_and(|s| !plain(s))
            })
            || !list(&semantics.conditions)
            || !list(&semantics.ranges)
            || !list(&semantics.exceptions)
            || semantics.uncertainties.len() > 16
            || semantics
                .uncertainties
                .iter()
                .any(|s| !plain(&s.description))
        {
            return Err(error(
                "SCAN_SCHEMA",
                2,
                "invalid bounded author scan declaration",
            ));
        }
        let page = decode_png(held.bytes(&case.page.file).map_err(held_error)?, [cw, ch])?;
        let region = decode_png(held.bytes(&case.region.file).map_err(held_error)?, [w, h])?;
        for row in 0..h as usize {
            check(c)?;
            let start = ((y as usize + row) * cw as usize + x as usize) * 4;
            let region_start = row * w as usize * 4;
            if page[start..start + w as usize * 4]
                != region[region_start..region_start + w as usize * 4]
            {
                return Err(error(
                    "SCAN_REGION_MISMATCH",
                    2,
                    "selected crop pixels do not match the selected page and rectangle",
                ));
            }
        }
        let value = Self {
            held,
            case,
            semantics,
        };
        value.verify(c)?;
        Ok(value)
    }
    pub(crate) fn verify(&self, c: &Control) -> Result<(), ProjectError> {
        check(c)?;
        self.held.verify(c).map_err(held_error)
    }
    pub(crate) fn bytes(&self, name: &str) -> Result<&[u8], ProjectError> {
        self.held.bytes(name).map_err(held_error)
    }
}
fn decode_png(bytes: &[u8], dimensions: [u32; 2]) -> Result<Vec<u8>, ProjectError> {
    let bad = || {
        error(
            "SCAN_PNG_INVALID",
            2,
            "selected PNG is incomplete, animated, over limit or not a bounded 8-bit image",
        )
    };
    if bytes.len() > 8 * 1024 * 1024 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(bad());
    }
    let mut offset = 8usize;
    let mut ended = false;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset.checked_add(8).ok_or_else(bad)?)
            .ok_or_else(bad)?;
        let length = u32::from_be_bytes(header[..4].try_into().map_err(|_| bad())?) as usize;
        let kind = &header[4..8];
        let end = offset
            .checked_add(12)
            .and_then(|n| n.checked_add(length))
            .ok_or_else(bad)?;
        if end > bytes.len() || matches!(kind, b"acTL" | b"fcTL" | b"fdAT") {
            return Err(bad());
        }
        if kind == b"IEND" {
            if length != 0 || end != bytes.len() {
                return Err(bad());
            }
            ended = true;
        }
        offset = end;
    }
    if !ended {
        return Err(bad());
    }
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: 32 * 1024 * 1024,
        },
    );
    decoder.ignore_checksums(false);
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|_| bad())?;
    if [reader.info().width, reader.info().height] != dimensions
        || reader.info().animation_control.is_some()
        || reader.info().bit_depth == png::BitDepth::Sixteen
    {
        return Err(bad());
    }
    let size = reader
        .output_buffer_size()
        .filter(|s| *s <= 32 * 1024 * 1024)
        .ok_or_else(bad)?;
    let mut data = vec![0; size];
    let frame = reader.next_frame(&mut data).map_err(|_| bad())?;
    if [frame.width, frame.height] != dimensions || frame.bit_depth != png::BitDepth::Eight {
        return Err(bad());
    }
    data.truncate(frame.buffer_size());
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err(bad()),
    };
    let mut rgba = Vec::with_capacity(dimensions[0] as usize * dimensions[1] as usize * 4);
    for pixel in data.chunks_exact(channels) {
        match channels {
            1 => rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], 255]),
            2 => rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]),
            3 => rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]),
            4 => rgba.extend_from_slice(pixel),
            _ => unreachable!(),
        }
    }
    if rgba.len() != dimensions[0] as usize * dimensions[1] as usize * 4 {
        return Err(bad());
    }
    reader.finish().map_err(|_| bad())?;
    Ok(rgba)
}
