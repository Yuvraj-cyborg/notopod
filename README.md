# notopod

Notes in your terminal, formatted while you type. Diagrams drawn by
hand, with the keyboard.

![A draw block open in kitty: sketchy boxes, a yellow diamond, a hatched
green box and labelled arrows, all in a hand-drawn font](docs/screenshot.png)

That is notopod running in a terminal. The diagram is a fenced code block
in an ordinary `.md` file, painted as a real picture at the terminal's
pixel resolution.

## What it is

A Markdown editor for the terminal, in one binary with no runtime.

- **Live preview, block by block.** Headings, lists, tasks, tables and
  quotes are shown formatted while you write. The one block your cursor
  is in shows its raw Markdown; leave it and it snaps back. Nothing is
  hidden or rewritten, so the file on disk is what you typed, byte for byte.
- **Drawings that are pictures, not character art.** A ` ```draw ` block
  is a sketch: strokes that wobble and overshoot like a quick pen drawing,
  hatched fills, and labels in Excalifont, the typeface Excalidraw uses.
  Terminals that speak the kitty graphics protocol show the picture;
  everywhere else gets the same shapes as braille dot art.
- **A drawing mode.** Press Ctrl+D and place boxes, arrows and labels with
  single keys — no coordinates to type. Every action rewrites the block's
  text, so the file always matches the picture.
- **A folder of notes, not just a file.** Tabs for the notes you have
  open, a file panel for the ones you don't, `[[links]]` between them
  that you can follow, and a graph of the whole folder — Obsidian's
  graph view, drawn by the same hand as the diagrams.
- **Plain files.** A drawing is a short, readable code block in any other
  editor, and your notes are `.md` all the way down.
- **Nine themes**, or your own in a TOML file, tried on for size with
  `notopod themes`. `default` borrows your terminal's colours, so notopod
  looks like the rest of your setup.
- **Vim keys if you want them**, off by default. One line in the config
  file turns them on; nothing else changes.
- **Small.** One 1.9 MB binary. Four megabytes of memory for a note, five
  with drawings; notes in background tabs cost only their text.

## Install

**From source** — needs Rust 1.88 or newer.

```
cargo install --git https://github.com/Yuvraj-cyborg/notopod
```

In a clone, `cargo install --path .` does the same from your working
copy. Either way the `notopod` binary lands in `~/.cargo/bin`, which
`rustup` already put on your `PATH`. Run the command again after pulling
to update it — an install is a snapshot, not a link — and
`cargo uninstall notopod` removes it.

**With Nix**

```
nix run github:Yuvraj-cyborg/notopod              # try it without installing
nix profile install github:Yuvraj-cyborg/notopod  # install
nix develop                                       # in a clone: a Rust shell
```

**Prebuilt binaries** — every release ships Linux, macOS and Windows
archives on the Releases page. Unpack and put `notopod` on your `PATH`.

## Use

```
notopod notes.md            open a note (created on first save if it does not exist)
notopod a.md b.md           open several, one tab each
notopod                     start empty; you are asked for a name when you save
notopod render notes.md     print a note formatted, then exit
notopod graph [DIR]         draw the notes in a folder and the links between them
notopod themes              try themes on a sample note and keep the one you like
notopod themes --list       just the names (which is what a pipe gets)
notopod themes nord         print a theme's TOML, to copy or redirect
notopod config              show where the config file and your themes come from
notopod --theme nord ...     use a theme for this run (`-t`)
notopod --graphics braille   force dot art where pictures would work (`-g`)
notopod --vim ...            vim keys for this run
```

`render` is for scripts and pipes, say `notopod render todo.md | less -R`.
It drops colours by itself when the output is not a terminal or `NO_COLOR`
is set; `--width 60` wraps at a fixed width and `--no-color` forces plain
text. Colourless output means braille drawings, because pictures travel
as escape codes.

## Keys

| Key | What it does |
|---|---|
| Arrows | Move. Up and Down step through wrapped lines and formatted blocks the way you would expect |
| Ctrl+Left / Ctrl+Right | Move by word (Alt+Left / Alt+Right too) |
| Home / End | Start / end of the line (Ctrl+A / Ctrl+E too) |
| Ctrl+Home / Ctrl+End | Start / end of the note |
| PageUp / PageDown | Move a screen at a time |
| Enter | New line. On a list item it continues the list; on an empty item it ends it |
| Tab / Shift+Tab | Indent / outdent by two spaces |
| Ctrl+Z / Ctrl+Y | Undo / redo (Ctrl+Shift+Z redoes as well) |
| Ctrl+S | Save |
| Ctrl+F | Find, matching as you type. Enter jumps to the next match, Esc stops |
| Ctrl+G | Next match for the last search |
| Ctrl+D | Draw: edit the drawing under the cursor, or start one here |
| Ctrl+] | Follow the link under the cursor |
| Ctrl+O | Open a note by name, in a new tab |
| Ctrl+T / Ctrl+W | New tab / close this tab (asking first if it has unsaved changes) |
| Ctrl+PageDown / Ctrl+PageUp | Next / previous tab (Ctrl+Tab / Ctrl+Shift+Tab too, in terminals that can tell them apart) |
| Ctrl+B | The file panel: open it, go to it, close it |
| Ctrl+K | The graph of your notes |
| Ctrl+P | Turn live preview off or on |
| Ctrl+Q | Quit, asking first if there are unsaved changes |

Pasting from the terminal works as usual.

## Files, tabs and links

Every note you open gets a tab; a bar above the note appears once there
are two. **Ctrl+B** opens a panel of the folder on the left — directories
first, then notes, nothing hidden or unrelated — starting on the note you
are in. Enter opens a note (or switches to its tab), Enter on a folder
unfolds it, Right and Left step in and out, `r` reads the disk again,
Esc goes back to the note with the panel still up, and Ctrl+B from the
panel puts it away. Only what you unfold is read.

Notes link to each other the way Obsidian's do: `[[Plan]]` is a link to
`Plan.md` anywhere under the folder, `[[Plan|the plan]]` shows other
words, and an ordinary `[text](other.md)` works too. Links render as
links, and **Ctrl+]** follows the one under the cursor into a tab; a
`[[name]]` that has no note yet becomes a new note next to this one, so
you can write the link first and the note after.

**Ctrl+K** draws the whole folder as a graph:

```
 7 notes · 14 links   ~/notes
        ╭────────╮          ╭─────────╮
        │  Home  │────────▶ │  Ideas  │ ─ ─ ▶ ( Lonely )
        ╰────────╯╲         ╰─────────╯
              │    ╲              │
              ▼     ╲             ▼
        ╭────────╮   ╲      ╭─────────╮      ╭────────╮
        │  Plan  │ ◀──╲─── │ Reading │      │ Rocket │───▶ Garden
        ╰────────╯     ╲    ╰─────────╯      ╰────────╯
```

Every note is an ellipse labelled with its first heading, every link an
arrow; notes with many links are filled and coloured, notes with none
are dashed. Arrows move to the nearest note in that direction, Tab walks
them in order, Enter opens one, `r` reads the folder again, Esc puts the
graph away. It is a real picture in terminals that show them and braille
elsewhere, in your theme's colours, because it is drawn by the same
renderer as a ` ```draw ` block. `notopod graph` prints it without the
editor.

## Vim keys

Off by default. `--vim`, or in the config file:

```toml
[keys]
vim = true
```

You start in normal mode. `h j k l w b e 0 ^ $ gg G` move (`j` and `k`
through rendered blocks, like the arrows), with counts; `d y c` work
over any of them and doubled for lines (`dd dw d$ cw yy cc`), plus `D C
Y S x X r J p P u Ctrl+R >> <<`. `i a I A o O` go into insert mode,
which is the ordinary editor, and Esc comes back. `/` searches as you
type, `n` and `N` step through the matches. `gt` and `gT` change tabs,
`gf` follows the link under the cursor, `ZZ` saves and quits. The `:`
line knows `w q q! wq x e bn bp bd tabnew files graph draw theme noh`
and a line number.

The Ctrl chords in the status bar keep working in both modes, so turning
this on takes nothing away. There is no visual mode yet: notopod has no
selection yet, and `v` says so.

## How the preview works

A note is a list of blocks — a heading, a paragraph, a list item, a table,
a code block. Every block is drawn formatted except the one holding your
cursor, which is drawn exactly as you typed it.

```
What you typed          What you see, with the cursor on the second line

# Shopping              # Shopping         formatted
- [ ] milk              - [ ] milk         raw, because you are editing it
- [x] bread             ☑ bread            formatted
```

The Markdown is the Markdown you know: `#` headings, `*italic*`,
`**bold**`, `` `code` ``, `- ` bullets, `1. ` numbers, `- [ ] ` tasks,
`> ` quotes, pipe tables, fenced code, `---` rules, links and
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
    text 1,9 "hand-drawn, in your terminal" color=muted
    ```

Which comes out as:

![The block above, rendered: sketchy anti-aliased strokes in a hand-drawn font](docs/drawing.png)

notopod paints that in your theme's colours on a transparent background
and places it over exactly the cells the block occupies, so it scrolls
and wraps with the text around it. `roughness` in your config or theme
sets the wobble; `0` gives exact geometry.

Pictures need a terminal that speaks the kitty graphics protocol:
**kitty**, **Ghostty**, **WezTerm** and **Konsole** do. notopod asks the
terminal on start-up. Where the answer is no — Terminal.app, Alacritty,
iTerm2, Windows Terminal, inside tmux — it draws the same shapes as
braille dot art, two by four dots per cell:

```
 ⡞⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⢳          ⢠⢊⠁⠲⡈⠑⢄⠈⠳⡈⠑⢌⠑⢄            ⣠⠞⠑⢄
 ⡇            ⢸      ⢠⡀ ⢠⠣⣌⠱⣄⠈⠢⡀⠑⢆⠈⠲⡄⠱⡄⢣         ⣠⠞⠁   ⠳⣄
 ⡇   Parser   ⢸⠤⠤⠤AST⠤⠬⡶⢼⢀⠈⠳Renderer⡈⠢⡀⢈⠆   ⠓⢀ ⢀⠞⠁       ⠑⣄
 ⡇            ⢸      ⠰⠋ ⠸⡄⠳⣄⠈⠳⣌⠑⣄⠈⠢⡀⠙⢄ ⡜⠉ ⠈⠓ ⠼⠷⣅   ok?    ⠈⢳
 ⢧⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⠜          ⠹⣄⠈⠑⢄⠈⠳⡌⠳⢄⠈⠒⣠⠞         ⠑⣄       ⣀⠔⠁
```

`--graphics kitty` forces pictures, for a terminal notopod does not
recognise; `--graphics braille` forces dot art; `[graphics] mode` in the
config file sets the default.

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

A line that ends inside a box stops at the box's border, so
`line 5,3 -> 30,3` from inside "Parser" to inside "Renderer" joins the two
exactly. Lines that do not parse are left alone, never rewritten.

### Drawing with the keyboard

Press **Ctrl+D** on a draw block to edit it, or anywhere else to start a
new one there. The block stays rendered; the cursor becomes a cell on the
canvas.

| Key | What it does |
|---|---|
| Arrows | Move the cursor. Shift moves five cells |
| `r` `e` `d` | Start a rectangle / ellipse / diamond; move to the opposite corner; Enter places it |
| `l` `a` | Start a line / arrow; Space adds a corner; Enter finishes at the cursor |
| `t` | Label the shape under the cursor, or type free text. Enter keeps it, Esc drops it |
| `m` | Pick up the shape under the cursor; arrows drag it; Enter drops it, Esc puts it back |
| `x` Delete Backspace | Delete the shape under the cursor |
| `f` `-` `o` `c` | Toggle fill, dashes, rounded corners; cycle the colour |
| `?` | Show the keys |
| Ctrl+Z / Ctrl+Y | Undo / redo, one drawing step at a time |
| Ctrl+S | Save |
| Esc | Cancel the current tool, or leave the drawing when no tool is active |

Start a line inside one box and finish it inside another to connect them.
While you draw, the picture shows a faint grid, the cell under the cursor,
and the shape you are placing or moving.

## Themes

Built in: `default`, `mono`, `catppuccin-mocha`, `gruvbox-dark`, `nord`,
`tokyo-night`, `dracula`, `solarized-dark` and `solarized-light`.

`notopod themes` is a picker. The themes are on the left, your own among
them, and a sample note is on the right in whichever one is highlighted —
headings, tasks, a table and a drawing, drawn by the same renderer the
editor uses, so it is the real thing and not a swatch:

```
   default          │# notopod
   mono             │
   catppuccin-mocha │Notes in your terminal, with bold, italic and code.
   gruvbox-dark     │
 • nord             │☑ pick a theme
   tokyo-night      │☐ write something
   dracula          │
   solarized-dark   │▎ Every block is formatted except the one you are editing.
   solarized-light  │
   mine             │ (a drawing here, as a picture or as braille)

 nord   built-in                 ↑↓ move   type to filter   Enter use   Esc cancel
```

Arrows move, typing filters the list, Enter saves your choice to the
config file and Esc leaves it alone. The dot marks the theme in use.
`notopod themes --set nord` saves one without the picker, and `--theme
nord` still overrides both for a single run.

Saving keeps the rest of your config file exactly as it was — comments and
all — so it is safe to hand-edit and to pick from:

```toml
# ~/.config/notopod/config.toml
# ($NOTOPOD_CONFIG or $XDG_CONFIG_HOME/notopod/config.toml also work)
theme = "nord"

[canvas]
roughness = 0.7    # 0.0 draws exact geometry, 1.0 is very sketchy

[graphics]
mode = "auto"      # auto | kitty | braille: how drawings are shown

[keys]
vim = false        # vim keys in the editor
```

For your own, start from a built-in one:

```
mkdir -p ~/.config/notopod/themes
notopod themes nord > ~/.config/notopod/themes/mine.toml
```

A theme file is mostly a palette of nine colours; every style is derived
from it. Any element can be overridden in `[styles]` with a string like
`"yellow bold underline"`. `notopod themes default` prints a commented
file listing what can be set.

## What is not there yet

- selecting text, copy and cut (and with them, vim's visual mode)
- ` ```mermaid ` blocks: flowcharts and sequence diagrams laid out for you
- search across notes, and a list of the notes that link here
- syntax colours inside code blocks
- pictures over Sixel or the iTerm2 protocol; export to PNG or SVG;
  runnable code blocks

## Project layout

One Cargo workspace. The `notopod` binary is the root package; the parts
it is built from are library crates under `lib/`.

| Path | What it does |
|---|---|
| `src/` | The `notopod` command: arguments, config file, dispatch |
| `lib/syntax` | Parses a note into blocks that remember where in the file they came from |
| `lib/canvas` | The ` ```draw ` language, its picture renderer and its braille fallback |
| `lib/graphics` | Shows pictures in the terminal: kitty graphics protocol, capability probing, image cache |
| `lib/render` | Turns blocks into styled, wrapped terminal text |
| `lib/theme` | Palettes, styles, built-in themes and the theme file format |
| `lib/editor` | The text buffer: cursor, editing, undo, saving |
| `lib/notes` | The links between the notes in a folder, and the graph they make |
| `lib/tui` | The screen: tabs, live preview, keys (classic and vim), file panel, graph, drawing mode, theme picker |

The library crates have short names (`syntax`, `render`, ...) and are not
published to crates.io on their own; the product is the binary. There is
more in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Contributing

Day-to-day work happens on the `dev` branch; `main` only ever receives
releases. [CONTRIBUTING.md](CONTRIBUTING.md) explains how to run the
checks, open a pull request and cut a release.

## License

MIT. See [LICENSE](LICENSE).
