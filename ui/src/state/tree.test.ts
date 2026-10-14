import { describe, expect, it } from "vitest";
import type { TreeNode } from "../ipc";
import { applyChanges, fuzzyIncludes, rowKey, visibleRows, type RootInput, type TreeOptions } from "./tree";

const file = (path: string, title: string | null = null, modifiedMs = 0): TreeNode => ({
  path,
  isDir: false,
  title,
  modifiedMs,
});
const dir = (path: string): TreeNode => ({ path, isDir: true, title: null, modifiedMs: 0 });

const opts = (o: Partial<TreeOptions> = {}): TreeOptions => ({
  sort: "name",
  showTitles: false,
  hideEmptyDirs: true,
  filter: "",
  ...o,
});

function root(nodes: TreeNode[], id = 1, name = "docs"): RootInput {
  return { id, name, nodes: new Map(nodes.map((n) => [n.path, n])) };
}

describe("visibleRows", () => {
  const r = root([
    dir("guide"),
    file("guide/b.md", "Beta"),
    file("guide/a.md", "Zulu"),
    dir("empty"),
    file("README.md", "Read me"),
    file("note10.md"),
    file("note2.md"),
  ]);

  it("lists folders first, sorts naturally and hides empty folders", () => {
    const rows = visibleRows([r], new Set(), opts());
    expect(rows.map((x) => x.path)).toEqual(["guide", "note2.md", "note10.md", "README.md"]);
  });

  it("expands folders and can sort by title", () => {
    const rows = visibleRows([r], new Set([rowKey(1, "guide")]), opts({ sort: "title", showTitles: true }));
    expect(rows.map((x) => x.label)).toEqual(["guide", "Beta", "Zulu", "note2.md", "note10.md", "Read me"]);
    expect(rows[1].depth).toBe(1);
  });

  it("filters by name or title and opens matching folders", () => {
    const rows = visibleRows([r], new Set(), opts({ filter: "zulu" }));
    expect(rows.map((x) => x.path)).toEqual(["guide", "guide/a.md"]);
    expect(rows[0].expanded).toBe(true);
  });

  it("shows root rows when several roots are open", () => {
    const rows = visibleRows([r, root([file("x.md")], 2, "notes")], new Set([rowKey(2, "")]), opts());
    expect(rows.map((x) => x.label)).toEqual(["docs", "notes", "x.md"]);
    expect(rows[2].depth).toBe(1);
  });
});

describe("applyChanges", () => {
  it("adds, renames folders with children, and removes trees", () => {
    const nodes = new Map<string, TreeNode>([["a", dir("a")], ["a/x.md", file("a/x.md")]]);
    applyChanges(nodes, [{ kind: "added", node: file("new/deep/y.md") }]);
    expect(nodes.has("new") && nodes.has("new/deep")).toBe(true);
    applyChanges(nodes, [{ kind: "renamed", from: "a", to: dir("b") }]);
    expect([...nodes.keys()].sort()).toEqual(["b", "b/x.md", "new", "new/deep", "new/deep/y.md"]);
    applyChanges(nodes, [{ kind: "removed", path: "new" }]);
    expect([...nodes.keys()].sort()).toEqual(["b", "b/x.md"]);
    expect(applyChanges(nodes, [{ kind: "rescan" }])).toBe(true);
  });
});

describe("fuzzyIncludes", () => {
  it("matches subsequences case-insensitively", () => {
    expect(fuzzyIncludes("README.md", "rdme")).toBe(true);
    expect(fuzzyIncludes("guide.md", "gx")).toBe(false);
  });
});
