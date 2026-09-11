//! Cursor coordinates.

/// A position in the buffer. Both fields are zero-based; `col` counts
/// characters (not bytes, not columns on screen).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    /// Zero-based line.
    pub line: usize,
    /// Zero-based character offset within the line.
    pub col: usize,
}

impl Position {
    /// Creates a position.
    pub const fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}
