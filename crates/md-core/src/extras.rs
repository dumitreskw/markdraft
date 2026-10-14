//! Markdown conventions beyond CommonMark/GFM that common tools use:
//! front matter fields, Obsidian image embeds and `[TOC]` markers.

use comrak::nodes::{AstNode, NodeHtmlBlock, NodeValue};
use serde::Serialize;

use crate::render::Heading;

/// One front matter entry, flattened for display (`tags: [a, b]` → "a, b").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontMatterField {
    pub key: String,
    pub value: String,
}

/// Reads simple YAML front matter: top-level `key: value` pairs, inline
/// lists (`[a, b]`) and block lists (`- a`). Nested maps are skipped.
pub fn front_matter_fields(raw: &str) -> Vec<FrontMatterField> {
    let mut fields: Vec<FrontMatterField> = Vec::new();
    let mut list: Option<usize> = None;
    for line in raw.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() || trimmed.trim_start().starts_with('#') || trimmed.starts_with("---")
        {
            continue;
        }
        let indented = line.starts_with([' ', '\t']);
        if indented {
            // Items of a block list under the previous key.
            if let (Some(i), Some(item)) = (list, trimmed.trim_start().strip_prefix("- ")) {
                let item = unquote(item.trim());
                let v = &mut fields[i].value;
                if !v.is_empty() {
                    v.push_str(", ");
                }
                v.push_str(&item);
            }
            continue;
        }
        list = None;
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.contains(' ') {
            continue;
        }
        let value = value.trim();
        let value = if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            inner
                .split(',')
                .map(|s| unquote(s.trim()))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            unquote(value)
        };
        if value.is_empty() {
            list = Some(fields.len());
        }
        fields.push(FrontMatterField {
            key: key.to_owned(),
            value,
        });
    }
    fields.retain(|f| !f.value.is_empty());
    fields
}

fn unquote(s: &str) -> String {
    s.trim_matches(|c| c == '"' || c == '\'').to_owned()
}

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "bmp"];

/// Turns Obsidian image embeds (`![[photo.png]]`, `![[photo.png|300]]`,
/// `![[photo.png|A caption]]`) into inline `<img>` HTML before parsing. The
/// path resolves relative to the document, like a normal Markdown image.
/// Fenced code blocks and inline code spans are left alone.
pub fn rewrite_embeds(markdown: &str) -> std::borrow::Cow<'_, str> {
    if !markdown.contains("![[") {
        return markdown.into();
    }
    let mut out = String::with_capacity(markdown.len());
    let mut fence: Option<String> = None;
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let marker: String = trimmed
            .chars()
            .take_while(|&c| c == '`' || c == '~')
            .collect();
        if let Some(open) = &fence {
            if marker.len() >= open.len() && marker.starts_with(&open[..1]) {
                fence = None;
            }
            out.push_str(line);
            continue;
        }
        if marker.len() >= 3 && (marker.starts_with("```") || marker.starts_with("~~~")) {
            fence = Some(marker);
            out.push_str(line);
            continue;
        }
        // Only text outside `code spans` (even segments between backticks).
        for (i, part) in line.split('`').enumerate() {
            if i > 0 {
                out.push('`');
            }
            if i % 2 == 0 {
                out.push_str(&replace_embeds(part));
            } else {
                out.push_str(part);
            }
        }
    }
    out.into()
}

fn replace_embeds(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("![[") {
        let after = &rest[start + 3..];
        let Some(end) = after.find("]]") else { break };
        let inner = &after[..end];
        let (target, label) = inner.split_once('|').map_or((inner, ""), |(t, l)| (t, l));
        let target = target.trim();
        let is_image = target
            .rsplit('.')
            .next()
            .is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()));
        out.push_str(&rest[..start]);
        if is_image && !target.contains('\n') {
            out.push_str(&embed_html(target, label.trim()));
        } else {
            out.push_str(&rest[start..start + 3 + end + 2]);
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn embed_html(target: &str, label: &str) -> String {
    // The label after `|` is a width (`300`, `300x200`) or alt text.
    let width = label
        .split_once('x')
        .map_or(label, |(w, _)| w)
        .parse::<u32>()
        .ok();
    let alt = if width.is_some() || label.is_empty() {
        target
    } else {
        label
    };
    let mut html = format!(
        "<img src=\"{}\" alt=\"{}\"",
        attr(&target.replace(' ', "%20")),
        attr(alt)
    );
    if let Some(w) = width {
        html.push_str(&format!(" width=\"{w}\""));
    }
    html.push('>');
    html
}

fn attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

/// Markers that ask for a table of contents, as written alone on a line.
const TOC_MARKERS: &[&str] = &["[toc]", "[[_toc_]]", "[[toc]]", "${toc}", "<!-- toc -->"];

/// Replaces `[TOC]`-style paragraphs with a nested list of links to the
/// document's headings. A lone level-1 title is left out of the list.
pub(crate) fn expand_toc<'a>(root: &'a AstNode<'a>, headings: &[Heading]) -> bool {
    let markers: Vec<_> = root
        .descendants()
        .filter(|n| {
            let value = &n.data.borrow().value;
            let text = match value {
                NodeValue::Paragraph => n.collect_text(),
                NodeValue::HtmlBlock(b) => b.literal.clone(),
                _ => return false,
            };
            TOC_MARKERS.contains(&text.trim().to_ascii_lowercase().as_str())
        })
        .collect();
    if markers.is_empty() {
        return false;
    }
    let html = toc_html(headings);
    for node in markers {
        for child in node.children().collect::<Vec<_>>() {
            child.detach();
        }
        node.data.borrow_mut().value = NodeValue::HtmlBlock(NodeHtmlBlock {
            block_type: 6,
            literal: html.clone(),
        });
    }
    true
}

fn toc_html(headings: &[Heading]) -> String {
    let skip_title = headings.iter().filter(|h| h.level == 1).count() == 1;
    let items: Vec<&Heading> = headings
        .iter()
        .filter(|h| !(skip_title && h.level == 1) && !h.text.is_empty())
        .collect();
    let Some(base) = items.iter().map(|h| h.level).min() else {
        return "<nav class=\"toc\"></nav>\n".into();
    };
    let mut out = String::from("<nav class=\"toc\" aria-label=\"Contents\">\n<ul>\n");
    let mut depth = 0usize;
    for (i, h) in items.iter().enumerate() {
        let level = usize::from(h.level - base);
        if i > 0 {
            if level > depth {
                for _ in depth..level {
                    out.push_str("\n<ul>\n");
                }
            } else {
                out.push_str("</li>\n");
                for _ in level..depth {
                    out.push_str("</ul>\n</li>\n");
                }
            }
        }
        depth = level;
        out.push_str(&format!(
            "<li><a href=\"#{}\">{}</a>",
            attr(&h.id),
            attr(&h.text)
        ));
    }
    out.push_str("</li>\n");
    for _ in 0..depth {
        out.push_str("</ul>\n</li>\n");
    }
    out.push_str("</ul>\n</nav>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_become_images_outside_code() {
        let md = "A ![[pic one.png|240]] and ![[b.jpg|A cat]] and ![[note]]
`![[c.png]]`
```
![[d.png]]
```
";
        let out = rewrite_embeds(md);
        assert!(
            out.contains(r#"<img src="pic%20one.png" alt="pic one.png" width="240">"#),
            "{out}"
        );
        assert!(out.contains(r#"<img src="b.jpg" alt="A cat">"#));
        assert!(
            out.contains("![[note]]")
                && out.contains("`![[c.png]]`")
                && out.contains(
                    "
![[d.png]]
"
                )
        );
    }

    #[test]
    fn front_matter_fields_flatten_lists() {
        let raw = "---\ntitle: \"Plan\"\ntags: [docs, 'q4']\nauthors:\n  - Ana\n  - Bo\nnested:\n  key: x\nempty:\n---\n";
        let f = front_matter_fields(raw);
        let pairs: Vec<_> = f
            .iter()
            .map(|f| (f.key.as_str(), f.value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("title", "Plan"),
                ("tags", "docs, q4"),
                ("authors", "Ana, Bo")
            ]
        );
    }

    #[test]
    fn toc_nests_by_level_and_skips_a_lone_title() {
        let h = |level, text: &str, id: &str| Heading {
            level,
            text: text.into(),
            id: id.into(),
            line: 1,
        };
        let html = toc_html(&[
            h(1, "Doc", "doc"),
            h(2, "A", "a"),
            h(3, "A1", "a1"),
            h(2, "B", "b"),
        ]);
        assert_eq!(
            html,
            "<nav class=\"toc\" aria-label=\"Contents\">\n<ul>\n<li><a href=\"#a\">A</a>\n<ul>\n<li><a href=\"#a1\">A1</a></li>\n</ul>\n</li>\n<li><a href=\"#b\">B</a></li>\n</ul>\n</nav>\n"
        );
    }
}
