//! Bounded source-image reproduction from explicit, hand-authored declarations.
use crate::{
    extraction_output::{self, OutputRoot},
    image_input::{self, Inputs, check, encoded, error, parse},
    image_model::*,
    image_render,
    knowledge_coordinate_input::fingerprint,
    process_input::SingleInput,
    project::{self, ProjectError},
    sidecar_process::Control,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

pub struct ImageOutput {
    pub json: String,
    pub failed: bool,
}
#[cfg(test)]
thread_local! {pub(crate) static TEST_HOOK:std::cell::RefCell<Option<Box<dyn FnMut(&str)>>> = std::cell::RefCell::new(None);}
#[cfg(test)]
fn stage(at: &str) {
    TEST_HOOK.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() {
            hook(at);
        }
    });
}
#[cfg(test)]
#[path = "image_guard_tests.rs"]
mod guard_tests;

struct Evaluation {
    decision: Decision,
    author_ok: bool,
    geometry_ok: bool,
}
fn push(codes: &mut Vec<String>, code: &str) {
    if !codes.iter().any(|v| v == code) {
        codes.push(code.into());
    }
}
fn inside_point(point: [f64; 2], element: &Element) -> bool {
    let [x, y, w, h] = element.bbox;
    if element.kind == Shape::Circle {
        let r = w as f64 / 2.0;
        ((point[0] - x as f64 - r) / r).powi(2) + ((point[1] - y as f64 - r) / r).powi(2) < 1.0
    } else {
        point[0] > x as f64
            && point[0] < (x + w) as f64
            && point[1] > y as f64
            && point[1] < (y + h) as f64
    }
}
fn border(point: [u32; 2], element: &Element) -> bool {
    let [x, y, w, h] = element.bbox;
    let [px, py] = point;
    if element.kind == Shape::Circle {
        let r = w as f64 / 2.0;
        let distance = (px as f64 - x as f64 - r).hypot(py as f64 - y as f64 - r);
        (distance - r).abs() <= 1.0
    } else {
        px >= x
            && px <= x + w
            && py >= y
            && py <= y + h
            && (px == x || px == x + w || py == y || py == y + h)
    }
}
fn contains(a: [u32; 4], b: [u32; 4]) -> bool {
    a[0] <= b[0] && a[1] <= b[1] && a[0] + a[2] >= b[0] + b[2] && a[1] + a[3] >= b[1] + b[3]
}
fn overlaps(a: [u32; 4], b: [u32; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}
fn circle_geometry(element: &Element) -> (f64, f64, f64) {
    let [x, y, w, _] = element.bbox;
    let r = w as f64 / 2.0;
    (x as f64 + r, y as f64 + r, r)
}
fn shape_contains(parent: &Element, child: &Element) -> bool {
    if !contains(parent.bbox, child.bbox) {
        return false;
    }
    if parent.kind != Shape::Circle {
        return true;
    }
    let (cx, cy, r) = circle_geometry(parent);
    if child.kind == Shape::Circle {
        let (x, y, radius) = circle_geometry(child);
        (x - cx).hypot(y - cy) + radius <= r
    } else {
        let [x, y, w, h] = child.bbox;
        [[x, y], [x + w, y], [x, y + h], [x + w, y + h]]
            .iter()
            .all(|p| (p[0] as f64 - cx).hypot(p[1] as f64 - cy) <= r)
    }
}
fn shape_overlaps(a: &Element, b: &Element) -> bool {
    if !overlaps(a.bbox, b.bbox) {
        return false;
    }
    match (a.kind, b.kind) {
        (Shape::Circle, Shape::Circle) => {
            let (ax, ay, ar) = circle_geometry(a);
            let (bx, by, br) = circle_geometry(b);
            (ax - bx).hypot(ay - by) < ar + br
        }
        (Shape::Circle, Shape::Rect) | (Shape::Rect, Shape::Circle) => {
            let (circle, rect) = if a.kind == Shape::Circle {
                (a, b)
            } else {
                (b, a)
            };
            let (cx, cy, r) = circle_geometry(circle);
            let [x, y, w, h] = rect.bbox;
            let nearest_x = cx.clamp(x as f64, (x + w) as f64);
            let nearest_y = cy.clamp(y as f64, (y + h) as f64);
            (nearest_x - cx).hypot(nearest_y - cy) < r
        }
        _ => true,
    }
}
fn geometry(semantics: &Semantics) -> bool {
    let element = |id: &str| {
        semantics
            .elements
            .iter()
            .find(|e| e.id == id)
            .expect("validated element reference")
    };
    for relation in &semantics.relations {
        let from = element(&relation.from);
        let to = element(&relation.to);
        if !border(relation.start, from) || !border(relation.end, to) {
            return false;
        }
        let [sx, sy] = relation.start;
        let [ex, ey] = relation.end;
        let dx = ex as f64 - sx as f64;
        let dy = ey as f64 - sy as f64;
        let length = dx.hypot(dy);
        let step = length.min(2.0) / length;
        if inside_point([sx as f64 + dx * step, sy as f64 + dy * step], from)
            || inside_point([ex as f64 - dx * step, ey as f64 - dy * step], to)
        {
            return false;
        }
    }
    for relation in &semantics.spatial_relations {
        let a_element = element(&relation.a);
        let b_element = element(&relation.b);
        let a = a_element.bbox;
        let b = b_element.bbox;
        let correct = match relation.relation {
            SpatialKind::LeftOf => a[0] + a[2] <= b[0],
            SpatialKind::Above => a[1] + a[3] <= b[1],
            SpatialKind::Inside => shape_contains(b_element, a_element),
            SpatialKind::Overlaps => shape_overlaps(a_element, b_element),
            SpatialKind::Unknown => true,
        };
        if !correct {
            return false;
        }
    }
    true
}
fn text_width(value: &str) -> f64 {
    value
        .chars()
        .map(|c| match c {
            'W' | 'M' => 14.0,
            'w' | 'm' => 12.0,
            'A'..='Z' => 10.0,
            '0'..='9' => 9.0,
            c if c.is_ascii() => 8.0,
            c if c as u32 >= 0x1f000 => 28.0,
            _ => 14.0,
        })
        .sum()
}
fn simple_text(value: &str) -> bool {
    value.trim() == value
        && !value.contains(['\n', '\r', '\t'])
        && !value
            .chars()
            .zip(value.chars().skip(1))
            .any(|(a, b)| a.is_whitespace() && b.is_whitespace())
}
fn text_box(value: &str, x: f64, y: f64) -> [f64; 4] {
    let w = text_width(value) + 12.0;
    [x - w / 2.0, y - 14.0, w, 18.0]
}
fn floating_overlap(a: [f64; 4], b: [f64; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}
fn floating_bbox(value: [u32; 4]) -> [f64; 4] {
    value.map(|n| n as f64)
}
fn line_through(start: [u32; 2], end: [u32; 2], bounds: [u32; 4]) -> bool {
    // Liang-Barsky clipping against an open interior; touching a border is not crossing a shape.
    let [x, y, w, h] = bounds;
    let margin = 0.01;
    let [sx, sy] = start.map(|n| n as f64);
    let [ex, ey] = end.map(|n| n as f64);
    let dx = ex - sx;
    let dy = ey - sy;
    let mut low = 0.0f64;
    let mut high = 1.0f64;
    for (p, q) in [
        (-dx, sx - x as f64 - margin),
        (dx, (x + w) as f64 - margin - sx),
        (-dy, sy - y as f64 - margin),
        (dy, (y + h) as f64 - margin - sy),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                low = low.max(t);
            } else {
                high = high.min(t);
            }
            if low > high {
                return false;
            }
        }
    }
    low <= high
}
fn head_overlaps_bounds(points: [[f64; 2]; 3], bounds: [u32; 4]) -> bool {
    // Separating-axis test against a conservative one-pixel stroke envelope.
    // Checking only the arrow's center line misses corners under its head.
    let [x, y, w, h] = bounds.map(|n| n as f64);
    let corners = [
        [x - 1.0, y - 1.0],
        [x + w + 1.0, y - 1.0],
        [x + w + 1.0, y + h + 1.0],
        [x - 1.0, y + h + 1.0],
    ];
    let axes = [
        [1.0, 0.0],
        [0.0, 1.0],
        [points[1][1] - points[0][1], points[0][0] - points[1][0]],
        [points[2][1] - points[1][1], points[1][0] - points[2][0]],
        [points[0][1] - points[2][1], points[2][0] - points[0][0]],
    ];
    for axis in axes {
        let project = |p: [f64; 2]| p[0] * axis[0] + p[1] * axis[1];
        let triangle = points.map(project);
        let rectangle = corners.map(project);
        let minimum = |values: &[f64]| values.iter().copied().fold(f64::INFINITY, f64::min);
        let maximum = |values: &[f64]| values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if maximum(&triangle) <= minimum(&rectangle) || maximum(&rectangle) <= minimum(&triangle) {
            return false;
        }
    }
    true
}
fn layout(semantics: &Semantics, dimensions: [u32; 2]) -> Option<&'static str> {
    // This finite renderer uses fixed black text. Keep the source palette
    // unchanged and admit only a conservative bright-fill range for SVG.
    if semantics.legend.iter().any(|legend| {
        [1, 3, 5].iter().any(|offset| {
            u8::from_str_radix(&legend.color[*offset..*offset + 2], 16)
                .expect("validated literal RGB color")
                < 192
        })
    }) {
        return Some("IMAGE_COLOR_CONTRAST");
    }
    let mut boxes: Vec<([f64; 4], Option<&str>)> = vec![];
    for element in &semantics.elements {
        let [x, y, w, h] = element.bbox;
        let cx = x as f64 + w as f64 / 2.0;
        let cy = y as f64 + h as f64 / 2.0;
        let capacity = if element.kind == Shape::Circle {
            w as f64 * 0.75
        } else {
            w as f64
        };
        let label = element.label.as_deref().expect("known label");
        let values = [Some(label), element.unit.as_deref()];
        if values
            .iter()
            .flatten()
            .any(|v| !simple_text(v) || text_width(v) + 12.0 > capacity)
            || (if element.unit.is_some() { 44 } else { 28 }) > h
        {
            return Some("IMAGE_TEXT_LAYOUT");
        }
        boxes.push((
            text_box(
                label,
                cx,
                cy + if element.unit.is_some() { -8.0 } else { 5.0 },
            ),
            Some(element.id.as_str()),
        ));
        if let Some(unit) = &element.unit {
            boxes.push((text_box(unit, cx, cy + 10.0), Some(element.id.as_str())));
        }
    }
    for legend in &semantics.legend {
        let [x, y, w, h] = legend.bbox;
        let cx = x as f64 + w as f64 / 2.0;
        let cy = y as f64 + h as f64 / 2.0;
        for (value, offset) in [
            (legend.label.as_deref().expect("known legend"), -8.0),
            (legend.meaning.as_deref().expect("known legend"), 10.0),
        ] {
            if !simple_text(value) || text_width(value) + 12.0 > w as f64 || h < 44 {
                return Some("IMAGE_TEXT_LAYOUT");
            }
            boxes.push((text_box(value, cx, cy + offset), None));
        }
        if semantics
            .elements
            .iter()
            .any(|e| overlaps(e.bbox, legend.bbox))
        {
            return Some("IMAGE_GEOMETRY_LAYOUT");
        }
    }
    for (i, element) in semantics.elements.iter().enumerate() {
        for other in semantics.elements.iter().skip(i + 1) {
            if overlaps(element.bbox, other.bbox)
                && !contains(element.bbox, other.bbox)
                && !contains(other.bbox, element.bbox)
            {
                return Some("IMAGE_GEOMETRY_LAYOUT");
            }
        }
    }
    for (i, legend) in semantics.legend.iter().enumerate() {
        if semantics
            .legend
            .iter()
            .skip(i + 1)
            .any(|l| overlaps(legend.bbox, l.bbox))
        {
            return Some("IMAGE_GEOMETRY_LAYOUT");
        }
    }
    for relation in &semantics.relations {
        let [sx, sy] = relation.start;
        let [ex, ey] = relation.end;
        let cx = (sx as f64 + ex as f64) / 2.0;
        let cy = (sy as f64 + ey as f64) / 2.0;
        let length = (ex as f64 - sx as f64).hypot(ey as f64 - sy as f64);
        if length < 20.0 {
            return Some("IMAGE_GEOMETRY_LAYOUT");
        }
        if matches!(relation.direction, Direction::Forward | Direction::Reverse)
            && image_render::triangle(relation).iter().any(|p| {
                p[0] < 0.0
                    || p[1] < 0.0
                    || p[0] > dimensions[0] as f64
                    || p[1] > dimensions[1] as f64
            })
        {
            return Some("IMAGE_GEOMETRY_LAYOUT");
        }
        if matches!(relation.direction, Direction::Forward | Direction::Reverse) {
            let head = image_render::triangle(relation);
            if semantics.elements.iter().any(|e| {
                e.id != relation.from && e.id != relation.to && head_overlaps_bounds(head, e.bbox)
            }) || semantics
                .legend
                .iter()
                .any(|legend| head_overlaps_bounds(head, legend.bbox))
            {
                return Some("IMAGE_GEOMETRY_LAYOUT");
            }
        }
        if semantics.elements.iter().any(|e| {
            e.id != relation.from
                && e.id != relation.to
                && line_through(relation.start, relation.end, e.bbox)
        }) || semantics
            .legend
            .iter()
            .any(|l| line_through(relation.start, relation.end, l.bbox))
        {
            return Some("IMAGE_GEOMETRY_LAYOUT");
        }
        let label = relation.label.as_deref().expect("known relation label");
        let effect = image_render::effect_text(relation.effect);
        for (value, offset) in [
            (Some(effect).filter(|v| !v.is_empty()), -28.0),
            (Some(label).filter(|v| !v.is_empty()), -10.0),
            (relation.unit.as_deref(), 18.0),
        ] {
            if let Some(value) = value {
                if !simple_text(value) || text_width(value) + 12.0 > length - 20.0 {
                    return Some("IMAGE_TEXT_LAYOUT");
                }
                boxes.push((text_box(value, cx, cy + offset), None));
            }
        }
    }
    for (i, (bounds, owner)) in boxes.iter().enumerate() {
        if bounds[0] < 0.0
            || bounds[1] < 0.0
            || bounds[0] + bounds[2] > dimensions[0] as f64
            || bounds[1] + bounds[3] > dimensions[1] as f64
        {
            return Some("IMAGE_TEXT_LAYOUT");
        }
        for element in &semantics.elements {
            if *owner != Some(element.id.as_str())
                && floating_overlap(*bounds, floating_bbox(element.bbox))
            {
                let allowed_parent = owner.is_some_and(|id| {
                    semantics
                        .elements
                        .iter()
                        .find(|e| e.id == id)
                        .is_some_and(|child| contains(element.bbox, child.bbox))
                });
                if !allowed_parent {
                    return Some("IMAGE_GEOMETRY_LAYOUT");
                }
            }
        }
        if boxes
            .iter()
            .skip(i + 1)
            .any(|(other, _)| floating_overlap(*bounds, *other))
        {
            return Some("IMAGE_GEOMETRY_LAYOUT");
        }
    }
    None
}
fn evaluate(inputs: &Inputs, profile: &Profile) -> Evaluation {
    let mut decision = Decision {
        representation: None,
        fallback_reasons: vec![],
        issue_codes: vec![],
    };
    let semantics = &inputs.semantics;
    if !semantics.complete {
        push(&mut decision.issue_codes, "IMAGE_DECLARATION_INCOMPLETE");
    }
    if semantics.elements.is_empty()
        || semantics.elements.iter().any(|e| e.label.is_none())
        || semantics
            .legend
            .iter()
            .any(|l| l.label.is_none() || l.meaning.is_none())
    {
        push(&mut decision.issue_codes, "IMAGE_UNKNOWN_LABEL");
    }
    if semantics.relations.iter().any(|r| {
        r.direction == Direction::Unknown || r.effect == Effect::Unknown || r.label.is_none()
    }) {
        push(&mut decision.issue_codes, "IMAGE_UNKNOWN_RELATION");
    }
    if semantics
        .spatial_relations
        .iter()
        .any(|r| r.relation == SpatialKind::Unknown)
    {
        push(&mut decision.issue_codes, "IMAGE_UNKNOWN_SPATIAL");
    }
    let author_ok = decision.issue_codes.is_empty();
    let geometry_ok = geometry(semantics);
    if !geometry_ok {
        push(&mut decision.issue_codes, "IMAGE_GEOMETRY_MISMATCH");
    }
    if inputs.case.readability == Readability::Unreadable {
        push(&mut decision.issue_codes, "IMAGE_SOURCE_UNREADABLE");
    }
    if !profile.local_images {
        push(&mut decision.issue_codes, "IMAGE_LOCAL_IMAGES_UNAVAILABLE");
    }
    if decision.issue_codes.is_empty() {
        match inputs.case.source_kind {
            SourceKind::ExperimentalCurve => push(
                &mut decision.fallback_reasons,
                if semantics.data_available {
                    "IMAGE_CURVE_ORIGINAL"
                } else {
                    "IMAGE_CURVE_NO_DATA"
                },
            ),
            SourceKind::Observation => {
                push(&mut decision.fallback_reasons, "IMAGE_OBSERVATION_ORIGINAL")
            }
            _ => {
                if semantics
                    .elements
                    .iter()
                    .any(|e| e.kind == Shape::Unsupported)
                {
                    push(&mut decision.fallback_reasons, "IMAGE_GEOMETRY_UNSUPPORTED");
                }
                if !profile.verified
                    || !crate::visual_capabilities::svg(&profile.app, &profile.app_version)
                {
                    push(&mut decision.fallback_reasons, "IMAGE_TARGET_UNVERIFIED");
                }
                if !profile.svg_supported {
                    push(&mut decision.fallback_reasons, "IMAGE_SVG_UNAVAILABLE");
                }
                if decision.fallback_reasons.is_empty() {
                    if let Some(reason) = layout(semantics, inputs.case.dimensions) {
                        push(&mut decision.fallback_reasons, reason);
                    }
                }
            }
        }
        decision.representation = Some(if decision.fallback_reasons.is_empty() {
            Representation::SvgRedraw
        } else {
            Representation::OriginalImage
        });
    }
    Evaluation {
        decision,
        author_ok,
        geometry_ok,
    }
}
fn save<T: Serialize + DeserializeOwned>(
    out: &mut OutputRoot,
    name: &str,
    value: &T,
) -> Result<Vec<u8>, ProjectError> {
    let bytes = encoded(value)?;
    out.add(name, &bytes)?;
    let actual = out.read(name)?;
    let decoded: T = parse(&actual)?;
    if actual != bytes || encoded(&decoded)? != bytes {
        return Err(error(
            "IMAGE_OUTPUT_CHANGED",
            3,
            "saved value differs from validated bytes",
        ));
    }
    Ok(bytes)
}
fn save_rendered(
    out: &mut OutputRoot,
    bytes: &[u8],
    file: &RenderedFile,
) -> Result<(), ProjectError> {
    out.add(&file.relative_path, bytes)?;
    let actual = out.read(&file.relative_path)?;
    if actual != bytes
        || fingerprint(&actual).sha256 != file.content_sha256
        || actual.len() as u64 != file.byte_length
    {
        return Err(error("IMAGE_RENDER_CHANGED", 3, "rendered file changed"));
    }
    for mapping in &file.mappings {
        let slice = actual
            .get(mapping.start_byte as usize..mapping.end_byte as usize)
            .ok_or_else(|| error("IMAGE_RENDER_MAPPING", 3, "mapping outside rendered UTF8"))?;
        std::str::from_utf8(slice).map_err(|e| error("IMAGE_RENDER_MAPPING", 3, e))?;
    }
    Ok(())
}
pub fn run(case: &Path, profile: &Path, output: &Path) -> Result<ImageOutput, ProjectError> {
    let control = Control::new(30000, Arc::new(AtomicBool::new(false)));
    let path = project::check_components(output)?;
    let inputs = Inputs::open(case, &control)?;
    let profile_input = SingleInput::open(profile, &control)
        .map_err(|e| error("IMAGE_PROFILE_INPUT", e.exit_code(), e))?;
    let profile: Profile = parse(&profile_input.bytes)?;
    image_input::validate_profile(&profile)?;
    if extraction_output::inside(&path, inputs.held.root())?
        || path == inputs.held.root()
        || profile_input.path.starts_with(&path)
    {
        return Err(error(
            "IMAGE_OUTPUT_OVERLAP",
            2,
            "output overlaps selected attachments or profile",
        ));
    }
    #[cfg(test)]
    stage("inputs_held");
    inputs.verify(&control)?;
    profile_input.verify(&control)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut out = OutputRoot::create_image(&path, &request_id)?;
    let result = (|| {
        let originals = BTreeMap::from([
            (
                "source-case.json".to_owned(),
                inputs.bytes("case.json")?.to_vec(),
            ),
            (
                "source-semantics.json".into(),
                inputs.bytes(&inputs.case.semantics.file)?.to_vec(),
            ),
            ("profile.json".into(), profile_input.bytes.clone()),
            (
                "original.png".into(),
                inputs.bytes(&inputs.case.image.file)?.to_vec(),
            ),
        ]);
        let input_fingerprints = originals
            .iter()
            .map(|(name, bytes)| (name.clone(), fingerprint(bytes)))
            .collect();
        for (name, bytes) in originals {
            check(&control)?;
            out.add(&name, &bytes)?;
        }
        let evaluation = evaluate(&inputs, &profile);
        let ready = evaluation.decision.representation.is_some();
        let mut files = vec![];
        let mut plan_fingerprint = None;
        if ready {
            let plan = ImagePlan {
                schema_version: 1,
                producer: "knowledge_image".into(),
                scope: "selected_image_fidelity_v1".into(),
                request_id: request_id.clone(),
                case: inputs.case.clone(),
                semantics: inputs.semantics.clone(),
                decision: evaluation.decision.clone(),
            };
            let bytes = save(&mut out, "plan.json", &plan)?;
            let plan: ImagePlan = parse(&bytes)?;
            plan_fingerprint = Some(fingerprint(&bytes));
            let (bytes, file) = image_render::note(&plan);
            save_rendered(&mut out, &bytes, &file)?;
            files.push(file);
            if plan.decision.representation == Some(Representation::SvgRedraw) {
                let (bytes, file) = image_render::svg(&plan);
                save_rendered(&mut out, &bytes, &file)?;
                files.push(file);
            }
        }
        let report = ImageReport {
            schema_version: 1,
            producer: "knowledge_image".into(),
            scope: "selected_image_fidelity_v1".into(),
            request_id,
            status: if ready { "passed" } else { "needs_review" }.into(),
            preview_status: if ready { "rendered" } else { "not_run" }.into(),
            knowledge_status: "pending".into(),
            scientific_quality: "not_run".into(),
            published: false,
            network_calls: 0,
            paid_calls: 0,
            case: inputs.case.clone(),
            semantics: inputs.semantics.clone(),
            profile,
            checks: Checks {
                source_preservation: "passed".into(),
                author_declaration: if evaluation.author_ok {
                    "passed"
                } else {
                    "needs_review"
                }
                .into(),
                geometry_review: if evaluation.geometry_ok {
                    "passed"
                } else {
                    "needs_review"
                }
                .into(),
                semantic_review: if ready { "passed" } else { "needs_review" }.into(),
                note_plan: if ready { "passed" } else { "not_run" }.into(),
                rendering: if ready { "passed" } else { "not_run" }.into(),
                saved_output: "passed".into(),
            },
            decision: evaluation.decision,
            plan_fingerprint,
            inputs: input_fingerprints,
            files,
            artifacts: out.fingerprints(),
        };
        let bytes = save(&mut out, "image.json", &report)?;
        let json = String::from_utf8(bytes).map_err(|e| error("IMAGE_SCHEMA", 3, e))?;
        let markdown = format!(
            "# 选定图像保真预览报告\n\n状态：{}；预览：{}；知识：pending；科学质量：not_run；发布：false。\n\n仅选定图像和手工结构声明工程核对；不执行自动图像理解；网络与付费调用：0。\n\n```json\n{json}\n```\n",
            report.status, report.preview_status
        );
        out.add("image.md", markdown.as_bytes())?;
        if out.read("image.md")? != markdown.as_bytes() {
            return Err(error(
                "IMAGE_OUTPUT_CHANGED",
                3,
                "saved Markdown report differs",
            ));
        }
        #[cfg(test)]
        stage("results_saved");
        inputs.verify(&control)?;
        profile_input.verify(&control)?;
        out.verify()?;
        check(&control)?;
        Ok(ImageOutput {
            json: format!("{json}\n"),
            failed: !ready,
        })
    })();
    if result.is_err() {
        out.discard_image_result()?;
    }
    result
}
