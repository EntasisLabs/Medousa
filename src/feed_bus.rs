//! Feed bus — publish, subscribe map, component patch fan-out.

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use chrono::Utc;
use medousa_types::environment::EnvironmentStreamEvent;
use medousa_types::feed::{
    ComponentFeedPatch, FeedEvent, FeedRef, FeedSource, DEFAULT_FEED_PAYLOAD_MAX_BYTES,
    WORKSHOP_PULSE_FEED_ID,
};
use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::environment_store::{environment_hub, resolve_profile_id};
use crate::feed_store::{feed_store, new_feed_event_id};

#[derive(Debug, Clone)]
pub struct FeedPublishRequest {
    pub profile_id: Option<String>,
    pub feed_id: String,
    pub source: FeedSource,
    pub summary: String,
    pub refs: Vec<FeedRef>,
    pub payload_slice: Option<Value>,
    pub payload_max_bytes: Option<usize>,
}

impl FeedPublishRequest {
    pub fn workshop_pulse(summary: impl Into<String>, payload: Value, refs: Vec<FeedRef>) -> Self {
        Self {
            profile_id: None,
            feed_id: WORKSHOP_PULSE_FEED_ID.to_string(),
            source: FeedSource::BoundWorkshop,
            summary: summary.into(),
            refs,
            payload_slice: Some(payload),
            payload_max_bytes: None,
        }
    }
}

pub fn feed_hub() -> &'static FeedHub {
    static HUB: std::sync::OnceLock<FeedHub> = std::sync::OnceLock::new();
    HUB.get_or_init(FeedHub::new)
}

pub struct FeedHub {
    tx: broadcast::Sender<medousa_types::feed::FeedStreamEvent>,
    component_state: tokio::sync::RwLock<HashMap<String, HashMap<String, Value>>>,
    pub patch_seq: std::sync::atomic::AtomicU64,
}

impl FeedHub {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(128);
        Self {
            tx,
            component_state: tokio::sync::RwLock::new(HashMap::new()),
            patch_seq: std::sync::atomic::AtomicU64::new(1),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<medousa_types::feed::FeedStreamEvent> {
        self.tx.subscribe()
    }

    pub async fn subscribers_for_feed(
        profile_id: &str,
        feed_id: &str,
    ) -> Vec<(String, Value)> {
        let env = environment_hub().get(profile_id).await.ok();
        let Some(record) = env else {
            return Vec::new();
        };
        record
            .spec
            .components
            .iter()
            .filter(|component| component.feeds.iter().any(|id| id == feed_id))
            .map(|component| (component.id.clone(), component.config.clone()))
            .collect()
    }

    fn merge_component_patch(existing: &Value, feed_id: &str, slice: &Value) -> Value {
        let mut merged = if existing.is_object() {
            existing.clone()
        } else {
            json!({})
        };
        if let Some(obj) = merged.as_object_mut() {
            obj.insert("feedId".to_string(), Value::String(feed_id.to_string()));
            obj.insert("lastPatch".to_string(), slice.clone());
            if let Some(feeds) = obj.get_mut("feeds") {
                if let Some(feeds_obj) = feeds.as_object_mut() {
                    feeds_obj.insert(feed_id.to_string(), slice.clone());
                } else {
                    obj.insert("feeds".to_string(), json!({ feed_id: slice.clone() }));
                }
            } else {
                obj.insert("feeds".to_string(), json!({ feed_id: slice.clone() }));
            }
        }
        merged
    }

    async fn component_runtime_state(&self, profile_id: &str, component_id: &str) -> Value {
        let guard = self.component_state.read().await;
        guard
            .get(profile_id)
            .and_then(|profile| profile.get(component_id))
            .cloned()
            .unwrap_or_else(|| json!({}))
    }

    async fn set_component_runtime_state(
        &self,
        profile_id: &str,
        component_id: &str,
        state: Value,
    ) {
        let mut guard = self.component_state.write().await;
        guard
            .entry(profile_id.to_string())
            .or_default()
            .insert(component_id.to_string(), state);
    }
}

pub async fn publish(request: FeedPublishRequest) -> Result<FeedEvent> {
    let profile_id = resolve_profile_id(request.profile_id.as_deref());
    let feed_id = request.feed_id.trim().to_string();
    if feed_id.is_empty() {
        return Err(anyhow!("feed_id is required"));
    }
    if !medousa_types::feed::is_valid_feed_id(&feed_id) {
        return Err(anyhow!("invalid feed_id '{feed_id}'"));
    }

    let max_bytes = request
        .payload_max_bytes
        .unwrap_or(DEFAULT_FEED_PAYLOAD_MAX_BYTES);
    let payload = if let Some(slice) = request.payload_slice {
        let encoded = serde_json::to_vec(&slice)?;
        if encoded.len() > max_bytes {
            return Err(anyhow!(
                "payload_slice exceeds max bytes ({}/{max_bytes})",
                encoded.len()
            ));
        }
        Some(slice)
    } else {
        None
    };

    let mut event = FeedEvent {
        id: String::new(),
        feed_id: feed_id.clone(),
        emitted_at_utc: Utc::now(),
        source: request.source.as_str().to_string(),
        summary: request.summary.clone(),
        refs: request.refs.clone(),
        payload: payload.clone(),
    };

    let seq = feed_store()
        .append(&profile_id, &feed_id, event.clone())
        .await?;
    event.id = new_feed_event_id(seq);

    let subscribers = FeedHub::subscribers_for_feed(&profile_id, &feed_id).await;
    let hub = feed_hub();
    let patch_seq_start = hub
        .patch_seq
        .fetch_add(subscribers.len().max(1) as u64, std::sync::atomic::Ordering::SeqCst);

    let slice = event.payload.clone().unwrap_or_else(|| {
        json!({
            "summary": event.summary,
            "source": event.source,
            "emittedAt": event.emitted_at_utc.to_rfc3339(),
        })
    });

    let mut patches = Vec::new();
    for (index, (component_id, _config)) in subscribers.iter().enumerate() {
        let prior = hub
            .component_runtime_state(&profile_id, component_id)
            .await;
        let merged = FeedHub::merge_component_patch(&prior, &feed_id, &slice);
        hub.set_component_runtime_state(&profile_id, component_id, merged.clone())
            .await;
        patches.push(ComponentFeedPatch {
            component_id: component_id.clone(),
            feed_id: feed_id.clone(),
            patch: merged,
            seq: patch_seq_start + index as u64,
        });
    }

    let stream_event = medousa_types::feed::FeedStreamEvent {
        seq,
        event_type: if patches.is_empty() {
            "feed_appended".to_string()
        } else {
            "component_patch".to_string()
        },
        emitted_at_utc: event.emitted_at_utc,
        feed_event: Some(event.clone()),
        component_patches: if patches.is_empty() {
            None
        } else {
            Some(patches.clone())
        },
    };
    let _ = hub.tx.send(stream_event);

    environment_hub()
        .emit_stream_event(EnvironmentStreamEvent {
            revision: patch_seq_start,
            event_type: "component_patch".to_string(),
            emitted_at_utc: event.emitted_at_utc,
            spec: None,
            component_patches: if patches.is_empty() {
                None
            } else {
                Some(patches)
            },
            feed_event: Some(event.clone()),
            runtime_probe: None,
        })
        .await;

    Ok(event)
}

pub async fn list_feeds(profile_id: Option<&str>) -> medousa_types::feed::FeedListResponse {
    let profile_id = resolve_profile_id(profile_id);
    let mut feed_ids: HashSet<String> = feed_store()
        .list_feed_ids(&profile_id)
        .await
        .into_iter()
        .collect();

    if let Ok(record) = environment_hub().get(&profile_id).await {
        for component in &record.spec.components {
            for feed_id in &component.feeds {
                feed_ids.insert(feed_id.clone());
            }
        }
    }

    let mut feeds = Vec::new();
    for feed_id in feed_ids {
        let subscribers = FeedHub::subscribers_for_feed(&profile_id, &feed_id).await;
        let event_count = feed_store().event_count(&profile_id, &feed_id).await;
        feeds.push(medousa_types::feed::FeedListEntry {
            feed_id,
            event_count,
            subscriber_component_ids: subscribers.into_iter().map(|(id, _)| id).collect(),
        });
    }
    feeds.sort_by(|a, b| a.feed_id.cmp(&b.feed_id));
    medousa_types::feed::FeedListResponse { feeds }
}

pub async fn component_feed_state(profile_id: Option<&str>, component_id: &str) -> Value {
    let profile_id = resolve_profile_id(profile_id);
    feed_hub()
        .component_runtime_state(&profile_id, component_id)
        .await
}
