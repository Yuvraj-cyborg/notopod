# notopod

<img src="docs/logo.png" alt="A pencil drawing two linked notes" width="320">

Notes in your terminal, formatted while you type. Diagrams drawn by hand,
with the keyboard.

![A draw block open in kitty: sketchy boxes, a yellow diamond, a hatched
green box and labelled arrows, all in a hand-drawn font](docs/screenshot.png)

That is an ordinary `.md` file. The diagram is a fenced code block, and
notopod paints it as a real picture at the terminal's pixel resolution.

## What it is

A Markdown editor for the terminal. One binary, no runtime.

- **Formatted while you type.** Headings, lists, tasks, tables and quotes
  are shown formatted as you write. The block your cursor is in shows its
  raw Markdown; leave it and it snaps back. Nothing is hidden or
  rewritten — the file on disk is what you typed, byte for byte.
- **Drawings that are pictures.** A ` ```draw ` block is a sketch:
  strokes that wobble like a quick pen drawing, hatched fills, labels in
  Excalifont. Ctrl+D places boxes and arrows with single keys. Terminals
  that speak the kitty graphics protocol show the picture; the rest get
  the same shapes as braille dots.
- **A folder of notes, not just a file.** Tabs, a file panel, `[[links]]`
  you can follow, and a graph of the whole folder.
- **Plain files.** Your notes are `.md`, and a drawing is a short,
  readable code block in any other editor.
- **Nine themes** or your own, and vim keys if you want them.

## Install

**From source** — needs Rust 1.88 or newer.

```
cargo install --git https://github.com/Yuvraj-cyborg/notopod
```

In a clone, `cargo install --path .` does the same from your working
copy. Either way `notopod` lands in `~/.cargo/bin`, which `rustup`
already put on your `PATH`. Run the command again after pulling to
update; `cargo uninstall notopod` removes it.

**With Nix**

```
nix run github:Yuvraj-cyborg/notopod              # try it without installing
nix profile install github:Yuvraj-cyborg/notopod  # install
nix develop                                       # in a clone: a Rust shell
```

**Prebuilt binaries** — every release ships Linux, macOS and Windows
archives on the Releases page. Unpack and put `notopod` on your `PATH`.

## Use

Point it at a folder and it shows you what is in it:

```
notopod ~/notes             a folder: the file panel opens on it
notopod notes.md            one note (created on first save if new)
notopod ~/notes plan.md     both: the folder on the left, the note open
notopod                     an empty note; you are asked for a name when you save
```

The folder you name stays where the file panel and the graph start, for
the whole session — open a note from anywhere else and the panel still
shows your notes.

The rest:

```
notopod render notes.md      print a note formatted, then exit
notopod graph [DIR]          the notes in a folder and the links between them
notopod themes               try themes on a sample note and keep one
notopod themes --list        just the names
notopod themes nord          print a theme's TOML, to copy or redirect
notopod config               where the config file and your themes live
notopod --theme nord ...     a theme for this run (-t)
notopod --graphics braille   dot art where pictures would work (-g)
notopod --vim ...            vim keys for this run
```

`render` is for pipes: `notopod render todo.md | less -R`. It drops
colour by itself when the output is not a terminal or `NO_COLOR` is set.

## Keys

| Key | What it does |
|---|---|
| Arrows | Move. Up and Down step through wrapped lines and formatted blocks |
| Ctrl+Left / Ctrl+Right | Move by word (Alt too) |
| Home / End | Start / end of the line (Ctrl+A / Ctrl+E too) |
| Ctrl+Home / Ctrl+End | Start / end of the note |
| Enter | New line. On a list item it continues the list; on an empty one it ends it |
| Tab / Shift+Tab | Indent / outdent by two spaces |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Ctrl+S | Save |
| Ctrl+F / Ctrl+G | Find as you type / next match |
| **Ctrl+B** | **The file panel: open it, go to it, close it** |
| Ctrl+K | The graph of your notes |
| Ctrl+D | Draw: edit the drawing under the cursor, or start one |
| Ctrl+] | Follow the link under the cursor |
| Ctrl+O | Open a note by name, in a new tab |
| Ctrl+T / Ctrl+W | New tab / close this tab |
| Ctrl+PageDown / Ctrl+PageUp | Next / previous tab |
| Ctrl+P | Live preview off or on |
| Ctrl+Q | Quit, asking first if anything is unsaved |

## The file panel

**Ctrl+B** opens a tree of your folder on the left — directories first,
then notes, nothing hidden. It starts on the note you are in.

| In the panel | |
|---|---|
| ↑ ↓ or `j` `k` | Move |
| Enter | Open a note, or fold a directory open and shut |
| → ← or `l` `h` | Step into a directory, step back out |
| `r` | Read the disk again |
| `g` | The graph |
| Esc | Back to the note, panel still open |
| Ctrl+B | Put the panel away |

Only the directories you unfold are read, so a panel over a big tree
costs nothing.

## Links and the graph

Notes link the way Obsidian's do: `[[Plan]]` points at `Plan.md`
anywhere under the folder, `[[Plan|the plan]]` shows other words, and an
ordinary `[text](other.md)` works too. **Ctrl+]** follows the link under
the cursor into a tab. A `[[name]]` with no note yet becomes a new note,
so you can write the link first and the note after.

**Ctrl+K** draws the whole folder as a graph: every note an ellipse
labelled with its first heading, every link an arrow, notes with many
links filled and coloured, notes with none dashed. Arrows move between
them, Tab walks them in order, Enter opens one, Esc puts it away.
`notopod graph` prints the same picture without the editor.

## How the preview works

A note is a list of blocks — a heading, a paragraph, a list item, a
table, a code block. Every block is drawn formatted except the one
holding your cursor, which is drawn exactly as you typed it.

```
What you typed          What you see, with the cursor on the second line

# Shopping              # Shopping         formatted
- [ ] milk              - [ ] milk         raw, because you are editing it
- [x] bread             ☑ bread            formatted
```

The Markdown is the Markdown you know: `#` headings, `*italic*`,
`**bold**`, `` `code` ``, `- ` bullets, `1. ` numbers, `- [ ] ` tasks,
`> ` quotes, pipe tables, fenced code, `---` rules, links and
`~~strikethrough~~`. A `---` block at the top is front matter.

## Drawings

A block tagged `draw` is a picture. Six lines of it:

    ```draw
    rect 1,1 14x5 "Parser" round
    ellipse 24,0 16x7 "Renderer" fill color=green
    diamond 46,1 14x7 "ok?" color=yellow
    line 8,3 -> 30,3 "AST"
    line 32,3 -> 52,4 dashed
    text 1,9 "hand-drawn, in your terminal" color=muted
    ```

![The block above, rendered: sketchy anti-aliased strokes in a hand-drawn font](docs/drawing.png)

Press **Ctrl+D** to draw with single keys — `r` rectangle, `e` ellipse,
`d` diamond, `l` line, `a` arrow, `t` label, `m` move, `x` delete — and
every key rewrites the block's text, so the file always matches the
picture. The whole language and every drawing key are in
[docs/DRAWING.md](docs/DRAWING.md).

## Themes and config

Built in: `default`, `mono`, `catppuccin-mocha`, `gruvbox-dark`, `nord`,
`tokyo-night`, `dracula`, `solarized-dark`, `solarized-light`. `default`
borrows your terminal's own colours.

`notopod themes` is a picker: themes on the left, a sample note on the
right in whichever is highlighted, drawings and all. Typing filters,
Enter saves your choice, Esc leaves it alone.

The config file is `~/.config/notopod/config.toml` (`notopod config`
says where). Saving a theme keeps the rest of the file, comments and all.

```toml
theme = "nord"

[canvas]
roughness = 0.7    # 0.0 exact geometry, 1.0 very sketchy

[graphics]
mode = "auto"      # auto | kitty | braille

[keys]
vim = false
```

For your own theme, start from a built-in one:

```
mkdir -p ~/.config/notopod/themes
notopod themes nord > ~/.config/notopod/themes/mine.toml
```

## Vim keys

Off by default; `--vim` or `[keys] vim = true`. You start in normal mode.
`h j k l w b e 0 ^ $ gg G` move with counts, `d y c` work over them and
doubled for lines, plus `D C Y S x X r J p P u Ctrl+R >> <<`. `i a I A o
O` go into insert mode — the ordinary editor — and Esc comes back. `/`
searches, `n` and `N` step. `gt` `gT` change tabs, `gf` follows a link,
`ZZ` saves and quits, and `:` knows `w q q! wq x e bn bp bd tabnew files
graph draw theme noh` and line numbers. The Ctrl keys above keep working
in both modes. No visual mode yet, because there is no selection yet.

## Not there yet

- selecting text, copy and cut (and with them, vim's visual mode)
- ` ```mermaid ` blocks
- search across notes, and a list of what links here
- syntax colours inside code blocks
- Sixel and iTerm2 pictures; export to PNG or SVG

## Project layout

One Cargo workspace. The `notopod` binary is the root package; what it is
built from are library crates under `lib/`.

| Path | What it does |
|---|---|
| `src/` | The command: arguments, config file, dispatch |
| `lib/syntax` | Parses a note into blocks that remember where they came from |
| `lib/canvas` | The ` ```draw ` language, its picture renderer and braille fallback |
| `lib/graphics` | Pictures in the terminal: kitty graphics protocol, probing, cache |
| `lib/render` | Blocks into styled, wrapped terminal text |
| `lib/theme` | Palettes, styles, built-in themes, the theme file format |
| `lib/editor` | The text buffer: cursor, editing, undo, saving |
| `lib/notes` | Links between notes in a folder, and the graph they make |
| `lib/tui` | The screen: tabs, preview, keys, file panel, graph, drawing mode |

More in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Contributing

Work happens on `dev`; `main` only receives releases.
[CONTRIBUTING.md](CONTRIBUTING.md) has the checks, the branch rules and
how a release is cut.

## License

MIT. See [LICENSE](LICENSE).
