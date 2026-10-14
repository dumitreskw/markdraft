import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), isTauri: () => false }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("./markdown.css?raw", () => ({ default: "" }));

vi.mock("./preview/enhance", () => ({ renderDot: vi.fn() }));

const { diagramSources, mermaidSources } = await import("./export");

describe("mermaidSources", () => {
  it("finds mermaid fences in order and ignores other code", () => {
    const md = [
      "# Doc",
      "```mermaid",
      "graph TD; A-->B",
      "```",
      "```js",
      "const x = 1;",
      "```",
      "~~~mermaid",
      "sequenceDiagram",
      "  A->>B: hi",
      "~~~",
    ].join("\n");
    expect(mermaidSources(md)).toEqual(["graph TD; A-->B\n", "sequenceDiagram\n  A->>B: hi\n"]);
  });
});

describe("diagramSources", () => {
  it("numbers mermaid and graphviz fences together, in order", () => {
    const md = "```dot\ndigraph{a->b}\n```\n\n```mermaid\ngraph TD;\n```\n\n```graphviz\ngraph{x}\n```\n";
    expect(diagramSources(md).map((d) => d.lang)).toEqual(["dot", "mermaid", "graphviz"]);
  });
});
