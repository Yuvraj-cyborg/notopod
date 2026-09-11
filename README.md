# notopod

Notes in your terminal, formatted while you type. Diagrams drawn by
hand, with the keyboard.

notopod is a text editor for Markdown notes that runs in the terminal.
While you write, headings, lists, tables and code blocks are shown
formatted. The one block your cursor is on shows its raw Markdown so you
can edit it. Move away and it snaps back into shape. A ` ```draw ` block
is a sketch: boxes, arrows and labels rendered as hand-drawn braille
graphics, and there is a drawing mode to make them without typing a
single coordinate.

Your notes stay ordinary `.md` files. Open them anywhere else whenever
you like; a drawing is a short, readable code block there.

## What it looks like

```
# Weekly plan                  <- cursor is on this line, so it shows raw Markdown

☑ Book the venue
☐ Send invites
  ◦ ask Sam for the list

┌─────┬───────┐
│ Day │ Task  │
├─────┼───────┤
│ Mon │ Draft │
└─────┴───────┘

 plan.md [+]   Ln 1, Col 14        ^S save  ^Q quit  ^F find  ^Z undo  ^P preview on
```

## Install

You need Rust 1.88 or newer for the first option.

**From source**

```
git clone https://github.com/Yuvraj-cyborg/notopod
cd notopod
cargo install --path .
```

This puts a `notopod` binary in `~/.cargo/bin`, which `rustup` already adds
to your `PATH`. Run the same command again after pulling changes to update
it; `cargo uninstall notopod` removes it. Without cloning:

```
cargo install --git https://github.com/Yuvraj-cyborg/notopod
```

**With Nix**

```
nix run github:Yuvraj-cyborg/notopod              # try it without installing
nix profile install github:Yuvraj-cyborg/notopod  # install
nix develop                                 # inside a clone: a shell with the Rust toolchain
```

**Prebuilt binaries**

Every release on the Releases page ships archives for Linux, macOS and
Windows. Unpack and put `notopod` somewhere on your `PATH`.

## Use

```
notopod notes.md            open a note (created on first save if it does not exist)
notopod                     start with an empty note; you are asked for a name when you save
notopod render notes.md     print a note formatted, then exit
notopod themes              list themes; `notopod themes nord` prints one to start your own
notopod config              show where the config file and your themes are read from
notopod --theme nord ...    use a theme for this run (`-t` for short)
```

`render` is for scripts and pipes, for example `notopod render todo.md | less -R`.
It drops colours on its own when the output is not a terminal or when
`NO_COLOR` is set. `--width 60` wraps at a fixed width; `--no-color` forces plain text.

## Keys

| Key | What it does |
|---|---|
| Arrows | Move. Up and Down go through wrapped lines and formatted blocks the way you would expect |
| Ctrl+Left / Ctrl+Right | Move by word (Alt+Left / Alt+Right works too) |
| Home / End | Start / end of the line (Ctrl+A / Ctrl+E as well) |
| Ctrl+Home / Ctrl+End | Start / end of the note |
| PageUp / PageDown | Move a screen at a time |
| Enter | New line. On a list item it continues the list; on an empty item it ends the list |
| Tab / Shift+Tab | Indent / outdent by two spaces |
| Ctrl+Z / Ctrl+Y | Undo / redo (Ctrl+Shift+Z also redoes) |
| Ctrl+S | Save |
| Ctrl+F | Find. Matches as you type. Enter jumps to the next match, Esc stops |
| Ctrl+G | Next match for the last search |
| Ctrl+D | Draw: edit the drawing under the cursor, or start a new one here (see below) |
| Ctrl+P | Turn live preview off or on. Off shows plain Markdown everywhere |
| Ctrl+Q | Quit. Asks first if there are unsaved changes |

Pasting from the terminal works as usual.

## How the preview works

notopod reads your note as a list of blocks: a heading, a paragraph, a
list item, a table, a code block. Every block is drawn formatted except
the one that contains your cursor, which is drawn exactly as you typed it.

```
What you typed          What you see, with the cursor on the second line

# Shopping              # Shopping         formatted
- [ ] milk              - [ ] milk         raw, because you are editing it
- [x] bread             ☑ bread            formatted
```

Nothing is converted and nothing is hidden. The file on disk is what you
typed, byte for byte.

Blocks are Markdown as you already know it: `#` headings, `*italic*` and
`**bold**`, `` `code` ``, `- ` bullets, `1. ` numbers, `- [ ] ` tasks,
`> ` quotes, pipe tables, fenced code blocks, `---` rules, links and
`~~strikethrough~~`. A `---` block at the very top is kept as front matter.

## Drawings

A fenced block tagged `draw` is a picture. Its text is a list of shapes,
one per line, placed on terminal columns and rows:

    ```draw
    rect 1,1 14x5 "Parser" round
    ellipse 24,0 16x7 "Renderer" fill color=green
    diamond 46,1 14x7 "ok?" color=yellow
    line 8,3 -> 30,3 "AST"
    line 32,3 -> 52,4 dashed
    text 1,9 "hand-drawn, in braille" color=muted
    ```

notopod renders it like this (colours not shown):

```
 ⡞⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⢳          ⢠⢊⠁⠲⡈⠑⢄⠈⠳⡈⠑⢌⠑⢄            ⣠⠞⠑⢄
 ⡇            ⢸      ⢠⡀ ⢠⠣⣌⠱⣄⠈⠢⡀⠑⢆⠈⠲⡄⠱⡄⢣         ⣠⠞⠁   ⠳⣄
 ⡇   Parser   ⢸⠤⠤⠤AST⠤⠬⡶⢼⢀⠈⠳Renderer⡈⠢⡀⢈⠆   ⠓⢀ ⢀⠞⠁       ⠑⣄
 ⡇            ⢸      ⠰⠋ ⠸⡄⠳⣄⠈⠳⣌⠑⣄⠈⠢⡀⠙⢄ ⡜⠉ ⠈⠓ ⠼⠷⣅   ok?    ⠈⢳
 ⢧⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⠜          ⠹⣄⠈⠑⢄⠈⠳⡌⠳⢄⠈⠒⣠⠞         ⠑⣄       ⣀⠔⠁
```

Shapes sit on cells, but they are drawn at two by four dots per cell, so
lines can be diagonal, ellipses are round, and every stroke has the
slightly wobbly, overshooting look of a quick pen sketch. `roughness` in
your config or theme controls how much; `0` gives exact geometry.

The language:

| Statement | Meaning |
|---|---|
| `rect X,Y WxH` | A rectangle with its top-left corner at column X, row Y |
| `ellipse X,Y WxH`, `diamond X,Y WxH` | The same box, drawn as an ellipse or a diamond |
| `line A -> B -> C` | A line through points; `->`, `<-`, `<->` put arrowheads on the ends, `--` none |
| `arrow A B` | Short for `line A -> B` |
| `text X,Y "words"` | Free text |
| `size WxH` | Minimum canvas size (it always grows to fit) |
| `"label"` | On a box: centred inside it. On a line: on its midpoint |
| `fill`, `dashed`, `round`, `color=red` | Hatch the inside, dash the stroke, round the corners, colour it. Colours are the theme's `red orange yellow green cyan blue magenta muted fg`, an ANSI name, or `#rrggbb` |
| `# comment` | Kept, not drawn |

A line whose end lies inside a box is drawn up to the box's border, so
`line 5,3 -> 30,3` from inside "Parser" to inside "Renderer" connects the
two boxes exactly. Lines that do not parse are left alone, never
rewritten.

### Drawing with the keyboard

Press **Ctrl+D** on a draw block to edit it visually, or anywhere else to
start a new drawing right there. The block stays rendered; the cursor
becomes a cell on the canvas.

| Key | What it does |
|---|---|
| Arrows | Move the cursor. Shift moves five cells |
| `r` `e` `d` | Start a rectangle / ellipse / diamond at the cursor; move to the opposite corner; Enter places it |
| `l` `a` | Start a line / arrow at the cursor; Space adds a corner; Enter finishes at the cursor |
| `t` | Type a label for the shape under the cursor, or free text at the cursor. Enter keeps it, Esc drops it |
| `m` | Pick up the shape under the cursor; arrows drag it; Enter drops it, Esc puts it back |
| `x` Delete Backspace | Delete the shape under the cursor |
| `f` `-` `o` `c` | Toggle fill, dashes, rounded corners; cycle the colour of the shape under the cursor |
| `?` | Show the keys |
| Ctrl+Z / Ctrl+Y | Undo / redo, one drawing step at a time |
| Ctrl+S | Save |
| Esc | Cancel the current tool, or leave the drawing when no tool is active |

Every action rewrites the block's text, so what you see is always what
is in the file. Start a line inside one box and finish it inside another
to connect them.

## Themes

notopod ships with `default`, `mono`, `catppuccin-mocha`, `gruvbox-dark`,
`nord`, `tokyo-night`, `dracula`, `solarized-dark` and `solarized-light`.
`default` uses your terminal's own colours, so it fits whatever your
terminal already looks like.

Pick one for a run with `--theme nord`, or for good in the config file:

```toml
# ~/.config/notopod/config.toml   ($NOTOPOD_CONFIG or $XDG_CONFIG_HOME/notopod/config.toml also work)
theme = "nord"

[canvas]
roughness = 0.7    # 0.0 draws exact geometry, 1.0 is very sketchy
```

To make your own, start from a built-in one:

```
mkdir -p ~/.config/notopod/themes
notopod themes nord > ~/.config/notopod/themes/mine.toml
```

A theme file is mostly a palette of nine colours; every style is derived
from it. Any element can be overridden in `[styles]` with a string like
`"yellow bold underline"`. `notopod themes default` prints a commented
file that lists what can be set.

## What is not there yet

This is version 0.2 territory. It edits and previews notes well and
draws diagrams. It does not yet have:

- selecting text, copy and cut
- ` ```mermaid ` blocks: flowcharts and sequence diagrams laid out for you
- a file tree, search across notes, `[[links]]` and backlinks
- syntax colours inside code blocks
- vim-style keys
- Kitty/Sixel graphics, export, runnable code blocks

## Project layout

One Cargo workspace. The `notopod` binary is the root package; the parts
it is built from are library crates under `lib/`.

| Path | What it does |
|---|---|
| `src/` | The `notopod` command: arguments, config file, dispatch |
| `lib/syntax` | Parses a note into blocks that remember where in the file they came from |
| `lib/canvas` | The ` ```draw ` language and its hand-drawn braille renderer |
| `lib/render` | Turns blocks into styled, wrapped terminal text |
| `lib/theme` | Palettes, styles, built-in themes and the theme file format |
| `lib/editor` | The text buffer: cursor, editing, undo, saving |
| `lib/tui` | The screen: live preview, keys, status bar, drawing mode |

The library crates have short names (`syntax`, `render`, ...) and are not
published to crates.io on their own; the product is the binary. There is
more in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Contributing

Day-to-day work happens on the `dev` branch. `main` only ever receives
releases. [CONTRIBUTING.md](CONTRIBUTING.md) explains how to run the
checks, open a pull request, and cut a release.

## License

MIT. See [LICENSE](LICENSE).
