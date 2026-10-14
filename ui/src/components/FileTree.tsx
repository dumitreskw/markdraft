import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import * as ipc from "../ipc";
import { baseName, dirName, joinPath, samePath, separatorOf } from "../paths";
import { activePath, openDoc, renamedByApp, closeTab, tabs } from "../state/docs";
import { settings } from "../state/settings";
import { visibleRows, type Row } from "../state/tree";
import { confirm, prompt, setModal, toast } from "../state/ui";
import {
  expanded,
  gitFolderHasChanges,
  gitStatusOf,
  removeRootAction,
  revealInTree,
  rootById,
  rootInputs,
  scanning,
  selectedKey,
  setSelectedKey,
  setScanning,
  toggleExpanded,
  treeFilter,
} from "../state/workspace";
import { openContextMenu, type MenuItem } from "./ContextMenu";
import { ChevronIcon, FileIcon, FolderIcon } from "./Icons";
import { openHistory, openDiff } from "./DiffView";

const ROW_H = 28;
const OVERSCAN = 8;

export function absOf(row: Row): string {
  const root = rootById(row.rootId);
  return root ? joinPath(root.path, row.path) : row.path;
}

const STATUS_LETTER: Record<ipc.StatusKind, string> = {
  modified: "M",
  added: "A",
  deleted: "D",
  renamed: "R",
  untracked: "U",
  conflicted: "!",
  typeChange: "T",
};

export function FileTree() {
  let scroller!: HTMLDivElement;
  const [scrollTop, setScrollTop] = createSignal(0);
  const [height, setHeight] = createSignal(400);
  const [renaming, setRenaming] = createSignal<string | null>(null);
  const [dropTarget, setDropTarget] = createSignal<string | null>(null);
  const [dragging, setDragging] = createSignal<string | null>(null);

  const rows = createMemo(() =>
    visibleRows(rootInputs(), expanded(), {
      sort: settings.sort,
      showTitles: settings.showTitles,
      hideEmptyDirs: settings.hideEmptyDirs,
      filter: treeFilter(),
    }),
  );

  const range = createMemo(() => {
    const start = Math.max(0, Math.floor(scrollTop() / ROW_H) - OVERSCAN);
    const end = Math.min(rows().length, Math.ceil((scrollTop() + height()) / ROW_H) + OVERSCAN);
    return { start, end };
  });
  const windowRows = createMemo(() => rows().slice(range().start, range().end).map((row, i) => ({ row, index: range().start + i })));

  const selectedIndex = () => rows().findIndex((r) => r.key === selectedKey());

  const scrollIntoView = (index: number) => {
    const top = index * ROW_H;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + ROW_H > scroller.scrollTop + scroller.clientHeight) scroller.scrollTop = top + ROW_H - scroller.clientHeight;
  };

  // Follow the active document in the tree.
  createEffect(
    on(activePath, (path) => {
      if (!path) return;
      revealInTree(path);
      requestAnimationFrame(() => {
        const i = selectedIndex();
        if (i >= 0) scrollIntoView(i);
      });
    }),
  );

  const activate = (row: Row) => {
    setSelectedKey(row.key);
    if (row.isDir) toggleExpanded(row.key);
    else void openDoc(absOf(row));
  };

  // ------------------------------------------------------------ keyboard

  let typeAhead = "";
  let typeAheadTimer: ReturnType<typeof setTimeout> | undefined;

  const onKeyDown = (e: KeyboardEvent) => {
    if (renaming()) return;
    const list = rows();
    if (!list.length) return;
    let i = selectedIndex();
    const row = list[i];
    const move = (to: number) => {
      const next = Math.max(0, Math.min(list.length - 1, to));
      setSelectedKey(list[next].key);
      scrollIntoView(next);
    };
    switch (e.key) {
      case "ArrowDown":
        move(i + 1);
        break;
      case "ArrowUp":
        move(i < 0 ? 0 : i - 1);
        break;
      case "Home":
        move(0);
        break;
      case "End":
        move(list.length - 1);
        break;
      case "PageDown":
        move(i + Math.floor(scroller.clientHeight / ROW_H));
        break;
      case "PageUp":
        move(i - Math.floor(scroller.clientHeight / ROW_H));
        break;
      case "ArrowRight":
        if (row?.isDir && !row.expanded) toggleExpanded(row.key, true);
        else if (row?.isDir) move(i + 1);
        break;
      case "ArrowLeft":
        if (row?.isDir && row.expanded) toggleExpanded(row.key, false);
        else if (row) {
          while (i > 0 && list[i].depth >= row.depth) i--;
          if (list[i].depth < row.depth) move(i);
        }
        break;
      case "Enter":
      case " ":
        if (row) activate(row);
        break;
      case "F2":
        if (row && !row.isRoot) setRenaming(row.key);
        break;
      case "Delete":
        if (row && !row.isRoot) void deleteRow(row);
        break;
      case "ContextMenu":
        if (row) {
          const el = scroller.querySelector<HTMLElement>(`[data-key="${CSS.escape(row.key)}"]`);
          const rect = el?.getBoundingClientRect();
          showMenu(row, rect ? rect.left + 24 : 40, rect ? rect.bottom : 40);
        }
        break;
      default:
        if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
          clearTimeout(typeAheadTimer);
          typeAhead += e.key.toLowerCase();
          typeAheadTimer = setTimeout(() => (typeAhead = ""), 700);
          const from = Math.max(0, i);
          for (let k = 0; k < list.length; k++) {
            const idx = (from + (typeAhead.length === 1 ? 1 : 0) + k) % list.length;
            if (list[idx].label.toLowerCase().startsWith(typeAhead)) {
              move(idx);
              break;
            }
          }
        } else return;
    }
    e.preventDefault();
  };

  // ------------------------------------------------------------ actions

  const folderFor = (row: Row): string => (row.isDir ? absOf(row) : dirName(absOf(row)));

  const newEntry = async (row: Row, isDir: boolean) => {
    const name = await prompt(isDir ? "New folder" : "New document", isDir ? "Folder name" : "Document name", isDir ? "" : "Untitled");
    if (!name) return;
    try {
      const created = await ipc.createEntry(folderFor(row), name, isDir);
      if (row.isDir) toggleExpanded(row.key, true);
      if (!isDir) await openDoc(created);
    } catch (e) {
      toast(ipc.errorMessage(e), "error");
    }
  };

  const deleteRow = async (row: Row) => {
    const abs = absOf(row);
    const choice = await confirm(
      `Delete ${row.isDir ? "folder" : "document"} “${row.name}”?`,
      row.isDir ? "The folder and everything in it will be moved to the trash." : "The file will be moved to the trash.",
      [
        { label: "Cancel", value: "cancel" },
        { label: "Move to trash", value: "delete", danger: true },
      ],
    );
    if (choice !== "delete") return;
    for (const t of tabs()) {
      if (samePath(t, abs) || t.replace(/\\/g, "/").toLowerCase().startsWith(abs.replace(/\\/g, "/").toLowerCase() + "/")) {
        await closeTab(t);
      }
    }
    try {
      await ipc.trashEntry(abs);
    } catch (e) {
      toast(ipc.errorMessage(e), "error");
    }
  };

  const commitRename = async (row: Row, name: string) => {
    // Enter commits, then the input's blur fires: only the first counts.
    if (renaming() !== row.key) return;
    setRenaming(null);
    const trimmed = name.trim();
    if (!trimmed || trimmed === row.name) return;
    const from = absOf(row);
    const to = dirName(from) + (dirName(from).endsWith(separatorOf(from)) ? "" : separatorOf(from)) + trimmed;
    try {
      const result = await ipc.renameEntry(from, to);
      renamedByApp(from, result);
    } catch (e) {
      toast(ipc.errorMessage(e), "error");
    }
  };

  const moveInto = async (source: Row, target: Row) => {
    const from = absOf(source);
    const dir = target.isDir ? absOf(target) : dirName(absOf(target));
    if (samePath(dirName(from), dir) || samePath(from, dir)) return;
    const to = dir + separatorOf(dir) + baseName(from);
    try {
      const result = await ipc.renameEntry(from, to);
      renamedByApp(from, result);
      if (target.isDir) toggleExpanded(target.key, true);
    } catch (e) {
      toast(ipc.errorMessage(e), "error");
    }
  };

  const exportFolder = async (folder: string, name: string) => {
    const out = await ipc.pickSavePath("pdf", `${name}.pdf`);
    if (!out) return;
    toast(`Exporting ${name}…`);
    try {
      const n = await ipc.exportFolderPdf(folder, out, { theme: "clean", paper: "A4", toc: true });
      toast(`Exported ${n} documents to ${baseName(out)}`, "success", { label: "Show", run: () => void ipc.revealPath(out).catch(() => {}) });
    } catch (e) {
      toast(`Export failed: ${ipc.errorMessage(e)}`, "error");
    }
  };

  const showMenu = (row: Row, x: number, y: number) => {
    setSelectedKey(row.key);
    const abs = absOf(row);
    const items: MenuItem[] = [];
    if (!row.isDir) {
      items.push({ label: "Open", run: () => void openDoc(abs) });
      items.push({ separator: true });
    }
    items.push({ label: "New document…", shortcut: "", run: () => void newEntry(row, false) });
    items.push({ label: "New folder…", run: () => void newEntry(row, true) });
    items.push({ separator: true });
    if (!row.isRoot) {
      items.push({ label: "Rename", shortcut: "F2", run: () => setRenaming(row.key) });
      if (!row.isDir) items.push({ label: "Duplicate", run: () => void ipc.duplicateEntry(abs).catch((e) => toast(ipc.errorMessage(e), "error")) });
      items.push({ label: "Move to trash", shortcut: "Del", danger: true, run: () => void deleteRow(row) });
      items.push({ separator: true });
    }
    if (!row.isDir) {
      const status = gitStatusOf(abs);
      items.push({ label: "Show history", run: () => openHistory(abs) });
      if (status && status.kind !== "untracked") items.push({ label: "Compare with last commit", run: () => openDiff(abs, { kind: "head" }, { kind: "workdir" }) });
      items.push({ label: "Export to PDF…", run: () => setModal({ kind: "export", path: abs }) });
      items.push({ separator: true });
    }
    if (row.isDir) {
      items.push({ label: "Export folder to PDF…", run: () => void exportFolder(abs, row.name) });
      items.push({ separator: true });
    }
    items.push({ label: "Reveal in file manager", run: () => void ipc.revealPath(abs).catch((e) => toast(ipc.errorMessage(e), "error")) });
    items.push({ label: "Copy path", run: () => void navigator.clipboard.writeText(abs) });
    if (!row.isRoot) items.push({ label: "Copy relative path", run: () => void navigator.clipboard.writeText(row.path) });
    if (row.isRoot) {
      items.push({ separator: true });
      items.push({
        label: "Rescan folder",
        run: () => {
          setScanning((s) => [...s, row.rootId]);
          void ipc.rescanRoot(row.rootId);
        },
      });
      items.push({ label: "Remove folder from workspace", run: () => void removeRootAction(row.rootId) });
    }
    openContextMenu(items, x, y);
  };

  // ------------------------------------------------------- drag to move

  let pending: { row: Row; x: number; y: number } | null = null;

  const onPointerDown = (e: PointerEvent, row: Row) => {
    if (e.button !== 0 || row.isRoot || renaming()) return;
    pending = { row, x: e.clientX, y: e.clientY };
  };

  const rowAt = (y: number): Row | undefined => {
    const rect = scroller.getBoundingClientRect();
    const index = Math.floor((y - rect.top + scroller.scrollTop) / ROW_H);
    return rows()[index];
  };

  const onPointerMove = (e: PointerEvent) => {
    if (!pending) return;
    if (!dragging()) {
      if (Math.hypot(e.clientX - pending.x, e.clientY - pending.y) < 5) return;
      setDragging(pending.row.key);
    }
    // Dropping on a document means "into its folder".
    const target = rowAt(e.clientY);
    if (!target) {
      setDropTarget(null);
      return;
    }
    const folder = target.isDir ? target.path : target.path.split("/").slice(0, -1).join("/");
    const key = `${target.rootId}:${folder}`;
    setDropTarget(key === pending.row.key ? null : key);
  };

  const onPointerUp = (e: PointerEvent) => {
    const source = pending?.row;
    pending = null;
    if (!dragging() || !source) return;
    setDragging(null);
    const key = dropTarget();
    setDropTarget(null);
    if (!key) return;
    const i = key.indexOf(":");
    const rootId = Number(key.slice(0, i));
    const path = key.slice(i + 1);
    const target: Row = rows().find((r) => r.key === key) ?? {
      key,
      rootId,
      path,
      depth: 0,
      isDir: true,
      isRoot: true,
      label: "",
      name: "",
      expanded: true,
      modifiedMs: 0,
    };
    e.preventDefault();
    void moveInto(source, target);
  };

  onMount(() => {
    const ro = new ResizeObserver(() => setHeight(scroller.clientHeight));
    ro.observe(scroller);
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
    onCleanup(() => {
      ro.disconnect();
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
    });
  });

  const activeKey = createMemo(() => {
    const p = activePath();
    if (!p) return null;
    const row = rows().find((r) => !r.isDir && samePath(absOf(r), p));
    return row?.key ?? null;
  });

  return (
    <div
      ref={scroller}
      class="tree"
      classList={{ dragging: !!dragging() }}
      role="tree"
      aria-label="Files"
      tabIndex={0}
      aria-activedescendant={selectedIndex() >= 0 ? `tree-row-${selectedIndex()}` : undefined}
      onScroll={() => setScrollTop(scroller.scrollTop)}
      onKeyDown={onKeyDown}
      onFocus={() => {
        if (selectedIndex() < 0 && rows().length) setSelectedKey(rows()[0].key);
      }}
    >
      <Show when={rows().length === 0 && scanning().length === 0}>
        <div class="tree-empty">{treeFilter() ? "No documents match." : "No Markdown documents here yet."}</div>
      </Show>
      <div class="tree-inner" style={{ height: `${rows().length * ROW_H}px` }}>
        <For each={windowRows()}>
          {(item) => {
            const row = () => item.row;
            const abs = () => absOf(row());
            const status = () => (row().isDir ? undefined : gitStatusOf(abs()));
            const folderChanged = () => row().isDir && !row().isRoot && gitFolderHasChanges(abs());
            return (
              <div
                id={`tree-row-${item.index}`}
                data-key={row().key}
                class="tree-row"
                classList={{
                  selected: row().key === selectedKey(),
                  active: row().key === activeKey(),
                  root: row().isRoot,
                  "drop-target": dropTarget() === row().key,
                  "drag-source": dragging() === row().key,
                }}
                role="treeitem"
                aria-level={row().depth + 1}
                aria-expanded={row().isDir ? row().expanded : undefined}
                aria-selected={row().key === selectedKey()}
                style={{ top: `${item.index * ROW_H}px`, "padding-left": `${8 + row().depth * 14}px` }}
                title={row().label !== row().name ? `${row().label}\n${row().path}` : row().path || row().label}
                onClick={() => activate(row())}
                onPointerDown={(e) => onPointerDown(e, row())}
                onContextMenu={(e) => {
                  e.preventDefault();
                  showMenu(row(), e.clientX, e.clientY);
                }}
              >
                <span class="tree-twisty" classList={{ open: row().expanded, hidden: !row().isDir }}>
                  <ChevronIcon />
                </span>
                <span class="tree-icon">{row().isDir ? <FolderIcon /> : <FileIcon />}</span>
                <Show
                  when={renaming() === row().key}
                  fallback={
                    <span class="tree-label" classList={{ "is-title": row().label !== row().name }}>
                      {row().label}
                    </span>
                  }
                >
                  <input
                    class="tree-rename"
                    value={row().name}
                    ref={(el) =>
                      requestAnimationFrame(() => {
                        el.focus();
                        const dot = row().isDir ? -1 : el.value.lastIndexOf(".");
                        el.setSelectionRange(0, dot > 0 ? dot : el.value.length);
                      })
                    }
                    onClick={(e) => e.stopPropagation()}
                    onKeyDown={(e) => {
                      e.stopPropagation();
                      if (e.key === "Enter") void commitRename(row(), e.currentTarget.value);
                      if (e.key === "Escape") setRenaming(null);
                    }}
                    onBlur={(e) => void commitRename(row(), e.currentTarget.value)}
                  />
                </Show>
                <Show when={status()}>
                  {(s) => (
                    <span class={`git-badge git-${s().kind}`} title={`Git: ${s().kind}`}>
                      {STATUS_LETTER[s().kind]}
                    </span>
                  )}
                </Show>
                <Show when={folderChanged()}>
                  <span class="git-dot" title="Contains changes" />
                </Show>
              </div>
            );
          }}
        </For>
      </div>
    </div>
  );
}
