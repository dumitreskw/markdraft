//! Opening folders, files and workspaces.

use std::path::PathBuf;

use fs_index::RootId;
use tauri::{AppHandle, State};

use crate::error::{CmdError, blocking};
use crate::roots::{self, OpenResult};
use crate::state::AppState;

#[tauri::command]
pub async fn open_path(app: AppHandle, path: PathBuf) -> Result<OpenResult, CmdError> {
    blocking(move || roots::open(&app, &path)).await
}

#[tauri::command]
pub async fn add_root(app: AppHandle, path: PathBuf) -> Result<OpenResult, CmdError> {
    blocking(move || roots::add_root(&app, &path)).await
}

#[tauri::command]
pub async fn remove_root(app: AppHandle, root_id: RootId) -> Result<OpenResult, CmdError> {
    Ok(roots::remove_root(&app, root_id))
}

#[tauri::command]
pub async fn rescan_root(app: AppHandle, root_id: RootId) -> Result<(), CmdError> {
    roots::rescan_root(&app, root_id)
}

#[tauri::command]
pub async fn current_roots(state: State<'_, AppState>) -> Result<OpenResult, CmdError> {
    Ok(roots::view(&state))
}

/// Saves the open folders as a workspace file. `file` must come from the
/// save dialog or be the workspace file already open.
#[tauri::command]
pub async fn save_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    file: PathBuf,
    name: String,
) -> Result<OpenResult, CmdError> {
    let is_current = state
        .roots_read()
        .workspace
        .as_ref()
        .is_some_and(|w| w.file == file);
    if !is_current && !state.is_approved_save(&file) {
        return Err(CmdError::NotAllowed(
            "choose where to save the workspace first".into(),
        ));
    }
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(CmdError::Invalid("the workspace needs a name".into()));
    }
    blocking(move || roots::save_workspace(&app, &file, &name)).await
}
