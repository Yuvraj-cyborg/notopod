//! The kitty graphics protocol, as far as notopod needs it.
//!
//! Pictures are sent once as PNG (`a=T`) together with a *virtual
//! placement* (`U=1`) that says how many cells they cover. They are then
//! shown by printing ordinary text: the placeholder character `U+10EEEE`
//! with the image id in its foreground colour and its row and column in
//! two combining diacritics. Being text, a picture scrolls, is clipped and
//! is redrawn with the rest of the screen, so a cell-based UI needs to
//! know nothing about images. Supported by kitty (0.28+), Ghostty,
//! Konsole and WezTerm's kitty mode.

use std::fmt::Write as _;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// The character a picture cell is drawn with.
pub const PLACEHOLDER: char = '\u{10EEEE}';

/// Combining characters that encode row and column numbers, in order:
/// `DIACRITICS[n]` means `n`. From kitty's `rowcolumn-diacritics.txt`.
pub const DIACRITICS: [char; 297] = [
    '\u{0305}',
    '\u{030d}',
    '\u{030e}',
    '\u{0310}',
    '\u{0312}',
    '\u{033d}',
    '\u{033e}',
    '\u{033f}',
    '\u{0346}',
    '\u{034a}',
    '\u{034b}',
    '\u{034c}',
    '\u{0350}',
    '\u{0351}',
    '\u{0352}',
    '\u{0357}',
    '\u{035b}',
    '\u{0363}',
    '\u{0364}',
    '\u{0365}',
    '\u{0366}',
    '\u{0367}',
    '\u{0368}',
    '\u{0369}',
    '\u{036a}',
    '\u{036b}',
    '\u{036c}',
    '\u{036d}',
    '\u{036e}',
    '\u{036f}',
    '\u{0483}',
    '\u{0484}',
    '\u{0485}',
    '\u{0486}',
    '\u{0487}',
    '\u{0592}',
    '\u{0593}',
    '\u{0594}',
    '\u{0595}',
    '\u{0597}',
    '\u{0598}',
    '\u{0599}',
    '\u{059c}',
    '\u{059d}',
    '\u{059e}',
    '\u{059f}',
    '\u{05a0}',
    '\u{05a1}',
    '\u{05a8}',
    '\u{05a9}',
    '\u{05ab}',
    '\u{05ac}',
    '\u{05af}',
    '\u{05c4}',
    '\u{0610}',
    '\u{0611}',
    '\u{0612}',
    '\u{0613}',
    '\u{0614}',
    '\u{0615}',
    '\u{0616}',
    '\u{0617}',
    '\u{0657}',
    '\u{0658}',
    '\u{0659}',
    '\u{065a}',
    '\u{065b}',
    '\u{065d}',
    '\u{065e}',
    '\u{06d6}',
    '\u{06d7}',
    '\u{06d8}',
    '\u{06d9}',
    '\u{06da}',
    '\u{06db}',
    '\u{06dc}',
    '\u{06df}',
    '\u{06e0}',
    '\u{06e1}',
    '\u{06e2}',
    '\u{06e4}',
    '\u{06e7}',
    '\u{06e8}',
    '\u{06eb}',
    '\u{06ec}',
    '\u{0730}',
    '\u{0732}',
    '\u{0733}',
    '\u{0735}',
    '\u{0736}',
    '\u{073a}',
    '\u{073d}',
    '\u{073f}',
    '\u{0740}',
    '\u{0741}',
    '\u{0743}',
    '\u{0745}',
    '\u{0747}',
    '\u{0749}',
    '\u{074a}',
    '\u{07eb}',
    '\u{07ec}',
    '\u{07ed}',
    '\u{07ee}',
    '\u{07ef}',
    '\u{07f0}',
    '\u{07f1}',
    '\u{07f3}',
    '\u{0816}',
    '\u{0817}',
    '\u{0818}',
    '\u{0819}',
    '\u{081b}',
    '\u{081c}',
    '\u{081d}',
    '\u{081e}',
    '\u{081f}',
    '\u{0820}',
    '\u{0821}',
    '\u{0822}',
    '\u{0823}',
    '\u{0825}',
    '\u{0826}',
    '\u{0827}',
    '\u{0829}',
    '\u{082a}',
    '\u{082b}',
    '\u{082c}',
    '\u{082d}',
    '\u{0951}',
    '\u{0953}',
    '\u{0954}',
    '\u{0f82}',
    '\u{0f83}',
    '\u{0f86}',
    '\u{0f87}',
    '\u{135d}',
    '\u{135e}',
    '\u{135f}',
    '\u{17dd}',
    '\u{193a}',
    '\u{1a17}',
    '\u{1a75}',
    '\u{1a76}',
    '\u{1a77}',
    '\u{1a78}',
    '\u{1a79}',
    '\u{1a7a}',
    '\u{1a7b}',
    '\u{1a7c}',
    '\u{1b6b}',
    '\u{1b6d}',
    '\u{1b6e}',
    '\u{1b6f}',
    '\u{1b70}',
    '\u{1b71}',
    '\u{1b72}',
    '\u{1b73}',
    '\u{1cd0}',
    '\u{1cd1}',
    '\u{1cd2}',
    '\u{1cda}',
    '\u{1cdb}',
    '\u{1ce0}',
    '\u{1dc0}',
    '\u{1dc1}',
    '\u{1dc3}',
    '\u{1dc4}',
    '\u{1dc5}',
    '\u{1dc6}',
    '\u{1dc7}',
    '\u{1dc8}',
    '\u{1dc9}',
    '\u{1dcb}',
    '\u{1dcc}',
    '\u{1dd1}',
    '\u{1dd2}',
    '\u{1dd3}',
    '\u{1dd4}',
    '\u{1dd5}',
    '\u{1dd6}',
    '\u{1dd7}',
    '\u{1dd8}',
    '\u{1dd9}',
    '\u{1dda}',
    '\u{1ddb}',
    '\u{1ddc}',
    '\u{1ddd}',
    '\u{1dde}',
    '\u{1ddf}',
    '\u{1de0}',
    '\u{1de1}',
    '\u{1de2}',
    '\u{1de3}',
    '\u{1de4}',
    '\u{1de5}',
    '\u{1de6}',
    '\u{1dfe}',
    '\u{20d0}',
    '\u{20d1}',
    '\u{20d4}',
    '\u{20d5}',
    '\u{20d6}',
    '\u{20d7}',
    '\u{20db}',
    '\u{20dc}',
    '\u{20e1}',
    '\u{20e7}',
    '\u{20e9}',
    '\u{20f0}',
    '\u{2cef}',
    '\u{2cf0}',
    '\u{2cf1}',
    '\u{2de0}',
    '\u{2de1}',
    '\u{2de2}',
    '\u{2de3}',
    '\u{2de4}',
    '\u{2de5}',
    '\u{2de6}',
    '\u{2de7}',
    '\u{2de8}',
    '\u{2de9}',
    '\u{2dea}',
    '\u{2deb}',
    '\u{2dec}',
    '\u{2ded}',
    '\u{2dee}',
    '\u{2def}',
    '\u{2df0}',
    '\u{2df1}',
    '\u{2df2}',
    '\u{2df3}',
    '\u{2df4}',
    '\u{2df5}',
    '\u{2df6}',
    '\u{2df7}',
    '\u{2df8}',
    '\u{2df9}',
    '\u{2dfa}',
    '\u{2dfb}',
    '\u{2dfc}',
    '\u{2dfd}',
    '\u{2dfe}',
    '\u{2dff}',
    '\u{a66f}',
    '\u{a67c}',
    '\u{a67d}',
    '\u{a6f0}',
    '\u{a6f1}',
    '\u{a8e0}',
    '\u{a8e1}',
    '\u{a8e2}',
    '\u{a8e3}',
    '\u{a8e4}',
    '\u{a8e5}',
    '\u{a8e6}',
    '\u{a8e7}',
    '\u{a8e8}',
    '\u{a8e9}',
    '\u{a8ea}',
    '\u{a8eb}',
    '\u{a8ec}',
    '\u{a8ed}',
    '\u{a8ee}',
    '\u{a8ef}',
    '\u{a8f0}',
    '\u{a8f1}',
    '\u{aab0}',
    '\u{aab2}',
    '\u{aab3}',
    '\u{aab7}',
    '\u{aab8}',
    '\u{aabe}',
    '\u{aabf}',
    '\u{aac1}',
    '\u{fe20}',
    '\u{fe21}',
    '\u{fe22}',
    '\u{fe23}',
    '\u{fe24}',
    '\u{fe25}',
    '\u{fe26}',
    '\u{10a0f}',
    '\u{10a38}',
    '\u{1d185}',
    '\u{1d186}',
    '\u{1d187}',
    '\u{1d188}',
    '\u{1d189}',
    '\u{1d1aa}',
    '\u{1d1ab}',
    '\u{1d1ac}',
    '\u{1d1ad}',
    '\u{1d242}',
    '\u{1d243}',
    '\u{1d244}',
];

/// Largest picture, in cells, that can be addressed with one diacritic
/// per axis.
pub const MAX_CELLS: usize = DIACRITICS.len();

const ESC: &str = "\x1b";
const CHUNK: usize = 4096;

/// The escape sequence that sends `png` as image `id` and creates a
/// virtual placement `cols` by `rows` cells for it. Quiet: the terminal
/// sends no reply.
pub fn transmit(id: u32, png: &[u8], cols: usize, rows: usize) -> String {
    let data = base64(png);
    let mut out = String::with_capacity(data.len() + 128);
    let mut chunks = data.as_bytes().chunks(CHUNK).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        out.push_str(ESC);
        out.push_str("_G");
        if first {
            let _ = write!(out, "a=T,f=100,q=2,U=1,i={id},c={cols},r={rows},m={more};");
            first = false;
        } else {
            let _ = write!(out, "m={more};");
        }
        // Base64 is ASCII.
        out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        out.push_str(ESC);
        out.push('\\');
    }
    out
}

/// The escape sequence that deletes image `id` and frees its memory.
pub fn delete(id: u32) -> String {
    format!("{ESC}_Ga=d,d=I,i={id},q=2{ESC}\\")
}

/// A query the terminal answers with `_Gi=31;OK` if it speaks the
/// protocol. Followed by a primary device attributes request, which every
/// terminal answers, so a reader knows when to stop waiting.
pub fn query() -> String {
    format!("{ESC}_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA{ESC}\\{ESC}[c")
}

/// Whether a reply to [`query`] says the protocol is supported.
pub fn query_ok(reply: &[u8]) -> bool {
    reply.windows(9).any(|w| w == b"_Gi=31;OK")
}

/// One styled cell of a placeholder for image `id`.
pub fn cell(id: u32, row: usize, col: usize) -> Span<'static> {
    let mut symbol = String::with_capacity(12);
    symbol.push(PLACEHOLDER);
    symbol.push(DIACRITICS[row.min(MAX_CELLS - 1)]);
    symbol.push(DIACRITICS[col.min(MAX_CELLS - 1)]);
    Span::styled(symbol, Style::new().fg(id_color(id)))
}

/// The placeholder text for image `id`, one line per row.
pub fn placeholder_lines(id: u32, cols: usize, rows: usize) -> Vec<Line<'static>> {
    let cols = cols.min(MAX_CELLS);
    (0..rows.min(MAX_CELLS))
        .map(|row| Line::from((0..cols).map(|col| cell(id, row, col)).collect::<Vec<_>>()))
        .collect()
}

/// The foreground colour that carries the low 24 bits of an image id.
pub fn id_color(id: u32) -> Color {
    Color::Rgb(
        ((id >> 16) & 0xff) as u8,
        ((id >> 8) & 0xff) as u8,
        (id & 0xff) as u8,
    )
}

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn transmit_is_chunked_with_continuation_flags() {
        let small = transmit(7, b"abc", 4, 2);
        assert_eq!(small, "\x1b_Ga=T,f=100,q=2,U=1,i=7,c=4,r=2,m=0;YWJj\x1b\\");
        let big = transmit(7, &[0u8; 6000], 4, 2);
        let parts: Vec<&str> = big.split("\x1b\\").filter(|s| !s.is_empty()).collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].starts_with("\x1b_Ga=T,") && parts[0].contains(",m=1;"));
        assert!(parts[1].starts_with("\x1b_Gm=0;"));
    }

    #[test]
    fn placeholder_cells_encode_id_row_and_column() {
        let lines = placeholder_lines(0x0001_0203, 3, 2);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].spans.len(), 3);
        let c = &lines[1].spans[2];
        let chars: Vec<char> = c.content.chars().collect();
        assert_eq!(chars, vec![PLACEHOLDER, DIACRITICS[1], DIACRITICS[2]]);
        assert_eq!(c.style.fg, Some(Color::Rgb(1, 2, 3)));
    }

    #[test]
    fn diacritics_are_distinct() {
        let mut sorted = DIACRITICS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), DIACRITICS.len());
        assert_eq!(DIACRITICS[0], '\u{0305}');
        assert_eq!(DIACRITICS[1], '\u{030d}');
    }

    #[test]
    fn query_reply_is_recognised() {
        assert!(query_ok(b"\x1b_Gi=31;OK\x1b\\\x1b[?62;c"));
        assert!(!query_ok(b"\x1b[?62;c"));
        assert!(query().ends_with("\x1b[c"));
        assert!(delete(5).contains("d=I,i=5"));
    }
}
