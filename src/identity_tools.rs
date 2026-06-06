//! Host-bus identity tools: read context, propose patches, commit under policy (AX-4c).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::ports::outbound::memory::identity_memory_models::{
    CommitEntityUpdateRequest, IdentityContextMode, ProposeEntityUpdateRequest,
};
use tokio::sync::mpsc;

use crate::events::TuiEvent;
use crate::identity_memory::{
    build_identity_context_request, resolve_identity_channel_id, resolve_identity_persona_id,
    resolve_identity_user_id,
};
use crate::identity_write_policy::{
    evaluate_identity_commit, load_identity_product_config, parse_identity_entity_type,
    parse_update_source,
};

async fn emit_invoked(event_tx: &mpsc::Sender<TuiEvent>, tool_name: &str, summary: &str) {
    let _ = event_tx
        .send(TuiEvent::ToolInvoked {
            tool_name: tool_name.to_string(),
            input_summary: summary.chars().take(80).collect(),
        })
        .await;
}

fn optional_str(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn parse_identity_context_mode(raw: Option<&str>) -> StasisResult<IdentityContextMode> {
    match raw
        .unwrap_or("cognitive")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "full" => Ok(IdentityContextMode::Full),
        "policy" => Ok(IdentityContextMode::Policy),
        "cognitive" => Ok(IdentityContextMode::Cognitive),
        other => Err(StasisError::PortFailure(format!(
            "unsupported identity context mode '{other}', expected full|policy|cognitive"
        ))),
    }
}

fn parse_utc_optional(value: Option<&str>, field: &str) -> Result<Option<DateTime<Utc>>, String> {
    match value {
        Some(raw) => DateTime::parse_from_rfc3339(raw)
            .map(|parsed| Some(parsed.with_timezone(&Utc)))
            .map_err(|_| format!("{field} must be an ISO8601 UTC datetime")),
        None => Ok(None),
    }
}

// ── cognition_identity_context ────────────────────────────────────────────────

pub struct CognitionIdentityContextTool {
    service: Arc<IdentityMemoryService>,
    default_user_id: String,
    default_persona_id: String,
    default_channel_id: String,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionIdentityContextTool {
    pub fn new(
        service: Arc<IdentityMemoryService>,
        default_user_id: String,
        default_persona_id: String,
        default_channel_id: String,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            service,
            default_user_id,
            default_persona_id,
            default_channel_id,
            event_tx,
        }
    }
}

#[async_trait]
impl StasisTool for CognitionIdentityContextTool {
    fn name(&self) -> &'static str {
        "cognition_identity_context"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Read identity graph context (persona, user, channels, relationships) for this turn.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "user_id": { "type": "string", "description": "Override identity user id" },
                "persona_id": { "type": "string", "description": "Override persona id" },
                "channel_id": { "type": "string", "description": "Override channel id" },
                "relationship_limit": { "type": "integer", "minimum": 1, "maximum": 64 },
                "mode": {
                    "type": "string",
                    "enum": ["full", "policy", "cognitive"],
                    "description": "Identity context slice (default: cognitive)"
                }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        emit_invoked(&self.event_tx, self.name(), "identity context").await;
        let user_id = optional_str(input.get("user_id").and_then(Value::as_str))
            .unwrap_or_else(|| self.default_user_id.clone());
        let persona_id = optional_str(input.get("persona_id").and_then(Value::as_str))
            .unwrap_or_else(|| self.default_persona_id.clone());
        let channel_id = optional_str(input.get("channel_id").and_then(Value::as_str))
            .unwrap_or_else(|| self.default_channel_id.clone());
        let relationship_limit = input
            .get("relationship_limit")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(8)
            .clamp(1, 64);
        let mode = parse_identity_context_mode(input.get("mode").and_then(Value::as_str))?;

        let response = self
            .service
            .get_identity_context(&build_identity_context_request(
                user_id,
                persona_id,
                channel_id,
                relationship_limit,
                mode,
            ))
            .await?;

        Ok(serde_json::to_value(response).map_err(|e| {
            StasisError::PortFailure(format!("cognition_identity_context encode: {e}"))
        })?)
    }
}

// ── cognition_identity_propose ────────────────────────────────────────────────

pub struct CognitionIdentityProposeTool {
    service: Arc<IdentityMemoryService>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionIdentityProposeTool {
    pub fn new(service: Arc<IdentityMemoryService>, event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { service, event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionIdentityProposeTool {
    fn name(&self) -> &'static str {
        "cognition_identity_propose"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Propose a durable identity patch (persona, user, relationship). Returns proposal_ids and tiers; use cognition_identity_commit when policy allows.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["entity_type", "entity_id", "patch"],
            "properties": {
                "entity_type": {
                    "type": "string",
                    "description": "persona | user | contact | relationship | channel | policy"
                },
                "entity_id": { "type": "string" },
                "patch": { "type": "object", "description": "Flat or nested JSON patch object" },
                "source": {
                    "type": "string",
                    "enum": ["user_direct", "model_inferred", "system_event"]
                },
                "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
                "reason": { "type": "string" },
                "actor": { "type": "string" },
                "expires_at": { "type": "string", "description": "RFC3339 UTC" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let entity_type_raw = input
            .get("entity_type")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("entity_type is required".to_string()))?;
        let entity_id = input
            .get("entity_id")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("entity_id is required".to_string()))?;
        let patch = input.get("patch").cloned().filter(Value::is_object).ok_or_else(|| {
            StasisError::PortFailure("patch must be a JSON object".to_string())
        })?;

        let entity_type = parse_identity_entity_type(entity_type_raw).map_err(StasisError::PortFailure)?;
        let source = parse_update_source(input.get("source").and_then(Value::as_str))
            .map_err(StasisError::PortFailure)?;
        let confidence = input
            .get("confidence")
            .and_then(Value::as_f64)
            .map(|v| v as f32)
            .unwrap_or(0.75)
            .clamp(0.0, 1.0);
        let reason = input
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("agent identity propose")
            .to_string();
        let actor = input
            .get("actor")
            .and_then(Value::as_str)
            .unwrap_or("medousa-agent")
            .to_string();
        let expires_at = parse_utc_optional(
            input.get("expires_at").and_then(Value::as_str),
            "expires_at",
        )
        .map_err(StasisError::PortFailure)?;

        emit_invoked(
            &self.event_tx,
            self.name(),
            &format!("{entity_type_raw}:{entity_id}"),
        )
        .await;

        let response = self
            .service
            .propose_entity_update(&ProposeEntityUpdateRequest {
                entity_type,
                entity_id: entity_id.to_string(),
                patch,
                source,
                confidence,
                reason,
                actor,
                receipt_id: None,
                expires_at,
            })
            .await?;

        Ok(serde_json::to_value(response).map_err(|e| {
            StasisError::PortFailure(format!("cognition_identity_propose encode: {e}"))
        })?)
    }
}

// ── cognition_identity_commit ─────────────────────────────────────────────────

pub struct CognitionIdentityCommitTool {
    service: Arc<IdentityMemoryService>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionIdentityCommitTool {
    pub fn new(service: Arc<IdentityMemoryService>, event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { service, event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionIdentityCommitTool {
    fn name(&self) -> &'static str {
        "cognition_identity_commit"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Commit a proposed identity patch when tier and Medousa policy allow. Pass expected_version from context; set approver for approval_required tiers.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["proposal_id", "expected_version"],
            "properties": {
                "proposal_id": { "type": "string" },
                "expected_version": { "type": "integer" },
                "approver": { "type": "string" },
                "entity_type": { "type": "string" },
                "entity_id": { "type": "string" },
                "patch": { "type": "object" },
                "source": { "type": "string" },
                "confidence": { "type": "number" },
                "tier": { "type": "string", "enum": ["auto_commit", "confirm_required", "approval_required"] }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let proposal_id = input
            .get("proposal_id")
            .and_then(Value::as_str)
            .ok_or_else(|| StasisError::PortFailure("proposal_id is required".to_string()))?;
        let expected_version = input
            .get("expected_version")
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                StasisError::PortFailure("expected_version is required".to_string())
            })? as i32;
        let approver = optional_str(input.get("approver").and_then(Value::as_str));

        emit_invoked(&self.event_tx, self.name(), proposal_id).await;

        let config = load_identity_product_config();

        if let (Some(entity_type_raw), Some(entity_id), Some(patch)) = (
            input.get("entity_type").and_then(Value::as_str),
            input.get("entity_id").and_then(Value::as_str),
            input.get("patch"),
        ) {
            if patch.is_object() {
                let entity_type =
                    parse_identity_entity_type(entity_type_raw).map_err(StasisError::PortFailure)?;
                let source = parse_update_source(input.get("source").and_then(Value::as_str))
                    .map_err(StasisError::PortFailure)?;
                let confidence = input
                    .get("confidence")
                    .and_then(Value::as_f64)
                    .map(|v| v as f32)
                    .unwrap_or(0.75);
                let tier = input
                    .get("tier")
                    .and_then(Value::as_str)
                    .map(|raw| match raw {
                        "confirm_required" => stasis::ports::outbound::memory::identity_memory_models::UpdateTier::ConfirmRequired,
                        "approval_required" => stasis::ports::outbound::memory::identity_memory_models::UpdateTier::ApprovalRequired,
                        _ => stasis::ports::outbound::memory::identity_memory_models::UpdateTier::AutoCommit,
                    })
                    .unwrap_or(stasis::ports::outbound::memory::identity_memory_models::UpdateTier::AutoCommit);

                let proposal_req = ProposeEntityUpdateRequest {
                    entity_type,
                    entity_id: entity_id.to_string(),
                    patch: patch.clone(),
                    source,
                    confidence,
                    reason: "commit gate".to_string(),
                    actor: "medousa-agent".to_string(),
                    receipt_id: None,
                    expires_at: None,
                };
                let commit_req = CommitEntityUpdateRequest {
                    proposal_id: proposal_id.to_string(),
                    expected_version,
                    approver: approver.clone(),
                };
                let gate = evaluate_identity_commit(&config, &proposal_req, tier, &commit_req);
                if !gate.allowed {
                    return Ok(json!({
                        "committed": false,
                        "policy_denied": true,
                        "rationale": gate.reason,
                    }));
                }
            }
        }

        let response = self
            .service
            .commit_entity_update(&CommitEntityUpdateRequest {
                proposal_id: proposal_id.to_string(),
                expected_version,
                approver,
            })
            .await?;

        Ok(serde_json::to_value(response).map_err(|e| {
            StasisError::PortFailure(format!("cognition_identity_commit encode: {e}"))
        })?)
    }
}

pub fn default_identity_tool_ids(
    session_user_id: Option<&str>,
    policy_profile: Option<&str>,
) -> (String, String, String) {
    let user_id = resolve_identity_user_id(session_user_id);
    let persona_id = resolve_identity_persona_id();
    let channel_id = resolve_identity_channel_id(policy_profile);
    (user_id, persona_id, channel_id)
}
