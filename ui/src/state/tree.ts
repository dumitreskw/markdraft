// Pure tree logic: turns flat node maps into the visible rows of the file
// tree, applying sorting, filtering and expansion.
import type { Change, TreeNode } from "../ipc";
import { baseName, relParent } from "../paths";

export type SortKey = "name" | "title" | "modified";

export interface TreeOptions {
  sort: SortKey;
  showTitles: boolean;
  hideEmptyDirs: boolean;
  filter: string;
}

export interface RootInput {
  id: number;
  name: string;
  nodes: Map<string, TreeNode>;
}

export interface Row {
  /** Unique across roots: `${rootId}:${path}`. */
  key: string;
  rootId: number;
  /** Relative path ("" for a root row). */
  path: string;
  depth: number;
  isDir: boolean;
  isRoot: boolean;
  /** What the row shows: the title or the file name. */
  label: string;
  /** File name, shown as a hint when the label is a title. */
  name: string;
  expanded: boolean;
  modifiedMs: number;
}

export const rowKey = (rootId: number, path: string) => `${rootId}:${path}`;

/** Applies watcher changes to a node map in place. */
export function applyChanges(nodes: Map<string, TreeNode>, changes: Change[]): boolean {
  let rescan = false;
  for (const c of changes) {
    switch (c.kind) {
      case "added":
      case "modified":
        nodes.set(c.node.path, c.node);
        ensureParents(nodes, c.node.path);
        break;
      case "removed":
        removeTree(nodes, c.path);
        break;
      case "renamed": {
        const prefix = c.from + "/";
        const moved: TreeNode[] = [];
        for (const [p, n] of nodes) {
          if (p.startsWith(prefix)) moved.push({ ...n, path: c.to.path + "/" + p.slice(prefix.length) });
        }
        removeTree(nodes, c.from);
        nodes.set(c.to.path, c.to);
        for (const n of moved) nodes.set(n.path, n);
        ensureParents(nodes, c.to.path);
        break;
      }
      case "rescan":
        rescan = true;
        break;
    }
  }
  return rescan;
}

function removeTree(nodes: Map<string, TreeNode>, path: string) {
  nodes.delete(path);
  const prefix = path + "/";
  for (const p of [...nodes.keys()]) if (p.startsWith(prefix)) nodes.delete(p);
}

/** A file created in a folder the tree has not seen yet implies the folder. */
function ensureParents(nodes: Map<string, TreeNode>, path: string) {
  let parent = relParent(path);
  while (parent && !nodes.has(parent)) {
    nodes.set(parent, { path: parent, isDir: true, title: null, modifiedMs: 0 });
    parent = relParent(parent);
  }
}

interface Index {
  children: Map<string, TreeNode[]>;
  /** Number of documents at or below each folder. */
  docCount: Map<string, number>;
}

function buildIndex(nodes: Map<string, TreeNode>): Index {
  const children = new Map<string, TreeNode[]>();
  const docCount = new Map<string, number>();
  for (const node of nodes.values()) {
    const parent = relParent(node.path);
    let list = children.get(parent);
    if (!list) children.set(parent, (list = []));
    list.push(node);
    if (!node.isDir) {
      let p = parent;
      for (;;) {
        docCount.set(p, (docCount.get(p) ?? 0) + 1);
        if (!p) break;
        p = relParent(p);
      }
    }
  }
  return { children, docCount };
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

export function labelOf(node: TreeNode, showTitles: boolean): string {
  return showTitles && node.title ? node.title : baseName(node.path);
}

function compare(a: TreeNode, b: TreeNode, opts: TreeOptions): number {
  if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
  if (opts.sort === "modified" && !a.isDir) return b.modifiedMs - a.modifiedMs;
  if (opts.sort === "title") return collator.compare(labelOf(a, true), labelOf(b, true));
  return collator.compare(baseName(a.path), baseName(b.path));
}

/** Case-insensitive subsequence match, so "rdme" finds "README". */
export function fuzzyIncludes(haystack: string, needle: string): boolean {
  const h = haystack.toLowerCase();
  const n = needle.toLowerCase().replace(/\s+/g, "");
  let i = 0;
  for (const ch of h) {
    if (ch === n[i]) i++;
    if (i === n.length) return true;
  }
  return n.length === 0;
}

/**
 * The rows to display. With several roots each root is a top-level row;
 * with one root its contents start at depth 0. A filter shows matching
 * documents with their folders expanded.
 */
export function visibleRows(roots: RootInput[], expanded: Set<string>, opts: TreeOptions): Row[] {
  const rows: Row[] = [];
  const filter = opts.filter.trim();
  const showRootRows = roots.length > 1;

  for (const root of roots) {
    const { children, docCount } = buildIndex(root.nodes);
    let matches: Set<string> | null = null;
    if (filter) {
      matches = new Set();
      for (const node of root.nodes.values()) {
        if (node.isDir) continue;
        if (fuzzyIncludes(baseName(node.path), filter) || (node.title && fuzzyIncludes(node.title, filter)) || node.path.toLowerCase().includes(filter.toLowerCase())) {
          matches.add(node.path);
          let p = relParent(node.path);
          while (p) {
            matches.add(p);
            p = relParent(p);
          }
        }
      }
      if (matches.size === 0) continue;
    }

    const rootKey = rowKey(root.id, "");
    const rootOpen = !showRootRows || filter.length > 0 || expanded.has(rootKey);
    if (showRootRows) {
      rows.push({
        key: rootKey,
        rootId: root.id,
        path: "",
        depth: 0,
        isDir: true,
        isRoot: true,
        label: root.name,
        name: root.name,
        expanded: rootOpen,
        modifiedMs: 0,
      });
    }
    if (!rootOpen) continue;

    const walk = (dir: string, depth: number) => {
      const list = [...(children.get(dir) ?? [])].sort((a, b) => compare(a, b, opts));
      for (const node of list) {
        if (matches && !matches.has(node.path)) continue;
        if (node.isDir && opts.hideEmptyDirs && !docCount.get(node.path)) continue;
        const key = rowKey(root.id, node.path);
        const open = node.isDir && (!!matches || expanded.has(key));
        rows.push({
          key,
          rootId: root.id,
          path: node.path,
          depth,
          isDir: node.isDir,
          isRoot: false,
          label: labelOf(node, opts.showTitles),
          name: baseName(node.path),
          expanded: open,
          modifiedMs: node.modifiedMs,
        });
        if (open) walk(node.path, depth + 1);
      }
    };
    walk("", showRootRows ? 1 : 0);
  }
  return rows;
}

/** Number of documents across all roots. */
export function countDocs(roots: RootInput[]): number {
  let n = 0;
  for (const r of roots) for (const node of r.nodes.values()) if (!node.isDir) n++;
  return n;
}
