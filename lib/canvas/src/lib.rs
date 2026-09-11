//! Hand-drawn diagrams inside a note.
//!
//! A ```` ```draw ```` block holds a tiny language of boxes, lines and
//! text (see [`parse`]). Shapes sit on terminal cells, so the editor can
//! move them with the cursor, and are drawn with a sketchy,
//! Excalidraw-like look in one of two ways:
//!
//! - [`render_image`] paints a real picture, anti-aliased strokes in a
//!   hand-drawn font, for terminals that can show images over text.
//! - [`render`] draws braille art at 2×4 dots per cell, for every other
//!   terminal.
//!
//! ```
//! use canvas::{parse, render, render_image, CellSize, Drawing};
//! use theme::Theme;
//!
//! let drawing: Drawing = parse("rect 0,0 10x3 \"hello\"\nline 4,4 -> 12,4\n");
//! let lines = render(&drawing, &Theme::default(), 80, None);
//! assert_eq!(lines.len(), 6);
//! assert!(lines[1].to_string().contains("hello"));
//!
//! let picture = render_image(&drawing, &Theme::default(), CellSize::new(10, 20), (255, 255, 255), 80, None).unwrap();
//! assert_eq!((picture.cols(), picture.rows()), (14, 6));
//! ```
//!
//! The editor uses [`Overlay`] to show a cursor, a highlighted shape and
//! a shape being placed, and [`to_source`] to write the drawing back.

mod model;
mod parse;
mod pixel;
mod raster;
mod render;
mod rough;
mod sketch;
mod text;

pub use model::{Attrs, BoxKind, Drawing, Heads, Point, Rect, Shape, COLOR_NAMES};
pub use parse::{parse, quote, to_source};
pub use pixel::{render_image, CellSize, Image};
pub use render::{canvas_size, render, render_source, Overlay};

/// The fence language tag that marks a drawing: ```` ```draw ````.
pub const LANG: &str = "draw";
