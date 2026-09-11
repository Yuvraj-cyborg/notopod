//! A dot bitmap that becomes braille characters.
//!
//! Every terminal cell holds a 2×4 grid of dots, so a canvas of `w`×`h`
//! cells is a bitmap of `2w`×`4h` dots. Each cell also carries one colour
//! (the colour of the last shape that touched it) and may be covered by
//! text, which hides the dots underneath.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

/// Dots per cell, horizontally.
pub const DOTS_X: i32 = 2;
/// Dots per cell, vertically.
pub const DOTS_Y: i32 = 4;

/// Braille bit for the dot at `(dx, dy)` inside a cell.
const BITS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

#[derive(Debug, Clone)]
struct Glyph {
    text: String,
    style: Style,
}

/// A canvas of cells, drawn on at dot resolution.
#[derive(Debug, Clone)]
pub struct Raster {
    cells_w: i32,
    cells_h: i32,
    bits: Vec<u8>,
    colors: Vec<Option<Color>>,
    text: Vec<Option<Glyph>>,
}

impl Raster {
    /// A blank canvas of `w`×`h` cells.
    pub fn new(w: i32, h: i32) -> Self {
        let (w, h) = (w.max(1), h.max(1));
        let n = (w * h) as usize;
        Self {
            cells_w: w,
            cells_h: h,
            bits: vec![0; n],
            colors: vec![None; n],
            text: vec![None; n],
        }
    }

    fn index(&self, cx: i32, cy: i32) -> Option<usize> {
        (cx >= 0 && cy >= 0 && cx < self.cells_w && cy < self.cells_h)
            .then(|| (cy * self.cells_w + cx) as usize)
    }

    /// Turns on the dot at `(x, y)` (dot coordinates), colouring its cell.
    ///
    /// A rough stroke along the top or left edge can wander above or left
    /// of the canvas; those dots are pulled onto the edge so the outline
    /// stays closed. Dots past the right or bottom edge are dropped.
    pub fn plot(&mut self, x: i32, y: i32, color: Option<Color>) {
        let (x, y) = (x.max(0), y.max(0));
        let Some(i) = self.index(x / DOTS_X, y / DOTS_Y) else {
            return;
        };
        self.bits[i] |= BITS[(y % DOTS_Y) as usize][(x % DOTS_X) as usize];
        if color.is_some() {
            self.colors[i] = color;
        }
    }

    /// Whether the dot at `(x, y)` is on.
    #[cfg(test)]
    pub fn get(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 {
            return false;
        }
        self.index(x / DOTS_X, y / DOTS_Y)
            .is_some_and(|i| self.bits[i] & BITS[(y % DOTS_Y) as usize][(x % DOTS_X) as usize] != 0)
    }

    /// Writes `text` starting at cell `(cx, cy)`, clipped to the canvas.
    /// Wide characters take two cells. Text hides the dots under it.
    pub fn put_text(&mut self, cx: i32, cy: i32, text: &str, style: Style) {
        let mut x = cx;
        for ch in text.chars() {
            let w = ch.width().unwrap_or(0) as i32;
            if w == 0 {
                continue;
            }
            if x + w > self.cells_w {
                break;
            }
            if let Some(i) = self.index(x, cy) {
                self.text[i] = Some(Glyph {
                    text: ch.to_string(),
                    style,
                });
                if w == 2 {
                    if let Some(j) = self.index(x + 1, cy) {
                        self.text[j] = Some(Glyph {
                            text: String::new(),
                            style,
                        });
                    }
                }
            }
            x += w;
        }
    }

    /// The braille character for a cell, or a space when it is empty.
    pub fn glyph(&self, cx: i32, cy: i32) -> char {
        match self.index(cx, cy) {
            Some(i) if self.bits[i] != 0 => {
                char::from_u32(0x2800 + u32::from(self.bits[i])).unwrap_or(' ')
            }
            _ => ' ',
        }
    }

    /// Converts the canvas to styled lines, one per cell row.
    ///
    /// `stroke` is the style of dots whose cell has no colour of its own;
    /// `cursor` marks one cell with `cursor_style` (patched over whatever
    /// is there).
    pub fn to_lines(
        &self,
        stroke: Style,
        cursor: Option<(i32, i32)>,
        cursor_style: Style,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(self.cells_h as usize);
        for cy in 0..self.cells_h {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut run = String::new();
            let mut run_style = Style::new();
            for cx in 0..self.cells_w {
                let i = (cy * self.cells_w + cx) as usize;
                let (text, mut style) = match &self.text[i] {
                    Some(g) => (g.text.clone(), g.style),
                    None if self.bits[i] != 0 => {
                        let base = match self.colors[i] {
                            Some(c) => stroke.fg(c),
                            None => stroke,
                        };
                        (self.glyph(cx, cy).to_string(), base)
                    }
                    None => (" ".to_owned(), Style::new()),
                };
                if cursor == Some((cx, cy)) {
                    style = style.patch(cursor_style);
                }
                if style != run_style && !run.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut run), run_style));
                }
                run_style = style;
                run.push_str(&text);
            }
            if !run.is_empty() {
                spans.push(Span::styled(run, run_style));
            }
            lines.push(Line::from(spans));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_map_to_braille_bits() {
        let mut r = Raster::new(2, 1);
        r.plot(0, 0, None); // top-left dot of cell 0
        r.plot(3, 3, None); // bottom-right dot of cell 1
        assert_eq!(r.glyph(0, 0), '⠁');
        assert_eq!(r.glyph(1, 0), '⢀' /* U+2880: bit 0x80 */);
        r.plot(2, 3, None); // bottom-left dot of cell 1
        assert_eq!(r.glyph(1, 0), '⣀' /* U+28C0: bits 0x40|0x80 */);
        assert!(r.get(0, 0));
        assert!(!r.get(1, 0));
        // Past the far edges is ignored; before the origin lands on the edge.
        r.plot(100, 100, None);
        assert_eq!(r.glyph(1, 0), '⣀');
        r.plot(-3, 1, None);
        assert!(r.get(0, 1));
    }

    #[test]
    fn full_cell_and_text_and_cursor() {
        let mut r = Raster::new(4, 1);
        for x in 0..2 {
            for y in 0..4 {
                r.plot(x, y, Some(Color::Red));
            }
        }
        assert_eq!(r.glyph(0, 0), '⣿');
        r.put_text(1, 0, "é日", Style::new());
        let lines = r.to_lines(Style::new(), Some((0, 0)), Style::new().bg(Color::Blue));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].to_string(), "⣿é日");
        assert_eq!(
            lines[0].spans[0].style,
            Style::new().fg(Color::Red).bg(Color::Blue)
        );
    }

    #[test]
    fn text_is_clipped_to_the_canvas() {
        let mut r = Raster::new(3, 1);
        r.put_text(1, 0, "abcdef", Style::new());
        assert_eq!(
            r.to_lines(Style::new(), None, Style::new())[0].to_string(),
            " ab"
        );
    }
}
