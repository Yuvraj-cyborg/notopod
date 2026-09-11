//! Hand-drawn diagrams inside a note.
//!
//! A ```` ```draw ```` block holds a tiny language of boxes, lines and
//! text (see [`parse`]). This crate turns it into braille art with a
//! sketchy, Excalidraw-like look: shapes sit on terminal cells, but are
//! drawn at 2×4 dots per cell so lines can be diagonal, ellipses round
//! and strokes wobbly.
//!
//! ```
//! use canvas::{parse, render, Drawing};
//! use theme::Theme;
//!
//! let drawing: Drawing = parse("rect 0,0 10x3 \"hello\"\nline 4,4 -> 12,4\n");
//! let lines = render(&drawing, &Theme::default(), 80, None);
//! assert_eq!(lines.len(), 6);
//! assert!(lines[1].to_string().contains("hello"));
//! ```
//!
//! The editor uses [`Overlay`] to show a cursor, a highlighted shape and
//! a shape being placed, and [`to_source`] to write the drawing back.

mod model;
mod parse;
mod raster;
mod render;
mod rough;

pub use model::{Attrs, BoxKind, Drawing, Heads, Point, Rect, Shape, COLOR_NAMES};
pub use parse::{parse, quote, to_source};
pub use render::{render, render_source, Overlay};

/// The fence language tag that marks a drawing: ```` ```draw ````.
pub const LANG: &str = "draw";
