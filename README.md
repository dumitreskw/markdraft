# MarkDraft

A fast, local-first Markdown reader and writer for Windows, macOS and Linux.
Open a folder or a file, browse every Markdown document in a tree, edit with
live preview, group folders into workspaces, and export to PDF.

> Status: **M0 Foundations** — project layout, Tauri shell, CI, empty three-pane UI.
> See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [DECISIONS.md](DECISIONS.md).

## Develop

Prerequisites: Rust (stable), Node 24, pnpm 11, and the
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

## Layout

| Path | What |
|---|---|
| `crates/md-core` | Markdown parsing, rendering, title and TOC extraction |
| `crates/fs-index` | Discovery rules, tree model, file watching |
| `crates/workspace` | Workspace files, settings, session state |
| `crates/search` | Fuzzy file finding, full-text search |
| `crates/export` | PDF and HTML export |
| `crates/cli` | Command-line interface and `markdraft-cli` |
| `src-tauri` | Tauri commands and events (no business logic) |
| `ui` | SolidJS frontend |

## Licence

MIT OR Apache-2.0
