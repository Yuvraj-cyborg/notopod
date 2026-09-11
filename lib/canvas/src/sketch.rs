//! Hand-drawn vector strokes, the rough.js way.
//!
//! Every stroke is drawn twice with independent wobble, as a pen would
//! when going over a line again: endpoints are nudged, straight lines bow
//! into cubic curves, ellipses are curves through slightly displaced
//! points, and fills are hatched with tilted lines. The amount of wobble
//! is `roughness` (0 is exact geometry, 1 is Excalidraw's "artist", 2 its
//! "cartoonist"), scaled by `unit`, the number of pixels one rough.js
//! pixel is worth at the current cell size.

use std::f32::consts::{PI, TAU};

use tiny_skia::{
    BlendMode, Color, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke,
    StrokeDash, Transform,
};

use crate::rough::Rng;

/// A point in pixels.
pub type Pt = (f32, f32);

/// How a stroke is painted.
#[derive(Debug, Clone, Copy)]
pub struct Pen {
    /// Stroke colour.
    pub color: Color,
    /// Stroke width in pixels.
    pub width: f32,
    /// Dashed instead of solid.
    pub dashed: bool,
}

/// Paints rough strokes onto a pixmap.
pub struct Sketch<'a> {
    pixmap: &'a mut Pixmap,
    rng: Rng,
    roughness: f32,
    bowing: f32,
    unit: f32,
}

impl<'a> Sketch<'a> {
    /// A sketcher for `pixmap`. `seed` makes the wobble reproducible.
    pub fn new(pixmap: &'a mut Pixmap, seed: u64, roughness: f32, unit: f32) -> Self {
        Self {
            pixmap,
            rng: Rng::new(seed),
            roughness: roughness.max(0.0),
            bowing: 1.0,
            unit: unit.max(0.5),
        }
    }

    fn exact(&self) -> bool {
        self.roughness <= 0.0
    }

    fn max_offset(&self) -> f32 {
        2.0 * self.unit
    }

    /// A random number in `[-x, x)` scaled by the roughness.
    fn offset_opt(&mut self, x: f32) -> f32 {
        self.roughness * self.rng.signed() * x
    }

    /// A random number in `[min, max)` scaled by the roughness.
    fn offset(&mut self, min: f32, max: f32) -> f32 {
        self.roughness * (self.rng.unit() * (max - min) + min)
    }

    // ----- painting -----

    /// Strokes a path.
    pub fn stroke_path(&mut self, path: &Path, pen: Pen) {
        let mut paint = Paint::default();
        paint.set_color(pen.color);
        paint.anti_alias = true;
        let mut stroke = Stroke {
            width: pen.width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        if pen.dashed {
            let on = 4.0 * self.unit;
            stroke.dash = StrokeDash::new(vec![on, on], 0.0);
        }
        self.pixmap
            .stroke_path(path, &paint, &stroke, Transform::identity(), None);
    }

    /// Fills a path with a flat colour.
    pub fn fill_path(&mut self, path: &Path, color: Color) {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        self.pixmap
            .fill_path(path, &paint, FillRule::Winding, Transform::identity(), None);
    }

    /// Makes the pixels inside `path` transparent again.
    pub fn clear_path(&mut self, path: &Path) {
        let mut paint = Paint {
            blend_mode: BlendMode::Clear,
            anti_alias: false,
            ..Paint::default()
        };
        paint.set_color(Color::BLACK);
        self.pixmap
            .fill_path(path, &paint, FillRule::Winding, Transform::identity(), None);
    }

    // ----- primitives -----

    /// One wobbly pass of a line, as rough.js draws it: a cubic curve
    /// whose ends and control points are nudged. `overlay` is the second,
    /// lighter pass.
    fn line_pass(&mut self, pb: &mut PathBuilder, a: Pt, b: Pt, overlay: bool) {
        let (x1, y1) = a;
        let (x2, y2) = b;
        if self.exact() {
            pb.move_to(x1, y1);
            pb.line_to(x2, y2);
            return;
        }
        let length_sq = (x1 - x2).powi(2) + (y1 - y2).powi(2);
        let mut offset = self.max_offset();
        if offset * offset * 100.0 > length_sq {
            offset = length_sq.sqrt() / 10.0;
        }
        let half = offset / 2.0;
        let diverge = 0.2 + self.rng.unit() * 0.2;
        let mut mid_x = self.bowing * self.max_offset() * (y2 - y1) / 200.0;
        let mut mid_y = self.bowing * self.max_offset() * (x1 - x2) / 200.0;
        mid_x = self.offset_opt(mid_x);
        mid_y = self.offset_opt(mid_y);
        let amount = if overlay { half } else { offset };
        let r = |s: &mut Self| s.offset_opt(amount);
        let start = (x1 + r(self), y1 + r(self));
        let c1 = (
            mid_x + x1 + (x2 - x1) * diverge + r(self),
            mid_y + y1 + (y2 - y1) * diverge + r(self),
        );
        let c2 = (
            mid_x + x1 + 2.0 * (x2 - x1) * diverge + r(self),
            mid_y + y1 + 2.0 * (y2 - y1) * diverge + r(self),
        );
        let end = (x2 + r(self), y2 + r(self));
        pb.move_to(start.0, start.1);
        pb.cubic_to(c1.0, c1.1, c2.0, c2.1, end.0, end.1);
    }

    /// Adds both passes of a line to `pb`.
    fn double_line(&mut self, pb: &mut PathBuilder, a: Pt, b: Pt) {
        self.line_pass(pb, a, b, false);
        if !self.exact() {
            self.line_pass(pb, a, b, true);
        }
    }

    /// A hand-drawn straight line.
    pub fn line(&mut self, a: Pt, b: Pt, pen: Pen) {
        let mut pb = PathBuilder::new();
        self.double_line(&mut pb, a, b);
        if let Some(path) = pb.finish() {
            self.stroke_path(&path, pen);
        }
    }

    /// Hand-drawn lines through `points`, closed back to the start when
    /// `close` is set.
    pub fn polyline(&mut self, points: &[Pt], close: bool, pen: Pen) {
        if points.len() < 2 {
            return;
        }
        let mut pb = PathBuilder::new();
        for w in points.windows(2) {
            self.double_line(&mut pb, w[0], w[1]);
        }
        if close && points.len() > 2 {
            self.double_line(&mut pb, points[points.len() - 1], points[0]);
        }
        if let Some(path) = pb.finish() {
            self.stroke_path(&path, pen);
        }
    }

    /// A hand-drawn ellipse centred on `(cx, cy)`.
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, pen: Pen) {
        let (increment, rx, ry) = self.ellipse_params(rx * 2.0, ry * 2.0);
        let mut pb = PathBuilder::new();
        let max = self.offset(0.4, 1.0);
        let overlap = increment * self.offset(0.1, max);
        let points = self.ellipse_points(increment, cx, cy, rx, ry, 1.0, overlap);
        curve_through(&mut pb, &points);
        if !self.exact() {
            let points = self.ellipse_points(increment, cx, cy, rx, ry, 1.5, 0.0);
            curve_through(&mut pb, &points);
        }
        if let Some(path) = pb.finish() {
            self.stroke_path(&path, pen);
        }
    }

    fn ellipse_params(&mut self, width: f32, height: f32) -> (f32, f32, f32) {
        const CURVE_STEPS: f32 = 9.0;
        const CURVE_FITTING: f32 = 0.95;
        let mean_sq = f32::midpoint((width / 2.0).powi(2), (height / 2.0).powi(2));
        let psq = (PI * 2.0 * mean_sq.sqrt()).sqrt();
        let steps = (CURVE_STEPS.max(CURVE_STEPS / 200f32.sqrt() * psq)).ceil();
        let increment = TAU / steps;
        let mut rx = (width / 2.0).abs();
        let mut ry = (height / 2.0).abs();
        let fit = 1.0 - CURVE_FITTING;
        rx += self.offset_opt(rx * fit);
        ry += self.offset_opt(ry * fit);
        (increment, rx, ry)
    }

    #[allow(clippy::too_many_arguments)]
    fn ellipse_points(
        &mut self,
        increment: f32,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        offset: f32,
        overlap: f32,
    ) -> Vec<Pt> {
        let mut all = Vec::new();
        if self.exact() {
            let inc = increment / 4.0;
            all.push((cx + rx * (-inc).cos(), cy + ry * (-inc).sin()));
            let mut angle = 0.0;
            while angle <= TAU {
                all.push((cx + rx * angle.cos(), cy + ry * angle.sin()));
                angle += inc;
            }
            all.push((cx + rx, cy));
            all.push((cx + rx * inc.cos(), cy + ry * inc.sin()));
            return all;
        }
        let offset = offset * self.unit;
        let rad_offset = self.offset_opt(0.5) - PI / 2.0;
        let jitter = |s: &mut Self| s.offset_opt(offset);
        all.push((
            jitter(self) + cx + 0.9 * rx * (rad_offset - increment).cos(),
            jitter(self) + cy + 0.9 * ry * (rad_offset - increment).sin(),
        ));
        let end = TAU + rad_offset - 0.01;
        let mut angle = rad_offset;
        while angle < end {
            all.push((
                jitter(self) + cx + rx * angle.cos(),
                jitter(self) + cy + ry * angle.sin(),
            ));
            angle += increment;
        }
        all.push((
            jitter(self) + cx + rx * (rad_offset + TAU + overlap * 0.5).cos(),
            jitter(self) + cy + ry * (rad_offset + TAU + overlap * 0.5).sin(),
        ));
        all.push((
            jitter(self) + cx + 0.98 * rx * (rad_offset + overlap).cos(),
            jitter(self) + cy + 0.98 * ry * (rad_offset + overlap).sin(),
        ));
        all.push((
            jitter(self) + cx + 0.9 * rx * (rad_offset + overlap * 0.5).cos(),
            jitter(self) + cy + 0.9 * ry * (rad_offset + overlap * 0.5).sin(),
        ));
        all
    }

    /// A rectangle with rounded corners of radius `r`.
    pub fn rounded_rect(&mut self, left: f32, top: f32, right: f32, bottom: f32, r: f32, pen: Pen) {
        let r = r
            .min((right - left) / 2.0)
            .min((bottom - top) / 2.0)
            .max(0.0);
        if r < 1.0 {
            self.polyline(
                &[(left, top), (right, top), (right, bottom), (left, bottom)],
                true,
                pen,
            );
            return;
        }
        let mut pb = PathBuilder::new();
        // Edges, then the corner joining each edge to the next, clockwise.
        let edges = [
            ((left + r, top), (right - r, top)),
            ((right, top + r), (right, bottom - r)),
            ((right - r, bottom), (left + r, bottom)),
            ((left, bottom - r), (left, top + r)),
        ];
        let corners = [
            (right - r, top + r, -PI / 2.0),
            (right - r, bottom - r, 0.0),
            (left + r, bottom - r, PI / 2.0),
            (left + r, top + r, PI),
        ];
        for pass in 0..2 {
            if pass == 1 && self.exact() {
                break;
            }
            for (edge, corner) in edges.iter().zip(corners.iter()) {
                self.line_pass(&mut pb, edge.0, edge.1, pass == 1);
                let (cx, cy, from) = *corner;
                let points = self.arc_points(
                    cx,
                    cy,
                    r,
                    from,
                    from + PI / 2.0,
                    if pass == 1 { 0.5 } else { 1.0 },
                );
                curve_through(&mut pb, &points);
            }
        }
        if let Some(path) = pb.finish() {
            self.stroke_path(&path, pen);
        }
    }

    /// Points along a circular arc, slightly displaced, with one extra
    /// point on either side so the curve through them has a tangent.
    fn arc_points(&mut self, cx: f32, cy: f32, r: f32, from: f32, to: f32, amount: f32) -> Vec<Pt> {
        let steps = ((r * (to - from).abs() / (4.0 * self.unit)).ceil() as usize).clamp(2, 12);
        let inc = (to - from) / steps as f32;
        let amp = if self.exact() {
            0.0
        } else {
            0.6 * self.unit * amount
        };
        let mut points = Vec::with_capacity(steps + 3);
        for i in -1..=(steps as i32 + 1) {
            let angle = from + inc * i as f32;
            let rr = r + self.offset_opt(amp);
            points.push((cx + rr * angle.cos(), cy + rr * angle.sin()));
        }
        points
    }

    /// Hatches the polygon with tilted lines `gap` pixels apart.
    pub fn hachure(&mut self, polygon: &[Pt], gap: f32, pen: Pen) {
        if polygon.len() < 3 || gap <= 0.0 {
            return;
        }
        // rough.js tilts its hatching by -41°: rotate the polygon so the
        // hatch lines are horizontal, scan, then rotate the lines back.
        let angle = -41f32.to_radians();
        let (cos, sin) = (angle.cos(), angle.sin());
        let rot = |p: Pt| (p.0 * cos - p.1 * sin, p.0 * sin + p.1 * cos);
        let unrot = |p: Pt| (p.0 * cos + p.1 * sin, -p.0 * sin + p.1 * cos);
        let rotated: Vec<Pt> = polygon.iter().map(|p| rot(*p)).collect();
        let (min_y, max_y) = rotated.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p.1), hi.max(p.1))
        });

        let mut pb = PathBuilder::new();
        let mut y = min_y + gap / 2.0;
        while y < max_y {
            let mut xs: Vec<f32> = Vec::new();
            for i in 0..rotated.len() {
                let a = rotated[i];
                let b = rotated[(i + 1) % rotated.len()];
                if (a.1 <= y && b.1 > y) || (b.1 <= y && a.1 > y) {
                    xs.push(a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0));
                }
            }
            xs.sort_by(f32::total_cmp);
            for pair in xs.as_chunks::<2>().0 {
                let from = unrot((pair[0], y));
                let to = unrot((pair[1], y));
                if (pair[1] - pair[0]).abs() >= 1.0 {
                    self.line_pass(&mut pb, from, to, false);
                }
            }
            y += gap + self.offset_opt(gap * 0.15);
        }
        if let Some(path) = pb.finish() {
            self.stroke_path(
                &path,
                Pen {
                    dashed: false,
                    ..pen
                },
            );
        }
    }
}

/// Appends a smooth curve through `points` (Catmull-Rom, as rough.js's
/// `curve` with tightness 0). Needs at least four points; fewer are
/// joined by straight lines.
fn curve_through(pb: &mut PathBuilder, points: &[Pt]) {
    match points.len() {
        0 | 1 => {}
        2 | 3 => {
            pb.move_to(points[0].0, points[0].1);
            for p in &points[1..] {
                pb.line_to(p.0, p.1);
            }
        }
        n => {
            pb.move_to(points[1].0, points[1].1);
            for i in 1..n - 2 {
                let p0 = points[i - 1];
                let p1 = points[i];
                let p2 = points[i + 1];
                let p3 = points[i + 2];
                pb.cubic_to(
                    p1.0 + (p2.0 - p0.0) / 6.0,
                    p1.1 + (p2.1 - p0.1) / 6.0,
                    p2.0 + (p1.0 - p3.0) / 6.0,
                    p2.1 + (p1.1 - p3.1) / 6.0,
                    p2.0,
                    p2.1,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixmap() -> Pixmap {
        Pixmap::new(200, 100).unwrap()
    }

    fn painted(p: &Pixmap) -> usize {
        p.pixels().iter().filter(|c| c.alpha() > 0).count()
    }

    fn pen() -> Pen {
        Pen {
            color: Color::BLACK,
            width: 2.0,
            dashed: false,
        }
    }

    #[test]
    fn exact_line_is_straight() {
        let mut p = pixmap();
        Sketch::new(&mut p, 1, 0.0, 2.0).line((10.0, 50.0), (190.0, 50.0), pen());
        // Every painted pixel sits on the row of the line.
        for (i, px) in p.pixels().iter().enumerate() {
            if px.alpha() > 0 {
                let y = i / 200;
                assert!((48..=52).contains(&y), "pixel at row {y}");
            }
        }
        assert!(painted(&p) > 180);
    }

    #[test]
    fn rough_line_wobbles_but_stays_close() {
        let mut p = pixmap();
        Sketch::new(&mut p, 3, 1.0, 2.0).line((10.0, 50.0), (190.0, 50.0), pen());
        let mut off_row = 0;
        for (i, px) in p.pixels().iter().enumerate() {
            if px.alpha() > 0 {
                let y = (i / 200) as i32;
                assert!((y - 50).abs() <= 8, "strayed to row {y}");
                if (y - 50).abs() >= 2 {
                    off_row += 1;
                }
            }
        }
        assert!(off_row > 0, "a rough line should leave the exact row");
    }

    #[test]
    fn same_seed_same_picture() {
        let mut a = pixmap();
        let mut b = pixmap();
        Sketch::new(&mut a, 9, 1.0, 2.0).ellipse(100.0, 50.0, 80.0, 30.0, pen());
        Sketch::new(&mut b, 9, 1.0, 2.0).ellipse(100.0, 50.0, 80.0, 30.0, pen());
        assert_eq!(a.data(), b.data());
        let mut c = pixmap();
        Sketch::new(&mut c, 10, 1.0, 2.0).ellipse(100.0, 50.0, 80.0, 30.0, pen());
        assert_ne!(a.data(), c.data());
    }

    #[test]
    fn ellipse_reaches_its_extremes() {
        let mut p = pixmap();
        Sketch::new(&mut p, 1, 0.0, 2.0).ellipse(100.0, 50.0, 80.0, 30.0, pen());
        let at = |x: u32, y: u32| p.pixel(x, y).is_some_and(|c| c.alpha() > 0);
        assert!(at(180, 50) || at(179, 50) || at(181, 50), "right");
        assert!(at(20, 50) || at(21, 50) || at(19, 50), "left");
        assert!(at(100, 20) || at(100, 21) || at(100, 19), "top");
        assert!(!at(100, 50), "centre is empty");
    }

    #[test]
    fn hachure_stays_inside_the_polygon() {
        let mut p = pixmap();
        let poly = [(40.0, 20.0), (160.0, 20.0), (160.0, 80.0), (40.0, 80.0)];
        Sketch::new(&mut p, 1, 0.0, 2.0).hachure(&poly, 8.0, pen());
        assert!(painted(&p) > 200);
        for (i, px) in p.pixels().iter().enumerate() {
            if px.alpha() > 0 {
                let (x, y) = ((i % 200) as f32, (i / 200) as f32);
                assert!(
                    (38.0..=162.0).contains(&x) && (18.0..=82.0).contains(&y),
                    "hatch at ({x},{y}) is outside"
                );
            }
        }
    }

    #[test]
    fn dashed_line_has_gaps() {
        let mut p = pixmap();
        Sketch::new(&mut p, 1, 0.0, 2.0).line(
            (10.0, 50.0),
            (190.0, 50.0),
            Pen {
                dashed: true,
                ..pen()
            },
        );
        let row: Vec<bool> = (0..200)
            .map(|x| p.pixel(x, 50).is_some_and(|c| c.alpha() > 0))
            .collect();
        assert!(
            row[10..190].iter().any(|on| !on),
            "no gaps in a dashed line"
        );
        assert!(row[10..190].iter().any(|on| *on));
    }

    #[test]
    fn clear_erases() {
        let mut p = pixmap();
        let mut s = Sketch::new(&mut p, 1, 0.0, 2.0);
        s.line((10.0, 50.0), (190.0, 50.0), pen());
        let hole =
            PathBuilder::from_rect(tiny_skia::Rect::from_ltrb(90.0, 40.0, 110.0, 60.0).unwrap());
        s.clear_path(&hole);
        assert!(p.pixel(100, 50).is_none_or(|c| c.alpha() == 0));
        assert!(p.pixel(50, 50).is_some_and(|c| c.alpha() > 0));
    }
}
