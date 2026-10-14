//! Human-readable diffs of Markdown documents.
//!
//! [`diff_markdown`] compares two versions block by block (paragraphs,
//! headings, list items, code blocks, tables...), the way tracked changes in
//! a word processor read: unchanged blocks, added and removed blocks, blocks
//! that moved, and modified blocks with word-level changes inside.
//!
//! [`diff_lines`] is a classic unified line diff for a source view, with
//! word-level highlighting inside changed lines.
//!
//! Both normalise a UTF-8 BOM and CRLF/CR line endings before comparing and
//! are bounded by a deadline so pathological inputs cannot stall the UI.

mod blocks;
mod lines;
mod words;

use std::collections::HashMap;
use std::ops::Range;
use std::time::{Duration, Instant};

use serde::Serialize;
use similar::{Algorithm, DiffOp, capture_diff_slices_deadline};

use blocks::{Block, Family, TableSrc};
pub use lines::{DiffLine, Hunk, LineKind, diff_lines};
use words::{bag, bag_similarity, char_similarity, count_words, line_segments, word_segments};

/// Upper bound on the time spent diffing one pair of documents. Past it the
/// diff stays correct but coarser (fewer pairings, whole-block changes).
pub(crate) const DEADLINE: Duration = Duration::from_secs(1);

/// Block-level diff of two Markdown documents.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocDiff {
    /// Every block of both documents in reading order: unchanged blocks,
    /// changes, and `movedAway` placeholders where moved blocks used to be.
    pub blocks: Vec<DiffBlock>,
    /// Totals for a summary line.
    pub stats: DiffStats,
    /// The deadline was hit; the result is valid but coarser than usual.
    pub timed_out: bool,
}

/// Totals for a diff.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffStats {
    /// Blocks only in the new document.
    pub added_blocks: usize,
    /// Blocks only in the old document.
    pub removed_blocks: usize,
    /// Blocks changed in place.
    pub modified_blocks: usize,
    /// Blocks moved unchanged to another position.
    pub moved_blocks: usize,
    /// Words inserted (in added and modified blocks).
    pub words_added: usize,
    /// Words deleted (in removed and modified blocks).
    pub words_removed: usize,
    /// No block changed. Inputs may still differ in line endings, a BOM or
    /// insignificant whitespace.
    pub identical: bool,
}

/// What happened to a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockChange {
    /// Same in both versions (insignificant whitespace aside).
    Unchanged,
    /// Only in the new version.
    Added,
    /// Only in the old version.
    Removed,
    /// Changed in place; see `segments`.
    Modified,
    /// Moved here unchanged from `old_lines` in the old version.
    Moved,
    /// Placeholder where a moved block used to be; `new_lines` says where it
    /// went. Not counted in the stats.
    MovedAway,
}

/// The kind of Markdown block.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BlockType {
    /// A paragraph.
    Paragraph,
    /// An ATX (`#`) or setext heading.
    Heading {
        /// 1 to 6.
        level: u8,
    },
    /// One list item, without its nested lists (those are separate blocks
    /// with a larger `depth`).
    ListItem {
        /// Numbered (`1.`) rather than bulleted.
        ordered: bool,
        /// Nesting depth, 0 for a top-level list.
        depth: u32,
        /// Task state for `- [ ]` / `- [x]` items, `None` for plain items.
        checked: Option<bool>,
        /// The item's number for ordered lists.
        number: Option<u32>,
    },
    /// A fenced or indented code block.
    Code {
        /// First word of the info string (`rust` in ```` ```rust ````).
        lang: Option<String>,
    },
    /// A GFM table (diffed per row and cell; see [`TableDiff`]).
    Table,
    /// A block quote (or GitHub alert), as one block.
    Quote,
    /// A thematic break (`---`).
    Rule,
    /// A raw HTML block.
    Html,
    /// YAML front matter between `---` lines.
    FrontMatter,
    /// Anything else: footnote definitions, link reference definitions,
    /// text after a nested list inside a list item.
    Other,
}

/// One block of the diff.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffBlock {
    /// What happened to the block.
    pub kind: BlockChange,
    /// The block's type in the new version (old version for removed and
    /// moved-away blocks).
    pub block_type: BlockType,
    /// The old type when a modified block changed type (heading level,
    /// task checkbox toggled, paragraph turned into a list item...).
    pub old_block_type: Option<BlockType>,
    /// Exact old source (LF line endings, no trailing newline).
    pub old_text: Option<String>,
    /// Exact new source (LF line endings, no trailing newline).
    pub new_text: Option<String>,
    /// 1-based inclusive line range in the old document.
    pub old_lines: Option<(u32, u32)>,
    /// 1-based inclusive line range in the new document (for `movedAway`,
    /// where the block went).
    pub new_lines: Option<(u32, u32)>,
    /// For modified blocks: the block's content as equal/insert/delete
    /// runs. Content excludes block markers (`## `, `- [ ] `, `> `, code
    /// fences), which `block_type` describes. Prose is diffed by word,
    /// code, HTML and front matter by whole line. Empty for other kinds.
    pub segments: Vec<Segment>,
    /// For modified tables: the table diffed row by row and cell by cell.
    pub table: Option<TableDiff>,
}

/// A run of text in a modified block or changed line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    /// Equal, inserted or deleted.
    pub kind: SegmentKind,
    /// The text of the run.
    pub text: String,
}

/// The role of a [`Segment`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SegmentKind {
    /// In both versions.
    Equal,
    /// Only in the new version.
    Insert,
    /// Only in the old version.
    Delete,
}

/// Column alignment of a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    /// No alignment given.
    None,
    /// `:--`
    Left,
    /// `:-:`
    Center,
    /// `--:`
    Right,
}

/// A modified table, ready to render as a table with highlighted cells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDiff {
    /// Column alignments of the new table.
    pub aligns: Vec<Align>,
    /// Header row first, then body rows in order (removed rows included).
    pub rows: Vec<RowDiff>,
}

/// One table row in a [`TableDiff`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowDiff {
    /// `unchanged`, `added`, `removed` or `modified`.
    pub kind: BlockChange,
    /// This is the header row.
    pub header: bool,
    /// Each cell's text as segments (a single `equal` segment when the cell
    /// did not change).
    pub cells: Vec<Vec<Segment>>,
}

/// An entry of [`DocDiff::collapsed`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CollapsedEntry {
    /// Show `blocks[index]`.
    Block {
        /// Index into [`DocDiff::blocks`].
        index: usize,
    },
    /// A run of unchanged blocks `blocks[start..end]` to show as
    /// "N unchanged blocks" (expandable).
    Gap {
        /// First hidden index.
        start: usize,
        /// One past the last hidden index.
        end: usize,
        /// `end - start`.
        count: usize,
    },
}

impl DocDiff {
    /// The changed blocks with up to `context` unchanged blocks around each,
    /// and the remaining unchanged runs folded into gaps. A gap that would
    /// hide a single block shows the block instead.
    pub fn collapsed(&self, context: usize) -> Vec<CollapsedEntry> {
        let n = self.blocks.len();
        let mut keep = vec![false; n];
        for (i, b) in self.blocks.iter().enumerate() {
            if b.kind != BlockChange::Unchanged {
                let from = i.saturating_sub(context);
                let to = (i + context + 1).min(n);
                keep[from..to].iter_mut().for_each(|k| *k = true);
            }
        }
        let mut out = Vec::new();
        let mut i = 0;
        while i < n {
            if keep[i] {
                out.push(CollapsedEntry::Block { index: i });
                i += 1;
                continue;
            }
            let start = i;
            while i < n && !keep[i] {
                i += 1;
            }
            if i - start == 1 {
                out.push(CollapsedEntry::Block { index: start });
            } else {
                out.push(CollapsedEntry::Gap {
                    start,
                    end: i,
                    count: i - start,
                });
            }
        }
        out
    }
}

/// Diffs two Markdown documents block by block. See the module docs.
pub fn diff_markdown(old: &str, new: &str) -> DocDiff {
    let deadline = Instant::now() + DEADLINE;
    let old = blocks::normalize(old);
    let new = blocks::normalize(new);
    let old_b = blocks::split(&old);
    if old == new {
        return identical(old_b);
    }
    let new_b = blocks::split(&new);
    Differ {
        old: &old_b,
        new: &new_b,
        deadline,
        out: Vec::new(),
        stats: DiffStats::default(),
        timed_out: false,
        old_bags: HashMap::new(),
        new_bags: HashMap::new(),
    }
    .run()
}

fn identical(blocks: Vec<Block>) -> DocDiff {
    DocDiff {
        blocks: blocks
            .into_iter()
            .map(|b| DiffBlock {
                kind: BlockChange::Unchanged,
                block_type: b.ty,
                old_block_type: None,
                old_text: Some(b.text.clone()),
                new_text: Some(b.text),
                old_lines: Some(b.lines),
                new_lines: Some(b.lines),
                segments: Vec::new(),
                table: None,
            })
            .collect(),
        stats: DiffStats {
            identical: true,
            ..DiffStats::default()
        },
        timed_out: false,
    }
}

struct Differ<'a> {
    old: &'a [Block],
    new: &'a [Block],
    deadline: Instant,
    out: Vec<DiffBlock>,
    stats: DiffStats,
    timed_out: bool,
    old_bags: HashMap<usize, Vec<u64>>,
    new_bags: HashMap<usize, Vec<u64>>,
}

/// Blocks shorter than this many words are never reported as moved: short
/// repeated text ("Note:", "TODO") would pair up by accident.
const MIN_MOVE_WORDS: usize = 3;

/// Prose blocks with at most this many words (headings, short list items)
/// are also compared by character bigrams, since they have too few words
/// for a word-based similarity to mean much.
const SHORT_BLOCK_WORDS: usize = 4;

/// Largest change region (old blocks x new blocks) aligned optimally; larger
/// regions are paired greedily.
const MAX_DP_CELLS: usize = 4096;

impl Differ<'_> {
    fn run(mut self) -> DocDiff {
        let old_keys: Vec<&str> = self.old.iter().map(|b| b.key.as_str()).collect();
        let new_keys: Vec<&str> = self.new.iter().map(|b| b.key.as_str()).collect();
        let ops = capture_diff_slices_deadline(
            Algorithm::Histogram,
            &old_keys,
            &new_keys,
            Some(self.deadline),
        );

        // Group consecutive non-equal ops into change regions.
        enum Item {
            Equal(Range<usize>, Range<usize>),
            Region(Range<usize>, Range<usize>),
        }
        let mut items: Vec<Item> = Vec::new();
        for op in &ops {
            let (o, n) = (op.old_range(), op.new_range());
            match (op, items.last_mut()) {
                (DiffOp::Equal { .. }, _) => items.push(Item::Equal(o, n)),
                (_, Some(Item::Region(ro, rn))) => {
                    ro.end = o.end;
                    rn.end = n.end;
                }
                _ => items.push(Item::Region(o, n)),
            }
        }

        // Moves: an old block deleted in one region whose exact key is
        // inserted in another (or the same) region.
        let mut moved_old: HashMap<usize, usize> = HashMap::new();
        let mut moved_new: HashMap<usize, usize> = HashMap::new();
        let mut inserted: HashMap<&str, Vec<usize>> = HashMap::new();
        for item in &items {
            if let Item::Region(_, n) = item {
                for j in n.clone().rev() {
                    inserted.entry(new_keys[j]).or_default().push(j);
                }
            }
        }
        for item in &items {
            if let Item::Region(o, _) = item {
                for i in o.clone() {
                    let b = &self.old[i];
                    if !movable(b) {
                        continue;
                    }
                    if let Some(j) = inserted.get_mut(b.key.as_str()).and_then(Vec::pop) {
                        moved_old.insert(i, j);
                        moved_new.insert(j, i);
                    }
                }
            }
        }

        for item in items {
            match item {
                Item::Equal(o, n) => {
                    for (i, j) in o.zip(n) {
                        self.emit_unchanged(i, j);
                    }
                }
                Item::Region(o, n) => self.region(o, n, &moved_old, &moved_new),
            }
        }

        self.stats.identical = self.stats.added_blocks
            + self.stats.removed_blocks
            + self.stats.modified_blocks
            + self.stats.moved_blocks
            == 0;
        let timed_out = self.timed_out || Instant::now() > self.deadline;
        DocDiff {
            blocks: self.out,
            stats: self.stats,
            timed_out,
        }
    }

    fn region(
        &mut self,
        o: Range<usize>,
        n: Range<usize>,
        moved_old: &HashMap<usize, usize>,
        moved_new: &HashMap<usize, usize>,
    ) {
        let olds: Vec<usize> = o.clone().filter(|i| !moved_old.contains_key(i)).collect();
        let news: Vec<usize> = n.clone().filter(|j| !moved_new.contains_key(j)).collect();
        let pairs = self.align(&olds, &news);

        let (mut oi, mut ni) = (o.start, n.start);
        let stops = pairs.iter().map(|&p| Some(p)).chain(std::iter::once(None));
        for pair in stops {
            let (stop_o, stop_n) = pair.unwrap_or((o.end, n.end));
            while oi < stop_o {
                match moved_old.get(&oi) {
                    Some(&j) => self.emit_moved_away(oi, j),
                    None => self.emit_removed(oi),
                }
                oi += 1;
            }
            while ni < stop_n {
                match moved_new.get(&ni) {
                    Some(&i) => self.emit_moved(i, ni),
                    None => self.emit_added(ni),
                }
                ni += 1;
            }
            if let Some((po, pn)) = pair {
                self.emit_modified(po, pn);
                oi = po + 1;
                ni = pn + 1;
            }
        }
    }

    /// Pairs old and new blocks of a change region that are versions of each
    /// other, keeping order. Optimal (by summed similarity) for small
    /// regions, greedy for big ones.
    fn align(&mut self, olds: &[usize], news: &[usize]) -> Vec<(usize, usize)> {
        if olds.is_empty() || news.is_empty() {
            return Vec::new();
        }
        if Instant::now() > self.deadline {
            self.timed_out = true;
            return Vec::new();
        }
        let single = olds.len() == 1 && news.len() == 1;
        let (h, w) = (olds.len(), news.len());
        let mut pairs = Vec::new();

        if h * w <= MAX_DP_CELLS {
            let mut score = vec![None; h * w];
            for (a, &i) in olds.iter().enumerate() {
                for (b, &j) in news.iter().enumerate() {
                    score[a * w + b] = self.pair_score(i, j, single);
                }
            }
            // dp[a][b]: best total for olds[a..] x news[b..].
            let mut dp = vec![0f32; (h + 1) * (w + 1)];
            for a in (0..h).rev() {
                for b in (0..w).rev() {
                    let skip = dp[(a + 1) * (w + 1) + b].max(dp[a * (w + 1) + b + 1]);
                    let take =
                        score[a * w + b].map_or(f32::MIN, |s| s + dp[(a + 1) * (w + 1) + b + 1]);
                    dp[a * (w + 1) + b] = skip.max(take);
                }
            }
            let (mut a, mut b) = (0, 0);
            while a < h && b < w {
                let here = dp[a * (w + 1) + b];
                if let Some(s) = score[a * w + b]
                    && (s + dp[(a + 1) * (w + 1) + b + 1] - here).abs() < 1e-6
                {
                    pairs.push((olds[a], news[b]));
                    a += 1;
                    b += 1;
                } else if (dp[(a + 1) * (w + 1) + b] - here).abs() < 1e-6 {
                    a += 1;
                } else {
                    b += 1;
                }
            }
        } else {
            // Greedy: each old block takes the first pairable new block in a
            // small window ahead of the last pairing.
            const WINDOW: usize = 8;
            let mut next = 0;
            for &i in olds {
                if Instant::now() > self.deadline {
                    self.timed_out = true;
                    break;
                }
                let end = (next + WINDOW).min(w);
                if let Some(b) = (next..end).find(|&b| self.pair_score(i, news[b], false).is_some())
                {
                    pairs.push((i, news[b]));
                    next = b + 1;
                }
            }
        }
        pairs
    }

    /// How alike two blocks are, or `None` when they should not be shown as
    /// one modified block.
    fn pair_score(&mut self, i: usize, j: usize, single: bool) -> Option<f32> {
        let (old, new) = (self.old, self.new);
        let (a, b) = (&old[i], &new[j]);
        let (fa, fb) = (a.family(), b.family());
        if fa != fb || fa == Family::Rule {
            return None;
        }
        let by_line = fa != Family::Prose && fa != Family::Table;
        let ab = self
            .old_bags
            .entry(i)
            .or_insert_with(|| bag(&a.content, by_line));
        let bb = self
            .new_bags
            .entry(j)
            .or_insert_with(|| bag(&b.content, by_line));
        let mut sim = bag_similarity(ab, bb);
        if fa == Family::Prose
            && count_words(&a.content) <= SHORT_BLOCK_WORDS
            && count_words(&b.content) <= SHORT_BLOCK_WORDS
        {
            sim = sim.max(char_similarity(&a.content, &b.content));
        }
        let same_kind = std::mem::discriminant(&a.ty) == std::mem::discriminant(&b.ty);
        let ok = match fa {
            Family::Prose if same_kind => {
                let headings = matches!(a.ty, BlockType::Heading { .. });
                sim >= 0.5 || (single && (sim >= 0.2 || headings))
            }
            Family::Prose => sim >= 0.6,
            Family::FrontMatter => true,
            Family::Code | Family::Table | Family::Html => sim >= 0.3 || single,
            Family::Rule => false,
        };
        ok.then_some(sim + if same_kind { 0.1 } else { 0.0 })
    }

    fn base(&self, kind: BlockChange, old: Option<usize>, new: Option<usize>) -> DiffBlock {
        let o = old.map(|i| &self.old[i]);
        let n = new.map(|j| &self.new[j]);
        DiffBlock {
            kind,
            block_type: n.or(o).map(|b| b.ty.clone()).unwrap_or(BlockType::Other),
            old_block_type: None,
            old_text: o.map(|b| b.text.clone()),
            new_text: n.map(|b| b.text.clone()),
            old_lines: o.map(|b| b.lines),
            new_lines: n.map(|b| b.lines),
            segments: Vec::new(),
            table: None,
        }
    }

    fn emit_unchanged(&mut self, i: usize, j: usize) {
        let b = self.base(BlockChange::Unchanged, Some(i), Some(j));
        self.out.push(b);
    }

    fn emit_added(&mut self, j: usize) {
        self.stats.added_blocks += 1;
        self.stats.words_added += count_words(&self.new[j].content);
        let b = self.base(BlockChange::Added, None, Some(j));
        self.out.push(b);
    }

    fn emit_removed(&mut self, i: usize) {
        self.stats.removed_blocks += 1;
        self.stats.words_removed += count_words(&self.old[i].content);
        let b = self.base(BlockChange::Removed, Some(i), None);
        self.out.push(b);
    }

    fn emit_moved(&mut self, i: usize, j: usize) {
        self.stats.moved_blocks += 1;
        let b = self.base(BlockChange::Moved, Some(i), Some(j));
        self.out.push(b);
    }

    fn emit_moved_away(&mut self, i: usize, j: usize) {
        let mut b = self.base(BlockChange::MovedAway, Some(i), None);
        b.new_lines = Some(self.new[j].lines);
        self.out.push(b);
    }

    fn emit_modified(&mut self, i: usize, j: usize) {
        let (a, n) = (&self.old[i], &self.new[j]);
        let deadline = if Instant::now() > self.deadline {
            // Out of time: show the whole block as replaced.
            self.timed_out = true;
            Some(Instant::now())
        } else {
            Some(self.deadline)
        };
        let mut b = self.base(BlockChange::Modified, Some(i), Some(j));
        if a.ty != n.ty {
            b.old_block_type = Some(a.ty.clone());
        }
        b.segments = match a.family() {
            Family::Prose | Family::Table => word_segments(&a.content, &n.content, true, deadline),
            _ => line_segments(&a.content, &n.content, deadline),
        };
        if let (Some(ta), Some(tn)) = (&a.table, &n.table) {
            b.table = Some(table_diff(ta, tn, deadline));
        }
        for s in &b.segments {
            match s.kind {
                SegmentKind::Insert => self.stats.words_added += count_words(&s.text),
                SegmentKind::Delete => self.stats.words_removed += count_words(&s.text),
                SegmentKind::Equal => {}
            }
        }
        self.stats.modified_blocks += 1;
        self.out.push(b);
    }
}

fn movable(b: &Block) -> bool {
    !matches!(b.ty, BlockType::Rule | BlockType::FrontMatter)
        && (count_words(&b.content) >= MIN_MOVE_WORDS || b.family() == Family::Code)
}

/// Row-and-cell diff of two tables.
fn table_diff(old: &TableSrc, new: &TableSrc, deadline: Option<Instant>) -> TableDiff {
    let mut rows = vec![row_pair(&old.header, &new.header, true, deadline)];
    let key = |r: &Vec<String>| r.join("\u{1F}");
    let ok: Vec<String> = old.rows.iter().map(key).collect();
    let nk: Vec<String> = new.rows.iter().map(key).collect();
    let ops = capture_diff_slices_deadline(Algorithm::Histogram, &ok, &nk, deadline);
    for op in ops {
        let (o, n) = (op.old_range(), op.new_range());
        if let DiffOp::Equal { .. } = op {
            for j in n {
                rows.push(whole_row(&new.rows[j], BlockChange::Unchanged));
            }
            continue;
        }
        // Pair rows in order when they share a cell or most words.
        let mut ins = Vec::new();
        for k in 0..o.len().max(n.len()) {
            let (a, b) = (o.clone().nth(k), n.clone().nth(k));
            match (a, b) {
                (Some(a), Some(b)) if rows_alike(&old.rows[a], &new.rows[b]) => {
                    rows.push(row_pair(&old.rows[a], &new.rows[b], false, deadline));
                }
                (a, b) => {
                    if let Some(a) = a {
                        rows.push(whole_row(&old.rows[a], BlockChange::Removed));
                    }
                    if let Some(b) = b {
                        ins.push(whole_row(&new.rows[b], BlockChange::Added));
                    }
                }
            }
        }
        rows.extend(ins);
    }
    TableDiff {
        aligns: new.aligns.clone(),
        rows,
    }
}

fn rows_alike(a: &[String], b: &[String]) -> bool {
    a.iter().zip(b).any(|(x, y)| x == y && !x.is_empty())
        || bag_similarity(&bag(&a.join(" "), false), &bag(&b.join(" "), false)) >= 0.5
}

fn whole_row(cells: &[String], kind: BlockChange) -> RowDiff {
    let seg = match kind {
        BlockChange::Added => SegmentKind::Insert,
        BlockChange::Removed => SegmentKind::Delete,
        _ => SegmentKind::Equal,
    };
    RowDiff {
        kind,
        header: false,
        cells: cells
            .iter()
            .map(|c| {
                vec![Segment {
                    kind: seg,
                    text: c.clone(),
                }]
            })
            .collect(),
    }
}

fn row_pair(old: &[String], new: &[String], header: bool, deadline: Option<Instant>) -> RowDiff {
    let mut changed = false;
    let cells = (0..old.len().max(new.len()))
        .map(|c| match (old.get(c), new.get(c)) {
            (Some(a), Some(b)) if a == b => vec![Segment {
                kind: SegmentKind::Equal,
                text: b.clone(),
            }],
            (Some(a), Some(b)) => {
                changed = true;
                word_segments(a, b, true, deadline)
            }
            (Some(a), None) => {
                changed = true;
                vec![Segment {
                    kind: SegmentKind::Delete,
                    text: a.clone(),
                }]
            }
            (None, b) => {
                changed = true;
                vec![Segment {
                    kind: SegmentKind::Insert,
                    text: b.cloned().unwrap_or_default(),
                }]
            }
        })
        .collect();
    RowDiff {
        kind: if changed {
            BlockChange::Modified
        } else {
            BlockChange::Unchanged
        },
        header,
        cells,
    }
}
