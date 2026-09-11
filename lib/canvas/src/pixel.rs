//! Rendering a drawing as a picture.
//!
//! Shapes are placed on the same cell grid the editor moves on, but drawn
//! at the terminal's real pixel resolution: a cell is `CellSize` pixels,
//! strokes are anti-aliased curves, labels use a hand-drawn font. The
//! picture has a transparent background so the terminal's own shows
//! through. Terminals that can display images put it exactly over the
//! cells the braille renderer would have used.

use ratatui::style::Color;
use theme::{to_rgb, Rgb, Theme};
use tiny_skia::{Color as SkColor, PathBuilder, Pixmap, Rect as SkRect};

use crate::model::{BoxKind, Drawing, Point, Rect, Shape};
use crate::render::{canvas_size, midpoint, shape_color, Overlay};
use crate::sketch::{Pen, Pt, Sketch};
use crate::text;

/// Pixel size of one terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellSize {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Default for CellSize {
    /// A typical HiDPI cell when the terminal does not say.
    fn default() -> Self {
        Self {
            width: 18,
            height: 38,
        }
    }
}

impl CellSize {
    /// Largest cell we render at, to keep pictures a sane size.
    pub const MAX: u32 = 96;

    /// A cell size, clamped to something drawable.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: width.clamp(4, Self::MAX),
            height: height.clamp(6, Self::MAX),
        }
    }
}

/// A rendered drawing.
#[derive(Debug, Clone)]
pub struct Image {
    pixmap: Pixmap,
    cols: i32,
    rows: i32,
}

impl Image {
    /// Columns the picture covers on screen.
    pub fn cols(&self) -> usize {
        self.cols.max(0) as usize
    }

    /// Rows the picture covers on screen.
    pub fn rows(&self) -> usize {
        self.rows.max(0) as usize
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// The picture as a PNG file. Encoded for speed rather than size:
    /// a drawing is re-encoded on every keystroke while it is edited.
    pub fn to_png(&self) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(self.pixmap.data().len());
        for p in self.pixmap.pixels() {
            let p = p.demultiply();
            rgba.extend_from_slice(&[p.red(), p.green(), p.blue(), p.alpha()]);
        }
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, self.width(), self.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        encoder.set_filter(png::Filter::Sub);
        let ok = encoder
            .write_header()
            .and_then(|mut w| w.write_image_data(&rgba))
            .is_ok();
        if ok {
            out
        } else {
            Vec::new()
        }
    }

    /// Straight-alpha RGBA of one pixel, for inspection.
    pub fn pixel(&self, x: u32, y: u32) -> Option<(u8, u8, u8, u8)> {
        let p = self.pixmap.pixel(x, y)?.demultiply();
        Some((p.red(), p.green(), p.blue(), p.alpha()))
    }
}

/// Renders a drawing at `cell` pixels per cell. `fg` is the colour used
/// where the theme says "the terminal's foreground". Returns `None` for a
/// picture with no area.
pub fn render_image(
    drawing: &Drawing,
    theme: &Theme,
    cell: CellSize,
    fg: Rgb,
    max_width: usize,
    overlay: Option<&Overlay<'_>>,
) -> Option<Image> {
    let (cols, rows) = canvas_size(drawing, overlay, max_width);
    if cols <= 0 || rows <= 0 {
        return None;
    }
    let mut pixmap = Pixmap::new(cols as u32 * cell.width, rows as u32 * cell.height)?;
    let ctx = Ctx::new(theme, cell, fg);

    if overlay.is_some() {
        ctx.grid(&mut pixmap, cols, rows);
    }
    for (i, shape) in drawing.shapes.iter().enumerate() {
        ctx.shape(&mut pixmap, drawing, shape, i as u64, None);
    }
    if let Some(o) = overlay {
        if let Some(sel) = o.selected.and_then(|i| drawing.shapes.get(i)) {
            let color = theme.canvas_selected.fg.or(Some(Color::Yellow));
            ctx.shape(
                &mut pixmap,
                drawing,
                sel,
                o.selected.unwrap_or(0) as u64,
                color,
            );
        }
        if let Some(p) = o.preview {
            let color = theme.canvas_preview.fg.or(Some(Color::Cyan));
            ctx.shape(&mut pixmap, drawing, p, drawing.shapes.len() as u64, color);
        }
        if let Some(c) = o.cursor.filter(|c| c.x >= 0 && c.y >= 0) {
            ctx.cursor(&mut pixmap, c);
        }
    }
    Some(Image { pixmap, cols, rows })
}

/// Everything the shape painters need to know about the picture.
struct Ctx<'t> {
    theme: &'t Theme,
    cw: f32,
    ch: f32,
    /// Pixels per rough.js pixel; also the stroke width.
    unit: f32,
    roughness: f32,
    fg: Rgb,
    /// Em size of labels.
    font: f32,
}

impl<'t> Ctx<'t> {
    fn new(theme: &'t Theme, cell: CellSize, fg: Rgb) -> Self {
        let ch = cell.height as f32;
        Self {
            theme,
            cw: cell.width as f32,
            ch,
            unit: (ch / 13.0).clamp(1.0, 4.5),
            // The theme's 0..=1 maps onto rough.js's scale, where 1 is
            // Excalidraw's "artist" and 2 its "cartoonist".
            roughness: theme.roughness * 1.5,
            fg,
            font: ch * 0.72,
        }
    }

    fn inset(&self) -> f32 {
        self.unit * 1.5
    }

    fn pen(&self, color: Rgb, dashed: bool) -> Pen {
        Pen {
            color: sk(color, 1.0),
            width: self.unit,
            dashed,
        }
    }

    fn rgb(&self, color: Option<Color>) -> Rgb {
        to_rgb(color.unwrap_or(Color::Reset), self.fg)
    }

    /// Pixel centre of a cell, where lines start and end.
    fn centre(&self, p: Point) -> Pt {
        ((p.x as f32 + 0.5) * self.cw, (p.y as f32 + 0.5) * self.ch)
    }

    /// Faint dots on the cell corners, shown while drawing.
    fn grid(&self, pixmap: &mut Pixmap, cols: i32, rows: i32) {
        let mut pb = PathBuilder::new();
        let r = (self.unit * 0.45).max(0.8);
        for y in 0..=rows {
            for x in 0..=cols {
                pb.push_circle(x as f32 * self.cw, y as f32 * self.ch, r);
            }
        }
        if let Some(path) = pb.finish() {
            let mut sketch = Sketch::new(pixmap, 0, 0.0, self.unit);
            sketch.fill_path(&path, sk(self.fg, 0.22));
        }
    }

    /// The cell under the cursor.
    fn cursor(&self, pixmap: &mut Pixmap, at: Point) {
        let style = self.theme.canvas_cursor;
        let color = self.rgb(style.bg.or(style.fg));
        let (x, y) = (at.x as f32 * self.cw, at.y as f32 * self.ch);
        let Some(rect) = SkRect::from_xywh(x + 1.0, y + 1.0, self.cw - 2.0, self.ch - 2.0) else {
            return;
        };
        let path = PathBuilder::from_rect(rect);
        let mut sketch = Sketch::new(pixmap, 0, 0.0, self.unit);
        sketch.fill_path(&path, sk(color, 0.28));
        sketch.stroke_path(
            &path,
            Pen {
                color: sk(color, 1.0),
                width: (self.unit * 0.9).max(1.0),
                dashed: false,
            },
        );
    }

    fn shape(
        &self,
        pixmap: &mut Pixmap,
        drawing: &Drawing,
        shape: &Shape,
        seed: u64,
        override_color: Option<Color>,
    ) {
        let color = self.rgb(shape_color(shape, override_color, self.theme));
        let mut sketch = Sketch::new(
            pixmap,
            seed.wrapping_mul(31).wrapping_add(7),
            self.roughness,
            self.unit,
        );
        match shape {
            Shape::Box {
                kind,
                rect,
                label,
                attrs,
            } => {
                let geo = Geo::new(*kind, *rect, self);
                if attrs.fill {
                    let fill = Pen {
                        color: sk(color, 1.0),
                        width: self.unit * 0.5,
                        dashed: false,
                    };
                    sketch.hachure(&geo.polygon(0.93), self.unit * 5.0, fill);
                }
                let pen = self.pen(color, attrs.dashed);
                match kind {
                    BoxKind::Rect => {
                        let radius = if attrs.round {
                            (geo.right - geo.left)
                                .min(geo.bottom - geo.top)
                                .mul_add(0.25, 0.0)
                                .min(self.ch * 1.2)
                        } else {
                            0.0
                        };
                        sketch.rounded_rect(geo.left, geo.top, geo.right, geo.bottom, radius, pen);
                    }
                    BoxKind::Ellipse => {
                        let (cx, cy) = geo.centre();
                        let (rx, ry) = geo.radii();
                        sketch.ellipse(cx, cy, rx, ry, pen);
                    }
                    BoxKind::Diamond => sketch.polyline(&geo.polygon(1.0), true, pen),
                }
                if let Some(label) = label {
                    let avail = geo.label_width();
                    let (cx, cy) = geo.centre();
                    self.label(pixmap, label, (cx, cy), avail, color, attrs.fill);
                }
            }
            Shape::Line {
                points,
                heads,
                label,
                attrs,
            } => {
                if points.len() < 2 {
                    return;
                }
                let mut pts: Vec<Pt> = points.iter().map(|p| self.centre(*p)).collect();
                self.bind_ends(&mut pts, points, drawing, shape);
                let pen = self.pen(color, attrs.dashed);
                sketch.polyline(&pts, false, pen);
                let n = pts.len();
                let head = self.pen(color, false);
                if heads.end {
                    self.arrowhead(&mut sketch, pts[n - 1], pts[n - 2], head);
                }
                if heads.start {
                    self.arrowhead(&mut sketch, pts[0], pts[1], head);
                }
                if let Some(label) = label {
                    let at = midpoint(&pts);
                    let avail = self.cw * (label.chars().count() as f32 + 2.0);
                    self.label(pixmap, label, at, avail, color, true);
                }
            }
            Shape::Text { at, text, .. } => {
                let layout = text::layout(text, self.font);
                let x = at.x as f32 * self.cw + self.cw * 0.1;
                let y = self.baseline(at.y as f32 * self.ch, &layout);
                text::draw(pixmap, &layout, x, y, color, 1.0);
            }
            Shape::Raw(_) => {}
        }
    }

    /// Baseline for text whose line box is centred in the row at `top`.
    fn baseline(&self, top: f32, layout: &text::Layout) -> f32 {
        top + (self.ch - layout.height()) / 2.0 + layout.ascent
    }

    /// Centred text at `at`, shrunk (down to a limit) and then cut to fit
    /// `avail` pixels. `clear` erases what is behind it first.
    fn label(&self, pixmap: &mut Pixmap, label: &str, at: Pt, avail: f32, color: Rgb, clear: bool) {
        let (text, size) = fit_label(label, self.font, avail.max(self.cw));
        let layout = text::layout(&text, size);
        let x = at.0 - layout.width / 2.0;
        let top = at.1 - self.ch / 2.0;
        if clear {
            let pad = self.unit;
            if let Some(rect) = SkRect::from_xywh(
                x - pad,
                top + (self.ch - layout.height()) / 2.0,
                layout.width + 2.0 * pad,
                layout.height(),
            ) {
                let mut sketch = Sketch::new(pixmap, 0, 0.0, self.unit);
                sketch.clear_path(&PathBuilder::from_rect(rect));
            }
        }
        let y = self.baseline(top, &layout);
        text::draw(pixmap, &layout, x, y, color, 1.0);
    }

    /// Two strokes from `tip`, pointing back towards `from`.
    fn arrowhead(&self, sketch: &mut Sketch<'_>, tip: Pt, from: Pt, pen: Pen) {
        let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1.0 {
            return;
        }
        let (ux, uy) = (dx / len, dy / len);
        let size = (self.ch * 1.1).min(len * 0.6);
        let angle = 22f32.to_radians();
        for side in [-1.0f32, 1.0] {
            let a = angle * side;
            let (rx, ry) = (-ux * a.cos() + uy * a.sin(), -ux * a.sin() - uy * a.cos());
            sketch.line(tip, (tip.0 + rx * size, tip.1 + ry * size), pen);
        }
    }

    /// Moves a line's ends from the centre of a cell inside a box to that
    /// box's border, so arrows connect shapes rather than pierce them.
    fn bind_ends(&self, pts: &mut [Pt], points: &[Point], drawing: &Drawing, this: &Shape) {
        let n = points.len();
        if n < 2 {
            return;
        }
        for (end, next) in [(0, 1), (n - 1, n - 2)] {
            let Some(geo) = drawing.shapes.iter().rev().find_map(|s| match s {
                Shape::Box { kind, rect, .. }
                    if !std::ptr::eq(s, this)
                        && rect.contains(points[end])
                        && !rect.contains(points[next]) =>
                {
                    Some(Geo::new(*kind, *rect, self))
                }
                _ => None,
            }) else {
                continue;
            };
            let inside = pts[end];
            let outside = pts[next];
            if geo.contains(outside, 1.0) {
                continue;
            }
            let (mut lo, mut hi) = (0.0f32, 1.0f32);
            for _ in 0..24 {
                let mid = f32::midpoint(lo, hi);
                let p = lerp(inside, outside, mid);
                if geo.contains(p, 1.0) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            // A little gap between border and line end.
            let gap = self.unit * 1.5;
            let dist = ((outside.0 - inside.0).powi(2) + (outside.1 - inside.1).powi(2)).sqrt();
            let t = (hi + gap / dist.max(1.0)).min(1.0);
            pts[end] = lerp(inside, outside, t);
        }
    }
}

fn lerp(a: Pt, b: Pt, t: f32) -> Pt {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

fn sk(color: Rgb, alpha: f32) -> SkColor {
    SkColor::from_rgba8(
        color.0,
        color.1,
        color.2,
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

/// Shrinks `label`'s font down to 60% of `size`, then cuts it with `…`,
/// until it is at most `avail` pixels wide.
fn fit_label(label: &str, size: f32, avail: f32) -> (String, f32) {
    let width = text::width(label, size);
    if width <= avail {
        return (label.to_owned(), size);
    }
    let shrunk = (size * avail / width).max(size * 0.6);
    if text::width(label, shrunk) <= avail {
        return (label.to_owned(), shrunk);
    }
    let chars: Vec<char> = label.chars().collect();
    let mut keep = chars.len();
    while keep > 0 {
        keep -= 1;
        let mut candidate: String = chars[..keep].iter().collect();
        candidate.push('…');
        if text::width(&candidate, shrunk) <= avail {
            return (candidate, shrunk);
        }
    }
    ("…".to_owned(), shrunk)
}

/// A box shape in pixels.
#[derive(Debug, Clone, Copy)]
struct Geo {
    kind: BoxKind,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Geo {
    fn new(kind: BoxKind, r: Rect, ctx: &Ctx<'_>) -> Self {
        let inset = ctx.inset();
        Self {
            kind,
            left: r.x as f32 * ctx.cw + inset,
            top: r.y as f32 * ctx.ch + inset,
            right: (r.x + r.w) as f32 * ctx.cw - inset,
            bottom: (r.y + r.h) as f32 * ctx.ch - inset,
        }
    }

    fn centre(&self) -> Pt {
        (
            f32::midpoint(self.left, self.right),
            f32::midpoint(self.top, self.bottom),
        )
    }

    fn radii(&self) -> (f32, f32) {
        (
            (self.right - self.left) / 2.0,
            (self.bottom - self.top) / 2.0,
        )
    }

    /// Whether `p` is inside the outline scaled by `scale` about the centre.
    fn contains(&self, p: Pt, scale: f32) -> bool {
        let (cx, cy) = self.centre();
        let (rx, ry) = self.radii();
        let (rx, ry) = ((rx * scale).max(0.5), (ry * scale).max(0.5));
        let (nx, ny) = ((p.0 - cx) / rx, (p.1 - cy) / ry);
        match self.kind {
            BoxKind::Rect => nx.abs() <= 1.0 && ny.abs() <= 1.0,
            BoxKind::Ellipse => nx * nx + ny * ny <= 1.0,
            BoxKind::Diamond => nx.abs() + ny.abs() <= 1.0,
        }
    }

    /// The outline as a polygon, scaled by `scale` about the centre.
    fn polygon(&self, scale: f32) -> Vec<Pt> {
        let (cx, cy) = self.centre();
        let (rx, ry) = self.radii();
        let (rx, ry) = (rx * scale, ry * scale);
        match self.kind {
            BoxKind::Rect => vec![
                (cx - rx, cy - ry),
                (cx + rx, cy - ry),
                (cx + rx, cy + ry),
                (cx - rx, cy + ry),
            ],
            BoxKind::Diamond => vec![(cx, cy - ry), (cx + rx, cy), (cx, cy + ry), (cx - rx, cy)],
            BoxKind::Ellipse => (0..48)
                .map(|i| {
                    let a = i as f32 / 48.0 * std::f32::consts::TAU;
                    (cx + rx * a.cos(), cy + ry * a.sin())
                })
                .collect(),
        }
    }

    /// Room for a centred label.
    fn label_width(&self) -> f32 {
        let (rx, ry) = self.radii();
        let w = rx * 2.0;
        match self.kind {
            BoxKind::Rect => w - ry.min(w * 0.15) * 0.5,
            // A label sits on the centre line, where the diamond and the
            // ellipse are widest, but their sides slope in around it.
            BoxKind::Ellipse => w * 0.82,
            BoxKind::Diamond => w * 0.6,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    fn cell() -> CellSize {
        CellSize::new(10, 20)
    }

    fn opaque(img: &Image) -> usize {
        (0..img.height())
            .flat_map(|y| (0..img.width()).map(move |x| (x, y)))
            .filter(|&(x, y)| img.pixel(x, y).is_some_and(|p| p.3 > 0))
            .count()
    }

    #[test]
    fn picture_covers_the_same_cells_as_the_text_renderer() {
        let d = parse("rect 2,1 14x5 \"Parser\"\nline 16,3 -> 24,3\n");
        let img = render_image(&d, &Theme::default(), cell(), (255, 255, 255), 200, None).unwrap();
        let (cols, rows) = canvas_size(&d, None, 200);
        assert_eq!(img.cols(), cols as usize);
        assert_eq!(img.rows(), rows as usize);
        assert_eq!(img.width(), cols as u32 * 10);
        assert_eq!(img.height(), rows as u32 * 20);
        assert!(opaque(&img) > 200);
        assert!(!img.to_png().is_empty());
    }

    #[test]
    fn empty_drawing_with_a_size_is_a_blank_picture() {
        let d = parse("size 8x2\n");
        let img = render_image(&d, &Theme::default(), cell(), (255, 255, 255), 200, None).unwrap();
        assert_eq!((img.cols(), img.rows()), (8, 2));
        assert_eq!(opaque(&img), 0);
    }

    #[test]
    fn width_is_cropped() {
        let d = parse("rect 0,0 40x2\n");
        let img = render_image(&d, &Theme::default(), cell(), (255, 255, 255), 10, None).unwrap();
        assert_eq!(img.cols(), 10);
    }

    #[test]
    fn colours_come_from_the_palette() {
        let theme = Theme::builtin("nord").unwrap();
        let d = parse("rect 0,0 10x3 color=red\n");
        let img = render_image(&d, &theme, cell(), (255, 255, 255), 80, None).unwrap();
        let red = to_rgb(theme.palette.red, (0, 0, 0));
        let hit = (0..img.height())
            .flat_map(|y| (0..img.width()).map(move |x| (x, y)))
            .filter_map(|(x, y)| img.pixel(x, y))
            .any(|p| p.3 == 255 && (p.0, p.1, p.2) == red);
        assert!(hit, "no fully opaque red pixel");
    }

    #[test]
    fn overlay_draws_cursor_and_grid() {
        let d = parse("rect 0,0 2x1\n");
        let plain = render_image(&d, &Theme::default(), cell(), (255, 255, 255), 80, None).unwrap();
        let overlay = Overlay {
            cursor: Some(Point::new(6, 3)),
            selected: Some(0),
            preview: None,
            min_size: (0, 0),
        };
        let edited = render_image(
            &d,
            &Theme::default(),
            cell(),
            (255, 255, 255),
            80,
            Some(&overlay),
        )
        .unwrap();
        assert!(edited.rows() >= 5 && edited.cols() >= 8);
        assert!(opaque(&edited) > opaque(&plain));
        // Something is painted in the cursor cell.
        let cursor_px = (60..70)
            .flat_map(|x| (60..80).map(move |y| (x, y)))
            .filter(|&(x, y)| edited.pixel(x, y).is_some_and(|p| p.3 > 0))
            .count();
        assert!(cursor_px > 10);
    }

    #[test]
    fn labels_shrink_then_truncate() {
        let (t, s) = fit_label("short", 20.0, 1000.0);
        assert_eq!((t.as_str(), s), ("short", 20.0));
        let (t, s) = fit_label("a somewhat longer label", 20.0, 120.0);
        assert!(s < 20.0);
        assert!(t.ends_with('…') || t == "a somewhat longer label");
        let (t, _) = fit_label("a somewhat longer label", 20.0, 30.0);
        assert!(t.ends_with('…'));
        assert!(text::width(&t, 12.0) <= 30.0);
    }

    #[test]
    fn same_input_same_picture() {
        let d = parse("ellipse 1,1 12x5 \"x\" fill\nline 0,0 -> 20,7 dashed\n");
        let a = render_image(&d, &Theme::default(), cell(), (255, 255, 255), 80, None).unwrap();
        let b = render_image(&d, &Theme::default(), cell(), (255, 255, 255), 80, None).unwrap();
        assert_eq!(a.to_png(), b.to_png());
    }
}
