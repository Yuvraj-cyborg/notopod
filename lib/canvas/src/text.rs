//! Labels in a hand-drawn font.
//!
//! The font is Excalifont, the typeface Excalidraw draws with, bundled
//! into the binary (SIL Open Font License 1.1; see `fonts/` in this crate).

use std::sync::OnceLock;

use ab_glyph::{Font, FontRef, Glyph, PxScale, ScaleFont};
use tiny_skia::{Pixmap, PremultipliedColorU8};

static FONT_BYTES: &[u8] = include_bytes!("../fonts/Excalifont-Regular.ttf");

fn font() -> &'static FontRef<'static> {
    static FONT: OnceLock<FontRef<'static>> = OnceLock::new();
    FONT.get_or_init(|| FontRef::try_from_slice(FONT_BYTES).expect("the bundled font parses"))
}

/// Text laid out on a baseline starting at `(0, 0)`.
#[derive(Debug, Clone)]
pub struct Layout {
    glyphs: Vec<Glyph>,
    /// Total advance in pixels.
    pub width: f32,
    /// Distance from the baseline up to the top of the tallest glyph.
    pub ascent: f32,
    /// Distance from the baseline down to the lowest descender (positive).
    pub descent: f32,
}

impl Layout {
    /// Height of the line box.
    pub fn height(&self) -> f32 {
        self.ascent + self.descent
    }
}

/// Lays out one line of text at `size` pixels (the em size).
pub fn layout(text: &str, size: f32) -> Layout {
    let scaled = font().as_scaled(PxScale::from(size.max(1.0)));
    let mut glyphs = Vec::with_capacity(text.len());
    let mut x = 0.0f32;
    let mut prev = None;
    for ch in text.chars() {
        if ch.is_control() {
            continue;
        }
        let id = scaled.glyph_id(ch);
        if let Some(p) = prev {
            x += scaled.kern(p, id);
        }
        glyphs.push(id.with_scale_and_position(scaled.scale(), ab_glyph::point(x, 0.0)));
        x += scaled.h_advance(id);
        prev = Some(id);
    }
    Layout {
        glyphs,
        width: x,
        ascent: scaled.ascent(),
        descent: -scaled.descent(),
    }
}

/// Width of `text` at `size` pixels, without laying it out for drawing.
pub fn width(text: &str, size: f32) -> f32 {
    layout(text, size).width
}

/// Paints `layout` with its baseline origin at `(x, y)`.
pub fn draw(pixmap: &mut Pixmap, layout: &Layout, x: f32, y: f32, color: (u8, u8, u8), alpha: f32) {
    let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
    let alpha = alpha.clamp(0.0, 1.0);
    for glyph in &layout.glyphs {
        let mut positioned = glyph.clone();
        positioned.position = ab_glyph::point(positioned.position.x + x, positioned.position.y + y);
        let Some(outlined) = font().outline_glyph(positioned) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        let (ox, oy) = (bounds.min.x.floor() as i32, bounds.min.y.floor() as i32);
        let pixels = pixmap.pixels_mut();
        outlined.draw(|gx, gy, coverage| {
            let (px, py) = (ox + gx as i32, oy + gy as i32);
            if px < 0 || py < 0 || px >= width || py >= height {
                return;
            }
            let cover = (coverage * alpha).clamp(0.0, 1.0);
            if cover <= 0.0 {
                return;
            }
            let dst = &mut pixels[(py * width + px) as usize];
            *dst = over(*dst, color, cover);
        });
    }
}

/// Source-over of a straight-alpha colour onto a premultiplied pixel.
fn over(dst: PremultipliedColorU8, color: (u8, u8, u8), a: f32) -> PremultipliedColorU8 {
    let ch = |src: u8, d: u8| -> u8 {
        let s = f32::from(src) * a;
        let d = f32::from(d) * (1.0 - a);
        (s + d).round().clamp(0.0, 255.0) as u8
    };
    let alpha = (a * 255.0 + f32::from(dst.alpha()) * (1.0 - a))
        .round()
        .clamp(0.0, 255.0) as u8;
    let r = ch(color.0, dst.red()).min(alpha);
    let g = ch(color.1, dst.green()).min(alpha);
    let b = ch(color.2, dst.blue()).min(alpha);
    PremultipliedColorU8::from_rgba(r, g, b, alpha).unwrap_or(dst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_loads_and_lays_out() {
        let l = layout("Parser", 20.0);
        assert!(l.width > 30.0 && l.width < 120.0, "width {}", l.width);
        assert!(l.ascent > 10.0);
        assert!(l.descent > 0.0);
        assert!(width("iii", 20.0) < width("mmm", 20.0));
        assert!(width("", 20.0).abs() < f32::EPSILON);
    }

    #[test]
    fn draws_something_inside_the_pixmap() {
        let mut p = Pixmap::new(120, 40).unwrap();
        let l = layout("hello", 20.0);
        draw(&mut p, &l, 4.0, 28.0, (255, 255, 255), 1.0);
        let painted = p.pixels().iter().filter(|c| c.alpha() > 0).count();
        assert!(painted > 50, "painted {painted}");
        // Off-canvas text is clipped, not a panic.
        draw(&mut p, &l, -500.0, 500.0, (255, 255, 255), 1.0);
    }
}
