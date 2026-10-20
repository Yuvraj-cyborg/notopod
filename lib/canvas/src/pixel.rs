//! Rendering a drawing as a picture.
//!
//! Shapes are placed on the same cell grid the editor moves on, but drawn
//! at the terminal's real pixel resolution: a cell is `CellSize` pixels,
//! strokes are anti-aliased curves, labels use a hand-drawn font. The
//! picture has a transparent background so the terminal's own shows
//! through. Terminals that can display images put it exactly over the
//! cells the braille renderer would have used.

use std::f32::consts::TAU;

use ratatui::style::Color;
use theme::{to_rgb, Rgb, Theme};
use tiny_skia::{Color as SkColor, PathBuilder, Pixmap, Rect as SkRect};

use crate::expr::Expr;
use crate::model::{BoxKind, Drawing, Point, Rect, Shape, MATH_EM_ROWS};
use crate::plot::{self, Frame};
use crate::render::{canvas_size, midpoint, shape_color, Overlay};
use crate::rough::Rng;
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
    /// A picture painted elsewhere, covering `cols` by `rows` cells. The
    /// pixmap should be exactly that many cells in size.
    pub fn from_pixmap(pixmap: Pixmap, cols: usize, rows: usize) -> Self {
        Self {
            pixmap,
            cols: i32::try_from(cols).unwrap_or(i32::MAX),
            rows: i32::try_from(rows).unwrap_or(i32::MAX),
        }
    }

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
            Shape::Math { at, tex, .. } => {
                let (x, y) = (at.x as f32 * self.cw, at.y as f32 * self.ch);
                formula(pixmap, tex, true, (x, y), self.ch * MATH_EM_ROWS, color);
            }
            Shape::Axes { .. } => self.axes_shape(pixmap, shape, color, seed),
            Shape::Plot { .. } => self.plot_shape(pixmap, drawing, shape, color, seed),
            Shape::Raw(_) => {}
        }
    }

    fn axes_shape(&self, pixmap: &mut Pixmap, shape: &Shape, color: Rgb, seed: u64) {
        let Shape::Axes {
            rect,
            x,
            y,
            grid,
            attrs,
        } = shape
        else {
            return;
        };
        let frame = self.frame(*rect, x.as_deref(), y.as_deref());
        // Axes are long straight lines: the wobble that makes a box look
        // drawn by hand bows an axis into a curve, so they get a third of it.
        let mut calm = Sketch::new(
            pixmap,
            seed.wrapping_mul(31).wrapping_add(7),
            self.roughness * 0.3,
            self.unit,
        );
        if *grid {
            self.grid_lines(&mut calm, &frame, color);
        }
        self.axes(&mut calm, &frame, color, attrs.dashed);
        self.tick_labels(pixmap, &frame, color);
    }

    fn plot_shape(
        &self,
        pixmap: &mut Pixmap,
        drawing: &Drawing,
        shape: &Shape,
        color: Rgb,
        seed: u64,
    ) {
        let Shape::Plot {
            expr,
            x,
            label,
            attrs,
        } = shape
        else {
            return;
        };
        let Some(Shape::Axes {
            rect, x: ax, y: ay, ..
        }) = plot::axes_for(drawing, shape)
        else {
            return;
        };
        let Ok(expr) = Expr::parse(expr) else {
            return;
        };
        let frame = self.frame(*rect, ax.as_deref(), ay.as_deref());
        let only = x.as_deref().and_then(crate::expr::range);
        let samples = ((frame.right - frame.left) / self.unit.max(1.0)) as usize;
        let runs = plot::curve(&expr, only, &frame, samples);
        let mut sketch = Sketch::new(
            pixmap,
            seed.wrapping_mul(31).wrapping_add(7),
            self.roughness,
            self.unit,
        );
        for run in &runs {
            self.curve(&mut sketch, run, color, attrs.dashed, seed);
        }
        if let (Some(label), Some(&end)) = (label, runs.last().and_then(|r| r.last())) {
            self.graph_label(pixmap, label, end, color);
        }
    }

    /// Em of the numbers along an axis.
    fn tick_em(&self) -> f32 {
        self.ch * 0.5
    }

    /// The area inside a pair of axes, in pixels, using the same tick
    /// spacing as the braille renderer: a tick every six columns and every
    /// two rows. An axis along an edge leaves room for its numbers.
    fn frame(&self, rect: Rect, x: Option<&str>, y: Option<&str>) -> Frame {
        let (xr, yr) = plot::ranges(x, y);
        let mut f = Frame {
            left: rect.x as f32 * self.cw + self.cw * 0.5,
            top: rect.y as f32 * self.ch + self.ch * 0.7,
            right: (rect.x + rect.w) as f32 * self.cw - self.cw * 1.2,
            bottom: (rect.y + rect.h) as f32 * self.ch - self.ch * 0.5,
            x: xr,
            y: yr,
        };
        if !(xr.0 <= 0.0 && 0.0 <= xr.1) {
            let (ticks, step) = plot::ticks(yr, f.bottom - f.top, self.ch * 2.0);
            let widest = ticks
                .iter()
                .map(|t| crate::expr::tick_label(*t, step).chars().count())
                .max()
                .unwrap_or(0);
            f.left += (widest as f32 * 0.6 + 0.5) * self.tick_em();
        }
        if !(yr.0 <= 0.0 && 0.0 <= yr.1) {
            f.bottom -= self.tick_em() * 1.6;
        }
        f.right = f.right.max(f.left + 1.0);
        f.bottom = f.bottom.max(f.top + 1.0);
        f
    }

    /// Faint, exact lines at every tick: Manim's `NumberPlane`.
    fn grid_lines(&self, sketch: &mut Sketch<'_>, f: &Frame, color: Rgb) {
        let (xs, _) = plot::ticks(f.x, f.right - f.left, self.cw * 6.0);
        let (ys, _) = plot::ticks(f.y, f.bottom - f.top, self.ch * 2.0);
        let mut pb = PathBuilder::new();
        for t in xs {
            pb.move_to(f.sx(t), f.top);
            pb.line_to(f.sx(t), f.bottom);
        }
        for t in ys {
            pb.move_to(f.left, f.sy(t));
            pb.line_to(f.right, f.sy(t));
        }
        if let Some(path) = pb.finish() {
            let pen = Pen {
                color: sk(color, 0.18),
                width: (self.unit * 0.6).max(1.0),
                dashed: false,
            };
            sketch.stroke_path(&path, pen);
        }
    }

    /// Both axes, hand-drawn, with small arrowheads at the positive ends
    /// and a tick at every round number.
    fn axes(&self, sketch: &mut Sketch<'_>, f: &Frame, color: Rgb, dashed: bool) {
        let pen = self.pen(color, dashed);
        let (ax, ay) = (f.y_axis(), f.x_axis());
        let head = self.ch * 0.45;
        let right = (f.right + self.cw * 0.9, ay);
        sketch.line((f.left, ay), right, pen);
        arrowhead(sketch, right, (f.left, ay), self.pen(color, false), head);
        let top = (ax, f.top - self.ch * 0.55);
        sketch.line((ax, f.bottom), top, pen);
        arrowhead(sketch, top, (ax, f.bottom), self.pen(color, false), head);

        let tick = self.ch * 0.16;
        let thin = Pen {
            width: self.unit * 0.8,
            ..self.pen(color, false)
        };
        for t in plot::x_ticks(f, self.cw * 6.0).0 {
            let px = f.sx(t);
            if (px - ax).abs() > self.unit * 2.0 {
                sketch.line((px, ay - tick), (px, ay + tick), thin);
            }
        }
        for t in plot::y_ticks(f, self.ch * 2.0).0 {
            let py = f.sy(t);
            if (py - ay).abs() > self.unit * 2.0 {
                sketch.line((ax - tick, py), (ax + tick, py), thin);
            }
        }
    }

    /// The numbers: under the x axis, left of the y axis, typeset like the
    /// rest of the maths. The origin's `0` is left out where the axes cross.
    fn tick_labels(&self, pixmap: &mut Pixmap, f: &Frame, color: Rgb) {
        let (ax, ay) = (f.y_axis(), f.x_axis());
        let em = self.tick_em();
        let gap = self.ch * 0.2;
        let width = pixmap.width() as f32;
        let (xs, step) = plot::x_ticks(f, self.cw * 6.0);
        for t in xs {
            if t == 0.0 && f.origin_in_view() {
                continue;
            }
            let label = crate::expr::tick_label(t, step);
            let (w, _) = formula_size(&label, false, em);
            let x = (f.sx(t) - w / 2.0).clamp(0.0, (width - w).max(0.0));
            formula(pixmap, &label, false, (x, ay + gap), em, color);
        }
        let (ys, step) = plot::y_ticks(f, self.ch * 2.0);
        for t in ys {
            if t == 0.0 && f.origin_in_view() {
                continue;
            }
            let label = crate::expr::tick_label(t, step);
            let (w, h) = formula_size(&label, false, em);
            let at = ((ax - gap - w).max(0.0), f.sy(t) - h / 2.0);
            formula(pixmap, &label, false, at, em, color);
        }
    }

    /// A curve that reads as drawn by hand but stays smooth: one exact
    /// stroke, and where the theme is rough, a lighter second pass that
    /// drifts slowly away from the first, as a pen going over a line again
    /// does. No jitter: a graph has to stay readable.
    fn curve(&self, sketch: &mut Sketch<'_>, run: &[Pt], color: Rgb, dashed: bool, seed: u64) {
        let stroke = |sketch: &mut Sketch<'_>, pts: &mut dyn Iterator<Item = Pt>, pen: Pen| {
            let mut pb = PathBuilder::new();
            if let Some((x, y)) = pts.next() {
                pb.move_to(x, y);
            }
            for (x, y) in pts {
                pb.line_to(x, y);
            }
            if let Some(path) = pb.finish() {
                sketch.stroke_path(&path, pen);
            }
        };
        let pen = Pen {
            width: self.unit * 1.15,
            ..self.pen(color, dashed)
        };
        stroke(sketch, &mut run.iter().copied(), pen);
        if self.roughness <= 0.0 {
            return;
        }
        let mut rng = Rng::new(seed ^ 0x9e37_79b9);
        let (p1, p2) = (rng.unit() * TAU, rng.unit() * TAU);
        let amp = self.unit * self.roughness * 0.9;
        let mut drift = run.iter().enumerate().map(|(i, &(x, y))| {
            let t = i as f32;
            let d = amp * ((t * 0.045 + p1).sin() * 0.7 + (t * 0.11 + p2).sin() * 0.3);
            (x + d * 0.3, y + d)
        });
        let light = Pen {
            color: sk(color, 0.55),
            width: self.unit * 0.8,
            dashed,
        };
        stroke(sketch, &mut drift, light);
    }

    /// A graph's label, typeset beside the end of its curve — to the left
    /// when there is no room on the right — and moved up or down off
    /// anything already drawn there, such as another graph's label.
    fn graph_label(&self, pixmap: &mut Pixmap, tex: &str, end: Pt, color: Rgb) {
        let em = self.ch * 0.75;
        let (w, h) = formula_size(tex, false, em);
        let mut x = end.0 + self.cw * 0.4;
        if x + w > pixmap.width() as f32 {
            x = end.0 - w - self.cw * 0.4;
        }
        let x = x.max(0.0);
        // Labels are lines of text: they are kept a line apart, judged by
        // where their bottoms sit, not by their own boxes, which are short
        // for `cos x` and tall for `\int`.
        let bottom = end.1 - self.ch * 0.1;
        let line = em * 1.15;
        let bottom = [0.0, -1.0, 1.0, -2.0, 2.0]
            .iter()
            .map(|k| bottom + k * line)
            .find(|&b| b - h >= 0.0 && ink(pixmap, x, b - em, w, em * 1.4) < 0.01)
            .unwrap_or(bottom.max(h));
        formula(pixmap, tex, false, (x, bottom - h), em, color);
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
        arrowhead(sketch, tip, from, pen, self.ch * 1.1);
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

/// An arrowhead at most `size` pixels long: two strokes from `tip`,
/// pointing back towards `from`.
fn arrowhead(sketch: &mut Sketch<'_>, tip: Pt, from: Pt, pen: Pen, size: f32) {
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let size = size.min(len * 0.6);
    let angle = 22f32.to_radians();
    for side in [-1.0f32, 1.0] {
        let a = angle * side;
        let (rx, ry) = (-ux * a.cos() + uy * a.sin(), -ux * a.sin() - uy * a.cos());
        sketch.line(tip, (tip.0 + rx * size, tip.1 + ry * size), pen);
    }
}

/// Width and height in pixels of `tex` at `em`: typeset where this build
/// can, otherwise its Unicode spelling in the hand-drawn font.
fn formula_size(tex: &str, display: bool, em: f32) -> (f32, f32) {
    if let Some((w, h)) = math::measure(tex, display) {
        return (w * em, h * em);
    }
    let layout = text::layout(&math::to_unicode(tex), em);
    (layout.width, layout.height())
}

/// Paints `tex` with its top-left at `at`.
fn formula(pixmap: &mut Pixmap, tex: &str, display: bool, at: Pt, em: f32, color: Rgb) {
    if let Ok(f) = math::layout(tex, display) {
        f.draw(pixmap, at.0, at.1, em, color);
        return;
    }
    let layout = text::layout(&math::to_unicode(tex), em);
    text::draw(pixmap, &layout, at.0, at.1 + layout.ascent, color, 1.0);
}

/// The share of pixels in a rectangle that already have something on
/// them, sampled every few pixels; anything outside the pixmap counts as
/// full.
fn ink(pixmap: &Pixmap, x: f32, y: f32, w: f32, h: f32) -> f32 {
    if x < 0.0 || y < 0.0 || x + w > pixmap.width() as f32 || y + h > pixmap.height() as f32 {
        return 1.0;
    }
    let (mut seen, mut inked) = (0u32, 0u32);
    let mut py = y;
    while py < y + h {
        let mut px = x;
        while px < x + w {
            seen += 1;
            if pixmap
                .pixel(px as u32, py as u32)
                .is_some_and(|p| p.alpha() > 40)
            {
                inked += 1;
            }
            px += 2.0;
        }
        py += 2.0;
    }
    if seen == 0 {
        0.0
    } else {
        inked as f32 / seen as f32
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
        assert!(img.to_png().starts_with(b"\x89PNG"));
    }

    #[test]
    fn axes_and_plots_are_drawn_inside_their_cells() {
        let theme = Theme::builtin("nord").unwrap();
        let axes = parse("axes 0,0 30x10 x=-3..3 y=-1..1 grid\n");
        let with_plot = parse(
            "axes 0,0 30x10 x=-3..3 y=-1..1 grid\nplot \"sin(x)\" \"\\sin x\" color=yellow\n",
        );
        let a = render_image(&axes, &theme, cell(), (255, 255, 255), 200, None).unwrap();
        let p = render_image(&with_plot, &theme, cell(), (255, 255, 255), 200, None).unwrap();
        assert!(opaque(&a) > 500, "axes, ticks and numbers");
        assert!(opaque(&p) > opaque(&a) + 300, "the curve adds ink");
        assert_eq!(
            (p.cols(), p.rows()),
            (a.cols(), a.rows()),
            "a plot takes no cells"
        );
        let yellow = to_rgb(theme.palette.yellow, (0, 0, 0));
        let curve = (0..p.height())
            .flat_map(|y| (0..p.width()).map(move |x| (x, y)))
            .filter_map(|(x, y)| p.pixel(x, y))
            .filter(|px| px.3 == 255 && (px.0, px.1, px.2) == yellow)
            .count();
        assert!(curve > 100, "drawn in the plot's colour");
    }

    #[test]
    fn a_formula_in_a_drawing_has_ink_and_room() {
        let d = parse("math 1,0 \"x = \\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}\"\n");
        let (cols, rows) = canvas_size(&d, None, 200);
        assert!(cols > 12, "wide enough for the formula: {cols}");
        let img = render_image(&d, &Theme::default(), cell(), (255, 255, 255), 200, None).unwrap();
        assert!(opaque(&img) > 300);
        if math::TYPESET {
            assert!(rows >= 3, "a fraction needs more than one row: {rows}");
        }
        // Nothing lands outside the picture's last column: the cells were
        // sized for it.
        let last = img.width() - 1;
        assert!((0..img.height()).all(|y| img.pixel(last, y).is_some_and(|p| p.3 == 0)));
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
