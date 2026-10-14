# MarkDraft

A fast, local-first Markdown reader and writer for Windows, macOS and Linux.
Open a folder or a file, browse every Markdown document in a tree, edit with
live preview, review changes the way a writer reads them, and export to PDF.
Files stay plain `.md` on disk — no database, no lock-in.

## Features

**Browse**
- Open a folder, a single file (its sibling documents appear too) or a saved
  multi-folder workspace — from the menu, by dropping it on the window, or
  `markdraft <path>`.
- Several folders or workspaces open side by side as **workspace tabs** in one
  window; search, quick open and Git follow the current tab. Tabs come back
  after a restart.
- System integration: type `markdraft` in a terminal to open the current
  folder, and right-click a folder for **Open with MarkDraft** /
  **Open MarkDraft here**. The Windows installers set this up (setup.exe asks;
  the MSI adds it for all users); otherwise switch it on in Settings → System
  (Windows, Linux; macOS gets the terminal command). The Linux `.deb`/`.rpm`
  put `markdraft` on the PATH already.
- File tree with document titles (or file names), natural sorting, filter,
  keyboard navigation and type-ahead; live updates as files change on disk.
- Respects `.gitignore`, `.ignore` and `.markdraftignore`; 10,000 files show
  their first rows in about 7 ms.
- Quick open (`Ctrl/⌘+P`) by name or title, full-text search with regex,
  heading outline, recent folders and recently edited documents.

**Read and write**
- Read, Edit and Split views with scroll sync; tabs with per-tab undo.
- CommonMark + GitHub Flavored Markdown: tables, task lists (click to tick),
  footnotes (with hover previews), callouts (`> [!NOTE]`), syntax highlighting
  with copy buttons, `==highlights==`, emoji shortcodes (`:rocket:`).
- Diagrams: **Mermaid** and **Graphviz** (```` ```dot ````), with copy/save as SVG.
- Math: KaTeX for `$…$`, `$$…$$` and ```` ```math ```` blocks.
- Notes-app conventions: wiki links (`[[Page]]`), Obsidian image embeds
  (`![[image.png|300]]`), `[TOC]` / `[[_TOC_]]` markers, definition lists,
  front matter shown as a properties card, backlinks, optional smart punctuation.
- Autosave, conflict detection when a file changes on disk (compare, keep
  mine, or take the disk version), encodings and line endings preserved.
- Create, rename (F2), duplicate, move (drag in the tree) and delete to trash.

**Review changes (Git)**
- Status badges in the tree and a Changes panel listing edited documents.
- A readable diff: paragraphs, list items and table rows marked added,
  removed, edited or moved, with word-level changes inside edited text and
  formatting kept — like tracked changes. A classic line view is one click away.
- File history: pick any commit to see what it changed in the document.

**Export**
- PDF through Typst with three themes (Clean, Academic, Compact), A4 or US
  Letter, optional table of contents; fonts are bundled so output is
  identical on every OS. Math and Mermaid diagrams included.
- Whole folders as one PDF with a cover and contents; self-contained HTML.
- Headless: `markdraft-cli export notes.md -o notes.pdf --theme academic --toc`.

## Keyboard shortcuts

`Ctrl` on Windows/Linux, `⌘` on macOS. All commands: `Ctrl/⌘+Shift+P`.

| Action | Keys |
|---|---|
| Go to document / command palette | `Ctrl+P` / `Ctrl+Shift+P` |
| Open folder / file | `Ctrl+O` / `Ctrl+Shift+O` |
| New document / save / close tab | `Ctrl+N` / `Ctrl+S` / `Ctrl+W` |
| Read / Edit / Split | `Ctrl+1` / `Ctrl+2` / `Ctrl+3` |
| Search in files / changed documents | `Ctrl+Shift+F` / `Ctrl+Shift+G` |
| Compare with last commit / history | `Ctrl+Alt+D` / `Ctrl+Alt+H` |
| Export to PDF | `Ctrl+Shift+E` |
| Bold / italic / link | `Ctrl+B` / `Ctrl+I` / `Ctrl+K` |
| Toggle sidebar / outline / focus mode | `Ctrl+\` / `Ctrl+Shift+\` / `F11` |
| Next / previous workspace tab, close it | `Ctrl+Alt+PgDn` / `Ctrl+Alt+PgUp`, `Ctrl+Alt+W` |
| Settings | `Ctrl+,` |

## Download

Every release ships, for each system:

| | Install | No install | Other |
|---|---|---|---|
| Windows x64 · ARM64 · x86 | `setup.exe` (per-user, no admin) | `portable.zip` | `.msi` for IT deployment (x64, x86) |
| macOS (Apple silicon, Intel) | `.dmg` — universal or per-architecture | | |
| Linux x64 · ARM64 | `.deb`, `.rpm` | `.AppImage` | |
| Headless CLI (all of the above) | | `markdraft-cli` archive | PDF export for servers and CI |

The portable Windows build keeps settings in a `data` folder beside
`markdraft.exe` (delete `portable.txt` to use the user profile).
Checksums are in `SHA256SUMS.txt`. How releases are built and signed:
[docs/SIGNING.md](docs/SIGNING.md).

## Develop

Prerequisites: Rust 1.92+, Node 24, pnpm 11, and the
[Tauri system dependencies](https://tauri.app/start/prerequisites/) for your OS
(WebView2 on Windows, Xcode tools on macOS, WebKitGTK 4.1 on Linux).

```sh
pnpm install
pnpm dev                 # run the app with hot reload
pnpm dev -- -- ./docs    # open a path on launch
pnpm build               # release installers in target/release/bundle
```

Checks (the same ones CI runs):

```sh
pnpm typecheck && pnpm test && pnpm ui:build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Architecture and the IPC contract: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
Design decisions: [DECISIONS.md](DECISIONS.md).

## Layout

| Path | What |
|---|---|
| `crates/md-core` | Markdown rendering, highlighting, title and heading extraction |
| `crates/fs-index` | Discovery, file watching, document read/save |
| `crates/workspace` | Workspace files, settings, recents, sessions |
| `crates/search` | Quick open ranking and full-text search |
| `crates/vcs` | Git status/history and the readable document diff |
| `crates/export` | PDF export through Typst |
| `crates/cli` | Command-line interface and `markdraft-cli` |
| `src-tauri` | Tauri commands and events (no business logic) |
| `ui` | SolidJS frontend |

## Licence

MarkDraft is **free for personal and noncommercial use** under the
[PolyForm Noncommercial License 1.0.0](LICENSE.md). Companies and paid work
need a [commercial license](COMMERCIAL.md). Contributions: see
[CONTRIBUTING.md](CONTRIBUTING.md).

Bundled fonts are under the SIL Open Font License and the GUST Font License
(see `crates/export/fonts/`); the vendored mitex Typst package is Apache-2.0.
