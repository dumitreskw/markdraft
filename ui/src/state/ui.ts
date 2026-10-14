// Transient UI state: view mode, panels, dialogs and notifications.
import { createSignal } from "solid-js";
import type { ViewMode } from "./settings";

export const [viewMode, setViewMode] = createSignal<ViewMode>("split");
export const [sidebarVisible, setSidebarVisible] = createSignal(true);
export const [outlineVisible, setOutlineVisible] = createSignal(true);
export type SidebarTab = "files" | "search" | "changes";
export const [sidebarTab, setSidebarTab] = createSignal<SidebarTab>("files");
export const [focusMode, setFocusMode] = createSignal(false);
export const [treeWidth, setTreeWidth] = createSignal(280);
export const [outlineWidth, setOutlineWidth] = createSignal(240);
/** Fraction of the document area given to the editor in split view. */
export const [splitRatio, setSplitRatio] = createSignal(0.5);

export type Modal =
  | { kind: "quickOpen"; commands?: boolean }
  | { kind: "settings" }
  | { kind: "export"; path: string }
  | { kind: "saveWorkspace" }
  | { kind: "about" }
  | {
      kind: "confirm";
      title: string;
      message: string;
      actions: { label: string; value: string; primary?: boolean; danger?: boolean }[];
      resolve: (value: string | null) => void;
    }
  | { kind: "prompt"; title: string; label: string; value: string; resolve: (value: string | null) => void };

export const [modal, setModal] = createSignal<Modal | null>(null);

/** Shows a dialog with buttons and resolves with the chosen value (null on cancel). */
export function confirm(
  title: string,
  message: string,
  actions: { label: string; value: string; primary?: boolean; danger?: boolean }[],
): Promise<string | null> {
  return new Promise((resolve) => setModal({ kind: "confirm", title, message, actions, resolve }));
}

/** Asks for a single line of text. */
export function prompt(title: string, label: string, value = ""): Promise<string | null> {
  return new Promise((resolve) => setModal({ kind: "prompt", title, label, value, resolve }));
}

export interface Toast {
  id: number;
  kind: "info" | "success" | "error";
  message: string;
  action?: { label: string; run: () => void };
}

export const [toasts, setToasts] = createSignal<Toast[]>([]);
let nextToast = 1;

export function toast(message: string, kind: Toast["kind"] = "info", action?: Toast["action"]) {
  const id = nextToast++;
  setToasts((t) => [...t.slice(-3), { id, kind, message, action }]);
  setTimeout(() => dismissToast(id), kind === "error" ? 8000 : 4000);
}

export function dismissToast(id: number) {
  setToasts((t) => t.filter((x) => x.id !== id));
}

export const isMac = typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent);
/** Label for the primary modifier: ⌘ on macOS, Ctrl elsewhere. */
export const MOD = isMac ? "⌘" : "Ctrl+";
export const SHIFT = isMac ? "⇧" : "Shift+";
export const ALT = isMac ? "⌥" : "Alt+";
