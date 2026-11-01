//! Deterministic fixture Markdown and independent read-back validation.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::fixture_input::LoadedCase;
use crate::fixture_model::*;
use crate::fixture_plan::{plain_text, validate_plan};

const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScopeLocator {
    pub(crate) section_id: String,
    pub(crate) node_id: String,
    pub(crate) kind: String,
    pub(crate) item_id: Option<String>,
    pub(crate) row_id: Option<String>,
    pub(crate) column_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContentMapping {
    pub(crate) scope: ScopeLocator,
    pub(crate) atom_ids: Vec<String>,
    pub(crate) start_byte: u64,
    pub(crate) end_byte: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LinkMapping {
    pub(crate) scope: ScopeLocator,
    pub(crate) target_concept_id: String,
    pub(crate) target_path: String,
    pub(crate) target_heading: String,
    pub(crate) display_text: String,
    pub(crate) start_byte: u64,
    pub(crate) end_byte: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RenderedFile {
    pub(crate) relative_path: String,
    pub(crate) note_id: String,
    pub(crate) plan_revision: u64,
    pub(crate) content_sha256: String,
    pub(crate) byte_length: u64,
    pub(crate) content_mappings: Vec<ContentMapping>,
    pub(crate) links: Vec<LinkMapping>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedNote {
    pub(crate) file: RenderedFile,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadBackFile {
    pub(crate) relative_path: String,
    pub(crate) bytes: Vec<u8>,
}

fn output_issue(rule: &str, refs: &[&str], message: &str) -> Issue {
    Issue::new(
        rule,
        refs.iter().map(|value| (*value).to_owned()).collect(),
        message,
    )
}

fn locator(
    section: &str,
    node: &str,
    item: Option<&str>,
    row: Option<&str>,
    column: Option<&str>,
) -> ScopeLocator {
    ScopeLocator {
        section_id: section.into(),
        node_id: node.into(),
        kind: if item.is_some() {
            "list_item"
        } else if row.is_some() {
            "table_cell"
        } else {
            "paragraph"
        }
        .into(),
        item_id: item.map(str::to_owned),
        row_id: row.map(str::to_owned),
        column_id: column.map(str::to_owned),
    }
}

fn plan_nodes(case: &LoadedCase) -> impl Iterator<Item = &Node> {
    case.plan
        .notes
        .iter()
        .flat_map(|note| &note.sections)
        .flat_map(|section| &section.ordered_nodes)
}

fn semantic_links(node: &Node) -> Vec<&SemanticLink> {
    match node {
        Node::Paragraph { links, .. } => links.iter().collect(),
        Node::OrderedList { items, .. } => items.iter().flat_map(|item| &item.links).collect(),
        Node::Table { rows, .. } => rows
            .iter()
            .flat_map(|row| &row.cells)
            .flat_map(|cell| &cell.links)
            .collect(),
    }
}

pub(crate) fn validate_renderable(case: &LoadedCase, targets: &[ResolvedTarget]) -> Vec<Issue> {
    let mut issues = Vec::new();
    let (plan_issues, canonical) = validate_plan(case);
    if !plan_issues.is_empty() || targets != canonical {
        issues.push(output_issue(
            "RENDER_TARGET",
            &[],
            "Rendering requires a valid plan and exactly its resolved canonical targets.",
        ));
    }
    for target in targets {
        if target.heading.contains('\\') {
            issues.push(output_issue("RENDER_TEXT_SYNTAX", &[&target.concept_id, &target.section_id], "This finite wiki address protocol cannot represent a canonical heading containing a backslash."));
        }
    }
    for note in &case.plan.notes {
        for heading in std::iter::once(note.title.as_str())
            .chain(note.sections.iter().map(|section| section.heading.as_str()))
        {
            if !plain_text(heading) || heading.contains(['\n', '\t']) {
                issues.push(output_issue(
                    "RENDER_TEXT_SYNTAX",
                    &[&note.note_id],
                    "Titles and headings require supported single-line text without Tab.",
                ));
            }
        }
    }
    for node in plan_nodes(case) {
        for link in semantic_links(node) {
            if !nonempty(&link.display_text)
                || !plain_text(&link.display_text)
                || link.display_text.contains(['[', ']', '|', '\n', '\t'])
            {
                issues.push(output_issue("RENDER_TEXT_SYNTAX", &[node.id(), &link.target_concept_id], "Wiki aliases cannot contain bracket/pipe separators, newline or Tab, and must retain supported plain text."));
            }
            if targets
                .iter()
                .filter(|target| target.concept_id == link.target_concept_id)
                .count()
                != 1
            {
                issues.push(output_issue(
                    "RENDER_TARGET",
                    &[node.id(), &link.target_concept_id],
                    "Each rendered link must have exactly one resolved canonical concept target.",
                ));
            }
        }
    }
    issues
}

fn encode_text(value: &str) -> String {
    let mut encoded = String::new();
    for character in value.chars() {
        match character {
            '\n' => encoded.push_str("<br>"),
            '\t' => encoded.push_str("&#9;"),
            character if character.is_ascii_punctuation() => {
                encoded.push('\\');
                encoded.push(character);
            }
            character => encoded.push(character),
        }
    }
    encoded
}

struct Builder {
    text: String,
    mappings: Vec<ContentMapping>,
    links: Vec<LinkMapping>,
    limit: usize,
}

impl Builder {
    fn push(&mut self, value: &str) -> Result<(), Vec<Issue>> {
        if self
            .text
            .len()
            .checked_add(value.len())
            .is_none_or(|size| size > self.limit)
        {
            return Err(vec![output_issue(
                "RENDER_OUTPUT_LIMIT",
                &[],
                "The complete output cannot exceed 64 MiB; no truncated note is returned.",
            )]);
        }
        self.text.push_str(value);
        Ok(())
    }

    fn payload(
        &mut self,
        text: &str,
        atom_ids: &[String],
        links: &[SemanticLink],
        scope: ScopeLocator,
        table: bool,
        targets: &[ResolvedTarget],
    ) -> Result<(), Vec<Issue>> {
        let start = self.text.len() as u64;
        self.push(&encode_text(text))?;
        self.mappings.push(ContentMapping {
            scope: scope.clone(),
            atom_ids: atom_ids.to_vec(),
            start_byte: start,
            end_byte: self.text.len() as u64,
        });
        for link in links {
            let Some(target) = targets
                .iter()
                .find(|target| target.concept_id == link.target_concept_id)
            else {
                return Err(vec![output_issue(
                    "RENDER_TARGET",
                    &[&link.target_concept_id],
                    "The canonical link target disappeared before rendering.",
                )]);
            };
            let Some(stem) = target
                .output_path
                .get(..target.output_path.len().saturating_sub(3))
            else {
                return Err(vec![output_issue(
                    "RENDER_TARGET",
                    &[&target.output_path],
                    "The canonical Markdown path cannot form a wiki stem.",
                )]);
            };
            self.push(" ")?;
            let start = self.text.len() as u64;
            let delimiter = if table { "\\|" } else { "|" };
            self.push(&format!(
                "[[{stem}#{}{delimiter}{}]]",
                target.heading,
                encode_text(&link.display_text)
            ))?;
            self.links.push(LinkMapping {
                scope: scope.clone(),
                target_concept_id: link.target_concept_id.clone(),
                target_path: target.output_path.clone(),
                target_heading: target.heading.clone(),
                display_text: link.display_text.clone(),
                start_byte: start,
                end_byte: self.text.len() as u64,
            });
        }
        Ok(())
    }

    fn node(
        &mut self,
        section: &str,
        node: &Node,
        targets: &[ResolvedTarget],
    ) -> Result<(), Vec<Issue>> {
        match node {
            Node::Paragraph {
                node_id,
                atom_ids,
                text,
                links,
                ..
            } => {
                self.payload(
                    text,
                    atom_ids,
                    links,
                    locator(section, node_id, None, None, None),
                    false,
                    targets,
                )?;
                self.push("\n\n")?;
            }
            Node::OrderedList { node_id, items, .. } => {
                for (index, item) in items.iter().enumerate() {
                    self.push(&format!("{}. ", index + 1))?;
                    self.payload(
                        &item.text,
                        &item.atom_ids,
                        &item.links,
                        locator(section, node_id, Some(&item.item_id), None, None),
                        false,
                        targets,
                    )?;
                    self.push("\n")?;
                }
                self.push("\n")?;
            }
            Node::Table {
                node_id,
                columns,
                rows,
                ..
            } => {
                self.push("|")?;
                for column in columns {
                    self.push(&format!(" {} |", encode_text(&column.label)))?;
                }
                self.push("\n|")?;
                for _ in columns {
                    self.push(" --- |")?;
                }
                self.push("\n")?;
                for row in rows {
                    self.push("|")?;
                    for (column, cell) in columns.iter().zip(&row.cells) {
                        self.push(" ")?;
                        self.payload(
                            &cell.text,
                            &cell.atom_ids,
                            &cell.links,
                            locator(
                                section,
                                node_id,
                                None,
                                Some(&row.row_id),
                                Some(&column.column_id),
                            ),
                            true,
                            targets,
                        )?;
                        self.push(" |")?;
                    }
                    self.push("\n")?;
                }
                self.push("\n")?;
            }
        }
        Ok(())
    }
}

pub(crate) fn render(
    case: &LoadedCase,
    targets: &[ResolvedTarget],
) -> Result<Vec<RenderedNote>, Vec<Issue>> {
    let issues = validate_renderable(case, targets);
    if !issues.is_empty() {
        return Err(issues);
    }
    let mut notes = Vec::new();
    let mut total = 0;
    for note in &case.plan.notes {
        let mut builder = Builder {
            text: String::new(),
            mappings: Vec::new(),
            links: Vec::new(),
            limit: MAX_OUTPUT_BYTES - total,
        };
        builder.push(&format!("# {}\n\n", encode_text(&note.title)))?;
        for section in &note.sections {
            builder.push(&format!("## {}\n\n", encode_text(&section.heading)))?;
            for node in &section.ordered_nodes {
                builder.node(&section.section_id, node, targets)?;
            }
        }
        let bytes = builder.text.into_bytes();
        total += bytes.len();
        let file = RenderedFile {
            relative_path: note.output_path.clone(),
            note_id: note.note_id.clone(),
            plan_revision: case.plan.revision,
            content_sha256: crate::text::sha256(&bytes),
            byte_length: bytes.len() as u64,
            content_mappings: builder.mappings,
            links: builder.links,
        };
        notes.push(RenderedNote { file, bytes });
    }
    Ok(notes)
}

#[derive(Debug)]
struct ParsedLink {
    scope: ScopeLocator,
    path: String,
    heading: String,
    display: String,
    start: usize,
    end: usize,
}

struct ParsedScope {
    scope: ScopeLocator,
    atom_ids: Vec<String>,
    expected_text: String,
    expected_links: Vec<SemanticLink>,
    text: String,
    start: usize,
    end: usize,
    links: Vec<ParsedLink>,
}

struct ParsedTable {
    node_id: String,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
}

struct ParsedNote {
    path: String,
    note_id: String,
    title: String,
    headings: Vec<String>,
    scopes: Vec<ParsedScope>,
    tables: Vec<ParsedTable>,
}

// This inverse codec does not call the encoder. Only the two intentional
// control tokens are recognized; escaped literal tokens are never re-decoded.
fn decode_text(raw: &str) -> Result<String, &'static str> {
    let mut remaining = raw;
    let mut text = String::new();
    while !remaining.is_empty() {
        if let Some(rest) = remaining.strip_prefix("<br>") {
            text.push('\n');
            remaining = rest;
            continue;
        }
        if let Some(rest) = remaining.strip_prefix("&#9;") {
            text.push('\t');
            remaining = rest;
            continue;
        }
        let Some(character) = remaining.chars().next() else {
            break;
        };
        remaining = &remaining[character.len_utf8()..];
        if character == '\\' {
            let Some(escaped) = remaining.chars().next() else {
                return Err("A text escape is truncated.");
            };
            if !escaped.is_ascii_punctuation() {
                return Err("Only ASCII punctuation may use a text escape.");
            }
            text.push(escaped);
            remaining = &remaining[escaped.len_utf8()..];
        } else if character.is_ascii_punctuation() || character.is_control() {
            return Err(
                "An unsupported raw control or Markdown punctuation occurs in displayed text.",
            );
        } else {
            text.push(character);
        }
    }
    Ok(text)
}

fn wiki_start(raw: &str) -> Option<usize> {
    let mut position = 0;
    while position < raw.len() {
        let tail = raw.get(position..)?;
        if tail.starts_with("[[") {
            return Some(position);
        }
        let character = tail.chars().next()?;
        position += character.len_utf8();
        if character == '\\' {
            if let Some(next) = raw.get(position..).and_then(|tail| tail.chars().next()) {
                position += next.len_utf8();
            }
        }
    }
    None
}

fn wiki_end(raw: &str, start: usize) -> Option<usize> {
    let mut position = start.checked_add(2)?;
    while position < raw.len() {
        let tail = raw.get(position..)?;
        if tail.starts_with("]]") {
            return position.checked_add(2);
        }
        let character = tail.chars().next()?;
        position += character.len_utf8();
        if character == '\\' {
            if let Some(next) = raw.get(position..).and_then(|tail| tail.chars().next()) {
                position += next.len_utf8();
            }
        }
    }
    None
}

fn parse_link(
    raw: &str,
    scope: ScopeLocator,
    start: usize,
    table: bool,
) -> Result<ParsedLink, &'static str> {
    if !raw.starts_with("[[") || !raw.ends_with("]]") || raw.len() < 4 {
        return Err("A wiki token is incomplete.");
    }
    let Some(inner) = raw.get(2..raw.len() - 2) else {
        return Err("A wiki token has invalid boundaries.");
    };
    let delimiter = if table { "\\|" } else { "|" };
    let Some((address, alias)) = inner.split_once(delimiter) else {
        return Err("A wiki link has no exact alias separator.");
    };
    let Some((stem, heading)) = address.split_once('#') else {
        return Err("A wiki link has no canonical heading.");
    };
    if stem.is_empty() || heading.is_empty() || heading.contains(['#', '\\', '[', ']', '|']) {
        return Err("A wiki address is outside the supported heading protocol.");
    }
    let display = decode_text(alias)?;
    if !nonempty(&display) || display.contains(['[', ']', '|', '\n', '\t']) {
        return Err("A wiki alias is unsupported.");
    }
    Ok(ParsedLink {
        scope,
        path: format!("{stem}.md"),
        heading: heading.into(),
        display,
        start,
        end: start + raw.len(),
    })
}

fn parse_scope(
    raw: &str,
    absolute: usize,
    scope: ScopeLocator,
    text: &str,
    atom_ids: &[String],
    expected_links: &[SemanticLink],
    table: bool,
) -> Result<ParsedScope, &'static str> {
    let body_end = if let Some(start) = wiki_start(raw) {
        if start == 0 || raw.as_bytes().get(start - 1) != Some(&b' ') {
            return Err("Appended semantic links require exactly one separator after the body.");
        }
        start - 1
    } else {
        raw.len()
    };
    let body = raw.get(..body_end).ok_or("The body boundary is invalid.")?;
    let mut links = Vec::new();
    let mut position = body_end;
    while position < raw.len() {
        let remaining = raw
            .get(position..)
            .ok_or("The link separator is invalid.")?;
        if !remaining.starts_with(" [[") {
            return Err("Unexpected data occurs after the body.");
        }
        let start = position + 1;
        let end = wiki_end(raw, start).ok_or("A semantic link is unterminated.")?;
        links.push(parse_link(
            raw.get(start..end).ok_or("The wiki boundary is invalid.")?,
            scope.clone(),
            absolute + start,
            table,
        )?);
        position = end;
    }
    Ok(ParsedScope {
        scope,
        atom_ids: atom_ids.to_vec(),
        expected_text: text.into(),
        expected_links: expected_links.to_vec(),
        text: decode_text(body)?,
        start: absolute,
        end: absolute + body_end,
        links,
    })
}

struct Cursor<'a> {
    text: &'a str,
    position: usize,
}

impl<'a> Cursor<'a> {
    fn line(&mut self) -> Result<(usize, &'a str), &'static str> {
        let tail = self
            .text
            .get(self.position..)
            .ok_or("The file cursor is invalid.")?;
        let length = tail
            .find('\n')
            .ok_or("Every structural line must end in LF.")?;
        let start = self.position;
        let end = start + length;
        let line = self
            .text
            .get(start..end)
            .ok_or("A line boundary is invalid.")?;
        self.position = end + 1;
        Ok((start, line))
    }

    fn blank(&mut self) -> Result<(), &'static str> {
        if self.line()?.1.is_empty() {
            Ok(())
        } else {
            Err("Structural blocks must be separated by a blank line.")
        }
    }

    fn heading(&mut self, marker: &str) -> Result<String, &'static str> {
        let (_, line) = self.line()?;
        let value = line
            .strip_prefix(marker)
            .ok_or("The expected title or section marker is missing.")?;
        let heading = decode_text(value)?;
        self.blank()?;
        Ok(heading)
    }
}

fn table_cells(line: &str, absolute: usize) -> Result<Vec<(usize, &str)>, &'static str> {
    let mut separators = Vec::new();
    let mut position = 0;
    while position < line.len() {
        let tail = line.get(position..).ok_or("A table cursor is invalid.")?;
        let character = tail.chars().next().ok_or("A table character is missing.")?;
        if character == '|' {
            separators.push(position);
        }
        position += character.len_utf8();
        if character == '\\' {
            if let Some(escaped) = line.get(position..).and_then(|tail| tail.chars().next()) {
                position += escaped.len_utf8();
            }
        }
    }
    if separators.len() < 2
        || separators.first() != Some(&0)
        || separators.last().copied() != line.len().checked_sub(1)
    {
        return Err("A table row requires outer pipes and unambiguous escaped cells.");
    }
    separators
        .windows(2)
        .map(|positions| {
            let start = positions[0] + 1;
            let end = positions[1];
            let cell = line
                .get(start..end)
                .ok_or("A table cell boundary is invalid.")?;
            if cell.len() < 2 || !cell.starts_with(' ') || !cell.ends_with(' ') {
                return Err("Each table cell must retain its exact padding.");
            }
            Ok((
                absolute + start + 1,
                cell.get(1..cell.len() - 1)
                    .ok_or("Table padding is invalid.")?,
            ))
        })
        .collect()
}

fn parse_table(
    cursor: &mut Cursor<'_>,
    section: &str,
    node_id: &str,
    columns: &[TableColumn],
    rows: &[TableRow],
    parsed: &mut ParsedNote,
) -> Result<(), &'static str> {
    let (start, line) = cursor.line()?;
    let header = table_cells(line, start)?;
    if header.len() != columns.len() {
        return Err("The actual table header is not rectangular.");
    }
    let actual_columns: Vec<_> = header
        .iter()
        .map(|(_, text)| decode_text(text))
        .collect::<Result<_, _>>()?;
    let (start, line) = cursor.line()?;
    let separator = table_cells(line, start)?;
    if separator.len() != columns.len() || separator.iter().any(|(_, value)| *value != "---") {
        return Err("The table separator must match every column.");
    }
    let mut actual_rows = Vec::new();
    for row in rows {
        let (start, line) = cursor.line()?;
        let cells = table_cells(line, start)?;
        if cells.len() != columns.len() || row.cells.len() != columns.len() {
            return Err("The actual table row is not rectangular.");
        }
        let mut actual_cells = Vec::new();
        for ((column, expected), (start, raw)) in columns.iter().zip(&row.cells).zip(cells) {
            let scope = parse_scope(
                raw,
                start,
                locator(
                    section,
                    node_id,
                    None,
                    Some(&row.row_id),
                    Some(&column.column_id),
                ),
                &expected.text,
                &expected.atom_ids,
                &expected.links,
                true,
            )?;
            actual_cells.push(scope.text.clone());
            parsed.scopes.push(scope);
        }
        actual_rows.push(actual_cells);
    }
    parsed.tables.push(ParsedTable {
        node_id: node_id.into(),
        columns: actual_columns,
        rows: actual_rows,
    });
    cursor.blank()
}

fn parse_note(note: &Note, text: &str) -> Result<ParsedNote, &'static str> {
    let mut cursor = Cursor { text, position: 0 };
    let mut parsed = ParsedNote {
        path: note.output_path.clone(),
        note_id: note.note_id.clone(),
        title: cursor.heading("# ")?,
        headings: Vec::new(),
        scopes: Vec::new(),
        tables: Vec::new(),
    };
    for section in &note.sections {
        parsed.headings.push(cursor.heading("## ")?);
        for node in &section.ordered_nodes {
            match node {
                Node::Paragraph {
                    node_id,
                    atom_ids,
                    text,
                    links,
                    ..
                } => {
                    let (start, raw) = cursor.line()?;
                    parsed.scopes.push(parse_scope(
                        raw,
                        start,
                        locator(&section.section_id, node_id, None, None, None),
                        text,
                        atom_ids,
                        links,
                        false,
                    )?);
                    cursor.blank()?;
                }
                Node::OrderedList { node_id, items, .. } => {
                    for (index, item) in items.iter().enumerate() {
                        let (start, raw) = cursor.line()?;
                        let marker = format!("{}. ", index + 1);
                        let body = raw
                            .strip_prefix(&marker)
                            .ok_or("The ordered list item sequence is invalid.")?;
                        parsed.scopes.push(parse_scope(
                            body,
                            start + marker.len(),
                            locator(
                                &section.section_id,
                                node_id,
                                Some(&item.item_id),
                                None,
                                None,
                            ),
                            &item.text,
                            &item.atom_ids,
                            &item.links,
                            false,
                        )?);
                    }
                    cursor.blank()?;
                }
                Node::Table {
                    node_id,
                    columns,
                    rows,
                    ..
                } => parse_table(
                    &mut cursor,
                    &section.section_id,
                    node_id,
                    columns,
                    rows,
                    &mut parsed,
                )?,
            }
        }
    }
    if cursor.position != text.len() {
        return Err("Extra actual data occurs outside the planned structural blocks.");
    }
    Ok(parsed)
}

pub(crate) fn validate_rendered(
    case: &LoadedCase,
    targets: &[ResolvedTarget],
    manifest: &[RenderedFile],
    files: &[ReadBackFile],
) -> Vec<Issue> {
    let mut issues = Vec::new();
    let paths: BTreeSet<_> = case
        .plan
        .notes
        .iter()
        .map(|note| note.output_path.as_str())
        .collect();
    let manifest_paths: BTreeSet<_> = manifest
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    let actual_paths: BTreeSet<_> = files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    let insensitive: BTreeSet<_> = files
        .iter()
        .map(|file| file.relative_path.to_lowercase())
        .collect();
    if paths != manifest_paths
        || paths != actual_paths
        || manifest_paths.len() != manifest.len()
        || actual_paths.len() != files.len()
        || insensitive.len() != files.len()
    {
        issues.push(output_issue("OUTPUT_MANIFEST", &[], "The complete actual file and manifest sets must exactly match the note paths without extra, missing or colliding identities."));
    }
    if files
        .iter()
        .try_fold(0usize, |sum, file| sum.checked_add(file.bytes.len()))
        .is_none_or(|total| total > MAX_OUTPUT_BYTES)
    {
        issues.push(output_issue(
            "OUTPUT_MANIFEST",
            &[],
            "The read-back output exceeds the complete 64 MiB bound.",
        ));
        return issues;
    }
    let mut parsed_notes = Vec::new();
    let mut all_links = Vec::new();
    for note in &case.plan.notes {
        let metadata: Vec<_> = manifest
            .iter()
            .filter(|file| file.relative_path == note.output_path)
            .collect();
        let actual: Vec<_> = files
            .iter()
            .filter(|file| file.relative_path == note.output_path)
            .collect();
        if metadata.len() != 1 || actual.len() != 1 {
            continue;
        }
        let metadata = metadata[0];
        let bytes = &actual[0].bytes;
        if metadata.note_id != note.note_id
            || metadata.plan_revision != case.plan.revision
            || metadata.byte_length != bytes.len() as u64
            || metadata.content_sha256 != crate::text::sha256(bytes)
        {
            issues.push(output_issue("OUTPUT_MANIFEST", &[&note.note_id, &note.output_path], "File identity, revision, actual byte length and read-back SHA256 must match the manifest."));
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            issues.push(output_issue(
                "OUTPUT_MANIFEST",
                &[&note.output_path],
                "The actual output is not complete valid UTF8.",
            ));
            continue;
        };
        validate_ranges(metadata, text, &mut issues);
        let parsed = match parse_note(note, text) {
            Ok(parsed) => {
                validate_parsed_file(case, targets, note, metadata, &parsed, &mut issues);
                Some(parsed)
            }
            Err(reason) => {
                issues.push(output_issue("OUTPUT_CONTENT", &[&note.output_path], reason));
                None
            }
        };
        all_links.extend(inspect_file_links(
            text,
            metadata,
            parsed.as_ref(),
            &mut issues,
        ));
        if let Some(parsed) = parsed {
            parsed_notes.push(parsed);
        }
    }
    validate_output_expectations(case, targets, &parsed_notes, &all_links, &mut issues);
    issues
}

fn valid_range(text: &str, start: u64, end: u64) -> bool {
    let (Ok(start), Ok(end)) = (usize::try_from(start), usize::try_from(end)) else {
        return false;
    };
    start < end && text.get(start..end).is_some()
}

fn validate_ranges(file: &RenderedFile, text: &str, issues: &mut Vec<Issue>) {
    if file
        .content_mappings
        .iter()
        .any(|mapping| !valid_range(text, mapping.start_byte, mapping.end_byte))
    {
        issues.push(output_issue("OUTPUT_MAPPING", &[&file.relative_path], "Every content interval must be nonempty, bounded and on actual UTF8 character boundaries."));
    }
    if file
        .links
        .iter()
        .any(|link| !valid_range(text, link.start_byte, link.end_byte))
    {
        issues.push(output_issue(
            "OUTPUT_LINK",
            &[&file.relative_path],
            "Every semantic link interval must have valid actual UTF8 boundaries.",
        ));
    }
}

fn validate_parsed_file(
    case: &LoadedCase,
    targets: &[ResolvedTarget],
    note: &Note,
    file: &RenderedFile,
    parsed: &ParsedNote,
    issues: &mut Vec<Issue>,
) {
    let headings: Vec<_> = note
        .sections
        .iter()
        .map(|section| section.heading.as_str())
        .collect();
    let actual_headings: Vec<_> = parsed.headings.iter().map(String::as_str).collect();
    let unique: BTreeSet<_> = parsed
        .headings
        .iter()
        .map(|heading| heading.trim().to_lowercase())
        .collect();
    if parsed.title != note.title
        || headings != actual_headings
        || unique.len() != parsed.headings.len()
    {
        issues.push(output_issue("OUTPUT_CONTENT", &[&note.note_id], "Actual note and section headings must retain exact text, order and unique heading addresses."));
    }
    if file.content_mappings.len() != parsed.scopes.len()
        || file
            .content_mappings
            .iter()
            .zip(&parsed.scopes)
            .any(|(mapping, actual)| {
                mapping.scope != actual.scope
                    || mapping.atom_ids != actual.atom_ids
                    || mapping.start_byte != actual.start as u64
                    || mapping.end_byte != actual.end as u64
            })
    {
        issues.push(output_issue("OUTPUT_MAPPING", &[&file.relative_path], "The complete ordered scope mapping inventory must equal independently parsed body boundaries and planned child identities/atoms; aliases are outside body ranges."));
    }
    let mut actual_link_metadata = Vec::new();
    for scope in &parsed.scopes {
        if scope.text != scope.expected_text
            || scope
                .atom_ids
                .iter()
                .any(|id| !scope_atom_complete(case, scope, id))
        {
            issues.push(output_issue("OUTPUT_CONTENT", &[&scope.scope.node_id], "This actual scope must retain its complete planned body and its own atom content and every qualifier, without borrowing from another scope or alias."));
        }
        if scope.links.len() != scope.expected_links.len() {
            issues.push(output_issue("OUTPUT_LINK", &[&scope.scope.node_id], "The actual semantic link sequence must match every planned link in this exact scope."));
        }
        for (actual, wanted) in scope.links.iter().zip(&scope.expected_links) {
            let canonical: Vec<_> = targets
                .iter()
                .filter(|target| target.concept_id == wanted.target_concept_id)
                .collect();
            if canonical.len() != 1
                || actual.path != canonical[0].output_path
                || actual.heading != canonical[0].heading
                || actual.display != wanted.display_text
            {
                issues.push(output_issue("OUTPUT_LINK", &[&scope.scope.node_id, &wanted.target_concept_id], "The actual link address and literal alias must retain the plan's canonical concept target."));
            }
            actual_link_metadata.push(LinkMapping {
                scope: actual.scope.clone(),
                target_concept_id: wanted.target_concept_id.clone(),
                target_path: actual.path.clone(),
                target_heading: actual.heading.clone(),
                display_text: actual.display.clone(),
                start_byte: actual.start as u64,
                end_byte: actual.end as u64,
            });
        }
    }
    if file.links != actual_link_metadata {
        issues.push(output_issue("OUTPUT_LINK", &[&file.relative_path], "The complete actual link identities, addresses, aliases and byte intervals must equal their manifest mappings."));
    }
}

fn scope_atom_complete(case: &LoadedCase, scope: &ParsedScope, id: &str) -> bool {
    if scope.text != scope.expected_text {
        return false;
    }
    let atoms: Vec<_> = case
        .ir
        .atoms
        .iter()
        .filter(|atom| atom.atom_id == id)
        .collect();
    if atoms.len() != 1
        || !nonempty(&atoms[0].content)
        || !scope.text.contains(&atoms[0].content)
        || atoms[0]
            .qualifiers
            .iter()
            .any(|qualifier| !nonempty(qualifier) || !scope.text.contains(qualifier))
    {
        return false;
    }
    if atoms[0].required {
        let expected: Vec<_> = case
            .expected
            .required_atoms
            .iter()
            .filter(|atom| atom.atom_id == id)
            .collect();
        expected.len() == 1
            && scope.text.contains(&expected[0].content)
            && expected[0]
                .qualifiers
                .iter()
                .all(|qualifier| scope.text.contains(qualifier))
    } else {
        true
    }
}

fn scan_link_ranges(text: &str) -> (Vec<(usize, usize)>, bool) {
    let mut ranges = Vec::new();
    let mut invalid = false;
    let mut position = 0;
    while position < text.len() {
        let Some(tail) = text.get(position..) else {
            invalid = true;
            break;
        };
        if tail.starts_with("[[") {
            if let Some(end) = wiki_end(text, position) {
                ranges.push((position, end));
                position = end;
                continue;
            }
            invalid = true;
        }
        let Some(character) = tail.chars().next() else {
            break;
        };
        if character == '[' || character == ']' || tail.starts_with("![") {
            invalid = true;
        }
        position += character.len_utf8();
        if character == '\\' {
            if let Some(escaped) = text.get(position..).and_then(|tail| tail.chars().next()) {
                position += escaped.len_utf8();
            }
        }
    }
    (ranges, invalid)
}

fn inspect_file_links(
    text: &str,
    file: &RenderedFile,
    parsed: Option<&ParsedNote>,
    issues: &mut Vec<Issue>,
) -> Vec<ParsedLink> {
    let (ranges, invalid) = scan_link_ranges(text);
    let metadata_ranges: Vec<_> = file
        .links
        .iter()
        .map(|link| (link.start_byte, link.end_byte))
        .collect();
    let actual_ranges: Vec<_> = ranges
        .iter()
        .map(|(start, end)| (*start as u64, *end as u64))
        .collect();
    let known = parsed.map(|parsed| {
        parsed
            .scopes
            .iter()
            .flat_map(|scope| &scope.links)
            .map(|link| (link.start, link.end))
            .collect::<Vec<_>>()
    });
    if invalid || actual_ranges != metadata_ranges || known.is_some_and(|known| known != ranges) {
        issues.push(output_issue("OUTPUT_LINK", &[&file.relative_path], "All actual file link tokens must occur in declared scopes and match the complete link mapping inventory; no unregistered raw link is allowed."));
    }
    let mut links = Vec::new();
    for (start, end) in ranges {
        let Some(raw) = text.get(start..end) else {
            continue;
        };
        let scope = file
            .links
            .iter()
            .find(|link| link.start_byte == start as u64 && link.end_byte == end as u64)
            .map(|link| link.scope.clone())
            .unwrap_or_else(|| locator("", "", None, None, None));
        match parse_link(raw, scope, start, raw.contains("\\|")) {
            Ok(link) => links.push(link),
            Err(reason) => issues.push(output_issue("OUTPUT_LINK", &[&file.relative_path], reason)),
        }
    }
    links
}

fn validate_output_expectations(
    case: &LoadedCase,
    targets: &[ResolvedTarget],
    notes: &[ParsedNote],
    links: &[ParsedLink],
    issues: &mut Vec<Issue>,
) {
    let mut displayed = BTreeSet::new();
    for note in notes {
        for scope in &note.scopes {
            displayed.extend(
                scope
                    .atom_ids
                    .iter()
                    .filter(|id| scope_atom_complete(case, scope, id))
                    .map(String::as_str),
            );
        }
    }
    for expected in &case.expected.required_atoms {
        if !displayed.contains(expected.atom_id.as_str()) {
            issues.push(output_issue("OUTPUT_CONTENT", &[&expected.atom_id], "An independent required atom has no complete actual body mapping retaining every condition/unit/exception."));
        }
    }
    for expected in &case.expected.canonical_placements {
        let complete = expected.atom_ids.iter().all(|id| {
            notes.iter().any(|note| {
                note.note_id == expected.note_id
                    && note.scopes.iter().any(|scope| {
                        scope.scope.section_id == expected.section_id
                            && scope.atom_ids.contains(id)
                            && scope_atom_complete(case, scope, id)
                    })
            })
        });
        if !complete || expected.atom_ids.is_empty() {
            issues.push(output_issue("OUTPUT_CONTENT", &[&expected.concept_id, &expected.section_id], "The canonical actual note/section must fully display all independently required explanation atoms."));
        }
    }
    validate_actual_tables(case, notes, issues);
    for link in links {
        let canonical: Vec<_> = targets
            .iter()
            .filter(|target| target.output_path == link.path && target.heading == link.heading)
            .collect();
        let count = notes
            .iter()
            .filter(|note| note.path == link.path)
            .flat_map(|note| &note.headings)
            .filter(|heading| **heading == link.heading)
            .count();
        if canonical.len() != 1 || count != 1 {
            issues.push(output_issue("OUTPUT_LINK", &[&link.path, &link.heading], "Every actual heading link must resolve to exactly one existing actual canonical file and heading."));
        }
    }
    for expected in &case.expected.required_links {
        let exists = targets
            .iter()
            .find(|target| target.concept_id == expected.target_concept_id)
            .is_some_and(|target| {
                links.iter().any(|link| {
                    link.scope.node_id == expected.from_node_id
                        && link.path == target.output_path
                        && link.heading == target.heading
                        && link.display == expected.display_text
                })
            });
        if !exists {
            issues.push(output_issue(
                "OUTPUT_LINK",
                &[&expected.from_node_id, &expected.target_concept_id],
                "An independently required link is missing from its actual source node.",
            ));
        }
    }
}

fn validate_actual_tables(case: &LoadedCase, notes: &[ParsedNote], issues: &mut Vec<Issue>) {
    let tables: Vec<_> = notes.iter().flat_map(|note| &note.tables).collect();
    if tables.len() != case.expected.tables.len() {
        issues.push(output_issue(
            "OUTPUT_CONTENT",
            &[],
            "Every actual table must have exactly one independent table expectation.",
        ));
    }
    for expected in &case.expected.tables {
        let actual: Vec<_> = tables
            .iter()
            .filter(|table| table.node_id == expected.node_id)
            .collect();
        let columns: Vec<_> = expected
            .columns
            .iter()
            .map(|column| column.label.clone())
            .collect();
        let rows: Vec<Vec<_>> = expected
            .rows
            .iter()
            .map(|row| row.cells.iter().map(|cell| cell.text.clone()).collect())
            .collect();
        let planned: Vec<_> = plan_nodes(case)
            .filter_map(|node| match node {
                Node::Table {
                    node_id,
                    columns,
                    rows,
                    ..
                } if *node_id == expected.node_id => Some((columns, rows)),
                _ => None,
            })
            .collect();
        if actual.len() != 1
            || actual[0].columns != columns
            || actual[0].rows != rows
            || planned.len() != 1
            || *planned[0].0 != expected.columns
            || *planned[0].1 != expected.rows
        {
            issues.push(output_issue("OUTPUT_CONTENT", &[&expected.node_id], "Actual table columns, rows, cell states/text/atom identities and semantic links must retain the independently reviewed ordered table."));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture_input::load;
    use crate::fixture_model::{ExpectedLink, Node, SemanticLink};
    use crate::fixture_plan::validate_plan;
    use crate::text::sha256;
    use std::path::Path;

    const NOTE_A: &str = "# 甲容器\n\n## 定义与比较\n\n甲容器是虚构的蓝砂收纳盒。参见乙容器。 [[笔记/乙容器#定义与条件|乙容器]]\n\n| 比较属性 | 甲容器 | 乙容器 |\n| --- | --- | --- |\n| 虚构刻度 | 甲容器的虚构刻度为5 cm。仅用于室温演示；装满后停止读数；破损时不适用。 | 乙容器的虚构刻度为8 cm。仅用于室温演示；空盒时不读数；浸水后不适用。 |\n| 单价 | 未给出 | 未给出 |\n\n";
    const NOTE_B: &str = "# 乙容器\n\n## 定义与条件\n\n乙容器是虚构的红砂收纳盒。参见甲容器。 [[笔记/甲容器#定义与比较|甲容器]]\n\n1. 乙容器的虚构刻度为8 cm。仅用于室温演示；空盒时不读数；浸水后不适用。\n\n";
    const TEXT_A: &str = "甲容器是虚构的蓝砂收纳盒。参见乙容器。";
    const TEXT_B: &str = "乙容器是虚构的红砂收纳盒。参见甲容器。";
    const VALUE_A: &str = "甲容器的虚构刻度为5 cm。仅用于室温演示；装满后停止读数；破损时不适用。";
    const VALUE_B: &str = "乙容器的虚构刻度为8 cm。仅用于室温演示；空盒时不读数；浸水后不适用。";

    fn fixture() -> (LoadedCase, Vec<ResolvedTarget>) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/text-compare-v1")
            .canonicalize()
            .unwrap();
        let case = load(&path).unwrap();
        assert!(case.source_issues.is_empty());
        let (issues, targets) = validate_plan(&case);
        assert!(issues.is_empty(), "{issues:?}");
        (case, targets)
    }

    fn scope(
        section: &str,
        node: &str,
        item: Option<&str>,
        row: Option<&str>,
        column: Option<&str>,
    ) -> ScopeLocator {
        ScopeLocator {
            section_id: section.into(),
            node_id: node.into(),
            kind: if item.is_some() {
                "list_item"
            } else if row.is_some() {
                "table_cell"
            } else {
                "paragraph"
            }
            .into(),
            item_id: item.map(str::to_owned),
            row_id: row.map(str::to_owned),
            column_id: column.map(str::to_owned),
        }
    }

    fn mapping(
        note: &str,
        text: &str,
        occurrence: usize,
        locator: ScopeLocator,
        atoms: &[&str],
    ) -> ContentMapping {
        let start = note.match_indices(text).nth(occurrence).unwrap().0;
        ContentMapping {
            scope: locator,
            atom_ids: atoms.iter().map(|id| (*id).into()).collect(),
            start_byte: start as u64,
            end_byte: (start + text.len()) as u64,
        }
    }

    fn link(
        note: &str,
        text: &str,
        locator: ScopeLocator,
        concept: &str,
        path: &str,
        heading: &str,
        alias: &str,
    ) -> LinkMapping {
        let start = note.find(text).unwrap();
        LinkMapping {
            scope: locator,
            target_concept_id: concept.into(),
            target_path: path.into(),
            target_heading: heading.into(),
            display_text: alias.into(),
            start_byte: start as u64,
            end_byte: (start + text.len()) as u64,
        }
    }

    fn literal_notes() -> Vec<RenderedNote> {
        let paragraph_a = scope("H2j", "Q2z", None, None, None);
        let paragraph_b = scope("H6n", "Q6z", None, None, None);
        let cell = |row, col| scope("H2j", "T5x", None, Some(row), Some(col));
        let a = RenderedFile {
            relative_path: "笔记/甲容器.md".into(),
            note_id: "N4u".into(),
            plan_revision: 1,
            content_sha256: sha256(NOTE_A.as_bytes()),
            byte_length: NOTE_A.len() as u64,
            content_mappings: vec![
                mapping(NOTE_A, TEXT_A, 0, paragraph_a.clone(), &["A3d"]),
                mapping(NOTE_A, "虚构刻度", 0, cell("W3g", "D2c"), &[]),
                mapping(NOTE_A, VALUE_A, 0, cell("W3g", "D4c"), &["A5p"]),
                mapping(NOTE_A, VALUE_B, 0, cell("W3g", "D6c"), &["A9w"]),
                mapping(NOTE_A, "单价", 0, cell("W7g", "D2c"), &[]),
                mapping(NOTE_A, "未给出", 0, cell("W7g", "D4c"), &[]),
                mapping(NOTE_A, "未给出", 1, cell("W7g", "D6c"), &[]),
            ],
            links: vec![link(
                NOTE_A,
                "[[笔记/乙容器#定义与条件|乙容器]]",
                paragraph_a,
                "C6h",
                "笔记/乙容器.md",
                "定义与条件",
                "乙容器",
            )],
        };
        let b = RenderedFile {
            relative_path: "笔记/乙容器.md".into(),
            note_id: "N8v".into(),
            plan_revision: 1,
            content_sha256: sha256(NOTE_B.as_bytes()),
            byte_length: NOTE_B.len() as u64,
            content_mappings: vec![
                mapping(NOTE_B, TEXT_B, 0, paragraph_b.clone(), &["A7e"]),
                mapping(
                    NOTE_B,
                    VALUE_B,
                    0,
                    scope("H6n", "L9x", Some("I4b"), None, None),
                    &["A9w"],
                ),
            ],
            links: vec![link(
                NOTE_B,
                "[[笔记/甲容器#定义与比较|甲容器]]",
                paragraph_b,
                "C2f",
                "笔记/甲容器.md",
                "定义与比较",
                "甲容器",
            )],
        };
        vec![
            RenderedNote {
                file: a,
                bytes: NOTE_A.as_bytes().to_vec(),
            },
            RenderedNote {
                file: b,
                bytes: NOTE_B.as_bytes().to_vec(),
            },
        ]
    }

    fn actual(notes: &[RenderedNote]) -> Vec<ReadBackFile> {
        notes
            .iter()
            .map(|note| ReadBackFile {
                relative_path: note.file.relative_path.clone(),
                bytes: note.bytes.clone(),
            })
            .collect()
    }

    fn manifest(notes: &[RenderedNote]) -> Vec<RenderedFile> {
        notes.iter().map(|note| note.file.clone()).collect()
    }

    fn has_rule(issues: &[Issue], rule: &str) {
        assert!(
            issues.iter().any(|issue| issue.rule_id == rule),
            "wanted {rule}, got {issues:?}"
        );
    }

    fn tamper(notes: &mut [RenderedNote], index: usize, before: &str, after: &str) {
        let text = String::from_utf8(notes[index].bytes.clone())
            .unwrap()
            .replace(before, after);
        notes[index].bytes = text.into_bytes();
        notes[index].file.byte_length = notes[index].bytes.len() as u64;
        notes[index].file.content_sha256 = sha256(&notes[index].bytes);
    }

    #[test]
    fn renderer_matches_two_author_declared_literal_notes_and_child_mappings() {
        let (case, targets) = fixture();
        assert!(validate_renderable(&case, &targets).is_empty());
        assert_eq!(render(&case, &targets).unwrap(), literal_notes());
    }

    #[test]
    fn independent_validator_accepts_literal_bytes_without_calling_renderer() {
        let (case, targets) = fixture();
        let notes = literal_notes();
        assert!(validate_rendered(&case, &targets, &manifest(&notes), &actual(&notes)).is_empty());
    }

    #[test]
    fn renderable_aliases_reject_unsupported_separators_and_whitespace() {
        for alias in ["乙|别名", "乙[别名]", "乙\n别名", "乙\t别名"] {
            let (mut case, targets) = fixture();
            if let Node::Paragraph { links, .. } =
                &mut case.plan.notes[0].sections[0].ordered_nodes[0]
            {
                links[0].display_text = alias.into();
            }
            has_rule(&validate_renderable(&case, &targets), "RENDER_TEXT_SYNTAX");
        }
    }

    #[test]
    fn wiki_target_headings_reject_backslashes_before_rendering() {
        let (mut case, _) = fixture();
        case.plan.notes[1].sections[0].heading = "定义\\".into();
        let (issues, targets) = validate_plan(&case);
        assert!(issues.is_empty());
        has_rule(&validate_renderable(&case, &targets), "RENDER_TEXT_SYNTAX");
    }

    #[test]
    fn independently_reviewed_table_alias_variant_has_literal_escaped_pipe() {
        let (mut case, targets) = fixture();
        // Author-reviewed variant: table cell link and both independent expected
        // declarations are specified before rendering, not copied from output.
        let semantic = SemanticLink {
            target_concept_id: "C6h".into(),
            display_text: "乙容器入口".into(),
        };
        if let Node::Table { rows, .. } = &mut case.plan.notes[0].sections[0].ordered_nodes[1] {
            rows[0].cells[1].links = vec![semantic.clone()];
        }
        case.expected.tables[0].rows[0].cells[1].links = vec![semantic];
        case.expected.required_links.push(ExpectedLink {
            from_node_id: "T5x".into(),
            target_concept_id: "C6h".into(),
            display_text: "乙容器入口".into(),
        });
        let notes = render(&case, &targets).unwrap();
        let text = std::str::from_utf8(&notes[0].bytes).unwrap();
        assert!(text.contains("[[笔记/乙容器#定义与条件\\|乙容器入口]]"));
        assert!(validate_rendered(&case, &targets, &manifest(&notes), &actual(&notes)).is_empty());
    }

    #[test]
    fn content_checks_catch_unit_condition_and_missing_cell_drift_after_sha_refresh() {
        let (case, targets) = fixture();
        for (old, new) in [
            ("5 cm", "5"),
            ("装满后停止读数", "继续读数"),
            ("| 单价 | 未给出 | 未给出 |", "| 单价 | 0 | 未给出 |"),
            (VALUE_A, ""),
        ] {
            let mut notes = literal_notes();
            tamper(&mut notes, 0, old, new);
            has_rule(
                &validate_rendered(&case, &targets, &manifest(&notes), &actual(&notes)),
                "OUTPUT_CONTENT",
            );
        }
    }

    #[test]
    fn scopes_do_not_borrow_units_from_another_cell_or_alias() {
        let (case, targets) = fixture();
        let mut notes = literal_notes();
        tamper(&mut notes, 0, "5 cm", "5");
        tamper(&mut notes, 0, "比较属性", "比较属性 cm");
        has_rule(
            &validate_rendered(&case, &targets, &manifest(&notes), &actual(&notes)),
            "OUTPUT_CONTENT",
        );
    }

    #[test]
    fn metadata_atom_ids_cannot_hide_missing_or_invalid_scope_ranges() {
        let (case, targets) = fixture();
        for mutation in ["omit", "duplicate", "boundary", "atom", "scope"] {
            let mut notes = literal_notes();
            match mutation {
                "omit" => {
                    notes[0].file.content_mappings.remove(0);
                }
                "duplicate" => {
                    let mapping = notes[0].file.content_mappings[0].clone();
                    notes[0].file.content_mappings.push(mapping);
                }
                "boundary" => notes[0].file.content_mappings[0].start_byte += 1,
                "atom" => notes[0].file.content_mappings[0].atom_ids = vec!["A5p".into()],
                "scope" => notes[0].file.content_mappings[0].scope.node_id = "Qghost".into(),
                _ => unreachable!(),
            }
            has_rule(
                &validate_rendered(&case, &targets, &manifest(&notes), &actual(&notes)),
                "OUTPUT_MAPPING",
            );
        }
    }

    #[test]
    fn manifest_and_utf8_failures_are_distinct_from_content_checks() {
        let (case, targets) = fixture();
        let notes = literal_notes();
        for mutation in ["missing", "extra", "duplicate", "sha", "utf8"] {
            let mut files = actual(&notes);
            let mut metadata = manifest(&notes);
            match mutation {
                "missing" => {
                    files.pop();
                }
                "extra" => files.push(ReadBackFile {
                    relative_path: "ghost.md".into(),
                    bytes: b"# ghost\n".to_vec(),
                }),
                "duplicate" => {
                    let file = files[0].clone();
                    files.push(file);
                }
                "sha" => metadata[0].content_sha256 = "0".repeat(64),
                "utf8" => files[0].bytes = vec![0xff],
                _ => unreachable!(),
            }
            has_rule(
                &validate_rendered(&case, &targets, &metadata, &files),
                "OUTPUT_MANIFEST",
            );
        }
    }

    #[test]
    fn all_actual_links_include_ghost_tokens_outside_declared_scopes() {
        let (case, targets) = fixture();
        for (old, new) in [
            ("#定义与条件|", "#幽灵标题|"),
            ("|乙容器]]", "|错误别名]]"),
            ("\n\n| 比较属性", "\n\n[[不存在#幽灵]]\n\n| 比较属性"),
        ] {
            let mut notes = literal_notes();
            tamper(&mut notes, 0, old, new);
            has_rule(
                &validate_rendered(&case, &targets, &manifest(&notes), &actual(&notes)),
                "OUTPUT_LINK",
            );
        }
    }

    #[test]
    fn links_cannot_disappear_or_move_into_the_body_mapping() {
        let (case, targets) = fixture();
        let mut notes = literal_notes();
        notes[0].file.links.clear();
        has_rule(
            &validate_rendered(&case, &targets, &manifest(&notes), &actual(&notes)),
            "OUTPUT_LINK",
        );
        let mut notes = literal_notes();
        notes[0].file.content_mappings[0].end_byte = notes[0].file.links[0].end_byte;
        has_rule(
            &validate_rendered(&case, &targets, &manifest(&notes), &actual(&notes)),
            "OUTPUT_MAPPING",
        );
    }

    #[test]
    fn actual_tables_and_headings_are_parsed_independently_of_metadata() {
        let (case, targets) = fixture();
        for (old, new) in [
            ("| --- | --- | --- |", "| --- | --- |"),
            ("| 虚构刻度 |", "| 刻度改写 |"),
            ("## 定义与比较", "## 移走的定义"),
            ("| 单价 | 未给出 | 未给出 |", "| 单价 | 未给出 |"),
        ] {
            let mut notes = literal_notes();
            tamper(&mut notes, 0, old, new);
            has_rule(
                &validate_rendered(&case, &targets, &manifest(&notes), &actual(&notes)),
                "OUTPUT_CONTENT",
            );
        }
    }

    #[test]
    fn codec_uses_literal_ascii_escapes_and_decodes_control_tokens_once() {
        let raw = format!("{}\n\t", r"\|[1,2] x < 2; $a$ *b* _c_ `d` &#9;");
        let encoded = format!(
            "{}<br>&#9;",
            r"\\\|\[1\,2\] x \< 2\; \$a\$ \*b\* \_c\_ \`d\` \&\#9\;"
        );
        assert_eq!(encode_text(&raw), encoded);
        assert_eq!(decode_text(&encoded).unwrap(), raw);
        assert_eq!(decode_text(r"\<br\>\&\#9\;").unwrap(), "<br>&#9;");
        for invalid in [
            "<hidden>",
            "[unregistered](ghost)",
            "raw\\letter",
            "unfinished\\",
        ] {
            assert!(decode_text(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn table_parser_counts_escape_parity_without_splitting_literal_pipes() {
        let cells = table_cells(r"| a\|b | c\\\|d |", 0).unwrap();
        assert_eq!(cells.len(), 2);
        assert_eq!(decode_text(cells[0].1).unwrap(), "a|b");
        assert_eq!(decode_text(cells[1].1).unwrap(), r"c\|d");
    }

    #[test]
    fn actual_newline_tab_and_literal_entity_variant_cannot_be_double_decoded() {
        let (mut case, targets) = fixture();
        if let Node::Paragraph { text, .. } = &mut case.plan.notes[0].sections[0].ordered_nodes[0] {
            *text = format!("{TEXT_A}\n\t字面 &#9;，[1,2]; x<2");
        }
        let mut notes = render(&case, &targets).unwrap();
        let actual_text = std::str::from_utf8(&notes[0].bytes).unwrap();
        assert!(actual_text.contains(r"<br>&#9;字面 \&\#9\;，\[1\,2\]\; x\<2"));
        assert!(validate_rendered(&case, &targets, &manifest(&notes), &actual(&notes)).is_empty());
        tamper(&mut notes, 0, r"\&\#9\;", "&#9;");
        has_rule(
            &validate_rendered(&case, &targets, &manifest(&notes), &actual(&notes)),
            "OUTPUT_CONTENT",
        );
    }

    #[test]
    fn an_alias_cannot_complete_a_qualifier_deleted_from_its_cell_body() {
        let (mut case, targets) = fixture();
        // Independently reviewed text-only alias variant; it does not change
        // the required source atom, its conditions or the expected cell body.
        let semantic = SemanticLink {
            target_concept_id: "C6h".into(),
            display_text: "破损时不适用".into(),
        };
        if let Node::Table { rows, .. } = &mut case.plan.notes[0].sections[0].ordered_nodes[1] {
            rows[0].cells[1].links = vec![semantic.clone()];
        }
        case.expected.tables[0].rows[0].cells[1].links = vec![semantic];
        case.expected.required_links.push(ExpectedLink {
            from_node_id: "T5x".into(),
            target_concept_id: "C6h".into(),
            display_text: "破损时不适用".into(),
        });
        let mut notes = render(&case, &targets).unwrap();
        tamper(&mut notes, 0, "；破损时不适用。", "。");
        assert!(
            std::str::from_utf8(&notes[0].bytes)
                .unwrap()
                .contains("[[笔记/乙容器#定义与条件\\|破损时不适用]]")
        );
        has_rule(
            &validate_rendered(&case, &targets, &manifest(&notes), &actual(&notes)),
            "OUTPUT_CONTENT",
        );
    }

    #[test]
    fn output_limit_refuses_a_complete_chunk_without_truncating_text() {
        let mut builder = Builder {
            text: String::new(),
            mappings: Vec::new(),
            links: Vec::new(),
            limit: 2,
        };
        assert!(builder.push("甲").is_err());
        assert!(builder.text.is_empty());
    }
}
