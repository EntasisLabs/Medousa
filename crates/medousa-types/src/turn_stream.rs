use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use crate::daemon_api::{
    ContextUsageReport, StreamToolArtifactRef, StreamUiArtifact, StreamUiScene, ToolInputParam,
};

pub const TURN_STREAM_SCHEMA_VERSION: u8 = 2;
pub const TURN_STREAM_V2_MEDIA_TYPE: &str = "text/event-stream; medousa-version=2";

fn deserialize_v2<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    let version = u8::deserialize(deserializer)?;
    if version != TURN_STREAM_SCHEMA_VERSION {
        return Err(serde::de::Error::custom(format!(
            "unsupported turn stream schema version {version}"
        )));
    }
    Ok(version)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TurnStreamEnvelopeV2 {
    #[serde(deserialize_with = "deserialize_v2")]
    #[cfg_attr(feature = "json-schema", schemars(range(min = 2, max = 2)))]
    pub schema_version: u8,
    pub turn_id: String,
    #[cfg_attr(feature = "json-schema", schemars(range(min = 1)))]
    pub seq: u64,
    pub emitted_at_utc: DateTime<Utc>,
    pub event: TurnStreamEventV2,
}

impl TurnStreamEnvelopeV2 {
    pub fn new(
        turn_id: impl Into<String>,
        seq: u64,
        emitted_at_utc: DateTime<Utc>,
        event: TurnStreamEventV2,
    ) -> Result<Self, TurnStreamEnvelopeError> {
        let envelope = Self {
            schema_version: TURN_STREAM_SCHEMA_VERSION,
            turn_id: turn_id.into(),
            seq,
            emitted_at_utc,
            event,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn validate(&self) -> Result<(), TurnStreamEnvelopeError> {
        if self.schema_version != TURN_STREAM_SCHEMA_VERSION {
            return Err(TurnStreamEnvelopeError::UnsupportedVersion(
                self.schema_version,
            ));
        }
        if self.turn_id.trim().is_empty() {
            return Err(TurnStreamEnvelopeError::EmptyTurnId);
        }
        if self.seq == 0 {
            return Err(TurnStreamEnvelopeError::ZeroSequence);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnStreamEnvelopeError {
    UnsupportedVersion(u8),
    EmptyTurnId,
    ZeroSequence,
}

impl std::fmt::Display for TurnStreamEnvelopeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(
                    formatter,
                    "unsupported turn stream schema version {version}"
                )
            }
            Self::EmptyTurnId => formatter.write_str("turn stream turn_id is empty"),
            Self::ZeroSequence => formatter.write_str("turn stream seq must be greater than zero"),
        }
    }
}

impl std::error::Error for TurnStreamEnvelopeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorkerAckKind {
    Worker,
    Workshop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TurnStreamEventV2 {
    ContentAppend {
        text: String,
    },
    ReasoningAppend {
        text: String,
    },
    Status {
        phase: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        operator_message: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        debug_message: Option<String>,
    },
    Progress {
        message: String,
        #[serde(default)]
        tool_names: Vec<String>,
    },
    PackHold {
        text: String,
        #[serde(default)]
        tool_names: Vec<String>,
    },
    ModelReceipt {
        provider: String,
        model: String,
    },
    Final {
        text: String,
        #[serde(default)]
        tool_names: Vec<String>,
    },
    NeedsInput {
        text: String,
        #[serde(default)]
        tool_names: Vec<String>,
    },
    Checkpoint {
        text: String,
        #[serde(default)]
        tool_names: Vec<String>,
    },
    WorkerAck {
        ack_kind: WorkerAckKind,
        text: String,
        #[serde(default)]
        tool_names: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work_id: Option<String>,
    },
    WorkerSynthesis {
        text: String,
        #[serde(default)]
        tool_names: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work_id: Option<String>,
    },
    FinalPending {
        text: String,
        #[serde(default)]
        tool_names: Vec<String>,
    },
    Error {
        operator_message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        debug_message: Option<String>,
    },
    ScratchReset,
    ToolStarted {
        tool_run_id: String,
        tool_name: String,
        input_summary: String,
        #[serde(default)]
        input_params: Vec<ToolInputParam>,
        tool_round: usize,
    },
    ToolFinished {
        tool_run_id: String,
        tool_name: String,
        status: String,
        input_summary: String,
        #[serde(default)]
        input_params: Vec<ToolInputParam>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_summary: Option<String>,
        tool_round: usize,
        #[serde(default)]
        artifact_refs: Vec<StreamToolArtifactRef>,
    },
    ArtifactPresented {
        artifact: StreamUiArtifact,
    },
    ArtifactUpdated {
        previous_artifact_id: String,
        artifact: StreamUiArtifact,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root_artifact_id: Option<String>,
    },
    UiScene {
        scene: StreamUiScene,
    },
    BudgetApprovalRequired {
        request_id: String,
        rounds_executed: usize,
        max_tool_rounds: usize,
        requested_rounds: usize,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        progress_summary: Option<String>,
    },
    BrowserChallenge {
        session_id: String,
        challenge_url: String,
        reason: String,
    },
    BrowserNavigated {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default)]
        opened_by_agent: bool,
    },
    ContextUsage {
        report: ContextUsageReport,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        operator_summary: Option<String>,
    },
    PermissionRequest {
        request_id: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_session_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_runtime: Option<String>,
    },
    /// Trusted UI prompt for credential material. This event carries request
    /// metadata only; the value travels over the dedicated fulfill endpoint.
    SecretRequest {
        request_id: String,
        label: String,
        reason: String,
        provider_type: String,
        credential_key: String,
        backend: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        allowed_hosts: Vec<String>,
    },
}

impl TurnStreamEventV2 {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Final { .. }
                | Self::NeedsInput { .. }
                | Self::Checkpoint { .. }
                | Self::WorkerSynthesis { .. }
                | Self::Error { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn samples() -> Vec<TurnStreamEventV2> {
        let artifact = StreamUiArtifact {
            artifact_id: "artifact-1".to_string(),
            mime: "text/html".to_string(),
            label: "Chart".to_string(),
            presentation: "inline".to_string(),
            byte_size: Some(42),
            height_px: Some(240),
        };
        let report = ContextUsageReport {
            layers: Vec::new(),
            total_tokens_estimate: 10,
            total_chars: 40,
            context_limit_tokens: Some(128_000),
            tool_count: 2,
            estimator: "chars/4".to_string(),
        };
        vec![
            TurnStreamEventV2::ContentAppend { text: "a".into() },
            TurnStreamEventV2::ReasoningAppend { text: "r".into() },
            TurnStreamEventV2::Status {
                phase: "working".into(),
                operator_message: Some("Working".into()),
                debug_message: None,
            },
            TurnStreamEventV2::Progress {
                message: "progress".into(),
                tool_names: vec!["search".into()],
            },
            TurnStreamEventV2::PackHold {
                text: "held".into(),
                tool_names: Vec::new(),
            },
            TurnStreamEventV2::ModelReceipt {
                provider: "openai".into(),
                model: "model".into(),
            },
            TurnStreamEventV2::Final {
                text: "done".into(),
                tool_names: Vec::new(),
            },
            TurnStreamEventV2::NeedsInput {
                text: "question".into(),
                tool_names: Vec::new(),
            },
            TurnStreamEventV2::Checkpoint {
                text: "checkpoint".into(),
                tool_names: Vec::new(),
            },
            TurnStreamEventV2::WorkerAck {
                ack_kind: WorkerAckKind::Worker,
                text: "started".into(),
                tool_names: Vec::new(),
                work_id: Some("work-1".into()),
            },
            TurnStreamEventV2::WorkerSynthesis {
                text: "result".into(),
                tool_names: Vec::new(),
                work_id: Some("work-1".into()),
            },
            TurnStreamEventV2::FinalPending {
                text: "wrapping".into(),
                tool_names: Vec::new(),
            },
            TurnStreamEventV2::Error {
                operator_message: "failed".into(),
                debug_message: Some("detail".into()),
            },
            TurnStreamEventV2::ScratchReset,
            TurnStreamEventV2::ToolStarted {
                tool_run_id: "run-1".into(),
                tool_name: "search".into(),
                input_summary: "query".into(),
                input_params: Vec::new(),
                tool_round: 1,
            },
            TurnStreamEventV2::ToolFinished {
                tool_run_id: "run-1".into(),
                tool_name: "search".into(),
                status: "ok".into(),
                input_summary: "query".into(),
                input_params: Vec::new(),
                output_summary: Some("found".into()),
                tool_round: 1,
                artifact_refs: Vec::new(),
            },
            TurnStreamEventV2::ArtifactPresented {
                artifact: artifact.clone(),
            },
            TurnStreamEventV2::ArtifactUpdated {
                previous_artifact_id: "artifact-0".into(),
                artifact,
                root_artifact_id: Some("artifact-root".into()),
            },
            TurnStreamEventV2::UiScene {
                scene: StreamUiScene {
                    turn_id: Some("turn-1".into()),
                    surface_id: Some("chat:turn-1".into()),
                    rev: Some(1),
                    ops: vec![json!({"op": "clear"})],
                },
            },
            TurnStreamEventV2::BudgetApprovalRequired {
                request_id: "budget-1".into(),
                rounds_executed: 10,
                max_tool_rounds: 10,
                requested_rounds: 5,
                reason: "continue".into(),
                progress_summary: Some("halfway".into()),
            },
            TurnStreamEventV2::BrowserChallenge {
                session_id: "browser-1".into(),
                challenge_url: "https://example.invalid".into(),
                reason: "captcha".into(),
            },
            TurnStreamEventV2::BrowserNavigated {
                url: "https://example.invalid".into(),
                title: Some("Example".into()),
                opened_by_agent: true,
            },
            TurnStreamEventV2::ContextUsage {
                report,
                operator_summary: Some("Context ready".into()),
            },
            TurnStreamEventV2::PermissionRequest {
                request_id: "permission-1".into(),
                message: "Allow?".into(),
                agent_session_id: Some("agent-1".into()),
                agent_runtime: Some("codex".into()),
            },
            TurnStreamEventV2::SecretRequest {
                request_id: "secret-1".into(),
                label: "GitHub token".into(),
                reason: "Read a private repository".into(),
                provider_type: "github".into(),
                credential_key: "GITHUB_TOKEN".into(),
                backend: "openshell_provider".into(),
                allowed_hosts: vec!["api.github.com".into()],
            },
        ]
    }

    #[test]
    fn every_variant_roundtrips_without_nullable_cross_variant_fields() {
        for (index, event) in samples().into_iter().enumerate() {
            let envelope =
                TurnStreamEnvelopeV2::new("turn-1", index as u64 + 1, Utc::now(), event).unwrap();
            let encoded = serde_json::to_value(&envelope).unwrap();
            let decoded: TurnStreamEnvelopeV2 = serde_json::from_value(encoded.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
        }
    }

    #[test]
    fn rejects_unknown_versions_variants_and_missing_variant_fields() {
        let unknown_version = json!({
            "schema_version": 3,
            "turn_id": "turn-1",
            "seq": 1,
            "emitted_at_utc": Utc::now(),
            "event": { "type": "scratch_reset" }
        });
        assert!(serde_json::from_value::<TurnStreamEnvelopeV2>(unknown_version).is_err());

        let impossible = json!({
            "schema_version": 2,
            "turn_id": "turn-1",
            "seq": 1,
            "emitted_at_utc": Utc::now(),
            "event": { "type": "content_append" }
        });
        assert!(serde_json::from_value::<TurnStreamEnvelopeV2>(impossible).is_err());

        let unknown = json!({
            "schema_version": 2,
            "turn_id": "turn-1",
            "seq": 1,
            "emitted_at_utc": Utc::now(),
            "event": { "type": "future_event" }
        });
        assert!(serde_json::from_value::<TurnStreamEnvelopeV2>(unknown).is_err());
    }
}
