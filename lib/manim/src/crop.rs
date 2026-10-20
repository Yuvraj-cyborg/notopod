//! Cutting a frame down to what was drawn on it.
//!
//! Manim renders the whole 16:9 camera frame, and a scene of one small
//! shape leaves most of it empty. In a note the empty part would only push
//! the text apart, so frames are cropped to their drawn content plus a
//! margin.

use std::io::Cursor;

/// Pixels of alpha below this count as empty.
const EMPTY: u8 = 8;

/// `png` cropped to its drawn pixels plus a margin of 4% of the frame,
/// or `None` when it cannot be read, is not 8-bit RGBA, or is blank.
pub fn crop(png: &[u8]) -> Option<Vec<u8>> {
    let decoder = png::Decoder::new(Cursor::new(png));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let (w, h) = (info.width as usize, info.height as usize);
    let stride = info.line_size;
    let (left, top, right, bottom) = bounds(w, h, |x, y| buf[y * stride + x * 4 + 3])?;
    let (cw, ch) = (right - left + 1, bottom - top + 1);
    let mut pixels = Vec::with_capacity(cw * ch * 4);
    for y in top..=bottom {
        let row = y * stride;
        pixels.extend_from_slice(&buf[row + left * 4..row + (right + 1) * 4]);
    }
    demultiply(&mut pixels);
    encode(cw, ch, &pixels)
}

/// Manim writes its transparent frames with the colour already
/// multiplied by the alpha, so anything see-through comes out too dark
/// once a terminal blends it again. Undone when every pixel says so (no
/// channel brighter than its alpha); a frame that is not like that is
/// left alone.
fn demultiply(pixels: &mut [u8]) {
    let premultiplied = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
    if !premultiplied {
        return;
    }
    for p in pixels.as_chunks_mut::<4>().0 {
        let a = u16::from(p[3]);
        if a > 0 && a < 255 {
            for c in &mut p[..3] {
                *c = ((u16::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

/// The box of drawn pixels in a `w`×`h` frame plus the margin, as
/// `(left, top, right, bottom)`, inclusive; `None` when nothing is drawn.
pub(crate) fn bounds(
    w: usize,
    h: usize,
    alpha: impl Fn(usize, usize) -> u8,
) -> Option<(usize, usize, usize, usize)> {
    let (mut left, mut top, mut right, mut bottom) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if alpha(x, y) > EMPTY {
                left = left.min(x);
                right = right.max(x);
                top = top.min(y);
                bottom = bottom.max(y);
            }
        }
    }
    if left > right {
        return None;
    }
    let margin = w.max(h) * 4 / 100;
    Some((
        left.saturating_sub(margin),
        top.saturating_sub(margin),
        (right + margin).min(w - 1),
        (bottom + margin).min(h - 1),
    ))
}

/// Straight RGBA pixels as a PNG.
pub(crate) fn encode(w: usize, h: usize, pixels: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w as u32, h as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder
        .write_header()
        .and_then(|mut wr| wr.write_image_data(pixels))
        .ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, ink: &[(u32, u32)]) -> Vec<u8> {
        let mut data = vec![0u8; (w * h * 4) as usize];
        for &(x, y) in ink {
            data[((y * w + x) * 4 + 3) as usize] = 255;
        }
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut wr| wr.write_image_data(&data))
            .expect("png");
        out
    }

    fn size(png: &[u8]) -> (u32, u32) {
        let r = png::Decoder::new(Cursor::new(png))
            .read_info()
            .expect("png");
        let i = r.info();
        (i.width, i.height)
    }

    #[test]
    fn crops_to_the_drawing_with_a_margin() {
        let frame = png(100, 50, &[(40, 20), (60, 30)]);
        let cropped = crop(&frame).expect("cropped");
        // 21×11 of drawing and 4 pixels of margin on each side.
        assert_eq!(size(&cropped), (29, 19));
    }

    #[test]
    fn premultiplied_colour_is_undone_and_straight_colour_kept() {
        let mut manim = vec![35, 78, 88, 102, 255, 255, 255, 255];
        demultiply(&mut manim);
        assert_eq!(&manim[..4], &[88, 195, 220, 102], "BLUE at 0.4, as meant");
        let mut straight = vec![88, 195, 220, 102, 0, 0, 0, 0];
        demultiply(&mut straight);
        assert_eq!(&straight[..4], &[88, 195, 220, 102], "already straight");
    }

    #[test]
    fn the_margin_stops_at_the_edges_and_blank_frames_are_left_alone() {
        let corner = crop(&png(100, 50, &[(0, 0)])).expect("cropped");
        assert_eq!(size(&corner), (5, 5));
        assert!(crop(&png(10, 10, &[])).is_none(), "nothing drawn");
        assert!(crop(b"not a png").is_none());
    }
}
