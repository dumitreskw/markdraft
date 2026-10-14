//! Which files and folders count as documents.

use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::Error;

/// Directory names never descended into, regardless of ignore files.
pub const ALWAYS_SKIPPED_DIRS: &[&str] = &[".git", "node_modules", "target"];

/// Project-specific ignore file, read alongside `.gitignore` and `.ignore`.
pub const MARKDRAFT_IGNORE_FILE: &str = ".markdraftignore";

/// File patterns treated as Markdown when the workspace sets none.
pub const DEFAULT_INCLUDE: &[&str] = &["**/*.md", "**/*.markdown"];

/// Which files count as documents in a root.
#[derive(Debug, Clone)]
pub struct DiscoveryRules {
    include: GlobSet,
    exclude: GlobSet,
    pub respect_gitignore: bool,
}

impl DiscoveryRules {
    pub fn new<S: AsRef<str>>(
        include: &[S],
        exclude: &[S],
        respect_gitignore: bool,
    ) -> Result<Self, Error> {
        Ok(Self {
            include: build_set(include)?,
            exclude: build_set(exclude)?,
            respect_gitignore,
        })
    }

    /// Whether a path relative to its root is a document to show in the tree.
    /// Matching is case-insensitive so `README.MD` is found on every OS.
    pub fn is_document(&self, relative: &Path) -> bool {
        self.include.is_match(relative) && !self.exclude.is_match(relative)
    }

    /// Whether a directory name is skipped outright.
    pub fn is_skipped_dir(name: &str) -> bool {
        ALWAYS_SKIPPED_DIRS.contains(&name)
    }
}

impl Default for DiscoveryRules {
    fn default() -> Self {
        Self::new::<&str>(DEFAULT_INCLUDE, &[], true).expect("default globs are valid")
    }
}

fn build_set<S: AsRef<str>>(patterns: &[S]) -> Result<GlobSet, Error> {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        let pattern = p.as_ref();
        let glob = globset::GlobBuilder::new(pattern)
            .case_insensitive(true)
            .literal_separator(true)
            .build()
            .map_err(|source| Error::Glob {
                pattern: pattern.to_owned(),
                source,
            })?;
        builder.add(glob);
    }
    builder.build().map_err(|source| Error::Glob {
        pattern: String::new(),
        source,
    })
}

/// Validates a single user-supplied glob pattern.
pub fn validate_pattern(pattern: &str) -> Result<(), Error> {
    Glob::new(pattern)
        .map(|_| ())
        .map_err(|source| Error::Glob {
            pattern: pattern.to_owned(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_rules_match_markdown_anywhere() {
        let rules = DiscoveryRules::default();
        assert!(rules.is_document(Path::new("README.md")));
        assert!(rules.is_document(Path::new("docs/guide/intro.markdown")));
        assert!(rules.is_document(Path::new("NOTES.MD")));
        assert!(!rules.is_document(Path::new("src/main.rs")));
        assert!(!rules.is_document(Path::new("readme.md.bak")));
    }

    #[test]
    fn exclude_wins_over_include() {
        let rules = DiscoveryRules::new(&["**/*.md"], &["archive/**"], true).unwrap();
        assert!(rules.is_document(Path::new("notes/a.md")));
        assert!(!rules.is_document(Path::new("archive/old.md")));
    }

    #[test]
    fn skipped_dirs() {
        assert!(DiscoveryRules::is_skipped_dir("node_modules"));
        assert!(!DiscoveryRules::is_skipped_dir("docs"));
    }

    #[test]
    fn invalid_pattern_is_reported() {
        let err = validate_pattern("docs/[").unwrap_err();
        assert!(err.to_string().contains("docs/["));
    }
}
