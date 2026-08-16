//! Ranked vault full-text search via the H07.3 search-index port.

use crate::daemon_api::{VaultNoteSummary, VaultSearchHit, VaultSearchResponse};
use crate::vault::store::{vault_search_index, vault_store, PROJECTION};

pub fn search_vault(query: &str, limit: usize) -> anyhow::Result<VaultSearchResponse> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(VaultSearchResponse {
            query: String::new(),
            hits: Vec::new(),
        });
    }

    let store = vault_store();
    let _ = store.ensure_index_fresh();

    let mut index = vault_search_index()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if index.is_stale() || index.docs_is_empty() {
        let entries = store.all_entries();
        let mut bodies = Vec::with_capacity(entries.len());
        for entry in &entries {
            let body = store.read_content(&entry.path).unwrap_or_default();
            bodies.push((entry.clone(), body));
        }
        let generation = PROJECTION.snapshot().generation.max(1);
        index.rebuild_from(
            bodies
                .iter()
                .map(|(entry, body)| (entry, body.as_str())),
            generation,
        );
    }

    let (hits, _stale) = index.search(query, limit);
    let response_hits = hits
        .into_iter()
        .filter_map(|hit| {
            let entry = store.peek_entry_public(&hit.path)?;
            Some(VaultSearchHit {
                note: VaultNoteSummary {
                    path: entry.path.clone(),
                    title: entry.title.clone(),
                    modified_at_utc: entry.modified_at_utc,
                    kind: entry
                        .kind
                        .clone()
                        .unwrap_or_else(|| crate::vault::note::resolve_kind_from_path(&entry.path)),
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
