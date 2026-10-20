# Changelog

All notable changes to notopod are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
uses [Semantic Versioning](https://semver.org/). All crates in the
workspace share one version number.

## [Unreleased]

## [0.5.0] - 2026-10-21

### Added

- Manim scenes drawn by notopod itself. A ```` ```manim ```` block of the
  usual kind — Manim's shapes, `Text`, `MathTex`, `VGroup`, `Axes` and
  `NumberPlane` with plotted functions and labels, placed with `next_to`,
  `to_edge`, `arrange` and the rest, shown with `self.add` or
  `self.play` — is read and painted in Rust, in milliseconds, with no
  Python and no LaTeX. Sizes, colours, gaps and placement are Manim's
  own, checked against real Manim. Anything more goes to real Manim.
- The scene under the block you are typing in. With the cursor in a
  ```` ```manim ```` block its code stays editable and its frame is drawn
  underneath, updated as you type; until a new frame arrives the last
  one stays, with the error under it if the code is half typed.
- `[manim] native = false` sends every scene to real Manim, and
  `[manim] python` names the Python it is installed in, for commands such
  as `uv run manim` that hide it.

### Changed

- ```` ```manim ```` blocks are drawn without asking: `--manim` is gone,
  and `[manim] enabled = false` turns them off. Scenes notopod draws
  itself run nothing; the rest run your Manim, so open notes you trust.
- Real Manim is much faster. A render used to start `manim render`
  afresh, which imports Manim and asks PyPI for a newer version first —
  forty seconds on a network where that stalls. Now one Python process
  keeps Manim loaded and forks for each scene: about 0.2s for the first,
  0.03s after. It starts with the first scene that needs it and exits
  after two idle minutes.

### Fixed

- See-through fills from real Manim came out too dark: Manim writes
  premultiplied colour into its transparent frames. It is undone now.
- `from x import *` in a ```` ```manim ```` body no longer ends up inside
  `construct`, where Python forbids it.

## [0.4.0] - 2026-10-20

### Added

- Equations. `$$…$$` on its own line, and a ```` ```math ```` block, are
  typeset as pictures: laid out by RaTeX, drawn in KaTeX's fonts at the
  terminal's resolution, centred, in the theme's colour (`\color` is
  honoured). Math inside a sentence, `$x^2$`, stays on its line as
  Unicode — `x = (−b ± √(b²−4ac))/(2a)` — and so does display math where
  there are no pictures. `$5 and $10` is still money. The new `math` theme
  style colours it.
- Plots and formulas in drawings, after Manim's objects: `axes X,Y WxH
  x=A..B y=C..D` (and `plane`, with a grid), `plot "sin(x)" "\sin x"` on
  the axes above it, and `math X,Y "tex"` for a formula anywhere. Ticks
  and their numbers place themselves, curves break at asymptotes and stop
  at the edges, a plot's label is typeset at the end of its curve, and
  moving the axes moves their plots. Expressions read like maths: `2x`,
  `sin x`, `|x|`, `π`, `e^(-x^2)`. In braille, curves are dots and
  formulas Unicode.
- Real Manim, off by default. A ```` ```manim ```` block holds Manim code
  — a `Scene`, or just the body of `construct` — and with `--manim` or
  `[manim] enabled = true` it shows the scene's last frame, rendered by
  your own `manim` in the background so typing never waits, cropped to
  what was drawn, and cached on disk so a note opened again shows its
  scenes at once. `command`, `quality` and `timeout` tune it. It runs the
  Python in the note, which is why it is off: until it is on, the block is
  its code with a line saying how. Failures show Python's last error
  line.

### Changed

- The binary is about 4 MB, up from 2: the equation layout and its fonts.
  `cargo build --no-default-features` leaves them out and writes
  equations as Unicode instead.

## [0.3.1] - 2026-09-11

### Added

- An installer, so there is a one-line way in:

  ```sh
  curl -fsSL https://raw.githubusercontent.com/Yuvraj-cyborg/notopod/main/install.sh | sh
  ```

  It works out the target from `uname`, takes that archive from the
  latest release, verifies it against the `.sha256` published beside it,
  and writes the binary to `~/.local/bin` — `--to DIR` and `--version
  TAG` change where and which, and both have environment-variable
  equivalents. It says so plainly when the install directory is not on
  `PATH`, when a musl system asks for a glibc build, and when there is no
  binary for the machine. Nothing outside the install directory is
  written, and nothing is installed if the checksum does not match.
- The installer is attached to every release too, so
  `.../releases/download/<tag>/install.sh` is a URL that cannot change
  after the fact, and release notes now end with how to install.

## [0.3.0] - 2026-09-11

### Added

- `notopod DIR` opens a folder. The file panel comes up on it with the
  keyboard, so you start by seeing what is there instead of having to
  name a note. The folder stays where the panel and the graph start for
  the rest of the session, whatever note you open from wherever, so
  opening a note from somewhere else no longer drags the panel with it.
  `notopod ~/notes plan.md` does both: the folder on the left, the note
  in a tab.

### Fixed

- `color=#rrggbb` in a ```` ```draw ```` block. `#` starts a comment, and
  the tokenizer split words on it wherever it appeared, so a hex colour
  was cut to `color=` and the whole line was left undrawn. It is now only
  a comment where a word starts.

### Changed

- The README is shorter; the full drawing reference moved to
  [docs/DRAWING.md](docs/DRAWING.md).

## [0.2.1] - 2026-09-11

### Added

- Tabs. Every open note has one; a bar appears above the note once there
  are two. Ctrl+T opens an empty tab, Ctrl+O asks for a name and opens
  it (a bare name lands next to the current note with `.md` added; a
  missing file is a note created on save), Ctrl+W closes a tab, asking
  first if it has unsaved changes. Ctrl+PageDown / Ctrl+PageUp move
  between tabs, and Ctrl+Tab / Ctrl+Shift+Tab where the terminal can
  tell them apart. `notopod a.md b.md` opens both. Quitting counts every
  dirty tab. A tab off screen keeps only its text.
- A file panel (Ctrl+B): the folder as a tree on the left, directories
  first, then `.md`, `.markdown` and `.txt` files, nothing hidden. Enter
  opens a note in a tab or folds a directory, Right and Left step in and
  out, `r` re-reads the disk, Esc goes back to the note, Ctrl+B from the
  panel closes it. Only unfolded directories are read.
- `[[wikilinks]]`: `[[Name]]` and `[[Name|shown]]` render as links, in
  paragraphs, headings, cells and list items. Ctrl+] follows the link
  under the cursor — a `[[name]]` is found anywhere under the folder
  ignoring case, a `[text](path.md)` is relative to the note, and a name
  with no note yet becomes a new note next to this one. URLs are shown,
  not opened.
- The graph of your notes (Ctrl+K, `g` in the file panel, `notopod graph
  [DIR]`): every note in the folder as an ellipse labelled with its first
  heading, every link as an arrow, laid out by a force simulation so
  linked notes sit together. Hubs are filled and coloured, notes with no
  links are dashed. Arrows move to the nearest note in that direction,
  Tab walks them in order, Enter opens one in a tab, `r` re-reads the
  folder, Esc puts the graph away. It is drawn by the same renderer as a
  `draw` block: a picture in kitty, Ghostty, WezTerm and Konsole, braille
  elsewhere, in the theme's colours. The scan stops at two thousand
  notes and skips files over two megabytes.
- Vim keys, off by default: `--vim` or `[keys] vim = true`. Normal mode
  has `h j k l w b e 0 ^ $ gg G` with counts, `d y c` over them and
  doubled for lines, `D C Y S x X r J p P u Ctrl+R >> <<`, `i a I A o O`
  into insert mode (the ordinary editor) and Esc back, `/ n N`, `gt gT`,
  `gf`, `ZZ ZQ`, and a `:` line with `w q q! wq x e bn bp bd b tabnew
  files graph draw theme noh` and line numbers. Ctrl chords work in both
  modes. No visual mode yet, since there is no selection yet.
- `notopod themes` is a picker: the themes on the left, a sample note on
  the right drawn in the highlighted one, drawings and all. Arrows move,
  typing filters the list, Enter saves the choice to the config file and
  Esc leaves it alone. A dot marks the theme in use, and your own themes
  are in the list next to the built-in ones. `--set NAME` saves without
  the picker; `--list` and a pipe get the plain list.
- The editor gained `char_idx`, `position_at`, `len_chars`, `slice`,
  `line_range` and `delete_range`.

### Changed

- The release binary is about 30% smaller (2.6 MB to 1.9 MB): fat LTO,
  `panic = "abort"` and `opt-level = "s"`, which costs nothing
  measurable in the picture renderer. Memory: about 4 MB for a note, 5
  MB with braille drawings, 14 MB with six large pictures on screen.
- Writing the theme keeps the rest of the config file as it was:
  comments, spacing and other settings are untouched, and the key is
  replaced where it stands rather than the file being rewritten.
- `--theme`'s help says `notopod themes` shows what there is, since it
  is a picker now.

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

[Unreleased]: https://github.com/Yuvraj-cyborg/notopod/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/Yuvraj-cyborg/notopod/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/Yuvraj-cyborg/notopod/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/Yuvraj-cyborg/notopod/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Yuvraj-cyborg/notopod/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/Yuvraj-cyborg/notopod/compare/v0.1.2...v0.2.1
[0.1.2]: https://github.com/Yuvraj-cyborg/notopod/compare/v0.1.0...v0.1.2
[0.1.0]: https://github.com/Yuvraj-cyborg/notopod/releases/tag/v0.1.0
