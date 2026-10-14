//! Markdown discovery, the tree model, file watching and document IO.
//!
//! Everything here is UI-free: the app turns scan batches and watcher
//! changes into events, and the UI builds its tree from them.

mod doc;
mod rules;
mod scan;
mod watch;

use serde::{Deserialize, Serialize};

pub use doc::{
    DocFormat, LineEnding, LoadedDoc, SaveOutcome, TextEncoding, read_document, save_document,
};
pub use rules::{
    ALWAYS_SKIPPED_DIRS, DEFAULT_INCLUDE, DiscoveryRules, MARKDRAFT_IGNORE_FILE, validate_pattern,
};
pub use scan::{ScanRequest, ScanStats, read_title, scan};
pub use watch::{RootWatcher, SelfWrites};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid glob pattern `{pattern}`: {source}")]
    Glob {
        pattern: String,
        #[source]
        source: globset::Error,
    },
    #[error("{path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{0} is larger than the {1} MB limit for documents")]
    TooLarge(std::path::PathBuf, u64),
    #[error("{0} looks like a binary file")]
    Binary(std::path::PathBuf),
    #[error("could not watch {path}: {message}")]
    Watch {
        path: std::path::PathBuf,
        message: String,
    },
}

impl Error {
    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            source,
        }
    }
}

/// Stable identifier of a workspace root, assigned when the root is opened.
pub type RootId = u32;

/// One row of the file tree, as sent to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    /// Path relative to the root, with `/` separators on every OS.
    pub path: String,
    pub is_dir: bool,
    /// Document title (front matter or first H1); `None` for directories.
    pub title: Option<String>,
    /// Last modification time, milliseconds since the Unix epoch.
    pub modified_ms: i64,
}

/// An incremental change to a root's tree, produced by the watcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Change {
    Added {
        node: TreeNode,
    },
    Removed {
        path: String,
    },
    Renamed {
        from: String,
        to: TreeNode,
    },
    Modified {
        node: TreeNode,
    },
    /// Events were lost (for example a watcher overflow); rescan the root.
    Rescan,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_serialises_with_kind_tag() {
        let change = Change::Removed {
            path: "a.md".into(),
        };
        let json = serde_json::to_string(&change).unwrap();
        assert_eq!(json, r#"{"kind":"removed","path":"a.md"}"#);
    }
}
