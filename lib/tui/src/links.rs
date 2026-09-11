//! Links between notes: finding the one under the cursor, and the note it
//! points at.

use std::fs;
use std::path::{Path, PathBuf};

/// Directory entries looked at before a search for a note gives up, so a
/// link into a huge tree costs a moment and not a hang.
const SEARCH_LIMIT: usize = 20_000;

/// A link found in a line of source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Link {
    /// `[[Name]]` or `[[Name|shown]]`: a note by name.
    Wiki(String),
    /// `[shown](target)`: a path, or a URL with a scheme.
    Url(String),
}

/// The link under character `col` of `line`, if the cursor is on one.
pub(crate) fn link_at(line: &str, col: usize) -> Option<Link> {
    let chars: Vec<char> = line.chars().collect();
    let col = col.min(chars.len().saturating_sub(1));
    wiki_at(&chars, col).or_else(|| markdown_at(&chars, col))
}

fn wiki_at(chars: &[char], col: usize) -> Option<Link> {
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] == '[' && chars[i + 1] == '[' {
            let start = i + 2;
            let mut j = start;
            while j + 1 < chars.len() && !(chars[j] == ']' && chars[j + 1] == ']') {
                if chars[j] == '[' {
                    break;
                }
                j += 1;
            }
            if j + 1 < chars.len() && chars[j] == ']' && chars[j + 1] == ']' {
                if (i..=j + 1).contains(&col) {
                    let inner: String = chars[start..j].iter().collect();
                    let target = inner.split_once('|').map_or(inner.as_str(), |(t, _)| t);
                    let target = target.split('#').next().unwrap_or(target).trim();
                    if target.is_empty() {
                        return None;
                    }
                    return Some(Link::Wiki(target.to_owned()));
                }
                i = j + 2;
                continue;
            }
        }
        i += 1;
    }
    None
}

fn markdown_at(chars: &[char], col: usize) -> Option<Link> {
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            // `[shown](target)`: the text may not nest brackets.
            let mut j = i + 1;
            while j < chars.len() && chars[j] != ']' && chars[j] != '[' {
                j += 1;
            }
            if j + 1 < chars.len() && chars[j] == ']' && chars[j + 1] == '(' {
                let mut k = j + 2;
                while k < chars.len() && chars[k] != ')' {
                    k += 1;
                }
                if k < chars.len() {
                    if (i..=k).contains(&col) {
                        let target: String = chars[j + 2..k].iter().collect();
                        let target = target.split_whitespace().next().unwrap_or("");
                        if target.is_empty() {
                            return None;
                        }
                        return Some(Link::Url(target.to_owned()));
                    }
                    i = k + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    None
}

/// Whether a link target is a URL rather than a file.
pub(crate) fn has_scheme(target: &str) -> bool {
    target.split_once(':').is_some_and(|(scheme, rest)| {
        scheme.len() > 1
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
            && (rest.starts_with("//") || scheme.eq_ignore_ascii_case("mailto"))
    })
}

/// The note called `name` under `root`: `root/name.md` when that exists,
/// otherwise the first note anywhere below `root` whose file name matches
/// ignoring case and extension. Hidden directories are skipped.
pub(crate) fn find_note(root: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim().trim_end_matches('/');
    if name.is_empty() {
        return None;
    }
    let direct = root.join(crate::app::with_md(PathBuf::from(name)));
    if direct.is_file() {
        return Some(direct);
    }
    let stem = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())?;
    let mut budget = SEARCH_LIMIT;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            budget = budget.checked_sub(1)?;
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
                && path
                    .file_stem()
                    .is_some_and(|s| s.to_string_lossy().to_lowercase() == stem)
            {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_link_under_the_cursor_and_nothing_elsewhere() {
        let line = "see [[Plan|the plan]] or [docs](docs/a.md) then [[B#top]] x";
        assert_eq!(link_at(line, 6), Some(Link::Wiki("Plan".into())));
        assert_eq!(
            link_at(line, 4),
            Some(Link::Wiki("Plan".into())),
            "on the bracket"
        );
        assert_eq!(
            link_at(line, 20),
            Some(Link::Wiki("Plan".into())),
            "closing bracket"
        );
        assert_eq!(link_at(line, 22), None, "between links");
        assert_eq!(link_at(line, 30), Some(Link::Url("docs/a.md".into())));
        assert_eq!(link_at(line, 41), Some(Link::Url("docs/a.md".into())));
        assert_eq!(link_at(line, 50), Some(Link::Wiki("B".into())));
        assert_eq!(link_at(line, 58), None);
        assert_eq!(link_at("", 0), None);
        assert_eq!(link_at("[[]]", 1), None);
        assert_eq!(link_at("[a](<b> \"t\")", 1), Some(Link::Url("<b>".into())));
    }

    #[test]
    fn schemes_are_told_from_paths() {
        assert!(has_scheme("https://x.y"));
        assert!(has_scheme("mailto:a@b"));
        assert!(!has_scheme("notes/c:d.md"));
        assert!(
            !has_scheme("c:/windows/style.md") || cfg!(windows),
            "one letter is a drive"
        );
        assert!(!has_scheme("plain.md"));
    }

    #[test]
    fn find_note_by_path_then_by_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("deep/er")).expect("mkdir");
        fs::create_dir_all(root.join(".git")).expect("mkdir");
        fs::write(root.join("Top.md"), "").expect("write");
        fs::write(root.join("deep/er/Buried Note.md"), "").expect("write");
        fs::write(root.join(".git/Secret.md"), "").expect("write");

        assert_eq!(find_note(root, "Top"), Some(root.join("Top.md")));
        // Case-insensitive file systems answer the direct lookup in the
        // asked-for case; the others fall through to the search.
        assert!(find_note(root, "top").is_some_and(|p| p
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("Top.md"))));
        assert_eq!(
            find_note(root, "buried note"),
            Some(root.join("deep/er/Buried Note.md"))
        );
        assert_eq!(
            find_note(root, "deep/er/Buried Note"),
            Some(root.join("deep/er/Buried Note.md"))
        );
        assert_eq!(
            find_note(root, "Secret"),
            None,
            "hidden directories are skipped"
        );
        assert_eq!(find_note(root, "Missing"), None);
        assert_eq!(find_note(root, ""), None);
    }
}
