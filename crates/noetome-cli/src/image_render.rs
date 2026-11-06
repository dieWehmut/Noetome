//! Fixed, local XML/Markdown grammar with source-field byte mappings.
use crate::{image_model::*, knowledge_coordinate_input::fingerprint};

fn mapped(out: &mut String, value: &str, kind: &str, field: &str, mappings: &mut Vec<Mapping>) {
    let start = out.len();
    out.push_str(value);
    mappings.push(Mapping {
        kind: kind.into(),
        source_field: field.into(),
        start_byte: start as u64,
        end_byte: out.len() as u64,
    });
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn primitive(
    out: &mut String,
    tag: String,
    kind: &str,
    field: String,
    mappings: &mut Vec<Mapping>,
) {
    mapped(out, &tag, kind, &field, mappings);
    out.push('\n');
}
fn svg_text(
    out: &mut String,
    id: String,
    value: &str,
    x: f64,
    y: f64,
    kind: &str,
    field: String,
    mappings: &mut Vec<Mapping>,
) {
    primitive(
        out,
        format!(
            "<text id=\"{id}\" x=\"{x:.3}\" y=\"{y:.3}\" fill=\"#000000\" font-size=\"14\" font-family=\"sans-serif\" text-anchor=\"middle\">{}</text>",
            xml(value)
        ),
        kind,
        field,
        mappings,
    );
}
pub(crate) fn effect_text(effect: Effect) -> &'static str {
    match effect {
        Effect::Positive => "+",
        Effect::Negative => "−",
        Effect::Neutral => "",
        Effect::Unknown => unreachable!("unknown effect has no successful plan"),
    }
}
pub(crate) fn triangle(relation: &Relation) -> [[f64; 2]; 3] {
    let (tip, base) = if relation.direction == Direction::Reverse {
        (relation.start, relation.end)
    } else {
        (relation.end, relation.start)
    };
    let dx = tip[0] as f64 - base[0] as f64;
    let dy = tip[1] as f64 - base[1] as f64;
    let length = dx.hypot(dy);
    let ux = dx / length;
    let uy = dy / length;
    let x = tip[0] as f64;
    let y = tip[1] as f64;
    [
        [x, y],
        [x - 10.0 * ux - 5.0 * uy, y - 10.0 * uy + 5.0 * ux],
        [x - 10.0 * ux + 5.0 * uy, y - 10.0 * uy - 5.0 * ux],
    ]
}
fn color<'a>(semantics: &'a Semantics, legend: &Option<String>, default: &'a str) -> &'a str {
    legend
        .as_ref()
        .map(|id| {
            semantics
                .legend
                .iter()
                .find(|l| l.id == *id)
                .expect("validated legend reference")
                .color
                .as_str()
        })
        .unwrap_or(default)
}
fn file(bytes: &[u8], name: &str, mappings: Vec<Mapping>) -> RenderedFile {
    let pin = fingerprint(bytes);
    RenderedFile {
        relative_path: name.into(),
        content_sha256: pin.sha256,
        byte_length: pin.byte_length,
        mappings,
    }
}
pub(crate) fn svg(plan: &ImagePlan) -> (Vec<u8>, RenderedFile) {
    let [width, height] = plan.case.dimensions;
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">\n"
    );
    let mut mappings = vec![];
    primitive(
        &mut out,
        format!(
            "<rect id=\"image-canvas\" x=\"0\" y=\"0\" width=\"{width}\" height=\"{height}\" fill=\"#ffffff\"/>"
        ),
        "canvas_background",
        "case.dimensions".into(),
        &mut mappings,
    );
    let mut order: Vec<_> = plan.semantics.elements.iter().enumerate().collect();
    order.sort_by_key(|(_, e)| std::cmp::Reverse(e.bbox[2] as u64 * e.bbox[3] as u64));
    for (i, element) in order {
        let [x, y, w, h] = element.bbox;
        let fill = color(&plan.semantics, &element.legend_id, "#ffffff");
        let tag = match element.kind {
            Shape::Rect => format!(
                "<rect id=\"element-{}\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{fill}\" stroke=\"#000000\" stroke-width=\"2\"/>",
                element.id
            ),
            Shape::Circle => format!(
                "<circle id=\"element-{}\" cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\" fill=\"{fill}\" stroke=\"#000000\" stroke-width=\"2\"/>",
                element.id,
                x as f64 + w as f64 / 2.0,
                y as f64 + h as f64 / 2.0,
                w as f64 / 2.0
            ),
            Shape::Unsupported => unreachable!("unsupported shape uses original image"),
        };
        primitive(
            &mut out,
            tag,
            "element_shape",
            format!("semantics.elements[{i}].bbox"),
            &mut mappings,
        );
    }
    for (i, element) in plan.semantics.elements.iter().enumerate() {
        let [x, y, w, h] = element.bbox;
        let cx = x as f64 + w as f64 / 2.0;
        let cy = y as f64 + h as f64 / 2.0;
        svg_text(
            &mut out,
            format!("element-{}-label", element.id),
            element.label.as_deref().expect("known label"),
            cx,
            cy + if element.unit.is_some() { -8.0 } else { 5.0 },
            "element_label",
            format!("semantics.elements[{i}].label"),
            &mut mappings,
        );
        if let Some(unit) = &element.unit {
            svg_text(
                &mut out,
                format!("element-{}-unit", element.id),
                unit,
                cx,
                cy + 10.0,
                "element_unit",
                format!("semantics.elements[{i}].unit"),
                &mut mappings,
            );
        }
    }
    for (i, legend) in plan.semantics.legend.iter().enumerate() {
        let [x, y, w, h] = legend.bbox;
        primitive(
            &mut out,
            format!(
                "<rect id=\"legend-{}\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{}\" stroke=\"#000000\" stroke-width=\"2\"/>",
                legend.id, legend.color
            ),
            "legend_swatch",
            format!("semantics.legend[{i}].bbox"),
            &mut mappings,
        );
        let cx = x as f64 + w as f64 / 2.0;
        let cy = y as f64 + h as f64 / 2.0;
        svg_text(
            &mut out,
            format!("legend-{}-label", legend.id),
            legend.label.as_deref().expect("known legend"),
            cx,
            cy - 8.0,
            "legend_label",
            format!("semantics.legend[{i}].label"),
            &mut mappings,
        );
        svg_text(
            &mut out,
            format!("legend-{}-meaning", legend.id),
            legend.meaning.as_deref().expect("known legend"),
            cx,
            cy + 10.0,
            "legend_meaning",
            format!("semantics.legend[{i}].meaning"),
            &mut mappings,
        );
    }
    for (i, relation) in plan.semantics.relations.iter().enumerate() {
        let [sx, sy] = relation.start;
        let [ex, ey] = relation.end;
        // Source arrows use black ink. Keep contrast on the explicit white
        // canvas; polarity remains visible text and a separate legend binding.
        let stroke = "#000000";
        primitive(
            &mut out,
            format!(
                "<line id=\"relation-{}\" x1=\"{sx}\" y1=\"{sy}\" x2=\"{ex}\" y2=\"{ey}\" stroke=\"{stroke}\" stroke-width=\"2\"/>",
                relation.id
            ),
            "relation_line",
            format!("semantics.relations[{i}]"),
            &mut mappings,
        );
        if matches!(relation.direction, Direction::Forward | Direction::Reverse) {
            let p = triangle(relation);
            primitive(
                &mut out,
                format!(
                    "<polygon id=\"relation-{}-head\" points=\"{:.3},{:.3} {:.3},{:.3} {:.3},{:.3}\" fill=\"{stroke}\" stroke=\"{stroke}\" stroke-width=\"2\"/>",
                    relation.id, p[0][0], p[0][1], p[1][0], p[1][1], p[2][0], p[2][1]
                ),
                "relation_head",
                format!("semantics.relations[{i}].direction"),
                &mut mappings,
            );
        }
        let cx = (sx as f64 + ex as f64) / 2.0;
        let cy = (sy as f64 + ey as f64) / 2.0;
        let effect = effect_text(relation.effect);
        if !effect.is_empty() {
            svg_text(
                &mut out,
                format!("relation-{}-effect", relation.id),
                effect,
                cx,
                cy - 28.0,
                "relation_effect",
                format!("semantics.relations[{i}].effect"),
                &mut mappings,
            );
        }
        svg_text(
            &mut out,
            format!("relation-{}-label", relation.id),
            relation.label.as_deref().expect("known relation label"),
            cx,
            cy - 10.0,
            "relation_label",
            format!("semantics.relations[{i}].label"),
            &mut mappings,
        );
        if let Some(unit) = &relation.unit {
            svg_text(
                &mut out,
                format!("relation-{}-unit", relation.id),
                unit,
                cx,
                cy + 18.0,
                "relation_unit",
                format!("semantics.relations[{i}].unit"),
                &mut mappings,
            );
        }
    }
    out.push_str("</svg>\n");
    let bytes = out.into_bytes();
    let metadata = file(&bytes, "redrawn.svg", mappings);
    (bytes, metadata)
}
fn literal(out: &mut String, value: &str, kind: &str, field: &str, mappings: &mut Vec<Mapping>) {
    let mut maximum = 0;
    let mut current = 0;
    for c in value.chars() {
        current = if c == '`' { current + 1 } else { 0 };
        maximum = maximum.max(current);
    }
    let fence = "`".repeat(3.max(maximum + 1));
    out.push_str(&format!("{fence}text\n"));
    mapped(out, value, kind, field, mappings);
    out.push_str(&format!("\n{fence}\n\n"));
}
fn field(
    out: &mut String,
    label: &str,
    value: &str,
    kind: &str,
    path: &str,
    mappings: &mut Vec<Mapping>,
) {
    out.push_str(&format!("{label}：\n\n"));
    literal(out, value, kind, path, mappings);
}
fn json<T: serde::Serialize>(value: T) -> String {
    serde_json::to_string(&value).expect("fixed semantic value serialization")
}
fn enum_value<T: serde::Serialize>(value: T) -> String {
    json(value).trim_matches('"').to_owned()
}
pub(crate) fn fallback_text(reason: &str) -> &'static str {
    match reason {
        "IMAGE_GEOMETRY_UNSUPPORTED" => "来源含本编译器尚不能保真的图形，使用完整原图。",
        "IMAGE_CURVE_NO_DATA" => "未提供原始实验数据，保留观测曲线原图，不生成精确曲线。",
        "IMAGE_CURVE_ORIGINAL" => "实验曲线使用原图；本入口不从图像或数据重建曲线。",
        "IMAGE_OBSERVATION_ORIGINAL" => "图像承载原始观测，保留完整原图。",
        "IMAGE_TARGET_UNVERIFIED" => "目标SVG能力尚未实测，使用本地原图。",
        "IMAGE_SVG_UNAVAILABLE" => "目标未启用SVG，使用本地原图。",
        "IMAGE_TEXT_LAYOUT" => "标签的空白、长度或位置不能在有限重绘中完整保留，使用原图。",
        "IMAGE_GEOMETRY_LAYOUT" => "有限重绘可能遮蔽图形、标签或箭头，使用原图。",
        "IMAGE_COLOR_CONTRAST" => "来源颜色与固定黑字不能在有限重绘中保证可读，保留原图。",
        _ => unreachable!("known fallback decision"),
    }
}
pub(crate) fn note(plan: &ImagePlan) -> (Vec<u8>, RenderedFile) {
    let mut out = "# 图像预览\n\n## 标题\n\n".to_owned();
    let mut mappings = vec![];
    literal(
        &mut out,
        &plan.case.title,
        "title",
        "case.title",
        &mut mappings,
    );
    out.push_str("## 图像\n\n");
    let (embed, source) = match plan.decision.representation {
        Some(Representation::SvgRedraw) => ("![[redrawn.svg]]", "semantics"),
        Some(Representation::OriginalImage) => ("![[original.png]]", "case.image"),
        None => unreachable!("blocked image has no note"),
    };
    mapped(&mut out, embed, "image_embed", source, &mut mappings);
    out.push_str("\n\n");
    for (i, reason) in plan.decision.fallback_reasons.iter().enumerate() {
        literal(
            &mut out,
            fallback_text(reason),
            "fallback_reason",
            &format!("decision.fallback_reasons[{i}]"),
            &mut mappings,
        );
    }
    out.push_str("## 图意\n\n");
    literal(
        &mut out,
        &plan.semantics.caption,
        "caption",
        "semantics.caption",
        &mut mappings,
    );
    out.push_str("## 元素\n\n");
    for (i, element) in plan.semantics.elements.iter().enumerate() {
        out.push_str(&format!("### 元素 {}\n\n", i + 1));
        for (label, value, kind, key) in [
            ("标识", element.id.clone(), "element_id", "id"),
            ("图形", enum_value(element.kind), "element_kind", "kind"),
            (
                "标签",
                element.label.clone().expect("known element label"),
                "element_label",
                "label",
            ),
            (
                "单位",
                element.unit.clone().unwrap_or("来源未给出单位".into()),
                "element_unit",
                "unit",
            ),
            (
                "图例",
                element.legend_id.clone().unwrap_or("来源未关联图例".into()),
                "element_legend",
                "legend_id",
            ),
            ("原图坐标", json(element.bbox), "element_geometry", "bbox"),
        ] {
            field(
                &mut out,
                label,
                &value,
                kind,
                &format!("semantics.elements[{i}].{key}"),
                &mut mappings,
            );
        }
    }
    out.push_str("## 关系\n\n");
    for (i, relation) in plan.semantics.relations.iter().enumerate() {
        out.push_str(&format!("### 关系 {}\n\n", i + 1));
        for (label, value, kind, key) in [
            ("标识", relation.id.clone(), "relation_id", "id"),
            ("起点元素", relation.from.clone(), "relation_from", "from"),
            ("终点元素", relation.to.clone(), "relation_to", "to"),
            ("起点坐标", json(relation.start), "relation_start", "start"),
            ("终点坐标", json(relation.end), "relation_end", "end"),
            (
                "箭头方向",
                enum_value(relation.direction),
                "relation_direction",
                "direction",
            ),
            (
                "正负作用",
                enum_value(relation.effect),
                "relation_effect",
                "effect",
            ),
            (
                "标签",
                relation.label.clone().expect("known relation label"),
                "relation_label",
                "label",
            ),
            (
                "单位",
                relation.unit.clone().unwrap_or("来源未给出单位".into()),
                "relation_unit",
                "unit",
            ),
            (
                "图例",
                relation
                    .legend_id
                    .clone()
                    .unwrap_or("来源未关联图例".into()),
                "relation_legend",
                "legend_id",
            ),
        ] {
            field(
                &mut out,
                label,
                &value,
                kind,
                &format!("semantics.relations[{i}].{key}"),
                &mut mappings,
            );
        }
    }
    out.push_str("## 空间关系\n\n");
    for (i, relation) in plan.semantics.spatial_relations.iter().enumerate() {
        out.push_str(&format!("### 空间关系 {}\n\n", i + 1));
        for (label, value, key) in [
            ("标识", relation.id.clone(), "id"),
            ("对象", relation.a.clone(), "a"),
            ("参照对象", relation.b.clone(), "b"),
            ("相对位置", enum_value(relation.relation), "relation"),
        ] {
            field(
                &mut out,
                label,
                &value,
                "spatial_relation",
                &format!("semantics.spatial_relations[{i}].{key}"),
                &mut mappings,
            );
        }
    }
    out.push_str("## 图例\n\n");
    for (i, legend) in plan.semantics.legend.iter().enumerate() {
        out.push_str(&format!("### 图例 {}\n\n", i + 1));
        for (label, value, kind, key) in [
            ("标识", legend.id.clone(), "legend_id", "id"),
            (
                "标签",
                legend.label.clone().expect("known legend label"),
                "legend_label",
                "label",
            ),
            (
                "含义",
                legend.meaning.clone().expect("known legend meaning"),
                "legend_meaning",
                "meaning",
            ),
            ("颜色", legend.color.clone(), "legend_color", "color"),
            ("原图坐标", json(legend.bbox), "legend_geometry", "bbox"),
        ] {
            field(
                &mut out,
                label,
                &value,
                kind,
                &format!("semantics.legend[{i}].{key}"),
                &mut mappings,
            );
        }
    }
    for (heading, key, values) in [
        ("适用条件", "conditions", &plan.semantics.conditions),
        ("例外", "exceptions", &plan.semantics.exceptions),
    ] {
        out.push_str(&format!("## {heading}\n\n"));
        if values.is_empty() {
            out.push_str("选定来源未列出。\n\n");
        }
        for (i, value) in values.iter().enumerate() {
            literal(
                &mut out,
                value,
                key,
                &format!("semantics.{key}[{i}]"),
                &mut mappings,
            );
        }
    }
    out.push_str("## 原始图像\n\n");
    mapped(
        &mut out,
        "[原始图像](original.png)",
        "attachment_link",
        "case.image",
        &mut mappings,
    );
    out.push_str("\n");
    let bytes = out.into_bytes();
    let metadata = file(&bytes, "图像预览.md", mappings);
    (bytes, metadata)
}
