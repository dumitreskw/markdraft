// Export helpers: PDF goes through Rust (Typst); HTML is assembled here from
// the rendered document and written by Rust.
import * as ipc from "./ipc";
import { stem } from "./paths";
import { docs } from "./state/docs";
import markdownCss from "./markdown.css?raw";
import { renderDot } from "./preview/enhance";

export interface PdfOptions {
  theme: "clean" | "academic" | "compact";
  paper: "A4" | "Letter";
  toc: boolean;
  smart?: boolean;
}

/** Diagram fences (mermaid, dot, graphviz) in document order — the same
 *  order the exporter numbers them in. */
export function diagramSources(markdown: string): { lang: string; source: string }[] {
  const out: { lang: string; source: string }[] = [];
  const re = /^(\s{0,3})(`{3,}|~{3,})\s*(mermaid|dot|graphviz)\b[^\n]*\n([\s\S]*?)^\1?\2[`~]*\s*$/gm;
  let m: RegExpExecArray | null;
  while ((m = re.exec(markdown))) out.push({ lang: m[3], source: m[4] });
  return out;
}

/** Only the Mermaid sources. */
export function mermaidSources(markdown: string): string[] {
  return diagramSources(markdown)
    .filter((d) => d.lang === "mermaid")
    .map((d) => d.source);
}

async function renderDiagrams(markdown: string): Promise<Record<number, string>> {
  const sources = diagramSources(markdown);
  if (!sources.length) return {};
  const out: Record<number, string> = {};
  let mermaid: (typeof import("mermaid"))["default"] | null = null;
  for (const [i, d] of sources.entries()) {
    try {
      if (d.lang === "mermaid") {
        if (!mermaid) {
          mermaid = (await import("mermaid")).default;
          // Light theme for print; plain-text labels: the PDF renderer cannot draw HTML labels.
          mermaid.initialize({
            startOnLoad: false,
            securityLevel: "strict",
            theme: "neutral",
            htmlLabels: false,
            flowchart: { htmlLabels: false },
          });
        }
        out[i] = (await mermaid.render(`export-diagram-${i}-${Date.now()}`, d.source)).svg;
      } else {
        out[i] = await renderDot(d.source);
      }
    } catch {
      // The exporter falls back to showing the diagram source.
    }
  }
  return out;
}

export async function exportPdf(path: string, out: string, options: PdfOptions) {
  const text = docs[path]?.text ?? (await ipc.readFile(path)).text;
  const diagrams = await renderDiagrams(text);
  await ipc.exportPdf(path, text, out, options, diagrams);
}

/** Turns app-only image URLs back into file URLs that work outside the app. */
function portableImages(root: HTMLElement) {
  for (const img of root.querySelectorAll<HTMLImageElement>("img")) {
    const src = img.getAttribute("src") ?? "";
    const m = /^(?:http:\/\/mdasset\.localhost|mdasset:\/\/localhost)\/(.+)$/.exec(src);
    if (!m) continue;
    const path = decodeURIComponent(m[1]).replace(/\\/g, "/");
    img.setAttribute("src", "file:///" + path.replace(/^\/+/, "").split("/").map(encodeURIComponent).join("/").replace(/^([A-Za-z])%3A/, "$1:"));
  }
}

export async function exportHtml(path: string, out: string) {
  const doc = docs[path];
  const text = doc?.text ?? (await ipc.readFile(path)).text;
  const rendered = await ipc.renderMarkdown(text, path, true);
  const holder = document.createElement("article");
  holder.className = "markdown-body";
  holder.innerHTML = rendered.html;
  // Render math and diagrams into the page so it needs no scripts.
  if (rendered.hasMath) {
    const { default: katex } = await import("katex");
    for (const el of holder.querySelectorAll<HTMLElement>("[data-math-style]")) {
      el.innerHTML = katex.renderToString(el.textContent ?? "", {
        displayMode: el.dataset.mathStyle === "display",
        throwOnError: false,
        output: "mathml",
      });
    }
  }
  if (rendered.hasMermaid || rendered.hasGraphviz) {
    const svgs = await renderDiagrams(text);
    const fences = "pre > code.language-mermaid, pre > code.language-dot, pre > code.language-graphviz";
    holder.querySelectorAll(fences).forEach((code, i) => {
      if (!svgs[i]) return;
      const div = document.createElement("div");
      div.className = "diagram";
      div.innerHTML = svgs[i];
      code.parentElement!.replaceWith(div);
    });
  }
  for (const el of holder.querySelectorAll("[data-sourcepos]")) el.removeAttribute("data-sourcepos");
  portableImages(holder);
  const highlight = await ipc.highlightCss();
  const title = rendered.title ?? stem(path);
  const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="MarkDraft">
<title>${escapeHtml(title)}</title>
<style>
:root{--bg:#fff;--text:#1f1d1a;--text-muted:#6b665d;--border:#e2ded6;--bg-pane:#f6f4ef;--accent:#2f6f62;--md-code-bg:#f3f1ec;--font-ui:system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;--font-serif:Georgia,"Times New Roman",serif;--font-mono:ui-monospace,"SF Mono","Cascadia Code",Consolas,monospace}
body{margin:0;background:var(--bg);color:var(--text);font:16px/1.7 var(--font-ui)}
${markdownCss}
${highlight.light}
.markdown-body{margin:48px auto;padding:0 24px}
</style>
</head>
<body>
${holder.outerHTML}
</body>
</html>
`;
  await ipc.writeExport(out, html);
}

function escapeHtml(s: string) {
  return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);
}
