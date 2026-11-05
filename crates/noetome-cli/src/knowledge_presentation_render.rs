//! Finite Markdown rendering and independent semantic read-back for active plans.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    fixture_input::valid_relative_path, fixture_plan::plain_text, knowledge_presentation_model::*,
    project::ProjectError,
};

const MAX_NOTE_BYTES: usize = 8 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct RenderedNote {
    pub(crate) file: RenderedFile,
    pub(crate) bytes: Vec<u8>,
}

fn problem(code: &'static str, message: impl Into<String>) -> ProjectError {
    ProjectError::new(code, 3, message)
}

fn locator(
    note: &Note,
    node: &str,
    kind: &str,
    row: Option<&str>,
    column: Option<&str>,
) -> ScopeLocator {
    ScopeLocator {
        section_id: note.section_id.clone(),
        node_id: node.into(),
        kind: kind.into(),
        row_id: row.map(str::to_owned),
        column_id: column.map(str::to_owned),
    }
}

struct Scope<'a> {
    locator: ScopeLocator,
    text: &'a str,
    facts: &'a [String],
    occurrences: &'a [String],
    concepts: &'a [String],
    links: &'a [SemanticLink],
}

impl Scope<'_> {
    fn mapping(&self, start: usize, end: usize) -> ContentMapping {
        ContentMapping {
            scope: self.locator.clone(),
            fact_ids: self.facts.into(),
            occurrence_ids: self.occurrences.into(),
            concept_ids: self.concepts.into(),
            start_byte: start as u64,
            end_byte: end as u64,
        }
    }
}

fn header_scope<'a>(note: &Note, node: &str, column: &'a Column) -> Scope<'a> {
    Scope {
        locator: locator(note, node, "table_header", None, Some(&column.column_id)),
        text: &column.label,
        facts: &[],
        occurrences: &column.occurrence_ids,
        concepts: &column.concept_ids,
        links: &[],
    }
}

fn cell_scope<'a>(note: &Note, node: &str, row: &Row, cell: &'a Cell) -> Scope<'a> {
    Scope {
        locator: locator(
            note,
            node,
            match cell.kind {
                CellKind::Label => "label",
                CellKind::Facts => "facts",
                CellKind::Missing => "missing",
            },
            Some(&row.row_id),
            Some(&cell.column_id),
        ),
        text: &cell.text,
        facts: &cell.fact_ids,
        occurrences: &cell.occurrence_ids,
        concepts: &cell.concept_ids,
        links: &cell.links,
    }
}

fn safe_path(value: &str) -> bool {
    valid_relative_path(value)
        && value.ends_with(".md")
        && !value.starts_with('.')
        && !value.contains('/')
        && !value.chars().any(|character| "#[]^".contains(character))
}

fn safe_heading(value: &str) -> bool {
    !value.is_empty()
        && plain_text(value)
        && !value
            .chars()
            .any(|character| character.is_control() || "#[]|\\".contains(character))
}

fn safe_display(value: &str) -> bool {
    !value.is_empty()
        && plain_text(value)
        && !value
            .chars()
            .any(|character| character.is_control() || "[]|".contains(character))
}

fn unique(values: &[String]) -> bool {
    let mut found = BTreeSet::new();
    values
        .iter()
        .all(|value| !value.is_empty() && found.insert(value))
}

fn validate_scope(
    scope: &Scope<'_>,
    facts: &BTreeMap<&str, &PresentationFact>,
    occurrences: &BTreeSet<&str>,
    concepts: &BTreeSet<&str>,
    destinations: &BTreeSet<(&str, &str)>,
) -> Result<(), ProjectError> {
    if scope.text.is_empty() || !plain_text(scope.text) {
        return Err(problem("PRESENT_RENDER_TEXT", "呈现正文须为非空有限明文。"));
    }
    if !unique(scope.facts)
        || !unique(scope.occurrences)
        || !unique(scope.concepts)
        || scope
            .facts
            .iter()
            .any(|id| !facts.contains_key(id.as_str()))
        || scope
            .occurrences
            .iter()
            .any(|id| !occurrences.contains(id.as_str()))
        || scope
            .concepts
            .iter()
            .any(|id| !concepts.contains(id.as_str()))
    {
        return Err(problem(
            "PRESENT_RENDER_REFS",
            "呈现scope含重复或不存在的事实、出现或概念引用。",
        ));
    }
    let mut fact_occurrences = BTreeSet::new();
    for id in scope.facts {
        let fact = facts[id.as_str()];
        fact_occurrences.extend(fact.occurrence_ids.iter().map(String::as_str));
        // Only the body's decoded text can satisfy these facts; link aliases
        // and neighbouring cells cannot supply missing values or context.
        for value in std::iter::once(fact.value.as_str())
            .chain(std::iter::once(fact.unit.as_str()))
            .chain(fact.conditions.iter().map(String::as_str))
            .chain(fact.exceptions.iter().map(String::as_str))
            .chain(
                fact.time
                    .iter()
                    .filter(|time| time.status == "known")
                    .filter_map(|time| time.value.as_deref()),
            )
        {
            if !value.is_empty() && !scope.text.contains(value) {
                return Err(problem(
                    "PRESENT_RENDER_FACT_CONTENT",
                    "事实正文缺少值、单位、完整条件、例外或已知时间。",
                ));
            }
        }
    }
    if !scope.facts.is_empty()
        && fact_occurrences != scope.occurrences.iter().map(String::as_str).collect()
    {
        return Err(problem(
            "PRESENT_RENDER_REFS",
            "事实正文的原出现集合与事实引用不一致。",
        ));
    }
    let mut links = BTreeSet::new();
    for link in scope.links {
        if !concepts.contains(link.target_concept_id.as_str())
            || !safe_path(&link.target_path)
            || !safe_heading(&link.target_heading)
            || !safe_display(&link.display_text)
            || link.target_path != format!("对象-{}.md", link.target_concept_id)
            || !destinations.contains(&(link.target_path.as_str(), link.target_heading.as_str()))
            || !links.insert((
                &link.target_concept_id,
                &link.target_path,
                &link.target_heading,
                &link.display_text,
            ))
        {
            return Err(problem(
                "PRESENT_RENDER_LINK",
                "结构链接须指向唯一实际笔记的安全地址与有效概念，并使用有限单行别名。",
            ));
        }
    }
    Ok(())
}

fn validate_plan(plan: &PresentationPlan) -> Result<(), ProjectError> {
    let mut paths = BTreeSet::new();
    let mut note_ids = BTreeSet::new();
    let mut section_ids = BTreeSet::new();
    let mut node_ids = BTreeSet::new();
    let mut destinations = BTreeSet::new();
    if plan.notes.is_empty() {
        return Err(problem("PRESENT_RENDER_PLAN", "呈现计划没有笔记。"));
    }
    for note in &plan.notes {
        if !safe_path(&note.output_path)
            || !paths.insert(note.output_path.to_lowercase())
            || note.note_id.is_empty()
            || !note_ids.insert(&note.note_id)
            || note.section_id.is_empty()
            || !section_ids.insert((&note.note_id, &note.section_id))
            || note.title.is_empty()
            || !plain_text(&note.title)
            || !safe_heading(&note.heading)
            || note.nodes.is_empty()
        {
            return Err(problem(
                "PRESENT_RENDER_PLAN",
                "笔记路径、标题、小节或标识不满足安全且唯一的呈现契约。",
            ));
        }
        destinations.insert((note.output_path.as_str(), note.heading.as_str()));
    }
    let mut concepts = BTreeSet::new();
    let mut objects = BTreeMap::new();
    let mut properties = BTreeMap::new();
    let occurrences: BTreeSet<_> = plan
        .provenance
        .iter()
        .map(|value| value.occurrence_id.as_str())
        .collect();
    if occurrences.len() != plan.provenance.len() || occurrences.contains("") {
        return Err(problem("PRESENT_RENDER_REFS", "原出现标识不唯一。"));
    }
    for (entities, destination) in [
        (&plan.objects, &mut objects),
        (&plan.properties, &mut properties),
    ] {
        for entity in entities {
            if entity.concept_id.is_empty()
                || destination.contains_key(entity.concept_id.as_str())
                || entity.label.is_empty()
                || !plain_text(&entity.label)
                || !unique(&entity.occurrence_ids)
                || entity
                    .occurrence_ids
                    .iter()
                    .any(|id| !occurrences.contains(id.as_str()))
            {
                return Err(problem("PRESENT_RENDER_REFS", "概念标签或原出现引用无效。"));
            }
            concepts.insert(entity.concept_id.as_str());
            destination.insert(entity.concept_id.as_str(), entity);
        }
    }
    for note in &plan.notes {
        if note.output_path != "知识比较.md"
            && !objects
                .keys()
                .any(|id| note.output_path == format!("对象-{id}.md"))
        {
            return Err(problem(
                "PRESENT_RENDER_PLAN",
                "呈现笔记须使用固定比较文件名或稳定对象标识文件名。",
            ));
        }
    }
    let mut facts = BTreeMap::new();
    for fact in &plan.facts {
        if fact.fact_id.is_empty()
            || facts.insert(fact.fact_id.as_str(), fact).is_some()
            || !objects.contains_key(fact.object_id.as_str())
            || !properties.contains_key(fact.property_id.as_str())
            || fact.value_status != "known"
            || fact.value.is_empty()
            || !std::iter::once(fact.value.as_str())
                .chain(std::iter::once(fact.unit.as_str()))
                .chain(fact.conditions.iter().map(String::as_str))
                .chain(fact.exceptions.iter().map(String::as_str))
                .all(plain_text)
            || fact
                .time
                .as_ref()
                .is_some_and(|time| match time.status.as_str() {
                    "known" => {
                        time.value
                            .as_deref()
                            .is_none_or(|value| value.is_empty() || !plain_text(value))
                            || !matches!(time.precision.as_deref(), Some("year" | "day"))
                    }
                    "unknown" => time.value.is_some() || time.precision.is_some(),
                    _ => true,
                })
            || !unique(&fact.occurrence_ids)
            || fact.occurrence_ids.is_empty()
            || fact
                .occurrence_ids
                .iter()
                .any(|id| !occurrences.contains(id.as_str()))
        {
            return Err(problem(
                "PRESENT_RENDER_REFS",
                "事实标识、对象属性或原出现引用无效。",
            ));
        }
    }
    let mut covered_facts = BTreeSet::new();
    for note in &plan.notes {
        for node in &note.nodes {
            let node_id = match node {
                Node::Table { node_id, .. } | Node::Paragraph { node_id, .. } => node_id,
            };
            if node_id.is_empty() || !node_ids.insert(node_id) {
                return Err(problem("PRESENT_RENDER_PLAN", "呈现节点标识为空或重复。"));
            }
            match node {
                Node::Paragraph {
                    text,
                    fact_ids,
                    occurrence_ids,
                    concept_ids,
                    links,
                    ..
                } => {
                    let scope = Scope {
                        locator: locator(note, node_id, "paragraph", None, None),
                        text,
                        facts: fact_ids,
                        occurrences: occurrence_ids,
                        concepts: concept_ids,
                        links,
                    };
                    validate_scope(&scope, &facts, &occurrences, &concepts, &destinations)?;
                    covered_facts.extend(fact_ids.iter().map(String::as_str));
                }
                Node::Table { columns, rows, .. } => {
                    let mut column_ids = BTreeSet::new();
                    let mut property_ids = BTreeSet::new();
                    if columns.len() < 2 || rows.is_empty() || columns[0].property_id.is_some() {
                        return Err(problem(
                            "PRESENT_RENDER_TABLE",
                            "表格须有对象标签列和至少一个属性列、数据行。",
                        ));
                    }
                    for (index, column) in columns.iter().enumerate() {
                        if column.column_id.is_empty()
                            || !column_ids.insert(&column.column_id)
                            || (index > 0
                                && column.property_id.as_deref().is_none_or(|id| {
                                    !properties.contains_key(id) || !property_ids.insert(id)
                                }))
                        {
                            return Err(problem("PRESENT_RENDER_TABLE", "表格列标识或属性无效。"));
                        }
                        validate_scope(
                            &header_scope(note, node_id, column),
                            &facts,
                            &occurrences,
                            &concepts,
                            &destinations,
                        )?;
                    }
                    let mut row_ids = BTreeSet::new();
                    let mut row_objects = BTreeSet::new();
                    for row in rows {
                        if row.row_id.is_empty()
                            || !row_ids.insert(&row.row_id)
                            || !row_objects.insert(&row.object_id)
                            || !objects.contains_key(row.object_id.as_str())
                            || row.cells.len() != columns.len()
                        {
                            return Err(problem(
                                "PRESENT_RENDER_TABLE",
                                "表格行标识、对象或单元格数量无效。",
                            ));
                        }
                        for (index, (column, cell)) in columns.iter().zip(&row.cells).enumerate() {
                            let pair_facts: BTreeSet<_> = plan
                                .facts
                                .iter()
                                .filter(|fact| {
                                    fact.object_id == row.object_id
                                        && column.property_id.as_deref()
                                            == Some(fact.property_id.as_str())
                                })
                                .map(|fact| fact.fact_id.as_str())
                                .collect();
                            if cell.column_id != column.column_id
                                || (index == 0
                                    && (cell.kind != CellKind::Label
                                        || !cell.fact_ids.is_empty()
                                        || cell.text != objects[row.object_id.as_str()].label))
                                || (index > 0 && cell.kind == CellKind::Label)
                                || (cell.kind == CellKind::Facts
                                    && (cell.fact_ids.is_empty()
                                        || pair_facts
                                            != cell.fact_ids.iter().map(String::as_str).collect()))
                                || (cell.kind == CellKind::Missing
                                    && (!pair_facts.is_empty()
                                        || cell.text != "选定事实未给出"
                                        || !cell.fact_ids.is_empty()
                                        || !cell.occurrence_ids.is_empty()
                                        || !cell.links.is_empty()))
                            {
                                return Err(problem(
                                    "PRESENT_RENDER_TABLE",
                                    "表格单元格与对象、属性、完整事实或缺值语义不一致。",
                                ));
                            }
                            validate_scope(
                                &cell_scope(note, node_id, row, cell),
                                &facts,
                                &occurrences,
                                &concepts,
                                &destinations,
                            )?;
                            covered_facts.extend(cell.fact_ids.iter().map(String::as_str));
                        }
                    }
                }
            }
        }
    }
    if covered_facts != facts.keys().copied().collect() {
        return Err(problem(
            "PRESENT_RENDER_REFS",
            "计划没有在实际正文scope中覆盖全部选定事实。",
        ));
    }
    Ok(())
}

fn encode_text(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '\n' => output.push_str("<br>"),
            '\t' => output.push_str("&#9;"),
            punctuation if punctuation.is_ascii_punctuation() => {
                output.push('\\');
                output.push(punctuation);
            }
            _ => output.push(character),
        }
    }
}

fn emit_scope(
    output: &mut String,
    scope: &Scope<'_>,
    table: bool,
    contents: &mut Vec<ContentMapping>,
    links: &mut Vec<LinkMapping>,
) {
    let start = output.len();
    encode_text(scope.text, output);
    contents.push(scope.mapping(start, output.len()));
    for link in scope.links {
        output.push(' ');
        let start = output.len();
        output.push_str("[[");
        output.push_str(
            link.target_path
                .strip_suffix(".md")
                .expect("validated Markdown path"),
        );
        output.push('#');
        output.push_str(&link.target_heading);
        output.push_str(if table { "\\|" } else { "|" });
        encode_text(&link.display_text, output);
        output.push_str("]]");
        links.push(LinkMapping {
            scope: scope.locator.clone(),
            target_concept_id: link.target_concept_id.clone(),
            target_path: link.target_path.clone(),
            target_heading: link.target_heading.clone(),
            display_text: link.display_text.clone(),
            start_byte: start as u64,
            end_byte: output.len() as u64,
        });
    }
}

pub(crate) fn render(plan: &PresentationPlan) -> Result<Vec<RenderedNote>, ProjectError> {
    validate_plan(plan)?;
    let mut result = Vec::new();
    let mut total = 0usize;
    for note in &plan.notes {
        let mut output = String::from("# ");
        // Titles retain the same finite plain text as canonical display names;
        // newlines/tabs are encoded within this heading, never as structure.
        encode_text(&note.title, &mut output);
        output.push_str("\n\n## ");
        encode_text(&note.heading, &mut output);
        output.push_str("\n\n");
        let mut contents = Vec::new();
        let mut links = Vec::new();
        for node in &note.nodes {
            match node {
                Node::Paragraph {
                    node_id,
                    text,
                    fact_ids,
                    occurrence_ids,
                    concept_ids,
                    links: planned_links,
                } => {
                    emit_scope(
                        &mut output,
                        &Scope {
                            locator: locator(note, node_id, "paragraph", None, None),
                            text,
                            facts: fact_ids,
                            occurrences: occurrence_ids,
                            concepts: concept_ids,
                            links: planned_links,
                        },
                        false,
                        &mut contents,
                        &mut links,
                    );
                    output.push_str("\n\n");
                }
                Node::Table {
                    node_id,
                    columns,
                    rows,
                } => {
                    output.push('|');
                    for column in columns {
                        output.push(' ');
                        emit_scope(
                            &mut output,
                            &header_scope(note, node_id, column),
                            true,
                            &mut contents,
                            &mut links,
                        );
                        output.push_str(" |");
                    }
                    output.push('\n');
                    output.push('|');
                    for _ in columns {
                        output.push_str(" --- |");
                    }
                    output.push('\n');
                    for row in rows {
                        output.push('|');
                        for cell in &row.cells {
                            output.push(' ');
                            emit_scope(
                                &mut output,
                                &cell_scope(note, node_id, row, cell),
                                true,
                                &mut contents,
                                &mut links,
                            );
                            output.push_str(" |");
                        }
                        output.push('\n');
                    }
                    output.push('\n');
                }
            }
            if output.len() > MAX_NOTE_BYTES {
                return Err(problem(
                    "PRESENT_RENDER_LIMIT",
                    "呈现笔记超过8 MiB限制，禁止截断正文。",
                ));
            }
        }
        total = total
            .checked_add(output.len())
            .ok_or_else(|| problem("PRESENT_RENDER_LIMIT", "呈现总字节数溢出。"))?;
        if total > MAX_OUTPUT_BYTES {
            return Err(problem("PRESENT_RENDER_LIMIT", "呈现输出超过64 MiB限制。"));
        }
        let bytes = output.into_bytes();
        result.push(RenderedNote {
            file: RenderedFile {
                relative_path: note.output_path.clone(),
                note_id: note.note_id.clone(),
                plan_revision: plan.revision,
                content_sha256: crate::text::sha256(&bytes),
                byte_length: bytes.len() as u64,
                content_mappings: contents,
                links,
            },
            bytes,
        });
    }
    Ok(result)
}

// The reader below decodes observed bytes. It deliberately never invokes the
// encoder or renderer to construct expected Markdown or expected byte ranges.
fn decode_text(raw: &str) -> Result<String, ProjectError> {
    let mut output = String::new();
    let mut offset = 0;
    while offset < raw.len() {
        let remaining = &raw[offset..];
        if remaining.starts_with("<br>") {
            output.push('\n');
            offset += 4;
            continue;
        }
        if remaining.starts_with("&#9;") {
            output.push('\t');
            offset += 4;
            continue;
        }
        let character = remaining.chars().next().expect("nonempty UTF-8 slice");
        offset += character.len_utf8();
        if character == '\\' {
            let escaped = raw[offset..]
                .chars()
                .next()
                .filter(char::is_ascii_punctuation)
                .ok_or_else(|| {
                    problem("PRESENT_OUTPUT_CONTENT", "Markdown明文含非法或不完整转义。")
                })?;
            output.push(escaped);
            offset += escaped.len_utf8();
        } else if character.is_ascii_punctuation() || character.is_control() {
            return Err(problem(
                "PRESENT_OUTPUT_CONTENT",
                "Markdown正文含未声明结构语法或控制字符。",
            ));
        } else {
            output.push(character);
        }
    }
    Ok(output)
}

fn wiki_delimiter(raw: &str, delimiter: &str) -> Option<usize> {
    let mut position = 0;
    while position < raw.len() {
        let character = raw[position..].chars().next()?;
        if character == '\\' {
            position += 1;
            position += raw[position..].chars().next()?.len_utf8();
        } else if raw[position..].starts_with(delimiter) {
            return Some(position);
        } else {
            position += character.len_utf8();
        }
    }
    None
}

fn parse_link(
    raw: &str,
    absolute: usize,
    scope: &Scope<'_>,
    expected: &SemanticLink,
    table: bool,
) -> Result<LinkMapping, ProjectError> {
    let inside = raw
        .strip_prefix("[[")
        .and_then(|value| value.strip_suffix("]]"))
        .ok_or_else(|| problem("PRESENT_OUTPUT_LINK", "结构链接缺少边界。"))?;
    let (address, alias) = inside
        .split_once(if table { "\\|" } else { "|" })
        .ok_or_else(|| problem("PRESENT_OUTPUT_LINK", "结构链接缺少受控别名分隔符。"))?;
    let (stem, heading) = address
        .split_once('#')
        .ok_or_else(|| problem("PRESENT_OUTPUT_LINK", "结构链接缺少小节目标。"))?;
    let path = format!("{stem}.md");
    let display = decode_text(alias)?;
    if !safe_path(&path)
        || !safe_heading(heading)
        || !safe_display(&display)
        || path != expected.target_path
        || heading != expected.target_heading
        || display != expected.display_text
    {
        return Err(problem(
            "PRESENT_OUTPUT_LINK",
            "实际链接地址或别名与独立计划语义不一致。",
        ));
    }
    Ok(LinkMapping {
        scope: scope.locator.clone(),
        target_concept_id: expected.target_concept_id.clone(),
        target_path: path,
        target_heading: heading.into(),
        display_text: display,
        start_byte: absolute as u64,
        end_byte: (absolute + raw.len()) as u64,
    })
}

fn parse_scope(
    raw: &str,
    absolute: usize,
    scope: &Scope<'_>,
    table: bool,
    contents: &mut Vec<ContentMapping>,
    links: &mut Vec<LinkMapping>,
) -> Result<(), ProjectError> {
    let body_end = match wiki_delimiter(raw, "[[") {
        Some(start) if start > 0 && raw.as_bytes()[start - 1] == b' ' => start - 1,
        Some(_) => {
            return Err(problem(
                "PRESENT_OUTPUT_LINK",
                "链接须与正文用一个空格分隔。",
            ));
        }
        None => raw.len(),
    };
    let text = decode_text(&raw[..body_end])?;
    if text != scope.text {
        return Err(problem(
            "PRESENT_OUTPUT_CONTENT",
            "实际正文与所属scope的完整原文不一致。",
        ));
    }
    contents.push(scope.mapping(absolute, absolute + body_end));
    let mut position = body_end;
    for expected in scope.links {
        if !raw[position..].starts_with(" [[") {
            return Err(problem("PRESENT_OUTPUT_LINK", "实际scope缺少计划链接。"));
        }
        position += 1;
        let end = wiki_delimiter(&raw[position + 2..], "]]")
            .map(|end| position + 2 + end + 2)
            .ok_or_else(|| problem("PRESENT_OUTPUT_LINK", "结构链接没有结束边界。"))?;
        links.push(parse_link(
            &raw[position..end],
            absolute + position,
            scope,
            expected,
            table,
        )?);
        position = end;
    }
    if position != raw.len() {
        return Err(problem(
            "PRESENT_OUTPUT_LINK",
            "实际scope存在额外链接或未映射内容。",
        ));
    }
    Ok(())
}

struct Cursor<'a> {
    text: &'a str,
    position: usize,
}

impl<'a> Cursor<'a> {
    fn line(&mut self) -> Result<(usize, &'a str), ProjectError> {
        let start = self.position;
        let length = self.text[start..]
            .find('\n')
            .ok_or_else(|| problem("PRESENT_OUTPUT_STRUCTURE", "Markdown结构行缺少结束换行。"))?;
        self.position += length + 1;
        Ok((start, &self.text[start..start + length]))
    }

    fn blank(&mut self) -> Result<(), ProjectError> {
        if self.line()?.1.is_empty() {
            Ok(())
        } else {
            Err(problem(
                "PRESENT_OUTPUT_STRUCTURE",
                "Markdown结构缺少受控空行。",
            ))
        }
    }

    fn heading(&mut self, marker: &str, expected: &str) -> Result<(), ProjectError> {
        let (_, line) = self.line()?;
        let raw = line
            .strip_prefix(marker)
            .ok_or_else(|| problem("PRESENT_OUTPUT_STRUCTURE", "笔记标题或小节级别不正确。"))?;
        if decode_text(raw)? != expected {
            return Err(problem(
                "PRESENT_OUTPUT_STRUCTURE",
                "实际标题或小节与计划不一致。",
            ));
        }
        self.blank()
    }
}

fn table_cells(line: &str, absolute: usize) -> Result<Vec<(usize, &str)>, ProjectError> {
    let mut delimiters = Vec::new();
    let mut position = 0;
    while position < line.len() {
        let character = line[position..]
            .chars()
            .next()
            .expect("UTF-8 character boundary");
        if character == '\\' {
            position += 1;
            position += line[position..]
                .chars()
                .next()
                .ok_or_else(|| problem("PRESENT_OUTPUT_STRUCTURE", "表格行包含不完整转义。"))?
                .len_utf8();
        } else {
            if character == '|' {
                delimiters.push(position);
            }
            position += character.len_utf8();
        }
    }
    if delimiters.len() < 2
        || delimiters[0] != 0
        || delimiters.last().copied() != Some(line.len() - 1)
    {
        return Err(problem(
            "PRESENT_OUTPUT_STRUCTURE",
            "表格缺少受控首尾pipe。",
        ));
    }
    delimiters
        .windows(2)
        .map(|pair| {
            let start = pair[0] + 1;
            let raw = &line[start..pair[1]];
            let value = raw
                .strip_prefix(' ')
                .and_then(|raw| raw.strip_suffix(' '))
                .ok_or_else(|| {
                    problem("PRESENT_OUTPUT_STRUCTURE", "表格单元格缺少受控空格边界。")
                })?;
            Ok((absolute + start + 1, value))
        })
        .collect()
}

fn parse_note(
    note: &Note,
    text: &str,
) -> Result<(Vec<ContentMapping>, Vec<LinkMapping>), ProjectError> {
    let mut cursor = Cursor { text, position: 0 };
    let mut contents = Vec::new();
    let mut links = Vec::new();
    cursor.heading("# ", &note.title)?;
    cursor.heading("## ", &note.heading)?;
    for node in &note.nodes {
        match node {
            Node::Paragraph {
                node_id,
                text,
                fact_ids,
                occurrence_ids,
                concept_ids,
                links: expected,
            } => {
                let (absolute, raw) = cursor.line()?;
                parse_scope(
                    raw,
                    absolute,
                    &Scope {
                        locator: locator(note, node_id, "paragraph", None, None),
                        text,
                        facts: fact_ids,
                        occurrences: occurrence_ids,
                        concepts: concept_ids,
                        links: expected,
                    },
                    false,
                    &mut contents,
                    &mut links,
                )?;
                cursor.blank()?;
            }
            Node::Table {
                node_id,
                columns,
                rows,
            } => {
                let (absolute, raw) = cursor.line()?;
                let header = table_cells(raw, absolute)?;
                if header.len() != columns.len() {
                    return Err(problem(
                        "PRESENT_OUTPUT_STRUCTURE",
                        "实际表头列数与计划不一致。",
                    ));
                }
                for ((start, raw), column) in header.into_iter().zip(columns) {
                    parse_scope(
                        raw,
                        start,
                        &header_scope(note, node_id, column),
                        true,
                        &mut contents,
                        &mut links,
                    )?;
                }
                let (absolute, raw) = cursor.line()?;
                let separators = table_cells(raw, absolute)?;
                if separators.len() != columns.len()
                    || separators.iter().any(|(_, raw)| *raw != "---")
                {
                    return Err(problem(
                        "PRESENT_OUTPUT_STRUCTURE",
                        "实际表格分隔行不符合受控列结构。",
                    ));
                }
                for row in rows {
                    let (absolute, raw) = cursor.line()?;
                    let cells = table_cells(raw, absolute)?;
                    if cells.len() != columns.len() {
                        return Err(problem(
                            "PRESENT_OUTPUT_STRUCTURE",
                            "实际表格行缺少、增加或错位单元格。",
                        ));
                    }
                    for ((start, raw), cell) in cells.into_iter().zip(&row.cells) {
                        parse_scope(
                            raw,
                            start,
                            &cell_scope(note, node_id, row, cell),
                            true,
                            &mut contents,
                            &mut links,
                        )?;
                    }
                }
                cursor.blank()?;
            }
        }
    }
    if cursor.position != text.len() {
        return Err(problem(
            "PRESENT_OUTPUT_STRUCTURE",
            "实际笔记有计划之外的行、列或正文。",
        ));
    }
    Ok((contents, links))
}

fn same_scope(left: &ScopeLocator, right: &ScopeLocator) -> bool {
    left.section_id == right.section_id
        && left.node_id == right.node_id
        && left.kind == right.kind
        && left.row_id == right.row_id
        && left.column_id == right.column_id
}

fn valid_range(text: &str, start: u64, end: u64) -> bool {
    usize::try_from(start)
        .ok()
        .zip(usize::try_from(end).ok())
        .is_some_and(|(start, end)| {
            start < end
                && end <= text.len()
                && text.is_char_boundary(start)
                && text.is_char_boundary(end)
        })
}

pub(crate) fn validate_rendered(
    plan: &PresentationPlan,
    files: &[RenderedFile],
    actual: &BTreeMap<String, Vec<u8>>,
) -> Result<(), ProjectError> {
    validate_plan(plan)?;
    if files.len() != plan.notes.len() || actual.len() != plan.notes.len() {
        return Err(problem(
            "PRESENT_OUTPUT_FILES",
            "实际文件或mapping文件数量与计划不一致。",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    for note in &plan.notes {
        let mut candidates = files
            .iter()
            .filter(|file| file.relative_path == note.output_path);
        let file = candidates
            .next()
            .ok_or_else(|| problem("PRESENT_OUTPUT_FILES", "实际mapping缺少计划笔记。"))?;
        if candidates.next().is_some()
            || !seen.insert(file.relative_path.to_lowercase())
            || file.note_id != note.note_id
            || file.plan_revision != plan.revision
        {
            return Err(problem(
                "PRESENT_OUTPUT_FILES",
                "实际mapping笔记身份、版本或路径不一致。",
            ));
        }
        let bytes = actual
            .get(&file.relative_path)
            .ok_or_else(|| problem("PRESENT_OUTPUT_FILES", "实际输出缺少计划笔记文件。"))?;
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| problem("PRESENT_OUTPUT_FILES", "实际总字节数溢出。"))?;
        if bytes.len() > MAX_NOTE_BYTES
            || total > MAX_OUTPUT_BYTES
            || file.byte_length != bytes.len() as u64
            || file.content_sha256 != crate::text::sha256(bytes)
        {
            return Err(problem(
                "PRESENT_OUTPUT_FINGERPRINT",
                "实际文件超限或哈希、字节数不一致。",
            ));
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| problem("PRESENT_OUTPUT_CONTENT", "实际笔记不是完整UTF-8。"))?;
        let (contents, links) = parse_note(note, text)?;
        if contents.len() != file.content_mappings.len() || links.len() != file.links.len() {
            return Err(problem(
                "PRESENT_OUTPUT_MAPPING",
                "实际scope或结构链接未被完整映射。",
            ));
        }
        for (observed, declared) in contents.iter().zip(&file.content_mappings) {
            if !valid_range(text, declared.start_byte, declared.end_byte)
                || !same_scope(&observed.scope, &declared.scope)
                || observed.fact_ids != declared.fact_ids
                || observed.occurrence_ids != declared.occurrence_ids
                || observed.concept_ids != declared.concept_ids
                || observed.start_byte != declared.start_byte
                || observed.end_byte != declared.end_byte
            {
                return Err(problem(
                    "PRESENT_OUTPUT_MAPPING",
                    "正文半开UTF-8范围、语义位置或完整引用与实际解析不一致。",
                ));
            }
        }
        for (observed, declared) in links.iter().zip(&file.links) {
            if !valid_range(text, declared.start_byte, declared.end_byte)
                || !same_scope(&observed.scope, &declared.scope)
                || observed.target_concept_id != declared.target_concept_id
                || observed.target_path != declared.target_path
                || observed.target_heading != declared.target_heading
                || observed.display_text != declared.display_text
                || observed.start_byte != declared.start_byte
                || observed.end_byte != declared.end_byte
            {
                return Err(problem(
                    "PRESENT_OUTPUT_MAPPING",
                    "链接半开UTF-8范围、目标或语义位置与实际解析不一致。",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{knowledge_coordination_v2_model::Time, sidecar_model::Fingerprint};

    const OBJECT: &str = "60000000-0000-4000-8000-000000000005";
    const PROPERTY: &str = "60000000-0000-4000-8000-000000000007";
    const DETAIL_PATH: &str = "对象-60000000-0000-4000-8000-000000000005.md";
    // Authored literal bytes, independent of both the encoder and parser.
    const TABLE: &str = "# 比较 \\[α\\]\n\n## 知识明细\n\n| 对象 | 速率 \\| \\[1\\] |\n| --- | --- |\n| 甲 \\[α\\] [[对象-60000000-0000-4000-8000-000000000005#知识明细\\|详情]] | 选定事实未给出 |\n\n";
    const DETAIL: &str =
        "# 甲 \\[α\\]\n\n## 知识明细\n\n甲 \\[α\\]<br>条件 x \\< 2&#9;单位 m\\/s\n\n";

    fn sample() -> PresentationPlan {
        PresentationPlan {
            schema_version: 1,
            producer: "knowledge_presentation".into(),
            scope: "active_presentation_v1".into(),
            representation_origin: "deterministic_planner".into(),
            revision: 1,
            request_id: "request".into(),
            project_id: "project".into(),
            coordination_schema_version: 3,
            coordination_ir_fingerprint: Fingerprint {
                sha256: "0".repeat(64),
                byte_length: 1,
            },
            knowledge_status: "pending".into(),
            published: false,
            scientific_quality: "not_run".into(),
            research_status: "not_requested".into(),
            representation: Representation::Table,
            decision: Decision {
                rule_id: "table".into(),
                reason: "codec specimen".into(),
                shared_property_ids: vec![PROPERTY.into()],
            },
            objects: vec![EntityLabel {
                concept_id: OBJECT.into(),
                label: "甲 [α]".into(),
                occurrence_ids: vec![],
            }],
            properties: vec![EntityLabel {
                concept_id: PROPERTY.into(),
                label: "速率 | [1]".into(),
                occurrence_ids: vec![],
            }],
            facts: vec![],
            provenance: vec![],
            notes: vec![
                Note {
                    note_id: "comparison".into(),
                    title: "比较 [α]".into(),
                    output_path: "知识比较.md".into(),
                    section_id: "comparison-section".into(),
                    heading: "知识明细".into(),
                    nodes: vec![Node::Table {
                        node_id: "table".into(),
                        columns: vec![
                            Column {
                                column_id: "object".into(),
                                property_id: None,
                                label: "对象".into(),
                                concept_ids: vec![],
                                occurrence_ids: vec![],
                            },
                            Column {
                                column_id: "property".into(),
                                property_id: Some(PROPERTY.into()),
                                label: "速率 | [1]".into(),
                                concept_ids: vec![PROPERTY.into()],
                                occurrence_ids: vec![],
                            },
                        ],
                        rows: vec![Row {
                            row_id: "row".into(),
                            object_id: OBJECT.into(),
                            cells: vec![
                                Cell {
                                    column_id: "object".into(),
                                    kind: CellKind::Label,
                                    text: "甲 [α]".into(),
                                    fact_ids: vec![],
                                    occurrence_ids: vec![],
                                    concept_ids: vec![OBJECT.into()],
                                    links: vec![SemanticLink {
                                        target_concept_id: OBJECT.into(),
                                        target_path: DETAIL_PATH.into(),
                                        target_heading: "知识明细".into(),
                                        display_text: "详情".into(),
                                    }],
                                },
                                Cell {
                                    column_id: "property".into(),
                                    kind: CellKind::Missing,
                                    text: "选定事实未给出".into(),
                                    fact_ids: vec![],
                                    occurrence_ids: vec![],
                                    concept_ids: vec![],
                                    links: vec![],
                                },
                            ],
                        }],
                    }],
                },
                Note {
                    note_id: "detail".into(),
                    title: "甲 [α]".into(),
                    output_path: DETAIL_PATH.into(),
                    section_id: "detail-section".into(),
                    heading: "知识明细".into(),
                    nodes: vec![Node::Paragraph {
                        node_id: "detail-paragraph".into(),
                        text: "甲 [α]\n条件 x < 2\t单位 m/s".into(),
                        fact_ids: vec![],
                        occurrence_ids: vec![],
                        concept_ids: vec![OBJECT.into()],
                        links: vec![],
                    }],
                },
            ],
        }
    }

    fn span(text: &str, snippet: &str) -> (u64, u64) {
        let start = text.find(snippet).expect("literal snippet");
        (start as u64, (start + snippet.len()) as u64)
    }

    fn expected_scope(
        section: &str,
        node: &str,
        kind: &str,
        row: Option<&str>,
        column: Option<&str>,
    ) -> ScopeLocator {
        ScopeLocator {
            section_id: section.into(),
            node_id: node.into(),
            kind: kind.into(),
            row_id: row.map(str::to_owned),
            column_id: column.map(str::to_owned),
        }
    }

    fn expected_content(
        scope: ScopeLocator,
        text: &str,
        snippet: &str,
        concepts: &[&str],
    ) -> ContentMapping {
        let (start_byte, end_byte) = span(text, snippet);
        ContentMapping {
            scope,
            fact_ids: vec![],
            occurrence_ids: vec![],
            concept_ids: concepts.iter().map(|id| (*id).into()).collect(),
            start_byte,
            end_byte,
        }
    }

    fn literal_files() -> (Vec<RenderedFile>, BTreeMap<String, Vec<u8>>) {
        let (start_byte, end_byte) = span(
            TABLE,
            "[[对象-60000000-0000-4000-8000-000000000005#知识明细\\|详情]]",
        );
        let files = vec![
            RenderedFile {
                relative_path: "知识比较.md".into(),
                note_id: "comparison".into(),
                plan_revision: 1,
                content_sha256: crate::text::sha256(TABLE.as_bytes()),
                byte_length: TABLE.len() as u64,
                content_mappings: vec![
                    expected_content(
                        expected_scope(
                            "comparison-section",
                            "table",
                            "table_header",
                            None,
                            Some("object"),
                        ),
                        TABLE,
                        "对象",
                        &[],
                    ),
                    expected_content(
                        expected_scope(
                            "comparison-section",
                            "table",
                            "table_header",
                            None,
                            Some("property"),
                        ),
                        TABLE,
                        "速率 \\| \\[1\\]",
                        &[PROPERTY],
                    ),
                    expected_content(
                        expected_scope(
                            "comparison-section",
                            "table",
                            "label",
                            Some("row"),
                            Some("object"),
                        ),
                        TABLE,
                        "甲 \\[α\\]",
                        &[OBJECT],
                    ),
                    expected_content(
                        expected_scope(
                            "comparison-section",
                            "table",
                            "missing",
                            Some("row"),
                            Some("property"),
                        ),
                        TABLE,
                        "选定事实未给出",
                        &[],
                    ),
                ],
                links: vec![LinkMapping {
                    scope: expected_scope(
                        "comparison-section",
                        "table",
                        "label",
                        Some("row"),
                        Some("object"),
                    ),
                    target_concept_id: OBJECT.into(),
                    target_path: DETAIL_PATH.into(),
                    target_heading: "知识明细".into(),
                    display_text: "详情".into(),
                    start_byte,
                    end_byte,
                }],
            },
            RenderedFile {
                relative_path: DETAIL_PATH.into(),
                note_id: "detail".into(),
                plan_revision: 1,
                content_sha256: crate::text::sha256(DETAIL.as_bytes()),
                byte_length: DETAIL.len() as u64,
                content_mappings: vec![expected_content(
                    expected_scope(
                        "detail-section",
                        "detail-paragraph",
                        "paragraph",
                        None,
                        None,
                    ),
                    DETAIL,
                    "甲 \\[α\\]<br>条件 x \\< 2&#9;单位 m\\/s",
                    &[OBJECT],
                )],
                links: vec![],
            },
        ];
        (
            files,
            BTreeMap::from([
                ("知识比较.md".into(), TABLE.as_bytes().to_vec()),
                (DETAIL_PATH.into(), DETAIL.as_bytes().to_vec()),
            ]),
        )
    }

    #[test]
    fn independently_authored_table_and_paragraph_bytes_have_exact_scopes() {
        let plan = sample();
        let (files, actual) = literal_files();
        validate_rendered(&plan, &files, &actual).unwrap();
        let output = render(&plan).unwrap();
        assert_eq!(output[0].bytes, TABLE.as_bytes());
        assert_eq!(output[1].bytes, DETAIL.as_bytes());
        for (observed, authored) in output.iter().zip(files) {
            assert_eq!(
                serde_json::to_value(&observed.file).unwrap(),
                serde_json::to_value(authored).unwrap()
            );
        }
    }

    #[test]
    fn body_drift_is_rejected_after_rehashing_output() {
        let (mut files, mut actual) = literal_files();
        let changed = TABLE.replace("选定事实未给出", "未知").into_bytes();
        files[0].byte_length = changed.len() as u64;
        files[0].content_sha256 = crate::text::sha256(&changed);
        actual.insert("知识比较.md".into(), changed);
        assert_eq!(
            validate_rendered(&sample(), &files, &actual)
                .unwrap_err()
                .code(),
            "PRESENT_OUTPUT_CONTENT"
        );
    }

    #[test]
    fn byte_ranges_and_scope_identity_are_checked_against_observed_structure() {
        let plan = sample();
        let (files, actual) = literal_files();
        let mut changes = Vec::new();
        let mut offset = files.clone();
        offset[0].content_mappings[2].start_byte += 1; // Inside the first Chinese UTF-8 character.
        changes.push(offset);
        let mut misplaced = files.clone();
        misplaced[0].content_mappings[2].scope.column_id = Some("property".into());
        changes.push(misplaced);
        let mut overlap = files.clone();
        overlap[0].content_mappings[2].end_byte = overlap[0].links[0].end_byte;
        changes.push(overlap);
        let mut lost_header = files.clone();
        lost_header[0].content_mappings.remove(0);
        changes.push(lost_header);
        let mut alias_range = files;
        alias_range[0].links[0].start_byte -= 1;
        changes.push(alias_range);
        for changed in changes {
            assert_eq!(
                validate_rendered(&plan, &changed, &actual)
                    .unwrap_err()
                    .code(),
                "PRESENT_OUTPUT_MAPPING"
            );
        }
    }

    #[test]
    fn unsafe_paths_and_undeclared_or_wrong_links_are_rejected() {
        let mut plan = sample();
        plan.notes[1].output_path = "../production.md".into();
        assert!(render(&plan).is_err());
        let plan = sample();
        let (mut files, mut actual) = literal_files();
        let changed = TABLE
            .replace("#知识明细\\|详情", "#不存在\\|详情")
            .into_bytes();
        files[0].byte_length = changed.len() as u64;
        files[0].content_sha256 = crate::text::sha256(&changed);
        actual.insert("知识比较.md".into(), changed);
        assert_eq!(
            validate_rendered(&plan, &files, &actual)
                .unwrap_err()
                .code(),
            "PRESENT_OUTPUT_LINK"
        );
        let (files, mut actual) = literal_files();
        actual.insert("额外.md".into(), b"undeclared".to_vec());
        assert_eq!(
            validate_rendered(&plan, &files, &actual)
                .unwrap_err()
                .code(),
            "PRESENT_OUTPUT_FILES"
        );
    }

    #[test]
    fn full_fact_context_must_occur_in_its_own_body_and_cannot_come_from_alias() {
        let fact = PresentationFact {
            fact_id: "fact".into(),
            object_id: OBJECT.into(),
            property_id: PROPERTY.into(),
            value_status: "known".into(),
            value: "10".into(),
            unit: "m/s".into(),
            conditions: vec!["x < 2".into()],
            exceptions: vec!["异常时不适用".into()],
            applicability_status: Some("known".into()),
            time: Some(Time {
                status: "known".into(),
                precision: Some("year".into()),
                value: Some("2020".into()),
            }),
            occurrence_ids: vec!["occurrence".into()],
        };
        let facts = BTreeMap::from([("fact", &fact)]);
        let occurrences = BTreeSet::from(["occurrence"]);
        let concepts = BTreeSet::from([OBJECT, PROPERTY]);
        let destinations = BTreeSet::from([(DETAIL_PATH, "知识明细")]);
        let ids = vec!["fact".into()];
        let refs = vec!["occurrence".into()];
        let links = vec![SemanticLink {
            target_concept_id: OBJECT.into(),
            target_path: DETAIL_PATH.into(),
            target_heading: "知识明细".into(),
            display_text: "m/s".into(),
        }];
        for incomplete in [
            "10；x < 2；异常时不适用；2020",
            "10 m/s；x <；异常时不适用；2020",
            "10 m/s；x < 2；2020",
            "10 m/s；x < 2；异常时不适用",
        ] {
            let scope = Scope {
                locator: expected_scope("s", "n", "paragraph", None, None),
                text: incomplete,
                facts: &ids,
                occurrences: &refs,
                concepts: &[],
                links: &links,
            };
            assert_eq!(
                validate_scope(&scope, &facts, &occurrences, &concepts, &destinations)
                    .unwrap_err()
                    .code(),
                "PRESENT_RENDER_FACT_CONTENT"
            );
        }
        let complete = Scope {
            locator: expected_scope("s", "n", "paragraph", None, None),
            text: "10 m/s；x < 2；异常时不适用；2020",
            facts: &ids,
            occurrences: &refs,
            concepts: &[],
            links: &links,
        };
        validate_scope(&complete, &facts, &occurrences, &concepts, &destinations).unwrap();
    }
}
