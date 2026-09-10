//! The editor state: text, cursor, history, file.

use std::fs;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

use ropey::Rope;

use crate::history::{Edit, History};
use crate::search;
use crate::smart::{self, Newline};
use crate::Position;

/// A text buffer with a cursor and undo history.
///
/// Line endings are normalised to `\n` when text is loaded or pasted.
#[derive(Debug)]
pub struct Editor {
    rope: Rope,
    cursor: Position,
    /// Column to aim for when moving up/down through shorter lines.
    sticky_col: Option<usize>,
    history: History,
    path: Option<PathBuf>,
    dirty: bool,
    version: u64,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    /// An empty, unnamed buffer.
    pub fn new() -> Self {
        Self::from_text("")
    }

    /// A buffer holding `text`, with no file attached.
    pub fn from_text(text: &str) -> Self {
        Self {
            rope: Rope::from_str(&normalize(text)),
            cursor: Position::default(),
            sticky_col: None,
            history: History::default(),
            path: None,
            dirty: false,
            version: 0,
        }
    }

    /// Loads `path`. A missing file gives an empty buffer that will be
    /// created on first save. Any other I/O error is returned.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e),
        };
        let mut editor = Self::from_text(&text);
        editor.path = Some(path.to_path_buf());
        Ok(editor)
    }

    // ----- state -----

    /// The file this buffer is attached to, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Attaches the buffer to `path` (used by "save as").
    pub fn set_path(&mut self, path: impl Into<PathBuf>) {
        self.path = Some(path.into());
    }

    /// `true` when there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// A counter that increases on every change to the text. Cheap way for
    /// callers to know whether a cached parse is still valid.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether undo has anything to revert.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Whether redo has anything to re-apply.
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// The current cursor position.
    pub fn cursor(&self) -> Position {
        self.cursor
    }

    /// Moves the cursor, clamping it into the buffer.
    pub fn set_cursor(&mut self, pos: Position) {
        let line = pos.line.min(self.len_lines() - 1);
        let col = pos.col.min(self.line_len(line));
        self.cursor = Position { line, col };
        self.sticky_col = None;
    }

    /// The whole text.
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Number of lines. A trailing `\n` counts as starting one more (empty) line.
    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    /// The text of line `idx` without its line break.
    pub fn line(&self, idx: usize) -> String {
        if idx >= self.len_lines() {
            return String::new();
        }
        let line = self.rope.line(idx);
        let end = self.line_len(idx);
        line.slice(..end).to_string()
    }

    /// Number of characters in line `idx`, excluding the line break.
    pub fn line_len(&self, idx: usize) -> usize {
        if idx >= self.len_lines() {
            return 0;
        }
        let line = self.rope.line(idx);
        let mut end = line.len_chars();
        while end > 0 && is_line_break(line.char(end - 1)) {
            end -= 1;
        }
        end
    }

    /// Character index of the cursor in the whole text.
    pub fn cursor_char_idx(&self) -> usize {
        self.pos_to_char(self.cursor)
    }

    /// Byte index of the cursor in the whole text.
    pub fn cursor_byte_idx(&self) -> usize {
        self.rope.char_to_byte(self.cursor_char_idx())
    }

    // ----- movement -----

    /// One character left, wrapping to the end of the previous line.
    pub fn move_left(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.line_len(self.cursor.line);
        }
        self.sticky_col = None;
    }

    /// One character right, wrapping to the start of the next line.
    pub fn move_right(&mut self) {
        if self.cursor.col < self.line_len(self.cursor.line) {
            self.cursor.col += 1;
        } else if self.cursor.line + 1 < self.len_lines() {
            self.cursor.line += 1;
            self.cursor.col = 0;
        }
        self.sticky_col = None;
    }

    /// One line up, remembering the column across short lines.
    pub fn move_up(&mut self) {
        self.move_vertical(-1);
    }

    /// One line down, remembering the column across short lines.
    pub fn move_down(&mut self) {
        self.move_vertical(1);
    }

    /// `n` lines up.
    pub fn page_up(&mut self, n: usize) {
        self.move_vertical(-(n as isize));
    }

    /// `n` lines down.
    pub fn page_down(&mut self, n: usize) {
        self.move_vertical(n as isize);
    }

    fn move_vertical(&mut self, delta: isize) {
        let target_col = self.sticky_col.unwrap_or(self.cursor.col);
        let last = self.len_lines() - 1;
        let line = (self.cursor.line as isize + delta).clamp(0, last as isize) as usize;
        self.cursor = Position {
            line,
            col: target_col.min(self.line_len(line)),
        };
        self.sticky_col = Some(target_col);
    }

    /// Start of the current line.
    pub fn line_start(&mut self) {
        self.cursor.col = 0;
        self.sticky_col = None;
    }

    /// End of the current line.
    pub fn line_end(&mut self) {
        self.cursor.col = self.line_len(self.cursor.line);
        self.sticky_col = None;
    }

    /// Start of the buffer.
    pub fn doc_start(&mut self) {
        self.cursor = Position::default();
        self.sticky_col = None;
    }

    /// End of the buffer.
    pub fn doc_end(&mut self) {
        let line = self.len_lines() - 1;
        self.cursor = Position {
            line,
            col: self.line_len(line),
        };
        self.sticky_col = None;
    }

    /// To the start of the previous word.
    pub fn word_left(&mut self) {
        if self.cursor.col == 0 {
            self.move_left();
            return;
        }
        let chars: Vec<char> = self.line(self.cursor.line).chars().collect();
        let mut i = self.cursor.col.min(chars.len());
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        if i > 0 {
            let kind = char_kind(chars[i - 1]);
            while i > 0 && char_kind(chars[i - 1]) == kind {
                i -= 1;
            }
        }
        self.cursor.col = i;
        self.sticky_col = None;
    }

    /// To the start of the next word.
    pub fn word_right(&mut self) {
        let len = self.line_len(self.cursor.line);
        if self.cursor.col >= len {
            self.move_right();
            return;
        }
        let chars: Vec<char> = self.line(self.cursor.line).chars().collect();
        let mut i = self.cursor.col;
        let kind = char_kind(chars[i]);
        while i < len && char_kind(chars[i]) == kind {
            i += 1;
        }
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        self.cursor.col = i;
        self.sticky_col = None;
    }

    // ----- editing -----

    /// Inserts one character at the cursor.
    pub fn insert_char(&mut self, ch: char) {
        let mut buf = [0u8; 4];
        self.insert_raw(ch.encode_utf8(&mut buf));
    }

    /// Inserts text at the cursor. Line endings are normalised.
    pub fn insert_str(&mut self, text: &str) {
        let text = normalize(text);
        if !text.is_empty() {
            self.insert_raw(&text);
        }
    }

    /// Inserts a line break, continuing list and quote markers.
    /// See [`smart::newline_for`].
    pub fn insert_newline(&mut self) {
        let line = self.line(self.cursor.line);
        match smart::newline_for(&line, self.cursor.col) {
            Newline::Plain => self.insert_raw("\n"),
            Newline::Continue(prefix) => self.insert_raw(&format!("\n{prefix}")),
            Newline::ClearLine => {
                let start = self.pos_to_char(Position::new(self.cursor.line, 0));
                let end = self.cursor_char_idx();
                self.remove_range(start..end, start);
            }
        }
    }

    /// Inserts a plain line break with no list continuation.
    pub fn insert_newline_plain(&mut self) {
        self.insert_raw("\n");
    }

    /// Removes up to two leading spaces (or one tab) from the current line.
    pub fn dedent_line(&mut self) {
        let line = self.line(self.cursor.line);
        let n = if line.starts_with('\t') {
            1
        } else {
            line.chars().take(2).take_while(|c| *c == ' ').count()
        };
        if n == 0 {
            return;
        }
        let start = self.pos_to_char(Position::new(self.cursor.line, 0));
        let cursor_after = self.cursor_char_idx().saturating_sub(n).max(start);
        self.remove_range(start..start + n, cursor_after);
    }

    /// Deletes the character before the cursor (joins lines at column 0).
    pub fn backspace(&mut self) {
        let idx = self.cursor_char_idx();
        if idx == 0 {
            return;
        }
        self.remove_range(idx - 1..idx, idx - 1);
    }

    /// Deletes the character after the cursor (joins lines at line end).
    pub fn delete_forward(&mut self) {
        let idx = self.cursor_char_idx();
        if idx >= self.rope.len_chars() {
            return;
        }
        self.remove_range(idx..idx + 1, idx);
    }

    /// Reverts the last change. Returns `false` if there was none.
    pub fn undo(&mut self) -> bool {
        let Some(edit) = self.history.pop_undo() else {
            return false;
        };
        let inserted_len = edit.inserted.chars().count();
        if inserted_len > 0 {
            self.rope.remove(edit.at..edit.at + inserted_len);
        }
        if !edit.removed.is_empty() {
            self.rope.insert(edit.at, &edit.removed);
        }
        self.cursor = edit.before;
        self.touch();
        true
    }

    /// Re-applies the last undone change. Returns `false` if there was none.
    pub fn redo(&mut self) -> bool {
        let Some(edit) = self.history.pop_redo() else {
            return false;
        };
        let removed_len = edit.removed.chars().count();
        if removed_len > 0 {
            self.rope.remove(edit.at..edit.at + removed_len);
        }
        if !edit.inserted.is_empty() {
            self.rope.insert(edit.at, &edit.inserted);
        }
        self.cursor = edit.after;
        self.touch();
        true
    }

    // ----- search -----

    /// Finds `query` starting at character `from`, wrapping around.
    /// See [`search::find`].
    pub fn find(&self, query: &str, from: usize) -> Option<Position> {
        search::find(&self.text(), query, from).map(|idx| self.char_to_pos(idx))
    }

    // ----- files -----

    /// Writes the buffer to its file. The write goes to a temporary file
    /// next to the target which is then renamed over it, so a crash mid-way
    /// never leaves a half-written note.
    pub fn save(&mut self) -> io::Result<()> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;
        // Follow symlinks so we replace the real file, not the link.
        let target = fs::canonicalize(&path).unwrap_or(path);
        let dir = target
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let name = target
            .file_name()
            .map_or_else(|| "note".to_owned(), |n| n.to_string_lossy().into_owned());
        let tmp = dir.join(format!(".{name}.notopad-tmp"));

        let result = (|| {
            let mut file = fs::File::create(&tmp)?;
            self.rope.write_to(&mut file)?;
            file.sync_all()?;
            if let Ok(meta) = fs::metadata(&target) {
                let _ = fs::set_permissions(&tmp, meta.permissions());
            }
            fs::rename(&tmp, &target)
        })();

        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result?;
        self.dirty = false;
        Ok(())
    }

    /// Attaches the buffer to `path` and saves.
    pub fn save_as(&mut self, path: impl Into<PathBuf>) -> io::Result<()> {
        self.set_path(path);
        self.save()
    }

    // ----- internals -----

    fn insert_raw(&mut self, text: &str) {
        let at = self.cursor_char_idx();
        let before = self.cursor;
        self.rope.insert(at, text);
        let after = self.char_to_pos(at + text.chars().count());
        self.cursor = after;
        self.touch();
        self.history.push(Edit {
            at,
            inserted: text.to_owned(),
            removed: String::new(),
            before,
            after,
        });
    }

    fn remove_range(&mut self, range: Range<usize>, cursor_after: usize) {
        if range.is_empty() {
            return;
        }
        let before = self.cursor;
        let removed = self.rope.slice(range.clone()).to_string();
        self.rope.remove(range.clone());
        let after = self.char_to_pos(cursor_after);
        self.cursor = after;
        self.touch();
        self.history.push(Edit {
            at: range.start,
            inserted: String::new(),
            removed,
            before,
            after,
        });
    }

    fn touch(&mut self) {
        self.dirty = true;
        self.version += 1;
        self.sticky_col = None;
    }

    fn pos_to_char(&self, pos: Position) -> usize {
        let line = pos.line.min(self.len_lines() - 1);
        self.rope.line_to_char(line) + pos.col.min(self.line_len(line))
    }

    fn char_to_pos(&self, idx: usize) -> Position {
        let idx = idx.min(self.rope.len_chars());
        let line = self.rope.char_to_line(idx);
        Position {
            line,
            col: idx - self.rope.line_to_char(line),
        }
    }
}

fn normalize(text: &str) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_owned()
    }
}

fn is_line_break(c: char) -> bool {
    c == '\n'
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum CharKind {
    Space,
    Word,
    Punct,
}

fn char_kind(c: char) -> CharKind {
    if c.is_whitespace() {
        CharKind::Space
    } else if c.is_alphanumeric() || c == '_' {
        CharKind::Word
    } else {
        CharKind::Punct
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_lines() {
        let mut e = Editor::new();
        assert_eq!(e.len_lines(), 1);
        e.insert_str("ab");
        e.insert_newline_plain();
        e.insert_char('c');
        assert_eq!(e.text(), "ab\nc");
        assert_eq!(e.len_lines(), 2);
        assert_eq!(e.line(0), "ab");
        assert_eq!(e.line(1), "c");
        assert_eq!(e.cursor(), Position::new(1, 1));
        assert!(e.is_dirty());
    }

    #[test]
    fn normalises_crlf() {
        let e = Editor::from_text("a\r\nb\rc");
        assert_eq!(e.text(), "a\nb\nc");
    }

    #[test]
    fn movement_wraps_across_lines() {
        let mut e = Editor::from_text("ab\ncde");
        e.move_right();
        e.move_right();
        assert_eq!(e.cursor(), Position::new(0, 2));
        e.move_right();
        assert_eq!(e.cursor(), Position::new(1, 0));
        e.move_left();
        assert_eq!(e.cursor(), Position::new(0, 2));
        e.doc_end();
        assert_eq!(e.cursor(), Position::new(1, 3));
        e.doc_start();
        assert_eq!(e.cursor(), Position::new(0, 0));
    }

    #[test]
    fn vertical_movement_remembers_column() {
        let mut e = Editor::from_text("abcdef\nab\nabcdef");
        e.line_end();
        e.move_down();
        assert_eq!(e.cursor(), Position::new(1, 2));
        e.move_down();
        assert_eq!(e.cursor(), Position::new(2, 6));
        e.move_up();
        e.move_up();
        assert_eq!(e.cursor(), Position::new(0, 6));
    }

    #[test]
    fn word_movement() {
        let mut e = Editor::from_text("foo  bar.baz");
        e.word_right();
        assert_eq!(e.cursor().col, 5);
        e.word_right();
        assert_eq!(e.cursor().col, 8);
        e.word_right();
        assert_eq!(e.cursor().col, 9);
        e.word_right();
        assert_eq!(e.cursor().col, 12);
        e.word_left();
        assert_eq!(e.cursor().col, 9);
        e.word_left();
        assert_eq!(e.cursor().col, 8);
        e.word_left();
        assert_eq!(e.cursor().col, 5);
        e.word_left();
        assert_eq!(e.cursor().col, 0);
    }

    #[test]
    fn backspace_and_delete_join_lines() {
        let mut e = Editor::from_text("ab\ncd");
        e.set_cursor(Position::new(1, 0));
        e.backspace();
        assert_eq!(e.text(), "abcd");
        assert_eq!(e.cursor(), Position::new(0, 2));
        e.delete_forward();
        assert_eq!(e.text(), "abd");
        e.doc_end();
        e.delete_forward();
        assert_eq!(e.text(), "abd");
        e.doc_start();
        e.backspace();
        assert_eq!(e.text(), "abd");
    }

    #[test]
    fn undo_redo_groups_typing_by_word() {
        let mut e = Editor::new();
        for c in "hello world".chars() {
            e.insert_char(c);
        }
        assert!(e.undo());
        assert_eq!(e.text(), "hello ");
        assert!(e.undo());
        assert_eq!(e.text(), "hello");
        assert!(e.undo());
        assert_eq!(e.text(), "");
        assert!(!e.undo());
        assert!(e.redo());
        assert_eq!(e.text(), "hello");
        assert_eq!(e.cursor(), Position::new(0, 5));
        assert!(e.redo());
        assert!(e.redo());
        assert_eq!(e.text(), "hello world");
        assert!(!e.redo());
    }

    #[test]
    fn undo_groups_backspaces() {
        let mut e = Editor::from_text("abc");
        e.doc_end();
        e.backspace();
        e.backspace();
        assert_eq!(e.text(), "a");
        e.undo();
        assert_eq!(e.text(), "abc");
        assert_eq!(e.cursor(), Position::new(0, 3));
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut e = Editor::from_text("a");
        e.doc_end();
        e.insert_char('b');
        e.undo();
        e.insert_char('c');
        assert!(!e.redo());
        assert_eq!(e.text(), "ac");
    }

    #[test]
    fn enter_continues_lists_and_ends_empty_items() {
        let mut e = Editor::from_text("- one");
        e.doc_end();
        e.insert_newline();
        assert_eq!(e.text(), "- one\n- ");
        e.insert_newline();
        assert_eq!(e.text(), "- one\n");
        assert_eq!(e.cursor(), Position::new(1, 0));
    }

    #[test]
    fn dedent_removes_leading_spaces() {
        let mut e = Editor::from_text("    x");
        e.line_end();
        e.dedent_line();
        assert_eq!(e.text(), "  x");
        assert_eq!(e.cursor(), Position::new(0, 3));
        e.dedent_line();
        e.dedent_line();
        assert_eq!(e.text(), "x");
        assert_eq!(e.cursor(), Position::new(0, 1));
    }

    #[test]
    fn find_moves_by_position() {
        let e = Editor::from_text("ab\ncd ab");
        assert_eq!(e.find("ab", 1), Some(Position::new(1, 3)));
        assert_eq!(e.find("ab", 7), Some(Position::new(0, 0)));
        assert_eq!(e.find("zz", 0), None);
    }

    #[test]
    fn set_cursor_clamps() {
        let mut e = Editor::from_text("ab\nc");
        e.set_cursor(Position::new(10, 10));
        assert_eq!(e.cursor(), Position::new(1, 1));
    }

    #[test]
    fn version_bumps_on_change_only() {
        let mut e = Editor::from_text("ab");
        let v = e.version();
        e.move_right();
        assert_eq!(e.version(), v);
        e.insert_char('x');
        assert!(e.version() > v);
    }

    #[test]
    fn save_and_open_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");

        let mut e = Editor::open(&path).unwrap();
        assert_eq!(e.text(), "");
        e.insert_str("# hi\n");
        e.save().unwrap();
        assert!(!e.is_dirty());
        assert_eq!(fs::read_to_string(&path).unwrap(), "# hi\n");

        let e2 = Editor::open(&path).unwrap();
        assert_eq!(e2.text(), "# hi\n");
        assert!(!dir.path().join(".note.md.notopad-tmp").exists());
    }

    #[test]
    fn save_without_path_fails() {
        let mut e = Editor::from_text("x");
        assert!(e.save().is_err());
    }
}
