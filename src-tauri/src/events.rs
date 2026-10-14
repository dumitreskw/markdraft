//! Events emitted from Rust to the UI. Names mirror `ui/src/ipc.ts`.

use std::path::PathBuf;

use serde::Serialize;

/// A path to open: from the command line, a second launch, or the OS.
pub const OPEN_PATH: &str = "app://open-path";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenRequest {
    pub path: PathBuf,
}
