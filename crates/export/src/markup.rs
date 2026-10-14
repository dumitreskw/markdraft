//! Low-level helpers for emitting Typst source safely.
//!
//! Everything user-controlled that ends up in the generated Typst file goes
//! through one of two functions:
//!
//! - [`escape_text`] for text placed in *markup* mode (paragraphs, headings,
//!   table cells, content blocks `[...]`);
//! - [`string_literal`] for text placed in *code* mode as a string literal
//!   (`raw("...")`, URLs, image paths, LaTeX sources, titles).
//!
//! Markup escaping is deliberately conservative: a backslash in front of any
//! character is a valid Typst escape, so escaping one character too many is
//! harmless while escaping one too few can change the document structure.

/// Escapes `text` so that Typst markup renders it literally.
///
/// The result never contains a newline: line breaks inside `text` become
/// plain spaces (callers emit explicit breaks themselves).
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let next = chars.get(i + 1).copied();
        match c {
            // Always special (or special in some position) in markup:
            // escapes, code, strong, emphasis, math, references, labels,
            // content blocks, raw, non-breaking space, smart quotes, headings,
            // list and enum markers, shorthands (`--`, `-?`, `-1`).
            '\\' | '#' | '*' | '_' | '$' | '@' | '<' | '>' | '[' | ']' | '`' | '~' | '\'' | '"'
            | '=' | '-' | '+' => {
                out.push('\\');
                out.push(c);
            }
            // `//` and `/*` start comments; `/ ` starts a term list item.
            '/' if matches!(next, None | Some('/' | '*'))
                || next.is_some_and(char::is_whitespace) =>
            {
                out.push_str("\\/");
            }
            // `...` is the ellipsis shorthand; `1. ` is an enum marker.
            '.' if next == Some('.')
                || (i > 0
                    && chars[i - 1].is_ascii_digit()
                    && next.is_none_or(char::is_whitespace)) =>
            {
                out.push_str("\\.");
            }
            // `http://` and `https://` become links automatically.
            ':' if chars.get(i + 1) == Some(&'/') && chars.get(i + 2) == Some(&'/') => {
                out.push_str("\\:");
            }
            '\n' | '\r' | '\t' => out.push(' '),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Returns `text` as a Typst string literal, including the quotes.
pub fn string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                out.push_str(&format!("\\u{{{:x}}}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Formats an optional string as a Typst value: a string literal or `none`.
pub fn opt_string(text: Option<&str>) -> String {
    text.map_or_else(|| "none".to_owned(), string_literal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::syntax::{SyntaxKind, SyntaxNode, parse};

    /// Parses `src` as Typst markup and returns all node kinds that are not
    /// plain text, escapes, spaces or the enclosing markup.
    fn structural_kinds(src: &str) -> Vec<SyntaxKind> {
        fn walk(node: &SyntaxNode, out: &mut Vec<SyntaxKind>) {
            match node.kind() {
                SyntaxKind::Markup | SyntaxKind::Text | SyntaxKind::Escape | SyntaxKind::Space => {}
                other => out.push(other),
            }
            for child in node.children() {
                walk(child, out);
            }
        }
        let root = parse(src);
        let mut kinds = Vec::new();
        walk(&root, &mut kinds);
        kinds
    }

    /// Re-reads escaped markup the way Typst does and returns the text.
    fn unescape_via_typst(src: &str) -> String {
        let root = parse(src);
        let mut out = String::new();
        fn walk(node: &SyntaxNode, out: &mut String) {
            match node.kind() {
                SyntaxKind::Text | SyntaxKind::Space => out.push_str(node.leaf_text()),
                SyntaxKind::Escape => {
                    let t = node.leaf_text();
                    out.push_str(&t[1..]);
                }
                _ => {
                    for child in node.children() {
                        walk(child, out);
                    }
                }
            }
        }
        walk(&root, &mut out);
        out
    }

    fn assert_literal(input: &str) {
        let escaped = escape_text(input);
        let kinds = structural_kinds(&escaped);
        assert!(
            kinds.is_empty(),
            "{input:?} escaped to {escaped:?} still has structure: {kinds:?}"
        );
        assert_eq!(
            unescape_via_typst(&escaped),
            input,
            "round trip of {escaped:?}"
        );
    }

    #[test]
    fn plain_text_is_untouched() {
        assert_eq!(escape_text("Hello, world"), "Hello, world");
        assert_eq!(escape_text("3.14 and v1.2.3"), "3.14 and v1.2.3");
        assert_eq!(escape_text("Ünïcödé — 日本語 🙂"), "Ünïcödé — 日本語 🙂");
    }

    #[test]
    fn markup_specials_are_escaped() {
        assert_eq!(escape_text("#set"), "\\#set");
        assert_eq!(escape_text("a*b*c"), "a\\*b\\*c");
        assert_eq!(escape_text("snake_case"), "snake\\_case");
        assert_eq!(escape_text("$5"), "\\$5");
        assert_eq!(escape_text("@user"), "\\@user");
        assert_eq!(escape_text("<label>"), "\\<label\\>");
        assert_eq!(escape_text("`code`"), "\\`code\\`");
        assert_eq!(escape_text("C:\\path\\"), "C:\\\\path\\\\");
        assert_eq!(escape_text("[x]"), "\\[x\\]");
        assert_eq!(escape_text("a ~ b"), "a \\~ b");
        assert_eq!(escape_text("it's \"quoted\""), "it\\'s \\\"quoted\\\"");
    }

    #[test]
    fn line_start_markers_are_escaped() {
        assert_eq!(escape_text("= Heading"), "\\= Heading");
        assert_eq!(escape_text("- item"), "\\- item");
        assert_eq!(escape_text("+ item"), "\\+ item");
        assert_eq!(escape_text("1. item"), "1\\. item");
        assert_eq!(escape_text("/ term: x"), "\\/ term: x");
    }

    #[test]
    fn comments_and_shorthands_are_escaped() {
        assert_eq!(escape_text("a // b"), "a \\/\\/ b");
        assert_eq!(escape_text("a /* b */"), "a \\/\\* b \\*\\/");
        assert_eq!(escape_text("wait..."), "wait\\.\\..");
        assert_eq!(escape_text("a -- b --- c"), "a \\-\\- b \\-\\-\\- c");
        assert_eq!(escape_text("a/b"), "a/b");
    }

    #[test]
    fn urls_do_not_become_links() {
        assert_eq!(escape_text("http://x.y"), "http\\:\\//x.y");
        assert_eq!(escape_text("see https://a.b/c"), "see https\\:\\//a.b/c");
    }

    #[test]
    fn newlines_and_controls() {
        assert_eq!(escape_text("a\nb\tc\u{7}"), "a b c");
    }

    #[test]
    fn escaped_text_parses_as_plain_text() {
        for input in [
            "#let x = 1",
            "*bold* and _emph_",
            "$x^2$",
            "@ref and <label>",
            "`raw` ```block```",
            "back\\slash\\",
            "[content] ]stray[",
            "// comment",
            "/* block */",
            "= heading",
            "== heading",
            "- list",
            "+ enum",
            "12. enum",
            "1) paren",
            "/ term: description",
            "a ~ b",
            "'single' \"double\"",
            "http://example.com https://example.com",
            "a -- b --- c -? d -1",
            "wait... ..",
            "\\",
            "#{panic()}",
            "#(1+1)",
            "x_1 y^2",
            "<a@b.c>",
            "50% off & more | pipes ^ carets",
        ] {
            assert_literal(input);
        }
    }

    #[test]
    fn escaped_text_is_literal_inside_content_blocks() {
        // Escaped text must not terminate an enclosing `[...]` block.
        let escaped = escape_text("a] #panic() [b");
        let src = format!("#strong[{escaped}]");
        let root = parse(&src);
        assert!(root.errors_and_warnings().0.is_empty(), "{src}");
    }

    #[test]
    fn string_literals() {
        assert_eq!(string_literal("plain"), "\"plain\"");
        assert_eq!(string_literal("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(string_literal("l1\nl2\r\tx"), "\"l1\\nl2\\r\\tx\"");
        assert_eq!(string_literal("\u{1}"), "\"\\u{1}\"");
        assert_eq!(string_literal("$#[]*_"), "\"$#[]*_\"");
        assert_eq!(opt_string(None), "none");
        assert_eq!(opt_string(Some("x")), "\"x\"");
    }

    #[test]
    fn string_literals_parse() {
        for input in ["\"", "\\", "\\\"", "a\nb", "#}{)(", "\u{0}\u{1f}"] {
            let src = format!("#{}", string_literal(input));
            let root = parse(&src);
            assert!(root.errors_and_warnings().0.is_empty(), "{src:?}");
        }
    }
}
