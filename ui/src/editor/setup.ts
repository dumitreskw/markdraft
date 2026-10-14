// CodeMirror configuration: Markdown language, highlighting that follows
// the app theme, keymaps and settings that can change at runtime.
import { autocompletion, closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown, markdownKeymap, markdownLanguage } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting, indentOnInput, bracketMatching } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { yamlFrontmatter } from "@codemirror/lang-yaml";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import {
  EditorView,
  crosshairCursor,
  drawSelection,
  dropCursor,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
  placeholder,
  rectangularSelection,
} from "@codemirror/view";
import { tags as t } from "@lezer/highlight";
import type { Settings } from "../state/settings";
import { command, insertCodeBlock, insertLink, insertTable, setHeading, toggleList, toggleWrap } from "./format";

export const wrapCompartment = new Compartment();
export const gutterCompartment = new Compartment();
export const spellCompartment = new Compartment();
export const readOnlyCompartment = new Compartment();
export const fontCompartment = new Compartment();

const FONTS = {
  mono: "var(--font-mono)",
  sans: "var(--font-ui)",
  serif: "var(--font-serif)",
};

export function fontTheme(s: Pick<Settings, "editorFont" | "editorFontSize">): Extension {
  return EditorView.theme({
    "&": { fontSize: `${s.editorFontSize}px` },
    ".cm-content, .cm-gutters": { fontFamily: FONTS[s.editorFont] },
  });
}

/** Colours come from CSS variables so light/dark switch without reconfiguring. */
const theme = EditorView.theme({
  "&": { height: "100%", color: "var(--text)", backgroundColor: "var(--bg)" },
  ".cm-scroller": { lineHeight: "1.65", overflow: "auto" },
  ".cm-content": { padding: "24px 0 40vh", caretColor: "var(--accent)", maxWidth: "var(--editor-width)", margin: "0 auto" },
  ".cm-line": { padding: "0 28px" },
  "&.cm-focused": { outline: "none" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)", borderLeftWidth: "2px" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "var(--selection) !important",
  },
  ".cm-activeLine": { backgroundColor: "var(--active-line)" },
  ".cm-gutters": { backgroundColor: "var(--bg)", color: "var(--text-faint)", border: "none" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--text-muted)" },
  ".cm-selectionMatch": { backgroundColor: "var(--match)" },
  ".cm-searchMatch": { backgroundColor: "var(--match)", outline: "1px solid var(--accent-soft-strong)" },
  ".cm-searchMatch-selected": { backgroundColor: "var(--accent-soft-strong)" },
  ".cm-panels": { backgroundColor: "var(--bg-pane)", color: "var(--text)", borderColor: "var(--border)" },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--border)" },
  ".cm-panel.cm-search": { padding: "8px 12px", fontFamily: "var(--font-ui)" },
  ".cm-panel.cm-search input, .cm-panel.cm-search button": { fontFamily: "var(--font-ui)", fontSize: "13px" },
  ".cm-textfield": {
    backgroundColor: "var(--bg-raised)",
    border: "1px solid var(--border)",
    borderRadius: "6px",
    color: "var(--text)",
    padding: "3px 6px",
  },
  ".cm-button": {
    backgroundImage: "none",
    backgroundColor: "var(--bg-raised)",
    border: "1px solid var(--border)",
    borderRadius: "6px",
    color: "var(--text)",
  },
  ".cm-placeholder": { color: "var(--text-faint)" },
  ".cm-tooltip": { backgroundColor: "var(--bg-raised)", border: "1px solid var(--border)", borderRadius: "8px" },
});

const highlight = HighlightStyle.define([
  { tag: t.heading1, fontSize: "1.45em", fontWeight: "700", color: "var(--md-heading)" },
  { tag: t.heading2, fontSize: "1.25em", fontWeight: "700", color: "var(--md-heading)" },
  { tag: t.heading3, fontSize: "1.1em", fontWeight: "650", color: "var(--md-heading)" },
  { tag: [t.heading4, t.heading5, t.heading6], fontWeight: "650", color: "var(--md-heading)" },
  { tag: t.strong, fontWeight: "700" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through", color: "var(--text-muted)" },
  { tag: [t.link, t.url], color: "var(--accent)" },
  { tag: t.monospace, fontFamily: "var(--font-mono)", color: "var(--md-code)" },
  { tag: [t.processingInstruction, t.meta], color: "var(--text-faint)" },
  { tag: t.quote, color: "var(--text-muted)", fontStyle: "italic" },
  { tag: t.contentSeparator, color: "var(--text-faint)" },
  // Code inside fences.
  { tag: [t.keyword, t.modifier, t.operatorKeyword], color: "var(--syn-keyword)" },
  { tag: [t.string, t.special(t.string), t.regexp], color: "var(--syn-string)" },
  { tag: [t.number, t.bool, t.null, t.atom], color: "var(--syn-number)" },
  { tag: [t.comment, t.lineComment, t.blockComment], color: "var(--syn-comment)", fontStyle: "italic" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "var(--syn-function)" },
  { tag: [t.typeName, t.className, t.namespace], color: "var(--syn-type)" },
  { tag: [t.propertyName, t.attributeName], color: "var(--syn-property)" },
  { tag: [t.tagName], color: "var(--syn-keyword)" },
]);

const formattingKeymap = keymap.of([
  { key: "Mod-b", run: command((s) => toggleWrap(s, "**")) },
  { key: "Mod-i", run: command((s) => toggleWrap(s, "*")) },
  { key: "Mod-Shift-x", run: command((s) => toggleWrap(s, "~~")) },
  { key: "Mod-e", run: command((s) => toggleWrap(s, "`", "code")) },
  { key: "Mod-k", run: command(insertLink) },
  { key: "Mod-Alt-1", run: command((s) => setHeading(s, 1)) },
  { key: "Mod-Alt-2", run: command((s) => setHeading(s, 2)) },
  { key: "Mod-Alt-3", run: command((s) => setHeading(s, 3)) },
  { key: "Mod-Alt-0", run: command((s) => setHeading(s, 0)) },
  { key: "Mod-Shift-8", run: command((s) => toggleList(s, "bullet")) },
  { key: "Mod-Shift-7", run: command((s) => toggleList(s, "ordered")) },
  { key: "Mod-Shift-9", run: command((s) => toggleList(s, "task")) },
  { key: "Mod-Alt-t", run: command(insertTable) },
  { key: "Mod-Alt-c", run: command(insertCodeBlock) },
]);

export interface EditorHooks {
  onChange: (text: string) => void;
  onCursor: (line: number, col: number, selected: number) => void;
}

export function createExtensions(settings: Settings, readOnly: boolean, hooks: EditorHooks): Extension[] {
  return [
    history(),
    drawSelection(),
    dropCursor(),
    rectangularSelection(),
    crosshairCursor(),
    highlightActiveLine(),
    highlightSelectionMatches(),
    indentOnInput(),
    bracketMatching(),
    closeBrackets(),
    autocompletion({ activateOnTyping: false }),
    search({ top: true }),
    EditorState.allowMultipleSelections.of(true),
    yamlFrontmatter({ content: markdown({ base: markdownLanguage, codeLanguages: languages, addKeymap: true }) }),
    syntaxHighlighting(highlight),
    theme,
    placeholder("Start writing…"),
    formattingKeymap,
    keymap.of([...closeBracketsKeymap, ...markdownKeymap, ...searchKeymap, ...historyKeymap, ...defaultKeymap, indentWithTab]),
    wrapCompartment.of(settings.lineWrap ? EditorView.lineWrapping : []),
    gutterCompartment.of(settings.lineNumbers ? [lineNumbers(), highlightActiveLineGutter()] : []),
    spellCompartment.of(EditorView.contentAttributes.of({ spellcheck: String(settings.spellcheck), autocorrect: "off" })),
    readOnlyCompartment.of([EditorState.readOnly.of(readOnly), EditorView.editable.of(!readOnly)]),
    fontCompartment.of(fontTheme(settings)),
    EditorView.updateListener.of((u) => {
      if (u.docChanged) hooks.onChange(u.state.doc.toString());
      if (u.docChanged || u.selectionSet) {
        const head = u.state.selection.main.head;
        const line = u.state.doc.lineAt(head);
        const selected = u.state.selection.ranges.reduce((n, r) => n + (r.to - r.from), 0);
        hooks.onCursor(line.number, head - line.from + 1, selected);
      }
    }),
  ];
}

export function settingEffects(settings: Settings) {
  return [
    wrapCompartment.reconfigure(settings.lineWrap ? EditorView.lineWrapping : []),
    gutterCompartment.reconfigure(settings.lineNumbers ? [lineNumbers(), highlightActiveLineGutter()] : []),
    spellCompartment.reconfigure(EditorView.contentAttributes.of({ spellcheck: String(settings.spellcheck), autocorrect: "off" })),
    fontCompartment.reconfigure(fontTheme(settings)),
  ];
}
