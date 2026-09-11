//! Plain text search.

/// Finds `query` in `text`, starting at character index `from` and wrapping
/// around to the beginning. Returns the character index of the match.
///
/// Matching is case-insensitive unless `query` contains an uppercase
/// letter ("smart case").
pub fn find(text: &str, query: &str, from: usize) -> Option<usize> {
    if query.is_empty() {
        return None;
    }
    let sensitive = query.chars().any(char::is_uppercase);
    let fold = |c: char| {
        if sensitive {
            c
        } else {
            c.to_lowercase().next().unwrap_or(c)
        }
    };
    let hay: Vec<char> = text.chars().map(fold).collect();
    let needle: Vec<char> = query.chars().map(fold).collect();
    if needle.len() > hay.len() {
        return None;
    }
    let last_start = hay.len() - needle.len();
    let from = from.min(last_start + 1);
    let matches_at = |i: usize| hay[i..i + needle.len()] == needle[..];
    (from..=last_start)
        .chain(0..from.min(last_start + 1))
        .find(|&i| matches_at(i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_forward_then_wraps() {
        let t = "abc abc";
        assert_eq!(find(t, "abc", 0), Some(0));
        assert_eq!(find(t, "abc", 1), Some(4));
        assert_eq!(find(t, "abc", 5), Some(0));
    }

    #[test]
    fn smart_case() {
        assert_eq!(find("Hello", "hello", 0), Some(0));
        assert_eq!(find("Hello", "Hello", 0), Some(0));
        assert_eq!(find("hello", "Hello", 0), None);
    }

    #[test]
    fn empty_and_missing() {
        assert_eq!(find("abc", "", 0), None);
        assert_eq!(find("abc", "zzz", 0), None);
        assert_eq!(find("", "a", 0), None);
    }

    #[test]
    fn counts_characters_not_bytes() {
        assert_eq!(find("日本語 abc", "abc", 0), Some(4));
    }
}
