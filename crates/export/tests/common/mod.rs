//! Helpers shared by the integration tests.

use std::path::{Path, PathBuf};

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// All fixture documents as `(stem, markdown)`, sorted by name.
pub fn fixtures() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(fixtures_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .map(|p| {
            let stem = p.file_stem().unwrap().to_string_lossy().into_owned();
            // Normalise line endings in case of a CRLF checkout.
            let md = std::fs::read_to_string(&p).unwrap().replace("\r\n", "\n");
            (stem, md)
        })
        .collect();
    out.sort();
    out
}

pub fn fixture(name: &str) -> String {
    std::fs::read_to_string(fixtures_dir().join(format!("{name}.md")))
        .unwrap()
        .replace("\r\n", "\n")
}
