//! Snapshot tests of the generated Typst markup for the fixture corpus.

mod common;

use std::collections::HashMap;

use export::{ExportInput, ExportOptions, Paper, Theme};

#[test]
fn typst_markup_of_fixtures() {
    let dir = common::fixtures_dir();
    for (name, markdown) in common::fixtures() {
        let mut input = ExportInput::new(&markdown);
        input.base_dir = Some(&dir);
        let src = export::markdown_to_typst(&input, &ExportOptions::default()).unwrap();
        insta::assert_snapshot!(format!("typst__{name}"), src);
    }
}

#[test]
fn typst_markup_with_options() {
    let markdown = common::fixture("frontmatter");
    let opts = ExportOptions {
        theme: Theme::Academic,
        paper: Paper::Letter,
        toc: true,
        smart: false,
    };
    let src = export::markdown_to_typst(&ExportInput::new(&markdown), &opts).unwrap();
    insta::assert_snapshot!(src);
}

#[test]
fn typst_markup_with_diagram() {
    let markdown = common::fixture("code");
    let svg = std::fs::read_to_string(common::fixtures_dir().join("images/diagram.svg")).unwrap();
    let diagrams = HashMap::from([(0, svg)]);
    let mut input = ExportInput::new(&markdown);
    input.diagrams = &diagrams;
    let src = export::markdown_to_typst(&input, &ExportOptions::default()).unwrap();
    insta::assert_snapshot!(src);
}

#[test]
fn typst_markup_combined() {
    let a = common::fixture("basics");
    let b = common::fixture("frontmatter");
    let c = "No title here, just text with a footnote.[^1]\n\n[^1]: Note.\n";
    let d = "Another untitled document with a footnote.[^1]\n\n[^1]: Same name, other document.\n";
    let mut titled = ExportInput::new(c);
    titled.title = Some("notes.md");
    let inputs = [
        ExportInput::new(&a),
        ExportInput::new(&b),
        titled,
        ExportInput::new(d),
    ];
    let opts = ExportOptions {
        toc: true,
        smart: false,
        ..ExportOptions::default()
    };
    let src = export::combine_to_typst(&inputs, &opts, Some("Project Notes")).unwrap();
    insta::assert_snapshot!(src);
}
