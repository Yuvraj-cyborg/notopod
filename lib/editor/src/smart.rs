//! Markdown-aware editing helpers.

/// What pressing Enter should do on a given line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Newline {
    /// Insert a plain line break.
    Plain,
    /// Insert a line break followed by this text (indent + list marker).
    Continue(String),
    /// The line is an empty list item: clear it instead of adding another.
    ClearLine,
}

/// Decides how Enter behaves on `line` with the cursor at character `col`.
///
/// Lines that start with a list marker (`- `, `* `, `+ `, `1. `, `1) `,
/// `- [ ] `) or a quote marker (`> `) continue that marker on the next
/// line, the way note apps do. Enter on an empty item ends the list.
pub fn newline_for(line: &str, col: usize) -> Newline {
    let indent_len = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
    let indent: String = line.chars().take(indent_len).collect();
    let rest = &line[indent.len()..];

    let Some((marker_len, next_marker)) = marker(rest) else {
        return Newline::Plain;
    };

    // Cursor inside the marker itself: do nothing clever.
    if col < indent_len + marker_len {
        return Newline::Plain;
    }

    let content = &rest[byte_len(rest, marker_len)..];
    if content.trim().is_empty() && col >= line.chars().count() {
        return Newline::ClearLine;
    }

    Newline::Continue(format!("{indent}{next_marker}"))
}

/// Recognises a list/quote marker at the start of `s`.
/// Returns the marker's length in characters and the marker for the next line.
fn marker(s: &str) -> Option<(usize, String)> {
    // Task items: "- [ ] " / "- [x] " with any bullet character.
    for bullet in ['-', '*', '+'] {
        for boxed in ["[ ] ", "[x] ", "[X] "] {
            let prefix = format!("{bullet} {boxed}");
            if s.starts_with(&prefix) {
                return Some((prefix.chars().count(), format!("{bullet} [ ] ")));
            }
        }
    }
    for bullet in ["- ", "* ", "+ ", "> "] {
        if s.starts_with(bullet) {
            return Some((2, bullet.to_owned()));
        }
    }
    // Numbered: digits then "." or ")" then a space.
    let digits = s.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let after = &s[digits..];
        if let Some(delim) = after.chars().next().filter(|c| *c == '.' || *c == ')') {
            if after[1..].starts_with(' ') {
                let n: u64 = s[..digits].parse().unwrap_or(0);
                return Some((digits + 2, format!("{}{delim} ", n.saturating_add(1))));
            }
        }
    }
    None
}

fn byte_len(s: &str, chars: usize) -> usize {
    s.char_indices().nth(chars).map_or(s.len(), |(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continues_bullets_numbers_tasks_and_quotes() {
        assert_eq!(newline_for("- a", 3), Newline::Continue("- ".into()));
        assert_eq!(newline_for("  * a", 5), Newline::Continue("  * ".into()));
        assert_eq!(newline_for("3. a", 4), Newline::Continue("4. ".into()));
        assert_eq!(newline_for("9) a", 4), Newline::Continue("10) ".into()));
        assert_eq!(
            newline_for("- [x] a", 7),
            Newline::Continue("- [ ] ".into())
        );
        assert_eq!(newline_for("> a", 3), Newline::Continue("> ".into()));
    }

    #[test]
    fn empty_item_ends_the_list() {
        assert_eq!(newline_for("- ", 2), Newline::ClearLine);
        assert_eq!(newline_for("- [ ] ", 6), Newline::ClearLine);
        assert_eq!(newline_for("1. ", 3), Newline::ClearLine);
    }

    #[test]
    fn plain_text_and_cursor_in_marker_are_plain() {
        assert_eq!(newline_for("hello", 5), Newline::Plain);
        assert_eq!(newline_for("- a", 1), Newline::Plain);
        assert_eq!(newline_for("-a", 2), Newline::Plain);
        assert_eq!(newline_for("1.a", 3), Newline::Plain);
    }

    #[test]
    fn splitting_an_item_mid_text_continues() {
        assert_eq!(newline_for("- ab", 3), Newline::Continue("- ".into()));
    }
}
