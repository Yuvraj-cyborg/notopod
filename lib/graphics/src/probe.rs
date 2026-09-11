//! Asking the terminal what it can do.
//!
//! One round trip: we write a kitty graphics query, requests for the
//! foreground and background colours (OSC 10 and 11), a cell-size request
//! (`CSI 16 t`) and finally a primary device attributes request, which
//! every terminal answers, so the reply to it tells us the others are in.
//! The terminal must be in raw mode so the replies reach us unechoed and
//! unbuffered; the caller takes care of that.

use std::io::Write;
use std::time::{Duration, Instant};

use canvas::CellSize;
use theme::Rgb;

use crate::kitty;

/// What the terminal told us.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Probe {
    /// It speaks the kitty graphics protocol.
    pub kitty: bool,
    /// Pixel size of a cell, when reported.
    pub cell: Option<CellSize>,
    /// Default foreground colour, when reported.
    pub fg: Option<Rgb>,
    /// Default background colour, when reported.
    pub bg: Option<Rgb>,
}

/// Queries the terminal on stdin/stdout, waiting at most `timeout` for the
/// reply. Returns an empty probe when nothing came back (not a terminal,
/// or one that does not answer).
pub fn probe(timeout: Duration) -> Probe {
    const REQUEST: &str =
        "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b]10;?\x07\x1b]11;?\x07\x1b[16t\x1b[c";
    let mut out = std::io::stdout();
    if out
        .write_all(REQUEST.as_bytes())
        .and_then(|()| out.flush())
        .is_err()
    {
        return Probe::default();
    }
    let reply = read_reply(timeout);
    let mut probe = parse_reply(&reply);
    if probe.cell.is_none() {
        probe.cell = window_cell_size();
    }
    probe
}

/// Cell size from the window size report, if the terminal fills in pixels.
pub fn window_cell_size() -> Option<CellSize> {
    let ws = crossterm::terminal::window_size().ok()?;
    if ws.columns == 0 || ws.rows == 0 || ws.width == 0 || ws.height == 0 {
        return None;
    }
    let w = u32::from(ws.width) / u32::from(ws.columns);
    let h = u32::from(ws.height) / u32::from(ws.rows);
    (w >= 2 && h >= 4).then(|| CellSize::new(w, h))
}

/// Reads stdin until a primary device attributes reply (`CSI ... c`)
/// arrives or `timeout` passes.
#[cfg(unix)]
fn read_reply(timeout: Duration) -> Vec<u8> {
    use rustix::event::{PollFd, PollFlags, Timespec};

    let stdin = rustix::stdio::stdin();
    let deadline = Instant::now() + timeout;
    let mut reply = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let ts = Timespec {
            tv_sec: left.as_secs() as _,
            tv_nsec: i64::from(left.subsec_nanos()) as _,
        };
        let mut fds = [PollFd::from_borrowed_fd(stdin, PollFlags::IN)];
        match rustix::event::poll(&mut fds, Some(&ts)) {
            Ok(0) => break,
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => break,
        }
        match rustix::io::read(stdin, &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => reply.extend_from_slice(&buf[..n]),
        }
        if has_da1(&reply) {
            break;
        }
    }
    reply
}

#[cfg(not(unix))]
fn read_reply(_timeout: Duration) -> Vec<u8> {
    let _ = Instant::now();
    Vec::new()
}

/// Whether `reply` contains a primary device attributes response.
fn has_da1(reply: &[u8]) -> bool {
    let mut i = 0;
    while let Some(pos) = reply[i..].windows(3).position(|w| w == b"\x1b[?") {
        let start = i + pos + 3;
        if let Some(end) = reply[start..]
            .iter()
            .position(|b| !b.is_ascii_digit() && *b != b';')
        {
            if reply[start + end] == b'c' {
                return true;
            }
        }
        i = start;
    }
    false
}

/// Picks the answers out of everything the terminal sent.
pub fn parse_reply(reply: &[u8]) -> Probe {
    let text = String::from_utf8_lossy(reply);
    Probe {
        kitty: kitty::query_ok(reply),
        cell: parse_cell_size(&text),
        fg: parse_osc_color(&text, "10"),
        bg: parse_osc_color(&text, "11"),
    }
}

/// `CSI 6 ; height ; width t`.
fn parse_cell_size(text: &str) -> Option<CellSize> {
    let start = text.find("\x1b[6;")? + 4;
    let rest = &text[start..];
    let end = rest.find('t')?;
    let mut parts = rest[..end].split(';');
    let h: u32 = parts.next()?.trim().parse().ok()?;
    let w: u32 = parts.next()?.trim().parse().ok()?;
    (w >= 2 && h >= 4).then(|| CellSize::new(w, h))
}

/// `OSC n ; rgb:RRRR/GGGG/BBBB ST`, with 1 to 4 hex digits per channel.
fn parse_osc_color(text: &str, n: &str) -> Option<Rgb> {
    let marker = format!("\x1b]{n};rgb:");
    let start = text.find(&marker)? + marker.len();
    let rest = &text[start..];
    let end = rest.find(['\x07', '\x1b']).unwrap_or(rest.len());
    let mut channels = rest[..end].split('/');
    let mut channel = || -> Option<u8> {
        let hex = channels.next()?.trim();
        if hex.is_empty() || hex.len() > 4 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        // Scale any width down to 8 bits by taking the top two digits.
        let value = u32::from_str_radix(hex, 16).ok()?;
        let max = (1u32 << (4 * hex.len())) - 1;
        Some((value * 255 / max) as u8)
    };
    Some((channel()?, channel()?, channel()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_reply() {
        let reply = b"\x1b_Gi=31;OK\x1b\\\x1b]10;rgb:cdcd/d6d6/f4f4\x1b\\\x1b]11;rgb:1e1e/1e1e/2e2e\x07\x1b[6;38;18t\x1b[?62;22c";
        let p = parse_reply(reply);
        assert!(p.kitty);
        assert_eq!(p.fg, Some((0xcd, 0xd6, 0xf4)));
        assert_eq!(p.bg, Some((0x1e, 0x1e, 0x2e)));
        assert_eq!(p.cell, Some(CellSize::new(18, 38)));
        assert!(has_da1(reply));
    }

    #[test]
    fn missing_pieces_are_none() {
        let p = parse_reply(b"\x1b[?1;2c");
        assert_eq!(p, Probe::default());
        assert!(has_da1(b"\x1b[?1;2c"));
        assert!(!has_da1(b"\x1b[?1;2"));
        assert!(!has_da1(b"\x1b_Gi=31;OK\x1b\\"));
    }

    #[test]
    fn short_hex_colours_scale() {
        assert_eq!(
            parse_osc_color("\x1b]10;rgb:ff/80/00\x07", "10"),
            Some((255, 128, 0))
        );
        assert_eq!(
            parse_osc_color("\x1b]10;rgb:f/8/0\x07", "10"),
            Some((255, 136, 0))
        );
        assert_eq!(parse_osc_color("\x1b]10;rgb:zz/00/00\x07", "10"), None);
    }
}
