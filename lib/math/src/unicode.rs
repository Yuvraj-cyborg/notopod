//! LaTeX math spelled out in Unicode.
//!
//! Not a typesetter: a reading aid. Greek letters, operators, arrows and
//! relations become their characters, scripts become superscript and
//! subscript characters where Unicode has them, `\frac{a}{b}` becomes
//! `a/b` (or a vulgar fraction like `½`), `\sqrt` a radical, `\mathbb{R}`
//! `ℝ`, and accents combining marks. Anything it does not know is shown as
//! its name, so nothing is silently lost.

/// `tex` as a line of Unicode text.
pub fn to_unicode(tex: &str) -> String {
    let mut p = Parser {
        chars: tex.chars().collect(),
        at: 0,
    };
    let out = p.sequence(Stop::End);
    tidy(&out)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stop {
    /// The end of the input.
    End,
    /// A closing brace.
    Brace,
    /// `\end{...}`.
    Env,
    /// `\right`.
    Right,
}

struct Parser {
    chars: Vec<char>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += 1;
        Some(c)
    }

    /// The name of the command starting at the cursor (after `\`), without
    /// consuming it.
    fn peek_command(&self) -> Option<String> {
        if self.peek() != Some('\\') {
            return None;
        }
        let rest = &self.chars[self.at + 1..];
        let letters: String = rest
            .iter()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        if letters.is_empty() {
            rest.first().map(char::to_string)
        } else {
            Some(letters)
        }
    }

    fn skip_spaces(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
    }

    fn sequence(&mut self, stop: Stop) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            match c {
                '}' if stop == Stop::Brace => {
                    self.at += 1;
                    return out;
                }
                '}' => self.at += 1,
                '\\' if stop == Stop::Env && self.peek_command().as_deref() == Some("end") => {
                    return out;
                }
                '\\' if stop == Stop::Right && self.peek_command().as_deref() == Some("right") => {
                    return out;
                }
                '^' | '_' => {
                    self.at += 1;
                    self.skip_spaces();
                    let script = self.atom();
                    out.push_str(&if c == '^' {
                        superscript(&script)
                    } else {
                        subscript(&script)
                    });
                }
                _ => {
                    let atom = self.atom();
                    out.push_str(&atom);
                }
            }
        }
        out
    }

    /// One item: a group, a command with its arguments, or a character.
    fn atom(&mut self) -> String {
        let Some(c) = self.bump() else {
            return String::new();
        };
        match c {
            '{' => self.sequence(Stop::Brace),
            '\\' => self.command(),
            '-' => "−".to_owned(),
            '*' => "∗".to_owned(),
            '\'' => "′".to_owned(),
            '~' => " ".to_owned(),
            '&' => "  ".to_owned(),
            c if c.is_whitespace() => " ".to_owned(),
            c => c.to_string(),
        }
    }

    /// A required argument: a group, or a single token.
    fn argument(&mut self) -> String {
        self.skip_spaces();
        self.atom()
    }

    /// The raw text of a `{...}` argument, for `\text` and environment names.
    fn raw_argument(&mut self) -> String {
        self.skip_spaces();
        if self.peek() != Some('{') {
            return self.bump().map(|c| c.to_string()).unwrap_or_default();
        }
        self.at += 1;
        let mut depth = 1;
        let mut out = String::new();
        while let Some(c) = self.bump() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            out.push(c);
        }
        out
    }

    /// An optional `[...]` argument.
    fn optional(&mut self) -> Option<String> {
        self.skip_spaces();
        if self.peek() != Some('[') {
            return None;
        }
        self.at += 1;
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c == ']' {
                self.at += 1;
                break;
            }
            out.push_str(&self.atom());
        }
        Some(out)
    }

    fn command(&mut self) -> String {
        // A command is a run of letters, or one other character.
        let mut name = String::new();
        while let Some(c) = self.peek().filter(char::is_ascii_alphabetic) {
            name.push(c);
            self.at += 1;
        }
        if name.is_empty() {
            return match self.bump() {
                Some(',' | ':' | ';' | ' ') => " ".to_owned(),
                Some('\\') => "; ".to_owned(),
                Some('|') => "‖".to_owned(),
                Some('!') | None => String::new(),
                Some(c) => c.to_string(),
            };
        }
        if let Some(sym) = symbol(&name) {
            return sym.to_owned();
        }
        if let Some(styled) = self.styled(&name) {
            return styled;
        }
        match name.as_str() {
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                let (num, den) = (self.argument(), self.argument());
                fraction(&num, &den)
            }
            "binom" | "dbinom" | "tbinom" => {
                let (n, k) = (self.argument(), self.argument());
                format!("C({}, {})", tidy(&n), tidy(&k))
            }
            "sqrt" => {
                let index = self.optional();
                let body = self.argument();
                let root = match index.as_deref().map(str::trim) {
                    Some("3") => "∛".to_owned(),
                    Some("4") => "∜".to_owned(),
                    Some(n) if !n.is_empty() => format!("{}√", superscript(n)),
                    _ => "√".to_owned(),
                };
                format!("{root}{}", wrap(&body))
            }
            "left" | "bigl" | "Bigl" | "biggl" | "Biggl" | "big" | "Big" | "bigg" | "Bigg"
            | "bigr" | "Bigr" | "biggr" | "Biggr" | "middle" => {
                if name == "left" {
                    let open = self.delimiter();
                    let inner = self.sequence(Stop::Right);
                    // Consume `\right` and its delimiter.
                    if self.peek_command().as_deref() == Some("right") {
                        self.at += "right".len() + 1;
                    }
                    let close = self.delimiter();
                    format!("{open}{}{close}", tidy(&inner))
                } else {
                    self.delimiter()
                }
            }
            "right" => self.delimiter(),
            "begin" => self.environment(),
            "end" => {
                self.raw_argument();
                String::new()
            }
            "label" | "nonumber" | "notag" | "limits" | "nolimits" => {
                if name == "label" {
                    self.raw_argument();
                }
                String::new()
            }
            "tag" => format!("  ({})", self.raw_argument()),
            "xrightarrow" | "xleftarrow" => {
                let _ = self.optional();
                let over = self.argument();
                let arrow = if name == "xrightarrow" { "→" } else { "←" };
                if over.trim().is_empty() {
                    arrow.to_owned()
                } else {
                    format!("─{}{arrow}", tidy(&over))
                }
            }
            other if is_function(other) => format!("{other} "),
            other => other.to_owned(),
        }
    }

    /// Commands that change how their argument looks rather than what it
    /// is: text, fonts, accents and colour.
    fn styled(&mut self, name: &str) -> Option<String> {
        Some(match name {
            "text" | "textrm" | "textit" | "textbf" | "textsf" | "texttt" | "mbox" | "hbox" => {
                self.raw_argument()
            }
            "mathrm" | "mathit" | "mathsf" | "mathtt" | "operatorname" | "boldsymbol" | "bm"
            | "mathnormal" | "displaystyle" | "textstyle" | "scriptstyle" => {
                if matches!(name, "displaystyle" | "textstyle" | "scriptstyle") {
                    String::new()
                } else {
                    self.argument()
                }
            }
            "mathbf" => alphabet(&self.argument(), Alphabet::Bold),
            "mathbb" => alphabet(&self.argument(), Alphabet::DoubleStruck),
            "mathcal" | "mathscr" => alphabet(&self.argument(), Alphabet::Script),
            "mathfrak" => alphabet(&self.argument(), Alphabet::Fraktur),
            "hat" | "widehat" => accent(&self.argument(), '\u{0302}', "hat"),
            "bar" | "overline" => accent(&self.argument(), '\u{0305}', "bar"),
            "vec" | "overrightarrow" => accent(&self.argument(), '\u{20D7}', "vec"),
            "dot" => accent(&self.argument(), '\u{0307}', "dot"),
            "ddot" => accent(&self.argument(), '\u{0308}', "ddot"),
            "tilde" | "widetilde" => accent(&self.argument(), '\u{0303}', "tilde"),
            "underline" => accent(&self.argument(), '\u{0332}', "underline"),
            "overbrace" | "underbrace" | "boxed" | "phantom" | "mathop" | "mathbin" | "mathrel"
            | "mathord" | "smash" => self.argument(),
            "color" => {
                // `\color{red}{x}` and the switch form `\color{red} x`.
                self.raw_argument();
                String::new()
            }
            "textcolor" | "colorbox" => {
                self.raw_argument();
                self.argument()
            }
            _ => return None,
        })
    }

    /// The delimiter after `\left`, `\right` or `\big`.
    fn delimiter(&mut self) -> String {
        self.skip_spaces();
        match self.bump() {
            Some('.') | None => String::new(),
            Some('\\') => {
                let mut name = String::new();
                while let Some(c) = self.peek().filter(char::is_ascii_alphabetic) {
                    name.push(c);
                    self.at += 1;
                }
                if name.is_empty() {
                    return match self.bump() {
                        Some('|') => "‖".to_owned(),
                        Some(c) => c.to_string(),
                        None => String::new(),
                    };
                }
                symbol(&name).map_or(name, str::to_owned)
            }
            Some(c) => c.to_string(),
        }
    }

    fn environment(&mut self) -> String {
        let env = self.raw_argument();
        let env = env.trim().trim_end_matches('*').to_owned();
        if env == "array" {
            // The column spec: `{cc|l}`.
            self.raw_argument();
        }
        let body = self.sequence(Stop::Env);
        // `\end{env}`.
        if self.peek_command().as_deref() == Some("end") {
            self.at += "end".len() + 1;
            self.raw_argument();
        }
        let body = tidy(&body).replace(" ;", ";");
        let body = body.trim_end_matches(';').trim();
        let (open, close) = match env.as_str() {
            "pmatrix" => ("(", ")"),
            "bmatrix" => ("[", "]"),
            "Bmatrix" => ("{", "}"),
            "vmatrix" => ("|", "|"),
            "Vmatrix" => ("‖", "‖"),
            "cases" => ("{ ", ""),
            _ => ("", ""),
        };
        format!("{open}{body}{close}")
    }
}

/// `body` in parentheses unless it is a single symbol or number.
fn wrap(body: &str) -> String {
    let body = tidy(body);
    if is_simple(&body) {
        body
    } else {
        format!("({body})")
    }
}

fn is_simple(s: &str) -> bool {
    let n = s.chars().count();
    n == 1
        || (n > 0 && s.chars().all(|c| c.is_ascii_digit() || c == '.'))
        || (s.starts_with('(') && s.ends_with(')') && !s[1..s.len() - 1].contains(['(', ')']))
}

fn fraction(num: &str, den: &str) -> String {
    let (num, den) = (tidy(num), tidy(den));
    // A fraction still being typed shows what there is of it.
    if den.is_empty() {
        return num;
    }
    if let Some(v) = vulgar(&num, &den) {
        return v.to_owned();
    }
    format!("{}/{}", wrap(&num), wrap(&den))
}

fn vulgar(num: &str, den: &str) -> Option<&'static str> {
    Some(match (num, den) {
        ("1", "2") => "½",
        ("1", "3") => "⅓",
        ("2", "3") => "⅔",
        ("1", "4") => "¼",
        ("3", "4") => "¾",
        ("1", "5") => "⅕",
        ("2", "5") => "⅖",
        ("3", "5") => "⅗",
        ("4", "5") => "⅘",
        ("1", "6") => "⅙",
        ("5", "6") => "⅚",
        ("1", "8") => "⅛",
        ("3", "8") => "⅜",
        ("5", "8") => "⅝",
        ("7", "8") => "⅞",
        _ => return None,
    })
}

fn accent(body: &str, mark: char, name: &str) -> String {
    let body = tidy(body);
    if body.chars().count() == 1 {
        format!("{body}{mark}")
    } else if mark == '\u{0305}' || mark == '\u{0332}' {
        // Over- and underlines run under every letter.
        body.chars().flat_map(|c| [c, mark]).collect()
    } else {
        format!("{name}({body})")
    }
}

/// Superscript characters for every character of `s`, or `^(s)` when one
/// of them has none.
fn superscript(s: &str) -> String {
    script(s, sup_char, '^')
}

fn subscript(s: &str) -> String {
    script(s, sub_char, '_')
}

fn script(s: &str, map: fn(char) -> Option<char>, marker: char) -> String {
    let s = tidy(s);
    if s.is_empty() {
        return String::new();
    }
    let converted: Option<String> = s.chars().filter(|c| *c != ' ').map(map).collect();
    match converted {
        Some(c) => c,
        None if s.chars().count() == 1 => format!("{marker}{s}"),
        None => format!("{marker}({s})"),
    }
}

fn sup_char(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' | '−' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'a' => 'ᵃ',
        'b' => 'ᵇ',
        'c' => 'ᶜ',
        'd' => 'ᵈ',
        'e' => 'ᵉ',
        'f' => 'ᶠ',
        'g' => 'ᵍ',
        'h' => 'ʰ',
        'i' => 'ⁱ',
        'j' => 'ʲ',
        'k' => 'ᵏ',
        'l' => 'ˡ',
        'm' => 'ᵐ',
        'n' => 'ⁿ',
        'o' => 'ᵒ',
        'p' => 'ᵖ',
        'r' => 'ʳ',
        's' => 'ˢ',
        't' => 'ᵗ',
        'u' => 'ᵘ',
        'v' => 'ᵛ',
        'w' => 'ʷ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'z' => 'ᶻ',
        'A' => 'ᴬ',
        'B' => 'ᴮ',
        'D' => 'ᴰ',
        'E' => 'ᴱ',
        'G' => 'ᴳ',
        'H' => 'ᴴ',
        'I' => 'ᴵ',
        'J' => 'ᴶ',
        'K' => 'ᴷ',
        'L' => 'ᴸ',
        'M' => 'ᴹ',
        'N' => 'ᴺ',
        'O' => 'ᴼ',
        'P' => 'ᴾ',
        'R' => 'ᴿ',
        'T' => 'ᵀ',
        'U' => 'ᵁ',
        'V' => 'ⱽ',
        'W' => 'ᵂ',
        'α' => 'ᵅ',
        'β' => 'ᵝ',
        'γ' => 'ᵞ',
        'δ' => 'ᵟ',
        'θ' => 'ᶿ',
        'φ' => 'ᵠ',
        'χ' => 'ᵡ',
        '′' => '′',
        '∗' => '*',
        _ => return None,
    })
}

fn sub_char(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' | '−' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        'β' => 'ᵦ',
        'γ' => 'ᵧ',
        'ρ' => 'ᵨ',
        'φ' => 'ᵩ',
        'χ' => 'ᵪ',
        _ => return None,
    })
}

#[derive(Clone, Copy)]
enum Alphabet {
    Bold,
    DoubleStruck,
    Script,
    Fraktur,
}

/// Letters in one of Unicode's mathematical alphabets. The few letters
/// that Unicode placed in Letterlike Symbols (ℝ, ℒ, ℭ, ...) come from there.
fn alphabet(body: &str, which: Alphabet) -> String {
    body.chars()
        .map(|c| {
            let special = match (which, c) {
                (Alphabet::DoubleStruck, 'C') => Some('ℂ'),
                (Alphabet::DoubleStruck, 'H') => Some('ℍ'),
                (Alphabet::DoubleStruck, 'N') => Some('ℕ'),
                (Alphabet::DoubleStruck, 'P') => Some('ℙ'),
                (Alphabet::DoubleStruck, 'Q') => Some('ℚ'),
                (Alphabet::DoubleStruck, 'R') => Some('ℝ'),
                (Alphabet::DoubleStruck, 'Z') => Some('ℤ'),
                (Alphabet::Script, 'B') => Some('ℬ'),
                (Alphabet::Script, 'E') => Some('ℰ'),
                (Alphabet::Script, 'F') => Some('ℱ'),
                (Alphabet::Script, 'H') => Some('ℋ'),
                (Alphabet::Script, 'I') => Some('ℐ'),
                (Alphabet::Script, 'L') => Some('ℒ'),
                (Alphabet::Script, 'M') => Some('ℳ'),
                (Alphabet::Script, 'R') => Some('ℛ'),
                (Alphabet::Script, 'e') => Some('ℯ'),
                (Alphabet::Script, 'g') => Some('ℊ'),
                (Alphabet::Script, 'o') => Some('ℴ'),
                (Alphabet::Fraktur, 'C') => Some('ℭ'),
                (Alphabet::Fraktur, 'H') => Some('ℌ'),
                (Alphabet::Fraktur, 'I') => Some('ℑ'),
                (Alphabet::Fraktur, 'R') => Some('ℜ'),
                (Alphabet::Fraktur, 'Z') => Some('ℨ'),
                _ => None,
            };
            if let Some(s) = special {
                return s;
            }
            let base = match which {
                Alphabet::Bold => 0x1D400,
                Alphabet::Script => 0x1D49C,
                Alphabet::Fraktur => 0x1D504,
                Alphabet::DoubleStruck => 0x1D538,
            };
            let offset = match c {
                'A'..='Z' => u32::from(c) - u32::from('A'),
                'a'..='z' => 26 + u32::from(c) - u32::from('a'),
                '0'..='9' if matches!(which, Alphabet::Bold) => {
                    return char::from_u32(0x1D7CE + u32::from(c) - u32::from('0')).unwrap_or(c);
                }
                '0'..='9' if matches!(which, Alphabet::DoubleStruck) => {
                    return char::from_u32(0x1D7D8 + u32::from(c) - u32::from('0')).unwrap_or(c);
                }
                _ => return c,
            };
            char::from_u32(base + offset).unwrap_or(c)
        })
        .collect()
}

fn is_function(name: &str) -> bool {
    matches!(
        name,
        "sin"
            | "cos"
            | "tan"
            | "cot"
            | "sec"
            | "csc"
            | "arcsin"
            | "arccos"
            | "arctan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "coth"
            | "log"
            | "ln"
            | "lg"
            | "exp"
            | "lim"
            | "liminf"
            | "limsup"
            | "max"
            | "min"
            | "sup"
            | "inf"
            | "det"
            | "gcd"
            | "arg"
            | "deg"
            | "dim"
            | "ker"
            | "hom"
            | "Pr"
            | "mod"
            | "bmod"
            | "pmod"
    )
}

/// Commands that stand for one symbol, and the characters they stand for.
const SYMBOLS: &[(&str, &str)] = &[
    // Greek
    ("alpha", "α"),
    ("beta", "β"),
    ("gamma", "γ"),
    ("delta", "δ"),
    ("epsilon", "ϵ"),
    ("varepsilon", "ε"),
    ("zeta", "ζ"),
    ("eta", "η"),
    ("theta", "θ"),
    ("vartheta", "ϑ"),
    ("iota", "ι"),
    ("kappa", "κ"),
    ("lambda", "λ"),
    ("mu", "μ"),
    ("nu", "ν"),
    ("xi", "ξ"),
    ("omicron", "ο"),
    ("pi", "π"),
    ("varpi", "ϖ"),
    ("rho", "ρ"),
    ("varrho", "ϱ"),
    ("sigma", "σ"),
    ("varsigma", "ς"),
    ("tau", "τ"),
    ("upsilon", "υ"),
    ("phi", "ϕ"),
    ("varphi", "φ"),
    ("chi", "χ"),
    ("psi", "ψ"),
    ("omega", "ω"),
    ("Gamma", "Γ"),
    ("Delta", "Δ"),
    ("Theta", "Θ"),
    ("Lambda", "Λ"),
    ("Xi", "Ξ"),
    ("Pi", "Π"),
    ("Sigma", "Σ"),
    ("Upsilon", "Υ"),
    ("Phi", "Φ"),
    ("Psi", "Ψ"),
    ("Omega", "Ω"),
    // Big operators
    ("sum", "∑"),
    ("prod", "∏"),
    ("coprod", "∐"),
    ("int", "∫"),
    ("iint", "∬"),
    ("iiint", "∭"),
    ("oint", "∮"),
    ("bigcup", "⋃"),
    ("bigcap", "⋂"),
    ("bigoplus", "⨁"),
    ("bigotimes", "⨂"),
    // Binary operators
    ("pm", "±"),
    ("mp", "∓"),
    ("times", "×"),
    ("div", "÷"),
    ("cdot", "·"),
    ("ast", "∗"),
    ("star", "⋆"),
    ("circ", "∘"),
    ("bullet", "∙"),
    ("oplus", "⊕"),
    ("ominus", "⊖"),
    ("otimes", "⊗"),
    ("odot", "⊙"),
    ("cup", "∪"),
    ("cap", "∩"),
    ("setminus", "∖"),
    ("wedge", "∧"),
    ("land", "∧"),
    ("vee", "∨"),
    ("lor", "∨"),
    // Relations
    ("leq", "≤"),
    ("le", "≤"),
    ("geq", "≥"),
    ("ge", "≥"),
    ("neq", "≠"),
    ("ne", "≠"),
    ("approx", "≈"),
    ("equiv", "≡"),
    ("sim", "∼"),
    ("simeq", "≃"),
    ("cong", "≅"),
    ("propto", "∝"),
    ("ll", "≪"),
    ("gg", "≫"),
    ("in", "∈"),
    ("notin", "∉"),
    ("ni", "∋"),
    ("subset", "⊂"),
    ("supset", "⊃"),
    ("subseteq", "⊆"),
    ("supseteq", "⊇"),
    ("perp", "⊥"),
    ("parallel", "∥"),
    ("mid", "∣"),
    ("models", "⊨"),
    ("vdash", "⊢"),
    // Arrows
    ("to", "→"),
    ("rightarrow", "→"),
    ("leftarrow", "←"),
    ("gets", "←"),
    ("leftrightarrow", "↔"),
    ("Rightarrow", "⇒"),
    ("implies", "⇒"),
    ("Leftarrow", "⇐"),
    ("Leftrightarrow", "⇔"),
    ("iff", "⇔"),
    ("mapsto", "↦"),
    ("uparrow", "↑"),
    ("downarrow", "↓"),
    ("longrightarrow", "⟶"),
    ("longleftarrow", "⟵"),
    ("hookrightarrow", "↪"),
    // Misc
    ("infty", "∞"),
    ("partial", "∂"),
    ("nabla", "∇"),
    ("forall", "∀"),
    ("exists", "∃"),
    ("nexists", "∄"),
    ("neg", "¬"),
    ("lnot", "¬"),
    ("emptyset", "∅"),
    ("varnothing", "∅"),
    ("ldots", "…"),
    ("dots", "…"),
    ("cdots", "⋯"),
    ("vdots", "⋮"),
    ("ddots", "⋱"),
    ("angle", "∠"),
    ("triangle", "△"),
    ("square", "□"),
    ("degree", "°"),
    ("prime", "′"),
    ("hbar", "ℏ"),
    ("ell", "ℓ"),
    ("Re", "ℜ"),
    ("Im", "ℑ"),
    ("aleph", "ℵ"),
    ("wp", "℘"),
    ("langle", "⟨"),
    ("rangle", "⟩"),
    ("lceil", "⌈"),
    ("rceil", "⌉"),
    ("lfloor", "⌊"),
    ("rfloor", "⌋"),
    ("lbrace", "{"),
    ("rbrace", "}"),
    ("vert", "|"),
    ("lvert", "|"),
    ("rvert", "|"),
    ("Vert", "‖"),
    ("lVert", "‖"),
    ("rVert", "‖"),
    ("quad", "  "),
    ("qquad", "    "),
    ("checkmark", "✓"),
];

fn symbol(name: &str) -> Option<&'static str> {
    SYMBOLS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

/// One space where the source had any run of them, none at the ends.
fn tidy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c == ' ' || c == '\t' || c == '\n' {
            space = !out.is_empty();
        } else {
            if space {
                out.push(' ');
                space = false;
            }
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::to_unicode as u;

    #[test]
    fn letters_operators_and_scripts() {
        assert_eq!(u(r"\alpha^2 + \beta_0"), "α² + β₀");
        assert_eq!(u(r"x_{i+1} = x_i^{n}"), "xᵢ₊₁ = xᵢⁿ");
        assert_eq!(u(r"a \leq b \neq c \to \infty"), "a ≤ b ≠ c → ∞");
        assert_eq!(u(r"e^{i\pi} + 1 = 0"), "e^(iπ) + 1 = 0");
    }

    #[test]
    fn fractions_and_roots() {
        assert_eq!(u(r"\frac{1}{2}x"), "½x");
        assert_eq!(u(r"\frac{a}{b}"), "a/b");
        assert_eq!(
            u(r"x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}"),
            "x = (−b ± √(b²−4ac))/(2a)"
        );
        assert_eq!(u(r"\sqrt[3]{x} + \sqrt[n]{y}"), "∛x + ⁿ√y");
    }

    #[test]
    fn big_operators_text_and_alphabets() {
        assert_eq!(u(r"\sum_{n=1}^{\infty} \frac{1}{n^2}"), "∑ₙ₌₁^∞ 1/(n²)");
        assert_eq!(u(r"\int_0^1 x\,dx"), "∫₀¹ x dx");
        assert_eq!(u(r"f: \mathbb{R}^n \to \mathbb{R}"), "f: ℝⁿ → ℝ");
        assert_eq!(u(r"\mathcal{L}\{f\}"), "ℒ{f}");
        assert_eq!(u(r"\text{if } x > 0"), "if x > 0");
        assert_eq!(u(r"\sin\theta \cos\theta"), "sin θ cos θ");
    }

    #[test]
    fn delimiters_environments_and_accents() {
        assert_eq!(u(r"\left( \frac{a}{b} \right)"), "(a/b)");
        assert_eq!(u(r"\left\langle v \right\rangle"), "⟨v⟩");
        assert_eq!(
            u(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}"),
            "(a b; c d)"
        );
        assert_eq!(
            u(r"|x| = \begin{cases} x & x \ge 0 \\ -x & \text{else} \end{cases}"),
            "|x| = { x x ≥ 0; −x else"
        );
        assert_eq!(u(r"\vec{v} \cdot \hat{n}"), "v\u{20D7} · n\u{0302}");
        assert_eq!(u(r"\color{red}{x} + y"), "x + y");
    }

    #[test]
    fn unknown_and_broken_input_degrades_rather_than_vanishing() {
        assert_eq!(u(r"\weird{x}"), "weirdx");
        for s in [
            "", r"\", "{", "}", "^", "_", r"\frac{", r"\sqrt[", r"\begin{", r"\left(",
        ] {
            let _ = u(s);
        }
        assert_eq!(u(r"\frac{1"), "1");
    }
}
