//! Byte offset to line number lookup.

use crate::Span;

/// Maps byte offsets in a text to zero-based line numbers.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset where each line starts. Always has at least one entry (0).
    starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    /// Builds the index for `text`. Lines are separated by `\n`.
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            text.bytes()
                .enumerate()
                .filter(|&(_, b)| b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Self {
            starts,
            len: text.len(),
        }
    }

    /// Number of lines. A text ending in `\n` has one extra (empty) line.
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// Zero-based line containing `byte`. Offsets past the end map to the last line.
    pub fn line_of(&self, byte: usize) -> usize {
        let byte = byte.min(self.len);
        self.starts
            .partition_point(|&s| s <= byte)
            .saturating_sub(1)
    }

    /// Byte offset where `line` starts.
    pub fn line_start(&self, line: usize) -> usize {
        self.starts.get(line).copied().unwrap_or(self.len)
    }

    /// First and last line (inclusive) touched by `span`.
    pub fn lines_of(&self, span: &Span) -> (usize, usize) {
        let first = self.line_of(span.start);
        let last_byte = span.end.saturating_sub(1).max(span.start);
        let last = self.line_of(last_byte);
        (first, last.max(first))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_bytes_to_lines() {
        let idx = LineIndex::new("ab\ncd\n\nef");
        assert_eq!(idx.line_count(), 4);
        assert_eq!(idx.line_of(0), 0);
        assert_eq!(idx.line_of(2), 0);
        assert_eq!(idx.line_of(3), 1);
        assert_eq!(idx.line_of(6), 2);
        assert_eq!(idx.line_of(7), 3);
        assert_eq!(idx.line_of(100), 3);
        assert_eq!(idx.line_start(1), 3);
    }

    #[test]
    fn span_to_lines_excludes_trailing_newline_line() {
        let idx = LineIndex::new("one\ntwo\n\nthree\n");
        // "one\ntwo\n" -> lines 0..=1, not 2.
        assert_eq!(idx.lines_of(&(0..8)), (0, 1));
        assert_eq!(idx.lines_of(&(9..15)), (3, 3));
        assert_eq!(idx.lines_of(&(4..4)), (1, 1));
    }
}
