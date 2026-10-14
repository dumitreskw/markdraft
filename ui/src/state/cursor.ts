import { createSignal } from "solid-js";

export interface Cursor {
  line: number;
  col: number;
  selected: number;
}

export const [cursor, setCursor] = createSignal<Cursor>({ line: 1, col: 1, selected: 0 });
