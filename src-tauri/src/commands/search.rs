//! Quick open across all open roots.

use std::path::PathBuf;

use fs_index::RootId;
use serde::Serialize;
use tauri::State;

use crate::error::{CmdError, blocking};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickOpenItem {
    pub root_id: RootId,
    pub path: PathBuf,
    /// Path shown to the user (prefixed with the root name when several roots are open).
    pub display: String,
    pub title: Option<String>,
    pub score: u32,
    /// Matched character positions in `display`.
    pub display_indices: Vec<u32>,
    /// Matched character positions in `title`.
    pub title_indices: Vec<u32>,
}

struct Candidate {
    root_id: RootId,
    path: PathBuf,
    display: String,
    title: Option<String>,
}

#[tauri::command]
pub async fn quick_open(
    state: State<'_, AppState>,
    query: String,
    limit: u32,
) -> Result<Vec<QuickOpenItem>, CmdError> {
    let candidates: Vec<Candidate> = {
        let roots = state.roots_read();
        let several = roots.list.len() > 1;
        roots
            .list
            .iter()
            .flat_map(|root| {
                let index = root.index.read().unwrap_or_else(|p| p.into_inner());
                index
                    .iter()
                    .map(|(rel, title)| Candidate {
                        root_id: root.id,
                        path: root.path.join(rel),
                        display: if several {
                            format!("{}/{rel}", root.name)
                        } else {
                            rel.clone()
                        },
                        title: title.clone(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    blocking(move || Ok(rank(&query, candidates, limit as usize))).await
}

/// Score, matched display positions, matched title positions.
type Best = (u32, Vec<u32>, Vec<u32>);

/// Ranks by path and by title, keeping each document's better match.
fn rank(query: &str, candidates: Vec<Candidate>, limit: usize) -> Vec<QuickOpenItem> {
    let displays: Vec<&str> = candidates.iter().map(|c| c.display.as_str()).collect();
    let titles: Vec<&str> = candidates
        .iter()
        .map(|c| c.title.as_deref().unwrap_or(""))
        .collect();
    let mut best: Vec<Option<Best>> = vec![None; candidates.len()];
    for m in search::rank(query, &displays, usize::MAX) {
        best[m.index] = Some((m.score, m.indices, Vec::new()));
    }
    for m in search::rank(query, &titles, usize::MAX) {
        // Titles are what people remember; give them a small edge.
        let score = m.score + m.score / 10;
        match &mut best[m.index] {
            Some((s, _, title_idx)) if *s >= score => {
                if title_idx.is_empty() {
                    *title_idx = m.indices;
                }
            }
            slot => *slot = Some((score, Vec::new(), m.indices)),
        }
    }
    let mut items: Vec<QuickOpenItem> = candidates
        .into_iter()
        .zip(best)
        .filter_map(|(c, b)| {
            let (score, display_indices, title_indices) = b?;
            Some(QuickOpenItem {
                root_id: c.root_id,
                path: c.path,
                display: c.display,
                title: c.title,
                score,
                display_indices,
                title_indices,
            })
        })
        .collect();
    items.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.display.len().cmp(&b.display.len()))
    });
    items.truncate(limit);
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(display: &str, title: Option<&str>) -> Candidate {
        Candidate {
            root_id: 1,
            path: PathBuf::from(display),
            display: display.into(),
            title: title.map(Into::into),
        }
    }

    #[test]
    fn titles_and_paths_both_match() {
        let items = rank(
            "roadmap",
            vec![
                c("docs/plan.md", Some("Product roadmap")),
                c("roadmap-old.md", None),
                c("x.md", None),
            ],
            10,
        );
        assert_eq!(items.len(), 2);
        assert!(
            items
                .iter()
                .any(|i| i.display == "docs/plan.md" && !i.title_indices.is_empty())
        );
    }
}
