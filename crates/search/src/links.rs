//! Backlinks: which documents link to a given document.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

/// Longest excerpt returned per link, in characters.
const MAX_EXCERPT_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backlink {
    /// The document containing the link.
    pub path: PathBuf,
    /// 1-based line of the link.
    pub line: u32,
    /// The line, trimmed.
    pub text: String,
}

/// Finds links to `target` in `files`: Markdown links whose relative target
/// resolves to it (with or without the `.md` extension) and wiki links
/// (`[[name]]`, `[[name|label]]`) naming its file stem or `title`.
pub fn find_backlinks(target: &Path, title: Option<&str>, files: &[PathBuf]) -> Vec<Backlink> {
    let target_key = path_key(target);
    let stem = target
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let title = title.map(str::to_lowercase);
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8);
    let chunk = files.len().div_ceil(threads).max(1);

    let mut found: Vec<(usize, Backlink)> = std::thread::scope(|s| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .enumerate()
            .map(|(ci, part)| {
                let (target_key, stem, title) = (&target_key, &stem, &title);
                s.spawn(move || {
                    let mut out = Vec::new();
                    for (i, file) in part.iter().enumerate() {
                        if path_key(file) == *target_key {
                            continue;
                        }
                        let Ok(text) = std::fs::read_to_string(file) else {
                            continue;
                        };
                        let dir = file.parent().unwrap_or(Path::new(""));
                        for (n, line) in text.lines().enumerate() {
                            let hit = markdown_targets(line)
                                .any(|href| resolves_to(dir, href, target_key))
                                || wiki_targets(line).any(|name| {
                                    let name = name.trim().to_lowercase();
                                    name == *stem || title.as_deref() == Some(name.as_str())
                                });
                            if hit {
                                out.push((
                                    ci * chunk + i,
                                    Backlink {
                                        path: file.clone(),
                                        line: u32::try_from(n + 1).unwrap_or(u32::MAX),
                                        text: line.trim().chars().take(MAX_EXCERPT_CHARS).collect(),
                                    },
                                ));
                            }
                        }
                    }
                    out
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });
    found.sort_by_key(|(i, b)| (*i, b.line));
    found.into_iter().map(|(_, b)| b).collect()
}

/// Targets of inline Markdown links on a line: the part inside `](…)`.
fn markdown_targets(line: &str) -> impl Iterator<Item = &str> {
    line.match_indices("](").filter_map(move |(i, _)| {
        let rest = &line[i + 2..];
        let end = rest.find(')')?;
        let inner = &rest[..end];
        // `[text](<target with spaces>)`
        if let Some(bracketed) = inner.strip_prefix('<') {
            return bracketed.split('>').next();
        }
        // `[text](target "title")`: the target ends at the first space.
        inner.split_whitespace().next()
    })
}

/// Names inside `[[…]]` on a line, without any `|label`.
fn wiki_targets(line: &str) -> impl Iterator<Item = &str> {
    line.match_indices("[[").filter_map(move |(i, _)| {
        let rest = &line[i + 2..];
        let end = rest.find("]]")?;
        let inner = &rest[..end];
        Some(
            inner
                .split('|')
                .next()
                .unwrap_or(inner)
                .split('#')
                .next()
                .unwrap_or(inner),
        )
    })
}

fn resolves_to(dir: &Path, href: &str, target_key: &str) -> bool {
    if href.is_empty()
        || href.starts_with('#')
        || href.contains("://")
        || href.starts_with("mailto:")
    {
        return false;
    }
    let path_part = href.split(['#', '?']).next().unwrap_or(href);
    let decoded = percent_decode(path_part);
    let joined = normalize(&dir.join(&decoded));
    if path_key(&joined) == target_key {
        return true;
    }
    // Links often omit the extension.
    Path::new(&decoded).extension().is_none()
        && path_key(&joined.with_extension("md")) == target_key
}

/// Resolves `.` and `..` without touching the file system.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// A comparison key: `/` separators, lower case on case-insensitive systems.
fn path_key(path: &Path) -> String {
    let s = normalize(path).to_string_lossy().replace('\\', "/");
    if cfg!(any(windows, target_os = "macos")) {
        s.to_lowercase()
    } else {
        s
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            let hex = |b: u8| (b as char).to_digit(16).unwrap_or(0) as u8;
            out.push(hex(bytes[i + 1]) * 16 + hex(bytes[i + 2]));
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }

    #[test]
    fn finds_relative_and_wiki_links() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let target = write(d, "guide/Getting Started.md", "# Getting started\n");
        let a = write(
            d,
            "README.md",
            "See [the guide](guide/Getting%20Started.md#install).\nNothing here.\n",
        );
        let b = write(
            d,
            "notes/x.md",
            "Up [one](../guide/Getting Started) and [[getting started|Start]].\n",
        );
        let c = write(
            d,
            "notes/y.md",
            "Title link: [[Getting started]]. External [x](https://a.b/guide/Getting%20Started.md)\n",
        );
        let none = write(d, "z.md", "[other](guide/other.md) [[Something else]]\n");
        let links = find_backlinks(
            &target,
            Some("Getting started"),
            &[a.clone(), b.clone(), c.clone(), none, target.clone()],
        );
        let hits: Vec<_> = links.iter().map(|l| (l.path.clone(), l.line)).collect();
        assert_eq!(hits, vec![(a, 1), (b, 1), (c, 1)]);
        assert!(links[0].text.starts_with("See [the guide]"));
    }

    #[test]
    fn link_parsing() {
        let t: Vec<_> =
            markdown_targets(r#"[a](x.md "Title") [b](<y z.md>) ![i](img.png)"#).collect();
        assert_eq!(t, vec!["x.md", "y z.md", "img.png"]);
        let w: Vec<_> = wiki_targets("[[A]] and [[B|label]] and [[C#heading]]").collect();
        assert_eq!(w, vec!["A", "B", "C"]);
        assert_eq!(percent_decode("My%20Note%2Fx"), "My Note/x");
        assert_eq!(percent_decode("100%é%2"), "100%é%2");
    }
}
