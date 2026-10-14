//! Markdown parsing, rendering and metadata extraction.
//!
//! This crate is UI-free: it turns Markdown text into HTML and metadata
//! (title, headings) that the app and the exporter consume.

mod highlight;
mod render;

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

pub use highlight::{HighlightTheme, highlight_css, warm_up};
pub use render::{AssetUrlFn, Heading, RenderContext, RenderedDoc, render};

/// Front matter delimiter recognised at the top of a document.
pub const FRONT_MATTER_DELIMITER: &str = "---";

/// Parser options shared by every MarkDraft code path, so preview, title
/// extraction and export all agree on what a document contains.
pub fn options() -> Options<'static> {
    let mut opts = Options::default();
    opts.extension.table = true;
    opts.extension.tasklist = true;
    opts.extension.strikethrough = true;
    opts.extension.autolink = true;
    opts.extension.footnotes = true;
    opts.extension.front_matter_delimiter = Some(FRONT_MATTER_DELIMITER.to_owned());
    opts.extension.alerts = true;
    opts.extension.math_dollars = true;
    opts.extension.wikilinks_title_after_pipe = true;
    opts.extension.highlight = true;
    opts.extension.header_id_prefix = Some(String::new());
    opts
}

/// Returns the document title: front matter `title:` first, then the first
/// level-1 heading. `None` when the document has neither.
pub fn extract_title(markdown: &str) -> Option<String> {
    let arena = Arena::new();
    let opts = options();
    let root = parse_document(&arena, markdown, &opts);

    let mut first_h1 = None;
    for node in root.descendants() {
        match &node.data.borrow().value {
            NodeValue::FrontMatter(raw) => {
                if let Some(title) = front_matter_title(raw) {
                    return Some(title);
                }
            }
            NodeValue::Heading(h) if h.level == 1 && first_h1.is_none() => {
                first_h1 = Some(node);
            }
            _ => {}
        }
    }
    first_h1
        .map(plain_text)
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
}

/// Reads a `title:` key from raw YAML front matter without a YAML parser:
/// only a top-level scalar on a single line is supported.
pub(crate) fn front_matter_title(raw: &str) -> Option<String> {
    raw.lines()
        .filter_map(|line| line.strip_prefix("title:"))
        .map(|v| v.trim().trim_matches(|c| c == '"' || c == '\'').trim())
        .find(|v| !v.is_empty())
        .map(str::to_owned)
}

/// Concatenates the text content of a node, dropping inline formatting.
fn plain_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut out = String::new();
    for child in node.descendants() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => out.push_str(t),
            NodeValue::Code(c) => out.push_str(&c.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_from_first_h1() {
        let md = "Intro text\n\n## Not this\n\n# The *real* `title`\n\n# Second\n";
        assert_eq!(extract_title(md).as_deref(), Some("The real title"));
    }

    #[test]
    fn front_matter_title_wins_over_h1() {
        let md = "---\nauthor: x\ntitle: \"Front Matter Title\"\n---\n\n# Heading\n";
        assert_eq!(extract_title(md).as_deref(), Some("Front Matter Title"));
    }

    #[test]
    fn setext_heading_counts_as_h1() {
        assert_eq!(extract_title("Setext\n======\n").as_deref(), Some("Setext"));
    }

    #[test]
    fn no_title() {
        assert_eq!(extract_title("just a paragraph\n\n## sub\n"), None);
        assert_eq!(extract_title("#   \n"), None);
    }
}
