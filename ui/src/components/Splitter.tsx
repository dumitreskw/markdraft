import { createSignal } from "solid-js";
import { clamp, widthForKey, type PaneBounds } from "../layout";

interface SplitterProps {
  /** Accessible name, e.g. "Resize file tree". */
  label: string;
  width: number;
  bounds: PaneBounds;
  /** 1 for a pane on the left of the splitter, -1 for one on the right. */
  direction: 1 | -1;
  controls: string;
  onResize: (width: number) => void;
}

/** A focusable, keyboard-operable divider between two panes. */
export function Splitter(props: SplitterProps) {
  const [dragging, setDragging] = createSignal(false);

  const onPointerDown = (e: PointerEvent) => {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startWidth = props.width;
    setDragging(true);
    const move = (ev: PointerEvent) =>
      props.onResize(clamp(startWidth + (ev.clientX - startX) * props.direction, props.bounds));
    const up = () => {
      setDragging(false);
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    const next = widthForKey(e.key, e.shiftKey, props.width, props.bounds, props.direction);
    if (next !== null) {
      e.preventDefault();
      props.onResize(next);
    }
  };

  return (
    <div
      class="splitter"
      classList={{ dragging: dragging() }}
      role="separator"
      tabIndex={0}
      aria-orientation="vertical"
      aria-label={props.label}
      aria-controls={props.controls}
      aria-valuemin={props.bounds.min}
      aria-valuemax={props.bounds.max}
      aria-valuenow={props.width}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
    />
  );
}
