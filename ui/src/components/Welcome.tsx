import { createMemo, createResource, For, Show } from "solid-js";
import { displayKeys } from "../commands";
import * as ipc from "../ipc";
import { relativeTo } from "../paths";
import { openDoc } from "../state/docs";
import { timeAgo } from "../state/diff";
import { allDocs, openPathAction, roots } from "../state/workspace";
import { FileIcon, FolderIcon, LayersIcon, CloseIcon } from "./Icons";

/** Start screen when nothing is open: actions and recent items. */
export function Welcome() {
  const [recents, { mutate }] = createResource(() => (ipc.inTauri() ? ipc.listRecents() : Promise.resolve([])));
  const pick = async (kind: ipc.PickKind) => {
    if (!ipc.inTauri()) return;
    const path = await ipc.pickPath(kind);
    if (path) await openPathAction(path);
  };
  const icon = (kind: ipc.RecentEntry["kind"]) =>
    kind === "folder" ? <FolderIcon /> : kind === "workspace" ? <LayersIcon /> : <FileIcon />;

  return (
    <section class="welcome" aria-labelledby="welcome-title">
      <div class="welcome-brand" aria-hidden="true">
        M↓
      </div>
      <h1 id="welcome-title">MarkDraft</h1>
      <p class="muted">Read, write and export Markdown — your files stay plain text on disk.</p>
      <div class="actions">
        <button type="button" class="action" onClick={() => pick("folder")}>
          <FolderIcon />
          <span>Open Folder</span>
          <kbd>{displayKeys("Mod+O")}</kbd>
        </button>
        <button type="button" class="action" onClick={() => pick("file")}>
          <FileIcon />
          <span>Open File</span>
          <kbd>{displayKeys("Mod+Shift+O")}</kbd>
        </button>
        <button type="button" class="action" onClick={() => pick("workspace")}>
          <LayersIcon />
          <span>Open Workspace</span>
        </button>
      </div>
      <Show when={recents()?.length}>
        <h2 class="welcome-subtitle">Recent</h2>
        <ul class="recent-list">
          <For each={recents()}>
            {(r) => (
              <li class="recent-item">
                <button type="button" class="recent-open" onClick={() => void openPathAction(r.path)} title={r.path}>
                  {icon(r.kind)}
                  <span class="recent-name">{r.name}</span>
                  <span class="recent-path">{r.path}</span>
                  <span class="recent-time">{timeAgo(r.openedMs)}</span>
                </button>
                <button
                  type="button"
                  class="icon-btn small"
                  aria-label={`Remove ${r.name} from recent`}
                  title="Remove from list"
                  onClick={async () => mutate(await ipc.removeRecent(r.path))}
                >
                  <CloseIcon />
                </button>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </section>
  );
}

/** Shown when a folder is open but no document is: recently edited files. */
export function NoDocument() {
  const recentlyEdited = createMemo(() =>
    [...allDocs()].sort((a, b) => b.modifiedMs - a.modifiedMs).slice(0, 8),
  );
  const relOf = (path: string) => {
    for (const r of roots()) {
      const rel = relativeTo(r.path, path);
      if (rel !== null) return rel;
    }
    return path;
  };
  return (
    <section class="welcome compact" aria-label="No document open">
      <h1>Pick a document</h1>
      <p class="muted">
        Choose one in the tree, or press <kbd>{displayKeys("Mod+P")}</kbd> to jump to any document by name.
      </p>
      <Show when={recentlyEdited().length}>
        <h2 class="welcome-subtitle">Recently edited</h2>
        <ul class="recent-list">
          <For each={recentlyEdited()}>
            {(d) => (
              <li class="recent-item">
                <button type="button" class="recent-open" onClick={() => void openDoc(d.path)}>
                  <FileIcon />
                  <span class="recent-name">{d.title ?? relOf(d.path)}</span>
                  <span class="recent-path">{relOf(d.path)}</span>
                  <span class="recent-time">{timeAgo(d.modifiedMs)}</span>
                </button>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </section>
  );
}
