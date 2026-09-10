//! The `notopad` command.

use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use notopad_render::Theme;

/// A terminal notes editor with live Markdown preview.
#[derive(Parser)]
#[command(
    name = "notopad",
    version,
    about,
    args_conflicts_with_subcommands = true
)]
struct Cli {
    /// Note to open. Created on first save if it does not exist.
    file: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Open a note in the editor (same as `notopad FILE`).
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => notopad_tui::run(cli.file.as_deref()),
        Some(Command::Edit { file }) => notopad_tui::run(file.as_deref()),
        Some(Command::Render {
            file,
            width,
            no_color,
        }) => render(&file, width, no_color),
    }
}

fn render(file: &PathBuf, width: Option<u16>, no_color: bool) -> Result<()> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    let stdout = std::io::stdout();
    let width = width
        .or_else(|| crossterm::terminal::size().ok().map(|(w, _)| w))
        .unwrap_or(80)
        .max(20);
    let color = !no_color && stdout.is_terminal() && std::env::var_os("NO_COLOR").is_none();

    let doc = notopad_core::parse(&text);
    let lines = notopad_render::render_document(&doc, usize::from(width), &Theme::default());
    print!("{}", notopad_render::ansi::to_ansi(&lines, color));
    Ok(())
}
