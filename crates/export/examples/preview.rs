//! Exports Markdown files and rasterizes the pages, for visual QA of themes.
//!
//! ```text
//! cargo run -p export --example preview -- OUT_DIR [--theme T] [--paper P] [--toc] [--combine] FILE.md...
//! ```
//!
//! Writes `<stem>-<theme>.pdf` and `<stem>-<theme>-<page>.png` per input (or
//! `combined-<theme>.*` with `--combine`) and prints timings.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use export::{ExportInput, ExportOptions};

fn main() {
    let mut args = std::env::args().skip(1);
    let out_dir = PathBuf::from(
        args.next()
            .expect("usage: preview OUT_DIR [options] FILE.md..."),
    );
    let mut opts = ExportOptions::default();
    let mut combine = false;
    let mut files = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--theme" => opts.theme = args.next().unwrap().parse().unwrap(),
            "--paper" => opts.paper = args.next().unwrap().parse().unwrap(),
            "--toc" => opts.toc = true,
            "--combine" => combine = true,
            _ => files.push(PathBuf::from(arg)),
        }
    }
    std::fs::create_dir_all(&out_dir).unwrap();
    let diagrams = HashMap::new();
    let sources: Vec<(PathBuf, String)> = files
        .iter()
        .map(|f| (f.clone(), std::fs::read_to_string(f).unwrap()))
        .collect();
    let inputs: Vec<ExportInput> = sources
        .iter()
        .map(|(path, md)| ExportInput {
            markdown: md,
            base_dir: path.parent(),
            title: None,
            diagrams: &diagrams,
        })
        .collect();

    if combine {
        let start = Instant::now();
        let doc = export::layout_combined(&inputs, &opts, Some("Combined Export")).unwrap();
        let pdf = export::combine_to_pdf(&inputs, &opts, Some("Combined Export")).unwrap();
        println!(
            "combined: {} pages, {:?} (two passes)",
            doc.pages().len(),
            start.elapsed()
        );
        write_outputs(&out_dir, &format!("combined-{}", opts.theme), &pdf, &doc);
        return;
    }
    for (input, (path, _)) in inputs.iter().zip(&sources) {
        let start = Instant::now();
        let pdf = match export::markdown_to_pdf(input, &opts) {
            Ok(pdf) => pdf,
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                continue;
            }
        };
        let elapsed = start.elapsed();
        let doc = export::layout_document(input, &opts).unwrap();
        println!(
            "{}: {} pages, {} bytes, {:?}",
            path.display(),
            doc.pages().len(),
            pdf.len(),
            elapsed
        );
        let stem = path.file_stem().unwrap().to_string_lossy();
        write_outputs(&out_dir, &format!("{stem}-{}", opts.theme), &pdf, &doc);
    }
}

fn write_outputs(dir: &Path, name: &str, pdf: &[u8], doc: &typst_layout::PagedDocument) {
    std::fs::write(dir.join(format!("{name}.pdf")), pdf).unwrap();
    let render = typst_render::RenderOptions {
        pixel_per_pt: std::env::var("PREVIEW_SCALE")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.4)
            .into(),
        render_bleed: false,
    };
    for (i, page) in doc.pages().iter().enumerate() {
        let pixmap = typst_render::render(page, &render);
        pixmap
            .save_png(dir.join(format!("{name}-{}.png", i + 1)))
            .unwrap();
    }
}
