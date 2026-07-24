//! Persistent feed event log per profile.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use medousa_types::feed::{FeedEvent, FeedLatestGoodResponse};
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

impl Default for FeedStore {
    fn default() -> Self {
        Self::new()
    }
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
        if root.exists()
            && let Ok(mut entries) = tokio::fs::read_dir(&root).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if let Some(feed_id) = name.strip_suffix(".jsonl") {
                        ids.push(feed_id.to_string());
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

    pub async fn latest_good(
        &self,
        profile_id: &str,
        feed_id: &str,
    ) -> Option<FeedLatestGoodResponse> {
        let events = self
            .tail(profile_id, feed_id, MAX_EVENTS_PER_FEED)
            .await;
        for event in events.iter().rev() {
            if let Some(result) = extract_latest_good(event) {
                return Some(FeedLatestGoodResponse {
                    feed_id: feed_id.to_string(),
                    datatype: result.datatype,
                    body: result.body,
                    job_id: result.job_id,
                    finished_at: result.finished_at,
                });
            }
        }
        None
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

#[derive(Debug, Clone)]
struct LatestGoodExtract {
    datatype: String,
    body: String,
    job_id: Option<String>,
    finished_at: Option<String>,
}

fn extract_latest_good(event: &FeedEvent) -> Option<LatestGoodExtract> {
    let payload = event.payload.as_ref()?;
    if !is_success_payload(payload) {
        return None;
    }

    let body = payload
        .get("body")
        .and_then(|value| value.as_str())
        .or_else(|| payload.get("excerpt").and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();

    let datatype = payload
        .get("datatype")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| infer_feed_datatype(&body));

    let job_id = payload
        .get("jobId")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            event
                .refs
                .iter()
                .find(|reference| reference.ref_type == "job")
                .map(|reference| reference.ref_id.clone())
        });

    let finished_at = payload
        .get("finishedAt")
        .or_else(|| payload.get("checkedAt"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| Some(event.emitted_at_utc.to_rfc3339()));

    Some(LatestGoodExtract {
        datatype,
        body,
        job_id,
        finished_at,
    })
}

fn is_success_payload(payload: &serde_json::Value) -> bool {
    let phase = payload
        .get("phase")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    match phase {
        "tick_succeeded" | "synthesis" => true,
        "started" | "working" | "wrapping_up" => false,
        _ => payload
            .get("status")
            .and_then(|value| value.as_str())
            .is_some_and(|status| status == "done"),
    }
}

fn infer_feed_datatype(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "text".to_string();
    }
    if trimmed.starts_with("data:image/")
        || looks_like_image_ref(trimmed)
    {
        return "image".to_string();
    }
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        return "json".to_string();
    }
    if looks_like_csv(trimmed) {
        return "csv".to_string();
    }
    if trimmed.contains("# ")
        || trimmed.contains("**")
        || trimmed.contains("\n- ")
        || trimmed.contains("\n* ")
    {
        return "md".to_string();
    }
    "text".to_string()
}

fn looks_like_image_ref(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("vault/")
        || lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".gif")
        || lower.ends_with(".svg")
}

fn looks_like_csv(text: &str) -> bool {
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() < 2 {
        return false;
    }
    let comma_lines = lines
        .iter()
        .filter(|line| line.contains(','))
        .count();
    comma_lines >= 2 && comma_lines * 2 >= lines.len()
}

#[cfg(test)]
mod latest_good_tests {
    use super::*;
    use chrono::Utc;
    use medousa_types::feed::{FeedRef, FeedSource};
    use serde_json::json;

    fn sample_event(payload: serde_json::Value) -> FeedEvent {
        FeedEvent {
            id: "feed-1".to_string(),
            feed_id: "summer-ai-digest".to_string(),
            emitted_at_utc: Utc::now(),
            source: FeedSource::RecurringJob.as_str().to_string(),
            summary: "ok".to_string(),
            refs: vec![FeedRef {
                ref_type: "job".to_string(),
                ref_id: "job-1".to_string(),
            }],
            payload: Some(payload),
        }
    }

    #[test]
    fn extract_latest_good_prefers_body_and_datatype() {
        let event = sample_event(json!({
            "phase": "tick_succeeded",
            "body": "# Digest\nHello",
            "datatype": "md",
            "jobId": "job-1",
            "checkedAt": "2026-07-22T12:00:00Z"
        }));
        let result = extract_latest_good(&event).expect("good");
        assert_eq!(result.datatype, "md");
        assert!(result.body.contains("Digest"));
        assert_eq!(result.job_id.as_deref(), Some("job-1"));
    }

    #[test]
    fn extract_latest_good_skips_failed_ticks() {
        let event = sample_event(json!({
            "phase": "tick_failed",
            "body": "should ignore"
        }));
        assert!(extract_latest_good(&event).is_none());
    }
}
