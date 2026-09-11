//! The notes in a folder and the links between them.
//!
//! [`scan`] reads every Markdown file under a directory, picks out the
//! `[[wikilinks]]` and `[text](note.md)` links in each, and returns a
//! [`Graph`] of notes and the links that connect them. [`Graph::drawing`]
//! lays that graph out with a force simulation and hands back a
//! [`canvas::Drawing`], so the graph is drawn by the same hand-drawn
//! renderer as a ```` ```draw ```` block: a picture in terminals that show
//! them, braille everywhere else, in the theme's colours.
//!
//! Everything here is bounded: the scan stops after [`MAX_NOTES`] files,
//! skips files over [`MAX_FILE_BYTES`], and reads one file at a time, so a
//! graph over a large vault costs the graph and not the vault.

mod layout;
mod links;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use canvas::{Attrs, BoxKind, Drawing, Heads, Point, Rect, Shape};

pub use links::links_in;

/// Notes read before a scan stops and says so.
pub const MAX_NOTES: usize = 2000;

/// Files larger than this are listed but not read for links.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Labels longer than this are cut with an ellipsis.
const LABEL_MAX: usize = 18;

/// One note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Where it is.
    pub path: PathBuf,
    /// The file name without extension; what `[[links]]` call it.
    pub name: String,
    /// The first `# heading`, when the note has one.
    pub title: Option<String>,
    /// Links from this note that resolve to another note.
    pub out: usize,
    /// Notes that link here.
    pub inbound: usize,
}

impl Node {
    /// What the node is labelled with in the drawing.
    pub fn label(&self) -> String {
        let text = self.title.as_deref().unwrap_or(&self.name);
        let count = text.chars().count();
        if count <= LABEL_MAX {
            text.to_owned()
        } else {
            let mut cut: String = text.chars().take(LABEL_MAX - 1).collect();
            cut.push('…');
            cut
        }
    }

    /// Links in and out.
    pub fn degree(&self) -> usize {
        self.out + self.inbound
    }
}

/// Notes and the links between them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    /// The folder that was scanned.
    pub root: PathBuf,
    /// Notes, in name order.
    pub nodes: Vec<Node>,
    /// `(from, to)` pairs of indices into `nodes`, each once, no loops.
    pub edges: Vec<(usize, usize)>,
    /// `true` when the scan stopped at [`MAX_NOTES`].
    pub truncated: bool,
}

/// Reads the notes under `root` and the links between them.
pub fn scan(root: &Path) -> Graph {
    let root = root.to_path_buf();
    let mut files = Vec::new();
    let mut truncated = false;
    collect(&root, &mut files, &mut truncated);
    files.sort();

    // Two ways a link can name a note: by relative path, or by file name.
    let mut by_path: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_name: BTreeMap<String, usize> = BTreeMap::new();
    let mut nodes: Vec<Node> = Vec::with_capacity(files.len());
    for path in files {
        let name = stem(&path);
        let index = nodes.len();
        if let Ok(rel) = path.strip_prefix(&root) {
            by_path.insert(key(&rel.to_string_lossy()), index);
        }
        by_name.entry(name.to_lowercase()).or_insert(index);
        nodes.push(Node {
            path,
            name,
            title: None,
            out: 0,
            inbound: 0,
        });
    }

    let mut edges: BTreeSet<(usize, usize)> = BTreeSet::new();
    for (from, node) in nodes.iter_mut().enumerate() {
        let small = fs::metadata(&node.path).is_ok_and(|m| m.len() <= MAX_FILE_BYTES);
        let Some(text) = small.then(|| fs::read_to_string(&node.path).ok()).flatten() else {
            continue;
        };
        node.title = first_heading(&text);
        let dir = node.path.parent().unwrap_or(&root);
        for target in links_in(&text) {
            let to = resolve(&target, dir, &root, &by_path, &by_name);
            if let Some(to) = to.filter(|&to| to != from) {
                edges.insert((from, to));
            }
        }
    }
    for &(from, to) in &edges {
        nodes[from].out += 1;
        nodes[to].inbound += 1;
    }

    Graph {
        root,
        nodes,
        edges: edges.into_iter().collect(),
        truncated,
    }
}

impl Graph {
    /// Whether there is anything to draw.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The graph as a drawing at most `width` cells wide: every note an
    /// ellipse labelled with its title or name, every link an arrow, laid
    /// out so linked notes sit near each other. Shapes come in the order
    /// edges, then nodes, so node `i` is shape `edges.len() + i` (see
    /// [`Graph::node_shape`]). The layout is deterministic for a given
    /// graph and width.
    pub fn drawing(&self, width: usize) -> Drawing {
        let mut drawing = Drawing::default();
        if self.nodes.is_empty() {
            drawing.shapes.push(Shape::Text {
                at: Point::new(1, 1),
                text: format!("No notes under {}", self.root.display()),
                attrs: Attrs {
                    color: Some("muted".to_owned()),
                    ..Attrs::default()
                },
            });
            return drawing;
        }
        let boxes = layout::place(self, width);
        for &(from, to) in &self.edges {
            drawing.shapes.push(Shape::Line {
                points: vec![center(&boxes[from]), center(&boxes[to])],
                heads: Heads {
                    start: false,
                    end: true,
                },
                label: None,
                attrs: Attrs {
                    color: Some("muted".to_owned()),
                    ..Attrs::default()
                },
            });
        }
        for (node, rect) in self.nodes.iter().zip(&boxes) {
            drawing.shapes.push(Shape::Box {
                kind: BoxKind::Ellipse,
                rect: *rect,
                label: Some(node.label()),
                attrs: Attrs {
                    color: node_color(node.degree()),
                    fill: node.degree() >= 4,
                    dashed: node.degree() == 0,
                    round: false,
                },
            });
        }
        drawing
    }

    /// Index in the drawing's shapes of node `node`.
    pub fn node_shape(&self, node: usize) -> usize {
        self.edges.len() + node
    }
}

fn node_color(degree: usize) -> Option<String> {
    match degree {
        0 => Some("muted".to_owned()),
        1 => None,
        2..=3 => Some("cyan".to_owned()),
        _ => Some("yellow".to_owned()),
    }
}

fn center(rect: &Rect) -> Point {
    Point::new(rect.x + rect.w / 2, rect.y + rect.h / 2)
}

/// Every Markdown file under `dir`, skipping hidden entries, up to
/// [`MAX_NOTES`].
fn collect(dir: &Path, out: &mut Vec<PathBuf>, truncated: &mut bool) {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = read.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if is_markdown(&path) {
                if out.len() >= MAX_NOTES {
                    *truncated = true;
                    return;
                }
                out.push(path);
            }
        }
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

/// A path as a lookup key: forward slashes, lower case, no leading `./`.
fn key(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .to_lowercase()
}

/// The first `# Heading` line, without the marker.
fn first_heading(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(|h| h.trim().trim_end_matches('#').trim().to_owned())
        .filter(|h| !h.is_empty())
}

/// Which note a link target names, if any: a path is taken relative to
/// the linking note (then to the root), a bare name is looked up by file
/// name; both ignore case.
fn resolve(
    target: &str,
    dir: &Path,
    root: &Path,
    by_path: &BTreeMap<String, usize>,
    by_name: &BTreeMap<String, usize>,
) -> Option<usize> {
    let mut target = target.to_owned();
    if !is_markdown(Path::new(&target)) {
        target.push_str(".md");
    }
    if target.contains(['/', '\\']) {
        let joined = normalize(&dir.join(&target));
        if let Ok(rel) = joined.strip_prefix(root) {
            if let Some(&i) = by_path.get(&key(&rel.to_string_lossy())) {
                return Some(i);
            }
        }
        if let Some(&i) = by_path.get(&key(&target)) {
            return Some(i);
        }
    } else if let Ok(rel) = normalize(&dir.join(&target)).strip_prefix(root) {
        if let Some(&i) = by_path.get(&key(&rel.to_string_lossy())) {
            return Some(i);
        }
    }
    let name = Path::new(&target)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())?;
    by_name.get(&name).copied()
}

/// `path` with `.` and `..` components folded away, without touching the
/// file system.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("projects")).expect("mkdir");
        fs::create_dir_all(root.join(".obsidian")).expect("mkdir");
        fs::write(
            root.join("Index.md"),
            "# Home\n\nSee [[Plan]], [[projects/Rocket|the rocket]] and [notes](projects/Rocket.md).\nAlso [[Nowhere]] and [web](https://x.y) and [[Index]].\n\n```\n[[Not A Link]]\n```\n",
        )
        .expect("write");
        fs::write(root.join("Plan.md"), "Back to [[index]].\n").expect("write");
        fs::write(
            root.join("projects/Rocket.md"),
            "# Rocket\n\n[up](../Plan.md)\n",
        )
        .expect("write");
        fs::write(root.join("Lonely.md"), "nothing\n").expect("write");
        fs::write(root.join(".obsidian/Hidden.md"), "[[Plan]]\n").expect("write");
        fs::write(root.join("picture.png"), "").expect("write");
        dir
    }

    fn names(g: &Graph) -> Vec<&str> {
        g.nodes.iter().map(|n| n.name.as_str()).collect()
    }

    fn edge_names(g: &Graph) -> Vec<(String, String)> {
        g.edges
            .iter()
            .map(|&(a, b)| (g.nodes[a].name.clone(), g.nodes[b].name.clone()))
            .collect()
    }

    #[test]
    fn scan_finds_notes_titles_and_links() {
        let dir = vault();
        let g = scan(dir.path());
        assert_eq!(names(&g), vec!["Index", "Lonely", "Plan", "Rocket"]);
        assert_eq!(g.nodes[0].title.as_deref(), Some("Home"));
        assert_eq!(g.nodes[3].title.as_deref(), Some("Rocket"));
        assert!(g.nodes[1].title.is_none());
        let mut edges = edge_names(&g);
        edges.sort();
        assert_eq!(
            edges,
            vec![
                ("Index".to_owned(), "Plan".to_owned()),
                ("Index".to_owned(), "Rocket".to_owned()),
                ("Plan".to_owned(), "Index".to_owned()),
                ("Rocket".to_owned(), "Plan".to_owned()),
            ],
            "one edge per pair, no self links, nothing from code blocks or unknown names"
        );
        assert_eq!(g.nodes[0].out, 2);
        assert_eq!(g.nodes[2].inbound, 2);
        assert_eq!(g.nodes[1].degree(), 0);
        assert!(!g.truncated);
    }

    #[test]
    fn drawing_has_an_edge_per_link_and_a_node_per_note() {
        let dir = vault();
        let g = scan(dir.path());
        let d = g.drawing(80);
        assert_eq!(d.shapes.len(), g.edges.len() + g.nodes.len());
        assert!(matches!(d.shapes[0], Shape::Line { .. }));
        let Shape::Box { label, attrs, .. } = &d.shapes[g.node_shape(1)] else {
            panic!("node 1 is a box");
        };
        assert_eq!(label.as_deref(), Some("Lonely"));
        assert!(attrs.dashed, "a note with no links is dashed");
        let (w, h) = d.extent();
        assert!(w <= 80, "fits the width: {w}");
        assert!(h >= 4);
        // Same input, same picture.
        assert_eq!(d, g.drawing(80));

        let empty = scan(&dir.path().join("projects").join("none"));
        assert!(empty.is_empty());
        assert!(matches!(empty.drawing(40).shapes[0], Shape::Text { .. }));
    }

    #[test]
    fn labels_are_cut_and_headings_cleaned() {
        let node = Node {
            path: PathBuf::new(),
            name: "a".repeat(30),
            title: None,
            out: 0,
            inbound: 0,
        };
        assert_eq!(node.label().chars().count(), LABEL_MAX);
        assert!(node.label().ends_with('…'));
        assert_eq!(
            first_heading("x\n# Title ##\n# Other\n"),
            Some("Title".to_owned())
        );
        assert_eq!(first_heading("## not h1\n"), None);
        assert_eq!(
            normalize(Path::new("/a/b/../c/./d.md")),
            PathBuf::from("/a/c/d.md")
        );
    }
}
