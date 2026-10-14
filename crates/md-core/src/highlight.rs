//! Code-block syntax highlighting with syntect, emitted as CSS classes so
//! the UI can switch light and dark palettes without re-rendering.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{self, Write};
use std::sync::OnceLock;

use comrak::adapters::SyntaxHighlighterAdapter;
use syntect::highlighting::ThemeSet;
use syntect::html::{ClassStyle, ClassedHTMLGenerator, css_for_theme_with_class_style};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Class prefix for highlighted tokens, e.g. `hl-keyword`.
const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "hl-" };

/// Code blocks larger than this are shown unhighlighted to keep rendering fast.
const MAX_HIGHLIGHT_BYTES: usize = 64 * 1024;

/// Fence languages rendered by the UI itself rather than highlighted.
const PASSTHROUGH_LANGS: &[&str] = &["mermaid", "dot", "graphviz", "math"];

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Loads syntax definitions ahead of the first render, off the UI path.
pub fn warm_up() {
    let _ = syntaxes();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightTheme {
    Light,
    Dark,
}

/// CSS for the `hl-*` classes in the given palette.
pub fn highlight_css(theme: HighlightTheme) -> String {
    let themes = ThemeSet::load_defaults();
    let name = match theme {
        HighlightTheme::Light => "InspiredGitHub",
        HighlightTheme::Dark => "base16-ocean.dark",
    };
    css_for_theme_with_class_style(&themes.themes[name], CLASS_STYLE)
        .expect("bundled syntect themes produce valid CSS")
}

pub(crate) struct Highlighter;

impl SyntaxHighlighterAdapter for Highlighter {
    fn write_highlighted(
        &self,
        output: &mut dyn Write,
        lang: Option<&str>,
        code: &str,
    ) -> fmt::Result {
        let lang = lang.unwrap_or("").trim();
        if lang.is_empty() || PASSTHROUGH_LANGS.contains(&lang) || code.len() > MAX_HIGHLIGHT_BYTES
        {
            return escape(output, code);
        }
        let set = syntaxes();
        let Some(syntax) = set
            .find_syntax_by_token(lang)
            .or_else(|| set.find_syntax_by_extension(lang))
        else {
            return escape(output, code);
        };
        let mut generator = ClassedHTMLGenerator::new_with_class_style(syntax, set, CLASS_STYLE);
        for line in LinesWithEndings::from(code) {
            if generator
                .parse_html_for_line_which_includes_newline(line)
                .is_err()
            {
                return escape(output, code);
            }
        }
        output.write_str(&generator.finalize())
    }

    fn write_pre_tag(
        &self,
        output: &mut dyn Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        write_tag(output, "pre", attributes)
    }

    fn write_code_tag(
        &self,
        output: &mut dyn Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        write_tag(output, "code", attributes)
    }
}

fn write_tag(
    output: &mut dyn Write,
    tag: &str,
    attributes: HashMap<&'static str, Cow<'_, str>>,
) -> fmt::Result {
    write!(output, "<{tag}")?;
    // Sorted so output is deterministic (HashMap order is not).
    let mut attrs: Vec<_> = attributes.into_iter().collect();
    attrs.sort_by_key(|(k, _)| *k);
    for (key, value) in attrs {
        write!(output, " {key}=\"")?;
        escape(output, &value)?;
        output.write_char('"')?;
    }
    output.write_char('>')
}

pub(crate) fn escape(output: &mut dyn Write, text: &str) -> fmt::Result {
    comrak::html::escape(output, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn highlight(lang: Option<&str>, code: &str) -> String {
        let mut out = String::new();
        Highlighter.write_highlighted(&mut out, lang, code).unwrap();
        out
    }

    #[test]
    fn known_language_gets_classes() {
        let html = highlight(Some("rust"), "fn main() {}\n");
        assert!(html.contains("hl-"), "{html}");
        assert!(html.contains("main"));
    }

    #[test]
    fn unknown_and_mermaid_are_escaped_only() {
        assert_eq!(highlight(Some("nope"), "<a>"), "&lt;a&gt;");
        assert_eq!(highlight(Some("mermaid"), "a-->b"), "a--&gt;b");
        assert_eq!(highlight(None, "x & y"), "x &amp; y");
    }

    #[test]
    fn css_is_generated_for_both_themes() {
        assert!(highlight_css(HighlightTheme::Light).contains(".hl-"));
        assert!(highlight_css(HighlightTheme::Dark).contains(".hl-"));
    }
}
