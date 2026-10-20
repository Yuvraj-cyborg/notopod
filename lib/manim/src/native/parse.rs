//! Logical lines to statements: the straight-line part of Python that
//! most scenes are written in.
//!
//! A scene is a list of assignments and calls; expressions are numbers,
//! strings, names, arithmetic, calls, attributes, indexing, lists,
//! tuples, dicts and lambdas. Loops, conditions, functions and classes
//! of one's own are real Python's job.

use super::lex::{Line, Tok};
use super::Error;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
    Num(f64),
    Str(String),
    Name(String),
    List(Vec<Expr>),
    Dict(Vec<(String, Expr)>),
    Neg(Box<Expr>),
    Bin(char, Box<Expr>, Box<Expr>),
    Attr(Box<Expr>, String),
    Call(Box<Expr>, Vec<Arg>),
    Index(Box<Expr>, Box<Expr>),
    Lambda(Vec<String>, Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Arg {
    Pos(Expr),
    Star(Expr),
    Key(String, Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Stmt {
    Assign(Vec<String>, Expr),
    Expr(Expr),
}

/// The statements of a scene: the body of its `construct`, or the whole
/// block when it is only a body.
pub(crate) fn program(lines: &[Line]) -> Result<Vec<Stmt>, Error> {
    let mut body: Vec<&Line> = Vec::new();
    let mut i = 0;
    let mut in_scene = false;
    while i < lines.len() {
        let line = &lines[i];
        if is_import(&line.toks)? {
            i += 1;
            continue;
        }
        if let [Tok::Name(kw), ..] = line.toks.as_slice() {
            if kw == "class" {
                if in_scene || !body.is_empty() {
                    return Err(Error::unsupported("more than one class"));
                }
                let (start, end) = scene_body(lines, i)?;
                body.extend(&lines[start..end]);
                in_scene = true;
                i = end;
                continue;
            }
        }
        if in_scene {
            return Err(Error::unsupported("code outside the scene"));
        }
        body.push(line);
        i += 1;
    }
    let Some(first) = body.first() else {
        return Ok(Vec::new());
    };
    let indent = first.indent;
    body.iter()
        .map(|line| {
            if line.indent != indent {
                return Err(Error::unsupported("indented blocks"));
            }
            statement(&line.toks)
        })
        .filter(|s| !matches!(s, Ok(None)))
        .map(|s| s.map(|s| s.expect("filtered")))
        .collect()
}

fn is_import(toks: &[Tok]) -> Result<bool, Error> {
    let words: Vec<&str> = toks
        .iter()
        .map(|t| match t {
            Tok::Name(n) => n.as_str(),
            Tok::Op(o) => o,
            _ => "",
        })
        .collect();
    Ok(match words.as_slice() {
        ["from", "manim", "import", ..]
        | ["import", "manim" | "math", ..]
        | ["import", "numpy", "as", "np"]
        | ["import", "numpy"] => true,
        ["from" | "import", module, ..] => {
            return Err(Error::unsupported(format!("importing {module}")));
        }
        _ => false,
    })
}

/// The lines of `def construct(self):` inside the class starting at
/// `class_line`, and where the class ends.
fn scene_body(lines: &[Line], class_line: usize) -> Result<(usize, usize), Error> {
    let header = &lines[class_line];
    let is_scene = matches!(
        header.toks.as_slice(),
        [Tok::Name(_), Tok::Name(_), Tok::Op("("), Tok::Name(base), Tok::Op(")"), Tok::Op(":")]
            if base == "Scene"
    );
    if !is_scene {
        return Err(Error::unsupported("scenes that are not a plain Scene"));
    }
    let end = lines[class_line + 1..]
        .iter()
        .position(|l| l.indent <= header.indent)
        .map_or(lines.len(), |p| class_line + 1 + p);
    let members = &lines[class_line + 1..end];
    let construct = matches!(
        members.first().map(|l| l.toks.as_slice()),
        Some([Tok::Name(def), Tok::Name(name), Tok::Op("("), Tok::Name(this), Tok::Op(")"), Tok::Op(":")])
            if def == "def" && name == "construct" && this == "self"
    );
    if !construct {
        return Err(Error::unsupported("scene methods other than construct"));
    }
    let def_indent = members[0].indent;
    if members[1..].iter().any(|l| l.indent <= def_indent) {
        return Err(Error::unsupported("scene methods other than construct"));
    }
    Ok((class_line + 2, end))
}

const KEYWORDS: [&str; 20] = [
    "for", "while", "if", "elif", "else", "def", "class", "with", "return", "try", "except",
    "finally", "raise", "yield", "global", "nonlocal", "del", "assert", "async", "await",
];

fn statement(toks: &[Tok]) -> Result<Option<Stmt>, Error> {
    if let [Tok::Name(kw), ..] = toks {
        if kw == "pass" && toks.len() == 1 {
            return Ok(None);
        }
        if KEYWORDS.contains(&kw.as_str()) {
            return Err(Error::unsupported(format!("`{kw}`")));
        }
    }
    if toks.iter().any(|t| matches!(t, Tok::Op(";"))) {
        return Err(Error::unsupported("`;`"));
    }
    if toks.last() == Some(&Tok::Op(":")) {
        return Err(Error::unsupported("indented blocks"));
    }
    // An `=` outside any bracket is an assignment.
    let mut depth = 0i32;
    let mut eq = None;
    for (i, t) in toks.iter().enumerate() {
        match t {
            Tok::Op("(" | "[" | "{") => depth += 1,
            Tok::Op(")" | "]" | "}") => depth -= 1,
            Tok::Op("=") if depth == 0 && eq.is_none() => eq = Some(i),
            Tok::Op("+=" | "-=" | "*=" | "/=" | "**=" | "//=") if depth == 0 => {
                return Err(Error::unsupported("augmented assignment"));
            }
            _ => {}
        }
    }
    if let Some(eq) = eq {
        let mut targets = Vec::new();
        for (i, t) in toks[..eq].iter().enumerate() {
            match (i % 2, t) {
                (0, Tok::Name(n)) => targets.push(n.clone()),
                (1, Tok::Op(",")) => {}
                _ => return Err(Error::unsupported("assigning to anything but names")),
            }
        }
        if targets.is_empty() {
            return Err(Error::Syntax("nothing to assign to".to_owned()));
        }
        return Ok(Some(Stmt::Assign(targets, whole(&toks[eq + 1..])?)));
    }
    Ok(Some(Stmt::Expr(whole(toks)?)))
}

/// An expression that must use every token, or a tuple of them.
fn whole(toks: &[Tok]) -> Result<Expr, Error> {
    let mut p = Parser { toks, at: 0 };
    let first = p.expr()?;
    let expr = if p.eat(",") {
        let mut items = vec![first];
        while p.peek().is_some() {
            items.push(p.expr()?);
            if !p.eat(",") {
                break;
            }
        }
        Expr::List(items)
    } else {
        first
    };
    match p.peek() {
        None => Ok(expr),
        Some(t) => Err(Error::unsupported(describe(t))),
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Name(n) => format!("`{n}`"),
        Tok::Op(o) => format!("`{o}`"),
        Tok::Num(_) | Tok::Str(_) => "that expression".to_owned(),
    }
}

struct Parser<'t> {
    toks: &'t [Tok],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn eat(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, op: &str) -> Result<(), Error> {
        if self.eat(op) {
            Ok(())
        } else {
            Err(match self.peek() {
                Some(t) => Error::unsupported(describe(t)),
                None => Error::Syntax(format!("expected '{op}'")),
            })
        }
    }

    fn expr(&mut self) -> Result<Expr, Error> {
        if matches!(self.peek(), Some(Tok::Name(n)) if n == "lambda") {
            self.at += 1;
            let mut params = Vec::new();
            while let Some(Tok::Name(n)) = self.peek() {
                params.push(n.clone());
                self.at += 1;
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(":")?;
            let body = self.expr()?;
            return Ok(Expr::Lambda(params, Box::new(body)));
        }
        let e = self.sum()?;
        if let Some(Tok::Name(n)) = self.peek() {
            if matches!(
                n.as_str(),
                "if" | "and" | "or" | "not" | "in" | "is" | "for"
            ) {
                return Err(Error::unsupported(format!("`{n}`")));
            }
        }
        if let Some(Tok::Op(o @ ("==" | "!=" | "<" | ">" | "<=" | ">="))) = self.peek() {
            return Err(Error::unsupported(format!("`{o}`")));
        }
        Ok(e)
    }

    fn sum(&mut self) -> Result<Expr, Error> {
        let mut e = self.term()?;
        loop {
            let op = if self.eat("+") {
                '+'
            } else if self.eat("-") {
                '-'
            } else {
                return Ok(e);
            };
            e = Expr::Bin(op, Box::new(e), Box::new(self.term()?));
        }
    }

    fn term(&mut self) -> Result<Expr, Error> {
        let mut e = self.unary()?;
        loop {
            let op = if self.eat("*") {
                '*'
            } else if self.eat("/") {
                '/'
            } else if self.eat("//") {
                'f'
            } else if self.eat("%") {
                '%'
            } else {
                return Ok(e);
            };
            e = Expr::Bin(op, Box::new(e), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Result<Expr, Error> {
        if self.eat("-") {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.eat("+") {
            return self.unary();
        }
        self.power()
    }

    fn power(&mut self) -> Result<Expr, Error> {
        let base = self.postfix()?;
        if self.eat("**") {
            return Ok(Expr::Bin('^', Box::new(base), Box::new(self.unary()?)));
        }
        Ok(base)
    }

    fn postfix(&mut self) -> Result<Expr, Error> {
        let mut e = self.atom()?;
        loop {
            if self.eat("(") {
                e = Expr::Call(Box::new(e), self.args()?);
            } else if self.eat(".") {
                let Some(Tok::Name(n)) = self.peek().cloned() else {
                    return Err(Error::Syntax("expected a name after '.'".to_owned()));
                };
                self.at += 1;
                e = Expr::Attr(Box::new(e), n);
            } else if self.eat("[") {
                let index = self.expr()?;
                if self.peek() == Some(&Tok::Op(":")) {
                    return Err(Error::unsupported("slices"));
                }
                self.expect("]")?;
                e = Expr::Index(Box::new(e), Box::new(index));
            } else {
                return Ok(e);
            }
        }
    }

    fn args(&mut self) -> Result<Vec<Arg>, Error> {
        let mut args = Vec::new();
        while !self.eat(")") {
            if self.eat("**") {
                return Err(Error::unsupported("`**` arguments"));
            }
            if self.eat("*") {
                args.push(Arg::Star(self.expr()?));
            } else if let (Some(Tok::Name(n)), Some(Tok::Op("="))) =
                (self.peek().cloned(), self.toks.get(self.at + 1))
            {
                self.at += 2;
                args.push(Arg::Key(n, self.expr()?));
            } else {
                args.push(Arg::Pos(self.expr()?));
            }
            if !self.eat(",") {
                self.expect(")")?;
                break;
            }
        }
        Ok(args)
    }

    fn atom(&mut self) -> Result<Expr, Error> {
        let Some(tok) = self.peek().cloned() else {
            return Err(Error::Syntax("unexpected end of line".to_owned()));
        };
        self.at += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Str(mut s) => {
                // Adjacent strings join, as in Python.
                while let Some(Tok::Str(more)) = self.peek() {
                    s.push_str(more);
                    self.at += 1;
                }
                Ok(Expr::Str(s))
            }
            Tok::Name(n) => Ok(Expr::Name(n)),
            Tok::Op("(") => {
                if self.eat(")") {
                    return Ok(Expr::List(Vec::new()));
                }
                let first = self.expr()?;
                if self.eat(")") {
                    return Ok(first);
                }
                let mut items = vec![first];
                while self.eat(",") {
                    if self.eat(")") {
                        return Ok(Expr::List(items));
                    }
                    items.push(self.expr()?);
                }
                self.expect(")")?;
                Ok(Expr::List(items))
            }
            Tok::Op("[") => {
                let mut items = Vec::new();
                while !self.eat("]") {
                    items.push(self.expr()?);
                    if matches!(self.peek(), Some(Tok::Name(n)) if n == "for") {
                        return Err(Error::unsupported("list comprehensions"));
                    }
                    if !self.eat(",") {
                        self.expect("]")?;
                        break;
                    }
                }
                Ok(Expr::List(items))
            }
            Tok::Op("{") => {
                let mut items = Vec::new();
                while !self.eat("}") {
                    let Some(Tok::Str(key)) = self.peek().cloned() else {
                        return Err(Error::unsupported("dicts with keys that are not strings"));
                    };
                    self.at += 1;
                    self.expect(":")?;
                    items.push((key, self.expr()?));
                    if !self.eat(",") {
                        self.expect("}")?;
                        break;
                    }
                }
                Ok(Expr::Dict(items))
            }
            Tok::Op(o) => Err(Error::unsupported(format!("`{o}`"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::lex::lines;
    use super::*;

    fn parse(src: &str) -> Result<Vec<Stmt>, Error> {
        program(&lines(src)?)
    }

    #[test]
    fn a_bare_body_and_a_scene_class_give_the_same_statements() {
        let body = "c = Circle(radius=2, color=BLUE)\nself.play(Create(c))\n";
        let mut class =
            String::from("from manim import *\n\nclass Demo(Scene):\n    def construct(self):\n");
        for line in body.lines() {
            class.push_str("        ");
            class.push_str(line);
            class.push('\n');
        }
        assert_eq!(parse(body).expect("body"), parse(&class).expect("class"));
        let stmts = parse(body).expect("body");
        assert!(matches!(&stmts[0], Stmt::Assign(names, Expr::Call(..)) if names == &["c"]));
    }

    #[test]
    fn expressions_bind_like_python() {
        let stmts = parse("x = -2**2 + 3*UP/2").expect("parses");
        let [Stmt::Assign(_, e)] = stmts.as_slice() else {
            panic!("{stmts:?}")
        };
        // -(2**2) + ((3*UP)/2)
        assert!(matches!(e, Expr::Bin('+', l, _) if matches!(**l, Expr::Neg(_))));
        assert!(parse("g = ax.plot(lambda x: np.sin(x), color=YELLOW)").is_ok());
        assert!(parse("a, b = Square(), Circle()").is_ok());
        assert!(parse("ax = Axes(axis_config={\"include_numbers\": True})").is_ok());
    }

    #[test]
    fn real_python_is_left_to_python() {
        for src in [
            "for i in range(3):\n    self.add(Dot())",
            "if True:\n    pass",
            "import random",
            "x += 1",
            "self.add(*[Dot() for _ in range(3)])",
            "x = a if b else c",
            "class A(ThreeDScene):\n    def construct(self):\n        pass",
        ] {
            assert!(matches!(parse(src), Err(Error::Unsupported(_))), "{src}");
        }
    }
}
