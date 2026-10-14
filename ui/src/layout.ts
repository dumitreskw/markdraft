// Pane sizing rules shared by the splitters and their tests.

export interface PaneBounds {
  min: number;
  max: number;
}

export const TREE_BOUNDS: PaneBounds = { min: 180, max: 520 };
export const OUTLINE_BOUNDS: PaneBounds = { min: 160, max: 420 };

/** Keyboard step in pixels; Shift makes it larger. */
export const KEY_STEP = 16;
export const KEY_STEP_LARGE = 64;

export function clamp(value: number, { min, max }: PaneBounds): number {
  return Math.min(max, Math.max(min, Math.round(value)));
}

/**
 * New width after a keyboard press on a splitter, or null when the key is
 * not a resize key. `direction` is 1 when growing to the right widens the
 * pane (left pane) and -1 when it narrows it (right pane).
 */
export function widthForKey(
  key: string,
  shift: boolean,
  width: number,
  bounds: PaneBounds,
  direction: 1 | -1,
): number | null {
  const step = shift ? KEY_STEP_LARGE : KEY_STEP;
  switch (key) {
    case "ArrowLeft":
      return clamp(width - step * direction, bounds);
    case "ArrowRight":
      return clamp(width + step * direction, bounds);
    case "Home":
      return bounds.min;
    case "End":
      return bounds.max;
    default:
      return null;
  }
}

/** Ctrl on Windows/Linux, Cmd on macOS. */
export function isPrimaryModifier(e: { ctrlKey: boolean; metaKey: boolean }, mac: boolean): boolean {
  return mac ? e.metaKey : e.ctrlKey;
}
