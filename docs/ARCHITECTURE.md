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

**Drawings are pictures, not character art.** A ` ```draw ` block is a
list of shapes placed on terminal cells, so the keyboard cursor can move
over them, but it is *rendered* as an image at the terminal's pixel
resolution: anti-aliased rough.js-style strokes and a hand-drawn font,
painted with tiny-skia, sent with the kitty graphics protocol and shown
over the very cells the block occupies. Terminals without that protocol
get the same shapes as braille dot art (2×4 dots per cell). The file
stays short, editable text either way.

**Pictures are text to the UI.** The kitty protocol's Unicode
placeholders make an image a run of ordinary characters whose
foreground colour names the image. The editor therefore treats a
picture like any other rendered block: it scrolls, is clipped, is
diffed and redrawn by ratatui, and the code that lays out rows knows
nothing about images.

## The pipeline

```
   keys                 text                    blocks                   rows
 ───────▶  Editor  ──────────▶  parse()  ──────────────▶  view::build  ─────────▶  draw
           (rope)              (syntax)                    (tui)
                                  │                          ▲
                                  │  ```draw blocks          │ the cursor position decides
                                  ▼                          │ which block is shown raw
                               canvas  ──▶  render  ─────────┘
                                 │           (theme decides every colour)
                                 │ picture (PNG)
                                 ▼
                              graphics  ──▶  escape codes to the terminal, placeholder rows to the view
```

1. `editor` owns the text as a rope and applies edits.
2. `syntax` parses the whole text into a `Document`: a list of
   top-level `Block`s, each carrying the byte range it came from.
3. `tui::view` walks the buffer line by line. A block that does not
   contain the cursor is handed to `render` and becomes formatted rows; a
   block that does is emitted as raw source rows. `render` hands
   ` ```draw ` blocks to a `Drawings` implementation: `graphics::Graphics`
   in the editor, which paints a picture with `canvas::render_image` and
   returns placeholder rows, or `render::Braille` where pictures are not
   possible.
4. `tui::ui` writes the pending image escape codes, then paints the rows
   and the status bar with ratatui, using the `theme`.

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
| `canvas` | theme, tiny-skia, ab_glyph, png, ratatui (types) | The ` ```draw ` language (`parse`, `to_source`); `render_image` (pixels: `sketch.rs` rough strokes, `text.rs` Excalifont labels, `pixel.rs` layout) and `render` (braille: `raster.rs`, `rough.rs`), both with an editing `Overlay`. |
| `graphics` | canvas, render, theme, crossterm, rustix | `Graphics`: probes the terminal (kitty graphics, cell size, fg/bg colours), sends PNGs with the kitty protocol, caches them by content, returns placeholder rows; falls back to braille. |
| `render` | syntax, canvas, theme | Block and inline rendering to `ratatui::text::Line`; word wrap; ANSI serialisation for stdout; the `Drawings` trait. |
| `editor` | ropey | `Editor`: cursor, movement, editing, undo/redo, search, atomic save, `replace_lines`. Knows nothing about Markdown except the list-continuation rules in `smart.rs`. |
| `tui` | all of the above, crossterm | `App` state machine (edit / find / save-as / confirm-quit / canvas), `view` (rows), `ui` (draw), key bindings, `canvas_mode`, and `picker` (the theme screen). |
| `notopod` | tui, graphics, render, theme, clap | The command: `notopod [FILE]`, `render`, `themes`, `config`; reads and writes the config file; `--theme`, `--graphics`. |

Dependencies point one way: `notopod -> tui -> {graphics, render, canvas,
editor, syntax, theme}`, `graphics -> {canvas, render, theme}`,
`render -> {canvas, syntax, theme}`, `canvas -> theme`. `syntax`,
`editor` and `theme` depend on nothing else in the workspace.

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

- **Cells in, pixels out.** Coordinates are terminal cells, so the
  cursor, labels and (later) the mouse line up with them. `render_image`
  paints a `Pixmap` of `cols × cell_width` by `rows × cell_height`
  pixels, the cell size coming from the terminal, so the picture maps
  1:1 onto the cells it will cover. The background is transparent; the
  terminal's own shows through.
- **Rough strokes.** `sketch.rs` is a port of the parts of rough.js that
  matter: every line is drawn twice as a cubic curve with nudged ends and
  control points, ellipses are Catmull-Rom curves through displaced
  points, fills are hatched at -41°, rounded corners are jittered arcs.
  `Theme::roughness` (0..=1) maps onto rough.js's scale where 1 is
  Excalidraw's "artist". Every shape seeds its own generator from its
  index, so nothing jitters between frames and a moved shape keeps its
  wobble. `rough.rs` does the same at braille resolution, with about
  half the wobble, since a dot is a coarse unit.
- **Labels.** `text.rs` lays out and rasterises Excalifont (bundled,
  OFL 1.1) with ab_glyph, sized to fit one cell row. Labels that do not
  fit their box shrink to 60% of that, then are cut with `…`.
- **Binding.** A line whose end cell is inside a box, and whose next
  point is outside it, is clipped to that box's border (found by
  bisection on the box's inside test). "Arrow from anywhere in A to
  anywhere in B" therefore connects the two borders.
- **Round trip.** `parse` never fails; lines it does not understand
  become `Shape::Raw` and `to_source` writes them back unchanged.
  `parse(to_source(d)) == d` for anything the editor produces.

### Pictures in the terminal

`graphics::Graphics` implements `render::Drawings`. On start-up (in raw
mode, so the reply is readable) it sends one round trip to the terminal:
a kitty graphics query, OSC 10/11 for the foreground and background
colours, `CSI 16 t` for the cell size in pixels, and a primary device
attributes request that every terminal answers, which marks the end of
the reply. If the graphics query is answered, drawings become pictures;
otherwise, and inside tmux or when stdin/stdout is not a terminal, they
are braille. `--graphics` and `[graphics] mode` override the decision.

For each drawing, `Graphics` hashes the drawing, theme, width and
overlay; a miss renders a PNG (`Compression::Fast`: a drawing is
re-encoded on every keystroke while it is edited), gives it a random
24-bit id, and queues `a=T,U=1,c=…,r=…` (transmit plus virtual
placement) as base64 in 4 KiB chunks. It returns rows of `U+10EEEE`
cells whose foreground colour is the id and whose two combining
diacritics are the row and column. `tui::ui` writes the queue to stdout
before ratatui writes the frame, so the image exists when the cells that
show it arrive. The cache holds 32 pictures; evictions and shutdown send
`a=d,d=I` so the terminal frees the memory. Pictures scroll, clip and
redraw for free because to ratatui they are text.

### The theme picker

`tui::picker` is the crate's second screen and reuses everything the first
one is made of: it renders a small sample note through
`render::render_document_with` with the highlighted theme, so what you see
is the real renderer, pictures included. It answers with a name and writes
nothing itself; `src/config.rs` puts the name in the config file by
editing the text rather than re-serialising it, which is what keeps a
hand-written file's comments and layout intact.

### Canvas mode

`tui::canvas_mode` keeps a `Drawing`, a cell cursor and a `Tool`. Every
change is written straight back into the buffer with
`Editor::replace_lines` as a single undo step, then the normal
re-parse/re-render cycle shows it. The block is displayed through a
`view::Override`: pre-rendered rows with the cursor, the shape under it
and the shape being placed drawn with an `Overlay` (a faint cell grid,
the cursor cell, the selected shape in the highlight colour, the
preview shape in the preview colour). Canvas mode therefore adds no
second source of truth: the text in the block is the drawing.

## Themes

`theme::Theme` is a flat struct of `ratatui::Style`s covering text
elements, the status bar and the canvas, plus `roughness`. Styles are
derived from a nine colour `Palette` (`fg`, `muted`, `red`, `orange`,
`yellow`, `green`, `cyan`, `blue`, `magenta`), so a theme file is usually
just a palette, with optional `[styles]` overrides written as strings
like `"yellow bold underline"`. Built-in themes are TOML files embedded
with `include_str!` and parsed by the same code as user themes; `default`
uses ANSI colour names so it follows the terminal's own palette.

When a drawing is painted as a picture, `theme::to_rgb` turns each
`ratatui::Color` into RGB: `Rgb` as is, indexed colours through the xterm
table, ANSI names through the usual defaults, and `Reset` into the
foreground the terminal reported (or a light grey on a dark background).

The binary owns configuration (`src/config.rs`): `~/.config/notopod/config.toml`
(or `$NOTOPOD_CONFIG`, or `$XDG_CONFIG_HOME/notopod/config.toml`) with
`theme = "..."`, a `[canvas] roughness` override and `[graphics] mode`.
`--theme` and `--graphics` win over the file. User themes are
`themes/<name>.toml` next to the config file.

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
`lib/` that turns the block's text into a `canvas::Drawing` (to get the
hand-drawn look, pictures and braille for free) or straight into
`Vec<Line<'static>>` given a `Theme` and a width, and add one match arm
in `render_block_at` that dispatches on the language tag, the way
` ```draw ` does. Nothing else changes.

**A shape.** `lib/canvas/src/model.rs` (variant, bounds, hit test,
translate), `parse.rs` (statement in and out, with a round-trip test),
`pixel.rs` (`Ctx::shape`) and `render.rs` (`draw_shape`), then a key in
`tui/src/canvas_mode.rs` and `app.rs` if it should be placeable.

**A graphics protocol** (Sixel, iTerm2). `lib/graphics`: detect it in
`probe.rs`, encode in a sibling of `kitty.rs`, and choose it in
`Graphics::draw`. Protocols without Unicode placeholders need the TUI to
place images by cursor position after each frame, which the current
design avoids; that is the part to think about first.

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
4. Later: Sixel and iTerm2 pictures for terminals without the kitty
   protocol, export of notes to HTML and of drawings to PNG and SVG
   (the picture renderer already produces the PNG), runnable code blocks.
