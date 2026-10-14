use serde::Serialize;

/// Error returned by every command, serialised as `{ kind, message }` so the
/// UI can branch on `kind` and show `message`.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum CmdError {
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    NotAllowed(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Internal(String),
}

impl From<std::io::Error> for CmdError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound(e.to_string()),
            _ => Self::Io(e.to_string()),
        }
    }
}

impl From<fs_index::Error> for CmdError {
    fn from(e: fs_index::Error) -> Self {
        match e {
            fs_index::Error::Io { source, path }
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Self::NotFound(format!("{} was not found", path.display()))
            }
            fs_index::Error::Glob { .. } => Self::Invalid(e.to_string()),
            _ => Self::Io(e.to_string()),
        }
    }
}

impl From<workspace::Error> for CmdError {
    fn from(e: workspace::Error) -> Self {
        Self::Invalid(e.to_string())
    }
}

impl From<workspace::StoreError> for CmdError {
    fn from(e: workspace::StoreError) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<tauri::Error> for CmdError {
    fn from(e: tauri::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

/// Runs blocking work (disk IO, rendering) off the async runtime.
pub async fn blocking<T, F>(f: F) -> Result<T, CmdError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CmdError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| CmdError::Internal(e.to_string()))?
}
