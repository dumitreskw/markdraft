// Markdown formatting commands for CodeMirror. Each works on an
// EditorState and returns the transaction to apply, so they are testable
// without a DOM.
import { EditorSelection, type EditorState, type TransactionSpec } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

/** Wraps each selection in `marker` (e.g. `**`), or unwraps it if already wrapped. */
export function toggleWrap(state: EditorState, marker: string, placeholder = "text"): TransactionSpec {
  const m = marker.length;
  return state.changeByRange((range) => {
    const before = state.sliceDoc(range.from - m, range.from);
    const after = state.sliceDoc(range.to, range.to + m);
    if (before === marker && after === marker) {
      return {
        changes: [
          { from: range.from - m, to: range.from },
          { from: range.to, to: range.to + m },
        ],
        range: EditorSelection.range(range.from - m, range.to - m),
      };
    }
    const selected = state.sliceDoc(range.from, range.to);
    if (selected.length >= 2 * m && selected.startsWith(marker) && selected.endsWith(marker)) {
      const inner = selected.slice(m, -m);
      return {
        changes: { from: range.from, to: range.to, insert: inner },
        range: EditorSelection.range(range.from, range.from + inner.length),
      };
    }
    const text = selected || placeholder;
    return {
      changes: { from: range.from, to: range.to, insert: marker + text + marker },
      range: EditorSelection.range(range.from + m, range.from + m + text.length),
    };
  });
}

/** Turns the selection into a link and selects the URL placeholder. */
export function insertLink(state: EditorState): TransactionSpec {
  return state.changeByRange((range) => {
    const text = state.sliceDoc(range.from, range.to);
    const isUrl = /^https?:\/\/\S+$/.test(text);
    const label = isUrl ? "link" : text || "link";
    const url = isUrl ? text : "https://";
    const insert = `[${label}](${url})`;
    const urlFrom = range.from + label.length + 3;
    return {
      changes: { from: range.from, to: range.to, insert },
      range: isUrl
        ? EditorSelection.range(range.from + 1, range.from + 1 + label.length)
        : EditorSelection.range(urlFrom, urlFrom + url.length),
    };
  });
}

const HEADING = /^(#{1,6})\s+/;
const LIST = /^(\s*)([-*+]|\d+[.)])\s+(\[[ xX]\]\s+)?/;

function eachLine(state: EditorState, fn: (text: string, from: number) => { from: number; to: number; insert: string } | null) {
  const changes: { from: number; to: number; insert: string }[] = [];
  const seen = new Set<number>();
  for (const range of state.selection.ranges) {
    for (let pos = range.from; pos <= range.to; ) {
      const line = state.doc.lineAt(pos);
      if (!seen.has(line.number)) {
        seen.add(line.number);
        const c = fn(line.text, line.from);
        if (c) changes.push(c);
      }
      pos = line.to + 1;
    }
  }
  return changes;
}

/** Sets the heading level of the current lines; the same level again removes it. */
export function setHeading(state: EditorState, level: number): TransactionSpec {
  const prefix = "#".repeat(level) + " ";
  return {
    changes: eachLine(state, (text, from) => {
      const m = HEADING.exec(text);
      if (m && m[1].length === level) return { from, to: from + m[0].length, insert: "" };
      return { from, to: from + (m ? m[0].length : 0), insert: level > 0 ? prefix : "" };
    }),
  };
}

/** Toggles a list marker (`- `, `1. ` or `- [ ] `) on the current lines. */
export function toggleList(state: EditorState, kind: "bullet" | "ordered" | "task"): TransactionSpec {
  let n = 0;
  const lines = eachLine(state, (text) => (LIST.test(text) || !text.trim() ? null : { from: 0, to: 0, insert: "" }));
  const allListed = lines.length === 0;
  return {
    changes: eachLine(state, (text, from) => {
      const m = LIST.exec(text);
      const indent = m?.[1] ?? "";
      const isTask = !!m?.[3];
      const isOrdered = !!m && /\d/.test(m[2]);
      const sameKind = m && (kind === "task" ? isTask : kind === "ordered" ? isOrdered && !isTask : !isOrdered && !isTask);
      if (allListed && sameKind) return { from, to: from + m![0].length, insert: indent };
      if (!text.trim() && !m) return null;
      n++;
      const marker = kind === "bullet" ? "- " : kind === "ordered" ? `${n}. ` : "- [ ] ";
      return { from, to: from + (m ? m[0].length : 0), insert: indent + marker };
    }),
  };
}

/** Toggles `[ ]` ⇄ `[x]` on a task-list line (1-based line number). */
export function toggleTaskAt(state: EditorState, lineNo: number): TransactionSpec | null {
  if (lineNo < 1 || lineNo > state.doc.lines) return null;
  const line = state.doc.line(lineNo);
  const m = /^(\s*(?:[-*+]|\d+[.)])\s+)\[([ xX])\]/.exec(line.text);
  if (!m) return null;
  const at = line.from + m[1].length + 1;
  return { changes: { from: at, to: at + 1, insert: m[2] === " " ? "x" : " " } };
}

/** Inserts a 3×2 table at the cursor, on its own lines. */
export function insertTable(state: EditorState): TransactionSpec {
  const pos = state.selection.main.head;
  const line = state.doc.lineAt(pos);
  const lead = line.text.trim() ? "\n\n" : "";
  const table = "| Column 1 | Column 2 | Column 3 |\n| --- | --- | --- |\n|  |  |  |\n";
  const insertAt = line.text.trim() ? line.to : line.from;
  const cell = insertAt + lead.length + table.indexOf("|  |") + 2;
  return {
    changes: { from: insertAt, insert: lead + table },
    selection: EditorSelection.cursor(cell),
  };
}

/** Wraps the selection in a fenced code block. */
export function insertCodeBlock(state: EditorState): TransactionSpec {
  return state.changeByRange((range) => {
    const text = state.sliceDoc(range.from, range.to);
    const insert = "```\n" + text + (text.endsWith("\n") || !text ? "" : "\n") + "```\n";
    return { changes: { from: range.from, to: range.to, insert }, range: EditorSelection.cursor(range.from + 3) };
  });
}

/** The smallest single replacement turning `a` into `b`. */
export function minimalChange(a: string, b: string) {
  let start = 0;
  const max = Math.min(a.length, b.length);
  while (start < max && a.charCodeAt(start) === b.charCodeAt(start)) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a.charCodeAt(endA - 1) === b.charCodeAt(endB - 1)) {
    endA--;
    endB--;
  }
  return { from: start, to: endA, insert: b.slice(start, endB) };
}

/** Adapts a state transform into a CodeMirror command. */
export const command =
  (fn: (state: EditorState) => TransactionSpec | null) =>
  (view: EditorView): boolean => {
    if (view.state.readOnly) return false;
    const spec = fn(view.state);
    if (!spec) return false;
    view.dispatch(view.state.update(spec, { scrollIntoView: true, userEvent: "input.format" }));
    return true;
  };
