//! Reading, saving, rendering and managing documents.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fs_index::{DocFormat, LoadedDoc, SaveOutcome};
use md_core::{RenderContext, RenderedDoc};
use tauri::State;

use crate::error::{CmdError, blocking};
use crate::protocol::asset_url;
use crate::state::{AppState, resolve};

#[tauri::command]
pub async fn read_file(state: State<'_, AppState>, path: PathBuf) -> Result<LoadedDoc, CmdError> {
    let path = state.allowed_path(&path)?;
    blocking(move || Ok(fs_index::read_document(&path)?)).await
}

#[tauri::command]
pub async fn save_file(
    state: State<'_, AppState>,
    path: PathBuf,
    text: String,
    format: DocFormat,
    expected_modified_ms: Option<i64>,
) -> Result<SaveOutcome, CmdError> {
    let path = state.allowed_path(&path)?;
    let writes = state.self_writes.clone();
    blocking(move || {
        let outcome = fs_index::save_document(&path, &text, &format, expected_modified_ms)?;
        if let SaveOutcome::Saved { modified_ms } = outcome {
            writes.record(&path, modified_ms);
        }
        Ok(outcome)
    })
    .await
}

/// Renders Markdown for the preview. `path` (the document's location) is
/// only used to resolve relative images.
#[tauri::command]
pub async fn render_markdown(text: String, path: Option<PathBuf>) -> Result<RenderedDoc, CmdError> {
    blocking(move || {
        let ctx = RenderContext {
            base_dir: path
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf),
            asset_url: Some(Arc::new(asset_url)),
        };
        Ok(md_core::render(&text, &ctx))
    })
    .await
}

/// Creates a document (`.md` is added when the name has no extension) or a
/// folder, and returns its path. New documents start with a title heading.
#[tauri::command]
pub async fn create_entry(
    state: State<'_, AppState>,
    parent: PathBuf,
    name: String,
    is_dir: bool,
) -> Result<PathBuf, CmdError> {
    let parent = state.allowed_path(&parent)?;
    let name = valid_name(&name)?.to_owned();
    blocking(move || {
        if is_dir {
            let path = parent.join(name);
            std::fs::create_dir(&path)?;
            return Ok(path);
        }
        let mut path = parent.join(name);
        if path.extension().is_none() {
            path.set_extension("md");
        }
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut f = std::fs::File::create_new(&path)?;
        std::io::Write::write_all(&mut f, format!("# {title}\n\n").as_bytes())?;
        Ok(path)
    })
    .await
}

/// Renames or moves a file or folder within the open folders.
#[tauri::command]
pub async fn rename_entry(
    state: State<'_, AppState>,
    from: PathBuf,
    to: PathBuf,
) -> Result<PathBuf, CmdError> {
    let from = state.allowed_path(&from)?;
    let to = state.allowed_path(&to)?;
    if let Some(name) = to.file_name() {
        valid_name(&name.to_string_lossy())?;
    }
    if to.starts_with(&from) && to != from {
        return Err(CmdError::Invalid(
            "a folder cannot be moved into itself".into(),
        ));
    }
    blocking(move || {
        // Allow case-only renames on case-insensitive file systems.
        let same_file = resolve(&to).ok() == resolve(&from).ok();
        if to.exists() && !same_file {
            return Err(CmdError::Invalid(format!(
                "{} already exists",
                to.file_name().unwrap_or_default().to_string_lossy()
            )));
        }
        std::fs::rename(&from, &to)?;
        Ok(to)
    })
    .await
}

/// Moves a file or folder to the OS trash / recycle bin.
#[tauri::command]
pub async fn trash_entry(state: State<'_, AppState>, path: PathBuf) -> Result<(), CmdError> {
    let path = state.allowed_path(&path)?;
    let is_root = state.roots_read().list.iter().any(|r| r.path == path);
    if is_root {
        return Err(CmdError::NotAllowed(
            "an open folder cannot be deleted from here".into(),
        ));
    }
    blocking(move || trash::delete(&path).map_err(|e| CmdError::Io(e.to_string()))).await
}

/// Copies a document next to itself as "name copy.md" and returns the new path.
#[tauri::command]
pub async fn duplicate_entry(
    state: State<'_, AppState>,
    path: PathBuf,
) -> Result<PathBuf, CmdError> {
    let path = state.allowed_path(&path)?;
    blocking(move || {
        if !path.is_file() {
            return Err(CmdError::Invalid("only documents can be duplicated".into()));
        }
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let ext = path
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let dir = path.parent().unwrap_or(Path::new("."));
        let target = (1..1000)
            .map(|i| {
                let suffix = if i == 1 {
                    " copy".to_owned()
                } else {
                    format!(" copy {i}")
                };
                dir.join(format!("{stem}{suffix}{ext}"))
            })
            .find(|p| !p.exists())
            .ok_or_else(|| CmdError::Invalid("too many copies".into()))?;
        std::fs::copy(&path, &target)?;
        Ok(target)
    })
    .await
}

/// Checks a single file or folder name typed by the user.
fn valid_name(name: &str) -> Result<&str, CmdError> {
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let name = name.trim();
    let invalid = |why: &str| Err(CmdError::Invalid(format!("“{name}” {why}")));
    if name.is_empty() || name == "." || name == ".." {
        return Err(CmdError::Invalid("a name is required".into()));
    }
    if name.len() > 255 {
        return invalid("is too long");
    }
    if let Some(c) = name.chars().find(|c| {
        matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()
    }) {
        return invalid(&format!("cannot contain “{c}”"));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return invalid("cannot end with a dot or a space");
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        return invalid("is reserved by Windows");
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_validated_for_every_os() {
        assert_eq!(valid_name("  Notes 2026.md ").unwrap(), "Notes 2026.md");
        assert!(valid_name("a/b").is_err());
        assert!(valid_name("what?").is_err());
        assert!(valid_name("con.md").is_err());
        assert!(valid_name("trailing.").is_err());
        assert!(valid_name("..").is_err());
        assert!(valid_name("").is_err());
        assert!(valid_name("console.md").is_ok());
    }
}
