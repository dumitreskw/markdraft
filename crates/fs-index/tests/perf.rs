//! Scan performance check: `cargo test -p fs-index --release --test perf -- --ignored --nocapture`.

use std::time::Instant;

use fs_index::{DiscoveryRules, ScanRequest, scan};

#[test]
#[ignore = "slow; run explicitly in release mode"]
fn ten_thousand_files_first_batch_under_a_second() {
    let dir = tempfile::tempdir().unwrap();
    for d in 0..100 {
        let folder = dir.path().join(format!("section-{d:03}"));
        std::fs::create_dir_all(&folder).unwrap();
        for f in 0..100 {
            std::fs::write(
                folder.join(format!("note-{f:03}.md")),
                format!("---\ntags: [a]\n---\n\n# Note {d}-{f}\n\nSome body text.\n"),
            )
            .unwrap();
        }
    }
    let req = ScanRequest {
        root: dir.path().to_path_buf(),
        rules: DiscoveryRules::default(),
        max_depth: None,
    };
    let started = Instant::now();
    let mut first = None;
    let mut total = 0;
    let stats = scan(&req, 500, |batch| {
        first.get_or_insert_with(|| started.elapsed());
        total += batch.len();
    });
    println!(
        "first batch after {:?}, {} nodes in {:?}",
        first.unwrap(),
        total,
        stats.elapsed
    );
    assert_eq!(stats.files, 10_000);
    assert!(first.unwrap().as_millis() < 1000);
}
