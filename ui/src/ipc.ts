// Typed wrappers for MarkDraft's Tauri commands and events.
// Names and shapes mirror src-tauri/src/commands and src-tauri/src/events.rs.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const inTauri = isTauri;

// ---------------------------------------------------------------- types

export interface AppInfo {
  name: string;
  version: string;
  os: string;
  arch: string;
}

export type PickKind = "folder" | "file" | "workspace";
export type SaveKind = "pdf" | "html" | "workspace";

export interface OpenRequest {
  path: string;
}

export type ErrorKind = "io" | "notFound" | "notAllowed" | "invalid" | "internal";

export interface CmdError {
  kind: ErrorKind;
  message: string;
}

export interface TreeNode {
  /** Relative to the root, `/`-separated. */
  path: string;
  isDir: boolean;
  title: string | null;
  modifiedMs: number;
}

export type Change =
  | { kind: "added"; node: TreeNode }
  | { kind: "removed"; path: string }
  | { kind: "renamed"; from: string; to: TreeNode }
  | { kind: "modified"; node: TreeNode }
  | { kind: "rescan" };

export interface RootView {
  id: number;
  path: string;
  name: string;
  singleFile: boolean;
}

export interface WorkspaceView {
  file: string;
  name: string;
}

export interface OpenResult {
  generation: number;
  roots: RootView[];
  focus: string | null;
  workspace: WorkspaceView | null;
  sessionKey: string;
}

export interface DocFormat {
  lineEnding: "lf" | "crlf";
  bom: boolean;
  encoding: "utf8" | "utf16Le" | "utf16Be" | "windows1252";
}

export interface LoadedDoc {
  text: string;
  format: DocFormat;
  modifiedMs: number;
  readonly: boolean;
}

export type SaveOutcome =
  | { status: "saved"; modifiedMs: number }
  | { status: "conflict"; diskModifiedMs: number };

export interface Heading {
  level: number;
  text: string;
  id: string;
  line: number;
}

export interface RenderedDoc {
  html: string;
  title: string | null;
  headings: Heading[];
  wordCount: number;
  hasMath: boolean;
  hasMermaid: boolean;
}

export interface QuickOpenItem {
  rootId: number;
  path: string;
  display: string;
  title: string | null;
  score: number;
  displayIndices: number[];
  titleIndices: number[];
}

export interface RecentEntry {
  path: string;
  kind: "folder" | "file" | "workspace";
  name: string;
  openedMs: number;
}

export interface TreeBatch {
  generation: number;
  rootId: number;
  nodes: TreeNode[];
  done: boolean;
  elapsedMs: number | null;
}

export interface FsChanged {
  generation: number;
  rootId: number;
  changes: Change[];
}

// ------------------------------------------------------------- commands

export const appInfo = () => invoke<AppInfo>("app_info");
export const pickPath = (kind: PickKind) => invoke<string | null>("pick_path", { kind });
export const pickSavePath = (kind: SaveKind, suggestedName: string) =>
  invoke<string | null>("pick_save_path", { kind, suggestedName });
export const takeOpenRequest = () => invoke<OpenRequest | null>("take_open_request");

export const getSettings = () => invoke<Record<string, unknown>>("get_settings");
export const setSettings = (settings: Record<string, unknown>) => invoke<void>("set_settings", { settings });
export const listRecents = () => invoke<RecentEntry[]>("list_recents");
export const removeRecent = (path: string) => invoke<RecentEntry[]>("remove_recent", { path });
export const getSession = (key: string) => invoke<unknown | null>("get_session", { key });
export const setSession = (key: string, session: unknown) => invoke<void>("set_session", { key, session });
export const highlightCss = () => invoke<{ light: string; dark: string }>("highlight_css");
export const openExternal = (url: string) => invoke<void>("open_external", { url });
export const revealPath = (path: string) => invoke<void>("reveal_path", { path });

export const openPath = (path: string) => invoke<OpenResult>("open_path", { path });
export const addRoot = (path: string) => invoke<OpenResult>("add_root", { path });
export const removeRoot = (rootId: number) => invoke<OpenResult>("remove_root", { rootId });
export const rescanRoot = (rootId: number) => invoke<void>("rescan_root", { rootId });
export const currentRoots = () => invoke<OpenResult>("current_roots");
export const saveWorkspace = (file: string, name: string) => invoke<OpenResult>("save_workspace", { file, name });

export const readFile = (path: string) => invoke<LoadedDoc>("read_file", { path });
export const saveFile = (path: string, text: string, format: DocFormat, expectedModifiedMs: number | null) =>
  invoke<SaveOutcome>("save_file", { path, text, format, expectedModifiedMs });
export const renderMarkdown = (text: string, path: string | null) =>
  invoke<RenderedDoc>("render_markdown", { text, path });
export const createEntry = (parent: string, name: string, isDir: boolean) =>
  invoke<string>("create_entry", { parent, name, isDir });
export const renameEntry = (from: string, to: string) => invoke<string>("rename_entry", { from, to });
export const trashEntry = (path: string) => invoke<void>("trash_entry", { path });
export const duplicateEntry = (path: string) => invoke<string>("duplicate_entry", { path });
/** Saves image bytes into an `assets` folder next to `doc`; returns the link. */
export const saveImage = (doc: string, name: string, bytes: Uint8Array) =>
  invoke<string>("save_image", bytes, {
    headers: { "x-doc": encodeURIComponent(doc), "x-name": encodeURIComponent(name) },
  });
/** Copies an image file into an `assets` folder next to `doc`; returns the link. */
export const importImage = (doc: string, source: string) => invoke<string>("import_image", { doc, source });
export const quickOpen = (query: string, limit: number) => invoke<QuickOpenItem[]>("quick_open", { query, limit });

// ----------------------------------------------------------------- export

export interface ExportOptions {
  theme: "clean" | "academic" | "compact";
  paper: "A4" | "Letter";
  toc: boolean;
}

/** Exports `text` (the document's current contents) to a PDF at `out`. */
export const exportPdf = (path: string, text: string, out: string, options: ExportOptions, diagrams: Record<number, string>) =>
  invoke<void>("export_pdf", { path, text, out, options, diagrams });

/** Exports every document in a folder into one PDF; returns the document count. */
export const exportFolderPdf = (folder: string, out: string, options: ExportOptions) =>
  invoke<number>("export_folder_pdf", { folder, out, options });

/** Writes an exported file to a path chosen in the save dialog. */
export const writeExport = (out: string, contents: string) => invoke<void>("write_export", { out, contents });

// ----------------------------------------------------------------- search

export interface TextQuery {
  pattern: string;
  regex: boolean;
  caseSensitive: boolean;
  wholeWord: boolean;
}

export interface LineMatch {
  line: number;
  text: string;
  /** UTF-16 offsets into `text`. */
  ranges: [number, number][];
}

export interface FileMatches {
  path: string;
  matches: LineMatch[];
  total: number;
}

export interface Backlink {
  path: string;
  line: number;
  text: string;
}

export const backlinks = (path: string) => invoke<Backlink[]>("backlinks", { path });

export const searchText = (query: TextQuery, maxFiles: number) =>
  invoke<FileMatches[]>("search_text", { query, maxFiles });

// -------------------------------------------------------------------- git

export type StatusKind = "modified" | "added" | "deleted" | "renamed" | "untracked" | "conflicted" | "typeChange";

export interface FileStatus {
  path: string;
  kind: StatusKind;
  staged: boolean;
  unstaged: boolean;
  oldPath: string | null;
}

export interface RepoInfo {
  workdir: string;
  branch: string | null;
  detached: boolean;
  head: { id: string; shortId: string; summary: string } | null;
  upstream: string | null;
  ahead: number | null;
  behind: number | null;
  operation: string | null;
}

export interface GitRoot {
  root: string;
  info: RepoInfo;
}

export interface CommitInfo {
  id: string;
  shortId: string;
  summary: string;
  message: string;
  authorName: string;
  authorEmail: string;
  timeMs: number;
  path: string;
}

export type RevArg =
  | { kind: "head" }
  | { kind: "index" }
  | { kind: "workdir" }
  | { kind: "commit"; id: string }
  | { kind: "parentOf"; id: string }
  | { kind: "text"; text: string };

export type SegmentKind = "equal" | "insert" | "delete";
export interface Segment {
  kind: SegmentKind;
  text: string;
}

export type BlockChange = "unchanged" | "added" | "removed" | "modified" | "moved" | "movedAway";

export type BlockType =
  | { type: "paragraph" }
  | { type: "heading"; level: number }
  | { type: "listItem"; ordered: boolean; depth: number /* 0 = top level */; checked: boolean | null; number: number | null }
  | { type: "code"; lang: string | null }
  | { type: "table" }
  | { type: "quote" }
  | { type: "rule" }
  | { type: "html" }
  | { type: "frontMatter" }
  | { type: "other" };

export interface TableDiff {
  aligns: ("none" | "left" | "center" | "right")[];
  rows: { kind: BlockChange; header: boolean; cells: Segment[][] }[];
}

export interface DiffBlock {
  kind: BlockChange;
  blockType: BlockType;
  oldBlockType: BlockType | null;
  oldText: string | null;
  newText: string | null;
  oldLines: [number, number] | null;
  newLines: [number, number] | null;
  segments: Segment[];
  table: TableDiff | null;
}

export interface DiffStats {
  addedBlocks: number;
  removedBlocks: number;
  modifiedBlocks: number;
  movedBlocks: number;
  wordsAdded: number;
  wordsRemoved: number;
  identical: boolean;
}

export type CollapsedEntry = { type: "block"; index: number } | { type: "gap"; start: number; end: number; count: number };

export interface DiffLine {
  kind: "context" | "insert" | "delete";
  oldNo: number | null;
  newNo: number | null;
  text: string;
  segments: Segment[];
}

export interface Hunk {
  oldStart: number;
  oldLen: number;
  newStart: number;
  newLen: number;
  lines: DiffLine[];
}

export interface DiffView {
  diff: { blocks: DiffBlock[]; stats: DiffStats; timedOut: boolean };
  collapsed: CollapsedEntry[];
  blocksHtml: (string | null)[];
  modifiedHtml: (string | null)[];
  hunks: Hunk[];
  oldExists: boolean;
  newExists: boolean;
}

export const gitInfo = () => invoke<GitRoot[]>("git_info");
export const gitStatus = (root: string) => invoke<FileStatus[]>("git_status", { root });
export const gitHistory = (path: string, limit: number) => invoke<CommitInfo[]>("git_history", { path, limit });
export const diffDocument = (path: string, old: RevArg, newRev: RevArg) =>
  invoke<DiffView>("diff_document", { path, old, new: newRev });

// --------------------------------------------------------------- events

export const onOpenPath = (handler: (req: OpenRequest) => void): Promise<UnlistenFn> =>
  listen<OpenRequest>("app://open-path", (e) => handler(e.payload));
export const onTreeBatch = (handler: (batch: TreeBatch) => void): Promise<UnlistenFn> =>
  listen<TreeBatch>("tree://batch", (e) => handler(e.payload));
export const onFsChanged = (handler: (changed: FsChanged) => void): Promise<UnlistenFn> =>
  listen<FsChanged>("fs://changed", (e) => handler(e.payload));

export function errorMessage(e: unknown): string {
  if (typeof e === "object" && e !== null && "message" in e) {
    return String((e as CmdError).message);
  }
  return String(e);
}

export function errorKind(e: unknown): ErrorKind | null {
  if (typeof e === "object" && e !== null && "kind" in e) {
    return (e as CmdError).kind;
  }
  return null;
}
