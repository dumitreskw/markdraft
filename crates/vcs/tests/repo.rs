//! Integration tests for `vcs::repo` against throwaway repositories built
//! with git2.

use std::fs;
use std::path::{Path, PathBuf};

use git2::{Repository, Signature, Time};
use tempfile::TempDir;
use vcs::{Repo, Revision, StatusKind};

struct Fixture {
    _dir: TempDir,
    root: PathBuf,
    git: Repository,
    commits: Vec<String>,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let git = Repository::init(&root).unwrap();
        Fixture {
            _dir: dir,
            root,
            git,
            commits: Vec::new(),
        }
    }

    fn path(&self, rel: &str) -> PathBuf {
        let mut p = self.root.clone();
        p.extend(rel.split('/'));
        p
    }

    fn write(&self, rel: &str, content: &str) {
        self.write_bytes(rel, content.as_bytes());
    }

    fn write_bytes(&self, rel: &str, content: &[u8]) {
        let p = self.path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    fn stage(&self, rel: &str) {
        let mut index = self.git.index().unwrap();
        if self.path(rel).exists() {
            index.add_path(Path::new(rel)).unwrap();
        } else {
            index.remove_path(Path::new(rel)).unwrap();
        }
        index.write().unwrap();
    }

    fn remove(&self, rel: &str) {
        fs::remove_file(self.path(rel)).unwrap();
    }

    /// Commits the current index with a fixed timestamp so history order is
    /// deterministic; returns the full id.
    fn commit(&mut self, message: &str) -> String {
        let seconds = 1_700_000_000 + 1000 * self.commits.len() as i64;
        let sig =
            Signature::new("Ada Lovelace", "ada@example.com", &Time::new(seconds, 0)).unwrap();
        let mut index = self.git.index().unwrap();
        let tree = self.git.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = self.git.head().ok().map(|h| h.peel_to_commit().unwrap());
        let parents: Vec<_> = parent.iter().collect();
        let id = self
            .git
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap();
        self.commits.push(id.to_string());
        id.to_string()
    }

    fn repo(&self) -> Repo {
        Repo::discover(&self.root)
            .unwrap()
            .expect("repository found")
    }
}

const GUIDE_V1: &str = "# Guide\n\nThe quick brown fox.\n";
const GUIDE_V2: &str = "# Guide\n\nThe fast brown fox.\n";
const GUIDE_STAGED: &str = "# Guide\n\nThe fast brown fox jumps.\n";
const GUIDE_WORKDIR: &str = "# Guide\n\nThe fast brown fox jumps high.\n";
const NAMING: &str = "# Naming\n\nFiles are named after their topic, in lower case.\n\
                      Words are separated by hyphens.\n";

/// History:
/// 1. add README, docs/guide, docs/old-name, docs/remove-me, docs/gone,
///    docs/staged-old, .gitignore
/// 2. edit docs/guide
/// 3. rename docs/old-name -> docs/new-name, delete docs/remove-me
/// 4. edit docs/new-name
///
/// Working state: README edited (unstaged), docs/guide staged + further
/// unstaged edit, docs/staged-old renamed in the index, docs/gone deleted on
/// disk, notes/drafts/idea.md untracked, debug.log + build/out.md ignored.
fn populated() -> Fixture {
    let mut fx = Fixture::new();
    fx.write(".gitignore", "*.log\nbuild/\n");
    fx.write("README.md", "# Project\n\nHello.\n");
    fx.write("docs/guide.md", GUIDE_V1);
    fx.write("docs/old-name.md", NAMING);
    fx.write("docs/remove-me.md", "Temporary.\n");
    fx.write("docs/gone.md", "Soon gone.\n");
    fx.write(
        "docs/staged-old.md",
        "A file that will be renamed in the index.\n",
    );
    for f in [
        ".gitignore",
        "README.md",
        "docs/guide.md",
        "docs/old-name.md",
        "docs/remove-me.md",
        "docs/gone.md",
        "docs/staged-old.md",
    ] {
        fx.stage(f);
    }
    fx.commit("Initial docs");

    fx.write("docs/guide.md", GUIDE_V2);
    fx.stage("docs/guide.md");
    fx.commit("Tighten guide wording");

    let old = fs::read_to_string(fx.path("docs/old-name.md")).unwrap();
    fx.remove("docs/old-name.md");
    fx.write("docs/new-name.md", &old);
    fx.remove("docs/remove-me.md");
    for f in ["docs/old-name.md", "docs/new-name.md", "docs/remove-me.md"] {
        fx.stage(f);
    }
    fx.commit("Rename naming doc\n\nAnd drop the temporary file.");

    fx.write("docs/new-name.md", &format!("{NAMING}Avoid spaces.\n"));
    fx.stage("docs/new-name.md");
    fx.commit("Expand naming rules");

    // Working state.
    fx.write("README.md", "# Project\n\nHello, world.\n");
    fx.write("docs/guide.md", GUIDE_STAGED);
    fx.stage("docs/guide.md");
    fx.write("docs/guide.md", GUIDE_WORKDIR);
    let staged = fs::read_to_string(fx.path("docs/staged-old.md")).unwrap();
    fx.remove("docs/staged-old.md");
    fx.write("docs/staged-new.md", &staged);
    fx.stage("docs/staged-old.md");
    fx.stage("docs/staged-new.md");
    fx.remove("docs/gone.md");
    fx.write("notes/drafts/idea.md", "An idea.\n");
    fx.write("debug.log", "noise\n");
    fx.write("build/out.md", "generated\n");
    fx
}

fn rel(fx: &Fixture, p: &Path) -> String {
    p.strip_prefix(&fx.root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn discover_finds_repo_from_files_and_folders() {
    let fx = populated();
    let from_file = Repo::discover(&fx.path("docs/guide.md")).unwrap().unwrap();
    assert_eq!(from_file.workdir(), fx.root);
    let from_dir = Repo::discover(&fx.path("notes/drafts")).unwrap().unwrap();
    assert_eq!(from_dir.workdir(), fx.root);
    // A file that does not exist (yet) still resolves to its repository.
    let missing = Repo::discover(&fx.path("docs/not-yet/new.md"))
        .unwrap()
        .unwrap();
    assert_eq!(missing.workdir(), fx.root);
}

#[test]
fn path_outside_any_repo_is_none() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("loose.md"), "x").unwrap();
    // Assumes the system temp folder is not inside a Git repository.
    assert!(
        Repo::discover(&dir.path().join("loose.md"))
            .unwrap()
            .is_none()
    );
    assert!(Repo::discover(dir.path()).unwrap().is_none());
}

#[test]
fn status_lists_changes_without_ignored_files() {
    let fx = populated();
    let repo = fx.repo();
    let status = repo.status(None).unwrap();
    let summary: Vec<_> = status
        .iter()
        .map(|s| {
            (
                rel(&fx, &s.path),
                s.kind,
                s.staged,
                s.unstaged,
                s.old_path.as_ref().map(|p| rel(&fx, p)),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            ("README.md".into(), StatusKind::Modified, false, true, None),
            (
                "docs/gone.md".into(),
                StatusKind::Deleted,
                false,
                true,
                None
            ),
            (
                "docs/guide.md".into(),
                StatusKind::Modified,
                true,
                true,
                None
            ),
            (
                "docs/staged-new.md".into(),
                StatusKind::Renamed,
                true,
                false,
                Some("docs/staged-old.md".into())
            ),
            (
                "notes/drafts/idea.md".into(),
                StatusKind::Untracked,
                false,
                true,
                None
            ),
        ]
    );
    for s in &status {
        assert!(s.path.is_absolute());
    }
}

#[test]
fn status_can_be_scoped_to_a_folder() {
    let fx = populated();
    let repo = fx.repo();
    let docs: Vec<_> = repo
        .status(Some(&fx.path("docs")))
        .unwrap()
        .into_iter()
        .map(|s| rel(&fx, &s.path))
        .collect();
    assert_eq!(
        docs,
        ["docs/gone.md", "docs/guide.md", "docs/staged-new.md"]
    );
    let notes = repo.status(Some(&fx.path("notes"))).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].kind, StatusKind::Untracked);
}

#[test]
fn file_status_per_file() {
    let fx = populated();
    let repo = fx.repo();
    let st = |p: &str| repo.file_status(&fx.path(p)).unwrap();
    assert_eq!(st("docs/guide.md"), Some(StatusKind::Modified));
    assert_eq!(st("README.md"), Some(StatusKind::Modified));
    assert_eq!(st("docs/staged-new.md"), Some(StatusKind::Renamed));
    assert_eq!(st("docs/gone.md"), Some(StatusKind::Deleted));
    assert_eq!(st("notes/drafts/idea.md"), Some(StatusKind::Untracked));
    assert_eq!(st("docs/new-name.md"), None, "committed and unchanged");
    assert_eq!(st("debug.log"), None, "ignored");
    assert_eq!(st("build/out.md"), None, "ignored folder");
    assert_eq!(st("does/not/exist.md"), None);
}

#[test]
fn history_lists_commits_newest_first() {
    let fx = populated();
    let repo = fx.repo();
    let history = repo.file_history(&fx.path("docs/guide.md"), 50).unwrap();
    let summaries: Vec<_> = history.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries, ["Tighten guide wording", "Initial docs"]);
    let c = &history[0];
    assert_eq!(c.id, fx.commits[1]);
    assert!(c.id.starts_with(&c.short_id) && c.short_id.len() >= 7);
    assert_eq!(c.author_name, "Ada Lovelace");
    assert_eq!(c.author_email, "ada@example.com");
    assert_eq!(c.time_ms, (1_700_000_000 + 1000) * 1000);
    assert_eq!(c.path, fx.path("docs/guide.md"));

    let limited = repo.file_history(&fx.path("docs/guide.md"), 1).unwrap();
    assert_eq!(limited.len(), 1);
}

#[test]
fn history_follows_renames() {
    let fx = populated();
    let repo = fx.repo();
    let history = repo.file_history(&fx.path("docs/new-name.md"), 50).unwrap();
    let summary: Vec<_> = history
        .iter()
        .map(|c| (c.summary.as_str(), rel(&fx, &c.path)))
        .collect();
    assert_eq!(
        summary,
        [
            ("Expand naming rules", "docs/new-name.md".to_owned()),
            ("Rename naming doc", "docs/new-name.md".to_owned()),
            ("Initial docs", "docs/old-name.md".to_owned()),
        ]
    );
    assert_eq!(
        history[1].message,
        "Rename naming doc\n\nAnd drop the temporary file."
    );
}

#[test]
fn read_at_revisions() {
    let fx = populated();
    let repo = fx.repo();
    let guide = fx.path("docs/guide.md");
    let read = |p: &Path, r: Revision| repo.read_at(p, &r).unwrap();

    assert_eq!(read(&guide, Revision::Head).as_deref(), Some(GUIDE_V2));
    assert_eq!(read(&guide, Revision::Index).as_deref(), Some(GUIDE_STAGED));
    assert_eq!(
        read(&guide, Revision::Workdir).as_deref(),
        Some(GUIDE_WORKDIR)
    );
    let first_short = &fx.commits[0][..8];
    assert_eq!(
        read(&guide, Revision::Commit(first_short.into())).as_deref(),
        Some(GUIDE_V1)
    );
    assert_eq!(
        read(&guide, Revision::ParentOf(fx.commits[1].clone())).as_deref(),
        Some(GUIDE_V1)
    );
    assert_eq!(
        read(&guide, Revision::ParentOf(fx.commits[0].clone())),
        None
    );

    // ParentOf follows the rename made by that commit.
    let renamed = fx.path("docs/new-name.md");
    assert_eq!(
        read(&renamed, Revision::ParentOf(fx.commits[2].clone())).as_deref(),
        Some(NAMING)
    );

    // Deleted file: present in the first commit only.
    let removed = fx.path("docs/remove-me.md");
    assert_eq!(read(&removed, Revision::Head), None);
    assert_eq!(read(&removed, Revision::Workdir), None);
    assert_eq!(
        read(&removed, Revision::Commit(fx.commits[0].clone())).as_deref(),
        Some("Temporary.\n")
    );

    // Untracked file exists only on disk.
    let idea = fx.path("notes/drafts/idea.md");
    assert_eq!(read(&idea, Revision::Index), None);
    assert_eq!(
        read(&idea, Revision::Workdir).as_deref(),
        Some("An idea.\n")
    );

    assert!(matches!(
        repo.read_at(&guide, &Revision::Commit("no-such-rev".into())),
        Err(vcs::Error::UnknownRevision(_))
    ));
}

#[test]
fn read_at_strips_bom_and_skips_binary() {
    let mut fx = Fixture::new();
    // LF only: a global `core.autocrlf` would rewrite CRLF on staging.
    fx.write_bytes("bom.md", b"\xEF\xBB\xBF# Title\n");
    fx.write_bytes("image.png", b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR");
    fx.stage("bom.md");
    fx.stage("image.png");
    fx.commit("Add files");
    let repo = fx.repo();
    assert_eq!(
        repo.read_at(&fx.path("bom.md"), &Revision::Head)
            .unwrap()
            .as_deref(),
        Some("# Title\n")
    );
    assert_eq!(
        repo.read_at(&fx.path("image.png"), &Revision::Head)
            .unwrap(),
        None
    );
    assert_eq!(
        repo.read_at(&fx.path("image.png"), &Revision::Workdir)
            .unwrap(),
        None
    );
}

#[test]
fn empty_repository_has_no_head_and_no_history() {
    let fx = Fixture::new();
    fx.write("first.md", "Draft\n");
    let repo = fx.repo();
    let info = repo.info();
    assert!(info.head.is_none());
    assert!(!info.detached);
    assert!(
        info.branch.is_some(),
        "unborn branch name is still reported"
    );
    assert!(
        repo.file_history(&fx.path("first.md"), 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repo.read_at(&fx.path("first.md"), &Revision::Head).unwrap(),
        None
    );
    assert_eq!(
        repo.file_status(&fx.path("first.md")).unwrap(),
        Some(StatusKind::Untracked)
    );
}

#[test]
fn info_reports_branch_and_head() {
    let fx = populated();
    let repo = fx.repo();
    let info = repo.info();
    assert!(!info.detached);
    assert!(info.branch.is_some());
    let head = info.head.unwrap();
    assert_eq!(head.id, fx.commits[3]);
    assert_eq!(head.summary, "Expand naming rules");
    assert_eq!(info.upstream, None);
    assert_eq!(info.operation, None);

    // Detach HEAD at the first commit.
    let first = git2::Oid::from_str(&fx.commits[0]).unwrap();
    fx.git.set_head_detached(first).unwrap();
    let info = repo.info();
    assert!(info.detached);
    assert_eq!(info.branch, None);
    assert_eq!(info.head.unwrap().id, fx.commits[0]);
}

#[test]
fn paths_outside_the_workdir_are_rejected() {
    let fx = populated();
    let repo = fx.repo();
    let other = tempfile::tempdir().unwrap();
    assert!(matches!(
        repo.file_status(&other.path().join("x.md")),
        Err(vcs::Error::OutsideRepo { .. })
    ));
    assert!(matches!(
        repo.relative(&fx.path("docs").join("..").join("..").join("escape.md")),
        Err(vcs::Error::OutsideRepo { .. })
    ));
}

#[test]
fn relative_paths_use_forward_slashes() {
    let fx = populated();
    let repo = fx.repo();
    assert_eq!(
        repo.relative(&fx.path("docs/guide.md")).unwrap(),
        "docs/guide.md"
    );
    assert_eq!(
        repo.relative(&fx.path("docs/missing/deep.md")).unwrap(),
        "docs/missing/deep.md"
    );
    assert_eq!(repo.absolute("docs/guide.md"), fx.path("docs/guide.md"));
}

#[cfg(windows)]
#[test]
fn windows_paths_ignore_case_and_separators() {
    let fx = populated();
    let repo = fx.repo();
    let upper = PathBuf::from(fx.path("docs/guide.md").to_string_lossy().to_uppercase());
    assert_eq!(repo.relative(&upper).unwrap(), "docs/guide.md");
    let slashes = PathBuf::from(
        fx.path("docs/guide.md")
            .to_string_lossy()
            .replace('\\', "/"),
    );
    assert_eq!(repo.relative(&slashes).unwrap(), "docs/guide.md");
    assert_eq!(
        repo.file_status(&upper).unwrap(),
        Some(StatusKind::Modified)
    );
}

#[test]
#[ignore = "timing check; run with --ignored --release"]
fn status_on_ten_thousand_files_is_fast() {
    let mut fx = Fixture::new();
    for i in 0..10_000 {
        fx.write(&format!("d{}/f{i}.md", i % 100), &format!("file {i}\n"));
    }
    let mut index = fx.git.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    fx.commit("many files");
    for i in (0..10_000).step_by(1000) {
        fx.write(&format!("d{}/f{i}.md", i % 100), "changed\n");
    }
    for i in 0..200 {
        fx.write(&format!("new/n{i}.md"), "new\n");
    }
    let repo = fx.repo();
    let _ = repo.status(None).unwrap(); // warm the OS cache
    let start = std::time::Instant::now();
    let status = repo.status(None).unwrap();
    let elapsed = start.elapsed();
    eprintln!("status on 10,000 tracked + 200 untracked files: {elapsed:?}");
    assert_eq!(status.len(), 10 + 200);
    assert!(elapsed.as_millis() < 1000, "{elapsed:?}");
}
