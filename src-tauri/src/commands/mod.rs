//! Commands callable from the UI. Each one is also listed in `build.rs` and
//! granted in `capabilities/default.json`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::error::CmdError;
use crate::events::OpenRequest;
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
}

#[tauri::command]
pub async fn app_info() -> Result<AppInfo, CmdError> {
    Ok(AppInfo {
        name: "MarkDraft",
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
    })
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PickKind {
    Folder,
    File,
    Workspace,
}

/// Shows a native open dialog. `None` when the user cancels.
#[tauri::command]
pub async fn pick_path(app: AppHandle, kind: PickKind) -> Result<Option<PathBuf>, CmdError> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let dialog = app.dialog().file();
        match kind {
            PickKind::Folder => dialog.blocking_pick_folder(),
            PickKind::File => dialog
                .add_filter("Markdown", &["md", "markdown"])
                .blocking_pick_file(),
            PickKind::Workspace => dialog
                .add_filter("MarkDraft workspace", &[workspace::WORKSPACE_EXTENSION])
                .blocking_pick_file(),
        }
    })
    .await
    .map_err(|e| CmdError::Internal(e.to_string()))?;

    picked
        .map(|p| p.into_path().map_err(|e| CmdError::Internal(e.to_string())))
        .transpose()
}

/// Returns the path given at launch, once. The UI calls this on start-up so
/// the request is not lost before its event listener is attached.
#[tauri::command]
pub async fn take_open_request(
    state: State<'_, AppState>,
) -> Result<Option<OpenRequest>, CmdError> {
    Ok(state.take_pending_open())
}
