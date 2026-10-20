//! What a scene is made of: mobjects, in Manim's units.
//!
//! The frame is 14⅑ by 8 units with the origin in the middle and y going
//! up, as in Manim. A shape is cubic Bézier curves, the way Manim keeps
//! them too, so moving, scaling and turning it is one affine map over its
//! points. Text and formulas are boxes that remember their centre, size
//! and angle, and are typeset when drawn.
//!
//! Mobjects live in an [`Arena`] and refer to their children by index,
//! so a group and the shapes in it are the same objects, as in Python:
//! changing a circle after putting it in a group changes the group.

use std::rc::Rc;

use super::parse::Expr;
use super::Error;

pub(crate) type P = [f64; 2];
pub(crate) type Rgb = (u8, u8, u8);
pub(crate) type Cubic = [P; 4];

pub(crate) const FRAME_W: f64 = 128.0 / 9.0;
pub(crate) const FRAME_H: f64 = 8.0;

pub(crate) const WHITE: Rgb = (0xFF, 0xFF, 0xFF);

/// `x' = xx·x + xy·y + x0`, `y' = yx·x + yy·y + y0`, as Cairo names them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Affine {
    xx: f64,
    yx: f64,
    xy: f64,
    yy: f64,
    x0: f64,
    y0: f64,
}

impl Affine {
    pub(crate) const IDENTITY: Self = Self::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    pub(crate) const fn new(xx: f64, yx: f64, xy: f64, yy: f64, x0: f64, y0: f64) -> Self {
        Self {
            xx,
            yx,
            xy,
            yy,
            x0,
            y0,
        }
    }

    pub(crate) fn shift(by: P) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, by[0], by[1])
    }

    /// Scaling by `sx`, `sy` with `about` staying put.
    pub(crate) fn scale(sx: f64, sy: f64, about: P) -> Self {
        Self::new(
            sx,
            0.0,
            0.0,
            sy,
            about[0] * (1.0 - sx),
            about[1] * (1.0 - sy),
        )
    }

    /// Turning by `angle` radians anticlockwise about `about`.
    pub(crate) fn rotate(angle: f64, about: P) -> Self {
        let (sin, cos) = angle.sin_cos();
        let [x, y] = about;
        Self::new(
            cos,
            sin,
            -sin,
            cos,
            x - cos * x + sin * y,
            y - sin * x - cos * y,
        )
    }

    pub(crate) fn apply(&self, p: P) -> P {
        [
            self.xx * p[0] + self.xy * p[1] + self.x0,
            self.yx * p[0] + self.yy * p[1] + self.y0,
        ]
    }

    /// `self` after `first`.
    pub(crate) fn after(&self, first: &Self) -> Self {
        Self::new(
            self.xx * first.xx + self.xy * first.yx,
            self.yx * first.xx + self.yy * first.yx,
            self.xx * first.xy + self.xy * first.yy,
            self.yx * first.xy + self.yy * first.yy,
            self.xx * first.x0 + self.xy * first.y0 + self.x0,
            self.yx * first.x0 + self.yy * first.y0 + self.y0,
        )
    }

    /// The scale and turn of a map that keeps shapes' proportions and does
    /// not mirror them; `None` for anything else.
    fn similarity(&self) -> Option<(f64, f64)> {
        let scale = self.xx.hypot(self.yx);
        let same = (self.xx - self.yy).abs() < 1e-9 && (self.yx + self.xy).abs() < 1e-9;
        (same && scale > 0.0).then(|| (scale, self.yx.atan2(self.xx)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Style {
    pub stroke: Rgb,
    pub stroke_width: f64,
    pub stroke_opacity: f64,
    pub fill: Rgb,
    pub fill_opacity: f64,
}

impl Style {
    /// Manim's default: a 4-wide stroke and an empty fill of the same colour.
    pub(crate) fn outline(color: Rgb) -> Self {
        Self {
            stroke: color,
            stroke_width: 4.0,
            stroke_opacity: 1.0,
            fill: color,
            fill_opacity: 0.0,
        }
    }

    /// A filled shape without an outline, like `Dot`.
    pub(crate) fn solid(color: Rgb) -> Self {
        Self {
            stroke_width: 0.0,
            fill_opacity: 1.0,
            ..Self::outline(color)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Item {
    Path {
        subpaths: Vec<Vec<Cubic>>,
        closed: bool,
        style: Style,
        /// Dash length in units; gaps are as long.
        dash: Option<f64>,
    },
    Tex {
        tex: String,
        /// Display style, for formulas; text style for words.
        display: bool,
        center: P,
        /// One em, in units.
        em: f64,
        angle: f64,
        /// Width and height before turning, in units.
        size: P,
        color: Rgb,
        opacity: f64,
    },
}

/// A function of `x` from the scene, kept for a graph so a label can be
/// put at any `x` later.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Closure {
    pub params: Vec<String>,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Meta {
    None,
    /// A line or arrow: where it starts and ends.
    Line {
        start: P,
        end: P,
    },
    /// A coordinate system: its ranges, how coordinates become points, and
    /// its two axes.
    Axes {
        x: [f64; 3],
        y: [f64; 3],
        map: Affine,
        x_axis: usize,
        y_axis: usize,
    },
    /// A function drawn on axes.
    Graph {
        map: Affine,
        func: Rc<Closure>,
        start: P,
        end: P,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mob {
    pub items: Vec<Item>,
    pub children: Vec<usize>,
    pub meta: Meta,
    /// What `generate_target` made, for `MoveToTarget`.
    pub target: Option<usize>,
}

impl Mob {
    pub(crate) fn new(items: Vec<Item>) -> Self {
        Self {
            items,
            children: Vec::new(),
            meta: Meta::None,
            target: None,
        }
    }

    pub(crate) fn group(children: Vec<usize>) -> Self {
        Self {
            children,
            ..Self::new(Vec::new())
        }
    }

    pub(crate) fn path(subpaths: Vec<Vec<Cubic>>, closed: bool, style: Style) -> Self {
        Self::new(vec![Item::Path {
            subpaths,
            closed,
            style,
            dash: None,
        }])
    }
}

/// Mobjects by index. A scene makes few enough that nothing is freed.
#[derive(Debug, Default)]
pub(crate) struct Arena {
    mobs: Vec<Mob>,
}

/// More mobjects than any scene drawn by hand needs.
const MAX_MOBS: usize = 20_000;

impl Arena {
    pub(crate) fn add(&mut self, mob: Mob) -> Result<usize, Error> {
        if self.mobs.len() >= MAX_MOBS {
            return Err(Error::unsupported("this many mobjects"));
        }
        self.mobs.push(mob);
        Ok(self.mobs.len() - 1)
    }

    pub(crate) fn get(&self, id: usize) -> &Mob {
        &self.mobs[id]
    }

    pub(crate) fn get_mut(&mut self, id: usize) -> &mut Mob {
        &mut self.mobs[id]
    }

    /// `id` and everything under it, each once, parents first.
    pub(crate) fn family(&self, id: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(m) = stack.pop() {
            if out.contains(&m) {
                continue;
            }
            out.push(m);
            stack.extend(self.mobs[m].children.iter().rev());
        }
        out
    }

    pub(crate) fn transform(&mut self, id: usize, t: &Affine) -> Result<(), Error> {
        for m in self.family(id) {
            let mob = &mut self.mobs[m];
            for item in &mut mob.items {
                match item {
                    Item::Path { subpaths, .. } => {
                        for cubic in subpaths.iter_mut().flatten() {
                            for p in cubic.iter_mut() {
                                *p = t.apply(*p);
                            }
                        }
                    }
                    Item::Tex {
                        center,
                        em,
                        angle,
                        size,
                        ..
                    } => {
                        let Some((s, turn)) = t.similarity() else {
                            return Err(Error::unsupported("stretching or flipping text"));
                        };
                        *center = t.apply(*center);
                        *em *= s;
                        *size = [size[0] * s, size[1] * s];
                        *angle += turn;
                    }
                }
            }
            match &mut mob.meta {
                Meta::None => {}
                Meta::Line { start, end } | Meta::Graph { start, end, .. } => {
                    *start = t.apply(*start);
                    *end = t.apply(*end);
                }
                Meta::Axes { map, .. } => *map = t.after(map),
            }
            if let Meta::Graph { map, .. } = &mut mob.meta {
                *map = t.after(map);
            }
        }
        Ok(())
    }

    /// The smallest box holding everything in `id`, strokes not counted.
    pub(crate) fn bounds(&self, id: usize) -> Option<(P, P)> {
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        let mut grow = |p: P| {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        };
        for m in self.family(id) {
            for item in &self.mobs[m].items {
                match item {
                    Item::Path { subpaths, .. } => {
                        subpaths.iter().flatten().flatten().for_each(|p| grow(*p));
                    }
                    Item::Tex {
                        center,
                        angle,
                        size,
                        ..
                    } => {
                        let (s, c) = angle.sin_cos();
                        for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                            let (x, y) = (dx * size[0] / 2.0, dy * size[1] / 2.0);
                            grow([center[0] + c * x - s * y, center[1] + s * x + c * y]);
                        }
                    }
                }
            }
        }
        (lo[0] <= hi[0]).then_some((lo, hi))
    }

    /// The point of `id`'s box in direction `dir`: an edge's middle, a
    /// corner, or the centre for `ORIGIN`. An empty mobject is a point at
    /// the origin.
    pub(crate) fn critical(&self, id: usize, dir: P) -> P {
        let Some((lo, hi)) = self.bounds(id) else {
            return [0.0, 0.0];
        };
        let pick = |i: usize| {
            if dir[i] > 0.0 {
                hi[i]
            } else if dir[i] < 0.0 {
                lo[i]
            } else {
                f64::midpoint(lo[i], hi[i])
            }
        };
        [pick(0), pick(1)]
    }

    pub(crate) fn center(&self, id: usize) -> P {
        self.critical(id, [0.0, 0.0])
    }

    pub(crate) fn size(&self, id: usize) -> P {
        self.bounds(id)
            .map_or([0.0, 0.0], |(lo, hi)| [hi[0] - lo[0], hi[1] - lo[1]])
    }

    /// A new mobject like `id`, its children copied too.
    pub(crate) fn deep_copy(&mut self, id: usize) -> Result<usize, Error> {
        let mut mob = self.mobs[id].clone();
        let old = std::mem::take(&mut mob.children);
        for child in old {
            let copy = self.deep_copy(child)?;
            mob.children.push(copy);
        }
        // Axes name their own axes among their children.
        if let Meta::Axes { x_axis, y_axis, .. } = &mut mob.meta {
            let src = &self.mobs[id].children;
            if let Some(i) = src.iter().position(|c| c == x_axis) {
                *x_axis = mob.children[i];
            }
            if let Some(i) = src.iter().position(|c| c == y_axis) {
                *y_axis = mob.children[i];
            }
        }
        self.add(mob)
    }

    /// Everything in `id`'s family, mutably, one item at a time.
    pub(crate) fn each_item(&mut self, id: usize, mut f: impl FnMut(&mut Item)) {
        for m in self.family(id) {
            self.mobs[m].items.iter_mut().for_each(&mut f);
        }
    }
}

// ----- shapes, as cubic curves -----

pub(crate) fn line(a: P, b: P) -> Cubic {
    let at = |t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    [a, at(1.0 / 3.0), at(2.0 / 3.0), b]
}

pub(crate) fn polyline(points: &[P], closed: bool) -> Vec<Cubic> {
    let mut out: Vec<Cubic> = points.windows(2).map(|w| line(w[0], w[1])).collect();
    if closed && points.len() > 2 {
        out.push(line(points[points.len() - 1], points[0]));
    }
    out
}

/// An arc of radius `r` about `c`, from `start` turning `sweep` radians
/// anticlockwise, in pieces of at most a quarter turn.
pub(crate) fn arc(center: P, radius: f64, start: f64, sweep: f64) -> Vec<Cubic> {
    let pieces = (sweep.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / pieces as f64;
    // How far a quarter circle's handles reach, scaled to the piece.
    let reach = 4.0 / 3.0 * (step / 4.0).tan();
    let at = |cos: f64, sin: f64| [center[0] + radius * cos, center[1] + radius * sin];
    (0..pieces)
        .map(|i| {
            let (from, to) = (start + step * i as f64, start + step * (i + 1) as f64);
            let (s0, c0) = from.sin_cos();
            let (s1, c1) = to.sin_cos();
            [
                at(c0, s0),
                at(c0 - reach * s0, s0 + reach * c0),
                at(c1 + reach * s1, s1 - reach * c1),
                at(c1, s1),
            ]
        })
        .collect()
}

pub(crate) fn circle(r: f64) -> Vec<Cubic> {
    arc([0.0, 0.0], r, 0.0, std::f64::consts::TAU)
}

pub(crate) fn rect(w: f64, h: f64) -> Vec<Cubic> {
    let (x, y) = (w / 2.0, h / 2.0);
    polyline(&[[x, y], [-x, y], [-x, -y], [x, -y]], true)
}

pub(crate) fn rounded_rect(width: f64, height: f64, radius: f64) -> Vec<Cubic> {
    let radius = radius.min(width / 2.0).min(height / 2.0);
    if radius <= 0.0 {
        return rect(width, height);
    }
    // The corners' centres, and where each straight side ends.
    let (cx, cy) = (width / 2.0 - radius, height / 2.0 - radius);
    let quarter = std::f64::consts::FRAC_PI_2;
    let sides = [
        [-cx, cy + radius],
        [-cx - radius, -cy],
        [cx, -cy - radius],
        [cx + radius, cy],
    ];
    let mut out = Vec::new();
    for (i, corner) in [[cx, cy], [-cx, cy], [-cx, -cy], [cx, -cy]]
        .into_iter()
        .enumerate()
    {
        out.extend(arc(corner, radius, quarter * i as f64, quarter));
        let from = out.last().map_or(sides[i], |c| c[3]);
        out.push(line(from, sides[i]));
    }
    out
}

/// The corners of a regular `n`-gon of circumradius `r`, the first at
/// `start` radians.
pub(crate) fn regular(n: usize, r: f64, start: f64) -> Vec<P> {
    (0..n)
        .map(|i| {
            let a = start + std::f64::consts::TAU * i as f64 / n as f64;
            [r * a.cos(), r * a.sin()]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: P, b: P) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn maps_compose_and_turn_about_a_point() {
        let turn = Affine::rotate(std::f64::consts::FRAC_PI_2, [1.0, 0.0]);
        assert!(close(turn.apply([2.0, 0.0]), [1.0, 1.0]));
        let both = Affine::shift([0.0, 1.0]).after(&Affine::scale(2.0, 2.0, [0.0, 0.0]));
        assert!(close(both.apply([1.0, 1.0]), [2.0, 3.0]));
        assert_eq!(Affine::scale(1.0, -1.0, [0.0, 0.0]).similarity(), None);
    }

    #[test]
    fn a_group_is_its_children_and_copies_are_deep() {
        let mut arena = Arena::default();
        let a = arena
            .add(Mob::path(vec![circle(1.0)], true, Style::outline(WHITE)))
            .expect("add");
        let b = arena
            .add(Mob::path(vec![rect(2.0, 2.0)], true, Style::outline(WHITE)))
            .expect("add");
        let g = arena.add(Mob::group(vec![a, b])).expect("add");
        arena
            .transform(b, &Affine::shift([3.0, 0.0]))
            .expect("moves");
        assert!(close(arena.center(g), [1.5, 0.0]), "the group saw b move");
        let copy = arena.deep_copy(g).expect("copy");
        arena
            .transform(copy, &Affine::shift([0.0, 5.0]))
            .expect("moves");
        assert!(close(arena.center(g), [1.5, 0.0]), "the original stayed");
        assert!(close(arena.critical(g, [1.0, 1.0]), [4.0, 1.0]));
    }
}
