import { EditorSelection, EditorState, type TransactionSpec } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { insertLink, insertTable, minimalChange, setHeading, toggleList, toggleTaskAt, toggleWrap } from "./format";

function state(doc: string, from: number, to = from) {
  return EditorState.create({ doc, selection: EditorSelection.single(from, to) });
}

function apply(s: EditorState, spec: TransactionSpec) {
  return s.update(spec).state;
}

describe("toggleWrap", () => {
  it("wraps and unwraps a selection", () => {
    const s = state("make bold here", 5, 9);
    const wrapped = apply(s, toggleWrap(s, "**"));
    expect(wrapped.doc.toString()).toBe("make **bold** here");
    expect(wrapped.sliceDoc(wrapped.selection.main.from, wrapped.selection.main.to)).toBe("bold");
    expect(apply(wrapped, toggleWrap(wrapped, "**")).doc.toString()).toBe("make bold here");
  });

  it("inserts a placeholder for an empty selection", () => {
    const s = state("x ", 2);
    const out = apply(s, toggleWrap(s, "*"));
    expect(out.doc.toString()).toBe("x *text*");
    expect(out.sliceDoc(out.selection.main.from, out.selection.main.to)).toBe("text");
  });
});

describe("insertLink", () => {
  it("uses selected text as the label and selects the URL", () => {
    const s = state("see docs now", 4, 8);
    const out = apply(s, insertLink(s));
    expect(out.doc.toString()).toBe("see [docs](https://) now");
    expect(out.sliceDoc(out.selection.main.from, out.selection.main.to)).toBe("https://");
  });

  it("uses a selected URL as the target", () => {
    const s = state("https://a.b", 0, 11);
    expect(apply(s, insertLink(s)).doc.toString()).toBe("[link](https://a.b)");
  });
});

describe("headings and lists", () => {
  it("sets, changes and removes heading levels", () => {
    const s = state("Title", 0);
    const h2 = apply(s, setHeading(s, 2));
    expect(h2.doc.toString()).toBe("## Title");
    const h3 = apply(h2, setHeading(h2, 3));
    expect(h3.doc.toString()).toBe("### Title");
    expect(apply(h3, setHeading(h3, 3)).doc.toString()).toBe("Title");
  });

  it("toggles lists over several lines", () => {
    const s = state("a\nb\n\nc", 0, 6);
    const bullets = apply(s, toggleList(s, "bullet"));
    expect(bullets.doc.toString()).toBe("- a\n- b\n\n- c");
    const all = EditorState.create({ doc: bullets.doc, selection: EditorSelection.single(0, bullets.doc.length) });
    expect(apply(all, toggleList(all, "bullet")).doc.toString()).toBe("a\nb\n\nc");
    const numbered = apply(s, toggleList(s, "ordered"));
    expect(numbered.doc.toString()).toBe("1. a\n2. b\n\n3. c");
  });

  it("toggles task checkboxes by line", () => {
    const s = state("- [ ] one\n- [x] two\ntext", 0);
    const out = apply(s, toggleTaskAt(s, 1)!);
    expect(out.doc.toString()).toBe("- [x] one\n- [x] two\ntext");
    expect(apply(out, toggleTaskAt(out, 2)!).doc.toString()).toBe("- [x] one\n- [ ] two\ntext");
    expect(toggleTaskAt(s, 3)).toBeNull();
  });

  it("inserts a table on its own lines", () => {
    const s = state("para", 4);
    const out = apply(s, insertTable(s));
    expect(out.doc.toString().startsWith("para\n\n| Column 1")).toBe(true);
  });
});

describe("minimalChange", () => {
  it("replaces only the differing middle", () => {
    expect(minimalChange("abcXdef", "abcYYdef")).toEqual({ from: 3, to: 4, insert: "YY" });
    expect(minimalChange("same", "same")).toEqual({ from: 4, to: 4, insert: "" });
    expect(minimalChange("", "new")).toEqual({ from: 0, to: 0, insert: "new" });
    expect(minimalChange("aaa", "aa")).toEqual({ from: 2, to: 3, insert: "" });
  });
});
