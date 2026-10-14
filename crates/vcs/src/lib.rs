//! Read-only Git access and human-readable Markdown diffs for MarkDraft.
//!
//! - [`repo`] opens the repository containing a document and answers
//!   "what changed", "what did this file look like before" and "who changed
//!   it when". It never writes to the repository.

pub mod repo;

pub use repo::{CommitInfo, Error, FileStatus, Repo, RepoInfo, Revision, StatusKind};
