//! Read-only Git access and human-readable Markdown diffs for MarkDraft.
//!
//! - [`repo`] opens the repository containing a document and answers
//!   "what changed", "what did this file look like before" and "who changed
//!   it when". It never writes to the repository.
//! - [`docdiff`] compares two versions of a Markdown document block by
//!   block, like tracked changes in a word processor, and also produces a
//!   classic line diff for a source view. It has no Git dependency.

pub mod docdiff;
pub mod repo;

pub use docdiff::{DocDiff, diff_lines, diff_markdown};
pub use repo::{CommitInfo, Error, FileStatus, Repo, RepoInfo, Revision, StatusKind};
