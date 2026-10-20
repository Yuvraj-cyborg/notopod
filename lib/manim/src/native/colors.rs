//! Manim's named colours, with the values Manim gives them.

use super::scene::Rgb;

const NAMED: &[(&str, &str)] = &[
    ("WHITE", "#FFFFFF"),
    ("BLACK", "#000000"),
    ("BLUE", "#58C4DD"),
    ("TEAL", "#5CD0B3"),
    ("GREEN", "#83C167"),
    ("YELLOW", "#F7D96F"),
    ("GOLD", "#F0AC5F"),
    ("RED", "#FC6255"),
    ("MAROON", "#C55F73"),
    ("PURPLE", "#9A72AC"),
    ("PINK", "#D147BD"),
    ("LIGHT_PINK", "#DC75CD"),
    ("ORANGE", "#FF862F"),
    ("GREY", "#888888"),
    ("GRAY", "#888888"),
    ("LIGHT_GREY", "#BBBBBB"),
    ("LIGHT_GRAY", "#BBBBBB"),
    ("DARK_GREY", "#444444"),
    ("DARK_GRAY", "#444444"),
    ("DARKER_GREY", "#222222"),
    ("DARKER_GRAY", "#222222"),
    ("DARK_BLUE", "#236B8E"),
    ("DARK_BROWN", "#8B4513"),
    ("LIGHT_BROWN", "#CD853F"),
    ("GREY_BROWN", "#736357"),
    ("GRAY_BROWN", "#736357"),
    ("PURE_RED", "#FF0000"),
    ("PURE_GREEN", "#00FF00"),
    ("PURE_BLUE", "#0000FF"),
    ("PURE_YELLOW", "#FFFF00"),
    ("PURE_CYAN", "#00FFFF"),
    ("PURE_MAGENTA", "#FF00FF"),
];

/// Each family's `_A` (lightest) to `_E` (darkest).
const SHADES: &[(&str, [&str; 5])] = &[
    (
        "BLUE",
        ["#C7E9F1", "#9CDCEB", "#58C4DD", "#29ABCA", "#236B8E"],
    ),
    (
        "TEAL",
        ["#ACEAD7", "#76DDC0", "#5CD0B3", "#55C1A7", "#49A88F"],
    ),
    (
        "GREEN",
        ["#C9E2AE", "#A6CF8C", "#83C167", "#77B05D", "#699C52"],
    ),
    (
        "YELLOW",
        ["#FFF1B6", "#FFEA94", "#F7D96F", "#F4D345", "#E8C11C"],
    ),
    (
        "GOLD",
        ["#F7C797", "#F9B775", "#F0AC5F", "#E1A158", "#C78D46"],
    ),
    (
        "RED",
        ["#F7A1A3", "#FF8080", "#FC6255", "#E65A4C", "#CF5044"],
    ),
    (
        "MAROON",
        ["#ECABC1", "#EC92AB", "#C55F73", "#A24D61", "#94424F"],
    ),
    (
        "PURPLE",
        ["#CAA3E8", "#B189C6", "#9A72AC", "#715582", "#644172"],
    ),
    (
        "GREY",
        ["#DDDDDD", "#BBBBBB", "#888888", "#444444", "#222222"],
    ),
    (
        "GRAY",
        ["#DDDDDD", "#BBBBBB", "#888888", "#444444", "#222222"],
    ),
];

fn rgb(hex: &str) -> Rgb {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    ((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// The colour a constant such as `BLUE` or `RED_E` names.
pub(crate) fn named(name: &str) -> Option<Rgb> {
    if let Some((_, v)) = NAMED.iter().find(|(n, _)| *n == name) {
        return Some(rgb(v));
    }
    let (family, shade) = name.rsplit_once('_')?;
    let index = "ABCDE".find(shade).filter(|_| shade.len() == 1)?;
    SHADES
        .iter()
        .find(|(n, _)| *n == family)
        .map(|(_, shades)| rgb(shades[index]))
}

/// `#rrggbb`, `#rgb`, or a colour's name in any case.
pub(crate) fn parse(text: &str) -> Option<Rgb> {
    if let Some(hex) = text.strip_prefix('#') {
        let expand = |s: &str| -> Option<u32> {
            match s.len() {
                6 => u32::from_str_radix(s, 16).ok(),
                3 => {
                    let v = u32::from_str_radix(s, 16).ok()?;
                    let (r, g, b) = ((v >> 8) & 0xF, (v >> 4) & 0xF, v & 0xF);
                    Some(((r * 17) << 16) | ((g * 17) << 8) | (b * 17))
                }
                _ => None,
            }
        };
        return expand(hex).map(|v| ((v >> 16) as u8, (v >> 8) as u8, v as u8));
    }
    named(&text.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_shades_and_hex() {
        assert_eq!(named("BLUE"), Some((0x58, 0xC4, 0xDD)));
        assert_eq!(named("BLUE_D"), Some((0x29, 0xAB, 0xCA)));
        assert_eq!(named("RED_A"), Some((0xF7, 0xA1, 0xA3)));
        assert_eq!(named("BLUE_F"), None);
        assert_eq!(parse("#fff"), Some((255, 255, 255)));
        assert_eq!(parse("#4c7fd4"), Some((76, 127, 212)));
        assert_eq!(parse("yellow"), Some((0xF7, 0xD9, 0x6F)));
    }
}
