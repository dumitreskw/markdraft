//! Markdown (comrak AST) → Typst markup.
//!
//! The generated markup only uses a small vocabulary of helper functions
//! (`md-alert`, `md-image`, `md-tasks`, ...) that every theme template
//! defines, so the structure of a document is independent of its look.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use comrak::nodes::{AlertType, AstNode, ListDelimType, ListType, NodeValue, TableAlignment};
use comrak::{Anchorizer, Arena, Options, parse_document};
use typst::foundations::Bytes;
use typst::visualize::{
    ExchangeFormat, ImageFormat, RasterFormat, RasterImage, SvgImage, VectorFormat,
};

use crate::ExportInput;
use crate::frontmatter;
use crate::markup::{escape_text, opt_string, string_literal};
use crate::math::latex_to_typst;

/// Front matter delimiter, matching `md-core`.
const FRONT_MATTER_DELIMITER: &str = "---";

/// Parser options used for export. They mirror `md_core::options()` and add
/// the extensions the exporter renders (alerts, math).
pub(crate) fn comrak_options() -> Options<'static> {
    let mut opts = Options::default();
    opts.extension.table = true;
    opts.extension.tasklist = true;
    opts.extension.strikethrough = true;
    opts.extension.autolink = true;
    opts.extension.footnotes = true;
    opts.extension.front_matter_delimiter = Some(FRONT_MATTER_DELIMITER.to_owned());
    opts.extension.alerts = true;
    opts.extension.math_dollars = true;
    opts.extension.math_code = true;
    // Same as the preview: [[Page|label]] prints its label, ==text== highlights.
    opts.extension.wikilinks_title_after_pipe = true;
    opts.extension.highlight = true;
    opts.extension.description_lists = true;
    opts
}

/// Fence languages drawn as diagrams when the UI supplies an SVG for them.
pub(crate) const DIAGRAM_LANGS: &[&str] = &["mermaid", "dot", "graphviz"];

/// Paragraphs (or HTML comments) that ask for a table of contents.
const TOC_MARKERS: &[&str] = &["[toc]", "[[_toc_]]", "[[toc]]", "${toc}", "<!-- toc -->"];

fn is_toc_marker(text: &str) -> bool {
    TOC_MARKERS.contains(&text.trim().to_ascii_lowercase().as_str())
}

/// How LaTeX math is emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MathMode {
    /// Converted to Typst math with mitex.
    Typeset,
    /// Shown as LaTeX source in a styled box.
    Source,
}

/// State shared by all documents of one export.
pub(crate) struct Shared {
    pub math_mode: MathMode,
    /// Curly quotes, dashes and ellipses.
    pub smart: bool,
    /// Whether any converted math was emitted (the math helpers are only
    /// imported when needed).
    pub uses_math: bool,
    /// Virtual files (images, diagrams) referenced by the markup.
    pub assets: Vec<(String, Bytes)>,
    images: HashMap<PathBuf, Option<ImageAsset>>,
}

/// An image registered as a virtual file.
#[derive(Debug, Clone)]
struct ImageAsset {
    path: String,
    /// Display width in points for raster images: pixels at 96 dpi, which is
    /// how a browser (and the preview) shows them regardless of metadata.
    width_pt: Option<f64>,
}

impl Shared {
    pub fn new(math_mode: MathMode) -> Self {
        Self {
            math_mode,
            smart: false,
            uses_math: false,
            assets: Vec::new(),
            images: HashMap::new(),
        }
    }

    fn add_asset(&mut self, ext: &str, data: Bytes) -> String {
        let path = format!("/assets/{}.{ext}", self.assets.len());
        self.assets.push((path.clone(), data));
        path
    }

    /// Loads, validates and registers a local image. `None` when the file is
    /// missing or not an image Typst can decode.
    fn local_image(&mut self, path: &Path) -> Option<ImageAsset> {
        if let Some(known) = self.images.get(path) {
            return known.clone();
        }
        let loaded = std::fs::read(path)
            .ok()
            .and_then(validate_image)
            .map(|image| ImageAsset {
                path: self.add_asset(image.ext, image.data),
                width_pt: image.width_pt,
            });
        self.images.insert(path.to_owned(), loaded.clone());
        loaded
    }
}

struct ValidImage {
    ext: &'static str,
    data: Bytes,
    width_pt: Option<f64>,
}

/// Checks that Typst can decode `data` and works out its file extension.
fn validate_image(data: Vec<u8>) -> Option<ValidImage> {
    let data = Bytes::new(data);
    let (ext, width_pt) = match ImageFormat::detect(&data)? {
        ImageFormat::Raster(RasterFormat::Exchange(format)) => {
            let image = RasterImage::plain(data.clone(), format).ok()?;
            let ext = match format {
                ExchangeFormat::Png => "png",
                ExchangeFormat::Jpg => "jpg",
                ExchangeFormat::Gif => "gif",
                ExchangeFormat::Webp => "webp",
            };
            // EXIF orientations 5-8 swap width and height.
            let px = if matches!(image.exif_rotation(), Some(5..=8)) {
                image.height()
            } else {
                image.width()
            };
            (ext, Some(f64::from(px) * 0.75))
        }
        // SVG user units are CSS pixels already.
        ImageFormat::Vector(VectorFormat::Svg) => {
            SvgImage::new(data.clone()).ok()?;
            ("svg", None)
        }
        _ => return None,
    };
    Some(ValidImage {
        ext,
        data,
        width_pt,
    })
}

/// Title and other metadata of one document.
#[derive(Debug, Default, Clone)]
pub(crate) struct DocMeta {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub author: Option<String>,
    pub date: Option<String>,
}

/// One document rendered to Typst markup (without preamble).
pub(crate) struct RenderedDoc {
    pub meta: DocMeta,
    pub body: String,
}

/// Renders one Markdown document. `doc` is the index of the document within
/// the export and keeps labels unique when several documents are combined.
pub(crate) fn render_doc(shared: &mut Shared, input: &ExportInput, doc: usize) -> RenderedDoc {
    let arena = Arena::new();
    let mut opts = comrak_options();
    opts.parse.smart = shared.smart;
    // Obsidian image embeds, the same way the preview reads them.
    let markdown = md_core::rewrite_embeds(input.markdown);
    let root = parse_document(&arena, &markdown, &opts);

    let fm = root
        .children()
        .find_map(|n| match &n.data.borrow().value {
            NodeValue::FrontMatter(raw) => Some(frontmatter::parse(raw)),
            _ => None,
        })
        .unwrap_or_default();

    // The first block, when it is a level-1 heading, is the document title.
    let first_h1 = root
        .children()
        .find(|n| !matches!(n.data.borrow().value, NodeValue::FrontMatter(_)))
        .filter(|n| matches!(n.data.borrow().value, NodeValue::Heading(ref h) if h.level == 1));
    let h1_text = first_h1.map(|n| plain_text(n).trim().to_owned());

    let clean = |s: &str| Some(s.trim().to_owned()).filter(|s| !s.is_empty());
    let title = input
        .title
        .and_then(clean)
        .or_else(|| fm.title.as_deref().and_then(clean))
        .or_else(|| h1_text.clone().filter(|t| !t.is_empty()));
    let promoted = first_h1.filter(|_| title.is_some() && h1_text == title);

    let mut r = Renderer::new(shared, input, doc, root, promoted);
    let body = r.blocks(root);
    RenderedDoc {
        meta: DocMeta {
            title,
            subtitle: fm.subtitle,
            author: fm.author,
            date: fm.date,
        },
        body,
    }
}

struct Renderer<'a, 's, 'i> {
    shared: &'s mut Shared,
    input: &'s ExportInput<'i>,
    doc: usize,
    /// Footnote definitions by name.
    footnotes: HashMap<String, &'a AstNode<'a>>,
    /// Footnotes currently being rendered (guards against self-reference).
    footnote_stack: Vec<String>,
    /// Footnotes that were already rendered once.
    footnotes_seen: HashSet<String>,
    /// Heading node → label, for headings that are rendered.
    heading_labels: HashMap<*const AstNode<'a>, String>,
    /// GitHub-style slugs that have a heading label.
    slugs: HashSet<String>,
    /// Heading node that became the title and is not rendered.
    promoted: Option<&'a AstNode<'a>>,
    /// Subtracted from heading levels so the top level in use becomes 1.
    level_shift: u8,
    /// Index of the next diagram fence (any of `DIAGRAM_LANGS`).
    diagram_index: usize,
}

impl<'a, 's, 'i> Renderer<'a, 's, 'i> {
    fn new(
        shared: &'s mut Shared,
        input: &'s ExportInput<'i>,
        doc: usize,
        root: &'a AstNode<'a>,
        promoted: Option<&'a AstNode<'a>>,
    ) -> Self {
        let mut footnotes = HashMap::new();
        let mut heading_labels = HashMap::new();
        let mut slugs = HashSet::new();
        let mut anchorizer = Anchorizer::new();
        let mut min_level = u8::MAX;
        for node in root.descendants() {
            match &node.data.borrow().value {
                NodeValue::FootnoteDefinition(def) => {
                    footnotes.insert(def.name.clone(), node);
                }
                NodeValue::Heading(h) => {
                    let slug = anchorizer.anchorize(&plain_text(node));
                    if promoted.is_some_and(|p| std::ptr::eq(p, node)) {
                        continue;
                    }
                    min_level = min_level.min(h.level);
                    heading_labels.insert(node as *const _, format!("md-{doc}-{slug}"));
                    slugs.insert(slug);
                }
                _ => {}
            }
        }
        Self {
            shared,
            input,
            doc,
            footnotes,
            footnote_stack: Vec::new(),
            footnotes_seen: HashSet::new(),
            heading_labels,
            slugs,
            promoted,
            level_shift: if min_level == u8::MAX {
                0
            } else {
                min_level - 1
            },
            diagram_index: 0,
        }
    }

    // ----- Blocks ---------------------------------------------------------

    /// Renders the block children of `parent`, separated by blank lines.
    fn blocks(&mut self, parent: &'a AstNode<'a>) -> String {
        let mut parts = Vec::new();
        for child in parent.children() {
            let part = self.block(child);
            if !part.trim().is_empty() {
                parts.push(part);
            }
        }
        parts.join("\n\n")
    }

    fn block(&mut self, node: &'a AstNode<'a>) -> String {
        if self.promoted.is_some_and(|p| std::ptr::eq(p, node)) {
            return String::new();
        }
        let value = node.data.borrow().value.clone();
        match value {
            NodeValue::FrontMatter(_) | NodeValue::FootnoteDefinition(_) => String::new(),
            NodeValue::Paragraph => self.paragraph(node),
            NodeValue::Heading(h) => {
                let depth = h.level.saturating_sub(self.level_shift).max(1);
                let body = self.inlines(node);
                let label = self
                    .heading_labels
                    .get(&(node as *const _))
                    .map(|l| format!("#label({})", string_literal(l)))
                    .unwrap_or_default();
                format!("#heading(depth: {depth})[{body}]{label}")
            }
            NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => {
                format!("#quote(block: true)[\n{}\n]", self.blocks(node))
            }
            NodeValue::Alert(alert) => {
                let kind = match alert.alert_type {
                    AlertType::Note => "note",
                    AlertType::Tip => "tip",
                    AlertType::Important => "important",
                    AlertType::Warning => "warning",
                    AlertType::Caution => "caution",
                };
                let title = alert
                    .title
                    .as_deref()
                    .map(str::trim)
                    .filter(|t| !t.is_empty());
                format!(
                    "#md-alert({}, title: {})[\n{}\n]",
                    string_literal(kind),
                    opt_string(title),
                    self.blocks(node)
                )
            }
            NodeValue::List(list) => self.list(node, &list),
            NodeValue::CodeBlock(cb) => self.code_block(&cb.info, &cb.literal),
            NodeValue::HtmlBlock(html) => self.html_block(&html.literal),
            NodeValue::ThematicBreak => "#md-rule()".to_owned(),
            NodeValue::Table(table) => self.table(node, &table.alignments),
            NodeValue::DescriptionTerm => {
                format!(
                    "#block(above: 0.9em, below: 0.4em, strong[{}])",
                    self.blocks(node)
                )
            }
            NodeValue::DescriptionDetails => {
                format!("#pad(left: 1.5em)[{}]", self.blocks(node))
            }
            // Containers we do not style specially: render their children.
            _ => self.blocks(node),
        }
    }

    fn paragraph(&mut self, node: &'a AstNode<'a>) -> String {
        if is_toc_marker(&node.collect_text()) {
            return "#md-toc()".to_owned();
        }
        // A paragraph holding a single image is a block image.
        let meaningful: Vec<_> = node
            .children()
            .filter(|c| match &c.data.borrow().value {
                NodeValue::Text(t) => !t.trim().is_empty(),
                NodeValue::SoftBreak | NodeValue::LineBreak => false,
                _ => true,
            })
            .collect();
        if let [only] = meaningful.as_slice() {
            let value = only.data.borrow().value.clone();
            if let NodeValue::Image(link) = value {
                return self.image(&ImageRef::markdown(only, &link), true);
            }
        }
        self.inlines(node).trim().to_owned()
    }

    fn list(&mut self, node: &'a AstNode<'a>, list: &comrak::nodes::NodeList) -> String {
        let tight = list.tight;
        let mut items = Vec::new();
        for item in node.children() {
            let value = item.data.borrow().value.clone();
            let body = self.blocks(item);
            if list.is_task_list {
                let state = match value {
                    NodeValue::TaskItem(task) => {
                        if task.symbol.is_some() {
                            "true"
                        } else {
                            "false"
                        }
                    }
                    _ => "none",
                };
                items.push(format!("({state}, [\n{body}\n])"));
            } else {
                items.push(format!("[\n{body}\n]"));
            }
        }
        let items = items.join(",\n");
        if list.is_task_list {
            return format!("#md-tasks(tight: {tight},\n{items}\n)");
        }
        match list.list_type {
            ListType::Bullet => format!("#list(tight: {tight},\n{items}\n)"),
            ListType::Ordered => {
                let numbering = match list.delimiter {
                    ListDelimType::Period => "1.",
                    ListDelimType::Paren => "1)",
                };
                format!(
                    "#enum(tight: {tight}, start: {}, numbering: {},\n{items}\n)",
                    list.start,
                    string_literal(numbering)
                )
            }
        }
    }

    fn code_block(&mut self, info: &str, literal: &str) -> String {
        let lang = info
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches(|c| c == '{' || c == '}' || c == '.')
            .to_owned();
        let code = literal.strip_suffix('\n').unwrap_or(literal);
        match lang.to_ascii_lowercase().as_str() {
            l if DIAGRAM_LANGS.contains(&l) => {
                let index = self.diagram_index;
                self.diagram_index += 1;
                if let Some(svg) = self.input.diagrams.get(&index)
                    && let Some(image) = validate_image(svg.as_bytes().to_vec())
                    && image.ext == "svg"
                {
                    let path = self.shared.add_asset("svg", image.data);
                    return format!("#md-diagram({})", string_literal(&path));
                }
            }
            "math" => {
                return self.math(code, true);
            }
            _ => {}
        }
        let lang = if lang.is_empty() {
            "none".to_owned()
        } else {
            string_literal(&lang)
        };
        format!("#raw(block: true, lang: {lang}, {})", string_literal(code))
    }

    fn html_block(&mut self, html: &str) -> String {
        if is_toc_marker(html) {
            return "#md-toc()".to_owned();
        }
        let mut parts = Vec::new();
        for img in html_images(html) {
            parts.push(self.image(&img, true));
        }
        let text = strip_html(html);
        if !text.trim().is_empty() {
            parts.push(escape_text(text.trim()));
        }
        parts.join("\n\n")
    }

    fn table(&mut self, node: &'a AstNode<'a>, alignments: &[TableAlignment]) -> String {
        let columns = alignments.len().max(1);
        let align: Vec<&str> = alignments
            .iter()
            .map(|a| match a {
                TableAlignment::None => "auto",
                TableAlignment::Left => "left",
                TableAlignment::Center => "center",
                TableAlignment::Right => "right",
            })
            .collect();
        let mut out = format!(
            "#table(\n  columns: {columns},\n  align: ({},),\n",
            align.join(", ")
        );
        for row in node.children() {
            let is_header = matches!(row.data.borrow().value, NodeValue::TableRow(true));
            let mut cells: Vec<String> = row
                .children()
                .map(|cell| format!("[{}]", self.inlines(cell).trim()))
                .collect();
            cells.resize(columns, "[]".to_owned());
            let cells = cells.join(", ");
            if is_header {
                out.push_str(&format!("  table.header({cells}),\n"));
            } else {
                out.push_str(&format!("  {cells},\n"));
            }
        }
        out.push(')');
        out
    }

    // ----- Inlines --------------------------------------------------------

    fn inlines(&mut self, parent: &'a AstNode<'a>) -> String {
        let mut out = String::new();
        for child in parent.children() {
            self.inline(child, &mut out);
        }
        out
    }

    fn inline(&mut self, node: &'a AstNode<'a>, out: &mut String) {
        let value = node.data.borrow().value.clone();
        match value {
            NodeValue::Text(text) => out.push_str(&escape_text(&text)),
            NodeValue::SoftBreak => out.push('\n'),
            NodeValue::LineBreak => out.push_str("\\\n"),
            NodeValue::Code(code) => {
                out.push_str(&format!("#raw({});", string_literal(&code.literal)));
            }
            NodeValue::Emph => out.push_str(&format!("#emph[{}];", self.inlines(node))),
            NodeValue::Strong => out.push_str(&format!("#strong[{}];", self.inlines(node))),
            NodeValue::Strikethrough => {
                out.push_str(&format!("#strike[{}];", self.inlines(node)));
            }
            NodeValue::Highlight => {
                out.push_str(&format!("#highlight[{}];", self.inlines(node)));
            }
            NodeValue::Link(link) => self.link(node, &link.url, out),
            NodeValue::Image(link) => {
                out.push_str(&self.image(&ImageRef::markdown(node, &link), false));
                out.push(';');
            }
            NodeValue::FootnoteReference(fref) => self.footnote(&fref.name, out),
            NodeValue::Math(math) => {
                out.push_str(&self.math(&math.literal, math.display_math));
                out.push(';');
            }
            NodeValue::HtmlInline(html) => {
                let lower = html.to_ascii_lowercase();
                if lower.starts_with("<br") {
                    out.push_str("#linebreak();");
                } else if lower.starts_with("<img") {
                    for img in html_images(&html) {
                        out.push_str(&self.image(&img, false));
                        out.push(';');
                    }
                }
            }
            NodeValue::Raw(text) => out.push_str(&escape_text(&text)),
            _ => {
                for child in node.children() {
                    self.inline(child, out);
                }
            }
        }
    }

    fn link(&mut self, node: &'a AstNode<'a>, url: &str, out: &mut String) {
        let mut body = self.inlines(node);
        if body.trim().is_empty() {
            body = escape_text(url);
        }
        if let Some(fragment) = url.strip_prefix('#') {
            let slug = percent_decode(fragment);
            if self.slugs.contains(&slug) {
                let label = format!("md-{}-{slug}", self.doc);
                out.push_str(&format!(
                    "#link(label({}))[{body}];",
                    string_literal(&label)
                ));
            } else {
                out.push_str(&body);
            }
            return;
        }
        if url.trim().is_empty() {
            out.push_str(&body);
            return;
        }
        out.push_str(&format!("#link({})[{body}];", string_literal(url)));
    }

    fn footnote(&mut self, name: &str, out: &mut String) {
        // Slugs never contain `:`, so this cannot collide with a heading label.
        let label = format!("md-{}:fn:{name}", self.doc);
        let Some(def) = self.footnotes.get(name).copied() else {
            return;
        };
        if self.footnote_stack.iter().any(|n| n == name) {
            return;
        }
        // The first reference renders the note; later ones point back to it.
        if !self.footnotes_seen.insert(name.to_owned()) {
            out.push_str(&format!("#footnote(label({}));", string_literal(&label)));
            return;
        }
        self.footnote_stack.push(name.to_owned());
        let body = self.blocks(def);
        self.footnote_stack.pop();
        out.push_str(&format!(
            "#footnote[{body}]#label({})",
            string_literal(&label)
        ));
    }

    fn math(&mut self, latex: &str, display: bool) -> String {
        if self.shared.math_mode == MathMode::Typeset
            && let Some(code) = latex_to_typst(latex)
        {
            self.shared.uses_math = true;
            return format!("#md-math({}, block: {display})", string_literal(&code));
        }
        format!(
            "#md-math-source({}, block: {display})",
            string_literal(latex.trim())
        )
    }

    fn image(&mut self, image: &ImageRef, block: bool) -> String {
        let alt = string_literal(image.alt.trim());
        let Some(asset) = self.resolve_image(&image.url) else {
            return format!(
                "#md-missing-image({}, alt: {alt}, block: {block})",
                string_literal(&image.url)
            );
        };
        // An explicit HTML width wins over the file's natural width.
        let width = image
            .width_px
            .map(|px| px * 0.75)
            .or(asset.width_pt)
            .map_or_else(|| "auto".to_owned(), |pt| format!("{pt:.1}pt"));
        let title = image.title.trim();
        let caption = if block && !title.is_empty() {
            format!(", caption: {}", string_literal(title))
        } else {
            String::new()
        };
        format!(
            "#md-image({}, alt: {alt}, width: {width}{caption}, block: {block})",
            string_literal(&asset.path)
        )
    }

    /// Maps an image URL to a virtual asset path, reading local files only.
    fn resolve_image(&mut self, url: &str) -> Option<ImageAsset> {
        let url = url.trim();
        let lower = url.to_ascii_lowercase();
        if url.is_empty()
            || lower.contains("://") && !lower.starts_with("file://")
            || lower.starts_with("data:")
            || lower.starts_with("//")
        {
            return None;
        }
        let path_part = url.strip_prefix("file://").unwrap_or(url);
        let path_part = path_part.split(['?', '#']).next().unwrap_or(path_part);
        let decoded = percent_decode(path_part);
        let path = Path::new(&decoded);
        let full = if path.is_absolute() {
            path.to_owned()
        } else {
            self.input.base_dir?.join(path)
        };
        self.shared.local_image(&full)
    }
}

/// Concatenates the text content of a node, dropping formatting.
fn plain_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut out = String::new();
    for child in node.descendants().skip(1) {
        match &child.data.borrow().value {
            NodeValue::Text(t) => out.push_str(t),
            NodeValue::Code(c) => out.push_str(&c.literal),
            NodeValue::Math(m) => out.push_str(&m.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            _ => {}
        }
    }
    out
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            )
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
}

/// An image reference from Markdown (`![alt](url "title")`) or HTML.
struct ImageRef {
    url: String,
    alt: String,
    title: String,
    /// `width` attribute of an HTML `<img>`, in CSS pixels.
    width_px: Option<f64>,
}

impl ImageRef {
    fn markdown<'a>(node: &'a AstNode<'a>, link: &comrak::nodes::NodeLink) -> Self {
        Self {
            url: link.url.clone(),
            alt: plain_text(node),
            title: link.title.clone(),
            width_px: None,
        }
    }
}

/// Extracts `<img src=... alt=... width=...>` tags from an HTML snippet.
fn html_images(html: &str) -> Vec<ImageRef> {
    let mut images = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut pos = 0;
    while let Some(start) = lower[pos..].find("<img") {
        let start = pos + start;
        let end = lower[start..].find('>').map_or(html.len(), |e| start + e);
        let tag = &html[start..end];
        if let Some(src) = html_attr(tag, "src") {
            let width_px = html_attr(tag, "width").and_then(|w| {
                w.trim()
                    .trim_end_matches("px")
                    .parse::<f64>()
                    .ok()
                    .filter(|w| *w > 0.0)
            });
            images.push(ImageRef {
                url: src,
                alt: html_attr(tag, "alt").unwrap_or_default(),
                title: html_attr(tag, "title").unwrap_or_default(),
                width_px,
            });
        }
        pos = end;
    }
    images
}

fn html_attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut search = 0;
    while let Some(found) = lower[search..].find(name) {
        let at = search + found;
        search = at + name.len();
        let before_ok = at > 0 && lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = tag[at + name.len()..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let rest = rest[1..].trim_start();
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
            _ => rest
                .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .next()
                .unwrap_or(""),
        };
        return Some(decode_entities(value));
    }
    None
}

/// Removes comments and tags from an HTML snippet and decodes common entities.
fn strip_html(html: &str) -> String {
    let mut text = String::new();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |e| &after[e + 3..]);
        } else if rest.starts_with('<') {
            rest = rest.find('>').map_or("", |e| &rest[e + 1..]);
            text.push(' ');
        } else {
            let next = rest.find('<').unwrap_or(rest.len());
            text.push_str(&rest[..next]);
            rest = &rest[next..];
        }
    }
    let decoded = decode_entities(&text);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", "\u{a0}")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExportOptions, markdown_to_typst};

    /// Generates Typst for `md` and checks that it parses cleanly.
    fn typst(md: &str) -> String {
        typst_with(ExportInput::new(md))
    }

    fn typst_with(input: ExportInput) -> String {
        let src = markdown_to_typst(&input, &ExportOptions::default()).unwrap();
        let errors = typst::syntax::parse(&src).errors_and_warnings().0;
        assert!(errors.is_empty(), "syntax errors {errors:?} in:\n{src}");
        src
    }

    /// The body after the preamble (theme import, setup, title).
    fn body(md: &str) -> String {
        let src = typst(md);
        src.lines()
            .filter(|l| {
                !l.starts_with("//")
                    && !l.starts_with("#import")
                    && !l.starts_with("#show: md-setup")
                    && !l.starts_with("#md-title")
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned()
    }

    #[test]
    fn paragraphs_and_inline_formatting() {
        assert_eq!(
            body("Hello *em* **strong** ~~gone~~ `code`."),
            "Hello #emph[em]; #strong[strong]; #strike[gone]; #raw(\"code\");."
        );
        assert_eq!(body("one\ntwo\n\nthree"), "one\ntwo\n\nthree");
        assert_eq!(body("hard  \nbreak"), "hard\\\nbreak");
    }

    #[test]
    fn headings_are_normalised_and_labelled() {
        let b = body("## Intro\n\n### Sub part");
        assert_eq!(
            b,
            "#heading(depth: 1)[Intro]#label(\"md-0-intro\")\n\n#heading(depth: 2)[Sub part]#label(\"md-0-sub-part\")"
        );
    }

    #[test]
    fn first_h1_becomes_the_title() {
        let src = typst("# The Title\n\nText\n\n# Second");
        assert!(src.contains("#md-title(title: \"The Title\""));
        assert!(!src.contains("[The Title]"));
        assert!(src.contains("#heading(depth: 1)[Second]"));
    }

    #[test]
    fn title_override_and_front_matter() {
        let mut input = ExportInput::new("---\ntitle: FM\nauthor: Ann\n---\n# Heading\n");
        let src = typst_with(input);
        assert!(src.contains("#md-title(title: \"FM\", subtitle: none, author: \"Ann\""));
        assert!(
            src.contains("[Heading]"),
            "H1 differs from the title and stays"
        );
        input.title = Some("Override");
        let src = typst_with(input);
        assert!(src.contains("title: \"Override\""));
    }

    #[test]
    fn lists() {
        assert_eq!(body("- a\n- b"), "#list(tight: true,\n[\na\n],\n[\nb\n]\n)");
        let b = body("3. x\n4. y");
        assert!(
            b.starts_with("#enum(tight: true, start: 3, numbering: \"1.\","),
            "{b}"
        );
        let b = body("1) x");
        assert!(b.contains("numbering: \"1)\""), "{b}");
        let b = body("- a\n\n- b");
        assert!(b.starts_with("#list(tight: false,"), "{b}");
        let b = body("- a\n  - nested");
        assert!(
            b.contains("[\na\n\n#list(tight: true,\n[\nnested\n]\n)\n]"),
            "{b}"
        );
    }

    #[test]
    fn task_lists() {
        let b = body("- [x] done\n- [ ] open\n- plain");
        assert_eq!(
            b,
            "#md-tasks(tight: true,\n(true, [\ndone\n]),\n(false, [\nopen\n]),\n(none, [\nplain\n])\n)"
        );
    }

    #[test]
    fn code_blocks() {
        assert_eq!(
            body("```rust\nfn main() {}\n```"),
            "#raw(block: true, lang: \"rust\", \"fn main() {}\")"
        );
        assert_eq!(
            body("    indented \"q\" \\"),
            "#raw(block: true, lang: none, \"indented \\\"q\\\" \\\\\")"
        );
        assert_eq!(
            body("```{.python .numberLines}\nx\n```"),
            "#raw(block: true, lang: \"python\", \"x\")"
        );
    }

    #[test]
    fn mermaid_uses_supplied_diagrams_by_index() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#;
        let diagrams = HashMap::from([(1, svg.to_owned())]);
        let md = "```mermaid\ngraph A\n```\n\n```mermaid\ngraph B\n```\n\n```mermaid\nbroken\n```";
        let mut input = ExportInput::new(md);
        input.diagrams = &diagrams;
        let b = typst_with(input);
        assert!(
            b.contains("#raw(block: true, lang: \"mermaid\", \"graph A\")"),
            "{b}"
        );
        assert!(b.contains("#md-diagram(\"/assets/0.svg\")"), "{b}");
        assert!(!b.contains("graph B"), "{b}");
        assert!(b.contains("\"broken\""), "{b}");
    }

    #[test]
    fn invalid_diagram_svg_falls_back_to_code() {
        let diagrams = HashMap::from([(0, "<not svg>".to_owned())]);
        let mut input = ExportInput::new("```mermaid\ngraph A\n```");
        input.diagrams = &diagrams;
        assert!(typst_with(input).contains("lang: \"mermaid\""));
    }

    #[test]
    fn block_quotes_and_alerts() {
        assert_eq!(body("> quoted"), "#quote(block: true)[\nquoted\n]");
        assert_eq!(
            body("> [!WARNING]\n> careful"),
            "#md-alert(\"warning\", title: none)[\ncareful\n]"
        );
        assert_eq!(
            body("> [!TIP] Pro tip\n> x"),
            "#md-alert(\"tip\", title: \"Pro tip\")[\nx\n]"
        );
        for kind in ["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"] {
            let b = body(&format!("> [!{kind}]\n> x"));
            assert!(b.contains(&format!("\"{}\"", kind.to_lowercase())), "{b}");
        }
    }

    #[test]
    fn tables() {
        let b = body("| a | b | c |\n|:--|:-:|--:|\n| 1 | *2* | 3 |\n| x |");
        assert_eq!(
            b,
            "#table(\n  columns: 3,\n  align: (left, center, right,),\n  table.header([a], [b], [c]),\n  [1], [#emph[2];], [3],\n  [x], [], [],\n)"
        );
    }

    #[test]
    fn links() {
        assert_eq!(
            body("[site](https://e.com/a?b=\"c\")"),
            "#link(\"https://e.com/a?b=\\\"c\\\"\")[site];"
        );
        assert_eq!(
            body("<https://e.com>"),
            "#link(\"https://e.com\")[https\\:\\//e.com];"
        );
        let b = body("## Part one\n\n[jump](#part-one) [nowhere](#missing) [empty]()");
        assert!(b.contains("#link(label(\"md-0-part-one\"))[jump];"), "{b}");
        assert!(b.contains(" nowhere empty"), "{b}");
    }

    #[test]
    fn footnotes() {
        let b = body("A[^n] B[^n] C[^m]\n\n[^n]: Note *n*.\n[^m]: Other.");
        assert_eq!(
            b,
            "A#footnote[Note #emph[n];.]#label(\"md-0:fn:n\") B#footnote(label(\"md-0:fn:n\")); C#footnote[Other.]#label(\"md-0:fn:m\")"
        );
    }

    #[test]
    fn thematic_break_and_html() {
        assert_eq!(body("a\n\n---\n\nb"), "a\n\n#md-rule()\n\nb");
        assert_eq!(body("a<br>b <span>c</span>"), "a#linebreak();b c");
        assert_eq!(body("<div>\n<b>x</b> &amp; y\n</div>"), "x & y");
        assert_eq!(body("<!-- hidden -->"), "");
    }

    #[test]
    fn math() {
        let b = body("$x^2$ and\n\n$$\n\\frac{a}{b}\n$$");
        assert!(b.contains("#md-math(\"x ^(2 )\", block: false);"), "{b}");
        assert!(b.contains("#md-math(\"frac(a ,b )\", block: true);"), "{b}");
        assert!(typst("$x$").contains("#import \"/markdraft/math.typ\": md-math"));
        assert!(!typst("no math").contains("math.typ"));
        let b = body("```math\n\\alpha\n```");
        assert_eq!(b, "#md-math(\"alpha \", block: true)");
    }

    #[test]
    fn images_resolve_against_base_dir_only() {
        let dir = tempfile::tempdir().unwrap();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"></svg>"#;
        std::fs::write(dir.path().join("pic.svg"), svg).unwrap();
        std::fs::write(dir.path().join("fake.png"), "text").unwrap();
        let md = "![A](pic.svg \"Cap\")\n\nInline ![B](pic.svg) ![C](fake.png) ![D](https://x.y/z.png) ![E](missing.png)";
        let mut input = ExportInput::new(md);
        input.base_dir = Some(dir.path());
        let b = typst_with(input);
        assert!(
            b.contains(
                "#md-image(\"/assets/0.svg\", alt: \"A\", width: auto, caption: \"Cap\", block: true)"
            ),
            "{b}"
        );
        assert!(
            b.contains("#md-image(\"/assets/0.svg\", alt: \"B\", width: auto, block: false);"),
            "the same file is registered once: {b}"
        );
        assert!(
            b.contains("#md-missing-image(\"fake.png\", alt: \"C\", block: false);"),
            "{b}"
        );
        assert!(b.contains("#md-missing-image(\"https://x.y/z.png\""), "{b}");
        assert!(b.contains("#md-missing-image(\"missing.png\""), "{b}");
        // Without a base folder relative paths are not resolved.
        assert!(typst("![A](pic.svg)").contains("md-missing-image"));
    }

    #[test]
    fn front_matter_is_not_printed() {
        let b = body("---\ntitle: T\nsecret: value\n---\nBody");
        assert_eq!(b, "Body");
    }

    #[test]
    fn hostile_text_stays_text() {
        let b = body("#panic() ]) [ $ @x <y // z <tag> dropped");
        // `<tag>` is inline HTML, which is dropped like in a sanitised preview.
        assert_eq!(b, "\\#panic() \\]) \\[ \\$ \\@x \\<y \\/\\/ z  dropped");
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(percent_decode("a%20b.png"), "a b.png");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn strips_html() {
        assert_eq!(
            strip_html("<div align=\"center\">\n  <b>Hi</b> &amp; bye<!-- x -->\n</div>"),
            "Hi & bye"
        );
        assert_eq!(strip_html("<!-- only a comment -->"), "");
    }

    #[test]
    fn finds_html_images() {
        let imgs =
            html_images(r#"<p><img width="20" src="a.png" alt='An "A"'> <IMG SRC=b.svg></p>"#);
        assert_eq!(imgs.len(), 2);
        assert_eq!(imgs[0].url, "a.png");
        assert_eq!(imgs[0].alt, "An \"A\"");
        assert_eq!(imgs[0].width_px, Some(20.0));
        assert_eq!(imgs[1].url, "b.svg");
        assert_eq!(imgs[1].width_px, None);
    }
}
