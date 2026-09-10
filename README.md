# notopad

Notes in your terminal, formatted while you type.

notopad is a text editor for Markdown notes that runs in the terminal.
While you write, headings, lists, tables and code blocks are shown
formatted. The one block your cursor is on shows its raw Markdown so you
can edit it. Move away and it snaps back into shape.

Your notes stay ordinary `.md` files. Open them anywhere else whenever
you like.

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
git clone https://github.com/notopad/notopad
cd notopad
cargo install --path crates/notopad
```

**With Nix**

```
nix run github:notopad/notopad              # try it without installing
nix profile install github:notopad/notopad  # install
nix develop                                 # inside a clone: a shell with the Rust toolchain
```

**Prebuilt binaries**

Every release on the Releases page ships archives for Linux, macOS and
Windows. Unpack and put `notopad` somewhere on your `PATH`.

## Use

```
notopad notes.md            open a note (created on first save if it does not exist)
notopad                     start with an empty note; you are asked for a name when you save
notopad render notes.md     print a note formatted, then exit
```

`render` is for scripts and pipes, for example `notopad render todo.md | less -R`.
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
| Ctrl+P | Turn live preview off or on. Off shows plain Markdown everywhere |
| Ctrl+Q | Quit. Asks first if there are unsaved changes |

Pasting from the terminal works as usual.

## How the preview works

notopad reads your note as a list of blocks: a heading, a paragraph, a
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

Anything inside a code block is shown exactly as written, so you can
draw with box characters and it stays put:

    ```
    ┌──────────┐      ┌──────────┐      ┌──────────┐
    │  notes   │─────▶│  parser  │─────▶│  screen  │
    └──────────┘      └──────────┘      └──────────┘
    ```

A diagram language that draws boxes and arrows for you, and a drawing
mode for sketching with the keyboard, are next on the list.

## What is not there yet

This is version 0.1. It edits and previews notes well. It does not yet have:

- selecting text, copy and cut
- a diagram language: write `a -> b`, get boxes and arrows
- a drawing mode for sketching with the keyboard
- syntax colours inside code blocks
- links between notes and search across many notes
- themes and vim-style keys

## Project layout

One Cargo workspace, five crates. Each can be published and used on its own.

| Crate | What it does |
|---|---|
| `notopad-core` | Parses Markdown into blocks that remember where in the file they came from |
| `notopad-render` | Turns blocks into styled, wrapped terminal text |
| `notopad-editor` | The text buffer: cursor, editing, undo, saving |
| `notopad-tui` | The screen: live preview, keys, status bar |
| `notopad` | The `notopad` command |

There is more in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Contributing

Day-to-day work happens on the `dev` branch. `main` only ever receives
releases. [CONTRIBUTING.md](CONTRIBUTING.md) explains how to run the
checks and open a pull request; [docs/RELEASING.md](docs/RELEASING.md)
explains how a release is cut.

## License

MIT. See [LICENSE](LICENSE).
