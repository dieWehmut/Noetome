//! Deterministic UTF-8 normalization and complete block-position inventory.
//! These drafts have no identities or knowledge coverage decisions.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const NORMALIZATION_VERSION: &str = "utf8-newlines-v1";
pub const PARSER_VERSION: &str = "blocks-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    Heading,
    Text,
    Code,
    Formatting,
}

impl SpanKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Heading => "heading",
            Self::Text => "text",
            Self::Code => "code",
            Self::Formatting => "formatting",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "heading" => Some(Self::Heading),
            "text" => Some(Self::Text),
            "code" => Some(Self::Code),
            "formatting" => Some(Self::Formatting),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParseStatus {
    Parsed,
    Failed,
    NeedsReview,
}

impl ParseStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Parsed => "parsed",
            Self::Failed => "failed",
            Self::NeedsReview => "needs_review",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "parsed" => Some(Self::Parsed),
            "failed" => Some(Self::Failed),
            "needs_review" => Some(Self::NeedsReview),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanDraft {
    pub kind: SpanKind,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub text_sha256: String,
    pub parse_status: ParseStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedText {
    pub text: String,
    pub normalized_sha256: String,
    pub spans: Vec<SpanDraft>,
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn analyze(raw: &[u8]) -> Result<ParsedText, std::str::Utf8Error> {
    // Decode the original bytes first so a failure offset still identifies the
    // original file, including a leading BOM. Never insert replacement text.
    let decoded = std::str::from_utf8(raw)?;
    let decoded = decoded.strip_prefix('\u{feff}').unwrap_or(decoded);
    let text = decoded.replace("\r\n", "\n").replace('\r', "\n");
    let normalized_sha256 = sha256(text.as_bytes());
    let spans = segment(&text);
    Ok(ParsedText {
        text,
        normalized_sha256,
        spans,
    })
}

struct Line<'a> {
    content: &'a str,
    start: usize,
    end: usize,
    number: usize,
}

#[derive(Clone, Copy)]
struct Fence {
    marker: u8,
    length: usize,
}

// Markdown-like structural markers are recognized only after zero to three
// ASCII spaces. Deeper indentation and tabs are preserved as ordinary text.
fn marker_content(line: &str) -> Option<&str> {
    let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
    (indentation <= 3).then(|| &line[indentation..])
}

fn heading(line: &str) -> bool {
    let Some(content) = marker_content(line) else {
        return false;
    };
    let hashes = content.bytes().take_while(|byte| *byte == b'#').count();
    (1..=6).contains(&hashes)
        && content[hashes..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
}

fn opening_fence(line: &str) -> Option<Fence> {
    let content = marker_content(line)?;
    let marker = *content.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let length = content.bytes().take_while(|byte| *byte == marker).count();
    (length >= 3).then_some(Fence { marker, length })
}

fn closing_fence(line: &str, fence: Fence) -> bool {
    let Some(content) = marker_content(line) else {
        return false;
    };
    let length = content
        .bytes()
        .take_while(|byte| *byte == fence.marker)
        .count();
    length >= fence.length
        && content[length..]
            .bytes()
            .all(|byte| byte == b' ' || byte == b'\t')
}

fn segment(text: &str) -> Vec<SpanDraft> {
    let mut offset = 0;
    let lines: Vec<_> = text
        .split_inclusive('\n')
        .enumerate()
        .map(|(index, line)| {
            let start = offset;
            offset += line.len();
            Line {
                content: line.strip_suffix('\n').unwrap_or(line),
                start,
                end: offset,
                number: index + 1,
            }
        })
        .collect();
    let mut spans = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let start = index;
        let (kind, parse_status) = if let Some(fence) = opening_fence(lines[index].content) {
            index += 1;
            let mut closed = false;
            while index < lines.len() {
                closed = closing_fence(lines[index].content, fence);
                index += 1;
                if closed {
                    break;
                }
            }
            (
                SpanKind::Code,
                if closed {
                    ParseStatus::Parsed
                } else {
                    ParseStatus::NeedsReview
                },
            )
        } else if heading(lines[index].content) {
            index += 1;
            (SpanKind::Heading, ParseStatus::Parsed)
        } else if lines[index].content.trim().is_empty() {
            index += 1;
            while index < lines.len() && lines[index].content.trim().is_empty() {
                index += 1;
            }
            (SpanKind::Formatting, ParseStatus::Parsed)
        } else {
            index += 1;
            while index < lines.len()
                && !lines[index].content.trim().is_empty()
                && !heading(lines[index].content)
                && opening_fence(lines[index].content).is_none()
            {
                index += 1;
            }
            (SpanKind::Text, ParseStatus::Parsed)
        };
        let first = &lines[start];
        let last = &lines[index - 1];
        spans.push(SpanDraft {
            kind,
            start_byte: first.start,
            end_byte: last.end,
            start_line: first.number,
            end_line: last.number,
            text_sha256: sha256(&text.as_bytes()[first.start..last.end]),
            parse_status,
        });
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    const MIXED_RAW: &str =
        "\u{feff}# 标题\r\n正文①\r续行\u{feff}😃\r\n\r\n```rust\r\n# 代码\r\n```\r\n尾部";
    const MIXED_NORMALIZED: &str = "# 标题\n正文①\n续行\u{feff}😃\n\n```rust\n# 代码\n```\n尾部";

    fn assert_complete(parsed: &ParsedText) {
        let mut cursor = 0;
        let mut reconstructed = String::new();
        let mut next_line = 1;
        for span in &parsed.spans {
            assert_eq!(span.start_byte, cursor, "gap or overlap");
            assert!(span.end_byte > span.start_byte, "empty span");
            assert!(parsed.text.is_char_boundary(span.start_byte));
            assert!(parsed.text.is_char_boundary(span.end_byte));
            let slice = &parsed.text[span.start_byte..span.end_byte];
            assert_eq!(span.start_line, next_line);
            assert_eq!(
                span.end_line,
                next_line + slice.split_inclusive('\n').count() - 1
            );
            assert_eq!(
                span.text_sha256,
                format!("{:x}", Sha256::digest(slice.as_bytes()))
            );
            next_line += slice.bytes().filter(|byte| *byte == b'\n').count();
            reconstructed.push_str(slice);
            cursor = span.end_byte;
        }
        assert_eq!(cursor, parsed.text.len(), "unrecorded trailing bytes");
        assert_eq!(reconstructed, parsed.text);
        assert_eq!(
            parsed.normalized_sha256,
            format!("{:x}", Sha256::digest(parsed.text.as_bytes()))
        );
    }

    #[test]
    fn sha256_matches_independent_empty_ascii_and_utf8_vectors() {
        assert_eq!(
            sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256("中文".as_bytes()),
            "72726d8818f693066ceb69afa364218b692e62ea92b385782363780f47529c21"
        );
    }

    #[test]
    fn only_the_first_bom_and_newline_encoding_are_normalized() {
        let parsed = analyze(MIXED_RAW.as_bytes()).unwrap();
        assert_eq!(parsed.text, MIXED_NORMALIZED);
        assert!(parsed.text.contains("续行\u{feff}😃"));
        assert_complete(&parsed);
    }

    #[test]
    fn mixed_unicode_blocks_have_explicit_byte_and_line_boundaries() {
        let parsed = analyze(MIXED_RAW.as_bytes()).unwrap();
        let expected = [
            (SpanKind::Heading, 0, 9, 1, 1, "# 标题\n"),
            (SpanKind::Text, 9, 33, 2, 3, "正文①\n续行\u{feff}😃\n"),
            (SpanKind::Formatting, 33, 34, 4, 4, "\n"),
            (SpanKind::Code, 34, 55, 5, 7, "```rust\n# 代码\n```\n"),
            (SpanKind::Text, 55, 61, 8, 8, "尾部"),
        ];
        assert_eq!(parsed.spans.len(), expected.len());
        for (span, (kind, start, end, start_line, end_line, content)) in
            parsed.spans.iter().zip(expected)
        {
            assert_eq!(span.kind, kind);
            assert_eq!((span.start_byte, span.end_byte), (start, end));
            assert_eq!((span.start_line, span.end_line), (start_line, end_line));
            assert_eq!(&parsed.text[start..end], content);
            assert_eq!(span.parse_status, ParseStatus::Parsed);
        }
        assert_complete(&parsed);
    }

    #[test]
    fn empty_or_only_bom_inputs_have_no_phantom_spans() {
        for raw in [b"".as_slice(), b"\xef\xbb\xbf".as_slice()] {
            let parsed = analyze(raw).unwrap();
            assert_eq!(parsed.text, "");
            assert!(parsed.spans.is_empty());
            assert_eq!(
                parsed.normalized_sha256,
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            );
            assert_complete(&parsed);
        }
    }

    #[test]
    fn blank_unicode_whitespace_is_preserved_in_one_formatting_span() {
        let text = "\n \t\n\u{3000}\n";
        let parsed = analyze(text.as_bytes()).unwrap();
        assert_eq!(parsed.text, text);
        assert_eq!(parsed.spans.len(), 1);
        let span = &parsed.spans[0];
        assert_eq!(span.kind, SpanKind::Formatting);
        assert_eq!((span.start_byte, span.end_byte), (0, 8));
        assert_eq!((span.start_line, span.end_line), (1, 3));
        assert_complete(&parsed);
    }

    #[test]
    fn headings_require_one_to_six_hashes_and_at_most_three_spaces() {
        let text = "#\n## two\n   #### 六\n####### ordinary\n#not heading\n    # indented\n\t# tabbed\n普通\n";
        let parsed = analyze(text.as_bytes()).unwrap();
        assert_eq!(parsed.spans.len(), 4);
        for (index, span) in parsed.spans[..3].iter().enumerate() {
            assert_eq!(span.kind, SpanKind::Heading);
            assert_eq!((span.start_line, span.end_line), (index + 1, index + 1));
        }
        assert_eq!(parsed.spans[3].kind, SpanKind::Text);
        assert_eq!(
            (parsed.spans[3].start_line, parsed.spans[3].end_line),
            (4, 8)
        );
        assert_complete(&parsed);
    }

    #[test]
    fn matching_longer_tilde_fence_keeps_headings_and_other_markers_as_code() {
        let text = "~~~lang\n# not heading\n```wrong\n~~~~\n尾\n";
        let parsed = analyze(text.as_bytes()).unwrap();
        assert_eq!(parsed.spans.len(), 2);
        assert_eq!(parsed.spans[0].kind, SpanKind::Code);
        assert_eq!(parsed.spans[0].parse_status, ParseStatus::Parsed);
        assert_eq!(
            (parsed.spans[0].start_line, parsed.spans[0].end_line),
            (1, 4)
        );
        assert_eq!(parsed.spans[1].kind, SpanKind::Text);
        assert_eq!(
            (parsed.spans[1].start_line, parsed.spans[1].end_line),
            (5, 5)
        );
        assert_complete(&parsed);
    }

    #[test]
    fn short_or_suffixed_closing_fences_do_not_hide_unclosed_code() {
        let text = "````lang\n```\n```` trailing\n~~~\n正文";
        let parsed = analyze(text.as_bytes()).unwrap();
        assert_eq!(parsed.spans.len(), 1);
        let span = &parsed.spans[0];
        assert_eq!(span.kind, SpanKind::Code);
        assert_eq!(span.parse_status, ParseStatus::NeedsReview);
        assert_eq!((span.start_line, span.end_line), (1, 5));
        assert_complete(&parsed);
    }

    #[test]
    fn consecutive_closed_fences_remain_distinct_blocks() {
        let text = "  ```\nx\n   ```` \t\n```\ny\n```\n";
        let parsed = analyze(text.as_bytes()).unwrap();
        assert_eq!(parsed.spans.len(), 2);
        assert_eq!(
            (parsed.spans[0].start_line, parsed.spans[0].end_line),
            (1, 3)
        );
        assert_eq!(
            (parsed.spans[1].start_line, parsed.spans[1].end_line),
            (4, 6)
        );
        assert!(
            parsed
                .spans
                .iter()
                .all(|s| s.kind == SpanKind::Code && s.parse_status == ParseStatus::Parsed)
        );
        assert_complete(&parsed);
    }

    #[test]
    fn deeply_indented_fences_are_text_and_not_code_delimiters() {
        let text = "    ```\n正文\n    ```\n";
        let parsed = analyze(text.as_bytes()).unwrap();
        assert_eq!(parsed.spans.len(), 1);
        assert_eq!(parsed.spans[0].kind, SpanKind::Text);
        assert_eq!(
            (parsed.spans[0].start_line, parsed.spans[0].end_line),
            (1, 3)
        );
        assert_complete(&parsed);
    }

    #[test]
    fn trailing_newlines_do_not_invent_uncovered_or_empty_lines() {
        let cases = [
            ("a", vec![(SpanKind::Text, 1, 1)]),
            ("a\n", vec![(SpanKind::Text, 1, 1)]),
            ("\n", vec![(SpanKind::Formatting, 1, 1)]),
            (
                "a\n\n",
                vec![(SpanKind::Text, 1, 1), (SpanKind::Formatting, 2, 2)],
            ),
            ("# 尾\n", vec![(SpanKind::Heading, 1, 1)]),
            ("```", vec![(SpanKind::Code, 1, 1)]),
        ];
        for (text, expected) in cases {
            let parsed = analyze(text.as_bytes()).unwrap();
            let actual: Vec<_> = parsed
                .spans
                .iter()
                .map(|s| (s.kind, s.start_line, s.end_line))
                .collect();
            assert_eq!(actual, expected, "source {text:?}");
            assert_complete(&parsed);
        }
    }

    #[test]
    fn invalid_utf8_reports_original_byte_offset_without_replacement() {
        for (raw, offset) in [
            (b"a\x80z".as_slice(), 1),
            (b"\xef\xbb\xbf\xff".as_slice(), 3),
            (b"\xe4\xb8".as_slice(), 0),
        ] {
            let error = analyze(raw).unwrap_err();
            assert_eq!(error.valid_up_to(), offset);
        }
    }

    #[test]
    fn enum_names_are_strict_and_all_declared_statuses_round_trip() {
        for kind in [
            SpanKind::Heading,
            SpanKind::Text,
            SpanKind::Code,
            SpanKind::Formatting,
        ] {
            assert_eq!(SpanKind::from_str(kind.as_str()), Some(kind));
            assert_eq!(
                serde_json::to_string(&kind).unwrap(),
                format!("\"{}\"", kind.as_str())
            );
        }
        for status in [
            ParseStatus::Parsed,
            ParseStatus::Failed,
            ParseStatus::NeedsReview,
        ] {
            assert_eq!(ParseStatus::from_str(status.as_str()), Some(status));
            assert_eq!(
                serde_json::to_string(&status).unwrap(),
                format!("\"{}\"", status.as_str())
            );
        }
        assert_eq!(SpanKind::from_str("Heading"), None);
        assert_eq!(ParseStatus::from_str("unknown"), None);
        assert!(serde_json::from_str::<SpanKind>("\"unknown\"").is_err());
        assert!(serde_json::from_str::<ParseStatus>("\"default\"").is_err());
    }
}
