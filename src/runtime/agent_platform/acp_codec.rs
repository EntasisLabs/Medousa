//! ACP JSON ↔ Stasis `AgentEnvelope` codec (Medousa-owned; no vendor adapters in Stasis).

use chrono::Utc;
use serde_json::{json, Value};
use stasis::domain::agent::envelope::{
    AgentEnvelope, AgentEnvelopeKind, EncodedAgentMessage, AGENT_ENVELOPE_SCHEMA_VERSION_V1,
};
use stasis::domain::errors::{Result, StasisError};
use stasis::infrastructure::agent::{
    JsonAgentMessageCodec, JSON_AGENT_CONTENT_TYPE, JSON_AGENT_SCHEMA_NAME,
};
use stasis::ports::outbound::agent::AgentMessageCodec;
use uuid::Uuid;

use super::ports::AcpTerminalKind;

/// Content type for ACP-shaped wire bytes decoded by Medousa.
pub const ACP_AGENT_CONTENT_TYPE: &str = "application/vnd.medousa.acp+json";
/// Schema name for ACP event payloads translated into envelopes.
pub const ACP_AGENT_SCHEMA_NAME: &str = "medousa.acp.agent_event.v1";

/// Wraps Stasis [`JsonAgentMessageCodec`] and accepts ACP terminal/progress JSON.
#[derive(Clone, Debug, Default)]
pub struct AcpAgentMessageCodec {
    json: JsonAgentMessageCodec,
}

impl AcpAgentMessageCodec {
    pub fn new() -> Self {
        Self {
            json: JsonAgentMessageCodec::v1(),
        }
    }

    /// Build a canonical envelope from an ACP session terminal (or progress) outcome.
    pub fn envelope_from_acp(
        &self,
        kind: AcpTerminalKind,
        session_id: &str,
        turn_id: Option<&str>,
        agent_session_id: &str,
        runtime: &str,
        message: &str,
        payload: Value,
    ) -> AgentEnvelope {
        let envelope_kind = match kind {
            AcpTerminalKind::Completed => AgentEnvelopeKind::TurnCompleted,
            AcpTerminalKind::Failed => AgentEnvelopeKind::Failed,
            AcpTerminalKind::Cancelled => AgentEnvelopeKind::Cancelled,
            AcpTerminalKind::Progress => AgentEnvelopeKind::Progress,
        };
        let mut body = payload;
        if !body.is_object() {
            body = json!({});
        }
        if let Some(obj) = body.as_object_mut() {
            obj.insert("runtime".into(), json!(runtime));
            obj.insert("agent_session_id".into(), json!(agent_session_id));
            if !message.is_empty() {
                obj.insert("message".into(), json!(message));
            }
            if matches!(
                kind,
                AcpTerminalKind::Failed | AcpTerminalKind::Cancelled
            ) && !obj.contains_key("error")
            {
                obj.insert(
                    "error".into(),
                    json!(if message.is_empty() {
                        format!("acp {:?}", kind)
                    } else {
                        message.to_string()
                    }),
                );
            }
        }

        AgentEnvelope {
            schema_version: AGENT_ENVELOPE_SCHEMA_VERSION_V1,
            kind: envelope_kind,
            envelope_id: format!("acp-{}", Uuid::new_v4()),
            session_id: session_id.to_string(),
            thread_id: None,
            turn_id: turn_id.map(|s| s.to_string()).or_else(|| {
                Some(agent_session_id.to_string())
            }),
            job_id: None,
            correlation_id: agent_session_id.to_string(),
            causation_id: format!("acp:{runtime}"),
            participant_id: Some(runtime.to_string()),
            occurred_at: Utc::now(),
            payload: body,
        }
    }

    fn decode_acp_event(&self, message: &EncodedAgentMessage) -> Result<AgentEnvelope> {
        let value: Value = serde_json::from_slice(&message.body).map_err(|err| {
            StasisError::PortFailure(format!("failed to decode ACP agent event json: {err}"))
        })?;
        let event = value
            .get("event")
            .and_then(|v| v.as_str())
            .unwrap_or("done")
            .to_ascii_lowercase();
        let kind = match event.as_str() {
            "error" | "failed" | "fail" => AcpTerminalKind::Failed,
            "cancelled" | "canceled" | "cancel" => AcpTerminalKind::Cancelled,
            "progress" | "status" => AcpTerminalKind::Progress,
            _ => AcpTerminalKind::Completed,
        };
        let session_id = value
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let agent_session_id = value
            .get("agent_session_id")
            .and_then(|v| v.as_str())
            .unwrap_or(session_id.as_str())
            .to_string();
        let runtime = value
            .get("runtime")
            .and_then(|v| v.as_str())
            .unwrap_or("external")
            .to_string();
        let msg = value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let turn_id = value
            .get("turn_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let payload = value.get("payload").cloned().unwrap_or(value);
        Ok(self.envelope_from_acp(
            kind,
            &session_id,
            turn_id.as_deref(),
            &agent_session_id,
            &runtime,
            &msg,
            payload,
        ))
    }
}

impl AgentMessageCodec for AcpAgentMessageCodec {
    fn content_type(&self) -> &'static str {
        // Waitable-turn grants encode as canonical Stasis JSON.
        JSON_AGENT_CONTENT_TYPE
    }

    fn schema_name(&self) -> &'static str {
        JSON_AGENT_SCHEMA_NAME
    }

    fn encode(&self, envelope: &AgentEnvelope) -> Result<EncodedAgentMessage> {
        self.json.encode(envelope)
    }

    fn decode(&self, message: &EncodedAgentMessage) -> Result<AgentEnvelope> {
        if message.content_type == ACP_AGENT_CONTENT_TYPE
            || message.schema_name == ACP_AGENT_SCHEMA_NAME
        {
            return self.decode_acp_event(message);
        }
        self.json.decode(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stasis::ports::outbound::agent::AgentMessageCodec;

    #[test]
    fn round_trips_canonical_envelope_via_json() {
        let codec = AcpAgentMessageCodec::new();
        let envelope = codec.envelope_from_acp(
            AcpTerminalKind::Completed,
            "sess-1",
            Some("turn-1"),
            "agent-1",
            "cursor",
            "done",
            json!({ "ok": true }),
        );
        let encoded = codec.encode(&envelope).expect("encode");
        let decoded = codec.decode(&encoded).expect("decode");
        assert_eq!(decoded.kind, AgentEnvelopeKind::TurnCompleted);
        assert_eq!(decoded.turn_id.as_deref(), Some("turn-1"));
    }

    #[test]
    fn decodes_acp_event_shape() {
        let codec = AcpAgentMessageCodec::new();
        let body = serde_json::to_vec(&json!({
            "event": "error",
            "session_id": "sess-1",
            "agent_session_id": "agent-9",
            "runtime": "codex",
            "message": "boom",
            "turn_id": "turn-9",
        }))
        .unwrap();
        let decoded = codec
            .decode(&EncodedAgentMessage {
                content_type: ACP_AGENT_CONTENT_TYPE.into(),
                schema_name: ACP_AGENT_SCHEMA_NAME.into(),
                body,
            })
            .expect("decode acp");
        assert_eq!(decoded.kind, AgentEnvelopeKind::Failed);
        assert_eq!(decoded.turn_id.as_deref(), Some("turn-9"));
    }
}
