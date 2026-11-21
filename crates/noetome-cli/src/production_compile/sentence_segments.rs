//! UTF-8 sentence-boundary hints for the fixed strategy 5 page splitter.

fn marker(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '。' | '！' | '？' | '…')
}

fn closer(c: char) -> bool {
    matches!(
        c,
        '"' | '\''
            | '”'
            | '’'
            | ')'
            | ']'
            | '}'
            | '）'
            | '】'
            | '》'
            | '〉'
            | '」'
            | '』'
            | '»'
            | '›'
    )
}

fn incomplete_period(text: &str, period: usize) -> bool {
    let prefix = &text[..period];
    let token_start = prefix
        .char_indices()
        .rev()
        .find(|&(_, c)| !(c.is_ascii_alphanumeric() || c == '.'))
        .map_or(0, |(index, c)| index + c.len_utf8());
    let token = &text[token_start..period];
    let abbreviation = [
        "Mr", "Mrs", "Ms", "Dr", "Prof", "St", "Sr", "Jr", "Fig", "Figs", "Eq", "Eqs", "Ref",
        "Refs", "No", "Nos", "Vol", "Ch", "p", "pp", "cf", "vs", "al", "etc",
    ]
    .iter()
    .any(|term| token.eq_ignore_ascii_case(term));
    let initial = token.len() == 1
        && token.as_bytes()[0].is_ascii_uppercase()
        && text[..token_start]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace() || matches!(c, '(' | '[' | '"' | '“'));
    let dotted_abbreviation = token.contains('.')
        && token.split('.').all(|part| {
            (1..=3).contains(&part.len()) && part.bytes().all(|c| c.is_ascii_alphabetic())
        });
    let numbered_heading = !token.is_empty()
        && (token.contains('.') || token.len() <= 2)
        && token.bytes().all(|c| c.is_ascii_digit() || c == b'.')
        && text[..token_start]
            .chars()
            .rev()
            .take_while(|&c| c != '\n')
            .all(char::is_whitespace);
    abbreviation || initial || dotted_abbreviation || numbered_heading
}

/// Prefer the last plausible sentence end whose punctuation and closers fit the window.
/// This is a layout-tolerant hint, not a proof of linguistic completeness.
pub(super) fn preferred_boundary(text: &str, start: usize, hard_end: usize) -> Option<usize> {
    if start >= hard_end || hard_end > text.len() {
        return None;
    }
    text.get(start..hard_end)?;
    let mut stream = text[start..].char_indices().peekable();
    let mut selected = None;
    while let Some((relative, first)) = stream.next() {
        let at = start + relative;
        if at >= hard_end {
            break;
        }
        if !marker(first) {
            continue;
        }
        let mut end = at + first.len_utf8();
        let mut ellipsis = first == '…';
        let mut ascii_only = first.is_ascii();
        let mut exclamation = first == '!';
        let mut single_period = first == '.';
        while let Some(&(relative, c)) = stream.peek().filter(|(_, c)| marker(*c)) {
            if start + relative >= hard_end {
                return selected;
            }
            stream.next();
            ellipsis |= c == '…' || c == '.';
            ascii_only &= c.is_ascii();
            exclamation |= c == '!';
            single_period = false;
            end = start + relative + c.len_utf8();
        }
        while let Some(&(relative, c)) = stream.peek().filter(|(_, c)| closer(*c)) {
            if start + relative >= hard_end {
                return selected;
            }
            stream.next();
            end = start + relative + c.len_utf8();
        }
        let following = &text[end..];
        if ellipsis || (ascii_only && !following.chars().next().is_none_or(char::is_whitespace)) {
            continue;
        }
        if single_period && incomplete_period(text, at) {
            continue;
        }
        if exclamation
            && following.trim_start().starts_with([
                '=', '+', '-', '−', '*', '/', '×', '÷', '<', '>', '≤', '≥', '≠',
            ])
        {
            continue;
        }
        selected = Some(end);
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_last_sentence_and_its_closing_punctuation() {
        let prefix = "One sentence. 第二句在引号里。’”";
        let text = format!("{prefix}第三句仍在继续");
        assert_eq!(
            preferred_boundary(&text, 0, prefix.len()),
            Some(prefix.len())
        );
        let before_closers = prefix.find('’').unwrap();
        assert_eq!(
            preferred_boundary(&text, 0, before_closers),
            Some("One sentence.".len())
        );
    }

    #[test]
    fn ignores_abbreviations_headings_decimals_and_partial_ellipses() {
        let text = "1.1. A. Prof. M.D. Ph.D. said e.g. pH 3.14... n! = 1 is still in the sentence";
        assert_eq!(preferred_boundary(text, 0, text.len() - 3), None);
        let cut_inside_ellipsis = text.find("...").unwrap() + 1;
        assert_eq!(preferred_boundary(text, 0, cut_inside_ellipsis), None);
    }

    #[test]
    fn preserves_line_wraps_and_real_sentence_ends_after_numbers() {
        let text = "This occurred in\n1828. A wrapped sentence\ncontinues across this kind of line";
        assert_eq!(
            preferred_boundary(text, 0, text.len() - 2),
            Some("This occurred in\n1828.".len())
        );
        assert_eq!(
            preferred_boundary(text, 0, text.find("1828").unwrap()),
            None
        );
    }
}
