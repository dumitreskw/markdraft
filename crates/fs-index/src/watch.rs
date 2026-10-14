//! Watches a root and turns debounced file-system events into tree diffs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

use crate::scan::{read_title, system_time_ms, to_slash};
use crate::{Change, DiscoveryRules, Error, MARKDRAFT_IGNORE_FILE, TreeNode};

const DEBOUNCE: Duration = Duration::from_millis(200);

/// How long a recorded self-write suppresses watcher events for its file.
const ECHO_WINDOW: Duration = Duration::from_secs(5);

/// Files MarkDraft itself just wrote, so the watcher does not report them
/// back as external changes. Cheap to clone; shared between roots.
#[derive(Debug, Clone, Default)]
pub struct SelfWrites {
    inner: Arc<Mutex<HashMap<PathBuf, (i64, Instant)>>>,
}

impl SelfWrites {
    /// Records that we wrote `path`, leaving it with modification time `modified_ms`.
    pub fn record(&self, path: &Path, modified_ms: i64) {
        let mut map = self.lock();
        map.retain(|_, (_, at)| at.elapsed() < ECHO_WINDOW);
        map.insert(path.to_owned(), (modified_ms, Instant::now()));
    }

    /// Whether the current state of `path` is the one we wrote ourselves.
    pub fn is_echo(&self, path: &Path) -> bool {
        let map = self.lock();
        let Some((written_ms, at)) = map.get(path) else {
            return false;
        };
        at.elapsed() < ECHO_WINDOW && modified_ms(path) == Some(*written_ms)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, (i64, Instant)>> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

fn modified_ms(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()
        .map(system_time_ms)
}

/// A running watcher for one root. Dropping it stops watching.
pub struct RootWatcher {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl RootWatcher {
    /// Starts watching `root` recursively. `on_changes` runs on the
    /// watcher's thread with the diffs from each debounced burst.
    pub fn start(
        root: PathBuf,
        rules: DiscoveryRules,
        self_writes: SelfWrites,
        on_changes: impl Fn(Vec<Change>) + Send + 'static,
    ) -> Result<Self, Error> {
        let mapper = EventMapper::new(root.clone(), rules, self_writes);
        let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
            let changes = match result {
                Ok(events) => mapper.map(events.iter().map(|e| &e.event)),
                Err(errors) => {
                    tracing::warn!(?errors, "file watcher errors; requesting a rescan");
                    vec![Change::Rescan]
                }
            };
            if !changes.is_empty() {
                on_changes(changes);
            }
        })
        .map_err(|e| watch_error(&root, &e))?;
        debouncer
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|e| watch_error(&root, &e))?;
        Ok(Self {
            _debouncer: debouncer,
        })
    }
}

fn watch_error(root: &Path, e: &notify::Error) -> Error {
    Error::Watch {
        path: root.to_owned(),
        message: e.to_string(),
    }
}

/// Pure translation from notify events to [`Change`]s; separated from the
/// watcher so it can be unit-tested with synthetic events.
struct EventMapper {
    root: PathBuf,
    rules: DiscoveryRules,
    ignores: Gitignore,
    self_writes: SelfWrites,
}

impl EventMapper {
    fn new(root: PathBuf, rules: DiscoveryRules, self_writes: SelfWrites) -> Self {
        // Only root-level ignore files are honoured for live events; a full
        // rescan applies nested ones.
        let mut builder = GitignoreBuilder::new(&root);
        if rules.respect_gitignore {
            builder.add(root.join(".gitignore"));
            builder.add(root.join(".ignore"));
        }
        builder.add(root.join(MARKDRAFT_IGNORE_FILE));
        let ignores = builder.build().unwrap_or_else(|_| Gitignore::empty());
        Self {
            root,
            rules,
            ignores,
            self_writes,
        }
    }

    fn map<'e>(&self, events: impl Iterator<Item = &'e notify::Event>) -> Vec<Change> {
        let mut changes: Vec<Change> = Vec::new();
        for event in events {
            if event.need_rescan() {
                return vec![Change::Rescan];
            }
            match event.kind {
                EventKind::Create(_) => {
                    if let Some(p) = event.paths.first() {
                        changes.extend(self.added(p));
                    }
                }
                EventKind::Remove(_) => {
                    if let Some(p) = event.paths.first() {
                        changes.extend(self.removed(p));
                    }
                }
                EventKind::Modify(ModifyKind::Name(mode)) => match (mode, event.paths.as_slice()) {
                    (RenameMode::Both, [from, to, ..]) => changes.extend(self.renamed(from, to)),
                    (RenameMode::From, [p, ..]) => changes.extend(self.removed(p)),
                    (RenameMode::To, [p, ..]) => changes.extend(self.added(p)),
                    // Platforms that cannot pair renames: decide by existence.
                    (_, [p, ..]) if p.exists() => changes.extend(self.added(p)),
                    (_, [p, ..]) => changes.extend(self.removed(p)),
                    _ => {}
                },
                EventKind::Modify(_) | EventKind::Any => {
                    if let Some(p) = event.paths.first()
                        && p.is_file()
                        && !self.self_writes.is_echo(p)
                        && let Some(node) = self.node(p)
                    {
                        changes.push(Change::Modified { node });
                    }
                }
                EventKind::Access(_) | EventKind::Other => {}
            }
        }
        dedupe(changes)
    }

    fn relative(&self, path: &Path) -> Option<String> {
        let rel = path.strip_prefix(&self.root).ok()?;
        if rel.as_os_str().is_empty() {
            return None;
        }
        for part in rel.components() {
            let name = part.as_os_str().to_string_lossy();
            if name.starts_with('.') || DiscoveryRules::is_skipped_dir(&name) {
                return None;
            }
        }
        Some(to_slash(rel))
    }

    fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        self.ignores
            .matched_path_or_any_parents(path, is_dir)
            .is_ignore()
    }

    /// The tree node for an existing path, if it belongs in the tree.
    fn node(&self, path: &Path) -> Option<TreeNode> {
        let rel = self.relative(path)?;
        let meta = std::fs::metadata(path).ok()?;
        let is_dir = meta.is_dir();
        if self.is_ignored(path, is_dir) {
            return None;
        }
        if !is_dir && !self.rules.is_document(Path::new(&rel)) {
            return None;
        }
        Some(TreeNode {
            path: rel,
            is_dir,
            title: if is_dir { None } else { read_title(path) },
            modified_ms: meta.modified().map(system_time_ms).unwrap_or(0),
        })
    }

    fn added(&self, path: &Path) -> Option<Change> {
        self.node(path).map(|node| Change::Added { node })
    }

    fn removed(&self, path: &Path) -> Option<Change> {
        // The path is gone, so we cannot tell file from folder; the UI
        // removes the path and anything below it.
        self.relative(path).map(|path| Change::Removed { path })
    }

    fn renamed(&self, from: &Path, to: &Path) -> Vec<Change> {
        match (self.relative(from), self.node(to)) {
            (Some(from), Some(to)) => vec![Change::Renamed { from, to }],
            (Some(from), None) => vec![Change::Removed { path: from }],
            (None, Some(node)) => vec![Change::Added { node }],
            (None, None) => Vec::new(),
        }
    }
}

/// Keeps only the last `Modified` per path in a burst.
fn dedupe(changes: Vec<Change>) -> Vec<Change> {
    let mut out: Vec<Change> = Vec::with_capacity(changes.len());
    for change in changes {
        if let Change::Modified { node } = &change
            && let Some(i) = out
                .iter()
                .position(|c| matches!(c, Change::Modified { node: n } if n.path == node.path))
        {
            out.remove(i);
        }
        out.push(change);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::Event;
    use notify::event::{CreateKind, DataChange, RemoveKind};
    use std::fs;

    fn mapper(root: &Path) -> EventMapper {
        EventMapper::new(
            root.to_path_buf(),
            DiscoveryRules::default(),
            SelfWrites::default(),
        )
    }

    fn ev(kind: EventKind, paths: &[&Path]) -> Event {
        let mut e = Event::new(kind);
        for p in paths {
            e = e.add_path(p.to_path_buf());
        }
        e
    }

    #[test]
    fn create_modify_remove() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let doc = root.join("a.md");
        fs::write(&doc, "# A\n").unwrap();
        let txt = root.join("a.txt");
        fs::write(&txt, "x").unwrap();
        let m = mapper(root);

        let changes = m.map(
            [
                ev(EventKind::Create(CreateKind::File), &[&doc]),
                ev(EventKind::Create(CreateKind::File), &[&txt]),
                ev(
                    EventKind::Modify(ModifyKind::Data(DataChange::Content)),
                    &[&doc],
                ),
                ev(
                    EventKind::Modify(ModifyKind::Data(DataChange::Content)),
                    &[&doc],
                ),
            ]
            .iter(),
        );
        assert_eq!(changes.len(), 2, "{changes:?}");
        assert!(
            matches!(&changes[0], Change::Added { node } if node.path == "a.md" && node.title.as_deref() == Some("A"))
        );
        assert!(matches!(&changes[1], Change::Modified { node } if node.path == "a.md"));

        fs::remove_file(&doc).unwrap();
        let changes = m.map([ev(EventKind::Remove(RemoveKind::File), &[&doc])].iter());
        assert_eq!(
            changes,
            [Change::Removed {
                path: "a.md".into()
            }]
        );
    }

    #[test]
    fn rename_and_ignored_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".gitignore"), "build/\n").unwrap();
        fs::create_dir_all(root.join("build")).unwrap();
        fs::create_dir_all(root.join("node_modules")).unwrap();
        let new = root.join("new.md");
        fs::write(&new, "").unwrap();
        let m = mapper(root);

        let changes = m.map(
            [ev(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                &[&root.join("old.md"), &new],
            )]
            .iter(),
        );
        assert!(
            matches!(&changes[0], Change::Renamed { from, to } if from == "old.md" && to.path == "new.md")
        );

        let ignored = root.join("build/x.md");
        fs::write(&ignored, "").unwrap();
        let skipped = root.join("node_modules/y.md");
        fs::write(&skipped, "").unwrap();
        let changes = m.map(
            [
                ev(EventKind::Create(CreateKind::File), &[&ignored]),
                ev(EventKind::Create(CreateKind::File), &[&skipped]),
            ]
            .iter(),
        );
        assert!(changes.is_empty(), "{changes:?}");
    }

    #[test]
    fn self_writes_are_not_reported() {
        let dir = tempfile::tempdir().unwrap();
        let doc = dir.path().join("a.md");
        fs::write(&doc, "x").unwrap();
        let writes = SelfWrites::default();
        writes.record(&doc, modified_ms(&doc).unwrap());
        let m = EventMapper::new(
            dir.path().to_path_buf(),
            DiscoveryRules::default(),
            writes.clone(),
        );
        let modify = ev(
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            &[&doc],
        );
        assert!(m.map([modify.clone()].iter()).is_empty());

        // A later external write with a different mtime is reported.
        std::thread::sleep(Duration::from_millis(20));
        fs::write(&doc, "changed elsewhere").unwrap();
        let later = std::time::SystemTime::now() + Duration::from_secs(2);
        fs::File::options()
            .write(true)
            .open(&doc)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_eq!(m.map([modify].iter()).len(), 1);
    }

    #[test]
    fn live_watcher_reports_new_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce_canonical(dir.path());
        let (tx, rx) = std::sync::mpsc::channel();
        let _w = RootWatcher::start(
            root.clone(),
            DiscoveryRules::default(),
            SelfWrites::default(),
            move |c| {
                let _ = tx.send(c);
            },
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        fs::write(root.join("live.md"), "# Live\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = false;
        while Instant::now() < deadline && !seen {
            if let Ok(changes) = rx.recv_timeout(Duration::from_millis(200)) {
                seen = changes.iter().any(|c| match c {
                    Change::Added { node } | Change::Modified { node } => node.path == "live.md",
                    _ => false,
                });
            }
        }
        assert!(seen, "watcher did not report live.md");
    }

    /// Temp dirs on macOS live behind a /var → /private/var symlink; the
    /// watcher reports resolved paths, so the root must be resolved too.
    fn dunce_canonical(p: &Path) -> PathBuf {
        let c = fs::canonicalize(p).unwrap();
        let s = c.to_string_lossy();
        s.strip_prefix(r"\\?\").map_or(c.clone(), PathBuf::from)
    }
}
