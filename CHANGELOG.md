# Changelog

All notable changes to notopod are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
uses [Semantic Versioning](https://semver.org/). All crates in the
workspace share one version number.

## [Unreleased]

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

[Unreleased]: https://github.com/Yuvraj-cyborg/notopod/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Yuvraj-cyborg/notopod/releases/tag/v0.1.0
