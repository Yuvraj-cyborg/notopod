//! Drawing the scenes notopod understands without Python.
//!
//! Most ```` ```manim ```` blocks in notes are short and straight: make
//! some shapes, text and axes, place them, show them. This reads that
//! kind of block itself — a small Python interpreter over Manim's own
//! objects, defaults and placement rules — and paints its last frame in
//! a few milliseconds, with formulas typeset by the `math` crate rather
//! than LaTeX. Fast enough to redraw on every key.
//!
//! Anything beyond it (loops, functions, 3D, plugins, …) is
//! [`Error::Unsupported`], and the block goes to real Manim instead.

mod colors;
mod draw;
mod eval;
mod lex;
mod parse;
mod scene;

/// Longest block read here; longer ones go to Manim.
const MAX_LEN: usize = 32 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Error {
    /// Python itself would refuse it: the block is wrong as written.
    Syntax(String),
    /// Fine Python, perhaps, but more than notopod draws itself.
    Unsupported(String),
    /// It ran, and left nothing on screen.
    Empty,
}

impl Error {
    pub(crate) fn unsupported(what: impl Into<String>) -> Self {
        Self::Unsupported(what.into())
    }
}

/// The last frame of the scene in `src`, cropped, as a PNG.
pub(crate) fn render(src: &str, quality: char) -> Result<Vec<u8>, Error> {
    if src.len() > MAX_LEN {
        return Err(Error::unsupported("a block this long"));
    }
    let lines = lex::lines(src)?;
    let stmts = parse::program(&lines)?;
    let mut scene = eval::Interp::new();
    scene.run(&stmts)?;
    draw::render(&scene.arena, &scene.shown, quality)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(png: &[u8]) -> (u32, u32) {
        let r = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .expect("png");
        (r.info().width, r.info().height)
    }

    #[test]
    fn the_common_scenes_are_drawn_here() {
        for src in [
            "self.add(Circle())",
            "c = Circle(color=BLUE).set_fill(BLUE, opacity=0.5)\nsq = Square().next_to(c, RIGHT)\nself.play(Create(c), FadeIn(sq))",
            "ax = Axes(x_range=[-3, 3, 1], y_range=[-1, 3, 1])\ng = ax.plot(lambda x: x**2 / 3, color=YELLOW)\nself.add(ax, g, ax.get_graph_label(g, \"x^2/3\"))",
            "plane = NumberPlane()\nv = Arrow(ORIGIN, [2, 1, 0], buff=0, color=YELLOW)\nself.add(plane, v)",
            "t = Text(\"hello\").to_edge(UP)\neq = MathTex(r\"e^{i\\pi} + 1 = 0\")\nself.play(Write(t), Write(eq))",
            "g = VGroup(*[Dot(), Dot(), Dot()]).arrange(RIGHT, buff=1)\nself.add(g)",
            "a = Square()\nb = Circle()\nself.play(Transform(a, b))\nself.play(a.animate.shift(UP).set_color(RED))",
            "from manim import *\n\nclass Demo(Scene):\n    def construct(self):\n        self.add(Star(color=GOLD, fill_opacity=1))\n",
        ] {
            let png = render(src, 'l').unwrap_or_else(|e| panic!("{src}: {e:?}"));
            let (w, h) = size(&png);
            assert!(w > 10 && h > 10, "{src}: {w}x{h}");
        }
    }

    #[test]
    fn a_circle_is_the_size_manim_makes_it() {
        // A radius-1 circle at 854×480 is 2 units of 14.2 across, plus the
        // stroke and the 4% margin on each side.
        let (w, h) = size(&render("self.add(Circle())", 'l').expect("drawn"));
        let unit: f64 = 854.0 / (128.0 / 9.0);
        let expect = (2.0 * unit + 4.0 * 0.01 * unit).round() as u32 + 2 * 34;
        assert!(
            w.abs_diff(expect) <= 3 && h.abs_diff(expect) <= 3,
            "{w}x{h}, expected ~{expect}"
        );
    }

    #[test]
    fn what_is_not_understood_goes_to_manim_and_mistakes_are_mistakes() {
        assert!(matches!(
            render("for i in range(3):\n    self.add(Dot())", 'l'),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            render("self.add(ThreeDAxes())", 'l'),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            render("self.add(Circle(", 'l'),
            Err(Error::Syntax(_))
        ));
        assert_eq!(render("c = Circle()", 'l'), Err(Error::Empty));
        assert!(matches!(
            render("self.play(FadeOut(Circle()))", 'l'),
            Err(Error::Empty)
        ));
    }
}
