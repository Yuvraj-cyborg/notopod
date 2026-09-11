# Changelog

All notable changes to notopod are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
uses [Semantic Versioning](https://semver.org/). All crates in the
workspace share one version number.

## [Unreleased]

## [0.1.2] - 2026-09-11

### Added

- Drawings. A fenced block tagged `draw` holds shapes, one per line
  (`rect`, `ellipse`, `diamond`, `line`/`arrow` with `-> <- <-> --`
  connectors and corners, `text`, `size`; `fill`, `dashed`, `round`,
  `color=`; quoted labels). Shapes sit on terminal cells so the keyboard
  can move over them. Lines that end inside a box connect to its border.
  Unparsed lines are kept as they are.
- Drawings are pictures. In kitty, Ghostty, WezTerm and Konsole a `draw`
  block is painted at the terminal's pixel resolution, Excalidraw style:
  anti-aliased rough.js strokes drawn twice with a wobble, hatched fills,
  dashed lines, rounded corners, arrowheads, and labels in Excalifont
  (bundled, OFL 1.1). The picture is transparent, uses the theme's
  colours and the terminal's own foreground, and sits on the exact cells
  the block occupies (kitty graphics protocol with Unicode placeholders),
  so it scrolls and redraws with the text. Pictures are cached by
  content and re-rendered on every keystroke while being drawn.
- Braille fallback. Everywhere else (Terminal.app, Alacritty, iTerm2,
  Windows Terminal, tmux) the same drawing is shown as dot art at 2×4
  dots per cell with the same hand-drawn strokes.
- `--graphics auto|kitty|braille` (`-g`) and `[graphics] mode` in the
  config file choose between the two; `auto` asks the terminal at
  start-up (graphics query, foreground/background colours, cell size).
  `notopod render` shows pictures too when writing to such a terminal.
- Canvas mode (`Ctrl+D`): draw in a block with the keyboard. `r`/`e`/`d`
  place boxes, `l`/`a` lines and arrows (Space adds corners), `t` labels,
  `m` moves, `x` deletes, `f`/`-`/`o`/`c` change fill, dashes, corners and
  colour. Every action is one undo step and rewrites the block's text.
- Themes. Nine built in (`default`, `mono`, `catppuccin-mocha`,
  `gruvbox-dark`, `nord`, `tokyo-night`, `dracula`, `solarized-dark`,
  `solarized-light`), `--theme`/`-t`, and user themes as TOML files: a
  nine colour palette plus optional per-element `[styles]` overrides.
  `notopod themes` lists them, `notopod themes NAME` prints one.
- A config file, `~/.config/notopod/config.toml` (`$NOTOPOD_CONFIG`,
  `$XDG_CONFIG_HOME` honoured): `theme`, `[canvas] roughness` and
  `[graphics] mode`. `notopod config` shows the paths.
- `^D draw` in the status bar.
- Key events that are already waiting are handled before the next frame,
  so a held arrow key no longer draws a frame per repeat.

### Changed

- The project is called notopod. Everything that said `notopad` (a typo
  in 0.1.0), including the binary name, now says `notopod`.
- The binary is the workspace root package: `cargo run`, `cargo test` and
  `cargo install --path .` work from the repository root with no `-p`.
- Library crates moved from `crates/notopad-*` to `lib/` and dropped the
  prefix: `syntax` (was `notopad-core`), `render`, `editor`, `tui`. They
  are not published to crates.io; the crates.io publish workflow is gone.
- The temporary file written during an atomic save is now
  `.<name>.notopod-tmp`.

## [0.1.0] - 2026-09-11

First release. A working editor with live preview; no diagram language yet.

### Added

- Editor: typing, cursor movement (by character, word, line, page, note),
  undo/redo with word-level grouping, atomic save, save-as prompt,
  unsaved-changes guard on quit.
- Live preview: every block renders formatted except the one under the
  cursor, which shows raw Markdown. Lists switch per item. `Ctrl+P` turns
  the preview off.
- Enter continues bullet, numbered, task and quote lines; Enter on an
  empty item ends the list. Tab / Shift+Tab indent and outdent.
- Find: incremental, smart-case, wraps around (`Ctrl+F`, `Ctrl+G`).
- Rendering for headings, paragraphs, emphasis, strong, strikethrough,
  inline code, links, images, bullet and numbered lists, task lists,
  block quotes, pipe tables with alignment, fenced and indented code
  blocks, horizontal rules, HTML blocks and YAML front matter.
- `notopad render FILE` (now `notopod render FILE`) prints a formatted note to stdout, with ANSI
  colours when writing to a terminal (`--width`, `--no-color`, `NO_COLOR`).
- Workspace split into `notopad-core`, `notopad-render`,
  `notopad-editor`, `notopad-tui` and the `notopad` binary.
- Nix flake (`nix build`, `nix run`, `nix develop`).
- CI on Linux, macOS and Windows; tagged releases build binaries for five
  targets.

[Unreleased]: https://github.com/Yuvraj-cyborg/notopod/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/Yuvraj-cyborg/notopod/compare/v0.1.0...v0.1.2
[0.1.0]: https://github.com/Yuvraj-cyborg/notopod/releases/tag/v0.1.0
