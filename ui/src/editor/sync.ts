// Scroll synchronisation between the editor and the preview, by source
// line. Whichever side the user scrolls leads; the other follows.
import { createSignal } from "solid-js";

export type Side = "editor" | "preview";

interface Pane {
  scrollToLine(line: number, smooth?: boolean): void;
}

const panes: Partial<Record<Side, Pane>> = {};
let leader: Side | null = null;
let leaderUntil = 0;

/** Topmost visible source line, for the outline's "you are here". */
export const [topLine, setTopLine] = createSignal(1);

export function registerPane(side: Side, pane: Pane) {
  panes[side] = pane;
  return () => {
    if (panes[side] === pane) delete panes[side];
  };
}

/** Called by a pane when the user scrolled it to `line` (may be fractional). */
export function scrolled(side: Side, line: number, enabled: boolean) {
  const now = performance.now();
  // Ignore scroll events caused by our own programmatic scrolling.
  if (leader && leader !== side && now < leaderUntil) return;
  leader = side;
  leaderUntil = now + 150;
  setTopLine(Math.max(1, Math.floor(line)));
  if (!enabled) return;
  const other = side === "editor" ? panes.preview : panes.editor;
  other?.scrollToLine(line);
}

/** Scrolls both panes to a line (outline clicks, search results). */
export function revealLine(line: number) {
  leader = null;
  panes.editor?.scrollToLine(line, true);
  panes.preview?.scrollToLine(line, true);
  setTopLine(line);
}
