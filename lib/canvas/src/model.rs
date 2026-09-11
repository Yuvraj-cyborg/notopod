//! The shapes in a drawing.
//!
//! All coordinates are terminal cells: `x` is a column, `y` a row. The
//! renderer works at four times the vertical and twice the horizontal
//! resolution, but shapes are placed where the cursor can be.

use unicode_width::UnicodeWidthStr;

/// A cell position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Point {
    /// Column.
    pub x: i32,
    /// Row.
    pub y: i32,
}

impl Point {
    /// A point.
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// A rectangle of cells, at least 1×1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    /// Left column.
    pub x: i32,
    /// Top row.
    pub y: i32,
    /// Width in cells, ≥ 1.
    pub w: i32,
    /// Height in cells, ≥ 1.
    pub h: i32,
}

impl Rect {
    /// A rectangle. Width and height are clamped to at least 1.
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            x,
            y,
            w: w.max(1),
            h: h.max(1),
        }
    }

    /// The smallest rectangle containing both cells.
    pub fn from_corners(a: Point, b: Point) -> Self {
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        Self::new(x, y, (a.x - b.x).abs() + 1, (a.y - b.y).abs() + 1)
    }

    /// Last column inside the rectangle.
    pub fn right(&self) -> i32 {
        self.x + self.w - 1
    }

    /// Last row inside the rectangle.
    pub fn bottom(&self) -> i32 {
        self.y + self.h - 1
    }

    /// Whether `p` is inside (borders included).
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    /// The smallest rectangle containing both.
    #[must_use]
    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let r = self.right().max(other.right());
        let b = self.bottom().max(other.bottom());
        Rect::new(x, y, r - x + 1, b - y + 1)
    }

    /// Moves the rectangle.
    #[must_use]
    pub fn translated(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

/// Which ends of a line carry an arrowhead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Heads {
    /// Arrowhead at the first point.
    pub start: bool,
    /// Arrowhead at the last point.
    pub end: bool,
}

/// The outline of a box shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BoxKind {
    /// A rectangle.
    #[default]
    Rect,
    /// An ellipse inscribed in the rectangle.
    Ellipse,
    /// A diamond (rhombus) inscribed in the rectangle.
    Diamond,
}

impl BoxKind {
    /// The keyword used in the block.
    pub fn keyword(self) -> &'static str {
        match self {
            BoxKind::Rect => "rect",
            BoxKind::Ellipse => "ellipse",
            BoxKind::Diamond => "diamond",
        }
    }
}

/// Optional styling shared by all shapes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Attrs {
    /// Stroke colour: a palette name (`red`), an ANSI name, or `#rrggbb`.
    /// `None` uses the theme's canvas stroke.
    pub color: Option<String>,
    /// Hatch the inside.
    pub fill: bool,
    /// Dashed stroke.
    pub dashed: bool,
    /// Rounded corners (rectangles only).
    pub round: bool,
}

/// Names a drawing may use for `color=`, in cycling order.
pub const COLOR_NAMES: [&str; 7] = [
    "red", "orange", "yellow", "green", "cyan", "blue", "magenta",
];

/// One element of a drawing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Shape {
    /// A rectangle, ellipse or diamond with an optional centred label.
    Box {
        /// Outline.
        kind: BoxKind,
        /// Position and size.
        rect: Rect,
        /// Text drawn in the middle.
        label: Option<String>,
        /// Styling.
        attrs: Attrs,
    },
    /// A polyline through two or more cells, with optional arrowheads and
    /// a label at its midpoint. Ends that lie inside a box are drawn up to
    /// that box's border.
    Line {
        /// The cells the line passes through, in order.
        points: Vec<Point>,
        /// Arrowheads.
        heads: Heads,
        /// Text drawn on the line's midpoint.
        label: Option<String>,
        /// Styling.
        attrs: Attrs,
    },
    /// Free text starting at a cell.
    Text {
        /// First cell of the text.
        at: Point,
        /// The text, one line.
        text: String,
        /// Styling (colour only).
        attrs: Attrs,
    },
    /// A source line that is not a shape: a comment, a blank line, or
    /// something the parser did not understand. Kept so the block
    /// round-trips unchanged; never drawn.
    Raw(String),
}

impl Shape {
    /// The cells this shape occupies, for layout and hit testing. `None`
    /// for raw lines.
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Shape::Box { rect, .. } => Some(*rect),
            Shape::Line { points, .. } => {
                let first = *points.first()?;
                Some(
                    points
                        .iter()
                        .fold(Rect::new(first.x, first.y, 1, 1), |r, p| {
                            r.union(&Rect::new(p.x, p.y, 1, 1))
                        }),
                )
            }
            Shape::Text { at, text, .. } => {
                Some(Rect::new(at.x, at.y, text.width().max(1) as i32, 1))
            }
            Shape::Raw(_) => None,
        }
    }

    /// Whether the cell `p` counts as "on" this shape: inside a box, on a
    /// line (within roughly half a cell), or on the text.
    pub fn hit(&self, p: Point) -> bool {
        match self {
            Shape::Box { rect, .. } => rect.contains(p),
            Shape::Text { .. } => self.bounds().is_some_and(|b| b.contains(p)),
            Shape::Line { points, .. } => points.windows(2).any(|w| {
                segment_distance(
                    (p.x as f32, p.y as f32),
                    (w[0].x as f32, w[0].y as f32),
                    (w[1].x as f32, w[1].y as f32),
                ) <= 0.6
            }),
            Shape::Raw(_) => false,
        }
    }

    /// Moves the shape by `(dx, dy)` cells.
    pub fn translate(&mut self, dx: i32, dy: i32) {
        match self {
            Shape::Box { rect, .. } => *rect = rect.translated(dx, dy),
            Shape::Line { points, .. } => {
                for p in points {
                    p.x += dx;
                    p.y += dy;
                }
            }
            Shape::Text { at, .. } => {
                at.x += dx;
                at.y += dy;
            }
            Shape::Raw(_) => {}
        }
    }

    /// The shape's styling, if it has any.
    pub fn attrs(&self) -> Option<&Attrs> {
        match self {
            Shape::Box { attrs, .. } | Shape::Line { attrs, .. } | Shape::Text { attrs, .. } => {
                Some(attrs)
            }
            Shape::Raw(_) => None,
        }
    }

    /// Mutable styling, if the shape has any.
    pub fn attrs_mut(&mut self) -> Option<&mut Attrs> {
        match self {
            Shape::Box { attrs, .. } | Shape::Line { attrs, .. } | Shape::Text { attrs, .. } => {
                Some(attrs)
            }
            Shape::Raw(_) => None,
        }
    }

    /// The label of a box or line, or the text of a text shape.
    pub fn label(&self) -> Option<&str> {
        match self {
            Shape::Box { label, .. } | Shape::Line { label, .. } => label.as_deref(),
            Shape::Text { text, .. } => Some(text),
            Shape::Raw(_) => None,
        }
    }

    /// Sets the label (or the text of a text shape). An empty string
    /// removes a label; an empty text shape keeps a single space so it
    /// still exists.
    pub fn set_label(&mut self, value: &str) {
        match self {
            Shape::Box { label, .. } | Shape::Line { label, .. } => {
                *label = (!value.is_empty()).then(|| value.to_owned());
            }
            Shape::Text { text, .. } => {
                *text = if value.is_empty() {
                    " ".to_owned()
                } else {
                    value.to_owned()
                };
            }
            Shape::Raw(_) => {}
        }
    }
}

/// Distance from `p` to the segment `a`-`b`.
pub(crate) fn segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

/// A whole ```` ```draw ```` block.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Drawing {
    /// Minimum size in cells, from a `size WxH` line. The drawing always
    /// grows to fit its shapes.
    pub size: Option<(i32, i32)>,
    /// Shapes in drawing order; later ones are drawn on top.
    pub shapes: Vec<Shape>,
}

impl Drawing {
    /// The cells covered by all shapes, or `None` for an empty drawing.
    pub fn bounds(&self) -> Option<Rect> {
        self.shapes
            .iter()
            .filter_map(Shape::bounds)
            .reduce(|a, b| a.union(&b))
    }

    /// Width and height in cells needed to show everything, with one
    /// cell of margin to the right and below the shapes.
    pub fn extent(&self) -> (i32, i32) {
        let (mut w, mut h) = self.size.unwrap_or((0, 0));
        if let Some(b) = self.bounds() {
            w = w.max(b.right() + 2);
            h = h.max(b.bottom() + 2);
        }
        (w.max(1), h.max(1))
    }

    /// Index of the topmost shape at cell `p`.
    pub fn hit(&self, p: Point) -> Option<usize> {
        self.shapes.iter().rposition(|s| s.hit(p))
    }

    /// `true` when nothing would be drawn.
    pub fn is_blank(&self) -> bool {
        self.shapes.iter().all(|s| matches!(s, Shape::Raw(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_geometry() {
        let r = Rect::from_corners(Point::new(5, 2), Point::new(1, 4));
        assert_eq!(r, Rect::new(1, 2, 5, 3));
        assert!(r.contains(Point::new(5, 4)));
        assert!(!r.contains(Point::new(6, 4)));
        assert_eq!(r.union(&Rect::new(0, 0, 1, 1)), Rect::new(0, 0, 6, 5));
        assert_eq!(Rect::new(0, 0, 0, -3), Rect::new(0, 0, 1, 1));
    }

    #[test]
    fn line_hit_and_bounds() {
        let line = Shape::Line {
            points: vec![Point::new(0, 0), Point::new(6, 0), Point::new(6, 3)],
            heads: Heads::default(),
            label: None,
            attrs: Attrs::default(),
        };
        assert_eq!(line.bounds(), Some(Rect::new(0, 0, 7, 4)));
        assert!(line.hit(Point::new(3, 0)));
        assert!(line.hit(Point::new(6, 2)));
        assert!(!line.hit(Point::new(3, 2)));
    }

    #[test]
    fn drawing_extent_grows_with_shapes() {
        let mut d = Drawing::default();
        assert_eq!(d.extent(), (1, 1));
        d.size = Some((20, 5));
        assert_eq!(d.extent(), (20, 5));
        d.shapes.push(Shape::Text {
            at: Point::new(30, 8),
            text: "hello".into(),
            attrs: Attrs::default(),
        });
        assert_eq!(d.extent(), (36, 10));
        assert_eq!(d.hit(Point::new(32, 8)), Some(0));
        assert_eq!(d.hit(Point::new(0, 0)), None);
    }
}
