//! Persistent feed event log per profile.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use medousa_types::feed::FeedEvent;
use tokio::sync::RwLock as AsyncRwLock;

const STORE_DIR: &str = "feeds";
const MAX_EVENTS_PER_FEED: usize = 200;

#[derive(Debug, Clone, Default)]
struct FeedChannelState {
    events: Vec<FeedEvent>,
    next_seq: u64,
    read_cursor: u64,
}

#[derive(Clone)]
pub struct FeedStore {
    inner: Arc<AsyncRwLock<HashMap<String, HashMap<String, FeedChannelState>>>>,
}

impl FeedStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AsyncRwLock::new(HashMap::new())),
        }
    }

    fn store_root() -> PathBuf {
        crate::paths::medousa_data_dir().join(STORE_DIR)
    }

    fn feed_path(profile_id: &str, feed_id: &str) -> PathBuf {
        Self::store_root()
            .join(profile_id)
            .join(format!("{feed_id}.jsonl"))
    }

    async fn load_feed_channel(profile_id: &str, feed_id: &str) -> FeedChannelState {
        let path = Self::feed_path(profile_id, feed_id);
        if !path.exists() {
            return FeedChannelState::default();
        }
        let raw = match tokio::fs::read_to_string(&path).await {
            Ok(content) => content,
            Err(_) => return FeedChannelState::default(),
        };
        let mut state = FeedChannelState::default();
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(event) = serde_json::from_str::<FeedEvent>(trimmed) {
                state.next_seq = state.next_seq.max(event.id.parse::<u64>().unwrap_or(0) + 1);
                state.events.push(event);
            }
        }
        if state.events.len() > MAX_EVENTS_PER_FEED {
            let drop = state.events.len() - MAX_EVENTS_PER_FEED;
            state.events.drain(0..drop);
        }
        state
    }

    async fn persist_feed_channel(
        profile_id: &str,
        feed_id: &str,
        state: &FeedChannelState,
    ) -> Result<()> {
        let path = Self::feed_path(profile_id, feed_id);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut body = String::new();
        for event in &state.events {
            body.push_str(&serde_json::to_string(event)?);
            body.push('\n');
        }
        tokio::fs::write(&path, body)
            .await
            .with_context(|| format!("write feed log {}", path.display()))?;
        Ok(())
    }

    pub async fn append(&self, profile_id: &str, feed_id: &str, mut event: FeedEvent) -> Result<u64> {
        let mut guard = self.inner.write().await;
        let profile = guard.entry(profile_id.to_string()).or_default();
        if !profile.contains_key(feed_id) {
            profile.insert(
                feed_id.to_string(),
                Self::load_feed_channel(profile_id, feed_id).await,
            );
        }
        let channel = profile.get_mut(feed_id).expect("feed channel");
        let seq = channel.next_seq;
        channel.next_seq += 1;
        event.id = super::feed_store::new_feed_event_id(seq);
        channel.events.push(event);
        if channel.events.len() > MAX_EVENTS_PER_FEED {
            channel.events.remove(0);
        }
        let snapshot = channel.clone();
        drop(guard);
        Self::persist_feed_channel(profile_id, feed_id, &snapshot).await?;
        Ok(seq)
    }

    pub async fn tail(&self, profile_id: &str, feed_id: &str, limit: usize) -> Vec<FeedEvent> {
        let mut guard = self.inner.write().await;
        let profile = guard.entry(profile_id.to_string()).or_default();
        if !profile.contains_key(feed_id) {
            profile.insert(
                feed_id.to_string(),
                Self::load_feed_channel(profile_id, feed_id).await,
            );
        }
        let channel = profile.get(feed_id).expect("feed channel");
        let start = channel.events.len().saturating_sub(limit);
        channel.events[start..].to_vec()
    }

    pub async fn list_feed_ids(&self, profile_id: &str) -> Vec<String> {
        let mut ids = Vec::new();
        let root = Self::store_root().join(profile_id);
        if root.exists() {
            if let Ok(mut entries) = tokio::fs::read_dir(&root).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if let Some(feed_id) = name.strip_suffix(".jsonl") {
                        ids.push(feed_id.to_string());
                    }
                }
            }
        }
        let guard = self.inner.read().await;
        if let Some(profile) = guard.get(profile_id) {
            for feed_id in profile.keys() {
                if !ids.iter().any(|existing| existing == feed_id) {
                    ids.push(feed_id.clone());
                }
            }
        }
        ids.sort();
        ids
    }

    pub async fn event_count(&self, profile_id: &str, feed_id: &str) -> u64 {
        let events = self.tail(profile_id, feed_id, MAX_EVENTS_PER_FEED).await;
        events.len() as u64
    }

    pub async fn set_read_cursor(&self, profile_id: &str, feed_id: &str, seq: u64) {
        let mut guard = self.inner.write().await;
        let profile = guard.entry(profile_id.to_string()).or_default();
        if !profile.contains_key(feed_id) {
            profile.insert(
                feed_id.to_string(),
                Self::load_feed_channel(profile_id, feed_id).await,
            );
        }
        if let Some(channel) = profile.get_mut(feed_id) {
            channel.read_cursor = seq;
        }
    }
}

static FEED_STORE: std::sync::OnceLock<FeedStore> = std::sync::OnceLock::new();

pub fn feed_store() -> &'static FeedStore {
    FEED_STORE.get_or_init(FeedStore::new)
}

pub fn new_feed_event_id(seq: u64) -> String {
    format!("feed-{seq}")
}

pub fn ensure_store_dir() -> Result<()> {
    std::fs::create_dir_all(FeedStore::store_root())?;
    Ok(())
}
