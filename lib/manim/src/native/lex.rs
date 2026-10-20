//! Python source to logical lines of tokens.
//!
//! Only as much of Python's lexical grammar as the scenes notopod draws
//! itself need. What it does not know it calls unsupported, so the block
//! goes to real Manim; it only calls something a syntax error when Python
//! would too (an unclosed bracket or string).

use super::Error;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tok {
    Name(String),
    Num(f64),
    Str(String),
    Op(&'static str),
}

/// One logical line: physical lines joined inside brackets and after `\`.
#[derive(Debug)]
pub(crate) struct Line {
    pub indent: usize,
    pub toks: Vec<Tok>,
}

const OPS: [&str; 37] = [
    "**=", "//=", "**", "//", "==", "!=", "<=", ">=", "->", "+=", "-=", "*=", "/=", ":=", "+", "-",
    "*", "/", "%", "(", ")", "[", "]", "{", "}", ",", ".", ":", "=", "<", ">", "@", "&", "|", "^",
    "~", ";",
];

pub(crate) fn lines(src: &str) -> Result<Vec<Line>, Error> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut toks = Vec::new();
    let mut closers: Vec<char> = Vec::new();
    let mut indent = 0;
    let mut at_line_start = true;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if at_line_start {
            let mut col = 0;
            while i < chars.len() && (chars[i] == ' ' || chars[i] == '\t') {
                col = if chars[i] == '\t' {
                    (col / 8 + 1) * 8
                } else {
                    col + 1
                };
                i += 1;
            }
            if i >= chars.len() {
                break;
            }
            if chars[i] == '\n' || chars[i] == '#' || chars[i] == '\r' {
                // Blank or comment-only: no line at all.
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                i += 1;
                continue;
            }
            indent = col;
            at_line_start = false;
            continue;
        }
        match c {
            '\n' => {
                i += 1;
                if closers.is_empty() {
                    if !toks.is_empty() {
                        out.push(Line {
                            indent,
                            toks: std::mem::take(&mut toks),
                        });
                    }
                    at_line_start = true;
                }
            }
            ' ' | '\t' | '\r' => i += 1,
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '\\' if chars.get(i + 1) == Some(&'\n') => i += 2,
            '\'' | '"' => {
                let (s, next) = string(&chars, i, false)?;
                toks.push(Tok::Str(s));
                i = next;
            }
            c if c.is_ascii_digit()
                || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) =>
            {
                let (n, next) = number(&chars, i)?;
                toks.push(Tok::Num(n));
                i = next;
            }
            c if c.is_alphabetic() || c == '_' => {
                let (tok, next) = word(&chars, i)?;
                toks.push(tok);
                i = next;
            }
            _ => {
                let op = operator(&chars[i..], &mut closers)?;
                toks.push(Tok::Op(op));
                i += op.len();
            }
        }
    }
    if let Some(close) = closers.last() {
        let open = match close {
            ')' => '(',
            ']' => '[',
            _ => '{',
        };
        return Err(Error::Syntax(format!("'{open}' was never closed")));
    }
    if !toks.is_empty() {
        out.push(Line { indent, toks });
    }
    Ok(out)
}

/// A name, or a string with a prefix such as `r"…"`, starting at `start`.
fn word(chars: &[char], start: usize) -> Result<(Tok, usize), Error> {
    let mut i = start;
    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
        i += 1;
    }
    let word: String = chars[start..i].iter().collect();
    if matches!(chars.get(i), Some('\'' | '"')) {
        match word.to_ascii_lowercase().as_str() {
            "r" | "u" => {
                let (s, next) = string(chars, i, word.eq_ignore_ascii_case("r"))?;
                return Ok((Tok::Str(s), next));
            }
            "f" | "rf" | "fr" => return Err(Error::unsupported("f-strings")),
            "b" | "rb" | "br" => return Err(Error::unsupported("bytes")),
            _ => {}
        }
    }
    Ok((Tok::Name(word), i))
}

/// The operator `rest` starts with, keeping `closers` — the brackets
/// still open — up to date.
fn operator(rest: &[char], closers: &mut Vec<char>) -> Result<&'static str, Error> {
    let head: String = rest.iter().take(3).collect();
    let Some(op) = OPS.iter().find(|op| head.starts_with(**op)) else {
        return Err(Error::unsupported(format!("`{}`", rest[0])));
    };
    match *op {
        "(" => closers.push(')'),
        "[" => closers.push(']'),
        "{" => closers.push('}'),
        ")" | "]" | "}" => {
            let close = op.chars().next().unwrap_or(')');
            if closers.pop() != Some(close) {
                return Err(Error::Syntax(format!("unmatched '{close}'")));
            }
        }
        _ => {}
    }
    Ok(op)
}

fn string(chars: &[char], start: usize, raw: bool) -> Result<(String, usize), Error> {
    let quote = chars[start];
    if chars.get(start + 1) == Some(&quote) && chars.get(start + 2) == Some(&quote) {
        return Err(Error::unsupported("triple-quoted strings"));
    }
    let mut out = String::new();
    let mut i = start + 1;
    while i < chars.len() {
        let c = chars[i];
        if c == quote {
            return Ok((out, i + 1));
        }
        if c == '\n' {
            break;
        }
        if c == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if raw {
                // A raw string keeps the backslash, even before a quote.
                out.push('\\');
                out.push(next);
            } else {
                match next {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'f' => out.push('\x0c'),
                    'v' => out.push('\x0b'),
                    'a' => out.push('\x07'),
                    'b' => out.push('\x08'),
                    '0' => out.push('\0'),
                    '\\' | '\'' | '"' => out.push(next),
                    '\n' => {}
                    other => {
                        out.push('\\');
                        out.push(other);
                    }
                }
            }
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    Err(Error::Syntax("unterminated string".to_owned()))
}

fn number(chars: &[char], start: usize) -> Result<(f64, usize), Error> {
    let mut i = start;
    let mut text = String::new();
    while i < chars.len() {
        let c = chars[i];
        let exponent_sign =
            (c == '-' || c == '+') && matches!(text.chars().last(), Some('e' | 'E'));
        if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || exponent_sign {
            text.push(c);
        } else if c != '_' {
            break;
        }
        i += 1;
    }
    if matches!(chars.get(i), Some('j' | 'J' | 'x' | 'o' | 'b')) {
        return Err(Error::unsupported("that kind of number"));
    }
    text.parse()
        .map(|n| (n, i))
        .map_err(|_| Error::unsupported(format!("the number {text}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Vec<Tok>> {
        lines(src)
            .expect("lexes")
            .into_iter()
            .map(|l| l.toks)
            .collect()
    }

    #[test]
    fn brackets_join_lines_and_comments_go() {
        let got = toks("a = f(1,\n      2.5e1)  # note\n\nself.add(a)\n");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0][4], Tok::Num(1.0));
        assert_eq!(got[0][6], Tok::Num(25.0));
        assert_eq!(got[1][0], Tok::Name("self".into()));
    }

    #[test]
    fn strings_raw_and_not() {
        assert_eq!(
            toks(r#"r"\frac{a}{b}""#)[0][0],
            Tok::Str(r"\frac{a}{b}".into())
        );
        assert_eq!(toks(r"'a\nb'")[0][0], Tok::Str("a\nb".into()));
        assert_eq!(toks(r"'\pi'")[0][0], Tok::Str(r"\pi".into()));
    }

    #[test]
    fn only_certain_mistakes_are_syntax_errors() {
        assert!(matches!(lines("f(1, 2"), Err(Error::Syntax(_))));
        assert!(matches!(lines("f(1]"), Err(Error::Syntax(_))));
        assert!(matches!(lines("x = 'abc"), Err(Error::Syntax(_))));
        assert!(matches!(lines("x = f'{a}'"), Err(Error::Unsupported(_))));
        assert!(matches!(
            lines("x = \"\"\"doc\"\"\""),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn indentation_is_measured() {
        let l = lines("class A(Scene):\n    def construct(self):\n        pass\n").expect("lexes");
        assert_eq!(
            l.iter().map(|l| l.indent).collect::<Vec<_>>(),
            vec![0, 4, 8]
        );
    }
}
