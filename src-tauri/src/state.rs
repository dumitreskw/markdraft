//! Shared app state: open roots, their file indexes and watchers.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use fs_index::{Change, DiscoveryRules, RootId, RootWatcher, SelfWrites, TreeNode};
use workspace::AppStore;

use crate::error::CmdError;
use crate::events::OpenRequest;

/// Relative document path → title, kept current from scans and the watcher.
pub type FileIndex = Arc<RwLock<BTreeMap<String, Option<String>>>>;

pub struct RootEntry {
    pub id: RootId,
    pub path: PathBuf,
    pub name: String,
    pub rules: DiscoveryRules,
    pub max_depth: Option<usize>,
    pub index: FileIndex,
    pub watcher: Option<RootWatcher>,
}

/// An open workspace file and the roots it opened.
#[derive(Debug, Clone)]
pub struct OpenWorkspace {
    pub file: PathBuf,
    pub def: workspace::WorkspaceDef,
    pub root_ids: Vec<RootId>,
}

#[derive(Default)]
pub struct Roots {
    pub list: Vec<RootEntry>,
    pub workspaces: Vec<OpenWorkspace>,
    next_id: RootId,
}

impl Roots {
    pub fn next_id(&mut self) -> RootId {
        self.next_id += 1;
        self.next_id
    }

    pub fn get(&self, id: RootId) -> Option<&RootEntry> {
        self.list.iter().find(|r| r.id == id)
    }

    pub fn get_mut(&mut self, id: RootId) -> Option<&mut RootEntry> {
        self.list.iter_mut().find(|r| r.id == id)
    }
}

pub struct AppState {
    /// Path given at launch, held until the UI is ready to receive it.
    pending_open: Mutex<Vec<OpenRequest>>,
    ui_ready: AtomicBool,
    pub roots: RwLock<Roots>,
    pub self_writes: SelfWrites,
    pub store: Option<AppStore>,
    /// Paths the user picked in a save dialog; only these may be written
    /// outside the open roots (exports, new workspace files).
    approved_saves: Mutex<HashSet<PathBuf>>,
}

impl AppState {
    pub fn new(pending_open: Option<OpenRequest>) -> Self {
        let store = match crate::portable_data_dir()
            .map_or_else(AppStore::open_default, |d| Ok(AppStore::at(d)))
        {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!("settings will not persist: {e}");
                None
            }
        };
        Self {
            pending_open: Mutex::new(pending_open.into_iter().collect()),
            ui_ready: AtomicBool::new(false),
            roots: RwLock::new(Roots::default()),
            self_writes: SelfWrites::default(),
            store,
            approved_saves: Mutex::new(HashSet::new()),
        }
    }

    pub fn take_pending_open(&self) -> Vec<OpenRequest> {
        self.ui_ready.store(true, Ordering::SeqCst);
        std::mem::take(&mut *lock(&self.pending_open))
    }

    /// Keeps open requests (several files opened together each send one)
    /// until the UI has started and drained them; once
    /// the UI is running it receives requests as events instead.
    pub fn queue_open(&self, req: OpenRequest) {
        if !self.ui_ready.load(Ordering::SeqCst) {
            lock(&self.pending_open).push(req);
        }
    }

    pub fn roots_read(&self) -> std::sync::RwLockReadGuard<'_, Roots> {
        self.roots.read().unwrap_or_else(|p| p.into_inner())
    }

    pub fn roots_write(&self) -> std::sync::RwLockWriteGuard<'_, Roots> {
        self.roots.write().unwrap_or_else(|p| p.into_inner())
    }

    pub fn approve_save(&self, path: &Path) {
        lock(&self.approved_saves).insert(path.to_owned());
    }

    pub fn is_approved_save(&self, path: &Path) -> bool {
        lock(&self.approved_saves).contains(path)
    }

    /// Resolves `path` and checks it lies inside an open root (or is the
    /// open workspace file). Paths that do not exist yet are checked
    /// through their parent folder.
    pub fn allowed_path(&self, path: &Path) -> Result<PathBuf, CmdError> {
        let resolved = resolve(path)?;
        let roots = self.roots_read();
        if roots.list.iter().any(|r| resolved.starts_with(&r.path))
            || roots.workspaces.iter().any(|w| w.file == resolved)
        {
            Ok(resolved)
        } else {
            Err(CmdError::NotAllowed(format!(
                "{} is outside the open folders",
                path.display()
            )))
        }
    }
}

/// Canonical form of a path, without Windows `\\?\` prefixes. For a path
/// that does not exist, its parent is canonicalised instead.
pub fn resolve(path: &Path) -> Result<PathBuf, CmdError> {
    if let Ok(p) = dunce::canonicalize(path) {
        return Ok(p);
    }
    let parent = path
        .parent()
        .ok_or_else(|| CmdError::NotFound(path.display().to_string()))?;
    let name = path
        .file_name()
        .ok_or_else(|| CmdError::Invalid(format!("{} has no file name", path.display())))?;
    let parent = dunce::canonicalize(parent)
        .map_err(|_| CmdError::NotFound(parent.display().to_string()))?;
    Ok(parent.join(name))
}

pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Adds scanned nodes to an index.
pub fn index_nodes(index: &FileIndex, nodes: &[TreeNode]) {
    let mut map = index.write().unwrap_or_else(|p| p.into_inner());
    for n in nodes.iter().filter(|n| !n.is_dir) {
        map.insert(n.path.clone(), n.title.clone());
    }
}

/// Applies watcher changes to an index.
pub fn index_changes(index: &FileIndex, changes: &[Change]) {
    let mut map = index.write().unwrap_or_else(|p| p.into_inner());
    for change in changes {
        match change {
            Change::Added { node } | Change::Modified { node } if !node.is_dir => {
                map.insert(node.path.clone(), node.title.clone());
            }
            Change::Removed { path } => remove_tree(&mut map, path),
            Change::Renamed { from, to } => {
                let prefix = format!("{from}/");
                let moved: Vec<_> = map
                    .range(prefix.clone()..)
                    .take_while(|(k, _)| k.starts_with(&prefix))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                remove_tree(&mut map, from);
                for (k, v) in moved {
                    map.insert(format!("{}/{}", to.path, &k[prefix.len()..]), v);
                }
                if !to.is_dir {
                    map.insert(to.path.clone(), to.title.clone());
                }
            }
            _ => {}
        }
    }
}

fn remove_tree(map: &mut BTreeMap<String, Option<String>>, path: &str) {
    map.remove(path);
    let prefix = format!("{path}/");
    let children: Vec<_> = map
        .range(prefix.clone()..)
        .take_while(|(k, _)| k.starts_with(&prefix))
        .map(|(k, _)| k.clone())
        .collect();
    for k in children {
        map.remove(&k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(path: &str, is_dir: bool) -> TreeNode {
        TreeNode {
            path: path.into(),
            is_dir,
            title: None,
            modified_ms: 0,
        }
    }

    #[test]
    fn index_tracks_renamed_folders() {
        let index: FileIndex = Arc::default();
        index_nodes(
            &index,
            &[
                node("a/x.md", false),
                node("a/y.md", false),
                node("ab.md", false),
                node("a", true),
            ],
        );
        index_changes(
            &index,
            &[Change::Renamed {
                from: "a".into(),
                to: node("b", true),
            }],
        );
        let keys: Vec<_> = index.read().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["ab.md", "b/x.md", "b/y.md"]);
        index_changes(&index, &[Change::Removed { path: "b".into() }]);
        let keys: Vec<_> = index.read().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["ab.md"]);
    }
}
