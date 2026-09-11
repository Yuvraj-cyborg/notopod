//! Colours and text attributes used when rendering.

use ratatui::style::{Color, Modifier, Style};

/// Every style the renderer uses. Build your own or start from
/// [`Theme::default`].
///
/// Colours are ANSI named colours so they follow the terminal's palette
/// and look right on both light and dark backgrounds.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Heading text, one entry per level (index 0 is `#`).
    pub heading: [Style; 6],
    /// The `#` marks in front of a heading.
    pub heading_marker: Style,
    /// Body text.
    pub text: Style,
    /// `*emphasis*`, patched over the surrounding style.
    pub emphasis: Style,
    /// `**strong**`, patched over the surrounding style.
    pub strong: Style,
    /// `~~strikethrough~~`, patched over the surrounding style.
    pub strikethrough: Style,
    /// Inline `` `code` ``.
    pub code: Style,
    /// Text inside a fenced code block.
    pub code_block: Style,
    /// The box drawn around a code block.
    pub code_border: Style,
    /// The language label on a code block.
    pub code_lang: Style,
    /// Link text.
    pub link: Style,
    /// The `[image: alt]` placeholder.
    pub image: Style,
    /// The bar in front of a block quote.
    pub quote_bar: Style,
    /// Quoted text, patched over the surrounding style.
    pub quote_text: Style,
    /// List bullets and numbers.
    pub bullet: Style,
    /// The check box of a finished task.
    pub task_done: Style,
    /// Text of a finished task, patched over the surrounding style.
    pub task_done_text: Style,
    /// The check box of an open task.
    pub task_todo: Style,
    /// Horizontal rules.
    pub rule: Style,
    /// Table borders.
    pub table_border: Style,
    /// Table header cells.
    pub table_header: Style,
    /// Raw HTML.
    pub html: Style,
    /// YAML front matter.
    pub metadata: Style,
}

impl Default for Theme {
    fn default() -> Self {
        let bold = Style::new().add_modifier(Modifier::BOLD);
        let dim = Style::new().add_modifier(Modifier::DIM);
        Self {
            heading: [
                bold.fg(Color::Yellow).add_modifier(Modifier::UNDERLINED),
                bold.fg(Color::Yellow),
                bold.fg(Color::Cyan),
                bold.fg(Color::Green),
                bold.fg(Color::Blue),
                bold.fg(Color::Magenta),
            ],
            heading_marker: dim,
            text: Style::new(),
            emphasis: Style::new().add_modifier(Modifier::ITALIC),
            strong: bold,
            strikethrough: Style::new().add_modifier(Modifier::CROSSED_OUT),
            code: Style::new().fg(Color::Red),
            code_block: Style::new(),
            code_border: dim,
            code_lang: dim.add_modifier(Modifier::ITALIC),
            link: Style::new()
                .fg(Color::Blue)
                .add_modifier(Modifier::UNDERLINED),
            image: dim.add_modifier(Modifier::ITALIC),
            quote_bar: Style::new().fg(Color::Magenta),
            quote_text: Style::new().add_modifier(Modifier::ITALIC),
            bullet: Style::new().fg(Color::Blue),
            task_done: Style::new().fg(Color::Green),
            task_done_text: dim.add_modifier(Modifier::CROSSED_OUT),
            task_todo: Style::new().fg(Color::Blue),
            rule: dim,
            table_border: dim,
            table_header: bold,
            html: dim,
            metadata: dim,
        }
    }
}
