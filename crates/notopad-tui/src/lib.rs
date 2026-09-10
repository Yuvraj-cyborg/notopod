//! The notopad terminal user interface.
//!
//! One screen: the note, with a status bar underneath. Blocks render as
//! formatted text except the one the cursor is in, which shows its raw
//! Markdown so it can be edited. That is the whole trick.
//!
//! Call [`run`] with an optional file path to start the editor.

mod app;
mod ui;
mod view;

use std::io;
use std::path::Path;

use anyhow::{Context, Result};
use notopad_editor::Editor;
use ratatui::crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use ratatui::crossterm::execute;

pub use app::App;

/// Opens `path` (or an empty buffer) in the editor and runs until the user quits.
///
/// Takes over the terminal for the duration and restores it afterwards,
/// including on panic.
pub fn run(path: Option<&Path>) -> Result<()> {
    let editor = match path {
        Some(p) => Editor::open(p).with_context(|| format!("cannot open {}", p.display()))?,
        None => Editor::new(),
    };
    let mut app = App::new(editor);

    let mut terminal = ratatui::init();
    let _ = execute!(io::stdout(), EnableBracketedPaste);
    let result = app.run(&mut terminal);
    let _ = execute!(io::stdout(), DisableBracketedPaste);
    ratatui::restore();
    result
}
