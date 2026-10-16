// Open documents and tabs: loading, editing, saving, autosave, rendering
// and reacting to changes made outside the app.
import { createSignal } from "solid-js";
import { createStore, produce } from "solid-js/store";
import * as ipc from "../ipc";
import type { Change, DocFormat, RenderedDoc } from "../ipc";
import { baseName, joinPath, samePath } from "../paths";
import { settings } from "./settings";
import { confirm, toast } from "./ui";

export type DocStatus = "clean" | "dirty" | "saving" | "conflict" | "error";

export interface DocState {
  path: string;
  name: string;
  text: string;
  /** Text as last loaded from or saved to disk. */
  savedText: string;
  format: DocFormat;
  modifiedMs: number;
  readonly: boolean;
  status: DocStatus;
  /** Set when the file changed or vanished on disk while it had unsaved edits. */
  external: null | "changed" | "deleted";
  rendered: RenderedDoc | null;
  error: string | null;
}

export const [docs, setDocs] = createStore<Record<string, DocState>>({});
export const [tabs, setTabs] = createSignal<string[]>([]);
export const [activePath, setActivePath] = createSignal<string | null>(null);
/** Most recently used first, for quick open with an empty query. */
export const [recentDocs, setRecentDocs] = createSignal<string[]>([]);

export const activeDoc = () => {
  const p = activePath();
  return p ? docs[p] : undefined;
};

// Editor bridge: the editor applies text replaced from outside (reloads)
// as an undoable transaction, and reveals requested lines.
type ReplaceListener = (path: string, text: string) => void;
const replaceListeners = new Set<ReplaceListener>();
export function onExternalReplace(fn: ReplaceListener) {
  replaceListeners.add(fn);
  return () => replaceListeners.delete(fn);
}

export interface RevealRequest {
  path: string;
  line?: number;
  fragment?: string;
  seq: number;
}
export const [revealRequest, setRevealRequest] = createSignal<RevealRequest | null>(null);
let revealSeq = 0;
export function reveal(path: string, target: { line?: number; fragment?: string }) {
  setRevealRequest({ path, ...target, seq: ++revealSeq });
}

function findOpen(path: string): string | undefined {
  return tabs().find((t) => samePath(t, path));
}

/** Documents being read from disk, so a second request waits for the first. */
const loading = new Map<string, Promise<boolean>>();

/** Opens a document in a tab (or focuses its tab) and makes it active. */
export async function openDoc(path: string, target?: { line?: number; fragment?: string }): Promise<boolean> {
  const existing = findOpen(path);
  if (existing) {
    activate(existing);
    if (target) reveal(existing, target);
    return true;
  }
  const key = [...loading.keys()].find((p) => samePath(p, path));
  if (key) {
    if (!(await loading.get(key))) return false;
    return openDoc(path, target);
  }
  const load = loadDoc(path, target).finally(() => loading.delete(path));
  loading.set(path, load);
  return load;
}

async function loadDoc(path: string, target?: { line?: number; fragment?: string }): Promise<boolean> {
  try {
    const loaded = await ipc.readFile(path);
    setDocs(path, {
      path,
      name: baseName(path),
      text: loaded.text,
      savedText: loaded.text,
      format: loaded.format,
      modifiedMs: loaded.modifiedMs,
      readonly: loaded.readonly,
      status: "clean",
      external: null,
      rendered: null,
      error: null,
    });
    setTabs((t) => {
      if (t.some((p) => samePath(p, path))) return t;
      const at = activePath() ? t.indexOf(activePath()!) + 1 : t.length;
      return [...t.slice(0, at), path, ...t.slice(at)];
    });
    activate(path);
    void renderNow(path);
    if (target) reveal(path, target);
    return true;
  } catch (e) {
    toast(`Could not open ${baseName(path)}: ${ipc.errorMessage(e)}`, "error");
    return false;
  }
}

export function activate(path: string) {
  const previous = activePath();
  if (previous && previous !== path && docs[previous]?.status === "dirty" && settings.autosave) {
    void saveDoc(previous);
  }
  setActivePath(path);
  setRecentDocs((r) => [path, ...r.filter((p) => p !== path)].slice(0, 30));
}

/** Closes a tab; asks first when it has unsaved changes. */
export async function closeTab(path: string): Promise<boolean> {
  const doc = docs[path];
  if (doc && (doc.status === "dirty" || doc.status === "conflict" || doc.status === "error")) {
    if (settings.autosave && doc.status === "dirty" && (await saveDoc(path))) {
      // Saved; close below.
    } else {
      const choice = await confirm(`Save changes to ${doc.name}?`, "Your changes will be lost if you don't save them.", [
        { label: "Don't save", value: "discard", danger: true },
        { label: "Cancel", value: "cancel" },
        { label: "Save", value: "save", primary: true },
      ]);
      if (choice === null || choice === "cancel") return false;
      if (choice === "save" && !(await saveDoc(path))) return false;
    }
  }
  const list = tabs();
  const i = list.indexOf(path);
  const next = list.filter((p) => p !== path);
  setTabs(next);
  if (activePath() === path) {
    const recent = recentDocs().find((p) => p !== path && next.includes(p));
    setActivePath(recent ?? next[Math.min(i, next.length - 1)] ?? null);
  }
  setRecentDocs((r) => r.filter((p) => p !== path));
  setDocs(produce((d) => delete d[path]));
  editorStateDrop(path);
  return true;
}

/** Closes every tab; false if the user cancelled. */
export async function closeAll(): Promise<boolean> {
  for (const path of [...tabs()]) {
    if (!(await closeTab(path))) return false;
  }
  return true;
}

let dropEditorState: (path: string) => void = () => {};
export function registerEditorStateDrop(fn: (path: string) => void) {
  dropEditorState = fn;
}
function editorStateDrop(path: string) {
  dropEditorState(path);
}

// ------------------------------------------------------------- editing

const autosaveTimers = new Map<string, ReturnType<typeof setTimeout>>();
const renderTimers = new Map<string, ReturnType<typeof setTimeout>>();

/** Called by the editor on every change. */
export function setText(path: string, text: string) {
  const doc = docs[path];
  if (!doc) return;
  const dirty = text !== doc.savedText;
  setDocs(path, {
    text,
    status: doc.status === "conflict" ? "conflict" : dirty ? "dirty" : "clean",
  });
  scheduleRender(path);
  if (dirty && settings.autosave && !doc.external && doc.status !== "conflict") {
    clearTimeout(autosaveTimers.get(path));
    autosaveTimers.set(
      path,
      setTimeout(() => void saveDoc(path), settings.autosaveDelayMs),
    );
  }
}

function scheduleRender(path: string) {
  clearTimeout(renderTimers.get(path));
  renderTimers.set(
    path,
    setTimeout(() => void renderNow(path), 120),
  );
}

let renderSeq = 0;
const latestRender = new Map<string, number>();

export async function renderNow(path: string) {
  const doc = docs[path];
  if (!doc) return;
  const seq = ++renderSeq;
  latestRender.set(path, seq);
  try {
    const rendered = await ipc.renderMarkdown(doc.text, path, settings.smartPunctuation);
    // Ignore results that arrive after a newer render was requested.
    if (latestRender.get(path) === seq && docs[path]) setDocs(path, "rendered", rendered);
  } catch (e) {
    if (docs[path]) setDocs(path, "error", ipc.errorMessage(e));
  }
}

const saving = new Map<string, Promise<boolean>>();

let afterSave: (path: string) => void = () => {};
/** Registers a callback run after every successful save (e.g. Git refresh). */
export function setAfterSave(fn: (path: string) => void) {
  afterSave = fn;
}

/**
 * Saves a document. With `force`, overwrites even if the file changed on
 * disk. Returns true when the file on disk now matches the editor.
 */
export function saveDoc(path: string, force = false): Promise<boolean> {
  const inflight = saving.get(path);
  if (inflight) return inflight.then(() => saveDoc(path, force));
  const run = doSave(path, force).finally(() => saving.delete(path));
  saving.set(path, run);
  return run;
}

async function doSave(path: string, force: boolean): Promise<boolean> {
  clearTimeout(autosaveTimers.get(path));
  const doc = docs[path];
  if (!doc) return true;
  if (doc.readonly) {
    toast(`${doc.name} is read-only.`, "error");
    return false;
  }
  if (!force && (doc.status === "clean" || doc.status === "conflict")) return doc.status === "clean";
  const text = doc.text;
  setDocs(path, "status", "saving");
  try {
    const outcome = await ipc.saveFile(path, text, doc.format, force || doc.external === "deleted" ? null : doc.modifiedMs);
    if (!docs[path]) return true;
    if (outcome.status === "saved") {
      afterSave(path);
      const stillDirty = docs[path].text !== text;
      setDocs(path, {
        savedText: text,
        modifiedMs: outcome.modifiedMs,
        status: stillDirty ? "dirty" : "clean",
        external: null,
        error: null,
      });
      return !stillDirty;
    }
    setDocs(path, { status: "conflict", external: "changed" });
    return false;
  } catch (e) {
    if (docs[path]) setDocs(path, { status: "error", error: ipc.errorMessage(e) });
    toast(`Could not save ${doc.name}: ${ipc.errorMessage(e)}`, "error");
    return false;
  }
}

export async function saveAll(): Promise<boolean> {
  let ok = true;
  for (const path of tabs()) {
    const s = docs[path]?.status;
    if (s === "dirty" || s === "error") ok = (await saveDoc(path)) && ok;
  }
  return ok;
}

/** Replaces the editor text with the file on disk (undoable in the editor). */
export async function reloadFromDisk(path: string) {
  try {
    const loaded = await ipc.readFile(path);
    if (!docs[path]) return;
    setDocs(path, {
      savedText: loaded.text,
      modifiedMs: loaded.modifiedMs,
      format: loaded.format,
      readonly: loaded.readonly,
      external: null,
      error: null,
    });
    for (const fn of replaceListeners) fn(path, loaded.text);
    setText(path, loaded.text);
  } catch (e) {
    toast(`Could not reload: ${ipc.errorMessage(e)}`, "error");
  }
}

/** Keeps the editor's text and overwrites the file on disk. */
export function keepMine(path: string) {
  setDocs(path, { external: null, status: "dirty" });
  return saveDoc(path, true);
}

// --------------------------------------------- changes from outside the app

/** Reacts to watcher changes affecting open documents. */
export async function handleFsChanges(rootPath: string, changes: Change[]) {
  for (const change of changes) {
    switch (change.kind) {
      case "modified": {
        const path = findOpen(joinPath(rootPath, change.node.path));
        if (path) await externalModify(path);
        break;
      }
      case "removed": {
        const removed = joinPath(rootPath, change.path);
        for (const path of tabs()) {
          if (samePath(path, removed) || isInside(removed, path)) {
            // Editors often save by delete + create; check before warning.
            setTimeout(() => void confirmDeleted(path), 400);
          }
        }
        break;
      }
      case "renamed": {
        const from = joinPath(rootPath, change.from);
        const to = joinPath(rootPath, change.to.path);
        for (const path of tabs()) {
          if (samePath(path, from)) rekey(path, to);
          else if (isInside(from, path)) rekey(path, to + path.slice(from.length));
        }
        break;
      }
      case "added": {
        // A file deleted earlier came back (delete + create save).
        const path = findOpen(joinPath(rootPath, change.node.path));
        if (path && docs[path]?.external === "deleted") await externalModify(path);
        break;
      }
      default:
        break;
    }
  }
}

function isInside(dir: string, path: string) {
  const d = dir.replace(/\\/g, "/").toLowerCase();
  return path.replace(/\\/g, "/").toLowerCase().startsWith(d + "/");
}

async function externalModify(path: string) {
  const doc = docs[path];
  if (!doc) return;
  let loaded: ipc.LoadedDoc;
  try {
    loaded = await ipc.readFile(path);
  } catch {
    return;
  }
  const current = docs[path];
  if (!current) return;
  if (loaded.text === current.text) {
    // Same content (e.g. a sync tool touched it): adopt the new timestamp.
    setDocs(path, { savedText: loaded.text, modifiedMs: loaded.modifiedMs, status: "clean", external: null });
    return;
  }
  if (current.status === "clean") {
    setDocs(path, { savedText: loaded.text, modifiedMs: loaded.modifiedMs, format: loaded.format, external: null });
    for (const fn of replaceListeners) fn(path, loaded.text);
    setText(path, loaded.text);
    return;
  }
  clearTimeout(autosaveTimers.get(path));
  setDocs(path, { external: "changed", status: "conflict" });
}

async function confirmDeleted(path: string) {
  if (!docs[path]) return;
  try {
    await ipc.readFile(path);
    await externalModify(path);
  } catch {
    clearTimeout(autosaveTimers.get(path));
    setDocs(path, { external: "deleted", status: docs[path].text ? "dirty" : "clean" });
  }
}

function rekey(from: string, to: string) {
  const doc = docs[from];
  if (!doc) return;
  setDocs(to, { ...doc, path: to, name: baseName(to) });
  setDocs(produce((d) => delete d[from]));
  setTabs((t) => t.map((p) => (p === from ? to : p)));
  setRecentDocs((r) => r.map((p) => (p === from ? to : p)));
  if (activePath() === from) setActivePath(to);
  rekeyEditorState(from, to);
}

let rekeyEditorState: (from: string, to: string) => void = () => {};
export function registerEditorStateRekey(fn: (from: string, to: string) => void) {
  rekeyEditorState = fn;
}

/** Updates open tabs after the app itself renamed or moved a path. */
export function renamedByApp(from: string, to: string) {
  for (const path of tabs()) {
    if (samePath(path, from)) rekey(path, to);
    else if (isInside(from, path)) rekey(path, to + path.slice(from.length));
  }
}

/** Whether any open document has changes not yet on disk. */
export const hasUnsaved = () =>
  Object.values(docs).some((d) => ["dirty", "saving", "conflict", "error"].includes(d.status));
