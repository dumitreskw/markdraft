use serde::Serialize;

/// Error returned by every command, serialised as `{ kind, message }` so the
/// UI can branch on `kind` and show `message`.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum CmdError {
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Internal(String),
}

impl From<std::io::Error> for CmdError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
