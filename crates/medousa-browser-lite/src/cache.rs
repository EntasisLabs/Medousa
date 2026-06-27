use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use sha2::{Digest, Sha256};

use crate::search::SearchResponse;

const DEFAULT_TTL: Duration = Duration::from_secs(30 * 60);
const DEFAULT_MAX_ENTRIES: usize = 500;

#[derive(Clone)]
struct CacheEntry {
    response: SearchResponse,
    inserted: Instant,
}

pub struct SearchCache {
    inner: Mutex<HashMap<String, CacheEntry>>,
    ttl: Duration,
    max_entries: usize,
}

impl Default for SearchCache {
    fn default() -> Self {
        Self::new(DEFAULT_TTL, DEFAULT_MAX_ENTRIES)
    }
}

impl SearchCache {
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            ttl,
            max_entries: max_entries.max(1),
        }
    }

    pub fn key(query: &str, max_results: usize) -> String {
        let normalized = query.trim().to_ascii_lowercase();
        format!("{normalized}:{max_results}")
    }

    pub fn get(&self, query: &str, max_results: usize) -> Option<SearchResponse> {
        let key = Self::key(query, max_results);
        let mut guard = self.inner.lock();
        let entry = guard.get(&key)?;
        if entry.inserted.elapsed() > self.ttl {
            guard.remove(&key);
            return None;
        }
        Some(entry.response.clone())
    }

    pub fn put(&self, query: &str, max_results: usize, response: SearchResponse) {
        let key = Self::key(query, max_results);
        let mut guard = self.inner.lock();
        if guard.len() >= self.max_entries {
            if let Some(oldest_key) = guard
                .iter()
                .min_by_key(|(_, v)| v.inserted)
                .map(|(k, _)| k.clone())
            {
                guard.remove(&oldest_key);
            }
        }
        guard.insert(
            key,
            CacheEntry {
                response,
                inserted: Instant::now(),
            },
        );
    }

    pub fn hash_query(query: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(query.trim().to_ascii_lowercase().as_bytes());
        format!("{:x}", hasher.finalize())[..16].to_string()
    }
}
