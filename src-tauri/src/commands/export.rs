//! PDF export through the `export` crate (Markdown → Typst → PDF).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use export::{ExportInput, ExportOptions};
use tauri::State;

use crate::error::{CmdError, blocking};
use crate::state::AppState;

impl From<export::Error> for CmdError {
    fn from(e: export::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

fn check_out(state: &AppState, out: &Path) -> Result<(), CmdError> {
    if state.is_approved_save(out) {
        Ok(())
    } else {
        Err(CmdError::NotAllowed(
            "choose where to save the export first".into(),
        ))
    }
}

/// Exports one document. `text` is the editor's current contents (so
/// unsaved edits are included); `diagrams` holds Mermaid SVGs rendered by
/// the UI, keyed by the index of each mermaid fence.
#[tauri::command]
pub async fn export_pdf(
    state: State<'_, AppState>,
    path: PathBuf,
    text: String,
    out: PathBuf,
    options: ExportOptions,
    diagrams: HashMap<usize, String>,
) -> Result<(), CmdError> {
    check_out(&state, &out)?;
    let path = state.allowed_path(&path)?;
    blocking(move || {
        let input = ExportInput {
            markdown: &text,
            base_dir: path.parent(),
            title: None,
            diagrams: &diagrams,
        };
        let pdf = export::markdown_to_pdf(&input, &options)?;
        std::fs::write(&out, pdf)?;
        Ok(())
    })
    .await
}

/// Exports every document in a folder (recursively, in path order) as one
/// PDF with a cover page and a table of contents.
#[tauri::command]
pub async fn export_folder_pdf(
    state: State<'_, AppState>,
    folder: PathBuf,
    out: PathBuf,
    options: ExportOptions,
) -> Result<usize, CmdError> {
    check_out(&state, &out)?;
    let folder = state.allowed_path(&folder)?;
    let files: Vec<PathBuf> = {
        let roots = state.roots_read();
        let mut files: Vec<PathBuf> = roots
            .list
            .iter()
            .flat_map(|root| {
                let index = root.index.read().unwrap_or_else(|p| p.into_inner());
                index
                    .keys()
                    .map(|rel| root.path.join(rel))
                    .collect::<Vec<_>>()
            })
            .filter(|p| p.starts_with(&folder))
            .collect();
        files.sort_by_key(|p| p.to_string_lossy().to_lowercase());
        files
    };
    if files.is_empty() {
        return Err(CmdError::Invalid("this folder has no documents".into()));
    }
    let cover = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    blocking(move || {
        let texts: Vec<String> = files
            .iter()
            .map(|p| fs_index::read_document(p).map(|d| d.text))
            .collect::<Result<_, _>>()?;
        let empty = HashMap::new();
        let inputs: Vec<ExportInput> = files
            .iter()
            .zip(&texts)
            .map(|(path, text)| ExportInput {
                markdown: text,
                base_dir: path.parent(),
                title: None,
                diagrams: &empty,
            })
            .collect();
        let pdf = export::combine_to_pdf(&inputs, &options, Some(&cover))?;
        std::fs::write(&out, pdf)?;
        Ok(inputs.len())
    })
    .await
}
