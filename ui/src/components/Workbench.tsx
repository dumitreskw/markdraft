// Tabs, the document toolbar, change banners, outline and status bar.
import { createEffect, createMemo, createSignal, For, on, Show } from "solid-js";
import * as ipc from "../ipc";
import { cursor } from "../state/cursor";
import { revealLine, topLine } from "../editor/sync";
import { activeDoc, activePath, closeTab, docs, keepMine, openDoc, reloadFromDisk, reveal, saveDoc, tabs } from "../state/docs";
import { setViewMode, viewMode, setModal, outlineVisible, setOutlineVisible } from "../state/ui";
import { allDocs, currentGitRoots, gitStatusOf, lastScanMs, locate, rootIds, treeVersionSignal as rootsVersion } from "../state/workspace";

const docTitle = (path: string) => allDocs().find((d) => d.path === path)?.title ?? null;
import { diffRequest } from "../state/diff";
import { openDiff, openHistory } from "./DiffView";
import { CloseIcon, DiffIcon, DownloadIcon, EyeIcon, GitIcon, HistoryIcon, OutlineIcon, PenIcon, SplitIcon } from "./Icons";
import { openContextMenu } from "./ContextMenu";
import { displayKeys } from "../commands";

export function Tabs() {
  let strip!: HTMLDivElement;
  const label = (path: string) => docs[path]?.name ?? path;
  const onKeyDown = (e: KeyboardEvent) => {
    const list = tabs();
    const i = list.indexOf(activePath() ?? "");
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      const next = list[(i + (e.key === "ArrowRight" ? 1 : -1) + list.length) % list.length];
      void openDoc(next);
      requestAnimationFrame(() => strip.querySelector<HTMLElement>('[aria-selected="true"]')?.focus());
      e.preventDefault();
    } else if (e.key === "Delete" && activePath()) {
      void closeTab(activePath()!);
    }
  };
  return (
    <div ref={strip} class="tabs" role="tablist" aria-label="Open documents" onKeyDown={onKeyDown}>
      <For each={tabs()}>
        {(path) => {
          const doc = () => docs[path];
          const active = () => activePath() === path;
          return (
            <div
              class="tab"
              classList={{ active: active(), dirty: doc()?.status === "dirty", conflict: doc()?.status === "conflict" }}
              role="tab"
              aria-selected={active()}
              tabIndex={active() ? 0 : -1}
              title={path}
              onClick={() => void openDoc(path)}
              onAuxClick={(e) => {
                if (e.button === 1) void closeTab(path);
              }}
              onContextMenu={(e) => {
                e.preventDefault();
                openContextMenu(
                  [
                    { label: "Close", run: () => void closeTab(path) },
                    {
                      label: "Close others",
                      run: async () => {
                        for (const p of tabs()) if (p !== path && !(await closeTab(p))) return;
                      },
                    },
                    {
                      label: "Close all",
                      run: async () => {
                        for (const p of [...tabs()]) if (!(await closeTab(p))) return;
                      },
                    },
                    { separator: true },
                    { label: "Copy path", run: () => void navigator.clipboard.writeText(path) },
                  ],
                  e.clientX,
                  e.clientY,
                );
              }}
            >
              <span class="tab-label">{label(path)}</span>
              <button
                type="button"
                class="tab-close"
                tabIndex={-1}
                aria-label={`Close ${doc()?.name ?? ""}`}
                onClick={(e) => {
                  e.stopPropagation();
                  void closeTab(path);
                }}
              >
                <span class="dirty-dot" aria-hidden="true" />
                <CloseIcon />
              </button>
            </div>
          );
        }}
      </For>
    </div>
  );
}

export function DocToolbar() {
  const status = () => (activePath() ? gitStatusOf(activePath()!) : undefined);
  const tracked = () => !!status() && status()!.kind !== "untracked";
  const inRepo = () => currentGitRoots().length > 0;
  const modeButton = (mode: "read" | "edit" | "split", label: string, icon: () => any, keys: string) => (
    <button type="button" aria-pressed={viewMode() === mode} onClick={() => setViewMode(mode)} title={`${label} (${displayKeys(keys)})`}>
      {icon()}
      <span>{label}</span>
    </button>
  );
  return (
    <div class="doc-toolbar">
      <div class="segmented" role="group" aria-label="View">
        {modeButton("read", "Read", () => <EyeIcon />, "Mod+1")}
        {modeButton("edit", "Edit", () => <PenIcon />, "Mod+2")}
        {modeButton("split", "Split", () => <SplitIcon />, "Mod+3")}
      </div>
      <div class="toolbar-spacer" />
      <Show when={inRepo() && activePath()}>
        <Show when={tracked() || activeDoc()?.text !== activeDoc()?.savedText}>
          <button
            type="button"
            class="btn ghost"
            onClick={() => {
              const d = activeDoc()!;
              openDiff(d.path, { kind: "head" }, d.text !== d.savedText ? { kind: "text", text: d.text } : { kind: "workdir" });
            }}
            title={`Compare with last commit (${displayKeys("Mod+Alt+D")})`}
          >
            <DiffIcon /> Changes
            <Show when={status()}>
              <span class={`git-badge git-${status()!.kind}`}>{status()!.kind[0].toUpperCase()}</span>
            </Show>
          </button>
        </Show>
        <button type="button" class="btn ghost" onClick={() => void openHistory(activePath()!)} title={`File history (${displayKeys("Mod+Alt+H")})`}>
          <HistoryIcon /> History
        </button>
      </Show>
      <button
        type="button"
        class="btn ghost"
        onClick={() => setModal({ kind: "export", path: activePath()! })}
        title={`Export to PDF (${displayKeys("Mod+Shift+E")})`}
      >
        <DownloadIcon /> PDF
      </button>
      <button
        type="button"
        class="icon-btn"
        aria-pressed={outlineVisible()}
        onClick={() => setOutlineVisible(!outlineVisible())}
        aria-label="Toggle outline"
        title={`Outline (${displayKeys("Mod+Shift+\\")})`}
      >
        <OutlineIcon />
      </button>
    </div>
  );
}

/** Shown when the open document changed or vanished on disk. */
export function ChangeBanner() {
  const doc = activeDoc;
  return (
    <Show when={doc()?.external || doc()?.status === "error"}>
      <div class="banner" role="alert" classList={{ error: doc()!.status === "error" }}>
        <BannerContent doc={doc()!} />
      </div>
    </Show>
  );
}

function BannerContent(props: { doc: NonNullable<ReturnType<typeof activeDoc>> }) {
  const d = () => props.doc;
  return (
    <>
      <Show when={d().external === "changed"}>
        <span>
          <strong>{d().name}</strong> changed on disk while you were editing.
        </span>
        <div class="banner-actions">
          <button
            type="button"
            class="btn"
            onClick={() => openDiff(d().path, { kind: "workdir" }, { kind: "text", text: d().text }, { old: "On disk", new: "Your edits" })}
          >
            Compare
          </button>
          <button type="button" class="btn" onClick={() => void reloadFromDisk(d().path)}>
            Use disk version
          </button>
          <button type="button" class="btn primary" onClick={() => void keepMine(d().path)}>
            Keep mine
          </button>
        </div>
      </Show>
      <Show when={d().external === "deleted"}>
        <span>
          <strong>{d().name}</strong> was deleted or moved outside MarkDraft.
        </span>
        <div class="banner-actions">
          <button type="button" class="btn" onClick={() => void closeTab(d().path)}>
            Close
          </button>
          <button type="button" class="btn primary" onClick={() => void saveDoc(d().path, true)}>
            Save it again
          </button>
        </div>
      </Show>
      <Show when={!d().external && d().status === "error"}>
        <span>Could not save: {d().error}</span>
        <div class="banner-actions">
          <button type="button" class="btn primary" onClick={() => void saveDoc(d().path, true)}>
            Retry
          </button>
        </div>
      </Show>
    </>
  );
}

/** Documents that link to the active one; refreshed on switch and save. */
function Backlinks() {
  const [links, setLinks] = createSignal<ipc.Backlink[]>([]);
  let seq = 0;
  createEffect(
    on(
      () => [activePath(), activeDoc()?.savedText, rootsVersion()] as const,
      ([path]) => {
        const mine = ++seq;
        if (!path || !ipc.inTauri()) return setLinks([]);
        // Debounced: saves of other files also land here through the tree.
        setTimeout(async () => {
          if (mine !== seq) return;
          try {
            const found = await ipc.backlinks(path, rootIds());
            if (mine === seq) setLinks(found);
          } catch {
            if (mine === seq) setLinks([]);
          }
        }, 400);
      },
    ),
  );
  const relOf = (path: string) => locate(path)?.rel ?? path;
  return (
    <section class="backlinks" aria-label="Linked from">
      <div class="panel-header">
        <h2>Linked from{links().length ? ` (${links().length})` : ""}</h2>
      </div>
      <Show when={links().length} fallback={<p class="muted empty-note">No other document links here.</p>}>
        <ul class="backlink-list">
          <For each={links()}>
            {(l) => (
              <li>
                <button
                  type="button"
                  class="backlink"
                  title={`${relOf(l.path)}:${l.line}`}
                  onClick={async () => {
                    await openDoc(l.path);
                    reveal(l.path, { line: l.line });
                  }}
                >
                  <span class="backlink-file">{docTitle(l.path) ?? relOf(l.path)}</span>
                  <span class="backlink-text">{l.text}</span>
                </button>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </section>
  );
}

export function Outline() {
  const headings = () => activeDoc()?.rendered?.headings ?? [];
  const minLevel = createMemo(() => Math.min(...headings().map((h) => h.level), 6));
  const current = createMemo(() => {
    const line = topLine();
    let idx = -1;
    headings().forEach((h, i) => {
      if (h.line <= line + 1) idx = i;
    });
    return idx;
  });
  return (
    <nav class="outline" aria-label="Outline">
      <div class="panel-header">
        <h2>Outline</h2>
        <button type="button" class="icon-btn" onClick={() => setOutlineVisible(false)} aria-label="Hide outline">
          <CloseIcon />
        </button>
      </div>
      <Show when={headings().length} fallback={<p class="muted empty-note">Headings in this document appear here.</p>}>
        <ol class="outline-list">
          <For each={headings()}>
            {(h, i) => (
              <li>
                <button
                  type="button"
                  class="outline-item"
                  classList={{ current: i() === current() }}
                  style={{ "padding-left": `${12 + (h.level - minLevel()) * 14}px` }}
                  onClick={() => revealLine(h.line)}
                  title={h.text}
                >
                  {h.text || "(untitled)"}
                </button>
              </li>
            )}
          </For>
        </ol>
      </Show>
      <Backlinks />
    </nav>
  );
}

export function StatusBar() {
  const doc = activeDoc;
  const words = () => doc()?.rendered?.wordCount ?? 0;
  const minutes = () => Math.max(1, Math.round(words() / 230));
  const saveState = () => {
    const d = doc();
    if (!d) return "";
    switch (d.status) {
      case "clean":
        return "Saved";
      case "dirty":
        return "Unsaved changes";
      case "saving":
        return "Saving…";
      case "conflict":
        return "Changed on disk";
      case "error":
        return "Save failed";
    }
  };
  const branch = () => {
    const p = activePath();
    const loc = p ? locate(p) : null;
    const g = currentGitRoots().find((r) => loc && r.root === loc.root.path) ?? currentGitRoots()[0];
    return g?.info.branch ?? null;
  };
  const format = () => {
    const f = doc()?.format;
    if (!f) return "";
    const enc = { utf8: "UTF-8", utf16Le: "UTF-16 LE", utf16Be: "UTF-16 BE", windows1252: "Windows-1252" }[f.encoding];
    return `${enc}${f.bom ? " BOM" : ""} · ${f.lineEnding.toUpperCase()}`;
  };
  return (
    <footer class="statusbar" role="status">
      <div class="status-left">
        <Show when={branch()}>
          <span class="status-item" title="Git branch">
            <GitIcon /> {branch()}
          </span>
        </Show>
        <Show when={doc()}>
          <span class={`status-item save-${doc()!.status}`}>{saveState()}</span>
          <Show when={doc()!.readonly}>
            <span class="status-item">Read-only</span>
          </Show>
        </Show>
        <Show when={diffRequest()}>
          <span class="status-item">Reviewing changes</span>
        </Show>
      </div>
      <div class="status-right">
        <Show when={doc()}>
          <span class="status-item">
            {words().toLocaleString()} words · {minutes()} min read
          </span>
          <Show when={viewMode() !== "read"}>
            <span class="status-item">
              Ln {cursor().line}, Col {cursor().col}
              <Show when={cursor().selected}> ({cursor().selected} selected)</Show>
            </span>
          </Show>
          <span class="status-item muted">{format()}</span>
        </Show>
        <Show when={!doc() && lastScanMs() !== null}>
          <span class="status-item muted">Indexed in {lastScanMs()} ms</span>
        </Show>
      </div>
    </footer>
  );
}
