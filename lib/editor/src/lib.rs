//! The text buffer behind notopod's editor.
//!
//! [`Editor`] owns a rope of text, a cursor, an undo history and (optionally)
//! the file the text came from. It knows nothing about the terminal; the
//! `tui` crate maps keys onto its methods and draws the result.
//!
//! ```
//! use editor::Editor;
//!
//! let mut editor = Editor::from_text("hello");
//! editor.line_end();
//! editor.insert_str(" world");
//! assert_eq!(editor.text(), "hello world");
//! editor.undo();
//! assert_eq!(editor.text(), "hello");
//! ```

mod editor;
mod history;
mod position;
pub mod search;
pub mod smart;

pub use editor::Editor;
pub use position::Position;
