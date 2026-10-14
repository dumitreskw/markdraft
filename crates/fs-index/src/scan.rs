//! Parallel discovery of documents under a root, streamed in batches.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ignore::{WalkBuilder, WalkState};

use crate::{DiscoveryRules, MARKDRAFT_IGNORE_FILE, TreeNode};

/// Bytes read from the start of each document to find its title.
const TITLE_PROBE_BYTES: usize = 16 * 1024;

/// A batch is flushed after this long even if it is not full, so the first
/// rows reach the UI quickly.
const FLUSH_INTERVAL: Duration = Duration::from_millis(30);

#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub root: PathBuf,
    pub rules: DiscoveryRules,
    /// `Some(1)` lists only the root's direct children (single-file mode).
    pub max_depth: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanStats {
    pub files: usize,
    pub dirs: usize,
    pub elapsed: Duration,
}

/// Walks `req.root` on all cores and calls `on_batch` with up to
/// `batch_size` nodes at a time, on the calling thread. Directories are
/// included; the UI decides whether to show empty ones.
pub fn scan(
    req: &ScanRequest,
    batch_size: usize,
    mut on_batch: impl FnMut(Vec<TreeNode>),
) -> ScanStats {
    let started = Instant::now();
    let (tx, rx) = crossbeam_channel::bounded::<TreeNode>(4096);
    let mut stats = ScanStats {
        files: 0,
        dirs: 0,
        elapsed: Duration::ZERO,
    };

    std::thread::scope(|s| {
        s.spawn(|| walk(req, tx));

        let mut batch = Vec::with_capacity(batch_size);
        let mut last_flush = Instant::now();
        loop {
            match rx.recv_timeout(FLUSH_INTERVAL) {
                Ok(node) => {
                    if node.is_dir {
                        stats.dirs += 1;
                    } else {
                        stats.files += 1;
                    }
                    batch.push(node);
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }
            if batch.len() >= batch_size
                || (!batch.is_empty() && last_flush.elapsed() >= FLUSH_INTERVAL)
            {
                on_batch(std::mem::replace(
                    &mut batch,
                    Vec::with_capacity(batch_size),
                ));
                last_flush = Instant::now();
            }
        }
        if !batch.is_empty() {
            on_batch(batch);
        }
    });

    stats.elapsed = started.elapsed();
    stats
}

fn walk(req: &ScanRequest, tx: crossbeam_channel::Sender<TreeNode>) {
    let respect = req.rules.respect_gitignore;
    let mut builder = WalkBuilder::new(&req.root);
    builder
        .hidden(true)
        .parents(respect)
        .ignore(respect)
        .git_ignore(respect)
        .git_global(respect)
        .git_exclude(respect)
        // Honour .gitignore in plain folders too, not only inside repos.
        .require_git(false)
        .max_depth(req.max_depth)
        .filter_entry(|entry| {
            let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
            !(is_dir && DiscoveryRules::is_skipped_dir(&entry.file_name().to_string_lossy()))
        });
    builder.add_custom_ignore_filename(MARKDRAFT_IGNORE_FILE);

    let root = req.root.as_path();
    let rules = &req.rules;
    builder.build_parallel().run(|| {
        let tx = tx.clone();
        Box::new(move |result| {
            let Ok(entry) = result else {
                return WalkState::Continue;
            };
            if entry.depth() == 0 {
                return WalkState::Continue;
            }
            let Ok(relative) = entry.path().strip_prefix(root) else {
                return WalkState::Continue;
            };
            let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
            if !is_dir && !rules.is_document(relative) {
                return WalkState::Continue;
            }
            let modified_ms = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map_or(0, system_time_ms);
            let node = TreeNode {
                path: to_slash(relative),
                is_dir,
                title: if is_dir {
                    None
                } else {
                    read_title(entry.path())
                },
                modified_ms,
            };
            if tx.send(node).is_err() {
                return WalkState::Quit;
            }
            WalkState::Continue
        })
    });
}

/// Reads a document's title from its first few kilobytes.
pub fn read_title(path: &Path) -> Option<String> {
    let mut buf = Vec::with_capacity(TITLE_PROBE_BYTES);
    File::open(path)
        .ok()?
        .take(TITLE_PROBE_BYTES as u64)
        .read_to_end(&mut buf)
        .ok()?;
    // Cut at the last newline so a multi-byte character is never split.
    if buf.len() == TITLE_PROBE_BYTES
        && let Some(end) = buf.iter().rposition(|&b| b == b'\n')
    {
        buf.truncate(end);
    }
    let text = String::from_utf8_lossy(&buf);
    md_core::extract_title(text.trim_start_matches('\u{feff}'))
}

/// A path relative to a root as a `/`-separated string.
pub(crate) fn to_slash(relative: &Path) -> String {
    let mut out = String::new();
    for (i, part) in relative.components().enumerate() {
        if i > 0 {
            out.push('/');
        }
        out.push_str(&part.as_os_str().to_string_lossy());
    }
    out
}

pub(crate) fn system_time_ms(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn scan_paths(req: &ScanRequest) -> (Vec<String>, Vec<TreeNode>) {
        let mut nodes = Vec::new();
        scan(req, 2, |batch| nodes.extend(batch));
        nodes.sort_by(|a, b| a.path.cmp(&b.path));
        (nodes.iter().map(|n| n.path.clone()).collect(), nodes)
    }

    #[test]
    fn finds_documents_and_respects_ignores() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "README.md", "# Read me\n");
        write(root, "docs/guide.markdown", "---\ntitle: Guide\n---\n");
        write(root, "docs/notes.txt", "not markdown");
        write(root, "node_modules/pkg/README.md", "# skipped");
        write(root, "build/out.md", "# ignored by gitignore");
        write(root, "private/secret.md", "# ignored by markdraftignore");
        write(root, ".hidden/x.md", "# hidden");
        write(root, ".gitignore", "build/\n");
        write(root, ".markdraftignore", "private/\n");

        let req = ScanRequest {
            root: root.to_path_buf(),
            rules: DiscoveryRules::default(),
            max_depth: None,
        };
        let (paths, nodes) = scan_paths(&req);
        assert_eq!(paths, ["README.md", "docs", "docs/guide.markdown"]);
        let titles: Vec<_> = nodes.iter().map(|n| n.title.as_deref()).collect();
        assert_eq!(titles, [Some("Read me"), None, Some("Guide")]);
        assert!(nodes[0].modified_ms > 0);
    }

    #[test]
    fn gitignore_can_be_disabled_and_depth_limited() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "a.md", "");
        write(root, "build/b.md", "");
        write(root, ".gitignore", "build/\n");

        let mut req = ScanRequest {
            root: root.to_path_buf(),
            rules: DiscoveryRules::new(&["**/*.md"], &[], false).unwrap(),
            max_depth: None,
        };
        assert_eq!(scan_paths(&req).0, ["a.md", "build", "build/b.md"]);
        req.max_depth = Some(1);
        assert_eq!(scan_paths(&req).0, ["a.md", "build"]);
    }

    #[test]
    fn title_probe_handles_bom_and_long_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.md");
        fs::write(&path, "\u{feff}# Bom title\n").unwrap();
        assert_eq!(read_title(&path).as_deref(), Some("Bom title"));
        let long = format!("{}\n# Late\n", "word ".repeat(10_000));
        fs::write(&path, long).unwrap();
        assert_eq!(read_title(&path), None);
    }

    #[test]
    fn batches_respect_size() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..10 {
            write(dir.path(), &format!("{i}.md"), "");
        }
        let req = ScanRequest {
            root: dir.path().to_path_buf(),
            rules: DiscoveryRules::default(),
            max_depth: None,
        };
        let mut sizes = Vec::new();
        let stats = scan(&req, 3, |b| sizes.push(b.len()));
        assert!(sizes.iter().all(|&n| n <= 3));
        assert_eq!(sizes.iter().sum::<usize>(), 10);
        assert_eq!(stats.files, 10);
    }
}
