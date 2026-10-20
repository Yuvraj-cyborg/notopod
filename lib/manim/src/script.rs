//! Turning a block into a file manim can run.

/// The class a block is wrapped in when it is only the body of
/// `construct`.
const WRAPPER: &str = "NotopodScene";

/// The Python for a block and the name of the scene to render.
///
/// A block that defines a `Scene` subclass is run as it is (with
/// `from manim import *` added when it imports nothing from manim), and
/// its last scene is the one rendered. Anything else is taken to be the
/// body of `construct` and wrapped, so a note can say just
/// `self.play(Create(Circle()))`.
pub fn script(src: &str) -> (String, String) {
    let imports = src.lines().any(|l| {
        let l = l.trim_start();
        l.starts_with("from manim") || l.starts_with("import manim")
    });
    let header = if imports {
        ""
    } else {
        "from manim import *\n\n"
    };
    if let Some(scene) = last_scene(src) {
        return (format!("{header}{src}"), scene);
    }
    let body = dedent(src);
    // `from x import *` is not allowed inside a function, so the body's
    // own imports go above the class.
    let is_import = |l: &str| l.starts_with("import ") || l.starts_with("from ");
    let mut code = header.to_owned();
    for line in body.lines().filter(|l| is_import(l)) {
        code.push_str(line);
        code.push('\n');
    }
    let body: Vec<&str> = body.lines().filter(|l| !is_import(l)).collect();
    code.push_str("class ");
    code.push_str(WRAPPER);
    code.push_str("(Scene):\n    def construct(self):\n");
    if body.iter().all(|l| l.trim().is_empty()) {
        code.push_str("        pass\n");
    }
    for line in body {
        if line.trim().is_empty() {
            code.push('\n');
        } else {
            code.push_str("        ");
            code.push_str(line);
            code.push('\n');
        }
    }
    (code, WRAPPER.to_owned())
}

/// The name of the last `class Name(...Scene...):` at the start of a line.
fn last_scene(src: &str) -> Option<String> {
    src.lines().rev().find_map(|line| {
        let rest = line.strip_prefix("class ")?;
        let (name, bases) = rest.split_once('(')?;
        let name = name.trim();
        let is_ident = !name.is_empty()
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !name.starts_with(|c: char| c.is_ascii_digit());
        (is_ident && bases.contains("Scene")).then(|| name.to_owned())
    })
}

/// `src` with the indentation common to all its non-blank lines removed.
fn dedent(src: &str) -> String {
    let indent = src
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    src.lines()
        .map(|l| {
            if l.len() >= indent {
                &l[indent..]
            } else {
                l.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_is_wrapped_in_a_scene() {
        let (code, scene) = script("c = Circle()\nself.play(Create(c))\n");
        assert_eq!(scene, "NotopodScene");
        assert_eq!(
            code,
            "from manim import *\n\nclass NotopodScene(Scene):\n    def construct(self):\n        c = Circle()\n        self.play(Create(c))\n"
        );
        // Indented bodies are dedented first, blank lines kept.
        let (code, _) = script("    a = 1\n\n    if a:\n        b = 2\n");
        assert!(
            code.ends_with("        a = 1\n\n        if a:\n            b = 2\n"),
            "{code}"
        );
        assert!(script("").0.contains("        pass\n"));

        // Imports in a body move out of `construct`, where `*` is illegal.
        let (code, _) =
            script("from numpy import *\nimport random\nself.add(Dot(array([1, 0, 0])))\n");
        assert_eq!(
            code,
            "from manim import *\n\nfrom numpy import *\nimport random\nclass NotopodScene(Scene):\n    def construct(self):\n        self.add(Dot(array([1, 0, 0])))\n"
        );
    }

    #[test]
    fn a_whole_scene_runs_as_written() {
        let src = "from manim import *\n\nclass A(Scene):\n    def construct(self):\n        pass\n\nclass B(ThreeDScene):\n    def construct(self):\n        pass\n";
        let (code, scene) = script(src);
        assert_eq!(code, src, "nothing added when manim is imported");
        assert_eq!(scene, "B", "the last scene");

        let (code, scene) = script("class Demo(Scene):\n    def construct(self):\n        pass\n");
        assert!(code.starts_with("from manim import *\n\n"));
        assert_eq!(scene, "Demo");

        // Not a scene: a helper class is part of the body.
        assert_eq!(script("class Point:\n    pass\n").1, "NotopodScene");
    }
}
