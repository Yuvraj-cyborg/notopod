//! Turns the buffer + parsed document into screen rows.
//!
//! Every top-level block (and every list item) is a *segment*. A segment
//! that does not contain the cursor is rendered; one that does is shown as
//! raw source so the cursor has real characters to sit on. Lines outside
//! any block (blank lines) are always raw.

use editor::Editor;
use ratatui::text::Line;
use render::{render_block, render_list_item, Theme};
use syntax::{BlockKind, Document, LineIndex};
use unicode_width::UnicodeWidthChar;

/// Screen columns a tab occupies in raw source.
const TAB_WIDTH: usize = 4;

/// Where a screen row comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Raw source: buffer `line`, characters `start_col..end_col`.
    Raw {
        /// Buffer line.
        line: usize,
        /// First character of this row.
        start_col: usize,
        /// One past the last character of this row.
        end_col: usize,
    },
    /// Rendered output of the block covering buffer lines `first..=last`.
    Rendered {
        /// First buffer line of the block.
        first: usize,
        /// Last buffer line of the block.
        last: usize,
    },
}

/// One screen row.
#[derive(Debug)]
pub struct Row {
    /// Styled content.
    pub line: Line<'static>,
    /// Which buffer text the row shows.
    pub source: Source,
}

/// All rows for the current buffer plus where the cursor lands.
#[derive(Debug)]
pub struct View {
    /// Rows in order. Never empty.
    pub rows: Vec<Row>,
    /// `(row index, x column)` of the cursor.
    pub cursor: (usize, usize),
}

struct Segment {
    first: usize,
    last: usize,
    target: Target,
}

#[derive(Clone, Copy)]
enum Target {
    Block(usize),
    Item { block: usize, item: usize },
}

/// Lays out the buffer at `width` columns.
pub fn build(
    editor: &Editor,
    doc: &Document,
    index: &LineIndex,
    width: usize,
    preview: bool,
    theme: &Theme,
) -> View {
    let width = width.max(1);
    let segments = if preview {
        segments(doc, index)
    } else {
        Vec::new()
    };
    let cursor = editor.cursor();
    let line_count = editor.len_lines();

    let mut rows: Vec<Row> = Vec::new();
    let mut cursor_pos = (0, 0);
    let mut seg_idx = 0;
    let mut line = 0;

    while line < line_count {
        while seg_idx < segments.len() && segments[seg_idx].first < line {
            seg_idx += 1;
        }
        if let Some(seg) = segments.get(seg_idx).filter(|s| s.first == line) {
            let last = seg.last.min(line_count - 1);
            let has_cursor = (seg.first..=last).contains(&cursor.line);
            if !has_cursor {
                let source = Source::Rendered {
                    first: seg.first,
                    last,
                };
                let rendered = render_target(doc, seg.target, width, theme);
                if rendered.is_empty() {
                    rows.push(Row {
                        line: Line::default(),
                        source,
                    });
                }
                rows.extend(rendered.into_iter().map(|line| Row { line, source }));
                line = last + 1;
                seg_idx += 1;
                continue;
            }
        }

        let text = editor.line(line);
        let chunks = wrap_raw(&text, width);
        let first_row = rows.len();
        for chunk in &chunks {
            rows.push(Row {
                line: Line::from(chunk.text.clone()),
                source: Source::Raw {
                    line,
                    start_col: chunk.start_col,
                    end_col: chunk.end_col,
                },
            });
        }
        if line == cursor.line {
            let (r, x) = raw_cursor(&text, &chunks, cursor.col, width);
            if first_row + r >= rows.len() {
                let len = text.chars().count();
                rows.push(Row {
                    line: Line::default(),
                    source: Source::Raw {
                        line,
                        start_col: len,
                        end_col: len,
                    },
                });
            }
            cursor_pos = (first_row + r, x);
        }
        line += 1;
    }

    if rows.is_empty() {
        rows.push(Row {
            line: Line::default(),
            source: Source::Raw {
                line: 0,
                start_col: 0,
                end_col: 0,
            },
        });
    }

    View {
        rows,
        cursor: cursor_pos,
    }
}

fn segments(doc: &Document, index: &LineIndex) -> Vec<Segment> {
    let mut out = Vec::new();
    for (bi, block) in doc.blocks.iter().enumerate() {
        if let BlockKind::List { items, .. } = &block.kind {
            for (ii, item) in items.iter().enumerate() {
                let (first, last) = index.lines_of(&item.span);
                out.push(Segment {
                    first,
                    last,
                    target: Target::Item {
                        block: bi,
                        item: ii,
                    },
                });
            }
        } else {
            let (first, last) = index.lines_of(&block.span);
            out.push(Segment {
                first,
                last,
                target: Target::Block(bi),
            });
        }
    }
    out
}

fn render_target(
    doc: &Document,
    target: Target,
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    match target {
        Target::Block(bi) => render_block(&doc.blocks[bi], width, theme),
        Target::Item { block, item } => match &doc.blocks[block].kind {
            BlockKind::List { start, items } => {
                render_list_item(*start, item, &items[item], width, theme)
            }
            _ => Vec::new(),
        },
    }
}

/// One wrapped piece of a raw line.
#[derive(Debug, PartialEq, Eq)]
pub struct Chunk {
    /// Display text (tabs expanded).
    pub text: String,
    /// First character index in the line.
    pub start_col: usize,
    /// One past the last character index.
    pub end_col: usize,
}

/// Screen width of one character of raw source.
pub fn char_width(ch: char) -> usize {
    if ch == '\t' {
        TAB_WIDTH
    } else if ch.is_control() {
        1
    } else {
        ch.width().unwrap_or(0)
    }
}

/// Wraps a raw line at character boundaries. Always returns at least one chunk.
pub fn wrap_raw(text: &str, width: usize) -> Vec<Chunk> {
    let width = width.max(1);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_w = 0;
    let mut start = 0;
    let mut count = 0;

    for (i, ch) in text.chars().enumerate() {
        let w = char_width(ch);
        if current_w + w > width && !current.is_empty() {
            chunks.push(Chunk {
                text: std::mem::take(&mut current),
                start_col: start,
                end_col: i,
            });
            start = i;
            current_w = 0;
        }
        match ch {
            '\t' => current.push_str(&" ".repeat(TAB_WIDTH)),
            c if c.is_control() => current.push('·'),
            c => current.push(c),
        }
        current_w += w;
        count = i + 1;
    }
    chunks.push(Chunk {
        text: current,
        start_col: start,
        end_col: count,
    });
    chunks
}

/// Maps a character column to `(row offset, x)` within the wrapped chunks.
fn raw_cursor(text: &str, chunks: &[Chunk], col: usize, width: usize) -> (usize, usize) {
    let row = chunks.iter().rposition(|c| c.start_col <= col).unwrap_or(0);
    let start = chunks[row].start_col;
    let x: usize = text
        .chars()
        .skip(start)
        .take(col.saturating_sub(start))
        .map(char_width)
        .sum();
    if x >= width {
        (row + 1, 0)
    } else {
        (row, x)
    }
}

/// Character column in `text` that sits at screen column `x`, starting the
/// count at character `start_col`.
pub fn col_at_x(text: &str, start_col: usize, x: usize) -> usize {
    let mut col = start_col;
    let mut w = 0;
    for ch in text.chars().skip(start_col) {
        let cw = char_width(ch);
        if w + cw > x {
            break;
        }
        w += cw;
        col += 1;
    }
    col
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::Position;

    fn view_for(text: &str, cursor: Position, width: usize, preview: bool) -> View {
        let mut editor = Editor::from_text(text);
        editor.set_cursor(cursor);
        let src = editor.text();
        let doc = syntax::parse(&src);
        let index = LineIndex::new(&src);
        build(&editor, &doc, &index, width, preview, &Theme::default())
    }

    fn texts(view: &View) -> Vec<String> {
        view.rows.iter().map(|r| r.line.to_string()).collect()
    }

    #[test]
    fn cursor_block_is_raw_others_rendered() {
        let v = view_for("# Title\n\n- a\n- b\n", Position::new(2, 1), 40, true);
        assert_eq!(texts(&v), vec!["# Title", "", "- a", "• b", ""]);
        assert_eq!(v.cursor, (2, 1));
        assert!(matches!(
            v.rows[0].source,
            Source::Rendered { first: 0, last: 0 }
        ));
        assert!(matches!(
            v.rows[3].source,
            Source::Rendered { first: 3, last: 3 }
        ));
    }

    #[test]
    fn preview_off_shows_everything_raw() {
        let v = view_for("# Title\n\n- a\n", Position::new(0, 0), 40, false);
        assert_eq!(texts(&v), vec!["# Title", "", "- a", ""]);
    }

    #[test]
    fn wrapped_raw_line_places_cursor() {
        let v = view_for("abcdefgh", Position::new(0, 6), 4, true);
        assert_eq!(texts(&v), vec!["abcd", "efgh"]);
        assert_eq!(v.cursor, (1, 2));
        let v = view_for("abcdefgh", Position::new(0, 8), 4, true);
        assert_eq!(texts(&v), vec!["abcd", "efgh", ""]);
        assert_eq!(v.cursor, (2, 0));
    }

    #[test]
    fn empty_buffer_has_one_row() {
        let v = view_for("", Position::new(0, 0), 10, true);
        assert_eq!(v.rows.len(), 1);
        assert_eq!(v.cursor, (0, 0));
    }

    #[test]
    fn wrap_raw_tracks_columns() {
        // "ab" (2) + tab (4) does not fit in 5, so the tab starts a new chunk.
        let chunks = wrap_raw("ab\tcd", 5);
        let got: Vec<(&str, usize, usize)> = chunks
            .iter()
            .map(|c| (c.text.as_str(), c.start_col, c.end_col))
            .collect();
        assert_eq!(got, vec![("ab", 0, 2), ("    c", 2, 4), ("d", 4, 5)]);
        assert_eq!(wrap_raw("", 5).len(), 1);
    }

    #[test]
    fn col_at_x_respects_wide_chars() {
        assert_eq!(col_at_x("日本語", 0, 0), 0);
        assert_eq!(col_at_x("日本語", 0, 1), 0);
        assert_eq!(col_at_x("日本語", 0, 2), 1);
        assert_eq!(col_at_x("日本語", 0, 99), 3);
        assert_eq!(col_at_x("abcdef", 3, 2), 5);
    }
}
