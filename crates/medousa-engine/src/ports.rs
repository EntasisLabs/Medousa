//! Engine outbound ports — channel-first interfaces the daemon implements.

use async_trait::async_trait;
use medousa_types::session::ConversationTurn;
use medousa_types::turn_ticket::{TurnTicket, TurnTicketConflict};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum ToolSinkEvent {
    BrowserChallenge {
        turn_correlation_id: String,
        session_id: String,
        challenge_url: String,
        reason: String,
    },
    BrowserNavigated {
        turn_correlation_id: String,
        url: String,
        title: Option<String>,
        opened_by_agent: bool,
    },
}

#[async_trait]
pub trait ToolSinkPort: Send + Sync {
    async fn emit(&self, event: ToolSinkEvent);
}

#[derive(Clone)]
pub struct ChannelToolSink {
    tx: mpsc::Sender<ToolSinkEvent>,
}

impl ChannelToolSink {
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<ToolSinkEvent>) {
        let (tx, rx) = mpsc::channel(capacity.max(1));
        (Self { tx }, rx)
    }
}

#[async_trait]
impl ToolSinkPort for ChannelToolSink {
    async fn emit(&self, event: ToolSinkEvent) {
        let _ = self.tx.send(event).await;
    }
}

#[async_trait]
pub trait TurnTicketPort: Send + Sync {
    async fn register(&self, ticket: TurnTicket) -> Result<(), TurnTicketConflict>;
    async fn note_event(&self, turn_id: &str, event_type: &str, terminal: bool);
    async fn mark_cancelled(&self, turn_id: &str);
    async fn clear(&self, turn_id: &str);
    /// Drop the ticket after orchestration unless a handoff ticket must remain.
    async fn clear_after_run(&self, turn_id: &str);
    async fn get(&self, turn_id: &str) -> Option<TurnTicket>;
}

#[async_trait]
pub trait TurnStreamRegistryPort: Send + Sync {
    async fn register_stream(&self, turn_id: &str) -> bool;
    async fn drop_stream(&self, turn_id: &str);
    async fn has_stream(&self, turn_id: &str) -> bool;
    /// Durable per-turn event log used for SSE `?since=N` replay.
    async fn event_log(&self, turn_id: &str) -> Option<std::sync::Arc<crate::turn_event_log::TurnEventLog>>;
    /// Mark the live fan-out channel closed while retaining replay state.
    async fn mark_stream_closed(&self, turn_id: &str);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    Inserted,
    AlreadyPresent,
}

#[derive(Debug, Clone)]
pub struct StoreError(pub String);

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "store error: {}", self.0)
    }
}

impl std::error::Error for StoreError {}

#[async_trait]
pub trait TurnStorePort: Send + Sync {
    async fn upsert_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        turn: ConversationTurn,
    ) -> Result<UpsertOutcome, StoreError>;

    async fn turn_exists(&self, session_id: &str, turn_id: &str) -> Result<bool, StoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn channel_tool_sink_delivers_event() {
        let (sink, mut rx) = ChannelToolSink::new(4);
        sink.emit(ToolSinkEvent::BrowserNavigated {
            turn_correlation_id: "t1".to_string(),
            url: "https://example.test".to_string(),
            title: Some("Example".to_string()),
            opened_by_agent: true,
        })
        .await;
        let event = rx.recv().await.expect("event delivered");
        match event {
            ToolSinkEvent::BrowserNavigated { url, opened_by_agent, .. } => {
                assert_eq!(url, "https://example.test");
                assert!(opened_by_agent);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
