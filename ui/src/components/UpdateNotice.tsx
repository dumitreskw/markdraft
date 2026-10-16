import { Match, Show, Switch } from "solid-js";
import * as ipc from "../ipc";
import { dismissUpdate, installUpdate, updateNotice } from "../state/update";

const mb = (bytes: number) => `${(bytes / 1_048_576).toFixed(1)} MB`;

/** "A new version is available" card, then download progress. */
export function UpdateNotice() {
  return (
    <Show when={updateNotice()}>
      {(notice) => (
        <div class="toast update-notice" role="status">
          <Switch>
            <Match when={notice().state === "available" && notice()}>
              {(n) => (
                <>
                  <span>
                    <strong>MarkDraft {n().info.version}</strong> is available
                    <small class="muted"> — you have {n().info.currentVersion}</small>
                  </span>
                  <div class="update-actions">
                    <button type="button" class="btn ghost small" onClick={() => void ipc.openExternal(n().info.pageUrl)}>
                      What’s new
                    </button>
                    <button type="button" class="btn primary small" onClick={() => void installUpdate()}>
                      {n().info.canInstall ? "Restart to update" : "Download"}
                    </button>
                  </div>
                  <button type="button" class="toast-close" aria-label="Not now" title="Not now" onClick={dismissUpdate}>
                    ×
                  </button>
                </>
              )}
            </Match>
            <Match when={notice().state === "installing" && notice()}>
              {(n) => {
                const p = () => {
                  const x = n();
                  return x.state === "installing" ? x.progress : null;
                };
                const pct = () => {
                  const x = p();
                  return x?.total ? Math.min(100, Math.round((x.downloaded / x.total) * 100)) : null;
                };
                return (
                  <div class="update-progress">
                    <span>
                      Updating to {n().info.version}…{" "}
                      <small class="muted">
                        {pct() === 100 ? "installing" : p() ? (pct() !== null ? `${pct()}%` : mb(p()!.downloaded)) : "starting"}
                      </small>
                    </span>
                    <progress max="100" value={pct() ?? undefined} aria-label="Download progress" />
                  </div>
                );
              }}
            </Match>
          </Switch>
        </div>
      )}
    </Show>
  );
}
