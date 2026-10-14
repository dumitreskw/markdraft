import { createMemo, createSignal, For, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import type { BlockType, CollapsedEntry, DiffBlock, Hunk, RevArg, Segment, TableDiff } from "../ipc";
import { baseName } from "../paths";
import { docs, openDoc } from "../state/docs";
import {
  closeDiff,
  diffError,
  diffLoading,
  diffRequest,
  diffResult,
  history,
  historyOpen,
  loadHistory,
  revLabel,
  setHistoryOpen,
  showCommit,
  showDiff,
  timeAgo,
} from "../state/diff";
import { setViewMode } from "../state/ui";
import { CloseIcon, HistoryIcon } from "./Icons";

/** Opens the diff view comparing two versions of a document. */
export function openDiff(path: string, old: RevArg, newRev: RevArg, labels?: { old?: string; new?: string }) {
  void showDiff({
    path,
    old,
    new: newRev,
    oldLabel: labels?.old ?? revLabel(old),
    newLabel: labels?.new ?? revLabel(newRev),
  });
}

/** Opens the history list for a document and shows its latest change. */
export async function openHistory(path: string) {
  setHistoryOpen(true);
  const commits = await loadHistory(path);
  const doc = docs[path];
  if (doc && doc.text !== doc.savedText) {
    openDiff(path, { kind: "head" }, { kind: "text", text: doc.text });
  } else if (commits[0]) {
    showCommit(path, commits[0]);
  } else {
    openDiff(path, { kind: "head" }, { kind: "workdir" });
  }
}

export function DiffPane() {
  const [mode, setMode] = createSignal<"document" | "source">("document");
  const [expandedGaps, setExpandedGaps] = createSignal<Set<number>>(new Set());
  let scroller!: HTMLDivElement;

  const entries = createMemo<CollapsedEntry[]>(() => {
    const r = diffResult();
    if (!r) return [];
    const open = expandedGaps();
    return r.collapsed.flatMap((e) =>
      e.type === "gap" && open.has(e.start)
        ? Array.from({ length: e.end - e.start }, (_, i) => ({ type: "block" as const, index: e.start + i }))
        : [e],
    );
  });

  const stats = () => diffResult()?.diff.stats;
  const req = () => diffRequest()!;

  const jump = (dir: 1 | -1) => {
    const changes = [...scroller.querySelectorAll<HTMLElement>("[data-change]")];
    if (!changes.length) return;
    const top = scroller.scrollTop + 8;
    const offsets = changes.map((el) => el.offsetTop - scroller.offsetTop);
    let target: number;
    if (dir === 1) target = offsets.findIndex((o) => o > top + 4);
    else target = offsets.map((o, i) => [o, i] as const).filter(([o]) => o < top - 4).pop()?.[1] ?? -1;
    if (target < 0) target = dir === 1 ? 0 : changes.length - 1;
    scroller.scrollTo({ top: offsets[target] - 8, behavior: "smooth" });
    changes[target].classList.add("flash");
    setTimeout(() => changes[target].classList.remove("flash"), 900);
  };

  const editAt = (line: number | undefined) => {
    const path = req().path;
    closeDiff();
    setViewMode("split");
    void openDoc(path, line ? { line } : undefined);
  };

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement).closest("input, textarea, .cm-editor")) return;
      if (e.key === "Escape") closeDiff();
      else if (e.key === "j" || e.key === "n") jump(1);
      else if (e.key === "k" || e.key === "p") jump(-1);
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
  });

  const editable = () => ["workdir", "text"].includes(req().new.kind);

  return (
    <div class="diff-pane" role="region" aria-label="Changes">
      <header class="diff-header">
        <div class="diff-title">
          <strong>{baseName(req().path)}</strong>
          <span class="diff-versions">
            <span class="rev old" title={req().oldLabel}>
              {req().oldLabel}
            </span>
            <span aria-hidden="true">→</span>
            <span class="rev new" title={req().newLabel}>
              {req().newLabel}
            </span>
          </span>
        </div>
        <div class="diff-actions">
          <div class="segmented" role="group" aria-label="Diff style">
            <button type="button" aria-pressed={mode() === "document"} onClick={() => setMode("document")}>
              Document
            </button>
            <button type="button" aria-pressed={mode() === "source"} onClick={() => setMode("source")}>
              Source
            </button>
          </div>
          <button type="button" class="btn ghost" onClick={() => jump(-1)} title="Previous change (K)">
            ↑ Prev
          </button>
          <button type="button" class="btn ghost" onClick={() => jump(1)} title="Next change (J)">
            ↓ Next
          </button>
          <button
            type="button"
            class="btn ghost"
            aria-pressed={historyOpen()}
            onClick={() => {
              const open = !historyOpen();
              setHistoryOpen(open);
              if (open && history()?.path !== req().path) void loadHistory(req().path);
            }}
            title="File history"
          >
            <HistoryIcon /> History
          </button>
          <button type="button" class="icon-btn" onClick={closeDiff} aria-label="Close changes (Esc)" title="Close (Esc)">
            <CloseIcon />
          </button>
        </div>
      </header>

      <Show when={stats()}>
        {(s) => (
          <div class="diff-summary" aria-live="polite">
            <Show when={!s().identical} fallback={<span>No changes between these versions.</span>}>
              <span class="pill ins">+{s().wordsAdded} words</span>
              <span class="pill del">−{s().wordsRemoved} words</span>
              <Show when={s().modifiedBlocks}>
                <span>{plural(s().modifiedBlocks, "block")} edited</span>
              </Show>
              <Show when={s().addedBlocks}>
                <span>{plural(s().addedBlocks, "block")} added</span>
              </Show>
              <Show when={s().removedBlocks}>
                <span>{plural(s().removedBlocks, "block")} removed</span>
              </Show>
              <Show when={s().movedBlocks}>
                <span>{plural(s().movedBlocks, "block")} moved</span>
              </Show>
            </Show>
            <Show when={diffResult()?.diff.timedOut}>
              <span class="muted">(large change: shown in less detail)</span>
            </Show>
            <Show when={!diffResult()?.oldExists}>
              <span class="muted">New file</span>
            </Show>
            <Show when={!diffResult()?.newExists}>
              <span class="muted">Deleted</span>
            </Show>
          </div>
        )}
      </Show>

      <div class="diff-body">
        <div ref={scroller} class="diff-scroller">
          <Show when={diffError()}>
            <div class="diff-message error">{diffError()}</div>
          </Show>
          <Show when={diffLoading() && !diffResult()}>
            <div class="diff-message">Comparing…</div>
          </Show>
          <Show when={diffResult()}>
            {(result) => (
              <Switch>
                <Match when={mode() === "document"}>
                  <article class="markdown-body diff-doc">
                    <For each={entries()}>
                      {(entry) =>
                        entry.type === "gap" ? (
                          <button
                            type="button"
                            class="diff-gap"
                            onClick={() => setExpandedGaps((s) => new Set([...s, entry.start]))}
                          >
                            ⋯ {plural(entry.count, "unchanged block")} — show
                          </button>
                        ) : (
                          <BlockView
                            block={result().diff.blocks[entry.index]}
                            html={result().blocksHtml[entry.index]}
                            onEdit={editable() ? editAt : undefined}
                          />
                        )
                      }
                    </For>
                  </article>
                </Match>
                <Match when={mode() === "source"}>
                  <SourceView hunks={result().hunks} onEdit={editable() ? editAt : undefined} />
                </Match>
              </Switch>
            )}
          </Show>
        </div>

        <Show when={historyOpen()}>
          <aside class="history" aria-label="File history">
            <h3>History</h3>
            <Show when={history()?.path === req().path} fallback={<p class="muted">Loading…</p>}>
              <button
                type="button"
                class="history-item"
                classList={{ selected: !req().commitId }}
                onClick={() => {
                  const doc = docs[req().path];
                  if (doc && doc.text !== doc.savedText) openDiff(req().path, { kind: "head" }, { kind: "text", text: doc.text });
                  else openDiff(req().path, { kind: "head" }, { kind: "workdir" });
                }}
              >
                <span class="history-summary">Uncommitted changes</span>
                <span class="history-meta">Working copy vs last commit</span>
              </button>
              <For each={history()!.commits} fallback={<p class="muted">No commits touch this file yet.</p>}>
                {(c) => (
                  <button
                    type="button"
                    class="history-item"
                    classList={{ selected: req().commitId === c.id }}
                    onClick={() => showCommit(req().path, c)}
                    title={c.message}
                  >
                    <span class="history-summary">{c.summary || "(no message)"}</span>
                    <span class="history-meta">
                      {c.authorName} · {timeAgo(c.timeMs)} · <code>{c.shortId}</code>
                    </span>
                  </button>
                )}
              </For>
            </Show>
          </aside>
        </Show>
      </div>
    </div>
  );
}

function plural(n: number, word: string) {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

const KIND_LABEL: Record<string, string> = {
  added: "Added",
  removed: "Removed",
  modified: "Edited",
  moved: "Moved here",
};

function BlockView(props: { block: DiffBlock; html: string | null; onEdit?: (line?: number) => void }) {
  const b = () => props.block;
  const changed = () => b().kind !== "unchanged" && b().kind !== "movedAway";
  const line = () => b().newLines?.[0];
  return (
    <section
      class={`diff-block kind-${b().kind}`}
      data-change={changed() ? "" : undefined}
      aria-label={changed() ? KIND_LABEL[b().kind] : undefined}
    >
      <Show when={changed()}>
        <span class="diff-tag">{KIND_LABEL[b().kind]}</span>
      </Show>
      <Show when={props.onEdit && changed() && b().kind !== "removed"}>
        <button type="button" class="diff-edit" onClick={() => props.onEdit?.(line())} title="Edit this in the document">
          Edit
        </button>
      </Show>
      <Switch>
        <Match when={b().kind === "movedAway"}>
          <p class="diff-moved-away">↪ Moved {b().newLines ? `to line ${b().newLines![0]}` : "elsewhere"}</p>
        </Match>
        <Match when={b().kind === "modified" && b().table}>
          <TableView table={b().table!} />
        </Match>
        <Match when={b().kind === "modified"}>
          <ModifiedView block={b()} />
        </Match>
        <Match when={props.html !== null}>
          <div class="diff-html" innerHTML={props.html!} />
        </Match>
      </Switch>
    </section>
  );
}

function Segments(props: { segments: Segment[] }) {
  return (
    <For each={props.segments}>
      {(s) =>
        s.kind === "insert" ? <ins>{s.text}</ins> : s.kind === "delete" ? <del>{s.text}</del> : <span>{s.text}</span>
      }
    </For>
  );
}

function listMarker(t: BlockType) {
  if (t.type !== "listItem") return "";
  const box = t.checked === null ? "" : t.checked ? "☑ " : "☐ ";
  return (t.ordered ? `${t.number ?? 1}. ` : "• ") + box;
}

/** A modified block drawn from word segments, styled like its block type. */
function ModifiedView(props: { block: DiffBlock }) {
  const t = () => props.block.blockType;
  const old = () => props.block.oldBlockType;
  const typeChange = () => {
    const o = old();
    if (!o) return null;
    if (o.type === "listItem" && t().type === "listItem") {
      const was = (o as { checked: boolean | null }).checked;
      const now = (t() as { checked: boolean | null }).checked;
      if (was !== now) return now ? "Checked ☑" : "Unchecked ☐";
    }
    if (o.type === "heading" && t().type === "heading") {
      return `Heading level ${(o as { level: number }).level} → ${(t() as { level: number }).level}`;
    }
    return `${o.type} → ${t().type}`;
  };
  return (
    <>
      <Show when={typeChange()}>
        <p class="diff-type-change">{typeChange()}</p>
      </Show>
      <Switch fallback={<p class="diff-prose"><Segments segments={props.block.segments} /></p>}>
        <Match when={t().type === "heading"}>
          <div class={`diff-heading h${(t() as { level: number }).level}`} role="heading" aria-level={(t() as { level: number }).level}>
            <Segments segments={props.block.segments} />
          </div>
        </Match>
        <Match when={t().type === "listItem"}>
          <p class="diff-prose diff-list" style={{ "padding-left": `${((t() as { depth: number }).depth - 1) * 1.5}em` }}>
            <span class="diff-marker">{listMarker(t())}</span>
            <Segments segments={props.block.segments} />
          </p>
        </Match>
        <Match when={t().type === "code" || t().type === "html" || t().type === "frontMatter"}>
          <pre class="diff-code">
            <code>
              <Segments segments={props.block.segments} />
            </code>
          </pre>
        </Match>
        <Match when={t().type === "quote"}>
          <blockquote>
            <p class="diff-prose">
              <Segments segments={props.block.segments} />
            </p>
          </blockquote>
        </Match>
      </Switch>
    </>
  );
}

function TableView(props: { table: TableDiff }) {
  return (
    <table class="diff-table">
      <tbody>
        <For each={props.table.rows}>
          {(row) => (
            <tr class={`row-${row.kind}`}>
              <For each={row.cells}>
                {(cell, i) => {
                  const align = props.table.aligns[i()];
                  const style = align && align !== "none" ? { "text-align": align } : undefined;
                  return row.header ? (
                    <th style={style}>
                      <Segments segments={cell} />
                    </th>
                  ) : (
                    <td style={style}>
                      <Segments segments={cell} />
                    </td>
                  );
                }}
              </For>
            </tr>
          )}
        </For>
      </tbody>
    </table>
  );
}

function SourceView(props: { hunks: Hunk[]; onEdit?: (line?: number) => void }) {
  return (
    <div class="source-diff" role="table" aria-label="Line changes">
      <Show when={props.hunks.length} fallback={<div class="diff-message">No changes.</div>}>
        <For each={props.hunks}>
          {(h) => (
            <div class="hunk" data-change="">
              <div class="hunk-header">
                <span>
                  Lines {h.oldStart}–{h.oldStart + Math.max(0, h.oldLen - 1)} → {h.newStart}–{h.newStart + Math.max(0, h.newLen - 1)}
                </span>
                <Show when={props.onEdit}>
                  <button type="button" class="diff-edit" onClick={() => props.onEdit?.(h.newStart)}>
                    Edit
                  </button>
                </Show>
              </div>
              <For each={h.lines}>
                {(l) => (
                  <div class={`line line-${l.kind}`} role="row">
                    <span class="ln" role="cell">
                      {l.oldNo ?? ""}
                    </span>
                    <span class="ln" role="cell">
                      {l.newNo ?? ""}
                    </span>
                    <span class="sign" aria-hidden="true">
                      {l.kind === "insert" ? "+" : l.kind === "delete" ? "−" : " "}
                    </span>
                    <span class="text" role="cell">
                      <Show when={l.segments.length} fallback={l.text || " "}>
                        <Segments segments={l.segments} />
                      </Show>
                    </span>
                  </div>
                )}
              </For>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}
