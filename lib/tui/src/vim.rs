//! Vim keys, for hands that already know them.
//!
//! Switched on with `[keys] vim = true` in the config file or `--vim`.
//! Normal mode has the usual motions (`h j k l w b e 0 ^ $ gg G`, with
//! counts), the operators `d`, `y` and `c` over them (`dd`, `dw`, `d$`,
//! `cw`, `yy`, ...), `x X r J p P u Ctrl+R > <`, the ways into insert
//! mode (`i a I A o O`), `/ n N` for search, `gt gT` for tabs, `gf` to
//! follow a link, `ZZ`, and a `:` command line with `w q q! wq x e bn bp
//! bd tabnew files graph draw theme` and a line number. Insert mode is the
//! ordinary editor; Esc leaves it. There is no visual mode yet, since
//! notopod has no selection yet. The Ctrl chords in the status bar keep
//! working in both modes, so nothing is lost by turning this on.

use editor::Position;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Mode};

/// Which of the two modes the keyboard is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VimMode {
    /// Keys are commands.
    Normal,
    /// Keys are text.
    Insert,
}

/// A key that is waiting for the key after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    /// `d`, `y`, `c`, `>`, `<`: waiting for a motion.
    Operator(char),
    /// `g`: `gg`, `gt`, `gT`, `gf`.
    G,
    /// `Z`: `ZZ`, `ZQ`.
    Z,
    /// `r`: the replacement character.
    Replace,
}

/// The one register: what the last delete or yank took.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Register {
    text: String,
    /// Whole lines, pasted on their own line.
    linewise: bool,
}

/// State of the vim keys.
#[derive(Debug, Default)]
pub(crate) struct Vim {
    mode: Option<VimMode>,
    count: Option<usize>,
    pending: Option<Pending>,
    register: Register,
    /// The `:` command line, while it is being typed.
    pub(crate) command: Option<String>,
}

impl Vim {
    pub(crate) fn new() -> Self {
        Self {
            mode: Some(VimMode::Normal),
            ..Self::default()
        }
    }

    pub(crate) fn mode(&self) -> VimMode {
        self.mode.unwrap_or(VimMode::Normal)
    }

    /// What the status bar shows about the vim state.
    pub(crate) fn status(&self) -> String {
        if let Some(cmd) = &self.command {
            return format!(":{cmd}");
        }
        let mut out = match self.mode() {
            VimMode::Normal => "NORMAL".to_owned(),
            VimMode::Insert => "INSERT".to_owned(),
        };
        if let Some(n) = self.count {
            out.push(' ');
            out.push_str(&n.to_string());
        }
        if let Some(p) = self.pending {
            out.push_str(match p {
                Pending::Operator(op) => match op {
                    'd' => " d",
                    'y' => " y",
                    'c' => " c",
                    '>' => " >",
                    _ => " <",
                },
                Pending::G => " g",
                Pending::Z => " Z",
                Pending::Replace => " r",
            });
        }
        out
    }

    fn take_count(&mut self) -> usize {
        self.count.take().unwrap_or(1)
    }

    fn reset(&mut self) {
        self.count = None;
        self.pending = None;
    }
}

/// Where a motion goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordForward,
    WordEnd,
    WordBack,
    LineStart,
    FirstNonBlank,
    LineEnd,
    DocStart,
    DocEnd,
    /// `G` with a count: that line.
    Line(usize),
}

impl Motion {
    fn from_key(code: KeyCode, count: Option<usize>) -> Option<Self> {
        Some(match code {
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => Self::Left,
            KeyCode::Char('l' | ' ') | KeyCode::Right => Self::Right,
            KeyCode::Char('k') | KeyCode::Up => Self::Up,
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Enter => Self::Down,
            KeyCode::Char('w') => Self::WordForward,
            KeyCode::Char('e') => Self::WordEnd,
            KeyCode::Char('b') => Self::WordBack,
            KeyCode::Char('0') | KeyCode::Home => Self::LineStart,
            KeyCode::Char('^') => Self::FirstNonBlank,
            KeyCode::Char('$') | KeyCode::End => Self::LineEnd,
            KeyCode::Char('G') => count.map_or(Self::DocEnd, |n| Self::Line(n.saturating_sub(1))),
            _ => return None,
        })
    }

    /// Whether an operator over this motion works on whole lines.
    fn linewise(self) -> bool {
        matches!(
            self,
            Self::Up | Self::Down | Self::DocStart | Self::DocEnd | Self::Line(_)
        )
    }

    /// Whether the character the motion lands on is included in an
    /// operator's range.
    fn inclusive(self) -> bool {
        matches!(self, Self::WordEnd)
    }
}

impl App {
    /// Handles a key in the note when vim keys are on. Returns `false`
    /// when the key should go to the ordinary editor instead.
    pub(crate) fn handle_vim_key(&mut self, key: KeyEvent) -> bool {
        let Some(vim) = &self.vim else {
            return false;
        };
        let (in_command, mode, pending) = (vim.command.is_some(), vim.mode(), vim.pending);
        if in_command {
            self.handle_command_key(key);
            return true;
        }
        if mode == VimMode::Insert {
            if key.code == KeyCode::Esc {
                let vim = self.vim_mut();
                vim.mode = Some(VimMode::Normal);
                vim.reset();
                if self.editor().cursor().col > 0 {
                    self.editor_mut().move_left();
                }
                return true;
            }
            return false;
        }
        // Normal mode. Ctrl chords are the app's, except the vim ones.
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Char('r') => {
                    self.redo();
                    true
                }
                _ => false,
            };
        }
        if let Some(pending) = pending {
            self.vim_mut().pending = None;
            self.handle_pending(pending, key);
            return true;
        }
        self.handle_normal_key(key);
        true
    }

    fn vim_mut(&mut self) -> &mut Vim {
        self.vim.as_mut().expect("vim keys are on")
    }

    fn enter_insert(&mut self) {
        let vim = self.vim_mut();
        vim.mode = Some(VimMode::Insert);
        vim.reset();
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        let count = self.vim_mut().count;
        match key.code {
            KeyCode::Char(d @ ('1'..='9' | '0')) if count.is_some() || d != '0' => {
                let digit = d.to_digit(10).unwrap_or(0) as usize;
                let vim = self.vim_mut();
                vim.count = Some(
                    vim.count
                        .unwrap_or(0)
                        .saturating_mul(10)
                        .saturating_add(digit)
                        .min(100_000),
                );
            }
            KeyCode::Esc => {
                self.vim_mut().reset();
                self.notice = None;
            }
            KeyCode::Char(op @ ('d' | 'y' | 'c' | '>' | '<')) => {
                self.vim_mut().pending = Some(Pending::Operator(op));
            }
            KeyCode::Char('g') => self.vim_mut().pending = Some(Pending::G),
            KeyCode::Char('Z') => self.vim_mut().pending = Some(Pending::Z),
            KeyCode::Char('r') => self.vim_mut().pending = Some(Pending::Replace),
            KeyCode::Char(':') => self.vim_mut().command = Some(String::new()),
            KeyCode::Char('/') => {
                self.vim_mut().reset();
                self.start_find();
            }
            KeyCode::Char('n') => {
                self.vim_mut().reset();
                self.find_next();
            }
            KeyCode::Char('N') => {
                self.vim_mut().reset();
                self.find_prev();
            }
            KeyCode::Char(how @ ('i' | 'a' | 'I' | 'A' | 'o' | 'O')) => self.insert_via(how),
            KeyCode::Char('x') | KeyCode::Delete => self.delete_under(true),
            KeyCode::Char('X') => self.delete_under(false),
            KeyCode::Char('D') => self.operate('d', Motion::LineEnd),
            KeyCode::Char('C') => self.operate('c', Motion::LineEnd),
            KeyCode::Char('Y') => self.operate_lines('y'),
            KeyCode::Char('S') => self.operate_lines('c'),
            KeyCode::Char('p') => self.paste(true),
            KeyCode::Char('P') => self.paste(false),
            KeyCode::Char('u') => {
                let n = self.vim_mut().take_count();
                for _ in 0..n {
                    self.undo();
                }
            }
            KeyCode::Char('J') => {
                let n = self.vim_mut().take_count().max(2) - 1;
                for _ in 0..n {
                    self.join_lines();
                }
            }
            KeyCode::Char('v' | 'V') => {
                self.vim_mut().reset();
                self.notify("Visual mode is not there yet: notopod has no selection yet");
            }
            KeyCode::PageUp => {
                self.vim_mut().reset();
                let page = self.body_height.saturating_sub(1).max(1);
                self.move_visual(-(page as isize));
            }
            KeyCode::PageDown => {
                self.vim_mut().reset();
                let page = self.body_height.saturating_sub(1).max(1);
                self.move_visual(page as isize);
            }
            code => {
                if let Some(motion) = Motion::from_key(code, count) {
                    let n = self.vim_mut().take_count();
                    self.move_by_motion(motion, n);
                } else {
                    self.vim_mut().reset();
                }
            }
        }
    }

    /// `i a I A o O`: where insert mode starts.
    fn insert_via(&mut self, how: char) {
        match how {
            'a' => {
                let cursor = self.editor().cursor();
                if cursor.col < self.editor().line_len(cursor.line) {
                    self.editor_mut().move_right();
                }
            }
            'I' => {
                let to = self.target(Motion::FirstNonBlank, 1);
                self.editor_mut().set_cursor(to);
            }
            'A' => self.editor_mut().line_end(),
            'o' => {
                self.editor_mut().line_end();
                self.editor_mut().insert_newline_plain();
            }
            'O' => {
                self.editor_mut().line_start();
                self.editor_mut().insert_newline_plain();
                self.editor_mut().move_up();
            }
            _ => {}
        }
        self.enter_insert();
    }

    /// `x` (under and after the cursor) or `X` (before it), `count`
    /// characters, into the register.
    fn delete_under(&mut self, forward: bool) {
        let n = self.vim_mut().take_count();
        let cursor = self.editor().cursor();
        let at = self.editor().char_idx(cursor);
        let range = if forward {
            let end = (cursor.col + n).min(self.editor().line_len(cursor.line));
            at..at + (end - cursor.col)
        } else {
            at - n.min(cursor.col)..at
        };
        if range.is_empty() {
            return;
        }
        let removed = self.editor_mut().delete_range(range);
        self.vim_mut().register = Register {
            text: removed,
            linewise: false,
        };
    }

    fn handle_pending(&mut self, pending: Pending, key: KeyEvent) {
        let count = self.vim_mut().count;
        match (pending, key.code) {
            (_, KeyCode::Esc) => self.vim_mut().reset(),
            (Pending::Operator(op), KeyCode::Char(c)) if c == op => self.operate_lines(op),
            (Pending::Operator('c'), KeyCode::Char('w')) => self.operate('c', Motion::WordEnd),
            (Pending::Operator(op), code) => match Motion::from_key(code, count) {
                Some(motion) => self.operate(op, motion),
                None => self.vim_mut().reset(),
            },
            (Pending::G, KeyCode::Char('g')) => {
                let n = self.vim_mut().take_count();
                let motion = if count.is_some() {
                    Motion::Line(n.saturating_sub(1))
                } else {
                    Motion::DocStart
                };
                self.move_by_motion(motion, 1);
            }
            (Pending::G, KeyCode::Char('t')) => {
                let n = self.vim_mut().take_count();
                if count.is_some() {
                    self.switch_tab(n.saturating_sub(1));
                } else {
                    let next = (self.active() + 1) % self.tabs().len();
                    self.switch_tab(next);
                }
            }
            (Pending::G, KeyCode::Char('T')) => {
                self.vim_mut().reset();
                let prev = (self.active() + self.tabs().len() - 1) % self.tabs().len();
                self.switch_tab(prev);
            }
            (Pending::G, KeyCode::Char('f')) => {
                self.vim_mut().reset();
                self.follow_link();
            }
            (Pending::Z, KeyCode::Char('Z')) => {
                self.vim_mut().reset();
                if self.editor().is_dirty() && self.editor().path().is_some() {
                    self.save();
                }
                self.request_quit();
            }
            (Pending::Z, KeyCode::Char('Q')) => {
                self.vim_mut().reset();
                self.force_quit();
            }
            (Pending::Replace, KeyCode::Char(c)) => {
                let n = self.vim_mut().take_count();
                let cursor = self.editor().cursor();
                let len = self.editor().line_len(cursor.line);
                if cursor.col + n <= len {
                    let at = self.editor().char_idx(cursor);
                    self.editor_mut().delete_range(at..at + n);
                    self.editor_mut().insert_str(&c.to_string().repeat(n));
                    self.editor_mut()
                        .set_cursor(Position::new(cursor.line, cursor.col + n - 1));
                }
            }
            _ => self.vim_mut().reset(),
        }
    }

    /// Where `motion`, `count` times, lands from the cursor.
    fn target(&self, motion: Motion, count: usize) -> Position {
        let editor = self.editor();
        let cursor = editor.cursor();
        let last_line = last_real_line(editor);
        match motion {
            Motion::Left => Position::new(cursor.line, cursor.col.saturating_sub(count)),
            Motion::Right => Position::new(
                cursor.line,
                (cursor.col + count).min(editor.line_len(cursor.line)),
            ),
            Motion::Up => Position::new(cursor.line.saturating_sub(count), cursor.col),
            Motion::Down => Position::new((cursor.line + count).min(last_line), cursor.col),
            Motion::LineStart => Position::new(cursor.line, 0),
            Motion::FirstNonBlank => {
                let text = editor.line(cursor.line);
                let col = text.chars().take_while(|c| c.is_whitespace()).count();
                Position::new(cursor.line, col.min(editor.line_len(cursor.line)))
            }
            Motion::LineEnd => Position::new(cursor.line, editor.line_len(cursor.line)),
            Motion::DocStart => Position::new(0, 0),
            Motion::DocEnd => Position::new(last_line, 0),
            Motion::Line(n) => Position::new(n.min(last_line), 0),
            Motion::WordForward | Motion::WordBack | Motion::WordEnd => {
                let mut pos = cursor;
                for _ in 0..count {
                    pos = word_step(editor, pos, motion);
                }
                pos
            }
        }
    }

    fn move_by_motion(&mut self, motion: Motion, count: usize) {
        match motion {
            // Up and down go through rendered blocks like the arrow keys.
            Motion::Up => self.move_visual(-(count as isize)),
            Motion::Down => self.move_visual(count as isize),
            _ => {
                let to = self.target(motion, count);
                self.editor_mut().set_cursor(to);
            }
        }
    }

    /// `d`, `y`, `c`, `>` or `<` over `motion`.
    fn operate(&mut self, op: char, motion: Motion) {
        let count = self.vim_mut().take_count();
        if motion.linewise() {
            let cursor = self.editor().cursor().line;
            let to = self.target(motion, count).line;
            self.operate_on_lines(op, cursor.min(to), cursor.max(to));
            return;
        }
        let cursor = self.editor().cursor();
        let mut to = self.target(motion, count);
        // `dw` at the end of a line stops there rather than eating the break.
        if to.line != cursor.line {
            to = Position::new(cursor.line, self.editor().line_len(cursor.line));
        }
        let (mut start, mut end) = (self.editor().char_idx(cursor), self.editor().char_idx(to));
        if start > end {
            std::mem::swap(&mut start, &mut end);
        }
        if motion.inclusive() {
            end = (end + 1).min(self.editor().len_chars());
        }
        match op {
            '>' | '<' => self.operate_on_lines(op, cursor.line, cursor.line),
            'y' => {
                let text = self.editor().slice(start..end);
                self.vim_mut().register = Register {
                    text,
                    linewise: false,
                };
                let to = self.editor().position_at(start);
                self.editor_mut().set_cursor(to);
            }
            _ => {
                if end > start {
                    let removed = self.editor_mut().delete_range(start..end);
                    self.vim_mut().register = Register {
                        text: removed,
                        linewise: false,
                    };
                }
                if op == 'c' {
                    self.enter_insert();
                }
            }
        }
    }

    /// `dd`, `yy`, `cc`, `>>`, `<<`: the current line and `count - 1` more.
    fn operate_lines(&mut self, op: char) {
        let count = self.vim_mut().take_count();
        let first = self.editor().cursor().line;
        let last = (first + count - 1).min(self.editor().len_lines() - 1);
        self.operate_on_lines(op, first, last);
    }

    fn operate_on_lines(&mut self, op: char, first: usize, last: usize) {
        match op {
            '>' | '<' => {
                let keep = self.editor().cursor();
                for line in first..=last {
                    self.editor_mut().set_cursor(Position::new(line, 0));
                    if op == '>' {
                        self.editor_mut().insert_str("  ");
                    } else {
                        self.editor_mut().dedent_line();
                    }
                }
                self.editor_mut().set_cursor(Position::new(first, keep.col));
            }
            'y' => {
                let range = self.lines_range(first, last);
                let mut text = self.editor().slice(range);
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                self.vim_mut().register = Register {
                    text,
                    linewise: true,
                };
            }
            'c' => {
                // Change keeps the lines' place: empty them into one, then type.
                let start = self.editor().char_idx(Position::new(first, 0));
                let end = self
                    .editor()
                    .char_idx(Position::new(last, self.editor().line_len(last)));
                let removed = self.editor_mut().delete_range(start..end);
                self.vim_mut().register = Register {
                    text: format!("{removed}\n"),
                    linewise: true,
                };
                self.enter_insert();
            }
            _ => {
                let range = self.lines_range(first, last);
                let mut removed = self.editor_mut().delete_range(range);
                if !removed.ends_with('\n') {
                    removed.push('\n');
                }
                self.vim_mut().register = Register {
                    text: removed,
                    linewise: true,
                };
                let line = first.min(self.editor().len_lines() - 1);
                let to = self.target_first_non_blank(line);
                self.editor_mut().set_cursor(to);
            }
        }
    }

    /// Character range of lines `first..=last` including their breaks, so
    /// removing it removes the lines. When it reaches the end of the text
    /// the break before it goes instead, so no empty last line is left.
    fn lines_range(&self, first: usize, last: usize) -> std::ops::Range<usize> {
        let editor = self.editor();
        let start = editor.char_idx(Position::new(first, 0));
        let end = if last + 1 < editor.len_lines() {
            editor.char_idx(Position::new(last + 1, 0))
        } else {
            editor.len_chars()
        };
        if end == editor.len_chars() && start > 0 && last + 1 == editor.len_lines() {
            start - 1..end
        } else {
            start..end
        }
    }

    fn target_first_non_blank(&self, line: usize) -> Position {
        let text = self.editor().line(line);
        let col = text.chars().take_while(|c| c.is_whitespace()).count();
        Position::new(line, col.min(self.editor().line_len(line)))
    }

    /// `p` (after) or `P` (before): the register, `count` times.
    fn paste(&mut self, after: bool) {
        let count = self.vim_mut().take_count();
        let register = self.vim_mut().register.clone();
        if register.text.is_empty() {
            self.notify("Nothing to paste");
            return;
        }
        let text = register.text.repeat(count);
        let cursor = self.editor().cursor();
        if register.linewise {
            let len_lines = self.editor().len_lines();
            if after {
                if cursor.line + 1 < len_lines {
                    self.editor_mut()
                        .set_cursor(Position::new(cursor.line + 1, 0));
                    self.editor_mut().insert_str(&text);
                } else {
                    // Below the last line: the break has to come first.
                    self.editor_mut().line_end();
                    self.editor_mut()
                        .insert_str(&format!("\n{}", text.trim_end_matches('\n')));
                }
                let to = self.target_first_non_blank(cursor.line + 1);
                self.editor_mut().set_cursor(to);
            } else {
                self.editor_mut().set_cursor(Position::new(cursor.line, 0));
                self.editor_mut().insert_str(&text);
                let to = self.target_first_non_blank(cursor.line);
                self.editor_mut().set_cursor(to);
            }
        } else {
            if after && cursor.col < self.editor().line_len(cursor.line) {
                self.editor_mut().move_right();
            }
            self.editor_mut().insert_str(&text);
            let cursor = self.editor().cursor();
            if cursor.col > 0 {
                self.editor_mut().move_left();
            }
        }
    }

    /// `J`: joins the next line onto this one with one space between,
    /// dropping the next line's leading blanks.
    fn join_lines(&mut self) {
        let cursor = self.editor().cursor();
        if cursor.line + 1 >= self.editor().len_lines() {
            return;
        }
        let next = self.editor().line(cursor.line + 1);
        let blanks = next.chars().take_while(|c| c.is_whitespace()).count();
        let here = self.editor().line(cursor.line);
        let start = self
            .editor()
            .char_idx(Position::new(cursor.line, here.chars().count()));
        let end = self
            .editor()
            .char_idx(Position::new(cursor.line + 1, blanks));
        self.editor_mut().delete_range(start..end);
        let joined_at = self.editor().cursor();
        let next_rest = next.chars().nth(blanks);
        if !here.is_empty() && next_rest.is_some_and(|c| c != ')' && c != ',') {
            self.editor_mut().insert_str(" ");
        }
        self.editor_mut().set_cursor(joined_at);
    }

    /// `N`: the match before the cursor, wrapping around.
    pub(crate) fn find_prev(&mut self) {
        let query = self.last_query().to_owned();
        if query.is_empty() {
            self.notify("Nothing to search for (/ to start)");
            return;
        }
        let cursor_idx = self.editor().cursor_char_idx();
        let mut from = 0;
        let mut before: Option<Position> = None;
        let mut last: Option<Position> = None;
        while let Some(pos) = self.editor().find(&query, from) {
            let idx = self.editor().char_idx(pos);
            if idx < from {
                break; // wrapped around
            }
            if idx < cursor_idx {
                before = Some(pos);
            }
            last = Some(pos);
            from = idx + 1;
        }
        match before.or(last) {
            Some(pos) => self.editor_mut().set_cursor(pos),
            None => self.notify(format!("No match for \"{query}\"")),
        }
    }

    // ----- the : command line -----

    fn handle_command_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match (key.code, ctrl) {
            (KeyCode::Esc, _) | (KeyCode::Char('c'), true) => self.vim_mut().command = None,
            (KeyCode::Enter, _) => {
                let Some(line) = self.vim_mut().command.take() else {
                    return;
                };
                self.run_command(line.trim());
            }
            (KeyCode::Backspace, _) => {
                let vim = self.vim_mut();
                if let Some(cmd) = &mut vim.command {
                    if cmd.pop().is_none() {
                        vim.command = None;
                    }
                }
            }
            (KeyCode::Char(c), false) => {
                if let Some(cmd) = &mut self.vim_mut().command {
                    cmd.push(c);
                }
            }
            _ => {}
        }
    }

    /// Runs one `:` command.
    pub(crate) fn run_command(&mut self, line: &str) {
        if line.is_empty() {
            return;
        }
        if let Ok(n) = line.parse::<usize>() {
            self.move_by_motion(Motion::Line(n.saturating_sub(1)), 1);
            return;
        }
        let (name, arg) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(n, a)| (n, a.trim()));
        match name {
            "w" | "write" => {
                if arg.is_empty() {
                    self.save();
                } else {
                    match self.editor_mut().save_as(arg) {
                        Ok(()) => self.notify(format!("Saved {arg}")),
                        Err(e) => self.notify(format!("Could not save {arg}: {e}")),
                    }
                }
            }
            "q" | "quit" => self.request_quit(),
            "q!" | "quit!" => self.force_quit(),
            "wq" | "x" | "xit" => {
                if !arg.is_empty() {
                    let _ = self.editor_mut().save_as(arg);
                } else if self.editor().path().is_some() {
                    self.save();
                } else {
                    self.notify("No file name: use :w NAME first");
                    return;
                }
                if !self.editor().is_dirty() {
                    self.request_quit();
                }
            }
            "e" | "edit" | "o" | "open" => {
                if arg.is_empty() {
                    self.mode = Mode::Open {
                        input: String::new(),
                    };
                } else {
                    let path = self.resolve_note(arg);
                    self.open_path(&path);
                }
            }
            "bn" | "bnext" | "tabn" | "tabnext" => {
                let next = (self.active() + 1) % self.tabs().len();
                self.switch_tab(next);
            }
            "bp" | "bprev" | "bprevious" | "tabp" | "tabprev" | "tabprevious" => {
                let prev = (self.active() + self.tabs().len() - 1) % self.tabs().len();
                self.switch_tab(prev);
            }
            "b" | "buffer" | "tabnew" | "enew" | "new" => {
                if let Ok(n) = arg.parse::<usize>() {
                    self.switch_tab(n.saturating_sub(1));
                } else {
                    self.new_tab();
                }
            }
            "bd" | "bdelete" | "tabc" | "tabclose" | "close" => self.request_close_tab(),
            "files" | "Ex" | "Explore" | "tree" => self.toggle_files(),
            "graph" => self.toggle_graph(),
            "draw" => self.enter_canvas(),
            "noh" | "nohlsearch" => self.notice = None,
            "theme" => match theme::Theme::builtin(arg) {
                Some(theme) => {
                    self.theme = theme;
                    self.notify(format!(
                        "Theme {arg} (for this run; `notopod themes` to keep it)"
                    ));
                }
                None => self.notify(format!(
                    "No built-in theme {arg:?}. Built in: {}",
                    theme::BUILTIN_NAMES.join(", ")
                )),
            },
            "preview" => {
                self.preview = !self.preview;
            }
            _ => self.notify(format!("Not an editor command: {line}")),
        }
    }
}

/// The last line that is really there: a text ending in a line break has
/// an empty line after it that the buffer counts, and vim does not.
fn last_real_line(ed: &editor::Editor) -> usize {
    let n = ed.len_lines();
    if n > 1 && ed.line_len(n - 1) == 0 {
        n - 2
    } else {
        n - 1
    }
}

/// One `w`, `b` or `e` step from `pos`, vim style: words are runs of
/// letters, digits and `_`, or runs of other non-blank characters; `w`
/// crosses line ends, `b` and `e` too.
fn word_step(ed: &editor::Editor, pos: Position, motion: Motion) -> Position {
    let text = ed.text();
    let chars: Vec<char> = text.chars().collect();
    let mut i = ed.char_idx(pos).min(chars.len());
    let kind = |c: char| -> u8 {
        if c.is_whitespace() {
            0
        } else if c.is_alphanumeric() || c == '_' {
            1
        } else {
            2
        }
    };
    match motion {
        Motion::WordForward => {
            if i < chars.len() {
                let k = kind(chars[i]);
                if k != 0 {
                    while i < chars.len() && kind(chars[i]) == k {
                        i += 1;
                    }
                }
                // An empty line counts as a word in vim; stop on it.
                while i < chars.len() && kind(chars[i]) == 0 {
                    if chars[i] == '\n' && i + 1 < chars.len() && chars[i + 1] == '\n' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
        }
        Motion::WordEnd => {
            i += 1;
            while i < chars.len() && kind(chars[i]) == 0 {
                i += 1;
            }
            if i < chars.len() {
                let k = kind(chars[i]);
                while i + 1 < chars.len() && kind(chars[i + 1]) == k {
                    i += 1;
                }
            } else {
                i = chars.len().saturating_sub(1);
            }
        }
        Motion::WordBack => {
            while i > 0 && kind(chars[i - 1]) == 0 {
                i -= 1;
            }
            if i > 0 {
                let k = kind(chars[i - 1]);
                while i > 0 && kind(chars[i - 1]) == k {
                    i -= 1;
                }
            }
        }
        _ => {}
    }
    ed.position_at(i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{app, key, type_str};
    use editor::Editor;
    use theme::Theme;

    fn vim(text: &str) -> App {
        App::new(Editor::from_text(text), Theme::default()).with_vim(true)
    }

    fn cursor(a: &App) -> (usize, usize) {
        let c = a.editor().cursor();
        (c.line, c.col)
    }

    #[test]
    fn motions_with_counts() {
        let mut a = vim("one two three\n  four five\nsix\n");
        type_str(&mut a, "ll");
        assert_eq!(cursor(&a), (0, 2));
        type_str(&mut a, "w");
        assert_eq!(cursor(&a), (0, 4));
        type_str(&mut a, "2w");
        assert_eq!(cursor(&a), (1, 2), "w crosses the line end");
        type_str(&mut a, "e");
        assert_eq!(cursor(&a), (1, 5));
        type_str(&mut a, "b");
        assert_eq!(cursor(&a), (1, 2));
        type_str(&mut a, "$");
        assert_eq!(cursor(&a), (1, 11));
        type_str(&mut a, "0");
        assert_eq!(cursor(&a), (1, 0));
        type_str(&mut a, "^");
        assert_eq!(cursor(&a), (1, 2));
        type_str(&mut a, "G");
        assert_eq!(
            cursor(&a),
            (2, 0),
            "G skips the empty line after the last break"
        );
        type_str(&mut a, "gg");
        assert_eq!(cursor(&a), (0, 0));
        type_str(&mut a, "3G");
        assert_eq!(cursor(&a), (2, 0));
        type_str(&mut a, "k");
        assert_eq!(cursor(&a), (1, 0));
        type_str(&mut a, "10l");
        assert_eq!(cursor(&a), (1, 10));
        assert_eq!(a.vim.as_ref().map(Vim::status), Some("NORMAL".to_owned()));
    }

    #[test]
    fn insert_ways_and_esc() {
        let mut a = vim("ab\n");
        type_str(&mut a, "ixy");
        assert_eq!(a.editor().text(), "xyab\n");
        assert_eq!(a.vim.as_ref().map(Vim::mode), Some(VimMode::Insert));
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.vim.as_ref().map(Vim::mode), Some(VimMode::Normal));
        assert_eq!(cursor(&a), (0, 1), "Esc steps back one like vim");
        type_str(&mut a, "A!");
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.editor().text(), "xyab!\n");
        type_str(&mut a, "onew");
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.editor().text(), "xyab!\nnew\n");
        type_str(&mut a, "Otop");
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.editor().text(), "xyab!\ntop\nnew\n");
        type_str(&mut a, "I>");
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.editor().text(), "xyab!\n>top\nnew\n");
        type_str(&mut a, "a<");
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.editor().text(), "xyab!\n><top\nnew\n");
    }

    #[test]
    fn delete_yank_change_and_paste() {
        let mut a = vim("alpha beta\ngamma\ndelta\n");
        type_str(&mut a, "dw");
        assert_eq!(a.editor().text(), "beta\ngamma\ndelta\n");
        type_str(&mut a, "x");
        assert_eq!(a.editor().text(), "eta\ngamma\ndelta\n");
        type_str(&mut a, "P");
        assert_eq!(
            a.editor().text(),
            "beta\ngamma\ndelta\n",
            "x fills the register"
        );

        type_str(&mut a, "jdd");
        assert_eq!(a.editor().text(), "beta\ndelta\n");
        assert_eq!(cursor(&a), (1, 0));
        type_str(&mut a, "p");
        assert_eq!(
            a.editor().text(),
            "beta\ndelta\ngamma\n",
            "linewise paste goes below"
        );
        type_str(&mut a, "ggyyjP");
        assert_eq!(a.editor().text(), "beta\nbeta\ndelta\ngamma\n");

        type_str(&mut a, "Gdd");
        assert_eq!(
            a.editor().text(),
            "beta\nbeta\ndelta\n",
            "deleting the last line takes its break"
        );
        type_str(&mut a, "gg2dd");
        assert_eq!(a.editor().text(), "delta\n");

        type_str(&mut a, "cwomega");
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.editor().text(), "omega\n");
        type_str(&mut a, "0D");
        assert_eq!(a.editor().text(), "\n");
        type_str(&mut a, "u");
        assert_eq!(a.editor().text(), "omega\n");
        type_str(&mut a, "3x");
        assert_eq!(a.editor().text(), "ga\n");
        type_str(&mut a, "rZ");
        assert_eq!(a.editor().text(), "Za\n");
    }

    #[test]
    fn join_indent_and_line_commands() {
        let mut a = vim("a\n   b\nc\n");
        type_str(&mut a, "J");
        assert_eq!(a.editor().text(), "a b\nc\n");
        type_str(&mut a, ">>");
        assert_eq!(a.editor().text(), "  a b\nc\n");
        type_str(&mut a, "<<");
        assert_eq!(a.editor().text(), "a b\nc\n");
        type_str(&mut a, ":2");
        key(&mut a, KeyCode::Enter);
        assert_eq!(cursor(&a), (1, 0));
        type_str(&mut a, ":nothing");
        key(&mut a, KeyCode::Enter);
        assert!(a
            .notice
            .as_ref()
            .is_some_and(|(m, _)| m.contains("Not an editor command")));
        type_str(&mut a, ":q");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.mode, Mode::ConfirmQuit, "dirty: asks");
        key(&mut a, KeyCode::Esc);
        type_str(&mut a, ":q!");
        key(&mut a, KeyCode::Enter);
        assert!(a.quit);
    }

    #[test]
    fn search_tabs_and_status() {
        let mut a = vim("x one\ntwo one\nthree one\n");
        type_str(&mut a, "/one");
        key(&mut a, KeyCode::Enter);
        key(&mut a, KeyCode::Esc);
        assert_eq!(cursor(&a), (1, 4), "/ finds the first, Enter the next");
        type_str(&mut a, "n");
        assert_eq!(cursor(&a), (2, 6));
        type_str(&mut a, "N");
        assert_eq!(cursor(&a), (1, 4));
        type_str(&mut a, "N");
        assert_eq!(cursor(&a), (0, 2));
        type_str(&mut a, "N");
        assert_eq!(cursor(&a), (2, 6), "wraps");

        type_str(&mut a, ":tabnew");
        key(&mut a, KeyCode::Enter);
        assert_eq!(a.tabs().len(), 2);
        assert_eq!(a.active(), 1);
        type_str(&mut a, "gT");
        assert_eq!(a.active(), 0);
        type_str(&mut a, "gt");
        assert_eq!(a.active(), 1);
        type_str(&mut a, "2d");
        assert_eq!(
            a.vim.as_ref().map(Vim::status),
            Some("NORMAL 2 d".to_owned())
        );
        key(&mut a, KeyCode::Esc);
        type_str(&mut a, ":");
        assert_eq!(a.vim.as_ref().map(Vim::status), Some(":".to_owned()));
        key(&mut a, KeyCode::Backspace);
        assert!(a.vim.as_ref().is_some_and(|v| v.command.is_none()));

        // Ctrl chords are still the app's.
        crate::app::tests::ctrl(&mut a, 't');
        assert_eq!(a.tabs().len(), 3);
    }

    #[test]
    fn the_ordinary_keys_stay_ordinary_without_vim() {
        let mut a = app("");
        type_str(&mut a, "dd");
        assert_eq!(a.editor().text(), "dd");
    }
}
