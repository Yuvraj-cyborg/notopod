//! How ```` ```draw ```` blocks and equations get onto the screen.
//!
//! The renderer knows nothing about terminals; it asks a [`Drawings`]
//! for the rows a drawing or a display equation takes. [`Braille`] draws
//! dot art and spells equations out in Unicode, and works everywhere. The
//! `graphics` crate provides one that shows real pictures where the
//! terminal can.

use canvas::{Drawing, Overlay};
use ratatui::text::{Line, Span};
use theme::Theme;

use crate::wrap::wrap_spans;

/// Turns a drawing into screen rows.
pub trait Drawings {
    /// Rows for `drawing`, at most `max_width` columns wide, with the
    /// editing overlay drawn on when given.
    fn draw(
        &mut self,
        drawing: &Drawing,
        theme: &Theme,
        max_width: usize,
        overlay: Option<&Overlay<'_>>,
    ) -> Vec<Line<'static>>;

    /// Parses and draws the text of a block. Nothing for a blank block.
    fn draw_source(&mut self, src: &str, theme: &Theme, max_width: usize) -> Vec<Line<'static>> {
        let drawing = canvas::parse(src);
        if drawing.is_blank() && drawing.size.is_none() {
            return Vec::new();
        }
        self.draw(&drawing, theme, max_width, None)
    }

    /// Rows for a display equation, centred in `max_width` columns. The
    /// default spells it out in Unicode.
    fn math(&mut self, tex: &str, theme: &Theme, max_width: usize) -> Vec<Line<'static>> {
        math_text(tex, theme, max_width)
    }

    /// Rows for a ```` ```manim ```` block. The default shows its code.
    fn manim(&mut self, src: &str, theme: &Theme, max_width: usize) -> Vec<Line<'static>> {
        crate::block::manim_text(src, MANIM_OFF, max_width, theme)
    }

    /// Rows shown under the ```` ```manim ```` block the cursor is in,
    /// whose source is on screen for typing: the scene as it stands.
    /// `block` is the block's first line, so a frame shown for one block
    /// is not mistaken for another's. The default shows nothing.
    fn manim_live(
        &mut self,
        block: usize,
        src: &str,
        theme: &Theme,
        max_width: usize,
    ) -> Vec<Line<'static>> {
        let _ = (block, src, theme, max_width);
        Vec::new()
    }
}

/// What a ```` ```manim ```` block says when rendering it is off.
pub const MANIM_OFF: &str = "manim: off ([manim] enabled = false in the config)";

/// A display equation as centred lines of Unicode, for where it cannot be
/// a picture. Nothing for an empty one.
pub fn math_text(tex: &str, theme: &Theme, max_width: usize) -> Vec<Line<'static>> {
    let text = math::to_unicode(tex);
    if text.is_empty() {
        return Vec::new();
    }
    let width = max_width.max(1);
    wrap_spans(vec![Span::styled(text, theme.math)], width)
        .into_iter()
        .map(|line| {
            let pad = width.saturating_sub(line.width()) / 2;
            if pad == 0 {
                return line;
            }
            let mut spans = vec![Span::raw(" ".repeat(pad))];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

/// Drawings as braille dot art, 2×4 dots per cell.
#[derive(Debug, Clone, Copy, Default)]
pub struct Braille;

impl Drawings for Braille {
    fn draw(
        &mut self,
        drawing: &Drawing,
        theme: &Theme,
        max_width: usize,
        overlay: Option<&Overlay<'_>>,
    ) -> Vec<Line<'static>> {
        canvas::render(drawing, theme, max_width, overlay)
    }
}
