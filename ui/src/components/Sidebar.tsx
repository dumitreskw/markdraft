import { createMemo, createSignal, For, Match, onCleanup, Show, Switch } from "solid-js";
import * as ipc from "../ipc";
import { extension, relativeTo } from "../paths";
import { openDoc, reveal } from "../state/docs";
import { settings, updateSettings } from "../state/settings";
import { countDocs } from "../state/tree";
import { setSidebarTab, sidebarTab, type SidebarTab } from "../state/ui";
import {
  currentGitRoots,
  gitState,
  locate,
  opened,
  refreshGit,
  rootIds,
  rootInputs,
  roots,
  scanning,
  setExpanded,
  setTreeFilter,
  treeFilter,
} from "../state/workspace";
import { openDiff, openHistory } from "./DiffView";
import { FileTree } from "./FileTree";
import { FileIcon, FolderIcon, GitIcon, ListIcon, PlusIcon, RefreshIcon, SearchIcon } from "./Icons";
import { openContextMenu } from "./ContextMenu";
import { commands } from "../commands";

const DOC_EXTENSIONS = new Set(["md", "markdown", "mdx"]);

export function Sidebar() {
  const changedCount = createMemo(
    () => Object.values(gitState.status).filter((s) => DOC_EXTENSIONS.has(extension(s.path)) && locate(s.path)).length,
  );
  const tab = (id: SidebarTab, label: string, icon: () => any, badge?: () => number) => (
    <button
      type="button"
      role="tab"
      class="sidebar-tab"
      aria-selected={sidebarTab() === id}
      onClick={() => setSidebarTab(id)}
      title={label}
    >
      {icon()}
      <span>{label}</span>
      <Show when={badge && badge() > 0}>
        <span class="count">{badge!()}</span>
      </Show>
    </button>
  );

  return (
    <div class="sidebar">
      <div class="sidebar-tabs" role="tablist" aria-label="Sidebar">
        {tab("files", "Files", () => <FolderIcon />)}
        {tab("search", "Search", () => <SearchIcon />)}
        <Show when={currentGitRoots().length > 0}>{tab("changes", "Changes", () => <GitIcon />, changedCount)}</Show>
      </div>
      <Switch>
        <Match when={sidebarTab() === "files"}>
          <FilesPanel />
        </Match>
        <Match when={sidebarTab() === "search"}>
          <SearchPanel />
        </Match>
        <Match when={sidebarTab() === "changes"}>
          <ChangesPanel />
        </Match>
      </Switch>
    </div>
  );
}

function FilesPanel() {
  const docCount = createMemo(() => countDocs(rootInputs()));
  const title = () => opened()?.workspace?.name ?? (roots().length === 1 ? roots()[0].name : "Workspace");

  // Same as Mod+N: in the active document's folder, then focus the editor.
  const newDoc = () => void commands.find((c) => c.id === "new-document")?.run();

  const viewMenu = (e: MouseEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const check = (on: boolean) => (on ? "✓" : "");
    openContextMenu(
      [
        { label: "Show document titles", shortcut: check(settings.showTitles), run: () => updateSettings({ showTitles: true }) },
        { label: "Show file names", shortcut: check(!settings.showTitles), run: () => updateSettings({ showTitles: false }) },
        { separator: true },
        { label: "Sort by name", shortcut: check(settings.sort === "name"), run: () => updateSettings({ sort: "name" }) },
        { label: "Sort by title", shortcut: check(settings.sort === "title"), run: () => updateSettings({ sort: "title" }) },
        { label: "Sort by last modified", shortcut: check(settings.sort === "modified"), run: () => updateSettings({ sort: "modified" }) },
        { separator: true },
        { label: "Hide empty folders", shortcut: check(settings.hideEmptyDirs), run: () => updateSettings({ hideEmptyDirs: !settings.hideEmptyDirs }) },
        { label: "Collapse all folders", run: () => setExpanded(new Set<string>()) },
      ],
      r.left,
      r.bottom + 4,
    );
  };

  return (
    <div class="panel">
      <div class="panel-header">
        <h2 title={roots().map((r) => r.path).join("\n")}>{title()}</h2>
        <div class="panel-actions">
          <button type="button" class="icon-btn" onClick={newDoc} aria-label="New document" title="New document">
            <PlusIcon />
          </button>
          <button type="button" class="icon-btn" onClick={viewMenu} aria-label="Tree options" title="Sort and display options">
            <ListIcon />
          </button>
        </div>
      </div>
      <div class="filter-box">
        <SearchIcon />
        <input
          type="search"
          placeholder="Filter documents"
          aria-label="Filter documents"
          value={treeFilter()}
          onInput={(e) => setTreeFilter(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setTreeFilter("");
            if (e.key === "ArrowDown") document.querySelector<HTMLElement>(".tree")?.focus();
          }}
        />
      </div>
      <FileTree />
      <div class="panel-footer">
        <Show when={scanning().length > 0} fallback={<span>{docCount().toLocaleString()} documents</span>}>
          <span class="scanning">Scanning… {docCount().toLocaleString()} found</span>
        </Show>
      </div>
    </div>
  );
}

// ------------------------------------------------------------- search

function SearchPanel() {
  const [query, setQuery] = createSignal("");
  const [regex, setRegex] = createSignal(false);
  const [caseSensitive, setCaseSensitive] = createSignal(false);
  const [wholeWord, setWholeWord] = createSignal(false);
  const [results, setResults] = createSignal<ipc.FileMatches[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0;
  onCleanup(() => clearTimeout(timer));

  const run = () => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const q = query();
      const mine = ++seq;
      if (!q.trim()) {
        setResults([]);
        setError(null);
        return;
      }
      setBusy(true);
      try {
        const r = await ipc.searchText(
          { pattern: q, regex: regex(), caseSensitive: caseSensitive(), wholeWord: wholeWord() },
          200,
          rootIds(),
        );
        if (mine === seq) {
          setResults(r);
          setError(null);
        }
      } catch (e) {
        if (mine === seq) setError(ipc.errorMessage(e));
      } finally {
        if (mine === seq) setBusy(false);
      }
    }, 200);
  };

  const total = () => results().reduce((n, f) => n + f.total, 0);
  const relOf = (path: string) => {
    for (const r of roots()) {
      const rel = relativeTo(r.path, path);
      if (rel !== null) return roots().length > 1 ? `${r.name}/${rel}` : rel;
    }
    return path;
  };

  const toggle = (label: string, title: string, get: () => boolean, set: (v: boolean) => void) => (
    <button
      type="button"
      class="toggle"
      aria-pressed={get()}
      title={title}
      onClick={() => {
        set(!get());
        run();
      }}
    >
      {label}
    </button>
  );

  return (
    <div class="panel">
      <div class="search-box">
        <input
          id="search-input"
          ref={(el) => requestAnimationFrame(() => el.focus())}
          type="search"
          placeholder="Search in documents"
          aria-label="Search in documents"
          value={query()}
          onInput={(e) => {
            setQuery(e.currentTarget.value);
            run();
          }}
        />
        <div class="search-toggles">
          {toggle("Aa", "Match case", caseSensitive, setCaseSensitive)}
          {toggle("ab", "Whole word", wholeWord, setWholeWord)}
          {toggle(".*", "Regular expression", regex, setRegex)}
        </div>
      </div>
      <div class="search-results" aria-live="polite">
        <Show when={error()}>
          <p class="search-error">{error()}</p>
        </Show>
        <Show when={query().trim() && !busy() && !error()}>
          <p class="muted search-count">
            {total()} {total() === 1 ? "match" : "matches"} in {results().length} {results().length === 1 ? "document" : "documents"}
          </p>
        </Show>
        <For each={results()}>
          {(file) => (
            <div class="search-file">
              <button type="button" class="search-file-name" onClick={() => void openDoc(file.path)} title={file.path}>
                <FileIcon />
                <span>{relOf(file.path)}</span>
                <span class="count">{file.total}</span>
              </button>
              <For each={file.matches}>
                {(m) => (
                  <button
                    type="button"
                    class="search-line"
                    onClick={async () => {
                      await openDoc(file.path);
                      reveal(file.path, { line: m.line });
                    }}
                  >
                    <span class="ln">{m.line}</span>
                    <span class="text">{highlight(m.text, m.ranges)}</span>
                  </button>
                )}
              </For>
            </div>
          )}
        </For>
      </div>
    </div>
  );
}

function highlight(text: string, ranges: [number, number][]) {
  const parts = [];
  let at = 0;
  for (const [s, e] of ranges) {
    if (s > at) parts.push(text.slice(at, s));
    parts.push(<mark>{text.slice(s, e)}</mark>);
    at = e;
  }
  parts.push(text.slice(at));
  return parts;
}

// ------------------------------------------------------------ changes

const KIND_LABEL: Record<ipc.StatusKind, string> = {
  modified: "Modified",
  added: "Added",
  deleted: "Deleted",
  renamed: "Renamed",
  untracked: "New",
  conflicted: "Conflict",
  typeChange: "Changed",
};

function ChangesPanel() {
  const [allFiles, setAllFiles] = createSignal(false);
  const changes = createMemo(() =>
    Object.values(gitState.status)
      .filter((s) => locate(s.path) && (allFiles() || DOC_EXTENSIONS.has(extension(s.path))))
      .sort((a, b) => a.path.localeCompare(b.path)),
  );
  const relOf = (path: string) => {
    for (const r of roots()) {
      const rel = relativeTo(r.path, path);
      if (rel !== null) return rel;
    }
    for (const g of gitState.roots) {
      const rel = relativeTo(g.info.workdir, path);
      if (rel !== null) return rel;
    }
    return path;
  };

  const show = (s: ipc.FileStatus) => {
    if (s.kind === "untracked" || s.kind === "added") {
      openDiff(s.path, { kind: "head" }, { kind: "workdir" }, { old: "Nothing", new: "New document" });
    } else if (s.kind === "deleted") {
      openDiff(s.path, { kind: "head" }, { kind: "workdir" }, { old: "Last commit", new: "Deleted" });
    } else {
      openDiff(s.path, { kind: "head" }, { kind: "workdir" });
    }
  };

  return (
    <div class="panel">
      <div class="panel-header">
        <h2>Changes</h2>
        <div class="panel-actions">
          <button type="button" class="icon-btn" onClick={() => void refreshGit()} aria-label="Refresh" title="Refresh">
            <RefreshIcon />
          </button>
        </div>
      </div>
      <For each={currentGitRoots()}>
        {(g) => (
          <div class="repo-info">
            <GitIcon />
            <span class="branch">{g.info.branch ?? (g.info.detached ? `detached at ${g.info.head?.shortId}` : "no commits")}</span>
            <Show when={g.info.ahead || g.info.behind}>
              <span class="muted">
                ↑{g.info.ahead ?? 0} ↓{g.info.behind ?? 0}
              </span>
            </Show>
            <Show when={g.info.operation}>
              <span class="pill warn">{g.info.operation}</span>
            </Show>
          </div>
        )}
      </For>
      <label class="check-row">
        <input type="checkbox" checked={allFiles()} onChange={(e) => setAllFiles(e.currentTarget.checked)} />
        Include files that are not documents
      </label>
      <div class="changes-list">
        <Show when={changes().length} fallback={<p class="muted empty-note">No uncommitted changes. Everything is committed.</p>}>
          <For each={changes()}>
            {(s) => (
              <div class="change-row">
                <button type="button" class="change-main" onClick={() => show(s)} title={`${KIND_LABEL[s.kind]} — show changes`}>
                  <span class={`git-badge git-${s.kind}`}>{KIND_LABEL[s.kind][0]}</span>
                  <span class="change-name">{relOf(s.path)}</span>
                  <span class="change-kind">{KIND_LABEL[s.kind]}</span>
                </button>
                <Show when={s.kind !== "deleted" && s.kind !== "untracked"}>
                  <button type="button" class="icon-btn small" onClick={() => void openHistory(s.path)} aria-label="History" title="History">
                    <ListIcon />
                  </button>
                </Show>
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
}
