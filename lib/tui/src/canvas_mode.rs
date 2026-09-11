//! Drawing inside a ```` ```draw ```` block with the keyboard.
//!
//! Canvas mode keeps a [`Drawing`] parsed from the block, a cell cursor,
//! and the tool in use. Every change is written straight back into the
//! buffer as text (one undo step each), so saving, undo and the live
//! preview need nothing special. The block itself is shown rendered with
//! the cursor, the shape under it, and any shape being placed drawn on
//! top.

use canvas::{Attrs, BoxKind, Drawing, Heads, Point, Rect, Shape, COLOR_NAMES};
use editor::Editor;
use syntax::{BlockKind, Document, LineIndex};

/// The canvas is never smaller than this while editing, so an empty
/// drawing has room to move around in.
pub(crate) const MIN_SIZE: (i32, i32) = (30, 6);

/// What the next Enter will create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placing {
    /// A box of this outline, from the anchor to the cursor.
    Box(BoxKind),
    /// A polyline; `true` puts an arrowhead on its end.
    Line(bool),
}

/// What the keyboard is doing on the canvas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tool {
    /// Moving the cursor; the shape under it is highlighted.
    Select,
    /// Placing a new shape.
    Placing {
        /// The kind of shape.
        what: Placing,
        /// First corner (boxes) or the points so far (lines).
        points: Vec<Point>,
    },
    /// Dragging a shape with the cursor.
    Moving {
        /// Index of the shape being moved.
        shape: usize,
        /// The shape as it was, restored on Esc.
        original: Shape,
    },
    /// Typing a label in the status bar.
    Text {
        /// Shape whose label is edited; `None` creates free text at the cursor.
        target: Option<usize>,
        /// The text so far.
        input: String,
    },
}

/// Canvas mode state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CanvasState {
    /// Buffer line of the opening ```` ```draw ```` fence.
    pub fence: usize,
    /// The drawing as currently shown (may be ahead of the buffer while
    /// a shape is being moved).
    pub drawing: Drawing,
    /// The cell under the cursor.
    pub cursor: Point,
    /// The active tool.
    pub tool: Tool,
    /// Bumped on every visible change, for the view cache.
    pub generation: u64,
}

impl CanvasState {
    /// The shape to highlight: the one under the cursor, or the one being
    /// moved or relabelled.
    pub fn selected(&self) -> Option<usize> {
        match &self.tool {
            Tool::Moving { shape, .. } => Some(*shape),
            Tool::Text {
                target: Some(i), ..
            } => Some(*i),
            Tool::Placing { .. } => None,
            Tool::Select | Tool::Text { target: None, .. } => self.drawing.hit(self.cursor),
        }
    }

    /// The shape that Enter would create, for the preview.
    pub fn preview(&self) -> Option<Shape> {
        let Tool::Placing { what, points } = &self.tool else {
            return None;
        };
        let anchor = *points.first()?;
        Some(match what {
            Placing::Box(kind) => Shape::Box {
                kind: *kind,
                rect: Rect::from_corners(anchor, self.cursor),
                label: None,
                attrs: Attrs::default(),
            },
            Placing::Line(arrow) => {
                let mut pts = points.clone();
                if pts.last() != Some(&self.cursor) {
                    pts.push(self.cursor);
                }
                if pts.len() < 2 {
                    pts.push(self.cursor);
                }
                Shape::Line {
                    points: pts,
                    heads: Heads {
                        start: false,
                        end: *arrow,
                    },
                    label: None,
                    attrs: Attrs::default(),
                }
            }
        })
    }

    /// One line describing the current tool, for the status bar.
    pub fn hint(&self) -> String {
        match &self.tool {
            Tool::Select => match self.selected() {
                Some(i) => format!(
                    "{}  t label  m move  x delete  f fill  - dash  o round  c colour",
                    describe(&self.drawing.shapes[i])
                ),
                None => "r rect  e ellipse  d diamond  l line  a arrow  t text  ? help".to_owned(),
            },
            Tool::Placing {
                what: Placing::Box(kind),
                ..
            } => format!(
                "{}: move to the opposite corner, Enter to place, Esc to cancel",
                kind.keyword()
            ),
            Tool::Placing {
                what: Placing::Line(_),
                points,
            } => format!(
                "line ({} pt): Space adds a corner, Enter finishes, Esc cancels",
                points.len()
            ),
            Tool::Moving { .. } => "move: arrows drag, Enter drops, Esc puts it back".to_owned(),
            Tool::Text { target, input } => {
                let what = if target.is_some() { "Label" } else { "Text" };
                format!("{what}: {input}")
            }
        }
    }

    /// Whether a label is being typed (the status bar owns the cursor).
    pub fn is_typing(&self) -> bool {
        matches!(self.tool, Tool::Text { .. })
    }

    fn bump(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

fn describe(shape: &Shape) -> String {
    match shape {
        Shape::Box { kind, label, .. } => match label {
            Some(l) => format!("{} \"{l}\"", kind.keyword()),
            None => kind.keyword().to_owned(),
        },
        Shape::Line { heads, label, .. } => {
            let kind = if heads.start || heads.end {
                "arrow"
            } else {
                "line"
            };
            match label {
                Some(l) => format!("{kind} \"{l}\""),
                None => kind.to_owned(),
            }
        }
        Shape::Text { text, .. } => format!("text \"{text}\""),
        Shape::Raw(_) => "raw line".to_owned(),
    }
}

/// The ```` ```draw ```` block whose lines contain `line`, as
/// `(fence line, last line of the block)`.
pub(crate) fn block_at(doc: &Document, index: &LineIndex, line: usize) -> Option<(usize, usize)> {
    doc.blocks.iter().find_map(|b| match &b.kind {
        BlockKind::CodeBlock { lang, .. } if lang.as_deref() == Some(canvas::LANG) => {
            let (first, last) = index.lines_of(&b.span);
            (first..=last).contains(&line).then_some((first, last))
        }
        _ => None,
    })
}

/// Lines of the block body (between the fences), given the fence line.
/// `None` when the fence is not (or no longer) a draw fence.
pub(crate) fn body_range(editor: &Editor, fence: usize) -> Option<(usize, usize)> {
    if !is_draw_fence(&editor.line(fence)) {
        return None;
    }
    let mut end = fence + 1;
    while end < editor.len_lines() {
        if editor.line(end).trim_start().starts_with("```") {
            return Some((fence + 1, end));
        }
        end += 1;
    }
    Some((fence + 1, end))
}

fn is_draw_fence(line: &str) -> bool {
    let rest = line.trim_start();
    rest.strip_prefix("```")
        .or_else(|| rest.strip_prefix("~~~"))
        .is_some_and(|info| info.split_whitespace().next() == Some(canvas::LANG))
}

/// Makes sure the block starting at `fence` has a closing fence and a
/// line after it to return to. Returns the closing fence line.
pub(crate) fn ensure_closed(editor: &mut Editor, fence: usize) -> Option<usize> {
    let (_, end) = body_range(editor, fence)?;
    let closed = end < editor.len_lines() && editor.line(end).trim_start().starts_with("```");
    if !closed {
        editor.replace_lines(end, end, "```\n");
    }
    Some(end)
}

/// Reads the drawing from the block at `fence`.
pub(crate) fn read(editor: &Editor, fence: usize) -> Option<Drawing> {
    let (first, end) = body_range(editor, fence)?;
    let mut src = String::new();
    for i in first..end {
        src.push_str(&editor.line(i));
        src.push('\n');
    }
    Some(canvas::parse(&src))
}

/// Writes `drawing` into the block at `fence`, replacing its body.
pub(crate) fn write(editor: &mut Editor, fence: usize, drawing: &Drawing) -> bool {
    let Some((first, end)) = body_range(editor, fence) else {
        return false;
    };
    editor.replace_lines(first, end, &canvas::to_source(drawing));
    true
}

/// Inserts an empty draw block at the cursor line (on it if the line is
/// blank, after it otherwise) and returns the fence line.
pub(crate) fn insert_block(editor: &mut Editor) -> usize {
    let line = editor.cursor().line;
    let at = if editor.line(line).trim().is_empty() {
        line
    } else {
        line + 1
    };
    let end = if at == line { line + 1 } else { at };
    let after_blank = end < editor.len_lines() && editor.line(end).trim().is_empty();
    let text = if after_blank {
        "```draw\n```\n"
    } else {
        "```draw\n```\n\n"
    };
    editor.replace_lines(at, end.min(editor.len_lines()), text);
    at
}

/// Applies a step of the tool state machine for a movement of the cursor.
impl CanvasState {
    /// Moves the cursor by `(dx, dy)` cells, dragging the shape when moving.
    pub fn nudge(&mut self, dx: i32, dy: i32, max_x: i32) {
        let target = Point::new(
            (self.cursor.x + dx).clamp(0, max_x.max(0)),
            (self.cursor.y + dy).max(0),
        );
        let (ddx, ddy) = (target.x - self.cursor.x, target.y - self.cursor.y);
        if ddx == 0 && ddy == 0 {
            return;
        }
        self.cursor = target;
        if let Tool::Moving { shape, .. } = &self.tool {
            if let Some(s) = self.drawing.shapes.get_mut(*shape) {
                s.translate(ddx, ddy);
                clamp_to_origin(s);
            }
        }
        self.bump();
    }

    /// Starts placing a shape at the cursor.
    pub fn start(&mut self, what: Placing) {
        self.tool = Tool::Placing {
            what,
            points: vec![self.cursor],
        };
        self.bump();
    }

    /// Adds the cursor as a corner of the line being placed.
    pub fn add_point(&mut self) {
        if let Tool::Placing {
            what: Placing::Line(_),
            points,
        } = &mut self.tool
        {
            if points.last() != Some(&self.cursor) {
                points.push(self.cursor);
                self.bump();
            }
        }
    }

    /// Finishes the current tool (Enter). Returns `true` when the drawing
    /// changed and must be written back.
    pub fn commit(&mut self) -> bool {
        if matches!(self.tool, Tool::Placing { .. }) {
            return self.place();
        }
        match std::mem::replace(&mut self.tool, Tool::Select) {
            Tool::Placing { .. } | Tool::Select => false,
            Tool::Moving { .. } => {
                self.bump();
                true
            }
            Tool::Text { target, input } => {
                let value = input.trim();
                match target {
                    Some(i) => {
                        if let Some(s) = self.drawing.shapes.get_mut(i) {
                            s.set_label(value);
                        }
                    }
                    None => {
                        if !value.is_empty() {
                            self.drawing.shapes.push(Shape::Text {
                                at: self.cursor,
                                text: value.to_owned(),
                                attrs: Attrs::default(),
                            });
                        }
                    }
                }
                self.bump();
                true
            }
        }
    }

    /// Finishes placing: turns the preview into a real shape.
    fn place(&mut self) -> bool {
        let Some(shape) = self.preview() else {
            return false;
        };
        let ok = match &shape {
            Shape::Line { points, .. } => {
                points.len() >= 2 && points.windows(2).any(|w| w[0] != w[1])
            }
            _ => true,
        };
        self.tool = Tool::Select;
        self.bump();
        if ok {
            self.drawing.shapes.push(shape);
            true
        } else {
            false
        }
    }

    /// Cancels the current tool. Returns `true` if the drawing must be
    /// restored from the buffer (a move was abandoned).
    pub fn cancel(&mut self) -> bool {
        let restore = match std::mem::replace(&mut self.tool, Tool::Select) {
            Tool::Moving { shape, original } => {
                if let Some(s) = self.drawing.shapes.get_mut(shape) {
                    *s = original;
                }
                true
            }
            _ => false,
        };
        self.bump();
        restore
    }

    /// Picks up the shape under the cursor.
    pub fn grab(&mut self) -> bool {
        let Some(i) = self.drawing.hit(self.cursor) else {
            return false;
        };
        self.tool = Tool::Moving {
            shape: i,
            original: self.drawing.shapes[i].clone(),
        };
        self.bump();
        true
    }

    /// Starts editing the label of the shape under the cursor, or new
    /// text at the cursor.
    pub fn edit_text(&mut self) {
        let target = self
            .drawing
            .hit(self.cursor)
            .filter(|i| !matches!(self.drawing.shapes[*i], Shape::Raw(_)));
        let input = target
            .and_then(|i| self.drawing.shapes[i].label())
            .map(str::trim)
            .unwrap_or_default()
            .to_owned();
        self.tool = Tool::Text { target, input };
        self.bump();
    }

    /// Deletes the shape under the cursor.
    pub fn delete(&mut self) -> bool {
        let Some(i) = self.drawing.hit(self.cursor) else {
            return false;
        };
        self.drawing.shapes.remove(i);
        self.bump();
        true
    }

    /// Changes the styling of the shape under the cursor with `f`.
    pub fn restyle(&mut self, f: impl FnOnce(&mut Attrs)) -> bool {
        let Some(i) = self.drawing.hit(self.cursor) else {
            return false;
        };
        let Some(attrs) = self.drawing.shapes[i].attrs_mut() else {
            return false;
        };
        f(attrs);
        self.bump();
        true
    }
}

/// Cycles a shape's colour: none, then each palette colour, then none.
pub(crate) fn next_color(current: Option<&str>) -> Option<String> {
    match current {
        None => Some(COLOR_NAMES[0].to_owned()),
        Some(c) => {
            let i = COLOR_NAMES.iter().position(|n| *n == c)?;
            COLOR_NAMES.get(i + 1).map(|n| (*n).to_owned())
        }
    }
}

/// Keeps a shape from being dragged off the top or left edge.
fn clamp_to_origin(shape: &mut Shape) {
    if let Some(b) = shape.bounds() {
        let dx = (-b.x).max(0);
        let dy = (-b.y).max(0);
        if dx > 0 || dy > 0 {
            shape.translate(dx, dy);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor::Position;

    fn state(src: &str) -> CanvasState {
        CanvasState {
            fence: 0,
            drawing: canvas::parse(src),
            cursor: Point::new(0, 0),
            tool: Tool::Select,
            generation: 0,
        }
    }

    #[test]
    fn placing_a_rect_uses_anchor_and_cursor() {
        let mut s = state("");
        s.cursor = Point::new(2, 1);
        s.start(Placing::Box(BoxKind::Rect));
        s.nudge(5, 2, 100);
        let preview = s.preview().unwrap();
        assert!(matches!(preview, Shape::Box { rect, .. } if rect == Rect::new(2, 1, 6, 3)));
        assert!(s.commit());
        assert_eq!(s.drawing.shapes.len(), 1);
        assert_eq!(s.tool, Tool::Select);
    }

    #[test]
    fn placing_a_line_collects_corners() {
        let mut s = state("");
        s.start(Placing::Line(true));
        s.nudge(4, 0, 100);
        s.add_point();
        s.nudge(0, 3, 100);
        assert!(s.commit());
        assert!(matches!(
            &s.drawing.shapes[0],
            Shape::Line { points, heads, .. } if points.len() == 3 && heads.end
        ));
        // A line with no length is dropped.
        s.start(Placing::Line(false));
        assert!(!s.commit());
        assert_eq!(s.drawing.shapes.len(), 1);
    }

    #[test]
    fn moving_drags_and_cancel_restores() {
        let mut s = state("rect 2,2 4x2\n");
        s.cursor = Point::new(3, 3);
        assert!(s.grab());
        s.nudge(-10, 1, 100);
        // Clamped at the origin horizontally, moved one row down.
        assert!(
            matches!(&s.drawing.shapes[0], Shape::Box { rect, .. } if rect.x == 0 && rect.y == 3)
        );
        assert!(s.cancel());
        assert!(
            matches!(&s.drawing.shapes[0], Shape::Box { rect, .. } if *rect == Rect::new(2, 2, 4, 2))
        );
    }

    #[test]
    fn text_tool_labels_or_creates() {
        let mut s = state("rect 0,0 6x3 \"old\"\n");
        s.cursor = Point::new(1, 1);
        s.edit_text();
        assert_eq!(
            s.tool,
            Tool::Text {
                target: Some(0),
                input: "old".into()
            }
        );
        if let Tool::Text { input, .. } = &mut s.tool {
            input.push_str("er");
        }
        assert!(s.commit());
        assert_eq!(s.drawing.shapes[0].label(), Some("older"));

        s.cursor = Point::new(20, 5);
        s.edit_text();
        assert_eq!(
            s.tool,
            Tool::Text {
                target: None,
                input: String::new()
            }
        );
        if let Tool::Text { input, .. } = &mut s.tool {
            input.push_str("hi");
        }
        assert!(s.commit());
        assert!(
            matches!(&s.drawing.shapes[1], Shape::Text { at, text, .. } if *at == Point::new(20, 5) && text == "hi")
        );
    }

    #[test]
    fn styling_and_colour_cycle() {
        let mut s = state("rect 0,0 6x3\n");
        assert!(s.restyle(|a| a.fill = !a.fill));
        assert!(s.drawing.shapes[0].attrs().unwrap().fill);
        assert_eq!(next_color(None).as_deref(), Some("red"));
        assert_eq!(next_color(Some("red")).as_deref(), Some("orange"));
        assert_eq!(next_color(Some("magenta")), None);
        assert_eq!(next_color(Some("bogus")), None);
        s.cursor = Point::new(20, 20);
        assert!(!s.restyle(|a| a.fill = true));
        assert!(!s.delete());
        s.cursor = Point::new(0, 0);
        assert!(s.delete());
        assert!(s.drawing.shapes.is_empty());
    }

    #[test]
    fn block_io_round_trips_through_the_editor() {
        let mut e = Editor::from_text("# T\n\n```draw\nrect 0,0 2x1\n```\nafter\n");
        assert_eq!(body_range(&e, 2), Some((3, 4)));
        assert_eq!(body_range(&e, 0), None);
        let mut d = read(&e, 2).unwrap();
        assert_eq!(d.shapes.len(), 1);
        d.shapes.push(Shape::Text {
            at: Point::new(1, 1),
            text: "x".into(),
            attrs: Attrs::default(),
        });
        assert!(write(&mut e, 2, &d));
        assert_eq!(
            e.text(),
            "# T\n\n```draw\nrect 0,0 2x1\ntext 1,1 \"x\"\n```\nafter\n"
        );
        assert_eq!(read(&e, 2).unwrap(), d);

        let src = e.text();
        let doc = syntax::parse(&src);
        let index = LineIndex::new(&src);
        assert_eq!(block_at(&doc, &index, 4), Some((2, 5)));
        assert_eq!(block_at(&doc, &index, 6), None);
    }

    #[test]
    fn unterminated_block_gets_closed() {
        let mut e = Editor::from_text("```draw\nrect 0,0 2x1");
        assert_eq!(ensure_closed(&mut e, 0), Some(2));
        assert_eq!(e.text(), "```draw\nrect 0,0 2x1\n```\n");
    }

    #[test]
    fn insert_block_on_blank_or_after_text() {
        let mut e = Editor::from_text("hello\n\nworld");
        e.set_cursor(Position::new(1, 0));
        assert_eq!(insert_block(&mut e), 1);
        assert_eq!(e.text(), "hello\n```draw\n```\n\nworld");

        let mut e = Editor::from_text("hello");
        assert_eq!(insert_block(&mut e), 1);
        assert_eq!(e.text(), "hello\n```draw\n```\n\n");
    }
}
