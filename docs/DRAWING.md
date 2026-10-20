# Drawings

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

notopod paints that in your theme's colours on a transparent background,
over exactly the cells the block occupies, so it scrolls and wraps with
the text around it.

## The language

| Statement | Meaning |
|---|---|
| `rect X,Y WxH` | A rectangle with its top-left corner at column X, row Y |
| `ellipse X,Y WxH`, `diamond X,Y WxH` | The same box, drawn as an ellipse or a diamond |
| `line A -> B -> C` | A line through points; `->`, `<-`, `<->` put arrowheads on the ends, `--` none |
| `arrow A B` | Short for `line A -> B` |
| `text X,Y "words"` | Free text |
| `axes X,Y WxH x=A..B y=C..D` | A pair of axes over those ranges (default `-5..5` and `-3..3`) |
| `plane X,Y WxH ...` | The same axes with a grid behind them |
| `plot "expr" "label" x=A..B` | A function of `x` on the closest axes above it; the label and the range are optional |
| `math X,Y "tex"` | A typeset formula, its top-left at X,Y |
| `size WxH` | Minimum canvas size (it always grows to fit) |
| `"label"` | On a box: centred inside it. On a line: on its midpoint |
| `fill`, `dashed`, `round`, `color=red` | Hatch the inside, dash the stroke, round the corners, colour it |
| `# comment` | Kept, not drawn |

Colours are the theme's `red orange yellow green cyan blue magenta muted
fg`, an ANSI name, or `#rrggbb`. A `#` only starts a comment where a word
starts, so `color=#4c7fd4` is a colour and not half a comment.

A line that ends inside a box stops at the box's border, so
`line 5,3 -> 30,3` from inside "Parser" to inside "Renderer" joins the
two exactly. Lines that do not parse are left alone, never rewritten —
your file always round-trips.

## Plots and formulas

`axes`, `plot` and `math` are named after the Manim objects they copy:

    ```draw
    plane 0,0 46x14 x=-2pi..2pi y=-1.5..1.5 color=muted
    plot "sin(x)" "\sin x" color=yellow
    plot "x^2/4" x=-2..2 dashed
    math 50,4 "e^{i\pi} + 1 = 0"
    ```

An expression is written the way maths is written: `2x` and `3sin(x)`
multiply, `sin x` is `sin(x)`, `-x^2` is `-(x^2)`, `|x|` is the absolute
value, and `π`, `**` and a leading `y =` are fine. There are `sin cos
tan`, their `a`- and `arc`- inverses and `h` versions, `exp ln log sqrt
abs floor ceil sign`, and the constants `pi tau e`. Range ends are
expressions too, so `x=-2pi..2pi` works.

A curve breaks where the function does — `tan x` has no line across its
asymptotes — and stops at the edge of its axes instead of running off
them. A plot's label sits at the end of its curve and is set as maths,
like the text of `math`; inside the quotes only `\"` and `\\` are escapes,
so LaTeX is written as it is. Moving the axes moves their plots with them.

In braille, curves are dots and formulas are Unicode (`e^(iπ) + 1 = 0`).

## Drawing with the keyboard

Press **Ctrl+D** on a draw block to edit it, or anywhere else to start a
new one. The block stays rendered; the cursor becomes a cell on the canvas.

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
While you draw, the picture shows a faint grid, the cell under the
cursor, and the shape you are placing.

## Pictures, or dots

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
config file sets the default. `roughness` in your config or theme sets
how much the strokes wobble; `0` gives exact geometry.

This project's own logo is a drawing: [`logo.draw`](logo.draw) rendered
by the same code.
