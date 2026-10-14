//! Events emitted from Rust to the UI. Names mirror `ui/src/ipc.ts`.

use std::path::PathBuf;

use fs_index::{Change, RootId, TreeNode};
use serde::Serialize;

/// A path to open: from the command line, a second launch, or the OS.
pub const OPEN_PATH: &str = "app://open-path";
/// Nodes discovered by a scan.
pub const TREE_BATCH: &str = "tree://batch";
/// Live changes from a root's watcher.
pub const FS_CHANGED: &str = "fs://changed";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenRequest {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeBatch {
    pub root_id: RootId,
    pub nodes: Vec<TreeNode>,
    /// True on the last batch of a scan.
    pub done: bool,
    pub elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FsChanged {
    pub root_id: RootId,
    pub changes: Vec<Change>,
}
