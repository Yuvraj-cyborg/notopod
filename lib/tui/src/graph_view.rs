//! The graph screen: every note in the folder and the links between them,
//! drawn as one picture, with a cursor that jumps from note to note.

use std::path::Path;

use canvas::{Drawing, Overlay, Point, Rect, Shape};

/// State of the graph screen.
pub(crate) struct GraphView {
    graph: notes::Graph,
    drawing: Drawing,
    width: usize,
    /// Index into `graph.nodes` of the highlighted note.
    selected: usize,
    /// First picture row on screen.
    pub(crate) scroll: usize,
}

impl GraphView {
    /// Scans `root` and lays the graph out `width` cells wide.
    pub(crate) fn new(root: &Path, width: usize) -> Self {
        let graph = notes::scan(root);
        let drawing = graph.drawing(width);
        Self {
            graph,
            drawing,
            width,
            selected: 0,
            scroll: 0,
        }
    }

    /// Reads the folder again, staying on the same note when it is still
    /// there.
    pub(crate) fn rescan(&mut self) {
        let keep = self.selected_path().map(Path::to_path_buf);
        self.graph = notes::scan(&self.graph.root);
        self.drawing = self.graph.drawing(self.width);
        self.selected = keep
            .and_then(|p| self.graph.nodes.iter().position(|n| n.path == p))
            .unwrap_or(0);
    }

    /// Lays the graph out again if the screen changed width.
    pub(crate) fn fit(&mut self, width: usize) {
        if width != self.width && width > 0 {
            self.width = width;
            self.drawing = self.graph.drawing(width);
        }
    }

    pub(crate) fn drawing(&self) -> &Drawing {
        &self.drawing
    }

    pub(crate) fn root(&self) -> &Path {
        &self.graph.root
    }

    /// The highlight for the selected note, for drawing over the picture.
    pub(crate) fn overlay(&self) -> Option<Overlay<'_>> {
        if self.graph.is_empty() {
            return None;
        }
        Some(Overlay {
            cursor: None,
            selected: Some(self.graph.node_shape(self.selected)),
            preview: None,
            min_size: (0, 0),
        })
    }

    /// Puts the cursor on the note at `path`, if it is in the graph.
    pub(crate) fn select_path(&mut self, path: &Path) -> bool {
        match self.graph.nodes.iter().position(|n| n.path == path) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    /// The note under the cursor.
    pub(crate) fn selected_path(&self) -> Option<&Path> {
        self.graph
            .nodes
            .get(self.selected)
            .map(|n| n.path.as_path())
    }

    /// The note under the cursor, described.
    pub(crate) fn selected_summary(&self) -> Option<String> {
        let node = self.graph.nodes.get(self.selected)?;
        Some(format!(
            "{}  →{} ←{}",
            node.path
                .strip_prefix(&self.graph.root)
                .unwrap_or(&node.path)
                .display(),
            node.out,
            node.inbound
        ))
    }

    /// "12 notes · 30 links", with a note when the scan stopped early.
    pub(crate) fn summary(&self) -> String {
        let mut out = format!(
            "{} {} · {} {}",
            self.graph.nodes.len(),
            if self.graph.nodes.len() == 1 {
                "note"
            } else {
                "notes"
            },
            self.graph.edges.len(),
            if self.graph.edges.len() == 1 {
                "link"
            } else {
                "links"
            },
        );
        if self.graph.truncated {
            use std::fmt::Write;
            let _ = write!(out, " (first {} only)", notes::MAX_NOTES);
        }
        out
    }

    /// Rows of the picture the selected note covers, for scrolling.
    pub(crate) fn selected_rows(&self) -> Option<(usize, usize)> {
        let rect = self.rect(self.selected)?;
        Some((rect.y.max(0) as usize, rect.bottom().max(0) as usize))
    }

    fn rect(&self, node: usize) -> Option<Rect> {
        match self.drawing.shapes.get(self.graph.node_shape(node)) {
            Some(Shape::Box { rect, .. }) => Some(*rect),
            _ => None,
        }
    }

    fn center(&self, node: usize) -> Option<Point> {
        let r = self.rect(node)?;
        Some(Point::new(r.x + r.w / 2, r.y + r.h / 2))
    }

    /// Moves the cursor to the nearest note in the direction `(dx, dy)`,
    /// where one of them is zero. Rows count double, since a cell is about
    /// twice as tall as it is wide. Stays put when nothing lies that way.
    pub(crate) fn step(&mut self, dx: i32, dy: i32) {
        let Some(from) = self.center(self.selected) else {
            return;
        };
        let mut best: Option<(i64, usize)> = None;
        for node in 0..self.graph.nodes.len() {
            if node == self.selected {
                continue;
            }
            let Some(to) = self.center(node) else {
                continue;
            };
            let (ex, ey) = (i64::from(to.x - from.x), i64::from(to.y - from.y) * 2);
            let ahead = match (dx.signum(), dy.signum()) {
                (1, _) => ex > 0 && ex.abs() >= ey.abs() / 2,
                (-1, _) => ex < 0 && ex.abs() >= ey.abs() / 2,
                (_, 1) => ey > 0 && ey.abs() >= ex.abs() / 2,
                (_, -1) => ey < 0 && ey.abs() >= ex.abs() / 2,
                _ => false,
            };
            if !ahead {
                continue;
            }
            let score = ex * ex + ey * ey;
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, node));
            }
        }
        if let Some((_, node)) = best {
            self.selected = node;
        }
    }

    /// Tab / Shift+Tab: the next or previous note in reading order (top to
    /// bottom, left to right), wrapping around.
    pub(crate) fn next(&mut self, delta: isize) {
        let n = self.graph.nodes.len();
        if n == 0 {
            return;
        }
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| self.center(i).map_or((0, 0), |c| (c.y, c.x)));
        let at = order.iter().position(|&i| i == self.selected).unwrap_or(0);
        let to = (at as isize + delta).rem_euclid(n as isize) as usize;
        self.selected = order[to];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("A.md"), "[[B]] [[C]]\n").expect("write");
        fs::write(dir.path().join("B.md"), "[[A]]\n").expect("write");
        fs::write(dir.path().join("C.md"), "\n").expect("write");
        dir
    }

    #[test]
    fn arrows_reach_every_note_and_tab_cycles() {
        let dir = vault();
        let mut view = GraphView::new(dir.path(), 60);
        assert_eq!(view.summary(), "3 notes · 3 links");
        assert!(view.overlay().is_some());
        assert!(view.selected_rows().is_some());

        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..3 {
            seen.insert(view.selected_path().expect("a note").to_path_buf());
            view.next(1);
        }
        assert_eq!(seen.len(), 3, "Tab visits every note");
        view.next(-1);
        let back = view.selected_path().expect("a note").to_path_buf();
        view.next(1);
        view.next(-1);
        assert_eq!(view.selected_path(), Some(back.as_path()));

        // Stepping in every direction from every note never leaves the graph.
        for start in 0..3 {
            view.selected = start;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                view.step(dx, dy);
                assert!(view.selected < 3);
            }
        }
    }

    #[test]
    fn rescan_follows_the_folder_and_an_empty_one_has_no_cursor() {
        let dir = vault();
        let mut view = GraphView::new(dir.path(), 60);
        view.next(1);
        let keep = view.selected_path().expect("a note").to_path_buf();
        fs::write(dir.path().join("D.md"), "[[A]]\n").expect("write");
        view.rescan();
        assert_eq!(view.selected_path(), Some(keep.as_path()));
        assert!(view.summary().starts_with("4 notes"));
        view.fit(40);
        assert!(view.drawing().extent().0 <= 40);

        let empty = tempfile::tempdir().expect("tempdir");
        let view = GraphView::new(empty.path(), 60);
        assert!(view.overlay().is_none());
        assert!(view.selected_path().is_none());
        assert_eq!(view.summary(), "0 notes · 0 links");
    }
}
