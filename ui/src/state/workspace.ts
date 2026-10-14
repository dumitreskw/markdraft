// Open roots and their file trees, fed by scan batches and watcher events;
// session save/restore; Git status for tree badges and the Changes panel.
import { batch, createSignal } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import * as ipc from "../ipc";
import type { FileStatus, GitRoot, OpenResult, RootView, TreeNode } from "../ipc";
import { joinPath, relativeTo, samePath } from "../paths";
import { activePath, closeAll, docs, openDoc, setAfterSave, handleFsChanges, tabs, renderNow } from "./docs";
import { settings, type ViewMode } from "./settings";
import { applyChanges, rowKey, type RootInput } from "./tree";
import {
  outlineWidth,
  setOutlineWidth,
  setSplitRatio,
  setTreeWidth,
  setViewMode,
  splitRatio,
  toast,
  treeWidth,
  viewMode,
} from "./ui";

export const [opened, setOpened] = createSignal<OpenResult | null>(null);
const nodeMaps = new Map<number, Map<string, TreeNode>>();
const [treeVersion, setTreeVersion] = createSignal(0);
/** Changes whenever any root's nodes change. */
export const treeVersionSignal = treeVersion;
export const [scanning, setScanning] = createSignal<number[]>([]);
export const [expanded, setExpanded] = createSignal<Set<string>>(new Set());
export const [selectedKey, setSelectedKey] = createSignal<string | null>(null);
export const [treeFilter, setTreeFilter] = createSignal("");
export const [lastScanMs, setLastScanMs] = createSignal<number | null>(null);

export const roots = (): RootView[] => opened()?.roots ?? [];
export const rootById = (id: number) => roots().find((r) => r.id === id);

/** Root inputs for the tree; tracks changes to the node maps. */
export function rootInputs(): RootInput[] {
  treeVersion();
  return roots().map((r) => ({ id: r.id, name: r.name, nodes: nodeMaps.get(r.id) ?? new Map() }));
}

export function nodesOf(rootId: number): Map<string, TreeNode> {
  treeVersion();
  return nodeMaps.get(rootId) ?? new Map();
}

/** All documents across roots as absolute paths with titles. */
export function allDocs(): { path: string; rel: string; title: string | null; modifiedMs: number; rootId: number }[] {
  treeVersion();
  const out = [];
  for (const r of roots()) {
    for (const n of nodeMaps.get(r.id)?.values() ?? []) {
      if (!n.isDir) out.push({ path: joinPath(r.path, n.path), rel: n.path, title: n.title, modifiedMs: n.modifiedMs, rootId: r.id });
    }
  }
  return out;
}

/** The root containing an absolute path, and the `/` path within it. */
export function locate(path: string): { root: RootView; rel: string } | null {
  let best: { root: RootView; rel: string } | null = null;
  for (const root of roots()) {
    const rel = relativeTo(root.path, path);
    if (rel !== null && (!best || root.path.length > best.root.path.length)) best = { root, rel };
  }
  return best;
}

let bumpPending = false;
function bump() {
  if (bumpPending) return;
  bumpPending = true;
  requestAnimationFrame(() => {
    bumpPending = false;
    setTreeVersion((v) => v + 1);
  });
}

// ----------------------------------------------------------- opening

/** Opens a folder, Markdown file or workspace file, replacing the current set. */
export async function openPathAction(path: string): Promise<boolean> {
  const current = opened();
  if (current) {
    await persistSession();
    if (!(await closeAll())) return false;
  }
  try {
    const result = await ipc.openPath(path);
    await applyOpen(result);
    return true;
  } catch (e) {
    toast(`Could not open ${path}: ${ipc.errorMessage(e)}`, "error");
    return false;
  }
}

async function applyOpen(result: OpenResult) {
  batch(() => {
    nodeMaps.clear();
    for (const r of result.roots) nodeMaps.set(r.id, new Map());
    setOpened(result);
    setScanning(result.roots.map((r) => r.id));
    setExpanded(new Set<string>());
    setTreeFilter("");
    setSelectedKey(null);
    setGitState({ roots: [], status: {} });
  });
  bump();
  sessionReady = false;
  const restored = settings.restoreSession ? await restoreSession(result) : false;
  if (result.focus) await openDoc(result.focus);
  if (!restored && !result.focus) setViewMode(settings.defaultView);
  sessionReady = true;
  void refreshGit();
}

/** After a UI reload, shows whatever Rust still has open and rescans it. */
export async function reattach(): Promise<boolean> {
  try {
    const current = await ipc.currentRoots();
    if (!current.roots.length) return false;
    await applyOpen(current);
    for (const r of current.roots) void ipc.rescanRoot(r.id);
    return true;
  } catch {
    return false;
  }
}

export async function addRootAction(path: string) {
  try {
    const result = await ipc.addRoot(path);
    for (const r of result.roots) if (!nodeMaps.has(r.id)) nodeMaps.set(r.id, new Map());
    setScanning((s) => [...s, ...result.roots.filter((r) => !s.includes(r.id) && nodeMaps.get(r.id)!.size === 0).map((r) => r.id)]);
    setOpened(result);
    bump();
  } catch (e) {
    toast(`Could not add folder: ${ipc.errorMessage(e)}`, "error");
  }
}

export async function removeRootAction(id: number) {
  const root = rootById(id);
  if (!root) return;
  for (const path of [...tabs()]) {
    if (relativeTo(root.path, path) !== null) {
      const { closeTab } = await import("./docs");
      if (!(await closeTab(path))) return;
    }
  }
  const result = await ipc.removeRoot(id);
  nodeMaps.delete(id);
  setOpened(result);
  bump();
}

export async function saveWorkspaceAs(name: string): Promise<boolean> {
  const suggested = name.replace(/[\\/:*?"<>|]/g, "-") + ".markdraft-workspace";
  const file = await ipc.pickSavePath("workspace", suggested);
  if (!file) return false;
  try {
    const result = await ipc.saveWorkspace(file, name);
    setOpened((o) => (o ? { ...o, workspace: result.workspace, sessionKey: result.sessionKey } : result));
    toast(`Saved workspace “${name}”.`, "success");
    return true;
  } catch (e) {
    toast(`Could not save the workspace: ${ipc.errorMessage(e)}`, "error");
    return false;
  }
}

/** Starts listening to scan and watcher events. Returns a cleanup function. */
export function initWorkspaceEvents(): () => void {
  const unlisteners = [
    ipc.onTreeBatch((b) => {
      if (b.generation !== opened()?.generation) return;
      let map = nodeMaps.get(b.rootId);
      if (!map) nodeMaps.set(b.rootId, (map = new Map()));
      for (const n of b.nodes) map.set(n.path, n);
      if (b.done) {
        setScanning((s) => s.filter((id) => id !== b.rootId));
        if (b.elapsedMs !== null) setLastScanMs(b.elapsedMs);
        scheduleGitRefresh();
      }
      bump();
    }),
    ipc.onFsChanged((c) => {
      if (c.generation !== opened()?.generation) return;
      const map = nodeMaps.get(c.rootId);
      const root = rootById(c.rootId);
      if (!map || !root) return;
      if (applyChanges(map, c.changes)) {
        map.clear();
        setScanning((s) => [...s, c.rootId]);
        void ipc.rescanRoot(c.rootId);
      }
      void handleFsChanges(root.path, c.changes);
      bump();
      scheduleGitRefresh();
    }),
  ];
  setAfterSave(() => scheduleGitRefresh());
  const onFocus = () => scheduleGitRefresh();
  window.addEventListener("focus", onFocus);
  return () => {
    for (const u of unlisteners) void u.then((stop) => stop());
    window.removeEventListener("focus", onFocus);
  };
}

export function toggleExpanded(key: string, open?: boolean) {
  setExpanded((prev) => {
    const next = new Set(prev);
    const shouldOpen = open ?? !next.has(key);
    if (shouldOpen) next.add(key);
    else next.delete(key);
    return next;
  });
}

/** Expands the folders above a document so it is visible in the tree. */
export function revealInTree(path: string) {
  const loc = locate(path);
  if (!loc) return;
  const keys: string[] = [rowKey(loc.root.id, "")];
  const parts = loc.rel.split("/");
  for (let i = 1; i < parts.length; i++) keys.push(rowKey(loc.root.id, parts.slice(0, i).join("/")));
  setExpanded((prev) => new Set([...prev, ...keys]));
  setSelectedKey(rowKey(loc.root.id, loc.rel));
}

// ----------------------------------------------------------- session

interface SessionData {
  tabs: string[];
  active: string | null;
  expanded: string[];
  viewMode: ViewMode;
  treeWidth: number;
  outlineWidth: number;
  splitRatio: number;
}

let sessionReady = false;

async function restoreSession(result: OpenResult): Promise<boolean> {
  let data: Partial<SessionData> | null = null;
  try {
    data = (await ipc.getSession(result.sessionKey)) as Partial<SessionData> | null;
  } catch {
    return false;
  }
  if (!data) return false;
  if (data.viewMode) setViewMode(data.viewMode);
  if (data.treeWidth) setTreeWidth(data.treeWidth);
  if (data.outlineWidth) setOutlineWidth(data.outlineWidth);
  if (data.splitRatio) setSplitRatio(data.splitRatio);
  if (data.expanded) {
    const keys = new Set<string>();
    for (const abs of data.expanded) {
      const loc = locate(abs);
      if (loc) keys.add(rowKey(loc.root.id, loc.rel));
    }
    setExpanded(keys);
  }
  for (const path of data.tabs ?? []) {
    if (locate(path)) await openDocQuiet(path);
  }
  const active = data.active && tabs().find((t) => samePath(t, data!.active!));
  if (active) await openDoc(active);
  return true;
}

/** Restores a tab without error toasts for files that no longer exist. */
async function openDocQuiet(path: string) {
  try {
    await ipc.readFile(path);
  } catch {
    return;
  }
  await openDoc(path);
}

export async function persistSession() {
  const o = opened();
  if (!o || !sessionReady || !ipc.inTauri()) return;
  const expandedAbs: string[] = [];
  for (const key of expanded()) {
    const i = key.indexOf(":");
    const root = rootById(Number(key.slice(0, i)));
    if (root) expandedAbs.push(joinPath(root.path, key.slice(i + 1)));
  }
  const data: SessionData = {
    tabs: tabs(),
    active: activePath(),
    expanded: expandedAbs,
    viewMode: viewMode(),
    treeWidth: treeWidth(),
    outlineWidth: outlineWidth(),
    splitRatio: splitRatio(),
  };
  try {
    await ipc.setSession(o.sessionKey, data);
  } catch {
    // Not critical.
  }
}

let sessionTimer: ReturnType<typeof setTimeout> | undefined;
export function scheduleSessionSave() {
  clearTimeout(sessionTimer);
  sessionTimer = setTimeout(() => void persistSession(), 1000);
}

// --------------------------------------------------------------- git

interface GitState {
  roots: GitRoot[];
  /** Lower-cased, `/`-separated absolute path → status. */
  status: Record<string, FileStatus>;
}

export const [gitState, setGitState] = createStore<GitState>({ roots: [], status: {} });

export const gitKey = (path: string) => path.replace(/\\/g, "/").toLowerCase();

export function gitStatusOf(path: string): FileStatus | undefined {
  return gitState.status[gitKey(path)];
}

/** Status of a folder: the "strongest" status of anything below it. */
export function gitFolderHasChanges(path: string): boolean {
  const prefix = gitKey(path) + "/";
  return Object.keys(gitState.status).some((k) => k.startsWith(prefix));
}

let gitTimer: ReturnType<typeof setTimeout> | undefined;
export function scheduleGitRefresh() {
  clearTimeout(gitTimer);
  gitTimer = setTimeout(() => void refreshGit(), 700);
}

export async function refreshGit() {
  if (!ipc.inTauri() || !opened()) return;
  try {
    const infos = await ipc.gitInfo();
    const status: Record<string, FileStatus> = {};
    for (const g of infos) {
      for (const s of await ipc.gitStatus(g.root)) status[gitKey(s.path)] = s;
    }
    setGitState("roots", infos);
    setGitState("status", reconcile(status));
  } catch {
    // Git is optional.
  }
}

/** Re-renders open documents (for example after a theme change). */
export function rerenderOpenDocs() {
  for (const p of tabs()) if (docs[p]) void renderNow(p);
}
