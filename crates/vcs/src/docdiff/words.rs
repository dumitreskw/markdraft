//! Word- and line-level diffs inside a block, tuned to read like tracked
//! changes rather than character soup.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::time::Instant;

use similar::{Algorithm, DiffOp, capture_diff_slices_deadline};
use unicode_segmentation::UnicodeSegmentation;

use super::{Segment, SegmentKind};

/// Splits text into Unicode word-boundary tokens: words, runs of
/// whitespace and single punctuation marks. Grapheme clusters (emoji, CJK
/// characters with combining marks) are never split; each CJK ideograph is
/// its own token.
pub(crate) fn tokens(text: &str) -> Vec<&str> {
    text.split_word_bounds().collect()
}

/// Whether a token counts as a word for statistics.
pub(crate) fn is_word(token: &str) -> bool {
    token.chars().any(char::is_alphanumeric)
}

pub(crate) fn count_words(text: &str) -> usize {
    text.split_word_bounds().filter(|t| is_word(t)).count()
}

fn is_space(token: &str) -> bool {
    token.chars().all(char::is_whitespace)
}

/// Word diff of two prose texts. With `ignore_whitespace`, runs of
/// whitespace (including line breaks) compare equal to each other, so
/// re-wrapped lines are not reported; equal segments then show the new
/// text.
pub(crate) fn word_segments(
    old: &str,
    new: &str,
    ignore_whitespace: bool,
    deadline: Option<Instant>,
) -> Vec<Segment> {
    let old_t = tokens(old);
    let new_t = tokens(new);
    fn key(t: &str, ignore_whitespace: bool) -> &str {
        if ignore_whitespace && is_space(t) {
            " "
        } else {
            t
        }
    }
    let old_k: Vec<&str> = old_t.iter().map(|t| key(t, ignore_whitespace)).collect();
    let new_k: Vec<&str> = new_t.iter().map(|t| key(t, ignore_whitespace)).collect();
    let ops = capture_diff_slices_deadline(Algorithm::Myers, &old_k, &new_k, deadline);
    let chunks = cleanup(chunks_from_ops(&ops), &old_t, &new_t);

    let mut out = Vec::new();
    for c in chunks {
        match c {
            Chunk::Equal(_, n) => push(&mut out, SegmentKind::Equal, &new_t[n]),
            Chunk::Change(o, n) => {
                push(&mut out, SegmentKind::Delete, &old_t[o]);
                push(&mut out, SegmentKind::Insert, &new_t[n]);
            }
        }
    }
    out
}

/// Line diff of two code (or other verbatim) texts; each segment holds
/// whole lines, ending in `\n` except possibly the last. Trailing
/// whitespace is ignored when matching lines.
pub(crate) fn line_segments(old: &str, new: &str, deadline: Option<Instant>) -> Vec<Segment> {
    let old_l: Vec<&str> = old.split_inclusive('\n').collect();
    let new_l: Vec<&str> = new.split_inclusive('\n').collect();
    let old_k: Vec<&str> = old_l.iter().map(|l| l.trim_end()).collect();
    let new_k: Vec<&str> = new_l.iter().map(|l| l.trim_end()).collect();
    let ops = capture_diff_slices_deadline(Algorithm::Histogram, &old_k, &new_k, deadline);
    let mut out = Vec::new();
    for c in chunks_from_ops(&ops) {
        match c {
            Chunk::Equal(_, n) => push_lines(&mut out, SegmentKind::Equal, &new_l[n]),
            Chunk::Change(o, n) => {
                push_lines(&mut out, SegmentKind::Delete, &old_l[o]);
                push_lines(&mut out, SegmentKind::Insert, &new_l[n]);
            }
        }
    }
    out
}

/// Pushes lines as one segment, making sure a segment that is followed by
/// another ends with a newline (the last line of a block may lack one).
fn push_lines(out: &mut Vec<Segment>, kind: SegmentKind, lines: &[&str]) {
    if lines.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut()
        && !last.text.ends_with('\n')
    {
        last.text.push('\n');
    }
    push(out, kind, lines);
}

fn push(out: &mut Vec<Segment>, kind: SegmentKind, parts: &[&str]) {
    if parts.is_empty() {
        return;
    }
    let text: String = parts.concat();
    if text.is_empty() {
        return;
    }
    match out.last_mut() {
        Some(last) if last.kind == kind => last.text.push_str(&text),
        _ => out.push(Segment { kind, text }),
    }
}

/// An equal run or a change (deletion and/or insertion) in token ranges.
#[derive(Debug, Clone)]
enum Chunk {
    Equal(Range<usize>, Range<usize>),
    Change(Range<usize>, Range<usize>),
}

fn chunks_from_ops(ops: &[DiffOp]) -> Vec<Chunk> {
    let mut out: Vec<Chunk> = Vec::new();
    for op in ops {
        let (o, n) = (op.old_range(), op.new_range());
        if o.is_empty() && n.is_empty() {
            continue;
        }
        let is_equal = matches!(op, DiffOp::Equal { .. });
        match (out.last_mut(), is_equal) {
            (Some(Chunk::Change(po, pn)), false) => {
                po.end = o.end;
                pn.end = n.end;
            }
            (Some(Chunk::Equal(po, pn)), true) => {
                po.end = o.end;
                pn.end = n.end;
            }
            _ if is_equal => out.push(Chunk::Equal(o, n)),
            _ => out.push(Chunk::Change(o, n)),
        }
    }
    out
}

/// Merges small equal islands between two changes into the changes, so
/// "quick brown" -> "slow red" reads as one replacement instead of
/// "~~quick~~slow ~~brown~~red". An island is absorbed when it is only
/// whitespace/punctuation, or when it is no longer than the changes on both
/// sides of it (the diff-match-patch "semantic cleanup" rule), measured in
/// characters.
fn cleanup(mut chunks: Vec<Chunk>, old_t: &[&str], new_t: &[&str]) -> Vec<Chunk> {
    let chars = |toks: &[&str]| toks.iter().map(|t| t.chars().count()).sum::<usize>();
    let change_len =
        |o: &Range<usize>, n: &Range<usize>| chars(&old_t[o.clone()]).max(chars(&new_t[n.clone()]));
    loop {
        let mut merged = false;
        let mut i = 1;
        while i + 1 < chunks.len() {
            let (Chunk::Change(lo, ln), Chunk::Equal(_, en), Chunk::Change(ro, rn)) =
                (&chunks[i - 1], &chunks[i], &chunks[i + 1])
            else {
                i += 1;
                continue;
            };
            let island = &new_t[en.clone()];
            let trivial = island.iter().all(|t| !is_word(t));
            let len = chars(island);
            let small = len <= change_len(lo, ln) && len <= change_len(ro, rn);
            if trivial || small {
                let joined = Chunk::Change(lo.start..ro.end, ln.start..rn.end);
                chunks.splice(i - 1..=i + 1, [joined]);
                merged = true;
            } else {
                i += 1;
            }
        }
        if !merged {
            return chunks;
        }
    }
}

/// Similarity in `0..=1` from the multiset of lower-cased words: cheap
/// (no alignment) and good enough to decide whether two blocks are versions
/// of each other.
pub(crate) fn bag_similarity(a: &[u64], b: &[u64]) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let (mut i, mut j, mut common) = (0, 0, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                common += 1;
                i += 1;
                j += 1;
            }
        }
    }
    2.0 * common as f32 / (a.len() + b.len()) as f32
}

/// Similarity in `0..=1` from shared character bigrams (case-insensitive).
/// Used for short blocks such as headings, where "Installing" and
/// "Installation" share no whole word but are clearly the same heading.
pub(crate) fn char_similarity(a: &str, b: &str) -> f32 {
    let grams = |s: &str| {
        let chars: Vec<char> = s
            .to_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let mut g: Vec<(char, char)> = chars.windows(2).map(|w| (w[0], w[1])).collect();
        g.sort_unstable();
        g
    };
    let (ga, gb) = (grams(a), grams(b));
    if ga.is_empty() && gb.is_empty() {
        return if a.trim().eq_ignore_ascii_case(b.trim()) {
            1.0
        } else {
            0.0
        };
    }
    let (mut i, mut j, mut common) = (0, 0, 0usize);
    while i < ga.len() && j < gb.len() {
        match ga[i].cmp(&gb[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                common += 1;
                i += 1;
                j += 1;
            }
        }
    }
    2.0 * common as f32 / (ga.len() + gb.len()) as f32
}

/// Sorted hashes of the words (or, for verbatim blocks, trimmed lines).
pub(crate) fn bag(text: &str, by_line: bool) -> Vec<u64> {
    let hash = |s: &str| {
        let mut h = DefaultHasher::new();
        s.hash(&mut h);
        h.finish()
    };
    let mut v: Vec<u64> = if by_line {
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(hash)
            .collect()
    } else {
        text.split_word_bounds()
            .filter(|t| is_word(t))
            .map(|t| hash(&t.to_lowercase()))
            .collect()
    };
    v.sort_unstable();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(segs: &[Segment]) -> String {
        segs.iter()
            .map(|s| match s.kind {
                SegmentKind::Equal => s.text.clone(),
                SegmentKind::Delete => format!("[-{}-]", s.text),
                SegmentKind::Insert => format!("{{+{}+}}", s.text),
            })
            .collect()
    }

    fn wd(a: &str, b: &str) -> String {
        render(&word_segments(a, b, true, None))
    }

    #[test]
    fn single_word_change() {
        assert_eq!(
            wd("the quick brown fox", "the fast brown fox"),
            "the [-quick-]{+fast+} brown fox"
        );
    }

    #[test]
    fn adjacent_word_changes_merge_across_spaces() {
        assert_eq!(
            wd("The quick brown fox jumps.", "A slow brown dog jumps."),
            "[-The quick-]{+A slow+} brown [-fox-]{+dog+} jumps."
        );
    }

    #[test]
    fn rewrapped_lines_are_equal() {
        assert_eq!(wd("one two\nthree", "one  two three"), "one  two three");
    }

    #[test]
    fn insertion_and_punctuation() {
        assert_eq!(
            wd("Install the app.", "Install the desktop app!"),
            "Install the {+desktop +}app[-.-]{+!+}"
        );
    }

    #[test]
    fn cjk_and_emoji_are_not_split() {
        let out = wd("我喜欢猫 👩‍👩‍👧 ok", "我喜欢狗 👩‍👩‍👧 ok");
        assert_eq!(out, "我喜欢[-猫-]{+狗+} 👩‍👩‍👧 ok");
        let out = wd("family 👩‍👩‍👧", "family 👨‍👩‍👦");
        assert_eq!(out, "family [-👩‍👩‍👧-]{+👨‍👩‍👦+}");
    }

    #[test]
    fn line_segments_keep_whole_lines() {
        let segs = line_segments("a\nb\nc", "a\nB\nc", None);
        assert_eq!(render(&segs), "a\n[-b\n-]{+B\n+}c");
    }

    #[test]
    fn similarity() {
        let a = bag("The quick brown fox", false);
        let b = bag("the quick red fox", false);
        assert!((bag_similarity(&a, &b) - 0.75).abs() < 1e-6);
        assert_eq!(bag_similarity(&[], &[]), 1.0);
        assert!(char_similarity("Installing", "Installation") >= 0.5);
        assert!(char_similarity("Usage", "Licence") < 0.3);
    }
}
