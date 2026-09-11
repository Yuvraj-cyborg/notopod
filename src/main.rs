//! The `notopod` command.

mod config;

use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

/// A terminal notes editor with live Markdown preview.
#[derive(Parser)]
#[command(name = "notopod", version, about)]
struct Cli {
    /// Notes to open, one tab each. A note that does not exist yet is
    /// created on first save.
    files: Vec<PathBuf>,

    /// Theme to use: a built-in name, a user theme, or a .toml file.
    /// Overrides the config file. `notopod themes` shows what there is.
    #[arg(long, short = 't', global = true, value_name = "NAME")]
    theme: Option<String>,

    /// How to show drawings: `auto` asks the terminal and uses pictures
    /// (kitty graphics protocol: kitty, Ghostty, WezTerm, Konsole) when it
    /// can, `kitty` forces pictures, `braille` draws dot art everywhere.
    /// Overrides the config file.
    #[arg(long, short = 'g', global = true, value_name = "MODE")]
    graphics: Option<graphics::Mode>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Open notes in the editor (same as `notopod FILE...`).
    Edit {
        /// Notes to open, one tab each.
        files: Vec<PathBuf>,
    },
    /// Print a note, formatted, and exit.
    Render {
        /// Note to print.
        file: PathBuf,
        /// Wrap at this many columns instead of the terminal width.
        #[arg(long)]
        width: Option<u16>,
        /// Plain text, no colours.
        #[arg(long)]
        no_color: bool,
    },
    /// Draw the graph of the notes in a folder and the links between them.
    Graph {
        /// Folder to scan. The working directory when left out.
        dir: Option<PathBuf>,
        /// Draw at this many columns instead of the terminal width.
        #[arg(long)]
        width: Option<u16>,
        /// Plain text, no colours.
        #[arg(long)]
        no_color: bool,
    },
    /// Pick a theme, list them, or print one as a TOML file to start your
    /// own from.
    Themes {
        /// Print this theme's TOML (built-in themes only).
        show: Option<String>,
        /// Print the names and exit, instead of opening the picker.
        #[arg(long)]
        list: bool,
        /// Save this theme to the config file without opening the picker.
        #[arg(long, value_name = "NAME")]
        set: Option<String>,
    },
    /// Show where the config file and user themes are read from.
    Config,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = config::load()?;
    let theme = || config::resolve_theme(cli.theme.as_deref(), &config);
    let graphics = || config::resolve_graphics(cli.graphics, &config);

    match cli.command {
        None => tui::run(&as_paths(&cli.files), theme()?, graphics()?),
        Some(Command::Edit { files }) => tui::run(&as_paths(&files), theme()?, graphics()?),
        Some(Command::Render {
            file,
            width,
            no_color,
        }) => render_note(&file, width, no_color, &theme()?, graphics()?),
        Some(Command::Graph {
            dir,
            width,
            no_color,
        }) => graph(dir.as_deref(), width, no_color, &theme()?, graphics()?),
        Some(Command::Themes { show, list, set }) => {
            themes(show.as_deref(), list, set.as_deref(), &config, graphics()?)
        }
        Some(Command::Config) => {
            println!("{}", config::describe_paths());
            Ok(())
        }
    }
}

fn as_paths(files: &[PathBuf]) -> Vec<&std::path::Path> {
    files.iter().map(PathBuf::as_path).collect()
}

fn render_note(
    file: &PathBuf,
    width: Option<u16>,
    no_color: bool,
    theme: &theme::Theme,
    graphics: graphics::Mode,
) -> Result<()> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    let (width, color, mut drawings) = output_setup(width, no_color, graphics);
    let doc = syntax::parse(&text);
    let lines = render::render_document_with(&doc, width, theme, &mut drawings);
    let mut out = std::io::stdout().lock();
    out.write_all(drawings.take_pending().as_bytes())?;
    out.write_all(render::ansi::to_ansi(&lines, color).as_bytes())?;
    out.flush()?;
    Ok(())
}

/// `notopod graph [DIR]`: the notes under a folder and the links between
/// them, as one picture.
fn graph(
    dir: Option<&std::path::Path>,
    width: Option<u16>,
    no_color: bool,
    theme: &theme::Theme,
    graphics: graphics::Mode,
) -> Result<()> {
    use render::Drawings;

    let dir = dir.map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf);
    let root =
        std::fs::canonicalize(&dir).with_context(|| format!("cannot read {}", dir.display()))?;
    let graph = notes::scan(&root);
    let (width, color, mut drawings) = output_setup(width, no_color, graphics);
    let drawing = graph.drawing(width);
    let lines = drawings.draw(&drawing, theme, width, None);
    let mut out = std::io::stdout().lock();
    writeln!(
        out,
        "{} notes, {} links under {}{}",
        graph.nodes.len(),
        graph.edges.len(),
        root.display(),
        if graph.truncated {
            format!(" (first {} only)", notes::MAX_NOTES)
        } else {
            String::new()
        }
    )?;
    out.write_all(drawings.take_pending().as_bytes())?;
    out.write_all(render::ansi::to_ansi(&lines, color).as_bytes())?;
    out.flush()?;
    Ok(())
}

/// What printing to stdout needs to know: how wide, whether to colour,
/// and how to show drawings. Pictures ride on colour codes, so no colour
/// means no pictures; the terminal is asked in raw mode so its answer
/// does not echo.
fn output_setup(
    width: Option<u16>,
    no_color: bool,
    graphics: graphics::Mode,
) -> (usize, bool, graphics::Graphics) {
    let stdout = std::io::stdout();
    let width = width
        .or_else(|| crossterm::terminal::size().ok().map(|(w, _)| w))
        .unwrap_or(80)
        .max(20);
    let color = !no_color && stdout.is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let drawings = if color {
        let raw = crossterm::terminal::enable_raw_mode().is_ok();
        let g = graphics::Graphics::detect(graphics);
        if raw {
            let _ = crossterm::terminal::disable_raw_mode();
        }
        g
    } else {
        graphics::Graphics::braille()
    };
    (usize::from(width), color, drawings)
}

/// `notopod themes`: pick one, print the list, print one theme's source,
/// or save a choice straight to the config file.
fn themes(
    show: Option<&str>,
    list: bool,
    set: Option<&str>,
    config: &config::Config,
    graphics: graphics::Mode,
) -> Result<()> {
    if let Some(name) = set {
        // Look it up first, so a typo is an error instead of a config file
        // that no longer loads.
        config::load_theme(name)?;
        return save_theme(name);
    }
    if let Some(name) = show {
        let src = theme::Theme::builtin_source(name).with_context(|| {
            format!(
                "no built-in theme {name:?}; built-in themes are {}",
                theme::BUILTIN_NAMES.join(", ")
            )
        })?;
        print!("{src}");
        return Ok(());
    }
    // The picker needs a terminal at both ends; a pipe gets the list.
    if !list && std::io::stdout().is_terminal() && std::io::stdin().is_terminal() {
        let chosen = tui::pick_theme(theme_entries(config), config.theme.as_deref(), graphics)?;
        return match chosen {
            Some(name) => save_theme(&name),
            None => Ok(()),
        };
    }
    list_themes();
    Ok(())
}

/// Every theme the picker can offer: the built-in ones in their usual
/// order, then the user's. A theme file that does not parse is skipped
/// rather than fatal; the others are still worth showing.
fn theme_entries(config: &config::Config) -> Vec<tui::ThemeEntry> {
    let roughness = config.canvas.roughness;
    let mut entries: Vec<tui::ThemeEntry> = theme::BUILTIN_NAMES
        .iter()
        .filter_map(|name| {
            Some(tui::ThemeEntry {
                name: (*name).to_owned(),
                theme: theme::Theme::builtin(name)?,
                user: false,
            })
        })
        .collect();
    for name in config::user_theme_names() {
        if let Ok(mut theme) = config::load_theme(&name) {
            // The file's own `name` may be anything, including another
            // theme's; the file stem is what the config file will say, and
            // what keeps two themes' pictures apart in the image cache.
            theme.name.clone_from(&name);
            entries.push(tui::ThemeEntry {
                name,
                theme,
                user: true,
            });
        }
    }
    if let Some(r) = roughness {
        for entry in &mut entries {
            entry.theme.roughness = r;
        }
    }
    entries
}

fn save_theme(name: &str) -> Result<()> {
    let path = config::set_theme(name)?;
    println!("theme = \"{name}\" saved to {}", path.display());
    Ok(())
}

fn list_themes() {
    println!("Built-in:");
    for name in theme::BUILTIN_NAMES {
        println!("  {name}");
    }
    let user = config::user_theme_names();
    if let Some(dir) = config::themes_dir() {
        println!("\nYours ({}):", dir.display());
        if user.is_empty() {
            println!(
                "  (none; `notopod themes nord > {}/mine.toml` to start one)",
                dir.display()
            );
        }
        for name in user {
            println!("  {name}");
        }
    }
}
