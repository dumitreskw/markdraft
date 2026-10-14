import { createResource, createSignal, onCleanup, onMount, Show } from "solid-js";
import { FileIcon, FolderIcon, LayersIcon, ListIcon } from "./components/Icons";
import { Splitter } from "./components/Splitter";
import * as ipc from "./ipc";
import { isPrimaryModifier, OUTLINE_BOUNDS, TREE_BOUNDS } from "./layout";

const isMac = navigator.userAgent.includes("Mac");
const mod = isMac ? "⌘" : "Ctrl+";

export function App() {
  const [treeWidth, setTreeWidth] = createSignal(280);
  const [outlineWidth, setOutlineWidth] = createSignal(240);
  const [openPath, setOpenPath] = createSignal<string | null>(null);
  const [status, setStatus] = createSignal<string | null>(null);
  const [info] = createResource(() => (ipc.inTauri() ? ipc.appInfo() : Promise.resolve(null)));

  const pick = async (kind: ipc.PickKind) => {
    if (!ipc.inTauri()) {
      setStatus("Running in a browser: native dialogs need the desktop app.");
      return;
    }
    try {
      const path = await ipc.pickPath(kind);
      if (path) setOpenPath(path);
    } catch (e) {
      setStatus(ipc.errorMessage(e));
    }
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (!isPrimaryModifier(e, isMac) || e.key.toLowerCase() !== "o") return;
    e.preventDefault();
    void pick(e.shiftKey ? "file" : "folder");
  };

  onMount(() => {
    window.addEventListener("keydown", onKeyDown);
    onCleanup(() => window.removeEventListener("keydown", onKeyDown));
    if (!ipc.inTauri()) return;
    // Cleanups must be registered synchronously, before any await.
    const unlisten = ipc.onOpenPath((req) => setOpenPath(req.path));
    onCleanup(() => void unlisten.then((stop) => stop()));
    // Listen first, then drain the launch path, so neither source is missed.
    void unlisten
      .then(() => ipc.takeOpenRequest())
      .then((pending) => pending && setOpenPath(pending.path))
      .catch((e) => setStatus(ipc.errorMessage(e)));
  });

  return (
    <div class="app">
      <header class="titlebar">
        <span class="brand">
          <span class="brand-mark" aria-hidden="true">
            M↓
          </span>
          MarkDraft
        </span>
        <span class="titlebar-path" title={openPath() ?? undefined}>
          {openPath() ?? "No folder open"}
        </span>
      </header>

      <div class="panes">
        <nav id="tree-pane" class="pane tree-pane" aria-label="Files" style={{ width: `${treeWidth()}px` }}>
          <div class="pane-header">
            <h2>Files</h2>
          </div>
          <Show
            when={openPath()}
            fallback={
              <div class="empty">
                <p>Open a folder to see its Markdown files here.</p>
                <button type="button" class="btn primary" onClick={() => pick("folder")}>
                  <FolderIcon /> Open Folder
                </button>
              </div>
            }
          >
            <div class="empty">
              <p class="muted">The file tree arrives in milestone M1.</p>
            </div>
          </Show>
        </nav>

        <Splitter
          label="Resize file tree"
          controls="tree-pane"
          width={treeWidth()}
          bounds={TREE_BOUNDS}
          direction={1}
          onResize={setTreeWidth}
        />

        <main class="pane doc-pane" aria-label="Document">
          <section class="welcome" aria-labelledby="welcome-title">
            <h1 id="welcome-title">Start writing</h1>
            <p class="muted">Open a folder of notes, a single Markdown file, or a saved workspace.</p>
            <div class="actions">
              <button type="button" class="action" onClick={() => pick("folder")}>
                <FolderIcon />
                <span>Open Folder</span>
                <kbd>{mod}O</kbd>
              </button>
              <button type="button" class="action" onClick={() => pick("file")}>
                <FileIcon />
                <span>Open File</span>
                <kbd>{mod}Shift+O</kbd>
              </button>
              <button type="button" class="action" onClick={() => pick("workspace")}>
                <LayersIcon />
                <span>Open Workspace</span>
              </button>
            </div>
          </section>
        </main>

        <Splitter
          label="Resize outline"
          controls="outline-pane"
          width={outlineWidth()}
          bounds={OUTLINE_BOUNDS}
          direction={-1}
          onResize={setOutlineWidth}
        />

        <aside id="outline-pane" class="pane outline-pane" aria-label="Outline" style={{ width: `${outlineWidth()}px` }}>
          <div class="pane-header">
            <h2>Outline</h2>
          </div>
          <div class="empty">
            <ListIcon />
            <p class="muted">Headings of the open document appear here.</p>
          </div>
        </aside>
      </div>

      <footer class="statusbar" role="status">
        <span>{status() ?? "Ready"}</span>
        <Show when={info()}>{(i) => <span class="muted">v{i().version} · {i().os}/{i().arch}</span>}</Show>
      </footer>
    </div>
  );
}
