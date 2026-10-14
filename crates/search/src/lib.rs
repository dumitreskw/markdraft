//! Fuzzy file finding for quick open; full-text search arrives in v1.1.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use serde::Serialize;

/// One quick-open result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMatch {
    /// Index into the candidate list passed to [`rank`].
    pub index: usize,
    pub score: u32,
    /// Character positions in the candidate that matched, for highlighting.
    pub indices: Vec<u32>,
}

/// Ranks `candidates` (relative paths or titles) against `query`, best first.
/// An empty query returns no matches; the UI shows recent files instead.
pub fn rank<S: AsRef<str>>(query: &str, candidates: &[S], limit: usize) -> Vec<FileMatch> {
    if query.trim().is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buf = Vec::new();
    let mut indices = Vec::new();

    let mut matches: Vec<FileMatch> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, c)| {
            indices.clear();
            let haystack = Utf32Str::new(c.as_ref(), &mut buf);
            let score = pattern.indices(haystack, &mut matcher, &mut indices)?;
            indices.sort_unstable();
            indices.dedup();
            Some(FileMatch {
                index,
                score,
                indices: indices.clone(),
            })
        })
        .collect();

    // Higher score first; ties keep the shorter candidate, then input order.
    matches.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| {
                candidates[a.index]
                    .as_ref()
                    .len()
                    .cmp(&candidates[b.index].as_ref().len())
            })
            .then_with(|| a.index.cmp(&b.index))
    });
    matches.truncate(limit);
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILES: &[&str] = &[
        "docs/guide/installation.md",
        "README.md",
        "docs/reference/api.md",
        "notes/meeting-2026-10-01.md",
    ];

    #[test]
    fn best_match_first() {
        let m = rank("readme", FILES, 10);
        assert_eq!(FILES[m[0].index], "README.md");
    }

    #[test]
    fn path_aware_abbreviation() {
        let m = rank("docapi", FILES, 10);
        assert_eq!(FILES[m[0].index], "docs/reference/api.md");
    }

    #[test]
    fn indices_point_at_matched_chars() {
        let m = rank("api", FILES, 1);
        let s: Vec<char> = FILES[m[0].index].chars().collect();
        let matched: String = m[0].indices.iter().map(|&i| s[i as usize]).collect();
        assert_eq!(matched.to_lowercase(), "api");
    }

    #[test]
    fn empty_query_and_limit() {
        assert!(rank("", FILES, 10).is_empty());
        assert!(rank("md", FILES, 2).len() <= 2);
        assert!(rank("zzzz", FILES, 10).is_empty());
    }
}
