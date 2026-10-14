# Architecture

MarkDraft is a Cargo workspace of UI-free crates behind a thin Tauri 2 shell.
The WebView renders and edits text; it never touches the file system.

```text
markdraft/
├─ crates/
│  ├─ md-core/     comrak → HTML (data-sourcepos), title, TOC
│  ├─ fs-index/    discovery rules, tree model, watcher → incremental diffs
│  ├─ workspace/   *.markdraft-workspace (TOML), session state, recents
│  ├─ search/      fuzzy ranking (nucleo-matcher); full-text later
│  ├─ export/      export options; AST → Typst → PDF (M3)
│  └─ cli/         clap definitions, headless runner, markdraft-cli binary
├─ src-tauri/      commands/, events.rs, state.rs, error.rs — glue only
└─ ui/             SolidJS + TypeScript + Vite
```

## IPC contract

Every command returns `Result<T, CmdError>`; `CmdError` serialises as
`{ "kind": "io" | "internal", "message": string }`. Each command is listed in
`src-tauri/build.rs` and granted in `src-tauri/capabilities/default.json`.
TypeScript wrappers live in `ui/src/ipc.ts`.

### Commands

| Milestone | Signature | Status |
|---|---|---|
| M0 | `app_info() -> AppInfo` | done |
| M0 | `pick_path(kind: PickKind) -> Option<PathBuf>` | done |
| M0 | `take_open_request() -> Option<OpenRequest>` | done |
| M1 | `open_path(path: PathBuf) -> OpenResult` | planned |
| M1 | `read_file(path: PathBuf) -> FileContent` | planned |
| M1 | `render_markdown(text: String, base: PathBuf) -> RenderedDoc` | planned |
| M1 | `set_tree_options(opts: TreeOptions) -> ()` | planned |
| M2 | `save_file(path: PathBuf, text: String, expected_mtime: Option<i64>) -> SaveOutcome` | planned |
| M2 | `create_entry(parent: PathBuf, name: String, is_dir: bool) -> PathBuf` | planned |
| M2 | `rename_entry(from: PathBuf, to: PathBuf) -> ()` | planned |
| M2 | `trash_entry(path: PathBuf) -> ()` | planned |
| M2 | `quick_open(query: String, limit: u32) -> Vec<FileMatch>` | planned |
| M3 | `open_workspace(file: PathBuf) -> OpenResult` | planned |
| M3 | `save_workspace(file: PathBuf, def: WorkspaceDef) -> ()` | planned |
| M3 | `list_recent() -> Vec<RecentEntry>` | planned |
| M3 | `get_session(key: String) -> SessionState` / `set_session(key, session) -> ()` | planned |
| M3 | `export_pdf(path: PathBuf, out: PathBuf, opts: ExportOptions) -> ExportReport` | planned |

### Events (Rust → UI)

| Name | Payload | Status |
|---|---|---|
| `app://open-path` | `OpenRequest { path }` — second launch, file association | done |
| `tree://batch` | `TreeBatch { root_id, nodes: Vec<TreeNode>, done }` | M1 |
| `fs://changed` | `FsDiff { root_id, changes: Vec<Change> }` | M1 |
| `export://progress` | `ExportProgress { stage, pct }` | M3 |
