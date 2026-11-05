//! Held explicit attachment inventory, selected byte fingerprints, and complete PNG decode.
use crate::{
    extraction_verify,
    formula_model::*,
    knowledge_coordinate_input::{Held, fingerprint, plain},
    project::ProjectError,
    sidecar_process::Control,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::BTreeSet, io::Cursor, path::Path};

pub(crate) fn error(code: &'static str, exit: u8, msg: impl ToString) -> ProjectError {
    ProjectError::new(code, exit, msg.to_string())
}
pub(crate) fn parse<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProjectError> {
    extraction_verify::recover_unambiguous_json(bytes)
        .map_err(|e| error("FORMULA_SCHEMA", 2, e))?;
    serde_json::from_slice(bytes).map_err(|e| error("FORMULA_SCHEMA", 2, e))
}
pub(crate) fn encoded<T: Serialize>(v: &T) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec_pretty(v).map_err(|e| error("FORMULA_SCHEMA", 2, e))
}
pub(crate) fn check(c: &Control) -> Result<(), ProjectError> {
    c.check().map_err(|e| error("FORMULA_TIMEOUT", 3, e))
}
fn filename(name: &str) -> bool {
    !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && crate::fixture_input::valid_relative_path(name)
}
fn pin(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn list(v: &[String]) -> bool {
    v.len() <= 16
        && v.iter().all(|s| plain(s))
        && v.iter().collect::<BTreeSet<_>>().len() == v.len()
}
fn held_error(e: ProjectError) -> ProjectError {
    error(
        if e.exit_code() == 2 {
            "FORMULA_INPUT"
        } else {
            "FORMULA_INPUT_CHANGED"
        },
        e.exit_code(),
        e,
    )
}
pub(crate) struct Inputs {
    pub(crate) held: Held,
    pub(crate) case: FormulaCase,
    pub(crate) semantics: Semantics,
}
impl Inputs {
    pub(crate) fn open(path: &Path, c: &Control) -> Result<Self, ProjectError> {
        check(c)?;
        let mut held = Held::open(path).map_err(held_error)?;
        held.add("case.json", 16384, c).map_err(held_error)?;
        let case: FormulaCase = parse(held.bytes("case.json").map_err(held_error)?)?;
        if !plain(&case.title)
            || !(1..=64).contains(&case.selection_id.len())
            || !case.selection_id.as_bytes()[0].is_ascii_lowercase()
            || !case
                .selection_id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
        {
            return Err(error(
                "FORMULA_SCHEMA",
                2,
                "invalid selection identity or title",
            ));
        }
        match case.source_kind {
            SourceKind::Tex if case.tex.is_some() => (),
            SourceKind::FormulaRegion | SourceKind::ExperimentalCurve
                if case.tex.is_none() && case.region.is_some() =>
            {
                ()
            }
            _ => {
                return Err(error(
                    "FORMULA_SCHEMA",
                    2,
                    "source kind does not match selected attachments",
                ));
            }
        }
        let mut names = vec!["case.json".to_owned()];
        let mut selected = vec![(&case.semantics.file, &case.semantics.sha256, 65536u64)];
        if let Some(tex) = &case.tex {
            selected.push((&tex.file, &tex.sha256, 65536));
        }
        if let Some(region) = &case.region {
            selected.push((&region.file, &region.sha256, 8 * 1024 * 1024));
        }
        for (name, sha, _) in &selected {
            if !filename(name) || !pin(sha) || names.contains(name) {
                return Err(error(
                    "FORMULA_SCHEMA",
                    2,
                    "distinct pinned ordinary attachments required",
                ));
            }
            names.push((*name).clone());
        }
        held.exact(&names).map_err(held_error)?;
        for (name, sha, max) in selected {
            held.add(name, max, c).map_err(held_error)?;
            if fingerprint(held.bytes(name).map_err(held_error)?).sha256 != *sha {
                return Err(error(
                    "FORMULA_SOURCE_PIN",
                    2,
                    "selected original bytes do not match their fingerprint",
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
        {
            return Err(error(
                "FORMULA_SCHEMA",
                2,
                "semantic declarations exceed their plain-text bounds",
            ));
        }
        if let Some(region) = &case.region {
            validate_png(held.bytes(&region.file).map_err(held_error)?, region)?;
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
pub(crate) fn validate_profile(profile: &Profile) -> Result<(), ProjectError> {
    if [&profile.app, &profile.app_version, &profile.mathjax_version]
        .iter()
        .all(|s| plain(s))
    {
        Ok(())
    } else {
        Err(error(
            "FORMULA_PROFILE_SCHEMA",
            2,
            "profile labels must be bounded plain text",
        ))
    }
}
fn validate_png(bytes: &[u8], region: &Region) -> Result<(), ProjectError> {
    let invalid = || {
        error(
            "FORMULA_REGION_INVALID",
            2,
            "selected PNG or its page/region bounds are invalid",
        )
    };
    let [canvas_w, canvas_h] = region.canvas;
    let [x, y, w, h] = region.bounds;
    if region.page == 0
        || region.page > 1_000_000
        || canvas_w == 0
        || canvas_h == 0
        || canvas_w > 10_000_000
        || canvas_h > 10_000_000
        || w == 0
        || h == 0
        || w > 2048
        || h > 2048
        || x.checked_add(w).is_none_or(|right| right > canvas_w)
        || y.checked_add(h).is_none_or(|bottom| bottom > canvas_h)
        || bytes.len() > 8 * 1024 * 1024
    {
        return Err(invalid());
    }
    // Check full chunk framing first: forbid APNG and bytes after IEND. The
    // decoder below independently validates CRC, decompression and image rows.
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
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
    if reader.info().width != w
        || reader.info().height != h
        || reader.info().animation_control.is_some()
    {
        return Err(invalid());
    }
    let size = reader
        .output_buffer_size()
        .filter(|s| *s <= 32 * 1024 * 1024)
        .ok_or_else(invalid)?;
    let mut decoded = vec![0; size];
    let output = reader.next_frame(&mut decoded).map_err(|_| invalid())?;
    if output.width != w || output.height != h {
        return Err(invalid());
    }
    reader.finish().map_err(|_| invalid())?;
    Ok(())
}
