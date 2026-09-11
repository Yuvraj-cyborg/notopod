//! Block-level rendering.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use syntax::{Alignment, Block, BlockKind, Document, Inline, ListItem};
use theme::Theme;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::drawings::{Braille, Drawings};
use crate::inline::render_inlines;
use crate::wrap::wrap_spans;

/// Bullet characters by nesting depth.
const BULLETS: [&str; 3] = ["•", "◦", "▪"];

/// Renders a whole document with a blank line between blocks. Drawings
/// come out as braille.
pub fn render_document(doc: &Document, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    render_document_with(doc, width, theme, &mut Braille)
}

/// Renders a whole document, drawing ```` ```draw ```` blocks with
/// `drawings`.
pub fn render_document_with(
    doc: &Document,
    width: usize,
    theme: &Theme,
    drawings: &mut dyn Drawings,
) -> Vec<Line<'static>> {
    render_blocks(&doc.blocks, width, theme, 0, true, drawings)
}

/// Renders one block to lines no wider than `width`. Drawings come out as
/// braille.
pub fn render_block(block: &Block, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    render_block_with(block, width, theme, &mut Braille)
}

/// Renders one block, drawing ```` ```draw ```` blocks with `drawings`.
pub fn render_block_with(
    block: &Block,
    width: usize,
    theme: &Theme,
    drawings: &mut dyn Drawings,
) -> Vec<Line<'static>> {
    render_block_at(block, width, theme, 0, drawings)
}

/// Renders a single list item on its own, as it would appear in its list.
///
/// `start` is the list's first number (`None` for bullets) and `index` the
/// item's position in the list, so numbering stays correct.
pub fn render_list_item(
    start: Option<u64>,
    index: usize,
    item: &ListItem,
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    render_list_item_with(start, index, item, width, theme, &mut Braille)
}

/// [`render_list_item`] with a choice of [`Drawings`].
pub fn render_list_item_with(
    start: Option<u64>,
    index: usize,
    item: &ListItem,
    width: usize,
    theme: &Theme,
    drawings: &mut dyn Drawings,
) -> Vec<Line<'static>> {
    list_item(start, index, item, width, theme, 0, drawings)
}

fn render_blocks(
    blocks: &[Block],
    width: usize,
    theme: &Theme,
    depth: usize,
    spaced: bool,
    drawings: &mut dyn Drawings,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for (i, block) in blocks.iter().enumerate() {
        let separate = spaced
            || (i > 0
                && matches!(block.kind, BlockKind::Paragraph(_))
                && matches!(blocks[i - 1].kind, BlockKind::Paragraph(_)));
        if i > 0 && separate {
            out.push(Line::default());
        }
        out.extend(render_block_at(block, width, theme, depth, drawings));
    }
    out
}

fn render_block_at(
    block: &Block,
    width: usize,
    theme: &Theme,
    depth: usize,
    drawings: &mut dyn Drawings,
) -> Vec<Line<'static>> {
    let width = width.max(1);
    match &block.kind {
        BlockKind::Paragraph(inlines) => paragraph(inlines, theme.text, width, theme),
        BlockKind::Heading { level, content } => {
            let level = usize::from(*level).clamp(1, 6);
            let style = theme.heading[level - 1];
            let mut spans = vec![Span::styled(
                format!("{} ", "#".repeat(level)),
                theme.heading_marker,
            )];
            spans.extend(render_inlines(content, style, theme).into_iter().flatten());
            wrap_spans(spans, width)
        }
        BlockKind::CodeBlock { lang, code } if lang.as_deref() == Some(canvas::LANG) => {
            drawings.draw_source(code, theme, width)
        }
        BlockKind::CodeBlock { lang, code } => code_block(lang.as_deref(), code, width, theme),
        BlockKind::BlockQuote(blocks) => {
            let inner = render_blocks(
                blocks,
                width.saturating_sub(2).max(1),
                theme,
                depth,
                true,
                drawings,
            );
            inner
                .into_iter()
                .map(|line| {
                    prefix_line(
                        Span::styled("▎ ", theme.quote_bar),
                        line.patch_style(theme.quote_text),
                    )
                })
                .collect()
        }
        BlockKind::List { start, items } => items
            .iter()
            .enumerate()
            .flat_map(|(i, item)| list_item(*start, i, item, width, theme, depth, drawings))
            .collect(),
        BlockKind::Table {
            alignments,
            header,
            rows,
        } => table(alignments, header, rows, width, theme),
        BlockKind::Rule => vec![Line::from(Span::styled("─".repeat(width), theme.rule))],
        BlockKind::Html(html) => styled_lines(html, theme.html),
        BlockKind::Metadata(text) => {
            let mut out = vec![Line::from(Span::styled("---", theme.metadata))];
            out.extend(styled_lines(text, theme.metadata));
            out.push(Line::from(Span::styled("---", theme.metadata)));
            out
        }
    }
}

fn paragraph(inlines: &[Inline], base: Style, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    render_inlines(inlines, base, theme)
        .into_iter()
        .flat_map(|spans| wrap_spans(spans, width))
        .collect()
}

fn styled_lines(text: &str, style: Style) -> Vec<Line<'static>> {
    text.trim_end_matches('\n')
        .lines()
        .map(|l| Line::from(Span::styled(expand_tabs(l), style)))
        .collect()
}

fn list_item(
    start: Option<u64>,
    index: usize,
    item: &ListItem,
    width: usize,
    theme: &Theme,
    depth: usize,
    drawings: &mut dyn Drawings,
) -> Vec<Line<'static>> {
    let (marker, marker_style) = match (item.checked, start) {
        (Some(true), _) => ("☑ ".to_owned(), theme.task_done),
        (Some(false), _) => ("☐ ".to_owned(), theme.task_todo),
        (None, Some(n)) => (format!("{}. ", n + index as u64), theme.bullet),
        (None, None) => (format!("{} ", BULLETS[depth % BULLETS.len()]), theme.bullet),
    };
    let indent = marker.width();
    let inner_width = width.saturating_sub(indent).max(1);
    let mut inner = render_blocks(&item.blocks, inner_width, theme, depth + 1, false, drawings);
    if item.checked == Some(true) {
        inner = inner
            .into_iter()
            .map(|l| l.patch_style(theme.task_done_text))
            .collect();
    }
    if inner.is_empty() {
        return vec![Line::from(Span::styled(marker, marker_style))];
    }
    inner
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let prefix = if i == 0 {
                Span::styled(marker.clone(), marker_style)
            } else {
                Span::raw(" ".repeat(indent))
            };
            prefix_line(prefix, line)
        })
        .collect()
}

fn code_block(lang: Option<&str>, code: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let body: Vec<String> = code
        .trim_end_matches('\n')
        .lines()
        .map(expand_tabs)
        .collect();
    let body = if code.trim().is_empty() {
        Vec::new()
    } else {
        body
    };

    // Too narrow for a box: just indent the code.
    if width < 8 {
        return body
            .into_iter()
            .map(|l| Line::from(Span::styled(l, theme.code_block)))
            .collect();
    }

    let inner = width - 4;
    let mut out = Vec::new();

    let label = lang.map(|l| format!(" {l} ")).unwrap_or_default();
    let label_w = label.width().min(width - 3);
    let mut top = vec![Span::styled("╭─", theme.code_border)];
    if !label.is_empty() {
        top.push(Span::styled(fit(&label, label_w), theme.code_lang));
    }
    top.push(Span::styled(
        format!("{}╮", "─".repeat(width - 3 - label_w)),
        theme.code_border,
    ));
    out.push(Line::from(top));

    for line in body {
        let text = fit(&line, inner);
        let pad = inner - text.width();
        out.push(Line::from(vec![
            Span::styled("│ ", theme.code_border),
            Span::styled(text, theme.code_block),
            Span::raw(" ".repeat(pad)),
            Span::styled(" │", theme.code_border),
        ]));
    }

    out.push(Line::from(Span::styled(
        format!("╰{}╯", "─".repeat(width - 2)),
        theme.code_border,
    )));
    out
}

fn table(
    alignments: &[Alignment],
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let ncols = header
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if ncols == 0 {
        return Vec::new();
    }

    let cell = |inlines: Option<&Vec<Inline>>, style: Style| -> Vec<Span<'static>> {
        inlines
            .map(|i| {
                let mut spans = Vec::new();
                for (n, line) in render_inlines(i, style, theme).into_iter().enumerate() {
                    if n > 0 {
                        spans.push(Span::styled(" ", style));
                    }
                    spans.extend(line);
                }
                spans
            })
            .unwrap_or_default()
    };

    let head: Vec<Vec<Span<'static>>> = (0..ncols)
        .map(|c| cell(header.get(c), theme.table_header))
        .collect();
    let body: Vec<Vec<Vec<Span<'static>>>> = rows
        .iter()
        .map(|row| (0..ncols).map(|c| cell(row.get(c), theme.text)).collect())
        .collect();

    let mut widths = vec![1usize; ncols];
    for row in std::iter::once(&head).chain(body.iter()) {
        for (c, spans) in row.iter().enumerate() {
            widths[c] = widths[c].max(spans_width(spans));
        }
    }
    // Each column costs its width plus "| " and " "; plus the final "|".
    let total = |w: &[usize]| w.iter().sum::<usize>() + 3 * ncols + 1;
    while total(&widths) > width {
        let Some((widest, _)) = widths.iter().enumerate().max_by_key(|(_, w)| **w) else {
            break;
        };
        if widths[widest] <= 3 {
            break;
        }
        widths[widest] -= 1;
    }

    let border = |left: &str, mid: &str, right: &str| -> Line<'static> {
        let segs: Vec<String> = widths.iter().map(|w| "─".repeat(w + 2)).collect();
        Line::from(Span::styled(
            format!("{left}{}{right}", segs.join(mid)),
            theme.table_border,
        ))
    };
    let row_line = |cells: &[Vec<Span<'static>>]| -> Line<'static> {
        let mut spans = vec![Span::styled("│", theme.table_border)];
        for (c, content) in cells.iter().enumerate() {
            let (content, w) = fit_spans(content, widths[c]);
            let pad = widths[c] - w;
            let (left, right) = match alignments.get(c).copied().unwrap_or(Alignment::None) {
                Alignment::Right => (pad, 0),
                Alignment::Center => (pad / 2, pad - pad / 2),
                Alignment::None | Alignment::Left => (0, pad),
            };
            spans.push(Span::raw(" ".repeat(left + 1)));
            spans.extend(content);
            spans.push(Span::raw(" ".repeat(right + 1)));
            spans.push(Span::styled("│", theme.table_border));
        }
        Line::from(spans)
    };

    let mut out = vec![
        border("┌", "┬", "┐"),
        row_line(&head),
        border("├", "┼", "┤"),
    ];
    out.extend(body.iter().map(|r| row_line(r)));
    out.push(border("└", "┴", "┘"));
    out
}

fn prefix_line(prefix: Span<'static>, line: Line<'static>) -> Line<'static> {
    let style = line.style;
    let mut spans = Vec::with_capacity(line.spans.len() + 1);
    spans.push(prefix);
    spans.extend(line.spans);
    Line::from(spans).style(style)
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|s| s.content.width()).sum()
}

/// Truncates `text` to `width` columns, ending with `…` when cut.
fn fit(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in text.chars() {
        let cw = ch.width().unwrap_or(0);
        if w + cw > width - 1 {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

/// Truncates styled spans to `width` columns. Returns the spans and their width.
fn fit_spans(spans: &[Span<'static>], width: usize) -> (Vec<Span<'static>>, usize) {
    let total = spans_width(spans);
    if total <= width {
        return (spans.to_vec(), total);
    }
    let mut out = Vec::new();
    let mut used = 0;
    let limit = width.saturating_sub(1);
    'outer: for span in spans {
        let mut piece = String::new();
        for ch in span.content.chars() {
            let cw = ch.width().unwrap_or(0);
            if used + cw > limit {
                if !piece.is_empty() {
                    out.push(Span::styled(piece, span.style));
                }
                break 'outer;
            }
            piece.push(ch);
            used += cw;
        }
        out.push(Span::styled(piece, span.style));
    }
    if width > 0 {
        out.push(Span::raw("…"));
        used += 1;
    }
    (out, used)
}

fn expand_tabs(line: &str) -> String {
    line.replace('\t', "    ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(src: &str, width: usize) -> Vec<String> {
        let doc = syntax::parse(src);
        render_document(&doc, width, &Theme::default())
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn heading_and_paragraph() {
        assert_eq!(
            render("# Title\n\nsome text here\n", 9),
            vec!["# Title", "", "some text", "here"]
        );
    }

    #[test]
    fn bullets_and_tasks() {
        assert_eq!(
            render("- a\n- [ ] b\n- [x] c\n  - d\n", 20),
            vec!["• a", "☐ b", "☑ c", "  ◦ d"]
        );
    }

    #[test]
    fn numbered_lists_continue_from_start() {
        assert_eq!(render("3. a\n4. b\n", 20), vec!["3. a", "4. b"]);
    }

    #[test]
    fn list_item_wraps_with_hanging_indent() {
        assert_eq!(render("- one two three\n", 9), vec!["• one two", "  three"]);
    }

    #[test]
    fn code_block_is_boxed() {
        assert_eq!(
            render("```rs\nlet x;\n```\n", 12),
            vec!["╭─ rs ─────╮", "│ let x;   │", "╰──────────╯"]
        );
    }

    #[test]
    fn draw_block_renders_as_a_drawing() {
        let lines = render("```draw\nrect 0,0 6x3 \"hi\"\n```\n", 40);
        assert_eq!(lines.len(), 4);
        assert!(!lines[0].starts_with('╭'), "no code box around a drawing");
        assert!(lines[1].contains("hi"));
    }

    #[test]
    fn code_block_truncates_long_lines() {
        assert_eq!(
            render("```\nabcdefghijklmnop\n```\n", 10),
            vec!["╭────────╮", "│ abcde… │", "╰────────╯"]
        );
    }

    #[test]
    fn quote_gets_bar() {
        assert_eq!(render("> hi\n> there\n", 20), vec!["▎ hi there"]);
    }

    #[test]
    fn table_is_drawn_with_borders() {
        assert_eq!(
            render("| a | bb |\n|---|---:|\n| 1 | 2 |\n", 40),
            vec![
                "┌───┬────┐",
                "│ a │ bb │",
                "├───┼────┤",
                "│ 1 │  2 │",
                "└───┴────┘",
            ]
        );
    }

    #[test]
    fn rule_spans_width() {
        assert_eq!(render("---\n", 5), vec!["─────"]);
    }

    #[test]
    fn fit_truncates_with_ellipsis() {
        assert_eq!(fit("abcdef", 4), "abc…");
        assert_eq!(fit("abc", 4), "abc");
        assert_eq!(fit("日本語", 4), "日…");
    }
}
