//! Reading and writing the text inside a ```` ```draw ```` block.
//!
//! One statement per line:
//!
//! ```text
//! size 60x14
//! rect 2,1 14x5 "Parser" fill
//! ellipse 20,1 14x5 "Start" color=green
//! diamond 40,1 14x7 "ok?" dashed
//! line 16,3 -> 20,3 "AST"
//! line 2,10 -> 10,10 -> 10,14 <-> 30,14
//! arrow 5,5 9,9                 # same as: line 5,5 -> 9,9
//! text 5,12 "free text" color=muted
//! # a comment
//! ```
//!
//! Points are `column,row`; sizes are `WIDTHxHEIGHT`; both in cells,
//! counted from the top-left of the block. Connectors between points are
//! `->`, `<-`, `<->` or `--` (no head). Anything in quotes is a label.
//! Flags are `fill`, `dashed`, `round` and `color=NAME`, in any order.
//!
//! Lines that do not parse are kept as [`Shape::Raw`] and written back
//! unchanged, so a hand-edited block is never damaged by the editor.

use std::fmt::Write as _;

use crate::model::{Attrs, BoxKind, Drawing, Heads, Point, Rect, Shape};

/// Parses a block. Never fails: unreadable lines become [`Shape::Raw`].
pub fn parse(src: &str) -> Drawing {
    let mut drawing = Drawing::default();
    for line in src.lines() {
        match parse_line(line) {
            Statement::Size(w, h) if drawing.size.is_none() => drawing.size = Some((w, h)),
            Statement::Size(..) => drawing.shapes.push(Shape::Raw(line.to_owned())),
            Statement::Shape(shape) => drawing.shapes.push(shape),
            // Blank lines and comments survive as raw lines.
            Statement::Blank | Statement::Unknown => {
                drawing.shapes.push(Shape::Raw(line.to_owned()));
            }
        }
    }
    drawing
}

/// Writes a drawing in canonical form. `parse(&to_source(&d)) == d` for
/// any drawing whose raw lines were themselves produced by this function
/// or are comments.
pub fn to_source(drawing: &Drawing) -> String {
    let mut out = String::new();
    if let Some((w, h)) = drawing.size {
        let _ = writeln!(out, "size {w}x{h}");
    }
    for shape in &drawing.shapes {
        match shape {
            Shape::Box {
                kind,
                rect,
                label,
                attrs,
            } => {
                let _ = write!(
                    out,
                    "{} {},{} {}x{}",
                    kind.keyword(),
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h
                );
                push_label(&mut out, label.as_deref());
                push_attrs(&mut out, attrs, *kind == BoxKind::Rect);
            }
            Shape::Line {
                points,
                heads,
                label,
                attrs,
            } => {
                out.push_str("line");
                let last = points.len().saturating_sub(1);
                for (i, p) in points.iter().enumerate() {
                    if i > 0 {
                        let start = i == 1 && heads.start;
                        let end = i == last && heads.end;
                        out.push_str(match (start, end) {
                            (true, true) => " <->",
                            (true, false) => " <-",
                            (false, true) => " ->",
                            (false, false) => " --",
                        });
                    }
                    let _ = write!(out, " {},{}", p.x, p.y);
                }
                push_label(&mut out, label.as_deref());
                push_attrs(&mut out, attrs, false);
            }
            Shape::Text { at, text, attrs } => {
                let _ = write!(out, "text {},{}", at.x, at.y);
                push_label(&mut out, Some(text));
                push_attrs(&mut out, attrs, false);
            }
            Shape::Raw(line) => out.push_str(line),
        }
        out.push('\n');
    }
    out
}

fn push_label(out: &mut String, label: Option<&str>) {
    if let Some(label) = label {
        out.push(' ');
        out.push_str(&quote(label));
    }
}

fn push_attrs(out: &mut String, attrs: &Attrs, allow_round: bool) {
    if attrs.fill {
        out.push_str(" fill");
    }
    if attrs.dashed {
        out.push_str(" dashed");
    }
    if attrs.round && allow_round {
        out.push_str(" round");
    }
    if let Some(color) = &attrs.color {
        let _ = write!(out, " color={color}");
    }
}

/// Quotes a label: `"` and `\` are escaped.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

enum Statement {
    Size(i32, i32),
    Shape(Shape),
    Blank,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    Quoted(String),
}

/// Splits a line into words and quoted strings. A `#` outside quotes
/// starts a comment.
fn tokenize(line: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '#' {
            break;
        } else if c == '"' {
            chars.next();
            let mut s = String::new();
            loop {
                match chars.next() {
                    None => return None, // unterminated quote
                    Some('"') => break,
                    Some('\\') => s.push(chars.next()?),
                    Some(ch) => s.push(ch),
                }
            }
            tokens.push(Token::Quoted(s));
        } else {
            let mut w = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() || ch == '"' || ch == '#' {
                    break;
                }
                w.push(ch);
                chars.next();
            }
            tokens.push(Token::Word(w));
        }
    }
    Some(tokens)
}

fn parse_line(line: &str) -> Statement {
    let Some(tokens) = tokenize(line) else {
        return Statement::Unknown;
    };
    let Some(Token::Word(keyword)) = tokens.first() else {
        return if tokens.is_empty() {
            Statement::Blank
        } else {
            Statement::Unknown
        };
    };
    let rest = &tokens[1..];
    let result = match keyword.as_str() {
        "size" => match rest {
            [Token::Word(s)] => parse_size(s).map(|(w, h)| Statement::Size(w, h)),
            _ => None,
        },
        "rect" => parse_box(BoxKind::Rect, rest),
        "ellipse" | "circle" => parse_box(BoxKind::Ellipse, rest),
        "diamond" => parse_box(BoxKind::Diamond, rest),
        "line" => parse_polyline(rest, false),
        "arrow" => parse_polyline(rest, true),
        "text" => parse_text(rest),
        _ => None,
    };
    result.unwrap_or(Statement::Unknown)
}

fn parse_box(kind: BoxKind, rest: &[Token]) -> Option<Statement> {
    let [Token::Word(pos), Token::Word(size), tail @ ..] = rest else {
        return None;
    };
    let at = parse_point(pos)?;
    let (w, h) = parse_size(size)?;
    let (label, attrs) = parse_tail(tail, kind == BoxKind::Rect)?;
    Some(Statement::Shape(Shape::Box {
        kind,
        rect: Rect::new(at.x, at.y, w, h),
        label,
        attrs,
    }))
}

fn parse_polyline(rest: &[Token], arrow_keyword: bool) -> Option<Statement> {
    let mut points = Vec::new();
    let mut heads = Heads::default();
    let mut i = 0;
    let mut expect_point = true;
    let mut connectors = 0;
    while let Some(Token::Word(w)) = rest.get(i) {
        if expect_point {
            match parse_point(w) {
                Some(p) => points.push(p),
                None => break,
            }
            expect_point = false;
        } else {
            let (start, end) = match w.as_str() {
                "->" => (false, true),
                "<-" => (true, false),
                "<->" => (true, true),
                "--" | "-" => (false, false),
                _ => break,
            };
            if connectors == 0 && start {
                heads.start = true;
            }
            // Only the head on the final connector counts as the end head;
            // a later connector replaces the decision.
            heads.end = end;
            connectors += 1;
            expect_point = true;
        }
        i += 1;
    }
    if arrow_keyword {
        // `arrow A B C`: points may be listed without connectors.
        while let Some(Token::Word(w)) = rest.get(i) {
            let Some(p) = parse_point(w) else {
                break;
            };
            points.push(p);
            i += 1;
        }
        heads.end = true;
    }
    if points.len() < 2 || expect_point {
        return None;
    }
    let (label, attrs) = parse_tail(&rest[i..], false)?;
    Some(Statement::Shape(Shape::Line {
        points,
        heads,
        label,
        attrs,
    }))
}

fn parse_text(rest: &[Token]) -> Option<Statement> {
    let [Token::Word(pos), tail @ ..] = rest else {
        return None;
    };
    let at = parse_point(pos)?;
    let (label, attrs) = parse_tail(tail, false)?;
    let text = label?;
    if text.is_empty() {
        return None;
    }
    Some(Statement::Shape(Shape::Text { at, text, attrs }))
}

/// Parses the label and flags that may follow a shape's geometry.
fn parse_tail(tail: &[Token], allow_round: bool) -> Option<(Option<String>, Attrs)> {
    let mut label = None;
    let mut attrs = Attrs::default();
    for token in tail {
        match token {
            Token::Quoted(s) => {
                if label.is_some() {
                    return None;
                }
                label = Some(s.clone());
            }
            Token::Word(w) => match w.as_str() {
                "fill" | "filled" => attrs.fill = true,
                "dashed" | "dash" => attrs.dashed = true,
                "round" | "rounded" if allow_round => attrs.round = true,
                _ => {
                    let color = w
                        .strip_prefix("color=")
                        .or_else(|| w.strip_prefix("color:"))?;
                    if color.is_empty() {
                        return None;
                    }
                    attrs.color = Some(color.to_owned());
                }
            },
        }
    }
    Some((label, attrs))
}

fn parse_point(s: &str) -> Option<Point> {
    let (x, y) = s.split_once(',')?;
    Some(Point::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn parse_size(s: &str) -> Option<(i32, i32)> {
    let (w, h) = s.split_once(['x', 'X'])?;
    let (w, h): (i32, i32) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (w >= 1 && h >= 1).then_some((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
size 60x14
rect 2,1 14x5 \"Parser\" fill
ellipse 20,1 14x5 \"Start\" color=green
diamond 40,1 14x7 \"ok?\" dashed
line 16,3 -> 20,3 \"AST\"
line 2,10 <- 10,10 -- 10,14 -> 30,14
text 5,12 \"free text\" color=muted
";

    #[test]
    fn parses_every_statement() {
        let d = parse(SAMPLE);
        assert_eq!(d.size, Some((60, 14)));
        assert_eq!(d.shapes.len(), 6);
        assert_eq!(
            d.shapes[0],
            Shape::Box {
                kind: BoxKind::Rect,
                rect: Rect::new(2, 1, 14, 5),
                label: Some("Parser".into()),
                attrs: Attrs {
                    fill: true,
                    ..Attrs::default()
                },
            }
        );
        assert!(
            matches!(&d.shapes[1], Shape::Box { kind: BoxKind::Ellipse, attrs, .. } if attrs.color.as_deref() == Some("green"))
        );
        assert!(
            matches!(&d.shapes[2], Shape::Box { kind: BoxKind::Diamond, attrs, .. } if attrs.dashed)
        );
        assert_eq!(
            d.shapes[3],
            Shape::Line {
                points: vec![Point::new(16, 3), Point::new(20, 3)],
                heads: Heads {
                    start: false,
                    end: true
                },
                label: Some("AST".into()),
                attrs: Attrs::default(),
            }
        );
        assert!(
            matches!(&d.shapes[4], Shape::Line { points, heads, .. } if points.len() == 4 && heads.start && heads.end)
        );
        assert_eq!(
            d.shapes[5],
            Shape::Text {
                at: Point::new(5, 12),
                text: "free text".into(),
                attrs: Attrs {
                    color: Some("muted".into()),
                    ..Attrs::default()
                },
            }
        );
    }

    #[test]
    fn round_trips_canonical_source() {
        let d = parse(SAMPLE);
        let src = to_source(&d);
        assert_eq!(src, SAMPLE);
        assert_eq!(parse(&src), d);
    }

    #[test]
    fn keeps_comments_and_unknown_lines() {
        let src = "# my drawing\nrect 0,0 3x2\nbanana 1,2\nrect 0,0\n\n";
        let d = parse(src);
        assert_eq!(d.shapes[0], Shape::Raw("# my drawing".into()));
        assert!(matches!(d.shapes[1], Shape::Box { .. }));
        assert_eq!(d.shapes[2], Shape::Raw("banana 1,2".into()));
        assert_eq!(d.shapes[3], Shape::Raw("rect 0,0".into()));
        assert_eq!(
            to_source(&d),
            "# my drawing\nrect 0,0 3x2\nbanana 1,2\nrect 0,0\n\n"
        );
    }

    #[test]
    fn arrow_alias_and_flexible_syntax() {
        let d = parse("arrow 1,1 5,1 5,4\nrect 0,0 4x4 round color:red \"a \\\"quoted\\\" label\"\nline 0,0 - 3,3 # comment\n");
        assert!(
            matches!(&d.shapes[0], Shape::Line { points, heads, .. } if points.len() == 3 && !heads.start && heads.end)
        );
        assert!(
            matches!(&d.shapes[1], Shape::Box { label: Some(l), attrs, .. } if l == "a \"quoted\" label" && attrs.round && attrs.color.as_deref() == Some("red"))
        );
        assert!(matches!(&d.shapes[2], Shape::Line { points, .. } if points.len() == 2));
        assert_eq!(
            to_source(&d).lines().nth(1).unwrap(),
            "rect 0,0 4x4 \"a \\\"quoted\\\" label\" round color=red"
        );
    }

    #[test]
    fn size_only_once_and_must_be_positive() {
        let d = parse("size 0x5\nsize 4x4\nsize 9x9\n");
        assert_eq!(d.size, Some((4, 4)));
        assert_eq!(d.shapes.len(), 2); // both rejected lines are kept raw
    }

    #[test]
    fn tokenizer_handles_quotes() {
        assert_eq!(
            tokenize("text 1,2 \"a b\" color=red # c \"x\"").unwrap(),
            vec![
                Token::Word("text".into()),
                Token::Word("1,2".into()),
                Token::Quoted("a b".into()),
                Token::Word("color=red".into()),
            ]
        );
        assert!(tokenize("text 1,2 \"open").is_none());
    }
}
