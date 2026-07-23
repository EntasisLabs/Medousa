//! Process-local agent platform ports shared by Stasis wire and `/v1/agents`.

use std::sync::{Arc, OnceLock};

use serde_json::Value;
use stasis::infrastructure::agent::{
    InMemoryAgentEventIngress, InMemoryTurnWaitStore, WaitCorrelatingAgentEventIngress,
};
use stasis::ports::outbound::agent::{
    AgentEventIngress, AgentMessageCodec, TurnWaitStore,
};

use super::acp_codec::AcpAgentMessageCodec;

/// Terminal (or progress) outcomes from ACP sessions that feed ingress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcpTerminalKind {
    Completed,
    Failed,
    Cancelled,
    Progress,
}

/// Handles retained for Medousa composition + ACP completion correlation.
#[derive(Clone)]
pub struct AgentPlatformPorts {
    pub codec: Arc<dyn AgentMessageCodec>,
    pub wait_store: Arc<dyn TurnWaitStore>,
    pub ingress: Arc<dyn AgentEventIngress>,
    /// Typed Medousa codec for ACP → envelope helpers.
    pub acp_codec: Arc<AcpAgentMessageCodec>,
}

/// MCP tool names safe to export toward external agents (read-heavy; recursion budget).
pub fn mcp_export_allowlist() -> Vec<&'static str> {
    vec![
        "vault_list",
        "vault_read",
        "vault_search",
        "calendar_list",
        "artifacts_list",
        "artifacts_fetch",
    ]
}

pub fn shared_agent_platform_ports() -> Arc<AgentPlatformPorts> {
    static PORTS: OnceLock<Arc<AgentPlatformPorts>> = OnceLock::new();
    PORTS
        .get_or_init(|| {
            let acp_codec = Arc::new(AcpAgentMessageCodec::new());
            let wait_store: Arc<dyn TurnWaitStore> = Arc::new(InMemoryTurnWaitStore::new());
            let inner: Arc<dyn AgentEventIngress> = Arc::new(InMemoryAgentEventIngress::new());
            let ingress: Arc<dyn AgentEventIngress> = Arc::new(
                WaitCorrelatingAgentEventIngress::new(inner, wait_store.clone()),
            );
            Arc::new(AgentPlatformPorts {
                codec: acp_codec.clone() as Arc<dyn AgentMessageCodec>,
                wait_store,
                ingress,
                acp_codec,
            })
        })
        .clone()
}

/// Publish an ACP terminal/progress outcome into Stasis agent ingress (best-effort).
pub async fn publish_acp_terminal(
    kind: AcpTerminalKind,
    session_id: &str,
    turn_id: Option<&str>,
    agent_session_id: &str,
    runtime: &str,
    message: &str,
    payload: Value,
) {
    let ports = shared_agent_platform_ports();
    let envelope = ports.acp_codec.envelope_from_acp(
        kind,
        session_id,
        turn_id,
        agent_session_id,
        runtime,
        message,
        payload,
    );
    if let Err(err) = ports.ingress.accept(envelope).await {
        tracing::debug!(error = %err, "agent platform ingress accept failed");
    }
}
