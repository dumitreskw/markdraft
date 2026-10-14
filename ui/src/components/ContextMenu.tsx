import { createSignal, For, onCleanup, onMount, Show } from "solid-js";

export type MenuItem =
  | { separator: true }
  | { label: string; shortcut?: string; danger?: boolean; disabled?: boolean; run: () => void; separator?: false };

interface MenuState {
  items: MenuItem[];
  x: number;
  y: number;
  restoreFocus: HTMLElement | null;
}

const [menu, setMenu] = createSignal<MenuState | null>(null);

export function openContextMenu(items: MenuItem[], x: number, y: number) {
  setMenu({ items, x, y, restoreFocus: document.activeElement as HTMLElement | null });
}

function close() {
  const m = menu();
  setMenu(null);
  requestAnimationFrame(() => m?.restoreFocus?.focus?.());
}

/** Renders the open context menu, kept inside the window. */
export function ContextMenuHost() {
  return (
    <Show when={menu()}>
      {(m) => <MenuView state={m()} />}
    </Show>
  );
}

function MenuView(props: { state: MenuState }) {
  let el!: HTMLDivElement;
  const [pos, setPos] = createSignal({ x: props.state.x, y: props.state.y });

  const buttons = () => [...el.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];

  onMount(() => {
    const rect = el.getBoundingClientRect();
    setPos({
      x: Math.min(props.state.x, window.innerWidth - rect.width - 8),
      y: Math.min(props.state.y, window.innerHeight - rect.height - 8),
    });
    buttons()[0]?.focus();
    const onDown = (e: PointerEvent) => {
      if (!el.contains(e.target as Node)) close();
    };
    const onBlur = () => close();
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("blur", onBlur);
    onCleanup(() => {
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("blur", onBlur);
    });
  });

  const onKeyDown = (e: KeyboardEvent) => {
    const list = buttons();
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "Escape") close();
    else if (e.key === "ArrowDown") list[(i + 1) % list.length]?.focus();
    else if (e.key === "ArrowUp") list[(i - 1 + list.length) % list.length]?.focus();
    else if (e.key === "Home") list[0]?.focus();
    else if (e.key === "End") list[list.length - 1]?.focus();
    else if (e.key === "Tab") close();
    else return;
    e.preventDefault();
    e.stopPropagation();
  };

  return (
    <div
      ref={el}
      class="context-menu"
      role="menu"
      style={{ left: `${pos().x}px`, top: `${pos().y}px` }}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => e.preventDefault()}
    >
      <For each={props.state.items}>
        {(item) =>
          item.separator ? (
            <div class="menu-separator" role="separator" />
          ) : (
            <button
              type="button"
              role="menuitem"
              class="menu-item"
              classList={{ danger: !!item.danger }}
              disabled={item.disabled}
              onClick={() => {
                close();
                item.run();
              }}
            >
              <span>{item.label}</span>
              <Show when={item.shortcut}>
                <kbd>{item.shortcut}</kbd>
              </Show>
            </button>
          )
        }
      </For>
    </div>
  );
}
