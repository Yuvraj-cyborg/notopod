//! Markdown to [`Document`], built on `pulldown-cmark`.
//!
//! The parser is CommonMark plus the GitHub extensions people actually use
//! in notes: tables, strikethrough, task lists and YAML front matter.

use std::iter::Peekable;
use std::ops::Range;

use pulldown_cmark::{
    Alignment as PdAlignment, CodeBlockKind, Event, OffsetIter, Options, Parser, Tag, TagEnd,
};

use crate::ast::{Alignment, Block, BlockKind, Document, Inline, ListItem, Span};

/// Parses Markdown `source` into a [`Document`].
pub fn parse(source: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);

    let parser = Parser::new_ext(source, options);
    let mut builder = Builder {
        events: parser.into_offset_iter().peekable(),
        pending_task: None,
    };
    let blocks = builder.blocks(None);
    Document { blocks }
}

struct Builder<'a> {
    events: Peekable<OffsetIter<'a>>,
    /// Set when a `TaskListMarker` is seen; claimed by the enclosing list item.
    pending_task: Option<bool>,
}

impl Builder<'_> {
    /// Parses blocks until the matching `until` end tag (or end of input).
    // One arm per block kind; splitting it up would only hide the shape.
    #[allow(clippy::too_many_lines)]
    fn blocks(&mut self, until: Option<TagEnd>) -> Vec<Block> {
        let mut out = Vec::new();
        loop {
            // Inline content directly at block level happens inside tight list
            // items. Treat it as a paragraph so every leaf is a block.
            if self.peek_is_inline() {
                let (inlines, span) = self.inlines();
                if let Some(span) = span {
                    out.push(Block {
                        span,
                        kind: BlockKind::Paragraph(inlines),
                    });
                }
                continue;
            }

            let Some((event, range)) = self.events.next() else {
                break;
            };

            match event {
                Event::End(end) => {
                    if Some(end) == until {
                        break;
                    }
                    // Stray end tag; nothing sensible to do but skip it.
                }
                Event::Start(tag) => {
                    let end = tag.to_end();
                    match tag {
                        Tag::Paragraph => {
                            let (inlines, _) = self.inlines();
                            self.expect_end(end);
                            out.push(Block {
                                span: range,
                                kind: BlockKind::Paragraph(inlines),
                            });
                        }
                        Tag::Heading { level, .. } => {
                            let (content, _) = self.inlines();
                            self.expect_end(end);
                            out.push(Block {
                                span: range,
                                kind: BlockKind::Heading {
                                    level: level as u8,
                                    content,
                                },
                            });
                        }
                        Tag::CodeBlock(kind) => {
                            let lang = match kind {
                                CodeBlockKind::Fenced(info) => {
                                    info.split_whitespace().next().map(str::to_owned)
                                }
                                CodeBlockKind::Indented => None,
                            };
                            let code = self.collect_text(end);
                            out.push(Block {
                                span: range,
                                kind: BlockKind::CodeBlock { lang, code },
                            });
                        }
                        Tag::BlockQuote(_) => {
                            let inner = self.blocks(Some(end));
                            out.push(Block {
                                span: range,
                                kind: BlockKind::BlockQuote(inner),
                            });
                        }
                        Tag::List(start) => {
                            let items = self.list_items();
                            out.push(Block {
                                span: range,
                                kind: BlockKind::List { start, items },
                            });
                        }
                        Tag::HtmlBlock => {
                            let html = self.collect_text(end);
                            out.push(Block {
                                span: range,
                                kind: BlockKind::Html(html),
                            });
                        }
                        Tag::MetadataBlock(_) => {
                            let text = self.collect_text(end);
                            out.push(Block {
                                span: range,
                                kind: BlockKind::Metadata(text),
                            });
                        }
                        Tag::Table(alignments) => {
                            let (header, rows) = self.table();
                            let alignments =
                                alignments.into_iter().map(convert_alignment).collect();
                            out.push(Block {
                                span: range,
                                kind: BlockKind::Table {
                                    alignments,
                                    header,
                                    rows,
                                },
                            });
                        }
                        // Anything we do not model (footnotes, definition lists,
                        // stray items/cells): keep its content, drop the wrapper.
                        _ => {
                            let inner = self.blocks(Some(end));
                            out.extend(inner);
                        }
                    }
                }
                Event::Rule => out.push(Block {
                    span: range,
                    kind: BlockKind::Rule,
                }),
                Event::Html(html) => out.push(Block {
                    span: range,
                    kind: BlockKind::Html(html.into_string()),
                }),
                Event::DisplayMath(math) => out.push(Block {
                    span: range,
                    kind: BlockKind::Paragraph(vec![Inline::Text(math.into_string())]),
                }),
                // Every inline event is handled by `peek_is_inline` above.
                _ => {}
            }
        }
        out
    }

    /// Parses list items until the closing `List` tag.
    fn list_items(&mut self) -> Vec<ListItem> {
        let mut items = Vec::new();
        while let Some((event, range)) = self.events.next() {
            match event {
                Event::Start(Tag::Item) => {
                    let saved = self.pending_task.take();
                    let blocks = self.blocks(Some(TagEnd::Item));
                    let checked = self.pending_task.take();
                    self.pending_task = saved;
                    items.push(ListItem {
                        span: range,
                        checked,
                        blocks,
                    });
                }
                Event::End(TagEnd::List(_)) => break,
                _ => {}
            }
        }
        items
    }

    /// Parses a table body until the closing `Table` tag.
    #[allow(clippy::type_complexity)]
    fn table(&mut self) -> (Vec<Vec<Inline>>, Vec<Vec<Vec<Inline>>>) {
        let mut header = Vec::new();
        let mut rows = Vec::new();
        while let Some((event, _)) = self.events.next() {
            match event {
                Event::Start(Tag::TableHead) => header = self.table_row(TagEnd::TableHead),
                Event::Start(Tag::TableRow) => rows.push(self.table_row(TagEnd::TableRow)),
                Event::End(TagEnd::Table) => break,
                _ => {}
            }
        }
        (header, rows)
    }

    fn table_row(&mut self, end: TagEnd) -> Vec<Vec<Inline>> {
        let mut cells = Vec::new();
        while let Some((event, _)) = self.events.next() {
            match event {
                Event::Start(Tag::TableCell) => {
                    let (inlines, _) = self.inlines();
                    self.expect_end(TagEnd::TableCell);
                    cells.push(inlines);
                }
                Event::End(e) if e == end => break,
                _ => {}
            }
        }
        cells
    }

    /// Parses a run of inline events. Stops (without consuming) at the first
    /// non-inline event. Returns the inlines and the source span they cover.
    fn inlines(&mut self) -> (Vec<Inline>, Option<Span>) {
        let mut out = Vec::new();
        let mut span: Option<Span> = None;

        while self.peek_is_inline() {
            let Some((event, range)) = self.events.next() else {
                break;
            };
            extend_span(&mut span, &range);

            match event {
                Event::Text(t) | Event::InlineHtml(t) | Event::InlineMath(t) => {
                    push_text(&mut out, &t);
                }
                Event::FootnoteReference(r) => push_text(&mut out, &format!("[^{r}]")),
                Event::Code(c) => out.push(Inline::Code(c.into_string())),
                Event::SoftBreak => out.push(Inline::SoftBreak),
                Event::HardBreak => out.push(Inline::HardBreak),
                Event::TaskListMarker(checked) => self.pending_task = Some(checked),
                Event::Start(tag) => {
                    let end = tag.to_end();
                    let (inner, inner_span) = self.inlines();
                    if let Some(s) = inner_span {
                        extend_span(&mut span, &s);
                    }
                    if let Some(end_range) = self.expect_end(end) {
                        extend_span(&mut span, &end_range);
                    }
                    match tag {
                        Tag::Emphasis => out.push(Inline::Emphasis(inner)),
                        Tag::Strong => out.push(Inline::Strong(inner)),
                        Tag::Strikethrough => out.push(Inline::Strikethrough(inner)),
                        Tag::Link { dest_url, .. } => out.push(Inline::Link {
                            url: dest_url.into_string(),
                            content: inner,
                        }),
                        Tag::Image { dest_url, .. } => out.push(Inline::Image {
                            url: dest_url.into_string(),
                            alt: Inline::plain_text(&inner),
                        }),
                        // Superscript / subscript: keep the text, drop the styling.
                        _ => out.extend(inner),
                    }
                }
                _ => {}
            }
        }
        (expand_wikilinks(out), span)
    }

    /// Concatenates text events until `end`. Used for code, HTML and metadata.
    fn collect_text(&mut self, end: TagEnd) -> String {
        let mut text = String::new();
        for (event, _) in self.events.by_ref() {
            match event {
                Event::Text(t) | Event::Html(t) | Event::InlineHtml(t) => text.push_str(&t),
                Event::End(e) if e == end => break,
                _ => {}
            }
        }
        text
    }

    /// Consumes events up to and including `End(end)`. Returns its range.
    fn expect_end(&mut self, end: TagEnd) -> Option<Range<usize>> {
        for (event, range) in self.events.by_ref() {
            if matches!(event, Event::End(e) if e == end) {
                return Some(range);
            }
        }
        None
    }

    fn peek_is_inline(&mut self) -> bool {
        self.events.peek().is_some_and(|(event, _)| {
            matches!(
                event,
                Event::Text(_)
                    | Event::Code(_)
                    | Event::InlineHtml(_)
                    | Event::InlineMath(_)
                    | Event::FootnoteReference(_)
                    | Event::SoftBreak
                    | Event::HardBreak
                    | Event::TaskListMarker(_)
                    | Event::Start(
                        Tag::Emphasis
                            | Tag::Strong
                            | Tag::Strikethrough
                            | Tag::Superscript
                            | Tag::Subscript
                            | Tag::Link { .. }
                            | Tag::Image { .. }
                    )
            )
        })
    }
}

/// `[[Target]]` and `[[Target|shown]]` in plain text become links to
/// `Target`, the way Obsidian and its relatives write links between
/// notes. CommonMark has no such syntax, so pulldown-cmark hands them
/// over as text and they are picked out here. A `#heading` suffix is
/// dropped from the target; the shown text is the alias, or the target.
fn expand_wikilinks(inlines: Vec<Inline>) -> Vec<Inline> {
    if !inlines
        .iter()
        .any(|i| matches!(i, Inline::Text(t) if t.contains("[[")))
    {
        return inlines;
    }
    let mut out = Vec::with_capacity(inlines.len() + 2);
    for inline in inlines {
        match inline {
            Inline::Text(text) if text.contains("[[") => split_wikilinks(&text, &mut out),
            other => out.push(other),
        }
    }
    out
}

fn split_wikilinks(text: &str, out: &mut Vec<Inline>) {
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(len) = after.find("]]") else {
            break;
        };
        let inner = &after[..len];
        let target = inner.split_once('|').map_or(inner, |(t, _)| t);
        let target = target.split('#').next().unwrap_or(target).trim();
        if inner.contains(['[', ']', '\n']) || target.is_empty() {
            // Not a link: keep the brackets and look past them.
            push_text(out, &rest[..start + 2]);
            rest = after;
            continue;
        }
        let shown = inner.split_once('|').map_or(target, |(_, s)| s.trim());
        let shown = if shown.is_empty() { target } else { shown };
        if start > 0 {
            push_text(out, &rest[..start]);
        }
        out.push(Inline::Link {
            url: target.to_owned(),
            content: vec![Inline::Text(shown.to_owned())],
        });
        rest = &after[len + 2..];
    }
    if !rest.is_empty() {
        push_text(out, rest);
    }
}

/// Appends text, merging with a preceding `Text` node.
fn push_text(out: &mut Vec<Inline>, text: &str) {
    if let Some(Inline::Text(last)) = out.last_mut() {
        last.push_str(text);
    } else {
        out.push(Inline::Text(text.to_owned()));
    }
}

fn extend_span(span: &mut Option<Span>, range: &Range<usize>) {
    match span {
        Some(s) => {
            s.start = s.start.min(range.start);
            s.end = s.end.max(range.end);
        }
        None => *span = Some(range.clone()),
    }
}

fn convert_alignment(a: PdAlignment) -> Alignment {
    match a {
        PdAlignment::None => Alignment::None,
        PdAlignment::Left => Alignment::Left,
        PdAlignment::Center => Alignment::Center,
        PdAlignment::Right => Alignment::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<BlockKind> {
        parse(src).blocks.into_iter().map(|b| b.kind).collect()
    }

    #[test]
    fn empty_input_is_empty_document() {
        assert!(parse("").is_empty());
        assert!(parse("\n\n").is_empty());
    }

    #[test]
    fn paragraphs_and_headings_keep_spans() {
        let src = "# Title\n\nHello *world*.\n";
        let doc = parse(src);
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(&src[doc.blocks[0].span.clone()], "# Title\n");
        assert_eq!(&src[doc.blocks[1].span.clone()], "Hello *world*.\n");
        assert_eq!(
            doc.blocks[0].kind,
            BlockKind::Heading {
                level: 1,
                content: vec![Inline::Text("Title".into())]
            }
        );
        assert_eq!(
            doc.blocks[1].kind,
            BlockKind::Paragraph(vec![
                Inline::Text("Hello ".into()),
                Inline::Emphasis(vec![Inline::Text("world".into())]),
                Inline::Text(".".into()),
            ])
        );
    }

    #[test]
    fn block_at_finds_containing_block() {
        let doc = parse("# A\n\npara\n");
        assert_eq!(doc.block_at(0), Some(0));
        assert_eq!(doc.block_at(4), None);
        assert_eq!(doc.block_at(6), Some(1));
        assert_eq!(doc.block_at(99), None);
    }

    #[test]
    fn tight_list_items_get_implicit_paragraphs() {
        let src = "- one\n- two\n  nested\n";
        let doc = parse(src);
        let BlockKind::List { start, items } = &doc.blocks[0].kind else {
            panic!("expected list");
        };
        assert_eq!(*start, None);
        assert_eq!(items.len(), 2);
        assert_eq!(&src[items[0].span.clone()], "- one\n");
        assert!(matches!(items[0].blocks[0].kind, BlockKind::Paragraph(_)));
        assert_eq!(
            items[1].blocks[0].kind,
            BlockKind::Paragraph(vec![
                Inline::Text("two".into()),
                Inline::SoftBreak,
                Inline::Text("nested".into()),
            ])
        );
    }

    #[test]
    fn task_markers_attach_to_their_item() {
        let src = "- [ ] todo\n- [x] done\n- plain\n\n1. loose\n\n2. [x] loose done\n";
        let doc = parse(src);
        let BlockKind::List { items, .. } = &doc.blocks[0].kind else {
            panic!("expected list");
        };
        assert_eq!(items[0].checked, Some(false));
        assert_eq!(items[1].checked, Some(true));
        assert_eq!(items[2].checked, None);
        let BlockKind::List { start, items } = &doc.blocks[1].kind else {
            panic!("expected ordered list");
        };
        assert_eq!(*start, Some(1));
        assert_eq!(items[0].checked, None);
        assert_eq!(items[1].checked, Some(true));
    }

    #[test]
    fn nested_task_does_not_leak_to_parent() {
        let src = "- parent\n  - [ ] child\n";
        let doc = parse(src);
        let BlockKind::List { items, .. } = &doc.blocks[0].kind else {
            panic!("expected list");
        };
        assert_eq!(items[0].checked, None);
        let BlockKind::List { items: inner, .. } = &items[0].blocks[1].kind else {
            panic!("expected nested list");
        };
        assert_eq!(inner[0].checked, Some(false));
    }

    #[test]
    fn code_blocks_keep_language_and_body() {
        let src = "```rust title=x\nfn main() {}\n```\n\n    indented\n";
        assert_eq!(
            kinds(src),
            vec![
                BlockKind::CodeBlock {
                    lang: Some("rust".into()),
                    code: "fn main() {}\n".into()
                },
                BlockKind::CodeBlock {
                    lang: None,
                    code: "indented\n".into()
                },
            ]
        );
    }

    #[test]
    fn tables_split_header_and_rows() {
        let src = "| a | b |\n|:--|--:|\n| 1 | 2 |\n| 3 | 4 |\n";
        let doc = parse(src);
        let BlockKind::Table {
            alignments,
            header,
            rows,
        } = &doc.blocks[0].kind
        else {
            panic!("expected table");
        };
        assert_eq!(alignments, &[Alignment::Left, Alignment::Right]);
        assert_eq!(header.len(), 2);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1][1], vec![Inline::Text("4".into())]);
    }

    #[test]
    fn quotes_rules_and_strikethrough() {
        let src = "> quoted\n\n---\n\n~~gone~~\n";
        let k = kinds(src);
        assert!(matches!(&k[0], BlockKind::BlockQuote(inner) if inner.len() == 1));
        assert_eq!(k[1], BlockKind::Rule);
        assert_eq!(
            k[2],
            BlockKind::Paragraph(vec![Inline::Strikethrough(vec![Inline::Text(
                "gone".into()
            )])])
        );
    }

    #[test]
    fn links_and_images() {
        let k = kinds("[t](http://x) ![alt *a*](i.png)\n");
        assert_eq!(
            k[0],
            BlockKind::Paragraph(vec![
                Inline::Link {
                    url: "http://x".into(),
                    content: vec![Inline::Text("t".into())]
                },
                Inline::Text(" ".into()),
                Inline::Image {
                    url: "i.png".into(),
                    alt: "alt a".into()
                },
            ])
        );
    }

    #[test]
    fn front_matter_becomes_metadata() {
        let k = kinds("---\ntitle: x\n---\n\n# H\n");
        assert_eq!(k[0], BlockKind::Metadata("title: x\n".into()));
        assert!(matches!(k[1], BlockKind::Heading { level: 1, .. }));
    }

    #[test]
    fn wikilinks_become_links() {
        let link = |url: &str, shown: &str| Inline::Link {
            url: url.into(),
            content: vec![Inline::Text(shown.into())],
        };
        let k = kinds("see [[Plan]] and [[ideas/Old|the old one]], [[Plan#Goals]].\n");
        assert_eq!(
            k[0],
            BlockKind::Paragraph(vec![
                Inline::Text("see ".into()),
                link("Plan", "Plan"),
                Inline::Text(" and ".into()),
                link("ideas/Old", "the old one"),
                Inline::Text(", ".into()),
                link("Plan", "Plan"),
                Inline::Text(".".into()),
            ])
        );

        // Inside emphasis too; brackets that are not a link stay text.
        let k = kinds("*[[A]]* [[]] [[x\n");
        assert_eq!(
            k[0],
            BlockKind::Paragraph(vec![
                Inline::Emphasis(vec![link("A", "A")]),
                Inline::Text(" [[]] [[x".into()),
            ])
        );
        // A real link is left alone.
        let k = kinds("[t](u) [[W]]\n");
        assert_eq!(
            k[0],
            BlockKind::Paragraph(vec![
                Inline::Link {
                    url: "u".into(),
                    content: vec![Inline::Text("t".into())]
                },
                Inline::Text(" ".into()),
                link("W", "W"),
            ])
        );
    }

    #[test]
    fn never_panics_on_odd_input() {
        for src in [
            "*", "**", "[", "](", "```", "- ", "1.", "|", "|--|", "> > >", "#", "#######", "- [ ]",
            "---\n", "\t\t", "\u{feff}",
        ] {
            let _ = parse(src);
        }
    }
}
