//! Colour themes.
//!
//! A [`Theme`] holds every style notopod draws with: text elements, the
//! status bar, and the drawing canvas. Styles are derived from a nine
//! colour [`Palette`], so a theme file is usually just a palette:
//!
//! ```toml
//! name = "nord"
//!
//! [colors]
//! fg = "#d8dee9"
//! muted = "#4c566a"
//! red = "#bf616a"
//! # ... orange, yellow, green, cyan, blue, magenta
//!
//! [styles]              # optional, overrides the derived styles
//! heading1 = "yellow bold underline"
//!
//! [canvas]              # optional
//! roughness = 0.7
//! ```
//!
//! Style strings are space separated tokens: a colour (a palette name,
//! an ANSI name such as `red` or `lightblue`, `#rrggbb`, or a 0-255
//! index), `bg:<colour>`, and the modifiers `bold`, `dim`, `italic`,
//! `underline`, `strike`, `reverse`.
//!
//! Built-in themes ship as TOML files inside this crate, so they are
//! parsed by the same code as user themes: see [`Theme::builtin`] and
//! [`BUILTIN_NAMES`].
//!
//! ```
//! use theme::Theme;
//!
//! let nord = Theme::builtin("nord").unwrap();
//! assert_eq!(nord.name, "nord");
//! assert!(Theme::builtin("no-such-theme").is_none());
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

/// Names of the themes compiled into the binary, in display order.
pub const BUILTIN_NAMES: [&str; 9] = [
    "default",
    "mono",
    "catppuccin-mocha",
    "gruvbox-dark",
    "nord",
    "tokyo-night",
    "dracula",
    "solarized-dark",
    "solarized-light",
];

const BUILTIN_FILES: [(&str, &str); 9] = [
    ("default", include_str!("../themes/default.toml")),
    ("mono", include_str!("../themes/mono.toml")),
    (
        "catppuccin-mocha",
        include_str!("../themes/catppuccin-mocha.toml"),
    ),
    ("gruvbox-dark", include_str!("../themes/gruvbox-dark.toml")),
    ("nord", include_str!("../themes/nord.toml")),
    ("tokyo-night", include_str!("../themes/tokyo-night.toml")),
    ("dracula", include_str!("../themes/dracula.toml")),
    (
        "solarized-dark",
        include_str!("../themes/solarized-dark.toml"),
    ),
    (
        "solarized-light",
        include_str!("../themes/solarized-light.toml"),
    ),
];

/// The nine colours a theme is built from.
///
/// `Color::Reset` means "no colour": the terminal's own foreground. The
/// `default` theme uses ANSI colour names so it follows the terminal's
/// palette; the other built-in themes use exact RGB values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Body text.
    pub fg: Color,
    /// Borders, markers, metadata. `Reset` here means "use the dim attribute".
    pub muted: Color,
    /// Red accent.
    pub red: Color,
    /// Orange accent.
    pub orange: Color,
    /// Yellow accent.
    pub yellow: Color,
    /// Green accent.
    pub green: Color,
    /// Cyan accent.
    pub cyan: Color,
    /// Blue accent.
    pub blue: Color,
    /// Magenta accent.
    pub magenta: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            fg: Color::Reset,
            muted: Color::Reset,
            red: Color::Red,
            orange: Color::LightRed,
            yellow: Color::Yellow,
            green: Color::Green,
            cyan: Color::Cyan,
            blue: Color::Blue,
            magenta: Color::Magenta,
        }
    }
}

impl Palette {
    /// Looks a colour up by its palette name.
    pub fn get(&self, name: &str) -> Option<Color> {
        Some(match name {
            "fg" => self.fg,
            "muted" => self.muted,
            "red" => self.red,
            "orange" => self.orange,
            "yellow" => self.yellow,
            "green" => self.green,
            "cyan" => self.cyan,
            "blue" => self.blue,
            "magenta" => self.magenta,
            _ => return None,
        })
    }

    /// The palette names, in a fixed order.
    pub const NAMES: [&'static str; 9] = [
        "fg", "muted", "red", "orange", "yellow", "green", "cyan", "blue", "magenta",
    ];
}

/// Every style notopod uses. Build one with [`Theme::builtin`],
/// [`Theme::from_toml`] or [`Theme::from_palette`].
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// The theme's name.
    pub name: String,
    /// The colours the styles were derived from. Drawings refer to these
    /// by name (`color=red`).
    pub palette: Palette,

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

    /// The status bar at the bottom of the screen.
    pub status_bar: Style,

    /// Default stroke of shapes in a drawing.
    pub canvas_stroke: Style,
    /// Labels and free text in a drawing.
    pub canvas_text: Style,
    /// The cell under the cursor while drawing.
    pub canvas_cursor: Style,
    /// The shape under the cursor while drawing.
    pub canvas_selected: Style,
    /// A shape that is being placed and not committed yet.
    pub canvas_preview: Style,
    /// How hand-drawn strokes look: 0 is exact geometry, 1 is very sketchy.
    pub roughness: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_palette("default", Palette::default())
    }
}

impl Theme {
    /// Derives every style from `palette`. This is the base every theme
    /// file starts from before its `[styles]` overrides are applied.
    pub fn from_palette(name: &str, palette: Palette) -> Self {
        let p = palette;
        let bold = Style::new().add_modifier(Modifier::BOLD);
        let muted = if p.muted == Color::Reset {
            Style::new().add_modifier(Modifier::DIM)
        } else {
            Style::new().fg(p.muted)
        };
        let text = fg(p.fg);
        Self {
            name: name.to_owned(),
            palette,
            heading: [
                bold.fg(p.yellow).add_modifier(Modifier::UNDERLINED),
                bold.fg(p.yellow),
                bold.fg(p.cyan),
                bold.fg(p.green),
                bold.fg(p.blue),
                bold.fg(p.magenta),
            ],
            heading_marker: muted,
            text,
            emphasis: Style::new().add_modifier(Modifier::ITALIC),
            strong: bold,
            strikethrough: Style::new().add_modifier(Modifier::CROSSED_OUT),
            code: fg(p.red),
            code_block: text,
            code_border: muted,
            code_lang: muted.add_modifier(Modifier::ITALIC),
            link: fg(p.blue).add_modifier(Modifier::UNDERLINED),
            image: muted.add_modifier(Modifier::ITALIC),
            quote_bar: fg(p.magenta),
            quote_text: Style::new().add_modifier(Modifier::ITALIC),
            bullet: fg(p.blue),
            task_done: fg(p.green),
            task_done_text: muted.add_modifier(Modifier::CROSSED_OUT),
            task_todo: fg(p.blue),
            rule: muted,
            table_border: muted,
            table_header: bold,
            html: muted,
            metadata: muted,
            status_bar: Style::new().add_modifier(Modifier::REVERSED),
            canvas_stroke: text,
            canvas_text: text,
            canvas_cursor: if p.muted == Color::Reset {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::new().bg(p.muted)
            },
            canvas_selected: fg(p.yellow),
            canvas_preview: fg(p.cyan),
            roughness: 0.7,
        }
    }

    /// A theme compiled into the binary, by name. See [`BUILTIN_NAMES`].
    pub fn builtin(name: &str) -> Option<Self> {
        let (_, src) = BUILTIN_FILES.iter().find(|(n, _)| *n == name)?;
        Some(
            Self::from_toml(src)
                .unwrap_or_else(|e| panic!("built-in theme {name} does not parse: {e}")),
        )
    }

    /// The TOML source of a built-in theme, handy as a starting point for
    /// a custom one.
    pub fn builtin_source(name: &str) -> Option<&'static str> {
        BUILTIN_FILES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, src)| *src)
    }

    /// Parses a theme file. See the crate docs for the format.
    pub fn from_toml(src: &str) -> Result<Self, Error> {
        let file: ThemeFile = toml::from_str(src).map_err(|e| Error(e.to_string()))?;
        let mut palette = Palette::default();
        for (key, value) in &file.colors {
            let color = parse_color(value, None)
                .ok_or_else(|| Error(format!("colors.{key}: {value:?} is not a colour")))?;
            match key.as_str() {
                "fg" => palette.fg = color,
                "muted" => palette.muted = color,
                "red" => palette.red = color,
                "orange" => palette.orange = color,
                "yellow" => palette.yellow = color,
                "green" => palette.green = color,
                "cyan" => palette.cyan = color,
                "blue" => palette.blue = color,
                "magenta" => palette.magenta = color,
                _ => {
                    return Err(Error(format!(
                        "colors.{key}: unknown palette colour (expected one of {})",
                        Palette::NAMES.join(", ")
                    )))
                }
            }
        }
        let mut theme = Self::from_palette(file.name.as_deref().unwrap_or("custom"), palette);
        for (key, value) in &file.styles {
            let style = parse_style(value, &palette)
                .ok_or_else(|| Error(format!("styles.{key}: cannot parse {value:?}")))?;
            let slot = theme
                .style_mut(key)
                .ok_or_else(|| Error(format!("styles.{key}: unknown element")))?;
            *slot = style;
        }
        if let Some(r) = file.canvas.roughness {
            if !(0.0..=1.0).contains(&r) {
                return Err(Error(format!("canvas.roughness: {r} is outside 0.0..=1.0")));
            }
            theme.roughness = r;
        }
        Ok(theme)
    }

    /// Names of every element that `[styles]` can set.
    pub const STYLE_NAMES: [&'static str; 34] = [
        "heading1",
        "heading2",
        "heading3",
        "heading4",
        "heading5",
        "heading6",
        "heading_marker",
        "text",
        "emphasis",
        "strong",
        "strikethrough",
        "code",
        "code_block",
        "code_border",
        "code_lang",
        "link",
        "image",
        "quote_bar",
        "quote_text",
        "bullet",
        "task_done",
        "task_done_text",
        "task_todo",
        "rule",
        "table_border",
        "table_header",
        "html",
        "metadata",
        "status_bar",
        "canvas_stroke",
        "canvas_text",
        "canvas_cursor",
        "canvas_selected",
        "canvas_preview",
    ];

    /// Mutable access to a style by its `[styles]` name.
    fn style_mut(&mut self, name: &str) -> Option<&mut Style> {
        Some(match name {
            "heading1" => &mut self.heading[0],
            "heading2" => &mut self.heading[1],
            "heading3" => &mut self.heading[2],
            "heading4" => &mut self.heading[3],
            "heading5" => &mut self.heading[4],
            "heading6" => &mut self.heading[5],
            "heading_marker" => &mut self.heading_marker,
            "text" => &mut self.text,
            "emphasis" => &mut self.emphasis,
            "strong" => &mut self.strong,
            "strikethrough" => &mut self.strikethrough,
            "code" => &mut self.code,
            "code_block" => &mut self.code_block,
            "code_border" => &mut self.code_border,
            "code_lang" => &mut self.code_lang,
            "link" => &mut self.link,
            "image" => &mut self.image,
            "quote_bar" => &mut self.quote_bar,
            "quote_text" => &mut self.quote_text,
            "bullet" => &mut self.bullet,
            "task_done" => &mut self.task_done,
            "task_done_text" => &mut self.task_done_text,
            "task_todo" => &mut self.task_todo,
            "rule" => &mut self.rule,
            "table_border" => &mut self.table_border,
            "table_header" => &mut self.table_header,
            "html" => &mut self.html,
            "metadata" => &mut self.metadata,
            "status_bar" => &mut self.status_bar,
            "canvas_stroke" => &mut self.canvas_stroke,
            "canvas_text" => &mut self.canvas_text,
            "canvas_cursor" => &mut self.canvas_cursor,
            "canvas_selected" => &mut self.canvas_selected,
            "canvas_preview" => &mut self.canvas_preview,
            _ => return None,
        })
    }

    /// The colour a drawing means by `name`: a palette name, an ANSI
    /// name, `#rrggbb` or an index.
    pub fn color(&self, name: &str) -> Option<Color> {
        parse_color(name, Some(&self.palette))
    }
}

/// A style with `color` as foreground, or no foreground for `Reset`.
fn fg(color: Color) -> Style {
    if color == Color::Reset {
        Style::new()
    } else {
        Style::new().fg(color)
    }
}

/// An sRGB colour, as drawn into an image.
pub type Rgb = (u8, u8, u8);

/// The RGB value `color` is painted with when a drawing is rendered as an
/// image rather than as text.
///
/// `Reset` has no value of its own and becomes `fallback` (the terminal's
/// foreground, when known). The sixteen ANSI names use the usual xterm
/// defaults, indexed colours the xterm 256-colour table.
pub fn to_rgb(color: Color, fallback: Rgb) -> Rgb {
    match color {
        Color::Reset => fallback,
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0x00, 0x00, 0x00),
        Color::Red => (0xcd, 0x31, 0x31),
        Color::Green => (0x0d, 0xbc, 0x79),
        Color::Yellow => (0xe5, 0xe5, 0x10),
        Color::Blue => (0x24, 0x72, 0xc8),
        Color::Magenta => (0xbc, 0x3f, 0xbc),
        Color::Cyan => (0x11, 0xa8, 0xcd),
        Color::Gray => (0xe5, 0xe5, 0xe5),
        Color::DarkGray => (0x66, 0x66, 0x66),
        Color::LightRed => (0xf1, 0x4c, 0x4c),
        Color::LightGreen => (0x23, 0xd1, 0x8b),
        Color::LightYellow => (0xf5, 0xf5, 0x43),
        Color::LightBlue => (0x3b, 0x8e, 0xea),
        Color::LightMagenta => (0xd6, 0x70, 0xd6),
        Color::LightCyan => (0x29, 0xb8, 0xdb),
        Color::White => (0xff, 0xff, 0xff),
        Color::Indexed(i) => indexed_rgb(i, fallback),
    }
}

fn indexed_rgb(i: u8, fallback: Rgb) -> Rgb {
    const ANSI: [Color; 16] = [
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::Gray,
        Color::DarkGray,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
        Color::White,
    ];
    match i {
        0..=15 => to_rgb(ANSI[usize::from(i)], fallback),
        16..=231 => {
            let n = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            (level(n / 36), level((n / 6) % 6), level(n % 6))
        }
        232..=255 => {
            let v = 8 + 10 * (i - 232);
            (v, v, v)
        }
    }
}

/// The on-disk shape of a theme file.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    name: Option<String>,
    #[serde(default)]
    colors: BTreeMap<String, String>,
    #[serde(default)]
    styles: BTreeMap<String, String>,
    #[serde(default)]
    canvas: CanvasSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CanvasSection {
    roughness: Option<f32>,
}

/// A theme file that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Parses one colour: a palette name (when a palette is given), an ANSI
/// name, `#rrggbb`, or an index 0-255.
pub fn parse_color(s: &str, palette: Option<&Palette>) -> Option<Color> {
    let s = s.trim();
    if let Some(c) = palette.and_then(|p| p.get(s)) {
        return Some(c);
    }
    if s.eq_ignore_ascii_case("none") {
        return Some(Color::Reset);
    }
    Color::from_str(s).ok()
}

/// Parses a style string: colour and modifier tokens separated by spaces.
pub fn parse_style(s: &str, palette: &Palette) -> Option<Style> {
    let mut style = Style::new();
    for token in s.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        let modifier = match lower.as_str() {
            "bold" => Some(Modifier::BOLD),
            "dim" => Some(Modifier::DIM),
            "italic" => Some(Modifier::ITALIC),
            "underline" | "underlined" => Some(Modifier::UNDERLINED),
            "strike" | "strikethrough" | "crossed" => Some(Modifier::CROSSED_OUT),
            "reverse" | "reversed" => Some(Modifier::REVERSED),
            _ => None,
        };
        if let Some(m) = modifier {
            style = style.add_modifier(m);
        } else if lower == "plain" {
            // Explicitly "no styling"; useful as the whole value.
        } else if let Some(bg) = lower.strip_prefix("bg:") {
            let color = parse_color(bg, Some(palette))?;
            if color != Color::Reset {
                style = style.bg(color);
            }
        } else {
            let color = parse_color(token, Some(palette))?;
            if color != Color::Reset {
                style = style.fg(color);
            }
        }
    }
    Some(style)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_parses_and_has_its_name() {
        for name in BUILTIN_NAMES {
            let t = Theme::builtin(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(t.name, name, "name inside {name}.toml must match");
        }
        assert_eq!(BUILTIN_NAMES.len(), BUILTIN_FILES.len());
    }

    #[test]
    fn default_theme_file_matches_the_code_default() {
        assert_eq!(Theme::builtin("default").unwrap(), Theme::default());
    }

    #[test]
    fn every_style_name_is_settable() {
        let mut t = Theme::default();
        for name in Theme::STYLE_NAMES {
            assert!(t.style_mut(name).is_some(), "{name} is not settable");
        }
    }

    #[test]
    fn style_strings() {
        let p = Palette::default();
        assert_eq!(
            parse_style("bold red", &p),
            Some(Style::new().fg(Color::Red).add_modifier(Modifier::BOLD))
        );
        assert_eq!(
            parse_style("#ff8800 underline bg:blue", &p),
            Some(
                Style::new()
                    .fg(Color::Rgb(0xff, 0x88, 0))
                    .bg(Color::Blue)
                    .add_modifier(Modifier::UNDERLINED)
            )
        );
        assert_eq!(parse_style("", &p), Some(Style::new()));
        assert_eq!(parse_style("nonsense", &p), None);
    }

    #[test]
    fn theme_file_overrides_and_errors() {
        let t = Theme::from_toml(
            "name = \"t\"\n[colors]\nred = \"#112233\"\n[styles]\nheading1 = \"red italic\"\n[canvas]\nroughness = 0.2\n",
        )
        .unwrap();
        assert_eq!(t.palette.red, Color::Rgb(0x11, 0x22, 0x33));
        assert_eq!(
            t.heading[0],
            Style::new()
                .fg(Color::Rgb(0x11, 0x22, 0x33))
                .add_modifier(Modifier::ITALIC)
        );
        assert_eq!(t.code, Style::new().fg(Color::Rgb(0x11, 0x22, 0x33)));
        assert!((t.roughness - 0.2).abs() < f32::EPSILON);

        assert!(Theme::from_toml("[colors]\npurple = \"#000\"\n").is_err());
        assert!(Theme::from_toml("[styles]\nnope = \"red\"\n").is_err());
        assert!(Theme::from_toml("[canvas]\nroughness = 3\n").is_err());
        assert!(Theme::from_toml("[typo]\n").is_err());
    }

    #[test]
    fn colors_resolve_palette_then_literals() {
        let t = Theme::builtin("nord").unwrap();
        assert_eq!(t.color("red"), Some(t.palette.red));
        assert_eq!(t.color("#010203"), Some(Color::Rgb(1, 2, 3)));
        assert_eq!(t.color("lightblue"), Some(Color::LightBlue));
        assert_eq!(t.color("none"), Some(Color::Reset));
        assert_eq!(t.color("bogus"), None);
    }
}
