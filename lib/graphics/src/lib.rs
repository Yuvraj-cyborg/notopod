//! Pictures in the terminal.
//!
//! [`Graphics`] is the [`Drawings`] implementation that shows a
//! ```` ```draw ```` block as a real picture when the terminal can display
//! images over text, and as braille when it cannot. It renders with
//! [`canvas::render_image`], sends the PNG once with the kitty graphics
//! protocol, remembers it by content, and returns placeholder rows that
//! the terminal fills in.
//!
//! Escape sequences are not written by the renderer itself: they collect
//! in a buffer and the caller writes them with [`Graphics::take_pending`]
//! before the rows that refer to them reach the screen.
//!
//! ```no_run
//! use graphics::{Graphics, Mode};
//!
//! // Ask the terminal (stdin/stdout must be a terminal in raw mode).
//! let mut gfx = Graphics::detect(Mode::Auto);
//! if gfx.pictures() {
//!     println!("this terminal shows pictures");
//! }
//! ```

pub mod kitty;
pub mod probe;

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::IsTerminal;
use std::time::Duration;

use canvas::{CellSize, Drawing, Overlay};
use ratatui::text::Line;
use render::Drawings;
use theme::{Rgb, Theme};

pub use probe::Probe;

/// Which renderer to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Pictures when the terminal supports them, braille otherwise.
    #[default]
    Auto,
    /// Pictures via the kitty graphics protocol, without asking.
    Kitty,
    /// Braille dot art everywhere.
    Braille,
}

impl std::str::FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "kitty" | "pictures" | "image" | "images" => Ok(Self::Kitty),
            "braille" | "text" | "none" | "off" => Ok(Self::Braille),
            other => Err(format!(
                "unknown graphics mode {other:?} (expected auto, kitty or braille)"
            )),
        }
    }
}

/// How long to wait for the terminal to answer the capability query.
const PROBE_TIMEOUT: Duration = Duration::from_millis(400);

/// Pictures kept alive in the terminal at once.
const CACHE_LIMIT: usize = 32;

/// Foreground assumed on a dark terminal that did not say.
const DEFAULT_FG: Rgb = (0xd0, 0xd0, 0xd0);

struct Entry {
    id: u32,
    cols: usize,
    rows: usize,
    used: u64,
}

/// Draws ```` ```draw ```` blocks as pictures where possible.
pub struct Graphics {
    pictures: bool,
    cell: CellSize,
    fg: Rgb,
    cache: HashMap<u64, Entry>,
    pending: String,
    clock: u64,
    ids: u64,
}

impl Default for Graphics {
    fn default() -> Self {
        Self::braille()
    }
}

impl Graphics {
    /// Braille only; never talks to the terminal.
    pub fn braille() -> Self {
        Self::with(false, CellSize::default(), DEFAULT_FG)
    }

    /// Pictures with the given cell size and foreground, without probing.
    pub fn kitty(cell: CellSize, fg: Rgb) -> Self {
        Self::with(true, cell, fg)
    }

    fn with(pictures: bool, cell: CellSize, fg: Rgb) -> Self {
        let seed = u64::from(std::process::id())
            ^ std::time::UNIX_EPOCH
                .elapsed()
                .map_or(0, |d| d.as_nanos() as u64);
        Self {
            pictures,
            cell,
            fg,
            cache: HashMap::new(),
            pending: String::new(),
            clock: 0,
            ids: seed | 1,
        }
    }

    /// Decides how to draw, asking the terminal in [`Mode::Auto`].
    ///
    /// The terminal must be in raw mode for the answer to be readable;
    /// when stdin or stdout is not a terminal the answer is braille.
    pub fn detect(mode: Mode) -> Self {
        match mode {
            Mode::Braille => Self::braille(),
            Mode::Kitty => {
                let p = if is_tty() {
                    probe::probe(PROBE_TIMEOUT)
                } else {
                    Probe::default()
                };
                Self::from_probe(Probe { kitty: true, ..p })
            }
            Mode::Auto => {
                if !is_tty() || in_multiplexer() {
                    return Self::braille();
                }
                Self::from_probe(probe::probe(PROBE_TIMEOUT))
            }
        }
    }

    /// Builds from a probe result, filling in defaults for what the
    /// terminal did not report.
    pub fn from_probe(p: Probe) -> Self {
        if !p.kitty {
            return Self::braille();
        }
        let fg = p.fg.unwrap_or_else(|| match p.bg {
            // Dark text on a light background that gave us no foreground.
            Some(bg) if luma(bg) > 128 => (0x30, 0x30, 0x30),
            _ => DEFAULT_FG,
        });
        Self::kitty(p.cell.unwrap_or_default(), fg)
    }

    /// Whether drawings come out as pictures.
    pub fn pictures(&self) -> bool {
        self.pictures
    }

    /// Pixel size of a cell in use.
    pub fn cell(&self) -> CellSize {
        self.cell
    }

    /// Escape sequences that must reach the terminal before the rows
    /// returned since the last call. Empties the buffer.
    pub fn take_pending(&mut self) -> String {
        std::mem::take(&mut self.pending)
    }

    /// Forgets every picture and returns the sequences that delete them
    /// from the terminal. For shutdown.
    pub fn release_all(&mut self) -> String {
        let mut out = std::mem::take(&mut self.pending);
        for entry in self.cache.values() {
            out.push_str(&kitty::delete(entry.id));
        }
        self.cache.clear();
        out
    }

    fn next_id(&mut self) -> u32 {
        loop {
            // xorshift64*, then 24 bits: the id travels in a colour.
            self.ids ^= self.ids >> 12;
            self.ids ^= self.ids << 25;
            self.ids ^= self.ids >> 27;
            let id = ((self.ids.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) & 0x00ff_ffff) as u32;
            if id != 0 && !self.cache.values().any(|e| e.id == id) {
                return id;
            }
        }
    }

    fn evict(&mut self) {
        while self.cache.len() > CACHE_LIMIT {
            let Some((&key, _)) = self.cache.iter().min_by_key(|(_, e)| e.used) else {
                break;
            };
            if let Some(entry) = self.cache.remove(&key) {
                self.pending.push_str(&kitty::delete(entry.id));
            }
        }
    }
}

fn is_tty() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// tmux and screen do not pass the protocol through by default.
fn in_multiplexer() -> bool {
    std::env::var_os("TMUX").is_some()
        || std::env::var("TERM").is_ok_and(|t| t.starts_with("screen") || t.starts_with("tmux"))
}

fn luma(c: Rgb) -> u32 {
    (u32::from(c.0) * 299 + u32::from(c.1) * 587 + u32::from(c.2) * 114) / 1000
}

fn cache_key(
    drawing: &Drawing,
    theme: &Theme,
    max_width: usize,
    overlay: Option<&Overlay<'_>>,
) -> u64 {
    let mut h = DefaultHasher::new();
    drawing.hash(&mut h);
    theme.name.hash(&mut h);
    theme.roughness.to_bits().hash(&mut h);
    max_width.hash(&mut h);
    if let Some(o) = overlay {
        1u8.hash(&mut h);
        o.cursor.hash(&mut h);
        o.selected.hash(&mut h);
        o.preview.hash(&mut h);
        o.min_size.hash(&mut h);
    }
    h.finish()
}

impl Drawings for Graphics {
    fn draw(
        &mut self,
        drawing: &Drawing,
        theme: &Theme,
        max_width: usize,
        overlay: Option<&Overlay<'_>>,
    ) -> Vec<Line<'static>> {
        if !self.pictures {
            return canvas::render(drawing, theme, max_width, overlay);
        }
        let max_width = max_width.min(kitty::MAX_CELLS);
        self.clock += 1;
        let key = cache_key(drawing, theme, max_width, overlay);
        if let Some(entry) = self.cache.get_mut(&key) {
            entry.used = self.clock;
            return kitty::placeholder_lines(entry.id, entry.cols, entry.rows);
        }
        let Some(image) =
            canvas::render_image(drawing, theme, self.cell, self.fg, max_width, overlay)
        else {
            return Vec::new();
        };
        let (cols, rows) = (image.cols(), image.rows());
        if rows > kitty::MAX_CELLS {
            // Too tall to address: fall back rather than show a part.
            return canvas::render(drawing, theme, max_width, overlay);
        }
        let id = self.next_id();
        self.pending
            .push_str(&kitty::transmit(id, &image.to_png(), cols, rows));
        self.cache.insert(
            key,
            Entry {
                id,
                cols,
                rows,
                used: self.clock,
            },
        );
        self.evict();
        kitty::placeholder_lines(id, cols, rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawing(src: &str) -> Drawing {
        canvas::parse(src)
    }

    #[test]
    fn braille_mode_never_queues_output() {
        let mut g = Graphics::braille();
        let lines = g.draw(&drawing("rect 0,0 6x2\n"), &Theme::default(), 80, None);
        assert_eq!(lines.len(), 3);
        assert!(g.take_pending().is_empty());
        assert!(!g.pictures());
    }

    #[test]
    fn pictures_are_sent_once_and_placeholders_returned() {
        let mut g = Graphics::kitty(CellSize::new(8, 16), (255, 255, 255));
        let d = drawing("rect 0,0 6x2 \"hi\"\n");
        let lines = g.draw(&d, &Theme::default(), 80, None);
        assert_eq!(lines.len(), 3, "rows match the braille layout");
        assert_eq!(lines[0].spans.len(), 7);
        assert!(lines[0].spans[0].content.starts_with(kitty::PLACEHOLDER));
        let sent = g.take_pending();
        assert!(sent.starts_with("\x1b_Ga=T,f=100,q=2,U=1,i="));
        assert!(sent.contains(",c=7,r=3,"));

        // Same drawing again: nothing new is sent, same id.
        let again = g.draw(&d, &Theme::default(), 80, None);
        assert!(g.take_pending().is_empty());
        assert_eq!(again[0].spans[0].style.fg, lines[0].spans[0].style.fg);

        // A different drawing gets a different id.
        let other = g.draw(
            &drawing("rect 0,0 6x2 \"yo\"\n"),
            &Theme::default(),
            80,
            None,
        );
        assert_ne!(other[0].spans[0].style.fg, lines[0].spans[0].style.fg);
        assert!(!g.take_pending().is_empty());
    }

    #[test]
    fn overlay_changes_the_picture() {
        let mut g = Graphics::kitty(CellSize::new(8, 16), (255, 255, 255));
        let d = drawing("rect 0,0 6x2\n");
        let a = g.draw(&d, &Theme::default(), 80, None);
        let overlay = Overlay {
            cursor: Some(canvas::Point::new(1, 1)),
            selected: None,
            preview: None,
            min_size: (0, 0),
        };
        let b = g.draw(&d, &Theme::default(), 80, Some(&overlay));
        assert_ne!(a[0].spans[0].style.fg, b[0].spans[0].style.fg);
    }

    #[test]
    fn cache_is_bounded_and_evictions_delete() {
        let mut g = Graphics::kitty(CellSize::new(8, 16), (255, 255, 255));
        for i in 0..(CACHE_LIMIT + 5) {
            let d = drawing(&format!("rect 0,0 {}x2\n", i + 1));
            g.draw(&d, &Theme::default(), 200, None);
        }
        assert!(g.cache.len() <= CACHE_LIMIT);
        assert!(g.take_pending().contains("a=d,d=I,i="));
        let bye = g.release_all();
        assert_eq!(bye.matches("a=d,d=I").count(), CACHE_LIMIT);
        assert!(g.cache.is_empty());
    }

    #[test]
    fn blank_drawing_has_no_rows() {
        let mut g = Graphics::kitty(CellSize::new(8, 16), (255, 255, 255));
        assert!(g
            .draw_source("# nothing\n", &Theme::default(), 80)
            .is_empty());
        assert_eq!(g.draw_source("size 4x2\n", &Theme::default(), 80).len(), 2);
    }

    #[test]
    fn mode_parses() {
        assert_eq!("auto".parse::<Mode>(), Ok(Mode::Auto));
        assert_eq!("Kitty".parse::<Mode>(), Ok(Mode::Kitty));
        assert_eq!("braille".parse::<Mode>(), Ok(Mode::Braille));
        assert!("sixel".parse::<Mode>().is_err());
    }

    #[test]
    fn probe_without_kitty_is_braille() {
        let g = Graphics::from_probe(Probe::default());
        assert!(!g.pictures());
        let g = Graphics::from_probe(Probe {
            kitty: true,
            bg: Some((250, 250, 250)),
            ..Probe::default()
        });
        assert!(g.pictures());
        assert_eq!(g.fg, (0x30, 0x30, 0x30));
    }
}
