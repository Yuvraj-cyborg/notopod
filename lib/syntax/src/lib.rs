//! Document model and Markdown parser for notopod.
//!
//! This crate turns Markdown source into a [`Document`]: a list of blocks
//! (paragraphs, headings, lists, tables, ...) where every block remembers
//! the byte range it came from. Those spans are what let the editor show
//! one block as raw text while rendering everything around it.
//!
//! Parsing never fails. Anything the parser does not understand is kept
//! as plain text so a note always renders.
//!
//! ```
//! let doc = syntax::parse("# Hello\n\nSome *text*.\n");
//! assert_eq!(doc.blocks.len(), 2);
//! ```

pub mod ast;
pub mod lines;
pub mod parse;

pub use ast::{Alignment, Block, BlockKind, Document, Inline, ListItem, Span};
pub use lines::LineIndex;
pub use parse::parse;
