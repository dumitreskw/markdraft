//! Prints the generated Typst source for a Markdown file and, with
//! `--check`, the compiler diagnostics.
//!
//! ```text
//! cargo run -p export --example typst_source -- FILE.md [--theme T] [--check]
//! ```

use export::{ExportInput, ExportOptions};

fn main() {
    let mut args = std::env::args().skip(1);
    let file = std::path::PathBuf::from(args.next().expect("usage: typst_source FILE.md"));
    let mut opts = ExportOptions::default();
    let mut check = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--theme" => opts.theme = args.next().unwrap().parse().unwrap(),
            "--toc" => opts.toc = true,
            "--check" => check = true,
            other => panic!("unknown argument {other}"),
        }
    }
    let markdown = std::fs::read_to_string(&file).unwrap();
    let mut input = ExportInput::new(&markdown);
    input.base_dir = file.parent();
    println!("{}", export::markdown_to_typst(&input, &opts).unwrap());
    if check {
        match export::debug_compile_typeset(&input, &opts) {
            Ok(warnings) => warnings.iter().for_each(|w| eprintln!("warning: {w}")),
            Err(e) => eprintln!("{e}"),
        }
    }
}
