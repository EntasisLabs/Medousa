//! ACP client bones for hot-swappable agentic runtimes.
//!
//! Home chat will eventually talk to Cursor / Codex (etc.) through this crate
//! while those agents reach Medousa space via `medousa-mcp-server`.
//!
//! 0.4.0 ships traits + config + a stub session — not full UX parity.

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Built-in runtime ids for 0.4.0 bones QA.
pub const RUNTIME_CURSOR: &str = "cursor";
pub const RUNTIME_CODEX: &str = "codex";
pub const RUNTIME_MEDOUSA: &str = "medousa";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRuntimeKind {
    Medousa,
    Cursor,
    Codex,
}

impl AgentRuntimeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Medousa => RUNTIME_MEDOUSA,
            Self::Cursor => RUNTIME_CURSOR,
            Self::Codex => RUNTIME_CODEX,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "medousa" | "native" => Some(Self::Medousa),
            "cursor" => Some(Self::Cursor),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpAgentConfig {
    pub kind: AgentRuntimeKind,
    /// Command to spawn (e.g. `agent`, `codex`). Empty = unset.
    pub command: String,
    pub args: Vec<String>,
    /// Working directory hint (workshop root).
    pub cwd: Option<String>,
}

impl AcpAgentConfig {
    pub fn cursor_default() -> Self {
        Self {
            kind: AgentRuntimeKind::Cursor,
            command: "agent".into(),
            args: vec!["acp".into()],
            cwd: None,
        }
    }

    pub fn codex_default() -> Self {
        Self {
            kind: AgentRuntimeKind::Codex,
            command: "codex".into(),
            args: vec!["acp".into()],
            cwd: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpSessionId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AcpEvent {
    MessageDelta { text: String },
    MessageDone { text: String },
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    PermissionRequest {
        id: String,
        summary: String,
    },
    Error { message: String },
    Done,
}

#[async_trait]
pub trait AcpClient: Send + Sync {
    async fn create_session(&self, config: &AcpAgentConfig) -> Result<AcpSessionId>;
    async fn prompt(&self, session: &AcpSessionId, text: &str) -> Result<()>;
    async fn cancel(&self, session: &AcpSessionId) -> Result<()>;
    async fn next_event(&self, session: &AcpSessionId) -> Result<Option<AcpEvent>>;
}

/// Placeholder client — proves wiring without requiring Cursor/Codex installed.
pub struct StubAcpClient;

#[async_trait]
impl AcpClient for StubAcpClient {
    async fn create_session(&self, config: &AcpAgentConfig) -> Result<AcpSessionId> {
        if matches!(config.kind, AgentRuntimeKind::Medousa) {
            bail!("use native Medousa turn path for medousa runtime");
        }
        Ok(AcpSessionId(format!(
            "stub-{}-{}",
            config.kind.as_str(),
            uuid_v4_lite()
        )))
    }

    async fn prompt(&self, session: &AcpSessionId, text: &str) -> Result<()> {
        tracing::info!(session = %session.0, chars = text.len(), "stub ACP prompt");
        Ok(())
    }

    async fn cancel(&self, session: &AcpSessionId) -> Result<()> {
        tracing::info!(session = %session.0, "stub ACP cancel");
        Ok(())
    }

    async fn next_event(&self, session: &AcpSessionId) -> Result<Option<AcpEvent>> {
        Ok(Some(AcpEvent::MessageDone {
            text: format!(
                "[medousa-acp-client bones] stub session {} — wire Cursor/Codex ACP next.",
                session.0
            ),
        }))
    }
}

fn uuid_v4_lite() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

/// Resolve a configured external runtime (Cursor / Codex only in 0.4.0).
pub fn external_runtime_config(kind: AgentRuntimeKind) -> Result<AcpAgentConfig> {
    match kind {
        AgentRuntimeKind::Cursor => Ok(AcpAgentConfig::cursor_default()),
        AgentRuntimeKind::Codex => Ok(AcpAgentConfig::codex_default()),
        AgentRuntimeKind::Medousa => {
            bail!("medousa runtime is not an ACP external agent")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_session_roundtrip() {
        let client = StubAcpClient;
        let cfg = AcpAgentConfig::cursor_default();
        let session = client.create_session(&cfg).await.unwrap();
        assert!(session.0.starts_with("stub-cursor-"));
        client.prompt(&session, "hello").await.unwrap();
        let ev = client.next_event(&session).await.unwrap();
        assert!(matches!(ev, Some(AcpEvent::MessageDone { .. })));
    }

    #[test]
    fn parses_runtime_kinds() {
        assert_eq!(
            AgentRuntimeKind::parse("Cursor"),
            Some(AgentRuntimeKind::Cursor)
        );
        assert_eq!(
            AgentRuntimeKind::parse("codex"),
            Some(AgentRuntimeKind::Codex)
        );
        assert!(external_runtime_config(AgentRuntimeKind::Medousa).is_err());
    }
}
