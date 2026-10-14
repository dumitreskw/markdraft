# Architecture

MarkDraft is a Cargo workspace of UI-free crates behind a thin Tauri 2 shell.
The WebView renders and edits text; it never touches the file system.

```text
markdraft/
├─ crates/
│  ├─ md-core/     comrak → sanitised HTML (data-sourcepos), syntect classes,
│  │               title, headings, word count; golden HTML snapshots
│  ├─ fs-index/    discovery rules, parallel scan, watcher → incremental diffs,
│  │               document read/save (encoding, BOM, line endings, conflicts)
│  ├─ workspace/   *.markdraft-workspace (TOML), app-data store: settings,
│  │               recents, per-workspace session state
│  ├─ search/      fuzzy ranking (nucleo-matcher), parallel full-text search
│  ├─ vcs/         read-only Git (git2): status, history, file at revision;
│  │               readable block/word document diff and line diff
│  ├─ export/      Markdown → Typst → PDF in-process, 3 themes, bundled fonts,
│  │               LaTeX via mitex, combined folder export
│  └─ cli/         clap definitions, headless export, markdraft-cli binary
├─ src-tauri/      commands/, roots.rs (scans + watchers), protocol.rs
│                  (mdasset images), state.rs, events.rs — glue only
└─ ui/             SolidJS + TypeScript + Vite; CodeMirror 6, KaTeX, Mermaid
```

## Data flow

1. `open_path` resolves a folder, a file (its folder, one level deep) or a
   workspace file into roots, added to the window's set; the UI groups the
   roots of each call into one workspace tab. Each root is scanned on a background thread;
   nodes stream to the UI as `tree://batch` events and into a per-root index
   (path → title) used by quick open and search.
2. After the scan a debounced watcher starts per root. Its diffs update the
   index and reach the UI as `fs://changed`. Writes made by MarkDraft are
   recorded (path + mtime) so their echo is ignored.
3. The UI builds the tree from the flat node maps, renders documents through
   `render_markdown`, and edits in CodeMirror. Saves pass the mtime read at
   load; a mismatch is a conflict, never an overwrite.
4. Every path a command receives is canonicalised and checked against the
   open roots. Paths outside them are writable only when the user chose them
   in a save dialog during this run.

## IPC contract

Every command returns `Result<T, CmdError>`; `CmdError` serialises as
`{ "kind": "io" | "notFound" | "notAllowed" | "invalid" | "internal", "message": string }`.
Each command is listed in `src-tauri/build.rs` (`COMMANDS`) and granted in
`src-tauri/capabilities/default.json`. TypeScript wrappers: `ui/src/ipc.ts`.

### Commands

| Area | Signature |
|---|---|
| App | `app_info() -> AppInfo` |
| App | `pick_path(kind: Folder\|File\|Workspace) -> Option<PathBuf>` |
| App | `pick_save_path(kind: Pdf\|Html\|Workspace, suggested_name: String) -> Option<PathBuf>` (approves the path for one write) |
| App | `take_open_request() -> Option<OpenRequest>` |
| App | `get_settings() -> Value` / `set_settings(settings: Value)` |
| App | `list_recents() -> Vec<RecentEntry>` / `remove_recent(path) -> Vec<RecentEntry>` |
| App | `get_session(key) -> Option<Value>` / `set_session(key, session: Value)` |
| App | `highlight_css() -> { light, dark }` |
| App | `open_external(url)` (http, https, mailto only) / `reveal_path(path)` |
| Roots | `open_path(path) -> OpenResult` (adds the roots of one workspace tab) |
| Roots | `add_root(path, siblings: Vec<RootId>) -> OpenResult` / `remove_roots(root_ids)` / `rescan_root(root_id)` |
| Roots | `current_roots() -> AllRoots { roots, workspaces }` (used after a WebView reload) |
| Roots | `save_workspace(file, name, root_ids) -> OpenResult` |
| Roots | `get_window_state() -> Option<Value>` / `set_window_state(window: Value)` (open workspace tabs) |
| Docs | `read_file(path) -> LoadedDoc` |
| Docs | `save_file(path, text, format: DocFormat, expected_modified_ms: Option<i64>) -> SaveOutcome` |
| Docs | `render_markdown(text, path: Option<PathBuf>) -> RenderedDoc` |
| Docs | `create_entry(parent, name, is_dir) -> PathBuf` / `rename_entry(from, to) -> PathBuf` |
| Docs | `trash_entry(path)` / `duplicate_entry(path) -> PathBuf` |
| Search | `quick_open(query, limit, root_ids: Option<Vec<RootId>>) -> Vec<QuickOpenItem>` |
| Search | `search_text(query: TextQuery, max_files, root_ids) -> Vec<FileMatches>` / `backlinks(path, root_ids) -> Vec<Backlink>` |
| Git | `git_info() -> Vec<GitRoot>` / `git_status(root) -> Vec<FileStatus>` |
| Git | `git_history(path, limit) -> Vec<CommitInfo>` |
| Git | `diff_document(path, old: RevArg, new: RevArg) -> DiffView` (RevArg: head, index, workdir, commit, parentOf, text) |
| Export | `export_pdf(path, text, out, options: ExportOptions, diagrams: HashMap<usize, String>)` |
| Export | `export_folder_pdf(folder, out, options) -> usize` |
| Export | `write_export(out, contents)` |
| System | `integration_status() -> { cli, contextMenu }` / `set_cli_on_path(enable) -> Feature` / `set_context_menu(enable) -> Feature` |

### Events (Rust → UI)

| Name | Payload |
|---|---|
| `app://open-path` | `OpenRequest { path }` — second launch, file association, macOS open |
| `tree://batch` | `TreeBatch { root_id, nodes: Vec<TreeNode>, done, elapsed_ms }` |
| `fs://changed` | `FsChanged { root_id, changes: Vec<Change> }` (added, removed, renamed, modified, rescan) |

### Custom protocol

`mdasset` (`http://mdasset.localhost/<encoded path>` on Windows,
`mdasset://localhost/<encoded path>` elsewhere) serves local images referenced
by documents. Only image types are returned, with a restrictive CSP header.
