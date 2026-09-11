//! Drawing a [`Drawing`] onto a [`Raster`] and out to styled lines.

use ratatui::style::Color;
use ratatui::text::Line;
use theme::Theme;
use unicode_width::UnicodeWidthStr;

use crate::model::{BoxKind, Drawing, Point, Rect, Shape};
use crate::raster::{Raster, DOTS_X, DOTS_Y};
use crate::rough::{self, Pen, Rng};

/// Extra state drawn on top of a drawing while it is being edited.
#[derive(Debug, Clone, Default)]
pub struct Overlay<'a> {
    /// The cell under the cursor.
    pub cursor: Option<Point>,
    /// Index of the shape to highlight.
    pub selected: Option<usize>,
    /// A shape that is being placed, drawn in the preview style.
    pub preview: Option<&'a Shape>,
    /// The canvas is at least this big (cells), so an empty drawing still
    /// has room to move around in.
    pub min_size: (i32, i32),
}

/// Renders a drawing to one styled line per cell row.
///
/// The canvas is as wide as the drawing needs, cropped to `max_width`
/// columns. Shapes are drawn in order; the overlay's selected shape is
/// drawn again on top in the theme's highlight colour.
pub fn render(
    drawing: &Drawing,
    theme: &Theme,
    max_width: usize,
    overlay: Option<&Overlay<'_>>,
) -> Vec<Line<'static>> {
    let (w, h) = canvas_size(drawing, overlay, max_width);
    let mut raster = Raster::new(w, h);
    let roughness = theme.roughness;

    for (i, shape) in drawing.shapes.iter().enumerate() {
        draw_shape(
            &mut raster,
            drawing,
            shape,
            i as u64,
            None,
            theme,
            roughness,
        );
    }
    if let Some(o) = overlay {
        if let Some(sel) = o.selected.and_then(|i| drawing.shapes.get(i)) {
            let color = theme.canvas_selected.fg.or(Some(Color::Yellow));
            draw_shape(
                &mut raster,
                drawing,
                sel,
                o.selected.unwrap_or(0) as u64,
                color,
                theme,
                roughness,
            );
        }
        if let Some(p) = o.preview {
            let color = theme.canvas_preview.fg.or(Some(Color::Cyan));
            draw_shape(
                &mut raster,
                drawing,
                p,
                drawing.shapes.len() as u64,
                color,
                theme,
                roughness,
            );
        }
    }

    let cursor = overlay
        .and_then(|o| o.cursor)
        .filter(|c| c.x >= 0 && c.y >= 0)
        .map(|c| (c.x, c.y));
    raster.to_lines(theme.canvas_stroke, cursor, theme.canvas_cursor)
}

/// Parses and renders the text of a ```` ```draw ```` block.
pub fn render_source(src: &str, theme: &Theme, max_width: usize) -> Vec<Line<'static>> {
    let drawing = crate::parse::parse(src);
    if drawing.is_blank() && drawing.size.is_none() {
        return Vec::new();
    }
    render(&drawing, theme, max_width, None)
}

/// Columns and rows a drawing takes on screen: its extent, grown to fit
/// the overlay's minimum size, cursor and preview, cropped to
/// `max_width` columns.
pub fn canvas_size(
    drawing: &Drawing,
    overlay: Option<&Overlay<'_>>,
    max_width: usize,
) -> (i32, i32) {
    let (mut w, mut h) = drawing.extent();
    if let Some(o) = overlay {
        w = w.max(o.min_size.0);
        h = h.max(o.min_size.1);
        if let Some(c) = o.cursor {
            w = w.max(c.x + 2);
            h = h.max(c.y + 2);
        }
        if let Some(b) = o.preview.and_then(Shape::bounds) {
            w = w.max(b.right() + 2);
            h = h.max(b.bottom() + 2);
        }
    }
    (w.min(max_width.max(1) as i32), h)
}

/// The colour of a shape: the override, its own `color=`, or the theme's
/// default stroke.
pub(crate) fn shape_color(
    shape: &Shape,
    override_color: Option<Color>,
    theme: &Theme,
) -> Option<Color> {
    override_color.or_else(|| {
        shape
            .attrs()
            .and_then(|a| a.color.as_deref())
            .and_then(|name| theme.color(name))
            .filter(|c| *c != Color::Reset)
            .or(theme.canvas_stroke.fg)
    })
}

fn draw_shape(
    raster: &mut Raster,
    drawing: &Drawing,
    shape: &Shape,
    seed: u64,
    override_color: Option<Color>,
    theme: &Theme,
    roughness: f32,
) {
    let color = shape_color(shape, override_color, theme);
    let text_style = match color {
        Some(c) => theme.canvas_text.fg(c),
        None => theme.canvas_text,
    };
    let mut rng = Rng::new(seed.wrapping_mul(31).wrapping_add(7));
    match shape {
        Shape::Box {
            kind,
            rect,
            label,
            attrs,
        } => {
            let geo = BoxGeo::new(*kind, *rect);
            if attrs.fill {
                let inside = |x: i32, y: i32| geo.contains_dot(x as f32, y as f32, 0.85);
                rough::hachure(
                    raster,
                    color,
                    geo.left as i32,
                    geo.top as i32,
                    geo.right as i32,
                    geo.bottom as i32,
                    &inside,
                    roughness,
                    &mut rng,
                );
            }
            let mut pen = Pen::new(raster, color).dashed(attrs.dashed);
            geo.outline(&mut pen, attrs.round, roughness, &mut rng);
            if let Some(label) = label {
                let (cx, cy) = centred_text(*rect, label);
                let label = fit(label, rect.w.max(1) as usize);
                raster.put_text(cx, cy, &label, text_style);
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
            let mut dots: Vec<(f32, f32)> = points.iter().map(|p| anchor(*p)).collect();
            bind_ends(&mut dots, points, drawing, shape);

            let mut pen = Pen::new(raster, color).dashed(attrs.dashed);
            for w in dots.windows(2) {
                rough::stroke(&mut pen, w[0], w[1], roughness, &mut rng);
            }
            let n = dots.len();
            if heads.end {
                rough::arrowhead(&mut pen, dots[n - 1], dots[n - 2], roughness, &mut rng);
            }
            if heads.start {
                rough::arrowhead(&mut pen, dots[0], dots[1], roughness, &mut rng);
            }
            if let Some(label) = label {
                let (mx, my) = midpoint(&dots);
                let lw = label.width() as i32;
                let cx = (mx / DOTS_X as f32).round() as i32 - lw / 2;
                let cy = (my / DOTS_Y as f32).floor() as i32;
                raster.put_text(cx, cy, label, text_style);
            }
        }
        Shape::Text { at, text, .. } => raster.put_text(at.x, at.y, text, text_style),
        Shape::Raw(_) => {}
    }
}

/// The dot at the centre of a cell, where lines start and end.
fn anchor(p: Point) -> (f32, f32) {
    ((p.x * DOTS_X) as f32 + 1.0, (p.y * DOTS_Y) as f32 + 2.0)
}

/// Moves a line's first and last dot to the border of the box shape the
/// corresponding cell lies in, if the neighbouring point is outside that
/// box. This is what makes "arrow from inside A to inside B" connect the
/// two borders.
fn bind_ends(dots: &mut [(f32, f32)], points: &[Point], drawing: &Drawing, this: &Shape) {
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
                Some(BoxGeo::new(*kind, *rect))
            }
            _ => None,
        }) else {
            continue;
        };
        let inside = dots[end];
        let outside = dots[next];
        if geo.contains_dot(outside.0, outside.1, 1.0) {
            continue;
        }
        // Bisect between the inside and outside dot for the border.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let mid = f32::midpoint(lo, hi);
            let p = (
                inside.0 + (outside.0 - inside.0) * mid,
                inside.1 + (outside.1 - inside.1) * mid,
            );
            if geo.contains_dot(p.0, p.1, 1.0) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        dots[end] = (
            inside.0 + (outside.0 - inside.0) * hi,
            inside.1 + (outside.1 - inside.1) * hi,
        );
    }
}

/// A box shape in dot coordinates.
#[derive(Debug, Clone, Copy)]
struct BoxGeo {
    kind: BoxKind,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl BoxGeo {
    fn new(kind: BoxKind, r: Rect) -> Self {
        Self {
            kind,
            left: (r.x * DOTS_X) as f32,
            top: (r.y * DOTS_Y) as f32,
            right: ((r.x + r.w) * DOTS_X - 1) as f32,
            bottom: ((r.y + r.h) * DOTS_Y - 1) as f32,
        }
    }

    fn centre(&self) -> (f32, f32) {
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

    /// Whether the dot is inside the outline, scaled by `scale` (below 1
    /// keeps a gap between a fill and the border).
    fn contains_dot(&self, x: f32, y: f32, scale: f32) -> bool {
        let (cx, cy) = self.centre();
        let (rx, ry) = self.radii();
        let (rx, ry) = ((rx * scale).max(0.5), (ry * scale).max(0.5));
        let (nx, ny) = ((x - cx) / rx, (y - cy) / ry);
        match self.kind {
            BoxKind::Rect => nx.abs() <= 1.0 && ny.abs() <= 1.0,
            BoxKind::Ellipse => nx * nx + ny * ny <= 1.0,
            BoxKind::Diamond => nx.abs() + ny.abs() <= 1.0,
        }
    }

    fn outline(&self, pen: &mut Pen<'_>, round: bool, roughness: f32, rng: &mut Rng) {
        let (left, top, right, bottom) = (self.left, self.top, self.right, self.bottom);
        match self.kind {
            BoxKind::Rect => {
                let radius = if round {
                    ((right - left) / 2.0)
                        .min((bottom - top) / 2.0)
                        .clamp(0.0, 3.0)
                } else {
                    0.0
                };
                if radius < 1.0 {
                    for (from, to) in [
                        ((left, top), (right, top)),
                        ((right, top), (right, bottom)),
                        ((right, bottom), (left, bottom)),
                        ((left, bottom), (left, top)),
                    ] {
                        pen.restart_dash();
                        rough::stroke(pen, from, to, roughness, rng);
                    }
                } else {
                    use std::f32::consts::{FRAC_PI_2, PI};
                    let rr = radius;
                    // Each edge is followed by the corner arc that joins it
                    // to the next edge, going clockwise from the top edge.
                    let edges = [
                        ((left + rr, top), (right - rr, top)),
                        ((right, top + rr), (right, bottom - rr)),
                        ((right - rr, bottom), (left + rr, bottom)),
                        ((left, bottom - rr), (left, top + rr)),
                    ];
                    let corners = [
                        (right - rr, top + rr, -FRAC_PI_2, 0.0),
                        (right - rr, bottom - rr, 0.0, FRAC_PI_2),
                        (left + rr, bottom - rr, FRAC_PI_2, PI),
                        (left + rr, top + rr, PI, PI + FRAC_PI_2),
                    ];
                    for (edge, corner) in edges.iter().zip(corners.iter()) {
                        pen.restart_dash();
                        rough::stroke(pen, edge.0, edge.1, roughness, rng);
                        let (cx, cy, from, to) = *corner;
                        rough::arc(pen, cx, cy, rr, rr, from, to, roughness * 0.5, rng);
                    }
                }
            }
            BoxKind::Ellipse => {
                let (cx, cy) = self.centre();
                let (rx, ry) = self.radii();
                rough::ellipse(pen, cx, cy, rx, ry, roughness, rng);
            }
            BoxKind::Diamond => {
                let (cx, cy) = self.centre();
                let pts = [(cx, top), (right, cy), (cx, bottom), (left, cy), (cx, top)];
                for w in pts.windows(2) {
                    pen.restart_dash();
                    rough::stroke(pen, w[0], w[1], roughness, rng);
                }
            }
        }
    }
}

/// Cell where a label should start so it is centred in `rect`.
fn centred_text(rect: Rect, label: &str) -> (i32, i32) {
    let lw = label.width().min(rect.w.max(1) as usize) as i32;
    let cx = rect.x + (rect.w - lw) / 2;
    let cy = rect.y + rect.h / 2;
    (cx, cy)
}

/// Point halfway along a polyline, by length.
pub(crate) fn midpoint(dots: &[(f32, f32)]) -> (f32, f32) {
    let total: f32 = dots
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum();
    let mut remaining = total / 2.0;
    for w in dots.windows(2) {
        let len = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
        if remaining <= len || len == 0.0 {
            let t = if len == 0.0 { 0.0 } else { remaining / len };
            return (
                w[0].0 + (w[1].0 - w[0].0) * t,
                w[0].1 + (w[1].1 - w[0].1) * t,
            );
        }
        remaining -= len;
    }
    dots.last().copied().unwrap_or((0.0, 0.0))
}

/// Truncates `text` to `width` columns, ending with `…` when cut.
pub(crate) fn fit(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    if width <= 1 {
        return "…".to_owned();
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in text.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > width - 1 {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    fn exact() -> Theme {
        Theme {
            roughness: 0.0,
            ..Theme::default()
        }
    }

    fn plain(src: &str) -> Vec<String> {
        render_source(src, &exact(), 200)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn empty_block_renders_nothing() {
        assert!(plain("").is_empty());
        assert!(plain("# just a comment\n").is_empty());
        assert_eq!(plain("size 4x2\n").len(), 2);
    }

    #[test]
    fn rect_has_a_closed_border_and_centred_label() {
        let lines = plain("rect 0,0 9x3 \"hi\"\n");
        assert_eq!(lines.len(), 4, "3 rows plus one of margin");
        let top = &lines[0];
        assert!(
            top.chars().take(9).all(|c| c != ' '),
            "top border unbroken: {top:?}"
        );
        let mid = &lines[1];
        let hi = mid.find("hi").expect("label present");
        assert_eq!(mid[..hi].chars().count(), 3, "label centred in {mid:?}");
        assert!(
            mid.starts_with('⡇')
                || mid.starts_with('⢸')
                || mid.starts_with('⣇')
                || !mid.starts_with(' ')
        );
    }

    #[test]
    fn rough_outline_on_the_top_left_corner_stays_closed() {
        let theme = Theme::default(); // rough
        for seed_shift in 0..4 {
            let src = format!("rect 0,0 {}x2\n", 12 + seed_shift);
            let lines = render_source(&src, &theme, 80);
            let top: Vec<char> = lines[0].to_string().chars().collect();
            let w = (12 + seed_shift) as usize;
            assert!(
                top[..w].iter().all(|c| *c != ' '),
                "gap in top edge of {src:?}: {:?}",
                lines[0].to_string()
            );
        }
    }

    #[test]
    fn arrow_binds_to_box_borders() {
        // Two boxes and an arrow from inside A to inside B: the stroke must
        // not cross the interiors, i.e. the cells between the boxes get dots
        // and the cells around each label do not.
        let lines = plain("rect 0,0 6x3 \"A\"\nrect 12,0 6x3 \"B\"\nline 2,1 -> 14,1\n");
        let row = &lines[1];
        let cells: Vec<char> = row.chars().collect();
        // Gap between the boxes (cells 6..12) carries the arrow.
        assert!(
            cells[6..12].iter().any(|c| *c != ' '),
            "arrow missing in gap: {row:?}"
        );
        // Interior next to the labels stays clean.
        assert_eq!(cells[1], ' ', "interior of A drawn over: {row:?}");
        assert_eq!(cells[13], ' ', "interior of B drawn over: {row:?}");
    }

    #[test]
    fn overlay_grows_canvas_and_marks_cursor() {
        let d = parse("rect 0,0 2x1\n");
        let theme = exact();
        let overlay = Overlay {
            cursor: Some(Point::new(10, 4)),
            selected: Some(0),
            preview: None,
            min_size: (0, 0),
        };
        let lines = render(&d, &theme, 200, Some(&overlay));
        assert_eq!(lines.len(), 6);
        assert!(lines[0].width() >= 12);
        // The cursor cell carries the cursor style.
        let styled = lines[4]
            .spans
            .iter()
            .any(|s| s.style.add_modifier == theme.canvas_cursor.add_modifier);
        assert!(styled);
    }

    #[test]
    fn width_is_cropped() {
        let lines = plain("rect 0,0 40x2\n");
        assert!(lines[0].width() <= 41);
        let narrow = render_source("rect 0,0 40x2\n", &exact(), 10);
        assert_eq!(narrow[0].width(), 10);
    }

    #[test]
    fn rough_rendering_is_stable_between_calls() {
        let theme = Theme::default();
        let a = render_source(
            "ellipse 1,1 12x5 \"x\" fill\nline 0,0 -> 20,7 dashed\n",
            &theme,
            80,
        );
        let b = render_source(
            "ellipse 1,1 12x5 \"x\" fill\nline 0,0 -> 20,7 dashed\n",
            &theme,
            80,
        );
        assert_eq!(
            a.iter().map(ToString::to_string).collect::<Vec<_>>(),
            b.iter().map(ToString::to_string).collect::<Vec<_>>()
        );
    }

    #[test]
    fn colours_come_from_the_palette() {
        let theme = Theme::builtin("nord").unwrap();
        let lines = render_source("line 0,0 -- 5,0 color=red\n", &theme, 80);
        assert!(lines[0]
            .spans
            .iter()
            .any(|s| s.style.fg == Some(theme.palette.red)));
    }
}
