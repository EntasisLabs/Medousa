//! Per-turn SSE fan-out registry backed by the durable [`TurnEventLog`] spine.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use medousa_engine::{
    Principal, TurnEnvelope, TurnEventLog, TurnStreamRegistryPort, configure_log_root,
    default_log_root,
};
use tokio::sync::RwLock;

use crate::daemon::turn_event_channel::TurnEventChannel;
use crate::paths;

#[derive(Clone)]
pub struct TurnStreamEntry {
    pub channel: Arc<TurnEventChannel>,
    pub log: Arc<TurnEventLog>,
}

pub type TurnStreamRegistry =
    Arc<RwLock<HashMap<String, TurnStreamEntry>>>;

pub fn new_turn_stream_registry() -> TurnStreamRegistry {
    Arc::new(RwLock::new(HashMap::new()))
}

fn ensure_log_root() {
    let root = paths::medousa_data_dir().join(medousa_engine::TURN_LOG_DIR);
    configure_log_root(root);
}

#[derive(Clone)]
pub struct TurnStreamRegistryPortAdapter {
    registry: TurnStreamRegistry,
}

impl TurnStreamRegistryPortAdapter {
    pub fn new(registry: TurnStreamRegistry) -> Self {
        Self { registry }
    }

    pub fn registry(&self) -> TurnStreamRegistry {
        self.registry.clone()
    }
}

#[async_trait]
impl TurnStreamRegistryPort for TurnStreamRegistryPortAdapter {
    async fn register_stream(&self, turn_id: &str) -> bool {
        ensure_log_root();
        let mut guard = self.registry.write().await;
        if guard.contains_key(turn_id) {
            return false;
        }
        let envelope = TurnEnvelope::new(turn_id, Principal::operator());
        let log = match TurnEventLog::open_in(default_log_root(), envelope) {
            Ok(log) => Arc::new(log),
            Err(err) => {
                tracing::warn!(turn_id, error = %err, "failed to open turn event log");
                return false;
            }
        };
        guard.insert(
            turn_id.to_string(),
            TurnStreamEntry {
                channel: TurnEventChannel::new(512),
                log,
            },
        );
        true
    }

    async fn drop_stream(&self, turn_id: &str) {
        self.registry.write().await.remove(turn_id);
    }

    async fn has_stream(&self, turn_id: &str) -> bool {
        self.registry.read().await.contains_key(turn_id)
    }

    async fn event_log(&self, turn_id: &str) -> Option<Arc<TurnEventLog>> {
        self.registry
            .read()
            .await
            .get(turn_id)
            .map(|entry| entry.log.clone())
    }

    async fn mark_stream_closed(&self, turn_id: &str) {
        if let Some(entry) = self.registry.read().await.get(turn_id) {
            entry.channel.mark_closed();
        }
    }
}

pub async fn turn_stream_channel(
    registry: &TurnStreamRegistry,
    turn_id: &str,
) -> Option<Arc<TurnEventChannel>> {
    registry
        .read()
        .await
        .get(turn_id)
        .map(|entry| entry.channel.clone())
}

pub async fn turn_stream_log(
    registry: &TurnStreamRegistry,
    turn_id: &str,
) -> Option<Arc<TurnEventLog>> {
    registry
        .read()
        .await
        .get(turn_id)
        .map(|entry| entry.log.clone())
}
