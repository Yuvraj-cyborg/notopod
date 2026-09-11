//! Drawing.

use std::io::Write;

use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use render::Drawings;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Focus, Mode};
use crate::graph_view::GraphView;

/// The screen has to be at least this wide for the file panel to be worth
/// the room it takes from the note.
const PANEL_MIN_SCREEN: u16 = 48;

impl App {
    /// Draws one frame: the tab bar when there is more than one note, the
    /// file panel when it is open, the note, then the status bar.
    pub(crate) fn draw(&mut self, frame: &mut Frame) {
        let tab_bar = u16::from(self.tabs().len() > 1);
        let [tabs, middle, status] = Layout::vertical([
            Constraint::Length(tab_bar),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        let (panel, body) = if self.files.is_some() && middle.width >= PANEL_MIN_SCREEN {
            let width = (middle.width / 3).clamp(18, 32);
            let [panel, body] =
                Layout::horizontal([Constraint::Length(width), Constraint::Min(1)]).areas(middle);
            (Some(panel), body)
        } else {
            (None, middle)
        };
        self.body_height = usize::from(body.height);
        self.body_width = usize::from(body.width);

        if self.focus == Focus::Graph && self.graph.is_some() {
            self.draw_graph(frame, body);
        } else {
            self.draw_note(frame, body);
        }
        if let Some(panel) = panel {
            self.draw_files(frame, panel);
        }
        if tab_bar > 0 {
            self.draw_tabs(frame, tabs);
        }
        self.draw_status(frame, status);
    }

    /// The graph of notes in place of the note: a heading, then the
    /// picture, scrolled so the selected note is on screen.
    fn draw_graph(&mut self, frame: &mut Frame, body: Rect) {
        let width = usize::from(body.width).max(1);
        let height = usize::from(body.height);
        let theme = self.theme.clone();
        let Some(view) = &mut self.graph else {
            return;
        };
        view.fit(width);
        let overlay = view.overlay();
        let lines = self
            .graphics
            .draw(view.drawing(), &theme, width, overlay.as_ref());
        let pending = self.graphics.take_pending();
        if !pending.is_empty() {
            let mut out = std::io::stdout();
            let _ = out.write_all(pending.as_bytes());
            let _ = out.flush();
        }

        let picture_height = height.saturating_sub(1);
        if let Some((top, bottom)) = view.selected_rows() {
            if top < view.scroll {
                view.scroll = top;
            }
            if picture_height > 0 && bottom >= view.scroll + picture_height {
                view.scroll = bottom + 1 - picture_height;
            }
        }
        view.scroll = view.scroll.min(lines.len().saturating_sub(picture_height));

        let heading = Line::styled(
            format!(" {}   {}", view.summary(), view.root().display()),
            theme.heading[1],
        );
        let mut rows = vec![heading];
        rows.extend(lines.into_iter().skip(view.scroll).take(picture_height));
        frame.render_widget(Paragraph::new(Text::from(rows)), body);
    }

    /// The file panel with a divider on its right.
    fn draw_files(&mut self, frame: &mut Frame, area: Rect) {
        let [tree, divider] =
            Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        let open = self.open_paths();
        let open: Vec<&std::path::Path> = open.iter().map(std::path::PathBuf::as_path).collect();
        let focused = self.focus == Focus::Files;
        let theme = self.theme.clone();
        let Some(panel) = &mut self.files else {
            return;
        };
        let lines = panel.lines(
            &theme,
            usize::from(tree.width),
            usize::from(tree.height),
            focused,
            &open,
        );
        frame.render_widget(Paragraph::new(Text::from(lines)), tree);
        let bar = Line::styled("│", theme.table_border);
        frame.render_widget(
            Paragraph::new(Text::from(vec![bar; usize::from(divider.height)])),
            divider,
        );
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
            Mode::Edit => self.vim.as_ref().is_some_and(|v| v.command.is_some()),
            Mode::ConfirmQuit | Mode::ConfirmClose => false,
        };
        if self.focus != Focus::Editor && !cursor_in_status {
            // The panel and the graph draw their own cursor; the terminal's
            // stays hidden.
        } else if cursor_in_status {
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
        if self.focus == Focus::Graph {
            if let Some((message, _)) = &self.notice {
                return format!(" {message}");
            }
            let picked = self
                .graph
                .as_ref()
                .and_then(GraphView::selected_summary)
                .unwrap_or_default();
            return format!(" graph  {picked}");
        }
        if self.focus == Focus::Files {
            if let Some((message, _)) = &self.notice {
                return format!(" {message}");
            }
            let root = self
                .files
                .as_ref()
                .map_or_else(String::new, |p| p.root().display().to_string());
            return format!(" files  {root}");
        }
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
                if let Some(vim) = &self.vim {
                    if vim.command.is_some() {
                        return format!(" {}", vim.status());
                    }
                }
                let vim = self
                    .vim
                    .as_ref()
                    .map_or_else(String::new, |v| format!(" {}  ", v.status()));
                if let Some((message, _)) = &self.notice {
                    return format!("{vim} {message}");
                }
                let cursor = self.editor().cursor();
                format!(
                    "{vim} {}{}   Ln {}, Col {}",
                    self.file_name(),
                    if self.editor().is_dirty() { " [+]" } else { "" },
                    cursor.line + 1,
                    cursor.col + 1
                )
            }
        }
    }

    fn status_right(&self) -> String {
        if self.focus == Focus::Graph {
            return "←↑↓→ pick  Tab next  Enter open  r re-read  Esc back ".to_owned();
        }
        if self.focus == Focus::Files {
            return "Enter open  ← → fold  g graph  r re-read  Esc back  ^B close ".to_owned();
        }
        match &self.mode {
            Mode::Find { .. } => "Enter next   Esc done ".to_owned(),
            Mode::SaveAs { .. } => "Enter save   Esc cancel ".to_owned(),
            Mode::Open { .. } => "Enter open   Esc cancel ".to_owned(),
            Mode::ConfirmQuit | Mode::ConfirmClose => String::new(),
            Mode::Canvas(_) => "Esc done ".to_owned(),
            Mode::Edit => format!(
                "^S save  ^O open  ^B files  ^K graph  ^Q quit  ^F find  ^D draw  ^Z undo  ^P preview {} ",
                if self.preview { "on" } else { "off" }
            ),
        }
    }
}
