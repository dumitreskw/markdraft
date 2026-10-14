// Workspace tabs: several folders / workspaces open in one window, each
// with its own roots, document tabs, tree state and layout. Trees are fed by
// scan batches and watcher events for every open root (background tabs stay
// live). Also: per-tab session save/restore and Git status.
import { batch, createSignal } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import * as ipc from "../ipc";
import type { FileStatus, GitRoot, RootView, TreeNode, WorkspaceView } from "../ipc";
import { baseName, joinPath, relativeTo, samePath } from "../paths";
import {
  activePath,
  closeAll,
  closeTab,
  docs,
  handleFsChanges,
  openDoc,
  recentDocs,
  renderNow,
  setActivePath,
  setAfterSave,
  setRecentDocs,
  setTabs,
  tabs,
} from "./docs";
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

// ------------------------------------------------------------ tab model

export type TabKind = "folder" | "file" | "workspace";

/** What the UI shows for one workspace tab. */
export interface TabView {
  roots: RootView[];
  workspace: WorkspaceView | null;
  /** Key for the saved session (document tabs, layout) of this tab. */
  sessionKey: string;
}

/** Per-tab UI state, stored while the tab is in the background. */
interface TabMemory {
  tabs: string[];
  active: string | null;
  recent: string[];
  expanded: Set<string>;
  selected: string | null;
  filter: string;
  viewMode: ViewMode;
}

export interface WorkspaceTab {
  id: number;
  kind: TabKind;
  /** What was opened: a folder, a file or a workspace file. */
  path: string;
  view: TabView;
  memory: TabMemory | null;
}

const [wsTabs, setWsTabs] = createSignal<WorkspaceTab[]>([]);
const [activeTabId, setActiveTabId] = createSignal<number | null>(null);
let nextTabId = 1;

export { wsTabs as workspaceTabs, activeTabId };
export const currentTab = () => wsTabs().find((t) => t.id === activeTabId()) ?? null;
/** The current tab's roots and workspace (null when nothing is open). */
export const opened = (): TabView | null => currentTab()?.view ?? null;

export function tabName(tab: WorkspaceTab): string {
  if (tab.view.workspace) return tab.view.workspace.name;
  if (tab.view.roots.length === 1) return tab.view.roots[0].name;
  if (tab.view.roots.length > 1) return `${tab.view.roots[0].name} +${tab.view.roots.length - 1}`;
  return baseName(tab.path);
}

const nodeMaps = new Map<number, Map<string, TreeNode>>();
const [treeVersion, setTreeVersion] = createSignal(0);
/** Changes whenever any root's nodes change. */
export const treeVersionSignal = treeVersion;
export const [scanning, setScanning] = createSignal<number[]>([]);
export const [expanded, setExpanded] = createSignal<Set<string>>(new Set());
export const [selectedKey, setSelectedKey] = createSignal<string | null>(null);
export const [treeFilter, setTreeFilter] = createSignal("");
export const [lastScanMs, setLastScanMs] = createSignal<number | null>(null);

/** Roots of the current tab. */
export const roots = (): RootView[] => opened()?.roots ?? [];
export const rootIds = (): number[] => roots().map((r) => r.id);
export const rootById = (id: number) => roots().find((r) => r.id === id);
/** A root in any tab (events arrive for background tabs too). */
const anyRootById = (id: number) => {
  for (const t of wsTabs()) {
    const r = t.view.roots.find((x) => x.id === id);
    if (r) return r;
  }
  return undefined;
};

/** Root inputs for the tree; tracks changes to the node maps. */
export function rootInputs(): RootInput[] {
  treeVersion();
  return roots().map((r) => ({ id: r.id, name: r.name, nodes: nodeMaps.get(r.id) ?? new Map() }));
}

export function nodesOf(rootId: number): Map<string, TreeNode> {
  treeVersion();
  return nodeMaps.get(rootId) ?? new Map();
}

/** Documents of the current tab as absolute paths with titles. */
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

/** The current-tab root containing an absolute path, and the `/` path within it. */
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

// ------------------------------------------------- switching and memory

function snapshot(): TabMemory {
  return {
    tabs: tabs(),
    active: activePath(),
    recent: recentDocs(),
    expanded: expanded(),
    selected: selectedKey(),
    filter: treeFilter(),
    viewMode: viewMode(),
  };
}

function apply(memory: TabMemory | null) {
  batch(() => {
    setTabs(memory?.tabs ?? []);
    setActivePath(memory?.active ?? null);
    setRecentDocs(memory?.recent ?? []);
    setExpanded(memory?.expanded ?? new Set<string>());
    setSelectedKey(memory?.selected ?? null);
    setTreeFilter(memory?.filter ?? "");
    if (memory) setViewMode(memory.viewMode);
  });
}

function updateTab(id: number, fn: (t: WorkspaceTab) => WorkspaceTab) {
  setWsTabs((list) => list.map((t) => (t.id === id ? fn(t) : t)));
}

/** Makes another workspace tab current, keeping each tab's state. */
export async function switchWorkspace(id: number) {
  const from = currentTab();
  if (from?.id === id) return;
  const to = wsTabs().find((t) => t.id === id);
  if (!to) return;
  if (from) {
    await persistSession();
    const memory = snapshot();
    updateTab(from.id, (t) => ({ ...t, memory }));
  }
  batch(() => {
    setActiveTabId(id);
    apply(to.memory);
  });
  updateTab(id, (t) => ({ ...t, memory: null }));
  bump();
  scheduleWindowSave();
}

/** Cycles through workspace tabs. */
export function cycleWorkspace(dir: 1 | -1) {
  const list = wsTabs();
  if (list.length < 2) return;
  const i = list.findIndex((t) => t.id === activeTabId());
  void switchWorkspace(list[(i + dir + list.length) % list.length].id);
}

// ----------------------------------------------------------- opening

function kindOf(result: ipc.OpenResult): TabKind {
  return result.workspace ? "workspace" : result.focus ? "file" : "folder";
}

/** A tab that already shows `path` (same folder, workspace file, or a file inside its roots). */
function tabShowing(path: string): { tab: WorkspaceTab; file: string | null } | null {
  for (const tab of wsTabs()) {
    if (samePath(tab.path, path)) return { tab, file: tab.kind === "file" ? path : null };
    if (tab.view.roots.length === 1 && tab.kind !== "file" && samePath(tab.view.roots[0].path, path)) return { tab, file: null };
    if (/\.(md|markdown|mdx)$/i.test(path)) {
      for (const r of tab.view.roots) {
        const rel = relativeTo(r.path, path);
        if (rel !== null && rel !== "" && (!r.singleFile || !rel.includes("/"))) return { tab, file: path };
      }
    }
  }
  return null;
}

/**
 * Opens a folder, Markdown file or workspace file in a new workspace tab,
 * or switches to the tab that already shows it.
 */
export async function openPathAction(path: string): Promise<boolean> {
  const existing = tabShowing(path);
  if (existing) {
    await switchWorkspace(existing.tab.id);
    if (existing.file) await openDoc(existing.file);
    return true;
  }
  try {
    const result = await ipc.openPath(path);
    await addTab(kindOf(result), path, result);
    return true;
  } catch (e) {
    toast(`Could not open ${path}: ${ipc.errorMessage(e)}`, "error");
    return false;
  }
}

async function addTab(kind: TabKind, path: string, result: ipc.OpenResult) {
  const from = currentTab();
  if (from) {
    await persistSession();
    const memory = snapshot();
    updateTab(from.id, (t) => ({ ...t, memory }));
  }
  const tab: WorkspaceTab = {
    id: nextTabId++,
    kind,
    path,
    view: { roots: result.roots, workspace: result.workspace, sessionKey: result.sessionKey },
    memory: null,
  };
  batch(() => {
    for (const r of result.roots) if (!nodeMaps.has(r.id)) nodeMaps.set(r.id, new Map());
    setWsTabs((list) => [...list, tab]);
    setActiveTabId(tab.id);
    setScanning((s) => [...s, ...result.roots.map((r) => r.id)]);
    apply(null);
  });
  bump();
  await afterOpen(tab, result.focus);
}

/** Restores the tab's saved session and focuses a file if asked. */
async function afterOpen(tab: WorkspaceTab, focus: string | null) {
  sessionReady = false;
  const restored = settings.restoreSession ? await restoreSession(tab.view) : false;
  if (focus) await openDoc(focus);
  if (!restored && !focus) setViewMode(settings.defaultView);
  sessionReady = true;
  scheduleWindowSave();
  void refreshGit();
}

/** Closes a workspace tab (asks about unsaved documents) and its roots. */
export async function closeWorkspace(id: number): Promise<boolean> {
  const tab = wsTabs().find((t) => t.id === id);
  if (!tab) return true;
  if (activeTabId() !== id) await switchWorkspace(id);
  await persistSession();
  if (!(await closeAll())) return false;
  const ids = tab.view.roots.map((r) => r.id);
  try {
    await ipc.removeRoots(ids);
  } catch {
    // The roots are gone from the UI either way.
  }
  for (const rid of ids) nodeMaps.delete(rid);
  const list = wsTabs();
  const index = list.findIndex((t) => t.id === id);
  const rest = list.filter((t) => t.id !== id);
  const next = rest[Math.min(index, rest.length - 1)] ?? null;
  batch(() => {
    setWsTabs(rest);
    setActiveTabId(next?.id ?? null);
    apply(next?.memory ?? null);
  });
  if (next) updateTab(next.id, (t) => ({ ...t, memory: null }));
  bump();
  scheduleWindowSave();
  return true;
}

/** Closes every workspace tab except `keep`. */
export async function closeOtherWorkspaces(keep: number) {
  for (const t of [...wsTabs()]) {
    if (t.id !== keep && !(await closeWorkspace(t.id))) return;
  }
  await switchWorkspace(keep);
}

/** Adds a folder to the current workspace tab. */
export async function addRootAction(path: string) {
  const tab = currentTab();
  if (!tab) {
    await openPathAction(path);
    return;
  }
  try {
    const result = await ipc.addRoot(path, rootIds());
    const added = result.roots.filter((r) => !tab.view.roots.some((x) => x.id === r.id));
    for (const r of added) nodeMaps.set(r.id, new Map());
    updateTab(tab.id, (t) => ({
      ...t,
      // Single-file tabs become full folders once another folder joins.
      kind: t.kind === "file" ? "folder" : t.kind,
      view: { ...t.view, roots: [...t.view.roots.map((r) => ({ ...r, singleFile: false })), ...added] },
    }));
    setScanning((s) => [...s, ...added.map((r) => r.id)]);
    bump();
    scheduleWindowSave();
  } catch (e) {
    toast(`Could not add folder: ${ipc.errorMessage(e)}`, "error");
  }
}

/** Removes one folder from the current workspace tab. */
export async function removeRootAction(id: number) {
  const tab = currentTab();
  const root = rootById(id);
  if (!tab || !root) return;
  if (tab.view.roots.length === 1) {
    await closeWorkspace(tab.id);
    return;
  }
  for (const path of [...tabs()]) {
    if (relativeTo(root.path, path) !== null && !(await closeTab(path))) return;
  }
  await ipc.removeRoots([id]);
  nodeMaps.delete(id);
  updateTab(tab.id, (t) => ({ ...t, view: { ...t.view, roots: t.view.roots.filter((r) => r.id !== id) } }));
  bump();
  scheduleWindowSave();
}

export async function saveWorkspaceAs(name: string): Promise<boolean> {
  const tab = currentTab();
  if (!tab) return false;
  const suggested = name.replace(/[\\/:*?"<>|]/g, "-") + ".markdraft-workspace";
  const file = await ipc.pickSavePath("workspace", suggested);
  if (!file) return false;
  try {
    const result = await ipc.saveWorkspace(file, name, rootIds());
    updateTab(tab.id, (t) => ({
      ...t,
      kind: "workspace",
      path: file,
      view: { ...t.view, workspace: result.workspace },
    }));
    scheduleWindowSave();
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
      if (!anyRootById(b.rootId)) return;
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
      const map = nodeMaps.get(c.rootId);
      const root = anyRootById(c.rootId);
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

// ------------------------------------------------ window (all tabs)

interface WindowTab {
  kind: TabKind;
  path: string;
  /** Folders added to the tab after it was opened. */
  extra: string[];
}

interface WindowData {
  tabs: WindowTab[];
  active: number;
}

function windowData(): WindowData {
  const list = wsTabs();
  return {
    tabs: list.map((t) => ({
      kind: t.kind,
      path: t.path,
      extra: t.kind === "workspace" ? [] : t.view.roots.slice(1).map((r) => r.path),
    })),
    active: Math.max(0, list.findIndex((t) => t.id === activeTabId())),
  };
}

let windowTimer: ReturnType<typeof setTimeout> | undefined;
let windowReady = false;
function scheduleWindowSave() {
  if (!windowReady || !ipc.inTauri()) return;
  clearTimeout(windowTimer);
  windowTimer = setTimeout(() => void ipc.setWindowState(windowData()).catch(() => {}), 400);
}

/**
 * Reopens the workspace tabs from the last run, then `extra` (a path from
 * the command line or the OS) as a new or existing tab.
 */
export async function restoreWindow(extra: string | null) {
  const saved = settings.restoreSession ? ((await ipc.getWindowState().catch(() => null)) as WindowData | null) : null;
  const opened: number[] = [];
  for (const t of saved?.tabs ?? []) {
    if (tabShowing(t.path) || !(await openQuiet(t.path))) continue;
    for (const folder of t.extra) await addRootAction(folder);
    opened.push(activeTabId()!);
  }
  const active = saved ? opened[Math.min(saved.active, opened.length - 1)] : undefined;
  if (active !== undefined) await switchWorkspace(active);
  windowReady = true;
  if (extra) await openPathAction(extra);
  scheduleWindowSave();
}

/** Opens a saved tab without error toasts for folders that are gone. */
async function openQuiet(path: string): Promise<boolean> {
  try {
    const result = await ipc.openPath(path);
    await addTab(kindOf(result), path, result);
    return true;
  } catch {
    return false;
  }
}

/** After a UI reload, rebuilds tabs from what Rust still has open. */
export async function reattach(): Promise<boolean> {
  let all: ipc.AllRoots;
  try {
    all = await ipc.currentRoots();
  } catch {
    return false;
  }
  if (!all.roots.length) return false;
  const saved = (await ipc.getWindowState().catch(() => null)) as WindowData | null;
  const unused = new Map(all.roots.map((r) => [r.id, r]));
  const take = (path: string) => {
    for (const [id, r] of unused) {
      if (samePath(r.path, path)) {
        unused.delete(id);
        return r;
      }
    }
    return null;
  };
  const groups: { kind: TabKind; path: string; roots: RootView[]; workspace: WorkspaceView | null }[] = [];
  for (const t of saved?.tabs ?? []) {
    const ws = t.kind === "workspace" ? all.workspaces.find((w) => samePath(w.file, t.path)) : undefined;
    let members: RootView[];
    if (ws) {
      members = ws.rootIds.map((id) => unused.get(id)).filter((r): r is RootView => !!r);
      for (const r of members) unused.delete(r.id);
    } else {
      const first = take(t.kind === "file" ? t.path.replace(/[\\/][^\\/]*$/, "") : t.path);
      members = [first, ...t.extra.map(take)].filter((r): r is RootView => !!r);
    }
    if (members.length) groups.push({ kind: t.kind, path: t.path, roots: members, workspace: ws ?? null });
  }
  for (const r of unused.values()) groups.push({ kind: "folder", path: r.path, roots: [r], workspace: null });
  for (const g of groups) {
    const result: ipc.OpenResult = { roots: g.roots, focus: null, workspace: g.workspace, sessionKey: "" };
    result.sessionKey = await sessionKeyFor(g.roots);
    await addTab(g.kind, g.path, result);
    for (const r of g.roots) void ipc.rescanRoot(r.id);
  }
  const active = wsTabs()[saved ? Math.min(saved.active, wsTabs().length - 1) : 0];
  if (active) await switchWorkspace(active.id);
  windowReady = true;
  return true;
}

/** Same key Rust computes: FNV-1a over the sorted, lower-cased root paths. */
async function sessionKeyFor(list: RootView[]): Promise<string> {
  const joined = list
    .map((r) => r.path.toLowerCase())
    .sort()
    .join("\n");
  let h = 0xcbf29ce484222325n;
  for (const b of new TextEncoder().encode(joined)) {
    h ^= BigInt(b);
    h = (h * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return h.toString(16).padStart(16, "0");
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

async function restoreSession(view: TabView): Promise<boolean> {
  let data: Partial<SessionData> | null = null;
  try {
    data = (await ipc.getSession(view.sessionKey)) as Partial<SessionData> | null;
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

/** Restores a document tab without error toasts for files that no longer exist. */
async function openDocQuiet(path: string) {
  try {
    await ipc.readFile(path);
  } catch {
    return;
  }
  await openDoc(path);
}

/** Saves the current tab's document tabs and layout. */
export async function persistSession() {
  const o = opened();
  if (!o || !sessionReady || !ipc.inTauri() || !o.sessionKey) return;
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

/** Whether anything below a folder has uncommitted changes. */
export function gitFolderHasChanges(path: string): boolean {
  const prefix = gitKey(path) + "/";
  return Object.keys(gitState.status).some((k) => k.startsWith(prefix));
}

/** Repositories of the current tab's roots. */
export const currentGitRoots = () => gitState.roots.filter((g) => roots().some((r) => samePath(r.path, g.root)));

let gitTimer: ReturnType<typeof setTimeout> | undefined;
export function scheduleGitRefresh() {
  clearTimeout(gitTimer);
  gitTimer = setTimeout(() => void refreshGit(), 700);
}

export async function refreshGit() {
  if (!ipc.inTauri() || !wsTabs().length) return;
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
