use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Tracks the live interactive turn for a session (mirrors ingest `active_ingest_jobs`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveSessionTurn {
    pub turn_id: String,
    pub session_id: String,
    pub stream_url: String,
    pub phase: String,
    pub composer_handoff: bool,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveSessionTurnResponse {
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<ActiveSessionTurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancelActiveSessionTurnResponse {
    pub cancelled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub message: String,
}

pub type ActiveSessionTurnRegistry = Arc<RwLock<HashMap<String, ActiveSessionTurn>>>;

pub fn new_registry() -> ActiveSessionTurnRegistry {
    Arc::new(RwLock::new(HashMap::new()))
}

pub async fn register_active_turn(
    registry: &ActiveSessionTurnRegistry,
    session_id: &str,
    turn_id: &str,
    stream_url: &str,
) {
    registry.write().await.insert(
        session_id.to_string(),
        ActiveSessionTurn {
            turn_id: turn_id.to_string(),
            session_id: session_id.to_string(),
            stream_url: stream_url.to_string(),
            phase: "streaming".to_string(),
            composer_handoff: false,
            started_at: Utc::now(),
        },
    );
}

pub async fn note_stream_event(
    registry: &ActiveSessionTurnRegistry,
    turn_id: &str,
    event_type: &str,
    phase: &str,
) {
    let mut guard = registry.write().await;
    for entry in guard.values_mut() {
        if entry.turn_id != turn_id {
            continue;
        }
        entry.phase = phase.to_string();
        if matches!(event_type, "worker_ack" | "budget_approval") {
            entry.composer_handoff = true;
        }
        break;
    }
}

pub async fn clear_active_turn(registry: &ActiveSessionTurnRegistry, session_id: &str) {
    registry.write().await.remove(session_id);
}

pub async fn clear_active_turn_by_turn_id(registry: &ActiveSessionTurnRegistry, turn_id: &str) {
    let mut guard = registry.write().await;
    guard.retain(|_, entry| entry.turn_id != turn_id);
}

pub async fn get_active_turn(
    registry: &ActiveSessionTurnRegistry,
    session_id: &str,
) -> ActiveSessionTurnResponse {
    let guard = registry.read().await;
    match guard.get(session_id) {
        Some(turn) => ActiveSessionTurnResponse {
            active: true,
            turn: Some(turn.clone()),
        },
        None => ActiveSessionTurnResponse {
            active: false,
            turn: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_get_and_clear_active_turn() {
        let registry = new_registry();
        register_active_turn(
            &registry,
            "session-a",
            "turn-1",
            "http://127.0.0.1/v1/interactive/turn/turn-1/stream",
        )
        .await;

        let active = get_active_turn(&registry, "session-a").await;
        assert!(active.active);
        let turn = active.turn.expect("turn");
        assert_eq!(turn.turn_id, "turn-1");
        assert_eq!(turn.phase, "streaming");
        assert!(!turn.composer_handoff);

        note_stream_event(&registry, "turn-1", "worker_ack", "worker_ack").await;
        let active = get_active_turn(&registry, "session-a").await;
        let turn = active.turn.expect("turn");
        assert_eq!(turn.phase, "worker_ack");
        assert!(turn.composer_handoff);

        clear_active_turn_by_turn_id(&registry, "turn-1").await;
        let inactive = get_active_turn(&registry, "session-a").await;
        assert!(!inactive.active);
    }
}
