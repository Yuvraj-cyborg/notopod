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
    let (mut left, mut top, mut right, mut bottom) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if buf[y * stride + x * 4 + 3] > EMPTY {
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
    let (left, top) = (left.saturating_sub(margin), top.saturating_sub(margin));
    let (right, bottom) = ((right + margin).min(w - 1), (bottom + margin).min(h - 1));
    let (cw, ch) = (right - left + 1, bottom - top + 1);
    let mut pixels = Vec::with_capacity(cw * ch * 4);
    for y in top..=bottom {
        let row = y * stride;
        pixels.extend_from_slice(&buf[row + left * 4..row + (right + 1) * 4]);
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, cw as u32, ch as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder
        .write_header()
        .and_then(|mut wr| wr.write_image_data(&pixels))
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
    fn the_margin_stops_at_the_edges_and_blank_frames_are_left_alone() {
        let corner = crop(&png(100, 50, &[(0, 0)])).expect("cropped");
        assert_eq!(size(&corner), (5, 5));
        assert!(crop(&png(10, 10, &[])).is_none(), "nothing drawn");
        assert!(crop(b"not a png").is_none());
    }
}
