// Turns rendered Markdown HTML into the interactive preview: math, diagrams
// (Mermaid, Graphviz), code-block tools, footnote previews. Heavy libraries
// load only when a document needs them.
import * as ipc from "../ipc";
import { toast } from "../state/ui";

export type Theme = "light" | "dark";

const svgCache = new Map<string, string>();
let diagramSeq = 0;

/** Inline `$…$`, display `$$…$$` and ```math blocks, via KaTeX. */
export async function renderMath(root: HTMLElement) {
  const nodes = [...root.querySelectorAll<HTMLElement>("[data-math-style]")];
  if (!nodes.length) return;
  const [{ default: katex }] = await Promise.all([import("katex"), import("katex/dist/katex.min.css")]);
  for (const el of nodes) {
    const tex = el.textContent ?? "";
    // ```math fences arrive as <pre><code data-math-style>; render them as a block.
    let target = el;
    if (el.tagName === "CODE" && el.parentElement?.tagName === "PRE") {
      const pre = el.parentElement;
      target = document.createElement("div");
      target.className = "math-display";
      target.dataset.sourcepos = pre.dataset.sourcepos ?? "";
      pre.replaceWith(target);
    }
    try {
      katex.render(tex, target, {
        displayMode: el.dataset.mathStyle === "display",
        throwOnError: false,
        output: "htmlAndMathml",
        trust: false,
      });
    } catch {
      target.classList.add("math-error");
      target.textContent = tex;
    }
  }
}

/** Replaces a code block with a diagram holder that keeps its source position. */
function holderFor(pre: HTMLElement, kind: string, source: string): HTMLDivElement {
  const holder = document.createElement("div");
  holder.className = `diagram ${kind}-diagram`;
  holder.dataset.sourcepos = pre.dataset.sourcepos ?? "";
  holder.dataset.source = source;
  return holder;
}

function showError(holder: HTMLElement, e: unknown) {
  holder.classList.add("diagram-error");
  holder.textContent = `Diagram error: ${e instanceof Error ? e.message : String(e)}`;
}

export async function renderMermaid(root: HTMLElement, theme: Theme) {
  const codes = [...root.querySelectorAll<HTMLElement>("pre > code.language-mermaid")];
  if (!codes.length) return;
  const { default: mermaid } = await import("mermaid");
  const mermaidTheme = theme === "dark" ? "dark" : "neutral";
  mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: mermaidTheme, fontFamily: "inherit" });
  for (const code of codes) {
    const source = code.textContent ?? "";
    const pre = code.parentElement!;
    const holder = holderFor(pre, "mermaid", source);
    const key = `mermaid\n${mermaidTheme}\n${source}`;
    try {
      let svg = svgCache.get(key);
      if (!svg) {
        svg = (await mermaid.render(`mermaid-${++diagramSeq}`, source)).svg;
        svgCache.set(key, svg);
      }
      holder.innerHTML = svg;
    } catch (e) {
      showError(holder, e);
    }
    if (pre.isConnected) pre.replaceWith(holder);
  }
}

type GraphvizInstance = { dot(src: string): string };
let graphviz: Promise<GraphvizInstance> | null = null;

/** Graphviz (DOT) through the WebAssembly build of Graphviz. */
export async function renderDot(source: string): Promise<string> {
  graphviz ??= import("@hpcc-js/wasm-graphviz").then(({ Graphviz }) => Graphviz.load());
  const gv = await graphviz;
  // Transparent canvas instead of Graphviz's white page background.
  return gv.dot(source).replace(/(<g id="graph0"[^>]*>\s*(?:<title>[^<]*<\/title>\s*)?<polygon )fill="white"/, '$1fill="none"');
}

export async function renderGraphviz(root: HTMLElement) {
  const codes = [...root.querySelectorAll<HTMLElement>("pre > code.language-dot, pre > code.language-graphviz")];
  if (!codes.length) return;
  for (const code of codes) {
    const source = code.textContent ?? "";
    const pre = code.parentElement!;
    const holder = holderFor(pre, "graphviz", source);
    const key = `dot\n${source}`;
    try {
      let svg = svgCache.get(key);
      if (!svg) {
        svg = await renderDot(source);
        svgCache.set(key, svg);
      }
      holder.innerHTML = svg;
    } catch (e) {
      showError(holder, e);
    }
    if (pre.isConnected) pre.replaceWith(holder);
  }
}

/** Language label and copy button on code blocks; tools on diagrams. */
export function decorate(root: HTMLElement, docName: string) {
  for (const pre of root.querySelectorAll<HTMLElement>("pre")) {
    if (pre.parentElement?.classList.contains("code-block")) continue;
    const code = pre.querySelector("code");
    const lang = /language-([\w+#.-]+)/.exec(code?.className ?? "")?.[1] ?? "";
    const wrap = document.createElement("div");
    wrap.className = "code-block";
    pre.replaceWith(wrap);
    wrap.append(pre);
    const bar = document.createElement("div");
    bar.className = "code-tools";
    if (lang) {
      const label = document.createElement("span");
      label.className = "code-lang";
      label.textContent = lang;
      bar.append(label);
    }
    bar.append(
      button("Copy", "Copy code", async (b) => {
        await navigator.clipboard.writeText(code?.textContent ?? pre.textContent ?? "");
        flash(b, "Copied");
      }),
    );
    wrap.append(bar);
  }

  for (const holder of root.querySelectorAll<HTMLElement>(".diagram:not(.diagram-error)")) {
    if (holder.querySelector(".diagram-tools")) continue;
    const bar = document.createElement("div");
    bar.className = "diagram-tools";
    const svg = () => holder.querySelector("svg")?.outerHTML ?? "";
    bar.append(
      button("Copy SVG", "Copy the diagram as SVG", async (b) => {
        await navigator.clipboard.writeText(svg());
        flash(b, "Copied");
      }),
      button("Save SVG…", "Save the diagram as an SVG file", async () => {
        const base = docName.replace(/\.[^.]+$/, "");
        const out = await ipc.pickSavePath("svg", `${base}-diagram.svg`);
        if (!out) return;
        try {
          await ipc.writeExport(out, svg());
          toast("Diagram saved.", "success");
        } catch (e) {
          toast(ipc.errorMessage(e), "error");
        }
      }),
      button("Copy source", "Copy the diagram source", async (b) => {
        await navigator.clipboard.writeText(holder.dataset.source ?? "");
        flash(b, "Copied");
      }),
    );
    holder.append(bar);
  }
}

function button(label: string, title: string, run: (b: HTMLButtonElement) => void | Promise<void>) {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "tool-btn";
  b.textContent = label;
  b.title = title;
  b.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    void Promise.resolve(run(b)).catch((err) => toast(ipc.errorMessage(err), "error"));
  });
  return b;
}

function flash(b: HTMLButtonElement, text: string) {
  const old = b.textContent;
  b.textContent = text;
  setTimeout(() => (b.textContent = old), 1200);
}

/** The footnote text for a footnote reference link, without its back-link. */
export function footnoteHtml(root: HTMLElement, ref: HTMLAnchorElement): string | null {
  const id = decodeURIComponent((ref.getAttribute("href") ?? "").replace(/^#/, ""));
  if (!id) return null;
  const note = root.querySelector<HTMLElement>(`[id="${CSS.escape(id)}"]`);
  if (!note) return null;
  const copy = note.cloneNode(true) as HTMLElement;
  copy.querySelectorAll("[data-footnote-backref], .footnote-backref").forEach((n) => n.remove());
  return copy.innerHTML.trim() || null;
}
