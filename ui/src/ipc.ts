// Typed wrappers for MarkDraft's Tauri commands and events.
// Names and shapes mirror src-tauri/src/commands and src-tauri/src/events.rs.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface AppInfo {
  name: string;
  version: string;
  os: string;
  arch: string;
}

export type PickKind = "folder" | "file" | "workspace";

export interface OpenRequest {
  path: string;
}

export interface CmdError {
  kind: "io" | "internal";
  message: string;
}

export const inTauri = isTauri;

export const appInfo = () => invoke<AppInfo>("app_info");

export const pickPath = (kind: PickKind) => invoke<string | null>("pick_path", { kind });

export const takeOpenRequest = () => invoke<OpenRequest | null>("take_open_request");

export const onOpenPath = (handler: (req: OpenRequest) => void): Promise<UnlistenFn> =>
  listen<OpenRequest>("app://open-path", (e) => handler(e.payload));

export function errorMessage(e: unknown): string {
  if (typeof e === "object" && e !== null && "message" in e) {
    return String((e as CmdError).message);
  }
  return String(e);
}
