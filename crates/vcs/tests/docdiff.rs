//! Behaviour tests for `vcs::docdiff` on realistic prose.

use std::fmt::Write as _;

use vcs::docdiff::{
    BlockChange, BlockType, CollapsedEntry, DiffBlock, DocDiff, LineKind, SegmentKind, diff_lines,
    diff_markdown,
};

/// Renders segments in word-diff notation: `[-old-]{+new+}`.
fn marked(b: &DiffBlock) -> String {
    b.segments
        .iter()
        .map(|s| match s.kind {
            SegmentKind::Equal => s.text.clone(),
            SegmentKind::Delete => format!("[-{}-]", s.text),
            SegmentKind::Insert => format!("{{+{}+}}", s.text),
        })
        .collect()
}

fn changed(d: &DocDiff) -> Vec<&DiffBlock> {
    d.blocks
        .iter()
        .filter(|b| b.kind != BlockChange::Unchanged)
        .collect()
}

/// A compact, human-readable rendering of a whole diff for snapshots.
fn render(d: &DocDiff) -> String {
    let mut out = String::new();
    let s = &d.stats;
    writeln!(
        out,
        "stats: +{} -{} ~{} moved {} | words +{} -{} | identical {}",
        s.added_blocks,
        s.removed_blocks,
        s.modified_blocks,
        s.moved_blocks,
        s.words_added,
        s.words_removed,
        s.identical
    )
    .unwrap();
    for b in &d.blocks {
        let sign = match b.kind {
            BlockChange::Unchanged => " ",
            BlockChange::Added => "+",
            BlockChange::Removed => "-",
            BlockChange::Modified => "~",
            BlockChange::Moved => ">",
            BlockChange::MovedAway => "<",
        };
        let lines = |l: Option<(u32, u32)>| match l {
            Some((a, b)) if a == b => format!("{a}"),
            Some((a, b)) => format!("{a}-{b}"),
            None => "_".into(),
        };
        let ty = match &b.old_block_type {
            Some(old) => format!("{} (was {})", type_name(&b.block_type), type_name(old)),
            None => type_name(&b.block_type),
        };
        let body = match b.kind {
            BlockChange::Modified => marked(b),
            BlockChange::Removed | BlockChange::MovedAway => b.old_text.clone().unwrap(),
            _ => b.new_text.clone().unwrap(),
        };
        let body = body.replace('\n', "\n    ");
        writeln!(
            out,
            "{sign} {ty} [{} -> {}]\n    {body}",
            lines(b.old_lines),
            lines(b.new_lines)
        )
        .unwrap();
        if let Some(t) = &b.table {
            for r in &t.rows {
                let cells: Vec<String> = r
                    .cells
                    .iter()
                    .map(|c| {
                        c.iter()
                            .map(|s| match s.kind {
                                SegmentKind::Equal => s.text.clone(),
                                SegmentKind::Delete => format!("[-{}-]", s.text),
                                SegmentKind::Insert => format!("{{+{}+}}", s.text),
                            })
                            .collect()
                    })
                    .collect();
                writeln!(out, "    | {:?}: {}", r.kind, cells.join(" | ")).unwrap();
            }
        }
    }
    out
}

fn type_name(t: &BlockType) -> String {
    match t {
        BlockType::Heading { level } => format!("h{level}"),
        BlockType::ListItem {
            ordered,
            depth,
            checked,
            number,
        } => {
            let mut s = format!("li d{depth}");
            if *ordered {
                write!(s, " #{}", number.unwrap_or(0)).unwrap();
            }
            match checked {
                Some(true) => s.push_str(" [x]"),
                Some(false) => s.push_str(" [ ]"),
                None => {}
            }
            s
        }
        BlockType::Code { lang } => format!("code {}", lang.as_deref().unwrap_or("-")),
        other => format!("{other:?}").to_lowercase(),
    }
}

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

#[test]
fn edit_a_word_in_a_paragraph() {
    let old = "# Title\n\nThe quick brown fox jumps over the lazy dog.\n\nLast line.\n";
    let new = "# Title\n\nThe fast brown fox jumps over the lazy dog.\n\nLast line.\n";
    let d = diff_markdown(old, new);
    let c = changed(&d);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].kind, BlockChange::Modified);
    assert_eq!(c[0].block_type, BlockType::Paragraph);
    assert_eq!(c[0].old_lines, Some((3, 3)));
    assert_eq!(c[0].new_lines, Some((3, 3)));
    assert_eq!(
        marked(c[0]),
        "The [-quick-]{+fast+} brown fox jumps over the lazy dog."
    );
    assert_eq!((d.stats.words_added, d.stats.words_removed), (1, 1));
    assert_eq!(d.stats.modified_blocks, 1);
    assert!(!d.stats.identical);
}

#[test]
fn insert_heading_and_paragraph() {
    let old = "# Guide\n\nIntro text.\n\n## Usage\n\nRun it.\n";
    let new = "# Guide\n\nIntro text.\n\n## Install\n\nDownload the installer and run it.\n\n## Usage\n\nRun it.\n";
    let d = diff_markdown(old, new);
    let c = changed(&d);
    assert_eq!(c.len(), 2, "{}", render(&d));
    assert_eq!(c[0].kind, BlockChange::Added);
    assert_eq!(c[0].block_type, BlockType::Heading { level: 2 });
    assert_eq!(c[0].new_text.as_deref(), Some("## Install"));
    assert_eq!(c[0].new_lines, Some((5, 5)));
    assert_eq!(c[1].kind, BlockChange::Added);
    assert_eq!(c[1].block_type, BlockType::Paragraph);
    assert_eq!(d.stats.added_blocks, 2);
    assert_eq!(d.stats.words_added, 1 + 6);
}

#[test]
fn delete_a_list_item() {
    let old = "Steps:\n\n- Open the app\n- Pick a folder\n- Start writing\n";
    let new = "Steps:\n\n- Open the app\n- Start writing\n";
    let d = diff_markdown(old, new);
    let c = changed(&d);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].kind, BlockChange::Removed);
    assert_eq!(c[0].old_text.as_deref(), Some("- Pick a folder"));
    assert_eq!(c[0].old_lines, Some((4, 4)));
    assert!(matches!(
        c[0].block_type,
        BlockType::ListItem {
            ordered: false,
            depth: 0,
            checked: None,
            ..
        }
    ));
    assert_eq!(d.stats.words_removed, 3);
}

#[test]
fn move_a_paragraph() {
    let old = "# Doc\n\nFirst paragraph about setup.\n\nSecond paragraph about usage.\n\nThird paragraph about support.\n";
    let new = "# Doc\n\nSecond paragraph about usage.\n\nFirst paragraph about setup.\n\nThird paragraph about support.\n";
    let d = diff_markdown(old, new);
    let c = changed(&d);
    assert_eq!(c.len(), 2, "{}", render(&d));
    let moved = c.iter().find(|b| b.kind == BlockChange::Moved).unwrap();
    let away = c.iter().find(|b| b.kind == BlockChange::MovedAway).unwrap();
    assert_eq!(moved.old_lines, away.old_lines);
    assert_eq!(moved.new_lines, away.new_lines);
    assert_eq!(d.stats.moved_blocks, 1);
    assert_eq!(
        (
            d.stats.added_blocks,
            d.stats.removed_blocks,
            d.stats.words_added
        ),
        (0, 0, 0)
    );
}

#[test]
fn change_a_line_in_a_code_block() {
    let old = "Run:\n\n```sh\nnpm install\nnpm run build\nnpm test\n```\n";
    let new = "Run:\n\n```sh\nnpm install\nnpm run build -- --release\nnpm test\n```\n";
    let d = diff_markdown(old, new);
    let c = changed(&d);
    assert_eq!(c.len(), 1);
    assert_eq!(
        c[0].block_type,
        BlockType::Code {
            lang: Some("sh".into())
        }
    );
    assert_eq!(
        marked(c[0]),
        "npm install\n[-npm run build\n-]{+npm run build -- --release\n+}npm test"
    );
}

#[test]
fn change_a_table_cell() {
    let old = "| Name | Role |\n|------|------|\n| Ada  | Lead |\n| Alan | Dev  |\n";
    let new = "| Name | Role |\n|------|------|\n| Ada  | Lead |\n| Alan | QA   |\n";
    let d = diff_markdown(old, new);
    let c = changed(&d);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].block_type, BlockType::Table);
    let t = c[0].table.as_ref().unwrap();
    let kinds: Vec<_> = t.rows.iter().map(|r| (r.kind, r.header)).collect();
    assert_eq!(
        kinds,
        [
            (BlockChange::Unchanged, true),
            (BlockChange::Unchanged, false),
            (BlockChange::Modified, false)
        ]
    );
    let cell = &t.rows[2].cells[1];
    let cell: Vec<_> = cell.iter().map(|s| (s.kind, s.text.as_str())).collect();
    assert_eq!(
        cell,
        [(SegmentKind::Delete, "Dev"), (SegmentKind::Insert, "QA")]
    );
    assert_eq!(t.rows[2].cells[0][0].kind, SegmentKind::Equal);
}

#[test]
fn toggle_a_task_checkbox() {
    let old = "- [ ] Write docs\n- [ ] Ship it\n";
    let new = "- [x] Write docs\n- [ ] Ship it\n";
    let d = diff_markdown(old, new);
    let c = changed(&d);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].kind, BlockChange::Modified);
    assert!(matches!(
        c[0].block_type,
        BlockType::ListItem {
            checked: Some(true),
            ..
        }
    ));
    assert!(matches!(
        c[0].old_block_type,
        Some(BlockType::ListItem {
            checked: Some(false),
            ..
        })
    ));
    // The words did not change, only the checkbox.
    assert_eq!(marked(c[0]), "Write docs");
    assert_eq!((d.stats.words_added, d.stats.words_removed), (0, 0));
}

#[test]
fn heading_level_change_is_a_modification() {
    let d = diff_markdown("## Setup\n\nText.\n", "### Setup\n\nText.\n");
    let c = changed(&d);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].block_type, BlockType::Heading { level: 3 });
    assert_eq!(c[0].old_block_type, Some(BlockType::Heading { level: 2 }));
}

#[test]
fn crlf_and_lf_are_identical() {
    let lf = fixture("user-guide.before.md");
    let crlf = lf.replace('\n', "\r\n");
    let d = diff_markdown(&lf, &crlf);
    assert!(d.stats.identical);
    assert!(changed(&d).is_empty());
    let bom = format!("\u{FEFF}{crlf}");
    assert!(diff_markdown(&lf, &bom).stats.identical);
    assert!(diff_lines(&lf, &bom, 3).is_empty());
}

#[test]
fn whitespace_only_changes_are_ignored() {
    let old = "# Title   \n\nA paragraph that is\nwrapped at a short width.\n\n\n- item one\n- item  two\n";
    let new =
        "# Title\n\nA paragraph that is wrapped at a short width.\n\n- item one  \n- item two\n";
    let d = diff_markdown(old, new);
    assert!(d.stats.identical, "{}", render(&d));
    // Line numbers still follow each side.
    let last = d.blocks.last().unwrap();
    assert_eq!(last.old_lines, Some((8, 8)));
    assert_eq!(last.new_lines, Some((6, 6)));
}

#[test]
fn new_and_deleted_documents() {
    let doc = "# Title\n\nBody text here.\n";
    let added = diff_markdown("", doc);
    assert!(added.blocks.iter().all(|b| b.kind == BlockChange::Added));
    assert_eq!(added.stats.added_blocks, 2);
    assert_eq!(added.stats.words_added, 4);
    let removed = diff_markdown(doc, "");
    assert!(
        removed
            .blocks
            .iter()
            .all(|b| b.kind == BlockChange::Removed)
    );
    assert_eq!(removed.stats.removed_blocks, 2);
    let empty = diff_markdown("", "");
    assert!(empty.stats.identical && empty.blocks.is_empty());
}

#[test]
fn identical_inputs_take_the_fast_path() {
    let doc = fixture("user-guide.before.md");
    let d = diff_markdown(&doc, &doc);
    assert!(d.stats.identical);
    assert!(d.blocks.iter().all(|b| b.kind == BlockChange::Unchanged));
    assert_eq!(d.collapsed(2).len(), 1, "everything folds into one gap");
}

#[test]
fn unicode_words_are_diffed_whole() {
    let d = diff_markdown(
        "東京は大きい都市です。🎉 Party!\n",
        "東京は小さい都市です。🎉 Party!\n",
    );
    // Unicode word boundaries keep a kanji with the kana that follows it.
    assert_eq!(
        marked(changed(&d)[0]),
        "東京は[-大き-]{+小さ+}い都市です。🎉 Party!"
    );
    let d = diff_markdown("Team: 👩‍💻 and 👨‍🔬.\n", "Team: 👩‍💻 and 👩‍🔬.\n");
    assert_eq!(marked(changed(&d)[0]), "Team: 👩‍💻 and [-👨‍🔬-]{+👩‍🔬+}.");
}

#[test]
fn collapsed_keeps_context_around_changes() {
    let mut old = String::new();
    for i in 0..20 {
        writeln!(old, "Paragraph number {i}.\n").unwrap();
    }
    let new = old.replace("Paragraph number 10.", "Paragraph number ten.");
    let d = diff_markdown(&old, &new);
    let entries = d.collapsed(2);
    assert_eq!(
        entries,
        [
            CollapsedEntry::Gap {
                start: 0,
                end: 8,
                count: 8
            },
            CollapsedEntry::Block { index: 8 },
            CollapsedEntry::Block { index: 9 },
            CollapsedEntry::Block { index: 10 },
            CollapsedEntry::Block { index: 11 },
            CollapsedEntry::Block { index: 12 },
            CollapsedEntry::Gap {
                start: 13,
                end: 20,
                count: 7
            },
        ]
    );
    assert_eq!(d.blocks[10].kind, BlockChange::Modified);
}

#[test]
fn unrelated_replacement_is_removed_plus_added() {
    let old = "Intro.\n\nWe use tabs for indentation in every file.\n\nOutro.\n";
    let new = "Intro.\n\nContact support by email for billing questions.\n\nOutro.\n";
    let d = diff_markdown(old, new);
    let kinds: Vec<_> = changed(&d).iter().map(|b| b.kind).collect();
    assert_eq!(kinds, [BlockChange::Removed, BlockChange::Added]);
}

#[test]
fn source_view_lines() {
    let old = fixture("user-guide.before.md");
    let new = fixture("user-guide.after.md");
    let hunks = diff_lines(&old, &new, 2);
    assert!(!hunks.is_empty());
    for h in &hunks {
        let olds = h
            .lines
            .iter()
            .filter(|l| l.kind != LineKind::Insert)
            .count();
        let news = h
            .lines
            .iter()
            .filter(|l| l.kind != LineKind::Delete)
            .count();
        assert_eq!(olds as u32, h.old_len);
        assert_eq!(news as u32, h.new_len);
    }
    // The version bump in the front matter is the first change.
    let first_delete = hunks[0]
        .lines
        .iter()
        .find(|l| l.kind == LineKind::Delete)
        .unwrap();
    assert_eq!(first_delete.text, "version: 0.3");
    assert_eq!(first_delete.old_no, Some(3));
}

#[test]
fn snapshot_user_guide() {
    let d = diff_markdown(
        &fixture("user-guide.before.md"),
        &fixture("user-guide.after.md"),
    );
    insta::assert_snapshot!(render(&d));
}

#[test]
fn snapshot_release_notes_crlf() {
    // The new version has Windows line endings; only real edits show.
    let new = fixture("release-notes.after.md").replace('\n', "\r\n");
    let d = diff_markdown(&fixture("release-notes.before.md"), &new);
    insta::assert_snapshot!(render(&d));
}

#[test]
fn snapshot_nested_lists() {
    let d = diff_markdown(
        &fixture("nested-lists.before.md"),
        &fixture("nested-lists.after.md"),
    );
    insta::assert_snapshot!(render(&d));
}

#[test]
fn snapshot_json_small_paragraph_edit() {
    let d = diff_markdown(
        "# Notes\n\nShip the beta on Friday.\n",
        "# Notes\n\nShip the public beta on Monday.\n",
    );
    insta::assert_json_snapshot!(d);
}

/// A realistic 5,000-line document with scattered edits.
fn big_pair() -> (String, String) {
    let mut old = String::new();
    let mut new = String::new();
    let mut line = 0;
    let mut i = 0;
    while line < 5000 {
        let section = format!("## Section {i}\n\n");
        let para = format!(
            "Paragraph {i} explains how the feature number {i} works in practice and\n\
             what a writer should expect when they use it for the first time today.\n\n"
        );
        let list = format!("- Point {i}.a about details\n- Point {i}.b about caveats\n\n");
        let code = format!("```rust\nfn feature_{i}() -> u32 {{\n    {i}\n}}\n```\n\n");
        old.push_str(&section);
        old.push_str(&para);
        old.push_str(&list);
        old.push_str(&code);
        if i % 10 == 3 {
            new.push_str(&section);
            new.push_str(&para.replace("first time", "very first time"));
            new.push_str(&list);
            new.push_str(&code.replace("    ", "    1 + "));
        } else if i % 25 == 7 {
            new.push_str(&section);
            new.push_str(&list);
            new.push_str(&code);
        } else {
            new.push_str(&section);
            new.push_str(&para);
            new.push_str(&list);
            new.push_str(&code);
        }
        line += 14;
        i += 1;
    }
    (old, new)
}

#[test]
fn large_documents_are_diffed_quickly() {
    let (old, new) = big_pair();
    assert!(old.lines().count() >= 5000);
    let start = std::time::Instant::now();
    let d = diff_markdown(&old, &new);
    let blocks = start.elapsed();
    let start = std::time::Instant::now();
    let hunks = diff_lines(&old, &new, 3);
    let lines = start.elapsed();
    eprintln!(
        "5,000-line document: diff_markdown {blocks:?} ({} blocks), diff_lines {lines:?} ({} hunks)",
        d.blocks.len(),
        hunks.len()
    );
    assert!(!d.timed_out);
    assert_eq!(d.stats.removed_blocks, 15);
    assert_eq!(d.stats.modified_blocks, 36 * 2);
    // Generous bound for unoptimised test builds; release is far faster.
    let limit = if cfg!(debug_assertions) { 2000 } else { 100 };
    assert!(blocks.as_millis() < limit, "{blocks:?}");
    assert!(lines.as_millis() < limit, "{lines:?}");
}

#[test]
fn completely_different_large_documents_stay_bounded() {
    let old: String = (0..3000)
        .map(|i| format!("Old paragraph {i} with some words.\n\n"))
        .collect();
    let new: String = (0..3000)
        .map(|i| format!("Brand new text {} here instead.\n\n", i * 7))
        .collect();
    let start = std::time::Instant::now();
    let d = diff_markdown(&old, &new);
    let elapsed = start.elapsed();
    eprintln!("3,000 vs 3,000 rewritten paragraphs: {elapsed:?}");
    assert!(elapsed.as_secs_f32() < 3.0, "{elapsed:?}");
    assert!(!d.stats.identical);
}

#[test]
fn odd_inputs_do_not_panic() {
    let samples = [
        "",
        "\n\n\n",
        "\u{FEFF}",
        "```\nunclosed fence\n",
        "- \u{3000}\u{3000}item with ideographic spaces\n  \u{3000}\u{3000}continued\n",
        "-\n-\n- [ ]\n- [x]\n",
        "1) one\n2) two\n   - nested\n\n     after nested\n",
        "> quote\nlazy continuation\n>\n> - list in quote\n",
        "| a |\n|---|\n| \\| escaped |\n| ragged | row | extra |\n",
        "Setext\n======\n\nSub\n---\n",
        "[^1]: Footnote text.\n\n[link]: https://example.com \"title\"\n",
        "<div>\n<b>html</b>\n</div>\n",
        "    indented code\n\ttabbed\n",
        "\r\r\r# CR only\rline\r",
        "Mixed 日本語 text, émoji 🧑🏽‍🚀 and combining e\u{301}.\n",
    ];
    for old in samples {
        for new in samples {
            let d = diff_markdown(old, new);
            let _ = d.collapsed(1);
            let _ = diff_lines(old, new, 2);
            if old == new {
                assert!(d.stats.identical, "{old:?}");
            }
        }
    }
}

#[test]
fn huge_paragraph_rewrite_is_bounded() {
    let words: Vec<String> = (0..20_000).map(|i| format!("w{}", i % 997)).collect();
    let old = words.join(" ");
    let new = words.iter().rev().cloned().collect::<Vec<_>>().join(" ");
    let start = std::time::Instant::now();
    let d = diff_markdown(&old, &new);
    let elapsed = start.elapsed();
    eprintln!(
        "20,000-word paragraph reversed: {elapsed:?} (timed out: {})",
        d.timed_out
    );
    assert!(elapsed.as_secs_f32() < 5.0, "{elapsed:?}");
    assert_eq!(d.blocks.len(), 1);
}
