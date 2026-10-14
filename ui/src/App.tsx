import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { createEffect, createSignal, on, onCleanup, onMount, Show } from "solid-js";
import { handleGlobalKey } from "./commands";
import { ContextMenuHost } from "./components/ContextMenu";
import { ModalHost, Toasts } from "./components/Dialogs";
import { DiffPane } from "./components/DiffView";
import { EditorPane, insertAtCursor } from "./components/EditorPane";
import { Preview } from "./components/Preview";
import { Sidebar } from "./components/Sidebar";
import { Splitter } from "./components/Splitter";
import { NoDocument, Welcome } from "./components/Welcome";
import { ChangeBanner, DocToolbar, Outline, StatusBar, Tabs } from "./components/Workbench";
import * as ipc from "./ipc";
import { OUTLINE_BOUNDS, TREE_BOUNDS } from "./layout";
import { diffRequest } from "./state/diff";
import { activeDoc, activePath, hasUnsaved, saveAll, tabs } from "./state/docs";
import { effectiveTheme, loadSettings, settings } from "./state/settings";
import {
  focusMode,
  outlineVisible,
  outlineWidth,
  setOutlineWidth,
  setSplitRatio,
  setTreeWidth,
  setViewMode,
  sidebarVisible,
  splitRatio,
  toast,
  treeWidth,
  viewMode,
} from "./state/ui";
import { addRootAction, expanded, initWorkspaceEvents, opened, openPathAction, persistSession, reattach, scheduleSessionSave } from "./state/workspace";

export function App() {
  const [ready, setReady] = createSignal(false);

  // Theme: data attribute for CSS tokens + syntect classes for code.
  createEffect(() => {
    const theme = settings.theme;
    if (theme === "system") delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme;
    document.documentElement.dataset.effectiveTheme = effectiveTheme();
  });

  const [highlight, setHighlight] = createSignal<{ light: string; dark: string } | null>(null);
  createEffect(() => {
    const css = highlight();
    if (!css) return;
    let el = document.getElementById("hl-theme") as HTMLStyleElement | null;
    if (!el) {
      el = document.createElement("style");
      el.id = "hl-theme";
      document.head.append(el);
    }
    el.textContent = effectiveTheme() === "dark" ? css.dark : css.light;
  });

  // Window title: document, dirty marker, folder.
  createEffect(() => {
    const doc = activeDoc();
    const o = opened();
    const where = o?.workspace?.name ?? (o?.roots.length === 1 ? o.roots[0].name : o ? "Workspace" : "");
    const parts = [doc ? `${doc.status === "dirty" ? "● " : ""}${doc.rendered?.title ?? doc.name}` : "", where, "MarkDraft"].filter(Boolean);
    const title = parts.join(" — ");
    document.title = title;
    if (ipc.inTauri()) void getCurrentWindow().setTitle(title).catch(() => {});
  });

  // Persist tabs/layout per folder set.
  createEffect(
    on(
      () => [tabs(), activePath(), viewMode(), expanded(), treeWidth(), outlineWidth(), splitRatio()],
      () => scheduleSessionSave(),
      { defer: true },
    ),
  );

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.defaultPrevented) handleGlobalKey(e);
    };
    window.addEventListener("keydown", onKey);
    const cleanupEvents = initWorkspaceEvents();

    const onBlur = () => {
      if (settings.autosave) void saveAll();
      void persistSession();
    };
    window.addEventListener("blur", onBlur);

    onCleanup(() => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", onBlur);
      cleanupEvents();
    });

    if (ipc.inTauri()) {
      // Listeners are registered before any await so their cleanups have an owner.
      const unlisteners = [
        ipc.onOpenPath((req) => void openPathAction(req.path)),
        // Dropping folders or files onto the window opens them (Shift adds a folder).
        getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type !== "drop" || !event.payload.paths.length) return;
          const paths = event.payload.paths;
          const [first] = paths;
          const doc = activePath();
          // Images dropped on an open document are inserted, not opened.
          if (doc && /\.(png|jpe?g|gif|webp|svg|avif|bmp)$/i.test(first)) {
            void (async () => {
              for (const source of paths) {
                try {
                  const link = await ipc.importImage(doc, source);
                  if (viewMode() === "read") setViewMode("split");
                  insertAtCursor(`![](${link})\n`);
                } catch (e) {
                  toast(ipc.errorMessage(e), "error");
                }
              }
            })();
          } else if (opened() && dropAdds) void addRootAction(first);
          else void openPathAction(first);
        }),
        // Ask before closing with unsaved work.
        getCurrentWindow().onCloseRequested(async (event) => {
          await persistSession();
          if (!hasUnsaved()) return;
          if (settings.autosave && (await saveAll())) return;
          event.preventDefault();
          toast("Some documents have unsaved changes. Save or close them first.", "error");
        }),
      ];
      onCleanup(() => unlisteners.forEach((u) => void u.then((stop) => stop())));
    }

    void (async () => {
      await loadSettings();
      setReady(true);
      if (!ipc.inTauri()) return;
      ipc.highlightCss().then(setHighlight, () => {});
      const pending = await ipc.takeOpenRequest();
      if (pending) await openPathAction(pending.path);
      else await reattach();
    })();
  });

  let dropAdds = false;
  const trackShift = (e: KeyboardEvent) => (dropAdds = e.shiftKey);
  window.addEventListener("keydown", trackShift);
  window.addEventListener("keyup", trackShift);

  const showEditor = () => viewMode() !== "read";
  const showPreview = () => viewMode() !== "edit";

  let docArea!: HTMLDivElement;
  const [docWidth, setDocWidth] = createSignal(800);
  onMount(() => {
    const ro = new ResizeObserver(() => setDocWidth(docArea?.clientWidth ?? 800));
    if (docArea) ro.observe(docArea);
    onCleanup(() => ro.disconnect());
  });

  return (
    <div class="app" classList={{ "focus-mode": focusMode(), loading: !ready() }}>
      <Show when={opened()} fallback={<Welcome />}>
        <div class="panes">
          <Show when={sidebarVisible() && !focusMode()}>
            <aside id="sidebar" class="pane sidebar-pane" style={{ width: `${treeWidth()}px` }}>
              <Sidebar />
            </aside>
            <Splitter label="Resize sidebar" controls="sidebar" width={treeWidth()} bounds={TREE_BOUNDS} direction={1} onResize={setTreeWidth} />
          </Show>

          <main class="pane doc-pane" aria-label="Document">
            <Show
              when={!diffRequest()}
              fallback={<DiffPane />}
            >
              <Show when={tabs().length > 0 && !focusMode()}>
                <div class="doc-header">
                  <Tabs />
                  <Show when={activePath()}>
                    <DocToolbar />
                  </Show>
                </div>
              </Show>
              <ChangeBanner />
              <div ref={docArea} class="doc-area" classList={{ [`mode-${viewMode()}`]: true }}>
                <Show when={activePath()} fallback={<NoDocument />}>
                  <div
                    class="editor-column"
                    style={{
                      display: showEditor() ? undefined : "none",
                      flex: viewMode() === "split" ? `0 0 ${Math.round(splitRatio() * 100)}%` : "1 1 auto",
                    }}
                  >
                    <EditorPane />
                  </div>
                  <Show when={viewMode() === "split"}>
                    <Splitter
                      label="Resize editor and preview"
                      controls="preview-column"
                      width={Math.round(splitRatio() * docWidth())}
                      bounds={{ min: Math.round(docWidth() * 0.2), max: Math.round(docWidth() * 0.8) }}
                      direction={1}
                      onResize={(w) => setSplitRatio(w / Math.max(1, docWidth()))}
                    />
                  </Show>
                  <Show when={showPreview()}>
                    <div id="preview-column" class="preview-column">
                      <Show when={activePath()} keyed>
                        {(path) => <Preview path={path} />}
                      </Show>
                    </div>
                  </Show>
                </Show>
              </div>
            </Show>
          </main>

          <Show when={outlineVisible() && activePath() && !diffRequest() && !focusMode()}>
            <Splitter label="Resize outline" controls="outline-pane" width={outlineWidth()} bounds={OUTLINE_BOUNDS} direction={-1} onResize={setOutlineWidth} />
            <aside id="outline-pane" class="pane outline-pane" style={{ width: `${outlineWidth()}px` }}>
              <Outline />
            </aside>
          </Show>
        </div>
        <Show when={!focusMode()}>
          <StatusBar />
        </Show>
      </Show>
      <ModalHost />
      <ContextMenuHost />
      <Toasts />
    </div>
  );
}
