//! The file panel: the notes around the one you are editing.
//!
//! A tree of the folder the note lives in, directories first, notes
//! (`.md`, `.markdown`, `.txt`) after; other files and dot-files are not
//! shown. Only expanded directories are read, and only when they are
//! expanded, so a panel over a large tree costs the rows on screen and not
//! the whole disk. Nothing is watched: `r` reads the tree again.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use theme::Theme;
use unicode_width::UnicodeWidthStr;

/// Rows the tree stops growing at, so one `r` on a giant folder cannot
/// take the process with it.
const MAX_ROWS: usize = 4000;

/// Cells of indent per level of nesting.
const INDENT: usize = 2;

/// One line of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    /// Where it is on disk.
    pub(crate) path: PathBuf,
    /// The file or directory name.
    pub(crate) name: String,
    /// Nesting below the root.
    pub(crate) depth: usize,
    /// A directory (shown with `▸` / `▾`), or a note.
    pub(crate) is_dir: bool,
}

/// The panel's state.
#[derive(Debug)]
pub(crate) struct FilePanel {
    root: PathBuf,
    rows: Vec<Row>,
    expanded: BTreeSet<PathBuf>,
    cursor: usize,
    scroll: usize,
}

impl FilePanel {
    /// A panel over `root`, with only the top level read.
    pub(crate) fn new(root: PathBuf) -> Self {
        let mut panel = Self {
            root,
            rows: Vec::new(),
            expanded: BTreeSet::new(),
            cursor: 0,
            scroll: 0,
        };
        panel.refresh();
        panel
    }

    /// The directory at the top of the tree.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// The visible rows, top to bottom.
    #[cfg(test)]
    pub(crate) fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The row under the cursor.
    pub(crate) fn selected(&self) -> Option<&Row> {
        self.rows.get(self.cursor)
    }

    /// Reads the tree again from disk, keeping what was expanded and
    /// staying on the same path where it still exists.
    pub(crate) fn refresh(&mut self) {
        let keep = self.selected().map(|r| r.path.clone());
        let mut rows = Vec::new();
        walk(&self.root, 0, &self.expanded, &mut rows);
        self.rows = rows;
        self.cursor = keep
            .and_then(|p| self.rows.iter().position(|r| r.path == p))
            .unwrap_or(0)
            .min(self.rows.len().saturating_sub(1));
    }

    pub(crate) fn move_by(&mut self, delta: isize) {
        let last = self.rows.len().saturating_sub(1) as isize;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
    }

    pub(crate) fn move_to(&mut self, index: usize) {
        self.cursor = index.min(self.rows.len().saturating_sub(1));
    }

    /// Expands or collapses the directory under the cursor. Returns
    /// `false` when the cursor is on a note.
    pub(crate) fn toggle(&mut self) -> bool {
        let Some(row) = self.selected().filter(|r| r.is_dir) else {
            return false;
        };
        let path = row.path.clone();
        if !self.expanded.remove(&path) {
            self.expanded.insert(path);
        }
        self.refresh();
        true
    }

    /// Right: opens the directory under the cursor, or steps into it when
    /// it is already open. Returns `false` on a note.
    pub(crate) fn expand(&mut self) -> bool {
        let Some(row) = self.selected().filter(|r| r.is_dir) else {
            return false;
        };
        if self.expanded.contains(&row.path) {
            self.move_by(1);
        } else {
            self.expanded.insert(row.path.clone());
            self.refresh();
        }
        true
    }

    /// Left: closes the directory under the cursor, or goes to the
    /// directory that contains the row.
    pub(crate) fn collapse(&mut self) {
        let Some(row) = self.selected() else {
            return;
        };
        let (is_dir, path, depth) = (row.is_dir, row.path.clone(), row.depth);
        if is_dir && self.expanded.remove(&path) {
            self.refresh();
            return;
        }
        if let Some(parent) = self.rows[..self.cursor]
            .iter()
            .rposition(|r| r.depth < depth)
        {
            self.cursor = parent;
        }
    }

    /// Puts the cursor on `path`, opening the directories above it.
    /// Returns `false` when `path` is not under the root.
    pub(crate) fn reveal(&mut self, path: &Path) -> bool {
        let Ok(rel) = path.strip_prefix(&self.root) else {
            return false;
        };
        let mut dir = self.root.clone();
        let mut parts = rel.components().peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                break;
            }
            dir.push(part);
            self.expanded.insert(dir.clone());
        }
        self.refresh();
        if let Some(i) = self.rows.iter().position(|r| r.path == path) {
            self.cursor = i;
            return true;
        }
        false
    }

    /// The panel as styled rows: a heading with the root's name, then the
    /// tree, scrolled so the cursor is visible. `open` marks notes that are
    /// open in a tab; `focused` draws the cursor row in the status bar's
    /// colours, otherwise just in bold.
    pub(crate) fn lines(
        &mut self,
        theme: &Theme,
        width: usize,
        height: usize,
        focused: bool,
        open: &[&Path],
    ) -> Vec<Line<'static>> {
        let mut out = Vec::with_capacity(height);
        if height == 0 {
            return out;
        }
        let root_name = self.root.file_name().map_or_else(
            || self.root.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        out.push(Line::styled(
            fit(&format!(" {root_name}/"), width),
            theme.heading[1],
        ));

        let body = height - 1;
        if body > 0 {
            if self.cursor < self.scroll {
                self.scroll = self.cursor;
            }
            if self.cursor >= self.scroll + body {
                self.scroll = self.cursor + 1 - body;
            }
        }
        if self.rows.is_empty() {
            out.push(Line::styled(
                fit("  (no notes here)", width),
                theme.table_border,
            ));
            return out;
        }
        for (i, row) in self.rows.iter().enumerate().skip(self.scroll).take(body) {
            let marker = if row.is_dir {
                if self.expanded.contains(&row.path) {
                    "▾ "
                } else {
                    "▸ "
                }
            } else if open.iter().any(|p| *p == row.path) {
                "• "
            } else {
                "  "
            };
            let text = format!(
                " {}{marker}{}{}",
                " ".repeat(row.depth * INDENT),
                row.name,
                if row.is_dir { "/" } else { "" }
            );
            let style: Style = if i == self.cursor {
                if focused {
                    theme.status_bar
                } else {
                    theme.strong
                }
            } else if row.is_dir {
                theme.bullet
            } else {
                theme.text
            };
            out.push(Line::from(Span::styled(fit(&text, width), style)));
        }
        out
    }
}

/// Whether a file name is one the panel shows.
fn is_note(name: &str) -> bool {
    Path::new(name).extension().is_some_and(|ext| {
        ext.eq_ignore_ascii_case("md")
            || ext.eq_ignore_ascii_case("markdown")
            || ext.eq_ignore_ascii_case("txt")
    })
}

/// The entries of `dir` the panel shows: directories first, then notes,
/// each group in case-insensitive name order. Unreadable directories are
/// empty.
fn list(dir: &Path) -> Vec<Row> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut rows: Vec<Row> = read
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let path = entry.path();
            let is_dir = entry
                .file_type()
                .is_ok_and(|t| t.is_dir() || (t.is_symlink() && path.is_dir()));
            if !is_dir && !is_note(&name) {
                return None;
            }
            Some(Row {
                path,
                name,
                depth: 0,
                is_dir,
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    rows
}

fn walk(dir: &Path, depth: usize, expanded: &BTreeSet<PathBuf>, out: &mut Vec<Row>) {
    for mut row in list(dir) {
        if out.len() >= MAX_ROWS {
            return;
        }
        row.depth = depth;
        let recurse = row.is_dir && expanded.contains(&row.path);
        let path = row.path.clone();
        out.push(row);
        if recurse {
            walk(&path, depth + 1, expanded, out);
        }
    }
}

/// `text` cut or padded to exactly `width` cells.
fn fit(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push_str(&" ".repeat(width.saturating_sub(out.width())));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (tempfile::TempDir, FilePanel) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("b-dir/inner")).expect("mkdir");
        fs::create_dir_all(root.join("a-dir")).expect("mkdir");
        fs::write(root.join("zeta.md"), "").expect("write");
        fs::write(root.join("Alpha.MD"), "").expect("write");
        fs::write(root.join("notes.txt"), "").expect("write");
        fs::write(root.join("image.png"), "").expect("write");
        fs::write(root.join(".hidden.md"), "").expect("write");
        fs::write(root.join("b-dir/inner/deep.md"), "").expect("write");
        fs::write(root.join("b-dir/top.md"), "").expect("write");
        let panel = FilePanel::new(root.to_path_buf());
        (dir, panel)
    }

    fn names(panel: &FilePanel) -> Vec<String> {
        panel
            .rows()
            .iter()
            .map(|r| format!("{}{}", " ".repeat(r.depth), r.name))
            .collect()
    }

    #[test]
    fn top_level_only_directories_first_notes_after_no_noise() {
        let (_dir, panel) = tree();
        assert_eq!(
            names(&panel),
            vec!["a-dir", "b-dir", "Alpha.MD", "notes.txt", "zeta.md"]
        );
    }

    #[test]
    fn directories_are_read_when_opened_and_forgotten_when_closed() {
        let (_dir, mut panel) = tree();
        panel.move_to(1);
        assert!(panel.toggle());
        assert_eq!(
            names(&panel),
            vec![
                "a-dir",
                "b-dir",
                " inner",
                " top.md",
                "Alpha.MD",
                "notes.txt",
                "zeta.md"
            ]
        );
        // Right on an open directory steps into it; Left from inside goes
        // back to it; Left on it closes it.
        assert!(panel.expand());
        assert_eq!(panel.selected().map(|r| r.name.as_str()), Some("inner"));
        panel.collapse();
        assert_eq!(panel.selected().map(|r| r.name.as_str()), Some("b-dir"));
        panel.collapse();
        assert_eq!(panel.rows().len(), 5);
        // Toggle on a note is a no-op that says so.
        panel.move_to(4);
        assert!(!panel.toggle());
    }

    #[test]
    fn reveal_opens_the_way_to_a_nested_note() {
        let (dir, mut panel) = tree();
        let deep = dir.path().join("b-dir/inner/deep.md");
        assert!(panel.reveal(&deep));
        assert_eq!(panel.selected().map(|r| r.path.clone()), Some(deep));
        assert!(!panel.reveal(Path::new("/nowhere/else.md")));
    }

    #[test]
    fn refresh_sees_new_files_and_keeps_its_place() {
        let (dir, mut panel) = tree();
        panel.move_to(4);
        fs::write(dir.path().join("middle.md"), "").expect("write");
        panel.refresh();
        assert_eq!(panel.selected().map(|r| r.name.as_str()), Some("zeta.md"));
        assert_eq!(panel.rows().len(), 6);
    }

    #[test]
    fn lines_have_a_heading_markers_and_the_cursor_row() {
        let (dir, mut panel) = tree();
        let open = dir.path().join("zeta.md");
        panel.move_to(4);
        let theme = Theme::default();
        let lines = panel.lines(&theme, 20, 10, true, &[open.as_path()]);
        let text: Vec<String> = lines.iter().map(ToString::to_string).collect();
        assert!(text[0].trim_end().ends_with('/'), "{:?}", text[0]);
        assert!(text[1].contains("▸ a-dir/"), "{:?}", text[1]);
        assert!(text[5].contains("• zeta.md"), "{:?}", text[5]);
        assert_eq!(lines[5].spans[0].style, theme.status_bar);
        assert!(text.iter().all(|t| t.width() == 20), "{text:?}");

        // Scrolls to keep the cursor in a short panel.
        let lines = panel.lines(&theme, 20, 3, false, &[]);
        assert_eq!(lines.len(), 3);
        assert!(lines[2].to_string().contains("zeta.md"));
    }
}
