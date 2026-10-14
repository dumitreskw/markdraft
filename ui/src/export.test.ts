import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), isTauri: () => false }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("./markdown.css?raw", () => ({ default: "" }));

const { mermaidSources } = await import("./export");

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
