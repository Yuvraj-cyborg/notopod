//! Hand-drawn strokes.
//!
//! The same trick as rough.js, at braille resolution: endpoints are
//! nudged, edges bow slightly, corners overshoot a little, ellipses
//! breathe, fills are hatched. `roughness` scales all of it; 0 gives
//! exact geometry. Randomness comes from a seeded generator so a shape
//! looks the same on every frame.

use std::f32::consts::{PI, TAU};

use ratatui::style::Color;

use crate::raster::Raster;

/// A small, fast, seedable generator (SplitMix64).
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// A generator seeded from `seed`.
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `[-1, 1)`.
    pub fn signed(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
    }

    /// A number in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Draws onto a raster with one colour, optional dashing and clipping.
pub struct Pen<'a> {
    raster: &'a mut Raster,
    color: Option<Color>,
    /// `(on, off)` dot counts for dashed strokes.
    dash: Option<(u32, u32)>,
    dash_pos: u32,
    /// The last dot drawn, so a curve sampled finely does not count the
    /// same dot twice against the dash pattern.
    last: Option<(i32, i32)>,
    clip: Option<&'a dyn Fn(i32, i32) -> bool>,
}

impl<'a> Pen<'a> {
    /// A solid pen.
    pub fn new(raster: &'a mut Raster, color: Option<Color>) -> Self {
        Self {
            raster,
            color,
            dash: None,
            dash_pos: 0,
            last: None,
            clip: None,
        }
    }

    /// Makes the stroke dashed.
    pub fn dashed(mut self, dashed: bool) -> Self {
        self.dash = dashed.then_some((4, 3));
        self
    }

    /// Only draws dots for which `clip` is true.
    pub fn clipped(mut self, clip: &'a dyn Fn(i32, i32) -> bool) -> Self {
        self.clip = Some(clip);
        self
    }

    /// Restarts the dash pattern, so each new stroke starts with a dash.
    pub fn restart_dash(&mut self) {
        self.dash_pos = 0;
        self.last = None;
    }

    /// One dot, subject to dashing and clipping.
    pub fn dot(&mut self, x: i32, y: i32) {
        if self.last == Some((x, y)) {
            return;
        }
        self.last = Some((x, y));
        let visible = match self.dash {
            Some((on, off)) => {
                let v = self.dash_pos % (on + off) < on;
                self.dash_pos += 1;
                v
            }
            None => true,
        };
        if visible && self.clip.is_none_or(|c| c(x, y)) {
            self.raster.plot(x, y, self.color);
        }
    }

    /// A straight run of dots (Bresenham).
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.dot(x, y);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Connects consecutive points with straight runs.
    pub fn polyline(&mut self, points: &[(f32, f32)]) {
        for w in points.windows(2) {
            let (a, b) = (w[0], w[1]);
            self.line(
                a.0.round() as i32,
                a.1.round() as i32,
                b.0.round() as i32,
                b.1.round() as i32,
            );
        }
    }
}

/// A hand-drawn straight stroke from `a` to `b` (dot coordinates).
pub fn stroke(pen: &mut Pen<'_>, a: (f32, f32), b: (f32, f32), roughness: f32, rng: &mut Rng) {
    if roughness <= 0.0 {
        pen.polyline(&[a, b]);
        return;
    }
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        pen.polyline(&[a, b]);
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let (px, py) = (-uy, ux);

    // Nudge the ends, and let the stroke run a little past its target the
    // way a quick pen stroke does.
    let jitter = 0.9 * roughness;
    let overshoot = rng.unit() * 1.6 * roughness;
    let a2 = (a.0 + rng.signed() * jitter, a.1 + rng.signed() * jitter);
    let b2 = (
        b.0 + rng.signed() * jitter + ux * overshoot,
        b.1 + rng.signed() * jitter + uy * overshoot,
    );

    // A single bow: the control point of a quadratic curve sits off the
    // midpoint, further for longer strokes.
    let bow = roughness * (len / 14.0).clamp(0.4, 2.2) * rng.signed();
    let mid = (
        f32::midpoint(a2.0, b2.0) + px * bow,
        f32::midpoint(a2.1, b2.1) + py * bow,
    );
    let steps = (len * 1.5).ceil().max(2.0) as usize;
    let mut points = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let it = 1.0 - t;
        points.push((
            it * it * a2.0 + 2.0 * it * t * mid.0 + t * t * b2.0,
            it * it * a2.1 + 2.0 * it * t * mid.1 + t * t * b2.1,
        ));
    }
    pen.polyline(&points);
}

/// An elliptical arc from angle `from` to `to` (radians, clockwise on
/// screen), with a slowly wandering radius when rough.
#[allow(clippy::too_many_arguments)]
pub fn arc(
    pen: &mut Pen<'_>,
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    from: f32,
    to: f32,
    roughness: f32,
    rng: &mut Rng,
) {
    // Radius offsets at a few anchor angles, blended smoothly between.
    const ANCHORS: usize = 8;
    let span = to - from;
    let steps = ((rx.max(ry) * span.abs() * 1.2).ceil() as usize).max(6);
    let amp = if roughness > 0.0 {
        roughness * (rx.min(ry) / 6.0).clamp(0.4, 1.4)
    } else {
        0.0
    };
    let mut offsets = [0.0f32; ANCHORS + 1];
    for o in offsets.iter_mut().take(ANCHORS) {
        *o = rng.signed() * amp;
    }
    offsets[ANCHORS] = offsets[0];

    let mut points = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let theta = from + span * t;
        // Position around the full circle decides the wobble, so an arc
        // that is part of a rounded corner still blends with its
        // neighbours.
        let k = (theta.rem_euclid(TAU) / TAU) * ANCHORS as f32;
        let (i0, frac) = (k.floor() as usize % ANCHORS, k - k.floor());
        let smooth = (1.0 - (frac * PI).cos()) / 2.0;
        let off = offsets[i0] * (1.0 - smooth) + offsets[i0 + 1] * smooth;
        points.push((cx + (rx + off) * theta.cos(), cy + (ry + off) * theta.sin()));
    }
    pen.polyline(&points);
}

/// A closed ellipse.
pub fn ellipse(
    pen: &mut Pen<'_>,
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    roughness: f32,
    rng: &mut Rng,
) {
    let start = if roughness > 0.0 {
        rng.unit() * TAU
    } else {
        0.0
    };
    arc(pen, cx, cy, rx, ry, start, start + TAU, roughness, rng);
}

/// An arrowhead whose tip is at `tip`, pointing away from `from`.
pub fn arrowhead(
    pen: &mut Pen<'_>,
    tip: (f32, f32),
    from: (f32, f32),
    roughness: f32,
    rng: &mut Rng,
) {
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.5 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    // Long enough to read as a chevron at braille resolution: about two
    // and a half cells back, most of a row up and down.
    let size = 6.0;
    let angle = 0.6;
    for side in [-1.0f32, 1.0] {
        let a = angle * side;
        // Rotate the reversed direction by ±angle.
        let (rx, ry) = (-ux * a.cos() + uy * a.sin(), -ux * a.sin() - uy * a.cos());
        let end = (tip.0 + rx * size, tip.1 + ry * size);
        pen.restart_dash();
        stroke(pen, tip, end, roughness * 0.5, rng);
    }
}

/// Hatches the area where `inside` is true, within the dot box
/// `(left, top)`-`(right, bottom)`, with diagonal strokes.
#[allow(clippy::too_many_arguments)]
pub fn hachure(
    raster: &mut Raster,
    color: Option<Color>,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    inside: &dyn Fn(i32, i32) -> bool,
    roughness: f32,
    rng: &mut Rng,
) {
    let spacing = 5;
    let height = bottom - top;
    let mut pen = Pen::new(raster, color).clipped(inside);
    let mut c = left - height;
    while c <= right {
        let jitter = if roughness > 0.0 {
            rng.signed() * roughness
        } else {
            0.0
        };
        let a = (c as f32 + jitter, top as f32);
        let b = ((c + height) as f32 + jitter, bottom as f32);
        stroke(&mut pen, a, b, roughness * 0.6, rng);
        c += spacing;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic_and_bounded() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            let x = a.signed();
            assert!(x.to_bits() == b.signed().to_bits());
            assert!((-1.0..1.0).contains(&x));
            let u = a.unit();
            assert!(u.to_bits() == b.unit().to_bits());
            assert!((0.0..1.0).contains(&u));
        }
        assert!(Rng::new(1).unit().to_bits() != Rng::new(2).unit().to_bits());
    }

    #[test]
    fn exact_line_is_bresenham() {
        let mut r = Raster::new(4, 1);
        Pen::new(&mut r, None).line(0, 0, 7, 0);
        for x in 0..8 {
            assert!(r.get(x, 0), "dot {x} missing");
        }
        assert!(!r.get(0, 1));
    }

    #[test]
    fn dashed_line_has_gaps() {
        let mut r = Raster::new(7, 1);
        Pen::new(&mut r, None).dashed(true).line(0, 0, 13, 0);
        let on: Vec<bool> = (0..14).map(|x| r.get(x, 0)).collect();
        assert_eq!(
            on,
            [
                true, true, true, true, false, false, false, true, true, true, true, false, false,
                false
            ]
        );
    }

    #[test]
    fn repeated_dots_do_not_advance_the_dash() {
        let mut r = Raster::new(4, 1);
        let mut pen = Pen::new(&mut r, None).dashed(true);
        for _ in 0..10 {
            pen.dot(0, 0);
        }
        pen.line(0, 0, 7, 0);
        let on: Vec<bool> = (0..8).map(|x| r.get(x, 0)).collect();
        assert_eq!(on, [true, true, true, true, false, false, false, true]);
    }

    #[test]
    fn rough_stroke_stays_near_the_line() {
        let mut r = Raster::new(20, 3);
        let mut rng = Rng::new(3);
        stroke(
            &mut Pen::new(&mut r, None),
            (2.0, 6.0),
            (36.0, 6.0),
            1.0,
            &mut rng,
        );
        let mut count = 0;
        for x in 0..40 {
            for y in 0..12 {
                if r.get(x, y) {
                    count += 1;
                    assert!((3..=9).contains(&y), "dot at ({x},{y}) strayed too far");
                }
            }
        }
        assert!(count >= 30, "stroke has only {count} dots");
    }

    #[test]
    fn ellipse_touches_all_four_sides() {
        let mut r = Raster::new(10, 3);
        let mut rng = Rng::new(1);
        ellipse(
            &mut Pen::new(&mut r, None),
            10.0,
            6.0,
            8.0,
            5.0,
            0.0,
            &mut rng,
        );
        assert!(r.get(18, 6), "right");
        assert!(r.get(2, 6), "left");
        assert!(r.get(10, 1), "top");
        assert!(r.get(10, 11), "bottom");
        assert!(!r.get(10, 6), "centre stays empty");
    }

    #[test]
    fn hachure_only_fills_inside() {
        let mut r = Raster::new(10, 3);
        let mut rng = Rng::new(1);
        let inside = |x: i32, y: i32| (4..=15).contains(&x) && (2..=9).contains(&y);
        hachure(&mut r, None, 0, 0, 19, 11, &inside, 0.0, &mut rng);
        let mut filled = 0;
        for x in 0..20 {
            for y in 0..12 {
                if r.get(x, y) {
                    assert!(inside(x, y), "dot outside at ({x},{y})");
                    filled += 1;
                }
            }
        }
        assert!(filled > 10);
    }
}
