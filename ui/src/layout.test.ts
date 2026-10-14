import { describe, expect, it } from "vitest";
import { clamp, isPrimaryModifier, KEY_STEP, KEY_STEP_LARGE, TREE_BOUNDS, widthForKey } from "./layout";

describe("clamp", () => {
  it("keeps widths inside the bounds", () => {
    expect(clamp(10, TREE_BOUNDS)).toBe(TREE_BOUNDS.min);
    expect(clamp(9999, TREE_BOUNDS)).toBe(TREE_BOUNDS.max);
    expect(clamp(300.4, TREE_BOUNDS)).toBe(300);
  });
});

describe("widthForKey", () => {
  it("grows a left pane with ArrowRight and shrinks a right pane", () => {
    expect(widthForKey("ArrowRight", false, 300, TREE_BOUNDS, 1)).toBe(300 + KEY_STEP);
    expect(widthForKey("ArrowRight", false, 300, TREE_BOUNDS, -1)).toBe(300 - KEY_STEP);
  });

  it("uses the large step with Shift and jumps with Home/End", () => {
    expect(widthForKey("ArrowLeft", true, 300, TREE_BOUNDS, 1)).toBe(300 - KEY_STEP_LARGE);
    expect(widthForKey("Home", false, 300, TREE_BOUNDS, 1)).toBe(TREE_BOUNDS.min);
    expect(widthForKey("End", false, 300, TREE_BOUNDS, 1)).toBe(TREE_BOUNDS.max);
  });

  it("ignores other keys", () => {
    expect(widthForKey("Enter", false, 300, TREE_BOUNDS, 1)).toBeNull();
  });
});

describe("isPrimaryModifier", () => {
  it("maps to Cmd on macOS and Ctrl elsewhere", () => {
    expect(isPrimaryModifier({ ctrlKey: false, metaKey: true }, true)).toBe(true);
    expect(isPrimaryModifier({ ctrlKey: true, metaKey: false }, true)).toBe(false);
    expect(isPrimaryModifier({ ctrlKey: true, metaKey: false }, false)).toBe(true);
  });
});
