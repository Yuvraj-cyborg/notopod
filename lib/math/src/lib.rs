//! Equations.
//!
//! [`layout`] typesets LaTeX math with RaTeX — a TeX-faithful layout
//! engine in Rust, with the KaTeX fonts — and [`Formula::draw`] paints the
//! result into a `tiny_skia::Pixmap` with the same rasterizer the drawings
//! use, so an equation is a picture like any other in notopod. Nothing is
//! shelled out and no TeX installation is needed.
//!
//! [`to_unicode`] is the other half: the same LaTeX spelled out in
//! Unicode (`\frac{1}{2}x^2` becomes `½x²`), for terminals that cannot show
//! pictures, for inline `$math$` in a line of text, and for builds without
//! the `typeset` feature.
//!
//! ```
//! assert_eq!(math::to_unicode(r"\alpha^2 + \beta_0"), "α² + β₀");
//! ```

#[cfg(feature = "typeset")]
mod fonts;
mod unicode;

use tiny_skia::Pixmap;

pub use unicode::to_unicode;

/// An RGB colour.
pub type Rgb = (u8, u8, u8);

/// Whether this build typesets formulas (the `typeset` feature). Without
/// it, [`layout`] always fails and callers fall back to [`to_unicode`].
pub const TYPESET: bool = cfg!(feature = "typeset");

/// Longest formula accepted, in bytes. Typesetting is fast, but a note is
/// re-rendered as you type and a pasted wall of LaTeX should not stall it.
pub const MAX_LEN: usize = 4096;

/// A typeset formula, ready to draw at any size.
#[derive(Debug, Clone)]
pub struct Formula {
    #[cfg(feature = "typeset")]
    list: ratex_types::display_item::DisplayList,
    #[cfg(not(feature = "typeset"))]
    _none: (),
}

/// Typesets `tex`, in display style (a block of its own, big operators
/// with limits above and below) or text style (inline).
///
/// Never panics: input that does not parse — including a formula that is
/// still being typed — is an error carrying RaTeX's message.
pub fn layout(tex: &str, display: bool) -> Result<Formula, String> {
    if tex.len() > MAX_LEN {
        return Err(format!("formula longer than {MAX_LEN} bytes"));
    }
    if tex.trim().is_empty() {
        return Err("empty formula".to_owned());
    }
    typeset::layout(tex, display)
}

impl Formula {
    /// Width, in em.
    pub fn width(&self) -> f32 {
        typeset::width(self)
    }

    /// Height above the baseline, in em.
    pub fn ascent(&self) -> f32 {
        typeset::ascent(self)
    }

    /// Ascent plus depth below the baseline, in em.
    pub fn height(&self) -> f32 {
        typeset::height(self)
    }

    /// Paints the formula with its top-left corner at `(left, top)` pixels,
    /// one em being `em` pixels. Anything without an explicit `\color` is
    /// drawn in `color`.
    pub fn draw(&self, pixmap: &mut Pixmap, left: f32, top: f32, em: f32, color: Rgb) {
        typeset::draw(self, pixmap, left, top, em, color);
    }
}

/// `tex` typeset and drawn on a transparent pixmap exactly as large as the
/// formula plus `pad` pixels all round, or `None` when it does not parse.
pub fn render(tex: &str, display: bool, em: f32, pad: f32, color: Rgb) -> Option<Pixmap> {
    let formula = layout(tex, display).ok()?;
    let w = (formula.width() * em + 2.0 * pad).ceil().max(1.0) as u32;
    let h = (formula.height() * em + 2.0 * pad).ceil().max(1.0) as u32;
    let mut pixmap = Pixmap::new(w, h)?;
    formula.draw(&mut pixmap, pad, pad, em, color);
    Some(pixmap)
}

#[cfg(feature = "typeset")]
mod typeset {
    use std::collections::HashMap;

    use ab_glyph::{Font, FontRef, OutlineCurve, Point as GlyphPoint};
    use ratex_layout::{layout as tex_layout, to_display_list, validate_layout, LayoutOptions};
    use ratex_types::color::Color;
    use ratex_types::display_item::DisplayItem;
    use ratex_types::math_style::MathStyle;
    use ratex_types::path_command::PathCommand;
    use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

    use super::{Formula, Rgb};

    /// The colour RaTeX is told to use by default. Items still carrying it
    /// are drawn in the caller's colour; anything else came from `\color`.
    const DEFAULT: Color = Color::BLACK;

    pub(super) fn layout(tex: &str, display: bool) -> Result<Formula, String> {
        let ast = ratex_parser::parser::parse(tex).map_err(|e| e.to_string())?;
        let options = LayoutOptions {
            style: if display {
                MathStyle::Display
            } else {
                MathStyle::Text
            },
            color: DEFAULT,
            ..LayoutOptions::default()
        };
        let root = tex_layout(&ast, &options);
        validate_layout(&root)?;
        let list = to_display_list(&root);
        if list.items.is_empty() {
            return Err("nothing to draw".to_owned());
        }
        Ok(Formula { list })
    }

    pub(super) fn width(formula: &Formula) -> f32 {
        formula.list.width as f32
    }

    pub(super) fn ascent(formula: &Formula) -> f32 {
        formula.list.height as f32
    }

    pub(super) fn height(formula: &Formula) -> f32 {
        (formula.list.height + formula.list.depth) as f32
    }

    fn paint(color: Color, fallback: Rgb) -> Paint<'static> {
        let mut paint = Paint::default();
        if color == DEFAULT {
            paint.set_color_rgba8(fallback.0, fallback.1, fallback.2, 0xff);
        } else {
            let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            paint.set_color_rgba8(
                channel(color.r),
                channel(color.g),
                channel(color.b),
                channel(color.a),
            );
        }
        paint.anti_alias = true;
        paint
    }

    fn fill(pixmap: &mut Pixmap, path: &tiny_skia::Path, paint: &Paint<'_>) {
        pixmap.fill_path(path, paint, FillRule::Winding, Transform::identity(), None);
    }

    pub(super) fn draw(
        formula: &Formula,
        pixmap: &mut Pixmap,
        left: f32,
        top: f32,
        em: f32,
        color: Rgb,
    ) {
        // Fonts are parsed once per draw, and only the ones the formula
        // uses; the bytes themselves are static.
        let mut fonts: HashMap<&str, Option<FontRef<'static>>> = HashMap::new();
        let at = |ix: f64, iy: f64| (left + ix as f32 * em, top + iy as f32 * em);

        for item in &formula.list.items {
            match item {
                DisplayItem::GlyphPath {
                    x,
                    y,
                    scale,
                    font,
                    char_code,
                    color: ink,
                } => {
                    let face = fonts.entry(font.as_str()).or_insert_with(|| load(font));
                    let (Some(face), Some(ch)) = (face.as_ref(), char::from_u32(*char_code)) else {
                        continue;
                    };
                    let (ox, oy) = at(*x, *y);
                    if let Some(path) = glyph(face, ch, ox, oy, em * *scale as f32) {
                        fill(pixmap, &path, &paint(*ink, color));
                    }
                }
                DisplayItem::Line {
                    x,
                    y,
                    width,
                    thickness,
                    color: ink,
                    ..
                } => {
                    let (ox, oy) = at(*x, *y);
                    let t = (*thickness as f32 * em).max(1.0);
                    if let Some(r) = Rect::from_xywh(ox, oy - t / 2.0, *width as f32 * em, t) {
                        fill(pixmap, &PathBuilder::from_rect(r), &paint(*ink, color));
                    }
                }
                DisplayItem::Rect {
                    x,
                    y,
                    width,
                    height,
                    color: ink,
                } => {
                    let (ox, oy) = at(*x, *y);
                    if let Some(r) =
                        Rect::from_xywh(ox, oy, *width as f32 * em, *height as f32 * em)
                    {
                        fill(pixmap, &PathBuilder::from_rect(r), &paint(*ink, color));
                    }
                }
                DisplayItem::Path {
                    x,
                    y,
                    commands,
                    fill: filled,
                    color: ink,
                } => {
                    let (ox, oy) = at(*x, *y);
                    let p = paint(*ink, color);
                    // Each sub-path on its own: KaTeX assembles stretchy
                    // arrows from pieces whose windings would cancel.
                    for part in subpaths(commands) {
                        let Some(path) = outline(part, ox, oy, em) else {
                            continue;
                        };
                        if *filled {
                            fill(pixmap, &path, &p);
                        } else {
                            let stroke = Stroke {
                                width: (em / 24.0).max(1.0),
                                ..Stroke::default()
                            };
                            pixmap.stroke_path(&path, &p, &stroke, Transform::identity(), None);
                        }
                    }
                }
            }
        }
    }

    fn load(font: &str) -> Option<FontRef<'static>> {
        FontRef::try_from_slice(crate::fonts::bytes(font)?).ok()
    }

    /// The outline of `ch` as a path, baseline-left at `(ox, oy)`.
    fn glyph(face: &FontRef<'_>, ch: char, ox: f32, oy: f32, size: f32) -> Option<tiny_skia::Path> {
        let id = face.glyph_id(ch);
        if id.0 == 0 {
            return None;
        }
        let outline = face.outline(id)?;
        let scale = size / face.units_per_em().unwrap_or(1000.0);
        let to = |p: GlyphPoint| (ox + p.x * scale, oy - p.y * scale);
        let mut pb = PathBuilder::new();
        let mut last: Option<GlyphPoint> = None;
        for curve in &outline.curves {
            let (start, end) = match *curve {
                OutlineCurve::Line(a, b)
                | OutlineCurve::Quad(a, _, b)
                | OutlineCurve::Cubic(a, _, _, b) => (a, b),
            };
            let joined =
                last.is_some_and(|l| (l.x - start.x).abs() < 0.01 && (l.y - start.y).abs() < 0.01);
            if !joined {
                if last.is_some() {
                    pb.close();
                }
                let (sx, sy) = to(start);
                pb.move_to(sx, sy);
            }
            match *curve {
                OutlineCurve::Line(_, to_pt) => {
                    let (px, py) = to(to_pt);
                    pb.line_to(px, py);
                }
                OutlineCurve::Quad(_, ctrl, to_pt) => {
                    let ((cx, cy), (px, py)) = (to(ctrl), to(to_pt));
                    pb.quad_to(cx, cy, px, py);
                }
                OutlineCurve::Cubic(_, ctrl1, ctrl2, to_pt) => {
                    let ((c1x, c1y), (c2x, c2y), (px, py)) = (to(ctrl1), to(ctrl2), to(to_pt));
                    pb.cubic_to(c1x, c1y, c2x, c2y, px, py);
                }
            }
            last = Some(end);
        }
        pb.close();
        pb.finish()
    }

    fn subpaths(commands: &[PathCommand]) -> impl Iterator<Item = &[PathCommand]> {
        let mut starts: Vec<usize> = commands
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c, PathCommand::MoveTo { .. }))
            .map(|(i, _)| i)
            .collect();
        if starts.first() != Some(&0) {
            starts.insert(0, 0);
        }
        let ends: Vec<usize> = starts
            .iter()
            .skip(1)
            .copied()
            .chain([commands.len()])
            .collect();
        starts
            .into_iter()
            .zip(ends)
            .filter(|(from, to)| from < to)
            .map(move |(from, to)| &commands[from..to])
    }

    fn outline(commands: &[PathCommand], ox: f32, oy: f32, em: f32) -> Option<tiny_skia::Path> {
        let sx = |v: f64| ox + v as f32 * em;
        let sy = |v: f64| oy + v as f32 * em;
        let mut pb = PathBuilder::new();
        for cmd in commands {
            match *cmd {
                PathCommand::MoveTo { x, y } => pb.move_to(sx(x), sy(y)),
                PathCommand::LineTo { x, y } => pb.line_to(sx(x), sy(y)),
                PathCommand::QuadTo { x1, y1, x, y } => {
                    pb.quad_to(sx(x1), sy(y1), sx(x), sy(y));
                }
                PathCommand::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => pb.cubic_to(sx(x1), sy(y1), sx(x2), sy(y2), sx(x), sy(y)),
                PathCommand::Close => pb.close(),
            }
        }
        pb.finish()
    }
}

#[cfg(not(feature = "typeset"))]
mod typeset {
    use tiny_skia::Pixmap;

    use super::{Formula, Rgb};

    pub(super) fn layout(_tex: &str, _display: bool) -> Result<Formula, String> {
        Err("this build of notopod cannot typeset formulas".to_owned())
    }

    pub(super) fn width(_formula: &Formula) -> f32 {
        0.0
    }

    pub(super) fn ascent(_formula: &Formula) -> f32 {
        0.0
    }

    pub(super) fn height(_formula: &Formula) -> f32 {
        0.0
    }

    pub(super) fn draw(_f: &Formula, _p: &mut Pixmap, _l: f32, _t: f32, _em: f32, _c: Rgb) {}
}

#[cfg(all(test, feature = "typeset"))]
mod tests {
    use super::*;

    fn ink(p: &Pixmap) -> usize {
        p.pixels().iter().filter(|px| px.alpha() > 0).count()
    }

    #[test]
    fn typesets_and_draws_a_real_formula() {
        let tex = r"x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}";
        let f = layout(tex, true).expect("parses");
        assert!(f.width() > 5.0, "a wide formula: {}", f.width());
        assert!(
            f.height() > f.ascent(),
            "a fraction reaches below the baseline"
        );
        let p = render(tex, true, 40.0, 4.0, (255, 255, 255)).expect("renders");
        assert!(ink(&p) > 500, "something was drawn");
        assert!(p.width() > p.height(), "wider than tall");
    }

    #[test]
    fn glyphs_are_drawn_not_just_rules() {
        // A lone letter has no fraction bar or radical to fall back on: if
        // it has ink, the fonts loaded.
        let p = render("x", false, 40.0, 0.0, (255, 255, 255)).expect("renders");
        assert!(ink(&p) > 100, "the letter itself was drawn");
        for font in [
            "Main-Regular",
            "Math-Italic",
            "AMS-Regular",
            "Size4-Regular",
        ] {
            assert!(crate::fonts::bytes(font).is_some(), "{font} is compiled in");
        }
    }

    #[test]
    fn display_style_is_taller_than_text_style() {
        let display = layout(r"\sum_{n=1}^{\infty} \frac{1}{n^2}", true).expect("parses");
        let inline = layout(r"\sum_{n=1}^{\infty} \frac{1}{n^2}", false).expect("parses");
        assert!(display.height() > inline.height());
    }

    #[test]
    fn colour_commands_win_over_the_default() {
        let p = render(r"\color{red}{x}", true, 40.0, 2.0, (0, 0, 255)).expect("renders");
        let red = p
            .pixels()
            .iter()
            .filter(|px| px.alpha() == 255 && px.red() > 200 && px.blue() < 50)
            .count();
        assert!(red > 20, "drawn in red, not the default blue");
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        for tex in [
            "",
            "   ",
            r"\frac{",
            "{{{{",
            r"\begin{matrix}",
            r"\undefinedcommand",
            "^",
        ] {
            let _ = layout(tex, true);
        }
        assert!(layout("", true).is_err());
        assert!(layout(&"x".repeat(MAX_LEN + 1), true).is_err());
    }
}
