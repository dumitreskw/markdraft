import { describe, expect, it, vi } from "vitest";

vi.mock("./components/EditorPane", () => ({ editorView: () => null, focusEditor: () => {} }));
vi.mock("./components/DiffView", () => ({ openDiff: () => {}, openHistory: () => {} }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), isTauri: () => false }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const { commands, displayKeys, matches, parseKeys } = await import("./commands");

function key(init: Partial<KeyboardEventInit> & { key: string; code?: string }) {
  return new KeyboardEvent("keydown", { code: "", ...init });
}

describe("key bindings", () => {
  it("maps Mod to Ctrl elsewhere and Cmd on macOS", () => {
    const b = parseKeys("Mod+Shift+P");
    expect(matches(b, key({ key: "P", ctrlKey: true, shiftKey: true }), false)).toBe(true);
    expect(matches(b, key({ key: "P", metaKey: true, shiftKey: true }), true)).toBe(true);
    expect(matches(b, key({ key: "P", ctrlKey: true, shiftKey: true }), true)).toBe(false);
    expect(matches(b, key({ key: "p", ctrlKey: true }), false)).toBe(false);
  });

  it("matches symbol keys by physical code", () => {
    expect(matches(parseKeys("Mod+\\"), key({ key: "\\", code: "Backslash", ctrlKey: true }), false)).toBe(true);
    expect(matches(parseKeys("Mod+="), key({ key: "+", code: "Equal", ctrlKey: true }), false)).toBe(true);
    expect(matches(parseKeys("Mod+1"), key({ key: "1", code: "Digit1", ctrlKey: true }), false)).toBe(true);
  });

  it("formats bindings per platform", () => {
    expect(displayKeys("Mod+Shift+P", false)).toBe("Ctrl+Shift+P");
    expect(displayKeys("Mod+Shift+P", true)).toBe("⇧⌘P");
    expect(displayKeys("F11", false)).toBe("F11");
  });

  it("has no duplicate bindings or ids", () => {
    const ids = commands.map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
    const keys = commands.filter((c) => c.keys).map((c) => c.keys!.toLowerCase());
    expect(new Set(keys).size).toBe(keys.length);
  });
});
