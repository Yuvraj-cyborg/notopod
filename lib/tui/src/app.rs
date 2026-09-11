//! Application state and key handling.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use canvas::BoxKind;
use editor::{Editor, Position};
use graphics::Graphics;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::text::Line;
use ratatui::DefaultTerminal;
use render::Drawings;
use syntax::{Document, LineIndex};
use theme::Theme;

use crate::canvas_mode::{self, CanvasState, Placing, Tool};
use crate::files::FilePanel;
use crate::view::{self, Override, Source, View};

/// How long a status-bar message stays visible.
const NOTICE_TTL: Duration = Duration::from_secs(4);

/// Which part of the screen has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    /// The note.
    Editor,
    /// The file panel on the left.
    Files,
}

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
    /// Typing the name of a note to open in a new tab.
    Open {
        /// The name so far.
        input: String,
    },
    /// Asked to quit with unsaved changes.
    ConfirmQuit,
    /// Asked to close a tab with unsaved changes.
    ConfirmClose,
    /// Drawing inside a ```` ```draw ```` block.
    Canvas(CanvasState),
}

/// One open note: its buffer plus what is cached about it.
pub(crate) struct Tab {
    pub(crate) editor: Editor,
    pub(crate) scroll: usize,
    parsed: Parsed,
    view: Option<CachedView>,
    /// Screen column to aim for when moving up/down.
    sticky_x: Option<usize>,
}

impl Tab {
    fn new(editor: Editor) -> Self {
        Self {
            editor,
            scroll: 0,
            parsed: Parsed::empty(),
            view: None,
            sticky_x: None,
        }
    }

    /// Drops everything that can be rebuilt from the text: the parse and
    /// the rendered rows. A tab that is not on screen keeps only its
    /// rope, so a dozen open notes cost a dozen buffers, not a dozen
    /// render caches.
    fn sleep(&mut self) {
        self.parsed = Parsed::empty();
        self.view = None;
    }

    /// The tab's name for the tab bar and the status bar.
    pub(crate) fn name(&self) -> String {
        self.editor.path().and_then(|p| p.file_name()).map_or_else(
            || "untitled".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        )
    }
}

/// The editor screen.
pub struct App {
    tabs: Vec<Tab>,
    active: usize,
    pub(crate) theme: Theme,
    pub(crate) graphics: Graphics,
    pub(crate) preview: bool,
    pub(crate) mode: Mode,
    pub(crate) notice: Option<(String, Instant)>,
    pub(crate) body_height: usize,
    pub(crate) body_width: usize,
    /// The file panel, while it is open.
    pub(crate) files: Option<FilePanel>,
    pub(crate) focus: Focus,
    last_query: String,
    quit: bool,
}

struct Parsed {
    version: u64,
    doc: Document,
    index: LineIndex,
}

impl Parsed {
    fn empty() -> Self {
        Self {
            version: u64::MAX,
            doc: Document::default(),
            index: LineIndex::new(""),
        }
    }
}

struct CachedView {
    /// Buffer version, cursor, width, preview flag, canvas generation.
    key: (u64, Position, usize, bool, Option<u64>),
    view: View,
}

impl App {
    /// Creates the screen around `editor`, drawn with `theme`. Drawings
    /// come out as braille until [`App::with_graphics`] says otherwise.
    pub fn new(editor: Editor, theme: Theme) -> Self {
        Self {
            tabs: vec![Tab::new(editor)],
            active: 0,
            theme,
            graphics: Graphics::braille(),
            preview: true,
            mode: Mode::Edit,
            notice: None,
            body_height: 0,
            body_width: 80,
            files: None,
            focus: Focus::Editor,
            last_query: String::new(),
            quit: false,
        }
    }

    /// Draws ```` ```draw ```` blocks with `graphics` (pictures, where the
    /// terminal can show them).
    #[must_use]
    pub fn with_graphics(mut self, graphics: Graphics) -> Self {
        self.graphics = graphics;
        self
    }

    /// Escape sequences that delete this session's pictures from the
    /// terminal. Write them before restoring the terminal.
    pub fn release_graphics(&mut self) -> String {
        self.graphics.release_all()
    }

    /// Runs the event loop until the user quits.
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;
            if self.quit {
                return Ok(());
            }
            if event::poll(Duration::from_millis(250))? {
                self.handle_event(event::read()?);
                // Catch up with whatever else is already waiting, so a held
                // key does not cost a frame per repeat.
                while !self.quit && event::poll(Duration::ZERO)? {
                    self.handle_event(event::read()?);
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

    // ----- tabs -----

    /// The note being edited.
    pub fn editor(&self) -> &Editor {
        &self.tabs[self.active].editor
    }

    /// The note being edited, for changing it.
    pub fn editor_mut(&mut self) -> &mut Editor {
        &mut self.tabs[self.active].editor
    }

    pub(crate) fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    pub(crate) fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    /// Every open note, in tab order.
    pub(crate) fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// Index of the tab on screen.
    pub(crate) fn active(&self) -> usize {
        self.active
    }

    /// Opens `path` in a new tab, or switches to it if it is already open.
    /// A file that does not exist yet is an empty note that will be
    /// created on save. Returns whether the note is now on screen.
    pub fn open_path(&mut self, path: &Path) -> bool {
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.editor.path().is_some_and(|p| same_file(p, path)))
        {
            self.switch_tab(i);
            return true;
        }
        match Editor::open(path) {
            Ok(editor) => {
                self.add_tab(editor);
                true
            }
            Err(e) => {
                self.notify(format!("Cannot open {}: {e}", path.display()));
                false
            }
        }
    }

    /// Opens an empty, unnamed note in a new tab.
    pub fn new_tab(&mut self) {
        self.add_tab(Editor::new());
    }

    fn add_tab(&mut self, editor: Editor) {
        // An untouched empty first tab is just the screen waiting for a
        // note; the opened one takes its place rather than sitting beside it.
        let placeholder = self.tabs.len() == 1
            && self.tabs[0].editor.path().is_none()
            && !self.tabs[0].editor.is_dirty()
            && self.tabs[0].editor.len_chars() == 0;
        self.leave_tab();
        if placeholder {
            self.tabs[0] = Tab::new(editor);
        } else {
            self.tabs.push(Tab::new(editor));
            self.active = self.tabs.len() - 1;
        }
    }

    /// Shows tab `index`.
    pub fn switch_tab(&mut self, index: usize) {
        if index >= self.tabs.len() || index == self.active {
            return;
        }
        self.leave_tab();
        self.active = index;
    }

    fn next_tab(&mut self) {
        self.switch_tab((self.active + 1) % self.tabs.len());
    }

    fn prev_tab(&mut self) {
        self.switch_tab((self.active + self.tabs.len() - 1) % self.tabs.len());
    }

    /// Puts the current tab to sleep and drops whatever the keyboard was
    /// doing in it: a drawing in progress is finished, a prompt cancelled.
    fn leave_tab(&mut self) {
        if matches!(self.mode, Mode::Canvas(_)) {
            self.exit_canvas();
        }
        self.mode = Mode::Edit;
        self.tabs[self.active].sleep();
    }

    /// Ctrl+W: closes the current tab, asking first if it has unsaved
    /// changes. The last tab is not closed but emptied.
    fn request_close_tab(&mut self) {
        if self.editor().is_dirty() {
            self.mode = Mode::ConfirmClose;
        } else {
            self.close_tab();
        }
    }

    fn close_tab(&mut self) {
        if matches!(self.mode, Mode::Canvas(_)) {
            self.exit_canvas();
        }
        self.mode = Mode::Edit;
        if self.tabs.len() == 1 {
            if self.editor().path().is_none() && self.editor().len_chars() == 0 {
                self.notify("Nothing to close");
            } else {
                self.tabs[0] = Tab::new(Editor::new());
            }
            return;
        }
        self.tabs.remove(self.active);
        self.active = self.active.min(self.tabs.len() - 1);
    }

    /// The parsed document for the current buffer, re-parsed only when the
    /// text changed.
    pub(crate) fn ensure_view(&mut self, width: usize) -> &View {
        let canvas_gen = match &self.mode {
            Mode::Canvas(c) => Some(c.generation),
            _ => None,
        };
        let key = {
            let editor = &self.tabs[self.active].editor;
            (
                editor.version(),
                editor.cursor(),
                width,
                self.preview,
                canvas_gen,
            )
        };
        let stale = self.tabs[self.active]
            .view
            .as_ref()
            .is_none_or(|v| v.key != key);
        if stale {
            let rendered = self.canvas_lines(width);
            let override_block = match (&self.mode, &rendered) {
                (Mode::Canvas(c), Some((last, lines))) => Some(Override {
                    first: c.fence,
                    last: *last,
                    lines,
                    cursor: (c.cursor.x.max(0) as usize, c.cursor.y.max(0) as usize),
                }),
                _ => None,
            };
            let tab = &mut self.tabs[self.active];
            if tab.parsed.version != tab.editor.version() {
                let text = tab.editor.text();
                tab.parsed = Parsed {
                    version: tab.editor.version(),
                    doc: syntax::parse(&text),
                    index: LineIndex::new(&text),
                };
            }
            let view = view::build(
                &tab.editor,
                &tab.parsed.doc,
                &tab.parsed.index,
                width,
                self.preview,
                &self.theme,
                override_block.as_ref(),
                &mut self.graphics,
            );
            tab.view = Some(CachedView { key, view });
        }
        &self.tabs[self.active]
            .view
            .as_ref()
            .expect("view was just built")
            .view
    }

    /// In canvas mode: the block's closing fence line and its rendered
    /// rows, with cursor, selection and preview drawn on.
    fn canvas_lines(&mut self, width: usize) -> Option<(usize, Vec<Line<'static>>)> {
        let Mode::Canvas(c) = &self.mode else {
            return None;
        };
        let (_, end) = canvas_mode::body_range(self.editor(), c.fence)?;
        let preview = c.preview();
        let overlay = canvas::Overlay {
            cursor: Some(c.cursor),
            selected: c.selected(),
            preview: preview.as_ref(),
            min_size: canvas_mode::MIN_SIZE,
        };
        let lines = self
            .graphics
            .draw(&c.drawing, &self.theme, width, Some(&overlay));
        Some((end, lines))
    }

    pub(crate) fn notify(&mut self, message: impl Into<String>) {
        self.notice = Some((message.into(), Instant::now()));
    }

    // ----- input -----

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Paste(text) => self.handle_paste(&text),
            _ => {}
        }
    }

    /// Handles one key press.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if self.focus == Focus::Files {
            self.handle_files_key(key);
            return;
        }
        match self.mode {
            Mode::Edit => self.handle_edit_key(key),
            Mode::ConfirmQuit => self.handle_confirm_quit_key(key),
            Mode::ConfirmClose => self.handle_confirm_close_key(key),
            Mode::Find { .. } => self.handle_find_key(key),
            Mode::SaveAs { .. } | Mode::Open { .. } => self.handle_prompt_key(key),
            Mode::Canvas(_) => self.handle_canvas_key(key),
        }
    }

    /// Handles pasted text.
    pub fn handle_paste(&mut self, text: &str) {
        match &mut self.mode {
            Mode::Edit => self.editor_mut().insert_str(text),
            Mode::Find { query, .. } => {
                query.push_str(text.lines().next().unwrap_or_default());
                self.search_from_origin();
            }
            Mode::SaveAs { input } | Mode::Open { input } => {
                input.push_str(text.lines().next().unwrap_or_default());
            }
            Mode::Canvas(c) => {
                if let Tool::Text { input, .. } = &mut c.tool {
                    input.push_str(text.lines().next().unwrap_or_default());
                }
            }
            Mode::ConfirmQuit | Mode::ConfirmClose => {}
        }
    }

    /// Keys that mean the same thing whatever is being edited: tabs, files,
    /// quitting. Returns `true` when the key was one of them.
    pub(crate) fn handle_global_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        if !ctrl {
            return false;
        }
        match key.code {
            KeyCode::Char('q') => self.request_quit(),
            KeyCode::Char('o') => {
                self.mode = Mode::Open {
                    input: String::new(),
                };
            }
            KeyCode::Char('t') => self.new_tab(),
            KeyCode::Char('w') => self.request_close_tab(),
            KeyCode::Char('b') => self.toggle_files(),
            KeyCode::PageDown | KeyCode::Tab if !shift => self.next_tab(),
            KeyCode::PageUp | KeyCode::BackTab | KeyCode::Tab => self.prev_tab(),
            _ => return false,
        }
        true
    }

    // ----- file panel -----

    /// Ctrl+B: opens the panel and gives it the keyboard; from the panel,
    /// closes it; from the note while the panel is open, goes to it.
    pub(crate) fn toggle_files(&mut self) {
        match (self.focus, &self.files) {
            (Focus::Files, _) => {
                self.files = None;
                self.focus = Focus::Editor;
            }
            (Focus::Editor, Some(_)) => self.focus = Focus::Files,
            (Focus::Editor, None) => {
                let (root, note) = self.panel_root();
                let mut panel = FilePanel::new(root);
                if let Some(note) = note {
                    panel.reveal(&note);
                }
                self.files = Some(panel);
                self.focus = Focus::Files;
            }
        }
    }

    /// Where the panel starts: the working directory when the current
    /// note is inside it, otherwise the note's own directory. Also the
    /// note's path in the same terms, for putting the cursor on it.
    fn panel_root(&self) -> (PathBuf, Option<PathBuf>) {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);
        let Some(path) = self.editor().path() else {
            return (cwd, None);
        };
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        let dir = abs.parent().map_or_else(|| cwd.clone(), Path::to_path_buf);
        let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
        let note = abs.file_name().map(|n| dir.join(n));
        if dir.starts_with(&cwd) {
            (cwd, note)
        } else {
            (dir, note)
        }
    }

    fn handle_files_key(&mut self, key: KeyEvent) {
        if self.handle_global_key(key) {
            return;
        }
        let page = self.body_height.saturating_sub(2).max(1) as isize;
        let Some(panel) = &mut self.files else {
            self.focus = Focus::Editor;
            return;
        };
        let mut open_note = false;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => panel.move_by(-1),
            KeyCode::Down | KeyCode::Char('j') => panel.move_by(1),
            KeyCode::PageUp => panel.move_by(-page),
            KeyCode::PageDown => panel.move_by(page),
            KeyCode::Home => panel.move_to(0),
            KeyCode::End | KeyCode::Char('G') => panel.move_to(usize::MAX),
            KeyCode::Left | KeyCode::Char('h') => panel.collapse(),
            KeyCode::Right | KeyCode::Char('l') => open_note = !panel.expand(),
            KeyCode::Enter => open_note = !panel.toggle(),
            KeyCode::Char('r') => {
                panel.refresh();
                self.notice = Some(("Files re-read".to_owned(), Instant::now()));
            }
            KeyCode::Esc | KeyCode::Tab => self.focus = Focus::Editor,
            _ => {}
        }
        if open_note {
            self.open_selected_file();
        }
    }

    /// Enter on a note in the panel: shows it and hands the keyboard back.
    fn open_selected_file(&mut self) {
        let Some(path) = self
            .files
            .as_ref()
            .and_then(FilePanel::selected)
            .map(|row| row.path.clone())
        else {
            return;
        };
        if self.open_path(&path) {
            self.focus = Focus::Editor;
        }
    }

    /// Paths of the notes open in tabs, for marking them in the panel.
    pub(crate) fn open_paths(&self) -> Vec<PathBuf> {
        self.tabs
            .iter()
            .filter_map(|t| t.editor.path())
            .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()))
            .collect()
    }

    fn handle_edit_key(&mut self, key: KeyEvent) {
        if self.handle_global_key(key) {
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let page = self.body_height.saturating_sub(1).max(1);
        let mut vertical = false;

        match (key.code, ctrl, alt) {
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
            (KeyCode::Char('d'), true, _) => self.enter_canvas(),
            (KeyCode::Left, true, _) | (KeyCode::Left, _, true) => self.editor_mut().word_left(),
            (KeyCode::Right, true, _) | (KeyCode::Right, _, true) => {
                self.editor_mut().word_right();
            }
            (KeyCode::Home, true, _) => self.editor_mut().doc_start(),
            (KeyCode::End, true, _) => self.editor_mut().doc_end(),
            (KeyCode::Char('a'), true, _) | (KeyCode::Home, ..) => self.editor_mut().line_start(),
            (KeyCode::Char('e'), true, _) | (KeyCode::End, ..) => self.editor_mut().line_end(),

            (KeyCode::Left, ..) => self.editor_mut().move_left(),
            (KeyCode::Right, ..) => self.editor_mut().move_right(),
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
            (KeyCode::Enter, ..) => self.editor_mut().insert_newline(),
            (KeyCode::Backspace, ..) => self.editor_mut().backspace(),
            (KeyCode::Delete, ..) => self.editor_mut().delete_forward(),
            (KeyCode::Tab, ..) => self.editor_mut().insert_str("  "),
            (KeyCode::BackTab, ..) => self.editor_mut().dedent_line(),
            (KeyCode::Esc, ..) => self.notice = None,
            (KeyCode::Char(c), false, false) => self.editor_mut().insert_char(c),
            _ => {}
        }

        if !vertical {
            self.tab_mut().sticky_x = None;
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

    fn handle_confirm_close_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('w') if ctrl => self.close_tab(),
            KeyCode::Char('y' | 'Y') => self.close_tab(),
            _ => {
                self.mode = Mode::Edit;
                self.notify("Close cancelled");
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

    /// Save-as and open share one line editor in the status bar.
    fn handle_prompt_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match (key.code, ctrl) {
            (KeyCode::Esc, _) => self.mode = Mode::Edit,
            (KeyCode::Enter, _) => {
                let input = match &self.mode {
                    Mode::SaveAs { input } | Mode::Open { input } => input.trim().to_owned(),
                    _ => return,
                };
                if input.is_empty() {
                    return;
                }
                let opening = matches!(self.mode, Mode::Open { .. });
                self.mode = Mode::Edit;
                if opening {
                    let path = self.resolve_note(&input);
                    self.open_path(&path);
                } else {
                    match self.editor_mut().save_as(&input) {
                        Ok(()) => self.notify(format!("Saved {input}")),
                        Err(e) => self.notify(format!("Could not save {input}: {e}")),
                    }
                }
            }
            (KeyCode::Backspace, _) => {
                if let Mode::SaveAs { input } | Mode::Open { input } = &mut self.mode {
                    input.pop();
                }
            }
            (KeyCode::Char(c), false) => {
                if let Mode::SaveAs { input } | Mode::Open { input } = &mut self.mode {
                    input.push(c);
                }
            }
            _ => {}
        }
    }

    /// Where a name typed at the open prompt points: as given if it exists
    /// or is a path, otherwise next to the current note, with `.md` added
    /// when it has no extension.
    pub(crate) fn resolve_note(&self, name: &str) -> PathBuf {
        let given = PathBuf::from(name);
        if given.exists() {
            return given;
        }
        let file = with_md(given);
        if file.exists() || file.is_absolute() || name.contains(['/', '\\']) {
            return file;
        }
        match self.editor().path().and_then(Path::parent) {
            Some(dir) if !dir.as_os_str().is_empty() => dir.join(file),
            _ => file,
        }
    }

    // ----- canvas mode -----

    /// Ctrl+D: draw in the block under the cursor, or start a new one.
    pub(crate) fn enter_canvas(&mut self) {
        let width = self.body_width.max(1);
        self.ensure_view(width);
        let line = self.editor().cursor().line;
        let existing = {
            let tab = &self.tabs[self.active];
            canvas_mode::block_at(&tab.parsed.doc, &tab.parsed.index, line)
        };
        let fence = match existing {
            Some((fence, _)) => fence,
            None => canvas_mode::insert_block(self.editor_mut()),
        };
        if canvas_mode::ensure_closed(self.editor_mut(), fence).is_none() {
            self.notify("Not a draw block");
            return;
        }
        let Some(drawing) = canvas_mode::read(self.editor(), fence) else {
            self.notify("Not a draw block");
            return;
        };
        let cursor = drawing
            .bounds()
            .map_or(canvas::Point::new(2, 1), |b| canvas::Point::new(b.x, b.y));
        self.editor_mut().set_cursor(Position::new(fence, 0));
        self.mode = Mode::Canvas(CanvasState {
            fence,
            drawing,
            cursor,
            tool: Tool::Select,
            generation: 0,
        });
        if existing.is_none() {
            self.notify("New drawing. r rect  e ellipse  d diamond  l line  a arrow  t text  ? help  Esc done");
        }
    }

    /// Leaves canvas mode with the cursor just after the block, so the
    /// drawing stays rendered.
    fn exit_canvas(&mut self) {
        let Mode::Canvas(c) = &self.mode else {
            return;
        };
        let after =
            canvas_mode::body_range(self.editor(), c.fence).map_or(c.fence, |(_, end)| end + 1);
        self.mode = Mode::Edit;
        self.editor_mut().set_cursor(Position::new(after, 0));
    }

    /// Writes the canvas drawing into the buffer.
    fn canvas_write_back(&mut self) {
        let Mode::Canvas(c) = &self.mode else {
            return;
        };
        let (fence, drawing) = (c.fence, c.drawing.clone());
        if !canvas_mode::write(self.editor_mut(), fence, &drawing) {
            self.notify("The draw block is gone");
            self.mode = Mode::Edit;
        }
    }

    /// Re-reads the drawing from the buffer (after undo/redo or a
    /// cancelled move). Leaves canvas mode if the block disappeared.
    fn canvas_reload(&mut self) {
        let fence = match &self.mode {
            Mode::Canvas(c) => c.fence,
            _ => return,
        };
        let drawing = canvas_mode::read(self.editor(), fence);
        let Mode::Canvas(c) = &mut self.mode else {
            return;
        };
        if let Some(drawing) = drawing {
            c.drawing = drawing;
            c.generation = c.generation.wrapping_add(1);
        } else {
            self.mode = Mode::Edit;
            self.notify("The draw block is gone");
        }
    }

    /// Typing a label in canvas mode: the status bar has the keyboard.
    fn handle_label_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let Mode::Canvas(c) = &mut self.mode else {
            return;
        };
        let Tool::Text { input, .. } = &mut c.tool else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                c.cancel();
            }
            KeyCode::Enter => {
                if c.commit() {
                    self.canvas_write_back();
                }
            }
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Char(ch) if !ctrl => input.push(ch),
            _ => {}
        }
    }

    fn handle_canvas_key(&mut self, key: KeyEvent) {
        if matches!(&self.mode, Mode::Canvas(c) if c.is_typing()) {
            self.handle_label_key(key);
            return;
        }
        if self.handle_global_key(key) {
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let max_x = self.body_width.saturating_sub(2) as i32;
        let Mode::Canvas(c) = &mut self.mode else {
            return;
        };

        let step = if shift { 5 } else { 1 };
        match (key.code, ctrl) {
            (KeyCode::Char('s'), true) => self.save(),
            (KeyCode::Char('z'), true) => {
                if !self.editor_mut().undo() {
                    self.notify("Nothing to undo");
                }
                self.canvas_reload();
            }
            (KeyCode::Char('y'), true) => {
                if !self.editor_mut().redo() {
                    self.notify("Nothing to redo");
                }
                self.canvas_reload();
            }
            (KeyCode::Char('d'), true) | (KeyCode::Esc, _) => {
                if c.tool == Tool::Select {
                    self.exit_canvas();
                } else if c.cancel() {
                    self.canvas_reload();
                }
            }
            (KeyCode::Enter, _) => {
                if c.commit() {
                    self.canvas_write_back();
                }
            }
            (KeyCode::Char(' '), _) => c.add_point(),
            (KeyCode::Left, _) => c.nudge(-step, 0, max_x),
            (KeyCode::Right, _) => c.nudge(step, 0, max_x),
            (KeyCode::Up, _) => c.nudge(0, -step, max_x),
            (KeyCode::Down, _) => c.nudge(0, step, max_x),
            (KeyCode::Char('r'), false) => c.start(Placing::Box(BoxKind::Rect)),
            (KeyCode::Char('e'), false) => c.start(Placing::Box(BoxKind::Ellipse)),
            (KeyCode::Char('d'), false) => c.start(Placing::Box(BoxKind::Diamond)),
            (KeyCode::Char('l'), false) => c.start(Placing::Line(false)),
            (KeyCode::Char('a'), false) => c.start(Placing::Line(true)),
            (KeyCode::Char('t'), false) => c.edit_text(),
            (KeyCode::Char('m'), false) => {
                if !c.grab() {
                    self.notify("Nothing under the cursor to move");
                }
            }
            (KeyCode::Char('x'), false) | (KeyCode::Delete | KeyCode::Backspace, _) => {
                if c.delete() {
                    self.canvas_write_back();
                } else {
                    self.notify("Nothing under the cursor to delete");
                }
            }
            (KeyCode::Char('f'), false) => {
                if c.restyle(|a| a.fill = !a.fill) {
                    self.canvas_write_back();
                }
            }
            (KeyCode::Char('-'), false) => {
                if c.restyle(|a| a.dashed = !a.dashed) {
                    self.canvas_write_back();
                }
            }
            (KeyCode::Char('o'), false) => {
                if c.restyle(|a| a.round = !a.round) {
                    self.canvas_write_back();
                }
            }
            (KeyCode::Char('c'), false) => {
                if c.restyle(|a| a.color = canvas_mode::next_color(a.color.as_deref())) {
                    self.canvas_write_back();
                }
            }
            (KeyCode::Char('?'), false) => self.notify(
                "arrows move (Shift: ×5)  r rect  e ellipse  d diamond  l line  a arrow  Space corner  t text  m move  x delete  f fill  - dash  o round  c colour  Esc done",
            ),
            _ => {}
        }
    }

    // ----- actions -----

    pub(crate) fn request_quit(&mut self) {
        if self.tabs.iter().any(|t| t.editor.is_dirty()) {
            self.mode = Mode::ConfirmQuit;
        } else {
            self.quit = true;
        }
    }

    /// Number of tabs with unsaved changes.
    pub(crate) fn dirty_count(&self) -> usize {
        self.tabs.iter().filter(|t| t.editor.is_dirty()).count()
    }

    pub(crate) fn save(&mut self) {
        if self.editor().path().is_none() {
            self.mode = Mode::SaveAs {
                input: String::new(),
            };
            return;
        }
        match self.editor_mut().save() {
            Ok(()) => {
                let name = self.file_name();
                self.notify(format!("Saved {name}"));
            }
            Err(e) => self.notify(format!("Could not save: {e}")),
        }
    }

    pub(crate) fn undo(&mut self) {
        if !self.editor_mut().undo() {
            self.notify("Nothing to undo");
        }
    }

    pub(crate) fn redo(&mut self) {
        if !self.editor_mut().redo() {
            self.notify("Nothing to redo");
        }
    }

    pub(crate) fn start_find(&mut self) {
        self.mode = Mode::Find {
            query: String::new(),
            origin: self.editor().cursor(),
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
            self.editor_mut().set_cursor(origin);
            return;
        }
        let from = self.editor().char_idx(origin);
        if let Some(pos) = self.editor().find(&query, from) {
            self.editor_mut().set_cursor(pos);
        } else {
            self.editor_mut().set_cursor(origin);
            self.notify(format!("No match for \"{query}\""));
        }
    }

    pub(crate) fn find_next(&mut self) {
        let query = match &self.mode {
            Mode::Find { query, .. } if !query.is_empty() => query.clone(),
            _ => self.last_query.clone(),
        };
        if query.is_empty() {
            self.notify("Nothing to search for (Ctrl+F to start)");
            return;
        }
        let from = self.editor().cursor_char_idx() + 1;
        match self.editor().find(&query, from) {
            Some(pos) => self.editor_mut().set_cursor(pos),
            None => self.notify(format!("No match for \"{query}\"")),
        }
    }

    /// Moves the cursor `delta` screen rows, through wrapped lines and
    /// rendered blocks alike.
    pub(crate) fn move_visual(&mut self, delta: isize) {
        let width = self.body_width.max(1);
        let (row, x) = self.ensure_view(width).cursor;
        let x = self.tab().sticky_x.unwrap_or(x);
        self.tab_mut().sticky_x = Some(x);

        let view = self.ensure_view(width);
        let last = view.rows.len().saturating_sub(1);
        let target = (row as isize + delta).clamp(0, last as isize) as usize;
        if target == row {
            // Already at the edge: fall back to plain line movement so Down
            // on the last row of a wrapped line still leaves it.
            if delta < 0 {
                self.editor_mut().move_up();
            } else {
                self.editor_mut().move_down();
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
                let text = self.editor().line(line);
                let col = view::col_at_x(&text, start_col, x).min(end_col);
                Position::new(line, col)
            }
            Source::Rendered { first, last } => {
                let line = if delta < 0 { last } else { first };
                let text = self.editor().line(line);
                Position::new(line, view::col_at_x(&text, 0, x))
            }
        };
        self.editor_mut().set_cursor(pos);
    }

    // ----- helpers -----

    pub(crate) fn file_name(&self) -> String {
        self.tab().name()
    }
}

/// `path` with `.md` added when it has no extension.
pub(crate) fn with_md(mut path: PathBuf) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension("md");
    }
    path
}

/// Whether two paths name the same file, seen through symlinks and
/// relative prefixes where the files exist.
pub(crate) fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn app(text: &str) -> App {
        App::new(Editor::from_text(text), Theme::default())
    }

    pub(crate) fn key(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    pub(crate) fn ctrl(app: &mut App, ch: char) {
        app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL));
    }

    pub(crate) fn type_str(app: &mut App, s: &str) {
        for ch in s.chars() {
            key(app, KeyCode::Char(ch));
        }
    }

    #[test]
    fn ctrl_d_creates_a_block_and_draws_a_labelled_rect() {
        let mut a = app("# Notes\n");
        a.editor_mut().set_cursor(Position::new(1, 0));
        ctrl(&mut a, 'd');
        assert!(matches!(a.mode, Mode::Canvas(_)));
        assert!(a.editor().text().contains("```draw\n```"));

        // Rectangle from (2,1) to (9,3), then label it.
        key(&mut a, KeyCode::Char('r'));
        for _ in 0..7 {
            key(&mut a, KeyCode::Right);
        }
        key(&mut a, KeyCode::Down);
        key(&mut a, KeyCode::Down);
        key(&mut a, KeyCode::Enter);
        assert!(
            a.editor().text().contains("rect 2,1 8x3\n"),
            "{}",
            a.editor().text()
        );

        key(&mut a, KeyCode::Char('t'));
        type_str(&mut a, "Parser");
        key(&mut a, KeyCode::Enter);
        assert!(a.editor().text().contains("rect 2,1 8x3 \"Parser\"\n"));

        // Fill, colour, then an arrow out of the box.
        key(&mut a, KeyCode::Char('f'));
        key(&mut a, KeyCode::Char('c'));
        assert!(a.editor().text().contains("\"Parser\" fill color=red\n"));
        key(&mut a, KeyCode::Char('a'));
        for _ in 0..12 {
            key(&mut a, KeyCode::Right);
        }
        key(&mut a, KeyCode::Enter);
        assert!(
            a.editor().text().contains("line 9,3 -> 21,3\n"),
            "{}",
            a.editor().text()
        );

        // Esc leaves canvas mode with the cursor after the block.
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.mode, Mode::Edit);
        let after = a.editor().cursor().line;
        assert!(a.editor().line(after - 1).starts_with("```"));

        // Every canvas action was one undo step.
        ctrl(&mut a, 'z');
        assert!(!a.editor().text().contains("line 9,3"));
        ctrl(&mut a, 'z');
        assert!(!a.editor().text().contains("color=red"));
    }

    #[test]
    fn ctrl_d_on_an_existing_block_edits_it() {
        let mut a = app("```draw\nrect 0,0 4x2\n```\n\n");
        a.editor_mut().set_cursor(Position::new(1, 3));
        ctrl(&mut a, 'd');
        let Mode::Canvas(c) = &a.mode else {
            panic!("not in canvas mode");
        };
        assert_eq!(c.fence, 0);
        assert_eq!(c.drawing.shapes.len(), 1);
        assert_eq!(c.cursor, canvas::Point::new(0, 0));

        // Move the rectangle right by two and down by one, then delete it.
        key(&mut a, KeyCode::Char('m'));
        key(&mut a, KeyCode::Right);
        key(&mut a, KeyCode::Right);
        key(&mut a, KeyCode::Down);
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.editor().text(), "```draw\nrect 2,1 4x2\n```\n\n");
        key(&mut a, KeyCode::Char('x'));
        assert_eq!(a.editor().text(), "```draw\n```\n\n");
        ctrl(&mut a, 'z');
        assert_eq!(a.editor().text(), "```draw\nrect 2,1 4x2\n```\n\n");
        assert!(matches!(&a.mode, Mode::Canvas(c) if c.drawing.shapes.len() == 1));
    }

    #[test]
    fn cancelled_move_restores_and_esc_in_a_tool_does_not_exit() {
        let mut a = app("```draw\nrect 0,0 4x2\n```\n");
        ctrl(&mut a, 'd');
        key(&mut a, KeyCode::Char('m'));
        key(&mut a, KeyCode::Right);
        key(&mut a, KeyCode::Esc);
        assert!(matches!(&a.mode, Mode::Canvas(c) if c.tool == Tool::Select));
        assert_eq!(a.editor().text(), "```draw\nrect 0,0 4x2\n```\n");
        key(&mut a, KeyCode::Char('r'));
        key(&mut a, KeyCode::Esc);
        assert!(matches!(&a.mode, Mode::Canvas(_)));
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.mode, Mode::Edit);
    }

    #[test]
    fn view_shows_the_drawing_rendered_while_editing() {
        let mut a = app("```draw\nrect 0,0 6x3 \"hi\"\n```\n");
        ctrl(&mut a, 'd');
        let view = a.ensure_view(40);
        let texts: Vec<String> = view.rows.iter().map(|r| r.line.to_string()).collect();
        assert!(
            !texts[0].starts_with("```"),
            "block must be rendered, got {texts:?}"
        );
        assert!(texts.iter().any(|t| t.contains("hi")));
        assert_eq!(view.cursor, (0, 0));
    }

    #[test]
    fn tabs_open_switch_and_close() {
        let mut a = app("first\n");
        assert_eq!(a.tabs().len(), 1);

        // Ctrl+T adds an empty tab and shows it.
        ctrl(&mut a, 't');
        assert_eq!(a.tabs().len(), 2);
        assert_eq!(a.active(), 1);
        assert_eq!(a.editor().text(), "");
        type_str(&mut a, "second");

        // Ctrl+PageUp goes back; the first tab is untouched.
        a.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::CONTROL));
        assert_eq!(a.active(), 0);
        assert_eq!(a.editor().text(), "first\n");
        a.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::CONTROL));
        assert_eq!(a.active(), 1);

        // Closing a dirty tab asks first; `y` closes it.
        ctrl(&mut a, 'w');
        assert_eq!(a.mode, Mode::ConfirmClose);
        key(&mut a, KeyCode::Char('n'));
        assert_eq!(a.mode, Mode::Edit);
        assert_eq!(a.tabs().len(), 2);
        ctrl(&mut a, 'w');
        key(&mut a, KeyCode::Char('y'));
        assert_eq!(a.tabs().len(), 1);
        assert_eq!(a.editor().text(), "first\n");

        // The last tab is emptied rather than closed.
        ctrl(&mut a, 'w');
        assert_eq!(a.tabs().len(), 1);
        assert_eq!(a.editor().text(), "");
        ctrl(&mut a, 'w');
        assert_eq!(a.tabs().len(), 1);
    }

    #[test]
    fn opening_a_note_reuses_an_untouched_first_tab_and_finds_open_ones() {
        let dir = tempfile::tempdir().expect("tempdir");
        let one = dir.path().join("one.md");
        let two = dir.path().join("two.md");
        std::fs::write(&one, "# one\n").expect("write");
        std::fs::write(&two, "# two\n").expect("write");

        let mut a = app("");
        assert!(a.open_path(&one));
        assert_eq!(a.tabs().len(), 1, "the empty start tab was replaced");
        assert_eq!(a.editor().text(), "# one\n");

        assert!(a.open_path(&two));
        assert_eq!(a.tabs().len(), 2);
        assert_eq!(a.active(), 1);

        // Opening the first again switches instead of duplicating.
        assert!(a.open_path(&one));
        assert_eq!(a.tabs().len(), 2);
        assert_eq!(a.active(), 0);

        // The open prompt resolves a bare name next to the current note.
        ctrl(&mut a, 'o');
        type_str(&mut a, "three");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.tabs().len(), 3);
        assert_eq!(
            a.editor().path(),
            Some(dir.path().join("three.md").as_path())
        );
        assert_eq!(a.tabs()[2].name(), "three.md");
    }

    #[test]
    fn the_file_panel_opens_notes_into_tabs() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.md"), "# a\n").expect("write");
        std::fs::write(dir.path().join("b.md"), "# b\n").expect("write");

        let mut a = app("");
        assert!(a.open_path(&dir.path().join("a.md")));
        // Ctrl+B opens the panel over the note's folder, on the note.
        ctrl(&mut a, 'b');
        assert_eq!(a.focus, Focus::Files);
        let panel = a.files.as_ref().expect("panel");
        assert_eq!(panel.selected().map(|r| r.name.as_str()), Some("a.md"));

        // Down, Enter: b.md opens in a second tab and the note has the keys.
        key(&mut a, KeyCode::Down);
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.focus, Focus::Editor);
        assert_eq!(a.tabs().len(), 2);
        assert_eq!(a.editor().text(), "# b\n");
        assert!(a.files.is_some(), "the panel stays open");

        // Ctrl+B from the note goes back to the panel; from the panel, closes it.
        ctrl(&mut a, 'b');
        assert_eq!(a.focus, Focus::Files);
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.focus, Focus::Editor);
        ctrl(&mut a, 'b');
        ctrl(&mut a, 'b');
        assert!(a.files.is_none());
        assert_eq!(a.focus, Focus::Editor);
    }

    #[test]
    fn switching_tabs_leaves_canvas_mode_and_quit_counts_every_dirty_tab() {
        let mut a = app("```draw\nrect 0,0 4x2\n```\n");
        ctrl(&mut a, 'd');
        assert!(matches!(a.mode, Mode::Canvas(_)));
        ctrl(&mut a, 't');
        assert_eq!(a.mode, Mode::Edit);
        type_str(&mut a, "x");
        a.switch_tab(0);
        type_str(&mut a, "y");
        assert_eq!(a.dirty_count(), 2);
        ctrl(&mut a, 'q');
        assert_eq!(a.mode, Mode::ConfirmQuit);
        key(&mut a, KeyCode::Esc);
        assert!(!a.quit);
    }
}
