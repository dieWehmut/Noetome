//! Fixed Markdown syntax with literal source declarations and precise UTF-8 mappings.
use crate::{formula_model::*, knowledge_coordinate_input::fingerprint};

fn mapped(out: &mut String, text: &str, kind: &str, field: &str, mappings: &mut Vec<Mapping>) {
    let start = out.len();
    out.push_str(text);
    mappings.push(Mapping {
        kind: kind.into(),
        source_field: field.into(),
        start_byte: start as u64,
        end_byte: out.len() as u64,
    });
}
fn text(out: &mut String, value: &str, kind: &str, field: &str, mappings: &mut Vec<Mapping>) {
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
pub(crate) fn render(plan: &FormulaPlan) -> (Vec<u8>, RenderedFile) {
    let mut out = "# 公式预览\n\n## 标题\n\n".to_owned();
    let mut mappings = vec![];
    text(
        &mut out,
        &plan.case.title,
        "title",
        "case.title",
        &mut mappings,
    );
    out.push_str("## 公式\n\n");
    match plan.decision.representation {
        Some(Representation::NativeMath) => {
            out.push_str("$$\n");
            mapped(
                &mut out,
                plan.compiled_expression
                    .as_deref()
                    .expect("validated native plan"),
                "expression",
                "tex",
                &mut mappings,
            );
            out.push_str("\n$$\n\n");
        }
        Some(Representation::FormulaImage) => {
            mapped(
                &mut out,
                "![[original.png]]",
                "image_embed",
                "region",
                &mut mappings,
            );
            out.push_str("\n\n");
        }
        None => unreachable!("blocked expression has no plan"),
    }
    out.push_str("## 符号定义\n\n");
    for (i, s) in plan.semantics.symbols.iter().enumerate() {
        out.push_str(&format!("### 符号 {}\n\n符号：\n\n", i + 1));
        text(
            &mut out,
            &s.symbol,
            "symbol",
            &format!("semantics.symbols[{i}].symbol"),
            &mut mappings,
        );
        out.push_str("定义：\n\n");
        text(
            &mut out,
            s.definition.as_deref().expect("confirmed symbol"),
            "definition",
            &format!("semantics.symbols[{i}].definition"),
            &mut mappings,
        );
        out.push_str("单位：\n\n");
        text(
            &mut out,
            s.unit.as_deref().unwrap_or("来源未给出单位"),
            "unit",
            &format!("semantics.symbols[{i}].unit"),
            &mut mappings,
        );
    }
    for (heading, field, kind, values) in [
        (
            "适用前提",
            "conditions",
            "condition",
            &plan.semantics.conditions,
        ),
        ("范围", "ranges", "range", &plan.semantics.ranges),
        (
            "例外",
            "exceptions",
            "exception",
            &plan.semantics.exceptions,
        ),
    ] {
        out.push_str(&format!("## {heading}\n\n"));
        if values.is_empty() {
            out.push_str("选定来源未列出。\n\n");
        }
        for (i, value) in values.iter().enumerate() {
            text(
                &mut out,
                value,
                kind,
                &format!("semantics.{field}[{i}]"),
                &mut mappings,
            );
        }
    }
    out.push_str("## 原始附件\n\n");
    if plan.case.tex.is_some() {
        mapped(
            &mut out,
            "[原始 TeX](original.tex)",
            "attachment_link",
            "tex",
            &mut mappings,
        );
        out.push_str("\n\n");
    }
    if plan.case.region.is_some() {
        mapped(
            &mut out,
            "[原始公式区域](original.png)",
            "attachment_link",
            "region",
            &mut mappings,
        );
        out.push_str("\n\n");
    }
    let bytes = out.into_bytes();
    let pin = fingerprint(&bytes);
    let file = RenderedFile {
        relative_path: "公式预览.md".into(),
        content_sha256: pin.sha256,
        byte_length: pin.byte_length,
        mappings,
    };
    (bytes, file)
}
