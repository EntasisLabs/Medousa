//! Ranked vault full-text search via the H07.3 search-index port.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::daemon_api::{VaultNoteSummary, VaultSearchHit, VaultSearchResponse};
use crate::vault::io::{VaultIoClass, vault_io};
use crate::vault::store::{PROJECTION, vault_search_index, vault_store};

static SEARCH_REBUILD_SCHEDULED: AtomicBool = AtomicBool::new(false);

fn rebuild_search_index_sync() {
    let store = vault_store();
    let _ = store.ensure_index_fresh();
    // Use peek-only path: avoid nested ensure + clone via all_entries' sort.
    let entries = store.peek_all_entries();
    let mut bodies = Vec::with_capacity(entries.len());
    for entry in &entries {
        let body = store.read_content(&entry.path).unwrap_or_default();
        bodies.push((entry.clone(), body));
    }
    let generation = PROJECTION.snapshot().generation.max(1);
    let mut index = vault_search_index()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    index.rebuild_from(
        bodies.iter().map(|(entry, body)| (entry, body.as_str())),
        generation,
    );
    SEARCH_REBUILD_SCHEDULED.store(false, Ordering::Release);
}

fn schedule_search_rebuild() {
    if SEARCH_REBUILD_SCHEDULED.swap(true, Ordering::AcqRel) {
        return;
    }
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async {
            let result = vault_io()
                .run(VaultIoClass::SearchRebuild, || {
                    rebuild_search_index_sync();
                    Ok(())
                })
                .await;
            if result.is_err() {
                SEARCH_REBUILD_SCHEDULED.store(false, Ordering::Release);
            }
        });
    } else {
        // Unit tests / sync callers: rebuild inline once.
        rebuild_search_index_sync();
    }
}

pub fn search_vault(query: &str, limit: usize) -> anyhow::Result<VaultSearchResponse> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(VaultSearchResponse {
            query: String::new(),
            hits: Vec::new(),
            indexing: None,
        });
    }

    let store = vault_store();
    let _ = store.ensure_index_fresh();

    let mut indexing = false;
    {
        let index = vault_search_index()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if index.docs_is_empty() {
            drop(index);
            // Cold start: must populate before answering when no runtime job
            // is available; with a runtime, schedule and report indexing.
            if tokio::runtime::Handle::try_current().is_ok() {
                schedule_search_rebuild();
                indexing = true;
            } else {
                rebuild_search_index_sync();
            }
        } else if index.is_stale() {
            indexing = true;
            schedule_search_rebuild();
            // Serve last warm postings while rebuild runs — never rescan
            // every body on the request path.
        }
    }

    let index = vault_search_index()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (hits, stale) = index.search(query, limit);
    indexing = indexing || stale;
    let response_hits =
        hits.into_iter()
            .filter_map(|hit| {
                let entry = store.peek_entry_public(&hit.path)?;
                Some(VaultSearchHit {
                    note: VaultNoteSummary {
                        path: entry.path.clone(),
                        title: entry.title.clone(),
                        modified_at_utc: entry.modified_at_utc,
                        kind: entry.kind.clone().unwrap_or_else(|| {
                            crate::vault::note::resolve_kind_from_path(&entry.path)
                        }),
                        tags: entry.tags.clone(),
                    },
                    score: hit.score.clamp(0.0, 1.0),
                    matched_terms: crate::vault::search_index::tokenize(query),
                    snippet: Some(hit.title),
                })
            })
            .collect();

    Ok(VaultSearchResponse {
        query: query.to_string(),
        hits: response_hits,
        indexing: indexing.then_some(true),
    })
}

#[cfg(test)]
mod tests {
    use crate::vault::search_index::tokenize;

    #[test]
    fn tokenize_strips_punctuation() {
        let terms = tokenize("medousa, vault!");
        assert!(terms.contains(&"medousa".to_string()));
        assert!(terms.contains(&"vault".to_string()));
    }
}
