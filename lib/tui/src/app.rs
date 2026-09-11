//! Application state and key handling.

use std::time::{Duration, Instant};

use anyhow::Result;
use canvas::BoxKind;
use editor::{Editor, Position};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::text::Line;
use ratatui::DefaultTerminal;
use syntax::{Document, LineIndex};
use theme::Theme;

use crate::canvas_mode::{self, CanvasState, Placing, Tool};
use crate::view::{self, Override, Source, View};

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
    /// Drawing inside a ```` ```draw ```` block.
    Canvas(CanvasState),
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
    /// Buffer version, cursor, width, preview flag, canvas generation.
    key: (u64, Position, usize, bool, Option<u64>),
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
        let canvas_gen = match &self.mode {
            Mode::Canvas(c) => Some(c.generation),
            _ => None,
        };
        let key = (
            self.editor.version(),
            self.editor.cursor(),
            width,
            self.preview,
            canvas_gen,
        );
        if self.view.as_ref().is_none_or(|v| v.key != key) {
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
            let view = view::build(
                &self.editor,
                &self.parsed.doc,
                &self.parsed.index,
                width,
                self.preview,
                &self.theme,
                override_block.as_ref(),
            );
            self.view = Some(CachedView { key, view });
        }
        &self.view.as_ref().expect("view was just built").view
    }

    /// In canvas mode: the block's closing fence line and its rendered
    /// rows, with cursor, selection and preview drawn on.
    fn canvas_lines(&self, width: usize) -> Option<(usize, Vec<Line<'static>>)> {
        let Mode::Canvas(c) = &self.mode else {
            return None;
        };
        let (_, end) = canvas_mode::body_range(&self.editor, c.fence)?;
        let preview = c.preview();
        let overlay = canvas::Overlay {
            cursor: Some(c.cursor),
            selected: c.selected(),
            preview: preview.as_ref(),
            min_size: canvas_mode::MIN_SIZE,
        };
        let lines = canvas::render(&c.drawing, &self.theme, width, Some(&overlay));
        Some((end, lines))
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
            Mode::Canvas(_) => self.handle_canvas_key(key),
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
            Mode::Canvas(c) => {
                if let Tool::Text { input, .. } = &mut c.tool {
                    input.push_str(text.lines().next().unwrap_or_default());
                }
            }
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
            (KeyCode::Char('d'), true, _) => self.enter_canvas(),
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

    // ----- canvas mode -----

    /// Ctrl+D: draw in the block under the cursor, or start a new one.
    fn enter_canvas(&mut self) {
        let width = self.body_width.max(1);
        self.ensure_view(width);
        let line = self.editor.cursor().line;
        let existing = canvas_mode::block_at(&self.parsed.doc, &self.parsed.index, line);
        let fence = match existing {
            Some((fence, _)) => fence,
            None => canvas_mode::insert_block(&mut self.editor),
        };
        if canvas_mode::ensure_closed(&mut self.editor, fence).is_none() {
            self.notify("Not a draw block");
            return;
        }
        let Some(drawing) = canvas_mode::read(&self.editor, fence) else {
            self.notify("Not a draw block");
            return;
        };
        let cursor = drawing
            .bounds()
            .map_or(canvas::Point::new(2, 1), |b| canvas::Point::new(b.x, b.y));
        self.editor.set_cursor(Position::new(fence, 0));
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
            canvas_mode::body_range(&self.editor, c.fence).map_or(c.fence, |(_, end)| end + 1);
        self.mode = Mode::Edit;
        self.editor.set_cursor(Position::new(after, 0));
    }

    /// Writes the canvas drawing into the buffer.
    fn canvas_write_back(&mut self) {
        let Mode::Canvas(c) = &self.mode else {
            return;
        };
        let (fence, drawing) = (c.fence, c.drawing.clone());
        if !canvas_mode::write(&mut self.editor, fence, &drawing) {
            self.notify("The draw block is gone");
            self.mode = Mode::Edit;
        }
    }

    /// Re-reads the drawing from the buffer (after undo/redo or a
    /// cancelled move). Leaves canvas mode if the block disappeared.
    fn canvas_reload(&mut self) {
        let Mode::Canvas(c) = &mut self.mode else {
            return;
        };
        if let Some(drawing) = canvas_mode::read(&self.editor, c.fence) {
            c.drawing = drawing;
            c.generation = c.generation.wrapping_add(1);
        } else {
            self.mode = Mode::Edit;
            self.notify("The draw block is gone");
        }
    }

    fn handle_canvas_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let max_x = self.body_width.saturating_sub(2) as i32;
        let Mode::Canvas(c) = &mut self.mode else {
            return;
        };

        // Typing a label: the status bar has the keyboard.
        if let Tool::Text { input, .. } = &mut c.tool {
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
            return;
        }

        let step = if shift { 5 } else { 1 };
        match (key.code, ctrl) {
            (KeyCode::Char('q'), true) => self.request_quit(),
            (KeyCode::Char('s'), true) => self.save(),
            (KeyCode::Char('z'), true) => {
                if !self.editor.undo() {
                    self.notify("Nothing to undo");
                }
                self.canvas_reload();
            }
            (KeyCode::Char('y'), true) => {
                if !self.editor.redo() {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn app(text: &str) -> App {
        App::new(Editor::from_text(text), Theme::default())
    }

    fn key(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn ctrl(app: &mut App, ch: char) {
        app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL));
    }

    fn type_str(app: &mut App, s: &str) {
        for ch in s.chars() {
            key(app, KeyCode::Char(ch));
        }
    }

    #[test]
    fn ctrl_d_creates_a_block_and_draws_a_labelled_rect() {
        let mut a = app("# Notes\n");
        a.editor.set_cursor(Position::new(1, 0));
        ctrl(&mut a, 'd');
        assert!(matches!(a.mode, Mode::Canvas(_)));
        assert!(a.editor.text().contains("```draw\n```"));

        // Rectangle from (2,1) to (9,3), then label it.
        key(&mut a, KeyCode::Char('r'));
        for _ in 0..7 {
            key(&mut a, KeyCode::Right);
        }
        key(&mut a, KeyCode::Down);
        key(&mut a, KeyCode::Down);
        key(&mut a, KeyCode::Enter);
        assert!(
            a.editor.text().contains("rect 2,1 8x3\n"),
            "{}",
            a.editor.text()
        );

        key(&mut a, KeyCode::Char('t'));
        type_str(&mut a, "Parser");
        key(&mut a, KeyCode::Enter);
        assert!(a.editor.text().contains("rect 2,1 8x3 \"Parser\"\n"));

        // Fill, colour, then an arrow out of the box.
        key(&mut a, KeyCode::Char('f'));
        key(&mut a, KeyCode::Char('c'));
        assert!(a.editor.text().contains("\"Parser\" fill color=red\n"));
        key(&mut a, KeyCode::Char('a'));
        for _ in 0..12 {
            key(&mut a, KeyCode::Right);
        }
        key(&mut a, KeyCode::Enter);
        assert!(
            a.editor.text().contains("line 9,3 -> 21,3\n"),
            "{}",
            a.editor.text()
        );

        // Esc leaves canvas mode with the cursor after the block.
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.mode, Mode::Edit);
        let after = a.editor.cursor().line;
        assert!(a.editor.line(after - 1).starts_with("```"));

        // Every canvas action was one undo step.
        ctrl(&mut a, 'z');
        assert!(!a.editor.text().contains("line 9,3"));
        ctrl(&mut a, 'z');
        assert!(!a.editor.text().contains("color=red"));
    }

    #[test]
    fn ctrl_d_on_an_existing_block_edits_it() {
        let mut a = app("```draw\nrect 0,0 4x2\n```\n\n");
        a.editor.set_cursor(Position::new(1, 3));
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
        assert_eq!(a.editor.text(), "```draw\nrect 2,1 4x2\n```\n\n");
        key(&mut a, KeyCode::Char('x'));
        assert_eq!(a.editor.text(), "```draw\n```\n\n");
        ctrl(&mut a, 'z');
        assert_eq!(a.editor.text(), "```draw\nrect 2,1 4x2\n```\n\n");
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
        assert_eq!(a.editor.text(), "```draw\nrect 0,0 4x2\n```\n");
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
}
