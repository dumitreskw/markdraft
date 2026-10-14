//! App-level commands: info, dialogs, settings, recents, sessions.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use workspace::RecentEntry;

use crate::error::{CmdError, blocking};
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
    let picked = blocking(move || {
        let dialog = app.dialog().file();
        Ok(match kind {
            PickKind::Folder => dialog.blocking_pick_folder(),
            PickKind::File => dialog
                .add_filter("Markdown", &["md", "markdown", "mdx", "txt"])
                .blocking_pick_file(),
            PickKind::Workspace => dialog
                .add_filter("MarkDraft workspace", &[workspace::WORKSPACE_EXTENSION])
                .blocking_pick_file(),
        })
    })
    .await?;
    picked
        .map(|p| p.into_path().map_err(|e| CmdError::Internal(e.to_string())))
        .transpose()
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SaveKind {
    Pdf,
    Html,
    Workspace,
}

/// Shows a native save dialog. The chosen path is approved for writing
/// once, even if it lies outside the open folders.
#[tauri::command]
pub async fn pick_save_path(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: SaveKind,
    suggested_name: String,
) -> Result<Option<PathBuf>, CmdError> {
    let dialog_app = app.clone();
    let picked = blocking(move || {
        let (label, ext) = match kind {
            SaveKind::Pdf => ("PDF document", "pdf"),
            SaveKind::Html => ("Web page", "html"),
            SaveKind::Workspace => ("MarkDraft workspace", workspace::WORKSPACE_EXTENSION),
        };
        Ok(dialog_app
            .dialog()
            .file()
            .add_filter(label, &[ext])
            .set_file_name(&suggested_name)
            .blocking_save_file())
    })
    .await?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|e| CmdError::Internal(e.to_string()))?;
    state.approve_save(&path);
    Ok(Some(path))
}

/// Returns the path given at launch, once. The UI calls this on start-up so
/// the request is not lost before its event listener is attached.
#[tauri::command]
pub async fn take_open_request(
    state: State<'_, AppState>,
) -> Result<Option<OpenRequest>, CmdError> {
    Ok(state.take_pending_open())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Value, CmdError> {
    Ok(state
        .store
        .as_ref()
        .map(|s| s.settings())
        .unwrap_or(Value::Object(Default::default())))
}

#[tauri::command]
pub async fn set_settings(state: State<'_, AppState>, settings: Value) -> Result<(), CmdError> {
    if let Some(store) = &state.store {
        store.save_settings(&settings)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn list_recents(state: State<'_, AppState>) -> Result<Vec<RecentEntry>, CmdError> {
    Ok(state
        .store
        .as_ref()
        .map(|s| s.recents())
        .unwrap_or_default())
}

#[tauri::command]
pub async fn remove_recent(
    state: State<'_, AppState>,
    path: PathBuf,
) -> Result<Vec<RecentEntry>, CmdError> {
    match &state.store {
        Some(store) => Ok(store.remove_recent(&path)?),
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
pub async fn get_session(
    state: State<'_, AppState>,
    key: String,
) -> Result<Option<Value>, CmdError> {
    check_key(&key)?;
    Ok(state.store.as_ref().and_then(|s| s.session(&key)))
}

#[tauri::command]
pub async fn set_session(
    state: State<'_, AppState>,
    key: String,
    session: Value,
) -> Result<(), CmdError> {
    check_key(&key)?;
    if let Some(store) = &state.store {
        store.save_session(&key, &session)?;
    }
    Ok(())
}

/// Session keys become file names; only the hex keys we hand out are valid.
fn check_key(key: &str) -> Result<(), CmdError> {
    if !key.is_empty() && key.len() <= 32 && key.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(CmdError::Invalid("invalid session key".into()))
    }
}

#[derive(Debug, Serialize)]
pub struct HighlightCss {
    pub light: String,
    pub dark: String,
}

#[tauri::command]
pub async fn highlight_css() -> Result<HighlightCss, CmdError> {
    blocking(|| {
        Ok(HighlightCss {
            light: md_core::highlight_css(md_core::HighlightTheme::Light),
            dark: md_core::highlight_css(md_core::HighlightTheme::Dark),
        })
    })
    .await
}

/// Opens a web or mail link in the system's default handler.
#[tauri::command]
pub async fn open_external(app: AppHandle, url: String) -> Result<(), CmdError> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("mailto:"))
    {
        return Err(CmdError::NotAllowed(format!("refusing to open {url}")));
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| CmdError::Io(e.to_string()))
}

/// Shows a file or folder in the system file manager.
#[tauri::command]
pub async fn reveal_path(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
) -> Result<(), CmdError> {
    let path = state.allowed_path(&path)?;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| CmdError::Io(e.to_string()))
}
