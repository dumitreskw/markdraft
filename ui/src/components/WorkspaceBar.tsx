import { createResource, For, Show } from "solid-js";
import { displayKeys } from "../commands";
import * as ipc from "../ipc";
import { docs } from "../state/docs";
import { setModal, toast } from "../state/ui";
import {
  activeTabId,
  closeOtherWorkspaces,
  closeWorkspace,
  openPathAction,
  switchWorkspace,
  tabName,
  workspaceTabs,
  type WorkspaceTab,
} from "../state/workspace";
import { openContextMenu, type MenuItem } from "./ContextMenu";
import { CloseIcon, FileIcon, FolderIcon, LayersIcon, PlusIcon } from "./Icons";
import { scrollableStrip } from "./Workbench";

/** Tabs for the workspaces open in this window, like browser tabs. */
export function WorkspaceBar() {
  const [recents] = createResource(() => (ipc.inTauri() ? ipc.listRecents() : Promise.resolve([])));

  const icon = (tab: WorkspaceTab) =>
    tab.kind === "workspace" ? <LayersIcon /> : tab.kind === "file" ? <FileIcon /> : <FolderIcon />;

  /** Unsaved documents in a background tab show as a dot on the tab. */
  const hasUnsaved = (tab: WorkspaceTab) => {
    const paths = tab.id === activeTabId() ? null : (tab.memory?.tabs ?? []);
    const list = paths ?? Object.keys(docs);
    return list.some((p) => {
      const s = docs[p]?.status;
      return s === "dirty" || s === "conflict" || s === "error";
    });
  };

  const pick = async (kind: ipc.PickKind) => {
    const path = await ipc.pickPath(kind);
    if (path) await openPathAction(path);
  };

  const newTabMenu = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const items: MenuItem[] = [
      { label: "Open Folder…", shortcut: displayKeys("Mod+O"), run: () => void pick("folder") },
      { label: "Open File…", shortcut: displayKeys("Mod+Shift+O"), run: () => void pick("file") },
      { label: "Open Workspace…", run: () => void pick("workspace") },
    ];
    const recent = (recents() ?? []).filter((x) => !workspaceTabs().some((t) => t.path === x.path)).slice(0, 6);
    if (recent.length) {
      items.push({ separator: true });
      for (const item of recent) items.push({ label: item.name, shortcut: item.kind, run: () => void openPathAction(item.path) });
    }
    openContextMenu(items, r.left, r.bottom + 4);
  };

  const tabMenu = (e: MouseEvent, tab: WorkspaceTab) => {
    e.preventDefault();
    openContextMenu(
      [
        { label: "Close", shortcut: displayKeys("Mod+Alt+W"), run: () => void closeWorkspace(tab.id) },
        { label: "Close other tabs", run: () => void closeOtherWorkspaces(tab.id) },
        { separator: true },
        {
          label: "Save as workspace…",
          run: async () => {
            await switchWorkspace(tab.id);
            setModal({ kind: "saveWorkspace" });
          },
        },
        { label: "Reveal in file manager", run: () => void ipc.revealPath(tab.path).catch((err) => toast(ipc.errorMessage(err), "error")) },
        { label: "Copy path", run: () => void navigator.clipboard.writeText(tab.path) },
      ],
      e.clientX,
      e.clientY,
    );
  };

  let bar!: HTMLDivElement;
  const onWheel = scrollableStrip(() => bar, () => [activeTabId(), workspaceTabs().length]);

  const onKeyDown = (e: KeyboardEvent) => {
    const list = workspaceTabs();
    const i = list.findIndex((t) => t.id === activeTabId());
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    e.preventDefault();
    const next = list[(i + (e.key === "ArrowRight" ? 1 : -1) + list.length) % list.length];
    if (next) void switchWorkspace(next.id);
  };

  return (
    <div ref={bar} class="workspace-bar" role="tablist" aria-label="Open workspaces" onKeyDown={onKeyDown} onWheel={onWheel}>
      <For each={workspaceTabs()}>
        {(tab) => {
          const active = () => tab.id === activeTabId();
          return (
            <div
              class="ws-tab"
              classList={{ active: active() }}
              role="tab"
              aria-selected={active()}
              tabIndex={active() ? 0 : -1}
              title={tab.path}
              onClick={() => void switchWorkspace(tab.id)}
              onAuxClick={(e) => e.button === 1 && void closeWorkspace(tab.id)}
              onContextMenu={(e) => tabMenu(e, tab)}
            >
              <span class="ws-icon">{icon(tab)}</span>
              <span class="ws-name">{tabName(tab)}</span>
              <Show when={hasUnsaved(tab)}>
                <span class="ws-dirty" title="Unsaved changes" />
              </Show>
              <button
                type="button"
                class="ws-close"
                tabIndex={-1}
                aria-label={`Close ${tabName(tab)}`}
                onClick={(e) => {
                  e.stopPropagation();
                  void closeWorkspace(tab.id);
                }}
              >
                <CloseIcon />
              </button>
            </div>
          );
        }}
      </For>
      <button type="button" class="ws-new" onClick={newTabMenu} aria-label="Open another workspace" title="Open another folder or workspace">
        <PlusIcon />
      </button>
    </div>
  );
}
