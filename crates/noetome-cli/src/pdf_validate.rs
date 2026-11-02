//! Core semantic audit of an already byte-validated PDF workspace.
use crate::{
    pdf_model::*,
    sidecar_model::*,
    sidecar_process::Control,
    sidecar_workspace::{ResourceData, valid_id},
};
use std::collections::{BTreeMap, BTreeSet};

fn error(code: &'static str, message: impl ToString) -> SidecarError {
    SidecarError::new(code, message)
}
fn same_fingerprint(a: &Fingerprint, b: &Fingerprint) -> bool {
    a.sha256 == b.sha256 && a.byte_length == b.byte_length
}
fn same_resource(a: &Resource, b: &Resource) -> bool {
    a.resource_id == b.resource_id
        && a.relative_path == b.relative_path
        && a.kind == b.kind
        && same_fingerprint(&a.fingerprint, &b.fingerprint)
}
fn blocked(manifest: &Manifest, id: &str) -> bool {
    manifest.issues.iter().any(|issue| {
        issue.severity == Severity::Blocking && issue.unit_ids.iter().any(|unit| unit == id)
    })
}
fn near(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 0.000001
}
fn in_page(bbox: &[f64; 4], geometry: &Geometry, positive: bool) -> bool {
    bbox.iter().all(|x| x.is_finite())
        && bbox[0] >= 0.0
        && bbox[1] >= 0.0
        && bbox[2] <= geometry.width_pt
        && bbox[3] <= geometry.height_pt
        && if positive {
            bbox[0] < bbox[2] && bbox[1] < bbox[3]
        } else {
            bbox[0] <= bbox[2] && bbox[1] <= bbox[3]
        }
}

fn validate_geometry(g: &Geometry) -> Result<(), SidecarError> {
    let [ml, mb, mr, mt] = g.media_box;
    let [l, b, r, t] = g.crop_box;
    let s = g.user_unit;
    if g.unit != "pt"
        || g.origin != "crop_top_left"
        || g.y_axis != "down"
        || !g
            .media_box
            .iter()
            .chain(g.crop_box.iter())
            .all(|x| x.is_finite())
        || ml >= mr
        || mb >= mt
        || l >= r
        || b >= t
        || l < ml
        || r > mr
        || b < mb
        || t > mt
        || !s.is_finite()
        || s <= 0.0
    {
        return Err(error(
            "PDF_LOCATOR",
            "invalid PDF MediaBox/CropBox/coordinate system",
        ));
    }
    let (matrix, width, height) = match g.rotation_degrees {
        0 => ([s, 0.0, 0.0, -s, -s * l, s * t], s * (r - l), s * (t - b)),
        90 => ([0.0, s, s, 0.0, -s * b, -s * l], s * (t - b), s * (r - l)),
        180 => ([-s, 0.0, 0.0, s, s * r, -s * b], s * (r - l), s * (t - b)),
        270 => ([0.0, -s, -s, 0.0, s * t, s * r], s * (t - b), s * (r - l)),
        _ => return Err(error("PDF_LOCATOR", "rotation is not 0/90/180/270 degrees")),
    };
    if width <= 0.0
        || height <= 0.0
        || !near(width, g.width_pt)
        || !near(height, g.height_pt)
        || matrix
            .iter()
            .zip(g.pdf_to_display)
            .any(|(a, b)| !near(*a, b))
    {
        return Err(error(
            "PDF_LOCATOR",
            "PDF-to-displayed-CropBox matrix/dimensions mismatch",
        ));
    }
    Ok(())
}

fn line_groups(page: &Page, control: &Control) -> Result<Vec<Vec<u32>>, SidecarError> {
    let mut indices: Vec<u32> = (0..page.glyphs.len() as u32).collect();
    indices.sort_by(|a, b| {
        let ga = &page.glyphs[*a as usize];
        let gb = &page.glyphs[*b as usize];
        ga.bbox[1]
            .total_cmp(&gb.bbox[1])
            .then(ga.bbox[0].total_cmp(&gb.bbox[0]))
            .then(a.cmp(b))
    });
    control.check()?;
    let mut groups: Vec<Vec<u32>> = Vec::new();
    let mut anchor = 0.0;
    for index in indices {
        control.check()?;
        let top = page.glyphs[index as usize].bbox[1];
        if groups.is_empty() || top - anchor > 3.0 {
            anchor = top;
            groups.push(Vec::new());
        }
        groups.last_mut().unwrap().push(index);
    }
    for group in &mut groups {
        control.check()?;
        group.sort_by(|a, b| {
            page.glyphs[*a as usize].bbox[0]
                .total_cmp(&page.glyphs[*b as usize].bbox[0])
                .then(a.cmp(b))
        });
    }
    Ok(groups)
}

fn compose_line(
    page: &Page,
    group: &[u32],
    maximum: usize,
    control: &Control,
) -> Result<(String, [f64; 4], bool), SidecarError> {
    let mut text = String::new();
    let first = &page.glyphs[group[0] as usize];
    let mut bbox = first.bbox;
    let mut previous: Option<&Glyph> = None;
    let mut review = false;
    for index in group {
        control.check()?;
        let glyph = &page.glyphs[*index as usize];
        for axis in [0, 1] {
            bbox[axis] = bbox[axis].min(glyph.bbox[axis]);
        }
        for axis in [2, 3] {
            bbox[axis] = bbox[axis].max(glyph.bbox[axis]);
        }
        if let Some(prior) = previous {
            let gap = glyph.bbox[0] - prior.bbox[2];
            if gap > 36.0 || gap < -0.5 {
                review = true;
            }
            if gap > 3.0
                && !prior.text.chars().last().is_some_and(char::is_whitespace)
                && !glyph.text.chars().next().is_some_and(char::is_whitespace)
            {
                if text.len() >= maximum {
                    return Err(error("PDF_TEXT", "inferred text exceeds actual artifact"));
                }
                text.push(' ');
            }
        }
        if (glyph.font_size - first.font_size).abs() > 0.1 {
            review = true;
        }
        if glyph.text.len() > maximum.saturating_sub(text.len()) {
            return Err(error("PDF_TEXT", "glyph text exceeds actual artifact"));
        }
        text.push_str(&glyph.text);
        previous = Some(glyph);
    }
    Ok((text, bbox, review))
}

fn validate_page(
    page: &Page,
    index: usize,
    manifest: &Manifest,
    response: &Response,
    data: &BTreeMap<String, ResourceData>,
    control: &Control,
) -> Result<(), SidecarError> {
    control.check()?;
    let id = page_id(index);
    let unit = &response.units[index];
    if page.physical_page_index != index as u32
        || page.display_page_number != index as u32 + 1
        || page.printed_page_label.is_some()
        || unit.unit_id != id
        || unit.ordinal != index as u32
        || unit.status != page.status
    {
        return Err(error(
            "PDF_PAGES",
            format!("physical page/sidecar unit mismatch at {index}"),
        ));
    }
    if page.status != ParseStatus::Parsed && !blocked(manifest, &id) {
        return Err(error(
            "PDF_STATUS",
            format!("page {index} lacks a blocking review/failure issue"),
        ));
    }
    if let Some(geometry) = &page.geometry {
        validate_geometry(geometry)?;
    }
    if page.status == ParseStatus::Failed {
        if page.text.is_some()
            || !page.glyphs.is_empty()
            || !page.regions.is_empty()
            || !page.visuals.is_empty()
            || unit.kind != UnitKind::Unknown
            || unit.text.is_some()
            || unit.resource_ids != [RAW_ID, MANIFEST_ID]
        {
            return Err(error(
                "PDF_STATUS",
                format!("failed page {index} contains fabricated extraction"),
            ));
        }
        return Ok(());
    }
    let geometry = page
        .geometry
        .as_ref()
        .ok_or_else(|| error("PDF_LOCATOR", format!("page {index} has no valid geometry")))?;
    let artifact = page.text.as_ref().ok_or_else(|| {
        error(
            "PDF_TEXT",
            format!("page {index} has no confirmed text artifact"),
        )
    })?;
    let resource_id = text_id(index);
    let resource = manifest
        .resources
        .iter()
        .find(|r| r.resource_id == resource_id)
        .ok_or_else(|| error("PDF_RESOURCE", "missing page text resource"))?;
    let text = data
        .get(&resource_id)
        .and_then(ResourceData::text)
        .ok_or_else(|| error("PDF_RESOURCE", "page text is not validated UTF8"))?;
    if artifact.resource_id != resource_id
        || artifact.sha256 != resource.fingerprint.sha256
        || artifact.byte_length != resource.fingerprint.byte_length
        || resource.kind != ResourceKind::Utf8Text
        || resource.relative_path != format!("pages/{index:06}.txt")
        || text.len() as u64 != artifact.byte_length
    {
        return Err(error(
            "PDF_RESOURCE",
            format!("page {index} artifact path/SHA/length mismatch"),
        ));
    }
    let kind = if text.is_empty() {
        UnitKind::Unknown
    } else {
        UnitKind::Text
    };
    let range_valid = if text.is_empty() {
        // Protocol v1 ranges are nonempty; an empty resource is evidence, not
        // a fabricated [0,0) body. Its page artifact and resource ID remain.
        unit.text.is_none()
    } else {
        unit.text.as_ref().is_some_and(|range| {
            range.resource_id == resource_id
                && range.start_byte == 0
                && range.end_byte == text.len() as u64
        })
    };
    if unit.kind != kind
        || !range_valid
        || unit.resource_ids
            != [
                RAW_ID.to_string(),
                MANIFEST_ID.to_string(),
                resource_id.clone(),
            ]
    {
        return Err(error(
            "PDF_TEXT",
            format!("page {index} whole-text/unit resource mismatch"),
        ));
    }
    let mut review = page.glyphs.is_empty()
        || text.chars().all(char::is_whitespace)
        || !page.visuals.is_empty()
        || geometry.rotation_degrees != 0
        || geometry.user_unit != 1.0;
    for (ordinal, glyph) in page.glyphs.iter().enumerate() {
        control.check()?;
        if glyph.ordinal != ordinal as u32
            || glyph.text.is_empty()
            || glyph.text.len() > 1024
            || glyph.font_name.is_empty()
            || glyph.font_name.len() > 256
            || !glyph.font_size.is_finite()
            || glyph.font_size <= 0.0
            || !in_page(&glyph.bbox, geometry, false)
        {
            return Err(error(
                "PDF_LOCATOR",
                format!("page {index} invalid glyph/coordinate at {ordinal}"),
            ));
        }
        let font = glyph.font_name.to_lowercase();
        review |= !glyph.upright
            || glyph.text.chars().nth(1).is_some()
            || font.contains("symbol")
            || font.contains("math")
            || glyph.text.contains("(cid:")
            || glyph.text.chars().any(char::is_control);
    }
    let groups = line_groups(page, control)?;
    if groups.len() != page.regions.len() {
        return Err(error(
            "PDF_TEXT",
            format!("page {index} missing/extra text regions"),
        ));
    }
    let mut cursor = 0usize;
    for (ordinal, (group, region)) in groups.iter().zip(&page.regions).enumerate() {
        control.check()?;
        if region.physical_page_index != page.physical_page_index
            || region.locator_kind != "pdf_page_region"
            || region.region_id != format!("region-{index:06}-{ordinal:06}")
            || !in_page(&region.bbox, geometry, true)
        {
            return Err(error(
                "PDF_LOCATOR",
                format!("page {index} invalid native region at {ordinal}"),
            ));
        }
        if region.glyph_indices != *group {
            return Err(error(
                "PDF_TEXT",
                format!("page {index} glyph allocation/reading order mismatch"),
            ));
        }
        let (line, bbox, line_review) = compose_line(page, group, text.len(), control)?;
        review |= line_review;
        if region.bbox.iter().zip(bbox).any(|(a, b)| !near(*a, b)) {
            return Err(error(
                "PDF_LOCATOR",
                format!("page {index} region {ordinal} is not its glyph envelope"),
            ));
        }
        let r = &region.text;
        if r.resource_id != resource_id || r.resource_sha256 != artifact.sha256 {
            return Err(error("PDF_RESOURCE", "region names the wrong artifact/SHA"));
        }
        let start = usize::try_from(r.start_byte)
            .map_err(|_| error("PDF_TEXT", "text offset cannot fit"))?;
        let end =
            usize::try_from(r.end_byte).map_err(|_| error("PDF_TEXT", "text offset cannot fit"))?;
        if !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
            || start != cursor
            || end != cursor + line.len()
            || text.get(start..end) != Some(line.as_str())
            || text.as_bytes().get(end) != Some(&b'\n')
        {
            return Err(error(
                "PDF_TEXT",
                format!("page {index} body/UTF8 half-open range/order mismatch at {ordinal}"),
            ));
        }
        cursor = end + 1;
    }
    if cursor != text.len() {
        return Err(error(
            "PDF_TEXT",
            format!("page {index} uncovered/missing body"),
        ));
    }
    // A staggered column or side annotation can occupy a separate line group,
    // so within-line gaps alone cannot establish a safe reading order. This
    // linear conservative check preserves text/geometry and requires review.
    for pair in page.regions.windows(2) {
        control.check()?;
        let [left, top, right, bottom] = pair[0].bbox;
        let [next_left, next_top, next_right, next_bottom] = pair[1].bbox;
        let horizontal_gap = (next_left - right).max(left - next_right);
        let top_delta = next_top - top;
        let nearby = 72.0_f64.max(6.0 * (bottom - top).max(next_bottom - next_top));
        review |= horizontal_gap > 36.0 && top_delta >= 0.0 && top_delta <= nearby;
    }
    let mut ids = BTreeSet::new();
    for visual in &page.visuals {
        control.check()?;
        if visual.physical_page_index != page.physical_page_index
            || visual.evidence_resource_id != RAW_ID
            || !valid_id(&visual.visual_id)
            || !ids.insert(&visual.visual_id)
            || !in_page(&visual.bbox, geometry, false)
        {
            return Err(error(
                "PDF_LOCATOR",
                format!("page {index} invalid visual/source region"),
            ));
        }
    }
    if review && page.status == ParseStatus::Parsed {
        return Err(error(
            "PDF_STATUS",
            format!("page {index} needs review but is claimed parsed"),
        ));
    }
    Ok(())
}

pub(crate) fn validate_manifest(
    manifest: &Manifest,
    response: &Response,
    data: &BTreeMap<String, ResourceData>,
    control: &Control,
) -> Result<(), SidecarError> {
    control.check()?;
    if manifest.schema_version != 1
        || manifest.format != "pdf-page-regions-v1"
        || manifest.parser != ParserIdentity::pinned()
        || response.protocol_version != 1
        || response.producer != Producer::Parser
        || response.parser.name != manifest.parser.engine
        || response.parser.version != manifest.parser.engine_version
        || response.parser.capability_version != 1
        || response.parser.capabilities != [Capability::SourceUnitsV1]
    {
        return Err(error(
            "PDF_VERSION",
            "unsupported PDF manifest/parser/protocol identity",
        ));
    }
    if manifest.raw_resource_id != RAW_ID
        || !same_fingerprint(&manifest.input_fingerprint, &response.input_fingerprint)
    {
        return Err(error("PDF_RESOURCE", "PDF input identity mismatch"));
    }
    let declared: Vec<_> = response
        .resources
        .iter()
        .filter(|r| r.resource_id != MANIFEST_ID)
        .collect();
    if declared.len() != manifest.resources.len()
        || declared
            .iter()
            .zip(&manifest.resources)
            .any(|(a, b)| !same_resource(a, b))
        || response
            .resources
            .iter()
            .filter(|r| {
                r.resource_id == MANIFEST_ID
                    && r.relative_path == "pdf-manifest.json"
                    && r.kind == ResourceKind::Utf8Text
            })
            .count()
            != 1
    {
        return Err(error(
            "PDF_RESOURCE",
            "PDF/sidecar resource registries differ",
        ));
    }
    let raw = manifest
        .resources
        .iter()
        .find(|r| r.resource_id == RAW_ID)
        .ok_or_else(|| error("PDF_RESOURCE", "no complete raw PDF resource"))?;
    if raw.relative_path != "raw.pdf"
        || raw.kind != ResourceKind::Binary
        || !same_fingerprint(&raw.fingerprint, &manifest.input_fingerprint)
        || !matches!(data.get(RAW_ID),Some(ResourceData::Binary(bytes)) if bytes.len() as u64==raw.fingerprint.byte_length)
    {
        return Err(error("PDF_RESOURCE", "raw PDF resource metadata mismatch"));
    }
    if manifest.pages.len() > MAX_PAGES
        || manifest
            .pages
            .iter()
            .any(|p| p.glyphs.len() > MAX_PAGE_GLYPHS)
        || manifest.pages.iter().map(|p| p.glyphs.len()).sum::<usize>() > MAX_GLYPHS
    {
        return Err(error("SIDECAR_LIMIT", "PDF page/glyph limit"));
    }
    let document_failure = manifest.page_count.is_none() || manifest.page_count == Some(0);
    if manifest
        .page_count
        .is_some_and(|count| count as usize != manifest.pages.len())
        || response.unit_count as usize != manifest.pages.len() + usize::from(document_failure)
        || response.units.len() != response.unit_count as usize
    {
        return Err(error(
            "PDF_PAGES",
            "incomplete physical page/sidecar unit registry",
        ));
    }
    if serde_json::to_value(&manifest.issues).ok() != serde_json::to_value(&response.issues).ok() {
        return Err(error("PDF_STATUS", "manifest/sidecar issues differ"));
    }
    for issue in &manifest.issues {
        control.check()?;
        if !issue.code.starts_with("PDF_")
            || !valid_id(&issue.code)
            || issue.message.trim().is_empty()
            || issue.message.len() > 4096
        {
            return Err(error("PDF_STATUS", "invalid PDF diagnostic"));
        }
    }
    let mut expected_resources = vec![RAW_ID.to_string()];
    for (index, page) in manifest.pages.iter().enumerate() {
        validate_page(page, index, manifest, response, data, control)?;
        if page.text.is_some() {
            expected_resources.push(text_id(index));
        }
    }
    if expected_resources
        != manifest
            .resources
            .iter()
            .map(|r| r.resource_id.clone())
            .collect::<Vec<_>>()
    {
        return Err(error("PDF_RESOURCE", "missing/extra PDF page resources"));
    }
    if document_failure {
        let unit = response
            .units
            .last()
            .ok_or_else(|| error("PDF_PAGES", "missing document failure unit"))?;
        if unit.unit_id != "pdf-document"
            || unit.ordinal != manifest.pages.len() as u32
            || unit.status != ParseStatus::Failed
            || unit.kind != UnitKind::Unknown
            || unit.text.is_some()
            || unit.resource_ids != [RAW_ID, MANIFEST_ID]
            || !blocked(manifest, "pdf-document")
        {
            return Err(error(
                "PDF_PAGES",
                "unconfirmed page count lacks document failure evidence",
            ));
        }
    }
    let status = if document_failure
        || manifest
            .pages
            .iter()
            .any(|p| p.status == ParseStatus::Failed)
    {
        ParseStatus::Failed
    } else if manifest
        .pages
        .iter()
        .any(|p| p.status == ParseStatus::NeedsReview)
        || manifest
            .issues
            .iter()
            .any(|i| i.severity == Severity::Blocking)
    {
        ParseStatus::NeedsReview
    } else {
        ParseStatus::Parsed
    };
    if manifest.status != status {
        return Err(error(
            "PDF_STATUS",
            "failed/review page incorrectly counted as whole-document parsed",
        ));
    }
    control.check()
}

#[cfg(test)]
#[path = "../tests/support/pdf_validate_tests.rs"]
mod tests;
