//! Ranked vault full-text search (V0 heuristic).

use chrono::Utc;

use crate::daemon_api::{VaultNoteSummary, VaultSearchHit, VaultSearchResponse};
use crate::vault::note::{extract_title, strip_frontmatter};
use crate::vault::store::vault_store;

const TITLE_WEIGHT: f32 = 0.40;
const HEADING_WEIGHT: f32 = 0.30;
const BODY_WEIGHT: f32 = 0.20;
const RECENCY_WEIGHT: f32 = 0.10;
const PHRASE_BOOST: f32 = 0.20;

pub fn search_vault(query: &str, limit: usize) -> anyhow::Result<VaultSearchResponse> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(VaultSearchResponse {
            query: String::new(),
            hits: Vec::new(),
        });
    }

    let terms = tokenize(query);
    if terms.is_empty() {
        return Ok(VaultSearchResponse {
            query: query.to_string(),
            hits: Vec::new(),
        });
    }

    let store = vault_store();
    let entries = store.all_entries();
    let newest = entries
        .iter()
        .map(|entry| entry.modified_at_utc)
        .max()
        .unwrap_or_else(Utc::now);

    let mut hits = Vec::new();
    for entry in entries {
        let title = entry.title.to_ascii_lowercase();
        let filename = entry
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&entry.path)
            .to_ascii_lowercase();
        let mut matched_terms = Vec::new();
        let mut score = 0.0f32;

        for term in &terms {
            if title.contains(term) || filename.contains(term) {
                score += TITLE_WEIGHT / terms.len() as f32;
                matched_terms.push(term.clone());
            }
        }

        let body = match store.read_content(&entry.path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let (content, _) = strip_frontmatter(&body);
        let heading = extract_title(&body, &entry.path).to_ascii_lowercase();
        for term in &terms {
            if heading.contains(term) && !matched_terms.iter().any(|value| value == term) {
                score += HEADING_WEIGHT / terms.len() as f32;
                matched_terms.push(term.clone());
            }
        }

        let content_lower = content.to_ascii_lowercase();
        let mut body_hits = 0usize;
        for term in &terms {
            if content_lower.contains(term) {
                body_hits += 1;
                if !matched_terms.iter().any(|value| value == term) {
                    matched_terms.push(term.clone());
                }
            }
        }
        if body_hits > 0 {
            score += BODY_WEIGHT * (body_hits as f32 / terms.len() as f32);
        }

        if content_lower.contains(&query.to_ascii_lowercase()) {
            score += PHRASE_BOOST;
        }

        if matched_terms.is_empty() {
            continue;
        }

        let age_hours = newest
            .signed_duration_since(entry.modified_at_utc)
            .num_hours()
            .max(0) as f32;
        let recency = 1.0 / (1.0 + age_hours / 24.0);
        score += RECENCY_WEIGHT * recency;
        score = score.clamp(0.0, 1.0);

        let snippet = snippet_for_terms(content, &terms);
        hits.push(VaultSearchHit {
            note: VaultNoteSummary {
                path: entry.path.clone(),
                title: entry.title.clone(),
                modified_at_utc: entry.modified_at_utc,
            },
            score,
            matched_terms,
            snippet,
        });
    }

    hits.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);

    Ok(VaultSearchResponse {
        query: query.to_string(),
        hits,
    })
}

fn tokenize(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|term| term.trim_matches(|ch: char| !ch.is_alphanumeric()).to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect()
}

fn snippet_for_terms(content: &str, terms: &[String]) -> Option<String> {
    for line in content.lines() {
        let lower = line.to_ascii_lowercase();
        if terms.iter().any(|term| lower.contains(term)) {
            let trimmed = line.trim();
            if trimmed.chars().count() > 180 {
                return Some(format!("{}…", trimmed.chars().take(180).collect::<String>()));
            }
            return Some(trimmed.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_strips_punctuation() {
        let terms = tokenize("medousa, vault!");
        assert_eq!(terms, vec!["medousa", "vault"]);
    }
}
