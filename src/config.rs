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
        "unknown theme {name:?}. Built-in themes: {}. `notopod themes` lists them all.",
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
