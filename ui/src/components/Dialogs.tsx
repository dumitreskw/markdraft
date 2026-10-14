import { createResource, createSignal, For, Match, onCleanup, onMount, Show, Switch, type JSX } from "solid-js";
import * as ipc from "../ipc";
import { baseName, stem } from "../paths";
import { docs } from "../state/docs";
import { settings, updateSettings, type Settings } from "../state/settings";
import { dismissToast, modal, setModal, toast, toasts } from "../state/ui";
import { opened, roots, saveWorkspaceAs } from "../state/workspace";
import { exportHtml, exportPdf } from "../export";
import { QuickOpen } from "./QuickOpen";

/** Renders whichever dialog is open. */
export function ModalHost() {
  return (
    <Show when={modal()}>
      {(m) => (
        <Switch>
          <Match when={m().kind === "quickOpen"}>
            <Backdrop onClose={() => setModal(null)} top>
              <QuickOpen commands={(m() as { commands?: boolean }).commands} />
            </Backdrop>
          </Match>
          <Match when={m().kind === "confirm" && m()}>
            {(c) => <ConfirmDialog modal={c() as Extract<ReturnType<typeof modal>, { kind: "confirm" }>} />}
          </Match>
          <Match when={m().kind === "prompt" && m()}>
            {(p) => <PromptDialog modal={p() as Extract<ReturnType<typeof modal>, { kind: "prompt" }>} />}
          </Match>
          <Match when={m().kind === "settings"}>
            <SettingsDialog />
          </Match>
          <Match when={m().kind === "saveWorkspace"}>
            <SaveWorkspaceDialog />
          </Match>
          <Match when={m().kind === "export" && m()}>
            {(e) => <ExportDialog path={(e() as { path: string }).path} />}
          </Match>
          <Match when={m().kind === "about"}>
            <AboutDialog />
          </Match>
        </Switch>
      )}
    </Show>
  );
}

function Backdrop(props: { children: JSX.Element; onClose: () => void; top?: boolean }) {
  let box!: HTMLDivElement;
  const previous = document.activeElement as HTMLElement | null;
  onMount(() => {
    const focusable = box.querySelector<HTMLElement>("[autofocus], input, select, button, textarea");
    focusable?.focus();
  });
  // Restore focus after the current key event, or the Enter that closed the
  // dialog would also "click" the button that opened it.
  onCleanup(() => requestAnimationFrame(() => previous?.focus?.()));
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      props.onClose();
    } else if (e.key === "Tab") {
      // Keep focus inside the dialog.
      const items = [...box.querySelectorAll<HTMLElement>("input, select, button, textarea, [tabindex='0']")].filter((el) => !el.hasAttribute("disabled"));
      if (!items.length) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        last.focus();
        e.preventDefault();
      } else if (!e.shiftKey && document.activeElement === last) {
        first.focus();
        e.preventDefault();
      }
    }
  };
  return (
    <div class="backdrop" classList={{ top: !!props.top }} onPointerDown={(e) => e.target === e.currentTarget && props.onClose()} onKeyDown={onKeyDown}>
      <div ref={box} class="dialog-frame">
        {props.children}
      </div>
    </div>
  );
}

function Dialog(props: { title: string; children: JSX.Element; footer?: JSX.Element; onClose: () => void; wide?: boolean }) {
  return (
    <Backdrop onClose={props.onClose}>
      <div class="dialog" classList={{ wide: !!props.wide }} role="dialog" aria-modal="true" aria-labelledby="dialog-title">
        <h2 id="dialog-title">{props.title}</h2>
        <div class="dialog-body">{props.children}</div>
        <Show when={props.footer}>
          <div class="dialog-footer">{props.footer}</div>
        </Show>
      </div>
    </Backdrop>
  );
}

function ConfirmDialog(props: { modal: Extract<ReturnType<typeof modal>, { kind: "confirm" }> }) {
  const done = (value: string | null) => {
    setModal(null);
    props.modal.resolve(value);
  };
  return (
    <Dialog
      title={props.modal.title}
      onClose={() => done(null)}
      footer={
        <For each={props.modal.actions}>
          {(a) => (
            <button
              type="button"
              class="btn"
              classList={{ primary: !!a.primary, danger: !!a.danger }}
              autofocus={a.primary}
              onClick={() => done(a.value)}
            >
              {a.label}
            </button>
          )}
        </For>
      }
    >
      <p>{props.modal.message}</p>
    </Dialog>
  );
}

function PromptDialog(props: { modal: Extract<ReturnType<typeof modal>, { kind: "prompt" }> }) {
  const [value, setValue] = createSignal(props.modal.value);
  const done = (v: string | null) => {
    setModal(null);
    props.modal.resolve(v);
  };
  return (
    <Dialog
      title={props.modal.title}
      onClose={() => done(null)}
      footer={
        <>
          <button type="button" class="btn" onClick={() => done(null)}>
            Cancel
          </button>
          <button type="button" class="btn primary" disabled={!value().trim()} onClick={() => done(value().trim())}>
            OK
          </button>
        </>
      }
    >
      <label class="field">
        <span>{props.modal.label}</span>
        <input
          autofocus
          value={value()}
          ref={(el) => requestAnimationFrame(() => el.select())}
          onInput={(e) => setValue(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && value().trim()) {
              e.preventDefault();
              done(value().trim());
            }
          }}
        />
      </label>
    </Dialog>
  );
}

function SaveWorkspaceDialog() {
  const [name, setName] = createSignal(opened()?.workspace?.name ?? roots()[0]?.name ?? "My workspace");
  const save = async () => {
    if (await saveWorkspaceAs(name().trim())) setModal(null);
  };
  return (
    <Dialog
      title="Save workspace"
      onClose={() => setModal(null)}
      footer={
        <>
          <button type="button" class="btn" onClick={() => setModal(null)}>
            Cancel
          </button>
          <button type="button" class="btn primary" disabled={!name().trim()} onClick={() => void save()}>
            Choose location…
          </button>
        </>
      }
    >
      <p class="muted">
        A workspace remembers a set of folders ({roots().length} open now) and their export settings in a small
        <code>.markdraft-workspace</code> file you can commit next to your docs.
      </p>
      <label class="field">
        <span>Name</span>
        <input
          autofocus
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && name().trim()) {
              e.preventDefault();
              void save();
            }
          }}
        />
      </label>
    </Dialog>
  );
}

function ExportDialog(props: { path: string }) {
  const [theme, setTheme] = createSignal<"clean" | "academic" | "compact">("clean");
  const [paper, setPaper] = createSignal<"A4" | "Letter">(navigator.language === "en-US" ? "Letter" : "A4");
  const [toc, setToc] = createSignal(false);
  const [format, setFormat] = createSignal<"pdf" | "html">("pdf");
  const [busy, setBusy] = createSignal(false);
  const doc = () => docs[props.path];

  const run = async () => {
    const kind = format();
    const out = await ipc.pickSavePath(kind, `${stem(props.path)}.${kind}`);
    if (!out) return;
    setBusy(true);
    try {
      if (kind === "pdf") await exportPdf(props.path, out, { theme: theme(), paper: paper(), toc: toc() });
      else await exportHtml(props.path, out);
      setModal(null);
      toast(`Exported ${baseName(out)}`, "success", { label: "Show", run: () => void ipc.revealPath(out).catch(() => {}) });
    } catch (e) {
      toast(`Export failed: ${ipc.errorMessage(e)}`, "error");
    } finally {
      setBusy(false);
    }
  };

  const themes = [
    { id: "clean", name: "Clean", note: "Modern sans-serif, generous spacing" },
    { id: "academic", name: "Academic", note: "Serif, justified, numbered headings" },
    { id: "compact", name: "Compact", note: "Smaller type, tight margins" },
  ] as const;

  return (
    <Dialog
      title={`Export “${doc()?.name ?? baseName(props.path)}”`}
      onClose={() => !busy() && setModal(null)}
      footer={
        <>
          <button type="button" class="btn" disabled={busy()} onClick={() => setModal(null)}>
            Cancel
          </button>
          <button type="button" class="btn primary" disabled={busy()} onClick={() => void run()}>
            {busy() ? "Exporting…" : "Export…"}
          </button>
        </>
      }
    >
      <div class="segmented wide" role="group" aria-label="Format">
        <button type="button" aria-pressed={format() === "pdf"} onClick={() => setFormat("pdf")}>
          PDF
        </button>
        <button type="button" aria-pressed={format() === "html"} onClick={() => setFormat("html")}>
          HTML page
        </button>
      </div>
      <Show
        when={format() === "pdf"}
        fallback={<p class="muted">A single self-contained web page with the rendered document, diagrams and math.</p>}
      >
        <fieldset class="theme-choices">
          <legend>Theme</legend>
          <For each={themes}>
            {(t) => (
              <label class="theme-choice" classList={{ selected: theme() === t.id }}>
                <input type="radio" name="theme" checked={theme() === t.id} onChange={() => setTheme(t.id)} />
                <span class={`theme-sample sample-${t.id}`} aria-hidden="true">
                  <b>Aa</b>
                  <i />
                  <i />
                  <i />
                </span>
                <span class="theme-name">{t.name}</span>
                <span class="theme-note">{t.note}</span>
              </label>
            )}
          </For>
        </fieldset>
        <div class="field-row">
          <label class="field">
            <span>Paper</span>
            <select value={paper()} onChange={(e) => setPaper(e.currentTarget.value as "A4" | "Letter")}>
              <option value="A4">A4</option>
              <option value="Letter">US Letter</option>
            </select>
          </label>
          <label class="check-row">
            <input type="checkbox" checked={toc()} onChange={(e) => setToc(e.currentTarget.checked)} />
            Table of contents
          </label>
        </div>
      </Show>
      <Show when={doc() && doc()!.text !== doc()!.savedText}>
        <p class="muted">Unsaved edits are included in the export.</p>
      </Show>
    </Dialog>
  );
}

function SettingsDialog() {
  const select = <K extends keyof Settings>(key: K, label: string, options: [Settings[K], string][]) => (
    <label class="field inline">
      <span>{label}</span>
      <select
        value={String(settings[key])}
        onChange={(e) => {
          const raw = e.currentTarget.value;
          const opt = options.find(([v]) => String(v) === raw);
          if (opt) updateSettings({ [key]: opt[0] } as Partial<Settings>);
        }}
      >
        <For each={options}>{([v, text]) => <option value={String(v)}>{text}</option>}</For>
      </select>
    </label>
  );
  const check = (key: keyof Settings, label: string, hint?: string) => (
    <label class="check-row">
      <input
        type="checkbox"
        checked={settings[key] as boolean}
        onChange={(e) => updateSettings({ [key]: e.currentTarget.checked } as Partial<Settings>)}
      />
      <span>
        {label}
        <Show when={hint}>
          <small class="muted"> — {hint}</small>
        </Show>
      </span>
    </label>
  );
  return (
    <Dialog
      title="Settings"
      wide
      onClose={() => setModal(null)}
      footer={
        <button type="button" class="btn primary" onClick={() => setModal(null)}>
          Done
        </button>
      }
    >
      <div class="settings-grid">
        <section>
          <h3>Appearance</h3>
          {select("theme", "Theme", [
            ["system", "Match system"],
            ["light", "Light"],
            ["dark", "Dark"],
          ])}
          {select("previewWidth", "Reading width", [
            ["narrow", "Narrow"],
            ["medium", "Medium"],
            ["wide", "Wide"],
          ])}
          {select("previewFont", "Reading font", [
            ["sans", "Sans-serif"],
            ["serif", "Serif"],
          ])}
          {select("defaultView", "Default view", [
            ["read", "Read"],
            ["edit", "Edit"],
            ["split", "Split"],
          ])}
        </section>
        <section>
          <h3>Editor</h3>
          {select("editorFont", "Font", [
            ["mono", "Monospace"],
            ["sans", "Sans-serif"],
            ["serif", "Serif"],
          ])}
          {select(
            "editorFontSize",
            "Font size",
            [12, 13, 14, 15, 16, 17, 18, 20, 22].map((n) => [n, `${n} px`] as [number, string]),
          )}
          {check("lineWrap", "Wrap long lines")}
          {check("lineNumbers", "Show line numbers")}
          {check("spellcheck", "Check spelling")}
        </section>
        <section>
          <h3>Files</h3>
          {check("autosave", "Save automatically", "after you stop typing")}
          {select("autosaveDelayMs", "Autosave delay", [
            [500, "0.5 s"],
            [1000, "1 s"],
            [3000, "3 s"],
            [10000, "10 s"],
          ])}
          {check("showTitles", "Show document titles in the tree", "instead of file names")}
          {select("sort", "Sort documents by", [
            ["name", "Name"],
            ["title", "Title"],
            ["modified", "Last modified"],
          ])}
          {check("hideEmptyDirs", "Hide folders without documents")}
        </section>
        <section>
          <h3>Startup</h3>
          {check("restoreSession", "Reopen tabs and layout", "per folder or workspace")}
        </section>
      </div>
    </Dialog>
  );
}

function AboutDialog() {
  const [info] = createResource(() => (ipc.inTauri() ? ipc.appInfo() : Promise.resolve(null)));
  return (
    <Dialog
      title="About MarkDraft"
      onClose={() => setModal(null)}
      footer={
        <button type="button" class="btn primary" onClick={() => setModal(null)}>
          Close
        </button>
      }
    >
      <p>A fast, local-first Markdown reader and writer.</p>
      <Show when={info()}>
        {(i) => (
          <p class="muted">
            Version {i().version} · {i().os}/{i().arch}
          </p>
        )}
      </Show>
      <p class="muted">Built with Rust, Tauri, SolidJS, CodeMirror, comrak, Typst, KaTeX and Mermaid.</p>
    </Dialog>
  );
}

export function Toasts() {
  return (
    <div class="toasts" aria-live="polite">
      <For each={toasts()}>
        {(t) => (
          <div class={`toast toast-${t.kind}`} role={t.kind === "error" ? "alert" : "status"}>
            <span>{t.message}</span>
            <Show when={t.action}>
              <button
                type="button"
                class="btn ghost small"
                onClick={() => {
                  t.action!.run();
                  dismissToast(t.id);
                }}
              >
                {t.action!.label}
              </button>
            </Show>
            <button type="button" class="toast-close" aria-label="Dismiss" onClick={() => dismissToast(t.id)}>
              ×
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
