//! The expressions a `plot` draws: `x^2`, `sin(2x)`, `e^(-x^2/2)`.
//!
//! Written the way maths is written rather than the way code is: `2x` and
//! `3sin(x)` multiply, `x^2^3` is `x^(2^3)`, `-x^2` is `-(x^2)`, `sin x`
//! applies `sin` to `x`, `|x|` is the absolute value, and `π`, `**`, `·`
//! and `−` are understood. A leading `y =` or `f(x) =` is ignored, so the
//! label of a graph can be pasted in as its expression.

use std::f64::consts::{E, PI, TAU};

/// Longest expression accepted, in bytes.
const MAX_LEN: usize = 512;

/// Deepest nesting accepted, so `((((...` cannot exhaust the stack.
const MAX_DEPTH: usize = 64;

/// A parsed expression in one variable, `x`.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr(Node);

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Num(f64),
    X,
    Neg(Box<Node>),
    Bin(Op, Box<Node>, Box<Node>),
    Call(Func, Box<Node>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Func {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Exp,
    Ln,
    Log,
    Sqrt,
    Abs,
    Floor,
    Ceil,
    Sign,
}

/// Names a word can be made of, longest first so `sinh` beats `sin`.
const NAMES: [(&str, Name); 22] = [
    ("arcsin", Name::Func(Func::Asin)),
    ("arccos", Name::Func(Func::Acos)),
    ("arctan", Name::Func(Func::Atan)),
    ("floor", Name::Func(Func::Floor)),
    ("sinh", Name::Func(Func::Sinh)),
    ("cosh", Name::Func(Func::Cosh)),
    ("tanh", Name::Func(Func::Tanh)),
    ("asin", Name::Func(Func::Asin)),
    ("acos", Name::Func(Func::Acos)),
    ("atan", Name::Func(Func::Atan)),
    ("sqrt", Name::Func(Func::Sqrt)),
    ("ceil", Name::Func(Func::Ceil)),
    ("sign", Name::Func(Func::Sign)),
    ("sin", Name::Func(Func::Sin)),
    ("cos", Name::Func(Func::Cos)),
    ("tan", Name::Func(Func::Tan)),
    ("exp", Name::Func(Func::Exp)),
    ("log", Name::Func(Func::Log)),
    ("abs", Name::Func(Func::Abs)),
    ("tau", Name::Const(TAU)),
    ("ln", Name::Func(Func::Ln)),
    ("pi", Name::Const(PI)),
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Name {
    Func(Func),
    Const(f64),
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    X,
    Const(f64),
    Func(Func),
    Op(char),
    Open,
    Close,
    Bar,
}

impl Expr {
    /// Parses `src`, or says what is wrong with it.
    pub fn parse(src: &str) -> Result<Self, String> {
        if src.len() > MAX_LEN {
            return Err(format!("longer than {MAX_LEN} bytes"));
        }
        // `y = x^2` and `f(x) = x^2` are the same graph as `x^2`.
        let src = src.rsplit_once('=').map_or(src, |(_, rhs)| rhs);
        let tokens = tokenize(src)?;
        if tokens.is_empty() {
            return Err("nothing to plot".to_owned());
        }
        let mut p = Parser {
            tokens,
            at: 0,
            depth: 0,
        };
        let node = p.expr()?;
        if p.at < p.tokens.len() {
            return Err(format!("unexpected {:?}", p.tokens[p.at]));
        }
        Ok(Self(node))
    }

    /// The value at `x`. Not finite where the expression is not defined.
    pub fn eval(&self, x: f64) -> f64 {
        eval(&self.0, x)
    }

    /// Whether `x` appears in it at all.
    pub fn uses_x(&self) -> bool {
        fn walk(node: &Node) -> bool {
            match node {
                Node::Num(_) => false,
                Node::X => true,
                Node::Neg(a) | Node::Call(_, a) => walk(a),
                Node::Bin(_, a, b) => walk(a) || walk(b),
            }
        }
        walk(&self.0)
    }
}

/// A number written as an expression without `x`, like `-2pi` or `1/3`.
pub fn constant(src: &str) -> Option<f64> {
    let e = Expr::parse(src).ok()?;
    if e.uses_x() {
        return None;
    }
    let v = e.eval(0.0);
    v.is_finite().then_some(v)
}

/// `a..b` as two numbers with `a < b`.
pub fn range(src: &str) -> Option<(f64, f64)> {
    let (lo, hi) = src.split_once("..")?;
    let (lo, hi) = (constant(lo)?, constant(hi)?);
    (lo < hi).then_some((lo, hi))
}

fn eval(node: &Node, x: f64) -> f64 {
    match node {
        Node::Num(n) => *n,
        Node::X => x,
        Node::Neg(a) => -eval(a, x),
        Node::Bin(op, a, b) => {
            let (a, b) = (eval(a, x), eval(b, x));
            match op {
                Op::Add => a + b,
                Op::Sub => a - b,
                Op::Mul => a * b,
                Op::Div => a / b,
                Op::Pow => a.powf(b),
            }
        }
        Node::Call(f, a) => {
            let a = eval(a, x);
            match f {
                Func::Sin => a.sin(),
                Func::Cos => a.cos(),
                Func::Tan => a.tan(),
                Func::Asin => a.asin(),
                Func::Acos => a.acos(),
                Func::Atan => a.atan(),
                Func::Sinh => a.sinh(),
                Func::Cosh => a.cosh(),
                Func::Tanh => a.tanh(),
                Func::Exp => a.exp(),
                Func::Ln => a.ln(),
                Func::Log => a.log10(),
                Func::Sqrt => a.sqrt(),
                Func::Abs => a.abs(),
                Func::Floor => a.floor(),
                Func::Ceil => a.ceil(),
                Func::Sign => {
                    if a == 0.0 {
                        0.0
                    } else {
                        a.signum()
                    }
                }
            }
        }
    }
}

fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() || c == '\\' => i += 1,
            '0'..='9' | '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let n: f64 = text.parse().map_err(|_| format!("bad number {text:?}"))?;
                out.push(Token::Num(n));
            }
            '*' if chars.get(i + 1) == Some(&'*') => {
                out.push(Token::Op('^'));
                i += 2;
            }
            '+' | '-' | '*' | '/' | '^' => {
                out.push(Token::Op(c));
                i += 1;
            }
            '−' => {
                out.push(Token::Op('-'));
                i += 1;
            }
            '·' | '×' => {
                out.push(Token::Op('*'));
                i += 1;
            }
            '÷' => {
                out.push(Token::Op('/'));
                i += 1;
            }
            '(' | '[' | '{' => {
                out.push(Token::Open);
                i += 1;
            }
            ')' | ']' | '}' => {
                out.push(Token::Close);
                i += 1;
            }
            '|' => {
                out.push(Token::Bar);
                i += 1;
            }
            'π' => {
                out.push(Token::Const(PI));
                i += 1;
            }
            'τ' => {
                out.push(Token::Const(TAU));
                i += 1;
            }
            c if c.is_alphabetic() => {
                let start = i;
                while i < chars.len() && chars[i].is_alphabetic() {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect::<String>().to_lowercase();
                split_word(&word, &mut out)?;
            }
            other => return Err(format!("unexpected {other:?}")),
        }
    }
    Ok(out)
}

/// A run of letters is a sequence of names and `x`s: `xsin` is `x sin`,
/// `pix` is `pi x`, `e` is Euler's number.
fn split_word(word: &str, out: &mut Vec<Token>) -> Result<(), String> {
    let mut rest = word;
    while !rest.is_empty() {
        if let Some((name, kind)) = NAMES.iter().find(|(n, _)| rest.starts_with(n)) {
            out.push(match kind {
                Name::Func(f) => Token::Func(*f),
                Name::Const(c) => Token::Const(*c),
            });
            rest = &rest[name.len()..];
        } else if let Some(tail) = rest.strip_prefix('x') {
            out.push(Token::X);
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix('e') {
            out.push(Token::Const(E));
            rest = tail;
        } else {
            return Err(format!("unknown name {rest:?} (the variable is x)"));
        }
    }
    Ok(())
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn eat(&mut self, t: &Token) -> bool {
        if self.peek() == Some(t) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn nest<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, String>) -> Result<T, String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err("nested too deeply".to_owned());
        }
        let out = f(self);
        self.depth -= 1;
        out
    }

    fn expr(&mut self) -> Result<Node, String> {
        let mut left = self.term()?;
        loop {
            let op = match self.peek() {
                Some(Token::Op('+')) => Op::Add,
                Some(Token::Op('-')) => Op::Sub,
                _ => return Ok(left),
            };
            self.at += 1;
            let right = self.term()?;
            left = Node::Bin(op, Box::new(left), Box::new(right));
        }
    }

    fn term(&mut self) -> Result<Node, String> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Token::Op('*')) => {
                    self.at += 1;
                    Op::Mul
                }
                Some(Token::Op('/')) => {
                    self.at += 1;
                    Op::Div
                }
                // `2x`, `3sin(x)`, `(x+1)(x-1)`: juxtaposition multiplies.
                Some(Token::Num(_) | Token::X | Token::Const(_) | Token::Func(_) | Token::Open) => {
                    Op::Mul
                }
                _ => return Ok(left),
            };
            let right = self.unary()?;
            left = Node::Bin(op, Box::new(left), Box::new(right));
        }
    }

    fn unary(&mut self) -> Result<Node, String> {
        if self.eat(&Token::Op('-')) {
            return self.nest(|p| Ok(Node::Neg(Box::new(p.unary()?))));
        }
        if self.eat(&Token::Op('+')) {
            return self.nest(Parser::unary);
        }
        self.power()
    }

    fn power(&mut self) -> Result<Node, String> {
        let base = self.atom()?;
        if self.eat(&Token::Op('^')) {
            let exponent = self.nest(Parser::unary)?;
            return Ok(Node::Bin(Op::Pow, Box::new(base), Box::new(exponent)));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<Node, String> {
        let Some(token) = self.peek().cloned() else {
            return Err("unfinished expression".to_owned());
        };
        self.at += 1;
        match token {
            Token::Num(n) => Ok(Node::Num(n)),
            Token::X => Ok(Node::X),
            Token::Const(c) => Ok(Node::Num(c)),
            Token::Open => {
                let inner = self.nest(Parser::expr)?;
                if !self.eat(&Token::Close) {
                    return Err("missing )".to_owned());
                }
                Ok(inner)
            }
            Token::Bar => {
                let inner = self.nest(Parser::expr)?;
                if !self.eat(&Token::Bar) {
                    return Err("missing |".to_owned());
                }
                Ok(Node::Call(Func::Abs, Box::new(inner)))
            }
            Token::Func(f) => {
                // `sin(x)^2` squares the sine; without brackets, `sin x^2`
                // takes the sine of the square, as on paper.
                let arg = if self.peek() == Some(&Token::Open) {
                    self.nest(Parser::atom)?
                } else {
                    self.nest(Parser::power)?
                };
                Ok(Node::Call(f, Box::new(arg)))
            }
            Token::Op(c) => Err(format!("unexpected {c:?}")),
            Token::Close => Err("unexpected )".to_owned()),
        }
    }
}

/// A tick step for `span`: 1, 2 or 5 times a power of ten, so that about
/// `max_ticks` ticks fit.
pub fn nice_step(span: f64, max_ticks: f64) -> f64 {
    let raw = (span / max_ticks.max(1.0)).abs().max(f64::MIN_POSITIVE);
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    };
    step * mag
}

/// `v` as a tick label at a step of `step`: no more decimals than the step
/// has, and never `-0`.
pub fn tick_label(v: f64, step: f64) -> String {
    let decimals = (-step.log10().floor()).max(0.0) as usize;
    let s = format!("{v:.decimals$}");
    if s.trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        "0".to_owned()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(src: &str, x: f64) -> f64 {
        Expr::parse(src)
            .unwrap_or_else(|e| panic!("{src}: {e}"))
            .eval(x)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn arithmetic_with_maths_precedence() {
        assert!(close(at("1 + 2*3", 0.0), 7.0));
        assert!(close(at("2^3^2", 0.0), 512.0), "right associative");
        assert!(
            close(at("-x^2", 3.0), -9.0),
            "minus binds looser than power"
        );
        assert!(close(at("2^-1", 0.0), 0.5));
        assert!(close(at("(1+2)(3+4)", 0.0), 21.0));
        assert!(close(at("x**2", 4.0), 16.0));
        assert!(close(at("6 ÷ 2 · 3", 0.0), 9.0));
        assert!(close(at("−x", 2.0), -2.0));
    }

    #[test]
    fn implicit_multiplication_names_and_functions() {
        assert!(close(at("2x", 3.0), 6.0));
        assert!(close(at("3sin(x)", PI / 2.0), 3.0));
        assert!(close(at("sin x", PI / 2.0), 1.0));
        assert!(
            close(at("sin(x)^2", PI / 2.0), 1.0),
            "the power is on the sine"
        );
        assert!(close(at("sin x^2", 2.0), 4f64.sin()), "the power is on x");
        assert!(close(at("2pi", 0.0), TAU));
        assert!(close(at("π", 0.0), PI));
        assert!(close(at("e^x", 1.0), E));
        assert!(close(at("e^(-x^2)", 0.0), 1.0));
        assert!(close(at("|x - 5|", 2.0), 3.0));
        assert!(close(at("sqrt(16)", 0.0), 4.0));
        assert!(close(at("ln(e)", 0.0), 1.0));
        assert!(close(at("log(1000)", 0.0), 3.0));
        assert!(close(at("sinh(0) + cosh(0)", 0.0), 1.0));
        assert!(close(at("xsin(x)", PI / 2.0), PI / 2.0), "x sin(x)");
        assert!(
            close(at("\\sin(x)", PI / 2.0), 1.0),
            "a LaTeX backslash is fine"
        );
    }

    #[test]
    fn a_leading_equation_is_ignored() {
        assert!(close(at("y = x^2", 3.0), 9.0));
        assert!(close(at("f(x) = 2x + 1", 1.0), 3.0));
    }

    #[test]
    fn undefined_points_are_not_finite() {
        assert!(!at("1/x", 0.0).is_finite());
        assert!(at("ln(x)", -1.0).is_nan());
        assert!(at("sqrt(x)", -4.0).is_nan());
    }

    #[test]
    fn bad_input_is_an_error() {
        for src in ["", "+", "x +", "(x", "x)", "|x", "y", "foo(x)", "2..3", "!"] {
            assert!(Expr::parse(src).is_err(), "{src:?} should not parse");
        }
        assert!(Expr::parse(&"(".repeat(200)).is_err(), "too deep");
        assert!(Expr::parse(&"x+".repeat(400)).is_err(), "too long");
    }

    #[test]
    fn constants_and_ranges() {
        assert_eq!(constant("-2"), Some(-2.0));
        assert!(constant("2pi").is_some_and(|v| close(v, TAU)));
        assert_eq!(constant("x"), None, "depends on x");
        assert!(range("-pi..pi").is_some_and(|(a, b)| close(a, -PI) && close(b, PI)));
        assert_eq!(range("3..1"), None, "backwards");
        assert_eq!(range("1"), None);
    }

    #[test]
    fn ticks() {
        assert!(close(nice_step(10.0, 5.0), 2.0));
        assert!(close(nice_step(1.0, 4.0), 0.5));
        assert!(close(nice_step(100.0, 8.0), 20.0));
        assert_eq!(tick_label(2.0, 1.0), "2");
        assert_eq!(tick_label(0.5, 0.5), "0.5");
        assert_eq!(tick_label(-0.0, 1.0), "0");
        assert_eq!(tick_label(-1e-12, 0.1), "0");
    }
}
