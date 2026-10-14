//! Splits a Markdown document into the blocks the document diff compares.
//!
//! Blocks come from comrak's AST and keep their exact source text and line
//! range. List items are split out one per block (nested lists become
//! deeper blocks), so editing one bullet does not mark the whole list as
//! changed. Lines no node claims (link reference definitions, for example)
//! become `Other` blocks so no edit goes unseen.

use comrak::nodes::{AstNode, ListType, NodeValue, TableAlignment};
use comrak::{Arena, Options, parse_document};

use super::{Align, BlockType};

/// One top-level unit of a document.
#[derive(Debug, Clone)]
pub(crate) struct Block {
    pub ty: BlockType,
    /// Exact source lines, joined with `\n`, without a trailing newline.
    pub text: String,
    /// 1-based inclusive line range.
    pub lines: (u32, u32),
    /// The text a reader sees changing: markers such as `## `, `- [ ] ` and
    /// `> ` are removed; for code, the code without its fences.
    pub content: String,
    /// Normalised form used to decide whether two blocks are the same.
    pub key: String,
    /// Parsed cells for tables.
    pub table: Option<TableSrc>,
}

/// A table split into cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TableSrc {
    pub aligns: Vec<Align>,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Coarse block families: only blocks of one family are paired as
/// "modified", except that prose blocks may change type (paragraph to
/// list item, say).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    Prose,
    Code,
    Table,
    Html,
    FrontMatter,
    Rule,
}

impl Block {
    pub fn family(&self) -> Family {
        match self.ty {
            BlockType::Paragraph
            | BlockType::Heading { .. }
            | BlockType::ListItem { .. }
            | BlockType::Quote
            | BlockType::Other => Family::Prose,
            BlockType::Code { .. } => Family::Code,
            BlockType::Table => Family::Table,
            BlockType::Html => Family::Html,
            BlockType::FrontMatter => Family::FrontMatter,
            BlockType::Rule => Family::Rule,
        }
    }
}

/// Parser options: GFM tables, task lists, strikethrough, autolinks,
/// footnotes and `---` front matter. Kept local so this crate does not
/// depend on other workspace crates; they match `md-core`'s options.
fn options() -> Options<'static> {
    let mut opts = Options::default();
    opts.extension.table = true;
    opts.extension.tasklist = true;
    opts.extension.strikethrough = true;
    opts.extension.autolink = true;
    opts.extension.footnotes = true;
    opts.extension.front_matter_delimiter = Some("---".to_owned());
    opts
}

/// Removes a UTF-8 BOM and converts CRLF and lone CR line endings to LF.
pub(crate) fn normalize(text: &str) -> std::borrow::Cow<'_, str> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n").into()
    } else {
        text.into()
    }
}

/// Splits normalised text into blocks, ordered by position.
pub(crate) fn split(text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.split('\n').collect();
    let arena = Arena::new();
    let opts = options();
    let root = parse_document(&arena, text, &opts);

    let mut b = Builder {
        lines: &lines,
        out: Vec::new(),
    };
    for node in root.children() {
        b.node(node);
    }
    let mut out = b.out;
    out.sort_by_key(|blk| blk.lines.0);
    fill_gaps(&lines, &mut out);
    out
}

struct Builder<'l> {
    lines: &'l [&'l str],
    out: Vec<Block>,
}

impl Builder<'_> {
    fn node<'a>(&mut self, node: &'a AstNode<'a>) {
        let (start, end) = span(node);
        let value = node.data.borrow().value.clone();
        let ty = match &value {
            NodeValue::List(_) => {
                self.list(node, 0);
                return;
            }
            NodeValue::Paragraph => BlockType::Paragraph,
            NodeValue::Heading(h) => BlockType::Heading { level: h.level },
            NodeValue::CodeBlock(c) => BlockType::Code {
                lang: c
                    .info
                    .split_whitespace()
                    .next()
                    .map(str::to_owned)
                    .filter(|s| !s.is_empty()),
            },
            NodeValue::Table(_) => BlockType::Table,
            NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) | NodeValue::Alert(_) => {
                BlockType::Quote
            }
            NodeValue::ThematicBreak => BlockType::Rule,
            NodeValue::HtmlBlock(_) => BlockType::Html,
            NodeValue::FrontMatter(_) => BlockType::FrontMatter,
            _ => BlockType::Other,
        };
        let Some((start, end)) = self.trim(start, end) else {
            return;
        };
        let text = self.text(start, end);
        let content = match (&ty, &value) {
            (BlockType::Code { .. }, NodeValue::CodeBlock(c)) => c
                .literal
                .strip_suffix('\n')
                .unwrap_or(&c.literal)
                .to_owned(),
            (BlockType::Heading { .. }, NodeValue::Heading(h)) => heading_content(&text, h.setext),
            (BlockType::Quote, _) => quote_content(&text),
            (BlockType::FrontMatter, _) => text.clone(),
            _ => text.clone(),
        };
        let table = matches!(value, NodeValue::Table(_)).then(|| {
            let aligns = match &value {
                NodeValue::Table(t) => t.alignments.iter().map(|a| align(*a)).collect(),
                _ => Vec::new(),
            };
            table_cells(&text, aligns)
        });
        self.push(ty, start, end, text, content, table);
    }

    fn list<'a>(&mut self, list: &'a AstNode<'a>, depth: u32) {
        let (ordered, first_number) = match &list.data.borrow().value {
            NodeValue::List(l) => (l.list_type == ListType::Ordered, l.start),
            _ => (false, 1),
        };
        for (i, item) in list.children().enumerate() {
            let checked = match &item.data.borrow().value {
                NodeValue::TaskItem(t) => Some(t.symbol.is_some()),
                _ => None,
            };
            let number = ordered.then(|| u32::try_from(first_number + i).unwrap_or(u32::MAX));
            let (start, end) = span(item);
            let nested: Vec<_> = item
                .children()
                .filter(|c| matches!(c.data.borrow().value, NodeValue::List(_)))
                .collect();

            // The item's own lines are its range minus nested lists.
            let mut chunks = Vec::new();
            let mut from = start;
            for n in &nested {
                let (ns, ne) = span(n);
                if ns > from {
                    chunks.push((from, ns - 1));
                }
                from = from.max(ne + 1);
            }
            if from <= end {
                chunks.push((from, end));
            }
            for (k, (s, e)) in chunks.into_iter().enumerate() {
                let Some((s, e)) = self.trim(s, e) else {
                    continue;
                };
                let text = self.text(s, e);
                if k == 0 && s == start {
                    let content = item_content(&text, checked.is_some());
                    let ty = BlockType::ListItem {
                        ordered,
                        depth,
                        checked,
                        number,
                    };
                    self.push(ty, s, e, text, content, None);
                } else {
                    // Text after a nested list, still inside the item.
                    let content = dedent(&text);
                    self.push(BlockType::Other, s, e, text, content, None);
                }
            }
            for n in nested {
                self.list(n, depth + 1);
            }
        }
    }

    /// Shrinks a 1-based inclusive range to exclude blank lines at its ends.
    fn trim(&self, mut start: usize, mut end: usize) -> Option<(usize, usize)> {
        end = end.min(self.lines.len());
        start = start.max(1);
        while start <= end && self.lines[start - 1].trim().is_empty() {
            start += 1;
        }
        while end >= start && self.lines[end - 1].trim().is_empty() {
            end -= 1;
        }
        (start <= end).then_some((start, end))
    }

    fn text(&self, start: usize, end: usize) -> String {
        self.lines[start - 1..end].join("\n")
    }

    fn push(
        &mut self,
        ty: BlockType,
        start: usize,
        end: usize,
        text: String,
        content: String,
        table: Option<TableSrc>,
    ) {
        let key = key(&ty, &content, table.as_ref());
        self.out.push(Block {
            ty,
            text,
            lines: (to_u32(start), to_u32(end)),
            content,
            key,
            table,
        });
    }
}

fn span(node: &AstNode<'_>) -> (usize, usize) {
    let pos = node.data.borrow().sourcepos;
    (pos.start.line, pos.end.line.max(pos.start.line))
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Turns runs of non-blank lines that no block covers into `Other` blocks.
fn fill_gaps(lines: &[&str], blocks: &mut Vec<Block>) {
    let mut covered = vec![false; lines.len() + 2];
    for b in blocks.iter() {
        for l in b.lines.0..=b.lines.1 {
            if let Some(c) = covered.get_mut(l as usize) {
                *c = true;
            }
        }
    }
    let mut extra = Vec::new();
    let mut run: Option<(usize, usize)> = None;
    for (i, line) in lines.iter().enumerate() {
        let n = i + 1;
        if !covered[n] && !line.trim().is_empty() {
            run = Some(match run {
                Some((s, _)) => (s, n),
                None => (n, n),
            });
        } else if let Some((s, e)) = run.take() {
            extra.push((s, e));
        }
    }
    if let Some(r) = run {
        extra.push(r);
    }
    if extra.is_empty() {
        return;
    }
    for (s, e) in extra {
        let text = lines[s - 1..e].join("\n");
        let key = key(&BlockType::Other, &text, None);
        blocks.push(Block {
            ty: BlockType::Other,
            content: text.clone(),
            text,
            lines: (to_u32(s), to_u32(e)),
            key,
            table: None,
        });
    }
    blocks.sort_by_key(|b| b.lines.0);
}

/// Equality key: block kind plus content with insignificant whitespace
/// removed (trailing spaces everywhere; for prose, all whitespace runs
/// collapse to one space so re-wrapped lines compare equal).
fn key(ty: &BlockType, content: &str, table: Option<&TableSrc>) -> String {
    let tag = match ty {
        BlockType::Paragraph | BlockType::Other => "p".to_owned(),
        BlockType::Heading { level } => format!("h{level}"),
        BlockType::ListItem {
            ordered,
            depth,
            checked,
            ..
        } => format!("li{}{depth}{checked:?}", if *ordered { 'o' } else { 'u' }),
        BlockType::Code { lang } => format!("code:{}", lang.as_deref().unwrap_or("")),
        BlockType::Table => "table".to_owned(),
        BlockType::Quote => "quote".to_owned(),
        BlockType::Rule => "rule".to_owned(),
        BlockType::Html => "html".to_owned(),
        BlockType::FrontMatter => "fm".to_owned(),
    };
    let body = match ty {
        BlockType::Rule => String::new(),
        BlockType::Table => match table {
            Some(t) => {
                let mut s = format!("{:?}", t.aligns);
                for row in std::iter::once(&t.header).chain(&t.rows) {
                    s.push('\n');
                    s.push_str(&row.join("\u{1F}"));
                }
                s
            }
            None => trim_line_ends(content),
        },
        BlockType::Code { .. } | BlockType::Html | BlockType::FrontMatter => {
            trim_line_ends(content)
        }
        _ => collapse_whitespace(content),
    };
    format!("{tag}\u{1E}{body}")
}

fn trim_line_ends(s: &str) -> String {
    s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
}

pub(crate) fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Heading text without `#` markers or the setext underline.
fn heading_content(text: &str, setext: bool) -> String {
    if setext {
        let lines: Vec<&str> = text.lines().collect();
        let body = &lines[..lines.len().saturating_sub(1)];
        return body.iter().map(|l| l.trim()).collect::<Vec<_>>().join("\n");
    }
    let s = text.trim().trim_start_matches('#').trim();
    // Optional closing sequence: `## Title ##`.
    let without_close = s.trim_end_matches('#');
    if without_close.len() < s.len()
        && (without_close.is_empty() || without_close.ends_with([' ', '\t']))
    {
        without_close.trim_end().to_owned()
    } else {
        s.to_owned()
    }
}

/// Quote text without the `>` markers.
fn quote_content(text: &str) -> String {
    text.lines()
        .map(|l| {
            let t = l.trim_start();
            match t.strip_prefix('>') {
                Some(rest) => rest.strip_prefix(' ').unwrap_or(rest),
                None => l,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// List item text without the bullet / number and task checkbox; later
/// lines lose the item's content indentation.
fn item_content(text: &str, task: bool) -> String {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    let lead = first.len() - first.trim_start_matches([' ', '\t']).len();
    let rest = &first[lead..];
    let marker = if rest.starts_with(['-', '+', '*']) {
        1
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && rest[digits..].starts_with(['.', ')']) {
            digits + 1
        } else {
            0
        }
    };
    let after = &rest[marker..];
    let spaces = after.len() - after.trim_start_matches(' ').len();
    let mut body = after[spaces.min(4)..].to_owned();
    if task {
        let t = body.trim_start();
        if t.len() >= 3 && t.starts_with('[') && t.as_bytes()[2] == b']' {
            body = t[3..].strip_prefix(' ').unwrap_or(&t[3..]).to_owned();
        }
    }
    let indent = lead + marker + spaces.clamp(1, 4);
    for line in lines {
        body.push('\n');
        let ws = line.len() - line.trim_start_matches([' ', '\t']).len();
        body.push_str(&line[ws.min(indent)..]);
    }
    body
}

/// Removes the common leading indentation.
fn dedent(text: &str) -> String {
    let indent = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start_matches([' ', '\t']).len())
        .min()
        .unwrap_or(0);
    text.lines()
        .map(|l| l.get(indent..).unwrap_or(l.trim_start()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn align(a: TableAlignment) -> Align {
    match a {
        TableAlignment::None => Align::None,
        TableAlignment::Left => Align::Left,
        TableAlignment::Center => Align::Center,
        TableAlignment::Right => Align::Right,
    }
}

/// Splits table source into header and body cells (the delimiter row is
/// skipped). Cells are split on unescaped pipes, as GFM does.
fn table_cells(text: &str, aligns: Vec<Align>) -> TableSrc {
    let mut rows = text.lines().map(split_row);
    let header = rows.next().unwrap_or_default();
    let _delimiter = rows.next();
    TableSrc {
        aligns,
        header,
        rows: rows.collect(),
    }
}

fn split_row(line: &str) -> Vec<String> {
    let mut s = line.trim();
    s = s.strip_prefix('|').unwrap_or(s);
    if s.ends_with('|') && !s.ends_with("\\|") {
        s = &s[..s.len() - 1];
    }
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut escaped = false;
    for c in s.chars() {
        if c == '|' && !escaped {
            cells.push(cell.trim().to_owned());
            cell.clear();
        } else {
            cell.push(c);
        }
        escaped = c == '\\' && !escaped;
    }
    cells.push(cell.trim().to_owned());
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(md: &str) -> Vec<(BlockType, (u32, u32), String)> {
        split(md)
            .into_iter()
            .map(|b| (b.ty, b.lines, b.content))
            .collect()
    }

    #[test]
    fn splits_top_level_blocks_with_line_ranges() {
        let md = "---\ntitle: T\n---\n\n# Title\n\nPara one\nline two.\n\n- a\n- [x] b\n  - nested\n\n1. first\n2. second\n\n```rust\nfn x() {}\n```\n\n| a | b |\n|---|--:|\n| 1 | 2 |\n\n> quoted\n> text\n\n***\n\n[ref]: https://example.com\n";
        let got = types(md);
        let expect = vec![
            (BlockType::FrontMatter, (1, 3), "---\ntitle: T\n---".into()),
            (BlockType::Heading { level: 1 }, (5, 5), "Title".into()),
            (BlockType::Paragraph, (7, 8), "Para one\nline two.".into()),
            (
                BlockType::ListItem {
                    ordered: false,
                    depth: 0,
                    checked: None,
                    number: None,
                },
                (10, 10),
                "a".into(),
            ),
            (
                BlockType::ListItem {
                    ordered: false,
                    depth: 0,
                    checked: Some(true),
                    number: None,
                },
                (11, 11),
                "b".into(),
            ),
            (
                BlockType::ListItem {
                    ordered: false,
                    depth: 1,
                    checked: None,
                    number: None,
                },
                (12, 12),
                "nested".into(),
            ),
            (
                BlockType::ListItem {
                    ordered: true,
                    depth: 0,
                    checked: None,
                    number: Some(1),
                },
                (14, 14),
                "first".into(),
            ),
            (
                BlockType::ListItem {
                    ordered: true,
                    depth: 0,
                    checked: None,
                    number: Some(2),
                },
                (15, 15),
                "second".into(),
            ),
            (
                BlockType::Code {
                    lang: Some("rust".into()),
                },
                (17, 19),
                "fn x() {}".into(),
            ),
            (
                BlockType::Table,
                (21, 23),
                "| a | b |\n|---|--:|\n| 1 | 2 |".into(),
            ),
            (BlockType::Quote, (25, 26), "quoted\ntext".into()),
            (BlockType::Rule, (28, 28), "***".into()),
            (
                BlockType::Other,
                (30, 30),
                "[ref]: https://example.com".into(),
            ),
        ];
        assert_eq!(got, expect);
    }

    #[test]
    fn table_cells_split_on_unescaped_pipes() {
        let t = table_cells(
            "| a | b \\| c |\n| :- | :-: |\n| 1 | `x` |",
            vec![Align::Left, Align::Center],
        );
        assert_eq!(t.header, ["a", "b \\| c"]);
        assert_eq!(t.rows, [vec!["1".to_owned(), "`x`".to_owned()]]);
    }

    #[test]
    fn heading_markers_are_stripped() {
        assert_eq!(heading_content("## Title ##", false), "Title");
        assert_eq!(heading_content("# C#", false), "C#");
        assert_eq!(heading_content("Title\n=====", true), "Title");
    }

    #[test]
    fn normalize_line_endings_and_bom() {
        assert_eq!(normalize("\u{FEFF}a\r\nb\rc\n"), "a\nb\nc\n");
    }
}
