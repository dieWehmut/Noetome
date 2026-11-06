//! Literal author context, explicit recovered-expression attribution and precise UTF-8 note mappings.
use crate::{knowledge_coordinate_input::fingerprint, scan_model::*};
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
pub(crate) fn render(plan: &ScanPlan) -> (Vec<u8>, RenderedFile) {
    let mut out = "# 扫描公式预览\n\n## 标题\n\n".to_owned();
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
                    .expect("validated reconstructed expression"),
                "expression",
                "compiled_expression",
                &mut mappings,
            );
            out.push_str("\n$$\n\n");
        }
        Some(Representation::FormulaImage) => {
            mapped(
                &mut out,
                "![[original.png]]",
                "image_embed",
                "case.region",
                &mut mappings,
            );
            out.push_str("\n\n");
        }
        None => unreachable!("blocked scan has no preview"),
    }
    out.push_str(
        "该表达由选定扫描区域重建，并经符号与前提核对；作者原始 TeX 未知。\n\n## 识别原表达\n\n",
    );
    text(
        &mut out,
        &plan.recognized_expression,
        "recognized_expression",
        "recognition.recognized_tex",
        &mut mappings,
    );
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
            s.definition.as_deref().expect("confirmed author symbol"),
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
    for (value, field) in [
        ("[原始公式区域](original.png)", "case.region"),
        ("[选定扫描页](page.png)", "case.page"),
        ("[识别原响应](raw-response.json)", "recognition"),
        ("[识别表达](recognized.tex)", "recognition.recognized_tex"),
    ] {
        mapped(&mut out, value, "attachment_link", field, &mut mappings);
        out.push_str("\n\n");
    }
    if plan.case.pdf.is_some() {
        mapped(
            &mut out,
            "[归档 PDF](original.pdf)",
            "attachment_link",
            "case.pdf",
            &mut mappings,
        );
        out.push_str("\n\n");
    }
    let bytes = out.into_bytes();
    let pin = fingerprint(&bytes);
    let file = RenderedFile {
        relative_path: "扫描公式预览.md".into(),
        content_sha256: pin.sha256,
        byte_length: pin.byte_length,
        mappings,
    };
    (bytes, file)
}
