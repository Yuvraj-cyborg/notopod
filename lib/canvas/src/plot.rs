//! Axes and the curves drawn on them, in whatever units a renderer works
//! in: pixels for pictures, dots for braille.

use crate::expr::{self, Expr};
use crate::model::{Drawing, Shape};

/// The x values axes cover when the block does not say.
pub(crate) const DEFAULT_X: (f64, f64) = (-5.0, 5.0);

/// The y values axes cover when the block does not say.
pub(crate) const DEFAULT_Y: (f64, f64) = (-3.0, 3.0);

/// The ranges of an `axes` statement, defaults filled in.
pub(crate) fn ranges(x: Option<&str>, y: Option<&str>) -> ((f64, f64), (f64, f64)) {
    (
        x.and_then(expr::range).unwrap_or(DEFAULT_X),
        y.and_then(expr::range).unwrap_or(DEFAULT_Y),
    )
}

/// The `axes` a `plot` is drawn on: the closest one before it in the
/// block, or the last one in the block for a shape that is not in it
/// (a shape being placed).
pub(crate) fn axes_for<'d>(drawing: &'d Drawing, plot: &Shape) -> Option<&'d Shape> {
    let end = drawing
        .shapes
        .iter()
        .position(|s| std::ptr::eq(s, plot))
        .unwrap_or(drawing.shapes.len());
    drawing.shapes[..end]
        .iter()
        .rev()
        .find(|s| matches!(s, Shape::Axes { .. }))
}

/// Where values land: the area inside a pair of axes, and the ranges it
/// covers. Screen y grows downwards; values grow upwards.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub x: (f64, f64),
    pub y: (f64, f64),
}

impl Frame {
    /// The screen position of value `x`.
    pub fn sx(&self, x: f64) -> f32 {
        let t = (x - self.x.0) / (self.x.1 - self.x.0);
        self.left + t as f32 * (self.right - self.left)
    }

    /// The screen position of value `y`.
    pub fn sy(&self, y: f64) -> f32 {
        let t = (y - self.y.0) / (self.y.1 - self.y.0);
        self.bottom - t as f32 * (self.bottom - self.top)
    }

    /// Screen y of the x axis: through zero when zero is in view, along
    /// the bottom otherwise.
    pub fn x_axis(&self) -> f32 {
        if self.y.0 <= 0.0 && 0.0 <= self.y.1 {
            self.sy(0.0)
        } else {
            self.bottom
        }
    }

    /// Screen x of the y axis: through zero when zero is in view, along
    /// the left otherwise.
    pub fn y_axis(&self) -> f32 {
        if self.x.0 <= 0.0 && 0.0 <= self.x.1 {
            self.sx(0.0)
        } else {
            self.left
        }
    }

    /// Whether the two axes cross at the origin.
    pub fn origin_in_view(&self) -> bool {
        self.x.0 <= 0.0 && 0.0 <= self.x.1 && self.y.0 <= 0.0 && 0.0 <= self.y.1
    }
}

/// Tick values across `range`, at a round step, about one every
/// `spacing` screen units over `length`. Returns the step too, for
/// formatting the labels.
pub(crate) fn ticks(range: (f64, f64), length: f32, spacing: f32) -> (Vec<f64>, f64) {
    let span = range.1 - range.0;
    let step = expr::nice_step(span, f64::from((length / spacing.max(1.0)).max(1.0)));
    let first = (range.0 / step).ceil() as i64;
    let last = (range.1 / step).floor() as i64;
    // A pathological range could ask for millions of ticks.
    if last - first > 200 {
        return (Vec::new(), step);
    }
    ((first..=last).map(|i| i as f64 * step).collect(), step)
}

/// Ticks along the x axis, leaving out the right end, where its arrowhead
/// is. `spacing` is in screen units.
pub(crate) fn x_ticks(f: &Frame, spacing: f32) -> (Vec<f64>, f64) {
    let (mut t, step) = ticks(f.x, f.right - f.left, spacing);
    t.retain(|v| f.x.1 - v > step * 0.01);
    (t, step)
}

/// Ticks up the y axis, leaving out the top, where its arrowhead is.
pub(crate) fn y_ticks(f: &Frame, spacing: f32) -> (Vec<f64>, f64) {
    let (mut t, step) = ticks(f.y, f.bottom - f.top, spacing);
    t.retain(|v| f.y.1 - v > step * 0.01);
    (t, step)
}

/// The curve of `expr` across `frame`, sampled `samples` times, as runs of
/// screen points. Runs stop where the curve is undefined or leaves the
/// frame, and end exactly on the frame's edge where it crosses one, so an
/// asymptote does not draw a line from top to bottom.
pub(crate) fn curve(
    expr: &Expr,
    only: Option<(f64, f64)>,
    frame: &Frame,
    samples: usize,
) -> Vec<Vec<(f32, f32)>> {
    let (mut lo, mut hi) = frame.x;
    if let Some((a, b)) = only {
        lo = lo.max(a);
        hi = hi.min(b);
    }
    if lo >= hi {
        return Vec::new();
    }
    let samples = samples.clamp(16, 4000);
    let (y0, y1) = frame.y;
    let inside = |y: f64| y.is_finite() && y >= y0 && y <= y1;
    let mut runs: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut run: Vec<(f32, f32)> = Vec::new();
    let mut prev: Option<(f64, f64)> = None;
    for i in 0..=samples {
        let x = lo + (hi - lo) * i as f64 / samples as f64;
        let y = expr.eval(x);
        match (prev, inside(y)) {
            (Some((px, py)), true) if !inside(py) && py.is_finite() => {
                // Coming back into view: start on the edge.
                let edge = if py > y1 { y1 } else { y0 };
                let ex = px + (x - px) * (edge - py) / (y - py);
                run.push((frame.sx(ex), frame.sy(edge)));
                run.push((frame.sx(x), frame.sy(y)));
            }
            (_, true) => run.push((frame.sx(x), frame.sy(y))),
            (Some((px, py)), false) if inside(py) && y.is_finite() => {
                // Leaving the view: end on the edge.
                let edge = if y > y1 { y1 } else { y0 };
                let ex = px + (x - px) * (edge - py) / (y - py);
                run.push((frame.sx(ex), frame.sy(edge)));
                runs.push(std::mem::take(&mut run));
            }
            (_, false) => {
                if !run.is_empty() {
                    runs.push(std::mem::take(&mut run));
                }
            }
        }
        prev = Some((x, y));
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs.retain(|r| r.len() >= 2);
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Frame {
        Frame {
            left: 0.0,
            top: 0.0,
            right: 100.0,
            bottom: 50.0,
            x: (-5.0, 5.0),
            y: (-1.0, 1.0),
        }
    }

    #[test]
    fn values_map_to_the_screen_with_y_upwards() {
        let f = frame();
        assert!((f.sx(-5.0) - 0.0).abs() < 1e-4 && (f.sx(5.0) - 100.0).abs() < 1e-4);
        assert!((f.sy(1.0) - 0.0).abs() < 1e-4, "the top value at the top");
        assert!((f.x_axis() - 25.0).abs() < 1e-4 && (f.y_axis() - 50.0).abs() < 1e-4);
        let off = Frame {
            x: (1.0, 3.0),
            y: (2.0, 4.0),
            ..f
        };
        assert!(
            (off.x_axis() - 50.0).abs() < 1e-4,
            "no zero in view: along the bottom"
        );
        assert!(off.y_axis().abs() < 1e-4, "and along the left");
        assert!(!off.origin_in_view());
    }

    #[test]
    fn ticks_are_round_and_inside() {
        let (t, step) = ticks((-5.0, 5.0), 100.0, 20.0);
        assert!((step - 2.0).abs() < 1e-9, "{step}");
        assert_eq!(t, vec![-4.0, -2.0, 0.0, 2.0, 4.0]);
        assert!(ticks((0.0, 1e12), 100.0, 0.001).0.len() <= 201);
    }

    #[test]
    fn a_curve_stays_inside_and_breaks_at_asymptotes() {
        let f = frame();
        let sine = curve(&Expr::parse("sin(x)").unwrap(), None, &f, 200);
        assert_eq!(sine.len(), 1, "one unbroken run");
        assert!(sine[0].iter().all(|(_, y)| (-1e-3..=50.001).contains(y)));

        let tan = curve(&Expr::parse("tan(x)").unwrap(), None, &f, 400);
        assert!(tan.len() >= 3, "broken at each asymptote: {}", tan.len());
        for run in &tan {
            assert!(run.iter().all(|(_, y)| (-1e-3..=50.001).contains(y)));
        }

        let half = curve(&Expr::parse("x/10").unwrap(), Some((0.0, 9.0)), &f, 100);
        assert!(half[0][0].0 >= 49.9, "starts at x = 0, halfway across");

        let none = curve(&Expr::parse("sqrt(x - 100)").unwrap(), None, &f, 100);
        assert!(none.is_empty(), "undefined everywhere in view");
    }

    #[test]
    fn plots_find_the_axes_above_them() {
        let d = crate::parse::parse(
            "plot \"x\"\naxes 0,0 10x5\nplot \"x^2\"\naxes 20,0 10x5 x=0..1\nplot \"1\"\n",
        );
        assert!(
            axes_for(&d, &d.shapes[0]).is_none(),
            "nothing above the first"
        );
        assert!(
            matches!(axes_for(&d, &d.shapes[2]), Some(Shape::Axes { rect, .. }) if rect.x == 0)
        );
        assert!(
            matches!(axes_for(&d, &d.shapes[4]), Some(Shape::Axes { rect, .. }) if rect.x == 20)
        );
    }
}
