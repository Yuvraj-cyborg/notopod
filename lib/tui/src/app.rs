//! Application state and key handling.

use std::time::{Duration, Instant};

use anyhow::Result;
use editor::{Editor, Position};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use syntax::{Document, LineIndex};
use theme::Theme;

use crate::view::{self, Source, View};

/// How long a status-bar message stays visible.
const NOTICE_TTL: Duration = Duration::from_secs(4);

/// What the keyboard is currently driving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Editing the note.
    Edit,
    /// Typing a search query in the status bar.
    Find {
        /// The query so far.
        query: String,
        /// Where the cursor was when the search started.
        origin: Position,
    },
    /// Typing a file name in the status bar.
    SaveAs {
        /// The name so far.
        input: String,
    },
    /// Asked to quit with unsaved changes.
    ConfirmQuit,
}

/// The editor screen.
pub struct App {
    pub(crate) editor: Editor,
    pub(crate) theme: Theme,
    pub(crate) preview: bool,
    pub(crate) mode: Mode,
    pub(crate) scroll: usize,
    pub(crate) notice: Option<(String, Instant)>,
    pub(crate) body_height: usize,
    pub(crate) body_width: usize,
    parsed: Parsed,
    view: Option<CachedView>,
    /// Screen column to aim for when moving up/down.
    sticky_x: Option<usize>,
    last_query: String,
    quit: bool,
}

struct Parsed {
    version: u64,
    doc: Document,
    index: LineIndex,
}

struct CachedView {
    key: (u64, Position, usize, bool),
    view: View,
}

impl App {
    /// Creates the screen around `editor`, drawn with `theme`.
    pub fn new(editor: Editor, theme: Theme) -> Self {
        Self {
            editor,
            theme,
            preview: true,
            mode: Mode::Edit,
            scroll: 0,
            notice: None,
            body_height: 0,
            body_width: 80,
            parsed: Parsed {
                version: u64::MAX,
                doc: Document::default(),
                index: LineIndex::new(""),
            },
            view: None,
            sticky_x: None,
            last_query: String::new(),
            quit: false,
        }
    }

    /// Runs the event loop until the user quits.
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;
            if self.quit {
                return Ok(());
            }
            if event::poll(Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(key) => self.handle_key(key),
                    Event::Paste(text) => self.handle_paste(&text),
                    _ => {}
                }
            }
            if self
                .notice
                .as_ref()
                .is_some_and(|(_, at)| at.elapsed() > NOTICE_TTL)
            {
                self.notice = None;
            }
        }
    }

    /// The parsed document for the current buffer, re-parsed only when the
    /// text changed.
    pub(crate) fn ensure_view(&mut self, width: usize) -> &View {
        if self.parsed.version != self.editor.version() {
            let text = self.editor.text();
            self.parsed = Parsed {
                version: self.editor.version(),
                doc: syntax::parse(&text),
                index: LineIndex::new(&text),
            };
        }
        let key = (
            self.editor.version(),
            self.editor.cursor(),
            width,
            self.preview,
        );
        if self.view.as_ref().is_none_or(|v| v.key != key) {
            let view = view::build(
                &self.editor,
                &self.parsed.doc,
                &self.parsed.index,
                width,
                self.preview,
                &self.theme,
            );
            self.view = Some(CachedView { key, view });
        }
        &self.view.as_ref().expect("view was just built").view
    }

    pub(crate) fn notify(&mut self, message: impl Into<String>) {
        self.notice = Some((message.into(), Instant::now()));
    }

    // ----- input -----

    /// Handles one key press.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        match self.mode {
            Mode::Edit => self.handle_edit_key(key),
            Mode::ConfirmQuit => self.handle_confirm_quit_key(key),
            Mode::Find { .. } => self.handle_find_key(key),
            Mode::SaveAs { .. } => self.handle_save_as_key(key),
        }
    }

    /// Handles pasted text.
    pub fn handle_paste(&mut self, text: &str) {
        match &mut self.mode {
            Mode::Edit => self.editor.insert_str(text),
            Mode::Find { query, .. } => {
                query.push_str(text.lines().next().unwrap_or_default());
                self.search_from_origin();
            }
            Mode::SaveAs { input } => input.push_str(text.lines().next().unwrap_or_default()),
            Mode::ConfirmQuit => {}
        }
    }

    fn handle_edit_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let page = self.body_height.saturating_sub(1).max(1);
        let mut vertical = false;

        match (key.code, ctrl, alt) {
            (KeyCode::Char('q'), true, _) => self.request_quit(),
            (KeyCode::Char('s'), true, _) => self.save(),
            (KeyCode::Char('z' | 'Z'), true, _) if shift => self.redo(),
            (KeyCode::Char('z'), true, _) => self.undo(),
            (KeyCode::Char('y'), true, _) => self.redo(),
            (KeyCode::Char('p'), true, _) => {
                self.preview = !self.preview;
                self.notify(if self.preview {
                    "Live preview on"
                } else {
                    "Live preview off (showing raw Markdown)"
                });
            }
            (KeyCode::Char('f'), true, _) => self.start_find(),
            (KeyCode::Char('g'), true, _) => self.find_next(),
            (KeyCode::Left, true, _) | (KeyCode::Left, _, true) => self.editor.word_left(),
            (KeyCode::Right, true, _) | (KeyCode::Right, _, true) => self.editor.word_right(),
            (KeyCode::Home, true, _) => self.editor.doc_start(),
            (KeyCode::End, true, _) => self.editor.doc_end(),
            (KeyCode::Char('a'), true, _) | (KeyCode::Home, ..) => self.editor.line_start(),
            (KeyCode::Char('e'), true, _) | (KeyCode::End, ..) => self.editor.line_end(),

            (KeyCode::Left, ..) => self.editor.move_left(),
            (KeyCode::Right, ..) => self.editor.move_right(),
            (KeyCode::Up, ..) => {
                vertical = true;
                self.move_visual(-1);
            }
            (KeyCode::Down, ..) => {
                vertical = true;
                self.move_visual(1);
            }
            (KeyCode::PageUp, ..) => {
                vertical = true;
                self.move_visual(-(page as isize));
            }
            (KeyCode::PageDown, ..) => {
                vertical = true;
                self.move_visual(page as isize);
            }
            (KeyCode::Enter, ..) => self.editor.insert_newline(),
            (KeyCode::Backspace, ..) => self.editor.backspace(),
            (KeyCode::Delete, ..) => self.editor.delete_forward(),
            (KeyCode::Tab, ..) => self.editor.insert_str("  "),
            (KeyCode::BackTab, ..) => self.editor.dedent_line(),
            (KeyCode::Esc, ..) => self.notice = None,
            (KeyCode::Char(c), false, false) => self.editor.insert_char(c),
            _ => {}
        }

        if !vertical {
            self.sticky_x = None;
        }
    }

    fn handle_confirm_quit_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') if ctrl => self.quit = true,
            KeyCode::Char('y' | 'Y') => self.quit = true,
            _ => {
                self.mode = Mode::Edit;
                self.notify("Quit cancelled");
            }
        }
    }

    fn handle_find_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match (key.code, ctrl) {
            (KeyCode::Esc, _) => {
                if let Mode::Find { query, .. } = &self.mode {
                    self.last_query.clone_from(query);
                }
                self.mode = Mode::Edit;
            }
            (KeyCode::Enter, _) | (KeyCode::Char('g' | 'f'), true) => {
                if let Mode::Find { query, .. } = &self.mode {
                    self.last_query.clone_from(query);
                }
                self.find_next();
            }
            (KeyCode::Backspace, _) => {
                if let Mode::Find { query, .. } = &mut self.mode {
                    query.pop();
                }
                self.search_from_origin();
            }
            (KeyCode::Char(c), false) => {
                if let Mode::Find { query, .. } = &mut self.mode {
                    query.push(c);
                }
                self.search_from_origin();
            }
            _ => {}
        }
    }

    fn handle_save_as_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match (key.code, ctrl) {
            (KeyCode::Esc, _) => self.mode = Mode::Edit,
            (KeyCode::Enter, _) => {
                let Mode::SaveAs { input } = &self.mode else {
                    return;
                };
                let name = input.trim().to_owned();
                if name.is_empty() {
                    return;
                }
                self.mode = Mode::Edit;
                match self.editor.save_as(&name) {
                    Ok(()) => self.notify(format!("Saved {name}")),
                    Err(e) => self.notify(format!("Could not save {name}: {e}")),
                }
            }
            (KeyCode::Backspace, _) => {
                if let Mode::SaveAs { input } = &mut self.mode {
                    input.pop();
                }
            }
            (KeyCode::Char(c), false) => {
                if let Mode::SaveAs { input } = &mut self.mode {
                    input.push(c);
                }
            }
            _ => {}
        }
    }

    // ----- actions -----

    fn request_quit(&mut self) {
        if self.editor.is_dirty() {
            self.mode = Mode::ConfirmQuit;
        } else {
            self.quit = true;
        }
    }

    fn save(&mut self) {
        if self.editor.path().is_none() {
            self.mode = Mode::SaveAs {
                input: String::new(),
            };
            return;
        }
        match self.editor.save() {
            Ok(()) => {
                let name = self.file_name();
                self.notify(format!("Saved {name}"));
            }
            Err(e) => self.notify(format!("Could not save: {e}")),
        }
    }

    fn undo(&mut self) {
        if !self.editor.undo() {
            self.notify("Nothing to undo");
        }
    }

    fn redo(&mut self) {
        if !self.editor.redo() {
            self.notify("Nothing to redo");
        }
    }

    fn start_find(&mut self) {
        self.mode = Mode::Find {
            query: String::new(),
            origin: self.editor.cursor(),
        };
    }

    /// Incremental search: jump to the first match at or after where the
    /// search started, or back to the origin when nothing matches.
    fn search_from_origin(&mut self) {
        let Mode::Find { query, origin } = &self.mode else {
            return;
        };
        let (query, origin) = (query.clone(), *origin);
        if query.is_empty() {
            self.editor.set_cursor(origin);
            return;
        }
        let from = self.char_idx_of(origin);
        if let Some(pos) = self.editor.find(&query, from) {
            self.editor.set_cursor(pos);
        } else {
            self.editor.set_cursor(origin);
            self.notify(format!("No match for \"{query}\""));
        }
    }

    fn find_next(&mut self) {
        let query = match &self.mode {
            Mode::Find { query, .. } if !query.is_empty() => query.clone(),
            _ => self.last_query.clone(),
        };
        if query.is_empty() {
            self.notify("Nothing to search for (Ctrl+F to start)");
            return;
        }
        let from = self.editor.cursor_char_idx() + 1;
        match self.editor.find(&query, from) {
            Some(pos) => self.editor.set_cursor(pos),
            None => self.notify(format!("No match for \"{query}\"")),
        }
    }

    /// Moves the cursor `delta` screen rows, through wrapped lines and
    /// rendered blocks alike.
    fn move_visual(&mut self, delta: isize) {
        let width = self.body_width.max(1);
        let (row, x) = self.ensure_view(width).cursor;
        let x = self.sticky_x.unwrap_or(x);
        self.sticky_x = Some(x);

        let view = self.ensure_view(width);
        let last = view.rows.len().saturating_sub(1);
        let target = (row as isize + delta).clamp(0, last as isize) as usize;
        if target == row {
            // Already at the edge: fall back to plain line movement so Down
            // on the last row of a wrapped line still leaves it.
            if delta < 0 {
                self.editor.move_up();
            } else {
                self.editor.move_down();
            }
            return;
        }
        let source = view.rows[target].source;
        let pos = match source {
            Source::Raw {
                line,
                start_col,
                end_col,
            } => {
                let text = self.editor.line(line);
                let col = view::col_at_x(&text, start_col, x).min(end_col);
                Position::new(line, col)
            }
            Source::Rendered { first, last } => {
                let line = if delta < 0 { last } else { first };
                let text = self.editor.line(line);
                Position::new(line, view::col_at_x(&text, 0, x))
            }
        };
        self.editor.set_cursor(pos);
    }

    // ----- helpers -----

    pub(crate) fn file_name(&self) -> String {
        self.editor.path().and_then(|p| p.file_name()).map_or_else(
            || "untitled".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    fn char_idx_of(&self, pos: Position) -> usize {
        (0..pos.line.min(self.editor.len_lines()))
            .map(|l| self.editor.line_len(l) + 1)
            .sum::<usize>()
            + pos.col
    }
}
