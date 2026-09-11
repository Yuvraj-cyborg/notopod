//! The notopod terminal user interface.
//!
//! One screen: the note, with a status bar underneath. Blocks render as
//! formatted text except the one the cursor is in, which shows its raw
//! Markdown so it can be edited. That is the whole trick.
//!
//! Call [`run`] with an optional file path to start the editor, or
//! [`pick_theme`] for the second screen this crate has: the list of themes
//! with a sample note beside it, drawn in whichever one is highlighted.

mod app;
mod canvas_mode;
mod files;
mod graph_view;
mod links;
mod picker;
mod ui;
mod view;
mod vim;

use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result};
use editor::Editor;
use graphics::{Graphics, Mode};
use ratatui::crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use theme::Theme;

pub use app::App;
pub use picker::{pick_theme, Entry as ThemeEntry};

/// Opens `paths` in the editor and runs until the user quits.
///
/// Notes get a tab each. A folder opens the file panel on it instead, so
/// `notopod ~/notes` starts by showing you what is there; with no paths
/// at all the editor starts on an empty note.
///
/// Takes over the terminal for the duration and restores it afterwards,
/// including on panic. `graphics` decides whether drawings are shown as
/// pictures; in [`Mode::Auto`] the terminal is asked. `vim` turns on vim
/// keys.
pub fn run(paths: &[&Path], theme: Theme, graphics: Mode, vim: bool) -> Result<()> {
    // A path that is not there yet is a note to create, not a folder.
    let (folders, notes): (Vec<&Path>, Vec<&Path>) =
        paths.iter().copied().partition(|p| p.is_dir());
    let editor = match notes.first() {
        Some(p) => Editor::open(p).with_context(|| format!("cannot open {}", p.display()))?,
        None => Editor::new(),
    };

    let mut terminal = ratatui::init();
    // Raw mode is on now, so the terminal's answer can be read.
    let graphics = Graphics::detect(graphics);
    let mut app = App::new(editor, theme)
        .with_graphics(graphics)
        .with_vim(vim);
    for path in notes.iter().skip(1) {
        app.open_path(path);
    }
    app.switch_tab(0);
    if let Some(folder) = folders.first() {
        app.open_folder(folder);
    }

    // Terminals that speak the kitty keyboard protocol can tell Ctrl+Tab
    // from Tab; the others just do not get that shortcut.
    let enhanced = ratatui::crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        let _ = execute!(
            io::stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
    }
    let _ = execute!(io::stdout(), EnableBracketedPaste);
    let result = app.run(&mut terminal);
    let _ = execute!(io::stdout(), DisableBracketedPaste);
    if enhanced {
        let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
    }
    let bye = app.release_graphics();
    if !bye.is_empty() {
        let mut out = io::stdout();
        let _ = out.write_all(bye.as_bytes());
        let _ = out.flush();
    }
    ratatui::restore();
    result
}
