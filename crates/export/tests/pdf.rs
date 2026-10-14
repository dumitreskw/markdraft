//! End-to-end tests: fixtures compile to valid, reproducible PDFs.

mod common;

use std::collections::HashMap;

use export::{ExportInput, ExportOptions, Paper, Theme};

const THEMES: [Theme; 3] = [Theme::Clean, Theme::Academic, Theme::Compact];

fn opts(theme: Theme) -> ExportOptions {
    ExportOptions {
        theme,
        paper: Paper::A4,
        toc: true,
    }
}

#[test]
fn every_fixture_compiles_in_every_theme() {
    let dir = common::fixtures_dir();
    for (name, markdown) in common::fixtures() {
        let mut input = ExportInput::new(&markdown);
        input.base_dir = Some(&dir);
        for theme in THEMES {
            let pdf = export::markdown_to_pdf(&input, &opts(theme))
                .unwrap_or_else(|e| panic!("{name} ({theme}): {e}"));
            assert!(pdf.starts_with(b"%PDF-"), "{name} ({theme})");
            assert!(pdf.len() > 1000, "{name} ({theme})");
        }
    }
}

#[test]
fn page_counts() {
    let dir = common::fixtures_dir();
    let pages = |name: &str, theme: Theme, toc: bool| {
        let markdown = common::fixture(name);
        let mut input = ExportInput::new(&markdown);
        input.base_dir = Some(&dir);
        let opts = ExportOptions {
            theme,
            paper: Paper::A4,
            toc,
        };
        export::layout_document(&input, &opts)
            .unwrap()
            .pages()
            .len()
    };
    for name in [
        "basics",
        "lists",
        "tables",
        "quotes_alerts",
        "math",
        "footnotes",
    ] {
        for theme in THEMES {
            assert_eq!(pages(name, theme, false), 1, "{name} ({theme})");
        }
    }
    // The long document spans several pages; compact needs fewer than clean.
    let clean = pages("long", Theme::Clean, true);
    let compact = pages("long", Theme::Compact, true);
    assert!((4..=9).contains(&clean), "clean: {clean} pages");
    assert!(compact < clean, "compact {compact} vs clean {clean}");
}

#[test]
fn letter_and_a4_page_sizes() {
    let input = ExportInput::new("Hello");
    for (paper, width_pt) in [(Paper::A4, 595.28), (Paper::Letter, 612.0)] {
        let opts = ExportOptions {
            paper,
            ..ExportOptions::default()
        };
        let doc = export::layout_document(&input, &opts).unwrap();
        let width = doc.pages()[0].frame.width().to_pt();
        assert!((width - width_pt).abs() < 0.5, "{paper}: {width}");
    }
}

#[test]
fn output_is_reproducible() {
    let markdown = common::fixture("long");
    let dir = common::fixtures_dir();
    let mut input = ExportInput::new(&markdown);
    input.base_dir = Some(&dir);
    let first = export::markdown_to_pdf(&input, &opts(Theme::Clean)).unwrap();
    let second = export::markdown_to_pdf(&input, &opts(Theme::Clean)).unwrap();
    assert!(first == second, "two exports of the same input differ");

    let images = common::fixture("images");
    let mut input = ExportInput::new(&images);
    input.base_dir = Some(&dir);
    let first = export::markdown_to_pdf(&input, &opts(Theme::Academic)).unwrap();
    let second = export::markdown_to_pdf(&input, &opts(Theme::Academic)).unwrap();
    assert!(first == second, "two exports with images differ");
}

#[test]
fn pdf_has_no_creation_date() {
    let pdf = export::markdown_to_pdf(&ExportInput::new("# T\n\nx"), &opts(Theme::Clean)).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(!text.contains("/CreationDate"));
    assert!(!text.contains("/ModDate"));
}

#[test]
fn math_is_typeset_without_fallback() {
    for name in ["math", "long"] {
        let markdown = common::fixture(name);
        export::debug_compile_typeset(&ExportInput::new(&markdown), &ExportOptions::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn fixtures_compile_without_warnings() {
    let dir = common::fixtures_dir();
    for (name, markdown) in common::fixtures() {
        let mut input = ExportInput::new(&markdown);
        input.base_dir = Some(&dir);
        for theme in THEMES {
            let warnings = export::debug_compile_typeset(&input, &opts(theme)).unwrap();
            // Glyphs missing from the bundled fonts are expected there.
            if name != "unicode" {
                assert!(warnings.is_empty(), "{name} ({theme}): {warnings:#?}");
            }
        }
    }
}

#[test]
fn math_that_typst_rejects_falls_back_to_source() {
    // mitex converts this to `frac(a )`, which Typst rejects (missing denominator).
    let input = ExportInput::new("Bad: $\\frac{a}$ and good: $x^2$.");
    assert!(export::debug_compile_typeset(&input, &ExportOptions::default()).is_err());
    let pdf = export::markdown_to_pdf(&input, &ExportOptions::default()).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
}

#[test]
fn mermaid_diagrams_are_embedded() {
    let markdown = common::fixture("code");
    let svg = std::fs::read_to_string(common::fixtures_dir().join("images/diagram.svg")).unwrap();
    let diagrams = HashMap::from([(0, svg)]);
    let mut input = ExportInput::new(&markdown);
    input.diagrams = &diagrams;
    let with = export::markdown_to_pdf(&input, &ExportOptions::default()).unwrap();
    let without =
        export::markdown_to_pdf(&ExportInput::new(&markdown), &ExportOptions::default()).unwrap();
    assert_ne!(with, without);
}

#[test]
fn combined_export() {
    let a = common::fixture("basics");
    let b = common::fixture("lists");
    let c = common::fixture("math");
    let inputs = [
        ExportInput::new(&a),
        ExportInput::new(&b),
        ExportInput::new(&c),
    ];
    for theme in THEMES {
        let opts = opts(theme);
        let doc = export::layout_combined(&inputs, &opts, Some("Notes")).unwrap();
        // Cover, contents, then one page per (single-page) document.
        assert_eq!(doc.pages().len(), 5, "{theme}");
        let pdf = export::combine_to_pdf(&inputs, &opts, Some("Notes")).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
    }
    let no_cover = ExportOptions::default();
    let doc = export::layout_combined(&inputs, &no_cover, None).unwrap();
    assert_eq!(doc.pages().len(), 3);
    assert!(matches!(
        export::combine_to_pdf(&[], &no_cover, None),
        Err(export::Error::NoDocuments)
    ));
}

#[test]
fn compile_errors_are_readable() {
    // User text is always escaped, so compile hand-written Typst to see how
    // a real compiler error is reported.
    let err = export::debug_compile_source("Hello\n\n#foo(1)\n").unwrap_err();
    assert_eq!(
        err.to_string(),
        "typesetting failed:\n/main.typ:3: `#foo(1)`: unknown variable: foo"
    );
}

#[test]
fn first_page_renders() {
    let markdown = common::fixture("long");
    let dir = common::fixtures_dir();
    let mut input = ExportInput::new(&markdown);
    input.base_dir = Some(&dir);
    let doc = export::layout_document(&input, &opts(Theme::Clean)).unwrap();
    let options = typst_render::RenderOptions {
        pixel_per_pt: 1.0.into(),
        render_bleed: false,
    };
    let pixmap = typst_render::render(&doc.pages()[0], &options);
    let png = pixmap.encode_png().unwrap();
    assert!(png.starts_with(b"\x89PNG"));
    // The page is not blank: some pixels are dark (text).
    let dark = pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 100 && p.green() < 100 && p.blue() < 100)
        .count();
    assert!(dark > 1000, "only {dark} dark pixels");
}

#[test]
fn twenty_pages_export_quickly() {
    // Build a ~20 page document from the long fixture. The bound is generous
    // because tests usually run unoptimised; see the report for release
    // timings.
    let long = common::fixture("long");
    let body = long
        .split_once("\n---\n\n")
        .map_or(long.as_str(), |(_, b)| b);
    let markdown = format!("# Twenty pages\n\n{}", body.repeat(4));
    let input = ExportInput::new(&markdown);
    let start = std::time::Instant::now();
    let doc = export::layout_document(&input, &ExportOptions::default()).unwrap();
    let pages = doc.pages().len();
    assert!(pages >= 16, "{pages} pages");
    let elapsed = start.elapsed();
    let limit = if cfg!(debug_assertions) { 30.0 } else { 2.0 };
    assert!(
        elapsed.as_secs_f64() < limit,
        "{pages} pages took {elapsed:?}"
    );
}
