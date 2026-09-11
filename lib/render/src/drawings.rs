//! How ```` ```draw ```` blocks get onto the screen.
//!
//! The renderer knows nothing about terminals; it asks a [`Drawings`]
//! for the rows a drawing takes. [`Braille`] draws dot art and works
//! everywhere. The `graphics` crate provides one that shows real pictures
//! where the terminal can.

use canvas::{Drawing, Overlay};
use ratatui::text::Line;
use theme::Theme;

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
