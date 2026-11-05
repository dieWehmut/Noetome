//! Finite Markdown/Mermaid writer and a separate semantic reader of saved bytes.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    process_flow_model::*, process_flow_plan, process_model::RelationKind, process_model::StepKind,
    project::ProjectError,
};

const MAX_NOTE_BYTES: usize = 8 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

fn error(code: &'static str, message: impl Into<String>) -> ProjectError {
    ProjectError::new(code, 3, message)
}
fn content_error() -> ProjectError {
    error(
        "FLOW_OUTPUT_CONTENT",
        "流程笔记的结构或实际语义与已核对计划不一致。",
    )
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id.as_bytes()[0].is_ascii_lowercase()
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}
fn validate_plan(plan: &FlowPlan) -> Result<(), ProjectError> {
    let strict: FlowPlan = serde_json::from_value(
        serde_json::to_value(plan)
            .map_err(|_| error("FLOW_PLAN_INVALID", "流程计划不能序列化。"))?,
    )
    .map_err(|_| error("FLOW_PLAN_INVALID", "流程计划含未支持的固定字段或类型。"))?;
    let ids: Vec<_> = std::iter::once("process")
        .chain(plan.document.steps.iter().map(|s| s.id.as_str()))
        .chain(plan.document.relations.iter().map(|r| r.id.as_str()))
        .collect();
    let unique: BTreeSet<_> = ids.iter().copied().collect();
    let evidence: BTreeSet<_> = plan
        .document
        .evidence
        .iter()
        .map(|e| e.object_id.as_str())
        .collect();
    if ids.iter().any(|id| !valid_id(id))
        || ids.len() != unique.len()
        || evidence != unique
        || plan.document.evidence.len() != ids.len()
    {
        return Err(error(
            "FLOW_PLAN_INVALID",
            "流程对象身份或完整证据清单不一致。",
        ));
    }
    let rebuilt = process_flow_plan::build(&plan.document, &plan.profile, &plan.request_id)
        .map_err(|_| error("FLOW_PLAN_INVALID", "流程计划包含未通过语义门槛的过程。"))?;
    if strict != rebuilt {
        return Err(error(
            "FLOW_PLAN_INVALID",
            "流程计划与确定性来源规划结果不一致。",
        ));
    }
    Ok(())
}

fn mapping(
    section: &FlowSection,
    block_id: &str,
    kind: &str,
    item_id: Option<&str>,
    ids: &[String],
    start: usize,
    end: usize,
) -> FlowContentMapping {
    FlowContentMapping {
        section_id: section.section_id.clone(),
        block_id: block_id.into(),
        kind: kind.into(),
        item_id: item_id.map(str::to_owned),
        source_object_ids: ids.into(),
        start_byte: start as u64,
        end_byte: end as u64,
    }
}
fn link_mapping(
    section: &FlowSection,
    block_id: &str,
    item_id: Option<&str>,
    link: &FlowLink,
    start: usize,
    end: usize,
) -> FlowLinkMapping {
    FlowLinkMapping {
        section_id: section.section_id.clone(),
        block_id: block_id.into(),
        item_id: item_id.map(str::to_owned),
        target_path: link.target_path.clone(),
        target_heading: link.target_heading.clone(),
        display_text: link.display_text.clone(),
        source_object_ids: link.source_object_ids.clone(),
        start_byte: start as u64,
        end_byte: end as u64,
    }
}
fn edge_sources(plan: &FlowPlan, edge: &FlowEdge) -> Vec<String> {
    let mut ids = vec![edge.relation_id.clone()];
    if edge.kind == RelationKind::Branch {
        if let Some(relation) = plan
            .document
            .relations
            .iter()
            .find(|r| r.id == edge.relation_id)
        {
            ids.push(relation.left.clone());
        }
    }
    ids.sort();
    ids.dedup();
    ids
}
fn max_ticks(text: &str) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for c in text.chars() {
        current = if c == '`' { current + 1 } else { 0 };
        longest = longest.max(current);
    }
    longest
}
fn encode_label(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_punctuation() {
            out.push_str(&format!("#{};", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}
fn write_text(out: &mut String, text: &str, indent: &str) -> (usize, usize) {
    let fence = "`".repeat(3.max(max_ticks(text) + 1));
    out.push_str(&format!("{indent}{fence}text\n"));
    let start = out.len();
    for line in text.split('\n') {
        out.push_str(indent);
        out.push_str(line);
        out.push('\n');
    }
    let end = out.len() - 1; // Exclude the one structural LF, preserve source trailing LF.
    out.push_str(&format!("{indent}{fence}\n\n"));
    (start, end)
}
fn write_links(
    out: &mut String,
    section: &FlowSection,
    block_id: &str,
    item: Option<&str>,
    links: &[FlowLink],
    indent: &str,
    mappings: &mut Vec<FlowLinkMapping>,
) {
    for link in links {
        out.push_str(indent);
        let start = out.len();
        let stem = link
            .target_path
            .strip_suffix(".md")
            .expect("fixed note path");
        out.push_str(&format!(
            "[[{stem}#{}|{}]]",
            link.target_heading, link.display_text
        ));
        mappings.push(link_mapping(
            section,
            block_id,
            item,
            link,
            start,
            out.len(),
        ));
        out.push('\n');
    }
    out.push('\n');
}

pub(crate) fn render(plan: &FlowPlan) -> Result<Vec<FlowRenderedNote>, ProjectError> {
    validate_plan(plan)?;
    let mut rendered = Vec::new();
    let mut total = 0usize;
    for note in &plan.notes {
        let mut out = format!("# {}\n\n", note.title);
        let mut content_mappings = Vec::new();
        let mut links = Vec::new();
        for section in &note.sections {
            out.push_str(&format!("## {}\n\n", section.heading));
            for block in &section.blocks {
                match block {
                    FlowBlock::Text { content } => {
                        let (start, end) = write_text(&mut out, &content.text, "");
                        content_mappings.push(mapping(
                            section,
                            &content.block_id,
                            "text",
                            None,
                            &content.source_object_ids,
                            start,
                            end,
                        ));
                    }
                    FlowBlock::Links {
                        block_id,
                        links: semantic_links,
                    } => write_links(
                        &mut out,
                        section,
                        block_id,
                        None,
                        semantic_links,
                        "",
                        &mut links,
                    ),
                    FlowBlock::OrderedList { block_id, items } => {
                        for (i, item) in items.iter().enumerate() {
                            let marker = format!("{}.", i + 1);
                            let indent = " ".repeat(marker.len() + 1);
                            out.push_str(&marker);
                            out.push('\n');
                            let (start, end) = write_text(&mut out, &item.text, &indent);
                            content_mappings.push(mapping(
                                section,
                                block_id,
                                "list_item",
                                Some(&item.item_id),
                                &item.source_object_ids,
                                start,
                                end,
                            ));
                            write_links(
                                &mut out,
                                section,
                                block_id,
                                Some(&item.item_id),
                                &item.links,
                                &indent,
                                &mut links,
                            );
                        }
                    }
                    FlowBlock::Diagram { block_id } => {
                        let diagram = plan
                            .diagram
                            .as_ref()
                            .ok_or_else(|| error("FLOW_PLAN_INVALID", "缺少类型化流程图。"))?;
                        out.push_str("```mermaid\nflowchart TD\n");
                        for node in &diagram.nodes {
                            let start = out.len();
                            let (open, close) = if node.kind == StepKind::Decision {
                                ('{', '}')
                            } else {
                                ('[', ']')
                            };
                            out.push_str(&format!(
                                "  {}{open}\"{}\"{close}",
                                node.node_id,
                                encode_label(&node.text)
                            ));
                            content_mappings.push(mapping(
                                section,
                                block_id,
                                "diagram_node",
                                Some(&node.node_id),
                                std::slice::from_ref(&node.step_id),
                                start,
                                out.len(),
                            ));
                            out.push('\n');
                        }
                        for edge in &diagram.edges {
                            let start = out.len();
                            out.push_str(&format!(
                                "  {} -->|\"{}\"| {}",
                                edge.left_node_id,
                                encode_label(&edge.text),
                                edge.right_node_id
                            ));
                            content_mappings.push(mapping(
                                section,
                                block_id,
                                "diagram_edge",
                                Some(&edge.edge_id),
                                &edge_sources(plan, edge),
                                start,
                                out.len(),
                            ));
                            out.push('\n');
                        }
                        out.push_str("```\n\n");
                    }
                }
                if out.len() > MAX_NOTE_BYTES {
                    return Err(error("FLOW_RENDER_LIMIT", "流程笔记超过输出字节上限。"));
                }
            }
        }
        total = total
            .checked_add(out.len())
            .ok_or_else(|| error("FLOW_RENDER_LIMIT", "流程输出大小溢出。"))?;
        if total > MAX_OUTPUT_BYTES {
            return Err(error("FLOW_RENDER_LIMIT", "流程输出总字节数超过上限。"));
        }
        let bytes = out.into_bytes();
        rendered.push(FlowRenderedNote {
            file: FlowRenderedFile {
                relative_path: note.output_path.clone(),
                note_id: note.note_id.clone(),
                plan_revision: plan.revision,
                content_sha256: crate::text::sha256(&bytes),
                byte_length: bytes.len() as u64,
                content_mappings,
                links,
            },
            bytes,
        });
    }
    Ok(rendered)
}

// The reader never calls the writer or encode_label. It consumes the saved
// grammar, decodes labels/fences, checks their typed meanings and records the
// intervals it actually consumed. Shared mapping constructors only set identity.
struct Line<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}
struct Reader<'a> {
    text: &'a str,
    offset: usize,
}
impl<'a> Reader<'a> {
    fn line(&mut self) -> Result<Line<'a>, ProjectError> {
        let rest = &self.text[self.offset..];
        let length = rest.find('\n').ok_or_else(content_error)?;
        let start = self.offset;
        let end = start + length;
        self.offset = end + 1;
        Ok(Line {
            text: &self.text[start..end],
            start,
            end,
        })
    }
    fn exact(&mut self, expected: &str) -> Result<(), ProjectError> {
        if self.line()?.text != expected {
            return Err(content_error());
        }
        Ok(())
    }
    fn text_block(&mut self, indent: &str) -> Result<(String, usize, usize), ProjectError> {
        let opening = self.line()?;
        let fence = opening
            .text
            .strip_prefix(indent)
            .and_then(|s| s.strip_suffix("text"))
            .ok_or_else(content_error)?;
        if fence.len() < 3 || !fence.bytes().all(|c| c == b'`') {
            return Err(content_error());
        }
        let start = self.offset;
        let mut decoded = String::new();
        let end;
        loop {
            let line = self.line()?;
            let body = line.text.strip_prefix(indent).ok_or_else(content_error)?;
            if body == fence {
                end = line.start.checked_sub(1).ok_or_else(content_error)?;
                break;
            }
            decoded.push_str(body);
            decoded.push('\n');
        }
        if decoded.pop() != Some('\n') || max_ticks(&decoded) >= fence.len() {
            return Err(content_error());
        }
        self.exact("")?;
        Ok((decoded, start, end))
    }
}
fn decode_label(encoded: &str) -> Result<String, ProjectError> {
    let mut result = String::new();
    let mut chars = encoded.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '#' {
            let mut digits = String::new();
            while chars.peek().is_some_and(char::is_ascii_digit) {
                digits.push(chars.next().expect("peeked decimal digit"));
                if digits.len() > 3 {
                    return Err(content_error());
                }
            }
            if digits.is_empty() || digits.starts_with('0') || chars.next() != Some(';') {
                return Err(content_error());
            }
            let value = digits
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(content_error)?;
            if !value.is_ascii_punctuation() {
                return Err(content_error());
            }
            result.push(value);
        } else {
            if c.is_ascii_punctuation() || c.is_control() {
                return Err(content_error());
            }
            result.push(c);
        }
    }
    Ok(result)
}
fn read_node(line: &str, expected: &FlowNode) -> Result<(), ProjectError> {
    let declaration = line.strip_prefix("  ").ok_or_else(content_error)?;
    let (id, label, kind) = if let Some((id, rest)) = declaration.split_once("[\"") {
        (
            id,
            rest.strip_suffix("\"]").ok_or_else(content_error)?,
            StepKind::Step,
        )
    } else if let Some((id, rest)) = declaration.split_once("{\"") {
        (
            id,
            rest.strip_suffix("\"}").ok_or_else(content_error)?,
            StepKind::Decision,
        )
    } else {
        return Err(content_error());
    };
    if id != expected.node_id || kind != expected.kind || decode_label(label)? != expected.text {
        return Err(content_error());
    }
    Ok(())
}
fn read_edge(line: &str, expected: &FlowEdge) -> Result<(), ProjectError> {
    let declaration = line.strip_prefix("  ").ok_or_else(content_error)?;
    let (left, rest) = declaration
        .split_once(" -->|\"")
        .ok_or_else(content_error)?;
    let (label, right) = rest.rsplit_once("\"| ").ok_or_else(content_error)?;
    if left != expected.left_node_id
        || right != expected.right_node_id
        || decode_label(label)? != expected.text
    {
        return Err(content_error());
    }
    Ok(())
}
fn read_links(
    reader: &mut Reader<'_>,
    section: &FlowSection,
    block_id: &str,
    item: Option<&str>,
    links: &[FlowLink],
    indent: &str,
    mappings: &mut Vec<FlowLinkMapping>,
) -> Result<(), ProjectError> {
    for expected in links {
        let line = reader.line()?;
        let token = line.text.strip_prefix(indent).ok_or_else(content_error)?;
        let inside = token
            .strip_prefix("[[")
            .and_then(|s| s.strip_suffix("]]"))
            .ok_or_else(|| error("FLOW_OUTPUT_LINK", "正文链接不是受支持的普通内部链接。"))?;
        let (target, alias) = inside.split_once('|').ok_or_else(content_error)?;
        let (stem, heading) = target.split_once('#').ok_or_else(content_error)?;
        if format!("{stem}.md") != expected.target_path
            || heading != expected.target_heading
            || alias != expected.display_text
        {
            return Err(error(
                "FLOW_OUTPUT_LINK",
                "正文链接目的地、标题或文字与语义计划不一致。",
            ));
        }
        mappings.push(link_mapping(
            section,
            block_id,
            item,
            expected,
            line.start + indent.len(),
            line.end,
        ));
    }
    reader.exact("")
}
fn read_note(
    plan: &FlowPlan,
    note: &FlowNote,
    bytes: &[u8],
) -> Result<FlowRenderedFile, ProjectError> {
    let text = std::str::from_utf8(bytes).map_err(|_| content_error())?;
    let mut reader = Reader { text, offset: 0 };
    let mut content_mappings = Vec::new();
    let mut links = Vec::new();
    reader.exact(&format!("# {}", note.title))?;
    reader.exact("")?;
    for section in &note.sections {
        reader.exact(&format!("## {}", section.heading))?;
        reader.exact("")?;
        for block in &section.blocks {
            match block {
                FlowBlock::Text { content } => {
                    let (decoded, start, end) = reader.text_block("")?;
                    if decoded != content.text {
                        return Err(content_error());
                    }
                    content_mappings.push(mapping(
                        section,
                        &content.block_id,
                        "text",
                        None,
                        &content.source_object_ids,
                        start,
                        end,
                    ));
                }
                FlowBlock::Links {
                    block_id,
                    links: semantic_links,
                } => read_links(
                    &mut reader,
                    section,
                    block_id,
                    None,
                    semantic_links,
                    "",
                    &mut links,
                )?,
                FlowBlock::OrderedList { block_id, items } => {
                    for (i, item) in items.iter().enumerate() {
                        let marker = format!("{}.", i + 1);
                        reader.exact(&marker)?;
                        let indent = " ".repeat(marker.len() + 1);
                        let (decoded, start, end) = reader.text_block(&indent)?;
                        if decoded != item.text {
                            return Err(content_error());
                        }
                        content_mappings.push(mapping(
                            section,
                            block_id,
                            "list_item",
                            Some(&item.item_id),
                            &item.source_object_ids,
                            start,
                            end,
                        ));
                        read_links(
                            &mut reader,
                            section,
                            block_id,
                            Some(&item.item_id),
                            &item.links,
                            &indent,
                            &mut links,
                        )?;
                    }
                }
                FlowBlock::Diagram { block_id } => {
                    let diagram = plan.diagram.as_ref().ok_or_else(content_error)?;
                    reader.exact("```mermaid")?;
                    reader.exact("flowchart TD")?;
                    for node in &diagram.nodes {
                        let line = reader.line()?;
                        read_node(line.text, node)?;
                        content_mappings.push(mapping(
                            section,
                            block_id,
                            "diagram_node",
                            Some(&node.node_id),
                            std::slice::from_ref(&node.step_id),
                            line.start,
                            line.end,
                        ));
                    }
                    for edge in &diagram.edges {
                        let line = reader.line()?;
                        read_edge(line.text, edge)?;
                        content_mappings.push(mapping(
                            section,
                            block_id,
                            "diagram_edge",
                            Some(&edge.edge_id),
                            &edge_sources(plan, edge),
                            line.start,
                            line.end,
                        ));
                    }
                    reader.exact("```")?;
                    reader.exact("")?;
                }
            }
        }
    }
    if reader.offset != text.len() {
        return Err(content_error());
    }
    Ok(FlowRenderedFile {
        relative_path: note.output_path.clone(),
        note_id: note.note_id.clone(),
        plan_revision: plan.revision,
        content_sha256: crate::text::sha256(bytes),
        byte_length: bytes.len() as u64,
        content_mappings,
        links,
    })
}

pub(crate) fn validate_rendered(
    plan: &FlowPlan,
    files: &[FlowRenderedFile],
    actual: &BTreeMap<String, Vec<u8>>,
) -> Result<(), ProjectError> {
    validate_plan(plan)?;
    let expected_paths: BTreeSet<_> = plan.notes.iter().map(|n| n.output_path.as_str()).collect();
    let file_paths: BTreeSet<_> = files.iter().map(|f| f.relative_path.as_str()).collect();
    let actual_paths: BTreeSet<_> = actual.keys().map(String::as_str).collect();
    if files.len() != plan.notes.len()
        || file_paths != expected_paths
        || actual_paths != expected_paths
    {
        return Err(error(
            "FLOW_OUTPUT_CONTENT",
            "流程笔记文件清单缺失、重复或含计划外文件。",
        ));
    }
    let mut total = 0usize;
    for note in &plan.notes {
        let bytes = &actual[&note.output_path];
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| error("FLOW_RENDER_LIMIT", "流程输出大小溢出。"))?;
        if bytes.len() > MAX_NOTE_BYTES || total > MAX_OUTPUT_BYTES {
            return Err(error("FLOW_RENDER_LIMIT", "流程输出超过字节上限。"));
        }
        let observed = read_note(plan, note, bytes)?;
        let declared = files
            .iter()
            .find(|f| f.relative_path == note.output_path)
            .ok_or_else(content_error)?;
        if observed.note_id != declared.note_id
            || observed.plan_revision != declared.plan_revision
            || observed.content_sha256 != declared.content_sha256
            || observed.byte_length != declared.byte_length
        {
            return Err(error(
                "FLOW_OUTPUT_CONTENT",
                "流程笔记身份、版本、长度或指纹与实际读取字节不一致。",
            ));
        }
        if observed.content_mappings != declared.content_mappings {
            return Err(error(
                "FLOW_OUTPUT_MAPPING",
                "流程内容映射与实际解析位置或来源对象不一致。",
            ));
        }
        if observed.links != declared.links {
            return Err(error(
                "FLOW_OUTPUT_LINK",
                "正文链接映射与实际解析位置或目标不一致。",
            ));
        }
        for link in &observed.links {
            if !plan.notes.iter().any(|target| {
                target.output_path == link.target_path
                    && target
                        .sections
                        .iter()
                        .any(|s| s.heading == link.target_heading)
            }) {
                return Err(error(
                    "FLOW_OUTPUT_LINK",
                    "正文链接缺少实际目标笔记或标题。",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        process_flow_plan::tests::{document, profile},
        process_model::ProcessGold,
    };

    fn plan(gold: &str) -> FlowPlan {
        process_flow_plan::build(
            &serde_json::from_str::<ProcessGold>(gold)
                .unwrap()
                .document(),
            &profile(),
            "test-request",
        )
        .unwrap()
    }
    fn captured(plan: &FlowPlan) -> (Vec<FlowRenderedFile>, BTreeMap<String, Vec<u8>>) {
        let notes = render(plan).unwrap();
        let files = notes.iter().map(|n| n.file.clone()).collect();
        let actual = notes
            .into_iter()
            .map(|n| (n.file.relative_path, n.bytes))
            .collect();
        (files, actual)
    }
    fn sync_fingerprint(files: &mut [FlowRenderedFile], actual: &BTreeMap<String, Vec<u8>>) {
        for file in files {
            file.content_sha256 = crate::text::sha256(&actual[&file.relative_path]);
            file.byte_length = actual[&file.relative_path].len() as u64;
        }
    }
    #[test]
    fn every_frozen_success_renders_and_independently_reads_back() {
        for gold in [
            include_str!("../../../fixtures/process-flow-v1/gold-linear.json"),
            include_str!("../../../fixtures/process-flow-v1/gold-branch.json"),
            include_str!("../../../fixtures/process-flow-v1/gold-causes.json"),
            include_str!("../../../fixtures/process-flow-v1/gold-promotes.json"),
            include_str!("../../../fixtures/process-flow-v1/gold-inhibits.json"),
            include_str!("../../../fixtures/process-flow-v1/gold-complex_labels.json"),
            include_str!("../../../fixtures/process-flow-v1/gold-multiline_labels.json"),
        ] {
            let plan = plan(gold);
            let (files, actual) = captured(&plan);
            validate_rendered(&plan, &files, &actual).unwrap();
            assert_eq!(files.len(), 2);
            assert!(
                files
                    .iter()
                    .flat_map(|f| &f.content_mappings)
                    .any(|m| m.source_object_ids.iter().any(|id| id == "process"))
            );
        }
        let mut p = profile();
        p.flowchart_support = FlowchartSupport::Unavailable;
        let plan = process_flow_plan::build(&document(), &p, "test-request").unwrap();
        let (files, actual) = captured(&plan);
        validate_rendered(&plan, &files, &actual).unwrap();
    }
    #[test]
    fn reversed_arrow_rejected_even_when_fingerprint_is_rewritten() {
        let plan = plan(include_str!(
            "../../../fixtures/process-flow-v1/gold-linear.json"
        ));
        let (mut files, mut actual) = captured(&plan);
        let source = String::from_utf8(actual["流程总览.md"].clone()).unwrap();
        assert!(source.contains("n0 -->|\"随后\"| n1"));
        actual.insert(
            "流程总览.md".into(),
            source
                .replace("n0 -->|\"随后\"| n1", "n1 -->|\"随后\"| n0")
                .into_bytes(),
        );
        sync_fingerprint(&mut files, &actual);
        assert!(validate_rendered(&plan, &files, &actual).is_err());
    }
    #[test]
    fn negative_sign_branch_criterion_and_exception_cannot_disappear() {
        for (gold, old, new) in [
            (
                include_str!("../../../fixtures/process-flow-v1/gold-inhibits.json"),
                "−：抑制",
                "抑制",
            ),
            (
                include_str!("../../../fixtures/process-flow-v1/gold-branch.json"),
                "判据为真：",
                "判据为假：",
            ),
            (
                include_str!("../../../fixtures/process-flow-v1/gold-linear.json"),
                "编号缺失时停止",
                "编号缺失时继续",
            ),
        ] {
            let plan = plan(gold);
            let (mut files, mut actual) = captured(&plan);
            let path = if old == "编号缺失时停止" {
                "过程明细.md"
            } else {
                "流程总览.md"
            };
            let source = String::from_utf8(actual[path].clone()).unwrap();
            assert!(source.contains(old));
            actual.insert(path.into(), source.replace(old, new).into_bytes());
            sync_fingerprint(&mut files, &actual);
            assert!(validate_rendered(&plan, &files, &actual).is_err(), "{old}");
        }
    }
    #[test]
    fn synchronized_mapping_and_plan_forgery_rejected_against_document() {
        let plan = plan(include_str!(
            "../../../fixtures/process-flow-v1/gold-linear.json"
        ));
        let (mut files, actual) = captured(&plan);
        let edge = files[0]
            .content_mappings
            .iter_mut()
            .find(|m| m.kind == "diagram_edge")
            .unwrap();
        edge.source_object_ids = vec!["r2".into()];
        edge.start_byte += 2;
        assert!(validate_rendered(&plan, &files, &actual).is_err());
        let mut forged = plan.clone();
        forged.diagram.as_mut().unwrap().edges[0].left_node_id = "n1".into();
        assert!(render(&forged).is_err());
        let mut forged = plan.clone();
        forged.published = true;
        assert!(render(&forged).is_err());
    }
    #[test]
    fn body_fence_delimiters_and_trailing_whitespace_preserve_literal_text() {
        let mut doc = document();
        doc.steps[0].description = "逐字保留\n```\n# 标题\n```mermaid\n\t制表内容\n``````\n".into();
        let plan = process_flow_plan::build(&doc, &profile(), "test-request").unwrap();
        let (files, actual) = captured(&plan);
        validate_rendered(&plan, &files, &actual).unwrap();
        let details = std::str::from_utf8(&actual["过程明细.md"]).unwrap();
        assert!(details.contains("```````text\n"));
        assert!(details.contains(&doc.steps[0].description));
    }
    #[test]
    fn punctuation_codec_is_one_pass_and_rejects_raw_syntax() {
        let original = "引号\"[α]|#35;<br>&+−\\`";
        assert_eq!(decode_label(&encode_label(original)).unwrap(), original);
        assert_eq!(decode_label("#35;35#59;").unwrap(), "#35;");
        for invalid in ["<br>", "#10;", "#9;", "#000;", "#1234;", "#35", "\"", "\n"] {
            assert!(decode_label(invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn extra_mermaid_instruction_and_broken_link_are_rejected() {
        let plan = plan(include_str!(
            "../../../fixtures/process-flow-v1/gold-linear.json"
        ));
        for (old, new) in [
            ("flowchart TD\n", "flowchart TD\nclick n0 evil\n"),
            ("过程明细#过程说明", "过程明细#不存在"),
        ] {
            let (mut files, mut actual) = captured(&plan);
            let source = String::from_utf8(actual["流程总览.md"].clone()).unwrap();
            assert!(source.contains(old));
            actual.insert("流程总览.md".into(), source.replace(old, new).into_bytes());
            sync_fingerprint(&mut files, &actual);
            assert!(validate_rendered(&plan, &files, &actual).is_err());
        }
    }
    #[test]
    fn missing_nullable_fields_do_not_deserialize_as_implicit_null() {
        let plan = plan(include_str!(
            "../../../fixtures/process-flow-v1/gold-linear.json"
        ));
        let mut value = serde_json::to_value(&plan).unwrap();
        value.as_object_mut().unwrap().remove("diagram");
        assert!(serde_json::from_value::<FlowPlan>(value).is_err());
        let (files, _) = captured(&plan);
        let mut file = serde_json::to_value(&files[0]).unwrap();
        file["content_mappings"][0]
            .as_object_mut()
            .unwrap()
            .remove("item_id");
        assert!(serde_json::from_value::<FlowRenderedFile>(file).is_err());
    }
}
