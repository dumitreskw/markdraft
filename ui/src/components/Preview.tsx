import { createEffect, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import { decorate, footnoteHtml, renderGraphviz, renderMath, renderMermaid } from "../preview/enhance";
import { toggleTaskAt } from "../editor/format";
import { registerPane, scrolled } from "../editor/sync";
import * as ipc from "../ipc";
import { extension, resolveLink, stem } from "../paths";
import { docs, openDoc, revealRequest } from "../state/docs";
import { effectiveTheme, settings } from "../state/settings";
import { toast, viewMode } from "../state/ui";
import { allDocs, locate } from "../state/workspace";
import { editorView } from "./EditorPane";

interface PreviewProps {
  path: string;
}

/** Preview scroll positions per document, kept across tab switches. */
const scrollMemory = new Map<string, number>();

export function Preview(props: PreviewProps) {
  let scroller!: HTMLDivElement;
  let body!: HTMLDivElement;
  let blocks: { line: number; el: HTMLElement }[] = [];
  let renderedHtml = "";

  const doc = () => docs[props.path];

  const collectBlocks = () => {
    blocks = [];
    for (const el of body.querySelectorAll<HTMLElement>("[data-sourcepos]")) {
      // Only block-level elements give stable positions.
      if (getComputedStyle(el).display.startsWith("inline")) continue;
      const line = Number(el.dataset.sourcepos!.split(":")[0]);
      if (line > 0) blocks.push({ line, el });
    }
  };

  const offsetOf = (el: HTMLElement) =>
    el.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;

  const scrollToLine = (line: number, smooth = false) => {
    if (blocks.length === 0) return;
    let i = 0;
    while (i + 1 < blocks.length && blocks[i + 1].line <= line) i++;
    const a = blocks[i];
    const b = blocks[i + 1];
    let top = offsetOf(a.el);
    if (b && b.line > a.line && line > a.line) {
      top += (offsetOf(b.el) - top) * Math.min(1, (line - a.line) / (b.line - a.line));
    }
    scroller.scrollTo({ top: Math.max(0, top - 16), behavior: smooth ? "smooth" : "auto" });
  };

  const lineAtTop = () => {
    const top = scroller.scrollTop + 16;
    let i = 0;
    while (i + 1 < blocks.length && offsetOf(blocks[i + 1].el) <= top) i++;
    const a = blocks[i];
    const b = blocks[i + 1];
    if (!a) return 1;
    if (!b) return a.line;
    const ta = offsetOf(a.el);
    const tb = offsetOf(b.el);
    const frac = tb > ta ? Math.min(1, Math.max(0, (top - ta) / (tb - ta))) : 0;
    return a.line + (b.line - a.line) * frac;
  };

  const enhance = async () => {
    const d = doc();
    if (!d?.rendered) return;
    collectBlocks();
    const editable = !d.readonly;
    for (const box of body.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')) {
      box.disabled = !editable;
      box.tabIndex = editable ? 0 : -1;
    }
    const theme = effectiveTheme();
    if (d.rendered.hasMath) await renderMath(body);
    if (d.rendered.hasMermaid) await renderMermaid(body, theme);
    if (d.rendered.hasGraphviz) await renderGraphviz(body);
    decorate(body, d.name);
    collectBlocks();
  };

  // Footnote previews: hovering a reference shows the note in place.
  const [note, setNote] = createSignal<{ html: string; x: number; y: number } | null>(null);
  const onOver = (e: MouseEvent) => {
    const ref = (e.target as HTMLElement).closest<HTMLAnchorElement>("a[data-footnote-ref], sup.footnote-ref > a");
    if (!ref) return setNote(null);
    const html = footnoteHtml(body, ref);
    if (!html) return;
    const r = ref.getBoundingClientRect();
    setNote({ html, x: Math.min(r.left, window.innerWidth - 380), y: r.bottom + 6 });
  };

  // Re-render only when the HTML actually changed; keeps scroll position.
  createEffect(
    on(
      () => [doc()?.rendered?.html, effectiveTheme()] as const,
      ([html]) => {
        if (html === undefined) return;
        if (html === renderedHtml && !body.querySelector(".diagram")) return;
        const first = renderedHtml === "";
        renderedHtml = html;
        const top = first ? (scrollMemory.get(props.path) ?? 0) : scroller.scrollTop;
        body.innerHTML = html;
        scroller.scrollTop = top;
        void enhance();
      },
    ),
  );

  createEffect(
    on(revealRequest, (req) => {
      if (!req || req.path !== props.path) return;
      if (req.fragment) scrollToFragment(req.fragment);
      else if (req.line && viewMode() === "read") scrollToLine(req.line, true);
    }),
  );

  const scrollToFragment = (fragment: string) => {
    const target = body.querySelector<HTMLElement>(`[id="${CSS.escape(fragment)}"]`);
    if (target) scroller.scrollTo({ top: offsetOf(target) - 16, behavior: "smooth" });
  };

  const onClick = async (e: MouseEvent) => {
    const target = e.target as HTMLElement;

    if (target instanceof HTMLInputElement && target.type === "checkbox") {
      e.preventDefault();
      toggleTask(target);
      return;
    }

    const anchor = target.closest("a");
    if (!anchor) return;
    const href = anchor.getAttribute("href");
    if (!href) return;
    e.preventDefault();

    if (anchor.dataset.wikilink === "true") {
      openWikiLink(decodeURIComponent(href));
      return;
    }
    if (href.startsWith("#")) {
      scrollToFragment(decodeURIComponent(href.slice(1)));
      return;
    }
    const local = resolveLink(props.path, href);
    if (!local) {
      if (/^(https?|mailto):/i.test(href)) await ipc.openExternal(href).catch((err) => toast(ipc.errorMessage(err), "error"));
      return;
    }
    if (local.path === props.path) {
      if (local.fragment) scrollToFragment(local.fragment);
      return;
    }
    let path = local.path;
    if (!extension(path)) path += ".md";
    if (!locate(path)) {
      toast("That link points outside the open folders.", "info");
      return;
    }
    await openDoc(path, local.fragment ? { fragment: local.fragment } : undefined);
  };

  const openWikiLink = (name: string) => {
    const wanted = name.toLowerCase();
    const match =
      allDocs().find((d) => stem(d.rel).toLowerCase() === wanted) ??
      allDocs().find((d) => d.title?.toLowerCase() === wanted);
    if (match) void openDoc(match.path);
    else toast(`No document named “${name}”.`, "info");
  };

  /** Clicking a task checkbox edits the source line it came from. */
  const toggleTask = (box: HTMLInputElement) => {
    const li = box.closest<HTMLElement>("[data-sourcepos]");
    const line = Number(li?.dataset.sourcepos?.split(":")[0]);
    const view = editorView();
    if (!view || !line || doc()?.readonly) return;
    const spec = toggleTaskAt(view.state, line);
    if (spec) {
      view.dispatch(view.state.update(spec, { userEvent: "input.toggle" }));
      box.checked = !box.checked;
    }
  };

  onMount(() => {
    const unregister = registerPane("preview", { scrollToLine });
    const onScroll = () => scrolled("preview", lineAtTop(), viewMode() === "split");
    scroller.addEventListener("scroll", onScroll, { passive: true });
    onCleanup(() => {
      scrollMemory.set(props.path, scroller.scrollTop);
      unregister();
      scroller.removeEventListener("scroll", onScroll);
    });
  });

  return (
    <div ref={scroller} class="preview-scroller" tabIndex={-1} onMouseOver={onOver} onMouseLeave={() => setNote(null)}>
      <Show when={doc()?.rendered?.frontMatter.filter((f) => f.key !== "title").length}>
        <aside
          class="front-matter"
          classList={{ [`width-${settings.previewWidth}`]: true }}
          aria-label="Document properties"
        >
          <For each={doc()!.rendered!.frontMatter.filter((f) => f.key !== "title")}>
            {(f) => (
              <div class="fm-row">
                <span class="fm-key">{f.key}</span>
                <span class="fm-value">{f.value}</span>
              </div>
            )}
          </For>
        </aside>
      </Show>
      <article
        ref={body}
        class="markdown-body"
        classList={{
          [`width-${settings.previewWidth}`]: true,
          "font-serif": settings.previewFont === "serif",
        }}
        onClick={onClick}
      />
      <Show when={note()}>
        {(n) => <div class="footnote-preview" role="tooltip" style={{ left: `${n().x}px`, top: `${n().y}px` }} innerHTML={n().html} />}
      </Show>
      <Show when={doc() && !doc()!.rendered}>
        <div class="preview-loading" aria-live="polite">
          Rendering…
        </div>
      </Show>
    </div>
  );
}
