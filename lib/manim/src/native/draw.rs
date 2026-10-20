//! Painting a scene the way Manim's camera would, then cropping it the
//! way frames from real Manim are cropped, so both look alike in a note.

use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, StrokeDash,
    Transform,
};

use super::scene::{Arena, Item, Rgb, FRAME_W, P};
use super::Error;

/// Manim's stroke widths are hundredths of a unit.
const STROKE_UNIT: f64 = 0.01;

/// The frame Manim renders at `quality`, in pixels.
fn frame(quality: char) -> (u32, u32) {
    match quality {
        'l' => (854, 480),
        'h' => (1920, 1080),
        _ => (1280, 720),
    }
}

pub(crate) fn render(arena: &Arena, shown: &[usize], quality: char) -> Result<Vec<u8>, Error> {
    let (w, h) = frame(quality);
    let mut pixmap = Pixmap::new(w, h).ok_or_else(|| Error::unsupported("a frame that size"))?;
    let scale = f64::from(w) / FRAME_W;
    let to_px = |p: P| {
        (
            (f64::from(w) / 2.0 + p[0] * scale) as f32,
            (f64::from(h) / 2.0 - p[1] * scale) as f32,
        )
    };
    let mut drawn = Vec::new();
    for &id in shown {
        for m in arena.family(id) {
            if drawn.contains(&m) {
                continue;
            }
            drawn.push(m);
            for item in &arena.get(m).items {
                paint(&mut pixmap, item, scale, &to_px)?;
            }
        }
    }
    crop(&pixmap).ok_or(Error::Empty)
}

fn paint(
    pixmap: &mut Pixmap,
    item: &Item,
    scale: f64,
    to_px: &dyn Fn(P) -> (f32, f32),
) -> Result<(), Error> {
    match item {
        Item::Path {
            subpaths,
            closed,
            style,
            dash,
        } => {
            let mut pb = PathBuilder::new();
            for sub in subpaths {
                let Some(first) = sub.first() else { continue };
                let (x, y) = to_px(first[0]);
                pb.move_to(x, y);
                for c in sub {
                    let (x1, y1) = to_px(c[1]);
                    let (x2, y2) = to_px(c[2]);
                    let (x3, y3) = to_px(c[3]);
                    pb.cubic_to(x1, y1, x2, y2, x3, y3);
                }
                if *closed {
                    pb.close();
                }
            }
            let Some(path) = pb.finish() else {
                return Ok(());
            };
            // Fill first, then the outline over it, as Manim does.
            if style.fill_opacity > 0.0 {
                let paint = solid(style.fill, style.fill_opacity);
                pixmap.fill_path(
                    &path,
                    &paint,
                    FillRule::EvenOdd,
                    Transform::identity(),
                    None,
                );
            }
            if style.stroke_width > 0.0 && style.stroke_opacity > 0.0 {
                // Square ends and sharp corners, as Manim's Cairo draws them.
                let mut stroke = Stroke {
                    width: (style.stroke_width * STROKE_UNIT * scale) as f32,
                    line_cap: LineCap::Butt,
                    line_join: LineJoin::Miter,
                    ..Stroke::default()
                };
                if let Some(d) = dash {
                    let d = (d * scale) as f32;
                    stroke.dash = StrokeDash::new(vec![d, d], 0.0);
                }
                let paint = solid(style.stroke, style.stroke_opacity);
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
        Item::Tex {
            tex,
            display,
            center,
            em,
            angle,
            color,
            opacity,
            ..
        } => {
            let em_px = (em * scale) as f32;
            let Some(glyphs) = math::render(tex, *display, em_px, 2.0, *color) else {
                return Err(Error::unsupported("this text"));
            };
            let (cx, cy) = to_px(*center);
            let (gw, gh) = (glyphs.width() as f32, glyphs.height() as f32);
            // Turned about its middle; up on screen is down in pixels.
            let t = Transform::from_translate(-gw / 2.0, -gh / 2.0)
                .post_concat(Transform::from_rotate(-angle.to_degrees() as f32))
                .post_concat(Transform::from_translate(cx, cy));
            let paint = PixmapPaint {
                opacity: opacity.clamp(0.0, 1.0) as f32,
                quality: tiny_skia::FilterQuality::Bilinear,
                ..PixmapPaint::default()
            };
            pixmap.draw_pixmap(0, 0, glyphs.as_ref(), &paint, t, None);
        }
    }
    Ok(())
}

fn solid(c: Rgb, opacity: f64) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        c.0,
        c.1,
        c.2,
        (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
    );
    paint.anti_alias = true;
    paint
}

/// What was drawn plus the margin real frames get, as a PNG; `None` when
/// nothing was.
fn crop(pixmap: &Pixmap) -> Option<Vec<u8>> {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let pixels = pixmap.pixels();
    let (left, top, right, bottom) = crate::crop::bounds(w, h, |x, y| pixels[y * w + x].alpha())?;
    let (cw, ch) = (right - left + 1, bottom - top + 1);
    let mut data = Vec::with_capacity(cw * ch * 4);
    for y in top..=bottom {
        for p in &pixels[y * w + left..=y * w + right] {
            let c = p.demultiply();
            data.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
    }
    crate::crop::encode(cw, ch, &data)
}
