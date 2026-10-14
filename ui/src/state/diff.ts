// What the diff view is showing: a document compared between two versions,
// plus the file's commit history for picking versions.
import { createSignal } from "solid-js";
import * as ipc from "../ipc";
import type { CommitInfo, DiffView, RevArg } from "../ipc";

export interface DiffRequest {
  path: string;
  old: RevArg;
  new: RevArg;
  oldLabel: string;
  newLabel: string;
  /** Selected commit in the history list, if the diff came from it. */
  commitId?: string;
}

export const [diffRequest, setDiffRequest] = createSignal<DiffRequest | null>(null);
export const [diffResult, setDiffResult] = createSignal<DiffView | null>(null);
export const [diffError, setDiffError] = createSignal<string | null>(null);
export const [diffLoading, setDiffLoading] = createSignal(false);
export const [history, setHistory] = createSignal<{ path: string; commits: CommitInfo[] } | null>(null);
export const [historyOpen, setHistoryOpen] = createSignal(false);

export function revLabel(rev: RevArg, commits?: CommitInfo[]): string {
  switch (rev.kind) {
    case "head":
      return "Last commit";
    case "index":
      return "Staged";
    case "workdir":
      return "On disk";
    case "text":
      return "Your edits";
    case "commit": {
      const c = commits?.find((x) => x.id.startsWith(rev.id) || rev.id.startsWith(x.shortId));
      return c ? `${c.shortId} · ${c.summary}` : rev.id.slice(0, 7);
    }
    case "parentOf":
      return `Before ${rev.id.slice(0, 7)}`;
  }
}

let seq = 0;

export async function showDiff(req: DiffRequest) {
  setDiffRequest(req);
  setDiffError(null);
  setDiffLoading(true);
  const mine = ++seq;
  try {
    const result = await ipc.diffDocument(req.path, req.old, req.new);
    if (mine === seq) setDiffResult(result);
  } catch (e) {
    if (mine === seq) {
      setDiffResult(null);
      setDiffError(ipc.errorMessage(e));
    }
  } finally {
    if (mine === seq) setDiffLoading(false);
  }
}

export function closeDiff() {
  seq++;
  setDiffRequest(null);
  setDiffResult(null);
  setDiffError(null);
  setHistoryOpen(false);
}

export async function loadHistory(path: string) {
  try {
    const commits = await ipc.gitHistory(path, 200);
    setHistory({ path, commits });
    return commits;
  } catch (e) {
    setHistory({ path, commits: [] });
    setDiffError(ipc.errorMessage(e));
    return [];
  }
}

/** Shows what a commit changed in a document. */
export function showCommit(path: string, commit: CommitInfo) {
  void showDiff({
    path,
    old: { kind: "parentOf", id: commit.id },
    new: { kind: "commit", id: commit.id },
    oldLabel: `Before ${commit.shortId}`,
    newLabel: `${commit.shortId} · ${commit.summary}`,
    commitId: commit.id,
  });
}

/** Relative time like "3 days ago". */
export function timeAgo(ms: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  const units: [number, string][] = [
    [60, "second"],
    [60, "minute"],
    [24, "hour"],
    [7, "day"],
    [4.35, "week"],
    [12, "month"],
    [Number.POSITIVE_INFINITY, "year"],
  ];
  let value = s;
  for (const [size, name] of units) {
    if (value < size) {
      const v = Math.floor(value);
      return v <= 1 && name === "second" ? "just now" : `${v} ${name}${v === 1 ? "" : "s"} ago`;
    }
    value /= size;
  }
  return "";
}
