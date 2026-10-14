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
pub async fn render_markdown(
    text: String,
    path: Option<PathBuf>,
    smart: Option<bool>,
) -> Result<RenderedDoc, CmdError> {
    blocking(move || {
        let ctx = RenderContext {
            base_dir: path
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf),
            asset_url: Some(Arc::new(asset_url)),
            smart: smart.unwrap_or(false),
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

/// Writes an exported document (for example HTML) to a path the user picked
/// in the save dialog.
#[tauri::command]
pub async fn write_export(
    state: State<'_, AppState>,
    out: PathBuf,
    contents: String,
) -> Result<(), CmdError> {
    if !state.is_approved_save(&out) {
        return Err(CmdError::NotAllowed(
            "choose where to save the export first".into(),
        ));
    }
    blocking(move || {
        std::fs::write(&out, contents)?;
        Ok(())
    })
    .await
}

/// Largest image accepted by paste or drop.
const MAX_IMAGE_BYTES: usize = 25 * 1024 * 1024;

/// Saves pasted image bytes next to a document, in an `assets` folder, and
/// returns the relative link to insert. The raw request body carries the
/// bytes; the `x-doc` and `x-name` headers (percent-encoded) say where.
#[tauri::command]
pub async fn save_image(
    state: State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> Result<String, CmdError> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err(CmdError::Invalid("expected image bytes".into()));
    };
    let header = |name: &str| -> Result<String, CmdError> {
        let raw = request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| CmdError::Invalid(format!("missing {name}")))?;
        Ok(percent_encoding::percent_decode_str(raw)
            .decode_utf8_lossy()
            .into_owned())
    };
    let doc = state.allowed_path(Path::new(&header("x-doc")?))?;
    let name = header("x-name")?;
    let bytes = bytes.clone();
    blocking(move || store_image(&doc, &name, &bytes)).await
}

/// Copies an image file (dropped from the file manager) next to a document.
#[tauri::command]
pub async fn import_image(
    state: State<'_, AppState>,
    doc: PathBuf,
    source: PathBuf,
) -> Result<String, CmdError> {
    let doc = state.allowed_path(&doc)?;
    blocking(move || {
        if crate::protocol::mime_for(&source).is_none() {
            return Err(CmdError::Invalid("only images can be inserted".into()));
        }
        let bytes = std::fs::read(&source)?;
        let name = source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        store_image(&doc, &name, &bytes)
    })
    .await
}

/// Writes `bytes` as `<doc folder>/assets/<name>` (made unique) and returns
/// the Markdown-ready relative link.
fn store_image(doc: &Path, name: &str, bytes: &[u8]) -> Result<String, CmdError> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(CmdError::Invalid(
            "images larger than 25 MB are not inserted".into(),
        ));
    }
    let path = Path::new(name);
    if crate::protocol::mime_for(path).is_none() {
        return Err(CmdError::Invalid(format!(
            "{name} is not a supported image type"
        )));
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let stem = slug(&path.file_stem().unwrap_or_default().to_string_lossy());
    let dir = doc
        .parent()
        .ok_or_else(|| CmdError::Invalid("the document has no folder".into()))?
        .join("assets");
    std::fs::create_dir_all(&dir)?;
    let target = (1..10_000)
        .map(|i| {
            let file = if i == 1 {
                format!("{stem}.{ext}")
            } else {
                format!("{stem}-{i}.{ext}")
            };
            dir.join(file)
        })
        .find(|p| !p.exists())
        .ok_or_else(|| CmdError::Invalid("too many images with this name".into()))?;
    std::fs::write(&target, bytes)?;
    let file = target.file_name().unwrap_or_default().to_string_lossy();
    Ok(format!("assets/{}", file.replace(' ', "%20")))
}

/// A file-name-friendly version of `s`: lowercase letters, digits and dashes.
fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() { "image".into() } else { out }
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
    fn images_are_stored_uniquely_next_to_the_document() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("note.md");
        std::fs::write(&doc, "").unwrap();
        assert_eq!(
            store_image(&doc, "Screen Shot 1.PNG", b"x").unwrap(),
            "assets/screen-shot-1.png"
        );
        assert_eq!(
            store_image(&doc, "Screen Shot 1.PNG", b"y").unwrap(),
            "assets/screen-shot-1-2.png"
        );
        assert_eq!(
            std::fs::read(dir.path().join("assets/screen-shot-1-2.png")).unwrap(),
            b"y"
        );
        assert!(store_image(&doc, "evil.exe", b"x").is_err());
        assert_eq!(slug("  ---  "), "image");
    }

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
