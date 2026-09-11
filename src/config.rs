//! The user's configuration file and theme lookup.
//!
//! The file is TOML and lives at the first of:
//!
//! 1. `$NOTOPOD_CONFIG`
//! 2. `$XDG_CONFIG_HOME/notopod/config.toml`
//! 3. `~/.config/notopod/config.toml` (`%APPDATA%\notopod\config.toml` on Windows)
//!
//! ```toml
//! theme = "nord"          # a built-in theme, or a file in themes/ next to this file
//!
//! [canvas]
//! roughness = 0.7         # 0.0 exact geometry, 1.0 very sketchy; overrides the theme
//!
//! [graphics]
//! mode = "auto"           # auto | kitty | braille: how drawings are shown
//!
//! [keys]
//! vim = false             # vim keys in the editor
//! ```
//!
//! User themes live in a `themes/` directory next to the config file, one
//! `<name>.toml` each, in the format described by the `theme` crate.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use theme::Theme;

/// Everything the config file can say. Missing keys keep their defaults.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Theme name or path.
    pub theme: Option<String>,
    /// Drawing options.
    #[serde(default)]
    pub canvas: Canvas,
    /// How drawings reach the screen.
    #[serde(default)]
    pub graphics: GraphicsSection,
    /// Keyboard options.
    #[serde(default)]
    pub keys: Keys,
}

/// The `[keys]` section.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keys {
    /// Vim keys in the editor.
    pub vim: Option<bool>,
}

/// The `[graphics]` section.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphicsSection {
    /// `auto`, `kitty` or `braille`.
    pub mode: Option<String>,
}

/// The `[canvas]` section.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Canvas {
    /// Overrides the theme's roughness.
    pub roughness: Option<f32>,
}

/// Where the config file is (or would be), whether or not it exists.
pub fn config_path() -> Option<PathBuf> {
    if let Some(p) = env::var_os("NOTOPOD_CONFIG") {
        return Some(PathBuf::from(p));
    }
    config_dir().map(|d| d.join("config.toml"))
}

/// The directory user themes are read from.
pub fn themes_dir() -> Option<PathBuf> {
    config_path().map(|p| p.with_file_name("themes"))
}

fn config_dir() -> Option<PathBuf> {
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(xdg).join("notopod"));
    }
    if cfg!(windows) {
        if let Some(appdata) = env::var_os("APPDATA") {
            return Some(PathBuf::from(appdata).join("notopod"));
        }
    }
    env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("notopod"))
}

/// Reads the config file. A missing file is an empty config; a file that
/// does not parse is an error, so typos are not silently ignored.
pub fn load() -> Result<Config> {
    let Some(path) = config_path() else {
        return Ok(Config::default());
    };
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
    };
    toml::from_str(&text).with_context(|| format!("{}", path.display()))
}

/// Resolves the theme to use: `flag` (from `--theme`) wins over the config
/// file, which wins over `default`. A name is looked up among the built-in
/// themes, then as `<name>.toml` in the user themes directory; anything
/// containing a path separator or ending in `.toml` is read as a file.
pub fn resolve_theme(flag: Option<&str>, config: &Config) -> Result<Theme> {
    let name = flag.or(config.theme.as_deref()).unwrap_or("default");
    let mut theme = lookup_theme(name)?;
    if let Some(r) = config.canvas.roughness {
        if !(0.0..=1.0).contains(&r) {
            bail!("config: canvas.roughness = {r} is outside 0.0..=1.0");
        }
        theme.roughness = r;
    }
    Ok(theme)
}

/// Resolves how drawings are shown: `flag` (from `--graphics`) wins over
/// the config file, which wins over `auto`.
pub fn resolve_graphics(flag: Option<graphics::Mode>, config: &Config) -> Result<graphics::Mode> {
    if let Some(mode) = flag {
        return Ok(mode);
    }
    match config.graphics.mode.as_deref() {
        Some(name) => name
            .parse()
            .map_err(|e: String| anyhow!("config: graphics.mode: {e}")),
        None => Ok(graphics::Mode::Auto),
    }
}

/// Whether vim keys are on: `--vim` wins, then `[keys] vim`, then off.
pub fn resolve_vim(flag: bool, config: &Config) -> bool {
    flag || config.keys.vim.unwrap_or(false)
}

/// Loads one theme by name or path, the same way [`resolve_theme`] does.
pub fn load_theme(name: &str) -> Result<Theme> {
    lookup_theme(name)
}

/// Writes `theme = "<name>"` into the config file, creating it if it is
/// not there, and returns the file it wrote. Everything else in the file —
/// comments, spacing, other settings — is left as it was.
pub fn set_theme(name: &str) -> Result<PathBuf> {
    let path = config_path().ok_or_else(|| {
        anyhow!("nowhere to write the config file: no $NOTOPOD_CONFIG and no HOME")
    })?;
    let old = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    fs::write(&path, with_theme(&old, name))
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path)
}

/// `src` with its top-level `theme` key set to `name`: the existing key is
/// replaced where it stands, or a new one goes in above the first section
/// header, which is where top-level keys have to live in TOML.
///
/// This edits text rather than reformatting the file through a TOML
/// writer, so a hand-written config keeps its comments and its shape. The
/// one thing it cannot see is a `theme =` inside a multi-line string.
fn with_theme(src: &str, name: &str) -> String {
    let assignment = format!("theme = {}", quote(name));
    let mut out: Vec<String> = Vec::new();
    let mut top_level = true;
    let mut placed = false;

    for line in src.lines() {
        let trimmed = line.trim_start();
        if top_level && trimmed.starts_with('[') {
            top_level = false;
            if !placed {
                out.push(assignment.clone());
                out.push(String::new());
                placed = true;
            }
        }
        if top_level && is_theme_key(trimmed) {
            // The first one becomes the new setting; a repeat is dropped,
            // since TOML would reject the file with both.
            if !placed {
                out.push(assignment.clone());
                placed = true;
            }
            continue;
        }
        out.push(line.to_owned());
    }
    if !placed {
        out.push(assignment);
    }

    let mut text = out.join("\n");
    text.push('\n');
    text
}

fn is_theme_key(line: &str) -> bool {
    ["theme", "\"theme\"", "'theme'"]
        .iter()
        .find_map(|key| line.strip_prefix(key))
        .is_some_and(|rest| rest.trim_start().starts_with('='))
}

fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn lookup_theme(name: &str) -> Result<Theme> {
    if let Some(t) = Theme::builtin(name) {
        return Ok(t);
    }
    let looks_like_path = Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        || name.contains(['/', '\\']);
    let path = if looks_like_path {
        PathBuf::from(name)
    } else if let Some(dir) = themes_dir() {
        dir.join(format!("{name}.toml"))
    } else {
        return Err(unknown_theme(name));
    };
    match fs::read_to_string(&path) {
        Ok(src) => Theme::from_toml(&src).map_err(|e| anyhow!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !looks_like_path => {
            Err(unknown_theme(name))
        }
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

fn unknown_theme(name: &str) -> anyhow::Error {
    anyhow!(
        "unknown theme {name:?}. Built-in themes: {}. `notopod themes` shows them all.",
        theme::BUILTIN_NAMES.join(", ")
    )
}

/// Names of the user's themes: every `*.toml` in the themes directory.
pub fn user_theme_names() -> Vec<String> {
    let Some(dir) = themes_dir() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
}

/// A short description of where configuration is read from, for `--help`
/// style output.
pub fn describe_paths() -> String {
    let show = |p: Option<PathBuf>| {
        p.map_or_else(
            || "(unknown: no HOME)".to_owned(),
            |p: PathBuf| Path::new(&p).display().to_string(),
        )
    };
    format!(
        "config: {}\nthemes: {}",
        show(config_path()),
        show(themes_dir())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_config_is_just_the_theme() {
        assert_eq!(with_theme("", "nord"), "theme = \"nord\"\n");
    }

    #[test]
    fn an_existing_setting_is_replaced_where_it_stands() {
        let src = "# my notes\ntheme = \"mono\"\n\n[canvas]\nroughness = 0.7\n";
        assert_eq!(
            with_theme(src, "nord"),
            "# my notes\ntheme = \"nord\"\n\n[canvas]\nroughness = 0.7\n"
        );
    }

    #[test]
    fn a_new_setting_goes_in_above_the_first_section() {
        let src = "[canvas]\nroughness = 0.7\n";
        assert_eq!(
            with_theme(src, "nord"),
            "theme = \"nord\"\n\n[canvas]\nroughness = 0.7\n"
        );
    }

    #[test]
    fn a_theme_key_inside_a_section_is_left_alone() {
        let src = "[styles]\ntheme = \"not this one\"\n";
        assert_eq!(
            with_theme(src, "nord"),
            "theme = \"nord\"\n\n[styles]\ntheme = \"not this one\"\n"
        );
    }

    #[test]
    fn repeated_keys_collapse_and_odd_names_survive_the_trip() {
        assert_eq!(
            with_theme("theme = \"a\"\ntheme = \"b\"\n", "nord"),
            "theme = \"nord\"\n"
        );
        let written = with_theme("", "say \"hi\"\\");
        assert_eq!(written, "theme = \"say \\\"hi\\\"\\\\\"\n");
        let parsed: Config = toml::from_str(&written).expect("still valid TOML");
        assert_eq!(parsed.theme.as_deref(), Some("say \"hi\"\\"));
    }

    #[test]
    fn everything_else_in_the_file_is_kept() {
        let src = "\
# written by hand
theme = 'mono'   # the old one

[canvas]
roughness = 0.4

[graphics]
mode = \"braille\"
";
        let out = with_theme(src, "nord");
        assert!(out.contains("# written by hand"));
        assert!(out.contains("theme = \"nord\""));
        assert!(!out.contains("mono"));
        assert!(out.contains("roughness = 0.4"));
        assert!(out.contains("mode = \"braille\""));
        toml::from_str::<Config>(&out).expect("still valid TOML");
    }
}
