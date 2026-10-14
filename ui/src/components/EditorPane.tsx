import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { createEffect, on, onCleanup, onMount } from "solid-js";
import { minimalChange } from "../editor/format";
import { createExtensions, readOnlyCompartment, settingEffects } from "../editor/setup";
import { registerPane, scrolled } from "../editor/sync";
import {
  activePath,
  docs,
  onExternalReplace,
  registerEditorStateDrop,
  registerEditorStateRekey,
  revealRequest,
  setText,
} from "../state/docs";
import { settings } from "../state/settings";
import { viewMode } from "../state/ui";
import { setCursor } from "../state/cursor";

/** The one live editor view, for commands that act on it. */
let currentView: EditorView | null = null;
export const editorView = () => currentView;

export function focusEditor() {
  currentView?.focus();
}

export function EditorPane() {
  let host!: HTMLDivElement;
  const states = new Map<string, EditorState>();
  const scrollTops = new Map<string, number>();
  let view!: EditorView;
  let shownPath: string | null = null;

  const extensionsFor = (path: string) =>
    createExtensions(settings, docs[path]?.readonly ?? false, {
      onChange: (text) => {
        if (shownPath) setText(shownPath, text);
      },
      onCursor: (line, col, selected) => setCursor({ line, col, selected }),
    });

  const stateFor = (path: string) => {
    let state = states.get(path);
    if (!state) {
      state = EditorState.create({ doc: docs[path]?.text ?? "", extensions: extensionsFor(path) });
      states.set(path, state);
    }
    return state;
  };

  const show = (path: string | null) => {
    if (shownPath) {
      states.set(shownPath, view.state);
      scrollTops.set(shownPath, view.scrollDOM.scrollTop);
    }
    shownPath = path;
    if (!path) return;
    view.setState(stateFor(path));
    // Settings may have changed while this document was in the background.
    view.dispatch({
      effects: [
        ...settingEffects(settings),
        readOnlyCompartment.reconfigure([
          EditorState.readOnly.of(docs[path]?.readonly ?? false),
          EditorView.editable.of(!(docs[path]?.readonly ?? false)),
        ]),
      ],
    });
    requestAnimationFrame(() => {
      view.scrollDOM.scrollTop = scrollTops.get(path) ?? 0;
    });
  };

  const scrollToLine = (line: number, smooth = false) => {
    const doc = view.state.doc;
    const whole = Math.min(Math.max(1, Math.floor(line)), doc.lines);
    const block = view.lineBlockAt(doc.line(whole).from);
    const next = whole < doc.lines ? view.lineBlockAt(doc.line(whole + 1).from) : block;
    const frac = line - Math.floor(line);
    const top = block.top + (next.top - block.top) * frac;
    view.scrollDOM.scrollTo({ top: Math.max(0, top - 8), behavior: smooth ? "smooth" : "auto" });
  };

  onMount(() => {
    view = new EditorView({ parent: host, state: EditorState.create({ doc: "" }) });
    currentView = view;

    const unregister = registerPane("editor", { scrollToLine });
    const onScroll = () => {
      const top = view.scrollDOM.scrollTop;
      const block = view.lineBlockAtHeight(top);
      const line = view.state.doc.lineAt(block.from).number;
      const frac = block.height > 0 ? (top - block.top) / block.height : 0;
      scrolled("editor", line + Math.min(Math.max(frac, 0), 0.99), viewMode() === "split");
    };
    view.scrollDOM.addEventListener("scroll", onScroll, { passive: true });

    const offReplace = onExternalReplace((path, text) => {
      // Replace only the changed span so scroll position and cursor survive.
      const replace = (state: EditorState) =>
        state.update({ changes: minimalChange(state.doc.toString(), text), userEvent: "input.reload" });
      if (path === shownPath) view.dispatch(replace(view.state));
      else if (states.has(path)) states.set(path, replace(states.get(path)!).state);
    });
    registerEditorStateDrop((path) => {
      states.delete(path);
      scrollTops.delete(path);
      if (shownPath === path) shownPath = null;
    });
    registerEditorStateRekey((from, to) => {
      const s = states.get(from);
      if (s) {
        states.delete(from);
        states.set(to, s);
      }
      if (shownPath === from) shownPath = to;
    });

    onCleanup(() => {
      unregister();
      offReplace();
      view.scrollDOM.removeEventListener("scroll", onScroll);
      view.destroy();
      currentView = null;
    });
  });

  createEffect(on(activePath, (path) => show(path)));

  createEffect(
    on(
      () => [settings.lineWrap, settings.lineNumbers, settings.spellcheck, settings.editorFont, settings.editorFontSize],
      () => {
        if (view) view.dispatch({ effects: settingEffects(settings) });
      },
      { defer: true },
    ),
  );

  createEffect(
    on(revealRequest, (req) => {
      if (!req || req.path !== shownPath || !req.line) return;
      const doc = view.state.doc;
      const line = doc.line(Math.min(Math.max(1, req.line), doc.lines));
      view.dispatch({ selection: EditorSelection.cursor(line.from) });
      scrollToLine(req.line, true);
      if (viewMode() !== "read") view.focus();
    }),
  );

  return <div ref={host} class="editor-host" />;
}
