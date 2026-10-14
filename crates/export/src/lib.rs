//! PDF export.
//!
//! Pipeline: Markdown → comrak AST → Typst markup → PDF. The Typst compiler
//! runs in-process against an in-memory world (see `world.rs`) that only
//! contains the bundled fonts and templates plus the images the document
//! references, so exports never touch the network or system fonts and are
//! byte-for-byte reproducible across platforms.
//!
//! - [`markdown_to_pdf`] exports one document.
//! - [`combine_to_pdf`] exports several documents into one PDF (folder
//!   export) with an optional cover page and a shared table of contents.
//! - [`markdown_to_typst`] / [`combine_to_typst`] return the generated Typst
//!   source, for tests and debugging.

mod frontmatter;
mod markup;
mod math;
mod render;
mod world;

use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use typst::foundations::{Bytes, Smart};
use typst_layout::PagedDocument;

use crate::markup::{opt_string, string_literal};
use crate::render::{MathMode, RenderedDoc, Shared, render_doc};
use crate::world::{ExportWorld, theme_path};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown theme `{0}` (expected clean, academic or compact)")]
    UnknownTheme(String),
    #[error("unknown paper size `{0}` (expected A4 or Letter)")]
    UnknownPaper(String),
    /// [`combine_to_pdf`] was called without documents.
    #[error("there are no documents to export")]
    NoDocuments,
    /// Typst rejected the generated document. The message lists each error
    /// with its location in the generated source.
    #[error("typesetting failed:\n{0}")]
    Compile(String),
    /// Writing the PDF failed.
    #[error("writing the PDF failed: {0}")]
    Pdf(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Clean,
    Academic,
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Paper {
    #[default]
    A4,
    Letter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    pub theme: Theme,
    pub paper: Paper,
    pub toc: bool,
    /// Curly quotes, en/em dashes and ellipses.
    #[serde(default)]
    pub smart: bool,
}

impl FromStr for Theme {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        match s.to_ascii_lowercase().as_str() {
            "clean" => Ok(Self::Clean),
            "academic" => Ok(Self::Academic),
            "compact" => Ok(Self::Compact),
            _ => Err(Error::UnknownTheme(s.to_owned())),
        }
    }
}

impl fmt::Display for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Clean => "clean",
            Self::Academic => "academic",
            Self::Compact => "compact",
        })
    }
}

impl FromStr for Paper {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        match s.to_ascii_lowercase().as_str() {
            "a4" => Ok(Self::A4),
            "letter" => Ok(Self::Letter),
            _ => Err(Error::UnknownPaper(s.to_owned())),
        }
    }
}

impl fmt::Display for Paper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::A4 => "A4",
            Self::Letter => "Letter",
        })
    }
}

impl Paper {
    /// The Typst paper name.
    fn typst_name(self) -> &'static str {
        match self {
            Self::A4 => "a4",
            Self::Letter => "us-letter",
        }
    }
}

/// One Markdown document to export.
#[derive(Debug, Clone, Copy)]
pub struct ExportInput<'a> {
    pub markdown: &'a str,
    /// Folder that relative image paths resolve against. Without it, only
    /// absolute image paths are loaded.
    pub base_dir: Option<&'a Path>,
    /// Overrides the title taken from front matter / first H1.
    pub title: Option<&'a str>,
    /// Pre-rendered SVG for diagram fences (```mermaid, ```dot, ```graphviz),
    /// keyed by the 0-based index of the diagram fence in document order,
    /// counting all three kinds together. Fences without an entry are shown
    /// as code.
    pub diagrams: &'a HashMap<usize, String>,
}

static NO_DIAGRAMS: std::sync::LazyLock<HashMap<usize, String>> =
    std::sync::LazyLock::new(HashMap::new);

impl<'a> ExportInput<'a> {
    /// An input with no base folder, title override or diagrams.
    pub fn new(markdown: &'a str) -> Self {
        Self {
            markdown,
            base_dir: None,
            title: None,
            diagrams: &NO_DIAGRAMS,
        }
    }
}

/// Generated Typst source plus the virtual files it references.
struct Generated {
    source: String,
    assets: Vec<(String, Bytes)>,
    uses_math: bool,
}

/// How the documents are laid out.
enum Layout<'a> {
    Single,
    Combined { cover_title: Option<&'a str> },
}

fn generate(
    inputs: &[ExportInput],
    opts: &ExportOptions,
    layout: &Layout,
    math_mode: MathMode,
) -> Generated {
    let mut shared = Shared::new(math_mode);
    shared.smart = opts.smart;
    let docs: Vec<RenderedDoc> = inputs
        .iter()
        .enumerate()
        .map(|(i, input)| render_doc(&mut shared, input, i))
        .collect();

    let mut src = String::from("// Generated by MarkDraft.\n");
    src.push_str(&format!(
        "#import {}: *\n",
        string_literal(theme_path(opts.theme))
    ));
    if shared.uses_math {
        src.push_str("#import \"/markdraft/math.typ\": md-math\n");
    }

    match layout {
        Layout::Single => {
            let meta = &docs[0].meta;
            src.push_str(&format!(
                "#show: md-setup.with(paper: {}, title: {}, author: {})\n\n",
                string_literal(opts.paper.typst_name()),
                opt_string(meta.title.as_deref()),
                opt_string(meta.author.as_deref()),
            ));
            if meta.title.is_some() || meta.author.is_some() || meta.date.is_some() {
                src.push_str(&format!(
                    "#md-title(title: {}, subtitle: {}, author: {}, date: {})\n\n",
                    opt_string(meta.title.as_deref()),
                    opt_string(meta.subtitle.as_deref()),
                    opt_string(meta.author.as_deref()),
                    opt_string(meta.date.as_deref()),
                ));
            }
            if opts.toc {
                src.push_str("#md-toc()\n\n");
            }
            src.push_str(&docs[0].body);
            src.push('\n');
        }
        Layout::Combined { cover_title } => {
            src.push_str(&format!(
                "#show: md-setup.with(paper: {}, title: {}, author: none)\n\n",
                string_literal(opts.paper.typst_name()),
                opt_string(*cover_title),
            ));
            if let Some(title) = cover_title {
                src.push_str(&format!("#md-cover(title: {})\n\n", string_literal(title)));
            }
            if opts.toc {
                src.push_str("#md-toc(combined: true)\n\n");
            }
            for doc in &docs {
                match &doc.meta.title {
                    Some(title) => {
                        src.push_str(&format!(
                            "#md-chapter(title: {}, subtitle: {}, author: {}, date: {})\n\n",
                            string_literal(title),
                            opt_string(doc.meta.subtitle.as_deref()),
                            opt_string(doc.meta.author.as_deref()),
                            opt_string(doc.meta.date.as_deref()),
                        ));
                        // The chapter heading is level 1; the document's own
                        // headings nest below it.
                        src.push_str("#[\n#set heading(offset: 1)\n\n");
                        src.push_str(&doc.body);
                        src.push_str("\n]\n\n");
                    }
                    None => {
                        src.push_str("#md-chapter(title: none)\n\n");
                        src.push_str(&doc.body);
                        src.push_str("\n\n");
                    }
                }
            }
        }
    }

    Generated {
        source: src,
        assets: shared.assets,
        uses_math: shared.uses_math,
    }
}

/// Generates the Typst source for one document (for tests and debugging).
pub fn markdown_to_typst(input: &ExportInput, opts: &ExportOptions) -> Result<String, Error> {
    let generated = generate(
        std::slice::from_ref(input),
        opts,
        &Layout::Single,
        MathMode::Typeset,
    );
    Ok(generated.source)
}

/// Generates the Typst source for a combined export (for tests and debugging).
pub fn combine_to_typst(
    inputs: &[ExportInput],
    opts: &ExportOptions,
    cover_title: Option<&str>,
) -> Result<String, Error> {
    if inputs.is_empty() {
        return Err(Error::NoDocuments);
    }
    let layout = Layout::Combined { cover_title };
    Ok(generate(inputs, opts, &layout, MathMode::Typeset).source)
}

/// Exports one Markdown document to PDF bytes.
pub fn markdown_to_pdf(input: &ExportInput, opts: &ExportOptions) -> Result<Vec<u8>, Error> {
    let document = compile(std::slice::from_ref(input), opts, &Layout::Single)?;
    write_pdf(&document)
}

/// Exports several documents into one PDF (folder export): an optional cover
/// page, one table of contents (when `opts.toc` is set), and each document
/// starting on a new page under its title.
pub fn combine_to_pdf(
    inputs: &[ExportInput],
    opts: &ExportOptions,
    cover_title: Option<&str>,
) -> Result<Vec<u8>, Error> {
    if inputs.is_empty() {
        return Err(Error::NoDocuments);
    }
    let document = compile(inputs, opts, &Layout::Combined { cover_title })?;
    write_pdf(&document)
}

/// Lays out one document without writing a PDF. Hidden: used by tests and
/// tools that rasterize pages.
#[doc(hidden)]
pub fn layout_document(input: &ExportInput, opts: &ExportOptions) -> Result<PagedDocument, Error> {
    compile(std::slice::from_ref(input), opts, &Layout::Single)
}

/// Lays out a combined export without writing a PDF. Hidden: used by tests.
#[doc(hidden)]
pub fn layout_combined(
    inputs: &[ExportInput],
    opts: &ExportOptions,
    cover_title: Option<&str>,
) -> Result<PagedDocument, Error> {
    if inputs.is_empty() {
        return Err(Error::NoDocuments);
    }
    compile(inputs, opts, &Layout::Combined { cover_title })
}

/// Compiles one document with math typeset and no fallback, so tests can
/// assert that formulas really are typeset. Returns Typst's warnings.
/// Hidden: used by tests.
#[doc(hidden)]
pub fn debug_compile_typeset(
    input: &ExportInput,
    opts: &ExportOptions,
) -> Result<Vec<String>, Error> {
    let generated = generate(
        std::slice::from_ref(input),
        opts,
        &Layout::Single,
        MathMode::Typeset,
    );
    let (result, warnings) =
        ExportWorld::new(generated.source, generated.assets).compile_with_warnings();
    result.map(|_| warnings).map_err(Error::Compile)
}

/// Compiles hand-written Typst source in the export world. Hidden: used by
/// tests of error reporting.
#[doc(hidden)]
pub fn debug_compile_source(source: &str) -> Result<(), Error> {
    ExportWorld::new(source.to_owned(), Vec::new())
        .compile()
        .map(|_| ())
        .map_err(Error::Compile)
}

/// Family names of the bundled fonts. Hidden: used by tests.
#[doc(hidden)]
pub fn bundled_font_families() -> Vec<String> {
    world::font_families()
}

fn compile(
    inputs: &[ExportInput],
    opts: &ExportOptions,
    layout: &Layout,
) -> Result<PagedDocument, Error> {
    let generated = generate(inputs, opts, layout, MathMode::Typeset);
    let uses_math = generated.uses_math;
    match ExportWorld::new(generated.source, generated.assets).compile() {
        Ok(document) => Ok(document),
        // A formula that mitex converted but Typst rejects (unknown command,
        // wrong arity, ...) must not sink the whole export: retry with all
        // math shown as LaTeX source. If the retry fails too, report the
        // retry's errors, which are not about math.
        Err(_) if uses_math => {
            let fallback = generate(inputs, opts, layout, MathMode::Source);
            ExportWorld::new(fallback.source, fallback.assets)
                .compile()
                .map_err(Error::Compile)
        }
        Err(message) => Err(Error::Compile(message)),
    }
}

fn write_pdf(document: &PagedDocument) -> Result<Vec<u8>, Error> {
    let options = typst_pdf::PdfOptions {
        // A fixed identifier and no timestamp keep the output reproducible.
        ident: Smart::Custom("markdraft-export".to_owned()),
        creator: Smart::Custom(Some("MarkDraft".to_owned())),
        timestamp: None,
        ..Default::default()
    };
    typst_pdf::pdf(document, &options).map_err(|errors| {
        Error::Pdf(
            errors
                .iter()
                .map(|e| e.message.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_case_insensitively() {
        assert_eq!("Academic".parse::<Theme>().unwrap(), Theme::Academic);
        assert_eq!("letter".parse::<Paper>().unwrap(), Paper::Letter);
        assert!("fancy".parse::<Theme>().is_err());
        assert!("A5".parse::<Paper>().is_err());
    }

    #[test]
    fn display_round_trips() {
        for t in [Theme::Clean, Theme::Academic, Theme::Compact] {
            assert_eq!(t.to_string().parse::<Theme>().unwrap(), t);
        }
        for p in [Paper::A4, Paper::Letter] {
            assert_eq!(p.to_string().parse::<Paper>().unwrap(), p);
        }
    }

    #[test]
    fn markdown_integrations() {
        let md = "# Doc

[TOC]

## A

Term
: Details here.

![[pic.png|200]]

```dot
digraph { a -> b }
```

\"Quoted\" -- text...
";
        let mut diagrams = HashMap::new();
        diagrams.insert(
            0usize,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"/>".to_owned(),
        );
        let input = ExportInput {
            diagrams: &diagrams,
            ..ExportInput::new(md)
        };
        let opts = ExportOptions {
            smart: true,
            ..ExportOptions::default()
        };
        let typst = markdown_to_typst(&input, &opts).unwrap();
        assert!(typst.contains("#md-toc()"), "{typst}");
        assert!(
            typst.contains("strong[") && typst.contains("#pad(left: 1.5em)"),
            "{typst}"
        );
        assert!(
            typst.contains("#md-diagram("),
            "graphviz SVG embedded: {typst}"
        );
        assert!(typst.contains('“') && typst.contains('–'), "{typst}");
        // The embed is resolved like an image (missing file → placeholder chip).
        assert!(!typst.contains("![["), "{typst}");
        assert!(markdown_to_pdf(&input, &opts).unwrap().starts_with(b"%PDF"));
    }

    #[test]
    fn emoji_have_a_font() {
        let input = ExportInput::new(
            "Shipped :rocket: 🎉 ✅
",
        );
        let typst = markdown_to_typst(&input, &ExportOptions::default()).unwrap();
        assert!(typst.contains('🚀'), "{typst}");
        let warnings = debug_compile_typeset(&input, &ExportOptions::default()).unwrap();
        assert!(
            warnings.iter().all(|w| !w.contains("glyph")),
            "missing glyphs: {warnings:?}"
        );
    }

    #[test]
    fn wiki_links_and_highlights_print_as_text() {
        let typst = markdown_to_typst(
            &ExportInput::new(
                "See [[Advanced]] and [[Guide|the guide]], ==important==.
",
            ),
            &ExportOptions::default(),
        )
        .unwrap();
        assert!(
            !typst.contains("[[") && typst.contains("Advanced") && typst.contains("the guide"),
            "{typst}"
        );
        assert!(typst.contains("#highlight[important]"), "{typst}");
    }

    #[test]
    fn options_json_shape() {
        let json = serde_json::to_string(&ExportOptions::default()).unwrap();
        assert_eq!(
            json,
            r#"{"theme":"clean","paper":"A4","toc":false,"smart":false}"#
        );
        // Older callers that omit `smart` still deserialise.
        let old: ExportOptions =
            serde_json::from_str(r#"{"theme":"clean","paper":"A4","toc":true}"#).unwrap();
        assert!(old.toc && !old.smart);
    }
}
