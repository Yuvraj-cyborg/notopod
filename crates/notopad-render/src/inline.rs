//! Inline content to styled spans.

use notopad_core::Inline;
use ratatui::style::Style;
use ratatui::text::Span;

use crate::Theme;

/// Renders inline content to spans, starting from `base` style.
///
/// Returns one `Vec<Span>` per line: a paragraph with no hard breaks gives
/// exactly one entry. Soft breaks become spaces.
pub fn render_inlines(inlines: &[Inline], base: Style, theme: &Theme) -> Vec<Vec<Span<'static>>> {
    let mut lines = vec![Vec::new()];
    push_inlines(inlines, base, theme, &mut lines);
    lines
}

fn push_inlines(
    inlines: &[Inline],
    style: Style,
    theme: &Theme,
    lines: &mut Vec<Vec<Span<'static>>>,
) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => push(lines, Span::styled(t.clone(), style)),
            Inline::Code(c) => push(lines, Span::styled(c.clone(), style.patch(theme.code))),
            Inline::Emphasis(inner) => {
                push_inlines(inner, style.patch(theme.emphasis), theme, lines);
            }
            Inline::Strong(inner) => push_inlines(inner, style.patch(theme.strong), theme, lines),
            Inline::Strikethrough(inner) => {
                push_inlines(inner, style.patch(theme.strikethrough), theme, lines);
            }
            Inline::Link { content, .. } => {
                push_inlines(content, style.patch(theme.link), theme, lines);
            }
            Inline::Image { alt, .. } => {
                let label = if alt.is_empty() {
                    "[image]".to_owned()
                } else {
                    format!("[image: {alt}]")
                };
                push(lines, Span::styled(label, style.patch(theme.image)));
            }
            Inline::SoftBreak => push(lines, Span::styled(" ", style)),
            Inline::HardBreak => lines.push(Vec::new()),
        }
    }
}

fn push(lines: &mut [Vec<Span<'static>>], span: Span<'static>) {
    if let Some(last) = lines.last_mut() {
        last.push(span);
    }
}
