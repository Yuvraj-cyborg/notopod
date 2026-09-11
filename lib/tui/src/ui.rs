//! Drawing.

use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::text::{Line, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode};

impl App {
    /// Draws one frame: the note, then the status bar.
    pub(crate) fn draw(&mut self, frame: &mut Frame) {
        let [body, status] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());
        self.body_height = usize::from(body.height);
        self.body_width = usize::from(body.width);

        let width = usize::from(body.width).max(1);
        let height = usize::from(body.height);
        let (cursor_row, cursor_x, row_count) = {
            let view = self.ensure_view(width);
            (view.cursor.0, view.cursor.1, view.rows.len())
        };

        if height > 0 {
            if cursor_row < self.scroll {
                self.scroll = cursor_row;
            }
            if cursor_row >= self.scroll + height {
                self.scroll = cursor_row + 1 - height;
            }
        }
        self.scroll = self.scroll.min(row_count.saturating_sub(1));
        let scroll = self.scroll;

        let lines: Vec<Line<'static>> = self
            .ensure_view(width)
            .rows
            .iter()
            .skip(scroll)
            .take(height)
            .map(|r| r.line.clone())
            .collect();
        frame.render_widget(Paragraph::new(Text::from(lines)), body);

        self.draw_status(frame, status);

        match &self.mode {
            Mode::Edit | Mode::ConfirmQuit => {
                let y = cursor_row.saturating_sub(scroll);
                if y < height && cursor_x < width {
                    frame.set_cursor_position(Position::new(
                        body.x + cursor_x as u16,
                        body.y + y as u16,
                    ));
                }
            }
            Mode::Find { .. } | Mode::SaveAs { .. } => {
                let x = self.status_left().width().min(usize::from(status.width));
                frame.set_cursor_position(Position::new(status.x + x as u16, status.y));
            }
        }
    }

    fn draw_status(&self, frame: &mut Frame, area: Rect) {
        let width = usize::from(area.width);
        let left = self.status_left();
        let right = self.status_right();

        let mut text = left.clone();
        if left.width() + right.width() + 2 <= width {
            let gap = width - left.width() - right.width();
            text.push_str(&" ".repeat(gap));
            text.push_str(&right);
        } else if left.width() > width {
            text = left.chars().take(width.saturating_sub(1)).collect();
            text.push('…');
        }
        let pad = width.saturating_sub(text.width());
        text.push_str(&" ".repeat(pad));

        frame.render_widget(Paragraph::new(text).style(self.theme.status_bar), area);
    }

    fn status_left(&self) -> String {
        match &self.mode {
            Mode::Find { query, .. } => format!(" Find: {query}"),
            Mode::SaveAs { input } => format!(" Save as: {input}"),
            Mode::ConfirmQuit => {
                " Unsaved changes. Press Ctrl+Q again to quit without saving, or Esc to go back."
                    .to_owned()
            }
            Mode::Edit => {
                if let Some((message, _)) = &self.notice {
                    return format!(" {message}");
                }
                let cursor = self.editor.cursor();
                format!(
                    " {}{}   Ln {}, Col {}",
                    self.file_name(),
                    if self.editor.is_dirty() { " [+]" } else { "" },
                    cursor.line + 1,
                    cursor.col + 1
                )
            }
        }
    }

    fn status_right(&self) -> String {
        match &self.mode {
            Mode::Find { .. } => "Enter next   Esc done ".to_owned(),
            Mode::SaveAs { .. } => "Enter save   Esc cancel ".to_owned(),
            Mode::ConfirmQuit => String::new(),
            Mode::Edit => format!(
                "^S save  ^Q quit  ^F find  ^Z undo  ^P preview {} ",
                if self.preview { "on" } else { "off" }
            ),
        }
    }
}
