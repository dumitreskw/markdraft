//! Classic unified line diff for the "Source" view.

use std::time::Instant;

use serde::Serialize;
use similar::{Algorithm, DiffOp, capture_diff_slices_deadline, group_diff_ops};

use super::blocks::normalize;
use super::words::word_segments;
use super::{DEADLINE, Segment, SegmentKind};

/// A run of changed lines with surrounding context, like a `@@` hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    /// First old line in the hunk (1-based; for an empty range, the line
    /// before it, as in unified diffs).
    pub old_start: u32,
    /// Number of old lines in the hunk.
    pub old_len: u32,
    /// First new line in the hunk (1-based, as `old_start`).
    pub new_start: u32,
    /// Number of new lines in the hunk.
    pub new_len: u32,
    /// The lines: context, then deletions before insertions per change.
    pub lines: Vec<DiffLine>,
}

/// One line of a hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    /// Context, insertion or deletion.
    pub kind: LineKind,
    /// 1-based line number in the old text (`None` for insertions).
    pub old_no: Option<u32>,
    /// 1-based line number in the new text (`None` for deletions).
    pub new_no: Option<u32>,
    /// The line without its line ending.
    pub text: String,
    /// For a changed line paired with its counterpart: the line split into
    /// equal and deleted (on `delete` lines) or inserted (on `insert`
    /// lines) words, for intra-line highlighting. Empty otherwise.
    pub segments: Vec<Segment>,
}

/// The role of a line in a hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    /// Unchanged line shown for context.
    Context,
    /// Line only in the new text.
    Insert,
    /// Line only in the old text.
    Delete,
}

/// Unified line diff of two texts with `context` lines around each change.
/// A BOM and CRLF/CR line endings are normalised first; otherwise lines
/// compare exactly (whitespace changes are shown). Identical texts give no
/// hunks.
pub fn diff_lines(old: &str, new: &str, context: usize) -> Vec<Hunk> {
    let old = normalize(old);
    let new = normalize(new);
    if old == new {
        return Vec::new();
    }
    let deadline = Some(Instant::now() + DEADLINE);
    let old_l = split_lines(&old);
    let new_l = split_lines(&new);
    let ops = capture_diff_slices_deadline(Algorithm::Histogram, &old_l, &new_l, deadline);

    let mut hunks = Vec::new();
    for group in group_diff_ops(ops, context) {
        let (Some(first), Some(last)) = (group.first(), group.last()) else {
            continue;
        };
        let old_range = first.old_range().start..last.old_range().end;
        let new_range = first.new_range().start..last.new_range().end;
        let mut lines = Vec::new();
        for op in &group {
            let (o, n) = (op.old_range(), op.new_range());
            match op {
                DiffOp::Equal { .. } => {
                    for (i, j) in o.zip(n) {
                        lines.push(line(LineKind::Context, Some(i), Some(j), old_l[i]));
                    }
                }
                _ => {
                    // Pair deleted and inserted lines in order for
                    // intra-line highlighting.
                    let pairs = o.len().min(n.len());
                    let mut ins = Vec::new();
                    for k in 0..o.len().max(n.len()) {
                        let (oi, ni) = (o.start + k, n.start + k);
                        let segs = (k < pairs)
                            .then(|| intra_line(old_l[oi], new_l[ni], deadline))
                            .flatten();
                        if k < o.len() {
                            let mut l = line(LineKind::Delete, Some(oi), None, old_l[oi]);
                            if let Some((del, _)) = &segs {
                                l.segments = del.clone();
                            }
                            lines.push(l);
                        }
                        if k < n.len() {
                            let mut l = line(LineKind::Insert, None, Some(ni), new_l[ni]);
                            if let Some((_, add)) = segs {
                                l.segments = add;
                            }
                            ins.push(l);
                        }
                    }
                    lines.extend(ins);
                }
            }
        }
        hunks.push(Hunk {
            old_start: start_no(&old_range),
            old_len: to_u32(old_range.len()),
            new_start: start_no(&new_range),
            new_len: to_u32(new_range.len()),
            lines,
        });
    }
    hunks
}

fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut v: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') {
        v.pop();
    }
    v
}

fn line(kind: LineKind, old: Option<usize>, new: Option<usize>, text: &str) -> DiffLine {
    DiffLine {
        kind,
        old_no: old.map(|i| to_u32(i + 1)),
        new_no: new.map(|i| to_u32(i + 1)),
        text: text.to_owned(),
        segments: Vec::new(),
    }
}

/// Splits a changed line pair into (old-side, new-side) segments, or `None`
/// when the lines share no words (highlighting everything adds nothing).
fn intra_line(
    old: &str,
    new: &str,
    deadline: Option<Instant>,
) -> Option<(Vec<Segment>, Vec<Segment>)> {
    let segs = word_segments(old, new, false, deadline);
    let shares_words = segs
        .iter()
        .any(|s| s.kind == SegmentKind::Equal && s.text.chars().any(char::is_alphanumeric));
    if !shares_words {
        return None;
    }
    let side = |drop: SegmentKind| {
        segs.iter()
            .filter(|s| s.kind != drop)
            .cloned()
            .collect::<Vec<_>>()
    };
    Some((side(SegmentKind::Insert), side(SegmentKind::Delete)))
}

fn start_no(r: &std::ops::Range<usize>) -> u32 {
    // Unified-diff convention: an empty range names the line before it.
    if r.is_empty() {
        to_u32(r.start)
    } else {
        to_u32(r.start + 1)
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_has_no_hunks() {
        assert!(diff_lines("a\nb\n", "a\r\nb\r\n", 3).is_empty());
    }

    #[test]
    fn hunk_with_context_and_word_segments() {
        let old = "1\n2\n3\nThe quick fox.\n5\n6\n7\n8\n";
        let new = "1\n2\n3\nThe fast fox.\n5\n6\n7\n8\nnine\n";
        let hunks = diff_lines(old, new, 1);
        assert_eq!(hunks.len(), 2);
        let h = &hunks[0];
        assert_eq!(
            (h.old_start, h.old_len, h.new_start, h.new_len),
            (3, 3, 3, 3)
        );
        let kinds: Vec<_> = h.lines.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            [
                LineKind::Context,
                LineKind::Delete,
                LineKind::Insert,
                LineKind::Context
            ]
        );
        assert_eq!(h.lines[1].old_no, Some(4));
        assert_eq!(h.lines[2].new_no, Some(4));
        let del: Vec<_> = h.lines[1]
            .segments
            .iter()
            .map(|s| (s.kind, s.text.as_str()))
            .collect();
        assert_eq!(
            del,
            [
                (SegmentKind::Equal, "The "),
                (SegmentKind::Delete, "quick"),
                (SegmentKind::Equal, " fox.")
            ]
        );
        let h = &hunks[1];
        assert_eq!(
            (h.old_start, h.old_len, h.new_start, h.new_len),
            (8, 1, 8, 2)
        );
        assert_eq!(h.lines[1].kind, LineKind::Insert);
        assert_eq!(h.lines[1].text, "nine");
        assert!(h.lines[1].segments.is_empty());
    }

    #[test]
    fn new_and_deleted_files() {
        let added = diff_lines("", "a\nb\n", 3);
        assert_eq!(added.len(), 1);
        assert_eq!(
            (
                added[0].old_start,
                added[0].old_len,
                added[0].new_start,
                added[0].new_len
            ),
            (0, 0, 1, 2)
        );
        let removed = diff_lines("a\nb\n", "", 3);
        assert_eq!(
            (
                removed[0].old_start,
                removed[0].old_len,
                removed[0].new_start,
                removed[0].new_len
            ),
            (1, 2, 0, 0)
        );
        assert!(removed[0].lines.iter().all(|l| l.kind == LineKind::Delete));
    }
}
