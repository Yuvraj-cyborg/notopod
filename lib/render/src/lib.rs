//! Renders a notopod [`Document`](syntax::Document) to styled terminal
//! text.
//!
//! Output is a list of [`ratatui`] [`Line`](ratatui::text::Line)s, already
//! wrapped to a given width. The TUI draws them directly; the `render` CLI
//! turns them into ANSI escape sequences with [`ansi::to_ansi`].
//!
//! ```
//! use render::{render_document, Theme};
//!
//! let doc = syntax::parse("# Hi\n\nSome **bold** text.\n");
//! let lines = render_document(&doc, 40, &Theme::default());
//! assert_eq!(lines[0].to_string(), "# Hi");
//! ```

pub mod ansi;
mod block;
mod inline;
pub mod theme;
pub mod wrap;

pub use block::{render_block, render_document, render_list_item};
pub use inline::render_inlines;
pub use theme::Theme;
