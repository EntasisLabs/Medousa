//! Daemon adapters implementing engine port traits over existing registries.

use std::sync::Arc;

use async_trait::async_trait;
use medousa_engine::{
    ToolSinkEvent, ToolSinkPort, TurnStorePort, TurnTicketPort, UpsertOutcome, StoreError,
};
use medousa_types::session::ConversationTurn;
use tokio::sync::RwLock;

use crate::daemon::turn_stream_registry::{
    TurnStreamRegistry, TurnStreamRegistryPortAdapter,
};
use crate::engine_recovery::{
    mark_recovery_ledger, recovery_ledger_contains,
};

/// Local newtype so the engine port can be implemented without orphan-rule issues.
pub struct TurnTicketPortAdapter(pub crate::turn_ticket::TurnTicketRegistry);

#[async_trait]
impl TurnTicketPort for TurnTicketPortAdapter {
    async fn register(&self, ticket: medousa_types::turn_ticket::TurnTicket) -> Result<(), medousa_types::turn_ticket::TurnTicketConflict> {
        crate::turn_ticket::register_turn(&self.0, ticket).await
    }
    async fn note_event(&self, turn_id: &str, event_type: &str, terminal: bool) {
        crate::turn_ticket::note_stream_event(&self.0, turn_id, event_type, event_type, terminal)
            .await
    }
    async fn mark_cancelled(&self, turn_id: &str) {
        crate::turn_ticket::mark_cancelled(&self.0, turn_id).await
    }
    async fn clear(&self, turn_id: &str) {
        crate::turn_ticket::clear_turn(&self.0, turn_id).await
    }
    async fn clear_after_run(&self, turn_id: &str) {
        crate::turn_ticket::clear_turn_after_run(&self.0, turn_id).await
    }
    async fn get(&self, turn_id: &str) -> Option<medousa_types::turn_ticket::TurnTicket> {
        crate::turn_ticket::get_turn(&self.0, turn_id).await
    }
}

/// Idempotent session turn store keyed by turn id (recovery + live persist).
pub struct SessionTurnStore;

#[async_trait]
impl TurnStorePort for SessionTurnStore {
    async fn upsert_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        turn: ConversationTurn,
    ) -> Result<UpsertOutcome, StoreError> {
        if self.turn_exists(session_id, turn_id).await? {
            return Ok(UpsertOutcome::AlreadyPresent);
        }
        crate::session_writer::persist_turn(session_id, turn, None);
        mark_recovery_ledger(session_id, turn_id);
        Ok(UpsertOutcome::Inserted)
    }

    async fn turn_exists(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<bool, StoreError> {
        Ok(recovery_ledger_contains(session_id, turn_id))
    }
}

pub fn turn_stream_registry_adapter(
    registry: TurnStreamRegistry,
) -> TurnStreamRegistryPortAdapter {
    TurnStreamRegistryPortAdapter::new(registry)
}

/// Forwards browser tool events into the active turn's [`AgentStreamSink`].
pub struct AgentStreamToolSinkAdapter {
    inner: medousa_engine::SharedAgentStreamSink,
}

impl AgentStreamToolSinkAdapter {
    pub fn new(inner: medousa_engine::SharedAgentStreamSink) -> Arc<Self> {
        Arc::new(Self { inner })
    }
}

#[async_trait]
impl ToolSinkPort for AgentStreamToolSinkAdapter {
    async fn emit(&self, event: ToolSinkEvent) {
        match event {
            ToolSinkEvent::BrowserChallenge {
                turn_correlation_id,
                session_id,
                challenge_url,
                reason,
            } => {
                self.inner
                    .browser_challenge_required(
                        &turn_correlation_id,
                        session_id,
                        challenge_url,
                        reason,
                    )
                    .await;
            }
            ToolSinkEvent::BrowserNavigated {
                turn_correlation_id,
                url,
                title,
                opened_by_agent,
            } => {
                self.inner
                    .browser_navigated(&turn_correlation_id, url, title, opened_by_agent)
                    .await;
            }
        }
    }
}

/// Per-turn ambient tool sink (replaces the legacy `active_stream_sink` global).
static ACTIVE_TOOL_SINK: once_cell::sync::Lazy<RwLock<Option<Arc<dyn ToolSinkPort + Send + Sync>>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(None));

pub async fn set_active_tool_sink(sink: Option<Arc<dyn ToolSinkPort + Send + Sync>>) {
    *ACTIVE_TOOL_SINK.write().await = sink;
}

pub async fn active_tool_sink() -> Option<Arc<dyn ToolSinkPort + Send + Sync>> {
    ACTIVE_TOOL_SINK.read().await.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_engine::TurnStreamRegistryPort;
    use chrono::Utc;
    use medousa_types::turn_ticket::{TurnTicket, TurnTicketMode, TurnTicketPhase};

    #[tokio::test]
    async fn ticket_port_adapter_enforces_interactive_mutex() {
        let registry = crate::turn_ticket::new_registry();
        let port = TurnTicketPortAdapter(registry);
        let port: &dyn TurnTicketPort = &port;
        let ticket = |id: &str| TurnTicket {
            turn_id: id.to_string(),
            session_id: "s1".to_string(),
            mode: TurnTicketMode::Interactive,
            phase: TurnTicketPhase::Streaming,
            stream_url: "http://localhost/s".to_string(),
            prompt_preview: String::new(),
            workspace_card_id: None,
            started_at: Utc::now(),
            updated_at: Utc::now(),
        };
        port.register(ticket("turn-1")).await.expect("first registers");
        assert!(port.register(ticket("turn-2")).await.is_err(), "mutex holds");
        assert!(port.get("turn-1").await.is_some());
    }

    #[tokio::test]
    async fn stream_registry_port_creates_log_and_channel() {
        let root = std::env::temp_dir().join(format!(
            "medousa-stream-registry-test-{}",
            std::process::id()
        ));
        medousa_engine::configure_log_root(root.clone());
        let registry = crate::daemon::turn_stream_registry::new_turn_stream_registry();
        let port = turn_stream_registry_adapter(registry.clone());
        let port: &dyn TurnStreamRegistryPort = &port;
        assert!(port.register_stream("turn-a").await);
        assert!(!port.register_stream("turn-a").await);
        assert!(port.has_stream("turn-a").await);
        assert!(port.event_log("turn-a").await.is_some());
        port.mark_stream_closed("turn-a").await;
        port.drop_stream("turn-a").await;
        assert!(!port.has_stream("turn-a").await);
        let _ = std::fs::remove_dir_all(&root);
    }
}
