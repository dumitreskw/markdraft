//! Read-only access to the Git repository that contains a document.
//!
//! Every path handed to or returned from this module is an absolute OS path.
//! Internally paths are mapped to repository-relative, `/`-separated paths
//! (the form Git stores) by [`Repo::relative`].
//!
//! MarkDraft never writes to the repository: no index refresh, no checkout,
//! no commits. Opening a [`Repo`] is cheap; callers can keep one per open
//! workspace root. `Repo` is `Send` but not `Sync` (libgit2 handles are not
//! thread-safe), so share it behind a `Mutex`.

use std::fs;
use std::path::{Component, Path, PathBuf};

use git2::{
    Delta, DiffFindOptions, ErrorCode, ObjectType, Oid, Repository, RepositoryOpenFlags, Sort,
    Status, StatusOptions,
};
use serde::Serialize;

/// Errors returned by [`Repo`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An error reported by libgit2.
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    /// A file-system error while resolving or reading a path.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// The path is not inside the repository's working directory.
    #[error("{} is outside the repository at {}", .path.display(), .workdir.display())]
    OutsideRepo {
        /// The path that was asked about.
        path: PathBuf,
        /// The repository working directory.
        workdir: PathBuf,
    },
    /// The revision string could not be resolved to a commit.
    #[error("unknown revision `{0}`")]
    UnknownRevision(String),
}

impl Serialize for Error {
    /// Errors cross the IPC boundary as their display message.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

/// Which version of a file to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Revision {
    /// The file as committed in `HEAD`.
    Head,
    /// The file as staged in the index (stage 0; "ours" while conflicted).
    Index,
    /// The file on disk in the working directory.
    Workdir,
    /// The file at a commit: full or abbreviated id, or any revspec Git
    /// understands (`main`, `HEAD~2`, a tag).
    Commit(String),
    /// The file in the first parent of the given commit, following a rename
    /// made by that commit. `None` for a root commit.
    ParentOf(String),
}

/// The kind of change Git reports for a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StatusKind {
    /// Content changed.
    Modified,
    /// New file staged in the index.
    Added,
    /// File deleted (staged or on disk).
    Deleted,
    /// File renamed in the index (`old_path` holds the previous name).
    Renamed,
    /// File not tracked by Git and not ignored.
    Untracked,
    /// Unresolved merge conflict.
    Conflicted,
    /// File type changed (e.g. file became a symlink).
    TypeChange,
}

/// One changed file in the working directory or index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    /// Absolute path of the file (the new name for renames).
    pub path: PathBuf,
    /// The most significant change; see [`StatusKind`].
    pub kind: StatusKind,
    /// The index differs from `HEAD` for this file.
    pub staged: bool,
    /// The working directory differs from the index (untracked files count).
    pub unstaged: bool,
    /// Absolute previous path for renames.
    pub old_path: Option<PathBuf>,
}

/// One commit in a file's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    /// Full hex object id.
    pub id: String,
    /// Abbreviated id (unique in the repository, at least 7 characters).
    pub short_id: String,
    /// First line of the message.
    pub summary: String,
    /// Full commit message.
    pub message: String,
    /// Author name.
    pub author_name: String,
    /// Author e-mail.
    pub author_email: String,
    /// Author time, milliseconds since the Unix epoch.
    pub time_ms: i64,
    /// Absolute path the file had in this commit (differs from the asked
    /// path when the file was renamed later).
    pub path: PathBuf,
}

/// The commit `HEAD` points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadCommit {
    /// Full hex object id.
    pub id: String,
    /// Abbreviated id.
    pub short_id: String,
    /// First line of the message.
    pub summary: String,
}

/// Summary of the repository state for a status bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    /// Absolute working directory.
    pub workdir: PathBuf,
    /// Current branch name; `None` when `HEAD` is detached.
    pub branch: Option<String>,
    /// `HEAD` is detached (checked out at a commit).
    pub detached: bool,
    /// The commit `HEAD` points at; `None` on a branch with no commits yet.
    pub head: Option<HeadCommit>,
    /// Upstream branch name, e.g. `origin/main`, when one is configured.
    pub upstream: Option<String>,
    /// Commits on the branch that the upstream does not have.
    pub ahead: Option<usize>,
    /// Commits on the upstream that the branch does not have.
    pub behind: Option<usize>,
    /// An operation is in progress: `merge`, `rebase`, `cherryPick`,
    /// `revert`, `bisect` or `applyMailbox`; `None` when clean.
    pub operation: Option<String>,
}

/// A Git repository with a working directory, opened read-only.
pub struct Repo {
    repo: Repository,
    workdir: PathBuf,
}

impl std::fmt::Debug for Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Repo")
            .field("workdir", &self.workdir)
            .finish()
    }
}

impl Repo {
    /// Finds the repository containing `path` (a file or folder, which need
    /// not exist). `Ok(None)` when the path is not inside a repository or the
    /// repository is bare.
    pub fn discover(path: &Path) -> Result<Option<Repo>, Error> {
        let resolved = resolve(path)?;
        // Start from the nearest existing directory.
        let mut start = resolved.as_path();
        while !start.is_dir() {
            match start.parent() {
                Some(parent) => start = parent,
                None => return Ok(None),
            }
        }
        let repo = match Repository::open_ext(
            start,
            RepositoryOpenFlags::empty(),
            std::iter::empty::<&std::ffi::OsStr>(),
        ) {
            Ok(repo) => repo,
            Err(e) if e.code() == ErrorCode::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let Some(workdir) = repo.workdir() else {
            return Ok(None);
        };
        let workdir = dunce::canonicalize(workdir)?;
        Ok(Some(Repo { repo, workdir }))
    }

    /// Absolute, canonical working directory.
    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    /// Branch, `HEAD` commit, upstream ahead/behind and in-progress operation.
    /// Never fails: parts that cannot be determined are `None`.
    pub fn info(&self) -> RepoInfo {
        let repo = &self.repo;
        let detached = repo.head_detached().unwrap_or(false);
        let head_ref = repo.head().ok();

        let branch = if detached {
            None
        } else if let Some(r) = &head_ref {
            r.shorthand().ok().map(str::to_owned)
        } else {
            // Unborn branch: HEAD is a symbolic ref to a branch with no commits.
            repo.find_reference("HEAD").ok().and_then(|r| {
                r.symbolic_target()
                    .ok()
                    .flatten()
                    .map(|t| t.trim_start_matches("refs/heads/").to_owned())
            })
        };

        let head_commit = head_ref.as_ref().and_then(|r| r.peel_to_commit().ok());
        let head = head_commit.as_ref().map(|c| HeadCommit {
            id: c.id().to_string(),
            short_id: short_id(c.as_object()),
            summary: lossy_summary(c),
        });

        let (mut upstream, mut ahead, mut behind) = (None, None, None);
        if let (Some(name), Some(commit)) = (&branch, &head_commit)
            && let Ok(local) = repo.find_branch(name, git2::BranchType::Local)
            && let Ok(up) = local.upstream()
        {
            upstream = up.name().ok().flatten().map(str::to_owned);
            if let Some(up_oid) = up.get().target()
                && let Ok((a, b)) = repo.graph_ahead_behind(commit.id(), up_oid)
            {
                ahead = Some(a);
                behind = Some(b);
            }
        }

        let operation = match repo.state() {
            git2::RepositoryState::Clean => None,
            git2::RepositoryState::Merge => Some("merge"),
            git2::RepositoryState::Revert | git2::RepositoryState::RevertSequence => Some("revert"),
            git2::RepositoryState::CherryPick | git2::RepositoryState::CherryPickSequence => {
                Some("cherryPick")
            }
            git2::RepositoryState::Bisect => Some("bisect"),
            git2::RepositoryState::Rebase
            | git2::RepositoryState::RebaseInteractive
            | git2::RepositoryState::RebaseMerge => Some("rebase"),
            git2::RepositoryState::ApplyMailbox | git2::RepositoryState::ApplyMailboxOrRebase => {
                Some("applyMailbox")
            }
        }
        .map(str::to_owned);

        RepoInfo {
            workdir: self.workdir.clone(),
            branch,
            detached,
            head,
            upstream,
            ahead,
            behind,
            operation,
        }
    }

    /// Changed files under the working directory, or under the `scope`
    /// folder. Ignored files are excluded; untracked files are included
    /// (untracked folders are listed file by file). Sorted by path.
    pub fn status(&self, scope: Option<&Path>) -> Result<Vec<FileStatus>, Error> {
        let prefix = match scope {
            Some(scope) => self.relative(scope)?,
            None => String::new(),
        };
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false)
            .exclude_submodules(true)
            .renames_head_to_index(true);
        let literal = !prefix.contains(['*', '?', '[', '\\']);
        if !prefix.is_empty() && literal {
            opts.pathspec(prefix.as_str());
        }
        let statuses = self.repo.statuses(Some(&mut opts))?;

        let mut out = Vec::with_capacity(statuses.len());
        for entry in statuses.iter() {
            let status = entry.status();
            if status.is_ignored() || status.is_empty() {
                continue;
            }
            // `path_bytes` is the old name for renames; prefer the new one.
            let renamed = entry
                .head_to_index()
                .filter(|d| d.status() == Delta::Renamed);
            let new_rel = renamed
                .as_ref()
                .and_then(|d| d.new_file().path_bytes())
                .unwrap_or(entry.path_bytes());
            let new_rel = String::from_utf8_lossy(new_rel);
            if !prefix.is_empty() && !in_scope(&new_rel, &prefix) {
                continue;
            }
            let old_path = renamed
                .as_ref()
                .and_then(|d| d.old_file().path_bytes())
                .map(|p| self.absolute(&String::from_utf8_lossy(p)));
            let mut fs = file_status_from(self.absolute(&new_rel), status);
            if fs.kind == StatusKind::Renamed || old_path.is_some() {
                fs.old_path = old_path;
            }
            out.push(fs);
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    /// Status of one file; `None` when it is unchanged, ignored or unknown.
    pub fn file_status(&self, path: &Path) -> Result<Option<StatusKind>, Error> {
        let rel = self.relative(path)?;
        let status = match self.repo.status_file(Path::new(&rel)) {
            Ok(s) => s,
            Err(e) if matches!(e.code(), ErrorCode::NotFound | ErrorCode::Ambiguous) => {
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        };
        if status.is_ignored() || status.is_empty() {
            return Ok(None);
        }
        // `status_file` does no rename detection; a staged new file may be
        // the destination of a rename, which only a full status reveals.
        if status.is_index_new() {
            let abs = self.absolute(&rel);
            if let Some(found) = self
                .status(abs.parent())?
                .into_iter()
                .find(|s| s.path == abs)
            {
                return Ok(Some(found.kind));
            }
        }
        Ok(Some(file_status_from(PathBuf::new(), status).kind))
    }

    /// Commits that changed `path`, newest first, at most `limit`. Renames
    /// are followed best-effort: when a commit created the file by renaming
    /// another, older commits are searched under the old name. Merge commits
    /// are listed only when the file differs from every parent.
    pub fn file_history(&self, path: &Path, limit: usize) -> Result<Vec<CommitInfo>, Error> {
        let mut out = Vec::new();
        if limit == 0 {
            return Ok(out);
        }
        let mut current = self.relative(path)?;
        let mut walk = self.repo.revwalk()?;
        walk.set_sorting(Sort::TIME)?;
        let head = match self.repo.head() {
            Ok(head) => head,
            // Unborn branch: no history yet.
            Err(e) if matches!(e.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => {
                return Ok(out);
            }
            Err(e) => return Err(e.into()),
        };
        walk.push(head.peel_to_commit()?.id())?;

        for oid in walk {
            let commit = self.repo.find_commit(oid?)?;
            let tree = commit.tree()?;
            let Some(entry) = entry_id(&tree, &current) else {
                continue;
            };
            let parents: Vec<_> = commit.parents().collect();
            let mut treesame = false;
            let mut in_first_parent = false;
            for (i, parent) in parents.iter().enumerate() {
                let parent_entry = entry_id(&parent.tree()?, &current);
                if i == 0 {
                    in_first_parent = parent_entry.is_some();
                }
                if parent_entry == Some(entry) {
                    treesame = true;
                    break;
                }
            }
            if treesame {
                continue;
            }
            out.push(self.commit_info(&commit, &current));
            if out.len() >= limit {
                break;
            }
            if !parents.is_empty() && !in_first_parent {
                // The file appeared in this commit: renamed or created.
                match self.rename_source(&parents[0].tree()?, &tree, &current)? {
                    Some(old) => current = old,
                    None => break,
                }
            }
        }
        Ok(out)
    }

    /// Text of `path` at `rev`; `None` when the file does not exist there or
    /// is binary. Text is decoded as UTF-8 (lossily for invalid bytes) with a
    /// leading BOM removed. Line endings are returned as stored.
    pub fn read_at(&self, path: &Path, rev: &Revision) -> Result<Option<String>, Error> {
        let rel = self.relative(path)?;
        match rev {
            Revision::Workdir => {
                let abs = self.absolute(&rel);
                match fs::read(&abs) {
                    Ok(bytes) if looks_binary(&bytes) => Ok(None),
                    Ok(bytes) => Ok(Some(decode(&bytes))),
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::IsADirectory
                        ) =>
                    {
                        Ok(None)
                    }
                    Err(e) => Err(e.into()),
                }
            }
            Revision::Index => {
                let index = self.repo.index()?;
                let entry = index
                    .get_path(Path::new(&rel), 0)
                    .or_else(|| index.get_path(Path::new(&rel), 2));
                match entry {
                    Some(entry) => self.blob_text(entry.id),
                    None => Ok(None),
                }
            }
            Revision::Head => {
                let head = match self.repo.head() {
                    Ok(h) => h,
                    Err(e) if matches!(e.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => {
                        return Ok(None);
                    }
                    Err(e) => return Err(e.into()),
                };
                let tree = head.peel_to_commit()?.tree()?;
                self.tree_text(&tree, &rel)
            }
            Revision::Commit(spec) => {
                let tree = self.find_commit(spec)?.tree()?;
                self.tree_text(&tree, &rel)
            }
            Revision::ParentOf(spec) => {
                let commit = self.find_commit(spec)?;
                let Ok(parent) = commit.parent(0) else {
                    return Ok(None);
                };
                let parent_tree = parent.tree()?;
                if entry_id(&parent_tree, &rel).is_some() {
                    return self.tree_text(&parent_tree, &rel);
                }
                match self.rename_source(&parent_tree, &commit.tree()?, &rel)? {
                    Some(old) => self.tree_text(&parent_tree, &old),
                    None => Ok(None),
                }
            }
        }
    }

    /// Maps an absolute (or workdir-relative) path to the repository-relative
    /// `/`-separated form Git uses. The path need not exist. On Windows the
    /// comparison with the working directory ignores case.
    pub fn relative(&self, path: &Path) -> Result<String, Error> {
        let abs = if path.is_absolute() {
            resolve(path)?
        } else {
            resolve(&self.workdir.join(path))?
        };
        let rest = strip_prefix(&abs, &self.workdir).ok_or_else(|| Error::OutsideRepo {
            path: path.to_path_buf(),
            workdir: self.workdir.clone(),
        })?;
        let mut parts = Vec::new();
        for c in rest.components() {
            match c {
                Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
                Component::CurDir => {}
                _ => {
                    return Err(Error::OutsideRepo {
                        path: path.to_path_buf(),
                        workdir: self.workdir.clone(),
                    });
                }
            }
        }
        Ok(parts.join("/"))
    }

    /// Absolute OS path of a repository-relative path.
    pub fn absolute(&self, rel: &str) -> PathBuf {
        let mut p = self.workdir.clone();
        p.extend(rel.split('/').filter(|s| !s.is_empty()));
        p
    }

    fn find_commit(&self, spec: &str) -> Result<git2::Commit<'_>, Error> {
        let obj = self
            .repo
            .revparse_single(spec)
            .map_err(|_| Error::UnknownRevision(spec.to_owned()))?;
        obj.peel_to_commit()
            .map_err(|_| Error::UnknownRevision(spec.to_owned()))
    }

    fn tree_text(&self, tree: &git2::Tree<'_>, rel: &str) -> Result<Option<String>, Error> {
        match entry_id(tree, rel) {
            Some(id) => self.blob_text(id),
            None => Ok(None),
        }
    }

    fn blob_text(&self, id: Oid) -> Result<Option<String>, Error> {
        let blob = match self.repo.find_blob(id) {
            Ok(b) => b,
            // Gitlinks (submodules) and missing objects have no text.
            Err(e) if e.code() == ErrorCode::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if blob.is_binary() {
            return Ok(None);
        }
        Ok(Some(decode(blob.content())))
    }

    /// When `rel` was added between `old_tree` and `new_tree`, returns the
    /// path it was renamed (or copied) from, if Git's similarity detection
    /// finds one.
    fn rename_source(
        &self,
        old_tree: &git2::Tree<'_>,
        new_tree: &git2::Tree<'_>,
        rel: &str,
    ) -> Result<Option<String>, Error> {
        let mut diff = self
            .repo
            .diff_tree_to_tree(Some(old_tree), Some(new_tree), None)?;
        let has_add = diff.deltas().any(|d| {
            d.status() == Delta::Added && d.new_file().path_bytes() == Some(rel.as_bytes())
        });
        if !has_add {
            return Ok(None);
        }
        diff.find_similar(Some(DiffFindOptions::new().renames(true)))?;
        Ok(diff
            .deltas()
            .find(|d| {
                matches!(d.status(), Delta::Renamed | Delta::Copied)
                    && d.new_file().path_bytes() == Some(rel.as_bytes())
            })
            .and_then(|d| d.old_file().path_bytes().map(<[u8]>::to_vec))
            .map(|b| String::from_utf8_lossy(&b).into_owned()))
    }

    fn commit_info(&self, commit: &git2::Commit<'_>, rel: &str) -> CommitInfo {
        let author = commit.author();
        let message = String::from_utf8_lossy(commit.message_bytes()).into_owned();
        CommitInfo {
            id: commit.id().to_string(),
            short_id: short_id(commit.as_object()),
            summary: lossy_summary(commit),
            message,
            author_name: String::from_utf8_lossy(author.name_bytes()).into_owned(),
            author_email: String::from_utf8_lossy(author.email_bytes()).into_owned(),
            time_ms: author.when().seconds() * 1000,
            path: self.absolute(rel),
        }
    }
}

/// Picks the most significant [`StatusKind`] for a set of status flags.
fn file_status_from(path: PathBuf, s: Status) -> FileStatus {
    let staged = s.intersects(
        Status::INDEX_NEW
            | Status::INDEX_MODIFIED
            | Status::INDEX_DELETED
            | Status::INDEX_RENAMED
            | Status::INDEX_TYPECHANGE,
    );
    let unstaged = s.intersects(
        Status::WT_NEW
            | Status::WT_MODIFIED
            | Status::WT_DELETED
            | Status::WT_RENAMED
            | Status::WT_TYPECHANGE,
    );
    let kind = if s.is_conflicted() {
        StatusKind::Conflicted
    } else if s.intersects(Status::INDEX_DELETED | Status::WT_DELETED) {
        StatusKind::Deleted
    } else if s.intersects(Status::INDEX_RENAMED | Status::WT_RENAMED) {
        StatusKind::Renamed
    } else if s.is_index_new() {
        StatusKind::Added
    } else if s.intersects(Status::INDEX_MODIFIED | Status::WT_MODIFIED) {
        StatusKind::Modified
    } else if s.intersects(Status::INDEX_TYPECHANGE | Status::WT_TYPECHANGE) {
        StatusKind::TypeChange
    } else {
        StatusKind::Untracked
    };
    FileStatus {
        path,
        kind,
        staged,
        unstaged: unstaged || s.is_conflicted(),
        old_path: None,
    }
}

/// Blob id of a regular file (or symlink) at `rel` in `tree`.
fn entry_id(tree: &git2::Tree<'_>, rel: &str) -> Option<Oid> {
    if rel.is_empty() {
        return None;
    }
    let entry = tree.get_path(Path::new(rel)).ok()?;
    (entry.kind() == Some(ObjectType::Blob)).then(|| entry.id())
}

fn in_scope(rel: &str, prefix: &str) -> bool {
    rel == prefix
        || rel
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
        || prefix.ends_with('/') && rel.starts_with(prefix)
}

fn short_id(obj: &git2::Object<'_>) -> String {
    obj.short_id()
        .ok()
        .and_then(|b| b.as_str().ok().map(str::to_owned))
        .unwrap_or_else(|| obj.id().to_string()[..7].to_owned())
}

fn lossy_summary(commit: &git2::Commit<'_>) -> String {
    commit
        .summary_bytes()
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default()
}

/// Same heuristic as Git: a NUL byte in the first 8000 bytes means binary.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(8000)].contains(&0)
}

/// Decodes file bytes as UTF-8 (lossy), dropping a UTF-8 BOM.
pub(crate) fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Makes `path` absolute and canonical where it exists: the longest existing
/// ancestor is canonicalised (resolving symlinks, `..` and on-disk case) and
/// the missing tail is appended as given.
fn resolve(path: &Path) -> Result<PathBuf, Error> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut existing = path.as_path();
    let mut tail = Vec::new();
    loop {
        match dunce::canonicalize(existing) {
            Ok(mut canon) => {
                for part in tail.iter().rev() {
                    canon.push(part);
                }
                return Ok(canon);
            }
            Err(_) => match (existing.parent(), existing.file_name()) {
                (Some(parent), Some(name)) => {
                    tail.push(name.to_os_string());
                    existing = parent;
                }
                _ => return Ok(path),
            },
        }
    }
}

/// `path` relative to `base`, comparing components case-insensitively on
/// Windows and macOS (whose default file systems ignore case).
fn strip_prefix<'a>(path: &'a Path, base: &Path) -> Option<&'a Path> {
    if let Ok(rest) = path.strip_prefix(base) {
        return Some(rest);
    }
    if !cfg!(any(windows, target_os = "macos")) {
        return None;
    }
    let mut rest = path.components();
    for b in base.components() {
        let p = rest.next()?;
        if !p
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
        {
            return None;
        }
    }
    Some(rest.as_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_strips_bom_and_handles_invalid_utf8() {
        assert_eq!(decode(b"\xEF\xBB\xBFhello"), "hello");
        assert_eq!(decode(b"a\xFFb"), "a\u{FFFD}b");
    }

    #[test]
    fn scope_matching() {
        assert!(in_scope("docs/a.md", "docs"));
        assert!(in_scope("docs", "docs"));
        assert!(!in_scope("docs2/a.md", "docs"));
    }

    #[test]
    fn binary_detection() {
        assert!(looks_binary(b"PK\x03\x04\0\0"));
        assert!(!looks_binary("héllo".as_bytes()));
    }
}
