# Architecture

## The pipeline

```
   keys                 text                    blocks                   rows
 ───────▶  Editor  ──────────▶  parse()  ──────────────▶  view::build  ─────────▶  draw
           (rope)              (core)                     (tui)
                                                            ▲
                                                            │ the cursor position decides
                                                            │ which block is shown raw
```

1. `notopad-editor` owns the text as a rope and applies edits.
2. `notopad-core` parses the whole text into a `Document`: a list of
   top-level `Block`s, each carrying the byte range it came from.
3. `notopad-tui::view` walks the buffer line by line. A block that does
   not contain the cursor is handed to `notopad-render` and becomes
   formatted rows; a block that does contain the cursor is emitted as raw
   source rows. Blank lines between blocks are raw too.
4. `notopad-tui::ui` paints the rows and the status bar with ratatui.

Parsing happens on every change. That is deliberate: a note is small, the
parser takes well under a millisecond for typical sizes, and it keeps the
design simple. The parse is cached by `Editor::version()` and the row
layout by `(version, cursor, width, preview)`, so moving the cursor
without typing re-lays out but does not re-parse.

## Crates

| Crate | Depends on | Job |
|---|---|---|
| `notopad-core` | pulldown-cmark | `Document`, `Block`, `Inline`; `parse()`; `LineIndex` (byte offset to line). No terminal code at all. |
| `notopad-render` | core, ratatui (types only) | `Theme`; block and inline rendering to `ratatui::text::Line`; word wrap; ANSI serialisation for stdout. |
| `notopad-editor` | ropey | `Editor`: cursor, movement, editing, undo/redo, search, atomic save. Knows nothing about Markdown except the list-continuation rules in `smart.rs`. |
| `notopad-tui` | core, render, editor, ratatui + crossterm | `App` state machine (edit / find / save-as / confirm-quit), `view` (rows), `ui` (draw), key bindings. |
| `notopad` | tui, render, core, clap | The command: `notopad [FILE]`, `notopad render FILE`. |

Dependencies point one way: `notopad -> tui -> {core, render, editor}`,
`render -> core`. `core` and `editor` depend on nothing else in the
workspace, so they are the easiest to reuse.

## Decisions worth knowing

**Markdown, not a new format.** Notes are CommonMark plus tables, task
lists, strikethrough and front matter. Anything notopad adds later lives
inside fenced code blocks, so a note still opens everywhere else.

**Blocks carry spans.** Every `Block` and `ListItem` has a byte range
into the source. `LineIndex` turns that into line numbers. This is what
makes live preview possible; keep spans correct when touching the parser.

**Preview granularity is the top-level block, except lists.** A list is
one block but each item is its own preview unit, so editing one item does
not turn a fifty-item list into raw text. Nested lists are part of their
parent item.

**The cursor lives in source coordinates. Always.** Rendered rows are
never cursor targets. Moving up or down into a rendered block puts the
cursor on that block's last or first source line, which makes the block
raw on the next frame. This avoids mapping screen cells back to source
characters, which is where WYSIWYG editors get complicated.

**Parsing never fails.** `parse()` returns a `Document` for any input.
Unknown syntax is text.

**Saving is atomic.** The buffer is written to a temporary file next to
the target and renamed over it. Symlinks are followed so the real file is
replaced, not the link.

**Line endings are `\n`.** CRLF is normalised on load and paste; the rope
is configured so only `\n` ends a line, matching `LineIndex`.

## Adding a block type

1. `notopad-core/src/ast.rs`: add a `BlockKind` variant.
2. `notopad-core/src/parse.rs`: add a match arm in `Builder::blocks` that
   builds it and keeps the `range` as its span. Add a test.
3. `notopad-render/src/block.rs`: add a match arm in `render_block_at`.
   Add a test that renders through `render_document`.

`view.rs` treats all blocks the same; nothing to change there unless the
block needs per-part preview like lists do.

## Where diagrams will go

A `notopad-diagram` crate: takes the text of a fenced block with a
recognised language tag, parses the diagram language, lays it out, and
returns `Vec<Line>`. `render`'s code block arm dispatches on the language
tag. `core`, `editor` and `tui` need no changes. Until the language
exists, box-drawing characters inside any code block already render
as-is, so hand-drawn diagrams work today.
