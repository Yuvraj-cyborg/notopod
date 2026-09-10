//! Width-aware word wrapping for styled spans.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Word-wraps `spans` to `width` columns, keeping each span's style.
///
/// Words longer than `width` are split at character boundaries. Leading
/// spaces on a wrapped line are dropped. Always returns at least one line.
pub fn wrap_spans(spans: Vec<Span<'static>>, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![Line::from(spans)];
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut line: Vec<Span<'static>> = Vec::new();
    let mut line_width = 0usize;

    let flush = |line: &mut Vec<Span<'static>>, lines: &mut Vec<Line<'static>>| {
        while line
            .last()
            .is_some_and(|s| s.content.chars().all(char::is_whitespace))
        {
            line.pop();
        }
        lines.push(Line::from(std::mem::take(line)));
    };

    for (text, style, is_space) in tokenize(spans) {
        let w = text.width();
        if is_space {
            if line_width == 0 {
                continue;
            }
            if line_width + w > width {
                flush(&mut line, &mut lines);
                line_width = 0;
                continue;
            }
            line.push(Span::styled(text, style));
            line_width += w;
            continue;
        }

        if line_width + w > width {
            if line_width > 0 {
                flush(&mut line, &mut lines);
                line_width = 0;
            }
            if w > width {
                // A single word wider than the line: split it by character.
                let mut piece = String::new();
                let mut piece_w = 0;
                for ch in text.chars() {
                    let cw = ch.width().unwrap_or(0);
                    if piece_w + cw > width && piece_w > 0 {
                        lines.push(Line::from(Span::styled(std::mem::take(&mut piece), style)));
                        piece_w = 0;
                    }
                    piece.push(ch);
                    piece_w += cw;
                }
                line.push(Span::styled(piece, style));
                line_width = piece_w;
                continue;
            }
        }
        line.push(Span::styled(text, style));
        line_width += w;
    }

    if !line.is_empty() || lines.is_empty() {
        flush(&mut line, &mut lines);
    }
    lines
}

/// Splits spans into runs of spaces and runs of non-spaces.
fn tokenize(spans: Vec<Span<'static>>) -> Vec<(String, Style, bool)> {
    let mut tokens = Vec::new();
    for span in spans {
        let style = span.style;
        let mut current = String::new();
        let mut current_is_space = None;
        for ch in span.content.chars() {
            let is_space = ch == ' ' || ch == '\t';
            if current_is_space.is_some_and(|s| s != is_space) {
                tokens.push((std::mem::take(&mut current), style, !is_space));
            }
            current.push(if ch == '\t' { ' ' } else { ch });
            current_is_space = Some(is_space);
        }
        if let Some(is_space) = current_is_space {
            tokens.push((current, style, is_space));
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Line<'static>]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn wraps_at_word_boundaries() {
        let out = wrap_spans(vec![Span::raw("the quick brown fox jumps")], 10);
        assert_eq!(text(&out), vec!["the quick", "brown fox", "jumps"]);
    }

    #[test]
    fn keeps_styles_across_spans() {
        let bold = Style::new().add_modifier(ratatui::style::Modifier::BOLD);
        let out = wrap_spans(
            vec![
                Span::raw("aa "),
                Span::styled("bb cc", bold),
                Span::raw(" dd"),
            ],
            5,
        );
        assert_eq!(text(&out), vec!["aa bb", "cc dd"]);
        assert_eq!(out[0].spans[2].style, bold);
        assert_eq!(out[1].spans[0].style, bold);
    }

    #[test]
    fn splits_long_words() {
        let out = wrap_spans(vec![Span::raw("abcdefghij")], 4);
        assert_eq!(text(&out), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn handles_wide_characters() {
        let out = wrap_spans(vec![Span::raw("日本語 テキスト")], 7);
        assert_eq!(text(&out), vec!["日本語", "テキス", "ト"]);
    }

    #[test]
    fn empty_input_gives_one_empty_line() {
        let out = wrap_spans(vec![], 10);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].width(), 0);
    }
}
