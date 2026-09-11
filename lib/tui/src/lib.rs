//! The notopod terminal user interface.
//!
//! One screen: the note, with a status bar underneath. Blocks render as
//! formatted text except the one the cursor is in, which shows its raw
//! Markdown so it can be edited. That is the whole trick.
//!
//! Call [`run`] with an optional file path to start the editor.

mod app;
mod canvas_mode;
mod ui;
mod view;

use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result};
use editor::Editor;
use graphics::{Graphics, Mode};
use ratatui::crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use ratatui::crossterm::execute;
use theme::Theme;

pub use app::App;

/// Opens `path` (or an empty buffer) in the editor and runs until the user quits.
///
/// Takes over the terminal for the duration and restores it afterwards,
/// including on panic. `graphics` decides whether drawings are shown as
/// pictures; in [`Mode::Auto`] the terminal is asked.
pub fn run(path: Option<&Path>, theme: Theme, graphics: Mode) -> Result<()> {
    let editor = match path {
        Some(p) => Editor::open(p).with_context(|| format!("cannot open {}", p.display()))?,
        None => Editor::new(),
    };

    let mut terminal = ratatui::init();
    // Raw mode is on now, so the terminal's answer can be read.
    let graphics = Graphics::detect(graphics);
    let mut app = App::new(editor, theme).with_graphics(graphics);

    let _ = execute!(io::stdout(), EnableBracketedPaste);
    let result = app.run(&mut terminal);
    let _ = execute!(io::stdout(), DisableBracketedPaste);
    let bye = app.release_graphics();
    if !bye.is_empty() {
        let mut out = io::stdout();
        let _ = out.write_all(bye.as_bytes());
        let _ = out.flush();
    }
    ratatui::restore();
    result
}
