//! Purpose-built vault search index port (H07.3).

use std::cmp::Ordering as CmpOrdering;
use std::collections::{BinaryHeap, HashMap};

use crate::vault::note::VaultIndexEntry;

#[derive(Debug, Clone, Default)]
pub struct VaultSearchIndex {
    pub indexed_generation: u64,
    /// term -> (path -> term frequency)
    postings: HashMap<String, HashMap<String, u32>>,
    docs: HashMap<String, SearchDocument>,
    stale: bool,
}

#[derive(Debug, Clone)]
struct SearchDocument {
    length: u32,
    title: String,
    /// Terms owned by this document for O(terms) removal.
    terms: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub path: String,
    pub score: f32,
    pub title: String,
}

impl VaultSearchIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_stale(&self) -> bool {
        self.stale
    }

    pub fn docs_is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    pub fn clear_stale(&mut self) {
        self.stale = false;
    }

    pub fn remove_document(&mut self, path: &str) {
        if let Some(doc) = self.docs.remove(path) {
            for term in &doc.terms {
                if let Some(postings) = self.postings.get_mut(term) {
                    postings.remove(path);
                    if postings.is_empty() {
                        self.postings.remove(term);
                    }
                }
            }
        }
    }

    pub fn upsert_document(&mut self, entry: &VaultIndexEntry, body: &str, generation: u64) {
        self.remove_document(&entry.path);
        let mut tf: HashMap<String, u32> = HashMap::new();
        for token in tokenize(&format!("{} {}", entry.title, body)) {
            *tf.entry(token).or_default() += 1;
        }
        let length = tf.values().sum::<u32>().max(1);
        let terms: Vec<String> = tf.keys().cloned().collect();
        for (term, count) in &tf {
            self.postings
                .entry(term.clone())
                .or_default()
                .insert(entry.path.clone(), *count);
        }
        self.docs.insert(
            entry.path.clone(),
            SearchDocument {
                length,
                title: entry.title.clone(),
                terms,
            },
        );
        self.indexed_generation = generation;
        self.stale = false;
    }

    pub fn search(&self, query: &str, limit: usize) -> (Vec<SearchHit>, bool) {
        let terms = tokenize(query);
        if terms.is_empty() {
            return (Vec::new(), self.stale);
        }
        let mut scores: HashMap<String, f32> = HashMap::new();
        for term in &terms {
            if let Some(postings) = self.postings.get(term) {
                let df = postings.len().max(1) as f32;
                let idf = (1.0 + (self.docs.len().max(1) as f32 / df)).ln();
                for (path, tf) in postings {
                    if let Some(doc) = self.docs.get(path) {
                        let tf_norm = *tf as f32 / doc.length as f32;
                        *scores.entry(path.clone()).or_default() += tf_norm * idf;
                    }
                }
            }
        }
        let mut heap = BinaryHeap::new();
        for (path, score) in scores {
            heap.push(Scored { path, score });
        }
        let mut hits = Vec::with_capacity(limit.min(heap.len()));
        while hits.len() < limit {
            let Some(item) = heap.pop() else { break };
            let title = self
                .docs
                .get(&item.path)
                .map(|doc| doc.title.clone())
                .unwrap_or_default();
            hits.push(SearchHit {
                path: item.path,
                score: item.score,
                title,
            });
        }
        (hits, self.stale)
    }

    pub fn rebuild_from<'a>(
        &mut self,
        entries: impl IntoIterator<Item = (&'a VaultIndexEntry, &'a str)>,
        generation: u64,
    ) {
        self.postings.clear();
        self.docs.clear();
        for (entry, body) in entries {
            self.upsert_document(entry, body, generation);
        }
        self.indexed_generation = generation;
        self.stale = false;
    }
}

#[derive(Debug, Clone)]
struct Scored {
    path: String,
    score: f32,
}

impl PartialEq for Scored {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score && self.path == other.path
    }
}

impl Eq for Scored {}

impl PartialOrd for Scored {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for Scored {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        self.score
            .total_cmp(&other.score)
            .then_with(|| other.path.cmp(&self.path))
    }
}

pub fn tokenize(input: &str) -> Vec<String> {
    input
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| token.len() >= 2)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::note::VaultNoteSource;
    use chrono::Utc;

    #[test]
    fn search_uses_postings_not_full_corpus_scan_api() {
        let mut index = VaultSearchIndex::new();
        let entry = VaultIndexEntry {
            path: "a.md".into(),
            title: "Alpha".into(),
            byte_size: 10,
            content_hash: "sha256:a".into(),
            modified_at_utc: Utc::now(),
            created_at_utc: Utc::now(),
            tags: Vec::new(),
            wikilinks_out: Vec::new(),
            kind: None,
            source: VaultNoteSource::User,
        };
        index.upsert_document(&entry, "unique token medousa vault", 1);
        let (hits, stale) = index.search("unique medousa", 5);
        assert!(!stale);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "a.md");
    }
}
