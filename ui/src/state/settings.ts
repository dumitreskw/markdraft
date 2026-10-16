// User settings, persisted in the app-data folder through Rust.
import { createSignal } from "solid-js";
import { createStore } from "solid-js/store";
import * as ipc from "../ipc";
import type { SortKey } from "./tree";

export type ThemePref = "system" | "light" | "dark";
export type ViewMode = "read" | "edit" | "split";

export interface Settings {
  theme: ThemePref;
  autosave: boolean;
  autosaveDelayMs: number;
  showTitles: boolean;
  sort: SortKey;
  hideEmptyDirs: boolean;
  editorFont: "mono" | "sans" | "serif";
  editorFontSize: number;
  lineNumbers: boolean;
  lineWrap: boolean;
  previewWidth: "narrow" | "medium" | "wide";
  previewFont: "sans" | "serif";
  spellcheck: boolean;
  restoreSession: boolean;
  smartPunctuation: boolean;
  defaultView: ViewMode;
  checkForUpdates: boolean;
}

export const DEFAULT_SETTINGS: Settings = {
  theme: "system",
  autosave: true,
  autosaveDelayMs: 1000,
  showTitles: true,
  sort: "name",
  hideEmptyDirs: true,
  editorFont: "mono",
  editorFontSize: 14,
  lineNumbers: false,
  lineWrap: true,
  previewWidth: "medium",
  previewFont: "sans",
  spellcheck: true,
  restoreSession: true,
  smartPunctuation: false,
  defaultView: "split",
  checkForUpdates: true,
};

export const [settings, setSettingsStore] = createStore<Settings>({ ...DEFAULT_SETTINGS });

/** Merges stored values over the defaults, ignoring unknown or mistyped keys. */
export function mergeSettings(stored: Record<string, unknown>): Settings {
  const merged: Record<string, unknown> = { ...DEFAULT_SETTINGS };
  for (const [key, value] of Object.entries(stored)) {
    const def = (DEFAULT_SETTINGS as unknown as Record<string, unknown>)[key];
    if (def !== undefined && typeof value === typeof def) merged[key] = value;
  }
  return merged as unknown as Settings;
}

export async function loadSettings() {
  if (!ipc.inTauri()) return;
  try {
    setSettingsStore(mergeSettings(await ipc.getSettings()));
  } catch {
    // Defaults are fine.
  }
}

let persistTimer: ReturnType<typeof setTimeout> | undefined;

export function updateSettings(patch: Partial<Settings>) {
  setSettingsStore(patch);
  clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    if (ipc.inTauri()) void ipc.setSettings({ ...settings }).catch(() => {});
  }, 300);
}

// The OS colour scheme, tracked so "system" follows it live.
const media = typeof window !== "undefined" && window.matchMedia ? window.matchMedia("(prefers-color-scheme: dark)") : null;
const [systemDark, setSystemDark] = createSignal(media?.matches ?? false);
media?.addEventListener("change", (e) => setSystemDark(e.matches));

export function effectiveTheme(): "light" | "dark" {
  return settings.theme === "system" ? (systemDark() ? "dark" : "light") : settings.theme;
}
