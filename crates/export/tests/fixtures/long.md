---
title: MarkDraft Architecture Guide
subtitle: How documents travel from keystrokes to PDF
author: The MarkDraft Team
date: October 2025
---

## Introduction

MarkDraft is a desktop application for reading and writing Markdown. It is
built on Tauri 2, with a Rust core that parses, indexes and exports documents,
and a lightweight web UI that renders the editor and the live preview. This
guide walks through the main subsystems, the data that flows between them and
the design decisions that shaped them.

The guide is written for contributors. It assumes familiarity with Rust and
with the basics of Markdown, but no prior knowledge of the code base. Each
section ends with pointers to the relevant crates so you can continue reading
the source.

> Good architecture is less about the boxes and more about the arrows between
> them. — an anonymous reviewer

### Goals

The project optimises for a small set of goals, in this order:

1. **Fidelity.** What you see in the preview is what you get in the export.
2. **Speed.** Opening a folder with thousands of notes must feel instant.
3. **Portability.** The same document renders identically on Windows, macOS
   and Linux.
4. **Simplicity.** Every dependency must earn its place.

### Non-goals

- Real-time collaboration. MarkDraft works on local files; synchronisation is
  left to tools that already do it well (Git, cloud drives).
- A plugin system. Extensibility is valuable, but a stable plugin API is a
  long-term commitment that the project is not ready to make.
- WYSIWYG editing. The source is always the truth.

## Workspace layout

The repository is a Cargo workspace with a handful of crates. Each crate has a
narrow responsibility and no crate depends on the UI.

| Crate | Responsibility | Depends on |
|:------|:---------------|:-----------|
| `md-core` | Parsing, HTML rendering, titles and headings | comrak |
| `fs-index` | Watching folders and indexing files | ignore, globset |
| `search` | Fuzzy and full-text search | nucleo-matcher |
| `workspace` | Saved workspaces and recent files | serde, toml |
| `export` | PDF export through Typst | typst, comrak |
| `cli` | Command-line interface | clap, export |

The desktop shell in `src-tauri` wires these crates to the web UI through
Tauri commands. Commands are thin: they validate input, call into a crate and
serialise the result.

### Building

A debug build of the whole workspace takes a couple of minutes on a laptop.
Release builds enable link-time optimisation and take noticeably longer.

```sh
# Run the app in development mode
pnpm tauri dev

# Run all Rust tests
cargo test --workspace

# Export a document from the command line
cargo run -p cli --bin markdraft-cli -- export notes.md -o notes.pdf --toc
```

> [!TIP]
> Use `cargo test -p <crate>` while iterating on one crate; it is much faster
> than testing the whole workspace.

## Parsing

All Markdown parsing goes through comrak, a CommonMark and GitHub Flavored
Markdown parser written in Rust. The parser options are defined in one place,
`md_core::options()`, so that the preview, the title extraction and the
exporter agree on what a document contains.

Comrak produces an arena-allocated syntax tree. Nodes are cheap to create and
are freed all at once when the arena is dropped, which suits the
parse-render-discard pattern of a previewer well.

```rust
use comrak::{Arena, parse_document};

pub fn headings(markdown: &str) -> Vec<String> {
    let arena = Arena::new();
    let root = parse_document(&arena, markdown, &md_core::options());
    root.descendants()
        .filter_map(|node| match &node.data.borrow().value {
            NodeValue::Heading(_) => Some(plain_text(node)),
            _ => None,
        })
        .collect()
}
```

### Extensions

MarkDraft enables the GitHub extensions that people expect: tables, task
lists, strikethrough, autolinks and footnotes.[^gfm] Front matter delimited by
`---` is recognised and hidden from the preview; its `title` key overrides the
first heading as the document title.

[^gfm]: See the GitHub Flavored Markdown specification for the details of
    each extension.

### Titles

The title of a document is resolved in this order:

1. The `title` key of the front matter.
2. The text of the first level-one heading.
3. The file name without its extension.

## Indexing

When a folder is opened, `fs-index` walks it with the `ignore` crate, which
honours `.gitignore` files, and records every Markdown file it finds. The walk
runs on a background thread and streams results to the UI so the sidebar
fills in progressively.

### Watching for changes

After the initial walk, a file watcher keeps the index up to date. Events are
debounced: editors often write a file several times in quick succession, and
re-indexing on every write would waste work.

- [x] Debounce bursts of events
- [x] Ignore temporary files written by editors
- [ ] Detect renames as a single event on every platform

> [!NOTE]
> On network drives, file watching may be unreliable. MarkDraft falls back to
> polling when it detects that events are not delivered.

## Search

Search combines two strategies. File names are matched with a fuzzy matcher
that ranks results by how well the query characters line up with word
boundaries. Contents are matched with a simple inverted index that is rebuilt
incrementally as files change.

The fuzzy score of a candidate $c$ for a query $q$ is the sum of the bonuses of
the matched characters minus a penalty for gaps:

$$
\operatorname{score}(q, c) = \sum_{i=1}^{|q|} b(q_i, c_{j_i}) - \lambda \sum_{i=2}^{|q|} (j_i - j_{i-1} - 1)
$$

where $b$ rewards matches at word starts and $\lambda$ is a small constant.

## Export

Export turns a Markdown document into a PDF without leaving the process. The
pipeline has three stages:

1. Parse the Markdown with the shared comrak options.
2. Translate the syntax tree into Typst markup, escaping all user text.
3. Compile the markup with the Typst compiler against an in-memory world that
   contains only bundled fonts, bundled templates and the document's images.

Because the world never reads system fonts or the network, the same input
produces byte-identical PDFs on every platform. Themes are ordinary Typst
templates; switching the theme swaps the set and show rules without changing
the generated markup.

> [!IMPORTANT]
> Never interpolate user text into Typst markup without escaping it. A stray
> `#` would otherwise be interpreted as code.

### Themes

| Theme | Body font | Size | Character |
|-------|-----------|-----:|-----------|
| Clean | Inter | 10.5 pt | Generous margins, teal accent |
| Academic | Source Serif 4 | 11 pt | Justified, numbered headings |
| Compact | Inter | 9.5 pt | Dense, for reference material |

### Performance

Typst compiles a twenty-page document in a fraction of a second. Most of the
time is spent on text shaping and layout; PDF writing is comparatively cheap.
Fonts are parsed once per process and reused across exports.

> [!WARNING]
> Very large images are embedded at full resolution. Downscale screenshots
> before exporting if file size matters.

## Error handling

Errors are values. Each crate defines an error enum with `thiserror`, and the
shell converts errors into messages for the UI at the boundary. Panics are
reserved for bugs.

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown theme `{0}`")]
    UnknownTheme(String),
    #[error("typesetting failed:\n{0}")]
    Compile(String),
}
```

> [!CAUTION]
> Do not use `unwrap()` on user-controlled input. A malformed file must never
> crash the application.

## Testing strategy

Tests live next to the code they exercise. Rendering code is tested with
snapshot tests that store the expected output in the repository, which makes
changes to the output visible in code review.

- **Unit tests** cover pure functions such as escaping and title extraction.
- **Snapshot tests** cover the Markdown to HTML and Markdown to Typst
  translations on a corpus of fixture documents.
- **Integration tests** compile every fixture to PDF and check that the output
  is valid and reproducible.

---

## Glossary

Arena
: A memory pool from which many small objects are allocated and freed together.

Front matter
: A block of metadata at the top of a document, delimited by `---` lines.

World
: Typst's interface to files, fonts and the current date.

## Appendix: keyboard shortcuts

| Action | Windows / Linux | macOS |
|--------|-----------------|-------|
| Open folder | <kbd>Ctrl</kbd>+<kbd>O</kbd> | <kbd>⌘</kbd>+<kbd>O</kbd> |
| Quick open | <kbd>Ctrl</kbd>+<kbd>P</kbd> | <kbd>⌘</kbd>+<kbd>P</kbd> |
| Search in files | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> | <kbd>⌘</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> |
| Export to PDF | <kbd>Ctrl</kbd>+<kbd>E</kbd> | <kbd>⌘</kbd>+<kbd>E</kbd> |
| Toggle preview | <kbd>Ctrl</kbd>+<kbd>K</kbd> <kbd>V</kbd> | <kbd>⌘</kbd>+<kbd>K</kbd> <kbd>V</kbd> |
