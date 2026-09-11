//! Renders a notopod [`Document`](syntax::Document) to styled terminal
//! text.
//!
//! Output is a list of [`ratatui`] [`Line`](ratatui::text::Line)s, already
//! wrapped to a given width. The TUI draws them directly; the `render` CLI
//! turns them into ANSI escape sequences with [`ansi::to_ansi`].
//!
//! ```
//! use render::render_document;
//! use theme::Theme;
//!
//! let doc = syntax::parse("# Hi\n\nSome **bold** text.\n");
//! let lines = render_document(&doc, 40, &Theme::default());
//! assert_eq!(lines[0].to_string(), "# Hi");
//! ```
//!
//! ```` ```draw ```` blocks are handed to a [`Drawings`] implementation:
//! the `*_with` functions take one, the plain ones use [`Braille`].

pub mod ansi;
mod block;
mod drawings;
mod inline;
pub mod wrap;

pub use block::{
    render_block, render_block_with, render_document, render_document_with, render_list_item,
    render_list_item_with,
};
pub use drawings::{Braille, Drawings};
pub use inline::render_inlines;
