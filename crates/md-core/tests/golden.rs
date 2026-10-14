//! Golden-file tests: every fixture's HTML is compared against a snapshot.
//! Review changes with `cargo insta review`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use md_core::{RenderContext, render};

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    files.sort();
    files
}

#[test]
fn rendered_html_matches_snapshots() {
    let ctx = RenderContext {
        base_dir: Some(PathBuf::from("/fixtures")),
        // Forward slashes keep snapshots identical on every OS.
        asset_url: Some(Arc::new(|p: &Path| {
            format!(
                "mdasset://localhost/{}",
                p.to_string_lossy().replace('\\', "/")
            )
        })),
    };
    let fixtures = fixtures();
    assert!(!fixtures.is_empty());
    for path in fixtures {
        let markdown = std::fs::read_to_string(&path).unwrap();
        let doc = render(&markdown, &ctx);
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        insta::assert_snapshot!(format!("{name}_html"), doc.html);
        insta::assert_debug_snapshot!(
            format!("{name}_meta"),
            (
                &doc.title,
                &doc.headings,
                doc.word_count,
                doc.has_math,
                doc.has_mermaid
            )
        );
    }
}
