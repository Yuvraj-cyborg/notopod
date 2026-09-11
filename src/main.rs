//! The `notopod` command.

mod config;

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

/// A terminal notes editor with live Markdown preview.
#[derive(Parser)]
#[command(name = "notopod", version, about)]
struct Cli {
    /// Note to open. Created on first save if it does not exist.
    file: Option<PathBuf>,

    /// Theme to use: a built-in name, a user theme, or a .toml file.
    /// Overrides the config file. `notopod themes` lists what is available.
    #[arg(long, short = 't', global = true, value_name = "NAME")]
    theme: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Open a note in the editor (same as `notopod FILE`).
    Edit {
        /// Note to open.
        file: Option<PathBuf>,
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
    /// List themes, or print one as a TOML file to start your own from.
    Themes {
        /// Print this theme's TOML (built-in themes only).
        show: Option<String>,
    },
    /// Show where the config file and user themes are read from.
    Config,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = config::load()?;
    let theme = || config::resolve_theme(cli.theme.as_deref(), &config);

    match cli.command {
        None => tui::run(cli.file.as_deref(), theme()?),
        Some(Command::Edit { file }) => tui::run(file.as_deref(), theme()?),
        Some(Command::Render {
            file,
            width,
            no_color,
        }) => render_note(&file, width, no_color, &theme()?),
        Some(Command::Themes { show }) => themes(show.as_deref()),
        Some(Command::Config) => {
            println!("{}", config::describe_paths());
            Ok(())
        }
    }
}

fn render_note(
    file: &PathBuf,
    width: Option<u16>,
    no_color: bool,
    theme: &theme::Theme,
) -> Result<()> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    let stdout = std::io::stdout();
    let width = width
        .or_else(|| crossterm::terminal::size().ok().map(|(w, _)| w))
        .unwrap_or(80)
        .max(20);
    let color = !no_color && stdout.is_terminal() && std::env::var_os("NO_COLOR").is_none();

    let doc = syntax::parse(&text);
    let lines = render::render_document(&doc, usize::from(width), theme);
    print!("{}", render::ansi::to_ansi(&lines, color));
    Ok(())
}

fn themes(show: Option<&str>) -> Result<()> {
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
    Ok(())
}
