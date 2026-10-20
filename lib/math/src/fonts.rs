//! The KaTeX fonts, compiled in. SIL Open Font License 1.1: see
//! `fonts/OFL.txt` and `fonts/FONT_NOTICE.txt`.

/// The bytes of the KaTeX font RaTeX calls `name` (`Main-Regular`, ...).
pub(crate) fn bytes(name: &str) -> Option<&'static [u8]> {
    Some(match name {
        "AMS-Regular" => include_bytes!("../fonts/KaTeX_AMS-Regular.ttf"),
        "Caligraphic-Bold" => include_bytes!("../fonts/KaTeX_Caligraphic-Bold.ttf"),
        "Caligraphic-Regular" => include_bytes!("../fonts/KaTeX_Caligraphic-Regular.ttf"),
        "Fraktur-Bold" => include_bytes!("../fonts/KaTeX_Fraktur-Bold.ttf"),
        "Fraktur-Regular" => include_bytes!("../fonts/KaTeX_Fraktur-Regular.ttf"),
        "Main-Bold" => include_bytes!("../fonts/KaTeX_Main-Bold.ttf"),
        "Main-BoldItalic" => include_bytes!("../fonts/KaTeX_Main-BoldItalic.ttf"),
        "Main-Italic" => include_bytes!("../fonts/KaTeX_Main-Italic.ttf"),
        "Main-Regular" => include_bytes!("../fonts/KaTeX_Main-Regular.ttf"),
        "Math-BoldItalic" => include_bytes!("../fonts/KaTeX_Math-BoldItalic.ttf"),
        "Math-Italic" => include_bytes!("../fonts/KaTeX_Math-Italic.ttf"),
        "SansSerif-Bold" => include_bytes!("../fonts/KaTeX_SansSerif-Bold.ttf"),
        "SansSerif-Italic" => include_bytes!("../fonts/KaTeX_SansSerif-Italic.ttf"),
        "SansSerif-Regular" => include_bytes!("../fonts/KaTeX_SansSerif-Regular.ttf"),
        "Script-Regular" => include_bytes!("../fonts/KaTeX_Script-Regular.ttf"),
        "Size1-Regular" => include_bytes!("../fonts/KaTeX_Size1-Regular.ttf"),
        "Size2-Regular" => include_bytes!("../fonts/KaTeX_Size2-Regular.ttf"),
        "Size3-Regular" => include_bytes!("../fonts/KaTeX_Size3-Regular.ttf"),
        "Size4-Regular" => include_bytes!("../fonts/KaTeX_Size4-Regular.ttf"),
        "Typewriter-Regular" => include_bytes!("../fonts/KaTeX_Typewriter-Regular.ttf"),
        _ => return None,
    })
}
