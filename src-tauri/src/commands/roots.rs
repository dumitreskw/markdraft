//! Opening folders, files and workspaces (one set of roots per UI tab).

use std::path::PathBuf;

use fs_index::RootId;
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::error::{CmdError, blocking};
use crate::roots::{self, AllRoots, OpenResult};
use crate::state::AppState;

/// Opens a folder, file or workspace file as a new set of roots.
#[tauri::command]
pub async fn open_path(app: AppHandle, path: PathBuf) -> Result<OpenResult, CmdError> {
    blocking(move || roots::open(&app, &path)).await
}

/// Adds a folder to a workspace tab whose roots are `siblings`.
#[tauri::command]
pub async fn add_root(
    app: AppHandle,
    path: PathBuf,
    siblings: Vec<RootId>,
) -> Result<OpenResult, CmdError> {
    blocking(move || roots::add_root(&app, &path, &siblings)).await
}

/// Closes roots (one folder, or a whole workspace tab).
#[tauri::command]
pub async fn remove_roots(app: AppHandle, root_ids: Vec<RootId>) -> Result<(), CmdError> {
    roots::remove_roots(&app, &root_ids);
    Ok(())
}

#[tauri::command]
pub async fn rescan_root(app: AppHandle, root_id: RootId) -> Result<(), CmdError> {
    roots::rescan_root(&app, root_id)
}

/// Every open root and workspace (used after a UI reload).
#[tauri::command]
pub async fn current_roots(state: State<'_, AppState>) -> Result<AllRoots, CmdError> {
    Ok(roots::all(&state))
}

/// Saves a workspace tab's folders as a workspace file. `file` must come
/// from the save dialog or be a workspace file already open.
#[tauri::command]
pub async fn save_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    file: PathBuf,
    name: String,
    root_ids: Vec<RootId>,
) -> Result<OpenResult, CmdError> {
    let is_open = state.roots_read().workspaces.iter().any(|w| w.file == file);
    if !is_open && !state.is_approved_save(&file) {
        return Err(CmdError::NotAllowed(
            "choose where to save the workspace first".into(),
        ));
    }
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(CmdError::Invalid("the workspace needs a name".into()));
    }
    blocking(move || roots::save_workspace(&app, &file, &name, &root_ids)).await
}

/// Workspace tabs to reopen on the next start (shape owned by the UI).
#[tauri::command]
pub async fn get_window_state(state: State<'_, AppState>) -> Result<Option<Value>, CmdError> {
    Ok(state.store.as_ref().and_then(|s| s.window_state()))
}

#[tauri::command]
pub async fn set_window_state(state: State<'_, AppState>, window: Value) -> Result<(), CmdError> {
    if let Some(store) = &state.store {
        store.save_window_state(&window)?;
    }
    Ok(())
}
