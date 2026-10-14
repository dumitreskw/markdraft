// Every app action in one place: used by keyboard shortcuts, the command
// palette, menus and toolbar buttons.
import { editorView, focusEditor } from "./components/EditorPane";
import { openDiff, openHistory } from "./components/DiffView";
import { command, formatTable, insertCodeBlock, insertLink, insertTable, setHeading, toggleList, toggleWrap } from "./editor/format";
import * as ipc from "./ipc";
import { dirName } from "./paths";
import { closeDiff, diffRequest } from "./state/diff";
import { activeDoc, activePath, closeTab, openDoc, reloadFromDisk, saveAll, saveDoc, setActivePath, tabs } from "./state/docs";
import { effectiveTheme, settings, updateSettings } from "./state/settings";
import {
  focusMode,
  isMac,
  outlineVisible,
  prompt,
  setFocusMode,
  setModal,
  setOutlineVisible,
  setSidebarTab,
  setSidebarVisible,
  setViewMode,
  sidebarVisible,
  toast,
  viewMode,
} from "./state/ui";
import { addRootAction, gitStatusOf, opened, openPathAction, roots } from "./state/workspace";

export interface Command {
  id: string;
  title: string;
  category: string;
  /** Key binding like "Mod+Shift+P" (Mod = Cmd on macOS, Ctrl elsewhere). */
  keys?: string;
  /** Only shown/enabled when this returns true. */
  when?: () => boolean;
  run: () => void | Promise<unknown>;
}

const hasDoc = () => !!activePath();
const hasRoots = () => roots().length > 0;
const editorCmd = (fn: Parameters<typeof command>[0]) => () => {
  const view = editorView();
  if (!view) return;
  if (viewMode() === "read") setViewMode("split");
  command(fn)(view);
  view.focus();
};

async function openFolder() {
  const path = await ipc.pickPath("folder");
  if (path) await openPathAction(path);
}

async function newDocument() {
  const doc = activePath();
  const folder = doc ? dirName(doc) : roots()[0]?.path;
  if (!folder) {
    toast("Open a folder first.", "info");
    return;
  }
  const name = await prompt("New document", "Document name", "Untitled");
  if (!name) return;
  try {
    const path = await ipc.createEntry(folder, name, false);
    await openDoc(path);
    if (viewMode() === "read") setViewMode("split");
    setTimeout(focusEditor, 50);
  } catch (e) {
    toast(ipc.errorMessage(e), "error");
  }
}

function cycleTab(dir: 1 | -1) {
  const list = tabs();
  if (list.length < 2) return;
  const i = list.indexOf(activePath() ?? "");
  void openDoc(list[(i + dir + list.length) % list.length]);
}

export const commands: Command[] = [
  // File
  { id: "open-folder", title: "Open Folder…", category: "File", keys: "Mod+O", run: openFolder },
  {
    id: "open-file",
    title: "Open File…",
    category: "File",
    keys: "Mod+Shift+O",
    run: async () => {
      const path = await ipc.pickPath("file");
      if (path) await openPathAction(path);
    },
  },
  {
    id: "open-workspace",
    title: "Open Workspace…",
    category: "File",
    run: async () => {
      const path = await ipc.pickPath("workspace");
      if (path) await openPathAction(path);
    },
  },
  {
    id: "add-folder",
    title: "Add Folder to Workspace…",
    category: "File",
    when: hasRoots,
    run: async () => {
      const path = await ipc.pickPath("folder");
      if (path) await addRootAction(path);
    },
  },
  { id: "save-workspace", title: "Save Workspace As…", category: "File", when: hasRoots, run: () => setModal({ kind: "saveWorkspace" }) },
  { id: "new-document", title: "New Document…", category: "File", keys: "Mod+N", when: hasRoots, run: newDocument },
  { id: "save", title: "Save", category: "File", keys: "Mod+S", when: hasDoc, run: () => saveDoc(activePath()!, true) },
  { id: "save-all", title: "Save All", category: "File", keys: "Mod+Alt+S", when: hasDoc, run: saveAll },
  { id: "close-tab", title: "Close Tab", category: "File", keys: "Mod+W", when: hasDoc, run: () => closeTab(activePath()!) },
  { id: "reload", title: "Reload from Disk", category: "File", when: hasDoc, run: () => reloadFromDisk(activePath()!) },
  { id: "export-pdf", title: "Export to PDF…", category: "File", keys: "Mod+Shift+E", when: hasDoc, run: () => setModal({ kind: "export", path: activePath()! }) },
  {
    id: "reveal",
    title: "Reveal in File Manager",
    category: "File",
    when: hasDoc,
    run: () => ipc.revealPath(activePath()!).catch((e) => toast(ipc.errorMessage(e), "error")),
  },

  // Go
  { id: "quick-open", title: "Go to Document…", category: "Go", keys: "Mod+P", when: hasRoots, run: () => setModal({ kind: "quickOpen" }) },
  { id: "palette", title: "Show All Commands", category: "Go", keys: "Mod+Shift+P", run: () => setModal({ kind: "quickOpen", commands: true }) },
  { id: "next-tab", title: "Next Tab", category: "Go", keys: "Ctrl+Tab", when: hasDoc, run: () => cycleTab(1) },
  { id: "prev-tab", title: "Previous Tab", category: "Go", keys: "Ctrl+Shift+Tab", when: hasDoc, run: () => cycleTab(-1) },
  {
    id: "search",
    title: "Search in Files",
    category: "Go",
    keys: "Mod+Shift+F",
    when: hasRoots,
    run: () => {
      setSidebarVisible(true);
      setSidebarTab("search");
      setTimeout(() => document.querySelector<HTMLInputElement>("#search-input")?.focus(), 30);
    },
  },
  {
    id: "show-files",
    title: "Show Files",
    category: "Go",
    keys: "Mod+Shift+X",
    when: hasRoots,
    run: () => {
      setSidebarVisible(true);
      setSidebarTab("files");
      setTimeout(() => document.querySelector<HTMLElement>(".tree")?.focus(), 30);
    },
  },

  // View
  { id: "view-read", title: "Reading View", category: "View", keys: "Mod+1", run: () => setViewMode("read") },
  { id: "view-edit", title: "Editing View", category: "View", keys: "Mod+2", run: () => setViewMode("edit") },
  { id: "view-split", title: "Split View", category: "View", keys: "Mod+3", run: () => setViewMode("split") },
  { id: "toggle-sidebar", title: "Toggle Sidebar", category: "View", keys: "Mod+\\", run: () => setSidebarVisible(!sidebarVisible()) },
  { id: "toggle-outline", title: "Toggle Outline", category: "View", keys: "Mod+Shift+\\", run: () => setOutlineVisible(!outlineVisible()) },
  { id: "focus-mode", title: "Toggle Focus Mode", category: "View", keys: "F11", run: () => setFocusMode(!focusMode()) },
  {
    id: "toggle-theme",
    title: "Toggle Light/Dark Theme",
    category: "View",
    run: () => updateSettings({ theme: effectiveTheme() === "dark" ? "light" : "dark" }),
  },
  { id: "titles", title: "Toggle Titles / File Names in Tree", category: "View", run: () => updateSettings({ showTitles: !settings.showTitles }) },
  { id: "line-numbers", title: "Toggle Line Numbers", category: "View", run: () => updateSettings({ lineNumbers: !settings.lineNumbers }) },
  { id: "zoom-in", title: "Increase Editor Font Size", category: "View", keys: "Mod+=", run: () => updateSettings({ editorFontSize: Math.min(28, settings.editorFontSize + 1) }) },
  { id: "zoom-out", title: "Decrease Editor Font Size", category: "View", keys: "Mod+-", run: () => updateSettings({ editorFontSize: Math.max(10, settings.editorFontSize - 1) }) },

  // Format (editor)
  { id: "bold", title: "Bold", category: "Format", keys: "Mod+B", when: hasDoc, run: editorCmd((s) => toggleWrap(s, "**")) },
  { id: "italic", title: "Italic", category: "Format", keys: "Mod+I", when: hasDoc, run: editorCmd((s) => toggleWrap(s, "*")) },
  { id: "strike", title: "Strikethrough", category: "Format", when: hasDoc, run: editorCmd((s) => toggleWrap(s, "~~")) },
  { id: "code", title: "Inline Code", category: "Format", when: hasDoc, run: editorCmd((s) => toggleWrap(s, "`", "code")) },
  { id: "link", title: "Insert Link", category: "Format", keys: "Mod+K", when: hasDoc, run: editorCmd(insertLink) },
  { id: "h1", title: "Heading 1", category: "Format", when: hasDoc, run: editorCmd((s) => setHeading(s, 1)) },
  { id: "h2", title: "Heading 2", category: "Format", when: hasDoc, run: editorCmd((s) => setHeading(s, 2)) },
  { id: "h3", title: "Heading 3", category: "Format", when: hasDoc, run: editorCmd((s) => setHeading(s, 3)) },
  { id: "bullets", title: "Bulleted List", category: "Format", when: hasDoc, run: editorCmd((s) => toggleList(s, "bullet")) },
  { id: "numbers", title: "Numbered List", category: "Format", when: hasDoc, run: editorCmd((s) => toggleList(s, "ordered")) },
  { id: "tasks", title: "Task List", category: "Format", when: hasDoc, run: editorCmd((s) => toggleList(s, "task")) },
  { id: "table", title: "Insert Table", category: "Format", when: hasDoc, run: editorCmd(insertTable) },
  { id: "format-table", title: "Format Table", category: "Format", keys: "Mod+Alt+F", when: hasDoc, run: editorCmd(formatTable) },
  { id: "code-block", title: "Insert Code Block", category: "Format", when: hasDoc, run: editorCmd(insertCodeBlock) },

  // Git
  {
    id: "git-changes",
    title: "Show Changed Documents",
    category: "Git",
    keys: "Mod+Shift+G",
    when: hasRoots,
    run: () => {
      setSidebarVisible(true);
      setSidebarTab("changes");
    },
  },
  {
    id: "git-diff",
    title: "Compare with Last Commit",
    category: "Git",
    keys: "Mod+Alt+D",
    when: () => hasDoc() && !!gitStatusOf(activePath()!) && gitStatusOf(activePath()!)!.kind !== "untracked",
    run: () => {
      const doc = activeDoc()!;
      openDiff(doc.path, { kind: "head" }, doc.text !== doc.savedText ? { kind: "text", text: doc.text } : { kind: "workdir" });
    },
  },
  { id: "git-history", title: "Show File History", category: "Git", keys: "Mod+Alt+H", when: hasDoc, run: () => openHistory(activePath()!) },
  {
    id: "compare-disk",
    title: "Compare Unsaved Edits with Disk",
    category: "Git",
    when: () => hasDoc() && activeDoc()!.text !== activeDoc()!.savedText,
    run: () => {
      const doc = activeDoc()!;
      openDiff(doc.path, { kind: "workdir" }, { kind: "text", text: doc.text }, { old: "On disk", new: "Your edits" });
    },
  },
  { id: "close-diff", title: "Close Changes View", category: "Git", when: () => !!diffRequest(), run: closeDiff },

  // App
  { id: "settings", title: "Settings", category: "App", keys: "Mod+,", run: () => setModal({ kind: "settings" }) },
  { id: "about", title: "About MarkDraft", category: "App", run: () => setModal({ kind: "about" }) },
  {
    id: "close-folder",
    title: "Close Folder",
    category: "File",
    when: hasRoots,
    run: async () => {
      const { closeAll } = await import("./state/docs");
      const { persistSession, setOpened } = await import("./state/workspace");
      await persistSession();
      if (await closeAll()) {
        setOpened(null);
        setActivePath(null);
      }
    },
  },
];

// -------------------------------------------------------- key handling

interface Binding {
  mod: boolean;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  key: string;
}

export function parseKeys(keys: string): Binding {
  const parts = keys.split("+");
  // "Mod+=" / "Mod+-" / "Mod++" keep their final symbol.
  const key = parts.pop() || "+";
  const set = new Set(parts.map((p) => p.toLowerCase()));
  return { mod: set.has("mod"), ctrl: set.has("ctrl"), shift: set.has("shift"), alt: set.has("alt"), key: key.toLowerCase() };
}

export function matches(binding: Binding, e: KeyboardEvent, mac = isMac): boolean {
  const primary = mac ? e.metaKey : e.ctrlKey;
  const wantCtrl = binding.ctrl || (!mac && binding.mod);
  const wantMeta = mac && binding.mod;
  if (wantCtrl !== e.ctrlKey || wantMeta !== e.metaKey || binding.shift !== e.shiftKey || binding.alt !== e.altKey) return false;
  if (binding.mod && !primary) return false;
  const key = e.key.toLowerCase();
  // Shifted digits and symbols vary by layout; fall back to the physical key.
  const code = e.code.replace(/^(Key|Digit)/, "").toLowerCase();
  return key === binding.key || code === binding.key || (binding.key === "\\" && e.code === "Backslash") || (binding.key === "=" && key === "+");
}

/** Display form of a binding: "⌘⇧P" on macOS, "Ctrl+Shift+P" elsewhere. */
export function displayKeys(keys: string, mac = isMac): string {
  const b = parseKeys(keys);
  const key = b.key.length === 1 ? b.key.toUpperCase() : b.key[0].toUpperCase() + b.key.slice(1);
  if (mac) return (b.ctrl ? "⌃" : "") + (b.alt ? "⌥" : "") + (b.shift ? "⇧" : "") + (b.mod ? "⌘" : "") + key;
  return [b.ctrl || b.mod ? "Ctrl" : "", b.alt ? "Alt" : "", b.shift ? "Shift" : "", key].filter(Boolean).join("+");
}

const bindings = commands.filter((c) => c.keys).map((c) => ({ command: c, binding: parseKeys(c.keys!) }));

/** Editor-owned shortcuts that should not trigger app commands while typing. */
const EDITOR_KEYS = new Set(["bold", "italic", "link"]);

export function handleGlobalKey(e: KeyboardEvent): boolean {
  const inEditor = !!(e.target as HTMLElement | null)?.closest?.(".cm-editor");
  for (const { command: c, binding } of bindings) {
    if (!matches(binding, e)) continue;
    if (inEditor && EDITOR_KEYS.has(c.id)) return false;
    if (c.when && !c.when()) return false;
    e.preventDefault();
    void c.run();
    return true;
  }
  return false;
}

export const isOpen = () => !!opened();
