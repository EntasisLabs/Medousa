//! Lightweight Forge freshness bus for Home SSE / invalidation.

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize)]
pub struct ForgeStreamEvent {
    pub work_id: String,
    pub state: String,
    pub event_kind: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct ForgeEventBus {
    tx: broadcast::Sender<ForgeStreamEvent>,
}

impl Default for ForgeEventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl ForgeEventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx }
    }

    pub fn publish(&self, work_id: &str, state: &str, event_kind: &str) {
        let _ = self.tx.send(ForgeStreamEvent {
            work_id: work_id.to_owned(),
            state: state.to_owned(),
            event_kind: event_kind.to_owned(),
            updated_at: Utc::now(),
        });
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ForgeStreamEvent> {
        self.tx.subscribe()
    }
}
