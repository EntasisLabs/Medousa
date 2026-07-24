//! Stasis 0.8 agent platform ports owned by Medousa (ADR-008 / Stasis ADR-0007).
//!
//! Vendor ACP stays in `medousa-acp-client`. These ports translate ACP outcomes into
//! canonical `AgentEnvelope`s and complete process-local waitable turns.

mod acp_codec;
mod ports;

pub use acp_codec::AcpAgentMessageCodec;
pub use ports::{
    mcp_export_allowlist, publish_acp_terminal, shared_agent_platform_ports, AgentPlatformPorts,
    AcpTerminalKind,
};
