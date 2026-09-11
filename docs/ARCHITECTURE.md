# Architecture

## Decisions

**All Rust.** One language, one toolchain, one binary. The parser, the
renderer, the editor, the drawing engine and the screen are library
crates in this workspace; `cargo install --path .` is the whole build.
An OCaml or Haskell core was considered and rejected: the live preview
re-parses on every keystroke and depends on byte spans into the text
buffer, which would have to cross an FFI boundary in both directions,
and every contributor and every release target would need a second
toolchain. Rust's enums, pattern matching and parser libraries give the
parts of a functional language a parser actually needs.

**Markdown, not a new format.** A note is CommonMark plus tables, task
lists, strikethrough and front matter. Everything notopod adds lives
inside fenced code blocks with a language tag (` ```draw ` today,
` ```mermaid ` and others later), so a note still opens everywhere else
and shows the block's text where notopod would show a picture. The
identity of the product is in the block languages, not in the container.

**Live preview is block-level.** Every top-level block renders formatted
except the one the cursor is in, which shows its source. A drawing
block being edited in canvas mode is the one exception: it is shown
rendered with the cursor drawn on top.

**Drawings are pixels, not ASCII art.** A ` ```draw ` block is a list of
shapes placed on terminal cells, but rendered on a braille raster at two
by four dots per cell. That is what lets lines be diagonal, ellipses be
round and strokes look hand-drawn, while the file stays short, editable
text.

## The pipeline

```
   keys                 text                    blocks                   rows
 ───────▶  Editor  ──────────▶  parse()  ──────────────▶  view::build  ─────────▶  draw
           (rope)              (syntax)                    (tui)
                                  │                          ▲
                                  │  ```draw blocks          │ the cursor position decides
                                  ▼                          │ which block is shown raw
                               canvas  ──▶  render  ─────────┘
                                             (theme decides every colour)
```

1. `editor` owns the text as a rope and applies edits.
2. `syntax` parses the whole text into a `Document`: a list of
   top-level `Block`s, each carrying the byte range it came from.
3. `tui::view` walks the buffer line by line. A block that does not
   contain the cursor is handed to `render` and becomes formatted rows; a
   block that does is emitted as raw source rows. `render` hands
   ` ```draw ` blocks to `canvas`.
4. `tui::ui` paints the rows and the status bar with ratatui, using the
   `theme`.

Parsing happens on every change. That is deliberate: a note is small,
the parser takes well under a millisecond for typical sizes, and it keeps
the design simple. The parse is cached by `Editor::version()` and the row
layout by `(version, cursor, width, preview, canvas generation)`.

## Crates

The binary is the workspace root package (`src/`); library crates live
under `lib/` with short names and `publish = false`.

| Crate | Depends on | Job |
|---|---|---|
| `syntax` | pulldown-cmark | `Document`, `Block`, `Inline`; `parse()`; `LineIndex` (byte offset to line). No terminal code at all. |
| `theme` | ratatui (types), serde, toml | `Theme`: every style, derived from a nine colour `Palette`; the theme file format; built-in themes as embedded TOML. |
| `canvas` | theme, ratatui (types) | The ` ```draw ` language (`parse`, `to_source`), the braille `Raster`, rough strokes, `render` with an editing `Overlay`. |
| `render` | syntax, canvas, theme | Block and inline rendering to `ratatui::text::Line`; word wrap; ANSI serialisation for stdout. |
| `editor` | ropey | `Editor`: cursor, movement, editing, undo/redo, search, atomic save, `replace_lines`. Knows nothing about Markdown except the list-continuation rules in `smart.rs`. |
| `tui` | all of the above, crossterm | `App` state machine (edit / find / save-as / confirm-quit / canvas), `view` (rows), `ui` (draw), key bindings, `canvas_mode`. |
| `notopod` | tui, render, theme, clap | The command: `notopod [FILE]`, `render`, `themes`, `config`; reads the config file. |

Dependencies point one way: `notopod -> tui -> {render, canvas, editor,
syntax, theme}`, `render -> {canvas, syntax, theme}`, `canvas -> theme`.
`syntax`, `editor` and `theme` depend on nothing else in the workspace.

The parser crate is called `syntax`, not `core`, on purpose: a crate
named `core` shadows Rust's built-in `core` in every crate that depends
on it, which breaks `core::` paths and the derive macros that expand to
them.

## Drawings

A ` ```draw ` block holds one statement per line (see the `canvas` crate
docs for the grammar):

```
size 60x14
rect 2,1 14x5 "Parser" round
ellipse 24,1 14x5 "Renderer" fill color=green
line 8,3 -> 30,3 "AST"
text 2,8 "free text" color=muted
```

- **Cells in, dots out.** Coordinates are terminal cells, so the cursor,
  labels and (later) the mouse line up with them. Rendering happens on a
  `Raster` of 2×4 dots per cell that becomes one braille character per
  cell, with one colour per cell and text overlaid on top of dots.
- **Rough strokes.** `rough.rs` does at dot resolution what rough.js does
  on a canvas: nudged endpoints, a slight bow, a little overshoot at
  corners, ellipses whose radius wanders, hatched fills. `Theme::roughness`
  scales it, 0 is exact geometry. Every shape seeds its own generator from
  its index, so nothing jitters between frames and a moved shape keeps
  its wobble.
- **Binding.** A line whose end cell is inside a box, and whose next
  point is outside it, is clipped to that box's border (found by
  bisection on the box's inside test). "Arrow from anywhere in A to
  anywhere in B" therefore connects the two borders.
- **Round trip.** `parse` never fails; lines it does not understand
  become `Shape::Raw` and `to_source` writes them back unchanged.
  `parse(to_source(d)) == d` for anything the editor produces.

### Canvas mode

`tui::canvas_mode` keeps a `Drawing`, a cell cursor and a `Tool`. Every
change is written straight back into the buffer with
`Editor::replace_lines` as a single undo step, then the normal
re-parse/re-render cycle shows it. The block is displayed through a
`view::Override`: pre-rendered rows with the cursor, the shape under it
and the shape being placed drawn by `canvas::render` with an `Overlay`.
Canvas mode therefore adds no second source of truth: the text in the
block is the drawing.

## Themes

`theme::Theme` is a flat struct of `ratatui::Style`s covering text
elements, the status bar and the canvas, plus `roughness`. Styles are
derived from a nine colour `Palette` (`fg`, `muted`, `red`, `orange`,
`yellow`, `green`, `cyan`, `blue`, `magenta`), so a theme file is usually
just a palette, with optional `[styles]` overrides written as strings
like `"yellow bold underline"`. Built-in themes are TOML files embedded
with `include_str!` and parsed by the same code as user themes; `default`
uses ANSI colour names so it follows the terminal's own palette.

The binary owns configuration (`src/config.rs`): `~/.config/notopod/config.toml`
(or `$NOTOPOD_CONFIG`, or `$XDG_CONFIG_HOME/notopod/config.toml`) with
`theme = "..."` and a `[canvas] roughness` override. `--theme` wins over
the file. User themes are `themes/<name>.toml` next to the config file.

## Other decisions worth knowing

**Blocks carry spans.** Every `Block` and `ListItem` has a byte range
into the source. `LineIndex` turns that into line numbers. This is what
makes live preview possible; keep spans correct when touching the parser.

**Preview granularity is the top-level block, except lists.** A list is
one block but each item is its own preview unit, so editing one item does
not turn a fifty-item list into raw text.

**The cursor lives in source coordinates.** Rendered rows are never
cursor targets; moving into a rendered block puts the cursor on that
block's first or last source line. Canvas mode is the exception, and it
maps its cell cursor onto the block's rows explicitly.

**Parsing never fails.** `parse()` returns a `Document` for any input.
Unknown syntax is text. The same holds for drawings.

**Saving is atomic.** The buffer is written to a temporary file next to
the target and renamed over it. Symlinks are followed so the real file is
replaced, not the link.

**Line endings are `\n`.** CRLF is normalised on load and paste.

## Adding things

**A block type.** `lib/syntax/src/ast.rs`: add a `BlockKind` variant.
`lib/syntax/src/parse.rs`: build it in `Builder::blocks`, keeping the
`range` as its span; add a test. `lib/render/src/block.rs`: add a match
arm in `render_block_at`; add a test through `render_document`.
`view.rs` treats all blocks the same.

**A fenced block language** (Mermaid, plots, ...). Make a crate under
`lib/` that turns the block's text into `Vec<Line<'static>>` given a
`Theme` and a width, and add one match arm in `render_block_at` that
dispatches on the language tag, the way ` ```draw ` does. Nothing else
changes.

**A shape.** `lib/canvas/src/model.rs` (variant, bounds, hit test,
translate), `parse.rs` (statement in and out, with a round-trip test),
`render.rs` (`draw_shape`), then a key in `tui/src/canvas_mode.rs` and
`app.rs` if it should be placeable.

**A theme.** Drop a TOML file in `lib/theme/themes/`, add it to
`BUILTIN_NAMES` and `BUILTIN_FILES`; the tests check that every built-in
parses and that its `name` matches.

## Roadmap

In the order we intend to build:

1. Selection, copy and cut in the editor.
2. ` ```mermaid ` blocks: flowchart (`graph LR`/`TD`, layered layout)
   and `sequenceDiagram`, drawn with the canvas raster. A `lib/mermaid`
   crate that produces a `canvas::Drawing`, so it gets the hand-drawn
   look and the same renderer for free.
3. Notes manager: a file tree for a folder of notes, search across notes,
   `[[wiki links]]` and backlinks.
4. Later: Kitty and Sixel graphics for terminals that have them, export to
   HTML and SVG, runnable code blocks.
