import { createEffect, createMemo, createSignal, For, on, onMount, Show } from "solid-js";
import { commands, displayKeys, type Command } from "../commands";
import * as ipc from "../ipc";
import { relativeTo } from "../paths";
import { docs, openDoc, recentDocs } from "../state/docs";
import { setModal } from "../state/ui";
import { roots } from "../state/workspace";
import { fuzzyIncludes } from "../state/tree";

interface Item {
  key: string;
  title: string;
  detail?: string;
  hint?: string;
  titleIndices?: number[];
  detailIndices?: number[];
  run: () => void;
}

/** Quick open (Mod+P) and the command palette (Mod+Shift+P or a leading ">"). */
export function QuickOpen(props: { commands?: boolean }) {
  const [query, setQuery] = createSignal(props.commands ? ">" : "");
  const [files, setFiles] = createSignal<ipc.QuickOpenItem[]>([]);
  const [index, setIndex] = createSignal(0);
  let input!: HTMLInputElement;
  let list!: HTMLUListElement;

  const commandMode = () => query().startsWith(">");

  createEffect(
    on(query, async (q) => {
      setIndex(0);
      if (q.startsWith(">") || !q.trim()) {
        setFiles([]);
        return;
      }
      try {
        setFiles(await ipc.quickOpen(q.trim(), 50));
      } catch {
        setFiles([]);
      }
    }),
  );

  const relOf = (path: string) => {
    for (const r of roots()) {
      const rel = relativeTo(r.path, path);
      if (rel !== null) return rel;
    }
    return path;
  };

  const items = createMemo<Item[]>(() => {
    if (commandMode()) {
      const q = query().slice(1).trim();
      return commands
        .filter((c: Command) => (!c.when || c.when()) && (!q || fuzzyIncludes(`${c.category} ${c.title}`, q)))
        .map((c) => ({
          key: c.id,
          title: c.title,
          detail: c.category,
          hint: c.keys ? displayKeys(c.keys) : undefined,
          run: () => void c.run(),
        }));
    }
    if (!query().trim()) {
      return recentDocs()
        .filter((p) => docs[p])
        .map((p) => ({ key: p, title: docs[p].rendered?.title ?? docs[p].name, detail: relOf(p), hint: "open", run: () => void openDoc(p) }));
    }
    return files().map((f) => ({
      key: f.path,
      title: f.title ?? f.display.split("/").pop()!,
      detail: f.display,
      titleIndices: f.title ? f.titleIndices : undefined,
      detailIndices: f.displayIndices,
      run: () => void openDoc(f.path),
    }));
  });

  const choose = (item: Item | undefined) => {
    if (!item) return;
    setModal(null);
    item.run();
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const n = items().length;
    if (e.key === "ArrowDown") setIndex((i) => (n ? (i + 1) % n : 0));
    else if (e.key === "ArrowUp") setIndex((i) => (n ? (i - 1 + n) % n : 0));
    else if (e.key === "Enter") choose(items()[index()]);
    else if (e.key === "Escape") setModal(null);
    else return;
    e.preventDefault();
    list.querySelector(`[data-index="${index()}"]`)?.scrollIntoView({ block: "nearest" });
  };

  onMount(() => {
    input.focus();
    input.setSelectionRange(input.value.length, input.value.length);
  });

  return (
    <div class="quick-open" role="dialog" aria-label={commandMode() ? "Command palette" : "Go to document"}>
      <input
        ref={input}
        class="quick-input"
        role="combobox"
        aria-expanded="true"
        aria-controls="quick-list"
        aria-activedescendant={`quick-item-${index()}`}
        placeholder={commandMode() ? "Type a command" : "Go to document — type a name or title (> for commands)"}
        value={query()}
        onInput={(e) => setQuery(e.currentTarget.value)}
        onKeyDown={onKeyDown}
      />
      <ul ref={list} id="quick-list" class="quick-list" role="listbox">
        <For each={items()}>
          {(item, i) => (
            <li
              id={`quick-item-${i()}`}
              data-index={i()}
              role="option"
              aria-selected={i() === index()}
              class="quick-item"
              classList={{ selected: i() === index() }}
              onMouseMove={() => setIndex(i())}
              onClick={() => choose(item)}
            >
              <span class="quick-title">{marked(item.title, item.titleIndices)}</span>
              <Show when={item.detail}>
                <span class="quick-detail">{marked(item.detail!, item.detailIndices)}</span>
              </Show>
              <Show when={item.hint}>
                <kbd class="quick-hint">{item.hint}</kbd>
              </Show>
            </li>
          )}
        </For>
        <Show when={items().length === 0}>
          <li class="quick-empty">{commandMode() ? "No matching commands" : query().trim() ? "No matching documents" : "Type to search documents"}</li>
        </Show>
      </ul>
    </div>
  );
}

/** Highlights matched characters (indices are char positions). */
function marked(text: string, indices?: number[]) {
  if (!indices?.length) return text;
  const set = new Set(indices);
  const chars = [...text];
  const out = [];
  let run = "";
  let runMarked = false;
  chars.forEach((ch, i) => {
    const m = set.has(i);
    if (m !== runMarked && run) {
      out.push(runMarked ? <b>{run}</b> : run);
      run = "";
    }
    runMarked = m;
    run += ch;
  });
  if (run) out.push(runMarked ? <b>{run}</b> : run);
  return out;
}
