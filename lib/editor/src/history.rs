//! Undo / redo.

use crate::Position;

/// A single reversible change: text removed then text inserted at `at`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edit {
    /// Character index where the change happened.
    pub at: usize,
    /// Text that was inserted (empty for a pure deletion).
    pub inserted: String,
    /// Text that was removed (empty for a pure insertion).
    pub removed: String,
    /// Cursor before the change; restored by undo.
    pub before: Position,
    /// Cursor after the change; restored by redo.
    pub after: Position,
}

impl Edit {
    /// Tries to fold `next` into `self` so one undo reverts both.
    ///
    /// Consecutive typed characters merge until the "kind" of character
    /// changes (word to space, space to word), and consecutive single-
    /// character deletions merge. Line breaks never merge.
    pub fn try_merge(&mut self, next: &Edit) -> bool {
        let one_char = |s: &str| s.chars().count() == 1 && !s.contains('\n');

        // Typing.
        if self.removed.is_empty()
            && next.removed.is_empty()
            && !self.inserted.contains('\n')
            && one_char(&next.inserted)
            && next.at == self.at + self.inserted.chars().count()
            && same_kind(self.inserted.chars().last(), next.inserted.chars().next())
        {
            self.inserted.push_str(&next.inserted);
            self.after = next.after;
            return true;
        }

        if !self.inserted.is_empty() || !next.inserted.is_empty() || !one_char(&next.removed) {
            return false;
        }

        // Backspace: the new deletion ends where the old one started.
        if next.at + 1 == self.at && !self.removed.contains('\n') {
            self.at = next.at;
            self.removed.insert_str(0, &next.removed);
            self.after = next.after;
            return true;
        }

        // Delete forward: the new deletion starts where the old one did.
        if next.at == self.at && !self.removed.contains('\n') {
            self.removed.push_str(&next.removed);
            self.after = next.after;
            return true;
        }

        false
    }
}

fn same_kind(a: Option<char>, b: Option<char>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.is_whitespace() == b.is_whitespace(),
        _ => true,
    }
}

#[derive(Debug, Default)]
pub(crate) struct History {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl History {
    pub fn push(&mut self, edit: Edit) {
        self.redo.clear();
        if let Some(last) = self.undo.last_mut() {
            if last.try_merge(&edit) {
                return;
            }
        }
        self.undo.push(edit);
    }

    pub fn pop_undo(&mut self) -> Option<Edit> {
        let edit = self.undo.pop()?;
        self.redo.push(edit.clone());
        Some(edit)
    }

    pub fn pop_redo(&mut self) -> Option<Edit> {
        let edit = self.redo.pop()?;
        self.undo.push(edit.clone());
        Some(edit)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
