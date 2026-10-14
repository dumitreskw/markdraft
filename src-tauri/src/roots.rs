//! Opening folders, files and workspaces. Several sets of roots can be open
//! at once (one per workspace tab in the UI); each root runs its own scan
//! and watcher on background threads. Root ids are never reused, so events
//! from a root that has been closed are simply dropped.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use fs_index::{Change, DiscoveryRules, RootId, RootWatcher, ScanRequest, scan};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use workspace::{RecentEntry, RecentKind, WORKSPACE_EXTENSION, WorkspaceDef, session_key};

use crate::error::CmdError;
use crate::events::{FS_CHANGED, FsChanged, TREE_BATCH, TreeBatch};
use crate::state::{AppState, FileIndex, OpenWorkspace, RootEntry, index_changes, index_nodes};

/// Nodes per `tree://batch` event.
const BATCH_SIZE: usize = 400;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootView {
    pub id: RootId,
    pub path: PathBuf,
    pub name: String,
    /// Opened for a single file: only the folder's direct children are listed.
    pub single_file: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceView {
    pub file: PathBuf,
    pub name: String,
    pub root_ids: Vec<RootId>,
}

/// What one open call added: the roots for one workspace tab.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenResult {
    pub roots: Vec<RootView>,
    /// File to show first (single-file mode).
    pub focus: Option<PathBuf>,
    pub workspace: Option<WorkspaceView>,
    /// Key for the UI's saved session (tabs, layout) for this set of roots.
    pub session_key: String,
}

/// Everything open, for a UI that reloads and has to rebuild its tabs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllRoots {
    pub roots: Vec<RootView>,
    pub workspaces: Vec<WorkspaceView>,
}

struct RootSpec {
    path: PathBuf,
    name: Option<String>,
    rules: DiscoveryRules,
    max_depth: Option<usize>,
}

/// Opens a folder, a Markdown file (with its siblings) or a workspace file
/// as a new set of roots, next to whatever is already open.
pub fn open(app: &AppHandle, path: &Path) -> Result<OpenResult, CmdError> {
    let path = dunce::canonicalize(path)
        .map_err(|_| CmdError::NotFound(format!("{} does not exist", path.display())))?;
    let state = app.state::<AppState>();

    let (specs, def, focus, kind) = if path.is_dir() {
        let spec = RootSpec {
            path: path.clone(),
            name: None,
            rules: DiscoveryRules::default(),
            max_depth: None,
        };
        (vec![spec], None, None, RecentKind::Folder)
    } else if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(WORKSPACE_EXTENSION))
    {
        let def = WorkspaceDef::load(&path)?;
        let specs = workspace_specs(&def, &path)?;
        (specs, Some(def), None, RecentKind::Workspace)
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| CmdError::Invalid(format!("{} has no folder", path.display())))?;
        let spec = RootSpec {
            path: parent.to_path_buf(),
            name: None,
            rules: DiscoveryRules::default(),
            max_depth: Some(1),
        };
        (vec![spec], None, Some(path.clone()), RecentKind::File)
    };

    let ids = add_specs(app, &state, specs);
    if let Some(def) = def {
        let mut roots = state.roots_write();
        roots.workspaces.retain(|w| w.file != path);
        roots.workspaces.push(OpenWorkspace {
            file: path.clone(),
            def,
            root_ids: ids.clone(),
        });
    }
    let mut result = result_for(&state, &ids);
    result.focus = focus;
    remember(&state, &path, kind, &result);
    Ok(result)
}

fn workspace_specs(def: &WorkspaceDef, file: &Path) -> Result<Vec<RootSpec>, CmdError> {
    let rules = DiscoveryRules::new(
        &def.discovery.include,
        &def.discovery.exclude,
        def.discovery.respect_gitignore,
    )
    .map_err(|e| CmdError::Invalid(e.to_string()))?;
    let mut specs = Vec::new();
    for (root, path) in def.roots.iter().zip(def.resolved_roots(file)) {
        match dunce::canonicalize(&path) {
            Ok(path) if path.is_dir() => specs.push(RootSpec {
                path,
                name: root.name.clone(),
                rules: rules.clone(),
                max_depth: None,
            }),
            _ => tracing::warn!("workspace root {} is missing; skipping it", path.display()),
        }
    }
    Ok(specs)
}

/// Registers roots and starts scanning them; returns their ids.
fn add_specs(app: &AppHandle, state: &AppState, specs: Vec<RootSpec>) -> Vec<RootId> {
    let mut ids = Vec::new();
    let mut jobs = Vec::new();
    {
        let mut roots = state.roots_write();
        for spec in specs {
            let id = roots.next_id();
            let entry = new_entry(id, spec);
            jobs.push(scan_job(&entry));
            roots.list.push(entry);
            ids.push(id);
        }
    }
    for job in jobs {
        spawn_scan(app.clone(), job);
    }
    ids
}

fn new_entry(id: RootId, spec: RootSpec) -> RootEntry {
    let name = spec.name.unwrap_or_else(|| folder_name(&spec.path));
    RootEntry {
        id,
        path: spec.path,
        name,
        rules: spec.rules,
        max_depth: spec.max_depth,
        index: FileIndex::default(),
        watcher: None,
    }
}

/// Adds a folder next to `siblings` (the roots of the same workspace tab).
/// A single-file sibling becomes a full folder, since the tab is now a
/// workspace. Returns just the new root.
pub fn add_root(app: &AppHandle, path: &Path, siblings: &[RootId]) -> Result<OpenResult, CmdError> {
    let path = dunce::canonicalize(path)
        .map_err(|_| CmdError::NotFound(format!("{} does not exist", path.display())))?;
    if !path.is_dir() {
        return Err(CmdError::Invalid(format!(
            "{} is not a folder",
            path.display()
        )));
    }
    let state = app.state::<AppState>();
    let (rules, widened) = {
        let mut roots = state.roots_write();
        if let Some(existing) = roots
            .list
            .iter()
            .find(|r| r.path == path && siblings.contains(&r.id))
        {
            let id = existing.id;
            drop(roots);
            return Ok(result_for(&state, &[id]));
        }
        let rules = siblings
            .first()
            .and_then(|id| roots.get(*id))
            .map(|r| r.rules.clone())
            .unwrap_or_default();
        let mut widened = Vec::new();
        for id in siblings {
            if let Some(r) = roots.get_mut(*id)
                && r.max_depth == Some(1)
            {
                r.max_depth = None;
                r.watcher = None;
                widened.push(scan_job(r));
            }
        }
        (rules, widened)
    };
    for job in widened {
        spawn_scan(app.clone(), job);
    }
    let ids = add_specs(
        app,
        &state,
        vec![RootSpec {
            path,
            name: None,
            rules,
            max_depth: None,
        }],
    );
    Ok(result_for(&state, &ids))
}

/// Closes roots (their watchers stop) and forgets workspaces left empty.
pub fn remove_roots(app: &AppHandle, ids: &[RootId]) {
    let state = app.state::<AppState>();
    let mut roots = state.roots_write();
    roots.list.retain(|r| !ids.contains(&r.id));
    for w in &mut roots.workspaces {
        w.root_ids.retain(|id| !ids.contains(id));
    }
    roots.workspaces.retain(|w| !w.root_ids.is_empty());
}

pub fn rescan_root(app: &AppHandle, id: RootId) -> Result<(), CmdError> {
    let state = app.state::<AppState>();
    let job = {
        let mut roots = state.roots_write();
        let entry = roots
            .get_mut(id)
            .ok_or_else(|| CmdError::NotFound(format!("root {id} is not open")))?;
        entry.watcher = None;
        scan_job(entry)
    };
    spawn_scan(app.clone(), job);
    Ok(())
}

/// Saves the given roots (one workspace tab) as a workspace file.
pub fn save_workspace(
    app: &AppHandle,
    file: &Path,
    name: &str,
    root_ids: &[RootId],
) -> Result<OpenResult, CmdError> {
    let state = app.state::<AppState>();
    let base = file.parent().unwrap_or(Path::new("."));
    let def = {
        let roots = state.roots_read();
        let members: Vec<&RootEntry> = root_ids.iter().filter_map(|id| roots.get(*id)).collect();
        if members.is_empty() {
            return Err(CmdError::Invalid("there are no folders to save".into()));
        }
        // Keep discovery and export settings of the workspace being re-saved.
        let mut def = roots
            .workspaces
            .iter()
            .find(|w| w.root_ids.iter().any(|id| root_ids.contains(id)))
            .map(|w| w.def.clone())
            .unwrap_or_else(|| WorkspaceDef::new(name));
        def.name = name.to_owned();
        def.roots = members
            .iter()
            .map(|r| workspace::Root {
                // Relative paths keep a committed workspace portable.
                path: relative_to(&r.path, base),
                name: (r.name != folder_name(&r.path)).then(|| r.name.clone()),
            })
            .collect();
        def
    };
    def.save(file)?;
    let widened = {
        let mut roots = state.roots_write();
        roots
            .workspaces
            .retain(|w| w.file != file && !w.root_ids.iter().any(|id| root_ids.contains(id)));
        roots.workspaces.push(OpenWorkspace {
            file: file.to_owned(),
            def,
            root_ids: root_ids.to_vec(),
        });
        let mut widened = Vec::new();
        for id in root_ids {
            if let Some(r) = roots.get_mut(*id)
                && r.max_depth == Some(1)
            {
                r.max_depth = None;
                r.watcher = None;
                widened.push(scan_job(r));
            }
        }
        widened
    };
    for job in widened {
        spawn_scan(app.clone(), job);
    }
    let result = result_for(&state, root_ids);
    remember(&state, file, RecentKind::Workspace, &result);
    Ok(result)
}

fn root_view(r: &RootEntry) -> RootView {
    RootView {
        id: r.id,
        path: r.path.clone(),
        name: r.name.clone(),
        single_file: r.max_depth == Some(1),
    }
}

/// Describes a set of roots (one tab) for the UI.
pub fn result_for(state: &AppState, ids: &[RootId]) -> OpenResult {
    let roots = state.roots_read();
    let list: Vec<RootView> = ids
        .iter()
        .filter_map(|id| roots.get(*id))
        .map(root_view)
        .collect();
    let paths: Vec<_> = list.iter().map(|r| r.path.clone()).collect();
    let workspace = roots
        .workspaces
        .iter()
        .find(|w| !ids.is_empty() && ids.iter().all(|id| w.root_ids.contains(id)))
        .map(|w| WorkspaceView {
            file: w.file.clone(),
            name: w.def.name.clone(),
            root_ids: w.root_ids.clone(),
        });
    OpenResult {
        roots: list,
        focus: None,
        workspace,
        session_key: session_key(&paths),
    }
}

/// Every open root and workspace.
pub fn all(state: &AppState) -> AllRoots {
    let roots = state.roots_read();
    AllRoots {
        roots: roots.list.iter().map(root_view).collect(),
        workspaces: roots
            .workspaces
            .iter()
            .map(|w| WorkspaceView {
                file: w.file.clone(),
                name: w.def.name.clone(),
                root_ids: w.root_ids.clone(),
            })
            .collect(),
    }
}

fn remember(state: &AppState, path: &Path, kind: RecentKind, result: &OpenResult) {
    let Some(store) = &state.store else { return };
    let name = match (&result.workspace, kind) {
        (Some(w), RecentKind::Workspace) => w.name.clone(),
        _ => folder_name(path),
    };
    let entry = RecentEntry {
        path: path.to_owned(),
        kind,
        name,
        opened_ms: now_ms(),
    };
    if let Err(e) = store.push_recent(entry) {
        tracing::warn!("could not update recent items: {e}");
    }
}

struct ScanJob {
    id: RootId,
    path: PathBuf,
    rules: DiscoveryRules,
    max_depth: Option<usize>,
    index: FileIndex,
}

fn scan_job(entry: &RootEntry) -> ScanJob {
    ScanJob {
        id: entry.id,
        path: entry.path.clone(),
        rules: entry.rules.clone(),
        max_depth: entry.max_depth,
        index: Arc::clone(&entry.index),
    }
}

fn is_open(app: &AppHandle, id: RootId) -> bool {
    app.state::<AppState>().roots_read().get(id).is_some()
}

fn spawn_scan(app: AppHandle, job: ScanJob) {
    std::thread::spawn(move || {
        job.index.write().unwrap_or_else(|p| p.into_inner()).clear();
        let req = ScanRequest {
            root: job.path.clone(),
            rules: job.rules.clone(),
            max_depth: job.max_depth,
        };
        let stats = scan(&req, BATCH_SIZE, |nodes| {
            if !is_open(&app, job.id) {
                return;
            }
            index_nodes(&job.index, &nodes);
            let batch = TreeBatch {
                root_id: job.id,
                nodes,
                done: false,
                elapsed_ms: None,
            };
            if let Err(e) = app.emit(TREE_BATCH, batch) {
                tracing::error!("failed to emit tree batch: {e}");
            }
        });
        if !is_open(&app, job.id) {
            return;
        }
        tracing::info!(
            root = %job.path.display(),
            files = stats.files,
            dirs = stats.dirs,
            ms = stats.elapsed.as_millis() as u64,
            "scan finished"
        );
        let _ = app.emit(
            TREE_BATCH,
            TreeBatch {
                root_id: job.id,
                nodes: Vec::new(),
                done: true,
                elapsed_ms: Some(u64::try_from(stats.elapsed.as_millis()).unwrap_or(u64::MAX)),
            },
        );
        start_watcher(&app, job);
    });
}

fn start_watcher(app: &AppHandle, job: ScanJob) {
    let state = app.state::<AppState>();
    let emitter = app.clone();
    let index = Arc::clone(&job.index);
    let shallow = job.max_depth == Some(1);
    let id = job.id;
    let watcher = RootWatcher::start(
        job.path.clone(),
        job.rules.clone(),
        state.self_writes.clone(),
        move |mut changes: Vec<Change>| {
            if !is_open(&emitter, id) {
                return;
            }
            if shallow {
                changes.retain(|c| !change_path(c).is_some_and(|p| p.contains('/')));
            }
            if changes.is_empty() {
                return;
            }
            index_changes(&index, &changes);
            let _ = emitter.emit(
                FS_CHANGED,
                FsChanged {
                    root_id: id,
                    changes,
                },
            );
        },
    );
    match watcher {
        Ok(w) => {
            let mut roots = state.roots_write();
            // A rescan may have started a newer watcher meanwhile; keep one.
            if let Some(entry) = roots.get_mut(id)
                && entry.watcher.is_none()
            {
                entry.watcher = Some(w);
            }
        }
        Err(e) => tracing::warn!("live updates unavailable for {}: {e}", job.path.display()),
    }
}

fn change_path(change: &Change) -> Option<&str> {
    match change {
        Change::Added { node } | Change::Modified { node } => Some(&node.path),
        Change::Removed { path } => Some(path),
        Change::Renamed { to, .. } => Some(&to.path),
        Change::Rescan => None,
    }
}

pub fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// `path` relative to `base` when it is inside it, otherwise absolute.
fn relative_to(path: &Path, base: &Path) -> PathBuf {
    let base = dunce::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    match path.strip_prefix(&base) {
        Ok(rel) if rel.as_os_str().is_empty() => PathBuf::from("."),
        Ok(rel) => rel.to_path_buf(),
        Err(_) => path.to_path_buf(),
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_for_workspace_files() {
        let dir = tempfile::tempdir().unwrap();
        let base = dunce::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(base.join("docs")).unwrap();
        assert_eq!(
            relative_to(&base.join("docs"), &base),
            PathBuf::from("docs")
        );
        assert_eq!(relative_to(&base, &base), PathBuf::from("."));
        let elsewhere = std::env::temp_dir();
        assert!(relative_to(&elsewhere, &base.join("docs")).is_absolute());
    }

    #[test]
    fn shallow_filter_uses_change_paths() {
        let c = Change::Removed {
            path: "a/b.md".into(),
        };
        assert_eq!(change_path(&c), Some("a/b.md"));
        assert_eq!(change_path(&Change::Rescan), None);
    }
}
