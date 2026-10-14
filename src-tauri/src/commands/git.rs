//! Git integration: status, file history and readable diffs. Read-only.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use md_core::RenderContext;
use serde::{Deserialize, Serialize};
use tauri::State;
use vcs::docdiff::{BlockChange, BlockType, CollapsedEntry, Hunk, Segment, SegmentKind};
use vcs::{CommitInfo, DocDiff, FileStatus, Repo, RepoInfo, Revision};

use crate::error::{CmdError, blocking};
use crate::protocol::asset_url;
use crate::state::AppState;

/// Unchanged blocks shown around each change before folding the rest.
const CONTEXT_BLOCKS: usize = 2;
/// Context lines around each hunk in the source view.
const CONTEXT_LINES: usize = 3;

impl From<vcs::Error> for CmdError {
    fn from(e: vcs::Error) -> Self {
        match e {
            vcs::Error::UnknownRevision(_) | vcs::Error::OutsideRepo { .. } => {
                Self::Invalid(e.to_string())
            }
            _ => Self::Io(e.to_string()),
        }
    }
}

fn open_repo(path: &Path) -> Result<Repo, CmdError> {
    Repo::discover(path)?
        .ok_or_else(|| CmdError::NotFound(format!("{} is not in a Git repository", path.display())))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRoot {
    /// The open folder this applies to.
    pub root: PathBuf,
    pub info: RepoInfo,
}

/// Repository details for each open folder that is inside a Git repository.
#[tauri::command]
pub async fn git_info(state: State<'_, AppState>) -> Result<Vec<GitRoot>, CmdError> {
    let roots: Vec<PathBuf> = state
        .roots_read()
        .list
        .iter()
        .map(|r| r.path.clone())
        .collect();
    blocking(move || {
        Ok(roots
            .into_iter()
            .filter_map(|root| {
                let repo = Repo::discover(&root).ok()??;
                Some(GitRoot {
                    info: repo.info(),
                    root,
                })
            })
            .collect())
    })
    .await
}

/// Changed files under an open folder (all files, not only documents; the
/// UI filters).
#[tauri::command]
pub async fn git_status(
    state: State<'_, AppState>,
    root: PathBuf,
) -> Result<Vec<FileStatus>, CmdError> {
    let root = state.allowed_path(&root)?;
    blocking(move || {
        let Some(repo) = Repo::discover(&root)? else {
            return Ok(Vec::new());
        };
        Ok(repo.status(Some(&root))?)
    })
    .await
}

#[tauri::command]
pub async fn git_history(
    state: State<'_, AppState>,
    path: PathBuf,
    limit: u32,
) -> Result<Vec<CommitInfo>, CmdError> {
    let path = state.allowed_path(&path)?;
    blocking(move || Ok(open_repo(&path)?.file_history(&path, limit as usize)?)).await
}

/// A version of a document, as chosen in the UI.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RevArg {
    Head,
    Index,
    /// The file on disk.
    Workdir,
    Commit {
        id: String,
    },
    ParentOf {
        id: String,
    },
    /// Text supplied by the UI (for example unsaved editor contents).
    Text {
        text: String,
    },
}

impl RevArg {
    fn read(&self, repo: Option<&Repo>, path: &Path) -> Result<Option<String>, CmdError> {
        let rev = match self {
            RevArg::Text { text } => return Ok(Some(text.clone())),
            RevArg::Workdir if repo.is_none() => {
                return Ok(fs_index::read_document(path).ok().map(|d| d.text));
            }
            RevArg::Head => Revision::Head,
            RevArg::Index => Revision::Index,
            RevArg::Workdir => Revision::Workdir,
            RevArg::Commit { id } => Revision::Commit(id.clone()),
            RevArg::ParentOf { id } => Revision::ParentOf(id.clone()),
        };
        let repo = repo.ok_or_else(|| {
            CmdError::NotFound(format!("{} is not in a Git repository", path.display()))
        })?;
        Ok(repo.read_at(path, &rev)?)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffView {
    pub diff: DocDiff,
    /// Which blocks to show, with folded runs of unchanged blocks.
    pub collapsed: Vec<CollapsedEntry>,
    /// Rendered HTML per block for unchanged, added, removed and moved
    /// blocks (`None` for modified blocks, which the UI draws from segments).
    pub blocks_html: Vec<Option<String>>,
    /// Inline HTML for edited prose blocks: the text rendered as Markdown
    /// with `<ins>`/`<del>` marks. `None` for other blocks; code and tables
    /// are drawn from segments by the UI.
    pub modified_html: Vec<Option<String>>,
    /// Classic line hunks for the source view.
    pub hunks: Vec<Hunk>,
    pub old_exists: bool,
    pub new_exists: bool,
}

/// Compares two versions of a document.
#[tauri::command]
pub async fn diff_document(
    state: State<'_, AppState>,
    path: PathBuf,
    old: RevArg,
    new: RevArg,
) -> Result<DiffView, CmdError> {
    let path = state.allowed_path(&path)?;
    blocking(move || {
        let repo = Repo::discover(&path)?;
        let old_text = old.read(repo.as_ref(), &path)?;
        let new_text = new.read(repo.as_ref(), &path)?;
        Ok(build_view(&path, old_text, new_text))
    })
    .await
}

fn build_view(path: &Path, old_text: Option<String>, new_text: Option<String>) -> DiffView {
    let old_exists = old_text.is_some();
    let new_exists = new_text.is_some();
    let old_text = old_text.unwrap_or_default();
    let new_text = new_text.unwrap_or_default();
    let diff = vcs::diff_markdown(&old_text, &new_text);
    let hunks = vcs::diff_lines(&old_text, &new_text, CONTEXT_LINES);
    let ctx = RenderContext {
        base_dir: path.parent().map(Path::to_path_buf),
        asset_url: Some(Arc::new(asset_url)),
    };
    let blocks_html = diff
        .blocks
        .iter()
        .map(|b| {
            let text = match b.kind {
                BlockChange::Modified => return None,
                BlockChange::Removed | BlockChange::MovedAway => b.old_text.as_deref(),
                _ => b.new_text.as_deref().or(b.old_text.as_deref()),
            }?;
            Some(md_core::render(text, &ctx).html)
        })
        .collect();
    let modified_html = diff
        .blocks
        .iter()
        .map(|b| {
            let prose = matches!(
                b.block_type,
                BlockType::Paragraph
                    | BlockType::Heading { .. }
                    | BlockType::ListItem { .. }
                    | BlockType::Quote
            );
            (b.kind == BlockChange::Modified && prose && b.table.is_none())
                .then(|| render_segments(&b.segments, &ctx))
        })
        .collect();
    DiffView {
        collapsed: diff.collapsed(CONTEXT_BLOCKS),
        diff,
        blocks_html,
        modified_html,
        hunks,
        old_exists,
        new_exists,
    }
}

// Private-use characters that survive Markdown rendering as plain text.
const INS_OPEN: char = '\u{E000}';
const INS_CLOSE: char = '\u{E001}';
const DEL_OPEN: char = '\u{E002}';
const DEL_CLOSE: char = '\u{E003}';
const MARKERS: [char; 4] = [INS_OPEN, INS_CLOSE, DEL_OPEN, DEL_CLOSE];

/// Renders word segments as inline Markdown, keeping insert/delete marks.
fn render_segments(segments: &[Segment], ctx: &RenderContext) -> String {
    let mut md = String::new();
    for seg in segments {
        let text = seg.text.replace(MARKERS, "");
        let (open, close) = match seg.kind {
            SegmentKind::Equal => {
                md.push_str(&text);
                continue;
            }
            SegmentKind::Insert => (INS_OPEN, INS_CLOSE),
            SegmentKind::Delete => (DEL_OPEN, DEL_CLOSE),
        };
        md.push(open);
        md.push_str(&text);
        md.push(close);
    }
    let html = md_core::render(&md, ctx).html;
    // A single paragraph: keep only its contents (the UI supplies the block).
    let trimmed = html.trim();
    let inner = trimmed
        .strip_prefix("<p")
        .and_then(|rest| rest.split_once('>'))
        .and_then(|(_, rest)| rest.strip_suffix("</p>"))
        .filter(|inner| !inner.contains("<p"))
        .unwrap_or(trimmed);
    mark_up(inner)
}

/// Turns the marker characters into tags, but only in text: markers that
/// ended up inside a tag (an edited link URL, say) are dropped.
fn mark_up(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + 32);
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(c);
            }
            '>' => {
                in_tag = false;
                out.push(c);
            }
            _ if in_tag && MARKERS.contains(&c) => {}
            INS_OPEN => out.push_str("<ins>"),
            INS_CLOSE => out.push_str("</ins>"),
            DEL_OPEN => out.push_str("<del>"),
            DEL_CLOSE => out.push_str("</del>"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modified_prose_keeps_formatting_and_marks() {
        let view = build_view(
            Path::new("/d/a.md"),
            Some("Welcome to the **handbook**. See [docs](a.md).\n".into()),
            Some("Welcome to the team **handbook**. See [docs](b.md).\n".into()),
        );
        let html = view
            .modified_html
            .iter()
            .flatten()
            .next()
            .expect("one modified block");
        assert!(html.contains("<ins>team"), "{html}");
        assert!(html.contains("<strong"), "{html}");
        assert!(!html.chars().any(|c| MARKERS.contains(&c)), "{html}");
    }

    #[test]
    fn markers_inside_tags_are_dropped() {
        let input = format!("<a href=\"x{INS_OPEN}y{INS_CLOSE}\">{DEL_OPEN}t{DEL_CLOSE}</a>");
        assert_eq!(mark_up(&input), "<a href=\"xy\"><del>t</del></a>");
    }

    #[test]
    fn view_renders_unmodified_blocks_only() {
        let view = build_view(
            Path::new("/d/a.md"),
            Some("# T\n\nOld words here.\n".into()),
            Some("# T\n\nNew words here.\n\nAdded *para*.\n".into()),
        );
        assert_eq!(view.diff.blocks.len(), view.blocks_html.len());
        for (b, html) in view.diff.blocks.iter().zip(&view.blocks_html) {
            assert_eq!(b.kind == BlockChange::Modified, html.is_none());
        }
        assert!(
            view.blocks_html
                .iter()
                .flatten()
                .any(|h| h.contains("para</em>"))
        );
        assert!(view.old_exists && view.new_exists);
    }
}
