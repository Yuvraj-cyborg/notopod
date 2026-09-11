//! Choosing a theme by looking at it.
//!
//! `notopod themes` opens this screen: every theme on the left, a sample
//! note drawn with the highlighted one on the right, redrawn as you move.
//! Enter answers with the name so the caller can save it; Esc answers with
//! nothing and the config file is left alone.

use std::io::Write;
use std::time::Duration;

use anyhow::Result;
use graphics::{Graphics, Mode};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::{DefaultTerminal, Frame};
use theme::Theme;
use unicode_width::UnicodeWidthStr;

/// A theme the picker can offer.
pub struct Entry {
    /// What it is called, and what gets written to the config file.
    pub name: String,
    /// The theme itself, already loaded.
    pub theme: Theme,
    /// Whether it came from the user's themes directory.
    pub user: bool,
}

/// The note shown beside the list. Small on purpose: it has to fit next to
/// the names on an 80-column terminal, and every line of it is a block kind
/// a theme colours differently.
const SAMPLE: &str = "\
# notopod

Notes in your terminal, with **bold**, *italic* and `code`.

- [x] pick a theme
- [ ] write something

> Every block is formatted except the one you are editing.

```draw
rect 0,0 14x5 \"notes\" round
line 14,2 -> 22,2
ellipse 22,0 16x5 \"pictures\" fill color=green
```

| block | drawn |
|---|---|
| this table | live |
";

/// Widest the list of names is allowed to get.
const LIST_MAX: u16 = 26;

/// Narrowest the preview is worth drawing at.
const PREVIEW_MIN: u16 = 24;

/// Runs the picker and returns the chosen theme's name, or `None` if the
/// user backed out.
///
/// Takes over the terminal for the duration and restores it afterwards.
/// `graphics` decides whether the sample's drawing is a picture.
pub fn pick_theme(
    entries: Vec<Entry>,
    current: Option<&str>,
    graphics: Mode,
) -> Result<Option<String>> {
    if entries.is_empty() {
        return Ok(None);
    }
    let mut terminal = ratatui::init();
    // Raw mode is on now, so the terminal's answer can be read.
    let mut picker = Picker::new(entries, current, Graphics::detect(graphics));
    let result = picker.run(&mut terminal);
    let bye = picker.graphics.release_all();
    if !bye.is_empty() {
        let mut out = std::io::stdout();
        let _ = out.write_all(bye.as_bytes());
        let _ = out.flush();
    }
    ratatui::restore();
    result
}

struct Picker {
    entries: Vec<Entry>,
    /// Index into `entries` of the theme in the config file, if it is one
    /// of these.
    current: Option<usize>,
    /// Indices into `entries` that match the filter, in display order.
    shown: Vec<usize>,
    /// Index into `shown`.
    cursor: usize,
    scroll: usize,
    filter: String,
    graphics: Graphics,
    doc: syntax::Document,
    /// Rows the list has room for, measured while drawing.
    height: usize,
    /// Set once the user has answered; the loop leaves on the next frame.
    done: Option<Done>,
}

/// How the picker ended.
#[derive(Debug, PartialEq, Eq)]
enum Done {
    /// Save this name.
    Chose(String),
    /// Leave the config file alone.
    Cancelled,
}

impl Picker {
    fn new(entries: Vec<Entry>, current: Option<&str>, graphics: Graphics) -> Self {
        let current = current.and_then(|name| entries.iter().position(|e| e.name == name));
        let shown = (0..entries.len()).collect();
        Self {
            entries,
            current,
            shown,
            cursor: current.unwrap_or(0),
            scroll: 0,
            filter: String::new(),
            graphics,
            doc: syntax::parse(SAMPLE),
            height: 1,
            done: None,
        }
    }

    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<Option<String>> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;
            match self.done.take() {
                Some(Done::Chose(name)) => return Ok(Some(name)),
                Some(Done::Cancelled) => return Ok(None),
                None => {}
            }
            if event::poll(Duration::from_millis(250))? {
                self.handle(&event::read()?);
                while self.done.is_none() && event::poll(Duration::ZERO)? {
                    self.handle(&event::read()?);
                }
            }
        }
    }

    fn handle(&mut self, event: &Event) {
        if let Event::Key(key) = event {
            if key.kind != KeyEventKind::Release {
                self.handle_key(*key);
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let page = self.height.saturating_sub(1).max(1) as isize;
        match (key.code, ctrl) {
            (KeyCode::Esc, _) if !self.filter.is_empty() => self.set_filter(String::new()),
            (KeyCode::Char('c' | 'q'), true) | (KeyCode::Esc, _) => {
                self.done = Some(Done::Cancelled);
            }
            (KeyCode::Enter, _) => {
                self.done = Some(Done::Chose(self.selected().name.clone()));
            }
            (KeyCode::Up, _) | (KeyCode::Char('p'), true) => self.move_by(-1),
            (KeyCode::Down, _) | (KeyCode::Char('n'), true) => self.move_by(1),
            (KeyCode::PageUp, _) => self.move_by(-page),
            (KeyCode::PageDown, _) => self.move_by(page),
            (KeyCode::Home, _) => self.move_to(0),
            (KeyCode::End, _) => self.move_to(self.shown.len().saturating_sub(1)),
            (KeyCode::Backspace, _) => {
                let mut filter = self.filter.clone();
                filter.pop();
                self.set_filter(filter);
            }
            (KeyCode::Char(c), false) => {
                let mut filter = self.filter.clone();
                filter.push(c);
                self.set_filter(filter);
            }
            _ => {}
        }
    }

    /// Narrows the list. A filter that would leave nothing to choose from
    /// is refused, so there is always a theme under the cursor and the
    /// preview never goes blank.
    fn set_filter(&mut self, filter: String) {
        let needle = filter.to_lowercase();
        let shown: Vec<usize> = (0..self.entries.len())
            .filter(|&i| self.entries[i].name.to_lowercase().contains(&needle))
            .collect();
        if shown.is_empty() {
            return;
        }
        // Stay on the same theme when it survives the change.
        let keep = self.shown.get(self.cursor).copied();
        self.cursor = keep
            .and_then(|i| shown.iter().position(|&j| j == i))
            .unwrap_or(0);
        self.shown = shown;
        self.filter = filter;
    }

    fn move_by(&mut self, delta: isize) {
        let last = self.shown.len().saturating_sub(1) as isize;
        let to = (self.cursor as isize + delta).clamp(0, last);
        self.move_to(to as usize);
    }

    fn move_to(&mut self, index: usize) {
        self.cursor = index.min(self.shown.len().saturating_sub(1));
    }

    fn selected(&self) -> &Entry {
        &self.entries[self.shown[self.cursor]]
    }

    fn draw(&mut self, frame: &mut Frame) {
        let theme = self.selected().theme.clone();
        let [body, status] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());
        let [list, divider, preview] = Layout::horizontal([
            Constraint::Length(self.list_width(body.width)),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(body);
        self.height = usize::from(body.height);

        // The preview is built first: drawing it queues the pictures that
        // its rows refer to, and those must reach the terminal before
        // ratatui writes the frame.
        let preview_lines = self.preview_lines(&theme, usize::from(preview.width));
        let pending = self.graphics.take_pending();
        if !pending.is_empty() {
            let mut out = std::io::stdout();
            let _ = out.write_all(pending.as_bytes());
            let _ = out.flush();
        }

        frame.render_widget(
            Paragraph::new(Text::from(self.list_lines(&theme, list))),
            list,
        );
        let bar = Line::styled("│", theme.table_border);
        frame.render_widget(
            Paragraph::new(Text::from(vec![bar; usize::from(divider.height)])),
            divider,
        );
        frame.render_widget(Paragraph::new(Text::from(preview_lines)), preview);
        frame.render_widget(
            Paragraph::new(self.status_line(usize::from(status.width))).style(theme.status_bar),
            status,
        );
    }

    /// Enough room for the longest name, but never more than a third of
    /// the screen: the preview is the point.
    fn list_width(&self, total: u16) -> u16 {
        let longest = self
            .entries
            .iter()
            .map(|e| e.name.width() as u16)
            .max()
            .unwrap_or(0);
        longest
            .saturating_add(4)
            .min(LIST_MAX)
            .min(total.saturating_sub(PREVIEW_MIN + 1))
            .max(1)
    }

    fn list_lines(&mut self, theme: &Theme, area: Rect) -> Vec<Line<'static>> {
        let height = usize::from(area.height);
        let width = usize::from(area.width);
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        }
        if height > 0 && self.cursor >= self.scroll + height {
            self.scroll = self.cursor + 1 - height;
        }

        self.shown
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(height)
            .map(|(row, &i)| {
                let entry = &self.entries[i];
                let mark = if self.current == Some(i) { "•" } else { " " };
                let text = format!(" {mark} {}", entry.name);
                let pad = width.saturating_sub(text.width());
                let style = if row == self.cursor {
                    theme.status_bar
                } else if entry.user {
                    theme.link
                } else {
                    theme.text
                };
                Line::from(Span::styled(format!("{text}{}", " ".repeat(pad)), style))
            })
            .collect()
    }

    fn preview_lines(&mut self, theme: &Theme, width: usize) -> Vec<Line<'static>> {
        if width < usize::from(PREVIEW_MIN) {
            return Vec::new();
        }
        render::render_document_with(&self.doc, width, theme, &mut self.graphics)
    }

    fn status_line(&self, width: usize) -> String {
        let left = if self.filter.is_empty() {
            let entry = self.selected();
            format!(
                " {}   {}",
                entry.name,
                if entry.user { "yours" } else { "built-in" }
            )
        } else {
            format!(" filter: {}", self.filter)
        };
        // The keys matter more than the flourish: on a narrow screen the
        // hint gets shorter rather than disappearing.
        let hints = [
            "↑↓ move   type to filter   Enter use   Esc cancel ",
            "Enter use   Esc cancel ",
            "",
        ];
        let mut out = left.clone();
        if let Some(right) = hints
            .iter()
            .find(|h| left.width() + h.width() <= width && !h.is_empty())
        {
            out.push_str(&" ".repeat(width - left.width() - right.width()));
            out.push_str(right);
        }
        let pad = width.saturating_sub(out.width());
        out.push_str(&" ".repeat(pad));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picker(current: Option<&str>) -> Picker {
        let entries = ["default", "nord", "dracula"]
            .into_iter()
            .map(|name| Entry {
                name: name.to_owned(),
                theme: Theme::builtin(name).expect("built-in"),
                user: false,
            })
            .chain(std::iter::once(Entry {
                name: "mine".to_owned(),
                theme: Theme::default(),
                user: true,
            }))
            .collect();
        Picker::new(entries, current, Graphics::braille())
    }

    fn press(p: &mut Picker, code: KeyCode) {
        p.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn type_str(p: &mut Picker, s: &str) {
        for c in s.chars() {
            press(p, KeyCode::Char(c));
        }
    }

    #[test]
    fn starts_on_the_theme_in_use() {
        let p = picker(Some("dracula"));
        assert_eq!(p.selected().name, "dracula");
        assert_eq!(p.current, Some(2));

        // A theme that is not in the list does not move the cursor.
        let p = picker(Some("gone"));
        assert_eq!(p.selected().name, "default");
        assert_eq!(p.current, None);
    }

    #[test]
    fn enter_answers_with_the_name_and_esc_with_nothing() {
        let mut p = picker(None);
        press(&mut p, KeyCode::Down);
        press(&mut p, KeyCode::Enter);
        assert_eq!(p.done, Some(Done::Chose("nord".to_owned())));

        let mut p = picker(None);
        press(&mut p, KeyCode::Esc);
        assert_eq!(p.done, Some(Done::Cancelled));
    }

    #[test]
    fn moving_stops_at_both_ends() {
        let mut p = picker(None);
        press(&mut p, KeyCode::Up);
        assert_eq!(p.selected().name, "default");
        for _ in 0..10 {
            press(&mut p, KeyCode::Down);
        }
        assert_eq!(p.selected().name, "mine");
        press(&mut p, KeyCode::Home);
        assert_eq!(p.selected().name, "default");
        press(&mut p, KeyCode::End);
        assert_eq!(p.selected().name, "mine");
    }

    #[test]
    fn typing_filters_and_keeps_the_selection_when_it_survives() {
        let mut p = picker(None);
        type_str(&mut p, "ra");
        assert_eq!(p.shown.len(), 1);
        assert_eq!(p.selected().name, "dracula");

        // A letter that would leave nothing is ignored, filter and all.
        type_str(&mut p, "zz");
        assert_eq!(p.filter, "ra");
        assert_eq!(p.selected().name, "dracula");

        press(&mut p, KeyCode::Backspace);
        assert_eq!(p.filter, "r");
        assert_eq!(p.selected().name, "dracula", "still on the same theme");

        // Esc clears the filter first, and only then leaves.
        press(&mut p, KeyCode::Esc);
        assert!(p.filter.is_empty());
        assert_eq!(p.shown.len(), 4);
        assert!(p.done.is_none());
        press(&mut p, KeyCode::Esc);
        assert_eq!(p.done, Some(Done::Cancelled));
    }

    #[test]
    fn the_sample_note_exercises_every_block_kind() {
        let mut p = picker(None);
        let theme = Theme::default();
        let lines = p.preview_lines(&theme, 60);
        let text: String = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("notopod"), "{text}");
        assert!(text.contains('☑'), "a done task: {text}");
        assert!(text.contains('│'), "a table: {text}");
        assert!(
            text.chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)),
            "a drawing, as braille here: {text}"
        );

        // Too narrow to be worth it: nothing rather than a mangled note.
        assert!(p.preview_lines(&theme, 4).is_empty());
    }

    #[test]
    fn the_screen_is_the_list_a_divider_and_the_note() {
        let mut p = picker(Some("nord"));
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(64, 14))
            .expect("test backend");
        terminal
            .draw(|frame| p.draw(frame))
            .expect("draw the picker");
        let screen: Vec<String> = terminal
            .backend()
            .buffer()
            .content()
            .chunks(64)
            .map(|row| row.iter().map(ratatui::buffer::Cell::symbol).collect())
            .collect();

        assert!(screen[0].starts_with("   default"), "{:?}", screen[0]);
        assert!(screen[0].contains('│'), "a divider: {:?}", screen[0]);
        assert!(screen[0].contains("notopod"), "the note: {:?}", screen[0]);
        assert!(
            screen[1].starts_with(" • nord"),
            "the theme in use is marked: {:?}",
            screen[1]
        );
        let status = screen.last().expect("a status bar");
        assert!(status.starts_with(" nord   built-in"), "{status:?}");
        assert!(status.trim_end().ends_with("Esc cancel"), "{status:?}");
    }

    #[test]
    fn the_list_leaves_room_for_the_preview() {
        let p = picker(None);
        assert_eq!(p.list_width(80), 11, "the longest name plus a margin");
        assert!(p.list_width(30) <= 30 - PREVIEW_MIN);
        assert!(p.list_width(10) >= 1, "never zero, however narrow");
    }
}
