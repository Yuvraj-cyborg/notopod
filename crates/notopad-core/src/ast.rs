//! The document tree.

use std::ops::Range;

/// A byte range into the source text a node was parsed from.
pub type Span = Range<usize>;

/// A parsed note: a flat list of top-level blocks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    /// Top-level blocks in source order.
    pub blocks: Vec<Block>,
}

impl Document {
    /// Index of the top-level block that contains `byte`, if any.
    ///
    /// A block "contains" a byte when `span.start <= byte < span.end`.
    pub fn block_at(&self, byte: usize) -> Option<usize> {
        self.blocks
            .iter()
            .position(|b| b.span.start <= byte && byte < b.span.end)
    }

    /// `true` when the document has no blocks.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

/// A block-level element together with the source range it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Byte range in the source. Always covers whole lines.
    pub span: Span,
    /// What kind of block this is and its content.
    pub kind: BlockKind,
}

/// The different block-level elements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    /// A run of inline text.
    Paragraph(Vec<Inline>),
    /// `# Heading` through `###### Heading`.
    Heading {
        /// 1 through 6.
        level: u8,
        /// The heading text.
        content: Vec<Inline>,
    },
    /// A fenced or indented code block. `code` keeps its line breaks.
    CodeBlock {
        /// The info string's first word, e.g. `rust` for ```` ```rust ````.
        lang: Option<String>,
        /// The code itself, verbatim.
        code: String,
    },
    /// `> quoted` content. Can contain any other block.
    BlockQuote(Vec<Block>),
    /// A bullet or numbered list.
    List {
        /// `Some(n)` for a numbered list starting at `n`, `None` for bullets.
        start: Option<u64>,
        /// The items, in order.
        items: Vec<ListItem>,
    },
    /// A pipe table.
    Table {
        /// One alignment per column.
        alignments: Vec<Alignment>,
        /// Header cells.
        header: Vec<Vec<Inline>>,
        /// Body rows, each a list of cells.
        rows: Vec<Vec<Vec<Inline>>>,
    },
    /// A horizontal rule (`---`).
    Rule,
    /// A raw HTML block, kept verbatim.
    Html(String),
    /// YAML front matter between `---` fences at the top of the file.
    Metadata(String),
}

/// One entry in a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// Byte range of the whole item, including nested content.
    pub span: Span,
    /// `Some(done)` for a task item (`- [ ]` / `- [x]`), `None` otherwise.
    pub checked: Option<bool>,
    /// The item's content. A tight list item holds a single paragraph.
    pub blocks: Vec<Block>,
}

/// Column alignment in a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    /// No alignment specified.
    None,
    /// `:---`
    Left,
    /// `:---:`
    Center,
    /// `---:`
    Right,
}

/// Inline (span-level) content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    /// Plain text.
    Text(String),
    /// `` `code` ``.
    Code(String),
    /// `*emphasis*`.
    Emphasis(Vec<Inline>),
    /// `**strong**`.
    Strong(Vec<Inline>),
    /// `~~strikethrough~~`.
    Strikethrough(Vec<Inline>),
    /// `[text](url)`.
    Link {
        /// The link destination.
        url: String,
        /// The link text.
        content: Vec<Inline>,
    },
    /// `![alt](url)`.
    Image {
        /// The image source.
        url: String,
        /// Alternative text.
        alt: String,
    },
    /// A line break inside a paragraph that renders as a space.
    SoftBreak,
    /// A forced line break (two trailing spaces or a backslash).
    HardBreak,
}

impl Inline {
    /// Flattens inline content to plain text, dropping all formatting.
    pub fn plain_text(inlines: &[Inline]) -> String {
        let mut out = String::new();
        Self::push_plain(inlines, &mut out);
        out
    }

    fn push_plain(inlines: &[Inline], out: &mut String) {
        for inline in inlines {
            match inline {
                Inline::Text(t) | Inline::Code(t) => out.push_str(t),
                Inline::Emphasis(inner)
                | Inline::Strong(inner)
                | Inline::Strikethrough(inner)
                | Inline::Link { content: inner, .. } => Self::push_plain(inner, out),
                Inline::Image { alt, .. } => out.push_str(alt),
                Inline::SoftBreak => out.push(' '),
                Inline::HardBreak => out.push('\n'),
            }
        }
    }
}
