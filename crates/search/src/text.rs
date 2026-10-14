//! Full-text search across documents, in parallel.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

/// Longest line excerpt returned per match, in characters.
const MAX_LINE_CHARS: usize = 240;
/// Matches kept per file; the rest are only counted.
const MAX_MATCHES_PER_FILE: usize = 50;
/// Files larger than this are skipped.
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextQuery {
    pub pattern: String,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub whole_word: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("invalid search pattern: {0}")]
    Pattern(#[from] regex::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineMatch {
    /// 1-based line number.
    pub line: u32,
    /// The line, trimmed around the first match when long.
    pub text: String,
    /// Matched ranges in `text`, as UTF-16 code unit offsets (for the UI).
    pub ranges: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMatches {
    pub path: PathBuf,
    pub matches: Vec<LineMatch>,
    /// All matching lines, including those not returned.
    pub total: usize,
}

pub fn compile(query: &TextQuery) -> Result<Regex, SearchError> {
    let mut pattern = if query.regex {
        query.pattern.clone()
    } else {
        regex::escape(&query.pattern)
    };
    if query.whole_word {
        pattern = format!(r"\b(?:{pattern})\b");
    }
    Ok(RegexBuilder::new(&pattern)
        .case_insensitive(!query.case_sensitive)
        .multi_line(true)
        .size_limit(1 << 22)
        .build()?)
}

/// Searches `files` and returns those with matches, in the input order.
/// Stops collecting after about `max_files` files have matched.
pub fn search_files(
    files: &[PathBuf],
    query: &TextQuery,
    max_files: usize,
) -> Result<Vec<FileMatches>, SearchError> {
    if query.pattern.is_empty() {
        return Ok(Vec::new());
    }
    let re = compile(query)?;
    let found = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8);
    let chunk = files.len().div_ceil(threads).max(1);

    let mut results: Vec<(usize, FileMatches)> = std::thread::scope(|s| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .enumerate()
            .map(|(ci, part)| {
                let re = &re;
                let found = &found;
                s.spawn(move || {
                    let mut out = Vec::new();
                    for (i, path) in part.iter().enumerate() {
                        if found.load(Ordering::Relaxed) >= max_files {
                            break;
                        }
                        if let Some(m) = search_file(path, re) {
                            found.fetch_add(1, Ordering::Relaxed);
                            out.push((ci * chunk + i, m));
                        }
                    }
                    out
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });
    results.sort_by_key(|(i, _)| *i);
    results.truncate(max_files);
    Ok(results.into_iter().map(|(_, m)| m).collect())
}

fn search_file(path: &PathBuf, re: &Regex) -> Option<FileMatches> {
    if std::fs::metadata(path).ok()?.len() > MAX_FILE_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let mut matches = Vec::new();
    let mut total = 0;
    for (n, line) in text.lines().enumerate() {
        let spans: Vec<(usize, usize)> = re
            .find_iter(line)
            .filter(|m| !m.is_empty())
            .map(|m| (m.start(), m.end()))
            .collect();
        if spans.is_empty() {
            continue;
        }
        total += 1;
        if matches.len() < MAX_MATCHES_PER_FILE {
            matches.push(excerpt(
                u32::try_from(n + 1).unwrap_or(u32::MAX),
                line,
                &spans,
            ));
        }
    }
    (total > 0).then(|| FileMatches {
        path: path.clone(),
        matches,
        total,
    })
}

/// Cuts a long line around its first match and converts byte spans to
/// UTF-16 offsets within the excerpt.
fn excerpt(line_no: u32, line: &str, spans: &[(usize, usize)]) -> LineMatch {
    let first = spans[0].0;
    let mut start = 0;
    if line.len() > MAX_LINE_CHARS * 2 || line[..first].chars().count() > MAX_LINE_CHARS / 2 {
        // Start a little before the first match, on a char boundary.
        start = line[..first]
            .char_indices()
            .rev()
            .nth(40)
            .map_or(0, |(i, _)| i);
    }
    let rest = &line[start..];
    let end = rest
        .char_indices()
        .nth(MAX_LINE_CHARS)
        .map_or(line.len(), |(i, _)| start + i);
    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if end < line.len() { "…" } else { "" };
    let body = &line[start..end];
    let text = format!("{prefix}{}{suffix}", body.trim_end());
    let utf16 = |byte: usize| -> u32 {
        let clamped = byte.clamp(start, end);
        let units = prefix.encode_utf16().count() + line[start..clamped].encode_utf16().count();
        u32::try_from(units).unwrap_or(u32::MAX)
    };
    let ranges = spans
        .iter()
        .filter(|(s, e)| *e > start && *s < end)
        .map(|&(s, e)| (utf16(s), utf16(e)))
        .collect();
    LineMatch {
        line: line_no,
        text,
        ranges,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(pattern: &str) -> TextQuery {
        TextQuery {
            pattern: pattern.into(),
            regex: false,
            case_sensitive: false,
            whole_word: false,
        }
    }

    fn files(dir: &std::path::Path, items: &[(&str, &str)]) -> Vec<PathBuf> {
        items
            .iter()
            .map(|(name, text)| {
                let p = dir.join(name);
                std::fs::write(&p, text).unwrap();
                p
            })
            .collect()
    }

    #[test]
    fn finds_lines_with_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let list = files(
            dir.path(),
            &[
                ("a.md", "# Title\nThe Roadmap and the roadmap\n"),
                ("b.md", "nothing"),
            ],
        );
        let res = search_files(&list, &q("roadmap"), 10).unwrap();
        assert_eq!(res.len(), 1);
        let m = &res[0].matches[0];
        assert_eq!(m.line, 2);
        assert_eq!(m.ranges, vec![(4, 11), (20, 27)]);
    }

    #[test]
    fn literal_by_default_regex_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let list = files(
            dir.path(),
            &[("a.md", "cost is $5 (approx)\nversion 1.2.3\n")],
        );
        assert_eq!(
            search_files(&list, &q("$5 (approx)"), 10).unwrap()[0].total,
            1
        );
        let mut rq = q(r"\d+\.\d+\.\d+");
        rq.regex = true;
        assert_eq!(search_files(&list, &rq, 10).unwrap()[0].matches[0].line, 2);
        rq.pattern = "(".into();
        assert!(search_files(&list, &rq, 10).is_err());
    }

    #[test]
    fn case_and_whole_word() {
        let dir = tempfile::tempdir().unwrap();
        let list = files(dir.path(), &[("a.md", "Note notes NOTE\n")]);
        let mut query = q("note");
        query.whole_word = true;
        assert_eq!(
            search_files(&list, &query, 10).unwrap()[0].matches[0]
                .ranges
                .len(),
            2
        );
        query.case_sensitive = true;
        assert!(search_files(&list, &query, 10).unwrap().is_empty());
    }

    #[test]
    fn long_lines_are_trimmed_with_utf16_ranges() {
        let line = format!("{}é target", "x".repeat(1000));
        let m = excerpt(1, &line, &[(line.find("target").unwrap(), line.len())]);
        assert!(m.text.starts_with('…'));
        let (s, e) = m.ranges[0];
        let units: Vec<u16> = m.text.encode_utf16().collect();
        assert_eq!(
            String::from_utf16(&units[s as usize..e as usize]).unwrap(),
            "target"
        );
    }

    #[test]
    fn max_files_limits_results() {
        let dir = tempfile::tempdir().unwrap();
        let items: Vec<(String, &str)> = (0..20).map(|i| (format!("{i}.md"), "hit")).collect();
        let refs: Vec<(&str, &str)> = items.iter().map(|(a, b)| (a.as_str(), *b)).collect();
        let list = files(dir.path(), &refs);
        assert!(search_files(&list, &q("hit"), 5).unwrap().len() <= 5);
    }
}
