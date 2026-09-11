//! Picking link targets out of Markdown text.

/// The targets of every link in `text` that could name a note:
/// `[[Name]]`, `[[Name|shown]]`, `[[Name#heading]]` and
/// `[shown](path.md)` (or a path with no extension). URLs, images and
/// anything inside a fenced code block are left out. Targets come back
/// as written, in order, duplicates included.
pub fn links_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        wiki_targets(line, &mut out);
        markdown_targets(line, &mut out);
    }
    out
}

fn wiki_targets(line: &str, out: &mut Vec<String>) {
    let mut rest = line;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(len) = after.find("]]") else {
            return;
        };
        let inner = &after[..len];
        if inner.contains(['[', ']']) {
            rest = after;
            continue;
        }
        let target = inner.split_once('|').map_or(inner, |(t, _)| t);
        let target = target.split('#').next().unwrap_or(target).trim();
        if !target.is_empty() {
            out.push(target.to_owned());
        }
        rest = &after[len + 2..];
    }
}

fn markdown_targets(line: &str, out: &mut Vec<String>) {
    let bytes = line.as_bytes();
    let mut rest = line;
    let mut offset = 0;
    while let Some(pos) = rest.find("](") {
        let close = rest[pos + 2..].find(')');
        let Some(len) = close else {
            return;
        };
        let inside = &rest[pos + 2..pos + 2 + len];
        // `![alt](image)` is a picture, not a link.
        let bracket = rest[..pos].rfind('[').map(|b| offset + b);
        let is_image = bracket.is_some_and(|b| b > 0 && bytes[b - 1] == b'!');
        // `<a name.md>` may hold spaces; a bare target ends at the first.
        let target = match inside.trim_start().strip_prefix('<') {
            Some(rest) => rest.split('>').next().unwrap_or(""),
            None => inside.split_whitespace().next().unwrap_or(""),
        };
        let target = target.split('#').next().unwrap_or(target);
        if !is_image && !target.is_empty() && !has_scheme(target) && names_a_note(target) {
            out.push(target.to_owned());
        }
        offset += pos + 2 + len + 1;
        rest = &rest[pos + 2 + len + 1..];
    }
}

/// A `.md` / `.markdown` path, or one with no extension at all.
fn names_a_note(target: &str) -> bool {
    let name = target.rsplit(['/', '\\']).next().unwrap_or(target);
    match name.rsplit_once('.') {
        Some((base, ext)) if !base.is_empty() => {
            ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown")
        }
        _ => true,
    }
}

fn has_scheme(target: &str) -> bool {
    target.split_once(':').is_some_and(|(scheme, rest)| {
        scheme.len() > 1
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
            && (rest.starts_with("//") || scheme.eq_ignore_ascii_case("mailto"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_of_note_link_and_nothing_else() {
        let text = "\
[[A]] and [[b/C|shown]] and [[D#part]] [[]] [[x[y]]
[t](E.md) [u](f/G.markdown#h) [v](<H I.md>) [w](plain) [img](pic.png) ![alt](J.md)
[web](https://x.y/z.md) [mail](mailto:a@b) [anchor](#top)
```
[[Code]] [c](Code.md)
```
[[After]]
";
        assert_eq!(
            links_in(text),
            vec![
                "A",
                "b/C",
                "D",
                "E.md",
                "f/G.markdown",
                "H I.md",
                "plain",
                "After"
            ]
        );
    }

    #[test]
    fn odd_input_does_not_panic() {
        for s in [
            "[[", "]]", "](", "[]()", "![](", "[[a|", "[a](b", "```", "[[a]]]]",
        ] {
            let _ = links_in(s);
        }
    }
}
