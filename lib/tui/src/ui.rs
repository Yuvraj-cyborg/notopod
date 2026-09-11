//! Drawing.

use std::io::Write;

use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode};

impl App {
    /// Draws one frame: the tab bar when there is more than one note, the
    /// note, then the status bar.
    pub(crate) fn draw(&mut self, frame: &mut Frame) {
        let tab_bar = u16::from(self.tabs().len() > 1);
        let [tabs, body, status] = Layout::vertical([
            Constraint::Length(tab_bar),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        self.body_height = usize::from(body.height);
        self.body_width = usize::from(body.width);

        self.draw_note(frame, body);
        if tab_bar > 0 {
            self.draw_tabs(frame, tabs);
        }
        self.draw_status(frame, status);
    }

    /// The note itself, with the cursor placed.
    fn draw_note(&mut self, frame: &mut Frame, body: Rect) {
        let width = usize::from(body.width).max(1);
        let height = usize::from(body.height);
        let (cursor_row, cursor_x, row_count) = {
            let view = self.ensure_view(width);
            (view.cursor.0, view.cursor.1, view.rows.len())
        };
        // Pictures must reach the terminal before the cells that show them.
        // ratatui writes the frame after this closure returns, so anything
        // written now comes first.
        let pending = self.graphics.take_pending();
        if !pending.is_empty() {
            let mut out = std::io::stdout();
            let _ = out.write_all(pending.as_bytes());
            let _ = out.flush();
        }

        let scroll = {
            let scroll = &mut self.tab_mut().scroll;
            if height > 0 {
                if cursor_row < *scroll {
                    *scroll = cursor_row;
                }
                if cursor_row >= *scroll + height {
                    *scroll = cursor_row + 1 - height;
                }
            }
            *scroll = (*scroll).min(row_count.saturating_sub(1));
            *scroll
        };

        let lines: Vec<Line<'static>> = self
            .ensure_view(width)
            .rows
            .iter()
            .skip(scroll)
            .take(height)
            .map(|r| r.line.clone())
            .collect();
        frame.render_widget(Paragraph::new(Text::from(lines)), body);

        let cursor_in_status = match &self.mode {
            Mode::Find { .. } | Mode::SaveAs { .. } | Mode::Open { .. } => true,
            Mode::Canvas(c) => c.is_typing(),
            Mode::Edit | Mode::ConfirmQuit | Mode::ConfirmClose => false,
        };
        if cursor_in_status {
            let status_y = body.y + body.height;
            let x = self.status_left().width().min(usize::from(body.width));
            frame.set_cursor_position(Position::new(body.x + x as u16, status_y));
        } else {
            let y = cursor_row.saturating_sub(scroll);
            if y < height && cursor_x < width {
                frame.set_cursor_position(Position::new(
                    body.x + cursor_x as u16,
                    body.y + y as u16,
                ));
            }
        }
    }

    /// One row of tab names, the current one in the status bar's colours.
    fn draw_tabs(&self, frame: &mut Frame, area: Rect) {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut used = 0usize;
        let width = usize::from(area.width);
        for (i, tab) in self.tabs().iter().enumerate() {
            let label = format!(
                " {}{} ",
                tab.name(),
                if tab.editor.is_dirty() { " +" } else { "" }
            );
            let w = label.width();
            if used + w > width {
                break;
            }
            let style = if i == self.active() {
                self.theme.status_bar
            } else {
                self.theme.table_border
            };
            spans.push(Span::styled(label, style));
            used += w;
        }
        let rest = width.saturating_sub(used);
        spans.push(Span::styled("─".repeat(rest), self.theme.table_border));
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
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

    pub(crate) fn status_left(&self) -> String {
        match &self.mode {
            Mode::Find { query, .. } => format!(" Find: {query}"),
            Mode::SaveAs { input } => format!(" Save as: {input}"),
            Mode::Open { input } => format!(" Open: {input}"),
            Mode::ConfirmQuit => {
                let n = self.dirty_count();
                format!(
                    " {} unsaved. Press Ctrl+Q again to quit without saving, or Esc to go back.",
                    if n == 1 {
                        "This note has changes".to_owned()
                    } else {
                        format!("{n} notes have changes")
                    }
                )
            }
            Mode::ConfirmClose => {
                " Unsaved changes. Press Ctrl+W again to close without saving, or Esc to go back."
                    .to_owned()
            }
            Mode::Canvas(c) => {
                if let Some((message, _)) = &self.notice {
                    if !c.is_typing() {
                        return format!(" {message}");
                    }
                }
                format!(" draw  {},{}  {}", c.cursor.x, c.cursor.y, c.hint())
            }
            Mode::Edit => {
                if let Some((message, _)) = &self.notice {
                    return format!(" {message}");
                }
                let cursor = self.editor().cursor();
                format!(
                    " {}{}   Ln {}, Col {}",
                    self.file_name(),
                    if self.editor().is_dirty() { " [+]" } else { "" },
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
            Mode::Open { .. } => "Enter open   Esc cancel ".to_owned(),
            Mode::ConfirmQuit | Mode::ConfirmClose => String::new(),
            Mode::Canvas(_) => "Esc done ".to_owned(),
            Mode::Edit => format!(
                "^S save  ^O open  ^Q quit  ^F find  ^D draw  ^Z undo  ^P preview {} ",
                if self.preview { "on" } else { "off" }
            ),
        }
    }
}
