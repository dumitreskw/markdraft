// Updates: a quiet check shortly after start-up and every few hours, a
// manual check from Settings, and installing (save everything, then let
// Rust download, verify, install and restart).
import { createSignal } from "solid-js";
import * as ipc from "../ipc";
import { saveAll } from "./docs";
import { settings } from "./settings";
import { toast } from "./ui";
import { persistSession } from "./workspace";

export type UpdateStatus =
  | { state: "idle" }
  | { state: "checking" }
  | { state: "current"; checkedAt: number }
  | { state: "available"; info: ipc.UpdateInfo }
  | { state: "installing"; info: ipc.UpdateInfo; progress: ipc.UpdateProgress | null }
  | { state: "error"; message: string };

export const [updateStatus, setUpdateStatus] = createSignal<UpdateStatus>({ state: "idle" });
/** The version whose notice the user closed; it stays quiet until the next one. */
const [dismissed, setDismissed] = createSignal<string | null>(null);

export const updateNotice = () => {
  const s = updateStatus();
  if (s.state === "installing") return s;
  if (s.state === "available" && s.info.version !== dismissed()) return s;
  return null;
};

export const dismissUpdate = () => {
  const s = updateStatus();
  if (s.state === "available") setDismissed(s.info.version);
};

/** Checks the release feed. `manual` checks report "up to date" and errors. */
export async function checkForUpdate(manual = false): Promise<void> {
  if (!ipc.inTauri()) return;
  const before = updateStatus().state;
  if (before === "checking" || before === "installing") return;
  setUpdateStatus({ state: "checking" });
  try {
    const info = await ipc.checkUpdate();
    setUpdateStatus(info ? { state: "available", info } : { state: "current", checkedAt: Date.now() });
    if (info && manual) setDismissed(null);
  } catch (e) {
    // A failed background check (offline, feed down) stays silent.
    setUpdateStatus(manual ? { state: "error", message: ipc.errorMessage(e) } : { state: "idle" });
  }
}

/** Saves every document and the session, then installs and restarts. */
export async function installUpdate(): Promise<void> {
  const s = updateStatus();
  if (s.state !== "available") return;
  if (!s.info.canInstall) {
    void ipc.openExternal(s.info.pageUrl);
    return;
  }
  if (!(await saveAll())) {
    toast("Save or close the documents with unsaved changes, then try again.", "error");
    return;
  }
  await persistSession().catch(() => {});
  setUpdateStatus({ state: "installing", info: s.info, progress: null });
  const stop = await ipc.onUpdateProgress((progress) => {
    const now = updateStatus();
    if (now.state === "installing") setUpdateStatus({ ...now, progress });
  });
  try {
    await ipc.installUpdate();
  } catch (e) {
    setUpdateStatus({ state: "available", info: s.info });
    toast(`Could not update: ${ipc.errorMessage(e)}`, "error");
  } finally {
    stop();
  }
}

const FIRST_CHECK_MS = 8_000;
const RECHECK_MS = 6 * 60 * 60 * 1000;

/** Starts the background checks; returns a cleanup. */
export function startUpdateChecks(): () => void {
  if (!ipc.inTauri()) return () => {};
  const run = () => {
    if (settings.checkForUpdates) void checkForUpdate();
  };
  const first = setTimeout(run, FIRST_CHECK_MS);
  const every = setInterval(run, RECHECK_MS);
  return () => {
    clearTimeout(first);
    clearInterval(every);
  };
}
