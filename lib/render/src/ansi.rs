//! Converts rendered lines to ANSI escape sequences for plain stdout.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use std::fmt::Write as _;

/// Serialises `lines` as text with ANSI SGR colour codes.
///
/// With `color == false` only the plain text is written.
pub fn to_ansi(lines: &[Line<'_>], color: bool) -> String {
    let mut out = String::new();
    for line in lines {
        for span in &line.spans {
            if color {
                let style = line.style.patch(span.style);
                out.push_str(&sgr(style));
            }
            out.push_str(&span.content);
            if color {
                out.push_str("\x1b[0m");
            }
        }
        out.push('\n');
    }
    out
}

/// Builds the SGR sequence for a style. Empty for the default style.
fn sgr(style: Style) -> String {
    let mut codes: Vec<String> = Vec::new();
    let m = style.add_modifier;
    for (flag, code) in [
        (Modifier::BOLD, 1),
        (Modifier::DIM, 2),
        (Modifier::ITALIC, 3),
        (Modifier::UNDERLINED, 4),
        (Modifier::SLOW_BLINK, 5),
        (Modifier::RAPID_BLINK, 6),
        (Modifier::REVERSED, 7),
        (Modifier::HIDDEN, 8),
        (Modifier::CROSSED_OUT, 9),
    ] {
        if m.contains(flag) {
            codes.push(code.to_string());
        }
    }
    if let Some(fg) = style.fg.and_then(|c| color_code(c, 30)) {
        codes.push(fg);
    }
    if let Some(bg) = style.bg.and_then(|c| color_code(c, 40)) {
        codes.push(bg);
    }
    if codes.is_empty() {
        String::new()
    } else {
        let mut s = String::from("\x1b[");
        let _ = write!(s, "{}", codes.join(";"));
        s.push('m');
        s
    }
}

/// `base` is 30 for foreground, 40 for background.
fn color_code(color: Color, base: u8) -> Option<String> {
    let bright = base + 60;
    let code = match color {
        Color::Reset => return None,
        Color::Black => base.to_string(),
        Color::Red => (base + 1).to_string(),
        Color::Green => (base + 2).to_string(),
        Color::Yellow => (base + 3).to_string(),
        Color::Blue => (base + 4).to_string(),
        Color::Magenta => (base + 5).to_string(),
        Color::Cyan => (base + 6).to_string(),
        Color::Gray => (base + 7).to_string(),
        Color::DarkGray => bright.to_string(),
        Color::LightRed => (bright + 1).to_string(),
        Color::LightGreen => (bright + 2).to_string(),
        Color::LightYellow => (bright + 3).to_string(),
        Color::LightBlue => (bright + 4).to_string(),
        Color::LightMagenta => (bright + 5).to_string(),
        Color::LightCyan => (bright + 6).to_string(),
        Color::White => (bright + 7).to_string(),
        Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", base + 8),
        Color::Indexed(i) => format!("{};5;{i}", base + 8),
    };
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Span;

    #[test]
    fn plain_when_color_off() {
        let lines = vec![Line::from(vec![
            Span::styled("a", Style::new().fg(Color::Red)),
            Span::raw("b"),
        ])];
        assert_eq!(to_ansi(&lines, false), "ab\n");
    }

    #[test]
    fn emits_sgr_codes() {
        let bold_red = Style::new().fg(Color::Red).add_modifier(Modifier::BOLD);
        let lines = vec![Line::from(Span::styled("x", bold_red))];
        assert_eq!(to_ansi(&lines, true), "\x1b[1;31mx\x1b[0m\n");
    }
}
