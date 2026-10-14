//! Markdown → sanitised HTML for the preview, plus document metadata.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use comrak::html::Anchorizer;
use comrak::nodes::NodeValue;
use comrak::options::Plugins;
use comrak::{Arena, format_html_with_plugins, parse_document};
use serde::Serialize;

use crate::highlight::Highlighter;

/// Maps an absolute file path to a URL the WebView can load (the app's
/// asset protocol). Used for local images.
pub type AssetUrlFn = Arc<dyn Fn(&Path) -> String + Send + Sync>;

/// Where the document lives, so relative image paths can be resolved.
#[derive(Clone, Default)]
pub struct RenderContext {
    /// Folder containing the document.
    pub base_dir: Option<PathBuf>,
    /// Rewrites local image paths; without it, relative images are left as-is.
    pub asset_url: Option<AssetUrlFn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Heading {
    pub level: u8,
    pub text: String,
    /// Anchor id, matching the `id` attribute in the rendered HTML.
    pub id: String,
    /// 1-based source line.
    pub line: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedDoc {
    pub html: String,
    pub title: Option<String>,
    pub headings: Vec<Heading>,
    pub word_count: usize,
    /// The UI loads KaTeX / Mermaid only when a document needs them.
    pub has_math: bool,
    pub has_mermaid: bool,
}

/// Renders Markdown to sanitised HTML with `data-sourcepos` attributes.
pub fn render(markdown: &str, ctx: &RenderContext) -> RenderedDoc {
    let mut opts = crate::options();
    opts.render.sourcepos = true;
    opts.render.r#unsafe = true; // raw HTML is allowed through, then sanitised below
    opts.render.tasklist_classes = true;

    let arena = Arena::new();
    let root = parse_document(&arena, markdown, &opts);

    let mut anchorizer = Anchorizer::new();
    let mut headings = Vec::new();
    let mut word_count = 0;
    let mut has_math = false;
    let mut has_mermaid = false;
    let mut front_matter_title = None;
    for node in root.descendants() {
        let data = node.data.borrow();
        match &data.value {
            NodeValue::Heading(h) => {
                let text = node.collect_text();
                headings.push(Heading {
                    level: h.level,
                    id: anchorizer.anchorize(&text),
                    text: text.trim().to_owned(),
                    line: u32::try_from(data.sourcepos.start.line).unwrap_or(u32::MAX),
                });
            }
            NodeValue::Text(t) => word_count += t.split_whitespace().count(),
            NodeValue::Math(_) => has_math = true,
            NodeValue::CodeBlock(cb) => {
                if cb.info.split_whitespace().next() == Some("mermaid") {
                    has_mermaid = true;
                }
            }
            NodeValue::FrontMatter(raw) => front_matter_title = crate::front_matter_title(raw),
            _ => {}
        }
    }

    let highlighter = Highlighter;
    let mut plugins = Plugins::default();
    plugins.render.codefence_syntax_highlighter = Some(&highlighter);
    let mut raw_html = String::with_capacity(markdown.len() * 2);
    format_html_with_plugins(root, &opts, &mut raw_html, &plugins)
        .expect("writing to a String cannot fail");

    let title = front_matter_title.or_else(|| {
        headings
            .iter()
            .find(|h| h.level == 1 && !h.text.is_empty())
            .map(|h| h.text.clone())
    });

    RenderedDoc {
        html: sanitize(&raw_html, ctx),
        title,
        headings,
        word_count,
        has_math,
        has_mermaid,
    }
}

fn sanitize(html: &str, ctx: &RenderContext) -> String {
    let base_dir = ctx.base_dir.clone();
    let asset_url = ctx.asset_url.clone();

    let mut builder = ammonia::Builder::default();
    builder
        .add_tags(&[
            "input",
            "section",
            "details",
            "summary",
            "kbd",
            "mark",
            "figure",
            "figcaption",
            "picture",
            "source",
        ])
        .add_generic_attributes(&[
            "id",
            "class",
            "align",
            "data-sourcepos",
            "data-math-style",
            "data-footnotes",
            "data-footnote-ref",
            "data-footnote-backref",
            "data-heading-content",
            "data-wikilink",
            "aria-label",
            "aria-hidden",
        ])
        .add_tag_attributes("input", &["type", "checked", "disabled"])
        .add_tag_attributes("img", &["width", "height", "loading"])
        .add_tag_attributes("details", &["open"])
        .add_tag_attributes("source", &["srcset", "media", "type"])
        .add_url_schemes(&["mdasset"])
        .url_relative(ammonia::UrlRelative::PassThrough)
        .link_rel(Some("noopener noreferrer"))
        .attribute_filter(
            move |element, attribute, value| match (element, attribute) {
                // Only read-only task-list checkboxes are allowed.
                ("input", "type") if value != "checkbox" => None,
                ("img", "src") | ("source", "srcset") => Some(
                    resolve_local(value, base_dir.as_deref(), asset_url.as_ref())
                        .map_or_else(|| value.into(), Into::into),
                ),
                _ => Some(value.into()),
            },
        );
    builder.clean(html).to_string()
}

/// Turns a relative or local image reference into an asset URL.
fn resolve_local(
    src: &str,
    base_dir: Option<&Path>,
    asset_url: Option<&AssetUrlFn>,
) -> Option<String> {
    let asset_url = asset_url?;
    if src.is_empty() || src.starts_with('#') || src.starts_with("//") || has_scheme(src) {
        return None;
    }
    let path_part = src.split(['?', '#']).next().unwrap_or(src);
    let decoded = percent_encoding::percent_decode_str(path_part).decode_utf8_lossy();
    let path = Path::new(decoded.as_ref());
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir?.join(path)
    };
    Some(asset_url(&absolute))
}

fn has_scheme(s: &str) -> bool {
    let Some((scheme, _)) = s.split_once(':') else {
        return false;
    };
    // A single letter is a Windows drive (`C:\…`), not a scheme.
    scheme.len() > 1
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(md: &str) -> String {
        render(md, &RenderContext::default()).html
    }

    #[test]
    fn source_positions_are_kept() {
        assert!(html("# Hi\n\ntext\n").contains(r#"data-sourcepos="3:1-3:4""#));
    }

    #[test]
    fn headings_have_ids_matching_metadata() {
        let doc = render(
            "# Intro\n\n## Intro\n\n### Getting *started*\n",
            &RenderContext::default(),
        );
        let ids: Vec<_> = doc.headings.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, ["intro", "intro-1", "getting-started"]);
        for id in ids {
            assert!(
                doc.html.contains(&format!(r#"id="{id}""#)),
                "{id} missing in {}",
                doc.html
            );
        }
        assert_eq!(doc.headings[2].line, 5);
        assert_eq!(doc.title.as_deref(), Some("Intro"));
    }

    #[test]
    fn scripts_and_handlers_are_stripped() {
        let out = html(
            "<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n[a](javascript:alert(1))\n",
        );
        assert!(!out.contains("<script"));
        assert!(!out.contains("onerror"));
        assert!(!out.contains("javascript:"));
    }

    #[test]
    fn task_lists_render_disabled_checkboxes() {
        let out = html("- [x] done\n- [ ] todo\n");
        assert!(out.contains(r#"type="checkbox""#));
        assert!(out.contains("checked"));
        assert!(!html("<input type=text>").contains("type=\"text\""));
    }

    #[test]
    fn math_mermaid_and_alerts() {
        let doc = render(
            "$x^2$\n\n```mermaid\ngraph TD; A-->B\n```\n\n> [!NOTE]\n> Hi\n",
            &RenderContext::default(),
        );
        assert!(doc.has_math && doc.has_mermaid);
        assert!(doc.html.contains("data-math-style"));
        assert!(doc.html.contains("language-mermaid"));
        assert!(doc.html.contains("markdown-alert-note"), "{}", doc.html);
    }

    #[test]
    fn local_images_use_the_asset_url() {
        let ctx = RenderContext {
            base_dir: Some(PathBuf::from("/docs")),
            asset_url: Some(Arc::new(|p: &Path| {
                format!("mdasset://localhost/{}", p.display())
            })),
        };
        let out = render("![a](img/my%20pic.png) ![b](https://x.org/a.png)", &ctx).html;
        let expected = Path::new("/docs").join("img/my pic.png");
        assert!(
            out.contains(&format!("mdasset://localhost/{}", expected.display())),
            "{out}"
        );
        assert!(out.contains("https://x.org/a.png"));
    }

    #[test]
    fn scheme_detection() {
        assert!(has_scheme("https://a"));
        assert!(has_scheme("mailto:a@b"));
        assert!(!has_scheme("C:\\img.png"));
        assert!(!has_scheme("img/a.png"));
    }

    #[test]
    fn word_count_counts_text_words() {
        let doc = render(
            "# Two words\n\nthree more words `code`\n",
            &RenderContext::default(),
        );
        assert_eq!(doc.word_count, 5);
    }
}
